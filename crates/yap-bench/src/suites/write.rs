use std::future::Future;
use std::pin::Pin;

use async_trait::async_trait;
use uuid::Uuid;
use web_time::Instant;
use yap_core::Store;

use crate::data::DataGen;
use crate::results::{BenchmarkResult, SuiteResult};

use super::BenchSuite;

pub struct WriteSuite;

#[async_trait]
impl BenchSuite for WriteSuite {
    fn name(&self) -> &str {
        "write_throughput"
    }

    fn description(&self) -> &str {
        "Measures block creation speed: flat (100 blocks) and nested (depth 5, branching 3)"
    }

    async fn run(&self, store: &dyn Store, test_root_id: Uuid, seed: u64) -> SuiteResult {
        let suite_start = Instant::now();
        let mut benchmarks = Vec::new();
        let mut dg = DataGen::new(seed);

        // --- Flat: 100 blocks under one parent ---
        let (flat_parent, _) = store
            .create_block_with_content(
                Some(test_root_id),
                "write_flat",
                "",
                &[],
                "namespace",
                &serde_json::json!({}),
            )
            .await
            .expect("create write_flat namespace");

        let flat_count = 100u64;
        let start = Instant::now();
        for _ in 0..flat_count {
            let name = dg.block_name();
            let content = dg.content(80);
            let props = dg.properties();
            store
                .create_block_with_content(
                    Some(flat_parent.id),
                    &name,
                    &content,
                    &[],
                    "content",
                    &props,
                )
                .await
                .expect("create flat block");
        }
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        benchmarks.push(BenchmarkResult::new("create_flat_100", elapsed, flat_count));

        // --- Nested: depth 5, branching factor 3 (3^0 + 3^1 + ... + 3^4 = 121 internal + 243 leaves isn't right)
        // Actually: 1 root + 3 + 9 + 27 + 81 + 243 = 364 total at depth 5 with branching 3
        // But we'll just do depth 5 with 3 children per level = (3^5 - 1) / (3-1) = 121 nodes under the root
        let (nested_parent, _) = store
            .create_block_with_content(
                Some(test_root_id),
                "write_nested",
                "",
                &[],
                "namespace",
                &serde_json::json!({}),
            )
            .await
            .expect("create write_nested namespace");

        let start = Instant::now();
        let nested_ops = create_tree(store, &mut dg, nested_parent.id, 5, 3).await;
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        benchmarks.push(BenchmarkResult::with_metadata(
            "create_nested_depth5_branch3",
            elapsed,
            nested_ops,
            serde_json::json!({ "depth": 5, "branching_factor": 3, "total_blocks": nested_ops }),
        ));

        SuiteResult {
            name: self.name().to_string(),
            description: self.description().to_string(),
            duration_ms: suite_start.elapsed().as_secs_f64() * 1000.0,
            benchmarks,
        }
    }
}

/// Recursively create a tree with given depth and branching factor.
/// Returns the total number of blocks created.
///
/// Uses `Pin<Box<...>>` because recursive async fns require indirection
/// to avoid infinitely-sized future types.
fn create_tree<'a>(
    store: &'a dyn Store,
    dg: &'a mut DataGen,
    parent_id: Uuid,
    depth: u32,
    branching: u32,
) -> Pin<Box<dyn Future<Output = u64> + Send + 'a>> {
    Box::pin(async move {
        if depth == 0 {
            return 0;
        }
        let mut count = 0u64;
        for _ in 0..branching {
            let name = dg.block_name();
            let content = dg.content(60);
            let (block, _) = store
                .create_block_with_content(
                    Some(parent_id),
                    &name,
                    &content,
                    &[],
                    "content",
                    &dg.properties(),
                )
                .await
                .expect("create nested block");
            count += 1;
            count += create_tree(store, dg, block.id, depth - 1, branching).await;
        }
        count
    })
}
