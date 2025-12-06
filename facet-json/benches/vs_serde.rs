//! Benchmark comparing facet_json vs serde_json deserialization performance.
//!
//! Uses standard JSON benchmark corpus files (brotli-compressed).

use divan::{Bencher, black_box};
use facet::Facet;
use serde::{Deserialize, Serialize};
use std::sync::LazyLock;

fn main() {
    divan::main();
}

// =============================================================================
// Corpus loading (brotli-compressed)
// =============================================================================

fn decompress_brotli(compressed: &[u8]) -> String {
    let mut decompressed = Vec::new();
    brotli::BrotliDecompress(&mut std::io::Cursor::new(compressed), &mut decompressed).unwrap();
    String::from_utf8(decompressed).unwrap()
}

static TWITTER_JSON: LazyLock<String> =
    LazyLock::new(|| decompress_brotli(include_bytes!("corpus/twitter.json.br")));

static CANADA_JSON: LazyLock<String> =
    LazyLock::new(|| decompress_brotli(include_bytes!("corpus/canada.json.br")));

static CITM_CATALOG_JSON: LazyLock<String> =
    LazyLock::new(|| decompress_brotli(include_bytes!("corpus/citm_catalog.json.br")));

// =============================================================================
// Twitter: Sparse struct definitions (only essential fields, rest skipped)
// =============================================================================

#[derive(Facet, Deserialize, Serialize, Debug)]
struct TwitterResponseSparse {
    statuses: Vec<StatusSparse>,
}

#[derive(Facet, Deserialize, Serialize, Debug)]
struct StatusSparse {
    id: u64,
    text: String,
    user: UserSparse,
    retweet_count: u32,
    favorite_count: u32,
}

#[derive(Facet, Deserialize, Serialize, Debug)]
struct UserSparse {
    id: u64,
    screen_name: String,
    followers_count: u32,
}

// =============================================================================
// Canada: GeoJSON structure (number-heavy)
// =============================================================================

#[derive(Facet, Deserialize, Serialize, Debug)]
struct Canada {
    #[facet(rename = "type")]
    #[serde(rename = "type")]
    type_: String,
    features: Vec<Feature>,
}

#[derive(Facet, Deserialize, Serialize, Debug)]
struct Feature {
    #[facet(rename = "type")]
    #[serde(rename = "type")]
    type_: String,
    properties: Properties,
    geometry: Geometry,
}

#[derive(Facet, Deserialize, Serialize, Debug)]
struct Properties {
    name: String,
}

#[derive(Facet, Deserialize, Serialize, Debug)]
struct Geometry {
    #[facet(rename = "type")]
    #[serde(rename = "type")]
    type_: String,
    coordinates: Vec<Vec<Vec<f64>>>,
}

// =============================================================================
// CITM Catalog: Event ticketing data
// =============================================================================

#[derive(Facet, Deserialize, Serialize, Debug)]
struct CitmCatalog {
    #[serde(rename = "areaNames")]
    #[facet(rename = "areaNames")]
    area_names: std::collections::HashMap<String, String>,
    #[serde(rename = "audienceSubCategoryNames")]
    #[facet(rename = "audienceSubCategoryNames")]
    audience_sub_category_names: std::collections::HashMap<String, String>,
    #[serde(rename = "blockNames")]
    #[facet(rename = "blockNames")]
    block_names: std::collections::HashMap<String, String>,
    events: std::collections::HashMap<String, Event>,
    performances: Vec<Performance>,
    #[serde(rename = "seatCategoryNames")]
    #[facet(rename = "seatCategoryNames")]
    seat_category_names: std::collections::HashMap<String, String>,
    #[serde(rename = "subTopicNames")]
    #[facet(rename = "subTopicNames")]
    sub_topic_names: std::collections::HashMap<String, String>,
    #[serde(rename = "subjectNames")]
    #[facet(rename = "subjectNames")]
    subject_names: std::collections::HashMap<String, String>,
    #[serde(rename = "topicNames")]
    #[facet(rename = "topicNames")]
    topic_names: std::collections::HashMap<String, String>,
    #[serde(rename = "topicSubTopics")]
    #[facet(rename = "topicSubTopics")]
    topic_sub_topics: std::collections::HashMap<String, Vec<u64>>,
    #[serde(rename = "venueNames")]
    #[facet(rename = "venueNames")]
    venue_names: std::collections::HashMap<String, String>,
}

