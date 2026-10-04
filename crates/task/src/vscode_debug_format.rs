use collections::HashMap;
use serde::Deserialize;
use util::ResultExt as _;

use crate::{
    DebugScenario, DebugTaskFile, EnvVariableReplacer, TcpArgumentsTemplate, VariableName,
};

// TODO support preLaunchTask linkage with other tasks
#[derive(Clone, Debug, PartialEq)]
struct VsCodeDebugTaskDefinition {
    r#type: String,
    name: String,
    port: Option<u16>,
    other_attributes: serde_json::Value,
}

impl<'de> Deserialize<'de> for VsCodeDebugTaskDefinition {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Definition {
            r#type: String,
            name: String,
            #[serde(default)]
            port: Option<u16>,
            #[serde(flatten)]
            other_attributes: serde_json::Value,
        }

        let mut configuration = serde_json::Value::deserialize(deserializer)?;
        apply_platform_overrides(&mut configuration, std::env::consts::OS);
        let definition: Definition =
            serde_json::from_value(configuration).map_err(serde::de::Error::custom)?;
        Ok(Self {
            r#type: definition.r#type,
            name: definition.name,
            port: definition.port,
            other_attributes: definition.other_attributes,
        })
    }
}

fn apply_platform_overrides(configuration: &mut serde_json::Value, operating_system: &str) {
    let Some(configuration) = configuration.as_object_mut() else {
        return;
    };
    let overrides = match operating_system {
        "macos" => configuration.remove("osx"),
        "windows" | "linux" => configuration.remove(operating_system),
        _ => None,
    };
    for platform in ["windows", "linux", "osx"] {
        configuration.remove(platform);
    }
    if let Some(serde_json::Value::Object(overrides)) = overrides {
        configuration.extend(overrides);
    }
}

impl VsCodeDebugTaskDefinition {
    fn try_to_zed(mut self, replacer: &EnvVariableReplacer) -> anyhow::Result<DebugScenario> {
        let label = replacer.replace(&self.name);
        let mut config = replacer.replace_value(self.other_attributes);
        let adapter = task_type_to_adapter_name(&self.r#type);
        if let Some(config) = config.as_object_mut()
            && matches!(adapter.as_str(), "JavaScript" | "Bun")
        {
            config.insert("type".to_owned(), self.r#type.clone().into());
            if let Some(port) = self.port.take() {
                config.insert("port".to_owned(), port.into());
            }
        }
        let definition = DebugScenario {
            label: label.into(),
            build: None,
            adapter: adapter.into(),
            tcp_connection: self.port.map(|port| TcpArgumentsTemplate {
                port: Some(port),
                host: None,
                timeout: None,
            }),
            config,
        };
        Ok(definition)
    }
}

#[derive(Clone, Debug, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct VsCodeDebugTaskFile {
    #[serde(default)]
    version: Option<String>,
    configurations: Vec<VsCodeDebugTaskDefinition>,
}

impl TryFrom<VsCodeDebugTaskFile> for DebugTaskFile {
    type Error = anyhow::Error;

