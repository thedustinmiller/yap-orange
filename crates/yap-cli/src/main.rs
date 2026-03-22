//! yap - Command-line interface for yap-orange
//!
//! Provides commands for managing atoms, blocks, edges, and namespaces.

mod client;

use std::collections::HashMap;
use std::io::Read as _;

use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use colored::Colorize;
use uuid::Uuid;

use client::{ApiClient, BlockResponse, ExportTree};

#[derive(Parser)]
#[command(name = "yap")]
#[command(about = "yap-orange note-taking CLI", version)]
struct Cli {
    /// Server URL (can also be set via YAP_SERVER_URL env var)
    #[arg(long, env = "YAP_SERVER_URL", default_value = "http://localhost:3000")]
    server_url: String,

    /// Output as JSON
    #[arg(long, global = true)]
    json: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Block operations
    Block {
        #[command(subcommand)]
        command: BlockCommands,
    },
    /// Atom operations
    Atom {
        #[command(subcommand)]
        command: AtomCommands,
    },
    /// Edge operations
    Edge {
        #[command(subcommand)]
        command: EdgeCommands,
    },
    /// Namespace operations
    Ns {
        #[command(subcommand)]
        command: NsCommands,
    },
    /// Link operations
    Link {
        #[command(subcommand)]
        command: LinkCommands,
    },
    /// Database operations
    Db {
        #[command(subcommand)]
        command: DbCommands,
    },
    /// Schema operations (custom type definitions)
    Schema {
        #[command(subcommand)]
        command: SchemaCommands,
    },
    /// Graph operations
    Graph {
        #[command(subcommand)]
        command: GraphCommands,
    },
    /// Check server health
    Health,
    /// Debug operations
    Debug {
        #[command(subcommand)]
        command: DebugCommands,
    },
    /// Search blocks by name or namespace path
    Search {
        /// Search query
        query: String,
    },
    /// Export a block subtree to JSON
    Export {
        /// Namespace path or block ID of the subtree root
        target: String,
        /// Output file (defaults to stdout)
        #[arg(short = 'o', long)]
        output: Option<String>,
        /// Comma-separated property keys to include (default: all non-underscore)
        #[arg(long)]
        include_keys: Option<String>,
    },
    /// Import a subtree from JSON
    Import {
        /// Path to JSON file produced by `yap export`
        file: String,
        /// Namespace path or block ID to import under (omit for root-level import)
        #[arg(long)]
        parent: Option<String>,
        /// Import mode: merge (default) or copy
        #[arg(long, default_value = "merge")]
        mode: String,
        /// Match strategy: auto, export_hash, content_identity, merkle, topology
        #[arg(long)]
        match_by: Option<String>,
        /// Search globally for matching content to hard-link
        #[arg(long)]
        global_link: bool,
        /// Delete existing matching subtree after successful import
        #[arg(long)]
        replace: bool,
    },
}

#[derive(Subcommand)]
enum BlockCommands {
    /// Create a new block
    Create {
        /// Block content (omit or use "-" to read from stdin)
        content: Option<String>,
        /// Namespace path (parent namespace, not including this block's name)
        #[arg(long)]
        namespace: String,
        /// Block name
        #[arg(long)]
        name: String,
        /// Content type
        #[arg(long, default_value = "content")]
        r#type: String,
        /// Properties as JSON object string (e.g. '{"email":"alice@example.com"}')
        #[arg(long)]
        prop: Option<String>,
    },
    /// Get a block by ID
    Get {
        /// Block ID
        id: String,
    },
    /// List blocks
    List {
        /// Filter by namespace prefix
        #[arg(long)]
        namespace: Option<String>,
        /// Show orphaned blocks
        #[arg(long)]
        orphans: bool,
        /// Search by name or namespace path
        #[arg(long, short = 's')]
        search: Option<String>,
        /// Filter by content type
        #[arg(long)]
        content_type: Option<String>,
        /// Filter by lineage ID
        #[arg(long)]
        lineage_id: Option<String>,
    },
    /// Update a block
    Update {
        /// Block ID
        id: String,
        /// New content (updates the atom)
        #[arg(long)]
        content: Option<String>,
        /// New name
        #[arg(long)]
        name: Option<String>,
        /// New position
        #[arg(long)]
        position: Option<String>,
        /// Properties as JSON object string
        #[arg(long)]
        prop: Option<String>,
    },
    /// Delete a block (soft delete)
    Delete {
        /// Block ID
        id: String,
        /// Recursively delete all descendants
        #[arg(long)]
        recursive: bool,
    },
    /// Restore a deleted block
    Restore {
        /// Block ID
        id: String,
        /// Also restore all deleted descendants
        #[arg(long)]
        recursive: bool,
    },
    /// Move a block to a new parent
    Move {
        /// Block ID
        id: String,
        /// New parent block ID (use "root" for no parent)
        #[arg(long)]
        parent: String,
        /// Position within parent
        #[arg(long)]
        position: Option<String>,
    },
    /// Render blocks as a markdown document with headers
    Tree {
        /// Namespace path to render
        path: String,
        /// Maximum depth to render
        #[arg(long)]
        depth: Option<usize>,
    },
    /// List property keys for a block
    PropertyKeys {
        /// Block ID
        id: String,
    },
}

#[derive(Subcommand)]
enum AtomCommands {
    /// Get an atom by ID
    Get {
        /// Atom ID
        id: String,
        /// Show raw template instead of rendered
        #[arg(long)]
        raw: bool,
    },
    /// Show atoms linking to this atom
    Backlinks {
        /// Atom ID
        id: String,
    },
    /// Show graph neighborhood
    Graph {
        /// Atom ID
        id: String,
    },
    /// Show atoms with edges pointing to this atom
    References {
        /// Atom ID
        id: String,
    },
}

#[derive(Subcommand)]
enum EdgeCommands {
    /// Create a new edge
    Create {
        /// Source atom ID
        from: String,
        /// Target atom ID
        to: String,
        /// Edge type
        r#type: String,
        /// Properties as JSON object string
        #[arg(long)]
        prop: Option<String>,
    },
    /// List edges for an atom
    List {
        /// Atom ID
        id: String,
    },
    /// Delete an edge
    Delete {
        /// Edge ID
        id: String,
    },
}

#[derive(Subcommand)]
enum NsCommands {
    /// Create a namespace (and parents)
    Create {
        /// Namespace path
        path: String,
    },
    /// List all namespaces
    List,
    /// Show namespace tree
    Tree {
        /// Root namespace path (optional, shows all if omitted)
        path: Option<String>,
    },
}

#[derive(Subcommand)]
enum LinkCommands {
    /// Resolve a link path to an atom ID
    Resolve {
        /// Link path
        path: String,
        /// Context namespace for relative resolution
        #[arg(long)]
        from: Option<String>,
    },
}

#[derive(Subcommand)]
enum DbCommands {
    /// Run pending migrations
    Migrate,
    /// Reset database (drop and recreate)
    Reset,
    /// Show migration status
    Status,
}

#[derive(Subcommand)]
enum SchemaCommands {
    /// Create a new schema type definition under types:: namespace
    Create {
        /// Schema name (creates block at types::<name>)
        name: String,
        /// Field definitions as JSON array string
        /// Example: '[{"name":"email","type":"string"},{"name":"role","type":"enum","options":["engineer","manager"]}]'
        #[arg(long)]
        fields: String,
    },
    /// List all schema definitions
    List,
    /// Get a schema definition by name
    Get {
        /// Schema name or full namespace path
        name: String,
    },
    /// Resolve a type name to its schema (with namespace walk-up)
    Resolve {
        /// Type name to resolve
        name: String,
        /// Context namespace for walk-up resolution
        #[arg(long)]
        from: Option<String>,
    },
}

