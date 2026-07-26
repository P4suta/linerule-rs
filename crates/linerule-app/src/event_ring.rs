//! Ring buffer of the last N tracing events for the panic hook to snapshot into
//! `crash_dump::CrashRecord::recent_events`.
//!
//! Each logging session owns an independent ring. The panic hook captures that
//! instance, so parallel tests and multiple subscribers never share state.

#![forbid(unsafe_code)]

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tracing::{Event, Subscriber};
use tracing_subscriber::field::Visit;
use tracing_subscriber::layer::{Context, Layer};

/// Ring buffer cap; ~16ms/frame means 256 entries holds roughly 4s of context.
const CAPACITY: usize = 256;

/// Snapshot of one tracing event.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct RingEntry {
    /// Milliseconds since the Unix epoch.
    pub(crate) unix_ms: i64,
    /// Stringified `tracing::Level`.
    pub(crate) level: String,
    /// Event target, for subsystem filtering.
    pub(crate) target: String,
    /// The event's message field.
    pub(crate) message: String,
    /// Remaining fields as a JSON object.
    pub(crate) fields: serde_json::Value,
}

/// Independently owned bounded event history.
#[derive(Clone)]
pub(crate) struct EventRing {
    entries: Arc<Mutex<VecDeque<RingEntry>>>,
}

impl EventRing {
    /// Create an empty ring.
    pub(crate) fn new() -> Self {
        Self {
            entries: Arc::new(Mutex::new(VecDeque::with_capacity(CAPACITY))),
        }
    }

    /// Snapshot the last `n` entries, oldest-to-newest. A poisoned lock is
    /// recovered so panic reporting can still make progress.
    pub(crate) fn snapshot_tail(&self, n: usize) -> Vec<RingEntry> {
        let guard = match self.entries.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        let start = guard.len().saturating_sub(n);
        guard.iter().skip(start).cloned().collect()
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.lock().map_or(0, |entries| entries.len())
    }
}

/// `Layer` that pushes events into the ring buffer.
pub(crate) struct RingBufferLayer {
    ring: EventRing,
}

impl RingBufferLayer {
    pub(crate) const fn new(ring: EventRing) -> Self {
        Self { ring }
    }
}

impl<S: Subscriber> Layer<S> for RingBufferLayer {
    fn on_event(&self, event: &Event<'_>, _ctx: Context<'_, S>) {
        let metadata = event.metadata();
        let level = metadata.level().to_string();
        let target = metadata.target().to_string();

        let mut visitor = FieldVisitor::default();
        event.record(&mut visitor);

        let entry = RingEntry {
            unix_ms: current_unix_ms(),
            level,
            target,
            message: visitor.message,
            fields: serde_json::Value::Object(visitor.fields),
        };

        let mut q = match self.ring.entries.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        };
        if q.len() == CAPACITY {
            q.pop_front();
        }
        q.push_back(entry);
    }
}

fn current_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// Collects event fields into a `serde_json::Map`; `message` is split out into
/// its own column (same convention as `events.jsonl`).
#[derive(Default)]
struct FieldVisitor {
    message: String,
    fields: serde_json::Map<String, serde_json::Value>,
}

impl Visit for FieldVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        let key = field.name();
        let formatted = format!("{value:?}");
        if key == "message" {
            self.message = formatted;
        } else {
            self.fields
                .insert(key.to_string(), serde_json::Value::String(formatted));
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        let key = field.name();
        if key == "message" {
            self.message = value.to_string();
        } else {
            self.fields.insert(
                key.to_string(),
                serde_json::Value::String(value.to_string()),
            );
        }
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        self.fields
            .insert(field.name().to_string(), serde_json::Value::from(value));
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        self.fields
            .insert(field.name().to_string(), serde_json::Value::from(value));
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        self.fields
            .insert(field.name().to_string(), serde_json::Value::from(value));
    }
}

#[cfg(test)]
#[cfg_attr(coverage_nightly, coverage(off))]
mod tests {
    use super::*;
    use tracing::subscriber::with_default;
    use tracing_subscriber::Registry;
    use tracing_subscriber::layer::SubscriberExt;

    fn subscriber() -> (EventRing, impl tracing::Subscriber) {
        let ring = EventRing::new();
        let subscriber = Registry::default().with(RingBufferLayer::new(ring.clone()));
        (ring, subscriber)
    }

    #[test]
    fn ring_capacity_caps_at_256_entries() {
        let (ring, subscriber) = subscriber();
        with_default(subscriber, || {
            for i in 0..300 {
                tracing::info!(idx = i, "fill");
            }
        });
        assert_eq!(ring.len(), CAPACITY);
    }

