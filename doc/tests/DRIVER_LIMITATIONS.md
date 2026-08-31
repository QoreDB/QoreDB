# QoreDB — Driver limitations

This document describes known driver-specific limits and the fallback behavior
expected by the UI and command layer.

## Common behavior

- Capabilities are reported via `DriverCapabilities`; the UI should gate
  features using these flags.
- Unsupported operations return `EngineError::NotSupported` with a clear
  message.
- Cancellation support is reported as `CancelSupport::None`, `BestEffort`, or
  `Driver`.

### Filter operators

`FilterOperator` supports the usual relational and null-check operators plus
two cross-engine operators for text search:

| Operator | Meaning | Pattern carried in `ColumnFilter.value` |
| --- | --- | --- |
| `regex` | Regular-expression match, optional flags via `options.regex_flags` | the regex pattern |
| `text` | Engine-native full-text search, optional language via `options.text_language` | the query text |

Mapping per driver:

| Driver | `regex` | `text` |
| --- | --- | --- |
| PostgreSQL / CockroachDB | `col ~ ?` (or `~*` when flags contain `i`) | `to_tsvector(<lang>, col::text) @@ plainto_tsquery(<lang>, ?)` |
| MySQL / MariaDB | `col REGEXP ?` (flags collapsed into `(?i)` prefix for case-insensitive) | `MATCH(col) AGAINST(? IN NATURAL LANGUAGE MODE)` — requires a `FULLTEXT` index |
| SQLite | `col REGEXP ?` — requires the REGEXP user-defined function to be loaded; fails at execution otherwise | substring fallback `col LIKE '%?%'` (FTS5 lives in dedicated virtual tables) |
| DuckDB | `regexp_matches(col::VARCHAR, ?[, flags])` | case-insensitive substring fallback `col::VARCHAR ILIKE '%?%'` |
| SQL Server | `PATINDEX('%?%', CAST(col AS NVARCHAR(MAX))) > 0` (flags are ignored; no native POSIX regex without CLR) | `CONTAINS(col, '"?"')` — requires a full-text catalog + index |
| MongoDB | `{ $regex: ?, $options: flags }` (flags filtered to `imxs`) | top-level `{ $text: { $search: ?, $language: lang? } }` — requires a `text` index; the column name is ignored |

The `TableIndex` struct carries an optional `index_type` field (e.g.
`btree`, `hash`, `gin`, `fulltext`, `text`, `2dsphere`) so the UI can warn
the user when picking `text` without a matching index.

## PostgreSQL

- Cancellation uses `pg_cancel_backend(pid)` on a separate pool connection.
  This requires the same role or the `pg_signal_backend` privilege.
- Namespace listing is scoped to the current database; cross-database browsing
  is not supported.

## MySQL

- Cancellation uses `KILL QUERY <id>` and requires permissions to kill the
  target query (usually the same user or the `PROCESS` privilege).
- Transactions assume a transactional engine (e.g., InnoDB). Non-transactional
  tables will not behave as expected.
- Namespace listing filters system schemas (`information_schema`, `mysql`,
  `performance_schema`, `sys`).

## PlanetScale

Wire-compatible with MySQL, served by the same driver under the `planetscale`
id. Everything in the MySQL section applies unchanged; what follows is what
differs, and what QoreDB does not model.

- TLS is forced on connect: PlanetScale rejects plaintext, so leaving the
  connection form on "no TLS" would fail with a protocol error rather than a
  readable one.
- No URL scheme of its own — PlanetScale hands out `mysql://` strings. The DSN
  detector recognises `*.psdb.cloud` hosts and picks the driver from there.
- Foreign keys are not enforced unless the branch has foreign key constraints
  enabled. The schema explorer still lists whatever `information_schema`
  reports.
- **Stored routines, triggers and the event scheduler are unsupported by
  Vitess**, so the driver reports them as absent and the UI hides them.
  Announcing MySQL's set would offer schema objects the engine refuses.
- **Deploy requests are not modelled.** Applying a migration writes directly to
  the connected branch instead of opening a deploy request. Point the
  connection at a development branch, not at production.
- `mysqldump`-based backup is untested against PlanetScale, which restricts
  some privileges (`PROCESS` in particular) that `mysqldump` expects.
