// SPDX-License-Identifier: Apache-2.0

use std::process::ExitCode;

use clap::{Parser, Subcommand};

use std::path::PathBuf;

use qore_core::{Namespace, SessionId};
use qore_service::ServiceContext;
use qore_service::agent_access::{self, AgentVault};
use qore_service::agent_tools::{self, AgentToolContext};
use qore_service::interceptor::QuerySource;
use qore_service::paths::{QUERY_TIMEOUT_MS, config_dir};

#[derive(Parser)]
#[command(
    name = "qore",
    about = "QoreDB CLI — query your saved connections from the terminal"
)]
struct Cli {
    /// Use the .qoredb workspace at this directory (or its parent) instead of
    /// the one detected from the working directory or the default vault
    #[arg(long, global = true, value_name = "DIR")]
    workspace: Option<PathBuf>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// List the saved connections exposed to AI agents
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

async fn connect(
    ctx: &ServiceContext,
    vault: &AgentVault,
    connection_id: &str,
) -> Result<SessionId, String> {
    agent_access::open_session(vault, &ctx.session_manager, connection_id).await
}

async fn run(cli: Cli) -> Result<String, String> {
    let ctx = ServiceContext::new();
    let workspace = agent_access::detect_workspace(cli.workspace.as_deref());
    if cli.workspace.is_some() && workspace.is_none() {
        return Err("no .qoredb/workspace.json found at the given --workspace path".to_string());
    }
    let vault = AgentVault::open(config_dir(), workspace.as_deref());

    match cli.command {
        Command::Connections => {
            let summary: Vec<_> = vault
                .exposed()?
                .iter()
                .map(agent_access::connection_summary)
                .collect();
            serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())
        }
        Command::Query { connection_id, sql } => {
            let session = connect(&ctx, &vault, &connection_id).await?;
            let result = agent_tools::run_query(
                &AgentToolContext::from_service(&ctx),
                session,
                &sql,
                None,
                false,
                Some(QUERY_TIMEOUT_MS),
                QuerySource::Cli,
            )
            .await?;
            serde_json::to_string_pretty(&result).map_err(|e| e.to_string())
        }
        Command::Tables {
            connection_id,
            database,
            schema,
        } => {
            let session = connect(&ctx, &vault, &connection_id).await?;
            let namespace = Namespace { database, schema };
            let list = agent_tools::list_tables(
                &AgentToolContext::from_service(&ctx),
                session,
                &namespace,
                None,
            )
            .await?;
            serde_json::to_string_pretty(&list).map_err(|e| e.to_string())
        }
        Command::Describe {
            connection_id,
            database,
            table,
            schema,
        } => {
            let session = connect(&ctx, &vault, &connection_id).await?;
            let namespace = Namespace { database, schema };
            let schema_info = agent_tools::describe_table(
                &AgentToolContext::from_service(&ctx),
                session,
                &namespace,
                &table,
                None,
            )
            .await?;
            serde_json::to_string_pretty(&schema_info).map_err(|e| e.to_string())
        }
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli).await {
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
