# Documentation index

Start with the task below and read the relevant guide. Current implementation is
established by source code and tests; proposals and dated reports have different
roles. Contributor instructions are shared through [AGENTS.md](../AGENTS.md).

## Find a workflow

| Task | First document | Related context |
| --- | --- | --- |
| Set up a clean checkout | [Contributing](../CONTRIBUTING.md) | [Testing](development/TESTING.md) |
| Locate a subsystem or trace a request | [Architecture](development/ARCHITECTURE.md) | [Product vision](PROJECT.md), [feature inventory](FEATURES.csv) |
| Add or extend a driver | [Driver guide](development/DRIVERS.md) | [Driver limitations](tests/DRIVER_LIMITATIONS.md) |
| Add a command or change IPC | [Command guide](development/COMMANDS.md) | [Architecture](development/ARCHITECTURE.md) |
| Choose tests and Cargo features | [Testing matrix](development/TESTING.md) | [SSH](tests/TESTING_SSH.md), [MCP](tests/TESTING_MCP.md) |
| Change UI or translations | [Frontend instructions](../src/AGENTS.md) | [Design rules](rules/DESIGN.md) |
| Create/move code across modules | [Licensing](development/LICENSING.md) | [Apache-2.0](../LICENSE), [BUSL-1.1](../LICENSE-BSL) |
| Improve agent work or resume a task | [Agent workflow](development/AGENT_WORKFLOW.md) | [Readiness audit](audits/AGENT_READINESS.md) |
| Change policy, credentials, or database writes | [Production safety](security/PRODUCTION_SAFETY.md) | [Threat model](security/THREAT_MODEL.md), [security reporting](../SECURITY.md) |
| Prepare a release | [Release process](release/RELEASE.md) | [Homebrew](../packaging/homebrew/README.md), [WinGet](../packaging/winget/README.md) |
| Update this documentation | [Documentation instructions](AGENTS.md) | `pnpm docs:check` |

## Current references and entry points

- [Product vision](PROJECT.md) and [feature inventory](FEATURES.csv): product
  context, to cross-check against the implementation for precise behavior.
- [Design](rules/DESIGN.md) and [Qore AI identity](brand/qore-ai.md): UI/brand rules.
- [CLI](../src-tauri/crates/qore-cli/README.md),
  [MCP server](../src-tauri/crates/qore-mcp/README.md), and
  [server](../src-tauri/crates/qore-server/README.md): their public interfaces.
- [Rust instructions](../src-tauri/AGENTS.md): package boundaries and invariants.

## Evidence and audits

These documents state observations at a date or baseline. They are not a promise
that every check passes on the current checkout.

- [Audit index](audits/README.md): security, privacy, dependency/build size,
  product claims, plugin capability checks, and agent readiness.
- [Driver validation, 2026-09-05](tests/DRIVERS_VALIDATION_2026-09-05.md): dated run.
- [v0.1.40 frontend weight, 2026-09-28](tests/V0_1_40_PERFORMANCE_2026-09-28.md):
  initial measurement and first reduction; native performance remains unmeasured.
- [v0.1.40 bulk edit precision, 2026-10-01](tests/V0_1_40_BULK_EDIT_2026-10-01.md):
  numeric rounding regression, DTO/SQL checks and remaining live validation.
- [v0.1.40 Time Travel, 2026-10-03](tests/V0_1_40_TIME_TRAVEL_2026-10-03.md):
  connection isolation, rollback/diff regressions, Core/Pro checks and bundle budget overrun.
- [v0.1.40 Sandbox batches, 2026-10-03](tests/V0_1_40_SANDBOX_2026-10-03.md):
  commit/rollback fidelity, mutation guards, live SQLite tests and deferred Bulk Edit loading.
- [v0.1.40 Time Travel masking, 2026-10-03](tests/V0_1_40_TIME_TRAVEL_PRIVACY_2026-10-03.md):
  key/JSON redaction, current connection rules on historical reads and safe rollback limits.
- [v0.1.40 inline editing, 2026-10-04](tests/V0_1_40_INLINE_EDIT_2026-10-04.md):
  canonical row reconciliation, browser scroll/selection/focus checks and live SQLite readback.
- [v0.1.40 Time Travel captures, 2026-10-04](tests/V0_1_40_TIME_TRAVEL_CAPTURE_2026-10-04.md):
  database images per mutation, verified keys, bounded batch images and safe refusal of incomplete rollbacks.
- [v0.1.40 generated keys, 2026-10-04](tests/V0_1_40_GENERATED_KEYS_2026-10-04.md):
  SQLite/PostgreSQL INSERT identities, live permission checks and restored frontend budgets.
