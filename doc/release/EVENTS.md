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
