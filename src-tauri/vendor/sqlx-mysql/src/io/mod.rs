// SPDX-License-Identifier: Apache-2.0
mod buf;
mod buf_mut;

pub use buf::MySqlBufExt;
pub use buf_mut::MySqlBufMutExt;

pub(crate) use sqlx_core::io::*;
