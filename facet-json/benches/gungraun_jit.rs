//! Deterministic benchmarks using gungraun (instruction counts via Valgrind).
//!
//! These complement the divan benchmarks by providing reproducible measurements
//! across different machines and CI runs.

use facet::Facet;
use facet_format::jit as format_jit;
use facet_format_json::JsonParser;
use gungraun::{library_benchmark, library_benchmark_group, main};
use std::hint::black_box;

// Simple flat struct - should JIT well
#[derive(Facet, Clone, Debug)]
struct SimpleRecord {
    id: u64,
    name: String,
    active: bool,
}

// Nested struct - tests recursive JIT compilation
#[derive(Facet, Clone, Debug)]
struct Outer {
    id: u64,
    inner: Inner,
    name: String,
}

#[derive(Facet, Clone, Debug)]
struct Inner {
    x: i64,
    y: i64,
}

fn setup_simple_jit() -> &'static [u8] {
    let json = br#"{"id": 42, "name": "test", "active": true}"#;
    // Warmup: trigger JIT compilation and caching
    let _ = format_jit::deserialize_with_fallback::<SimpleRecord, _>(JsonParser::new(json));
    json
}

#[library_benchmark]
#[bench::cached(setup = setup_simple_jit)]
fn simple_struct_facet_format_jit(json: &[u8]) -> SimpleRecord {
    let parser = JsonParser::new(black_box(json));
    black_box(format_jit::deserialize_with_fallback::<SimpleRecord, _>(parser).unwrap())
}

#[library_benchmark]
fn simple_struct_facet_format_json() -> SimpleRecord {
    let json = br#"{"id": 42, "name": "test", "active": true}"#;
    black_box(facet_format_json::from_slice::<SimpleRecord>(black_box(json)).unwrap())
}

#[library_benchmark]
fn simple_struct_facet_json() -> SimpleRecord {
    let json = br#"{"id": 42, "name": "test", "active": true}"#;
    black_box(facet_json::from_slice::<SimpleRecord>(black_box(json)).unwrap())
}

#[cfg(feature = "cranelift")]
fn setup_simple_cranelift() -> &'static str {
    let json = r#"{"id": 42, "name": "test", "active": true}"#;
    // Warmup: trigger cranelift compilation and caching
    let _ = facet_json::cranelift::from_str_with_fallback::<SimpleRecord>(json);
    json
}

#[cfg(feature = "cranelift")]
#[library_benchmark]
#[bench::cached(setup = setup_simple_cranelift)]
fn simple_struct_facet_json_cranelift(json: &str) -> SimpleRecord {
    black_box(
        facet_json::cranelift::from_str_with_fallback::<SimpleRecord>(black_box(json)).unwrap(),
    )
}

fn setup_nested_jit() -> &'static [u8] {
    let json = br#"{"id": 42, "inner": {"x": 10, "y": 20}, "name": "test"}"#;
    // Warmup: trigger JIT compilation for Outer and Inner (both get cached)
    let _ = format_jit::deserialize_with_fallback::<Outer, _>(JsonParser::new(json));
    json
}

#[library_benchmark]
#[bench::cached(setup = setup_nested_jit)]
fn nested_struct_facet_format_jit(json: &[u8]) -> Outer {
    let parser = JsonParser::new(black_box(json));
    black_box(format_jit::deserialize_with_fallback::<Outer, _>(parser).unwrap())
}

#[library_benchmark]
fn nested_struct_facet_format_json() -> Outer {
    let json = br#"{"id": 42, "inner": {"x": 10, "y": 20}, "name": "test"}"#;
    black_box(facet_format_json::from_slice::<Outer>(black_box(json)).unwrap())
}

#[library_benchmark]
fn nested_struct_facet_json() -> Outer {
    let json = br#"{"id": 42, "inner": {"x": 10, "y": 20}, "name": "test"}"#;
    black_box(facet_json::from_slice::<Outer>(black_box(json)).unwrap())
}

library_benchmark_group!(
    name = jit_benchmarks;
    benchmarks =
        simple_struct_facet_format_jit,
        simple_struct_facet_format_json,
        simple_struct_facet_json,
        nested_struct_facet_format_jit,
        nested_struct_facet_format_json,
        nested_struct_facet_json
);

#[cfg(feature = "cranelift")]
library_benchmark_group!(
    name = jit_benchmarks_cranelift;
    benchmarks = simple_struct_facet_json_cranelift
);

#[cfg(feature = "cranelift")]
main!(
    library_benchmark_groups = jit_benchmarks,
    jit_benchmarks_cranelift
);

#[cfg(not(feature = "cranelift"))]
main!(library_benchmark_groups = jit_benchmarks);
