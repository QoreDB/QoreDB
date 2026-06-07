// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use qore_core::DriverRegistry;
use qore_drivers::drivers::{
    clickhouse::ClickHouseDriver, cockroachdb::CockroachDbDriver, duckdb::DuckDbDriver,
    mariadb::MariaDbDriver, mongodb::MongoDriver, mysql::MySqlDriver, neon::NeonDriver,
    postgres::PostgresDriver, redis::RedisDriver, sqlite::SqliteDriver, sqlserver::SqlServerDriver,
    supabase::SupabaseDriver, timescaledb::TimescaleDbDriver,
};
use qore_drivers::query_manager::QueryManager;
use qore_drivers::session_manager::SessionManager;

use crate::cache::QueryCache;
use crate::interceptor::InterceptorPipeline;
use crate::license::LicenseManager;
use crate::policy::SafetyPolicy;
use crate::ratelimit::QueryRateLimiter;
use crate::vault::backend::default_provider;
use crate::vault::VaultLock;
use crate::virtual_relations::VirtualRelationStore;

pub struct ServiceContext {
    pub registry: Arc<DriverRegistry>,
    pub session_manager: Arc<SessionManager>,
    pub query_manager: Arc<QueryManager>,
    pub query_rate_limiter: Arc<QueryRateLimiter>,
    pub query_cache: Arc<QueryCache>,
    pub policy: SafetyPolicy,
    pub interceptor: Arc<InterceptorPipeline>,
    pub virtual_relations: Arc<VirtualRelationStore>,
    pub vault_lock: VaultLock,
    pub license_manager: LicenseManager,
}

impl ServiceContext {
    pub fn new() -> Self {
        let mut registry = DriverRegistry::new();
        registry.register(Arc::new(PostgresDriver::new()));
        registry.register(Arc::new(MySqlDriver::new()));
        registry.register(Arc::new(MongoDriver::new()));
        registry.register(Arc::new(RedisDriver::new()));
        registry.register(Arc::new(SqliteDriver::new()));
        registry.register(Arc::new(DuckDbDriver::new()));
        registry.register(Arc::new(CockroachDbDriver::new()));
        registry.register(Arc::new(SqlServerDriver::new()));
        registry.register(Arc::new(MariaDbDriver::new()));
        registry.register(Arc::new(SupabaseDriver::new()));
        registry.register(Arc::new(NeonDriver::new()));
        registry.register(Arc::new(TimescaleDbDriver::new()));
        registry.register(Arc::new(ClickHouseDriver::new()));

        let registry = Arc::new(registry);
        let session_manager = Arc::new(SessionManager::new(Arc::clone(&registry)));
        let mut vault_lock = VaultLock::new(default_provider());
        let policy = SafetyPolicy::load();
        let query_manager = Arc::new(QueryManager::new());

        let data_dir = crate::paths::app_data_dir();
        let interceptor = Arc::new(InterceptorPipeline::new(data_dir.join("interceptor")));
        let _ = interceptor.load_config();
        let virtual_relations =
            Arc::new(VirtualRelationStore::new(data_dir.join("virtual_relations")));

        let _ = vault_lock.auto_unlock_if_no_password();
        let license_manager = LicenseManager::new(default_provider());

        Self {
            registry,
            session_manager,
            query_manager,
            query_rate_limiter: Arc::new(QueryRateLimiter::with_defaults()),
            query_cache: Arc::new(QueryCache::new()),
            policy,
            interceptor,
            virtual_relations,
            vault_lock,
            license_manager,
        }
    }
}

impl Default for ServiceContext {
    fn default() -> Self {
        Self::new()
    }
}