- Sequences are unsupported, exactly as on MySQL.
- TLS is forced even when the connection carries an explicit disabling
  `ssl_mode`: the mode outranks the flag downstream, so leaving it in place
  would quietly defeat the forcing.
- Never verified against a live PlanetScale cluster. `planetscale_e2e` runs the
  driver against the local MySQL service — TLS negotiation, schema
  introspection and paging all go through the real path — but PlanetScale's own
  gateway, branches and deploy requests are out of reach without an account.

## TiDB, StarRocks, Apache Doris and SingleStore

These identities use `MySqlDriver` and the MySQL wire protocol. They are not
server fingerprints: a self-hosted URL has no reliable flavor marker, so the
user must select the matching identity. DSN detection only selects TiDB Cloud
(`*.tidbcloud.com`) and SingleStore Helios (`*.svc.singlestore.com`).

- TiDB uses port 4000. Stored procedures/functions, triggers, events and the
  MySQL maintenance actions are hidden because TiDB does not implement them.
  The visual DDL editor remains available for its MySQL-compatible subset.
- StarRocks and Apache Doris use port 9030. QoreDB exposes queries, catalog
  browsing and manual migrations, but disables structured grid mutations,
  transactions, maintenance and visual DDL. Their OLAP key, distribution and
  index syntax cannot be generated safely by the MySQL table builder.
- SingleStore uses port 3306. Its session `time_zone` variable is read-only, so
  the shared pool initialization deliberately skips the UTC assignment for
  this flavor. The visual DDL editor hides foreign keys, CHECK constraints and
  uniqueness; stored-object editing is also hidden because SingleStore PSQL is
  not MySQL's routine grammar.
- `EXPLAIN` uses the common text form for these four identities rather than
  assuming MySQL's JSON format.
- `mysql_wire_compatible_a1_e2e` exercises all four identities against the
  local MySQL service. It validates the shared connection/query path, not each
  vendor's SQL extensions. `mysqldump` backup and live vendor endpoints remain
  unverified.

## YugabyteDB

- Uses `PostgresDriver`, port 5433 and database `yugabyte` by default. The full
  PostgreSQL catalog path, SQL dialect, migrations and schema tooling are
  reused.
- Hosted endpoints under `*.yugabyte.cloud` are detected from a pasted DSN.
  Self-hosted PostgreSQL URLs require explicit driver selection.
- `yugabytedb_wire_compatible_a1_e2e` runs against the local PostgreSQL
  stand-in. Distributed transactions, topology and Yugabyte-specific catalog
  extensions have not been verified against a live cluster.

## Azure SQL and Azure Synapse

- Both identities use `SqlServerDriver`, port 1433 and force TLS even when the
  connection form leaves it disabled. Forcing TLS also pins the SSL mode to
  `verify-full` when the user set none: unlike a LAN SQL Server, an Azure
  endpoint is reached over the public internet and presents a publicly-signed
  certificate, so accepting an unverified one would leave the connection open to
  interception. An SSL mode chosen explicitly by the user is left untouched.
  Azure SQL reuses the complete SQL Server capability set.
- Azure SQL hosts under `*.database.windows.net` and Synapse hosts under
  `*.sql.azuresynapse.net` are detected from DSNs. Synapse detection runs first
  so its more specific suffix cannot be mistaken for Azure SQL.
- The current authentication surface is SQL Server authentication; Microsoft
  Entra access tokens are not implemented.
- Synapse covers both dedicated and serverless SQL endpoints. Their DDL, DML
  and transaction surfaces differ, so structured mutations, transactions,
  maintenance, triggers and visual DDL are disabled conservatively. Manual
  queries remain available.
- `azure_sql_wire_compatible_a1_e2e` validates both identities and forced TLS
  against the local SQL Server service. Azure firewall, Entra authentication
  and Synapse-specific semantics require live cloud testing.

## Cassandra and ScyllaDB

Both identities run on the CQL v4 client in `qore-drivers/src/drivers/cql/`,
written against the protocol rather than built on the `scylla` crate. ScyllaDB
is a flavor of `CassandraDriver`: the wire protocol is identical, only the id
and the display name differ, so a self-hosted URL carries no marker and the
user picks the identity.

