// SPDX-License-Identifier: Apache-2.0

//! Serializes background event emission with the webview lifecycle.
//!
//! In `tauri-runtime-wry`, `emit()` becomes an `eval()` on the target webview,
//! processed by `handle_webview_message` which borrows the runtime's `windows`
//! map. If a background task emits while a webview is being created or reloaded
//! (e.g. a Vite HMR full reload in dev, or startup), that borrow can collide
//! with the mutable borrow held during creation and panic with
//! `RefCell already mutably borrowed`. See tauri-apps/tauri#8177, #9775, #10987.
//!
//! The gate holds events until the frontend signals it has mounted, then flushes
//! them, and targets the `main` window instead of broadcasting to every webview
//! (fewer concurrent evals = smaller collision window).

use std::collections::VecDeque;
use std::sync::Mutex;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

const MAIN_WINDOW: &str = "main";

/// Bounded so a frontend that never signals ready can't grow this unbounded.
/// Overflow drops the oldest event.
const MAX_BUFFERED: usize = 512;

struct GateInner {
    ready: bool,
    buffer: VecDeque<(String, Value)>,
}

pub struct EmitGate {
    inner: Mutex<GateInner>,
}

impl EmitGate {
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(GateInner {
                ready: false,
                buffer: VecDeque::new(),
            }),
        }
    }

    fn emit<S: Serialize>(&self, app: &AppHandle, event: &str, payload: &S) {
        // Decide + buffer under the lock so a concurrent `open_and_flush`
        // can't slip between the ready check and the push (which would strand
        // the event in the buffer until the next flush).
        let ready = {
            let mut inner = self.inner.lock().unwrap();
            if inner.ready {
                true
            } else {
                if let Ok(value) = serde_json::to_value(payload) {
                    if inner.buffer.len() >= MAX_BUFFERED {
                        inner.buffer.pop_front();
                    }
                    inner.buffer.push_back((event.to_string(), value));
                }
                false
            }
        };
        if ready {
            let _ = app.emit_to(MAIN_WINDOW, event, payload);
        }
    }

    fn open_and_flush(&self, app: &AppHandle) {
        let drained: Vec<(String, Value)> = {
            let mut inner = self.inner.lock().unwrap();
            inner.ready = true;
            inner.buffer.drain(..).collect()
        };
        for (event, value) in drained {
            let _ = app.emit_to(MAIN_WINDOW, event.as_str(), value);
        }
    }

    fn close(&self) {
        self.inner.lock().unwrap().ready = false;
    }
}

impl Default for EmitGate {
    fn default() -> Self {
        Self::new()
    }
}

/// Emit through the gate if one is managed, otherwise fall back to a direct
/// `main`-targeted emit (keeps callers working even before the gate is managed).
pub fn emit_gated<S: Serialize>(app: &AppHandle, event: &str, payload: &S) {
    match app.try_state::<EmitGate>() {
        Some(gate) => gate.emit(app, event, payload),
        None => {
            let _ = app.emit_to(MAIN_WINDOW, event, payload);
        }
    }
}

/// The frontend calls this on mount (`ready = true`) and, best-effort, before a
/// reload (`ready = false`). Opening flushes any events buffered during startup.
#[tauri::command]
pub fn set_frontend_ready(ready: bool, app: AppHandle, gate: tauri::State<'_, EmitGate>) {
    if ready {
        gate.open_and_flush(&app);
    } else {
        gate.close();
    }
}
