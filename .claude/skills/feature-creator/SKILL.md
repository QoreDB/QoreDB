---
name: feature-creator
description: Standardized workflow for adding a new "vertical slice" feature to QoreDB, from Rust backend to React frontend. Use this when the user asks to "add a feature", "create a new command", or "connect backend to frontend".
---

# Feature Creator

Guides the creation of a complete feature in QoreDB, keeping type safety and
architectural consistency across the Rust/Tauri/React stack.

## Workflow

### 1. Backend (Rust)

1. **Create the command file**:
   New file in `src-tauri/src/commands/<feature>.rs` using the template below.
   - Start it with the SPDX header (`// SPDX-License-Identifier: Apache-2.0`,
     or `BUSL-1.1` for a Premium feature).
   - Define a `[Feature]Response` struct deriving `Serialize`.
   - Implement the function with `#[tauri::command]`.
   - Use `crate::SharedState` when state access needs locking
     (`state.lock().await`).

2. **Register the command**:
   In `src-tauri/src/lib.rs`:
   - Add `pub mod <feature>;` in the `commands` module block.
   - Add the command to the `tauri::generate_handler!` macro list.

### 2. Frontend interface (TypeScript)

`src/lib/tauri.ts` is a barrel that re-exports the per-domain submodules in
`src/lib/tauri/` (`query.ts`, `schema-browse.ts`, `mutations.ts`, …).

1. **Add types + wrapper in the matching submodule** `src/lib/tauri/<domain>.ts`
   (create a new submodule and add it to the barrel only if no domain fits):
   - SPDX header at the top of the file.
   - Define the argument/response interfaces.
   - Export a typed `async function` that calls `invoke('command_name', { args })`.
     Import `invoke` from `@/lib/transport` (not `@tauri-apps/api/core`
     directly — the transport layer also handles the web build).
   - **Rule**: never call `invoke` directly in components. Always go through the
     `src/lib/tauri` SDK.

### 3. Frontend logic (React)

1. **Create a hook (optional but recommended)**:
   If the feature involves loading states or side effects, create
   `src/hooks/use<Feature>.ts` using the `assets/hook.ts` pattern. The hook
   calls the typed wrapper from `@/lib/tauri`, never `invoke` directly.

2. **Build the UI**:
   - Import the wrapper (or hook) from `@/lib/tauri`.
   - Handle `loading` and `error` explicitly.

## Templates

### Rust command (`src-tauri/src/commands/`)

```rust
// SPDX-License-Identifier: Apache-2.0
use serde::Serialize;
use crate::SharedState;

#[derive(Debug, Serialize)]
pub struct FeatureResponse {
    pub success: bool,
    pub data: Option<String>, // Replace with a specific type
    pub error: Option<String>,
}

#[tauri::command]
pub async fn feature_command(
    state: tauri::State<'_, SharedState>,
    id: String,
) -> Result<FeatureResponse, String> {
    // let state = state.lock().await;

    Ok(FeatureResponse {
        success: true,
        data: Some("Success".to_string()),
        error: None,
    })
}
```

### TypeScript wrapper (`src/lib/tauri/<domain>.ts`)

```typescript
// SPDX-License-Identifier: Apache-2.0
import { invoke } from '@/lib/transport';

export interface FeatureResponse {
  success: boolean;
  data?: string;
  error?: string;
}

export async function featureCommand(id: string): Promise<FeatureResponse> {
  return invoke('feature_command', { id });
}
```

## Checklist

- [ ] Rust: SPDX header + command created and public
- [ ] Rust: command registered in `generate_handler!`
- [ ] TS: interface + wrapper exported from `src/lib/tauri/<domain>.ts`
- [ ] TS: `invoke` imported from `@/lib/transport`, not called from a component
- [ ] UI: error handling implemented
