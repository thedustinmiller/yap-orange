use async_trait::async_trait;
use uuid::Uuid;
use web_time::Instant;
use yap_core::Store;

use crate::data::DataGen;
use crate::results::{BenchmarkResult, SuiteResult};

use super::BenchSuite;

pub struct HierarchySuite;

#[async_trait]
impl BenchSuite for HierarchySuite {
    fn name(&self) -> &str {
        "hierarchy_operations"
    }

    fn description(&self) -> &str {
        "Measures move_block and delete_block_recursive on subtrees of size 10, 50"
    }

    async fn run(&self, store: &dyn Store, test_root_id: Uuid, seed: u64) -> SuiteResult {
        let suite_start = Instant::now();
        let mut benchmarks = Vec::new();
        let mut dg = DataGen::new(seed);

        for &subtree_size in &[10u64, 50] {
            // Create source subtree
            let (src, _) = store
                .create_block_with_content(
                    Some(test_root_id),
                    &format!("hier_src_{subtree_size}"),
                    "",
                    &[],
                    "namespace",
                    &serde_json::json!({}),
                )
                .await
                .expect("create src");

            // Create destination for moves
            let (dst, _) = store
                .create_block_with_content(
                    Some(test_root_id),
                    &format!("hier_dst_{subtree_size}"),
                    "",
                    &[],
                    "namespace",
                    &serde_json::json!({}),
                )
                .await
                .expect("create dst");

            // Populate source with children
            let breadth = (subtree_size as f64).sqrt().max(1.0) as u64;
            let per_branch = subtree_size / breadth;

            let mut branch_ids = Vec::new();
            for _ in 0..breadth {
                let name = dg.block_name();
                let (block, _) = store
                    .create_block_with_content(
                        Some(src.id),
                        &name,
                        &dg.content(60),
                        &[],
                        "content",
                        &dg.properties(),
                    )
                    .await
                    .expect("create branch");
                branch_ids.push(block.id);
            }
            for branch_id in &branch_ids {
                for _ in 0..per_branch {
                    let name = dg.block_name();
                    store
                        .create_block_with_content(
                            Some(*branch_id),
                            &name,
                            &dg.content(40),
                            &[],
                            "content",
                            &dg.properties(),
                        )
                        .await
                        .expect("create leaf");
                }
            }

            // Bench: move_block (reparent src under dst, then back)
            let start = Instant::now();
            store
                .move_block(src.id, Some(dst.id), None)
                .await
                .expect("move forward");
            let fwd_ms = start.elapsed().as_secs_f64() * 1000.0;

            let start = Instant::now();
            store
                .move_block(src.id, Some(test_root_id), None)
                .await
                .expect("move back");
            let back_ms = start.elapsed().as_secs_f64() * 1000.0;

            benchmarks.push(BenchmarkResult::with_metadata(
                format!("move_block_{subtree_size}"),
                fwd_ms + back_ms,
                2,
                serde_json::json!({
                    "subtree_size": subtree_size,
                    "forward_ms": fwd_ms,
                    "back_ms": back_ms,
                }),
            ));

            // Create a separate subtree for deletion benchmark
            let (del_root, _) = store
                .create_block_with_content(
                    Some(test_root_id),
                    &format!("hier_del_{subtree_size}"),
                    "",
                    &[],
                    "namespace",
                    &serde_json::json!({}),
                )
                .await
                .expect("create del root");

            for _ in 0..subtree_size {
                let name = dg.block_name();
                store
                    .create_block_with_content(
                        Some(del_root.id),
                        &name,
                        &dg.content(40),
                        &[],
                        "content",
                        &dg.properties(),
                    )
                    .await
                    .expect("create del child");
            }

            // Bench: delete_block_recursive
            let start = Instant::now();
            store
                .delete_block_recursive(del_root.id)
                .await
                .expect("delete_block_recursive");
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            benchmarks.push(BenchmarkResult::with_metadata(
                format!("delete_recursive_{subtree_size}"),
                elapsed,
                1,
                serde_json::json!({ "subtree_size": subtree_size }),
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
