//! Benchmark parsing classic JSON benchmarks converted to postcard binary format.
//!
//! The JSON fixtures are loaded, parsed, serialized to postcard, then we benchmark
//! deserializing that binary data.

use divan::{Bencher, black_box};
use facet::Facet;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::LazyLock;

fn main() {
    divan::main();
}

// =============================================================================
// Types for citm_catalog
// =============================================================================

#[derive(Debug, Deserialize, Serialize, Facet)]
#[serde(rename_all = "camelCase")]
#[facet(rename_all = "camelCase")]
struct CitmCatalog {
    area_names: HashMap<String, String>,
    audience_sub_category_names: HashMap<String, String>,
    block_names: HashMap<String, String>,
    events: HashMap<String, Event>,
    performances: Vec<Performance>,
    seat_category_names: HashMap<String, String>,
    sub_topic_names: HashMap<String, String>,
    subject_names: HashMap<String, String>,
    topic_names: HashMap<String, String>,
    topic_sub_topics: HashMap<String, Vec<u64>>,
    venue_names: HashMap<String, String>,
}

#[derive(Debug, Deserialize, Serialize, Facet)]
#[serde(rename_all = "camelCase")]
#[facet(rename_all = "camelCase")]
struct Event {
    description: Option<String>,
    id: u64,
    logo: Option<String>,
    name: String,
    sub_topic_ids: Vec<u64>,
    subject_code: Option<String>,
    subtitle: Option<String>,
    topic_ids: Vec<u64>,
}

#[derive(Debug, Deserialize, Serialize, Facet)]
#[serde(rename_all = "camelCase")]
#[facet(rename_all = "camelCase")]
struct Performance {
    event_id: u64,
    id: u64,
    logo: Option<String>,
    name: Option<String>,
    prices: Vec<Price>,
    seat_categories: Vec<SeatCategory>,
    seat_map_image: Option<String>,
    start: u64,
    venue_code: String,
}

#[derive(Debug, Deserialize, Serialize, Facet)]
#[serde(rename_all = "camelCase")]
#[facet(rename_all = "camelCase")]
struct Price {
    amount: u64,
    audience_sub_category_id: u64,
    seat_category_id: u64,
}

#[derive(Debug, Deserialize, Serialize, Facet)]
#[serde(rename_all = "camelCase")]
#[facet(rename_all = "camelCase")]
struct SeatCategory {
    areas: Vec<Area>,
    seat_category_id: u64,
}

#[derive(Debug, Deserialize, Serialize, Facet)]
#[serde(rename_all = "camelCase")]
#[facet(rename_all = "camelCase")]
struct Area {
    area_id: u64,
    block_ids: Vec<u64>,
}

// =============================================================================
// Types for canada.json (GeoJSON)
// =============================================================================

#[derive(Debug, Deserialize, Serialize, Facet)]
struct FeatureCollection {
    #[serde(rename = "type")]
    #[facet(rename = "type")]
    type_: String,
    features: Vec<Feature>,
}

#[derive(Debug, Deserialize, Serialize, Facet)]
struct Feature {
    #[serde(rename = "type")]
    #[facet(rename = "type")]
    type_: String,
    properties: Properties,
    geometry: Geometry,
}

#[derive(Debug, Deserialize, Serialize, Facet)]
struct Properties {
    name: String,
}

#[derive(Debug, Deserialize, Serialize, Facet)]
struct Geometry {
    #[serde(rename = "type")]
    #[facet(rename = "type")]
    type_: String,
    coordinates: Vec<Vec<Vec<f64>>>,
}

// =============================================================================
// Data loading - convert JSON fixtures to postcard
// =============================================================================

static CITM_POSTCARD: LazyLock<Vec<u8>> = LazyLock::new(|| {
    let json_str = &*facet_json_classics::CITM_CATALOG;
    let data: CitmCatalog = serde_json::from_str(json_str).expect("Failed to parse citm JSON");
    postcard::to_allocvec(&data).expect("Failed to serialize citm to postcard")
});

static CANADA_POSTCARD: LazyLock<Vec<u8>> = LazyLock::new(|| {
    let json_str = &*facet_json_classics::CANADA;
    let data: FeatureCollection = serde_json::from_str(json_str).expect("Failed to parse canada JSON");
    postcard::to_allocvec(&data).expect("Failed to serialize canada to postcard")
});

// =============================================================================
// Benchmarks - citm
// =============================================================================

#[divan::bench]
fn citm_postcard_serde(bencher: Bencher) {
    let data = &*CITM_POSTCARD;
    bencher.bench(|| {
        let result: CitmCatalog = black_box(postcard::from_bytes(black_box(data)).unwrap());
        black_box(result)
    });
}

#[divan::bench]
fn citm_facet_postcard(bencher: Bencher) {
    let data = &*CITM_POSTCARD;
    bencher.bench(|| {
        let result: CitmCatalog = black_box(facet_postcard::from_slice(black_box(data)).unwrap());
        black_box(result)
    });
}

// =============================================================================
// Benchmarks - canada (deeply nested float arrays)
// =============================================================================

#[divan::bench]
fn canada_postcard_serde(bencher: Bencher) {
    let data = &*CANADA_POSTCARD;
    bencher.bench(|| {
        let result: FeatureCollection = black_box(postcard::from_bytes(black_box(data)).unwrap());
        black_box(result)
    });
}

#[divan::bench]
fn canada_facet_postcard(bencher: Bencher) {
    let data = &*CANADA_POSTCARD;
    bencher.bench(|| {
        let result: FeatureCollection = black_box(facet_postcard::from_slice(black_box(data)).unwrap());
        black_box(result)
    });
}
