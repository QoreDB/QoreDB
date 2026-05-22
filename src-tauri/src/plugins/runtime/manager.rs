// SPDX-License-Identifier: Apache-2.0

//! Plugin host: loads executable plugins and dispatches hooks to them.
//!
//! Owns the WASM runtime and the set of loaded plugin instances. A hook runs
//! against every enabled executable plugin; a plugin that errors is logged and
//! skipped — it can never block a query by failing.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use super::{Budget, Decision, HookContext, PluginInstance, PluginRuntime, WasmiRuntime};
use crate::plugins::{plugins_dir, registry};

/// Loads executable plugins and runs their hooks.
pub struct PluginHost {
    runtime: Arc<dyn PluginRuntime>,
    /// Loaded instances, keyed by plugin id.
    instances: Mutex<HashMap<String, Box<dyn PluginInstance>>>,
}

impl PluginHost {
    pub fn new() -> Self {
        Self {
            runtime: Arc::new(WasmiRuntime::new()),
            instances: Mutex::new(HashMap::new()),
        }
    }

    /// Rescans the plugins directory and (re)loads every enabled, compatible
    /// executable plugin. Called at startup and whenever plugins change.
    pub fn reload(&self) {
        let dir = plugins_dir();
        let mut instances = self.instances.lock().unwrap();
        instances.clear();
        for plugin in registry::list_plugins(&dir) {
            if !plugin.enabled || !plugin.compatible {
                continue;
            }
            let Some(runtime) = &plugin.manifest.runtime else {
                continue;
            };
            let wasm_path = dir.join(&plugin.dir_name).join(&runtime.entry);
            let wasm = match std::fs::read(&wasm_path) {
                Ok(bytes) => bytes,
                Err(e) => {
                    tracing::warn!(
                        plugin = %plugin.manifest.id,
                        error = %e,
                        "could not read plugin WASM module"
                    );
                    continue;
                }
            };
            match self.runtime.load(&wasm, Budget::default()) {
                Ok(instance) => {
                    instances.insert(plugin.manifest.id.clone(), instance);
                }
                Err(e) => {
                    tracing::warn!(
                        plugin = %plugin.manifest.id,
                        error = %e,
                        "could not load plugin"
                    );
                }
            }
        }
    }

    /// Runs the `pre_execute` hook of every loaded plugin and aggregates the
    /// verdicts: any `Block` wins; otherwise the first `Warn`; else `Allow`.
    pub fn run_pre_execute(&self, context: &HookContext) -> Decision {
        let mut instances = self.instances.lock().unwrap();
        let mut warning: Option<String> = None;
        for (id, instance) in instances.iter_mut() {
            match instance.pre_execute(context) {
                Ok(Decision::Allow) => {}
                Ok(Decision::Warn { message }) => {
                    warning.get_or_insert(message);
                }
                Ok(Decision::Block { reason }) => return Decision::Block { reason },
                Err(e) => {
                    tracing::warn!(plugin = %id, error = %e, "plugin pre_execute hook failed");
                }
            }
        }
        match warning {
            Some(message) => Decision::Warn { message },
            None => Decision::Allow,
        }
    }

    /// Number of currently loaded executable plugins.
    pub fn loaded_count(&self) -> usize {
        self.instances.lock().unwrap().len()
    }
}

impl Default for PluginHost {
    fn default() -> Self {
        Self::new()
    }
}
