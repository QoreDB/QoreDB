// SPDX-License-Identifier: Apache-2.0

//! Holds background events until the frontend is mounted, then flushes them,
//! and targets `main` instead of broadcasting. Emitting into a webview that is
//! mid-create/reload can hit a reentrant borrow in tauri-runtime-wry and panic
//! (`RefCell already mutably borrowed`; tauri-apps/tauri#8177).

use std::collections::VecDeque;
use std::sync::Mutex;

use serde::Serialize;
use serde_json::Value;
use tauri::{AppHandle, Emitter, Manager};

const MAIN_WINDOW: &str = "main";

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
        // Check + buffer under one lock so a concurrent flush can't strand the event.
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

/// Falls back to a direct `main`-targeted emit if the gate isn't managed yet.
pub fn emit_gated<S: Serialize>(app: &AppHandle, event: &str, payload: &S) {
    match app.try_state::<EmitGate>() {
        Some(gate) => gate.emit(app, event, payload),
        None => {
            let _ = app.emit_to(MAIN_WINDOW, event, payload);
        }
    }
}

/// Frontend calls this on mount (true) and, best-effort, before reload (false).
#[tauri::command]
pub fn set_frontend_ready(ready: bool, app: AppHandle, gate: tauri::State<'_, EmitGate>) {
    if ready {
        gate.open_and_flush(&app);
    } else {
        gate.close();
    }
}
