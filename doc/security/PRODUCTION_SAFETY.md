# Production Safety

QoreDB includes specific features to prevent accidents in production environments.

## Connection Settings

When editing a connection, you can set its **Environment**:
- **Development**: No special restrictions.
- **Staging**: Visual cues.
- **Production**: Strict safety rules enabled.

You can also toggle **Read-Only Mode** independently.

## Safety Rules

| Feature | Development | Production |
| ------- | ----------- | ---------- |
| **Visual Theme** | Neutral | **Red Warning Borders** |
| **Read-Only** | Optional | Optional (Recommended) |
| **Dangerous SQL** | Allowed | **Blocked / Confirmation Required** |
| **Mutations** | Allowed | Blocked if Read-Only |

## Dangerous Operations

The following SQL operations are considered dangerous and trigger warnings or blocks in Production:
- `DROP` (TABLE, DATABASE, etc.)
- `TRUNCATE`
- `ALTER`
- `DELETE` without a `WHERE` clause
- `UPDATE` without a `WHERE` clause

## Configuration

Safety policies can be overridden via `config.json` or Environment Variables:
- `QOREDB_PROD_BLOCK_DANGEROUS`: Force block dangerous queries.
- `QOREDB_PROD_REQUIRE_CONFIRMATION`: Require explicit user confirmation (default).
