use gpui::{Keystroke, WindowId};
use parking_lot::Mutex;
use std::{
    ffi::{c_char, c_int, c_void},
    fs,
    path::PathBuf,
    sync::OnceLock,
};

// WindowServer fires symbolic hotkeys even when the app handles their NSEvent.
const MAX_SYMBOLIC_HOTKEY_ID: i32 = 512;
const RESTORE_FILE_PREFIX: &str = "gpui-macos-symbolic-hotkeys-";

const RTLD_LAZY: c_int = 1;

unsafe extern "C" {
    fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

type SetEnabledFn = unsafe extern "C" fn(i32, bool) -> i32;
type GetValueFn = unsafe extern "C" fn(i32, *mut u16, *mut u16, *mut u32) -> i32;
type IsEnabledFn = unsafe extern "C" fn(i32) -> bool;

struct SkyLight {
    set_enabled: SetEnabledFn,
    is_enabled: IsEnabledFn,
    get_value: GetValueFn,
}

struct LockState {
    locked: bool,
    previously_enabled: Vec<i32>,
    window: Option<WindowId>,
    suppressed: Vec<Keystroke>,
}

static STATE: Mutex<LockState> = Mutex::new(LockState {
    locked: false,
    previously_enabled: Vec::new(),
    window: None,
    suppressed: Vec::new(),
});

fn restore_path() -> PathBuf {
    std::env::temp_dir().join(format!("{RESTORE_FILE_PREFIX}{}", std::process::id()))
}

fn sky_light() -> Option<&'static SkyLight> {
    static SKY_LIGHT: OnceLock<Option<SkyLight>> = OnceLock::new();
    SKY_LIGHT
        .get_or_init(|| {
            unsafe {
                let handle = dlopen(
                    c"/System/Library/PrivateFrameworks/SkyLight.framework/SkyLight".as_ptr(),
                    RTLD_LAZY,
                );
                if handle.is_null() {
                    log::warn!(
                        "SkyLight.framework not available; macOS system hotkeys cannot be suppressed while focused"
                    );
                    return None;
                }
                let set_enabled = dlsym(handle, c"CGSSetSymbolicHotKeyEnabled".as_ptr());
                let is_enabled = dlsym(handle, c"CGSIsSymbolicHotKeyEnabled".as_ptr());
                let get_value = dlsym(handle, c"CGSGetSymbolicHotKeyValue".as_ptr());
                if set_enabled.is_null() || is_enabled.is_null() || get_value.is_null() {
                    log::warn!(
                        "CGS symbolic hotkey APIs missing; macOS system hotkeys cannot be suppressed while focused"
                    );
                    return None;
                }
                Some(SkyLight {
                    set_enabled: std::mem::transmute::<*mut c_void, SetEnabledFn>(set_enabled),
                    is_enabled: std::mem::transmute::<*mut c_void, IsEnabledFn>(is_enabled),
                    get_value: std::mem::transmute::<*mut c_void, GetValueFn>(get_value),
                })
            }
        })
        .as_ref()
}

fn set_enabled(id: i32, enabled: bool) -> bool {
    let Some(sky_light) = sky_light() else {
        return false;
    };
    let error = unsafe { (sky_light.set_enabled)(id, enabled) };
    if error != 0 {
        log::error!("failed to set macOS hotkey {id} enabled={enabled}: {error}");
    }
    error == 0
}

const COMMAND: u32 = 1 << 20;
const SHIFT: u32 = 1 << 17;
const CONTROL: u32 = 1 << 18;
const OPTION: u32 = 1 << 19;
const FUNCTION: u32 = 1 << 23;

fn should_suppress(key: &str, modifiers: u32, suppressed: &[Keystroke]) -> bool {
    let modifiers = modifiers & (COMMAND | SHIFT | CONTROL | OPTION | FUNCTION);
    if key.len() != 1 || !key.bytes().all(|byte| byte.is_ascii_digit()) {
        return false;
    }
    if modifiers == COMMAND | SHIFT {
        return true;
    }
    modifiers == COMMAND
        && suppressed.iter().any(|keystroke| {
            keystroke.key == key
                && keystroke.modifiers.platform
                && !keystroke.modifiers.shift
                && !keystroke.modifiers.control
                && !keystroke.modifiers.alt
                && !keystroke.modifiers.function
        })
}

fn conflicting_ids(suppressed: &[Keystroke]) -> Vec<i32> {
    let Some(sky_light) = sky_light() else {
        return Vec::new();
    };
    (0..=MAX_SYMBOLIC_HOTKEY_ID)
        .filter(|id| {
            let mut equivalent = 0;
            let mut key_code = 0;
            let mut modifiers = 0;
            let error = unsafe {
                (sky_light.get_value)(*id, &mut equivalent, &mut key_code, &mut modifiers)
            };
            if error != 0 || modifiers & COMMAND == 0 {
                return false;
            }
            let key = crate::events::command_key_for_key_code(key_code);
            should_suppress(&key, modifiers, suppressed)
        })
        .collect()
}

fn is_enabled(id: i32) -> bool {
    let Some(sky_light) = sky_light() else {
        return false;
    };
    unsafe { (sky_light.is_enabled)(id) }
}

fn write_restore_file(ids: &[i32]) -> bool {
    let path = restore_path();
    let contents = ids
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",");
    let temporary_path = path.with_extension("tmp");
    if let Err(error) =
        fs::write(&temporary_path, contents).and_then(|()| fs::rename(&temporary_path, &path))
    {
        log::error!(
            "failed to persist macOS hotkey restore list to {}: {error}",
            path.display()
        );
        return false;
    }
    true
}