    #[test]
    fn ring_oldest_entries_evicted_first() {
        let (ring, subscriber) = subscriber();
        with_default(subscriber, || {
            for i in 0..CAPACITY + 50 {
                tracing::info!(idx = i, "fill");
            }
        });
        let tail = ring.snapshot_tail(CAPACITY);
        // Oldest is idx=50 (0..50 evicted).
        let first_idx = tail
            .first()
            .and_then(|e| e.fields.get("idx"))
            .and_then(serde_json::Value::as_i64);
        assert_eq!(first_idx, Some(50));
        let last_idx = tail
            .last()
            .and_then(|e| e.fields.get("idx"))
            .and_then(serde_json::Value::as_i64);
        let expected_last = i64::try_from(CAPACITY + 50 - 1).expect("fits in i64");
        assert_eq!(last_idx, Some(expected_last));
    }

    #[test]
    fn snapshot_tail_returns_at_most_n_entries() {
        let (ring, subscriber) = subscriber();
        with_default(subscriber, || {
            for i in 0..10 {
                tracing::info!(idx = i, "fill");
            }
        });
        let tail = ring.snapshot_tail(5);
        assert_eq!(tail.len(), 5);
        // Last 5 are idx=5..10.
        let first_idx = tail.first().unwrap().fields.get("idx").unwrap().as_i64();
        assert_eq!(first_idx, Some(5));
    }

    #[test]
    fn message_field_is_extracted_separately() {
        let (ring, subscriber) = subscriber();
        with_default(subscriber, || {
            tracing::info!(key = "value", "hello world");
        });
        let tail = ring.snapshot_tail(1);
        assert_eq!(tail.len(), 1);
        let entry = &tail[0];
        assert_eq!(entry.message, "hello world");
        assert_eq!(
            entry.fields.get("key").and_then(|v| v.as_str()),
            Some("value")
        );
        // message is not duplicated into fields.
        assert!(entry.fields.get("message").is_none());
    }

    #[test]
    fn entry_records_level_and_target() {
        let (ring, subscriber) = subscriber();
        with_default(subscriber, || {
            tracing::warn!(target: "test_subsystem", "warn level event");
        });
        let tail = ring.snapshot_tail(1);
        let entry = &tail[0];
        assert_eq!(entry.level, "WARN");
        assert_eq!(entry.target, "test_subsystem");
    }

    /// event -> ring -> snapshot -> serialize -> deserialize preserves contents.
    #[test]
    fn ring_snapshot_round_trips_through_serde_json() {
        #[derive(serde::Deserialize)]
        struct ReadEntry {
            level: String,
            target: String,
            message: String,
            fields: serde_json::Value,
        }

        let (ring, subscriber) = subscriber();
        with_default(subscriber, || {
            tracing::warn!(target: "crash_dump_integration", key = "value", "panic-adjacent");
            tracing::info!(target: "crash_dump_integration", "after");
        });

        let tail = ring.snapshot_tail(64);
        assert_eq!(tail.len(), 2, "expected exactly 2 entries in the snapshot");

        let json = serde_json::to_string(&tail).expect("serialize ring snapshot");
        assert!(json.contains("panic-adjacent"));
        assert!(json.contains("crash_dump_integration"));

        let parsed: Vec<ReadEntry> = serde_json::from_str(&json).expect("round-trip");
        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].level, "WARN");
        assert_eq!(parsed[0].target, "crash_dump_integration");
        assert_eq!(parsed[0].message, "panic-adjacent");
        assert_eq!(
            parsed[0].fields.get("key").and_then(|v| v.as_str()),
            Some("value")
        );
        assert_eq!(parsed[1].level, "INFO");
        assert_eq!(parsed[1].message, "after");
    }

    /// A zero-length snapshot returns an empty `Vec` and does not panic.
    #[test]
    fn snapshot_tail_zero_returns_empty_vec() {
        let (ring, subscriber) = subscriber();
        with_default(subscriber, || {
            tracing::info!("a");
            tracing::info!("b");
        });
        let tail = ring.snapshot_tail(0);
        assert!(tail.is_empty(), "snapshot_tail(0) should yield empty Vec");
    }

    #[test]
    fn poisoned_ring_lock_is_recovered_for_new_events() {
        let (ring, subscriber) = subscriber();
        let poison_target = ring.entries.clone();
        let _ = std::thread::spawn(move || {
            let _guard = poison_target.lock().expect("lock before poison");
            panic!("poison fixture");
        })
        .join();

        with_default(subscriber, || {
            tracing::info!(message = "after poison");
        });
        let tail = ring.snapshot_tail(1);
        assert_eq!(tail.len(), 1);
        assert_eq!(tail[0].message, "after poison");
    }
}
