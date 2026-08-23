// SPDX-License-Identifier: Apache-2.0

export interface ChangelogItem {
  title: string;
  description: string;
  type: 'feature' | 'improvement' | 'fix';
  proOnly?: boolean;
}

export interface ChangelogEntry {
  version: string;
  date: string;
  items: ChangelogItem[];
}

/**
 * Changelog entries for the What's New panel.
 * Keep entries in reverse-chronological order (newest first).
 * Strings are literal text (not i18n keys) — release notes are factual and language-neutral.
 */
export const CHANGELOG: ChangelogEntry[] = [
  {
    version: '0.1.38',
    date: '2026-08-21',
    items: [
      {
        title: 'Query Replay Lab',
        description:
          'Record the queries you run while you work, then replay the set after a migration or against another connection. The report says what broke, what returns a different row count, what changed in content and what got slower.',
        type: 'feature',
        proOnly: true,
      },
      {
        title: 'See the difference, not just the verdict',
        description:
          'Captured rows stay on your machine and never reach the repository, so any report row opens the baseline ↔ run diff. Ignored columns keep timestamps from reporting a change on every run.',
        type: 'feature',
        proOnly: true,
      },
      {
        title: 'Replay sets are shared through Git',
        description:
          'A set lives in .qoredb/replays/ and holds queries and expectations — never a result row, though the query text is versioned as-is. Mutations are excluded from replay by default and refused outright against production, classified from the preflight rather than from the file.',
        type: 'feature',
        proOnly: true,
      },
      {
        title: 'Compare two connections side by side',
        description:
          'Replay the same set on two live connections — production against a migrated staging, for instance — and compare the two results against each other rather than against a recording.',
        type: 'feature',
        proOnly: true,
      },
      {
        title: 'PlanetScale, Amazon DocumentDB and Dragonfly',
        description:
          'Three wire-compatible engines now have their own identity, icon and DSN detection: PlanetScale over the MySQL driver and DocumentDB over the MongoDB driver, both with TLS forced on connect, and Dragonfly over the Redis driver.',
        type: 'feature',
      },
    ],
  },
  {
    version: '0.1.35',
    date: '2026-07-27',
    items: [
      {
        title: 'Q, your database agent',
        description:
          'A dedicated chat tab where you ask in plain language. Q explores the schema, runs read-only queries and answers from the rows it actually read, showing every step and result inline.',
        type: 'feature',
        proOnly: true,
      },
      {
        title: 'Approval before anything is written',
        description:
          'Writes and cross-connection access pause the agent and ask you first. Mutations are refused outright in production, and an approval is only remembered in development and staging.',
        type: 'feature',
        proOnly: true,
      },
      {
        title: 'Conversations that survive a restart',
        description:
          'Conversations are listed, renamed and resumed across sessions. Only the messages and short tool summaries are stored — query results never touch the disk.',
        type: 'feature',
        proOnly: true,
      },
      {
        title: 'Qore AI Local',
        description:
          'Run the agent against a local model with no API key: a bundled llama-server runtime for macOS, Windows and Linux, built reproducibly, installed resumably and verified by SHA-256 against a signed manifest.',
        type: 'feature',
        proOnly: true,
      },
    ],
  },
  {
    version: '0.1.34',
    date: '2026-07-16',
    items: [
      {
        title: 'Migrations Manager',
        description:
          'Version your schema as .sql files in .qoredb/migrations/, share them through Git, and apply or roll them back against a connection. Applied state, checksum drift, and failures are tracked in the target database.',
        type: 'feature',
      },
      {
        title: 'Migration generation from schema diff',
        description:
          'Turn a schema comparison into a ready-to-run migration, with explicit warnings when the target dialect cannot express a change.',
        type: 'feature',
        proOnly: true,
      },
      {
        title: 'Migration runner safety',
        description:
          'Every statement is vetted before any of them runs, MySQL and MariaDB DDL migrations no longer report a failed run as applied, and the SQL splitter now executes exactly the SQL you wrote.',
        type: 'fix',
      },
    ],
  },
  {
    version: '0.1.33',
    date: '2026-07-02',
    items: [
      {
        title: 'SQL import workflow',
        description:
          'Import SQL files directly from a connection, with progress feedback and safer error handling.',
        type: 'feature',
      },
      {
        title: 'Database maintenance tools',
        description:
          'Run supported maintenance operations from the database tree across PostgreSQL-compatible drivers.',
        type: 'feature',
      },
      {
        title: 'Faster database navigation',
        description:
          'Tabs, the database tree, and large result views use less work while navigating and rendering.',
        type: 'improvement',
      },
    ],
  },
  {
    version: '0.1.32',
    date: '2026-06-29',
    items: [
      {
        title: 'Index suggestions',
        description:
          'Query plans can now surface actionable index suggestions with ready-to-copy SQL.',
        type: 'feature',
        proOnly: true,
      },
      {
        title: 'Cross-environment comparison',
        description:
          'Compare matching tables across development, staging, and production connections.',
        type: 'feature',
      },
      {
        title: 'Execution timing and driver polish',
        description:
          'Result grids expose clearer timing information alongside stability and compatibility fixes.',
        type: 'improvement',
      },
    ],
  },
  {
    version: '0.1.31',
    date: '2026-06-17',
    items: [
      {
        title: 'Natural-language filters',
        description:
          'Describe the rows you need and turn the request into structured table filters.',
        type: 'feature',
        proOnly: true,
      },
      {
        title: 'Test data generator',
        description: 'Generate realistic seed data while keeping schema constraints in view.',
        type: 'feature',
        proOnly: true,
      },
      {
        title: 'Parameterized notebooks',
        description: 'Reuse variables across notebook cells for repeatable database workflows.',
        type: 'improvement',
      },
    ],
  },
  {
    version: '0.1.30',
    date: '2026-06-08',
    items: [
      {
        title: 'Command-line access',
        description:
          'New qore CLI runs queries against your saved connections from scripts and CI, reusing the same vault and safety gates as the desktop app.',
        type: 'feature',
      },
      {
        title: 'MCP server',
        description:
          'Expose your connections to AI agents through a read-only MCP server, with destructive operations blocked at the source.',
        type: 'feature',
      },
      {
        title: 'Multi-statement queries',
        description:
          'Run several statements at once and browse each result set separately, with a new table context menu to open queries quickly.',
        type: 'improvement',
      },
    ],
  },
  {
    version: '0.1.29',
    date: '2026-05-21',
    items: [
      {
        title: 'Query result cache',
        description:
          'Recent table navigation is served instantly from a local cache, invalidated automatically when you change data through QoreDB.',
        type: 'feature',
      },
      {
        title: 'Plugin system',
        description:
          'Install declarative plugins that contribute SQL snippet packs, connection templates, and color themes — no code execution.',
        type: 'feature',
      },
      {
        title: 'Security hardening',
        description:
          'Per-connection query rate limiting stops runaway loops, and filesystem access is now restricted to an explicit allow-list.',
        type: 'improvement',
      },
    ],
  },
  {
    version: '0.1.28',
    date: '2026-05-17',
    items: [
      {
        title: 'Data Contracts',
        description:
          'Define and enforce schema invariants across your databases — catch breaking changes at connect time.',
        type: 'feature',
        proOnly: true,
      },
      {
        title: 'Instant Data API',
        description:
          'Expose any table as a local REST or GraphQL endpoint with zero configuration.',
        type: 'feature',
        proOnly: true,
      },
      {
        title: 'ClickHouse driver',
        description: 'New driver for ClickHouse — fast analytical queries on columnar data.',
        type: 'feature',
      },
    ],
  },
  {
    version: '0.1.26',
    date: '2026-04-22',
    items: [
      {
        title: 'SQL Server Windows Authentication (NTLM)',
        description:
          'Connect to SQL Server with DOMAIN\\user NTLM credentials instead of a SQL login. Available on Windows clients; unlocks AD-only enterprise environments.',
        type: 'feature',
      },
      {
        title: 'SQL Server Integrated Authentication (SSPI + Kerberos)',
        description:
          'Zero-password connection reusing the current OS session. Uses SSPI on Windows, and Kerberos/GSSAPI on macOS and Linux (requires a kinit ticket).',
        type: 'feature',
      },
    ],
  },
  {
    version: '0.1.21',
    date: '2026-03-19',
    items: [
      {
        title: 'Database Notebooks',
        description:
          'Multi-cell notebooks with SQL, Markdown, and Chart cells with inter-cell variable references',
        type: 'feature',
      },
      {
        title: 'Zen Mode',
        description: 'Distraction-free query editing with a single shortcut',
        type: 'feature',
      },
      {
        title: 'Mistral & Gemini AI',
        description: 'New AI providers for natural language query generation',
        type: 'feature',
      },
      {
        title: 'Transaction Management',
        description: 'BEGIN, COMMIT, ROLLBACK with statement counter in the toolbar',
        type: 'feature',
      },
      {
        title: 'Tab Pinning & Reordering',
        description: 'Pin important tabs and reorder them via context menu',
        type: 'improvement',
      },
      {
        title: 'Server-side Column Filters',
        description: 'Filter columns directly on the server for large datasets',
        type: 'improvement',
      },
      {
        title: 'EXPLAIN Plan Viewer',
        description: 'Visualize query execution plans for PostgreSQL and MySQL',
        type: 'feature',
      },
      {
        title: 'Keyboard Shortcuts Cheatsheet',
        description: 'Press ? to see all available shortcuts',
        type: 'improvement',
      },
      {
        title: 'Feature Tour',
        description: 'Guided tour for new users on first launch',
        type: 'improvement',
      },
      {
        title: 'In-app Updates',
        description: 'Check and install updates directly from the app',
        type: 'feature',
      },
      {
        title: 'Accessibility',
        description: 'ARIA roles, skip links, and improved keyboard navigation',
        type: 'improvement',
      },
    ],
  },
  {
    version: '0.1.20',
    date: '2026-03-09',
    items: [
      {
        title: 'Column Pinning',
        description: 'Pin columns left or right in the DataGrid',
        type: 'feature',
      },
      {
        title: 'Content Breadcrumb',
        description: 'Navigate database > schema > table via a breadcrumb bar',
        type: 'improvement',
      },
      {
        title: 'MongoDB Federation Fix',
        description: 'Fixed document flattening in cross-database federation queries',
        type: 'fix',
      },
    ],
  },
  {
    version: '0.1.19',
    date: '2026-03-07',
    items: [
      {
        title: 'CockroachDB Driver',
        description: 'Full support for CockroachDB with PostgreSQL wire protocol',
        type: 'feature',
      },
      {
        title: 'Routines Management',
        description: 'View, create, and drop stored procedures and functions',
        type: 'feature',
      },
      {
        title: 'Triggers & Events',
        description: 'Browse and manage database triggers and scheduled events',
        type: 'feature',
      },
      {
        title: 'Snapshots',
        description: 'Save and compare query result snapshots over time',
        type: 'feature',
      },
      {
        title: 'Connection Health',
        description: 'Automatic health monitoring with SSH tunnel reconnection',
        type: 'improvement',
      },
    ],
  },
  {
    version: '0.1.18',
    date: '2026-02-21',
    items: [
      {
        title: 'AI Assistant',
        description: 'Natural language to SQL, result explanation, and error fixing',
        type: 'feature',
      },
      {
        title: 'Cross-database Federation',
        description: 'Query multiple databases in a single SQL statement via DuckDB',
        type: 'feature',
      },
      {
        title: 'DuckDB & SQL Server Drivers',
        description: 'Two new database drivers for analytics and enterprise use',
        type: 'feature',
      },
      {
        title: 'XLSX & Parquet Export',
        description: 'Export query results to Excel and Parquet formats',
        type: 'feature',
      },
      {
        title: 'Infinite Scroll',
        description: 'Seamless lazy loading in the DataGrid for large result sets',
        type: 'improvement',
      },
      {
        title: 'ER Diagrams',
        description: 'Visual entity-relationship diagrams now available in Core tier',
        type: 'feature',
      },
    ],
  },
  {
    version: '0.1.17',
    date: '2026-02-14',
    items: [
      {
        title: 'Redis Driver',
        description: 'Full Redis integration with key browsing and command execution',
        type: 'feature',
      },
      {
        title: 'Trigger & Event Support',
        description: 'Manage triggers and scheduled events for MySQL, PostgreSQL, and SQLite',
        type: 'feature',
      },
      {
        title: 'Connection Validation',
        description: 'Improved connection testing with clearer error messages',
        type: 'improvement',
      },
      {
        title: 'Update Checks',
        description: 'Automatic update check on startup',
        type: 'improvement',
      },
    ],
  },
  {
    version: '0.1.16',
    date: '2026-02-05',
    items: [
      {
        title: 'Database Routines',
        description: 'Browse and manage PostgreSQL/MySQL functions and procedures',
        type: 'feature',
      },
      {
        title: 'Data Diff',
        description: 'Compare two query results or table snapshots side by side',
        type: 'feature',
      },
      {
        title: 'HTML Export',
        description: 'Export query results as styled HTML tables',
        type: 'feature',
      },
      {
        title: 'PostgreSQL Enum Handling',
        description: 'Improved driver support for enum types',
        type: 'fix',
      },
    ],
  },
  {
    version: '0.1.15',
    date: '2026-02-02',
    items: [
      {
        title: 'SQLite Support',
        description: 'New SQLite driver for local and file-based databases',
        type: 'feature',
      },
      {
        title: 'Streaming Export',
        description: 'Export large datasets without memory issues via streaming pipeline',
        type: 'improvement',
      },
      {
        title: 'Windows Title Bar Fix',
        description: 'Fixed window freeze on custom title bar interactions',
        type: 'fix',
      },
    ],
  },
  {
    version: '0.1.14',
    date: '2026-01-31',
    items: [
      {
        title: 'Connection URL Parsing',
        description: 'Connect via URL/DSN with real-time validation and auto-fill',
        type: 'feature',
      },
      {
        title: 'Backend Pagination',
        description: 'Server-driven pagination for consistent performance on large tables',
        type: 'improvement',
      },
    ],
  },
  {
    version: '0.1.12',
    date: '2026-01-30',
    items: [
      {
        title: 'UI/UX Overhaul',
        description: 'Complete redesign with custom title bar and modern layout',
        type: 'improvement',
      },
      {
        title: 'Full-text Search',
        description: 'Search across all tables and columns in a database',
        type: 'feature',
      },
      {
        title: 'Safety Rules Editor',
        description: 'Configure production safety rules with confirmation dialogs',
        type: 'feature',
      },
      {
        title: 'French & English',
        description: 'Full localization for both languages',
        type: 'feature',
      },
    ],
  },
];
