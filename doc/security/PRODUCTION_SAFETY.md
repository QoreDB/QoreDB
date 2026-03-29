# Production Safety

QoreDB includes specific features to prevent accidents in production environments.

## Connection Settings

When editing a connection, you can set its **Environment**:

- **Development**: No special restrictions.
- **Staging**: Visual cues.
- **Production**: Strict safety rules enabled.

You can also toggle **Read-Only Mode** independently.

## Safety Rules

| Feature           | Development | Production                          |
| ----------------- | ----------- | ----------------------------------- |
| **Visual Theme**  | Neutral     | **Red Warning Borders**             |
| **Read-Only**     | Optional    | Optional (Recommended)              |
| **Dangerous SQL** | Allowed     | **Blocked / Confirmation Required** |
| **Mutations**     | Allowed     | Blocked if Read-Only                |

## Dangerous Operations

The following SQL operations are considered dangerous and trigger warnings or blocks in Production:

- `DROP` (TABLE, DATABASE, etc.)
- `TRUNCATE`
- `ALTER`
- `DELETE` without a `WHERE` clause
- `UPDATE` without a `WHERE` clause

## Query Governance

Resource limits prevent runaway queries and protect shared database servers:

| Limit | Environment Variable | Default | Description |
|-------|---------------------|---------|-------------|
| Max query duration | `QOREDB_MAX_QUERY_DURATION_MS` | None | Auto-cancel queries after N ms |
| Max result rows | `QOREDB_MAX_RESULT_ROWS` | None | Truncate results beyond N rows |
| Max concurrent queries | `QOREDB_MAX_CONCURRENT_QUERIES` | None | Block new queries when limit reached |

These limits are configurable via Settings > Security > Interceptor, or via environment variables for managed deployments.

When results are truncated, the UI displays a warning with the original row count.

## Configuration

Safety policies can be overridden via `config.json` or Environment Variables:

- `QOREDB_PROD_BLOCK_DANGEROUS`: Force block dangerous queries.
- `QOREDB_PROD_REQUIRE_CONFIRMATION`: Require explicit user confirmation (default).
- `QOREDB_MAX_QUERY_DURATION_MS`: Maximum query execution time (milliseconds).
- `QOREDB_MAX_RESULT_ROWS`: Maximum number of rows returned per query.
- `QOREDB_MAX_CONCURRENT_QUERIES`: Maximum number of concurrent queries.
