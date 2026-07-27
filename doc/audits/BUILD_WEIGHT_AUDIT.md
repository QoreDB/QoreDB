# Build weight audit

> **Status:** Reviewed, targeted optimization applied
> **Original date:** 2026-07-26
> **Last reviewed:** 2026-07-26
> **Code baseline:** `bc16a9337094e6a9469afa0ad22984aac01dbacc`
> **Review depth:** Targeted build and dependency-weight review
> **Scope:** Rust development builds, DuckDB linkage, distributed binaries, and frontend bundle weight

## Outcome

DuckDB remains a supported driver and remains available to federation and native
DuckDB backup code. The linkage strategy now differs by use case:

- Development and CI link the matching official prebuilt shared DuckDB library.
- Distributed desktop, server, Docker, MSIX, and AUR builds enable the
  `duckdb-bundled` feature and keep the previous self-contained static linkage.

This removes DuckDB's C++ compilation from the normal development loop without
changing the runtime model of distributed artifacts.

## Baseline observations

The existing local `src-tauri/target` directory was already 66 GiB at the start
of the review. It contained five separate `libduckdb-sys` build directories of
about 3.3 GiB each. Each directory contained a static `libduckdb.a` of about
1.7 GB. These figures describe accumulated Cargo fingerprints, not the size of
a clean build.

The downloaded DuckDB cache is about 186 MiB. It is stored below
`src-tauri/target/duckdb-download`, reused by subsequent builds, and contains the
official archive, headers, and shared library. Existing multi-gigabyte bundled
artifacts are not removed automatically because cleanup would be destructive to
the developer's build cache.

The Rust dependency graph remains substantial even without C++ compilation.
Before the dependency alignment, the `duckdb` crate pulled Arrow 58 while the
Pro export path directly used Arrow and Parquet 54.

## Applied changes

### DuckDB linkage

The workspace no longer enables `duckdb/bundled` unconditionally. A
`duckdb-bundled` feature propagates through `qore-drivers`, `qore-service`, and
the binary crates.

The repository Cargo configuration sets `DUCKDB_DOWNLOAD_LIB=1` with
`force=false`. Developers can still provide `DUCKDB_LIB_DIR`, or override the
download setting when using an installed system library.

Every current distribution entry point explicitly enables
`duckdb-bundled`:

- the Tauri release workflow;
- the standalone MSIX workflow;
- `pnpm tauri build`;
- the Docker server build;
- the AUR package build.

### Development artifacts

The Tauri library now emits only an `rlib`. The removed `staticlib` and
`cdylib` outputs were unused by the current desktop targets and multiplied
development linking and disk use. They must be restored if Android or iOS
projects are introduced.

Development debug information is reduced from level 2 to level 1. Line-level
debugging and backtraces remain available, while full local-variable debug
information is no longer emitted for every crate.

Rust package scripts execute Cargo from `src-tauri`, so the existing local Cargo
configuration is consistently applied. The repository-level DuckDB setting also
covers direct commands using `--manifest-path` from the repository root.

### Arrow and Parquet alignment

The Pro export path now uses the same Arrow and Parquet 58.3.0 family as
DuckDB. Arrow's default features are disabled because the export writer only
uses its array types; this also removes the unused Arrow CSV and JSON crates.
Parquet keeps Arrow support and explicitly enables Snappy, matching the
writer's existing compression setting.

The lockfile contains no Arrow or Parquet 54 package after the alignment.
Fifteen packages from that dependency family were removed. With the explicit
Snappy dependency added, the complete lockfile decreased from 928 to 914
packages.

The only required source adaptation uses Parquet 58's metadata accessors. A
round-trip test now writes a Snappy-compressed file and reads it back, covering
booleans, integers, floats, binary data, text, and null values.

### Driver features

The driver crate and service now expose one feature per advertised driver.
Desktop dependencies explicitly enable `all-drivers`, and CLI, MCP, and server
builds retain `all-drivers` as their default. Existing development and
distribution commands therefore keep the same 16 registered drivers.