- One connection, one statement at a time, on protocol stream id 0. There is no
  request multiplexing, no token-aware routing and no topology discovery; a
  desktop client exercises none of them, and the node it is pointed at serves
  every request as coordinator.
- No transactions. Lightweight transactions are a compare-and-set on a single
  partition, not a multi-statement transaction, and nothing here opens one.
- No cancel. The protocol has no cancel frame, so `cancel_support` is `None`
  and a statement runs to completion or hits the 60-second I/O timeout.
- No routines, triggers, events or sequences: CQL has none.
- No visual DDL. The type palette is empty on purpose, which is what hides
  "create table" and schema export. Table structure stays viewable, and CQL DDL
  runs from the editor.
- Pagination uses the native paging state — the first driver here to declare
  `keyset` without needing a unique key of ours, since the cursor is the
  server's. It walks forward only: a jump to an arbitrary page number is
  refused rather than served by silently re-reading every page before it.
- No row count. `SELECT COUNT(*)` on a table is a ring-wide scan, so the table
  browser is served `total_rows: None` and relies on `has_more`.
- No sort and no cross-column search from the grid. CQL orders only by a
  clustering column inside a single partition; both come back as an explicit
  error pointing at the editor.
- Grid filters are bound, never interpolated — there is no CQL literal escaping
  in this driver. A predicate the ring cannot serve from the primary key is
  rewritten from Cassandra's `ALLOW FILTERING` message into one that says the
  query has no partition to start from.
- Row editing requires the full primary key on insert, update and delete, and
  refuses to change a key column in place. Binding a `tuple`, a UDT or a
  `duration` returns `NotSupported` rather than guessing an encoding.
- `ALLOW FILTERING`, `TRUNCATE`, `DROP KEYSPACE`, `DROP TABLE` and any `SELECT`
  with neither `WHERE` nor `LIMIT` are refused on a production connection.
- TLS has no permissive mode. A cluster with an internal CA points
  `ssl_ca_cert` at its PEM bundle; there is no "trust anything" fallback.
- Authentication covers `PasswordAuthenticator` (SASL PLAIN). A multi-step SASL
  exchange, and therefore Kerberos or LDAP through GSSAPI, is not implemented.
- SSH tunnelling works: unlike the HTTPS warehouses, CQL is a plain socket on
  9042.
- `docker-compose.yml` ships `cassandra:5` on 9042 without authentication and
  `scylladb/scylla` on 9043 with `PasswordAuthenticator`, so the bare STARTUP
  path and the SASL PLAIN exchange are both covered locally.

The type codec is the one place where a mistake is silent: a misread value
renders in the grid without raising anything. Its unit tests are built from the
wire encoding rather than from a round-trip through our own writer, and cover
NULL against an empty blob, signed widths, `decimal` without `f64`, `varint`
beyond `i64`, the 2^31 bias on `date`, the zigzag vints of `duration`,
truncated UDTs and non-string map keys. They have not yet been run against a
live cluster.

## MongoDB

- Query execution supports `find` with simple JSON payloads and a dedicated
  `aggregate` path that validates the pipeline before execution.
- Aggregation pipelines are parsed into a typed AST
  (`qore-drivers::mongo_pipeline`): unknown stages, operators missing the `$`
  prefix, `$out`/`$merge` that are not terminal, and dangerous operators
  (`$function`, `$accumulator`, `$where`) are rejected fail-closed.
- The pipeline depth is capped at 50 stages and the safety classifier scans
  recursively up to 64 levels deep for forbidden operators.
- Other `operation` values are handled explicitly (`create_collection`,
  `insert_one`/`insert_many`, `update_one`/`update_many`,
  `delete_one`/`delete_many`, `drop_collection`, `drop_database`).
- Transactions are reported as unsupported for standalone servers (replica sets
  are required for transactions).
- Cancellation is best-effort: the client task is aborted, but server-side work
  may continue.
- Namespace listing filters `admin`, `config`, and `local` databases.

### Aggregation examples

Count by group:

```json
{ "operation": "aggregate", "database": "app", "collection": "orders",
  "pipeline": [
    { "$match": { "status": "paid" } },
    { "$group": { "_id": "$country", "count": { "$sum": 1 } } }
  ] }
```

