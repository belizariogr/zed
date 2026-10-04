use std::borrow::Cow;

use anyhow::{Result, bail};
use async_trait::async_trait;
use dap::{DapLocator, DebugRequest, adapters::DebugAdapterName};
use gpui::{BackgroundExecutor, SharedString};

use task::{DebugScenario, SpawnInTerminal, TaskTemplate, VariableName};

pub(crate) struct NodeLocator;
pub(crate) struct BunLocator;

const TYPESCRIPT_RUNNER_VARIABLE: VariableName =
    VariableName::Custom(Cow::Borrowed("TYPESCRIPT_RUNNER"));

#[async_trait]
impl DapLocator for NodeLocator {
    fn name(&self) -> SharedString {
        SharedString::new_static("Node")
    }

    /// Determines whether this locator can generate debug target for given task.
    async fn create_scenario(
        &self,
        build_config: &TaskTemplate,
        resolved_label: &str,
        adapter: &DebugAdapterName,
    ) -> Option<DebugScenario> {
        if adapter.0.as_ref() != "JavaScript" {
            return None;
        }
        if build_config.command != TYPESCRIPT_RUNNER_VARIABLE.template_value()
            && build_config.command != "npm"
            && build_config.command != "pnpm"
            && build_config.command != "yarn"
        {
            return None;
        }

        let config = serde_json::json!({
            "request": "launch",
            "type": "pwa-node",
            "args": build_config.args.clone(),
            "cwd": build_config.cwd.clone(),
            "runtimeExecutable": build_config.command.clone(),
            "env": build_config.env.clone(),
            "runtimeArgs": ["--inspect-brk"],
            "console": "integratedTerminal",
        });

        Some(DebugScenario {
            adapter: adapter.0.clone(),
            label: resolved_label.to_string().into(),
            build: None,
            config,
            tcp_connection: None,
        })
    }

    async fn run(&self, _: SpawnInTerminal, _executor: BackgroundExecutor) -> Result<DebugRequest> {
        bail!("JavaScript locator should not require DapLocator::run to be ran");
    }
}

#[async_trait]
impl DapLocator for BunLocator {
    fn name(&self) -> SharedString {
        SharedString::new_static("Bun")
    }

    async fn create_scenario(
        &self,
        build_config: &TaskTemplate,
        resolved_label: &str,
        adapter: &DebugAdapterName,
    ) -> Option<DebugScenario> {
        if adapter.0.as_ref() != "Bun"
            || !matches!(build_config.command.as_str(), "bun" | "bun.exe")
            || build_config.args.is_empty()
        {
            return None;
        }

        // Built-in tasks quote variable templates for the shell, but the adapter uses argv.
        let args: Vec<_> = build_config
            .args
            .iter()
            .map(|argument| {
                if argument.starts_with("\"$ZED_") {
                    argument
                        .strip_prefix('"')
                        .and_then(|argument| argument.strip_suffix('"'))
                        .unwrap_or(argument)
                } else if argument.starts_with("'$ZED_") {
                    argument
                        .strip_prefix('\'')
                        .and_then(|argument| argument.strip_suffix('\''))
                        .unwrap_or(argument)
                } else {
                    argument
                }
            })
            .collect();

        Some(DebugScenario {
            adapter: adapter.0.clone(),
            label: resolved_label.to_string().into(),
            build: None,
            config: serde_json::json!({
                "request": "launch",
                "program": build_config.command,
                "args": args,
                "cwd": build_config.cwd,
                "env": build_config.env,
            }),
            tcp_connection: None,
        })
    }

    async fn run(&self, _: SpawnInTerminal, _executor: BackgroundExecutor) -> Result<DebugRequest> {
        bail!("Bun locator should not require DapLocator::run to be ran");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[gpui::test]
    async fn bun_locator_preserves_run_and_test_arguments() {
        for args in [
            vec!["run", "dev", "--port", "3000"],
            vec!["test", "src/example.test.ts"],
            vec!["src/main.ts", "argument with spaces"],
            vec!["-e", "\"hello\""],
        ] {
            let build_config = TaskTemplate {
                command: "bun".to_owned(),
                args: args.iter().map(|argument| argument.to_string()).collect(),
                cwd: Some(VariableName::WorktreeRoot.template_value()),
                env: [("NODE_ENV".to_owned(), "test".to_owned())]
                    .into_iter()
                    .collect(),
                ..TaskTemplate::default()
            };
            let scenario = BunLocator
                .create_scenario(&build_config, "Debug Bun", &DebugAdapterName("Bun".into()))
                .await
                .expect("Bun task should produce a debug scenario");

            assert_eq!(scenario.label.as_ref(), "Debug Bun");
            assert_eq!(scenario.adapter.as_ref(), "Bun");
            assert!(scenario.build.is_none());
            assert_eq!(
                scenario.config,
                serde_json::json!({
                    "request": "launch",
                    "program": "bun",
                    "args": args,
                    "cwd": VariableName::WorktreeRoot.template_value(),
                    "env": { "NODE_ENV": "test" },
                })
            );
        }
    }

    #[gpui::test]
    async fn bun_locator_removes_shell_quotes_from_test_pattern() {
        let test_name = VariableName::Custom(Cow::Borrowed("TYPESCRIPT_BUN_TEST_NAME"));
        let build_config = TaskTemplate {
            command: "bun.exe".to_owned(),
            args: vec![
                "test".to_owned(),
                "--test-name-pattern".to_owned(),
                test_name.template_value_with_whitespace(),
                VariableName::File.template_value_with_whitespace(),
            ],
            ..TaskTemplate::default()
        };
        let scenario = BunLocator
            .create_scenario(
                &build_config,
                "Debug Bun test",
                &DebugAdapterName("Bun".into()),
            )
            .await
            .expect("Bun test task should produce a debug scenario");

        assert_eq!(scenario.config["program"], "bun.exe");
        assert_eq!(
            scenario.config["args"],
            serde_json::json!([
                "test",
                "--test-name-pattern",
                test_name.template_value(),
                VariableName::File.template_value(),
            ])
        );
    }

    #[gpui::test]
    async fn bun_locator_does_not_claim_other_runtimes() {
        for command in [
            "node",
            "npm",
            "pnpm",
            "yarn",
            &TYPESCRIPT_RUNNER_VARIABLE.template_value(),
        ] {
            let build_config = TaskTemplate {
                command: command.to_owned(),
                args: vec!["run".to_owned(), "dev".to_owned()],
                ..TaskTemplate::default()
            };
            assert!(
                BunLocator
                    .create_scenario(&build_config, "Debug task", &DebugAdapterName("Bun".into()))
                    .await
                    .is_none()
            );
        }

        let build_config = TaskTemplate {
            command: "bun".to_owned(),
            args: vec!["test".to_owned()],
            ..TaskTemplate::default()
        };
        assert!(
            BunLocator
                .create_scenario(
                    &build_config,
                    "Debug task",
                    &DebugAdapterName("JavaScript".into()),
                )
                .await
                .is_none()
        );
        assert!(
            NodeLocator
                .create_scenario(&build_config, "Debug task", &DebugAdapterName("Bun".into()))
                .await
                .is_none()
        );
    }
}
