mod edges;
mod edit;
mod hierarchy;
mod links;
mod namespace;
mod read;
mod search;
mod write;

use async_trait::async_trait;
use uuid::Uuid;
use yap_core::Store;

use crate::results::SuiteResult;

/// A benchmark suite that tests a specific aspect of store performance.
#[async_trait]
pub trait BenchSuite: Send + Sync {
    /// Machine-readable name (e.g. "write_throughput").
    fn name(&self) -> &str;

    /// Human-readable description.
    fn description(&self) -> &str;

    /// Run the suite against the given store, using `test_root_id` as the parent namespace.
    async fn run(&self, store: &dyn Store, test_root_id: Uuid, seed: u64) -> SuiteResult;
}

/// Return all registered benchmark suites.
pub fn all_suites() -> Vec<Box<dyn BenchSuite>> {
    vec![
        Box::new(write::WriteSuite),
        Box::new(read::ReadSuite),
        Box::new(edit::EditSuite),
        Box::new(search::SearchSuite),
        Box::new(namespace::NamespaceSuite),
        Box::new(links::LinksSuite),
        Box::new(hierarchy::HierarchySuite),
        Box::new(edges::EdgesSuite),
    ]
}
