//! Profile citm_catalog.json parsing (for callgrind, etc.)
//!
//! cargo build --release -p facet-json --example profile_citm
//! valgrind --tool=callgrind ./target/release/examples/profile_citm

use facet::Facet;
use facet_json::from_str;
use std::collections::HashMap;
use std::hint::black_box;

#[derive(Debug, Facet)]
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

#[derive(Debug, Facet)]
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

#[derive(Debug, Facet)]
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

#[derive(Debug, Facet)]
#[facet(rename_all = "camelCase")]
struct Price {
    amount: u64,
    audience_sub_category_id: u64,
    seat_category_id: u64,
}

#[derive(Debug, Facet)]
#[facet(rename_all = "camelCase")]
struct SeatCategory {
    areas: Vec<Area>,
    seat_category_id: u64,
}

#[derive(Debug, Facet)]
#[facet(rename_all = "camelCase")]
struct Area {
    area_id: u64,
    block_ids: Vec<u64>,
}

fn main() {
    let json = std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../citm_catalog.json"
    ))
    .expect("citm_catalog.json not found");

    for _ in 0..100 {
        let result: CitmCatalog = from_str(&json).unwrap();
        black_box(result);
    }
}
