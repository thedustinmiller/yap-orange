pub mod data;
pub mod results;
pub mod runner;
pub mod suites;

pub use results::{BenchmarkResult, BenchmarkResults, SuiteResult};
pub use runner::{BenchmarkConfig, run_benchmarks};
pub use suites::all_suites;