#[derive(Subcommand)]
enum GraphCommands {
    /// Get graph data for a set of lineage IDs
    Subtree {
        /// Lineage IDs (space-separated)
        lineage_ids: Vec<String>,
    },
}

#[derive(Subcommand)]
enum DebugCommands {
    /// Fetch recent server log entries
    Logs {
        /// Only return entries after this ID
        #[arg(long)]
        since: Option<u64>,
    },
    /// Run performance benchmarks
    Benchmarks {
        /// Comma-separated suite names
        #[arg(long)]
        suites: Option<String>,
        /// Random seed
        #[arg(long)]
        seed: Option<u64>,
    },
}

// =============================================================================
// Output Helpers
// =============================================================================

fn print_json<T: serde::Serialize>(value: &T) -> Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

fn parse_uuid(s: &str) -> Result<Uuid> {
    Uuid::parse_str(s).context(format!("Invalid UUID: {}", s))
}

fn truncate(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}...", &s[..max_len - 3])
    }
}

/// Read content from an argument or stdin.
///
/// - `Some("-")` or `None` with piped stdin → reads stdin
/// - `Some(text)` → uses text directly
/// - `None` with TTY stdin → returns empty string
fn read_content(content: Option<&str>) -> Result<String> {
    match content {
        Some("-") => {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .context("Failed to read from stdin")?;
            Ok(buf.trim_end().to_string())
        }
        Some(c) => Ok(c.to_string()),
        None if !atty::is(atty::Stream::Stdin) => {
            let mut buf = String::new();
            std::io::stdin()
                .read_to_string(&mut buf)
                .context("Failed to read from stdin")?;
            Ok(buf.trim_end().to_string())
        }
        None => Ok(String::new()),
    }
}

// =============================================================================
// Block Commands
// =============================================================================

async fn cmd_block_create(
    client: &ApiClient,
    json_output: bool,
    content: &str,
    namespace: &str,
    name: &str,
    content_type: &str,
    properties: Option<serde_json::Value>,
) -> Result<()> {
    let response = client
        .create_block(namespace, name, content, content_type, properties)
        .await?;

    if json_output {
        print_json(&response)?;
    } else {
        println!(
            "{} block: {} (lineage: {})",
            "Created".green().bold(),
            response.block_id.to_string().cyan(),
            response.lineage_id.to_string().cyan()
        );
        println!("Namespace: {}", response.namespace.yellow());
    }

    Ok(())
}

async fn cmd_block_get(client: &ApiClient, json_output: bool, id: &str) -> Result<()> {
    let id = parse_uuid(id)?;
    let block = client.get_block(id).await?;

    if json_output {
        print_json(&block)?;
    } else {
        println!("{}: {}", "ID".bold(), block.id.to_string().cyan());
        println!(
            "{}: {}",
            "Lineage ID".bold(),
            block.lineage_id.to_string().cyan()
        );
        println!("{}: {}", "Namespace".bold(), block.namespace.yellow());
        println!("{}: {}", "Name".bold(), block.name);
        println!("{}: {}", "Type".bold(), block.content_type);
        println!("{}: {}", "Position".bold(), block.position);
        if let Some(parent_id) = block.parent_id {
            println!("{}: {}", "Parent".bold(), parent_id.to_string().cyan());
        }
        println!("{}:", "Content".bold());
        for line in block.content.lines() {
            println!("  {}", line);
        }
    }

    Ok(())
}

async fn cmd_block_list(
    client: &ApiClient,
    json_output: bool,
    namespace: Option<&str>,
    orphans: bool,
    search: Option<&str>,
    content_type: Option<&str>,
    lineage_id: Option<&str>,
) -> Result<()> {
    let blocks = if orphans {
        client.list_orphans().await?
    } else if let Some(lid) = lineage_id {
        let id = parse_uuid(lid)?;
        client.list_blocks_by_lineage_id(id).await?
    } else if let Some(ct) = content_type {
        client.list_blocks_by_content_type(ct).await?
    } else if let Some(query) = search {
        client.search_blocks(query).await?
    } else {
        client.list_blocks(namespace).await?
    };

    if json_output {
        print_json(&blocks)?;
    } else if blocks.is_empty() {
        if orphans {
            println!("{}", "No orphaned blocks found.".dimmed());
        } else {
            println!("{}", "No blocks found.".dimmed());
        }
    } else {
        for block in &blocks {
            let type_indicator =
                if block.content_type != "content" && !block.content_type.is_empty() {
                    format!("({})", block.content_type).blue()
                } else if block.content.is_empty() {
                    "(namespace)".dimmed()
                } else {
                    "".normal()
                };
            println!(
                "{} {} {}",
                block.namespace.yellow(),
                type_indicator,
                truncate(&block.content, 50).dimmed()
            );
        }
        println!("\n{} block(s)", blocks.len().to_string().green());
    }

    Ok(())
}

async fn cmd_block_update(
    client: &ApiClient,
    json_output: bool,
    id: &str,
    content: Option<&str>,
    name: Option<&str>,
    position: Option<&str>,
    properties: Option<serde_json::Value>,
) -> Result<()> {
    let id = parse_uuid(id)?;

    // If content or properties are provided, update the atom
    if content.is_some() || properties.is_some() {
        let block = client.get_block(id).await?;
        let content_str = content.unwrap_or(&block.content);
        client
            .update_atom_full(block.lineage_id, content_str, None, properties)
            .await?;
    }

    // Update block metadata if name or position provided
    let block = if name.is_some() || position.is_some() {
        client.update_block(id, name, position).await?
    } else {
        client.get_block(id).await?
    };

    if json_output {
        print_json(&block)?;
    } else {
        println!(
            "{} block: {}",
            "Updated".green().bold(),
            id.to_string().cyan()
        );
        println!("Namespace: {}", block.namespace.yellow());
    }

    Ok(())
}

async fn cmd_block_delete(
    client: &ApiClient,
    json_output: bool,
    id: &str,
    recursive: bool,
) -> Result<()> {
    let id = parse_uuid(id)?;

    if recursive {
        client.delete_block_recursive(id).await?;
    } else {
        client.delete_block(id).await?;
    }

    if json_output {
        print_json(&serde_json::json!({"deleted": id.to_string(), "recursive": recursive}))?;
    } else if recursive {
        println!(
            "{} block: {}",
            "Recursively deleted".red().bold(),
            id.to_string().cyan()
        );
    } else {
        println!(
            "{} block: {}",
            "Deleted".red().bold(),
            id.to_string().cyan()
        );
    }

    Ok(())
}

async fn cmd_block_restore(
    client: &ApiClient,
    json_output: bool,
    id: &str,
    recursive: bool,
) -> Result<()> {
    let id = parse_uuid(id)?;

    if recursive {
        let result = client.restore_block_recursive(id).await?;
        if json_output {
            print_json(&result)?;
        } else {
            let count = result.get("restored").and_then(|v| v.as_u64()).unwrap_or(0);
            println!(
                "{} {} block(s) recursively: {}",
                "Restored".green().bold(),
                count,
                id.to_string().cyan()
            );
        }
    } else {
        let block = client.restore_block(id).await?;
        if json_output {
            print_json(&block)?;
        } else {
            println!(
                "{} block: {}",
                "Restored".green().bold(),
                id.to_string().cyan()
            );
            println!("Namespace: {}", block.namespace.yellow());
        }
    }

    Ok(())
}

