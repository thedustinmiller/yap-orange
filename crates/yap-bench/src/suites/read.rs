use async_trait::async_trait;
use uuid::Uuid;
use web_time::Instant;
use yap_core::Store;

use crate::data::DataGen;
use crate::results::{BenchmarkResult, SuiteResult};

use super::BenchSuite;

pub struct ReadSuite;

#[async_trait]
impl BenchSuite for ReadSuite {
    fn name(&self) -> &str {
        "read_throughput"
    }

    fn description(&self) -> &str {
        "Measures read speed: get_block, get_block_children, get_block_with_atom"
    }

    async fn run(&self, store: &dyn Store, test_root_id: Uuid, seed: u64) -> SuiteResult {
        let suite_start = Instant::now();
        let mut benchmarks = Vec::new();
        let mut dg = DataGen::new(seed);

        // Setup: create parent + 50 children
        let (parent, _) = store
            .create_block_with_content(
                Some(test_root_id),
                "read_parent",
                "",
                &[],
                "namespace",
                &serde_json::json!({}),
            )
            .await
            .expect("create read parent");

        let mut block_ids = Vec::new();
        for _ in 0..50 {
            let name = dg.block_name();
            let content = dg.content(100);
            let (block, _) = store
                .create_block_with_content(
                    Some(parent.id),
                    &name,
                    &content,
                    &[],
                    "content",
                    &dg.properties(),
                )
                .await
                .expect("create read child");
            block_ids.push(block.id);
        }

        // Bench: get_block x 50
        let start = Instant::now();
        for &id in &block_ids {
            store.get_block(id).await.expect("get_block");
        }
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        benchmarks.push(BenchmarkResult::new("get_block_x50", elapsed, 50));

        // Bench: get_block_children on parent with 50 children
        let start = Instant::now();
        let iterations = 10u64;
        for _ in 0..iterations {
            store
                .get_block_children(parent.id)
                .await
                .expect("get_block_children");
        }
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        benchmarks.push(BenchmarkResult::with_metadata(
            "get_block_children_50",
            elapsed,
            iterations,
            serde_json::json!({ "child_count": 50 }),
        ));

        // Bench: get_block_with_atom x 50
        let start = Instant::now();
        for &id in &block_ids {
            store
                .get_block_with_atom(id)
                .await
                .expect("get_block_with_atom");
        }
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        benchmarks.push(BenchmarkResult::new("get_block_with_atom_x50", elapsed, 50));

        SuiteResult {
            name: self.name().to_string(),
            description: self.description().to_string(),
            duration_ms: suite_start.elapsed().as_secs_f64() * 1000.0,
            benchmarks,
        }
    }
}
