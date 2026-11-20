// SPDX-License-Identifier: Apache-2.0

use std::path::PathBuf;
use std::process::ExitCode;

use clap::{Parser, Subcommand, ValueEnum};

use qore_core::{Namespace, SessionId};
use qore_service::ServiceContext;
use qore_service::agent_access::{self, AgentVault};
use qore_service::agent_tools::{self, AgentToolContext};
use qore_service::ci_runner;
use qore_service::contracts::storage as contract_storage;
use qore_service::interceptor::QuerySource;
use qore_service::paths::{QUERY_TIMEOUT_MS, config_dir};
use qore_service::replay::ReplaySetStore;

#[derive(Clone, Copy, Debug, PartialEq, Eq, ValueEnum)]
enum OutputFormat {
    Json,
    Junit,
}

#[derive(Parser)]
#[command(
    name = "qore",
    about = "QoreDB CLI — query connections and run contracts or replay checks from the terminal"
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
    /// List or run workspace Data Contracts
    Contract {
        #[command(subcommand)]
        action: ContractCommand,
    },
    /// List or run workspace Query Replay sets
    Replay {
        #[command(subcommand)]
        action: ReplayCommand,
    },
}

#[derive(Subcommand)]
enum ContractCommand {
    /// List Data Contracts in the active workspace
    List,
    /// Run one or all workspace Data Contracts against an exposed connection
    Run {
        connection_id: String,
        #[arg(long)]
        contract: Option<String>,
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
        /// Append each run to .qoredb/contracts/.history (off by default so CI
        /// checkouts stay clean)
        #[arg(long)]
        record_history: bool,
    },
}

#[derive(Subcommand)]
enum ReplayCommand {
    /// List Query Replay sets in the active workspace
    List,
    /// Replay a saved set against an exposed connection
    Run {
        connection_id: String,
        set_id: String,
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
        /// Also fail when an entry runs slower than its recording
        #[arg(long)]
        fail_on_slower: bool,
    },
}

async fn connect(
    ctx: &ServiceContext,
    vault: &AgentVault,
    connection_id: &str,
) -> Result<SessionId, String> {
    agent_access::open_session(vault, &ctx.session_manager, connection_id).await
}

async fn run(cli: Cli) -> Result<(String, bool), String> {
    let ctx = ServiceContext::new();
    let workspace = agent_access::detect_workspace(cli.workspace.as_deref());
    if cli.workspace.is_some() && workspace.is_none() {
        return Err("no .qoredb/workspace.json found at the given --workspace path".to_string());
    }
    let vault = AgentVault::open(config_dir(), workspace.as_deref());
    let ws_root = vault
        .workspace_path()
        .map(PathBuf::from)
        .unwrap_or_else(config_dir);

    match cli.command {
        Command::Connections => {
            let summary: Vec<_> = vault
                .exposed()?
                .iter()
                .map(agent_access::connection_summary)
                .collect();
            let json = serde_json::to_string_pretty(&summary).map_err(|e| e.to_string())?;
            Ok((json, true))
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
            let json = serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?;
            Ok((json, true))
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
            let json = serde_json::to_string_pretty(&list).map_err(|e| e.to_string())?;
            Ok((json, true))
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
            let json = serde_json::to_string_pretty(&schema_info).map_err(|e| e.to_string())?;
            Ok((json, true))
        }
        Command::Contract { action } => match action {
            ContractCommand::List => {
                let list = contract_storage::list_contracts(&ws_root).map_err(|e| e.to_string())?;
                let json = serde_json::to_string_pretty(&list).map_err(|e| e.to_string())?;
                Ok((json, true))
            }
            ContractCommand::Run {
                connection_id,
                contract,
                format,
                record_history,
            } => {
                let session = connect(&ctx, &vault, &connection_id).await?;
                let runs = ci_runner::run_workspace_contracts(
                    &ctx,
                    session,
                    &connection_id,
                    &ws_root,
                    contract.as_deref(),
                    record_history,
                )
                .await?;
                if runs.is_empty() {
                    eprintln!("warning: no Data Contracts found in {}", ws_root.display());
                }
                let passed = ci_runner::contract_runs_passed(&runs);
                let output = match format {
                    OutputFormat::Json => {
                        serde_json::to_string_pretty(&runs).map_err(|e| e.to_string())?
                    }
                    OutputFormat::Junit => ci_runner::contracts_to_junit_xml(&runs),
                };
                Ok((output, passed))
            }
        },
        Command::Replay { action } => match action {
            ReplayCommand::List => {
                let list = ReplaySetStore::new(&ws_root).list()?;
                let json = serde_json::to_string_pretty(&list).map_err(|e| e.to_string())?;
                Ok((json, true))
            }
            ReplayCommand::Run {
                connection_id,
                set_id,
                format,
                fail_on_slower,
            } => {
                let session = connect(&ctx, &vault, &connection_id).await?;
                let report =
                    ci_runner::run_workspace_replay(&ctx, session, &ws_root, &set_id).await?;
                let gate = ci_runner::CiGate { fail_on_slower };
                if report.summary.total == report.summary.skipped {
                    eprintln!("warning: every replay entry was skipped; nothing was compared");
                }
                let passed = ci_runner::replay_report_passed(&report, gate);
                let output = match format {
                    OutputFormat::Json => {
                        serde_json::to_string_pretty(&report).map_err(|e| e.to_string())?
                    }
                    OutputFormat::Junit => ci_runner::replay_to_junit_xml(&report, gate),
                };
                Ok((output, passed))
            }
        },
    }
}

#[tokio::main]
async fn main() -> ExitCode {
    let cli = Cli::parse();
    match run(cli).await {
        Ok((output, true)) => {
            println!("{output}");
            ExitCode::SUCCESS
        }
        Ok((output, false)) => {
            println!("{output}");
            ExitCode::from(2)
        }
        Err(err) => {
            eprintln!("error: {err}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_contract_and_replay_subcommands() {
        let cli = Cli::try_parse_from([
            "qore",
            "--workspace",
            "/tmp/proj",
            "contract",
            "run",
            "staging",
            "--contract",
            "users",
            "--format",
            "junit",
        ])
        .unwrap();
        assert_eq!(cli.workspace, Some(PathBuf::from("/tmp/proj")));
        assert!(matches!(
            cli.command,
            Command::Contract {
                action: ContractCommand::Run {
                    format: OutputFormat::Junit,
                    record_history: false,
                    ..
                }
            }
        ));

        let cli = Cli::try_parse_from(["qore", "replay", "list"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Replay {
                action: ReplayCommand::List
            }
        ));

        let cli = Cli::try_parse_from(["qore", "replay", "run", "prod", "smoke-set"]).unwrap();
        assert!(matches!(
            cli.command,
            Command::Replay {
                action: ReplayCommand::Run {
                    format: OutputFormat::Json,
                    fail_on_slower: false,
                    ..
                }
            }
        ));

        let cli = Cli::try_parse_from([
            "qore",
            "replay",
            "run",
            "prod",
            "smoke-set",
            "--fail-on-slower",
        ])
        .unwrap();
        assert!(matches!(
            cli.command,
            Command::Replay {
                action: ReplayCommand::Run {
                    fail_on_slower: true,
                    ..
                }
            }
        ));
    }
}
