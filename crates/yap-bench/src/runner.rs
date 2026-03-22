use serde::{Deserialize, Serialize};
use web_time::Instant;
use yap_core::Store;

use crate::results::BenchmarkResults;
use crate::suites::all_suites;

/// Configuration for a benchmark run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchmarkConfig {
    /// Which suites to run (empty = all).
    #[serde(default)]
    pub suites: Vec<String>,

    /// Random seed for deterministic data generation.
    #[serde(default = "default_seed")]
    pub seed: u64,
}

fn default_seed() -> u64 {
    42
}

impl Default for BenchmarkConfig {
    fn default() -> Self {
        Self {
            suites: Vec::new(),
            seed: default_seed(),
        }
    }
}

/// Run benchmarks against the given store.
///
/// Creates a `test` root namespace, runs selected suites, then cleans up
/// via soft-delete. All operations use the Store trait — no backend-specific code.
pub async fn run_benchmarks(store: &dyn Store, config: BenchmarkConfig) -> BenchmarkResults {
    let started_at = chrono::Utc::now();
    let start = Instant::now();

    // Clean up any existing test namespace from a prior run
    if let Ok(Some(block)) = store.find_block_by_namespace("test").await {
        let _ = store.delete_block_recursive(block.id).await;
    }

    // Create fresh test root
    let (test_root, _) = store
        .create_block_with_content(None, "test", "", &[], "namespace", &serde_json::json!({}))
        .await
        .expect("create test root for benchmarks");

    // Determine which suites to run
    let all = all_suites();
    let suites_to_run: Vec<_> = if config.suites.is_empty() {
        all.iter().collect()
    } else {
        all.iter()
            .filter(|s| config.suites.contains(&s.name().to_string()))
            .collect()
    };

    // Run suites sequentially
    let mut suite_results = Vec::new();
    for suite in suites_to_run {
        suite_results.push(suite.run(store, test_root.id, config.seed).await);
    }

    // Clean up test namespace
    let _ = store.delete_block_recursive(test_root.id).await;

    let total_duration = start.elapsed();

    BenchmarkResults {
        started_at,
        completed_at: chrono::Utc::now(),
        total_duration_ms: total_duration.as_secs_f64() * 1000.0,
        suites: suite_results,
    }
}
