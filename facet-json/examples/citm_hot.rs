use facet::Facet;
use facet_format::jit as format_jit;
use facet_format_json::JsonParser;
use std::hint::black_box;

#[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
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

#[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
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

#[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
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

#[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
struct Price {
    amount: u64,
    #[serde(rename = "audienceSubCategoryId")]
    #[facet(rename = "audienceSubCategoryId")]
    audience_sub_category_id: u64,
    #[serde(rename = "seatCategoryId")]
    #[facet(rename = "seatCategoryId")]
    seat_category_id: u64,
}

#[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
struct SeatCategory {
    areas: Vec<Area>,
    #[serde(rename = "seatCategoryId")]
    #[facet(rename = "seatCategoryId")]
    seat_category_id: u64,
}

#[derive(Debug, PartialEq, Facet, serde::Serialize, serde::Deserialize, Clone)]
struct Area {
    #[serde(rename = "areaId")]
    #[facet(rename = "areaId")]
    area_id: u64,
    #[serde(rename = "blockIds")]
    #[facet(rename = "blockIds")]
    block_ids: Vec<u64>,
}

fn main() {
    // Print shape addresses for type mapping
    eprintln!("Shape addresses:");
    eprintln!("  CitmCatalog: {:p}", CitmCatalog::SHAPE);
    eprintln!("  Event: {:p}", Event::SHAPE);
    eprintln!("  Performance: {:p}", Performance::SHAPE);
    eprintln!("  Price: {:p}", Price::SHAPE);
    eprintln!("  SeatCategory: {:p}", SeatCategory::SHAPE);
    eprintln!("  Area: {:p}", Area::SHAPE);

    let compressed = include_bytes!("../../tools/benchmark-generator/corpus/citm_catalog.json.br");
    let mut json = Vec::new();
    brotli::BrotliDecompress(&mut std::io::Cursor::new(compressed), &mut json).unwrap();

    // Warmup - trigger JIT compilation (10 iterations to be sure)
    for _ in 0..10 {
        let result: CitmCatalog =
            format_jit::deserialize_with_format_jit_fallback(JsonParser::new(black_box(&json)))
                .unwrap();
        black_box(result);
    }

    // Hot path - 100 iterations (measured)
    for _ in 0..100 {
        let result: CitmCatalog =
            format_jit::deserialize_with_format_jit_fallback(JsonParser::new(black_box(&json)))
                .unwrap();
        black_box(result);
    }
}
