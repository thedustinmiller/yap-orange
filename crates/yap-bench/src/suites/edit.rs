use async_trait::async_trait;
use uuid::Uuid;
use web_time::Instant;
use yap_core::Store;

use crate::data::DataGen;
use crate::results::{BenchmarkResult, SuiteResult};

use super::BenchSuite;

pub struct EditSuite;

#[async_trait]
impl BenchSuite for EditSuite {
    fn name(&self) -> &str {
        "edit_throughput"
    }

    fn description(&self) -> &str {
        "Measures edit_lineage speed: update content on 50 existing lineages"
    }

    async fn run(&self, store: &dyn Store, test_root_id: Uuid, seed: u64) -> SuiteResult {
        let suite_start = Instant::now();
        let mut benchmarks = Vec::new();
        let mut dg = DataGen::new(seed);

        // Setup: create parent + 50 blocks
        let (parent, _) = store
            .create_block_with_content(
                Some(test_root_id),
                "edit_parent",
                "",
                &[],
                "namespace",
                &serde_json::json!({}),
            )
            .await
            .expect("create edit parent");

        let mut lineage_ids = Vec::new();
        for _ in 0..50 {
            let name = dg.block_name();
            let content = dg.content(80);
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
                .expect("create edit child");
            lineage_ids.push(block.lineage_id);
        }

        // Bench: edit_lineage x 50
        let start = Instant::now();
        for &lineage_id in &lineage_ids {
            let new_content = dg.content(120);
            let new_props = dg.properties();
            store
                .edit_lineage(lineage_id, "content", &new_content, &[], &new_props)
                .await
                .expect("edit_lineage");
        }
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        benchmarks.push(BenchmarkResult::new("edit_lineage_x50", elapsed, 50));

        SuiteResult {
            name: self.name().to_string(),
            description: self.description().to_string(),
            duration_ms: suite_start.elapsed().as_secs_f64() * 1000.0,
            benchmarks,
        }
    }
}
