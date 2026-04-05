// SPDX-License-Identifier: Apache-2.0

export interface ChangelogItem {
  title: string;
  description: string;
  type: 'feature' | 'improvement' | 'fix';
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
