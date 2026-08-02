// SPDX-License-Identifier: Apache-2.0

use std::sync::Arc;

use qore_core::DriverRegistry;
#[cfg(feature = "driver-clickhouse")]
use qore_drivers::drivers::clickhouse::ClickHouseDriver;
#[cfg(feature = "driver-cockroachdb")]
use qore_drivers::drivers::cockroachdb::CockroachDbDriver;
#[cfg(feature = "driver-duckdb")]
use qore_drivers::drivers::duckdb::DuckDbDriver;
#[cfg(feature = "driver-elasticsearch")]
use qore_drivers::drivers::elasticsearch::ElasticsearchDriver;
#[cfg(feature = "driver-mariadb")]
use qore_drivers::drivers::mariadb::MariaDbDriver;
#[cfg(feature = "driver-mongodb")]
use qore_drivers::drivers::mongodb::MongoDriver;
#[cfg(feature = "driver-motherduck")]
use qore_drivers::drivers::motherduck::MotherDuckDriver;
#[cfg(feature = "driver-mysql")]
use qore_drivers::drivers::mysql::MySqlDriver;
#[cfg(feature = "driver-neon")]
use qore_drivers::drivers::neon::NeonDriver;
#[cfg(feature = "driver-opensearch")]
use qore_drivers::drivers::opensearch::OpenSearchDriver;
#[cfg(feature = "driver-postgres")]
use qore_drivers::drivers::postgres::PostgresDriver;
#[cfg(any(feature = "driver-redis", feature = "driver-valkey"))]
use qore_drivers::drivers::redis::RedisDriver;
#[cfg(feature = "driver-sqlite")]
use qore_drivers::drivers::sqlite::SqliteDriver;
#[cfg(feature = "driver-sqlserver")]
use qore_drivers::drivers::sqlserver::SqlServerDriver;
#[cfg(feature = "driver-supabase")]
use qore_drivers::drivers::supabase::SupabaseDriver;
#[cfg(feature = "driver-timescaledb")]
use qore_drivers::drivers::timescaledb::TimescaleDbDriver;
use qore_drivers::query_manager::QueryManager;
use qore_drivers::session_manager::SessionManager;

use crate::cache::QueryCache;
use crate::interceptor::InterceptorPipeline;
use crate::license::LicenseManager;
use crate::policy::SafetyPolicy;
use crate::ratelimit::QueryRateLimiter;
use crate::vault::VaultLock;
use crate::vault::backend::default_provider;
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
        #[allow(unused_mut)]
        let mut registry = DriverRegistry::new();
        #[cfg(feature = "driver-postgres")]
        registry.register(Arc::new(PostgresDriver::new()));
        #[cfg(feature = "driver-mysql")]
        registry.register(Arc::new(MySqlDriver::new()));
        #[cfg(feature = "driver-mongodb")]
        registry.register(Arc::new(MongoDriver::new()));
        #[cfg(feature = "driver-redis")]
        registry.register(Arc::new(RedisDriver::new()));
        #[cfg(feature = "driver-valkey")]
        registry.register(Arc::new(RedisDriver::valkey()));
        #[cfg(feature = "driver-sqlite")]
        registry.register(Arc::new(SqliteDriver::new()));
        #[cfg(feature = "driver-duckdb")]
        registry.register(Arc::new(DuckDbDriver::new()));
        #[cfg(feature = "driver-motherduck")]
        registry.register(Arc::new(MotherDuckDriver::new()));
        #[cfg(feature = "driver-cockroachdb")]
        registry.register(Arc::new(CockroachDbDriver::new()));
        #[cfg(feature = "driver-sqlserver")]
        registry.register(Arc::new(SqlServerDriver::new()));
        #[cfg(feature = "driver-mariadb")]
        registry.register(Arc::new(MariaDbDriver::new()));
        #[cfg(feature = "driver-supabase")]
        registry.register(Arc::new(SupabaseDriver::new()));
        #[cfg(feature = "driver-neon")]
        registry.register(Arc::new(NeonDriver::new()));
        #[cfg(feature = "driver-timescaledb")]
        registry.register(Arc::new(TimescaleDbDriver::new()));
        #[cfg(feature = "driver-clickhouse")]
        registry.register(Arc::new(ClickHouseDriver::new()));
        #[cfg(feature = "driver-elasticsearch")]
        registry.register(Arc::new(ElasticsearchDriver::new()));
        #[cfg(feature = "driver-opensearch")]
        registry.register(Arc::new(OpenSearchDriver::new()));

        let registry = Arc::new(registry);
        let session_manager = Arc::new(SessionManager::new(Arc::clone(&registry)));
        let mut vault_lock = VaultLock::new(default_provider());
        let policy = SafetyPolicy::load();
        let query_manager = Arc::new(QueryManager::new());

        let data_dir = crate::paths::app_data_dir();
        let interceptor = Arc::new(InterceptorPipeline::new(data_dir.join("interceptor")));
        let _ = interceptor.load_config();
        let virtual_relations = Arc::new(VirtualRelationStore::new(
            data_dir.join("virtual_relations"),
        ));

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
