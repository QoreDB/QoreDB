#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Updates PKGBUILD and .SRCINFO with a new version for AUR publishing.
# Usage: ./scripts/update-aur.sh <version>
# Example: ./scripts/update-aur.sh 0.1.22

set -euo pipefail

VERSION="${1:?Usage: $0 <version>}"
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
AUR_DIR="${SCRIPT_DIR}/../aur"

echo "Updating AUR package to version ${VERSION}..."

# Update PKGBUILD
sed -i "s/^pkgver=.*/pkgver=${VERSION}/" "${AUR_DIR}/PKGBUILD"
sed -i "s/^pkgrel=.*/pkgrel=1/" "${AUR_DIR}/PKGBUILD"

# Regenerate .SRCINFO from PKGBUILD
cat > "${AUR_DIR}/.SRCINFO" <<EOF
pkgbase = qoredb-bin
	pkgdesc = Modern, lightweight database client for developers (PostgreSQL, MySQL, MongoDB, SQLite)
	pkgver = ${VERSION}
	pkgrel = 1
	url = https://github.com/QoreDB/QoreDB
	arch = x86_64
	license = Apache-2.0
	depends = webkit2gtk-4.1
	depends = gtk3
	depends = libappindicator-gtk3
	depends = librsvg
	depends = openssl
	optdepends = docker: for running test databases locally
	provides = qoredb
	conflicts = qoredb
	options = !strip
	options = !debug
	noextract = QoreDB-${VERSION}.deb
	source = QoreDB-${VERSION}.deb::https://github.com/QoreDB/QoreDB/releases/download/v${VERSION}/qore-db_${VERSION}_amd64.deb
	sha256sums = SKIP

pkgname = qoredb-bin
EOF

echo "Done. PKGBUILD and .SRCINFO updated to ${VERSION}."
