// SPDX-License-Identifier: Apache-2.0

//! Per-session query rate limiter (in-memory, Core).
//!
//! Anti-loop guardrail: protects against accidental runaway query loops
//! (e.g. a script that spams `SELECT`s) by capping the query rate per session.
//! The budget is deliberately generous — a human never reaches it, only a
//! tight programmatic loop does. Token bucket with continuous refill, no
//! persistence: counters reset on restart.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

/// Default budget: 60 queries per 10-second window, per session.
pub const DEFAULT_CAPACITY: f64 = 60.0;
/// Refill rate matching the default budget (60 tokens / 10 s).
pub const DEFAULT_REFILL_PER_SEC: f64 = 6.0;

#[derive(Debug, Clone)]
struct Bucket {
    tokens: f64,
    last_refill: Instant,
}

/// Thread-safe token-bucket rate limiter keyed by session id.
pub struct QueryRateLimiter {
    capacity: f64,
    refill_per_sec: f64,
    buckets: Mutex<HashMap<String, Bucket>>,
}

impl QueryRateLimiter {
    pub fn new(capacity: f64, refill_per_sec: f64) -> Self {
        Self {
            capacity,
            refill_per_sec,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    /// Default anti-loop budget (60 queries / 10 s per session).
    pub fn with_defaults() -> Self {
        Self::new(DEFAULT_CAPACITY, DEFAULT_REFILL_PER_SEC)
    }

    /// Attempts to consume one token for `session_id`. Returns `true` when the
    /// query is allowed, `false` when the budget is exhausted.
    pub fn try_acquire(&self, session_id: &str) -> bool {
        let now = Instant::now();
        let mut buckets = self.buckets.lock().unwrap();
        let bucket = buckets.entry(session_id.to_string()).or_insert(Bucket {
            tokens: self.capacity,
            last_refill: now,
        });

        let elapsed = now.duration_since(bucket.last_refill).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * self.refill_per_sec).min(self.capacity);
        bucket.last_refill = now;

        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        }
    }

    /// Forgets the bucket for `session_id`. Call this when a session closes so
    /// the map doesn't grow unbounded across reconnects.
    pub fn forget(&self, session_id: &str) {
        self.buckets.lock().unwrap().remove(session_id);
    }
}

impl Default for QueryRateLimiter {
    fn default() -> Self {
        Self::with_defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn allows_initial_burst_up_to_capacity() {
        let limiter = QueryRateLimiter::new(3.0, 1.0);
        assert!(limiter.try_acquire("s1"));
        assert!(limiter.try_acquire("s1"));
        assert!(limiter.try_acquire("s1"));
        assert!(!limiter.try_acquire("s1"));
    }

    #[test]
    fn buckets_are_independent_per_session() {
        let limiter = QueryRateLimiter::new(1.0, 1.0);
        assert!(limiter.try_acquire("s1"));
        assert!(!limiter.try_acquire("s1"));
        assert!(limiter.try_acquire("s2"));
    }

    #[test]
    fn refills_over_time() {
        let limiter = QueryRateLimiter::new(10.0, 10.0);
        for _ in 0..10 {
            assert!(limiter.try_acquire("s1"));
        }
        assert!(!limiter.try_acquire("s1"));
        sleep(Duration::from_millis(250));
        // ~2 tokens refilled (10 tokens/s × 0.25 s).
        assert!(limiter.try_acquire("s1"));
    }

    #[test]
    fn forget_clears_bucket() {
        let limiter = QueryRateLimiter::new(1.0, 1.0);
        assert!(limiter.try_acquire("s1"));
        limiter.forget("s1");
        // After forget, the bucket starts full again.
        assert!(limiter.try_acquire("s1"));
    }

    #[test]
    fn default_budget_absorbs_human_pace_but_stops_a_loop() {
        let limiter = QueryRateLimiter::with_defaults();
        // A tight loop exhausts the 60-token budget…
        for _ in 0..60 {
            assert!(limiter.try_acquire("s1"));
        }
        // …and the 61st query in the same instant is refused.
        assert!(!limiter.try_acquire("s1"));
    }
}
