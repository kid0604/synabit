//! Hybrid Logical Clock (HLC) for LWW conflict resolution.
//!
//! Combines physical wall clock with a logical counter to produce
//! timestamps that are monotonic even under clock skew. A node ID
//! breaks ties when both time and counter are equal.

use serde::{Deserialize, Serialize};

/// Hybrid Logical Clock timestamp for LWW conflict resolution.
/// Combines physical wall clock with logical counter to handle clock skew.
#[derive(Debug, Clone, Serialize, Deserialize, Eq, PartialEq)]
pub struct HybridTimestamp {
    /// Physical wall clock (milliseconds since Unix epoch)
    pub time_ms: u64,
    /// Logical counter (breaks ties when time_ms is equal)
    pub counter: u32,
    /// Node identifier (breaks ties when counter is also equal)
    pub node_id: String,
}

impl HybridTimestamp {
    /// Create a new timestamp from the current wall clock.
    pub fn now(node_id: &str) -> Self {
        let time_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;
        Self {
            time_ms,
            counter: 0,
            node_id: node_id.to_string(),
        }
    }

    /// Receive/merge: update this clock given a remote timestamp.
    /// Returns a new timestamp that is strictly greater than both self and remote.
    pub fn receive(&self, remote: &Self) -> Self {
        let phys = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        let max_time = self.time_ms.max(remote.time_ms).max(phys);

        let counter = if max_time == self.time_ms && max_time == remote.time_ms {
            self.counter.max(remote.counter) + 1
        } else if max_time == self.time_ms {
            self.counter + 1
        } else if max_time == remote.time_ms {
            remote.counter + 1
        } else {
            0
        };

        Self {
            time_ms: max_time,
            counter,
            node_id: self.node_id.clone(),
        }
    }

    /// Tick: advance this clock for a local event.
    pub fn tick(&mut self) {
        let phys = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        if phys > self.time_ms {
            self.time_ms = phys;
            self.counter = 0;
        } else {
            self.counter += 1;
        }
    }
}

impl Ord for HybridTimestamp {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.time_ms
            .cmp(&other.time_ms)
            .then(self.counter.cmp(&other.counter))
            .then(self.node_id.cmp(&other.node_id))
    }
}

impl PartialOrd for HybridTimestamp {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_now_creates_timestamp() {
        let ts = HybridTimestamp::now("node-A");
        assert!(ts.time_ms > 0, "time_ms should be a positive wall clock");
        assert_eq!(ts.counter, 0, "fresh timestamp starts at counter 0");
        assert_eq!(ts.node_id, "node-A");
    }

    #[test]
    fn test_tick_increments() {
        let mut ts = HybridTimestamp {
            time_ms: u64::MAX - 1, // far-future so phys < time_ms
            counter: 5,
            node_id: "node-A".to_string(),
        };
        ts.tick();
        // Physical clock is behind, so counter should increment
        assert_eq!(ts.counter, 6);
        assert_eq!(ts.time_ms, u64::MAX - 1);

        // When physical clock is ahead, counter resets
        let mut ts2 = HybridTimestamp {
            time_ms: 0,
            counter: 42,
            node_id: "node-A".to_string(),
        };
        ts2.tick();
        assert_eq!(ts2.counter, 0, "counter resets when wall clock advances");
        assert!(ts2.time_ms > 0, "time_ms updated to wall clock");
    }

    #[test]
    fn test_receive_merges() {
        let local = HybridTimestamp {
            time_ms: 1000,
            counter: 3,
            node_id: "node-A".to_string(),
        };
        let remote = HybridTimestamp {
            time_ms: 1000,
            counter: 5,
            node_id: "node-B".to_string(),
        };

        let merged = local.receive(&remote);
        // Wall clock is >> 1000, so phys wins, counter = 0
        assert!(merged.time_ms >= 1000);
        // The merged timestamp must be > both local and remote
        assert!(merged > local);
        assert!(merged > remote);
    }

    #[test]
    fn test_ordering() {
        let a = HybridTimestamp {
            time_ms: 100,
            counter: 0,
            node_id: "A".to_string(),
        };
        let b = HybridTimestamp {
            time_ms: 200,
            counter: 0,
            node_id: "A".to_string(),
        };
        assert!(a < b, "higher time_ms should sort later");

        let c = HybridTimestamp {
            time_ms: 100,
            counter: 1,
            node_id: "A".to_string(),
        };
        assert!(a < c, "higher counter should sort later with same time");

        let d = HybridTimestamp {
            time_ms: 100,
            counter: 0,
            node_id: "B".to_string(),
        };
        assert!(a < d, "higher node_id should sort later as tiebreaker");
    }

    #[test]
    fn test_clock_skew_handling() {
        // Simulate a remote clock far in the future
        let local = HybridTimestamp {
            time_ms: 1000,
            counter: 0,
            node_id: "local".to_string(),
        };
        let remote_future = HybridTimestamp {
            time_ms: u64::MAX - 1,
            counter: 10,
            node_id: "remote".to_string(),
        };

        let merged = local.receive(&remote_future);
        // max_time should be remote's time_ms (far future)
        assert_eq!(merged.time_ms, u64::MAX - 1);
        // Since max_time == remote.time_ms (and > local and phys),
        // counter = remote.counter + 1
        assert_eq!(merged.counter, 11);
        assert!(merged > remote_future);
        assert!(merged > local);
    }
}