Top N most recent:

```json
{ "operation": "aggregate", "database": "app", "collection": "events",
  "pipeline": [
    { "$match": { "level": "error" } },
    { "$sort": { "createdAt": -1 } },
    { "$limit": 10 }
  ] }
```

Join via `$lookup`:

```json
{ "operation": "aggregate", "database": "app", "collection": "orders",
  "pipeline": [
    { "$lookup": { "from": "users", "localField": "userId",
                   "foreignField": "_id", "as": "user" } },
    { "$unwind": { "path": "$user", "preserveNullAndEmptyArrays": true } }
  ] }
```

Writes via `$out` or `$merge` are allowed only when they are the last stage;
they are routed through the mutation confirmation path like any other write.

### Bulk writes and atomic find-and-modify

- `bulkWrite` executes a list of heterogeneous write operations in a single
  server round-trip. Supported operation kinds: `insertOne`, `updateOne`,
  `updateMany`, `replaceOne`, `deleteOne`, `deleteMany`. All operations share
  the top-level `database`/`collection`; the namespace is stamped on each
  model internally.
- Upstream API : the driver delegates to `Client::bulk_write` (MongoDB 3.x)
  which is cross-collection; the handler pins every model to the payload's
  namespace for safety.
- `findOneAndUpdate` / `findOneAndReplace` / `findOneAndDelete` run
  atomically. The response contains zero or one document, matching
  `options.returnDocument` (`"before"` = default, `"after"` = updated value)
  for update/replace. `findOneAndDelete` always returns the document that
  was removed.
- All of these operations are classified as mutations and gated by the
  production-safety confirmation path.

```json
{ "operation": "bulkWrite", "database": "app", "collection": "orders",
  "operations": [
    { "insertOne": { "document": { "ref": "R1" } } },
    { "updateOne":  { "filter": { "ref": "R1" }, "update": { "$set": { "paid": true } } } },
    { "deleteMany": { "filter": { "cancelled": true } } }
  ] }
```

```json
{ "operation": "findOneAndUpdate", "database": "app", "collection": "orders",
  "filter": { "_id": 42 },
  "update": { "$inc": { "retries": 1 } },
  "options": { "returnDocument": "after" } }
```

### Index management

- `list_indexes` is exposed as a read operation (both via the JSON payload
  `{"operation":"listIndexes"}` and the shell-like `.getIndexes()`/`.indexes()`
  helpers).
- `createIndex` / `dropIndex` are classified as mutations and routed through
  the production-safety confirmation path when the environment is not
  `development`.
- Supported index options on create: `name`, `unique`, `sparse`,
  `expireAfterSeconds` (TTL), `partialFilterExpression` (JSON object).
- TTL indexes must cover a single ascending or descending key; the UI rejects
  mixed-direction or multi-field TTL declarations before reaching the driver.
- The `_id_` default index cannot be dropped; wildcard drops (`*`) are also
  rejected driver-side to prevent accidental mass removal.
- Direction values accepted per key: `1`, `-1`, `"text"`, `"2dsphere"`.

```json
{ "operation": "createIndex", "database": "app", "collection": "orders",
  "keys": { "userId": 1, "createdAt": -1 },
  "options": { "unique": true, "name": "user_recent_orders" } }
```

```json
{ "operation": "dropIndex", "database": "app", "collection": "orders",
  "name": "user_recent_orders" }
```

## Amazon DocumentDB

Implements the MongoDB wire protocol, served by the same driver under the
`documentdb` id. Everything in the MongoDB section applies; what follows is
what differs.

- TLS is forced on connect. Clusters are signed by an Amazon CA that most
  machines do not trust, so point `ssl_ca_cert` at the
  `global-bundle.pem` Amazon publishes — the driver hands that bundle to the
  TLS layer instead of the system trust store, and refuses to connect if the
  path does not exist rather than silently falling back.
- Clusters live in a private VPC: connections usually go through the SSH tunnel
  the connection form already offers. Through a tunnel the host is the local
  end while the certificate names the cluster, so hostname verification is
  relaxed **only then** — the CA is still verified either way.
