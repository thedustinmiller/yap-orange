use async_trait::async_trait;
use uuid::Uuid;
use web_time::Instant;
use yap_core::Store;

use crate::data::DataGen;
use crate::results::{BenchmarkResult, SuiteResult};

use super::BenchSuite;

pub struct LinksSuite;

#[async_trait]
impl BenchSuite for LinksSuite {
    fn name(&self) -> &str {
        "links_backlinks"
    }

    fn description(&self) -> &str {
        "Measures get_backlinks and count_backlinks with N=10 and N=50 linkers"
    }

    async fn run(&self, store: &dyn Store, test_root_id: Uuid, seed: u64) -> SuiteResult {
        let suite_start = Instant::now();
        let mut benchmarks = Vec::new();
        let mut dg = DataGen::new(seed);

        // Create a target block that others will link to
        let (target_block, _) = store
            .create_block_with_content(
                Some(test_root_id),
                "link_target",
                "I am the link target",
                &[],
                "content",
                &serde_json::json!({}),
            )
            .await
            .expect("create link target");

        let target_lineage_id = target_block.lineage_id;

        for &link_count in &[10u64, 50] {
            // Create a sub-namespace for this batch
            let (linkers_parent, _) = store
                .create_block_with_content(
                    Some(test_root_id),
                    &format!("linkers_{link_count}"),
                    "",
                    &[],
                    "namespace",
                    &serde_json::json!({}),
                )
                .await
                .expect("create linkers parent");

            // Create N blocks that link to the target
            for i in 0..link_count {
                let name = dg.block_name();
                let template = format!("links to target via {{0}} item {i}");
                store
                    .create_block_with_content(
                        Some(linkers_parent.id),
                        &name,
                        &template,
                        &[target_lineage_id],
                        "content",
                        &serde_json::json!({}),
                    )
                    .await
                    .expect("create linker block");
            }

            // Bench: get_backlinks
            let iterations = 10u64;
            let start = Instant::now();
            for _ in 0..iterations {
                store
                    .get_backlinks(target_lineage_id)
                    .await
                    .expect("get_backlinks");
            }
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            benchmarks.push(BenchmarkResult::with_metadata(
                format!("get_backlinks_n{link_count}"),
                elapsed,
                iterations,
                serde_json::json!({ "linker_count": link_count }),
            ));

            // Bench: count_backlinks
            let start = Instant::now();
            for _ in 0..iterations {
                store
                    .count_backlinks(target_lineage_id)
                    .await
                    .expect("count_backlinks");
            }
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            benchmarks.push(BenchmarkResult::with_metadata(
                format!("count_backlinks_n{link_count}"),
                elapsed,
                iterations,
                serde_json::json!({ "linker_count": link_count }),
            ));
        }

        SuiteResult {
            name: self.name().to_string(),
            description: self.description().to_string(),
            duration_ms: suite_start.elapsed().as_secs_f64() * 1000.0,
            benchmarks,
        }
    }
}
