<!-- SPDX-License-Identifier: Apache-2.0 -->

# qore-mcp

Read-only [MCP](https://modelcontextprotocol.io) server for QoreDB. Lets an AI
agent discover and query the database connections you already saved in the
QoreDB desktop app, reusing the same engine (`qore-service`) and the same safety
gates — mutations are blocked.

## How it works

- Transport: **stdio** (the client spawns the binary and speaks JSON-RPC).
- Connections + credentials are read from the **same OS keyring and config dir as
  the desktop app** (`VaultStorage`, project `default`). No extra credential
  setup: if the desktop can connect, so can the server.
- Every session is opened with `read_only = true`; the existing preflight/execute
  gates reject any mutation.

## Tools

| Tool | Description |
| ---- | ----------- |
| `list_connections` | List saved connections (id, name, driver, host, db, environment). |
| `list_namespaces` | List databases/schemas for a connection. |
| `list_tables` | List tables/collections in a namespace. |
| `describe_table` | Columns and keys of a table/collection. |
| `run_query` | Run a read-only query and return the rows. |

## Build & register

```bash
cargo build -p qore-mcp --release        # -> target/release/qore-mcp
claude mcp add qoredb -- /absolute/path/to/target/release/qore-mcp
```

Set `QOREDB_CONFIG_DIR` to point at a non-default config directory (defaults to
the desktop app's config dir).