async fn cmd_block_move(
    client: &ApiClient,
    json_output: bool,
    id: &str,
    parent: &str,
    position: Option<&str>,
) -> Result<()> {
    let id = parse_uuid(id)?;
    let parent_id = if parent.to_lowercase() == "root" {
        None
    } else {
        Some(parse_uuid(parent)?)
    };

    let block = client.move_block(id, parent_id, position).await?;

    if json_output {
        print_json(&block)?;
    } else {
        println!(
            "{} block: {}",
            "Moved".green().bold(),
            id.to_string().cyan()
        );
        println!("New namespace: {}", block.namespace.yellow());
    }

    Ok(())
}

async fn cmd_block_tree(
    client: &ApiClient,
    json_output: bool,
    path: &str,
    max_depth: Option<usize>,
) -> Result<()> {
    let blocks = client.list_blocks(Some(path)).await?;

    if json_output {
        print_json(&blocks)?;
        return Ok(());
    }

    if blocks.is_empty() {
        println!("{}", "No blocks found.".dimmed());
        return Ok(());
    }

    // Build content tree nodes
    let tree = build_content_tree(&blocks, path);
    print_markdown_tree(&tree, 1, max_depth);

    Ok(())
}

/// Tree node for markdown rendering (includes content)
struct ContentTreeNode {
    name: String,
    content: String,
    children: Vec<ContentTreeNode>,
}

fn build_content_tree(blocks: &[BlockResponse], root_path: &str) -> Vec<ContentTreeNode> {
    let mut ns_map: HashMap<String, (&str, &str)> = HashMap::new(); // namespace -> (name, content)
    for block in blocks {
        ns_map.insert(block.namespace.clone(), (&block.name, &block.content));
    }

    let mut namespaces: Vec<&String> = ns_map.keys().collect();
    namespaces.sort();

    let filtered: Vec<&&String> = namespaces
        .iter()
        .filter(|ns| ns.starts_with(root_path))
        .collect();

    // Find root-level nodes
    let mut roots = Vec::new();
    for ns in &filtered {
        let parent_ns = get_parent_namespace(ns);
        let is_root = match &parent_ns {
            Some(p) => !p.starts_with(root_path) || p.len() < root_path.len(),
            None => true,
        };

        if is_root {
            roots.push(build_content_node(ns, &ns_map, &filtered));
        }
    }

    roots
}

fn build_content_node(
    namespace: &str,
    ns_map: &HashMap<String, (&str, &str)>,
    all_namespaces: &[&&String],
) -> ContentTreeNode {
    let (name, content) = ns_map
        .get(namespace)
        .map(|(n, c)| (n.to_string(), c.to_string()))
        .unwrap_or_else(|| (namespace.to_string(), String::new()));

    let prefix = format!("{}::", namespace);
    let mut children = Vec::new();

    for ns in all_namespaces {
        if ns.starts_with(&prefix) {
            let suffix = &ns[prefix.len()..];
            if !suffix.contains("::") {
                children.push(build_content_node(ns, ns_map, all_namespaces));
            }
        }
    }

    children.sort_by(|a, b| a.name.cmp(&b.name));

    ContentTreeNode {
        name,
        content,
        children,
    }
}

fn print_markdown_tree(nodes: &[ContentTreeNode], depth: usize, max_depth: Option<usize>) {
    if let Some(max) = max_depth
        && depth > max
    {
        return;
    }

    for node in nodes {
        let hashes = "#".repeat(depth);
        println!("{} {}", hashes, node.name);

        if !node.content.is_empty() {
            println!();
            println!("{}", node.content);
        }

        println!();
        print_markdown_tree(&node.children, depth + 1, max_depth);
    }
}

// =============================================================================
// Search Command
// =============================================================================

async fn cmd_search(client: &ApiClient, json_output: bool, query: &str) -> Result<()> {
    let blocks = client.search_blocks(query).await?;

    if json_output {
        print_json(&blocks)?;
    } else if blocks.is_empty() {
        println!("{}", "No blocks found.".dimmed());
    } else {
        for block in &blocks {
            let type_indicator =
                if block.content_type != "content" && !block.content_type.is_empty() {
                    format!(" ({})", block.content_type).blue()
                } else if block.content.is_empty() {
                    " (namespace)".dimmed()
                } else {
                    "".normal()
                };
            println!(
                "{}{}  {}",
                block.namespace.yellow(),
                type_indicator,
                truncate(&block.content, 50).dimmed()
            );
        }
        println!("\n{} result(s)", blocks.len().to_string().green());
    }

    Ok(())
}

// =============================================================================
// Atom Commands
// =============================================================================

async fn cmd_atom_get(client: &ApiClient, json_output: bool, id: &str, raw: bool) -> Result<()> {
    let id = parse_uuid(id)?;

    if raw {
        let atom = client.get_atom(id).await?;

        if json_output {
            print_json(&atom)?;
        } else {
            println!("{}: {}", "ID".bold(), atom.id.to_string().cyan());
            println!("{}: {}", "Type".bold(), atom.content_type);
            println!("{}:", "Template".bold());
            for line in atom.content_template.lines() {
                println!("  {}", line);
            }
            println!("{}:", "Links".bold());
            for (i, link) in atom.links.iter().enumerate() {
                println!("  {}: {}", i, link.to_string().cyan());
            }
        }
    } else {
        let atom = client.get_atom_rendered(id).await?;

        if json_output {
            print_json(&atom)?;
        } else {
            println!("{}: {}", "ID".bold(), atom.id.to_string().cyan());
            println!("{}: {}", "Type".bold(), atom.content_type);
            println!("{}:", "Content".bold());
            for line in atom.content.lines() {
                println!("  {}", line);
            }
        }
    }

    Ok(())
}

async fn cmd_atom_backlinks(client: &ApiClient, json_output: bool, id: &str) -> Result<()> {
    let id = parse_uuid(id)?;
    let backlinks = client.get_backlinks(id).await?;

    if json_output {
        print_json(&backlinks)?;
    } else if backlinks.is_empty() {
        println!("{}", "No backlinks found.".dimmed());
    } else {
        println!("{}:", "Atoms linking to this".bold());
        for bl in &backlinks {
            let ns = bl.namespace.as_deref().unwrap_or("(no namespace)");
            println!("  {} {}", ns.yellow(), truncate(&bl.content, 60).dimmed());
        }
        println!("\n{} backlink(s)", backlinks.len().to_string().green());
    }

    Ok(())
}

