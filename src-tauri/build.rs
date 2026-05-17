// SPDX-License-Identifier: Apache-2.0

fn main() {
    let _ = dotenvy::from_path("../.env");

    let key = std::env::var("PUBLIC_KEY_BASE64").unwrap_or_default();
    let profile = std::env::var("PROFILE").unwrap_or_default();

    if key.is_empty() {
        // A zero key would silently accept signatures verified against an
        // all-zero ed25519 public key. Refuse to build release artefacts in
        // that state — the CI must inject the real licensing public key.
        if profile == "release" {
            panic!(
                "PUBLIC_KEY_BASE64 must be set when building in release mode. \
                 Provide it via .env or the build environment."
            );
        }
        println!(
            "cargo:warning=PUBLIC_KEY_BASE64 is not set; using a zero key for development only. \
             Release builds will refuse to compile without it."
        );
        println!("cargo:rustc-env=PUBLIC_KEY_BASE64=AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=");
    } else {
        println!("cargo:rustc-env=PUBLIC_KEY_BASE64={}", key);
    }
    println!("cargo:rerun-if-changed=../.env");
    println!("cargo:rerun-if-env-changed=PUBLIC_KEY_BASE64");

    tauri_build::build()
}