fn read_restore_file(path: &std::path::Path) -> Vec<i32> {
    match fs::read_to_string(path) {
        Ok(contents) => contents
            .split(',')
            .filter_map(|part| part.trim().parse::<i32>().ok())
            .collect(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Vec::new(),
        Err(error) => {
            log::error!(
                "failed to read macOS hotkey restore list from {}: {error}",
                path.display()
            );
            Vec::new()
        }
    }
}

fn remove_restore_file(path: &std::path::Path) {
    if let Err(error) = fs::remove_file(path)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        log::error!(
            "failed to remove macOS hotkey restore list {}: {error}",
            path.display()
        );
    }
}

fn pid_is_running(pid: u32) -> bool {
    unsafe extern "C" {
        fn kill(pid: i32, signal: c_int) -> c_int;
    }
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    unsafe { kill(pid, 0) == 0 }
}

fn enable_ids(ids: &[i32]) -> bool {
    let mut restored = true;
    for id in ids {
        restored &= set_enabled(*id, true);
    }
    restored
}

/// Re-enable hotkeys left disabled by a previous crash of this or another GPUI app.
pub(crate) fn restore_stale() {
    let temp_dir = std::env::temp_dir();
    let entries = match fs::read_dir(&temp_dir) {
        Ok(entries) => entries,
        Err(error) => {
            log::error!(
                "failed to scan {} for macOS hotkey restore files: {error}",
                temp_dir.display()
            );
            return;
        }
    };

    for entry in entries {
        let entry = match entry {
            Ok(entry) => entry,
            Err(error) => {
                log::error!("failed to read temp directory entry: {error}");
                continue;
            }
        };
        let name = entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        let Some(pid_str) = name.strip_prefix(RESTORE_FILE_PREFIX) else {
            continue;
        };
        let Ok(pid) = pid_str.parse::<u32>() else {
            continue;
        };
        if pid == std::process::id() || pid_is_running(pid) {
            continue;
        }
        let path = entry.path();
        let ids = read_restore_file(&path);
        if enable_ids(&ids) {
            remove_restore_file(&path);
        }
    }
}

fn update_locked_hotkeys(state: &mut LockState) {
    let desired = conflicting_ids(&state.suppressed);
    state
        .previously_enabled
        .retain(|id| desired.contains(id) || !set_enabled(*id, true));
    let newly_disabled: Vec<_> = desired
        .into_iter()
        .filter(|id| !state.previously_enabled.contains(id) && is_enabled(*id))
        .collect();
    let mut restore_ids = state.previously_enabled.clone();
    restore_ids.extend(&newly_disabled);
    if restore_ids.is_empty() {
        remove_restore_file(&restore_path());
        return;
    }
    // Persist before disabling so a crash cannot strand a system shortcut.
    if !write_restore_file(&restore_ids) {
        return;
    }
    for id in newly_disabled {
        if set_enabled(id, false) {
            state.previously_enabled.push(id);
        }
    }
}

pub(crate) fn set_window_hotkeys(window: WindowId, suppressed: Option<&[Keystroke]>) {
    let mut state = STATE.lock();
    if let Some(suppressed) = suppressed {
        if state.window == Some(window) && state.suppressed == suppressed {
            return;
        }
        state.window = Some(window);
        state.suppressed = suppressed.to_vec();
    } else if state.window == Some(window) {
        state.window = None;
        state.suppressed.clear();
    } else {
        return;
    }
    if state.locked {
        update_locked_hotkeys(&mut state);
    }
}

pub(crate) fn lock() {
    if sky_light().is_none() {
        return;
    }
    let mut state = STATE.lock();
    state.locked = true;
    update_locked_hotkeys(&mut state);
}

pub(crate) fn unlock() {
    let mut state = STATE.lock();
    state
        .previously_enabled
        .retain(|id| !set_enabled(*id, true));
    state.locked = false;
    if state.previously_enabled.is_empty() {
        remove_restore_file(&restore_path());
    } else {
        write_restore_file(&state.previously_enabled);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui::Modifiers;

    #[test]
    fn suppress_only_numbered_bookmark_shortcuts() {
        let suppressed = [Keystroke {
            modifiers: Modifiers {
                platform: true,
                ..Modifiers::none()
            },
            key: "4".into(),
            key_char: None,
        }];
        for number in 0..=9 {
            let key = number.to_string();
            assert!(should_suppress(&key, COMMAND | SHIFT, &[]));
            assert_eq!(should_suppress(&key, COMMAND, &suppressed), number == 4);
            assert!(!should_suppress(&key, COMMAND, &[]));
            for extra_modifier in [CONTROL, OPTION, FUNCTION] {
                assert!(!should_suppress(
                    &key,
                    COMMAND | extra_modifier,
                    &suppressed
                ));
                assert!(!should_suppress(
                    &key,
                    COMMAND | SHIFT | extra_modifier,
                    &suppressed
                ));
            }
            assert!(!should_suppress(&key, SHIFT, &suppressed));
            assert!(!should_suppress(&key, CONTROL, &suppressed));
        }
        for key in [" ", "space", "n", "q", "tab", "f4", "!"] {
            assert!(!should_suppress(key, COMMAND, &suppressed));
            assert!(!should_suppress(key, COMMAND | SHIFT, &suppressed));
        }
    }
}
