use clap::{Parser, Subcommand};
use rand::Rng;
use sqlx::PgPool;
use std::env;
use std::process;
use uuid::Uuid;
use yap_core::Store;
use yap_core::content::serialize_content;
use yap_core::models::CreateEdge;
use yap_store_pg::PgStore;

#[derive(Parser)]
#[command(name = "xtask", about = "Development tasks for yap-orange")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    /// Database management commands
    Db {
        #[command(subcommand)]
        action: DbAction,
    },
    /// Run development servers
    Run {
        #[command(subcommand)]
        target: RunTarget,
    },
    /// Run checks and lints
    Check {
        #[command(subcommand)]
        target: CheckTarget,
    },
    /// Build artifacts
    Build {
        #[command(subcommand)]
        target: BuildTarget,
    },
    /// Run test suites
    Test {
        #[command(subcommand)]
        target: TestTarget,
    },
}

#[derive(Subcommand)]
enum DbAction {
    /// Run migrations
    Setup,
    /// Truncate all data tables (preserves schema)
    Clear,
    /// Drop all tables and re-run migrations (nuclear option for schema changes)
    Reset,
    /// Seed the database with procedurally generated blocks
    Seed {
        /// Number of leaf blocks to generate (default: 75)
        #[arg(short, long, default_value_t = 75)]
        count: usize,
    },
    /// Clear all data then seed (shortcut for clear + seed)
    Reseed {
        /// Number of leaf blocks to generate (default: 75)
        #[arg(short, long, default_value_t = 75)]
        count: usize,
    },
}

#[derive(Subcommand)]
enum RunTarget {
    /// Start the Rust API server (cargo run -p yap-server)
    Server,
    /// Start the Vite dev server (npm run dev in web/)
    Web,
    /// Start the Tauri desktop app (cargo tauri dev in crates/yap-desktop)
    Desktop,
    /// Build WASM and start the web UI in SPA mode (no backend server)
    Spa,
}

#[derive(Subcommand)]
enum CheckTarget {
    /// Run cargo check + web typecheck
    All,
}

#[derive(Subcommand)]
enum BuildTarget {
    /// Build the WASM module for browser SPA mode
    Wasm,
    /// Build the Rust API server
    Server {
        /// Build in release mode
        #[arg(long)]
        release: bool,
    },
    /// Build the CLI binary
    Cli {
        /// Build in release mode
        #[arg(long)]
        release: bool,
    },
    /// Build the web frontend (vite build)
    Web,
    /// Build the Tauri desktop app bundle
    Desktop {
        /// Build in debug mode (default is release)
        #[arg(long)]
        debug: bool,
    },
    /// Build everything (server, cli, web, wasm, desktop)
    All {
        /// Build in release mode
        #[arg(long)]
        release: bool,
    },
}

#[derive(Subcommand)]
enum TestTarget {
    /// Run all test suites (Rust + web)
    All,
    /// Run all Rust tests (cargo test --workspace)
    Rust {
        /// Additional arguments passed to cargo test
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Run all web tests (vitest + playwright)
    Web,
    /// Run web unit tests only (vitest)
    Unit,
    /// Run web e2e tests only (playwright)
    E2e,
}

fn get_database_url() -> String {
    env::var("DATABASE_URL").unwrap_or_else(|_| "postgres://yap:yap@localhost:5432/yap".to_string())
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Db { action } => match action {
            DbAction::Setup => db_setup().await?,
            DbAction::Clear => db_clear().await?,
            DbAction::Reset => db_reset().await?,
            DbAction::Seed { count } => seed(count).await?,
            DbAction::Reseed { count } => {
                db_clear().await?;
                seed(count).await?;
            }
        },
        Command::Run { target } => match target {
            RunTarget::Server => run_server()?,
            RunTarget::Web => run_web()?,
            RunTarget::Desktop => run_desktop()?,
            RunTarget::Spa => run_spa()?,
        },
        Command::Check { target } => match target {
            CheckTarget::All => check_all()?,
        },
        Command::Build { target } => match target {
            BuildTarget::Wasm => build_wasm()?,
            BuildTarget::Server { release } => build_server(release)?,
            BuildTarget::Cli { release } => build_cli(release)?,
            BuildTarget::Web => build_web()?,
            BuildTarget::Desktop { debug } => build_desktop(debug)?,
            BuildTarget::All { release } => build_all(release)?,
        },
        Command::Test { target } => match target {
            TestTarget::All => test_all()?,
            TestTarget::Rust { args } => test_rust(&args)?,
            TestTarget::Web => test_web()?,
            TestTarget::Unit => test_unit()?,
            TestTarget::E2e => test_e2e()?,
        },
    }

