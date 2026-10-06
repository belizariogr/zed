use parking_lot::Mutex;
use std::{
    ffi::{c_char, c_int, c_void},
    fs,
    path::PathBuf,
    sync::OnceLock,
};

/// macOS screenshot and related capture shortcuts live in the WindowServer
/// symbolic-hotkey table and fire even when the app handles the same NSEvent.
/// Disable them while this process is the active app, and restore afterwards.
const MAX_SYMBOLIC_HOTKEY_ID: i32 = 512;
const RESTORE_FILE_PREFIX: &str = "gpui-macos-symbolic-hotkeys-";

const RTLD_LAZY: c_int = 1;

unsafe extern "C" {
    fn dlopen(filename: *const c_char, flags: c_int) -> *mut c_void;
    fn dlsym(handle: *mut c_void, symbol: *const c_char) -> *mut c_void;
}

type SetEnabledFn = unsafe extern "C" fn(i32, bool);
type IsEnabledFn = unsafe extern "C" fn(i32) -> bool;

struct SkyLight {
    set_enabled: SetEnabledFn,
    is_enabled: IsEnabledFn,
}

struct LockState {
    locked: bool,
    previously_enabled: Vec<i32>,
}

static STATE: Mutex<LockState> = Mutex::new(LockState {
    locked: false,
    previously_enabled: Vec::new(),
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
                if set_enabled.is_null() || is_enabled.is_null() {
                    log::warn!(
                        "CGS symbolic hotkey APIs missing; macOS system hotkeys cannot be suppressed while focused"
                    );
                    return None;
                }
                Some(SkyLight {
                    set_enabled: std::mem::transmute::<*mut c_void, SetEnabledFn>(set_enabled),
                    is_enabled: std::mem::transmute::<*mut c_void, IsEnabledFn>(is_enabled),
                })
            }
        })
        .as_ref()
}

fn set_enabled(id: i32, enabled: bool) {
    let Some(sky_light) = sky_light() else {
        return;
    };
    unsafe {
        (sky_light.set_enabled)(id, enabled);
    }
}

fn is_enabled(id: i32) -> bool {
    let Some(sky_light) = sky_light() else {
        return false;
    };
    unsafe { (sky_light.is_enabled)(id) }
}

fn write_restore_file(ids: &[i32]) {
    let path = restore_path();
    let contents = ids
        .iter()
        .map(|id| id.to_string())
        .collect::<Vec<_>>()
        .join(",");
    if let Err(error) = fs::write(&path, contents) {
        log::error!(
            "failed to persist macOS hotkey restore list to {}: {error}",
            path.display()
        );
    }
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

fn enable_ids(ids: &[i32]) {
    for id in ids {
        set_enabled(*id, true);
    }
}

const SCREENSHOT_HOTKEYS: &[i32] = &[28, 29, 30, 31, 181, 184];

/// Re-enable hotkeys left disabled by a previous crash of this or another GPUI app.
pub(crate) fn restore_stale() {
    enable_ids(SCREENSHOT_HOTKEYS);

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
        enable_ids(&ids);
        remove_restore_file(&path);
    }
}

pub(crate) fn lock() {
    if sky_light().is_none() {
        return;
    }

    let mut state = STATE.lock();
    if state.locked {
        return;
    }

    let mut previously_enabled = Vec::new();
    for id in 0..=MAX_SYMBOLIC_HOTKEY_ID {
        if is_enabled(id) {
            previously_enabled.push(id);
        }
    }

    write_restore_file(&previously_enabled);
    for id in &previously_enabled {
        set_enabled(*id, false);
    }
    state.previously_enabled = previously_enabled;
    state.locked = true;
}

pub(crate) fn unlock() {
    let mut state = STATE.lock();
    if !state.locked {
        return;
    }

    enable_ids(&state.previously_enabled);
    state.previously_enabled.clear();
    state.locked = false;
    remove_restore_file(&restore_path());
}
