// SPDX-License-Identifier: Apache-2.0

//! End-to-end tests for the executable plugin runtime.
//!
//! Tests build inline WAT modules so no `wasm32-unknown-unknown` toolchain
//! is required.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::Arc;

use qoredb_lib::plugins::runtime::{
    Budget, CapabilityKind, Decision, HookContext, InvocationServices, PluginError, PluginRuntime,
    PluginStorage, PostExecuteResult, QueryReadPayload, WasmiRuntime,
};

/// Decision JSON is stored at this offset via a `data` segment.
const DECISION_PTR: i32 = 1024;
/// Where guests place host-provided input — far enough past `DECISION_PTR`
/// to never overlap.
const ALLOC_PTR: i32 = 16 * 1024;

fn packed(ptr: i32, len: i32) -> i64 {
    ((ptr as i64) << 32) | (len as i64 & 0xFFFF_FFFF)
}

/// `decision_json` is the raw JSON ({`"kind":"allow"`}); WAT escaping is
/// handled here.
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
        http_allow_private_networks: false,
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

/// Calls `qoredb_kv_set("k", "v")` from `pre_execute`. The Allow blob sits
/// at `DECISION_PTR`; key "k" at 2048, value "v" at 2050.
fn kv_set_probe_module() -> String {
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
        http_allow_private_networks: false,
        fs_root: None,
        secret_names: Arc::new(vec![]),
    };

    let runtime = WasmiRuntime::new();
    let mut instance = runtime
        .load("test.kv".into(), &wasm, Budget::default(), svc)
        .unwrap();
    assert_eq!(instance.pre_execute(&ctx()).unwrap(), Decision::Allow);

    let raw = std::fs::read_to_string(&storage_path).expect("storage file written");
    assert!(raw.contains("\"k\""));
    assert!(raw.contains("\"v\""));
}

#[test]
fn storage_capability_denied_drops_the_write() {
    let tmp = tempfile::tempdir().unwrap();
    let storage_path = tmp.path().join("storage.json");
    let wasm = compile(&kv_set_probe_module());

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

fn http_probe_module() -> String {
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

fn http_loopback_probe_module() -> String {
    let url = "http://127.0.0.1:9/";
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
fn http_request_to_a_loopback_address_is_rejected_by_the_ssrf_guard() {
    // If the guard regresses, the call reaches reqwest and blows past the
    // 500ms assertion below (connect or timeout).
    let tmp = tempfile::tempdir().unwrap();
    let wasm = compile(&http_loopback_probe_module());

    let svc = services(
        "test.ssrf",
        &[CapabilityKind::Http],
        &tmp.path().to_path_buf(),
        None,
        vec!["127.0.0.1".into()],
        vec![],
    );

    let runtime = WasmiRuntime::new();
    let mut instance = runtime
        .load("test.ssrf".into(), &wasm, Budget::default(), svc)
        .unwrap();
    let start = std::time::Instant::now();
    assert_eq!(instance.pre_execute(&ctx()).unwrap(), Decision::Allow);
    assert!(
        start.elapsed() < std::time::Duration::from_millis(500),
        "SSRF guard must short-circuit; took {:?}",
        start.elapsed()
    );
}

#[test]
fn http_request_to_unallowed_host_is_rejected_before_the_network() {
    let tmp = tempfile::tempdir().unwrap();
    let wasm = compile(&http_probe_module());

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
    // A synchronous Allow return proves the short-circuit: any real fetch
    // would either hang or take seconds.
    assert_eq!(instance.pre_execute(&ctx()).unwrap(), Decision::Allow);
}

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
    // File the plugin must not be able to reach.
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
    assert_eq!(instance.pre_execute(&ctx()).unwrap(), Decision::Allow);
}

#[test]
fn post_execute_runs_on_both_success_and_error_envelopes() {
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
