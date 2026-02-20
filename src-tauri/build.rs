// SPDX-License-Identifier: Apache-2.0

fn main() {
    // Load .env from project root to get PUBLIC_KEY_BASE64 at compile time
    let _ = dotenvy::from_path("../.env");

    if let Ok(key) = std::env::var("PUBLIC_KEY_BASE64") {
        println!("cargo:rustc-env=PUBLIC_KEY_BASE64={}", key);
    }

    // Rebuild if .env changes
    println!("cargo:rerun-if-changed=../.env");
    println!("cargo:rerun-if-env-changed=PUBLIC_KEY_BASE64");

    tauri_build::build()
}
