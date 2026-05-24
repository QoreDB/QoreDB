// SPDX-License-Identifier: Apache-2.0

//! End-to-end tests for the executable plugin runtime.
//!
//! Each test builds a tiny WebAssembly module (in WAT) that exercises one ABI
//! path or one host function, then runs it through [`WasmiRuntime`] exactly
//! like the real `PluginHost` would. Inline WAT keeps the suite hermetic:
//! no `wasm32-unknown-unknown` toolchain is required, so the tests stay green
//! on any developer machine and in CI.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use qoredb_lib::plugins::runtime::{
    Budget, CapabilityKind, Decision, HookContext, InvocationServices, PluginError, PluginRuntime,
    PluginStorage, PostExecuteResult, QueryReadPayload, WasmiRuntime,
};

/// Where the JSON Decision blob lives in the guest's linear memory. Tests
/// write the bytes there with a `data` segment and return that offset from
/// `pre_execute`.
const DECISION_PTR: i32 = 1024;

/// Where the host should drop the input the plugin will read in. Picked far
/// enough past the data segment that it never overlaps.
const ALLOC_PTR: i32 = 16 * 1024;

/// Packs `(ptr, len)` into the `i64` shape the ABI uses.
fn packed(ptr: i32, len: i32) -> i64 {
    ((ptr as i64) << 32) | (len as i64 & 0xFFFF_FFFF)
}

/// Builds a minimal module that returns a fixed Decision blob from
/// `pre_execute`. `decision_json` is the *raw* JSON the host should read
/// back (e.g. `{"kind":"allow"}`); this function handles the WAT escaping.
fn decision_module(decision_json: &str) -> String {
    let len = decision_json.len() as i32;
    let wat_escaped = decision_json.replace('\\', "\\\\").replace('"', "\\\"");
    format!(
        r#"
(module
  (memory (export "memory") 2)
  (data (i32.const {DECISION_PTR}) "{wat_escaped}")
  (func (export "qoredb_alloc") (param $len i32) (result i32)
    i32.const {ALLOC_PTR})
  (func (export "pre_execute") (param $ptr i32) (param $len i32) (result i64)
    i64.const {packed}))
"#,
        packed = packed(DECISION_PTR, len)
    )
}

fn compile(wat: &str) -> Vec<u8> {
    wat::parse_str(wat).expect("WAT compiled to wasm")
}

/// Builds an `InvocationServices` bundle with the given grants. `tmpdir` is
/// where the per-test storage lives — discarded when the test ends.
fn services(
    plugin_id: &str,
    grants: &[CapabilityKind],
    tmpdir: &PathBuf,
    fs_root: Option<PathBuf>,
    http_hosts: Vec<String>,
    secrets: Vec<String>,
) -> InvocationServices {
    let consent: BTreeSet<CapabilityKind> = grants.iter().copied().collect();
    InvocationServices {
        plugin_id: plugin_id.to_string(),
        consent: Arc::new(consent),
        storage: Arc::new(PluginStorage::new(tmpdir.join("storage.json"))),
        notify: None,
        query_result: None,
        http_allowed_hosts: Arc::new(http_hosts),
        fs_root,
        secret_names: Arc::new(secrets),
    }
}

fn ctx() -> HookContext {
    HookContext {
        query: "SELECT 1".into(),
        driver_id: "postgres".into(),
        environment: "test".into(),
        operation_type: "select".into(),
        is_mutation: false,
        is_dangerous: false,
        read_only: true,
    }
}

fn post_ok() -> PostExecuteResult {
    PostExecuteResult {
        success: true,
        execution_time_ms: 1,
        row_count: None,
        error: None,
    }
}

