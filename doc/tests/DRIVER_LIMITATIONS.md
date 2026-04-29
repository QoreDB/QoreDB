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
