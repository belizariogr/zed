use crate::*;
use anyhow::{Context as _, bail};
use collections::HashMap;
use dap::{StartDebuggingRequestArguments, adapters::DebugTaskDefinition};
use gpui::AsyncApp;
use serde_json::Value;
use smol::lock::OnceCell;
use std::{ffi::OsStr, path::Path, path::PathBuf};
use task::DebugRequest;

#[derive(Default)]
pub(crate) struct BunDebugAdapter {
    adapter_path: OnceCell<PathBuf>,
}

impl BunDebugAdapter {
    const ADAPTER_NAME: &'static str = "Bun";
    const PACKAGE_NAME: &'static str = "bun-dap-x";
    const ADAPTER_PATH: &'static str = "node_modules/bun-dap-x/dist/bun-dap-x";

    fn inspector_preload_path() -> PathBuf {
        paths::debug_adapters_dir()
            .join(Self::ADAPTER_NAME)
            .join("bun_inspector.js")
    }

    async fn installed_adapter_path(&self, delegate: &Arc<dyn DapDelegate>) -> Result<PathBuf> {
        self.adapter_path
            .get_or_try_init(|| async {
                let directory = paths::debug_adapters_dir().join(Self::ADAPTER_NAME);
                let adapter_path = directory.join(Self::ADAPTER_PATH);
                delegate
                    .fs()
                    .create_dir(&directory)
                    .await
                    .context("Failed to create Bun debug adapter directory")?;

                delegate
                    .output_to_console("Checking latest version of Bun debug adapter...".into());
                let node_runtime = delegate.node_runtime();
                let installation = async {
                    let version = node_runtime
                        .npm_package_latest_version(Self::PACKAGE_NAME)
                        .await?;
                    let installed_version = node_runtime
                        .npm_package_installed_version(&directory, Self::PACKAGE_NAME)
                        .await?;
                    if installed_version.is_none_or(|installed_version| installed_version < version)
                        || delegate.fs().metadata(&adapter_path).await?.is_none()
                    {
                        delegate.output_to_console(format!(
                            "Installing Bun debug adapter {}...",
                            version
                        ));
                        node_runtime
                            .npm_install_packages(
                                &directory,
                                &[(Self::PACKAGE_NAME, &version.to_string())],
                            )
                            .await?;
                    }
                    anyhow::Ok(())
                }
                .await;

                if let Err(error) = installation {
                    if delegate.fs().metadata(&adapter_path).await?.is_some() {
                        log::warn!(
                            "Failed to update Bun debug adapter, using cached version: {error:#}"
                        );
                        delegate.output_to_console(format!(
                            "Failed to update Bun debug adapter, using cached version: {error:#}"
                        ));
                    } else {
                        return Err(error.context("Failed to install Bun debug adapter"));
                    }
                }

                anyhow::ensure!(
                    delegate.fs().metadata(&adapter_path).await?.is_some(),
                    "Bun debug adapter installation did not produce {}",
                    adapter_path.display()
                );
                delegate
                    .fs()
                    .atomic_write(
                        Self::inspector_preload_path(),
                        include_str!("bun_inspector.js").to_owned(),
                    )
                    .await
                    .context("Failed to prepare Bun inspector source map support")?;
                Ok(adapter_path)
            })
            .await
            .cloned()
    }

    fn runtime(configuration: &Value) -> &str {
        configuration
            .get("runtime")
            .and_then(Value::as_str)
            .filter(|runtime| !runtime.is_empty())
            .or_else(|| {
                configuration
                    .get("runtimeExecutable")
                    .and_then(Value::as_str)
                    .filter(|runtime| !runtime.is_empty())
            })
            .or_else(|| {
                configuration
                    .get("program")
                    .and_then(Value::as_str)
                    .filter(|program| Self::is_bun_executable(program))
            })
            .unwrap_or("bun")
    }

    fn is_bun_executable(program: &str) -> bool {
        program
            .rsplit(['/', '\\'])
            .next()
            .is_some_and(|name| matches!(name, "bun" | "bun.exe"))
    }

