<div align="center">

# 🗄️ QoreDB

**Next Generation Database Client**

A modern, powerful, and intuitive database management tool built with Tauri, React, and Rust.

[![License](https://img.shields.io/badge/license-Apache--2.0%20%2F%20BUSL--1.1-blue.svg)](LICENSE)
[![Tauri](https://img.shields.io/badge/Tauri-2.0-blue.svg)](https://tauri.app/)
[![React](https://img.shields.io/badge/React-19.1-blue.svg)](https://reactjs.org/)
[![TypeScript](https://img.shields.io/badge/TypeScript-5.8-blue.svg)](https://www.typescriptlang.org/)
[![Rust](https://img.shields.io/badge/Rust-2021-orange.svg)](https://www.rust-lang.org/)

[Features](#-features) • [Installation](#-installation) • [Usage](#-usage) • [Development](#-development) • [Contributing](#-contributing)

</div>

---

## ✨ Features

### 🎯 Multi-Database Support

- **PostgreSQL**
- **MySQL / MariaDB**
- **SQL Server**
- **SQLite**
- **DuckDB**
- **MongoDB**
- **Redis**

### 🚀 Query & Schema Toolkit

- **SQL + Mongo Editors** - Syntax highlighting, formatting, snippets, multi-statement execution
- **Query Library** - Folders, tags, JSON import/export, reusable queries
- **ER Diagram** - Interactive schema graph with isolate/focus workflows
- **Visual Data Diff** - Side-by-side comparison of table/query results
- **Global Full-Text Search** - Search values across tables and columns
- **Foreign Key Peek + Virtual Relations** - Better navigation even without native FK constraints

### 📊 Data Operations

- **High-Performance Data Grid** - Virtualization, server-side filtering/sorting, pagination, infinite scroll
- **Inline Editing** - Edit rows directly in SQL and NoSQL datasets
- **Export Pipeline** - CSV, JSON, SQL, HTML (+ XLSX/Parquet in Pro)
- **Cross-Database Federation** - Query and join across active connections (DuckDB engine)

### 🔐 Security & Reliability

- **Secure Vault** - Native OS keychain storage + optional app lock
- **SSH Tunneling** - Native OpenSSH-based secure remote access
- **Environment Safety** - Dev/Staging/Prod guards, dangerous query detection, read-only mode
- **Universal Query Interceptor** - Central hooks for safety, audit, and profiling
- **Resilience Tooling** - Crash recovery, auto-update, project import/export, settings backup

### 🎨 User Experience

- **Multi-Tab Workspace** - Query tabs + table tabs with persistent context
- **Global Search (Cmd/Ctrl + K)** - Fast access to connections, history, commands, and library items
- **Dark/Light Theme** - Adaptive theming
- **Internationalization** - English + French
- **Responsive Desktop UI** - Built with Radix UI and Tailwind CSS

---

## 📦 Installation

### Prerequisites

- **Node.js** 18+ and **pnpm**
- **Rust** 1.70+ (for Tauri backend)
- **System Dependencies** for Tauri ([see Tauri prerequisites](https://tauri.app/start/prerequisites/))

On Ubuntu/Debian, the following packages are commonly required to build the Rust/Tauri backend (they provide `pkg-config` files like `glib-2.0.pc`):

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

### Download

Download the latest release for your platform from the [Releases page](https://github.com/raphplt/QoreDB/releases).

### Build from Source

```bash
# Clone the repository
git clone https://github.com/raphplt/QoreDB.git
cd QoreDB

# Install dependencies
pnpm install

# Run in development mode
pnpm tauri dev

# Build for production
pnpm tauri build
```

---

## 🚀 Usage

### Quick Start

1. **Launch QoreDB** - Open the application
2. **Add Connection** - Click the "+" button to create a new database connection
3. **Connect** - Select your connection from the sidebar
4. **Explore** - Browse databases, tables, and run queries

### Keyboard Shortcuts

| Shortcut       | Action            |
| -------------- | ----------------- |
| `Cmd/Ctrl + K` | Global search     |
| `Cmd/Ctrl + N` | New query tab     |
| `Cmd/Ctrl + W` | Close current tab |
| `Cmd/Ctrl + ,` | Settings          |

### Connection Configuration

```json
{
  "name": "My Database",
  "driver": "postgres",
  "host": "localhost",
  "port": 5432,
  "database": "mydb",
  "username": "user",
  "environment": "development",
  "read_only": false
}
```

---

## 🛠️ Development

### Tech Stack

**Frontend:**

- React 19.1
- TypeScript 5.8
- Vite 7
- Tailwind CSS 4
- Radix UI
- CodeMirror 6
- TanStack Table

**Backend:**

- Rust (Edition 2021)
- Tauri 2.0
- SQLx (PostgreSQL, MySQL, SQLite)
- Tiberius + bb8 (SQL Server)
- MongoDB + Redis native drivers
- DuckDB (embedded analytics + federation engine)
- Tokio (Async Runtime)

### Project Structure

```
QoreDB/
├── src/                    # React frontend source
│   ├── components/         # UI components
│   │   ├── Browser/        # Database/Table browsers
│   │   ├── Connection/     # Connection management
│   │   ├── Query/          # Query editor
│   │   ├── Sidebar/        # Navigation sidebar
│   │   ├── Tabs/           # Tab system
│   │   └── ui/             # Reusable UI components
│   ├── lib/                # Utilities and Tauri bindings
│   ├── locales/            # i18n translations
│   └── App.tsx             # Main application
├── src-tauri/              # Rust backend
│   ├── src/                # Rust source code
│   └── Cargo.toml          # Rust dependencies
├── public/                 # Static assets
└── doc/                    # Documentation
```

### Scripts

```bash
# Development
pnpm dev                    # Start Vite dev server
pnpm tauri dev              # Run Tauri app in dev mode

# Building
pnpm build                  # Build frontend
pnpm tauri build            # Build full application

# Code Quality
pnpm lint                   # Lint code
pnpm lint:fix               # Fix linting issues
pnpm format                 # Check formatting
pnpm format:write           # Format code
```

### Docker Support

```bash
# Start development databases
docker-compose up -d
```

---

## 🤝 Contributing

Contributions are welcome! Please feel free to submit a Pull Request.

1. Fork the repository
2. Create your feature branch (`git checkout -b feature/amazing-feature`)
3. Commit your changes (`git commit -m 'Add some amazing feature'`)
4. Push to the branch (`git push origin feature/amazing-feature`)
5. Open a Pull Request

### Development Guidelines

- Follow the existing code style
- Write meaningful commit messages
- Add tests for new features
- Update documentation as needed

---

## 📄 License

Core files are licensed under Apache 2.0 (see [LICENSE](LICENSE)).

Premium files are licensed under Business Source License 1.1 (see [LICENSE-BSL](LICENSE-BSL)).

---

## 👤 Author

**Raphaël Plassart**

- Email: <qoredb@gmail.com>
- LinkedIn: [raphaël-plassart](www.linkedin.com/in/raphaël-plassart)
- GitHub: [@raphplt](https://github.com/raphplt)

---

## 🙏 Acknowledgments

- [Tauri](https://tauri.app/) - For the amazing framework
- [CodeMirror](https://codemirror.net/) - SQL editor component
- [Radix UI](https://www.radix-ui.com/) - Accessible component primitives
- [Tailwind CSS](https://tailwindcss.com/) - Styling framework
- [SQLx](https://github.com/launchbadge/sqlx) - Async SQL toolkit

---

<div align="center">

Made with ❤️ by [raphplt](https://github.com/raphplt)

</div>
