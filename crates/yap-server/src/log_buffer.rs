//! Ring buffer that captures tracing events for the debug log panel.
//!
//! Usage:
//! 1. Create a shared `LogBuffer` with `LogBuffer::new(capacity)`
//! 2. Install `BufferLayer::new(buffer.clone())` into the tracing subscriber
//! 3. Read entries via `buffer.entries_since(last_id)` from API handlers

use std::collections::VecDeque;
use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;

/// A single captured log entry.
#[derive(Clone, Serialize)]
#[cfg_attr(feature = "openapi", derive(utoipa::ToSchema))]
pub struct LogEntry {
    pub id: u64,
    pub timestamp: String,
    pub level: String,
    pub target: String,
    pub message: String,
}

/// Thread-safe ring buffer of log entries.
pub struct LogBuffer {
    entries: Mutex<VecDeque<LogEntry>>,
    max_size: usize,
    next_id: AtomicU64,
}

impl LogBuffer {
    pub fn new(max_size: usize) -> Arc<Self> {
        Arc::new(Self {
            entries: Mutex::new(VecDeque::with_capacity(max_size)),
            max_size,
            next_id: AtomicU64::new(1),
        })
    }

    pub fn push(&self, level: &str, target: &str, message: String) {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let timestamp = chrono::Utc::now().to_rfc3339_opts(chrono::SecondsFormat::Millis, true);
        let entry = LogEntry {
            id,
            timestamp,
            level: level.to_string(),
            target: target.to_string(),
            message,
        };
        let mut entries = self.entries.lock().unwrap();
        if entries.len() >= self.max_size {
            entries.pop_front();
        }
        entries.push_back(entry);
    }

    /// Return all entries with id > since_id.
    pub fn entries_since(&self, since_id: u64) -> Vec<LogEntry> {
        let entries = self.entries.lock().unwrap();
        entries
            .iter()
            .filter(|e| e.id > since_id)
            .cloned()
            .collect()
    }
}

// ---------------------------------------------------------------------------
// Tracing subscriber layer
// ---------------------------------------------------------------------------

/// A tracing Layer that captures formatted events into a LogBuffer.
pub struct BufferLayer {
    buffer: Arc<LogBuffer>,
}

impl BufferLayer {
    pub fn new(buffer: Arc<LogBuffer>) -> Self {
        Self { buffer }
    }
}

impl<S> tracing_subscriber::Layer<S> for BufferLayer
where
    S: tracing::Subscriber,
{
    fn on_event(
        &self,
        event: &tracing::Event<'_>,
        _ctx: tracing_subscriber::layer::Context<'_, S>,
    ) {
        let metadata = event.metadata();
        let level = metadata.level().as_str();
        let target = metadata.target();

        // Extract the message field from the event
        let mut visitor = MessageVisitor(String::new());
        event.record(&mut visitor);

        self.buffer.push(level, target, visitor.0);
    }
}

/// Simple visitor that concatenates all fields into a message string.
struct MessageVisitor(String);

impl tracing::field::Visit for MessageVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn fmt::Debug) {
        if field.name() == "message" {
            self.0 = format!("{:?}", value);
            // Remove surrounding quotes from Debug formatting
            if self.0.starts_with('"') && self.0.ends_with('"') {
                self.0 = self.0[1..self.0.len() - 1].to_string();
            }
        } else if !self.0.is_empty() {
            self.0.push_str(&format!(" {}={:?}", field.name(), value));
        } else {
            self.0 = format!("{}={:?}", field.name(), value);
        }
    }

    fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
        if field.name() == "message" {
            self.0 = value.to_string();
        } else if !self.0.is_empty() {
            self.0.push_str(&format!(" {}={}", field.name(), value));
        } else {
            self.0 = format!("{}={}", field.name(), value);
        }
    }

    fn record_u64(&mut self, field: &tracing::field::Field, value: u64) {
        if !self.0.is_empty() {
            self.0.push_str(&format!(" {}={}", field.name(), value));
        } else {
            self.0 = format!("{}={}", field.name(), value);
        }
    }

    fn record_i64(&mut self, field: &tracing::field::Field, value: i64) {
        if !self.0.is_empty() {
            self.0.push_str(&format!(" {}={}", field.name(), value));
        } else {
            self.0 = format!("{}={}", field.name(), value);
        }
    }

    fn record_bool(&mut self, field: &tracing::field::Field, value: bool) {
        if !self.0.is_empty() {
            self.0.push_str(&format!(" {}={}", field.name(), value));
        } else {
            self.0 = format!("{}={}", field.name(), value);
        }
    }
}
