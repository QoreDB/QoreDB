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

/// Dispatch a stream event to the frontend, preferring the binary Channel path
/// when available. Falls back to `window.emit` (JSON) otherwise.
pub fn dispatch_stream_event(
    event: StreamEvent,
    channel: Option<&Channel<InvokeResponseBody>>,
    window: &Window,
    query_id: &str,
) {
    if let Some(ch) = channel {
        let msg = match &event {
            StreamEvent::Columns(cols) => StreamMsg::Columns(cols.as_slice()),
            StreamEvent::Row(row) => StreamMsg::Row(row),
            StreamEvent::RowBatch(batch) => StreamMsg::RowBatch(batch.as_slice()),
            StreamEvent::Error(e) => StreamMsg::Error(e.as_str()),
            StreamEvent::Done(a) => StreamMsg::Done(*a),
        };
        match rmp_serde::to_vec_named(&msg) {
            Ok(bytes) => {
                if ch.send(InvokeResponseBody::Raw(bytes)).is_ok() {
                    return;
                }
                // Channel closed — degrade silently; the frontend has already
                // moved on.
            }
            Err(err) => {
                tracing::warn!(?err, "msgpack encode failed; falling back to window.emit");
                dispatch_via_emit(event, window, query_id);
            }
        }
    } else {
        dispatch_via_emit(event, window, query_id);
    }
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