async fn cmd_atom_graph(client: &ApiClient, json_output: bool, id: &str) -> Result<()> {
    let id = parse_uuid(id)?;
    let graph = client.get_graph(id).await?;

    if json_output {
        print_json(&graph)?;
    } else {
        println!("{}:", "Atom".bold());
        println!("  ID: {}", graph.atom.id.to_string().cyan());
        println!("  Type: {}", graph.atom.content_type);
        println!("  Content: {}", truncate(&graph.atom.content, 60));

        println!("\n{} ({}):", "Outlinks".bold(), graph.outlinks.len());
        for link in &graph.outlinks {
            let ns = link.namespace.as_deref().unwrap_or("(no namespace)");
            println!(
                "  -> {} {}",
                ns.yellow(),
                truncate(&link.content, 40).dimmed()
            );
        }

        println!("\n{} ({}):", "Backlinks".bold(), graph.backlinks.len());
        for link in &graph.backlinks {
            let ns = link.namespace.as_deref().unwrap_or("(no namespace)");
            println!(
                "  <- {} {}",
                ns.yellow(),
                truncate(&link.content, 40).dimmed()
            );
        }

        println!(
            "\n{} ({} out, {} in):",
            "Edges".bold(),
            graph.edges.outgoing.len(),
            graph.edges.incoming.len()
        );
        for edge in &graph.edges.outgoing {
            println!(
                "  --{}-> {}",
                edge.edge_type.blue(),
                edge.to_lineage_id.to_string().cyan()
            );
        }
        for edge in &graph.edges.incoming {
            println!(
                "  <-{}- {}",
                edge.edge_type.blue(),
                edge.from_lineage_id.to_string().cyan()
            );
        }
    }

    Ok(())
}

// =============================================================================
// Edge Commands
// =============================================================================

async fn cmd_edge_create(
    client: &ApiClient,
    json_output: bool,
    from: &str,
    to: &str,
    edge_type: &str,
    properties: serde_json::Value,
) -> Result<()> {
    let from_id = parse_uuid(from)?;
    let to_id = parse_uuid(to)?;

    let edge = client
        .create_edge(from_id, to_id, edge_type, properties)
        .await?;

    if json_output {
        print_json(&edge)?;
    } else {
        println!(
            "{} edge: {}",
            "Created".green().bold(),
            edge.id.to_string().cyan()
        );
        println!(
            "{} --{}-> {}",
            from_id.to_string().cyan(),
            edge_type.blue(),
            to_id.to_string().cyan()
        );
    }

    Ok(())
}

async fn cmd_edge_list(client: &ApiClient, json_output: bool, id: &str) -> Result<()> {
    let id = parse_uuid(id)?;
    let edges = client.get_atom_edges(id).await?;

    if json_output {
        print_json(&edges)?;
    } else {
        println!("{}:", "Outgoing".bold());
        if edges.outgoing.is_empty() {
            println!("  {}", "(none)".dimmed());
        } else {
            for edge in &edges.outgoing {
                println!(
                    "  {} -> {} ({})",
                    edge.edge_type.blue(),
                    edge.to_lineage_id.to_string().cyan(),
                    edge.id.to_string().dimmed()
                );
            }
        }

        println!("\n{}:", "Incoming".bold());
        if edges.incoming.is_empty() {
            println!("  {}", "(none)".dimmed());
        } else {
            for edge in &edges.incoming {
                println!(
                    "  {} <- {} ({})",
                    edge.edge_type.blue(),
                    edge.from_lineage_id.to_string().cyan(),
                    edge.id.to_string().dimmed()
                );
            }
        }
    }

    Ok(())
}

async fn cmd_edge_delete(client: &ApiClient, json_output: bool, id: &str) -> Result<()> {
    let id = parse_uuid(id)?;
    client.delete_edge(id).await?;

    if json_output {
        print_json(&serde_json::json!({"deleted": id.to_string()}))?;
    } else {
        println!("{} edge: {}", "Deleted".red().bold(), id.to_string().cyan());
    }

    Ok(())
}

// =============================================================================
// Namespace Commands
// =============================================================================

async fn cmd_ns_create(client: &ApiClient, json_output: bool, path: &str) -> Result<()> {
    let response = client.create_namespace(path).await?;

    if json_output {
        print_json(&response)?;
    } else {
        println!(
            "{} namespace: {}",
            "Created".green().bold(),
            response.namespace.yellow()
        );
        println!("Block ID: {}", response.block_id.to_string().cyan());
    }

    Ok(())
}

async fn cmd_ns_list(client: &ApiClient, json_output: bool) -> Result<()> {
    let namespaces = client.list_roots().await?;

    if json_output {
        print_json(&namespaces)?;
    } else if namespaces.is_empty() {
        println!("{}", "No namespaces found.".dimmed());
    } else {
        for ns in &namespaces {
            println!("{}", ns.namespace.yellow());
        }
        println!("\n{} namespace(s)", namespaces.len().to_string().green());
    }

    Ok(())
}

async fn cmd_ns_tree(client: &ApiClient, json_output: bool, root_path: Option<&str>) -> Result<()> {
    let all_blocks = match root_path {
        Some(path) => client.list_blocks(Some(path)).await?,
        None => {
            // Get all blocks by listing without namespace filter
            // We need to get the full tree, so we fetch root blocks and children recursively
            client.list_blocks(None).await?
        }
    };

    if json_output {
        print_json(&all_blocks)?;
        return Ok(());
    }

    if all_blocks.is_empty() {
        println!("{}", "No blocks found.".dimmed());
        return Ok(());
    }

    // Build a tree structure
    let tree = build_namespace_tree(&all_blocks, root_path);
    print_tree(&tree, "", true);

    Ok(())
}

/// Tree node for namespace display
struct TreeNode {
    name: String,
    content_type: String,
    children: Vec<TreeNode>,
}

fn build_namespace_tree(blocks: &[BlockResponse], root_path: Option<&str>) -> Vec<TreeNode> {
    // Build a map of namespace -> block info
    let mut ns_map: HashMap<String, (String, String)> = HashMap::new(); // namespace -> (name, content_type)
    for block in blocks {
        ns_map.insert(
            block.namespace.clone(),
            (block.name.clone(), block.content_type.clone()),
        );
    }

    // Build tree structure from namespaces
    let mut root_nodes: Vec<TreeNode> = Vec::new();

    // Sort namespaces for consistent ordering
    let mut namespaces: Vec<&String> = ns_map.keys().collect();
    namespaces.sort();

    // Filter by root path if provided
    let filtered: Vec<&&String> = if let Some(root) = root_path {
        namespaces
            .iter()
            .filter(|ns| ns.starts_with(root))
            .collect()
    } else {
        namespaces.iter().collect()
    };

    // Find root-level namespaces (those with no parent in our filtered set)
    for ns in &filtered {
        let parent_ns = get_parent_namespace(ns);
        let is_root = match &parent_ns {
            Some(p) => {
                if let Some(root) = root_path {
                    !p.starts_with(root) || p.len() < root.len()
                } else {
                    !ns_map.contains_key(p)
                }
            }
            None => true,
        };

        if is_root {
            let node = build_tree_node(ns, &ns_map, &filtered);
            root_nodes.push(node);
        }
    }

    root_nodes
}

fn get_parent_namespace(ns: &str) -> Option<String> {
    let parts: Vec<&str> = ns.split("::").collect();
    if parts.len() > 1 {
        Some(parts[..parts.len() - 1].join("::"))
    } else {
        None
    }
}

fn build_tree_node(
    namespace: &str,
    ns_map: &HashMap<String, (String, String)>,
    all_namespaces: &[&&String],
) -> TreeNode {
    let (name, content_type) = ns_map
        .get(namespace)
        .cloned()
        .unwrap_or_else(|| (namespace.to_string(), "unknown".to_string()));

    // Find children
    let prefix = format!("{}::", namespace);
    let mut children: Vec<TreeNode> = Vec::new();

    for ns in all_namespaces {
        if ns.starts_with(&prefix) {
            // Check if this is a direct child (no additional :: separators after the prefix)
            let suffix = &ns[prefix.len()..];
            if !suffix.contains("::") {
                children.push(build_tree_node(ns, ns_map, all_namespaces));
            }
        }
    }

    // Sort children by name
    children.sort_by(|a, b| a.name.cmp(&b.name));

    TreeNode {
        name,
        content_type,
        children,
    }
}

