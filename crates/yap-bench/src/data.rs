use rand::rngs::StdRng;
use rand::seq::IndexedRandom;
use rand::{Rng, SeedableRng};

const ADJECTIVES: &[&str] = &[
    "quick", "lazy", "bright", "dark", "silent", "loud", "ancient", "modern", "simple", "complex",
    "warm", "cold", "deep", "shallow", "broad", "narrow", "swift", "steady", "sharp", "smooth",
    "bold", "calm", "dense", "sparse", "fresh", "stale", "grand", "humble", "keen", "mild",
];

const NOUNS: &[&str] = &[
    "note", "idea", "plan", "task", "link", "node", "edge", "path", "tree", "leaf", "root",
    "branch", "block", "atom", "graph", "mesh", "draft", "sketch", "outline", "summary", "review",
    "report", "memo", "log", "query", "index", "table", "schema", "field", "record",
];

const TOPICS: &[&str] = &[
    "exploring new approaches to knowledge management",
    "hierarchical organization enables flexible note structures",
    "graph linking connects ideas across namespaces",
    "immutable atoms preserve complete edit history",
    "fractional indexing allows conflict-free reordering",
    "wiki-style links resolve through namespace paths",
    "custom types bring structured data into the note hierarchy",
    "backlinks surface implicit connections between blocks",
    "recursive soft deletion preserves data integrity",
    "edge relationships model explicit semantic connections",
    "content hashing enables deduplication across the store",
    "namespace traversal walks the parent chain upward",
    "schema resolution searches from local to global scope",
    "outliner views render deeply nested hierarchies",
    "lineage pointers provide stable identity for linked content",
];

/// Seeded data generator for deterministic benchmark data.
pub struct DataGen {
    rng: StdRng,
}

impl DataGen {
    /// Create a new generator with the given seed.
    pub fn new(seed: u64) -> Self {
        Self {
            rng: StdRng::seed_from_u64(seed),
        }
    }

    /// Generate a realistic block name like "quick_note".
    pub fn block_name(&mut self) -> String {
        let adj = ADJECTIVES.choose(&mut self.rng).unwrap();
        let noun = NOUNS.choose(&mut self.rng).unwrap();
        let suffix = self.rng.random_range(0u32..1000);
        format!("{adj}_{noun}_{suffix}")
    }

    /// Generate content of approximately `len` characters.
    pub fn content(&mut self, len: usize) -> String {
        let mut result = String::new();
        while result.len() < len {
            let topic = TOPICS.choose(&mut self.rng).unwrap();
            if !result.is_empty() {
                result.push(' ');
            }
            result.push_str(topic);
        }
        result.truncate(len);
        result
    }

    /// Generate realistic properties JSON.
    pub fn properties(&mut self) -> serde_json::Value {
        let statuses = ["active", "draft", "archived", "review"];
        let status = statuses.choose(&mut self.rng).unwrap();
        let priority = self.rng.random_range(1u32..=5);
        serde_json::json!({
            "status": status,
            "priority": priority
        })
    }

    /// Generate an edge type string.
    pub fn edge_type(&mut self) -> String {
        let types = [
            "references",
            "depends_on",
            "related_to",
            "blocks",
            "extends",
        ];
        types.choose(&mut self.rng).unwrap().to_string()
    }

    /// Generate a searchable name containing `bench_term`.
    pub fn searchable_name(&mut self, index: usize) -> String {
        let adj = ADJECTIVES.choose(&mut self.rng).unwrap();
        if index.is_multiple_of(3) {
            format!("{adj}_bench_term_{index}")
        } else {
            format!("{adj}_{index}")
        }
    }
}