    Ok(())
}

// ── Database commands ────────────────────────────────────────────────

async fn db_setup() -> anyhow::Result<()> {
    let url = get_database_url();
    println!("Connecting to database...");
    let pool = PgPool::connect(&url).await?;

    println!("Running migrations...");
    sqlx::migrate!("../migrations/postgres").run(&pool).await?;
    println!("Migrations complete.");

    Ok(())
}

async fn db_clear() -> anyhow::Result<()> {
    let url = get_database_url();
    let db = PgStore::connect(&url)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;

    println!("Clearing all data...");
    db.clear_all_data()
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    println!("All data cleared.");

    Ok(())
}

async fn db_reset() -> anyhow::Result<()> {
    let url = get_database_url();
    let pool = PgPool::connect(&url).await?;

    println!("Dropping all tables...");

    // Drop in FK-safe order, each guarded by IF EXISTS
    let tables = ["edges", "blocks", "lineages", "atoms", "_sqlx_migrations"];
    for table in &tables {
        let sql = format!("DROP TABLE IF EXISTS {table} CASCADE");
        sqlx::query(&sql).execute(&pool).await?;
    }

    // Also clean up the trigger function left behind
    sqlx::query("DROP FUNCTION IF EXISTS update_updated_at() CASCADE")
        .execute(&pool)
        .await?;

    println!("All tables dropped.");

    // Re-run migrations
    println!("Running migrations...");
    sqlx::migrate!("../migrations/postgres").run(&pool).await?;
    println!("Database reset complete.");

    Ok(())
}

// ── Run commands ─────────────────────────────────────────────────────

