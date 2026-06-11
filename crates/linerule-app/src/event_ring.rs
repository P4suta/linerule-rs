//! A layer that keeps the last N tracing events in an in-memory ring buffer,
//! plus an API for the panic hook to read a snapshot.
//!
//! Purpose: bundle these into `crash_dump::CrashRecord::recent_events` on panic
//! so the lead-up can be reconstructed without grepping `events.jsonl`.
//!
//! Design notes:
//! - Global state via `static OnceLock<Mutex<VecDeque<RingEntry>>>` because the
//!   panic hook (`'static`) must reach it.
//! - Capacity 256 entries; with `env_filter` set to `warn`+ this is near zero
//!   in release.
//! - On lock poisoning during a panic, take the inner via
//!   `PoisonError::into_inner()`; on other failure return an empty tail so the
//!   crash dump is still written.
//! - `RingBufferLayer` is `Send + Sync` via `Mutex + OnceLock`.

#![forbid(unsafe_code)]

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

use serde::Serialize;
use tracing::{Event, Subscriber};
use tracing_subscriber::field::Visit;
use tracing_subscriber::layer::{Context, Layer};

/// Ring buffer cap. At ~16ms per frame, 256 entries holds roughly 4s of
/// context; an `env_filter` of `warn`+ widens the window.
const CAPACITY: usize = 256;

/// Snapshot of one tracing event. `Serialize` so it embeds directly into the
/// crash dump.
#[derive(Debug, Clone, Serialize)]
pub(crate) struct RingEntry {
    /// Milliseconds since the Unix epoch.
    pub(crate) unix_ms: i64,
    /// Stringified `tracing::Level`.
    pub(crate) level: String,
    /// `event.metadata().target()`, for subsystem filtering.
    pub(crate) target: String,
    /// The event's message field.
    pub(crate) message: String,
    /// Remaining fields as a JSON object.
    pub(crate) fields: serde_json::Value,
}

static RING: OnceLock<Mutex<VecDeque<RingEntry>>> = OnceLock::new();

fn ring() -> &'static Mutex<VecDeque<RingEntry>> {
    RING.get_or_init(|| Mutex::new(VecDeque::with_capacity(CAPACITY)))
}

/// Return a snapshot of the last `n` `RingEntry`s in oldest-to-newest order.
/// Called from the panic hook; takes the inner via `PoisonError::into_inner()`
/// when the lock is poisoned.
pub(crate) fn snapshot_tail(n: usize) -> Vec<RingEntry> {
    let guard = match ring().lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    let len = guard.len();
    let start = len.saturating_sub(n);
    guard.iter().skip(start).cloned().collect()
}

/// Number of entries currently in the ring (test helper).
#[cfg(test)]
pub(crate) fn len() -> usize {
    ring().lock().map_or(0, |g| g.len())
}

/// `tracing_subscriber::Layer` that pushes events into the ring buffer. Add via
/// `.with(RingBufferLayer)` on the registry.
pub(crate) struct RingBufferLayer;

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

        if let Ok(mut q) = ring().lock() {
            if q.len() == CAPACITY {
                q.pop_front();
            }
            q.push_back(entry);
        }
    }
}

fn current_unix_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

/// `tracing_subscriber::Visit` impl that collects event fields into a
/// `serde_json::Map`. The `message` field is pulled into its own column (same
/// convention as `events.jsonl`).
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
mod tests {
    use super::*;
    use tracing::subscriber::with_default;
    use tracing_subscriber::Registry;
    use tracing_subscriber::layer::SubscriberExt;

    /// The `static` ring is shared across tests; clear it first when needed.
    fn clear_ring() {
        if let Ok(mut q) = ring().lock() {
            q.clear();
        }
    }

    #[test]
    fn ring_capacity_caps_at_256_entries() {
        clear_ring();
        let subscriber = Registry::default().with(RingBufferLayer);
        with_default(subscriber, || {
            for i in 0..300 {
                tracing::info!(idx = i, "fill");
            }
        });
        assert_eq!(len(), CAPACITY);
    }

    #[test]
    fn ring_oldest_entries_evicted_first() {
        clear_ring();
        let subscriber = Registry::default().with(RingBufferLayer);
        with_default(subscriber, || {
            for i in 0..CAPACITY + 50 {
                tracing::info!(idx = i, "fill");
            }
        });
        let tail = snapshot_tail(CAPACITY);
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
        clear_ring();
        let subscriber = Registry::default().with(RingBufferLayer);
        with_default(subscriber, || {
            for i in 0..10 {
                tracing::info!(idx = i, "fill");
            }
        });
        let tail = snapshot_tail(5);
        assert_eq!(tail.len(), 5);
        // Last 5 are idx=5..10.
        let first_idx = tail.first().unwrap().fields.get("idx").unwrap().as_i64();
        assert_eq!(first_idx, Some(5));
    }

    #[test]
    fn message_field_is_extracted_separately() {
        clear_ring();
        let subscriber = Registry::default().with(RingBufferLayer);
        with_default(subscriber, || {
            tracing::info!(key = "value", "hello world");
        });
        let tail = snapshot_tail(1);
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
        clear_ring();
        let subscriber = Registry::default().with(RingBufferLayer);
        with_default(subscriber, || {
            tracing::warn!(target: "test_subsystem", "warn level event");
        });
        let tail = snapshot_tail(1);
        let entry = &tail[0];
        assert_eq!(entry.level, "WARN");
        assert_eq!(entry.target, "test_subsystem");
    }

    /// The ring feeds `crash_dump`'s `recent_events`. Check the
    /// event → ring → [`snapshot_tail`] → serialize → deserialize round-trip
    /// preserves contents.
    #[test]
    fn ring_snapshot_round_trips_through_serde_json() {
        #[derive(serde::Deserialize)]
        struct ReadEntry {
            level: String,
            target: String,
            message: String,
            fields: serde_json::Value,
        }

        clear_ring();
        let subscriber = Registry::default().with(RingBufferLayer);
        with_default(subscriber, || {
            tracing::warn!(target: "crash_dump_integration", key = "value", "panic-adjacent");
            tracing::info!(target: "crash_dump_integration", "after");
        });

        let tail = snapshot_tail(64);
        assert_eq!(tail.len(), 2, "expected exactly 2 entries in the snapshot");

        // Serialize ring entries (same shape as CrashRecord::recent_events).
        let json = serde_json::to_string(&tail).expect("serialize ring snapshot");
        assert!(json.contains("panic-adjacent"));
        assert!(json.contains("crash_dump_integration"));

        // Deserialize via a structurally-equivalent shape, compatible with
        // CrashRecord: level / target / message / fields read back.
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

    /// [`snapshot_tail`] with `0` returns an empty `Vec` and does not panic.
    #[test]
    fn snapshot_tail_zero_returns_empty_vec() {
        clear_ring();
        let subscriber = Registry::default().with(RingBufferLayer);
        with_default(subscriber, || {
            tracing::info!("a");
            tracing::info!("b");
        });
        let tail = snapshot_tail(0);
        assert!(tail.is_empty(), "snapshot_tail(0) should yield empty Vec");
    }
}
