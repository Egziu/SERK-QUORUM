//! In-process fixed-window throttle with bounded memory.
//!
//! Limitation: state is per process. With several API replicas each enforces
//! its own window; a shared store is required before horizontal scaling.

use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

const MAX_KEYS: usize = 50_000;

pub struct Throttle {
    limit: u32,
    window: Duration,
    entries: Mutex<HashMap<String, (u32, Instant)>>,
}

impl Throttle {
    pub fn new(limit: u32, window: Duration) -> Self {
        Self {
            limit,
            window,
            entries: Mutex::new(HashMap::new()),
        }
    }

    /// True if `key` has reached its limit within the current window.
    pub fn is_limited(&self, key: &str) -> bool {
        let now = Instant::now();
        let Ok(entries) = self.entries.lock() else {
            return true; // poisoned: fail closed
        };
        entries.get(key).is_some_and(|(count, start)| {
            now.duration_since(*start) < self.window && *count >= self.limit
        })
    }

    /// Records one event for `key`.
    pub fn hit(&self, key: &str) {
        let now = Instant::now();
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        if entries.len() >= MAX_KEYS {
            let window = self.window;
            entries.retain(|_, (_, start)| now.duration_since(*start) < window);
            if entries.len() >= MAX_KEYS {
                entries.clear();
            }
        }
        let entry = entries.entry(key.to_owned()).or_insert((0, now));
        if now.duration_since(entry.1) >= self.window {
            *entry = (0, now);
        }
        entry.0 = entry.0.saturating_add(1);
    }

    pub fn reset(&self, key: &str) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.remove(key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn limits_after_threshold_and_resets() {
        let t = Throttle::new(2, Duration::from_secs(60));
        assert!(!t.is_limited("a"));
        t.hit("a");
        assert!(!t.is_limited("a"));
        t.hit("a");
        assert!(t.is_limited("a"));
        assert!(!t.is_limited("b"));
        t.reset("a");
        assert!(!t.is_limited("a"));
    }

    #[test]
    fn window_expires() {
        let t = Throttle::new(1, Duration::from_millis(20));
        t.hit("a");
        assert!(t.is_limited("a"));
        std::thread::sleep(Duration::from_millis(30));
        assert!(!t.is_limited("a"));
    }
}
