# Changelog

All notable changes to QoreDB will be documented in this file.

## [0.1.23] - 2026-04-02

### Bug Fixes

- Mark completed tasks for Database Notebooks feature
- Mark features as completed in Database Notebooks and related sections

### Features

- Add tauri-plugin-process and integrate restart functionality
- Enhance connection form to support proxy settings and improve share provider config handling

## [0.1.22] - 2026-03-30

### Bug Fixes

- Update Rust SBOM generation command to use overridden filename
- Update SBOM job to depend on build and publish release as non-draft
- Add pull request permissions to release job
- Allow continuation on error for CHANGELOG.md PR creation

### Miscellaneous

- Update release workflow to create PR for CHANGELOG.md and switch to pnpm for frontend SBOM generation

## [0.1.21] - 2026-03-19

### Miscellaneous

- Bump version to 0.1.21 in package.json, Cargo.toml, and Cargo.lock

### Refactoring

- Rename props for clarity in NotebookCell component

## [0.1.20] - 2026-03-09

### Features

- Flatten MongoDB document results and improve query handling in federation

## [0.1.19] - 2026-03-07

### Bug Fixes

- Replace tokio::spawn with tauri::async_runtime::spawn for health check interval

### Features

- Add maintenance operations support for PostgreSQL, SQLite, and SQL Server
- Integrate license checks for sandbox and query library features

### Refactoring

- Move formatBytes function to improve code organization and reduce duplication

## [0.1.18] - 2026-02-21

### Features

- Implement resizable sidebar functionality (#24)
- Update README to enhance multi-database support and core capabilities
- Enhance Windows build process with Defender monitoring disable and increase Tauri app timeout

## [0.1.17] - 2026-02-14

### Features

- Add Redis support to COLUMN_TYPES and SQL formatter

## [0.1.16] - 2026-02-05

### Bug Fixes

- Add type annotations for DescribeTableResponse in DiffConfigPanel

### Features

- Add speed display to ExportProgressToast and update rows_per_second type

## [0.1.11] - 2026-01-27

### Features

- Add editable data cell and foreign key peek tooltip components

## [0.1.10] - 2026-01-25

### Features

- Add action column with open row functionality and update translations

## [0.1.9] - 2026-01-24

### Miscellaneous

- Bump version to 0.1.9 and update MSIX identity in release workflow

## [0.1.4] - 2026-01-23

### Features

- Setup macOS notarization + bump version 0.1.3

## [0.1.1] - 2026-01-23

### Bug Fixes

- Adjust overflow behavior in App and TabBar components for better layout management

### Features

- Add initial multi-database drivers, comprehensive data browsing and query UI, and internationalization support.
- Enhance global search with connection and favorite query support, and improve query handling in QueryPanel
- Refactor GlobalSearch component for improved search functionality and UI consistency
- Implement read-only mode across application
- Add StatusBar component and implement connection indicators with localization support
- Rename project references from 'qoreqb' to 'qoredb', add MongoDB collection creation support, and enhance UI components for better user experience
- **DataGrid, RowModal**: Add delete confirmation dialog with preview functionality and localization support
- Implement core database management UI including database browser, data grid, and internationalization support.
- Add MySQL driver and integrate into database browser
- Initialize core application with tab system, database browser, connection management, and essential UI components.
- Enhance database management with schema refresh triggers, improved query safety checks, and localization updates
- Implement connection context menu and table change events
- Enhance diagnostics settings with logging and history management options
- Implement core Tauri backend with database drivers, session management, and vault, alongside initial UI for database browsing, data display, and internationalization.
- Introduce a new DataGrid component with search, column management, pagination, and data export/copy capabilities, supported by a new database driver definition module.
- Enhance DataGridToolbar with filter toggle and new filter component
- Add SSH tunneling support and configuration for Docker setup
- Implement safety policy management with configuration and UI integration
- Implement Query Management System
- Enhance SQL safety analysis and add cancellation support for database drivers
- Implement driver capabilities for transactions and mutations support
- Add connectedConnectionId prop to Sidebar for improved connection handling
- Enhance observability with structured logging and tracing support
- Implement log export functionality with UI support
- Add metrics tracking and retrieval for development builds
- **connection-modal**: Add advanced connection settings and SSH tunnel configuration
- Add CI workflow for backend tests and implement driver limitations documentation
- Implement MongoDB query safety classification and enhance SQL safety checks
- Enhance SQL editor with snippet support, formatting, and explain functionality
- Implement lazy loading for collections and add load more functionality
- Enhance DocumentResults component with pagination and connection pool settings
- Implement duplicate connection functionality and update dependencies
- Add Query Library functionality with folder and item management
- Add command palette functionality and integrate query library modal
- Enhance logging and observability features with structured logs and keyboard shortcuts
- Add Tooltip component and integrate tooltips in various UI elements
- Implement query panel toolbar, connection context menu, query history, and internationalization with tooltip support.
- Add UI for creating tables and inserting/updating rows.
- Add onboarding flow with analytics consent and a new query panel toolbar.
- Add comprehensive logging, error handling, and a new query panel toolbar.
- Add config backup and project transfer functionality
- Add analytics events for connection testing and onboarding process
- Implement production safety features, enhance credential security with `Sensitive` types, and add comprehensive security documentation including a threat model.
- Add project documentation including security policy, contributing guidelines, and a feature list.

### Refactoring

- **QueryPanel**: Improve code readability and maintainability by standardizing formatting and indentation
- Update vault storage to use dynamic app config directory, add dev dependency, and update todo documentation.


