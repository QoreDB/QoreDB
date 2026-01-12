// QoreDB - Modern local-first database client
// Core library

pub mod engine;

use std::sync::Arc;
use tokio::sync::Mutex;

use engine::drivers::mongodb::MongoDriver;
use engine::drivers::mysql::MySqlDriver;
use engine::drivers::postgres::PostgresDriver;
use engine::DriverRegistry;

/// Application state shared across Tauri commands
pub struct AppState {
    pub registry: DriverRegistry,
}

impl AppState {
    pub fn new() -> Self {
        let mut registry = DriverRegistry::new();

        // Register all built-in drivers
        registry.register(Arc::new(PostgresDriver::new()));
        registry.register(Arc::new(MySqlDriver::new()));
        registry.register(Arc::new(MongoDriver::new()));

        Self { registry }
    }
}

impl Default for AppState {
    fn default() -> Self {
        Self::new()
    }
}

#[tauri::command]
fn greet(name: &str) -> String {
    format!("Hello, {}! You've been greeted from Rust!", name)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    let state = AppState::new();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(Arc::new(Mutex::new(state)))
        .invoke_handler(tauri::generate_handler![greet])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
