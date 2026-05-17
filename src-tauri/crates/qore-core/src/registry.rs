// SPDX-License-Identifier: Apache-2.0

//! Central registry of database drivers, keyed by `driver_id()`.

use std::collections::HashMap;
use std::sync::Arc;

use crate::traits::DataEngine;
use crate::types::DriverInfo;

/// Registry that holds all available database drivers
pub struct DriverRegistry {
    drivers: HashMap<String, Arc<dyn DataEngine>>,
}

impl DriverRegistry {
    pub fn new() -> Self {
        Self {
            drivers: HashMap::new(),
        }
    }

    /// Registers a driver. The driver's `driver_id()` is used as the key.
    pub fn register(&mut self, driver: Arc<dyn DataEngine>) {
        let id = driver.driver_id().to_string();
        self.drivers.insert(id, driver);
    }

    pub fn get(&self, driver_id: &str) -> Option<Arc<dyn DataEngine>> {
        self.drivers.get(driver_id).cloned()
    }

    pub fn list(&self) -> Vec<&str> {
        self.drivers.keys().map(|s| s.as_str()).collect()
    }

    /// Lists all registered drivers with their metadata.
    pub fn list_infos(&self) -> Vec<DriverInfo> {
        let mut infos: Vec<DriverInfo> = self
            .drivers
            .values()
            .map(|driver| DriverInfo {
                id: driver.driver_id().to_string(),
                name: driver.driver_name().to_string(),
                capabilities: driver.capabilities(),
            })
            .collect();
        infos.sort_by(|a, b| a.id.cmp(&b.id));
        infos
    }

    pub fn len(&self) -> usize {
        self.drivers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.drivers.is_empty()
    }
}

impl Default for DriverRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {}
