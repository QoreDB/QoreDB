# Add or change a backend command

Trace an existing command with similar behavior before implementing. For a query,
use [commands/query.rs](../../src-tauri/src/commands/query.rs) and
[tauri/query.ts](../../src/lib/tauri/query.ts); for a smaller operation, select the
nearest handler in the same domain.

## Implementation path

1. Define the observable behavior, request/response shape, error behavior, and
   affected surfaces. Decide whether this is desktop-only or shared behavior.
2. Put shared behavior in [qore-service](../../src-tauri/crates/qore-service/src/)
   or the appropriate engine crate. Keep Tauri state/events in the desktop adapter.
   Reuse policy, preflight, vault, masking, and license checks; UI gating alone
   does not authorize an operation.
3. Add/update the handler under
   [src-tauri/src/commands](../../src-tauri/src/commands/). For new modules, export
   from `commands/mod.rs`; register new handlers in the `generate_handler!` list
   in [lib.rs](../../src-tauri/src/lib.rs). Follow nearby feature gates for Premium.
4. Add the typed wrapper and payload types under
   [src/lib/tauri](../../src/lib/tauri/), exposing new modules through
   [tauri.ts](../../src/lib/tauri.ts) where needed. Some Premium domains maintain
   separate binding modules such as `src/lib/ai.ts`; follow the existing domain.
5. Keep command names, argument casing, `Option`/null behavior, enum serialization,
   and response envelopes aligned. Types are not automatically generated from
   Rust. Stream changes also require the matching decoder and completion/error
   handling, not just an updated interface.
6. Inspect [transport.ts](../../src/lib/transport.ts) and relevant server routes
   for web behavior. Shared behavior may also need CLI/MCP adapters; make an
   explicit scope decision rather than implying those surfaces work automatically.
7. Connect the UI using existing components and translate all visible text.
   Apply the [licensing rules](LICENSING.md) to handlers, helpers, and tests.

## Verification

Test service behavior, denial/error paths, and serialized contracts where they
can drift. Run the relevant Rust package tests, TypeScript typecheck and affected
Vitest tests from [the testing guide](TESTING.md). Exercise the actual IPC flow in
Tauri for new commands: a successful Rust unit test does not prove registration
or argument casing. Check Core/Pro builds when feature gates change, and web or
headless behavior when those entry points are in scope.
