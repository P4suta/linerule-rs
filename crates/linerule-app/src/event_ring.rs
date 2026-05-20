//! 直近 N 件の tracing event を in-memory ring buffer に貯めるレイヤと、その
//! snapshot を panic hook から読み取る API。
//!
//! 用途: panic 発生時に `crash_dump::CrashRecord::recent_events` に同梱する
//! ことで「panic に至るまで直前に何が起きていたか」を `events.jsonl` を手で
//! 漁らずに再構成できるようにする。`events.jsonl` には全 event が出るが、
//! crash JSON にも直近 64 件を抱き合わせることで grep / jq の手数を削減。
//!
//! ## 設計判断
//!
//! - `static OnceLock<Mutex<VecDeque<RingEntry>>>` で global state。panic hook
//!   (`'static` lifetime) からアクセスする必要があるため。
//! - capacity 256 entry × ~1 KB = ~256 KB heap (環境依存)。`env_filter` で
//!   `warn` 以上に絞れば release ビルドで実質ゼロ。
//! - panic 中の lock poisoning は `PoisonError::into_inner()` で奪取する。
//!   失敗時は空 tail を返して crash dump 自体は残す方針。
//! - `RingBufferLayer` は `Send + Sync` を `Mutex + OnceLock` 経由で自動充足。

#![forbid(unsafe_code)]

use std::collections::VecDeque;
use std::sync::{Mutex, OnceLock};

use serde::Serialize;
use tracing::{Event, Subscriber};
use tracing_subscriber::field::Visit;
use tracing_subscriber::layer::{Context, Layer};

/// Ring buffer の上限。1 frame ~16ms 単位で event が出るとして 256 entry ≒ 4 秒分
/// の文脈を保持する。`env_filter` で `warn` 以上に絞ればもっと長い窓になる。
const CAPACITY: usize = 256;

/// 1 つの tracing event の snapshot。crash dump にそのまま埋め込めるよう
/// `Serialize` を実装する。
#[derive(Debug, Clone, Serialize)]
pub(crate) struct RingEntry {
    /// Unix epoch からの ms (panic 後の post-mortem 解析用)。
    pub(crate) unix_ms: i64,
    /// `tracing::Level` を文字列化したもの。
    pub(crate) level: String,
    /// `event.metadata().target()`。subsystem 絞り込み用。
    pub(crate) target: String,
    /// event の message field (`tracing::info!("text")` の "text" 部分)。
    pub(crate) message: String,
    /// その他 fields を JSON Object として並べたもの。
    pub(crate) fields: serde_json::Value,
}

static RING: OnceLock<Mutex<VecDeque<RingEntry>>> = OnceLock::new();

fn ring() -> &'static Mutex<VecDeque<RingEntry>> {
    RING.get_or_init(|| Mutex::new(VecDeque::with_capacity(CAPACITY)))
}

/// 直近 `n` 件の `RingEntry` を新しい順から古い順で並べた snapshot を返す。
/// panic hook から呼ばれる。lock 取得に失敗した (panic 中の同 thread が
/// 取りに来ている等) ケースでは `PoisonError::into_inner()` で奪取する。
pub(crate) fn snapshot_tail(n: usize) -> Vec<RingEntry> {
    let guard = match ring().lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    let len = guard.len();
    let start = len.saturating_sub(n);
    guard.iter().skip(start).cloned().collect()
}

/// 現在 ring に積まれている entry 数を返す (test 用 helper)。
#[cfg(test)]
pub(crate) fn len() -> usize {
    ring().lock().map_or(0, |g| g.len())
}

/// Ring buffer に event を push する `tracing_subscriber::Layer`。registry に
/// `.with(RingBufferLayer)` で追加する。
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

/// `tracing_subscriber::Visit` 実装で event の fields を `serde_json::Map` に
/// 取り出す。`message` field だけは特別扱いして専用 column に詰める (`events.jsonl`
/// と同じ慣行)。
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

    /// `static` ring buffer はテスト間で共有されるので、必要なら最初にクリアする。
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
        // 最古は idx=50 から (0..50 が evict されているはず)
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
        // 末尾 5 件は idx=5..10
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
        // message は fields の中には出ない
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
}
