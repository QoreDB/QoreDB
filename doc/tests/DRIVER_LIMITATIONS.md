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
