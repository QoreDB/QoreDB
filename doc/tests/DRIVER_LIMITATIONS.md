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