- Retryable writes are turned off: DocumentDB does not implement them, and
  leaving them on makes every write fail. This one is not a preference — an
  explicit `retryWrites=true` is overridden rather than honoured into a broken
  connection. Hostname relaxation, by contrast, is a preference and an explicit
  value is kept.
- No URL scheme of its own — DocumentDB hands out `mongodb://` strings. The DSN
  detector recognises `*.docdb.amazonaws.com` hosts.
- **DocumentDB is not a complete MongoDB.** Several aggregation operators,
  `$lookup` beyond its simple form, and some index types are unsupported by the
  server. The driver delegates as-is, so the server's own error is what the user
  sees; QoreDB does not rewrite those messages.
- Never verified against a live DocumentDB cluster. The `mongodb-tls` service
  in `docker-compose.yml` is the local stand-in: a MongoDB requiring TLS behind
  a CA that signs a distinct server certificate, the same shape as Amazon's.
  `documentdb_e2e` connects through it, and
  `documentdb_refuses_an_unverifiable_certificate` checks that dropping the CA
  bundle makes the connection fail rather than succeed unverified. What remains
  untested is DocumentDB's own server behaviour.

## Redis

- Key browsing uses `SCAN` which is not atomic; keys may be missed or
  duplicated if the keyspace changes during iteration.
- Namespaces map to Redis databases (db0–db15). Only non-empty databases are
  listed; database 0 is always shown.
- No traditional schema — `describe_table` returns type-specific column
  definitions based on the key's Redis data type (string, hash, list, set,
  sorted set, stream).
- Mutations (SET, DEL, etc.) are only available through `execute()` with raw
  Redis commands; the mutation UI is not supported in V1.
- The maximum number of databases depends on the server's `databases`
  configuration (default 16).
- Cancellation is best-effort: the client task is aborted, but server-side
  work may continue.
- Connection supports both `redis://` and `rediss://` (TLS) URL schemes.
- Valkey is served by the same driver under the `valkey` id and adds the
  `valkey://` / `valkeys://` schemes. Every limitation above applies
  unchanged; the fork is wire-compatible and is not probed for its flavor.
- Dragonfly is served by the same driver under the `dragonfly` id. It has no
  URL scheme of its own — it announces itself over `redis://` — and is not
  probed for its flavor either. Every limitation above applies unchanged, and
  `dragonfly_e2e` in `tests/integration_databases.rs` runs the shared code
  against a real Dragonfly from `docker-compose.yml`.
- KeyDB and Garnet are served by the same driver under the `keydb` and `garnet`
  ids. Both use Redis URL schemes and require explicit selection because their
  DSNs carry no stable flavor marker. `redis_wire_compatible_a1_e2e` exercises
  both identities against the local Redis stand-in; vendor-specific commands
  are passed through but not modeled.
- Authentication is optional — many development setups run without a password.

### Lua scripting

- `EVAL`, `EVALSHA` and `FCALL` are classified as mutations (they can write),
  so they go through the production-safety confirmation path like any other
  write in non-development environments.
- `SCRIPT LOAD` is available to pre-register a script and obtain its SHA1;
  `SCRIPT FLUSH` and `SCRIPT KILL` are classified as `Dangerous` and always
  require explicit acknowledgement.
- The Lua script editor wraps the script in a single textual `EVAL`/`EVALSHA`
  command sent through `execute_query`; no dedicated Rust helper is used.
- A best-effort regex check (`detectDangerousLuaCalls`) warns the user when
  the script body contains `redis.call('FLUSHALL' | 'FLUSHDB' | 'SHUTDOWN' |
  'CONFIG' | 'SCRIPT', 'FLUSH' | 'DEBUG', 'SLEEP')`. The warning is advisory
  — the backend classifier remains the source of truth.
- `KEYS` and `ARGV` are passed as separate whitespace-quoted arguments; the
  number of keys is computed automatically from the `KEYS` list length.

```
EVAL "redis.call('SET', KEYS[1], ARGV[1]); return 'OK'" 1 user:42 hello
SCRIPT LOAD "return redis.call('GET', KEYS[1])"
EVALSHA 6b1bf486c81ceb7151e06fcc02e36ce45e4c1ed1 1 user:42
```
