//! Performance benchmarks for yap-orange database operations.
//!
//! Tests scaling characteristics of parent_id-based hierarchy operations,
//! particularly for outliner view patterns (root blocks, children, descendants).
//!
//! Run with: cargo test -p yap-store-pg --test benchmarks -- --nocapture

use std::env;
use std::time::Instant;
use uuid::Uuid;

use yap_core::Store;
use yap_store_pg::PgStore;

// =============================================================================
// Setup / Teardown
// =============================================================================

fn get_database_url() -> String {
    env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://yap:yap@localhost:5432/yap".to_string())
}

async fn setup_db() -> PgStore {
    let db = PgStore::connect(&get_database_url())
        .await
        .expect("Failed to connect to database");
    yap_store_pg::run_migrations(db.pool())
        .await
        .expect("Failed to run migrations");
    db
}

async fn setup_bench_root(db: &PgStore, task: &str) -> Uuid {
    let parent_id = db
        .ensure_namespace_block(&format!("test::benchmarks::{}", task))
        .await
        .expect("create bench namespace");
    let run_name = format!("run_{}", Uuid::now_v7().simple());
    let (block, _atom) = db
        .create_block(
            Some(parent_id),
            &run_name,
            "",
            "namespace",
            &serde_json::json!({}),
        )
        .await
        .expect("create bench run root");
    block.id
}

async fn teardown(db: &PgStore, root_id: Uuid) {
    let rows: Vec<(Uuid, Uuid)> = sqlx::query_as(
        r#"
        WITH RECURSIVE tree AS (
            SELECT id, lineage_id FROM blocks WHERE id = $1
            UNION ALL
            SELECT b.id, b.lineage_id FROM blocks b JOIN tree t ON b.parent_id = t.id
        )
        SELECT id, lineage_id FROM tree
        "#,
    )
    .bind(root_id)
    .fetch_all(db.pool())
    .await
    .unwrap_or_default();

    let block_ids: Vec<Uuid> = rows.iter().map(|(id, _)| *id).collect();
    let lineage_ids: Vec<Uuid> = rows.iter().map(|(_, l)| *l).collect();

    if !lineage_ids.is_empty() {
        sqlx::query("DELETE FROM edges WHERE from_lineage_id = ANY($1) OR to_lineage_id = ANY($1)")
            .bind(&lineage_ids)
            .execute(db.pool())
            .await
            .ok();
    }

    if !block_ids.is_empty() {
        sqlx::query("UPDATE blocks SET parent_id = NULL WHERE parent_id = ANY($1)")
            .bind(&block_ids)
            .execute(db.pool())
            .await
            .ok();

        sqlx::query("DELETE FROM blocks WHERE id = ANY($1)")
            .bind(&block_ids)
            .execute(db.pool())
            .await
            .ok();
    }

    if !lineage_ids.is_empty() {
        sqlx::query("DELETE FROM lineages WHERE id = ANY($1)")
            .bind(&lineage_ids)
            .execute(db.pool())
            .await
            .ok();
    }
}

// =============================================================================
// Helpers
// =============================================================================

async fn create_children(
    db: &PgStore,
    parent_id: Uuid,
    n: usize,
    content_prefix: &str,
) -> Vec<Uuid> {
    let mut children = Vec::with_capacity(n);
    for i in 0..n {
        let name = format!("{}_{}", content_prefix, i);
        let content = format!("Content for {} item {}", content_prefix, i);
        let (block, _atom) = db
            .create_block(
                Some(parent_id),
                &name,
                &content,
                "content",
                &serde_json::json!({}),
            )
            .await
            .expect("create child");
        children.push(block.id);
    }
    children
}

async fn create_deep_chain(db: &PgStore, root_id: Uuid, depth: usize) -> Vec<Uuid> {
    let mut chain = Vec::with_capacity(depth);
    let mut parent_id = root_id;
    for i in 0..depth {
        let name = format!("depth_{}", i);
        let (block, _atom) = db
            .create_block(
                Some(parent_id),
                &name,
                &format!("Level {}", i),
                "content",
                &serde_json::json!({}),
            )
            .await
            .expect("create deep child");
        parent_id = block.id;
        chain.push(block.id);
    }
    chain
}

struct TimingResult {
    label: String,
    count: usize,
    total_ms: f64,
    avg_ms: f64,
    min_ms: f64,
    max_ms: f64,
}

impl TimingResult {
    fn print(&self) {
        println!(
            "  {:<45} n={:<5} total={:>8.2}ms  avg={:>8.3}ms  min={:>8.3}ms  max={:>8.3}ms",
            self.label, self.count, self.total_ms, self.avg_ms, self.min_ms, self.max_ms
        );
    }
}