fn print_tree(nodes: &[TreeNode], prefix: &str, _is_last_set: bool) {
    for (i, node) in nodes.iter().enumerate() {
        let is_last = i == nodes.len() - 1;
        let connector = if is_last { "└── " } else { "├── " };

        let type_indicator = if node.content_type != "content" && !node.content_type.is_empty() {
            format!(" ({})", node.content_type).blue()
        } else {
            "".normal()
        };

        println!(
            "{}{}{}{}",
            prefix,
            connector,
            node.name.yellow(),
            type_indicator
        );

        let new_prefix = if is_last {
            format!("{}    ", prefix)
        } else {
            format!("{}│   ", prefix)
        };

        print_tree(&node.children, &new_prefix, is_last);
    }
}

// =============================================================================
// Link Commands
// =============================================================================

async fn cmd_link_resolve(
    client: &ApiClient,
    json_output: bool,
    path: &str,
    from_namespace: Option<&str>,
) -> Result<()> {
    let response = client.resolve_link(path, from_namespace).await?;

    if json_output {
        print_json(&response)?;
    } else {
        println!(
            "{}: {}",
            "Lineage".bold(),
            response.lineage_id.to_string().cyan()
        );
        println!(
            "{}: {}",
            "Block".bold(),
            response.block_id.to_string().cyan()
        );
        println!("{}: {}", "Namespace".bold(), response.namespace.yellow());
    }

    Ok(())
}

// =============================================================================
// Database Commands
// =============================================================================

async fn cmd_db_migrate(json_output: bool) -> Result<()> {
    use sqlx::migrate::Migrator;
    use sqlx::postgres::PgPoolOptions;
    use std::path::Path;

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://yap:yap@localhost:5432/yap".to_string());

    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await
        .context("Failed to connect to database")?;

    let migrator = Migrator::new(Path::new("./migrations"))
        .await
        .context("Failed to load migrations")?;

    migrator
        .run(&pool)
        .await
        .context("Failed to run migrations")?;

    if json_output {
        print_json(&serde_json::json!({
            "status": "success",
            "message": "Migrations applied successfully"
        }))?;
    } else {
        println!("{} migrations applied", "Successfully".green().bold());
    }

    Ok(())
}

async fn cmd_db_reset(json_output: bool) -> Result<()> {
    use dialoguer::Confirm;
    use sqlx::postgres::PgPoolOptions;

    // Confirm before proceeding
    if !json_output {
        let confirmed = Confirm::new()
            .with_prompt("WARNING: This will delete all data. Continue?")
            .default(false)
            .interact()
            .context("Failed to get confirmation")?;

        if !confirmed {
            println!("{}", "Aborted.".yellow());
            return Ok(());
        }
    }

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://yap:yap@localhost:5432/yap".to_string());

    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await
        .context("Failed to connect to database")?;

    // Drop all tables
    sqlx::query("DROP TABLE IF EXISTS edges CASCADE")
        .execute(&pool)
        .await
        .context("Failed to drop edges table")?;

    sqlx::query("DROP TABLE IF EXISTS blocks CASCADE")
        .execute(&pool)
        .await
        .context("Failed to drop blocks table")?;

    sqlx::query("DROP TABLE IF EXISTS lineages CASCADE")
        .execute(&pool)
        .await
        .context("Failed to drop lineages table")?;

    sqlx::query("DROP TABLE IF EXISTS atoms CASCADE")
        .execute(&pool)
        .await
        .context("Failed to drop atoms table")?;

    sqlx::query("DROP TABLE IF EXISTS _sqlx_migrations CASCADE")
        .execute(&pool)
        .await
        .context("Failed to drop migrations table")?;

    // Run migrations
    use sqlx::migrate::Migrator;
    use std::path::Path;

    let migrator = Migrator::new(Path::new("./migrations"))
        .await
        .context("Failed to load migrations")?;

    migrator
        .run(&pool)
        .await
        .context("Failed to run migrations")?;

    if json_output {
        print_json(&serde_json::json!({
            "status": "success",
            "message": "Database reset complete"
        }))?;
    } else {
        println!("{}", "Database reset complete.".green().bold());
    }

    Ok(())
}

async fn cmd_db_status(json_output: bool) -> Result<()> {
    use sqlx::postgres::PgPoolOptions;

    let database_url = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://yap:yap@localhost:5432/yap".to_string());

    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&database_url)
        .await
        .context("Failed to connect to database")?;

    // Check if migrations table exists
    let migrations: Vec<(i64, String, chrono::DateTime<chrono::Utc>)> = sqlx::query_as(
        r#"
        SELECT version, description, installed_on
        FROM _sqlx_migrations
        ORDER BY version ASC
        "#,
    )
    .fetch_all(&pool)
    .await
    .unwrap_or_default();

    if json_output {
        let migration_data: Vec<serde_json::Value> = migrations
            .iter()
            .map(|(version, description, installed_on)| {
                serde_json::json!({
                    "version": version,
                    "description": description,
                    "installed_on": installed_on.to_rfc3339()
                })
            })
            .collect();
        print_json(&serde_json::json!({
            "connected": true,
            "migrations": migration_data
        }))?;
    } else {
        println!("{}: {}", "Database".bold(), "connected".green());
        println!("\n{}:", "Applied migrations".bold());

        if migrations.is_empty() {
            println!("  {}", "(none)".dimmed());
        } else {
            for (version, description, installed_on) in &migrations {
                println!(
                    "  {} {} ({})",
                    version.to_string().cyan(),
                    description,
                    installed_on.format("%Y-%m-%d %H:%M:%S")
                );
            }
        }
    }

    Ok(())
}

// =============================================================================
// Export / Import Commands
// =============================================================================

/// Resolve a target string (namespace path or UUID) to a block ID.
async fn resolve_target_to_block_id(client: &ApiClient, target: &str) -> Result<uuid::Uuid> {
    // Try UUID first
    if let Ok(id) = uuid::Uuid::parse_str(target) {
        return Ok(id);
    }
    // Resolve as namespace path
    let resolved = client
        .resolve_link(target, None)
        .await
        .with_context(|| format!("Could not resolve '{}' to a block", target))?;
    Ok(resolved.block_id)
}

