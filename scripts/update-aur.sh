#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Updates PKGBUILD and .SRCINFO with a new version for AUR publishing.
# Usage: ./scripts/update-aur.sh <version> [local-deb]
# Example: ./scripts/update-aur.sh 0.1.23

set -euo pipefail

VERSION="${1:?Usage: $0 <version> [local-deb]}"
if [[ ! "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "Expected a stable numeric version, got: $VERSION" >&2
    exit 1
fi
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
AUR_DIR="${SCRIPT_DIR}/../aur/qoredb-bin"

if [ -n "${2:-}" ]; then
    DEB_PATH="$2"
else
    DEB_PATH="$(mktemp)"
    trap 'rm -f "$DEB_PATH"' EXIT
    curl --fail --location --retry 3 \
        "https://github.com/QoreDB/QoreDB/releases/download/v${VERSION}/QoreDB_${VERSION}_amd64.deb" \
        --output "$DEB_PATH"
fi
DEB_SHA256="$(sha256sum "$DEB_PATH" | cut -d ' ' -f 1)"

echo "Updating AUR package to version ${VERSION}..."

# Update PKGBUILD
sed -i "s/^pkgver=.*/pkgver=${VERSION}/" "${AUR_DIR}/PKGBUILD"
sed -i "s/^pkgrel=.*/pkgrel=1/" "${AUR_DIR}/PKGBUILD"
sed -i "s/^sha256sums=.*/sha256sums=('${DEB_SHA256}')/" "${AUR_DIR}/PKGBUILD"

# Regenerate .SRCINFO from PKGBUILD
cat > "${AUR_DIR}/.SRCINFO" <<EOF
pkgbase = qoredb-bin
	pkgdesc = Next gen database client — lightweight alternative to DBeaver/pgAdmin (binary release)
	pkgver = ${VERSION}
	pkgrel = 1
	url = https://github.com/QoreDB/QoreDB
	arch = x86_64
	license = Apache-2.0
	license = BUSL-1.1
	depends = cairo
	depends = dbus
	depends = gdk-pixbuf2
	depends = glib2
	depends = gtk3
	depends = hicolor-icon-theme
	depends = libsoup3
	depends = openssl
	depends = pango
	depends = webkit2gtk-4.1
	optdepends = postgresql-libs: PostgreSQL connection support
	optdepends = libmysqlclient: MySQL connection support
	optdepends = sqlite: SQLite connection support
	optdepends = openssh: SSH tunnel support
	provides = qoredb
	conflicts = qoredb
	conflicts = qoredb-git
	noextract = QoreDB_${VERSION}_amd64.deb
	options = !strip
	options = !debug
	source = QoreDB_${VERSION}_amd64.deb::https://github.com/QoreDB/QoreDB/releases/download/v${VERSION}/QoreDB_${VERSION}_amd64.deb
	sha256sums = ${DEB_SHA256}

pkgname = qoredb-bin
EOF

echo "Done. PKGBUILD and .SRCINFO updated to ${VERSION}."
