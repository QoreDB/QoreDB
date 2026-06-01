// Injects the embedded license public key at compile time, mirroring the app's
// build script: `key.rs` reads it via `env!("PUBLIC_KEY_BASE64")`.
fn main() {
    let _ = dotenvy::from_path("../../../.env");

    let key = std::env::var("PUBLIC_KEY_BASE64").unwrap_or_default();
    let profile = std::env::var("PROFILE").unwrap_or_default();

    if key.is_empty() {
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
    println!("cargo:rerun-if-changed=../../../.env");
    println!("cargo:rerun-if-env-changed=PUBLIC_KEY_BASE64");
}