Headless binaries can use `--no-default-features` with only the driver features
they need. On the current Linux graph, a driver-free `qore-service` build drops
from 538 to 193 unique dependency nodes. A SQLite-only CLI build uses 255 nodes
instead of 550 in the previous unconditional graph.

Database client dependencies are optional behind their driver features.
SQLx no longer enables its broad default feature set; the runtime, TLS, JSON,
numeric, UUID, and selected database backends are declared explicitly. The
desktop still enables PostgreSQL, MySQL, and SQLite backends.

### Archive dependencies

QoreDB's direct ZIP dependency now uses version 6, matching DuckDB's build
dependency. ZIP 2 is absent from the graph and lockfile, which decreases the
lockfile from 914 to 913 packages. ZIP 7 remains required by `rust_xlsxwriter`;
the updater also requires ZIP 4 on supported target platforms.

Both QoreDB ZIP extraction paths have round-trip tests against ZIP 6.

## Validation

The following checks passed on Linux x86-64:

- all 216 `qore-drivers` tests;
- all 107 `qore-service` tests;
- all 15 DuckDB driver tests;
- all 15 federation tests;
- all 4 native DuckDB backup tests;
- all 218 root library unit tests and the application-state integration test;
- all 128 TypeScript tests;
- a dynamic-link development application launch;
- a bundled-feature `qore-drivers` compile check;
- Cargo metadata and feature-tree checks for both linkage modes;
- a single-version Arrow/Parquet 58.3.0 Pro dependency graph;
- the Snappy-compressed Parquet export round-trip test;
- no-driver and each of the 16 individual driver-feature service builds;
- default all-driver and SQLite-only CLI, MCP, and server builds;
- Core and Pro desktop compile checks with all drivers;
- ZIP extraction tests for plugin and local AI runtime archives;
- 482 of 484 Pro root library tests, including all export, federation, and
  native DuckDB backup tests;
- the TypeScript and Vite production build;
- syntax parsing of the modified GitHub Actions workflows;
- Biome checks for the Tauri wrapper and `package.json`;
- `git diff --check`.

The remaining two Pro root library tests fail reproducibly in unrelated AI
provider and keychain state assertions. Neither test exercises Arrow, Parquet,
DuckDB, federation, backup, or export code.

The distributed build paths retain the same `duckdb/bundled` feature that they
used before this change. Cross-platform packaging must remain part of the
release matrix because the shared development path and the static distribution
path intentionally differ.

## Deferred findings

### Duplicate HTTP stacks

The desktop graph contains reqwest 0.12 and 0.13. QoreDB's HTTP clients and
database drivers use 0.12, DuckDB also requires 0.12 as a build dependency, and
the Tauri updater requires 0.13. Moving all runtime clients to 0.13 would either
change the updater's TLS backend through Cargo feature unification or add the
AWS-LC provider to every desktop build. The duplicate remains until these
upstream constraints converge.

### Frontend initial bundle

Non-English locales, Recharts, and `sql-formatter` are now loaded on demand.
The main production JavaScript chunk decreased from 3.90 MB to 2.20 MB
uncompressed, and from 1.15 MB to 636 KB gzip. The total installed bundle stays
roughly constant because all chunks remain packaged for offline use; the gain is
in startup loading and parsing rather than feature removal.

### DuckDB remains a large Rust dependency

Prebuilt linkage removes C++ compilation but does not remove the `duckdb` Rust
crate or its Arrow dependency graph. Removing that graph from the default
desktop binary while retaining DuckDB support would require an optional
sidecar/plugin architecture. That is a product and packaging change rather than
a safe dependency tweak.

### Prebuilt development archive integrity

`libduckdb-sys` downloads the version-matched archive from DuckDB's GitHub
release over HTTPS, but its download path does not verify a repository-pinned
checksum. Distributed binaries do not use this path. Adding per-platform hashes
would strengthen CI reproducibility if the prebuilt path is retained long term.

## Cleanup

No existing project build cache was deleted during this review. The isolated
temporary targets used to validate the dependency and driver-feature
migrations were removed after the tests. Once no other development process
uses the main target directory, old bundled DuckDB fingerprints can be removed
with an explicit, scoped cleanup or a full `cargo clean`. That cleanup is
optional and recoverable only through recompilation.