#[derive(Facet, Deserialize, Serialize, Debug)]
struct Event {
    description: Option<String>,
    id: u64,
    logo: Option<String>,
    name: String,
    #[serde(rename = "subTopicIds")]
    #[facet(rename = "subTopicIds")]
    sub_topic_ids: Vec<u64>,
    #[serde(rename = "subjectCode")]
    #[facet(rename = "subjectCode")]
    subject_code: Option<String>,
    subtitle: Option<String>,
    #[serde(rename = "topicIds")]
    #[facet(rename = "topicIds")]
    topic_ids: Vec<u64>,
}

#[derive(Facet, Deserialize, Serialize, Debug)]
struct Performance {
    #[serde(rename = "eventId")]
    #[facet(rename = "eventId")]
    event_id: u64,
    id: u64,
    logo: Option<String>,
    name: Option<String>,
    prices: Vec<Price>,
    #[serde(rename = "seatCategories")]
    #[facet(rename = "seatCategories")]
    seat_categories: Vec<SeatCategory>,
    #[serde(rename = "seatMapImage")]
    #[facet(rename = "seatMapImage")]
    seat_map_image: Option<String>,
    start: u64,
    #[serde(rename = "venueCode")]
    #[facet(rename = "venueCode")]
    venue_code: String,
}

#[derive(Facet, Deserialize, Serialize, Debug)]
struct Price {
    amount: u64,
    #[serde(rename = "audienceSubCategoryId")]
    #[facet(rename = "audienceSubCategoryId")]
    audience_sub_category_id: u64,
    #[serde(rename = "seatCategoryId")]
    #[facet(rename = "seatCategoryId")]
    seat_category_id: u64,
}

#[derive(Facet, Deserialize, Serialize, Debug)]
struct SeatCategory {
    areas: Vec<Area>,
    #[serde(rename = "seatCategoryId")]
    #[facet(rename = "seatCategoryId")]
    seat_category_id: u64,
}

#[derive(Facet, Deserialize, Serialize, Debug)]
struct Area {
    #[serde(rename = "areaId")]
    #[facet(rename = "areaId")]
    area_id: u64,
    #[serde(rename = "blockIds")]
    #[facet(rename = "blockIds")]
    block_ids: Vec<u64>,
}

// =============================================================================
// Twitter benchmarks
// =============================================================================

mod twitter {
    use super::*;

    #[divan::bench]
    fn facet_sparse_struct(bencher: Bencher) {
        let json = &*TWITTER_JSON;
        bencher.bench(|| {
            let result: TwitterResponseSparse = facet_json::from_str(black_box(json)).unwrap();
            black_box(result)
        });
    }

    #[divan::bench]
    fn serde_sparse_struct(bencher: Bencher) {
        let json = &*TWITTER_JSON;
        bencher.bench(|| {
            let result: TwitterResponseSparse = serde_json::from_str(black_box(json)).unwrap();
            black_box(result)
        });
    }

    #[divan::bench]
    fn facet_value(bencher: Bencher) {
        let json = &*TWITTER_JSON;
        bencher.bench(|| {
            let result: facet_value::Value = facet_json::from_str(black_box(json)).unwrap();
            black_box(result)
        });
    }

    #[divan::bench]
    fn serde_value(bencher: Bencher) {
        let json = &*TWITTER_JSON;
        bencher.bench(|| {
            let result: serde_json::Value = serde_json::from_str(black_box(json)).unwrap();
            black_box(result)
        });
    }

    #[divan::bench]
    fn facet_serialize_value(bencher: Bencher) {
        let json = &*TWITTER_JSON;
        let data: facet_value::Value = facet_json::from_str(json).unwrap();
        bencher.bench(|| {
            let result = facet_json::to_string(black_box(&data));
            black_box(result)
        });
    }

    #[divan::bench]
    fn serde_serialize_value(bencher: Bencher) {
        let json = &*TWITTER_JSON;
        let data: serde_json::Value = serde_json::from_str(json).unwrap();
        bencher.bench(|| {
            let result = serde_json::to_string(black_box(&data)).unwrap();
            black_box(result)
        });
    }

