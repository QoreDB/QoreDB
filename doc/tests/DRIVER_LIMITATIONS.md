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

- Query execution supports `find` with simple JSON payloads and limited
  `operation` handling (e.g., `create_collection`).
- Transactions are reported as unsupported for standalone servers (replica sets
  are required for transactions).
- Cancellation is best-effort: the client task is aborted, but server-side work
  may continue.
- Namespace listing filters `admin`, `config`, and `local` databases.

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
