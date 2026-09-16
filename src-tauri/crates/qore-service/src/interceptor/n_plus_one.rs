// SPDX-License-Identifier: BUSL-1.1

//! N+1 detection: the same fingerprint executed many times within a couple of
//! seconds on one session. Flagged once per session so the audit log and the
//! notification do not repeat on every extra iteration.

use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

use parking_lot::Mutex;

pub const MIN_EXECUTIONS: usize = 20;
pub const WINDOW: Duration = Duration::from_secs(2);
const HISTORY_PER_SESSION: usize = 64;

#[derive(Default)]
struct Inner {
    recent: HashMap<String, VecDeque<(String, Instant)>>,
    notified: HashSet<String>,
}

#[derive(Default)]
pub struct NPlusOneDetector {
    inner: Mutex<Inner>,
}

impl NPlusOneDetector {
    /// Records one execution and returns the burst size the first time a
    /// session crosses the threshold.
    pub fn observe(&self, session_id: &str, fingerprint: &str, now: Instant) -> Option<usize> {
        let mut inner = self.inner.lock();
        let history = inner.recent.entry(session_id.to_string()).or_default();
        history.push_back((fingerprint.to_string(), now));
        while history.len() > HISTORY_PER_SESSION {
            history.pop_front();
        }
        while history
            .front()
            .is_some_and(|(_, at)| now.duration_since(*at) > WINDOW)
        {
            history.pop_front();
        }
        let count = history.iter().filter(|(fp, _)| fp == fingerprint).count();
        if count < MIN_EXECUTIONS || inner.notified.contains(session_id) {
            return None;
        }
        inner.notified.insert(session_id.to_string());
        Some(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fires_once_per_session_when_a_burst_crosses_the_threshold() {
        let detector = NPlusOneDetector::default();
        let start = Instant::now();
        for i in 0..MIN_EXECUTIONS - 1 {
            assert_eq!(
                detector.observe("s1", "fp", start + Duration::from_millis(i as u64)),
                None
            );
        }
        assert_eq!(
            detector.observe("s1", "fp", start + Duration::from_millis(50)),
            Some(MIN_EXECUTIONS)
        );
        assert_eq!(
            detector.observe("s1", "fp", start + Duration::from_millis(60)),
            None
        );
        assert_eq!(
            detector.observe("s2", "fp", start + Duration::from_millis(60)),
            None
        );
    }

    #[test]
    fn a_slow_loop_is_not_a_burst() {
        let detector = NPlusOneDetector::default();
        let start = Instant::now();
        for i in 0..40 {
            assert_eq!(
                detector.observe("s1", "fp", start + Duration::from_millis(500 * i)),
                None
            );
        }
    }

    #[test]
    fn different_fingerprints_do_not_add_up() {
        let detector = NPlusOneDetector::default();
        let start = Instant::now();
        for i in 0..40 {
            assert_eq!(detector.observe("s1", &format!("fp{i}"), start), None);
        }
    }
}