    fn configuration(
        mut configuration: Value,
        worktree_root: &Path,
        runtime: Option<&Path>,
    ) -> Result<Value> {
        let object = configuration
            .as_object_mut()
            .context("Bun debug configuration must be an object")?;
        object
            .entry("cwd")
            .or_insert_with(|| Value::String(worktree_root.to_string_lossy().into_owned()));

        if let Some(runtime) = runtime {
            let runtime = Value::String(runtime.to_string_lossy().into_owned());
            // The adapter treats `program: bun` as a command only when no runtime is set.
            if object
                .get("program")
                .and_then(Value::as_str)
                .is_some_and(Self::is_bun_executable)
            {
                if runtime.as_str().is_some_and(Self::is_bun_executable) {
                    object.remove("runtime");
                    object.remove("runtimeExecutable");
                    object.insert("program".into(), runtime);
                } else {
                    let arguments = object
                        .get_mut("args")
                        .and_then(Value::as_array_mut)
                        .filter(|arguments| !arguments.is_empty())
                        .context("A Bun command requires arguments to run or test a program")?;
                    let program = arguments.remove(0);
                    object.insert("program".into(), program);
                    object.insert("runtime".into(), runtime);
                }
            } else {
                object.insert("runtime".into(), runtime);
            }
        }

        Ok(configuration)
    }
}

#[async_trait(?Send)]
impl DebugAdapter for BunDebugAdapter {
    fn name(&self) -> DebugAdapterName {
        Self::ADAPTER_NAME.into()
    }

    async fn config_from_zed_format(&self, zed_scenario: ZedDebugConfig) -> Result<DebugScenario> {
        let DebugRequest::Launch(launch) = zed_scenario.request else {
            bail!(
                "Bun does not support attaching by process ID. Add a Bun attach configuration with the inspector WebSocket `url` to .zed/debug.json."
            );
        };
        let mut configuration = json!({
            "request": "launch",
            "program": launch.program,
            "args": launch.args,
            "env": launch.env_json(),
        });
        if let Some(cwd) = launch.cwd {
            configuration["cwd"] = cwd.to_string_lossy().into_owned().into();
        }
        if let Some(stop_on_entry) = zed_scenario.stop_on_entry {
            configuration["stopOnEntry"] = stop_on_entry.into();
        }
        Ok(DebugScenario {
            adapter: zed_scenario.adapter,
            label: zed_scenario.label,
            build: None,
            config: configuration,
            tcp_connection: None,
        })
    }

    async fn get_binary(
        &self,
        delegate: &Arc<dyn DapDelegate>,
        task_definition: &DebugTaskDefinition,
        user_installed_path: Option<PathBuf>,
        user_args: Option<Vec<String>>,
        user_env: Option<HashMap<String, String>>,
        _: &mut AsyncApp,
    ) -> Result<DebugAdapterBinary> {
        let request = self.request_kind(&task_definition.config).await?;
        if let Some(connection) = task_definition.tcp_connection.clone() {
            anyhow::ensure!(
                connection.port.is_some(),
                "Bun DAP server connection requires a port"
            );
            let (host, port, timeout) = configure_tcp_connection(connection).await?;
            return Ok(DebugAdapterBinary {
                command: None,
                arguments: Vec::new(),
                envs: HashMap::default(),
                cwd: None,
                connection: Some(adapters::TcpArguments {
                    host,
                    port,
                    timeout,
                }),
                request_args: StartDebuggingRequestArguments {
                    configuration: Self::configuration(
                        task_definition.config.clone(),
                        delegate.worktree_root_path(),
                        None,
                    )?,
                    request,
                },
            });
        }
        let runtime = if user_installed_path.is_none()
            || matches!(request, dap::StartDebuggingRequestArgumentsRequest::Launch)
        {
            let runtime = Self::runtime(&task_definition.config);
            Some(delegate.which(OsStr::new(runtime)).await.with_context(|| {
                format!(
                    "Could not find Bun executable `{runtime}`. Install Bun or set `runtime` to its executable path in the debug configuration."
                )
            })?)
        } else {
            None
        };
        let configuration = Self::configuration(
            task_definition.config.clone(),
            delegate.worktree_root_path(),
            if matches!(request, dap::StartDebuggingRequestArgumentsRequest::Launch) {
                runtime.as_deref()
            } else {
                None
            },
        )?;
        let (command, arguments) = if let Some(user_installed_path) = user_installed_path {
            (
                user_installed_path.to_string_lossy().into_owned(),
                user_args.unwrap_or_default(),
            )
        } else {
            let adapter_path = self.installed_adapter_path(delegate).await?;
            let mut arguments = vec![
                "--preload".into(),
                Self::inspector_preload_path()
                    .to_string_lossy()
                    .into_owned(),
                adapter_path.to_string_lossy().into_owned(),
            ];
            arguments.extend(user_args.unwrap_or_default());
            (
                runtime
                    .context("Bun is required to run the debug adapter")?
                    .to_string_lossy()
                    .into_owned(),
                arguments,
            )
        };
        let mut envs = delegate.shell_env().await;
        envs.extend(user_env.unwrap_or_default());

        Ok(DebugAdapterBinary {
            command: Some(command),
            arguments,
            envs,
            cwd: Some(delegate.worktree_root_path().to_path_buf()),
            connection: None,
            request_args: StartDebuggingRequestArguments {
                configuration,
                request,
            },
        })
    }

