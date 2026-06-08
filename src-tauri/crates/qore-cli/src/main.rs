// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand};

use qore_core::{CollectionListOptions, Namespace, SessionId};
use qore_service::vault::backend::KeyringProvider;
use qore_service::vault::VaultStorage;
use qore_service::ServiceContext;

const PROJECT_ID: &str = "default";
const QUERY_TIMEOUT_MS: u64 = 30_000;

#[derive(Parser)]
#[command(
    name = "qore",
    about = "QoreDB CLI — query your saved connections from the terminal"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List saved connections
    Connections,
    /// Run a query on a saved connection
    Query { connection_id: String, sql: String },
    /// List tables/collections in a namespace
    Tables {
        connection_id: String,
        database: String,
        #[arg(long)]
        schema: Option<String>,
    },
    /// Describe a table/collection schema
    Describe {
        connection_id: String,
        database: String,
        table: String,
        #[arg(long)]
        schema: Option<String>,
    },
}

fn config_dir() -> PathBuf {
    if let Ok(dir) = std::env::var("QOREDB_CONFIG_DIR") {
        return PathBuf::from(dir);
    }
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("com.rapha.qoredb")
}

fn storage() -> VaultStorage {
    VaultStorage::new(PROJECT_ID, config_dir(), Box::new(KeyringProvider::new()))
}

async fn connect(ctx: &ServiceContext, connection_id: &str) -> Result<SessionId, String> {
    let storage = storage();
    let saved = storage
        .get_connection(connection_id)
        .map_err(|e| e.to_string())?;
    let creds = storage
        .get_credentials(connection_id)
        .map_err(|e| e.to_string())?;
    let config = saved
        .to_connection_config(&creds)
        .map_err(|e| e.to_string())?;
    qore_service::connection::connect(&ctx.session_manager, config)
        .await
        .map_err(|e| e.sanitized())
}

async fn run(command: Command) -> Result<String, String> {
    let ctx = ServiceContext::new();

    match command {
        Command::Connections => {
            let connections = storage()
                .list_connections_full()
                .map_err(|e| e.to_string())?;
            let summary: Vec<_> = connections
                .into_iter()
                .map(|c| {
                    serde_json::json!({
                        "id": c.id,
                        "name": c.name,
                        "driver": c.driver,
                        "host": c.host,
                        "database": c.database,
                        "environment": c.environment.as_str(),
                        "read_only": c.read_only,
                    })
                })
                .collect();
            serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())
        }
        Command::Query { connection_id, sql } => {
            let session = connect(&ctx, &connection_id).await?;
            let session_id = session.0.to_string();
            let pf = qore_service::query::preflight(
                &ctx.session_manager,
                &ctx.query_rate_limiter,
                &ctx.interceptor,
                &ctx.policy,
                session,
                &session_id,
                &sql,
                None,
                false,
            )
            .await?;
            let query_id = ctx.query_manager.register(session).await;
            let outcome = qore_service::query::execute(
                &ctx.query_manager,
                &ctx.query_cache,
                &ctx.interceptor,
                &ctx.policy,
                pf.driver,
                &pf.context,
                session,
                None,
                &sql,
                query_id,
                pf.is_mutation,
                pf.connection_key.as_deref(),
                pf.safety_warning.as_deref(),
                Some(QUERY_TIMEOUT_MS),
                false,
                None,
                None,
                |_, _| {},
            )
            .await;
            if let Some(err) = outcome.error {
                return Err(err);
            }
            serde_json::to_string_pretty(&outcome.result).map_err(|e| e.to_string())
        }
        Command::Tables {
            connection_id,
            database,
            schema,
        } => {
            let session = connect(&ctx, &connection_id).await?;
            let driver = ctx
                .session_manager
                .get_driver(session)
                .await
                .map_err(|e| e.sanitized_message())?;
            let namespace = Namespace { database, schema };
            let options = CollectionListOptions {
                search: None,
                page: None,
                page_size: None,
            };
            let list = driver
                .list_collections(session, &namespace, options)
                .await
                .map_err(|e| e.sanitized_message())?;
            serde_json::to_string_pretty(&list).map_err(|e| e.to_string())
        }
        Command::Describe {
            connection_id,
            database,
            table,
            schema,
        } => {
            let session = connect(&ctx, &connection_id).await?;
            let namespace = Namespace { database, schema };
            let schema_info = qore_service::query::describe_table(
                &ctx.session_manager,
                &ctx.virtual_relations,
                session,
                &namespace,
                &table,
                None,
            )
            .await
            .map_err(|e| e.sanitized())?;
            serde_json::to_string_pretty(&schema_info).map_err(|e| e.to_string())
        }
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli.command).await {
        Ok(output) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}
