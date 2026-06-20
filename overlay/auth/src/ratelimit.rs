//! In-memory leaky-bucket rate limiter.
//!
//! Algorithm cloned from smhanov/auth/ratelimit.go (MIT) — translated to
//! Rust. Each named bucket tracks a "value" that drains at `rate / period`
//! per second. `allow()` succeeds if value + cost stays under rate, then
//! charges the cost. Concurrent calls are serialized with a Mutex; the
//! limiter is held in a single Arc and shared across handler threads.
//!
//! Time is passed in explicitly (`now_secs`) so tests are deterministic.

use std::collections::HashMap;
use std::sync::Mutex;

#[derive(Debug, Clone, Copy)]
struct Record {
    value: f64,
    at: f64,
}

#[derive(Default)]
pub struct RateLimiter {
    state: Mutex<HashMap<String, Record>>,
}

impl RateLimiter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns true if the operation is allowed, charging `cost` against the
    /// bucket. `rate` is max allowed cost per `period_secs`.
    pub fn allow(&self, key: &str, rate: f64, period_secs: f64, now_secs: i64) -> bool {
        let mut map = self.state.lock().expect("ratelimit mutex");
        let now = now_secs as f64;
        let rec = map.entry(key.to_string()).or_insert(Record { value: 0.0, at: now });

        // Drain proportional to elapsed time.
        let elapsed = (now - rec.at).max(0.0);
        rec.value = (rec.value - elapsed * rate / period_secs).max(0.0);
        rec.at = now;

        if rec.value + 1.0 <= rate {
            rec.value += 1.0;
            true
        } else {
            false
        }
    }
}