- [v0.1.40 capture transactions, 2026-10-04](tests/V0_1_40_CAPTURE_TRANSACTIONS_2026-10-04.md):
  isolated PostgreSQL reads, cancellation recovery, lost-transaction guards and commit verification.
- [v0.1.40 retained history, 2026-10-04](tests/V0_1_40_HISTORY_RETENTION_2026-10-04.md):
  reads beyond the cache, atomic retention, rollback limits and a 50 000-event fixture.
- [v0.1.40 workspace history, 2026-10-04](tests/V0_1_40_WORKSPACE_HISTORY_2026-10-04.md):
  session-bound history origin, scoped masking updates and workspace-bound deletion confirmations.
- [v0.1.40 retention policy, 2026-10-04](tests/V0_1_40_RETENTION_POLICY_2026-10-04.md):
  scheduled duration/count/size cleanup, durable settings errors and explicit settings saves.
- [v0.1.40 Pro workflows, 2026-10-06](tests/V0_1_40_PRO_WORKFLOWS_2026-10-06.md):
  single-pass notebook references, exact numeric literals and faithful Visual Diff results.
- [Driver limitations](tests/DRIVER_LIMITATIONS.md): coverage boundaries,
  unsupported operations, and mock/live distinctions.
- [v0.1.40 notebooks, 2026-10-08](tests/V0_1_40_NOTEBOOKS_2026-10-08.md):
  reference invalidation, execution lifecycle and file round trips.
- [v0.1.40 Data API, 2026-10-08](tests/V0_1_40_DATA_API_2026-10-08.md):
  workspace masking, expired sessions and concurrent connection opening.
- [v0.1.40 schema captures, 2026-10-08](tests/V0_1_40_SCHEMA_DIFF_2026-10-08.md):
  fail-closed enumeration, complete baselines and comparison-session cleanup.

- [v0.1.40 Replay Lab, 2026-10-08](tests/V0_1_40_REPLAY_2026-10-08.md):
  PostgreSQL cancellation, report retention, late responses, workspace targets
  and complementary live PostgreSQL/Time Travel checks.

- [v0.1.40 références Replay, 2026-10-09](tests/V0_1_40_REPLAY_2026-10-09.md):
  acceptation atomique des attentes, captures historiques et rétention.
- [v0.1.40 workspaces Replay, 2026-10-09](tests/V0_1_40_REPLAY_WORKSPACES_2026-10-09.md):
  enregistrements cloisonnés, commandes périmées refusées et erreurs de nettoyage.
- [v0.1.40 exports, 2026-10-09](tests/V0_1_40_EXPORTS_2026-10-09.md):
  exact XLSX/Parquet values and explicit conversion failures.
- [v0.1.40 profils Time Travel, 2026-10-09](tests/V0_1_40_PROFILE_HISTORY_2026-10-09.md):
  réglages illisibles, récupération, captures tardives et compatibilité du journal v0.1.39.
- [v0.1.40 bibliothèque et workspaces, 2026-10-09](tests/V0_1_40_QUERY_LIBRARY_2026-10-09.md):
  sauvegardes liées au projet, reprise après échec et compatibilité du format v0.1.39.
- [v0.1.40 intégrité de la bibliothèque, 2026-10-09](tests/V0_1_40_LIBRARY_INTEGRITY_2026-10-09.md):
  capacité sans suppression silencieuse, fichiers complets, actions périmées et nombres MCP exacts.

## Proposals and ongoing plans

Files under `todo/` can mix delivered work with unfinished items. Inspect their
status and source before treating any requirement as implemented. Move a fully
completed spec to `archive/` when delivery is established and update its links.

| Plan | Topic |
| --- | --- |
| [v0.1.40](todo/V0_1_40.md) | Weight/performance baseline, data editing and stabilization of existing Pro features |
| [Databases](todo/DATABASES.md) | Driver roadmap and investigation notes |
| [v2](todo/v2.md) | Product roadmap |
| [v3](todo/v3.md) | Subsequent product roadmap |
| [AI rework](todo/AI_REWORK.md) | AI experience changes |
| [Local Qore AI](todo/QORE_AI_LOCAL.md) | Local runtime work |
| [Query Replay Lab](todo/QUERY_REPLAY_LAB.md) | Replay specification and remaining work |
| [Pagination and rendering](todo/PAGINATION_DATA_RENDERING.md) | Data browsing performance |
| [Enterprise readiness](todo/ENTERPRISE_READINESS.md) | Enterprise requirements |

## Historical and local material

[archive/](archive/) contains shipped specifications and past release plans.
Use them for rationale and history, not as current operating instructions.

`doc/private/` is ignored local material and absent from a normal clone. Public
workflows do not depend on it. Local agent handoffs can live under ignored
`dev/agent-notes/`; durable project knowledge belongs in a reviewed guide.
