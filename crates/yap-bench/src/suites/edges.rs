use async_trait::async_trait;
use uuid::Uuid;
use web_time::Instant;
use yap_core::Store;
use yap_core::models::CreateEdge;

use crate::data::DataGen;
use crate::results::{BenchmarkResult, SuiteResult};

use super::BenchSuite;

pub struct EdgesSuite;

#[async_trait]
impl BenchSuite for EdgesSuite {
    fn name(&self) -> &str {
        "edge_operations"
    }

    fn description(&self) -> &str {
        "Measures create_edge, get_all_edges, get_edges_from, get_edges_to with 30 nodes"
    }

    async fn run(&self, store: &dyn Store, test_root_id: Uuid, seed: u64) -> SuiteResult {
        let suite_start = Instant::now();
        let mut benchmarks = Vec::new();
        let mut dg = DataGen::new(seed);

        // Setup: create 30 nodes
        let (edge_parent, _) = store
            .create_block_with_content(
                Some(test_root_id),
                "edge_bench",
                "",
                &[],
                "namespace",
                &serde_json::json!({}),
            )
            .await
            .expect("create edge parent");

        let mut lineage_ids = Vec::new();
        for i in 0..30 {
            let name = format!("edge_node_{i}");
            let (block, _) = store
                .create_block_with_content(
                    Some(edge_parent.id),
                    &name,
                    &dg.content(40),
                    &[],
                    "content",
                    &serde_json::json!({}),
                )
                .await
                .expect("create edge node");
            lineage_ids.push(block.lineage_id);
        }

        let hub = lineage_ids[0];

        // Bench: create_edge (29 edges from hub to spokes)
        let start = Instant::now();
        for &spoke in &lineage_ids[1..] {
            let edge_type = dg.edge_type();
            store
                .create_edge(&CreateEdge {
                    from_lineage_id: hub,
                    to_lineage_id: spoke,
                    edge_type,
                    properties: serde_json::json!({}),
                })
                .await
                .expect("create_edge");
        }
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        benchmarks.push(BenchmarkResult::new("create_edge_x29", elapsed, 29));

        // Bench: get_all_edges on hub
        let iterations = 10u64;
        let start = Instant::now();
        for _ in 0..iterations {
            store.get_all_edges(hub).await.expect("get_all_edges");
        }
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        benchmarks.push(BenchmarkResult::with_metadata(
            "get_all_edges_29",
            elapsed,
            iterations,
            serde_json::json!({ "edge_count": 29 }),
        ));

        // Bench: get_edges_from on hub (29 outgoing)
        let start = Instant::now();
        for _ in 0..iterations {
            store.get_edges_from(hub).await.expect("get_edges_from");
        }
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        benchmarks.push(BenchmarkResult::with_metadata(
            "get_edges_from_29",
            elapsed,
            iterations,
            serde_json::json!({ "outgoing": 29 }),
        ));

        // Bench: get_edges_to on a spoke (1 incoming)
        let spoke = lineage_ids[1];
        let start = Instant::now();
        for _ in 0..iterations {
            store.get_edges_to(spoke).await.expect("get_edges_to");
        }
        let elapsed = start.elapsed().as_secs_f64() * 1000.0;
        benchmarks.push(BenchmarkResult::with_metadata(
            "get_edges_to_1",
            elapsed,
            iterations,
            serde_json::json!({ "incoming": 1 }),
        ));

        SuiteResult {
            name: self.name().to_string(),
            description: self.description().to_string(),
            duration_ms: suite_start.elapsed().as_secs_f64() * 1000.0,
            benchmarks,
        }
    }
}
