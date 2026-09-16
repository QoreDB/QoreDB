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
- [Driver limitations](tests/DRIVER_LIMITATIONS.md): coverage boundaries,
  unsupported operations, and mock/live distinctions.

## Proposals and ongoing plans

Files under `todo/` can mix delivered work with unfinished items. Inspect their
status and source before treating any requirement as implemented. Move a fully
completed spec to `archive/` when delivery is established and update its links.

| Plan | Topic |
| --- | --- |
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