    #[divan::bench]
    fn facet_serialize_sparse(bencher: Bencher) {
        let json = &*TWITTER_JSON;
        let data: TwitterResponseSparse = facet_json::from_str(json).unwrap();
        bencher.bench(|| {
            let result = facet_json::to_string(black_box(&data));
            black_box(result)
        });
    }

    #[divan::bench]
    fn serde_serialize_sparse(bencher: Bencher) {
        let json = &*TWITTER_JSON;
        let data: TwitterResponseSparse = serde_json::from_str(json).unwrap();
        bencher.bench(|| {
            let result = serde_json::to_string(black_box(&data)).unwrap();
            black_box(result)
        });
    }
}

// =============================================================================
// Canada benchmarks (number-heavy GeoJSON)
// =============================================================================

mod canada {
    use super::*;

    #[divan::bench]
    fn facet_typed(bencher: Bencher) {
        let json = &*CANADA_JSON;
        bencher.bench(|| {
            let result: Canada = facet_json::from_str(black_box(json)).unwrap();
            black_box(result)
        });
    }

    #[divan::bench]
    fn serde_typed(bencher: Bencher) {
        let json = &*CANADA_JSON;
        bencher.bench(|| {
            let result: Canada = serde_json::from_str(black_box(json)).unwrap();
            black_box(result)
        });
    }

    #[divan::bench]
    fn facet_value(bencher: Bencher) {
        let json = &*CANADA_JSON;
        bencher.bench(|| {
            let result: facet_value::Value = facet_json::from_str(black_box(json)).unwrap();
            black_box(result)
        });
    }

    #[divan::bench]
    fn serde_value(bencher: Bencher) {
        let json = &*CANADA_JSON;
        bencher.bench(|| {
            let result: serde_json::Value = serde_json::from_str(black_box(json)).unwrap();
            black_box(result)
        });
    }

    #[divan::bench]
    fn facet_serialize_typed(bencher: Bencher) {
        let json = &*CANADA_JSON;
        let data: Canada = facet_json::from_str(json).unwrap();
        bencher.bench(|| {
            let result = facet_json::to_string(black_box(&data));
            black_box(result)
        });
    }

    #[divan::bench]
    fn serde_serialize_typed(bencher: Bencher) {
        let json = &*CANADA_JSON;
        let data: Canada = serde_json::from_str(json).unwrap();
        bencher.bench(|| {
            let result = serde_json::to_string(black_box(&data)).unwrap();
            black_box(result)
        });
    }
}

// =============================================================================
// CITM Catalog benchmarks
// =============================================================================

mod citm_catalog {
    use super::*;

    #[divan::bench]
    fn facet_typed(bencher: Bencher) {
        let json = &*CITM_CATALOG_JSON;
        bencher.bench(|| {
            let result: CitmCatalog = facet_json::from_str(black_box(json)).unwrap();
            black_box(result)
        });
    }

    #[divan::bench]
    fn serde_typed(bencher: Bencher) {
        let json = &*CITM_CATALOG_JSON;
        bencher.bench(|| {
            let result: CitmCatalog = serde_json::from_str(black_box(json)).unwrap();
            black_box(result)
        });
    }

    #[divan::bench]
    fn facet_value(bencher: Bencher) {
        let json = &*CITM_CATALOG_JSON;
        bencher.bench(|| {
            let result: facet_value::Value = facet_json::from_str(black_box(json)).unwrap();
            black_box(result)
        });
    }

    #[divan::bench]
    fn serde_value(bencher: Bencher) {
        let json = &*CITM_CATALOG_JSON;
        bencher.bench(|| {
            let result: serde_json::Value = serde_json::from_str(black_box(json)).unwrap();
            black_box(result)
        });
    }

    #[divan::bench]
    fn facet_serialize_typed(bencher: Bencher) {
        let json = &*CITM_CATALOG_JSON;
        let data: CitmCatalog = facet_json::from_str(json).unwrap();
        bencher.bench(|| {
            let result = facet_json::to_string(black_box(&data));
            black_box(result)
        });
    }

    #[divan::bench]
    fn serde_serialize_typed(bencher: Bencher) {
        let json = &*CITM_CATALOG_JSON;
        let data: CitmCatalog = serde_json::from_str(json).unwrap();
        bencher.bench(|| {
            let result = serde_json::to_string(black_box(&data)).unwrap();
            black_box(result)
        });
    }
}
