use anyhow::Result;
use async_trait::async_trait;
use collections::FxHashMap;
use gpui::{App, BackgroundExecutor, Global, SharedString};
use language::LanguageName;
use parking_lot::RwLock;
use task::{
    AdapterSchema, AdapterSchemas, DebugRequest, DebugScenario, SpawnInTerminal, TaskTemplate,
};

use crate::adapters::{DebugAdapter, DebugAdapterName};
use std::{collections::BTreeMap, sync::Arc};

/// Given a user build configuration, locator creates a fill-in debug target ([DebugScenario]) on behalf of the user.
#[async_trait]
pub trait DapLocator: Send + Sync {
    fn name(&self) -> SharedString;
    /// Determines whether this locator can generate debug target for given task.
    async fn create_scenario(
        &self,
        build_config: &TaskTemplate,
        resolved_label: &str,
        adapter: &DebugAdapterName,
    ) -> Option<DebugScenario>;

    async fn run(
        &self,
        build_config: SpawnInTerminal,
        executor: BackgroundExecutor,
    ) -> Result<DebugRequest>;
}

#[derive(Default)]
struct DapRegistryState {
    adapters: BTreeMap<DebugAdapterName, Arc<dyn DebugAdapter>>,
    locators: FxHashMap<SharedString, Arc<dyn DapLocator>>,
}

#[derive(Clone, Default)]
/// Stores available debug adapters.
pub struct DapRegistry(Arc<RwLock<DapRegistryState>>);
impl Global for DapRegistry {}

impl DapRegistry {
    pub fn global(cx: &mut App) -> &mut Self {
        cx.default_global::<Self>()
    }

    pub fn add_adapter(&self, adapter: Arc<dyn DebugAdapter>) {
        let name = adapter.name();
        let _previous_value = self.0.write().adapters.insert(name, adapter);
    }

    pub fn add_locator(&self, locator: Arc<dyn DapLocator>) {
        self.0.write().locators.insert(locator.name(), locator);
    }

    pub fn remove_adapter(&self, name: &str) {
        self.0.write().adapters.remove(name);
    }

    pub fn remove_locator(&self, locator: &str) {
        self.0.write().locators.remove(locator);
    }

    pub fn adapter_language(&self, adapter_name: &str) -> Option<LanguageName> {
        self.adapter(adapter_name)
            .and_then(|adapter| adapter.adapter_language_name())
    }

    pub fn adapters_schema(&self) -> task::AdapterSchemas {
        let mut schemas = vec![];

        let adapters = &self.0.read().adapters;

        for (name, adapter) in adapters.into_iter() {
            schemas.push(AdapterSchema {
                adapter: name.clone().into(),
                schema: adapter.dap_schema(),
            });
        }

        AdapterSchemas(schemas)
    }

    pub fn locators(&self) -> FxHashMap<SharedString, Arc<dyn DapLocator>> {
        self.0.read().locators.clone()
    }

    pub async fn debug_scenario_for_task(
        &self,
        task: &TaskTemplate,
        label: &str,
        adapters: &[DebugAdapterName],
    ) -> Option<DebugScenario> {
        let locators = self.locators();
        for adapter in adapters {
            for locator in locators.values() {
                if let Some(scenario) = locator.create_scenario(task, label, adapter).await {
                    return Some(scenario);
                }
            }
        }
        None
    }

    pub fn adapter(&self, name: &str) -> Option<Arc<dyn DebugAdapter>> {
        self.0.read().adapters.get(name).cloned()
    }

    pub fn enumerate_adapters<B: FromIterator<DebugAdapterName>>(&self) -> B {
        self.0.read().adapters.keys().cloned().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestLocator {
        adapter: DebugAdapterName,
        command: &'static str,
    }

    #[async_trait]
    impl DapLocator for TestLocator {
        fn name(&self) -> SharedString {
            self.adapter.0.clone()
        }

        async fn create_scenario(
            &self,
            task: &TaskTemplate,
            label: &str,
            adapter: &DebugAdapterName,
        ) -> Option<DebugScenario> {
            if adapter != &self.adapter || task.command != self.command {
                return None;
            }
            Some(DebugScenario {
                adapter: adapter.0.clone(),
                label: label.to_owned().into(),
                build: None,
                config: serde_json::json!({"request": "launch", "program": task.command}),
                tcp_connection: None,
            })
        }

        async fn run(&self, _: SpawnInTerminal, _: BackgroundExecutor) -> Result<DebugRequest> {
            anyhow::bail!("Test locator does not run build tasks")
        }
    }

    #[gpui::test]
    async fn debug_scenario_for_task_falls_back_to_matching_adapter() {
        let registry = DapRegistry::default();
        registry.add_locator(Arc::new(TestLocator {
            adapter: DebugAdapterName("JavaScript".into()),
            command: "npm",
        }));
        registry.add_locator(Arc::new(TestLocator {
            adapter: DebugAdapterName("Bun".into()),
            command: "bun",
        }));

        for (command, expected_adapter) in [("npm", "JavaScript"), ("bun", "Bun")] {
            let task = TaskTemplate {
                command: command.to_owned(),
                ..TaskTemplate::default()
            };
            let scenario = registry
                .debug_scenario_for_task(
                    &task,
                    "Debug task",
                    &[
                        DebugAdapterName("JavaScript".into()),
                        DebugAdapterName("Bun".into()),
                    ],
                )
                .await
                .expect("A matching adapter should produce a scenario");
            assert_eq!(scenario.adapter.as_ref(), expected_adapter);
        }
    }

    #[gpui::test]
    async fn debug_scenario_for_task_preserves_adapter_preference() {
        let registry = DapRegistry::default();
        for adapter in ["first", "second"] {
            registry.add_locator(Arc::new(TestLocator {
                adapter: DebugAdapterName(adapter.into()),
                command: "runtime",
            }));
        }
        let task = TaskTemplate {
            command: "runtime".to_owned(),
            ..TaskTemplate::default()
        };
        for adapters in [["first", "second"], ["second", "first"]] {
            let scenario = registry
                .debug_scenario_for_task(
                    &task,
                    "Debug task",
                    &adapters.map(|adapter| DebugAdapterName(adapter.into())),
                )
                .await
                .expect("Both adapters should match the task");
            assert_eq!(scenario.adapter.as_ref(), adapters[0]);
        }
    }
}