async fn time_async<F, Fut, T>(label: &str, iterations: usize, f: F) -> TimingResult
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = T>,
{
    let mut times = Vec::with_capacity(iterations);
    for _ in 0..iterations {
        let start = Instant::now();
        let _ = f().await;
        times.push(start.elapsed().as_secs_f64() * 1000.0);
    }
    let total: f64 = times.iter().sum();
    let min = times.iter().copied().fold(f64::INFINITY, f64::min);
    let max = times.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    TimingResult {
        label: label.to_string(),
        count: iterations,
        total_ms: total,
        avg_ms: total / iterations as f64,
        min_ms: min,
        max_ms: max,
    }
}

// =============================================================================
// Benchmarks
// =============================================================================

#[tokio::test]
async fn bench_basic_crud() {
    let db = setup_db().await;
    let root_id = setup_bench_root(&db, "basic_crud").await;

    println!("\n{}", "=".repeat(70));
    println!("BENCHMARK: Basic CRUD Operations");
    println!("{}", "=".repeat(70));

    let r = time_async("atom create", 100, || {
        let db = db.clone();
        async move {
            let create = yap_core::models::CreateAtom {
                content_type: "content".to_string(),
                content_template: "bench content".to_string(),
                links: vec![],
                properties: serde_json::json!({}),
            };
            db.create_atom(&create).await.unwrap()
        }
    })
    .await;
    r.print();

    let mut block_ids = Vec::new();
    let r = time_async("block create (under root)", 100, || {
        let db = db.clone();
        let pid = root_id;
        async move {
            let name = format!("block_{}", Uuid::now_v7().simple());
            let (b, _a) = db
                .create_block(
                    Some(pid),
                    &name,
                    "bench content",
                    "content",
                    &serde_json::json!({}),
                )
                .await
                .unwrap();
            b
        }
    })
    .await;
    r.print();

    let children = db.get_block_children(root_id).await.unwrap();
    for c in children.iter().take(100) {
        block_ids.push(c.id);
    }

    let ids = block_ids.clone();
    let r = time_async("block get by ID", ids.len(), || {
        let db = db.clone();
        let ids = ids.clone();
        async move {
            for id in &ids {
                db.get_block(*id).await.unwrap();
            }
        }
    })
    .await;
    let per_op = r.total_ms / ids.len() as f64;
    println!(
        "  {:<45} n={:<5} per_op={:>8.3}ms",
        "block get by ID (per op)",
        ids.len(),
        per_op
    );

    let ids = block_ids.clone();
    let r = time_async("block get_with_atom", ids.len(), || {
        let db = db.clone();
        let ids = ids.clone();
        async move {
            for id in &ids {
                db.get_block_with_atom(*id).await.unwrap();
            }
        }
    })
    .await;
    let per_op = r.total_ms / ids.len() as f64;
    println!(
        "  {:<45} n={:<5} per_op={:>8.3}ms",
        "block get_with_atom (per op)",
        ids.len(),
        per_op
    );

    teardown(&db, root_id).await;
    println!();
}

#[tokio::test]
async fn bench_outliner_scaling() {
    let db = setup_db().await;
    let root_id = setup_bench_root(&db, "outliner_scaling").await;

    println!("\n{}", "=".repeat(70));
    println!("BENCHMARK: Outliner View Scaling (parent_id children queries)");
    println!("{}", "=".repeat(70));

    for &child_count in &[10, 50, 100, 500, 1000] {
        let sub_id = {
            let name = format!("wide_{}", child_count);
            let (b, _a) = db
                .create_block(
                    Some(root_id),
                    &name,
                    "",
                    "namespace",
                    &serde_json::json!({}),
                )
                .await
                .unwrap();
            b.id
        };

        let start = Instant::now();
        create_children(&db, sub_id, child_count, &format!("child_{}", child_count)).await;
        let insert_ms = start.elapsed().as_secs_f64() * 1000.0;
        println!(
            "  [setup] Created {} children in {:.1}ms ({:.3}ms/item)",
            child_count,
            insert_ms,
            insert_ms / child_count as f64
        );

        let r = time_async(
            &format!("get_block_children (n={})", child_count),
            10,
            || {
                let db = db.clone();
                async move { db.get_block_children(sub_id).await.unwrap() }
            },
        )
        .await;
        r.print();
    }

    println!();

    let wide_root_id = {
        let name = format!("__bench_wide_roots_{}", Uuid::now_v7().simple());
        let (b, _a) = db
            .create_block(None, &name, "", "namespace", &serde_json::json!({}))
            .await
            .unwrap();
        b.id
    };

    for &count in &[10, 100, 500] {
        let sub_id = {
            let name = format!("roots_{}", count);
            let (b, _a) = db
                .create_block(
                    Some(wide_root_id),
                    &name,
                    "",
                    "namespace",
                    &serde_json::json!({}),
                )
                .await
                .unwrap();
            b.id
        };

        create_children(&db, sub_id, count, &format!("r_{}", count)).await;

        let r = time_async(
            &format!("get_block_children as root analog (n={})", count),
            10,
            || {
                let db = db.clone();
                async move { db.get_block_children(sub_id).await.unwrap() }
            },
        )
        .await;
        r.print();
    }

    teardown(&db, root_id).await;
    teardown(&db, wide_root_id).await;
    println!();
}

