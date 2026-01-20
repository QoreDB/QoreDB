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
