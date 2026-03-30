#!/usr/bin/env bash
# SPDX-License-Identifier: Apache-2.0
# Generate CycloneDX SBOMs for QoreDB (Rust backend + Frontend)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

echo "=== QoreDB SBOM Generation ==="
echo ""

echo "[1/2] Generating Rust SBOM (CycloneDX)..."
cd "$PROJECT_ROOT/src-tauri"
cargo cyclonedx --format json --output-file "$PROJECT_ROOT/sbom-rust.cdx.json"
echo "  -> sbom-rust.cdx.json"

echo "[2/2] Generating Frontend SBOM (CycloneDX)..."
cd "$PROJECT_ROOT"
pnpm dlx @cyclonedx/cdxgen -o sbom-frontend.cdx.json --project-name qoredb
echo "  -> sbom-frontend.cdx.json"

echo ""
echo "=== SBOMs generated ==="
ls -lh "$PROJECT_ROOT"/sbom-*.cdx.json
