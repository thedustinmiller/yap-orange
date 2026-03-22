use async_trait::async_trait;
use uuid::Uuid;
use web_time::Instant;
use yap_core::Store;

use crate::data::DataGen;
use crate::results::{BenchmarkResult, SuiteResult};

use super::BenchSuite;

pub struct SearchSuite;

#[async_trait]
impl BenchSuite for SearchSuite {
    fn name(&self) -> &str {
        "search_performance"
    }

    fn description(&self) -> &str {
        "Measures search_blocks speed across 100 blocks with varied names"
    }

    async fn run(&self, store: &dyn Store, test_root_id: Uuid, seed: u64) -> SuiteResult {
        let suite_start = Instant::now();
        let mut benchmarks = Vec::new();
        let mut dg = DataGen::new(seed);

        // Setup: create 100 blocks with searchable names
        let (parent, _) = store
            .create_block_with_content(
                Some(test_root_id),
                "search_parent",
                "",
                &[],
                "namespace",
                &serde_json::json!({}),
            )
            .await
            .expect("create search parent");

        for i in 0..100 {
            let name = dg.searchable_name(i);
            let content = dg.content(80);
            store
                .create_block_with_content(
                    Some(parent.id),
                    &name,
                    &content,
                    &[],
                    "content",
                    &dg.properties(),
                )
                .await
                .expect("create search block");
        }

        // Bench: search_blocks x 10
        let iterations = 10u64;
        let start = Instant::now();
        for _ in 0..iterations {
            store
                .search_blocks("bench_term")
                .await
                .expect("search_blocks");
        }
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        benchmarks.push(BenchmarkResult::with_metadata(
            "search_blocks_x10",
            elapsed,
            iterations,
            serde_json::json!({ "query": "bench_term", "corpus_size": 100 }),
        ));

        SuiteResult {
            name: self.name().to_string(),
            description: self.description().to_string(),
            duration_ms: suite_start.elapsed().as_secs_f64() * 1000.0,
            benchmarks,
        }
    }
}
