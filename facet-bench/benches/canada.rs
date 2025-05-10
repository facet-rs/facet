#![allow(clippy::approx_constant)]

use divan::{Bencher, black_box};
use facet::Facet;
use serde::{Deserialize, Serialize};
use std::fs;

/// GeoJSON structures for canada.json benchmark
#[derive(Debug, PartialEq, Clone, Facet, Serialize, Deserialize)]
struct Polygon {
    #[serde(rename = "type")]
    #[facet(rename = "type")]
    polygon_type: String,
    coordinates: Vec<Vec<Vec<f64>>>,
}

#[derive(Debug, PartialEq, Clone, Facet, Serialize, Deserialize)]
struct Properties {
    name: String,
}

#[derive(Debug, PartialEq, Clone, Facet, Serialize, Deserialize)]
struct Feature {
    #[serde(rename = "type")]
    #[facet(rename = "type")]
    feature_type: String,
    properties: Properties,
    geometry: Polygon,
}

#[derive(Debug, PartialEq, Clone, Facet, Serialize, Deserialize)]
struct FeatureCollection {
    #[serde(rename = "type")]
    #[facet(rename = "type")]
    collection_type: String,
    features: Vec<Feature>,
}

/// Helper function to load and decompress the canada.json.xz file
fn load_canada_json() -> String {
    // Load the XZ-compressed canada.json file
    let compressed_data =
        fs::read("benches/data/canada.json.xz").expect("Failed to read canada.json.xz");

    // Decompress the data
    let mut decompressor = xz2::read::XzDecoder::new(&compressed_data[..]);
    let mut json_string = String::new();
    std::io::Read::read_to_string(&mut decompressor, &mut json_string)
        .expect("Failed to decompress canada.json.xz");

    json_string
}

/// Benchmark for reading canada.json
#[divan::bench(name = "Deserialize - canada.json - facet_json")]
fn bench_canada_facet_json_deserialize(bencher: Bencher) {
    let json_string = load_canada_json();

    bencher.bench(|| {
        let res: FeatureCollection =
            black_box(facet_json::from_str(black_box(&json_string))).unwrap();
        black_box(res)
    });
}

#[divan::bench(name = "Deserialize - canada.json - serde")]
fn bench_canada_serde_deserialize(bencher: Bencher) {
    let json_string = load_canada_json();

    bencher.bench(|| {
        let res: FeatureCollection =
            black_box(serde_json::from_str(black_box(&json_string))).unwrap();
        black_box(res)
    });
}

/// Benchmark for writing canada.json
/// Note: Currently will error with "Unsupported shape: String"
/// See issue #338
#[divan::bench(name = "Serialize - canada.json - facet_json")]
fn bench_canada_facet_json_serialize(bencher: Bencher) {
    let json_string = load_canada_json();
    let data: FeatureCollection = serde_json::from_str(&json_string).unwrap();

    // Now benchmark only the serialization
    bencher.bench(|| black_box(facet_json::to_string(black_box(&data))));
}

#[divan::bench(name = "Serialize - canada.json - serde")]
fn bench_canada_serde_serialize(bencher: Bencher) {
    let json_string = load_canada_json();

    // Parse the JSON
    let data: FeatureCollection = serde_json::from_str(&json_string).unwrap();

    // Now benchmark only the serialization
    bencher.bench(|| black_box(serde_json::to_string(black_box(&data))));
}

fn main() {
    divan::main();
}
