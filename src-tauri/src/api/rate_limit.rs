// SPDX-License-Identifier: BUSL-1.1

//! Per-endpoint token bucket rate limiter (in-memory).
//!
//! Targets the simplest invariant: ≤ `capacity` requests per second per
//! endpoint, refilling continuously at `capacity` tokens/second. Bursts are
//! capped at `capacity`. No persistence — counters reset on restart.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;

/// Default budget: 10 requests / second / endpoint.
pub const DEFAULT_CAPACITY: f64 = 10.0;

#[derive(Debug, Clone)]
struct Bucket {
    tokens: f64,
    last_refill: Instant,
}

/// Thread-safe rate limiter keyed by endpoint id.
pub struct RateLimiter {
    capacity: f64,
    refill_per_sec: f64,
    buckets: Mutex<HashMap<String, Bucket>>,
}

impl RateLimiter {
    pub fn new(capacity: f64) -> Self {
        Self {
            capacity,
            refill_per_sec: capacity,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    pub fn default_capacity() -> Self {
        Self::new(DEFAULT_CAPACITY)
    }

    /// Attempts to consume one token for `endpoint_id`. Returns `true` when
    /// the request is allowed, `false` when the bucket is empty.
    pub fn try_acquire(&self, endpoint_id: &str) -> bool {
        let now = Instant::now();
        let mut buckets = self.buckets.lock().unwrap();
        let bucket = buckets.entry(endpoint_id.to_string()).or_insert(Bucket {
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

    /// Forgets the bucket for `endpoint_id`. Call this when an endpoint is
    /// deleted so memory doesn't grow indefinitely.
    pub fn forget(&self, endpoint_id: &str) {
        self.buckets.lock().unwrap().remove(endpoint_id);
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::default_capacity()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;
    use std::time::Duration;

    #[test]
    fn allows_initial_burst_up_to_capacity() {
        let r = RateLimiter::new(3.0);
        assert!(r.try_acquire("a"));
        assert!(r.try_acquire("a"));
        assert!(r.try_acquire("a"));
        assert!(!r.try_acquire("a"));
    }

    #[test]
    fn buckets_are_independent_per_endpoint() {
        let r = RateLimiter::new(1.0);
        assert!(r.try_acquire("a"));
        assert!(!r.try_acquire("a"));
        assert!(r.try_acquire("b"));
    }

    #[test]
    fn refills_over_time() {
        let r = RateLimiter::new(10.0);
        for _ in 0..10 {
            assert!(r.try_acquire("a"));
        }
        assert!(!r.try_acquire("a"));
        sleep(Duration::from_millis(250));
        // ~2 tokens refilled (10 tokens/s × 0.25s).
        assert!(r.try_acquire("a"));
    }

    #[test]
    fn forget_clears_bucket() {
        let r = RateLimiter::new(1.0);
        assert!(r.try_acquire("a"));
        r.forget("a");
        // After forget, the bucket starts full again.
        assert!(r.try_acquire("a"));
    }
}
