use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

/// Top-level benchmark run results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResults {
    pub started_at: DateTime<Utc>,
    pub completed_at: DateTime<Utc>,
    pub total_duration_ms: f64,
    pub suites: Vec<SuiteResult>,
}

/// Results for a single benchmark suite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SuiteResult {
    pub name: String,
    pub description: String,
    pub duration_ms: f64,
    pub benchmarks: Vec<BenchmarkResult>,
}

/// Results for a single benchmark within a suite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkResult {
    pub name: String,
    pub duration_ms: f64,
    pub ops: u64,
    pub ops_per_sec: f64,
    pub metadata: serde_json::Value,
}

impl BenchmarkResult {
    /// Create a benchmark result from timing data.
    pub fn new(name: impl Into<String>, duration_ms: f64, ops: u64) -> Self {
        let ops_per_sec = if duration_ms > 0.0 {
            ops as f64 / (duration_ms / 1000.0)
        } else {
            0.0
        };
        Self {
            name: name.into(),
            duration_ms,
            ops,
            ops_per_sec,
            metadata: serde_json::json!({}),
        }
    }

    /// Create a benchmark result with additional metadata.
    pub fn with_metadata(
        name: impl Into<String>,
        duration_ms: f64,
        ops: u64,
        metadata: serde_json::Value,
    ) -> Self {
        let ops_per_sec = if duration_ms > 0.0 {
            ops as f64 / (duration_ms / 1000.0)
        } else {
            0.0
        };
        Self {
            name: name.into(),
            duration_ms,
            ops,
            ops_per_sec,
            metadata,
        }
    }
}
