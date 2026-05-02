<div align="center">

# QoreDB

**Next-Generation Database Client**

A modern, powerful, and intuitive database management tool built with Tauri, React, and Rust.
Lightweight alternative to DBeaver and pgAdmin, designed for developers.

[![License](https://img.shields.io/badge/license-Apache--2.0%20%2F%20BUSL--1.1-blue.svg)](LICENSE)
[![Tauri](https://img.shields.io/badge/Tauri-2.10-blue.svg)](https://tauri.app/)
[![React](https://img.shields.io/badge/React-19-blue.svg)](https://reactjs.org/)
[![TypeScript](https://img.shields.io/badge/TypeScript-5.9-blue.svg)](https://www.typescriptlang.org/)
[![Rust](https://img.shields.io/badge/Rust-2021-orange.svg)](https://www.rust-lang.org/)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey.svg)]()

[Features](#-features) · [Installation](#-installation) · [Usage](#-usage) · [Development](#-development) · [Contributing](#-contributing)

</div>

---

## Features

### Multi-Database Support

| SQL | NoSQL / Analytical |
| --- | --- |
| PostgreSQL | MongoDB |
| MySQL / MariaDB | Redis |
| SQL Server | DuckDB |
| SQLite | |
| CockroachDB | |

### Database Notebooks

Executable documents mixing SQL/Mongo and Markdown cells, connected to a live database.

- Parameterized variables (`$customer_id`, `{{date_from}}`) with typed inputs
- Run All / Run From Here with stop-on-error
- Inter-cell references and Chart cells (bar, line, pie, scatter) [Pro]
- Import from `.sql` / `.md`, export to Markdown or standalone HTML
- `.qnb` file format, Git-diffable

### Query & Schema Toolkit

- **SQL + Mongo Editors** — Syntax highlighting, formatting, snippets, multi-statement execution
- **Query Library** — Folders, tags, JSON import/export, reusable queries
- **ER Diagram** — Interactive schema graph with isolate/focus workflows [Pro]
- **Visual DDL Editor** — Create and Alter Table with live SQL preview, foreign keys, indexes, check constraints (PG, MySQL, SQLite, DuckDB, SQL Server, CockroachDB)
- **Explain Plan Visualization** — Interactive execution plan tree with cost highlighting (PostgreSQL, MySQL, SQL Server)
- **Visual Data Diff** — Side-by-side comparison of table/query results [Pro]
- **Global Full-Text Search** — Search values across all tables and columns
- **Foreign Key Peek + Virtual Relations** — Navigation even without native FK constraints
- **Routines, Procedures, Triggers & Events** — List, create, and edit stored objects

### Data Operations

- **High-Performance Data Grid** — Virtualization, server-side filtering/sorting, pagination, infinite scroll, column pinning
- **Inline Editing** — Edit rows directly in SQL and NoSQL datasets
- **Bulk Edit** — Multi-row column updates with live SQL preview (≤ 5 rows Core, more in Pro)
- **Blob/Binary Viewer** — Hex / base64 / image preview (PNG, JPEG, GIF, SVG, BMP, ICO) with download and copy as data URI
- **CSV Import** — Automatic separator/encoding detection, column mapping, preview before import
- **Transaction Management** — Toggle autocommit, explicit Commit/Rollback, active transaction indicator
- **Export Pipeline** — CSV, JSON, SQL, HTML, self-contained HTML (+ XLSX/Parquet in Pro)
- **Cross-Database Federation** — Query and join across active connections via DuckDB
- **Result Snapshots** — Save and compare query results over time
- **Sandbox Mode** — Isolated local changes with migration generation

### Security & Reliability

- **Secure Vault** — Native OS keychain storage (Argon2) + optional app lock
- **SSH Tunneling** — Native OpenSSH-based secure remote access
- **Environment Safety** — Dev/Staging/Prod guards, dangerous query detection, read-only mode
- **Universal Query Interceptor** — Central hooks for safety, audit, and profiling
- **Connection Resilience** — Automatic reconnection, health monitoring, smart keep-alive
- **Content Security Policy** — Strict CSP configuration
- **Background Job Manager** — Async execution for long-running tasks with error recovery

### AI Assistant [Pro]

- Contextual query generation and error correction
- Schema-aware suggestions

### User Experience

- **Multi-Tab Workspace** — Drag-and-drop reorder, pinned tabs, optional grouping by connection, persistent context across connection switches
- **Global Search (Cmd/Ctrl + K)** — Connections, history, commands, library items
- **Breadcrumb Navigation** — `Connection > Database > Schema > Table` clickable path
- **Dark / Light Theme**
- **9 Languages** — English, French, Spanish, German, Portuguese (BR), Russian, Japanese, Korean, Chinese (Simplified)

---

## Installation

### Download

Download the latest release for your platform from the [Releases page](https://github.com/raphplt/QoreDB/releases).

| Platform | Format |
| --- | --- |
| **Windows** | `.msi` / `.exe` |
| **macOS** | `.dmg` (ARM64 & Intel) |
| **Linux** | `.deb` / `.AppImage` |

### Arch Linux (AUR)

```bash
yay -S qoredb-bin
```

### Build from Source

**Prerequisites:** Node.js 18+, pnpm, Rust 1.70+, [Tauri system dependencies](https://tauri.app/start/prerequisites/)

<details>
<summary>Ubuntu/Debian system packages</summary>

```bash
sudo apt-get update
sudo apt-get install -y \
  pkg-config \
  libglib2.0-dev \
  libgtk-3-dev \
  libwebkit2gtk-4.1-dev \
  libayatana-appindicator3-dev \
  librsvg2-dev
```

</details>

```bash
git clone https://github.com/raphplt/QoreDB.git
cd QoreDB
pnpm install
pnpm tauri dev      # development
pnpm tauri build    # production
```

---

## Usage

### Quick Start

1. **Launch QoreDB**
2. **Add a connection** — Click "+" in the sidebar
3. **Connect** — Select your connection
4. **Explore** — Browse databases, tables, run queries or open a notebook

### Keyboard Shortcuts

| Shortcut | Action |
| --- | --- |
| `Cmd/Ctrl + K` | Global search |
| `Cmd/Ctrl + N` | New query tab |
| `Cmd/Ctrl + W` | Close current tab |
| `Cmd/Ctrl + Enter` | Execute query |
| `Cmd/Ctrl + S` | Save |
| `Cmd/Ctrl + ,` | Settings |

---

## Development

### Tech Stack

**Frontend:**

- React 19 · TypeScript 5.9 · Vite 8 · Tailwind CSS 4
- Radix UI · CodeMirror 6 · TanStack Table · i18next

**Backend:**

- Rust 2021 · Tauri 2.10 · Tokio
- SQLx (PostgreSQL, MySQL, SQLite) · Tiberius + bb8 (SQL Server)
- MongoDB & Redis native drivers · DuckDB (embedded analytics + federation)

### Project Structure

```
QoreDB/
├── src/                    # React frontend
│   ├── components/         # UI components
│   │   ├── Browser/        # Database/table browsers
│   │   ├── Connection/     # Connection management
│   │   ├── Notebook/       # Database notebooks
│   │   ├── Query/          # Query editor
│   │   ├── Schema/         # ER diagram, explain plan
│   │   ├── Sidebar/        # Navigation sidebar
│   │   ├── Tabs/           # Tab system
│   │   └── ui/             # Reusable primitives (Radix-based)
│   ├── hooks/              # Custom React hooks
│   ├── lib/                # Tauri bindings, utilities, types
│   └── locales/            # i18n translations (9 languages)
├── src-tauri/              # Rust backend
│   ├── src/commands/       # Tauri command handlers
│   ├── src/engine/         # Database abstraction (traits, drivers)
│   └── src/vault/          # Encrypted credential storage
├── doc/                    # Documentation
└── aur/                    # AUR package definition
```

### Scripts

```bash
pnpm tauri dev              # Run app in dev mode (hot reload)
pnpm tauri build            # Build production app
pnpm lint:fix               # Lint + auto-fix
pnpm format:write           # Format code
pnpm test                   # Run Rust tests
docker-compose up -d        # Start dev databases
```

---

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

### Guidelines

- Follow existing code style
- Write meaningful commit messages
- Add SPDX license headers to new files (`Apache-2.0` for core, `BUSL-1.1` for premium)
- Update documentation as needed

---

## License

Core files are licensed under **Apache 2.0** — see [LICENSE](LICENSE).

Premium files are licensed under **Business Source License 1.1** — see [LICENSE-BSL](LICENSE-BSL).

---

## Author

**Raphaël Plassart**

- Email: <qoredb@gmail.com>
- LinkedIn: [raphaël-plassart](https://www.linkedin.com/in/raphaël-plassart)
- GitHub: [@raphplt](https://github.com/raphplt)

---

## Acknowledgments

- [Tauri](https://tauri.app/) — Desktop framework
- [CodeMirror](https://codemirror.net/) — SQL editor
- [Radix UI](https://www.radix-ui.com/) — Accessible component primitives
- [Tailwind CSS](https://tailwindcss.com/) — Styling
- [SQLx](https://github.com/launchbadge/sqlx) — Async SQL toolkit
- [DuckDB](https://duckdb.org/) — Embedded analytics engine

---

<div align="center">

Made with ❤️ by [raphplt](https://github.com/raphplt)

</div>
