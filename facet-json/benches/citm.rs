//! Benchmark parsing citm_catalog.json from nativejson-benchmark.

use divan::{Bencher, black_box};
use facet::Facet;
use serde::Deserialize;
use std::collections::HashMap;

fn main() {
    divan::main();
}

// =============================================================================
// Types for citm_catalog.json
// =============================================================================

#[derive(Debug, Deserialize, Facet)]
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

#[derive(Debug, Deserialize, Facet)]
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

#[derive(Debug, Deserialize, Facet)]
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

#[derive(Debug, Deserialize, Facet)]
#[serde(rename_all = "camelCase")]
#[facet(rename_all = "camelCase")]
struct Price {
    amount: u64,
    audience_sub_category_id: u64,
    seat_category_id: u64,
}

#[derive(Debug, Deserialize, Facet)]
#[serde(rename_all = "camelCase")]
#[facet(rename_all = "camelCase")]
struct SeatCategory {
    areas: Vec<Area>,
    seat_category_id: u64,
}

#[derive(Debug, Deserialize, Facet)]
#[serde(rename_all = "camelCase")]
#[facet(rename_all = "camelCase")]
struct Area {
    area_id: u64,
    block_ids: Vec<u64>,
}

// =============================================================================
// Data loading
// =============================================================================

fn json_str() -> &'static str {
    &facet_json_classics::CITM_CATALOG
}

// =============================================================================
// Benchmarks
// =============================================================================

#[divan::bench]
fn serde_json(bencher: Bencher) {
    let data = json_str();
    bencher.bench(|| {
        let result: CitmCatalog = black_box(serde_json::from_str(black_box(data)).unwrap());
        black_box(result)
    });
}

#[divan::bench]
fn facet_json(bencher: Bencher) {
    let data = json_str();
    bencher.bench(|| {
        let result: CitmCatalog = black_box(facet_json::from_str(black_box(data)).unwrap());
        black_box(result)
    });
}
