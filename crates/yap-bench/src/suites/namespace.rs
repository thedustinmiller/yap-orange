use async_trait::async_trait;
use uuid::Uuid;
use web_time::Instant;
use yap_core::Store;

use crate::results::{BenchmarkResult, SuiteResult};

use super::BenchSuite;

pub struct NamespaceSuite;

#[async_trait]
impl BenchSuite for NamespaceSuite {
    fn name(&self) -> &str {
        "namespace_traversal"
    }

    fn description(&self) -> &str {
        "Measures compute_namespace and find_block_by_namespace at depths 5, 10, 20"
    }

    async fn run(&self, store: &dyn Store, test_root_id: Uuid, _seed: u64) -> SuiteResult {
        let suite_start = Instant::now();
        let mut benchmarks = Vec::new();

        // Create the namespace parent for this suite
        let (ns_parent, _) = store
            .create_block_with_content(
                Some(test_root_id),
                "namespace_bench",
                "",
                &[],
                "namespace",
                &serde_json::json!({}),
            )
            .await
            .expect("create namespace_bench");

        for &depth in &[5u32, 10, 20] {
            // Create a chain of the given depth
            let (chain_root, _) = store
                .create_block_with_content(
                    Some(ns_parent.id),
                    &format!("chain_{depth}"),
                    "",
                    &[],
                    "namespace",
                    &serde_json::json!({}),
                )
                .await
                .expect("create chain root");

            let mut current_id = chain_root.id;
            let mut deepest_id = chain_root.id;
            for i in 0..depth {
                let (block, _) = store
                    .create_block_with_content(
                        Some(current_id),
                        &format!("level_{i}"),
                        "",
                        &[],
                        "namespace",
                        &serde_json::json!({}),
                    )
                    .await
                    .expect("create chain level");
                current_id = block.id;
                deepest_id = block.id;
            }

            // Bench: compute_namespace at the deepest level
            let iterations = 10u64;
            let start = Instant::now();
            let mut namespace_path = String::new();
            for _ in 0..iterations {
                namespace_path = store
                    .compute_namespace(deepest_id)
                    .await
                    .expect("compute_namespace");
            }
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            benchmarks.push(BenchmarkResult::with_metadata(
                format!("compute_namespace_depth_{depth}"),
                elapsed,
                iterations,
                serde_json::json!({ "depth": depth }),
            ));

            // Bench: find_block_by_namespace using the full path
            let start = Instant::now();
            for _ in 0..iterations {
                store
                    .find_block_by_namespace(&namespace_path)
                    .await
                    .expect("find_block_by_namespace");
            }
            let elapsed = start.elapsed().as_secs_f64() * 1000.0;
            benchmarks.push(BenchmarkResult::with_metadata(
                format!("find_by_namespace_depth_{depth}"),
                elapsed,
                iterations,
                serde_json::json!({ "depth": depth, "path": namespace_path }),
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
