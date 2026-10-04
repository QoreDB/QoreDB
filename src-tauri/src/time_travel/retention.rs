// SPDX-License-Identifier: BUSL-1.1

use std::collections::VecDeque;
use std::sync::{Weak, atomic::Ordering};
use std::time::Duration as StdDuration;

use chrono::{DateTime, Duration, Utc};
use tracing::{info, warn};

use super::{ChangelogEntry, ChangelogStore, TimeTravelConfig};

const MAINTENANCE_INTERVAL: StdDuration = StdDuration::from_secs(15 * 60);

impl ChangelogStore {
    /// Apply duration, count and byte limits even when new capture is disabled.
    pub fn enforce_retention(&self) -> Result<(), String> {
        let _file_guard = self.file_lock.lock();
        self.enforce_retention_locked(&self.config.read().clone(), Utc::now())
    }

    pub(super) fn enforce_retention_locked(
        &self,
        config: &TimeTravelConfig,
        now: DateTime<Utc>,
    ) -> Result<(), String> {
        if !self.retention_config_valid.load(Ordering::Relaxed) {
            return Err("Retention requires a readable time-travel configuration".into());
        }
        let cutoff = (config.retention_days != 0).then(|| {
            now.checked_sub_signed(Duration::days(i64::from(config.retention_days)))
                .unwrap_or(DateTime::<Utc>::MIN_UTC)
        });
        let max_bytes = (config.max_file_size_mb != 0)
            .then(|| config.max_file_size_mb.saturating_mul(1_048_576));
        let mut retained = VecDeque::new();
        let mut total = 0;
        let mut bytes = 0u64;
        // Only indices and lengths are retained here; row images stay on disk.
        self.visit_file(|line| {
            let expired = cutoff.is_some_and(|cutoff| {
                serde_json::from_str::<ChangelogEntry>(line)
                    .is_ok_and(|entry| entry.timestamp < cutoff)
            });
            if !expired {
                let size = (line.len() as u64).saturating_add(1);
                retained.push_back((total, size));
                bytes = bytes.saturating_add(size);
            }
            total += 1;
        })?;
        let count_target = if retained.len() > config.max_entries {
            (config.max_entries as u128 * 3 / 4) as usize
        } else {
            config.max_entries
        };
        let byte_target = max_bytes.map(|limit| {
            if bytes > limit {
                (u128::from(limit) * 3 / 4) as u64
            } else {
                limit
            }
        });
        while retained.len() > count_target || byte_target.is_some_and(|target| bytes > target) {
            let Some((_, size)) = retained.pop_front() else {
                break;
            };
            bytes -= size;
        }
        if retained.len() == total {
            return Ok(());
        }
        let first_index = retained.front().map(|(index, _)| *index);
        // Keep a contiguous suffix of non-expired records. Skipping an oversized
        // newest event while keeping an older state would falsely present it as current.
        let removed = self.retain_file(|index, entry| {
            first_index.is_some_and(|first| index >= first)
                && entry.is_none_or(|entry| cutoff.is_none_or(|cutoff| entry.timestamp >= cutoff))
        })?;
        info!("Applied time-travel retention, removed {removed} entries");
        Ok(())
    }
}

/// The first tick runs immediately; later passes also cover idle/disabled capture.
/// Holding only a Weak between passes lets the application release its store.
pub async fn run_retention_maintenance(store: Weak<ChangelogStore>) {
    maintenance_loop(store, MAINTENANCE_INTERVAL).await;
}

pub(super) async fn maintenance_loop(store: Weak<ChangelogStore>, period: StdDuration) {
    let mut interval = tokio::time::interval(period);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    loop {
        interval.tick().await;
        let Some(store) = store.upgrade() else {
            return;
        };
        match tokio::task::spawn_blocking(move || store.enforce_retention()).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) => warn!("Time-travel retention failed: {error}"),
            Err(error) => warn!("Time-travel retention task failed: {error}"),
        }
    }
}