async fn cmd_export(
    client: &ApiClient,
    json_output: bool,
    target: &str,
    output: Option<&str>,
    include_keys: Option<&str>,
) -> Result<()> {
    let block_id = resolve_target_to_block_id(client, target).await?;
    let tree = client.export_tree(block_id, include_keys).await?;
    let json = serde_json::to_string_pretty(&tree)?;

    match output {
        Some(path) => {
            std::fs::write(path, &json).with_context(|| format!("Failed to write to {}", path))?;
            if !json_output {
                println!("{} to {}", "Exported".green().bold(), path.yellow());
                println!("  {} nodes, {} edges", tree.nodes.len(), tree.edges.len());
                println!("  Source: {}", tree.source_namespace.yellow());
            } else {
                print_json(&serde_json::json!({
                    "output": path,
                    "nodes": tree.nodes.len(),
                    "edges": tree.edges.len(),
                    "source_namespace": tree.source_namespace,
                }))?;
            }
        }
        None => {
            // Output JSON to stdout regardless of --json flag
            println!("{}", json);
        }
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn cmd_import(
    client: &ApiClient,
    json_output: bool,
    file: &str,
    parent: Option<&str>,
    mode: &str,
    match_by: Option<&str>,
    global_link: bool,
    replace: bool,
) -> Result<()> {
    let json = std::fs::read_to_string(file).with_context(|| format!("Failed to read {}", file))?;
    let tree: ExportTree = serde_json::from_str(&json)
        .with_context(|| format!("Failed to parse {} as export tree", file))?;

    let result = match parent {
        Some(p) => {
            let parent_id = resolve_target_to_block_id(client, p).await?;
            client
                .import_tree(parent_id, &tree, mode, match_by, global_link, replace)
                .await?
        }
        None => {
            client
                .import_tree_at_root(&tree, mode, match_by, global_link, replace)
                .await?
        }
    };

    if json_output {
        print_json(&result)?;
    } else {
        println!("{}", "Import complete.".green().bold());
        println!("  Created: {}", result.created.to_string().green());
        println!("  Skipped: {}", result.skipped.to_string().yellow());
        if result.linked > 0 {
            println!("  Linked:  {}", result.linked.to_string().blue());
        }
        if let Some(root_id) = result.root_block_id {
            println!("  Root block: {}", root_id.to_string().cyan());
        }
        if result.edges_created > 0 || result.edges_skipped > 0 {
            println!(
                "  Edges created: {}",
                result.edges_created.to_string().green()
            );
            if result.edges_skipped > 0 {
                println!(
                    "  Edges skipped: {}",
                    result.edges_skipped.to_string().yellow()
                );
            }
        }
        if !result.failed_external_links.is_empty() {
            println!(
                "  {} unresolved external link(s):",
                result.failed_external_links.len().to_string().red()
            );
            for f in &result.failed_external_links {
                println!(
                    "    node={} placeholder={} path={}",
                    f.node_local_id,
                    f.placeholder_index,
                    f.target_path.yellow()
                );
            }
        }
        if !result.edges_failed.is_empty() {
            println!(
                "  {} failed edge(s):",
                result.edges_failed.len().to_string().red()
            );
            for f in &result.edges_failed {
                println!(
                    "    {}→{} type={} reason={}",
                    f.from_local_id,
                    f.to_local_id,
                    f.edge_type.yellow(),
                    f.reason.red()
                );
            }
        }
    }

    Ok(())
}

// =============================================================================
// Main
// =============================================================================

#[tokio::main]
async fn main() -> Result<()> {
    let cli = Cli::parse();
    let client = ApiClient::new(&cli.server_url);

    let result = match cli.command {
        Commands::Block { command } => match command {
            BlockCommands::Create {
                content,
                namespace,
                name,
                r#type,
                prop,
            } => {
                let content = read_content(content.as_deref())?;
                let properties = if let Some(p) = prop {
                    Some(
                        serde_json::from_str::<serde_json::Value>(&p)
                            .context("--prop must be a valid JSON object")?,
                    )
                } else {
                    None
                };
                cmd_block_create(
                    &client, cli.json, &content, &namespace, &name, &r#type, properties,
                )
                .await
            }
            BlockCommands::Get { id } => cmd_block_get(&client, cli.json, &id).await,
            BlockCommands::List {
                namespace,
                orphans,
                search,
                content_type,
                lineage_id,
            } => {
                cmd_block_list(
                    &client,
                    cli.json,
                    namespace.as_deref(),
                    orphans,
                    search.as_deref(),
                    content_type.as_deref(),
                    lineage_id.as_deref(),
                )
                .await
            }
            BlockCommands::Update {
                id,
                content,
                name,
                position,
                prop,
            } => {
                let content = match content.as_deref() {
                    Some("-") => Some(read_content(Some("-"))?),
                    other => other.map(String::from),
                };
                let properties = if let Some(p) = prop {
                    Some(
                        serde_json::from_str::<serde_json::Value>(&p)
                            .context("--prop must be a valid JSON object")?,
                    )
                } else {
                    None
                };
                cmd_block_update(
                    &client,
                    cli.json,
                    &id,
                    content.as_deref(),
                    name.as_deref(),
                    position.as_deref(),
                    properties,
                )
                .await
            }
            BlockCommands::Delete { id, recursive } => {
                cmd_block_delete(&client, cli.json, &id, recursive).await
            }
            BlockCommands::Restore { id, recursive } => {
                cmd_block_restore(&client, cli.json, &id, recursive).await
            }
            BlockCommands::Move {
                id,
                parent,
                position,
            } => cmd_block_move(&client, cli.json, &id, &parent, position.as_deref()).await,
            BlockCommands::Tree { path, depth } => {
                cmd_block_tree(&client, cli.json, &path, depth).await
            }
            BlockCommands::PropertyKeys { id } => {
                let id = parse_uuid(&id)?;
                let keys = client.get_property_keys(id).await?;
                if cli.json {
                    print_json(&keys)?;
                } else if keys.is_empty() {
                    println!("{}", "No property keys.".dimmed());
                } else {
                    for key in &keys {
                        println!("{}", key);
                    }
                    println!("\n{} key(s)", keys.len().to_string().green());
                }
                Ok(())
            }
        },

        Commands::Atom { command } => match command {
            AtomCommands::Get { id, raw } => cmd_atom_get(&client, cli.json, &id, raw).await,
            AtomCommands::Backlinks { id } => cmd_atom_backlinks(&client, cli.json, &id).await,
            AtomCommands::Graph { id } => cmd_atom_graph(&client, cli.json, &id).await,
            AtomCommands::References { id } => {
                let id = parse_uuid(&id)?;
                let refs = client.get_references(id).await?;
                if cli.json {
                    print_json(&refs)?;
                } else if refs.is_empty() {
                    println!("{}", "No references found.".dimmed());
                } else {
                    println!("{}:", "Atoms referencing this (edges)".bold());
                    for r in &refs {
                        let ns = r.namespace.as_deref().unwrap_or("(no namespace)");
                        println!("  {} {}", ns.yellow(), truncate(&r.content, 60).dimmed());
                    }
                    println!("\n{} reference(s)", refs.len().to_string().green());
                }
                Ok(())
            }
        },

        Commands::Edge { command } => match command {
            EdgeCommands::Create {
                from,
                to,
                r#type,
                prop,
            } => {
                let properties = if let Some(p) = prop {
                    serde_json::from_str::<serde_json::Value>(&p)
                        .context("--prop must be a valid JSON object")?
                } else {
                    serde_json::json!({})
                };
                cmd_edge_create(&client, cli.json, &from, &to, &r#type, properties).await
            }
            EdgeCommands::List { id } => cmd_edge_list(&client, cli.json, &id).await,
            EdgeCommands::Delete { id } => cmd_edge_delete(&client, cli.json, &id).await,
        },

        Commands::Ns { command } => match command {
            NsCommands::Create { path } => cmd_ns_create(&client, cli.json, &path).await,
            NsCommands::List => cmd_ns_list(&client, cli.json).await,
            NsCommands::Tree { path } => cmd_ns_tree(&client, cli.json, path.as_deref()).await,
        },

        Commands::Link { command } => match command {
            LinkCommands::Resolve { path, from } => {
                cmd_link_resolve(&client, cli.json, &path, from.as_deref()).await
            }
        },

        Commands::Db { command } => match command {
            DbCommands::Migrate => cmd_db_migrate(cli.json).await,
            DbCommands::Reset => cmd_db_reset(cli.json).await,
            DbCommands::Status => cmd_db_status(cli.json).await,
        },

        Commands::Schema { command } => match command {
            SchemaCommands::Create { name, fields } => {
                // Parse fields JSON
                let fields_value: serde_json::Value =
                    serde_json::from_str(&fields).context("--fields must be a valid JSON array")?;

                // Create schema block at types::<name>
                let properties = serde_json::json!({ "fields": fields_value });
                let response = client
                    .create_block(
                        "types",
                        &name,
                        &format!("Schema: {}", name),
                        "schema",
                        Some(properties),
                    )
                    .await?;

                if cli.json {
                    print_json(&response)?;
                } else {
                    println!("{} {}", "Created schema:".green().bold(), name);
                    println!("  Block ID:  {}", response.block_id);
                    println!("  Lineage:   {}", response.lineage_id);
                    println!("  Namespace: {}", response.namespace);
                }
                Ok(())
            }

            SchemaCommands::List => {
                let schemas = client.list_schemas().await?;
                if cli.json {
                    print_json(&schemas)?;
                } else if schemas.is_empty() {
                    println!(
                        "No schemas defined. Create one with `yap schema create <name> --fields '<json>'`"
                    );
                } else {
                    println!("{:<20} {:<8} FIELDS", "NAME", "VERSION");
                    println!("{}", "-".repeat(50));
                    for schema in &schemas {
                        let field_count = schema.fields.as_array().map(|a| a.len()).unwrap_or(0);
                        println!(
                            "{:<20} {:<8} {} fields",
                            schema.name, schema.version, field_count
                        );
                    }
                }
                Ok(())
            }

            SchemaCommands::Get { name } => {
                let schemas = client.list_schemas().await?;
                let schema = schemas
                    .iter()
                    .find(|s| s.name == name || s.namespace == name)
                    .ok_or_else(|| anyhow::anyhow!("Schema '{}' not found", name))?;

                if cli.json {
                    print_json(schema)?;
                } else {
                    println!("{} {}", "Schema:".green().bold(), schema.name);
                    println!("  Namespace: {}", schema.namespace);
                    println!("  Version:   {}", schema.version);
                    println!("  Lineage:   {}", schema.lineage_id);
                    println!();
                    println!("Fields:");
                    if let Some(fields) = schema.fields.as_array() {
                        for field in fields {
                            let fname = field.get("name").and_then(|v| v.as_str()).unwrap_or("?");
                            let ftype = field.get("type").and_then(|v| v.as_str()).unwrap_or("?");
                            let required = field
                                .get("required")
                                .and_then(|v| v.as_bool())
                                .unwrap_or(false);
                            let req_str = if required { " (required)" } else { "" };
                            print!("  - {}: {}{}", fname, ftype, req_str);
                            if let Some(opts) = field.get("options").and_then(|v| v.as_array()) {
                                let opt_strs: Vec<&str> =
                                    opts.iter().filter_map(|v| v.as_str()).collect();
                                print!(" [{}]", opt_strs.join(", "));
                            }
                            println!();
                        }
                    } else {
                        println!("  (no fields defined)");
                    }
                }
                Ok(())
            }

            SchemaCommands::Resolve { name, from } => {
                let schema = client.resolve_schema(&name, from.as_deref()).await?;
                if cli.json {
                    print_json(&schema)?;
                } else {
                    println!(
                        "{} {} -> {}",
                        "Resolved:".green().bold(),
                        name,
                        schema.namespace
                    );
                    println!("  Lineage:  {}", schema.lineage_id);
                    println!("  Version:  {}", schema.version);
                    let field_count = schema.fields.as_array().map(|a| a.len()).unwrap_or(0);
                    println!("  Fields:   {} defined", field_count);
                }
                Ok(())
            }
        },

        Commands::Graph { command } => match command {
            GraphCommands::Subtree { lineage_ids } => {
                let ids: Vec<Uuid> = lineage_ids
                    .iter()
                    .map(|s| parse_uuid(s))
                    .collect::<Result<Vec<_>>>()?;
                let graph = client.get_subtree_graph(&ids).await?;
                if cli.json {
                    print_json(&graph)?;
                } else {
                    println!(
                        "{}: {}",
                        "Content links".bold(),
                        graph.content_links.len().to_string().green()
                    );
                    for link in &graph.content_links {
                        println!(
                            "  {} -> {}",
                            link.from_lineage_id.to_string().cyan(),
                            link.to_lineage_id.to_string().cyan()
                        );
                    }
                    println!(
                        "{}: {}",
                        "Edges".bold(),
                        graph.edges.len().to_string().green()
                    );
                    for edge in &graph.edges {
                        println!(
                            "  {} --{}-> {}",
                            edge.from_lineage_id.to_string().cyan(),
                            edge.edge_type.blue(),
                            edge.to_lineage_id.to_string().cyan()
                        );
                    }
                }
                Ok(())
            }
        },

        Commands::Health => {
            let response = client.health().await?;
            if cli.json {
                print_json(&response)?;
            } else {
                println!(
                    "Server: {} (database: {})",
                    response.status.green(),
                    response.database.green()
                );
            }
            Ok(())
        }

        Commands::Debug { command } => match command {
            DebugCommands::Logs { since } => {
                let entries = client.get_debug_logs(since).await?;
                if cli.json {
                    print_json(&entries)?;
                } else if entries.is_empty() {
                    println!("{}", "No log entries.".dimmed());
                } else {
                    for entry in &entries {
                        println!("[{}] {}: {}", entry.level, entry.target, entry.message);
                    }
                }
                Ok(())
            }
            DebugCommands::Benchmarks { suites, seed } => {
                let result = client.run_benchmarks(suites.as_deref(), seed).await?;
                print_json(&result)?;
                Ok(())
            }
        },

        Commands::Search { query } => cmd_search(&client, cli.json, &query).await,

        Commands::Export {
            target,
            output,
            include_keys,
        } => {
            cmd_export(
                &client,
                cli.json,
                &target,
                output.as_deref(),
                include_keys.as_deref(),
            )
            .await
        }

        Commands::Import {
            file,
            parent,
            mode,
            match_by,
            global_link,
            replace,
        } => {
            cmd_import(
                &client,
                cli.json,
                &file,
                parent.as_deref(),
                &mode,
                match_by.as_deref(),
                global_link,
                replace,
            )
            .await
        }
    };

    if let Err(e) = result {
        if cli.json {
            print_json(&serde_json::json!({"error": e.to_string()}))?;
        } else {
            eprintln!("{}: {}", "Error".red().bold(), e);
        }
        std::process::exit(1);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_block(ns: &str, name: &str) -> BlockResponse {
        BlockResponse {
            id: Uuid::now_v7(),
            lineage_id: Uuid::now_v7(),
            parent_id: None,
            namespace: ns.to_string(),
            name: name.to_string(),
            position: "80".to_string(),
            content: String::new(),
            content_type: "namespace".to_string(),
            properties: serde_json::json!({}),
            created_at: chrono::Utc::now(),
        }
    }

    fn test_block_with_content(ns: &str, name: &str, content: &str) -> BlockResponse {
        let mut b = test_block(ns, name);
        b.content = content.to_string();
        b
    }

    // =========================================================================
    // parse_uuid tests
    // =========================================================================

    #[test]
    fn parse_uuid_valid() {
        let result = parse_uuid("550e8400-e29b-41d4-a716-446655440000");
        assert!(result.is_ok());
        assert_eq!(
            result.unwrap().to_string(),
            "550e8400-e29b-41d4-a716-446655440000"
        );
    }

    #[test]
    fn parse_uuid_valid_v7() {
        let id = Uuid::now_v7();
        let result = parse_uuid(&id.to_string());
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), id);
    }

    #[test]
    fn parse_uuid_invalid_string() {
        let result = parse_uuid("not-a-uuid");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Invalid UUID"), "got: {}", err_msg);
    }

    #[test]
    fn parse_uuid_empty_string() {
        let result = parse_uuid("");
        assert!(result.is_err());
        let err_msg = result.unwrap_err().to_string();
        assert!(err_msg.contains("Invalid UUID"), "got: {}", err_msg);
    }

    // =========================================================================
    // truncate tests
    // =========================================================================

    #[test]
    fn truncate_short_string() {
        assert_eq!(truncate("hi", 10), "hi");
    }

    #[test]
    fn truncate_exact_length() {
        assert_eq!(truncate("hello", 5), "hello");
    }

    #[test]
    fn truncate_one_over() {
        assert_eq!(truncate("hello!", 5), "he...");
    }

    #[test]
    fn truncate_long_string() {
        assert_eq!(truncate("hello world, this is long", 10), "hello w...");
    }

    #[test]
    fn truncate_empty_string() {
        assert_eq!(truncate("", 10), "");
    }

    #[test]
    #[should_panic]
    fn truncate_max_len_too_small() {
        // max_len=2 causes usize underflow on (max_len - 3) when string is longer
        truncate("hello", 2);
    }

    // =========================================================================
    // get_parent_namespace tests
    // =========================================================================

    #[test]
    fn get_parent_namespace_simple() {
        assert_eq!(
            get_parent_namespace("parent::child"),
            Some("parent".to_string())
        );
    }

    #[test]
    fn get_parent_namespace_deep() {
        assert_eq!(get_parent_namespace("a::b::c"), Some("a::b".to_string()));
    }

    #[test]
    fn get_parent_namespace_root() {
        assert_eq!(get_parent_namespace("root"), None);
    }

    #[test]
    fn get_parent_namespace_empty() {
        assert_eq!(get_parent_namespace(""), None);
    }

    // =========================================================================
    // build_namespace_tree tests
    // =========================================================================

    #[test]
    fn build_namespace_tree_empty() {
        let blocks: Vec<BlockResponse> = vec![];
        let tree = build_namespace_tree(&blocks, None);
        assert!(tree.is_empty());
    }

    #[test]
    fn build_namespace_tree_single_root() {
        let mut block = test_block("", "notes");
        block.namespace = "notes".to_string();
        let blocks = vec![block];
        let tree = build_namespace_tree(&blocks, None);
        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].name, "notes");
        assert!(tree[0].children.is_empty());
    }

    #[test]
    fn build_namespace_tree_hierarchy() {
        let mut a = test_block("", "a");
        a.namespace = "a".to_string();
        let mut ab = test_block("a", "b");
        ab.namespace = "a::b".to_string();
        let mut abc = test_block("a::b", "c");
        abc.namespace = "a::b::c".to_string();

        let blocks = vec![a, ab, abc];
        let tree = build_namespace_tree(&blocks, None);

        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].name, "a");
        assert_eq!(tree[0].children.len(), 1);
        assert_eq!(tree[0].children[0].name, "b");
        assert_eq!(tree[0].children[0].children.len(), 1);
        assert_eq!(tree[0].children[0].children[0].name, "c");
        assert!(tree[0].children[0].children[0].children.is_empty());
    }

    #[test]
    fn build_namespace_tree_multiple_roots() {
        let mut x = test_block("", "x");
        x.namespace = "x".to_string();
        let mut y = test_block("", "y");
        y.namespace = "y".to_string();

        let blocks = vec![x, y];
        let tree = build_namespace_tree(&blocks, None);

        assert_eq!(tree.len(), 2);
        let names: Vec<&str> = tree.iter().map(|n| n.name.as_str()).collect();
        assert!(names.contains(&"x"));
        assert!(names.contains(&"y"));
    }

    #[test]
    fn build_namespace_tree_with_root_path_filter() {
        let mut a = test_block("", "a");
        a.namespace = "a".to_string();
        let mut ab = test_block("a", "b");
        ab.namespace = "a::b".to_string();
        let mut b = test_block("", "b");
        b.namespace = "b".to_string();
        let mut bc = test_block("b", "c");
        bc.namespace = "b::c".to_string();

        let blocks = vec![a, ab, b, bc];
        let tree = build_namespace_tree(&blocks, Some("a"));

        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].name, "a");
        assert_eq!(tree[0].children.len(), 1);
        assert_eq!(tree[0].children[0].name, "b");
    }

    // =========================================================================
    // build_content_tree tests
    // =========================================================================

    #[test]
    fn build_content_tree_simple() {
        let mut root = test_block_with_content("", "notes", "Root content");
        root.namespace = "notes".to_string();
        let mut child = test_block_with_content("notes", "todo", "Buy milk");
        child.namespace = "notes::todo".to_string();

        let blocks = vec![root, child];
        let tree = build_content_tree(&blocks, "notes");

        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].name, "notes");
        assert_eq!(tree[0].content, "Root content");
        assert_eq!(tree[0].children.len(), 1);
        assert_eq!(tree[0].children[0].name, "todo");
        assert_eq!(tree[0].children[0].content, "Buy milk");
    }

    #[test]
    fn build_content_tree_children_sorted_alphabetically() {
        let mut root = test_block_with_content("", "root", "");
        root.namespace = "root".to_string();
        let mut c = test_block_with_content("root", "cherry", "C");
        c.namespace = "root::cherry".to_string();
        let mut a = test_block_with_content("root", "apple", "A");
        a.namespace = "root::apple".to_string();
        let mut b = test_block_with_content("root", "banana", "B");
        b.namespace = "root::banana".to_string();

        let blocks = vec![root, c, a, b];
        let tree = build_content_tree(&blocks, "root");

        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].children.len(), 3);
        assert_eq!(tree[0].children[0].name, "apple");
        assert_eq!(tree[0].children[1].name, "banana");
        assert_eq!(tree[0].children[2].name, "cherry");
    }

    #[test]
    fn build_content_tree_deep_nesting() {
        let mut l1 = test_block_with_content("", "a", "level 1");
        l1.namespace = "a".to_string();
        let mut l2 = test_block_with_content("a", "b", "level 2");
        l2.namespace = "a::b".to_string();
        let mut l3 = test_block_with_content("a::b", "c", "level 3");
        l3.namespace = "a::b::c".to_string();

        let blocks = vec![l1, l2, l3];
        let tree = build_content_tree(&blocks, "a");

        assert_eq!(tree.len(), 1);
        assert_eq!(tree[0].name, "a");
        assert_eq!(tree[0].content, "level 1");
        assert_eq!(tree[0].children.len(), 1);
        assert_eq!(tree[0].children[0].name, "b");
        assert_eq!(tree[0].children[0].content, "level 2");
        assert_eq!(tree[0].children[0].children.len(), 1);
        assert_eq!(tree[0].children[0].children[0].name, "c");
        assert_eq!(tree[0].children[0].children[0].content, "level 3");
        assert!(tree[0].children[0].children[0].children.is_empty());
    }
}
