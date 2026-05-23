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
//! - `post_execute(ptr: i32, len: i32)` *(optional)* — receives a JSON
//!   `{context, result}` envelope; returns nothing.
//!
//! Phase 2 capability host functions (see [`host_fns`]) are always registered;
//! whether they do anything depends on the per-invocation consent set.

use std::sync::Arc;

use wasmi::{Engine, Linker, Module, Store, StoreLimits, StoreLimitsBuilder};

use super::{
    host_fns, Budget, Decision, HookContext, InvocationServices, PluginError, PluginInstance,
    PluginRuntime, PostExecuteResult, QueryReadPayload,
};

/// A WASM page is 64 KiB — the unit `Budget::memory_pages` is denominated in.
const WASM_PAGE_BYTES: usize = 64 * 1024;

/// Allocator export the host calls to place input bytes into guest memory.
const ALLOC_EXPORT: &str = "qoredb_alloc";
const PRE_EXECUTE_EXPORT: &str = "pre_execute";
const POST_EXECUTE_EXPORT: &str = "post_execute";

/// The data the `Store` carries: the resource limiter (for `memory.grow`
/// caps) and the host-services bundle host functions read from.
pub(crate) struct StoreData {
    pub(crate) limits: StoreLimits,
    pub(crate) services: InvocationServices,
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
    fn load(
        &self,
        plugin_id: String,
        wasm: &[u8],
        budget: Budget,
        services: InvocationServices,
    ) -> Result<Box<dyn PluginInstance>, PluginError> {
        let mut config = wasmi::Config::default();
        config.consume_fuel(true);
        let engine = Engine::new(&config);
        let module = Module::new(&engine, wasm).map_err(|e| PluginError::Load(e.to_string()))?;
        Ok(Box::new(WasmiInstance {
            engine,
            module,
            budget,
            services,
            _plugin_id: plugin_id,
        }))
    }
}

/// A parsed module ready to run hooks. Each call gets a fresh `Store`.
struct WasmiInstance {
    engine: Engine,
    module: Module,
    budget: Budget,
    /// Snapshotted at build time: consent, storage handle, notify sender,
    /// plugin id. Cloned into the per-invocation `Store`.
    services: InvocationServices,
    _plugin_id: String,
}

impl PluginInstance for WasmiInstance {
    fn pre_execute(&mut self, context: &HookContext) -> Result<Decision, PluginError> {
        let input = serde_json::to_vec(context).map_err(|e| PluginError::Abi(e.to_string()))?;
        let (mut store, instance) = build_store_and_instance(self, None)?;

        let hook = match instance.get_typed_func::<(i32, i32), i64>(&store, PRE_EXECUTE_EXPORT) {
            Ok(func) => func,
            Err(_) => return Ok(Decision::Allow),
        };
        let (ptr, len) = write_input(&mut store, &instance, &input)?;
        let packed = hook
            .call(&mut store, (ptr, len))
            .map_err(|e| map_call_error(&store, e))?;

        let memory = instance
            .get_memory(&store, "memory")
            .ok_or_else(|| PluginError::Abi("plugin exports no memory".into()))?;
        let (out_ptr, out_len) = unpack(packed);
        let mut out = vec![0u8; out_len];
        memory
            .read(&store, out_ptr, &mut out)
            .map_err(|e| PluginError::Abi(e.to_string()))?;
        serde_json::from_slice(&out).map_err(|e| PluginError::Abi(e.to_string()))
    }

    fn post_execute(
        &mut self,
        context: &HookContext,
        result: &PostExecuteResult,
        payload: Option<Arc<QueryReadPayload>>,
    ) -> Result<(), PluginError> {
        let envelope = serde_json::json!({ "context": context, "result": result });
        let input = serde_json::to_vec(&envelope).map_err(|e| PluginError::Abi(e.to_string()))?;
        let (mut store, instance) = build_store_and_instance(self, payload)?;

        let hook = match instance.get_typed_func::<(i32, i32), ()>(&store, POST_EXECUTE_EXPORT) {
            Ok(func) => func,
            Err(_) => return Ok(()),
        };
        let (ptr, len) = write_input(&mut store, &instance, &input)?;
        hook.call(&mut store, (ptr, len))
            .map_err(|e| map_call_error(&store, e))?;
        Ok(())
    }
}

/// Spins up a fresh `Store` + `Linker` and instantiates the module against
/// them. `query_result` is handed to the services so the `queryRead` host
/// function can return it; pass `None` for `preExecute`.
fn build_store_and_instance(
    inst: &WasmiInstance,
    query_result: Option<Arc<QueryReadPayload>>,
) -> Result<(Store<StoreData>, wasmi::Instance), PluginError> {
    let memory_size_bytes = (inst.budget.memory_pages as usize).saturating_mul(WASM_PAGE_BYTES);
    let limits = StoreLimitsBuilder::new()
        .memory_size(memory_size_bytes)
        .trap_on_grow_failure(true)
        .build();

    let mut services = inst.services.clone();
    services.query_result = query_result;

    let mut store = Store::new(&inst.engine, StoreData { limits, services });
    store.limiter(|data| &mut data.limits);
    store
        .set_fuel(inst.budget.fuel)
        .map_err(|e| PluginError::Load(e.to_string()))?;

    let mut linker: Linker<StoreData> = Linker::new(&inst.engine);
    host_fns::register(&mut linker).map_err(|e| PluginError::Load(e.to_string()))?;
    let instance = linker
        .instantiate_and_start(&mut store, &inst.module)
        .map_err(|e| PluginError::Load(e.to_string()))?;
    Ok((store, instance))
}

/// Writes `input` into the guest, returning `(ptr, len)`.
fn write_input(
    store: &mut Store<StoreData>,
    instance: &wasmi::Instance,
    input: &[u8],
) -> Result<(i32, i32), PluginError> {
    let alloc = instance
        .get_typed_func::<i32, i32>(&*store, ALLOC_EXPORT)
        .map_err(|_| PluginError::Abi("plugin exports no allocator".into()))?;
    let len =
        i32::try_from(input.len()).map_err(|_| PluginError::Abi("hook input too large".into()))?;
    let ptr = alloc
        .call(&mut *store, len)
        .map_err(|e| map_call_error(&*store, e))?;
    let memory = instance
        .get_memory(&*store, "memory")
        .ok_or_else(|| PluginError::Abi("plugin exports no memory".into()))?;
    memory
        .write(&mut *store, ptr as usize, input)
        .map_err(|e| PluginError::Abi(e.to_string()))?;
    Ok((ptr, len))
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
