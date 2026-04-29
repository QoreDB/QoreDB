// SPDX-License-Identifier: Apache-2.0

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[global_allocator]
static GLOBAL: mimalloc::MiMalloc = mimalloc::MiMalloc;

fn main() {
    // WebKitGTK 2.42+ DMA-BUF renderer causes unbounded VRAM/RAM usage on
    // certain GPU drivers (notably AMD/Radeon), leading to full system freeze.
    // Disable it before any WebKitGTK initialization.
    #[cfg(target_os = "linux")]
    {
        std::env::set_var("WEBKIT_DISABLE_DMABUF_RENDERER", "1");
    }

    qoredb_lib::run()
}
