use facet_core::Facet;
use std::collections::HashMap;

#[derive(Debug, PartialEq, Facet, Clone)]
struct CitmCatalog {
    area_names: HashMap<String, String>,
    events: HashMap<String, Event>,
    performances: Vec<Performance>,
    topic_sub_topics: HashMap<String, Vec<u64>>,
}

#[derive(Debug, PartialEq, Facet, Clone)]
struct Event {
    description: Option<String>,
    id: u64,
    name: String,
    sub_topic_ids: Vec<u64>,
}

#[derive(Debug, PartialEq, Facet, Clone)]
struct Performance {
    event_id: u64,
    id: u64,
    name: Option<String>,
    prices: Vec<Price>,
}

#[derive(Debug, PartialEq, Facet, Clone)]
struct Price {
    amount: u64,
    seat_category_id: u64,
}

fn main() {
    println!(
        "CitmCatalog compatible: {}",
        facet_format::jit::is_format_jit_compatible(CitmCatalog::SHAPE)
    );
    println!(
        "Event compatible: {}",
        facet_format::jit::is_format_jit_compatible(Event::SHAPE)
    );
    println!(
        "Performance compatible: {}",
        facet_format::jit::is_format_jit_compatible(Performance::SHAPE)
    );
    println!(
        "Price compatible: {}",
        facet_format::jit::is_format_jit_compatible(Price::SHAPE)
    );
}