fn run_server() -> anyhow::Result<()> {
    let status = process::Command::new("cargo")
        .args(["run", "-p", "yap-server"])
        .status()?;

    if !status.success() {
        process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

fn run_web() -> anyhow::Result<()> {
    let status = process::Command::new("npm")
        .args(["run", "dev"])
        .current_dir("web")
        .status()?;

    if !status.success() {
        process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

fn run_desktop() -> anyhow::Result<()> {
    let status = process::Command::new("cargo")
        .args(["tauri", "dev"])
        .current_dir("crates/yap-desktop")
        .status()?;

    if !status.success() {
        process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

fn build_wasm() -> anyhow::Result<()> {
    println!("Building WASM module...");
    let status = process::Command::new("wasm-pack")
        .args([
            "build",
            "--target",
            "web",
            "--out-dir",
            "../../web/public/wasm",
            "--out-name",
            "yap_server_wasm",
        ])
        .current_dir("crates/yap-server-wasm")
        .status()?;

    if !status.success() {
        eprintln!("wasm-pack build failed");
        process::exit(status.code().unwrap_or(1));
    }
    println!("WASM build complete → web/public/wasm/");
    Ok(())
}

fn run_spa() -> anyhow::Result<()> {
    // Build WASM first
    build_wasm()?;

    println!("\nStarting web UI in SPA mode (no backend server)...");
    let status = process::Command::new("npm")
        .args(["run", "dev"])
        .current_dir("web")
        .status()?;

    if !status.success() {
        process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

// ── Check commands ───────────────────────────────────────────────────

fn check_all() -> anyhow::Result<()> {
    println!("Running cargo check --workspace...");
    let cargo = process::Command::new("cargo")
        .args(["check", "--workspace"])
        .status()?;

    if !cargo.success() {
        eprintln!("cargo check failed");
        process::exit(cargo.code().unwrap_or(1));
    }

    println!("\nRunning npm run check in web/...");
    let npm = process::Command::new("npm")
        .args(["run", "check"])
        .current_dir("web")
        .status()?;

    if !npm.success() {
        eprintln!("npm check failed");
        process::exit(npm.code().unwrap_or(1));
    }

    println!("\nChecking WASM crates...");
    let wasm_store = process::Command::new("cargo")
        .args(["check", "--target", "wasm32-unknown-unknown"])
        .current_dir("crates/yap-store-wasm")
        .status()?;

    if !wasm_store.success() {
        eprintln!("yap-store-wasm check failed");
        process::exit(wasm_store.code().unwrap_or(1));
    }

    let wasm_server = process::Command::new("cargo")
        .args(["check", "--target", "wasm32-unknown-unknown"])
        .current_dir("crates/yap-server-wasm")
        .status()?;

    if !wasm_server.success() {
        eprintln!("yap-server-wasm check failed");
        process::exit(wasm_server.code().unwrap_or(1));
    }

    println!("\nAll checks passed.");
    Ok(())
}

// ── Build commands ──────────────────────────────────────────────────

fn build_server(release: bool) -> anyhow::Result<()> {
    let profile = if release { "release" } else { "debug" };
    println!("Building server ({profile})...");
    let mut args = vec!["build", "-p", "yap-server"];
    if release {
        args.push("--release");
    }
    let status = process::Command::new("cargo").args(&args).status()?;
    if !status.success() {
        eprintln!("Server build failed");
        process::exit(status.code().unwrap_or(1));
    }
    println!("Server build complete ({profile}).");
    Ok(())
}

fn build_cli(release: bool) -> anyhow::Result<()> {
    let profile = if release { "release" } else { "debug" };
    println!("Building CLI ({profile})...");
    let mut args = vec!["build", "-p", "yap-cli"];
    if release {
        args.push("--release");
    }
    let status = process::Command::new("cargo").args(&args).status()?;
    if !status.success() {
        eprintln!("CLI build failed");
        process::exit(status.code().unwrap_or(1));
    }
    println!("CLI build complete ({profile}).");
    Ok(())
}

fn build_web() -> anyhow::Result<()> {
    println!("Building web frontend...");
    let status = process::Command::new("npm")
        .args(["run", "build"])
        .current_dir("web")
        .status()?;
    if !status.success() {
        eprintln!("Web frontend build failed");
        process::exit(status.code().unwrap_or(1));
    }
    println!("Web frontend build complete → web/dist/");
    Ok(())
}

fn build_desktop(debug: bool) -> anyhow::Result<()> {
    let profile = if debug { "debug" } else { "release" };
    println!("Building Tauri desktop app ({profile})...");
    let mut args = vec!["tauri", "build"];
    if debug {
        args.push("--debug");
    }
    let status = process::Command::new("cargo")
        .args(&args)
        .current_dir("crates/yap-desktop")
        .status()?;
    if !status.success() {
        eprintln!("Tauri build failed");
        process::exit(status.code().unwrap_or(1));
    }
    println!("Tauri desktop build complete ({profile}).");
    Ok(())
}

fn build_all(release: bool) -> anyhow::Result<()> {
    build_server(release)?;
    build_cli(release)?;
    build_web()?;
    build_wasm()?;
    build_desktop(!release)?;
    println!("\nAll builds complete.");
    Ok(())
}

// ── Test commands ───────────────────────────────────────────────────

fn run_cmd(program: &str, args: &[&str], dir: Option<&str>) -> anyhow::Result<()> {
    let mut cmd = process::Command::new(program);
    cmd.args(args);
    if let Some(d) = dir {
        cmd.current_dir(d);
    }
    let status = cmd.status()?;
    if !status.success() {
        process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

fn test_rust(extra_args: &[String]) -> anyhow::Result<()> {
    let mut args: Vec<&str> = vec!["test"];
    if extra_args.is_empty() {
        args.push("--workspace");
        println!("Running Rust tests (cargo test --workspace)...");
    } else {
        println!(
            "Running Rust tests (cargo test {})...",
            extra_args.join(" ")
        );
    }
    let extra_refs: Vec<&str> = extra_args.iter().map(|s| s.as_str()).collect();
    args.extend(&extra_refs);
    run_cmd("cargo", &args, None)?;
    println!("Rust tests passed.");
    Ok(())
}

fn test_unit() -> anyhow::Result<()> {
    println!("Running web unit tests (vitest)...");
    run_cmd("npm", &["run", "test"], Some("web"))?;
    println!("Web unit tests passed.");
    Ok(())
}

fn test_e2e() -> anyhow::Result<()> {
    println!("Running web e2e tests (playwright)...");
    run_cmd("npx", &["playwright", "test"], Some("web"))?;
    println!("Web e2e tests passed.");
    Ok(())
}

fn test_web() -> anyhow::Result<()> {
    test_unit()?;
    test_e2e()?;
    println!("\nAll web tests passed.");
    Ok(())
}

fn test_all() -> anyhow::Result<()> {
    test_rust(&[])?;
    test_web()?;
    println!("\nAll tests passed.");
    Ok(())
}

// ── Seed command ─────────────────────────────────────────────────────

/// Procedural seed data generator.
///
/// Creates a realistic hierarchy of namespaces and blocks with:
/// - Topic-based namespaces (projects, research, journal)
/// - Date-based namespaces (2026::01::15, 2026::02::03, etc.)
/// - Content blocks with wiki-link references between them
/// - Semantic edges connecting related atoms
async fn seed(count: usize) -> anyhow::Result<()> {
    let url = get_database_url();
    let db = PgStore::connect(&url)
        .await
        .map_err(|e| anyhow::anyhow!("{}", e))?;
    let mut rng = rand::rng();

    println!("Seeding database with ~{count} blocks...");

    // Track created lineages for linking
    let mut lineage_ids: Vec<Uuid> = Vec::new();
    let mut block_namespaces: Vec<String> = Vec::new();

    // ── 0. System bootstrap (types + settings) ─────────────────────

    println!("  Bootstrapping system namespaces...");
    yap_core::bootstrap::bootstrap(&db, &[]).await?;
    println!("    Bootstrap complete.");

    // ── 1. Topic-based namespaces ──────────────────────────────────

    let topic_namespaces = [
        "projects",
        "projects::yap-orange",
        "projects::yap-orange::design",
        "projects::yap-orange::implementation",
        "projects::yap-orange::bugs",
        "projects::website",
        "research",
        "research::ml",
        "research::ml::transformers",
        "research::ml::rl",
        "research::databases",
        "research::databases::postgres",
        "research::plt",
        "notes",
        "notes::ideas",
        "notes::reading",
        "notes::meetings",
        "reference",
        "reference::rust",
        "reference::sql",
    ];

    println!("  Creating topic namespaces...");
    for ns in &topic_namespaces {
        db.ensure_namespace(ns).await?;
    }

    // ── 2. Date-based namespaces ───────────────────────────────────

    let dates = [
        (2026, 1, 10),
        (2026, 1, 15),
        (2026, 1, 22),
        (2026, 1, 28),
        (2026, 2, 1),
        (2026, 2, 2),
        (2026, 2, 3),
    ];

    println!("  Creating date namespaces...");
    for (y, m, d) in &dates {
        let ns = format!("journal::{}::{:02}::{:02}", y, m, d);
        db.ensure_namespace(&ns).await?;
    }

    // ── 3. Content blocks ──────────────────────────────────────────

    let block_defs: Vec<(&str, &str, &str)> = vec![
        // projects::yap-orange::design
        (
            "projects::yap-orange::design",
            "data-model",
            "Core data model uses atoms (inodes) and blocks (directory entries). See [[research::databases::postgres]] for storage notes.",
        ),
        (
            "projects::yap-orange::design",
            "link-syntax",
            "Wiki-link style: [[namespace::path::to::block]]. Supports relative paths with ./ and ../ prefixes.",
        ),
        (
            "projects::yap-orange::design",
            "graph-model",
            "Edges connect atoms non-hierarchically. Types include references, inspired-by, depends-on.",
        ),
        // projects::yap-orange::implementation
        (
            "projects::yap-orange::implementation",
            "phase-1",
            "Foundation: database schema, core library, content transforms. See [[projects::yap-orange::design::data-model]].",
        ),
        (
            "projects::yap-orange::implementation",
            "phase-2",
            "HTTP API server with Axum. All endpoints from [[projects::yap-orange::design]] exposed.",
        ),
        (
            "projects::yap-orange::implementation",
            "phase-3",
            "CLI interface using clap derive macros. See [[reference::rust]] for patterns.",
        ),
        (
            "projects::yap-orange::implementation",
            "phase-4",
            "Web UI with Svelte 5 + xyflow. Outliner, sidebar, graph preview.",
        ),
        (
            "projects::yap-orange::bugs",
            "stale-namespace-cache",
            "Moving blocks sometimes leaves stale namespace cache. Need recursive update.",
        ),
        // projects::website
        (
            "projects::website",
            "stack",
            "Static site with Hugo. Deployed to Netlify. Considering switching to Astro.",
        ),
        (
            "projects::website",
            "redesign",
            "New color scheme, improved typography. See [[notes::ideas]] for brainstorm.",
        ),
        // research::ml::transformers
        (
            "research::ml::transformers",
            "attention-mechanism",
            "Self-attention computes Q, K, V projections. Scaled dot-product attention: softmax(QK^T/√d)V.",
        ),
        (
            "research::ml::transformers",
            "positional-encoding",
            "Sinusoidal encoding or learned embeddings. RoPE gaining popularity for long context.",
        ),
        (
            "research::ml::transformers",
            "flash-attention",
            "IO-aware exact attention. Reduces memory from O(N²) to O(N). Key insight: tiling and recomputation.",
        ),
        // research::ml::rl
        (
            "research::ml::rl",
            "ppo",
            "Proximal Policy Optimization. Clips probability ratios to prevent large policy updates.",
        ),
        (
            "research::ml::rl",
            "rlhf",
            "RL from human feedback. Train reward model on preferences, then optimize policy with PPO. See [[research::ml::rl::ppo]].",
        ),
        // research::databases::postgres
        (
            "research::databases::postgres",
            "gin-indexes",
            "Generalized Inverted Index for jsonb and array columns. Essential for the links[] array queries.",
        ),
        (
            "research::databases::postgres",
            "partial-indexes",
            "Indexes with WHERE clause. Used for unique constraints that exclude soft-deleted rows.",
        ),
        (
            "research::databases::postgres",
            "uuidv7",
            "Time-sortable UUIDs. Better than UUIDv4 for B-tree locality. Supported natively in PG 17+.",
        ),
        // research::plt
        (
            "research::plt",
            "effect-systems",
            "Track side effects in the type system. Algebraic effects allow modular composition.",
        ),
        (
            "research::plt",
            "dependent-types",
            "Types that depend on values. Enables proofs as programs. See Idris, Agda, Lean.",
        ),
        // notes::ideas
        (
            "notes::ideas",
            "block-canvas",
            "Blocks displayed on 2D canvas with x,y coordinates from properties. Alternative view to outliner.",
        ),
        (
            "notes::ideas",
            "semantic-search",
            "Embed block content with an LLM, store vectors in pgvector. Query by meaning not just text.",
        ),
        (
            "notes::ideas",
            "plugin-system",
            "User-defined views and transforms. Lua or WASM plugins that operate on the block graph.",
        ),
        // notes::reading
        (
            "notes::reading",
            "designing-data-intensive-apps",
            "Kleppmann's book on distributed systems. Key chapters: replication, partitioning, stream processing.",
        ),
        (
            "notes::reading",
            "structure-and-interpretation",
            "SICP. Metacircular evaluator, streams, register machines. See [[research::plt]] for connections.",
        ),
        // notes::meetings
        (
            "notes::meetings",
            "2026-01-15-standup",
            "Discussed [[projects::yap-orange::implementation::phase-1]] progress. Blocked on migration testing.",
        ),
        (
            "notes::meetings",
            "2026-01-28-design-review",
            "Reviewed [[projects::yap-orange::design::data-model]]. Decided on inode-like architecture.",
        ),
        (
            "notes::meetings",
            "2026-02-03-sprint-planning",
            "Planned [[projects::yap-orange::implementation::phase-4]] work. Focus on outliner and graph preview.",
        ),
        // reference::rust
        (
            "reference::rust",
            "error-handling",
            "thiserror for library errors, anyhow for application errors. Implement From for error conversion.",
        ),
        (
            "reference::rust",
            "async-patterns",
            "Pin<Box<dyn Future>> for recursive async. tokio::spawn for background tasks.",
        ),
        (
            "reference::rust",
            "sqlx-tips",
            "Use query_as for typed results. Compile-time checking with sqlx-cli. See [[research::databases::postgres]].",
        ),
        // reference::sql
        (
            "reference::sql",
            "window-functions",
            "ROW_NUMBER, RANK, DENSE_RANK over partitions. Used in migration 002 for duplicate cleanup.",
        ),
        (
            "reference::sql",
            "cte-patterns",
            "WITH RECURSIVE for tree traversal. Useful for namespace hierarchy queries.",
        ),
    ];

    println!("  Creating {} content blocks...", block_defs.len());
    let mut links_resolved = 0usize;
    let mut links_unresolved = 0usize;
    for (ns, name, content) in &block_defs {
        let parent_id = db.ensure_namespace_block(ns).await?;
        let serialized = serialize_content(&db, content, Some(ns)).await?;
        links_resolved += serialized.links.len();
        links_unresolved += serialized.unresolved.len();
        match db
            .create_block_with_content(
                Some(parent_id),
                name,
                &serialized.template,
                &serialized.links,
                "content",
                &serde_json::json!({}),
            )
            .await
        {
            Ok((block, atom)) => {
                lineage_ids.push(atom.id);
                let display_path = db.compute_namespace(block.id).await?;
                block_namespaces.push(display_path);
            }
            Err(yap_core::Error::Conflict(_)) => {
                // Block already exists from a previous seed, skip
            }
            Err(e) => return Err(e.into()),
        }
    }

    // ── 4. Date journal entries ────────────────────────────────────

    let journal_entries = [
        (
            "journal::2026::01::10",
            "morning",
            "Started exploring Rust for the new project. Read about Axum and SQLx.",
        ),
        (
            "journal::2026::01::10",
            "evening",
            "Set up workspace structure. See [[projects::yap-orange::implementation::phase-1]].",
        ),
        (
            "journal::2026::01::15",
            "standup-notes",
            "Discussed data model choices. See [[notes::meetings::2026-01-15-standup]].",
        ),
        (
            "journal::2026::01::15",
            "research",
            "Deep dive into [[research::ml::transformers::attention-mechanism]]. Need to understand for embedding feature.",
        ),
        (
            "journal::2026::01::22",
            "progress",
            "Phase 1 tests passing. Content round-trip works. [[projects::yap-orange::design::link-syntax]] finalized.",
        ),
        (
            "journal::2026::01::28",
            "design-day",
            "Major design review. See [[notes::meetings::2026-01-28-design-review]]. Decision: atoms as inodes.",
        ),
        (
            "journal::2026::02::01",
            "sprint-start",
            "Beginning phase 4 web UI. Svelte 5 + xyflow stack confirmed.",
        ),
        (
            "journal::2026::02::02",
            "sidebar-wip",
            "Implementing sidebar component. Recursive tree rendering with expand/collapse.",
        ),
        (
            "journal::2026::02::03",
            "morning",
            "Sprint planning. See [[notes::meetings::2026-02-03-sprint-planning]]. Graph preview is the focus.",
        ),
        (
            "journal::2026::02::03",
            "afternoon",
            "Debugging [[projects::yap-orange::bugs::stale-namespace-cache]]. Root cause: missing recursive update.",
        ),
        (
            "journal::2026::02::03",
            "evening",
            "Fixed namespace cache bug. All 76 Rust tests passing. Added [[research::databases::postgres::partial-indexes]] notes.",
        ),
    ];

    println!("  Creating {} journal entries...", journal_entries.len());
    for (ns, name, content) in &journal_entries {
        let parent_id = db.ensure_namespace_block(ns).await?;
        let serialized = serialize_content(&db, content, Some(ns)).await?;
        links_resolved += serialized.links.len();
        links_unresolved += serialized.unresolved.len();
        match db
            .create_block_with_content(
                Some(parent_id),
                name,
                &serialized.template,
                &serialized.links,
                "content",
                &serde_json::json!({}),
            )
            .await
        {
            Ok((block, atom)) => {
                lineage_ids.push(atom.id);
                let display_path = db.compute_namespace(block.id).await?;
                block_namespaces.push(display_path);
            }
            Err(yap_core::Error::Conflict(_)) => {}
            Err(e) => return Err(e.into()),
        }
    }

    // ── 5. Procedurally generated filler blocks ────────────────────

    let filler_namespaces = [
        "projects::yap-orange::design",
        "projects::yap-orange::implementation",
        "research::ml::transformers",
        "research::databases::postgres",
        "notes::ideas",
        "notes::reading",
        "reference::rust",
        "reference::sql",
    ];

    let filler_adjectives = [
        "quick",
        "detailed",
        "rough",
        "revised",
        "experimental",
        "final",
        "draft",
        "archived",
        "important",
        "followup",
    ];

    let filler_nouns = [
        "note",
        "thought",
        "observation",
        "snippet",
        "sketch",
        "plan",
        "question",
        "answer",
        "comparison",
        "summary",
    ];

    let remaining = count.saturating_sub(block_defs.len() + journal_entries.len());
    println!("  Creating {remaining} procedural filler blocks...");

    for i in 0..remaining {
        let ns = filler_namespaces[rng.random_range(0..filler_namespaces.len())];
        let adj = filler_adjectives[rng.random_range(0..filler_adjectives.len())];
        let noun = filler_nouns[rng.random_range(0..filler_nouns.len())];
        let name = format!("{}-{}-{}", adj, noun, i);

        // Optionally reference an existing block in the content
        let content = if !block_namespaces.is_empty() && rng.random_range(0..3) == 0 {
            let ref_idx = rng.random_range(0..block_namespaces.len());
            format!(
                "Auto-generated {} about {}. Related to [[{}]].",
                noun, adj, block_namespaces[ref_idx]
            )
        } else {
            format!(
                "Auto-generated {} about {}. Created for seed data.",
                noun, adj
            )
        };

        let parent_id = db.ensure_namespace_block(ns).await?;
        let serialized = serialize_content(&db, &content, Some(ns)).await?;
        links_resolved += serialized.links.len();
        links_unresolved += serialized.unresolved.len();
        match db
            .create_block_with_content(
                Some(parent_id),
                &name,
                &serialized.template,
                &serialized.links,
                "content",
                &serde_json::json!({}),
            )
            .await
        {
            Ok((_block, atom)) => {
                lineage_ids.push(atom.id);
            }
            Err(yap_core::Error::Conflict(_)) => {}
            Err(e) => return Err(e.into()),
        }
    }

    // ── 6. Semantic edges ──────────────────────────────────────────

    let edge_types = [
        "references",
        "inspired-by",
        "depends-on",
        "related-to",
        "extends",
    ];
    let edge_count = lineage_ids.len().min(20);

    println!("  Creating ~{edge_count} semantic edges...");
    let mut edges_created = 0;
    for _ in 0..edge_count {
        if lineage_ids.len() < 2 {
            break;
        }
        let from_idx = rng.random_range(0..lineage_ids.len());
        let mut to_idx = rng.random_range(0..lineage_ids.len());
        // Avoid self-edges
        while to_idx == from_idx {
            to_idx = rng.random_range(0..lineage_ids.len());
        }

        let edge_type = edge_types[rng.random_range(0..edge_types.len())];

        let create = CreateEdge {
            from_lineage_id: lineage_ids[from_idx],
            to_lineage_id: lineage_ids[to_idx],
            edge_type: edge_type.to_string(),
            properties: serde_json::json!({}),
        };

        match db.create_edge(&create).await {
            Ok(_) => edges_created += 1,
            Err(yap_core::Error::Conflict(_)) => {}
            Err(e) => return Err(e.into()),
        }
    }

    let total = block_defs.len() + journal_entries.len() + remaining;
    println!("\nSeed complete:");
    println!(
        "  Namespaces: {} topic + {} date",
        topic_namespaces.len(),
        dates.len()
    );
    println!("  Content blocks: ~{total}");
    println!("  Wiki-links resolved: {links_resolved} (stored as lineage refs in links[])");
    println!("  Wiki-links unresolved: {links_unresolved} (kept as literal text)");
    println!("  Semantic edges: {edges_created}");

    Ok(())
}
