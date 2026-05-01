# QoreDB v0.1.27 — Power-user DataGrid, DDL & Cloud Postgres

## ✨ Highlights

This release closes the last gaps between QoreDB and DBeaver/pgAdmin for power users, with a fully visual DDL editor, bulk row editing, a polished blob viewer, tab grouping by connection, and four new managed-database drivers.

## 🎯 New features

### Visual DDL Editor (CREATE & ALTER TABLE)

A complete graphical editor for table schemas with live SQL preview.

- **CREATE TABLE** — columns, types, primary keys, foreign keys, indexes, check constraints, comments, all with driver-specific defaults
- **ALTER TABLE** — diff-based: load the existing schema, edit it, see the exact `ALTER` statements before applying
- Live SQL preview as you type
- Driver coverage: PostgreSQL, MySQL, MariaDB, SQLite, DuckDB, SQL Server, CockroachDB
- Driver-specific warnings (e.g. SQLite < 3.35 limitations on `DROP COLUMN` / `ALTER COLUMN`)
- ENUM and ARRAY types for PostgreSQL, charset/collation hints for MySQL

### Bulk Edit with Preview

Multi-row column updates from the DataGrid with a transactional preview.

- Select multiple rows → edit a common column → preview the generated `UPDATE` statements before applying
- Compatible with the Sandbox mode (changes pushed to the migration store)
- Direct apply uses a single transaction (`apply_sandbox_changes` with `use_transaction = true`)
- Core threshold: ≤ 5 rows. Larger batches require Pro (with a clear upsell)

### Blob/Binary Viewer (finalized)

The blob viewer is now production-ready.

- Hex dump and base64 modes
- Image preview for PNG, JPEG, GIF, SVG, BMP, ICO
- New "SVG source" tab and "Copy as Data URI" action
- Open externally via `tauri-plugin-dialog`
- `Space` / `Enter` keyboard shortcut on a focused binary cell to open the viewer

### Tab Groups by Connection

When working across multiple connections, tabs can now be grouped under collapsible headers.

- Toggle in **Settings → General → Tabs**
- Each group shows the connection name + an environment dot (green/amber/red)
- Drag-and-drop reorder is intra-group only (no accidental connection swap)
- Tabs persist across connection switches; clicking a tab in another connection's group auto-switches to that connection

### Native support for 3 managed Postgres databases

All three reuse the existing PostgreSQL engine — paste your DSN and the right driver picks itself.

- **Supabase** — managed Postgres, smart-paste detection on `*.supabase.co` and `pooler.supabase.com`
- **Neon** — serverless Postgres with branching, smart-paste on `*.neon.tech`
- **TimescaleDB** — Postgres with the time-series extension

## 🛠 Under the hood

- New module `src/lib/ddl/` — pure TypeScript SQL generators per driver, splittable per dialect
- Shared `pg_compat` helpers (`list_namespaces_default`, `list_collections_default`) so PG-compatible drivers stay thin (~250 lines each)
- New PostHog events: `blob_viewer_opened`, `blob_downloaded`, `bulk_edit_opened`, `bulk_edit_applied`, `ddl_create_table_*`, `ddl_alter_table_*`
- 9 locales updated (en, fr, de, es, pt-BR, ru, ja, ko, zh-CN)

## ⚠️ Known limitations

- **SQLite < 3.35**: `DROP COLUMN` / `ALTER COLUMN` not auto-generated — explicit warning surfaced before apply.
- **MongoDB / Redis**: DDL editor not applicable. Mongo collection management is planned for a later release via a dedicated `CreateCollectionModal`.
- **Sandbox**: stays DML-only in this release. DDL has its own confirm-and-apply dialog.

## 📦 Driver matrix

| Driver | DDL Create | DDL Alter | Bulk Edit | Blob Viewer |
| ------ | :--------: | :-------: | :-------: | :---------: |
| PostgreSQL / Supabase / Neon / TimescaleDB / CockroachDB | ✅ | ✅ | ✅ | ✅ |
| MySQL / MariaDB | ✅ | ✅ | ✅ | ✅ |
| SQLite | ✅ | ⚠️ | ✅ | ✅ |
| DuckDB | ✅ | ✅ | ✅ | ✅ |
| SQL Server | ✅ | ✅ | ✅ | ✅ |
| MongoDB | ❌ | ❌ | ✅ | ✅ |
| Redis | ❌ | ❌ | ✅ (key types) | — |

---

**Full changelog**: `git log v0.1.26..v0.1.27`
