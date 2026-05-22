// SPDX-License-Identifier: Apache-2.0

//! Query result cache Tauri commands.

use std::sync::Arc;

use tauri::State;

use crate::cache::{CacheConfig, CacheStats};
use crate::SharedState;

/// Returns the current cache configuration.
#[tauri::command]
pub async fn get_cache_config(state: State<'_, SharedState>) -> Result<CacheConfig, String> {
    let cache = Arc::clone(&state.lock().await.query_cache);
    Ok(cache.config())
}

/// Persists and applies a new cache configuration.
#[tauri::command]
pub async fn set_cache_config(
    state: State<'_, SharedState>,
    mut config: CacheConfig,
) -> Result<CacheConfig, String> {
    config.clamp();
    config.save()?;
    let cache = Arc::clone(&state.lock().await.query_cache);
    cache.set_config(config);
    Ok(cache.config())
}

/// Empties the query result cache.
#[tauri::command]
pub async fn clear_query_cache(state: State<'_, SharedState>) -> Result<(), String> {
    let cache = Arc::clone(&state.lock().await.query_cache);
    cache.clear();
    Ok(())
}

/// Returns runtime cache counters.
#[tauri::command]
pub async fn get_cache_stats(state: State<'_, SharedState>) -> Result<CacheStats, String> {
    let cache = Arc::clone(&state.lock().await.query_cache);
    Ok(cache.stats())
}