#[tokio::test]
async fn bench_deep_hierarchy() {
    let db = setup_db().await;
    let root_id = setup_bench_root(&db, "deep_hierarchy").await;

    println!("\n{}", "=".repeat(70));
    println!("BENCHMARK: Deep Hierarchy (parent_id depth scaling)");
    println!("{}", "=".repeat(70));

    for &depth in &[5, 10, 20, 30, 50] {
        let sub_id = {
            let name = format!("deep_{}", depth);
            let (b, _a) = db
                .create_block(
                    Some(root_id),
                    &name,
                    "",
                    "namespace",
                    &serde_json::json!({}),
                )
                .await
                .unwrap();
            b.id
        };

        let start = Instant::now();
        let chain = create_deep_chain(&db, sub_id, depth).await;
        let insert_ms = start.elapsed().as_secs_f64() * 1000.0;
        println!(
            "  [setup] Created depth-{} chain in {:.1}ms ({:.3}ms/level)",
            depth,
            insert_ms,
            insert_ms / depth as f64
        );

        let mid_idx = depth / 2;
        let mid_id = chain[mid_idx];

        let r = time_async(
            &format!("get_block_children at depth {} (of {})", mid_idx, depth),
            10,
            || {
                let db = db.clone();
                async move { db.get_block_children(mid_id).await.unwrap() }
            },
        )
        .await;
        r.print();

        let deepest_id = chain[depth - 1];
        let r = time_async(&format!("compute_namespace at depth {}", depth), 10, || {
            let db = db.clone();
            async move { db.compute_namespace(deepest_id).await.unwrap() }
        })
        .await;
        r.print();

        let display_path = db.compute_namespace(sub_id).await.unwrap();
        let r = time_async(
            &format!("list_blocks_by_namespace (depth {} subtree)", depth),
            10,
            || {
                let db = db.clone();
                let ns = display_path.clone();
                async move { db.list_blocks_by_namespace(&ns).await.unwrap() }
            },
        )
        .await;
        r.print();

        println!();
    }

    teardown(&db, root_id).await;
}

#[tokio::test]
async fn bench_block_move() {
    let db = setup_db().await;
    let root_id = setup_bench_root(&db, "block_move").await;

    println!("\n{}", "=".repeat(70));
    println!("BENCHMARK: Block Move (single-row parent_id update)");
    println!("{}", "=".repeat(70));

    for &subtree_size in &[1, 10, 50, 100] {
        let src_id = {
            let name = format!("src_{}", subtree_size);
            let (b, _a) = db
                .create_block(
                    Some(root_id),
                    &name,
                    "",
                    "namespace",
                    &serde_json::json!({}),
                )
                .await
                .unwrap();
            b.id
        };

        let dst_id = {
            let name = format!("dst_{}", subtree_size);
            let (b, _a) = db
                .create_block(
                    Some(root_id),
                    &name,
                    "",
                    "namespace",
                    &serde_json::json!({}),
                )
                .await
                .unwrap();
            b.id
        };

        if subtree_size == 1 {
            create_children(&db, src_id, 1, "leaf").await;
        } else {
            let breadth = (subtree_size as f64).sqrt() as usize;
            let per_branch = subtree_size / breadth;
            let branches = create_children(&db, src_id, breadth, "branch").await;
            for branch_id in &branches {
                create_children(&db, *branch_id, per_branch, "leaf").await;
            }
        }

        let start = Instant::now();
        db.move_block(src_id, Some(dst_id), None).await.unwrap();
        let move_ms = start.elapsed().as_secs_f64() * 1000.0;

        let start = Instant::now();
        db.move_block(src_id, Some(root_id), None).await.unwrap();
        let moveback_ms = start.elapsed().as_secs_f64() * 1000.0;

        println!(
            "  move_block ({} subtree nodes)                 fwd={:.3}ms  back={:.3}ms",
            subtree_size, move_ms, moveback_ms
        );
    }

    teardown(&db, root_id).await;
    println!();
}

