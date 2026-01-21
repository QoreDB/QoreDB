<div align="center">

# 🗄️ QoreDB

**Next Generation Database Client**

A modern, powerful, and intuitive database management tool built with Tauri, React, and Rust. 

[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Tauri](https://img.shields.io/badge/Tauri-2.0-blue.svg)](https://tauri.app/)
[![React](https://img.shields.io/badge/React-19.1-blue.svg)](https://reactjs.org/)
[![TypeScript](https://img.shields.io/badge/TypeScript-5.8-blue.svg)](https://www.typescriptlang.org/)
[![Rust](https://img.shields.io/badge/Rust-2021-orange.svg)](https://www.rust-lang.org/)

[Features](#-features) • [Installation](#-installation) • [Usage](#-usage) • [Development](#-development) • [Contributing](#-contributing)

</div>

---

## ✨ Features

### 🎯 Multi-Database Support
- **PostgreSQL** - Full support with advanced features
- **MySQL** - Complete integration
- **MongoDB** - NoSQL database support

### 🚀 Core Capabilities
- **SQL Query Editor** - Syntax highlighting with CodeMirror
- **Table Browser** - Intuitive data exploration and visualization
- **Database Browser** - Schema navigation and management
- **Connection Manager** - Save and organize multiple database connections
- **Query History** - Track and reuse previous queries
- **Favorites** - Bookmark your most-used queries
- **Global Search** - Quick access to connections, queries, and favorites

### 🔐 Security & Features
- **Secure Credential Storage** - Using native OS keychains
- **SSH Tunneling Support** - Secure connections via SSH
- **Environment Labels** - Distinguish between dev, staging, and production
- **Read-Only Mode** - Protect production databases from accidental modifications

### 🎨 User Experience
- **Multi-Tab Interface** - Work with multiple queries and tables simultaneously
- **Dark/Light Theme** - Adaptive theming for your preference
- **Internationalization** - Multi-language support (EN, FR)
- **Virtual Scrolling** - Handle large datasets efficiently
- **Responsive UI** - Modern interface built with Radix UI and Tailwind CSS

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

| Shortcut | Action |
|----------|--------|
| `Cmd/Ctrl + K` | Global search |
| `Cmd/Ctrl + N` | New query tab |
| `Cmd/Ctrl + W` | Close current tab |
| `Cmd/Ctrl + ,` | Settings |

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
- SQLx (PostgreSQL, MySQL)
- MongoDB Driver
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

This project is licensed under the MIT License - see the [LICENSE](LICENSE) file for details.

---

## 👤 Author

**Raphaël Plassart**
- Email: qoredb@gmail.com
- GitHub: [@raphplt](https://github.com/raphplt)

---

## 🙏 Acknowledgments

- [Tauri](https://tauri.app/) - For the amazing framework
- [CodeMirror](https://codemirror.net/) - SQL editor component
- [Radix UI](https://www.radix-ui.com/) - Accessible component primitives
- [Tailwind CSS](https://tailwindcss.com/) - Styling framework

---

<div align="center">

**[⬆ Back to Top](#-qoredb)**

Made with ❤️ by [raphplt](https://github.com/raphplt)

</div>