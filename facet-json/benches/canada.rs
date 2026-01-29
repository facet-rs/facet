//! Benchmark parsing canada.json (GeoJSON) from nativejson-benchmark.
//!
//! This tests deeply nested arrays of floating-point coordinates.

use divan::{Bencher, black_box};
use facet::Facet;
use serde::Deserialize;

fn main() {
    divan::main();
}

// =============================================================================
// Types for canada.json (GeoJSON)
// =============================================================================

#[derive(Debug, Deserialize, Facet)]
struct FeatureCollection {
    #[serde(rename = "type")]
    #[facet(rename = "type")]
    type_: String,
    features: Vec<Feature>,
}

#[derive(Debug, Deserialize, Facet)]
struct Feature {
    #[serde(rename = "type")]
    #[facet(rename = "type")]
    type_: String,
    properties: Properties,
    geometry: Geometry,
}

#[derive(Debug, Deserialize, Facet)]
struct Properties {
    name: String,
}

#[derive(Debug, Deserialize, Facet)]
struct Geometry {
    #[serde(rename = "type")]
    #[facet(rename = "type")]
    type_: String,
    coordinates: Vec<Vec<Vec<f64>>>,
}

// =============================================================================
// Data loading
// =============================================================================

fn json_str() -> &'static str {
    &facet_json_classics::CANADA
}

// =============================================================================
// Benchmarks
// =============================================================================

#[divan::bench]
fn serde_json(bencher: Bencher) {
    let data = json_str();
    bencher.bench(|| {
        let result: FeatureCollection = black_box(serde_json::from_str(black_box(data)).unwrap());
        black_box(result)
    });
}

#[divan::bench]
fn facet_json(bencher: Bencher) {
    let data = json_str();
    bencher.bench(|| {
        let result: FeatureCollection = black_box(facet_json::from_str(black_box(data)).unwrap());
        black_box(result)
    });
}
