---
name: feature-creator
description: Standardized workflow for adding a new "Vertical Slice" feature to QoreDB, from Rust backend to React frontend. Use this when the user asks to "add a feature", "create a new command", or "connect backend to frontend".
---

# Feature Creator

This skill guides the creation of a complete feature in QoreDB, ensuring type safety and architectural consistency across the Rust/Tauri/React stack.

## Workflow

### 1. Backend (Rust)

1.  **Create Command File**:
    Create a new file in `src-tauri/src/commands/<feature>.rs` using the template below.
    *   Define a `[Feature]Response` struct deriving `Serialize`.
    *   Implement the function with `#[tauri::command]`.
    *   Use `crate::SharedState` if state access involves locking (e.g. `state.lock().await`).

2.  **Register Module**:
    In `src-tauri/src/lib.rs`:
    *   Add `pub mod <feature>;` in the `commands` module block (or `src-tauri/src/commands/mod.rs` if applicable, but QoreDB uses `lib.rs` for registration).
    *   Add the command to the `tauri::generate_handler!` macro list.

### 2. Frontend Interface (TypeScript)

1.  **Update `src/lib/tauri.ts`**:
    This file acts as the central SDK for the backend.
    *   Define the Types (Interfaces) for Arguments and Responses.
    *   Export a typed `async function` that calls `invoke('command_name', { args })`.
    *   **Rule**: Never call `invoke` directly in components. Always go through `src/lib/tauri.ts`.

### 3. Frontend Logic (React)

1.  **Create Hook (Optional but Recommended)**:
    If the feature involves loading states or complex side effects, create `src/hooks/use<Feature>.ts`.
    *   Use the `assets/hook.ts` pattern.

2.  **Implement UI**:
    *   Import the function from `@lib/tauri` (or the hook).
    *   Handle `loading` and `error` states explicitly.

## Templates

### Rust Command (`src-tauri/src/commands/`)

```rust
use serde::Serialize;
use crate::SharedState;

#[derive(Debug, Serialize)]
pub struct FeatureResponse {
    pub success: bool,
    pub data: Option<String>, // Replace with specific type
    pub error: Option<String>,
}

#[tauri::command]
pub async fn feature_command(
    state: tauri::State<'_, SharedState>,
    id: String,
) -> Result<FeatureResponse, String> {
    // Access state:
    // let state = state.lock().await;
    
    Ok(FeatureResponse {
        success: true,
        data: Some("Success".to_string()),
        error: None,
    })
}
```

### TypeScript SDK (`src/lib/tauri.ts`)

```typescript
// TYPES
export interface FeatureResponse {
  success: boolean;
  data?: string;
  error?: string;
}

// COMMANDS
export async function featureCommand(id: string): Promise<FeatureResponse> {
  return invoke('feature_command', { id });
}
```

## Checklist

- [ ] Rust: Command created and public
- [ ] Rust: Command registered in `generate_handler!`
- [ ] TS: Interface defined in `src/lib/tauri.ts`
- [ ] TS: Wrapper function exported in `src/lib/tauri.ts`
- [ ] UI: Error handling implemented (Try/Catch or State)