    fn dap_schema(&self) -> Value {
        json!({
            "oneOf": [
                {
                    "type": "object",
                    "required": ["request", "program"],
                    "properties": {
                        "request": { "type": "string", "enum": ["launch"] },
                        "program": {
                            "type": "string",
                            "description": "File to run with Bun, or the Bun executable for a run or test command"
                        },
                        "cwd": {
                            "type": "string",
                            "description": "Working directory of the program; defaults to the worktree root"
                        },
                        "args": {
                            "type": "array", "items": { "type": "string" },
                            "description": "Arguments passed after the program"
                        },
                        "runtime": {
                            "type": "string", "default": "bun",
                            "description": "Path or name of the Bun executable"
                        },
                        "runtimeExecutable": {
                            "type": "string",
                            "description": "Alias for runtime"
                        },
                        "runtimeArgs": {
                            "type": "array", "items": { "type": "string" },
                            "description": "Arguments passed to Bun before the program"
                        },
                        "env": {
                            "type": "object", "additionalProperties": { "type": "string" },
                            "description": "Environment variables for the program"
                        },
                        "strictEnv": {
                            "type": "boolean", "default": false,
                            "description": "Use only env for the program's environment"
                        },
                        "stopOnEntry": {
                            "type": "boolean", "default": false,
                            "description": "Pause at the program's entry point"
                        },
                        "disableTestTimeout": {
                            "type": "boolean", "default": true,
                            "description": "Disable Bun test timeouts while debugging"
                        }
                    }
                },
                {
                    "type": "object",
                    "required": ["request"],
                    "anyOf": [
                        { "required": ["url"] },
                        { "required": ["inspectorUrl"] },
                        { "required": ["path"] }
                    ],
                    "properties": {
                        "request": { "type": "string", "enum": ["attach"] },
                        "url": {
                            "type": "string",
                            "description": "Full inspector WebSocket URL printed by bun --inspect, including its path"
                        },
                        "inspectorUrl": { "type": "string", "description": "Alias for url" },
                        "host": { "type": "string", "default": "127.0.0.1" },
                        "port": { "type": "integer", "minimum": 1, "maximum": 65535, "default": 6499 },
                        "path": {
                            "type": "string",
                            "description": "Inspector WebSocket path; required when using host and port"
                        },
                        "disableTestTimeout": { "type": "boolean" }
                    }
                }
            ]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_mocks::MockDelegate;
    use gpui::TestAppContext;
    use task::{AttachRequest, LaunchRequest};

    #[gpui::test]
    async fn converts_launch_configuration() {
        let scenario = BunDebugAdapter::default()
            .config_from_zed_format(ZedDebugConfig {
                adapter: "Bun".into(),
                label: "Debug Bun".into(),
                request: DebugRequest::Launch(LaunchRequest {
                    program: "src/main.ts".into(),
                    args: vec!["--flag".into()],
                    cwd: Some("/project".into()),
                    env: [("EXAMPLE".into(), "value".into())].into_iter().collect(),
                }),
                stop_on_entry: Some(true),
            })
            .await
            .expect("valid Bun configuration");
        assert_eq!(
            scenario.config,
            json!({
                "request": "launch",
                "program": "src/main.ts",
                "args": ["--flag"],
                "cwd": "/project",
                "env": { "EXAMPLE": "value" },
                "stopOnEntry": true,
            })
        );
        assert_eq!(scenario.adapter, "Bun");
        assert!(scenario.build.is_none());
        assert!(scenario.tcp_connection.is_none());
    }

    #[gpui::test]
    async fn rejects_process_id_attach() {
        let error = BunDebugAdapter::default()
            .config_from_zed_format(ZedDebugConfig {
                adapter: "Bun".into(),
                label: "Attach".into(),
                request: DebugRequest::Attach(AttachRequest {
                    process_id: Some(123),
                }),
                stop_on_entry: None,
            })
            .await
            .expect_err("Bun requires an inspector URL for attach");
        assert!(error.to_string().contains("WebSocket `url`"));
    }

    #[test]
    fn normalizes_runtime_and_preserves_launch_options() -> Result<()> {
        let configuration = BunDebugAdapter::configuration(
            json!({
                "request": "launch",
                "program": "src/main.ts",
                "runtimeExecutable": "custom-bun",
                "runtimeArgs": ["--smol"],
                "args": ["argument with spaces"],
                "cwd": "/custom",
                "env": { "VALUE": "test" },
                "stopOnEntry": true,
            }),
            Path::new("/project"),
            Some(Path::new("/tools/custom-bun")),
        )?;
        assert_eq!(
            configuration.get("runtime"),
            Some(&json!("/tools/custom-bun"))
        );
        assert_eq!(configuration.get("runtimeArgs"), Some(&json!(["--smol"])));
        assert_eq!(
            configuration.get("args"),
            Some(&json!(["argument with spaces"]))
        );
        assert_eq!(configuration.get("cwd"), Some(&json!("/custom")));
        assert_eq!(configuration.get("env"), Some(&json!({ "VALUE": "test" })));
        assert_eq!(configuration.get("stopOnEntry"), Some(&json!(true)));
        Ok(())
    }

    #[test]
    fn preserves_bun_run_and_test_commands() -> Result<()> {
        for program in ["bun", "bun.exe", "/tools/bun", "C:\\tools\\bun.exe"] {
            let input = json!({
                "request": "launch",
                "program": program,
                "args": ["test", "src/example.test.ts", "--test-name-pattern", "some test"],
            });
            assert_eq!(BunDebugAdapter::runtime(&input), program);
            let configuration = BunDebugAdapter::configuration(
                input,
                Path::new("/project"),
                Some(Path::new("/resolved/bun")),
            )?;
            assert_eq!(configuration.get("program"), Some(&json!("/resolved/bun")));
            assert!(configuration.get("runtime").is_none());
            assert_eq!(configuration.get("cwd"), Some(&json!("/project")));
            assert_eq!(
                configuration.get("args"),
                Some(&json!([
                    "test",
                    "src/example.test.ts",
                    "--test-name-pattern",
                    "some test"
                ]))
            );
        }
        Ok(())
    }

    #[test]
    fn bun_command_with_explicit_runtime_runs_once() -> Result<()> {
        for runtime_key in ["runtime", "runtimeExecutable"] {
            let mut input = json!({
                "request": "launch", "program": "bun", "args": ["test", "example.test.ts"]
            });
            input[runtime_key] = "/custom/bun".into();
            let configuration = BunDebugAdapter::configuration(
                input,
                Path::new("/project"),
                Some(Path::new("/custom/bun")),
            )?;
            assert_eq!(configuration.get("program"), Some(&json!("/custom/bun")));
            assert!(configuration.get("runtime").is_none());
            assert!(configuration.get("runtimeExecutable").is_none());
            assert_eq!(
                configuration.get("args"),
                Some(&json!(["test", "example.test.ts"]))
            );
        }
        let configuration = BunDebugAdapter::configuration(
            json!({"request": "launch", "program": "bun", "args": ["test", "example.test.ts"]}),
            Path::new("/project"),
            Some(Path::new("/custom/renamed-bun")),
        )?;
        assert_eq!(configuration.get("program"), Some(&json!("test")));
        assert_eq!(
            configuration.get("runtime"),
            Some(&json!("/custom/renamed-bun"))
        );
        assert_eq!(configuration.get("args"), Some(&json!(["example.test.ts"])));
        Ok(())
    }

    #[test]
    fn runtime_alias_handles_empty_or_null_runtime() {
        for runtime in [Value::Null, json!("")] {
            assert_eq!(
                BunDebugAdapter::runtime(&json!({
                    "runtime": runtime, "runtimeExecutable": "alternate-bun"
                })),
                "alternate-bun"
            );
        }
    }

    #[gpui::test]
    async fn managed_adapter_uses_bun_and_inspector_preload(cx: &mut TestAppContext) {
        let delegate = MockDelegate::with_commands_and_env(
            [("bun".into(), PathBuf::from("/tools/bun"))]
                .into_iter()
                .collect(),
            HashMap::default(),
        );
        let adapter = BunDebugAdapter::default();
        let adapter_path = PathBuf::from("/cache/bun-dap-x/dist/bun-dap-x");
        adapter
            .adapter_path
            .set(adapter_path.clone())
            .await
            .expect("empty cache");
        let binary = adapter
            .get_binary(
                &delegate,
                &DebugTaskDefinition {
                    label: "Bun".into(),
                    adapter: "Bun".into(),
                    config: json!({"request": "launch", "program": "main.ts"}),
                    tcp_connection: None,
                },
                None,
                None,
                None,
                &mut cx.to_async(),
            )
            .await
            .expect("cached adapter");
        assert_eq!(binary.command.as_deref(), Some("/tools/bun"));
        assert_eq!(
            binary.arguments,
            vec![
                "--preload".to_owned(),
                BunDebugAdapter::inspector_preload_path()
                    .to_string_lossy()
                    .into_owned(),
                adapter_path.to_string_lossy().into_owned(),
            ]
        );
        assert!(binary.connection.is_none());
    }

    #[gpui::test]
    async fn external_dap_connection_skips_local_installation(cx: &mut TestAppContext) {
        let configuration = json!({"request": "attach", "url": "ws://remote:6499/inspector-id"});
        let binary = BunDebugAdapter::default()
            .get_binary(
                &MockDelegate::new(),
                &DebugTaskDefinition {
                    label: "Bun".into(),
                    adapter: "Bun".into(),
                    config: configuration.clone(),
                    tcp_connection: Some(task::TcpArgumentsTemplate {
                        port: Some(4711),
                        timeout: Some(5000),
                        ..Default::default()
                    }),
                },
                None,
                None,
                None,
                &mut cx.to_async(),
            )
            .await
            .expect("existing DAP server");
        assert!(binary.command.is_none());
        assert!(binary.arguments.is_empty());
        let connection = binary.connection.expect("TCP connection");
        assert_eq!(connection.port, 4711);
        assert_eq!(connection.timeout, Some(5000));
        assert_eq!(
            binary.request_args.configuration.get("url"),
            configuration.get("url")
        );
    }

    #[gpui::test]
    async fn respects_custom_adapter_and_attach_url(cx: &mut TestAppContext) {
        let delegate = MockDelegate::with_commands_and_env(
            HashMap::default(),
            [
                ("PATH".into(), "/tools".into()),
                ("VALUE".into(), "shell".into()),
            ]
            .into_iter()
            .collect(),
        );
        let configuration = json!({
            "request": "attach",
            "url": "ws://127.0.0.1:6499/inspector-id",
            "port": 6499,
        });
        let binary = BunDebugAdapter::default()
            .get_binary(
                &delegate,
                &DebugTaskDefinition {
                    label: "Attach Bun".into(),
                    adapter: "Bun".into(),
                    config: configuration.clone(),
                    tcp_connection: None,
                },
                Some("/tools/custom-adapter".into()),
                Some(vec!["--stdio".into()]),
                Some([("VALUE".into(), "override".into())].into_iter().collect()),
                &mut cx.to_async(),
            )
            .await
            .expect("valid Bun configuration");
        assert_eq!(binary.command.as_deref(), Some("/tools/custom-adapter"));
        assert_eq!(binary.arguments, ["--stdio"]);
        assert_eq!(binary.envs.get("PATH").map(String::as_str), Some("/tools"));
        assert_eq!(
            binary.envs.get("VALUE").map(String::as_str),
            Some("override")
        );
        assert!(binary.connection.is_none());
        assert_eq!(
            binary.request_args.request,
            dap::StartDebuggingRequestArgumentsRequest::Attach
        );
        assert_eq!(
            binary.request_args.configuration.get("url"),
            configuration.get("url")
        );
        assert_eq!(
            binary.request_args.configuration.get("port"),
            Some(&json!(6499))
        );
    }

    #[gpui::test]
    async fn custom_adapter_launch_resolves_bun(cx: &mut TestAppContext) {
        let delegate = MockDelegate::with_commands_and_env(
            [("alternate-bun".into(), PathBuf::from("/tools/bun"))]
                .into_iter()
                .collect(),
            HashMap::default(),
        );
        let binary = BunDebugAdapter::default()
            .get_binary(
                &delegate,
                &DebugTaskDefinition {
                    label: "Launch Bun".into(),
                    adapter: "Bun".into(),
                    config: json!({
                        "request": "launch", "program": "app.ts", "runtime": "alternate-bun"
                    }),
                    tcp_connection: None,
                },
                Some("/tools/adapter".into()),
                None,
                None,
                &mut cx.to_async(),
            )
            .await
            .expect("valid Bun configuration");
        assert_eq!(
            binary.request_args.configuration.get("runtime"),
            Some(&json!("/tools/bun"))
        );
        assert_eq!(
            binary.request_args.configuration.get("cwd"),
            Some(&json!("/tmp/test"))
        );
    }

    #[gpui::test]
    async fn reports_missing_bun_before_installing(cx: &mut TestAppContext) {
        let error = BunDebugAdapter::default()
            .get_binary(
                &MockDelegate::new(),
                &DebugTaskDefinition {
                    label: "Bun".into(),
                    adapter: "Bun".into(),
                    config: json!({"request": "launch", "program": "main.ts"}),
                    tcp_connection: None,
                },
                None,
                None,
                None,
                &mut cx.to_async(),
            )
            .await
            .expect_err("missing Bun must return an actionable error");
        assert!(error.to_string().contains("Install Bun"));
    }
}
