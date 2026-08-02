# Patched SQLx MySQL driver

This directory vendors `sqlx-mysql` 0.8.6 under its original MIT OR Apache-2.0
license.

QoreDB carries one behavioral patch in `src/connection/auth.rs`: an empty
password produces an empty authentication response after a MySQL authentication
plugin switch. Upstream SQLx otherwise hashes the empty string, causing MySQL to
report `using password: YES` for passwordless accounts.

Remove the `[patch.crates-io]` override when this behavior is fixed in the SQLx
release used by QoreDB.