#[tokio::test]
async fn bench_backlinks() {
    let db = setup_db().await;
    let root_id = setup_bench_root(&db, "backlinks").await;

    println!("\n{}", "=".repeat(70));
    println!("BENCHMARK: Backlinks Query Scaling");
    println!("{}", "=".repeat(70));

    let (_target_block, target_atom) = db
        .create_block(
            Some(root_id),
            "target",
            "I am the target",
            "content",
            &serde_json::json!({}),
        )
        .await
        .unwrap();

    for &link_count in &[0, 10, 50, 100, 500] {
        let sub_id = {
            let name = format!("linkers_{}", link_count);
            let (b, _a) = db
                .create_block(
                    Some(root_id),
                    &name,
                    "",
                    "namespace",
                    &serde_json::json!({}),
                )
                .await
                .unwrap();
            b.id
        };

        for i in 0..link_count {
            let name = format!("linker_{}", i);
            let template = format!("links to target via {{0}} item {}", i);
            db.create_block_with_content(
                Some(sub_id),
                &name,
                &template,
                &[target_atom.id],
                "content",
                &serde_json::json!({}),
            )
            .await
            .unwrap();
        }

        let target_id = target_atom.id;
        let r = time_async(
            &format!("get_backlinks (n={} linkers)", link_count),
            10,
            || {
                let db = db.clone();
                async move { db.get_backlinks(target_id).await.unwrap() }
            },
        )
        .await;
        r.print();

        let r = time_async(
            &format!("count_backlinks (n={} linkers)", link_count),
            10,
            || {
                let db = db.clone();
                async move { db.count_backlinks(target_id).await.unwrap() }
            },
        )
        .await;
        r.print();
    }

    teardown(&db, root_id).await;
    println!();
}

#[tokio::test]
async fn bench_namespace_resolution() {
    let db = setup_db().await;
    let root_id = setup_bench_root(&db, "namespace_resolution").await;

    println!("\n{}", "=".repeat(70));
    println!("BENCHMARK: Namespace Resolution (path walking)");
    println!("{}", "=".repeat(70));

    let root_ns = db.compute_namespace(root_id).await.unwrap();
    let mut current_parent_id = root_id;
    let mut display_segments = vec![root_ns];

    for depth in 1..=10 {
        let name = format!("ns_level_{}", depth);
        let (block, _atom) = db
            .create_block(
                Some(current_parent_id),
                &name,
                "",
                "namespace",
                &serde_json::json!({}),
            )
            .await
            .unwrap();
        current_parent_id = block.id;
        display_segments.push(name);

        let display_ns = display_segments.join("::");

        let r = time_async(
            &format!("find_block_by_namespace (depth {})", depth),
            10,
            || {
                let db = db.clone();
                let ns = display_ns.clone();
                async move { db.find_block_by_namespace(&ns).await.unwrap() }
            },
        )
        .await;
        r.print();
    }

    teardown(&db, root_id).await;
    println!();
}

