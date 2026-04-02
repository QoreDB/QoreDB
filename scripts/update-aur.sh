#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Updates PKGBUILD and .SRCINFO with a new version for AUR publishing.
# Usage: ./scripts/update-aur.sh <version>
# Example: ./scripts/update-aur.sh 0.1.23

set -euo pipefail

VERSION="${1:?Usage: $0 <version>}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
AUR_DIR="${SCRIPT_DIR}/../aur/qoredb-bin"

echo "Updating AUR package to version ${VERSION}..."

# Update PKGBUILD
sed -i "s/^pkgver=.*/pkgver=${VERSION}/" "${AUR_DIR}/PKGBUILD"
sed -i "s/^pkgrel=.*/pkgrel=1/" "${AUR_DIR}/PKGBUILD"

# Regenerate .SRCINFO from PKGBUILD
cat > "${AUR_DIR}/.SRCINFO" <<EOF
pkgbase = qoredb-bin
	pkgdesc = Next gen database client — lightweight alternative to DBeaver/pgAdmin (binary release)
	pkgver = ${VERSION}
	pkgrel = 1
	url = https://github.com/QoreDB/QoreDB
	arch = x86_64
	license = Apache-2.0
	depends = cairo
	depends = dbus
	depends = gdk-pixbuf2
	depends = glib2
	depends = gtk3
	depends = hicolor-icon-theme
	depends = libsoup
	depends = openssl
	depends = pango
	depends = webkit2gtk
	optdepends = postgresql-libs: PostgreSQL connection support
	optdepends = libmysqlclient: MySQL connection support
	optdepends = sqlite: SQLite connection support
	optdepends = openssh: SSH tunnel support
	provides = qoredb
	conflicts = qoredb
	conflicts = qoredb-git
	options = !strip
	options = !debug
	noextract = QoreDB_${VERSION}_amd64.deb
	source = QoreDB_${VERSION}_amd64.deb::https://github.com/QoreDB/QoreDB/releases/download/v${VERSION}/QoreDB_${VERSION}_amd64.deb
	sha256sums = SKIP

pkgname = qoredb-bin
EOF

echo "Done. PKGBUILD and .SRCINFO updated to ${VERSION}."
