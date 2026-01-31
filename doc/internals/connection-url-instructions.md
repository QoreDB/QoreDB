# Connection URL Support - Implementation Instructions

## Goal
Allow users to connect by pasting a URL/DSN (e.g., Postgres, MySQL, MongoDB) while keeping the internal connection flow driver-agnostic and future-proof. The URL should be parsed into the existing normalized `ConnectionConfig`, then handled by the same validation and session pipeline as today.

## Principles
- Keep `ConnectionConfig` as the normalized internal format used by drivers.
- Parse URLs in a single backend module, not in each UI or driver.
- Do not store raw URLs with passwords; store normalized fields instead.
- Allow per-driver parsing while keeping the API generic for future drivers.

## Backend Design
1) **New input model**
   - Introduce `ConnectionInput` (or extend the commands layer) to accept:
     - `connection_url: Option<String>`
     - Optional explicit fields (host, port, username, database, ssl, etc.)
   - Convert `ConnectionInput -> ConnectionConfig` during normalization.

2) **Central URL parsing module**
   - Add `src-tauri/src/engine/connection_url.rs` (or similar).
   - Define a small trait:
     - `trait ConnectionUrlParser { fn parse(&self, url: &str) -> Result<PartialConfig, String>; }`
   - Register per-driver parsers in one place (match or map).
   - `PartialConfig` contains only URL-derived fields (host/port/user/pass/db/ssl/options).

3) **Driver-specific parsers**
   - Postgres: `postgres://` and `postgresql://`, support `sslmode`.
   - MySQL: `mysql://`.
   - MongoDB: `mongodb://` and `mongodb+srv://`, support `authSource`, `tls`, etc.
   - Apply default ports if missing.

4) **Priority rules**
   - Parse URL first, then merge explicit fields as overrides.
   - Continue using `normalize_config()` to validate and fill defaults.

5) **Vault and storage**
   - Do not persist the raw URL; persist normalized fields only.
   - If the UI includes the URL input, store it only in UI state.

## Frontend Design
- Add a toggle in the connection modal: "Use URL/DSN".
- When enabled, show a single URL input + "Parse" action.
- Populate the derived fields (host/port/user/db) as read-only or preview.
- Saving uses the normalized fields, not the raw URL.

## Auto-Refresh Connections
Add automatic refresh of connection-related UI data when it is likely to be stale:
- After creating a new connection.
- After editing an existing connection.
- After deleting a connection.
- After a successful test/connect attempt that changes stored metadata.

The refresh should update:
- The saved connections list.
- The connection details panel (if open).
- The active session view (if it references the edited connection).

## Tests
- Unit tests for each parser (valid URLs, missing parts, ssl/tls params).
- Test merge priority (URL + explicit overrides).
- Test that stored connections do not contain the raw URL.

## Notes
- Keep the parsing and normalization in the backend so future clients can reuse it.
- This approach lets new drivers add URL support by registering a parser without changing UI or core connection flow.
