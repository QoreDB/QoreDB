// SPDX-License-Identifier: Apache-2.0

//! Driver Registry
//!
//! Central registry for all available database drivers.
//! Provides plugin-like architecture for adding new drivers.

use std::collections::HashMap;
use std::sync::Arc;

use crate::engine::traits::DataEngine;
use crate::engine::types::DriverInfo;

/// Registry that holds all available database drivers
pub struct DriverRegistry {
    drivers: HashMap<String, Arc<dyn DataEngine>>,
}

impl DriverRegistry {
    /// Creates a new empty registry
    pub fn new() -> Self {
        Self {
            drivers: HashMap::new(),
        }
    }

    /// Registers a new driver
    ///
    /// The driver's `driver_id()` is used as the key.
    pub fn register(&mut self, driver: Arc<dyn DataEngine>) {
        let id = driver.driver_id().to_string();
        self.drivers.insert(id, driver);
    }

    /// Gets a driver by its ID
    pub fn get(&self, driver_id: &str) -> Option<Arc<dyn DataEngine>> {
        self.drivers.get(driver_id).cloned()
    }

    /// Lists all registered driver IDs
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

    /// Returns the number of registered drivers
    pub fn len(&self) -> usize {
        self.drivers.len()
    }

    /// Returns true if no drivers are registered
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
mod tests {
    // Tests will be added when we have mock drivers
}