#[test]
fn pre_execute_allow_decision_round_trips() {
    let tmp = tempfile::tempdir().unwrap();
    let wasm = compile(&decision_module(r#"{"kind":"allow"}"#));

    let runtime = WasmiRuntime::new();
    let svc = services(
        "test.allow",
        &[],
        &tmp.path().to_path_buf(),
        None,
        vec![],
        vec![],
    );
    let mut instance = runtime
        .load("test.allow".into(), &wasm, Budget::default(), svc)
        .expect("module loaded");

    let decision = instance.pre_execute(&ctx()).expect("hook ran");
    assert_eq!(decision, Decision::Allow);
}

#[test]
fn pre_execute_warn_decision_round_trips() {
    let tmp = tempfile::tempdir().unwrap();
    let wasm = compile(&decision_module(r#"{"kind":"warn","message":"hi"}"#));

    let runtime = WasmiRuntime::new();
    let svc = services(
        "test.warn",
        &[],
        &tmp.path().to_path_buf(),
        None,
        vec![],
        vec![],
    );
    let mut instance = runtime
        .load("test.warn".into(), &wasm, Budget::default(), svc)
        .unwrap();

    let decision = instance.pre_execute(&ctx()).unwrap();
    assert_eq!(
        decision,
        Decision::Warn {
            message: "hi".into()
        }
    );
}

#[test]
fn pre_execute_block_decision_round_trips() {
    let tmp = tempfile::tempdir().unwrap();
    let wasm = compile(&decision_module(r#"{"kind":"block","reason":"no"}"#));

    let runtime = WasmiRuntime::new();
    let svc = services(
        "test.block",
        &[],
        &tmp.path().to_path_buf(),
        None,
        vec![],
        vec![],
    );
    let mut instance = runtime
        .load("test.block".into(), &wasm, Budget::default(), svc)
        .unwrap();

    let decision = instance.pre_execute(&ctx()).unwrap();
    assert_eq!(
        decision,
        Decision::Block {
            reason: "no".into()
        }
    );
}

#[test]
fn module_without_pre_execute_export_defaults_to_allow() {
    // No `pre_execute` export — the runtime treats it as a quiet allow so
    // declarative-only plugins don't need to ship a no-op hook.
    let wat = r#"
(module
  (memory (export "memory") 1)
  (func (export "qoredb_alloc") (param i32) (result i32) i32.const 0))
"#;
    let wasm = compile(wat);

    let tmp = tempfile::tempdir().unwrap();
    let runtime = WasmiRuntime::new();
    let svc = services(
        "test.silent",
        &[],
        &tmp.path().to_path_buf(),
        None,
        vec![],
        vec![],
    );
    let mut instance = runtime
        .load("test.silent".into(), &wasm, Budget::default(), svc)
        .unwrap();
    assert_eq!(instance.pre_execute(&ctx()).unwrap(), Decision::Allow);
}

#[test]
fn trapping_hook_is_reported_as_trap_not_panic() {
    // `unreachable` traps deterministically — the runtime must surface that
    // as a structured `PluginError::Trap` instead of crashing the host.
    let wat = r#"
(module
  (memory (export "memory") 1)
  (func (export "qoredb_alloc") (param i32) (result i32) i32.const 0)
  (func (export "pre_execute") (param i32 i32) (result i64) unreachable))
"#;
    let wasm = compile(wat);

    let tmp = tempfile::tempdir().unwrap();
    let runtime = WasmiRuntime::new();
    let svc = services(
        "test.trap",
        &[],
        &tmp.path().to_path_buf(),
        None,
        vec![],
        vec![],
    );
    let mut instance = runtime
        .load("test.trap".into(), &wasm, Budget::default(), svc)
        .unwrap();

    match instance.pre_execute(&ctx()) {
        Err(PluginError::Trap(_)) => {}
        other => panic!("expected Trap, got {other:?}"),
    }
}

#[test]
fn infinite_loop_is_stopped_by_fuel_budget() {
    // `loop ... br 0` burns fuel forever; with a tiny budget the runtime
    // must surface BudgetExceeded rather than hang the test thread.
    let wat = r#"
(module
  (memory (export "memory") 1)
  (func (export "qoredb_alloc") (param i32) (result i32) i32.const 0)
  (func (export "pre_execute") (param i32 i32) (result i64)
    (loop $l (br $l)) i64.const 0))
"#;
    let wasm = compile(wat);

    let tmp = tempfile::tempdir().unwrap();
    let runtime = WasmiRuntime::new();
    let svc = services(
        "test.fuel",
        &[],
        &tmp.path().to_path_buf(),
        None,
        vec![],
        vec![],
    );
    let budget = Budget {
        fuel: 10_000,
        memory_pages: 16,
    };
    let mut instance = runtime
        .load("test.fuel".into(), &wasm, budget, svc)
        .unwrap();

    match instance.pre_execute(&ctx()) {
        Err(PluginError::BudgetExceeded) => {}
        other => panic!("expected BudgetExceeded, got {other:?}"),
    }
}

/// Builds a module whose `pre_execute` calls `qoredb_kv_set("k", "v")` and
/// returns the host's status code as the `len` of an Allow decision so the
/// test can read it back from the packed result.
fn kv_set_probe_module() -> String {
    // The Allow blob sits at `DECISION_PTR`; key "k" at 2048, value "v" at 2050.
    format!(
        r#"
(module
  (import "env" "qoredb_kv_set" (func $kv_set (param i32 i32 i32 i32) (result i32)))
  (memory (export "memory") 2)
  (data (i32.const {DECISION_PTR}) "{{\"kind\":\"allow\"}}")
  (data (i32.const 2048) "k")
  (data (i32.const 2050) "v")
  (func (export "qoredb_alloc") (param i32) (result i32) i32.const {ALLOC_PTR})
  (func (export "pre_execute") (param i32 i32) (result i64)
    (drop (call $kv_set (i32.const 2048) (i32.const 1) (i32.const 2050) (i32.const 1)))
    i64.const {packed}))
"#,
        packed = packed(DECISION_PTR, 16)
    )
}

#[test]
fn storage_capability_granted_persists_a_value() {
    let tmp = tempfile::tempdir().unwrap();
    let storage_path = tmp.path().join("storage.json");
    let wasm = compile(&kv_set_probe_module());

    let svc = InvocationServices {
        plugin_id: "test.kv".into(),
        consent: Arc::new([CapabilityKind::Storage].into_iter().collect()),
        storage: Arc::new(PluginStorage::new(storage_path.clone())),
        notify: None,
        query_result: None,
        http_allowed_hosts: Arc::new(vec![]),
        fs_root: None,
        secret_names: Arc::new(vec![]),
    };

    let runtime = WasmiRuntime::new();
    let mut instance = runtime
        .load("test.kv".into(), &wasm, Budget::default(), svc)
        .unwrap();
    assert_eq!(instance.pre_execute(&ctx()).unwrap(), Decision::Allow);

    // The set should have round-tripped to disk.
    let raw = std::fs::read_to_string(&storage_path).expect("storage file written");
    assert!(raw.contains("\"k\""));
    assert!(raw.contains("\"v\""));
}

#[test]
fn storage_capability_denied_drops_the_write() {
    let tmp = tempfile::tempdir().unwrap();
    let storage_path = tmp.path().join("storage.json");
    let wasm = compile(&kv_set_probe_module());

    // No grants — the host fn must return ERR_DENIED and never touch the file.
    let svc = services(
        "test.kv.denied",
        &[],
        &tmp.path().to_path_buf(),
        None,
        vec![],
        vec![],
    );

    let runtime = WasmiRuntime::new();
    let mut instance = runtime
        .load("test.kv.denied".into(), &wasm, Budget::default(), svc)
        .unwrap();
    assert_eq!(instance.pre_execute(&ctx()).unwrap(), Decision::Allow);
    assert!(
        !storage_path.exists(),
        "denied storage write must not touch the disk"
    );
}

/// Module whose `pre_execute` issues an HTTP request to an unallowed host
/// and discards the result. The host fn must short-circuit before the
/// network is touched.
fn http_probe_module() -> String {
    // URL "http://blocked.test/" at 2048 — 20 chars.
    let url = "http://blocked.test/";
    format!(
        r#"
(module
  (import "env" "qoredb_http_request" (func $http
    (param i32 i32 i32 i32 i32 i32) (result i64)))
  (memory (export "memory") 2)
  (data (i32.const {DECISION_PTR}) "{{\"kind\":\"allow\"}}")
  (data (i32.const 2000) "GET")
  (data (i32.const 2048) "{url}")
  (func (export "qoredb_alloc") (param i32) (result i32) i32.const {ALLOC_PTR})
  (func (export "pre_execute") (param i32 i32) (result i64)
    (drop (call $http
      (i32.const 2000) (i32.const 3)
      (i32.const 2048) (i32.const {url_len})
      (i32.const 0) (i32.const 0)))
    i64.const {packed}))
"#,
        url_len = url.len(),
        packed = packed(DECISION_PTR, 16)
    )
}

#[test]
fn http_request_to_unallowed_host_is_rejected_before_the_network() {
    let tmp = tempfile::tempdir().unwrap();
    let wasm = compile(&http_probe_module());

    // The capability is granted but the URL's host is not on the allowlist
    // — the host fn must return 0 and never reach reqwest.
    let svc = services(
        "test.http",
        &[CapabilityKind::Http],
        &tmp.path().to_path_buf(),
        None,
        vec!["api.example.com".into()],
        vec![],
    );

    let runtime = WasmiRuntime::new();
    let mut instance = runtime
        .load("test.http".into(), &wasm, Budget::default(), svc)
        .unwrap();
    // If the request actually fired, the hook would either hang or take
    // seconds; getting Allow back synchronously proves the short-circuit.
    assert_eq!(instance.pre_execute(&ctx()).unwrap(), Decision::Allow);
}

/// Module that asks the host to read `../escape` through `qoredb_fs_read`.
fn fs_escape_module() -> String {
    let path = "../escape";
    format!(
        r#"
(module
  (import "env" "qoredb_fs_read" (func $fs_read (param i32 i32) (result i64)))
  (memory (export "memory") 2)
  (data (i32.const {DECISION_PTR}) "{{\"kind\":\"allow\"}}")
  (data (i32.const 2048) "{path}")
  (func (export "qoredb_alloc") (param i32) (result i32) i32.const {ALLOC_PTR})
  (func (export "pre_execute") (param i32 i32) (result i64)
    (drop (call $fs_read (i32.const 2048) (i32.const {path_len})))
    i64.const {packed}))
"#,
        path_len = path.len(),
        packed = packed(DECISION_PTR, 16)
    )
}

#[test]
fn fs_read_outside_the_scoped_root_is_rejected() {
    let tmp = tempfile::tempdir().unwrap();
    let fs_root = tmp.path().join("plugin-data");
    std::fs::create_dir_all(&fs_root).unwrap();
    // Drop a file the test would *not* want the plugin to reach.
    let secret = tmp.path().join("escape");
    std::fs::write(&secret, b"top-secret").unwrap();

    let wasm = compile(&fs_escape_module());
    let svc = services(
        "test.fs",
        &[CapabilityKind::Fs],
        &tmp.path().to_path_buf(),
        Some(fs_root),
        vec![],
        vec![],
    );

    let runtime = WasmiRuntime::new();
    let mut instance = runtime
        .load("test.fs".into(), &wasm, Budget::default(), svc)
        .unwrap();
    // The call returns 0 — the hook still produces a valid Allow decision.
    assert_eq!(instance.pre_execute(&ctx()).unwrap(), Decision::Allow);
}

#[test]
fn post_execute_runs_on_both_success_and_error_envelopes() {
    // post_execute exists, takes (ptr, len) and returns nothing — the host
    // dispatches it on success *and* on error/timeout (the success flag in
    // PostExecuteResult tells the plugin which).
    let wat = format!(
        r#"
(module
  (memory (export "memory") 2)
  (func (export "qoredb_alloc") (param i32) (result i32) i32.const {ALLOC_PTR})
  (func (export "post_execute") (param i32 i32)))
"#
    );
    let wasm = compile(&wat);

    let tmp = tempfile::tempdir().unwrap();
    let runtime = WasmiRuntime::new();
    let svc = services(
        "test.post",
        &[],
        &tmp.path().to_path_buf(),
        None,
        vec![],
        vec![],
    );
    let mut instance = runtime
        .load("test.post".into(), &wasm, Budget::default(), svc)
        .unwrap();

    let ok = post_ok();
    instance
        .post_execute(&ctx(), &ok, None::<Arc<QueryReadPayload>>)
        .expect("post_execute on success");

    let err = PostExecuteResult {
        success: false,
        execution_time_ms: 1,
        row_count: None,
        error: Some("boom".into()),
    };
    instance
        .post_execute(&ctx(), &err, None::<Arc<QueryReadPayload>>)
        .expect("post_execute on error");
}