    fn try_from(file: VsCodeDebugTaskFile) -> Result<Self, Self::Error> {
        let replacer = EnvVariableReplacer::new(HashMap::from_iter([
            (
                "workspaceFolder".to_owned(),
                VariableName::WorktreeRoot.to_string(),
            ),
            (
                "relativeFile".to_owned(),
                VariableName::RelativeFile.to_string(),
            ),
            ("file".to_owned(), VariableName::File.to_string()),
        ]))
        .with_commands([(
            "pickMyProcess".to_owned(),
            VariableName::PickProcessId.to_string(),
        )]);
        let templates = file
            .configurations
            .into_iter()
            .filter_map(|config| config.try_to_zed(&replacer).log_err())
            .collect::<Vec<_>>();
        Ok(DebugTaskFile(templates))
    }
}

fn task_type_to_adapter_name(task_type: &str) -> String {
    match task_type {
        "pwa-node" | "node" | "node-terminal" | "chrome" | "pwa-chrome" | "edge" | "pwa-edge"
        | "msedge" | "pwa-msedge" => "JavaScript",
        "bun" | "bun-dap-x" => "Bun",
        "go" => "Delve",
        "php" => "Xdebug",
        "cppdbg" | "lldb" => "CodeLLDB",
        "debugpy" => "Debugpy",
        "rdbg" => "rdbg",
        _ => task_type,
    }
    .to_owned()
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use crate::{DebugScenario, DebugTaskFile, VariableName};

    use super::{VsCodeDebugTaskFile, apply_platform_overrides};

    #[test]
    fn test_platform_overrides_replace_properties() {
        for (operating_system, platform) in
            [("windows", "windows"), ("linux", "linux"), ("macos", "osx")]
        {
            let mut configuration = json!({
                "request": "launch",
                "runtimeExecutable": "default-browser",
                "runtimeArgs": ["--default"],
                "env": {"BASE": "base"},
                "windows": {"runtimeExecutable": "windows-browser"},
                "linux": {"runtimeExecutable": "linux-browser"},
                "osx": {"runtimeExecutable": "osx-browser"},
            });
            configuration[platform]["runtimeArgs"] = json!(["--platform"]);
            configuration[platform]["env"] = json!({"PLATFORM": "platform"});
            apply_platform_overrides(&mut configuration, operating_system);
            assert_eq!(
                configuration,
                json!({
                    "request": "launch",
                    "runtimeExecutable": format!("{platform}-browser"),
                    "runtimeArgs": ["--platform"],
                    "env": {"PLATFORM": "platform"},
                })
            );
        }
    }

    #[test]
    fn test_missing_platform_override_preserves_defaults() {
        let mut configuration = json!({
            "runtimeExecutable": "default-browser",
            "runtimeArgs": ["--default"],
            "windows": {"runtimeExecutable": "windows-browser"},
        });
        apply_platform_overrides(&mut configuration, "linux");
        assert_eq!(
            configuration,
            json!({"runtimeExecutable": "default-browser", "runtimeArgs": ["--default"]})
        );
    }

    #[test]
    fn test_platform_specific_launch_options() -> anyhow::Result<()> {
        let parsed: VsCodeDebugTaskFile = serde_json::from_value(json!({
            "configurations": [{
                "name": "Launch Chrome",
                "type": "chrome",
                "request": "launch",
                "url": "http://localhost:3000/",
                "webRoot": "${workspaceFolder}",
                "runtimeExecutable": "default-browser",
                "runtimeArgs": ["--default"],
                "port": 9222,
                "windows": {
                    "runtimeExecutable": "C:\\Chromium\\chrome.exe",
                    "runtimeArgs": ["--user-data-dir=C:\\Chrome Dev"],
                    "port": 9223,
                },
                "linux": {
                    "runtimeExecutable": "/usr/bin/chromium",
                    "runtimeArgs": ["--user-data-dir=${workspaceFolder}/chrome-dev"],
                    "port": 9224,
                },
                "osx": {
                    "runtimeExecutable": "/Applications/Chromium.app/Contents/MacOS/Chromium",
                    "runtimeArgs": ["--user-data-dir=${workspaceFolder}/chrome-dev"],
                    "port": 9225,
                },
            }],
        }))?;
        let scenarios = DebugTaskFile::try_from(parsed)?;
        let scenario = scenarios
            .0
            .first()
            .expect("Chrome configuration should convert");
        let (executable, arguments, port) = if cfg!(target_os = "windows") {
            (
                "C:\\Chromium\\chrome.exe",
                json!(["--user-data-dir=C:\\Chrome Dev"]),
                9223,
            )
        } else if cfg!(target_os = "macos") {
            (
                "/Applications/Chromium.app/Contents/MacOS/Chromium",
                json!(["--user-data-dir=${ZED_WORKTREE_ROOT}/chrome-dev"]),
                9225,
            )
        } else {
            (
                "/usr/bin/chromium",
                json!(["--user-data-dir=${ZED_WORKTREE_ROOT}/chrome-dev"]),
                9224,
            )
        };
        assert_eq!(scenario.config["runtimeExecutable"], executable);
        assert_eq!(scenario.config["runtimeArgs"], arguments);
        assert_eq!(scenario.config["port"], port);
        assert_eq!(scenario.config["webRoot"], "${ZED_WORKTREE_ROOT}");
        for platform in ["windows", "linux", "osx"] {
            assert!(scenario.config.get(platform).is_none());
        }
        Ok(())
    }

    #[test]
    fn test_parsing_vscode_launch_json() {
        let raw = r#"
            {
                "version": "0.2.0",
                "configurations": [
                    {
                        "name": "Debug my JS app",
                        "request": "launch",
                        "type": "node",
                        "program": "${workspaceFolder}/xyz.js",
                        "showDevDebugOutput": false,
                        "stopOnEntry": true,
                        "args": ["--foo", "${workspaceFolder}/thing"],
                        "cwd": "${workspaceFolder}/${env:FOO}/sub",
                        "env": {
                            "X": "Y"
                        },
                        "port": 17
                    },
                ]
            }
        "#;
        let parsed: VsCodeDebugTaskFile =
            serde_json_lenient::from_str(raw).expect("deserializing launch.json");
        let zed = DebugTaskFile::try_from(parsed).expect("converting to Zed debug templates");
        pretty_assertions::assert_eq!(
            zed,
            DebugTaskFile(vec![DebugScenario {
                label: "Debug my JS app".into(),
                adapter: "JavaScript".into(),
                config: json!({
                    "request": "launch",
                    "program": "${ZED_WORKTREE_ROOT}/xyz.js",
                    "showDevDebugOutput": false,
                    "stopOnEntry": true,
                    "args": [
                        "--foo",
                        "${ZED_WORKTREE_ROOT}/thing",
                    ],
                    "cwd": "${ZED_WORKTREE_ROOT}/${FOO}/sub",
                    "env": {
                        "X": "Y",
                    },
                    "type": "node",
                    "port": 17,
                }),
                tcp_connection: None,
                build: None
            }])
        );
    }

    #[test]
    fn test_command_pickmyprocess_replacement() {
        let raw = r#"
            {
                "version": "0.2.0",
                "configurations": [
                    {
                        "name": "Attach to Process",
                        "request": "attach",
                        "type": "cppdbg",
                        "processId": "${command:pickMyProcess}"
                    }
                ]
            }
        "#;
        let parsed: VsCodeDebugTaskFile =
            serde_json_lenient::from_str(raw).expect("deserializing launch.json");
        let zed = DebugTaskFile::try_from(parsed).expect("converting to Zed debug templates");

        let expected_placeholder = format!("${{{}}}", VariableName::PickProcessId);
        pretty_assertions::assert_eq!(
            zed,
            DebugTaskFile(vec![DebugScenario {
                label: "Attach to Process".into(),
                adapter: "CodeLLDB".into(),
                config: json!({
                    "request": "attach",
                    "processId": expected_placeholder,
                }),
                tcp_connection: None,
                build: None
            }])
        );
    }

    #[test]
    fn test_parsing_bun_launch_json() -> anyhow::Result<()> {
        for task_type in ["bun", "bun-dap-x"] {
            let parsed: VsCodeDebugTaskFile = serde_json::from_value(json!({
                "version": "0.2.0",
                "configurations": [
                    {
                        "name": "Debug Bun file",
                        "type": task_type,
                        "request": "launch",
                        "program": "${file}",
                        "cwd": "${workspaceFolder}",
                        "runtime": "bun",
                    },
                    {
                        "name": "Attach to Bun",
                        "type": task_type,
                        "request": "attach",
                        "host": "127.0.0.1",
                        "port": 6499,
                        "path": "/inspector-id",
                    },
                ],
            }))?;
            let zed = DebugTaskFile::try_from(parsed)?;
            assert_eq!(
                zed,
                DebugTaskFile(vec![
                    DebugScenario {
                        label: "Debug Bun file".into(),
                        adapter: "Bun".into(),
                        config: json!({
                            "type": task_type,
                            "request": "launch",
                            "program": "${ZED_FILE}",
                            "cwd": "${ZED_WORKTREE_ROOT}",
                            "runtime": "bun",
                        }),
                        tcp_connection: None,
                        build: None,
                    },
                    DebugScenario {
                        label: "Attach to Bun".into(),
                        adapter: "Bun".into(),
                        config: json!({
                            "type": task_type,
                            "request": "attach",
                            "host": "127.0.0.1",
                            "port": 6499,
                            "path": "/inspector-id",
                        }),
                        tcp_connection: None,
                        build: None,
                    },
                ])
            );
        }
        Ok(())
    }
}
