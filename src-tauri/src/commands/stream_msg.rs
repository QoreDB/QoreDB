// SPDX-License-Identifier: Apache-2.0

//! Wire format for streaming query results to the frontend.
//!
//! Two transports are supported:
//! - **Tauri Channel + MessagePack** (preferred): frontend passes a `Channel`
//!   to the command; we encode each [`StreamEvent`] as msgpack raw bytes.
//!   Avoids JSON encode/decode for large row batches.
//! - **`window.emit()` fallback** (legacy): when no channel is provided.

use qore_core::traits::StreamEvent;
use qore_core::types::{ColumnInfo, Row};
use serde::Serialize;
use tauri::ipc::{Channel, InvokeResponseBody};
use tauri::{Emitter, Window};

/// Envelope for streaming messages. Internally-tagged so the JS side can
/// inspect `msg.t` to dispatch.
#[derive(Serialize)]
#[serde(tag = "t", content = "v")]
enum StreamMsg<'a> {
    #[serde(rename = "c")]
    Columns(&'a [ColumnInfo]),
    #[serde(rename = "rb")]
    RowBatch(&'a [Row]),
    #[serde(rename = "r")]
    Row(&'a Row),
    #[serde(rename = "e")]
    Error(&'a str),
    #[serde(rename = "d")]
    Done(u64),
}

/// Initial buffer capacity used the first time we encode a stream event.
/// Tuned to fit a small `Columns` payload without growing.
const INITIAL_BUFFER_CAPACITY: usize = 512;

/// Stateful dispatcher that encodes [`StreamEvent`]s as MessagePack and pushes
/// them to a Tauri `Channel`. Holds a per-stream capacity hint that tracks the
/// largest payload seen so far, avoiding the grow-realloc cascade that
/// `rmp_serde::to_vec_named` triggers on every call (it starts from `Vec::new()`
/// and doubles).
///
/// One dispatcher per active stream — keep it alive across the receive loop so
/// the hint accumulates across batches.
pub struct StreamDispatcher<'a> {
    channel: Option<&'a Channel<InvokeResponseBody>>,
    window: &'a Window,
    query_id: &'a str,
    capacity_hint: usize,
}

impl<'a> StreamDispatcher<'a> {
    pub fn new(
        channel: Option<&'a Channel<InvokeResponseBody>>,
        window: &'a Window,
        query_id: &'a str,
    ) -> Self {
        Self {
            channel,
            window,
            query_id,
            capacity_hint: INITIAL_BUFFER_CAPACITY,
        }
    }

    /// Push an event downstream. Falls back to `window.emit` if no channel was
    /// provided or if msgpack encoding fails.
    pub fn dispatch(&mut self, event: StreamEvent) {
        if let Some(ch) = self.channel {
            let msg = match &event {
                StreamEvent::Columns(cols) => StreamMsg::Columns(cols.as_slice()),
                StreamEvent::Row(row) => StreamMsg::Row(row),
                StreamEvent::RowBatch(batch) => StreamMsg::RowBatch(batch.as_slice()),
                StreamEvent::Error(e) => StreamMsg::Error(e.as_str()),
                StreamEvent::Done(a) => StreamMsg::Done(*a),
            };
            let mut buf = Vec::with_capacity(self.capacity_hint);
            match rmp_serde::encode::write_named(&mut buf, &msg) {
                Ok(()) => {
                    // Track the largest payload seen so the next encode lands
                    // in a buffer big enough on the first try.
                    if buf.len() > self.capacity_hint {
                        self.capacity_hint = buf.len();
                    }
                    if ch.send(InvokeResponseBody::Raw(buf)).is_ok() {
                        return;
                    }
                    // Channel closed — degrade silently; the frontend has already
                    // moved on.
                }
                Err(err) => {
                    tracing::warn!(?err, "msgpack encode failed; falling back to window.emit");
                    dispatch_via_emit(event, self.window, self.query_id);
                }
            }
        } else {
            dispatch_via_emit(event, self.window, self.query_id);
        }
    }
}

/// One-shot helper for callers that don't keep a long-lived dispatcher (e.g.
/// timeout / error paths that emit a single event). Allocates a fresh
/// dispatcher each time, so prefer [`StreamDispatcher`] in the streaming
/// receive loop.
pub fn dispatch_stream_event(
    event: StreamEvent,
    channel: Option<&Channel<InvokeResponseBody>>,
    window: &Window,
    query_id: &str,
) {
    StreamDispatcher::new(channel, window, query_id).dispatch(event);
}

/// Legacy JSON event path. Kept for callers that don't provide a Channel (and
/// as a safety net when msgpack encoding fails).
pub fn dispatch_via_emit(event: StreamEvent, window: &Window, query_id: &str) {
    match event {
        StreamEvent::Columns(cols) => {
            let _ = window.emit(&format!("query_stream_columns:{}", query_id), cols);
        }
        StreamEvent::Row(row) => {
            let _ = window.emit(&format!("query_stream_row:{}", query_id), row);
        }
        StreamEvent::RowBatch(batch) => {
            let _ = window.emit(&format!("query_stream_row_batch:{}", query_id), batch);
        }
        StreamEvent::Error(e) => {
            let _ = window.emit(&format!("query_stream_error:{}", query_id), e);
        }
        StreamEvent::Done(a) => {
            let _ = window.emit(&format!("query_stream_done:{}", query_id), a);
        }
    }
}
