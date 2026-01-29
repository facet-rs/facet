//! Benchmark parsing classic JSON benchmarks converted to TOML.
//!
//! The JSON fixtures are loaded, parsed, serialized to TOML, then we benchmark
//! deserializing that TOML data.

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
// Data loading - convert JSON fixtures to TOML
// =============================================================================

static CITM_TOML: LazyLock<String> = LazyLock::new(|| {
    let json_str = &*facet_json_classics::CITM_CATALOG;
    let data: CitmCatalog = serde_json::from_str(json_str).expect("Failed to parse citm JSON");
    toml::to_string(&data).expect("Failed to serialize citm to TOML")
});

// =============================================================================
// Benchmarks
// =============================================================================

#[divan::bench]
fn citm_toml_serde(bencher: Bencher) {
    let data = &*CITM_TOML;
    bencher.bench(|| {
        let result: CitmCatalog = black_box(toml::from_str(black_box(data)).unwrap());
        black_box(result)
    });
}

#[divan::bench]
fn citm_facet_toml(bencher: Bencher) {
    let data = &*CITM_TOML;
    bencher.bench(|| {
        let result: CitmCatalog = black_box(facet_toml::from_str(black_box(data)).unwrap());
        black_box(result)
    });
}
