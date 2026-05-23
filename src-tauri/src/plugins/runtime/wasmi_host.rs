// SPDX-License-Identifier: Apache-2.0

//! `wasmi` implementation of the plugin runtime.
//!
//! `wasmi` is a pure-Rust WASM interpreter — small binary footprint, no JIT.
//! Each hook invocation runs in a fresh `Store` with its own fuel budget, so a
//! plugin cannot accumulate state or starve a later call. A fuel-exhausted or
//! trapping module fails that *invocation* only; the host carries on.
//!
//! ## ABI v1
//!
//! The guest module must export:
//! - `memory` — its linear memory;
//! - `qoredb_alloc(len: i32) -> i32` — reserve `len` bytes, return the offset;
//! - `pre_execute(ptr: i32, len: i32) -> i64` — read the JSON [`HookContext`]
//!   at `[ptr, len)` and return a packed `(out_ptr << 32 | out_len)` pointing
//!   at the JSON [`Decision`] it produced.

use wasmi::{Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};

use super::{Budget, Decision, HookContext, PluginError, PluginInstance, PluginRuntime};

/// A WASM page is 64 KiB — the unit `Budget::memory_pages` is denominated in.
const WASM_PAGE_BYTES: usize = 64 * 1024;

/// Allocator export the host calls to place input bytes into guest memory.
const ALLOC_EXPORT: &str = "qoredb_alloc";
/// `pre_execute` hook entry point.
const PRE_EXECUTE_EXPORT: &str = "pre_execute";

/// Per-invocation `Store` data: holds the resource limiter so a plugin
/// cannot grow its linear memory past the host-imposed ceiling.
struct StoreData {
    limits: StoreLimits,
}

/// The `wasmi`-backed plugin runtime.
pub struct WasmiRuntime;

impl WasmiRuntime {
    pub fn new() -> Self {
        Self
    }
}

impl Default for WasmiRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginRuntime for WasmiRuntime {
    fn load(&self, wasm: &[u8], budget: Budget) -> Result<Box<dyn PluginInstance>, PluginError> {
        let mut config = wasmi::Config::default();
        config.consume_fuel(true);
        let engine = Engine::new(&config);
        let module = Module::new(&engine, wasm).map_err(|e| PluginError::Load(e.to_string()))?;
        Ok(Box::new(WasmiInstance {
            engine,
            module,
            budget,
        }))
    }
}

/// A parsed module ready to run hooks. Each call gets a fresh `Store`.
struct WasmiInstance {
    engine: Engine,
    module: Module,
    budget: Budget,
}

impl PluginInstance for WasmiInstance {
    fn pre_execute(&mut self, context: &HookContext) -> Result<Decision, PluginError> {
        let input = serde_json::to_vec(context).map_err(|e| PluginError::Abi(e.to_string()))?;

        // `trap_on_grow_failure` makes a refused `memory.grow` trap (caught
        // below) instead of silently returning -1 to the plugin — clearer
        // signal that the budget was reached.
        let memory_size_bytes = (self.budget.memory_pages as usize)
            .saturating_mul(WASM_PAGE_BYTES);
        let limits = StoreLimitsBuilder::new()
            .memory_size(memory_size_bytes)
            .trap_on_grow_failure(true)
            .build();
        let mut store = Store::new(&self.engine, StoreData { limits });
        store.limiter(|data| &mut data.limits);
        store
            .set_fuel(self.budget.fuel)
            .map_err(|e| PluginError::Load(e.to_string()))?;

        // Phase 1 plugins are pure compute: no host functions are imported.
        let linker: Linker<StoreData> = Linker::new(&self.engine);
        let instance = linker
            .instantiate_and_start(&mut store, &self.module)
            .map_err(|e| PluginError::Load(e.to_string()))?;

        let memory = instance
            .get_memory(&store, "memory")
            .ok_or_else(|| PluginError::Abi("plugin exports no memory".into()))?;

        // A module that does not export `pre_execute` simply allows the query.
        let hook = match instance.get_typed_func::<(i32, i32), i64>(&store, PRE_EXECUTE_EXPORT) {
            Ok(func) => func,
            Err(_) => return Ok(Decision::Allow),
        };
        let alloc = instance
            .get_typed_func::<i32, i32>(&store, ALLOC_EXPORT)
            .map_err(|_| PluginError::Abi("plugin exports no allocator".into()))?;

        let len =
            i32::try_from(input.len()).map_err(|_| PluginError::Abi("hook input too large".into()))?;
        let ptr = alloc.call(&mut store, len).map_err(|e| map_call_error(&store, e))?;
        memory
            .write(&mut store, ptr as usize, &input)
            .map_err(|e| PluginError::Abi(e.to_string()))?;

        let packed = hook
            .call(&mut store, (ptr, len))
            .map_err(|e| map_call_error(&store, e))?;

        let (out_ptr, out_len) = unpack(packed);
        let mut out = vec![0u8; out_len];
        memory
            .read(&store, out_ptr, &mut out)
            .map_err(|e| PluginError::Abi(e.to_string()))?;

        serde_json::from_slice(&out).map_err(|e| PluginError::Abi(e.to_string()))
    }
}

/// Splits the `i64` a hook returns into a `(ptr, len)` pair.
fn unpack(packed: i64) -> (usize, usize) {
    let ptr = (packed >> 32) as u32;
    let len = (packed & 0xFFFF_FFFF) as u32;
    (ptr as usize, len as usize)
}

/// Classifies a failed `wasmi` call: an exhausted fuel budget becomes
/// `BudgetExceeded`, anything else a `Trap`.
fn map_call_error(store: &Store<StoreData>, error: wasmi::Error) -> PluginError {
    if store.get_fuel().map(|fuel| fuel == 0).unwrap_or(false) {
        PluginError::BudgetExceeded
    } else {
        PluginError::Trap(error.to_string())
    }
}
