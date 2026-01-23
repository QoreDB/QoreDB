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
    use super::*;
    use crate::engine::types::{
        CollectionList, CollectionListOptions, ConnectionConfig, Namespace, QueryId, QueryResult,
        SessionId, TableSchema, Value,
    };
    use crate::engine::error::EngineResult;
    use async_trait::async_trait;

    #[derive(Debug)]
    struct MockDriver {
        id: &'static str,
    }

    impl MockDriver {
        fn new(id: &'static str) -> Self {
            Self { id }
        }
    }

    #[async_trait]
    impl DataEngine for MockDriver {
        fn driver_id(&self) -> &'static str {
            self.id
        }

        fn driver_name(&self) -> &'static str {
            "Mock Driver"
        }

        async fn test_connection(&self, _config: &ConnectionConfig) -> EngineResult<()> {
            Ok(())
        }

        async fn connect(&self, _config: &ConnectionConfig) -> EngineResult<SessionId> {
            Ok(SessionId::new())
        }

        async fn disconnect(&self, _session: SessionId) -> EngineResult<()> {
            Ok(())
        }

        async fn list_namespaces(&self, _session: SessionId) -> EngineResult<Vec<Namespace>> {
            Ok(vec![])
        }

        async fn list_collections(
            &self,
            _session: SessionId,
            _namespace: &Namespace,
            _options: CollectionListOptions,
        ) -> EngineResult<CollectionList> {
            Ok(CollectionList {
                collections: vec![],
                total_count: 0,
            })
        }

        async fn create_database(
            &self,
            _session: SessionId,
            _name: &str,
            _options: Option<Value>,
        ) -> EngineResult<()> {
            Ok(())
        }

        async fn drop_database(&self, _session: SessionId, _name: &str) -> EngineResult<()> {
            Ok(())
        }

        async fn execute(
            &self,
            _session: SessionId,
            _query: &str,
            _query_id: QueryId,
        ) -> EngineResult<QueryResult> {
            Ok(QueryResult::empty())
        }

        async fn describe_table(
            &self,
            _session: SessionId,
            _namespace: &Namespace,
            _table: &str,
        ) -> EngineResult<TableSchema> {
            Ok(TableSchema {
                columns: vec![],
                primary_key: None,
                foreign_keys: vec![],
                row_count_estimate: None,
            })
        }

        async fn preview_table(
            &self,
            _session: SessionId,
            _namespace: &Namespace,
            _table: &str,
            _limit: u32,
        ) -> EngineResult<QueryResult> {
            Ok(QueryResult::empty())
        }
    }

    #[test]
    fn test_registry_basics() {
        let mut registry = DriverRegistry::new();
        assert!(registry.is_empty());

        registry.register(Arc::new(MockDriver::new("mock1")));
        assert_eq!(registry.len(), 1);
        assert!(!registry.is_empty());

        registry.register(Arc::new(MockDriver::new("mock2")));
        assert_eq!(registry.len(), 2);

        assert!(registry.get("mock1").is_some());
        assert!(registry.get("mock2").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_list_drivers() {
        let mut registry = DriverRegistry::new();
        registry.register(Arc::new(MockDriver::new("a")));
        registry.register(Arc::new(MockDriver::new("b")));

        let list = registry.list();
        assert!(list.contains(&"a"));
        assert!(list.contains(&"b"));
        assert_eq!(list.len(), 2);
    }

    #[test]
    fn test_list_infos() {
        let mut registry = DriverRegistry::new();
        registry.register(Arc::new(MockDriver::new("test")));

        let infos = registry.list_infos();
        assert_eq!(infos.len(), 1);
        assert_eq!(infos[0].id, "test");
        assert_eq!(infos[0].name, "Mock Driver");
    }
}
