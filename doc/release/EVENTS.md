# Analytics events

List of PostHog events emitted by the app.

## Core lifecycle

- `app_opened` (once per day)
- `onboarding_completed`
- `analytics_opt_in`

## Connections

- `connection_created` (properties: `source`, `driver`)
- `connection_tested_success` (properties: `source`, `driver`)
- `connection_tested_failed` (properties: `source`, `driver`)
- `connected_success` (properties: `source`, `driver`)
- `connected_failed` (properties: `source`, `driver`)

## Navigation/resources

- `resource_opened` (properties: `source`, `resource_type`, `driver`)
- `table_view_loaded` (properties: `driver`, `resource_type`)

## Querying

- `query_executed` (properties: `dialect`, `driver`, `row_count`)

## Export

- `export_used` (properties: `format`, `destination`)

## Logs

- `error_view_opened` (properties: `source`)

## Data grid

- `blob_viewer_opened` (properties: `tab`, `column_type`, `size_bucket`)
- `blob_downloaded` (properties: `mime`, `size_bucket`)
- `bulk_edit_opened` (properties: `driver`, `selected_count`)
- `bulk_edit_applied` (properties: `driver`, `affected_count`, `via_sandbox`)

## Schema management (DDL)

- `ddl_create_table_opened` (properties: `driver`)
- `ddl_create_table_applied` (properties: `driver`, `column_count`, `has_foreign_keys`, `has_indexes`)
- `ddl_alter_table_opened` (properties: `driver`)
- `ddl_alter_table_applied` (properties: `driver`, `op_count`)

## Data Contracts (v0.1.28, Pro)

- `contract_created` (properties: `rules_count`)
- `contract_run_started` (properties: `driver`, `rules_count`)
- `contract_run_completed` (properties: `driver`, `rules_count`, `violations_count`, `duration_ms`)

## Instant Data API (v0.1.28, Pro)

- `instant_api_started` (properties: `port`)
- `instant_api_endpoint_created` (properties: `params_count`, `shape`)
- `instant_api_request` (properties: `driver`, `status_code`, `duration_ms`) — sampled 1/100, **never** includes the endpoint name or request params

## Backup / Restore (v0.1.28)

- `backup_started` (properties: `driver`, `mode`)
- `backup_completed` (properties: `driver`, `mode`, `duration_ms`, `size_bytes_bucket`)
- `restore_started` (properties: `driver`)
- `restore_completed` (properties: `driver`, `duration_ms`)

## Keyboard shortcuts (v0.1.28)

- `shortcut_customized` (properties: `shortcut_id`, `category`)
- `shortcuts_reset` (properties: `scope`)

## Audit log (v0.1.28)

- `audit_exported` (properties: `format`, `entries_count`)
- `audit_filtered_by_fingerprint`