#[tokio::test]
async fn bench_outliner_realistic() {
    let db = setup_db().await;
    let root_id = setup_bench_root(&db, "outliner_realistic").await;

    println!("\n{}", "=".repeat(70));
    println!("BENCHMARK: Realistic Outliner Scenario");
    println!("{}", "=".repeat(70));

    println!("  [setup] Building tree: 5 projects x 10 areas x 20 notes = 1055 blocks...");
    let setup_start = Instant::now();

    let mut project_ids = Vec::new();
    let mut area_ids = Vec::new();

    for p in 0..5 {
        let (proj_block, _) = db
            .create_block(
                Some(root_id),
                &format!("project_{}", p),
                &format!("Project {} overview", p),
                "namespace",
                &serde_json::json!({}),
            )
            .await
            .unwrap();
        project_ids.push(proj_block.id);

        for a in 0..10 {
            let (area_block, _) = db
                .create_block(
                    Some(proj_block.id),
                    &format!("area_{}", a),
                    &format!("Area {} in project {}", a, p),
                    "namespace",
                    &serde_json::json!({}),
                )
                .await
                .unwrap();
            area_ids.push(area_block.id);

            for n in 0..20 {
                db.create_block(
                    Some(area_block.id),
                    &format!("note_{}", n),
                    &format!("Note {} in area {} project {}", n, a, p),
                    "content",
                    &serde_json::json!({"status": "active"}),
                )
                .await
                .unwrap();
            }
        }
    }

    let setup_ms = setup_start.elapsed().as_secs_f64() * 1000.0;
    println!("  [setup] Tree built in {:.0}ms\n", setup_ms);

    let bench_root = root_id;
    let r = time_async("load top-level projects (5 items)", 20, || {
        let db = db.clone();
        async move { db.get_block_children(bench_root).await.unwrap() }
    })
    .await;
    r.print();

    let pid = project_ids[0];
    let r = time_async("expand project -> areas (10 items)", 20, || {
        let db = db.clone();
        async move { db.get_block_children(pid).await.unwrap() }
    })
    .await;
    r.print();

    let aid = area_ids[0];
    let r = time_async("expand area -> notes (20 items)", 20, || {
        let db = db.clone();
        async move { db.get_block_children(aid).await.unwrap() }
    })
    .await;
    r.print();

    let note_children = db.get_block_children(area_ids[0]).await.unwrap();
    if let Some(note) = note_children.first() {
        let note_id = note.id;
        let r = time_async("compute_namespace (depth 4)", 20, || {
            let db = db.clone();
            async move { db.compute_namespace(note_id).await.unwrap() }
        })
        .await;
        r.print();
    }

    let r = time_async("full outliner drill-down (3 levels)", 10, || {
        let db = db.clone();
        async move {
            let projects = db.get_block_children(bench_root).await.unwrap();
            let areas = db.get_block_children(projects[0].id).await.unwrap();
            let notes = db.get_block_children(areas[0].id).await.unwrap();
            for note in &notes {
                let _ = db.compute_namespace(note.id).await.unwrap();
            }
            notes.len()
        }
    })
    .await;
    r.print();

    let root_ns = db.compute_namespace(root_id).await.unwrap();
    let rns = root_ns.clone();
    let r = time_async("list_blocks_by_namespace (all 1055)", 5, || {
        let db = db.clone();
        let ns = rns.clone();
        async move {
            let blocks = db.list_blocks_by_namespace(&ns).await.unwrap();
            blocks.len()
        }
    })
    .await;
    r.print();

    let r = time_async("list_orphaned_blocks (1055 blocks present)", 5, || {
        let db = db.clone();
        async move { db.list_orphaned_blocks().await.unwrap() }
    })
    .await;
    r.print();

    let r = time_async("get_root_blocks (1055 blocks present)", 5, || {
        let db = db.clone();
        async move { db.get_root_blocks().await.unwrap() }
    })
    .await;
    r.print();

    teardown(&db, root_id).await;
    println!();
}

#[tokio::test]
async fn bench_edges() {
    let db = setup_db().await;
    let root_id = setup_bench_root(&db, "edges").await;

    println!("\n{}", "=".repeat(70));
    println!("BENCHMARK: Edge Operations");
    println!("{}", "=".repeat(70));

    let mut atom_ids = Vec::new();
    for i in 0..50 {
        let (_, atom) = db
            .create_block(
                Some(root_id),
                &format!("edge_node_{}", i),
                &format!("Node {}", i),
                "content",
                &serde_json::json!({}),
            )
            .await
            .unwrap();
        atom_ids.push(atom.id);
    }

    let start = Instant::now();
    for i in 1..atom_ids.len() {
        let create = yap_core::models::CreateEdge {
            from_lineage_id: atom_ids[0],
            to_lineage_id: atom_ids[i],
            edge_type: "references".to_string(),
            properties: serde_json::json!({}),
        };
        db.create_edge(&create).await.unwrap();
    }
    let create_ms = start.elapsed().as_secs_f64() * 1000.0;
    println!(
        "  [setup] Created {} edges in {:.1}ms ({:.3}ms/edge)",
        atom_ids.len() - 1,
        create_ms,
        create_ms / (atom_ids.len() - 1) as f64
    );

    let hub = atom_ids[0];
    let r = time_async("get_all_edges (49 edges)", 20, || {
        let db = db.clone();
        async move { db.get_all_edges(hub).await.unwrap() }
    })
    .await;
    r.print();

    let r = time_async("get_edges_from (49 outgoing)", 20, || {
        let db = db.clone();
        async move { db.get_edges_from(hub).await.unwrap() }
    })
    .await;
    r.print();

    let spoke = atom_ids[1];
    let r = time_async("get_edges_to (1 incoming)", 20, || {
        let db = db.clone();
        async move { db.get_edges_to(spoke).await.unwrap() }
    })
    .await;
    r.print();

    teardown(&db, root_id).await;
    println!();
}
