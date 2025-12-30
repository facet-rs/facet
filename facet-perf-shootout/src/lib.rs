//! Performance shootout benchmarks for facet formats.
//!
//! This crate contains benchmarks comparing facet format implementations
//! against their reference counterparts (serde_json, postcard, rmp-serde, etc.)
//!
//! Benchmark suites are defined in KDL files under `benches/`:
//! - `json.kdl` - JSON format benchmarks
//! - `postcard.kdl` - Postcard format benchmarks
//!
//! Run benchmarks with:
//! ```sh
//! cargo bench -p facet-perf-shootout --features jit
//! ```

// Type modules are generated/included by the benchmark generator
pub mod json_types;
pub mod postcard_types;

// Shared benchmark operations - used by both divan and gungraun benchmarks
pub mod bench_ops;
