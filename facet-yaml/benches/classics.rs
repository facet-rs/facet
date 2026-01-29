//! Benchmark parsing classic JSON benchmarks converted to YAML.
//!
//! The JSON fixtures are loaded, parsed, serialized to YAML, then we benchmark
//! deserializing that YAML data.

use divan::{Bencher, black_box};
use facet::Facet;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
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
// Types for twitter.json
// =============================================================================

#[derive(Debug, Deserialize, Serialize, Facet)]
struct Twitter<'a> {
    #[serde(borrow)]
    statuses: Vec<Status<'a>>,
    #[serde(borrow)]
    search_metadata: SearchMetadata<'a>,
}

#[derive(Debug, Deserialize, Serialize, Facet)]
struct SearchMetadata<'a> {
    completed_in: f64,
    max_id: u64,
    #[serde(borrow)]
    max_id_str: Cow<'a, str>,
    #[serde(borrow)]
    next_results: Cow<'a, str>,
    #[serde(borrow)]
    query: Cow<'a, str>,
    #[serde(borrow)]
    refresh_url: Cow<'a, str>,
    count: u64,
    since_id: u64,
    #[serde(borrow)]
    since_id_str: Cow<'a, str>,
}

#[derive(Debug, Deserialize, Serialize, Facet)]
struct Status<'a> {
    #[serde(borrow)]
    metadata: Metadata<'a>,
    #[serde(borrow)]
    created_at: Cow<'a, str>,
    id: u64,
    #[serde(borrow)]
    id_str: Cow<'a, str>,
    #[serde(borrow)]
    text: Cow<'a, str>,
    #[serde(borrow)]
    source: Cow<'a, str>,
    truncated: bool,
    in_reply_to_status_id: Option<u64>,
    #[serde(borrow)]
    in_reply_to_status_id_str: Option<Cow<'a, str>>,
    in_reply_to_user_id: Option<u64>,
    #[serde(borrow)]
    in_reply_to_user_id_str: Option<Cow<'a, str>>,
    #[serde(borrow)]
    in_reply_to_screen_name: Option<Cow<'a, str>>,
    #[serde(borrow)]
    user: User<'a>,
    geo: Option<()>,
    coordinates: Option<()>,
    place: Option<()>,
    contributors: Option<()>,
    retweet_count: u64,
    favorite_count: u64,
    #[serde(borrow)]
    entities: Entities<'a>,
    favorited: bool,
    retweeted: bool,
    #[serde(borrow)]
    lang: Cow<'a, str>,
    #[serde(default, borrow)]
    #[facet(default)]
    retweeted_status: Option<Box<Status<'a>>>,
    #[serde(default)]
    #[facet(default)]
    possibly_sensitive: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, Facet)]
struct Metadata<'a> {
    #[serde(borrow)]
    result_type: Cow<'a, str>,
    #[serde(borrow)]
    iso_language_code: Cow<'a, str>,
}

#[derive(Debug, Deserialize, Serialize, Facet)]
struct User<'a> {
    id: u64,
    #[serde(borrow)]
    id_str: Cow<'a, str>,
    #[serde(borrow)]
    name: Cow<'a, str>,
    #[serde(borrow)]
    screen_name: Cow<'a, str>,
    #[serde(borrow)]
    location: Cow<'a, str>,
    #[serde(borrow)]
    description: Cow<'a, str>,
    #[serde(borrow)]
    url: Option<Cow<'a, str>>,
    #[serde(borrow)]
    entities: UserEntities<'a>,
    protected: bool,
    followers_count: u64,
    friends_count: u64,
    listed_count: u64,
    #[serde(borrow)]
    created_at: Cow<'a, str>,
    favourites_count: u64,
    utc_offset: Option<i64>,
    #[serde(borrow)]
    time_zone: Option<Cow<'a, str>>,
    geo_enabled: bool,
    verified: bool,
    statuses_count: u64,
    #[serde(borrow)]
    lang: Cow<'a, str>,
    contributors_enabled: bool,
    is_translator: bool,
    is_translation_enabled: bool,
    #[serde(borrow)]
    profile_background_color: Cow<'a, str>,
    #[serde(borrow)]
    profile_background_image_url: Cow<'a, str>,
    #[serde(borrow)]
    profile_background_image_url_https: Cow<'a, str>,
    profile_background_tile: bool,
    #[serde(borrow)]
    profile_image_url: Cow<'a, str>,
    #[serde(borrow)]
    profile_image_url_https: Cow<'a, str>,
    #[serde(default, borrow)]
    #[facet(default)]
    profile_banner_url: Option<Cow<'a, str>>,
    #[serde(borrow)]
    profile_link_color: Cow<'a, str>,
    #[serde(borrow)]
    profile_sidebar_border_color: Cow<'a, str>,
    #[serde(borrow)]
    profile_sidebar_fill_color: Cow<'a, str>,
    #[serde(borrow)]
    profile_text_color: Cow<'a, str>,
    profile_use_background_image: bool,
    default_profile: bool,
    default_profile_image: bool,
    following: bool,
    follow_request_sent: bool,
    notifications: bool,
}

#[derive(Debug, Deserialize, Serialize, Facet)]
struct UserEntities<'a> {
    #[serde(default, borrow)]
    #[facet(default)]
    url: Option<EntityUrl<'a>>,
    #[serde(borrow)]
    description: EntityDescription<'a>,
}

#[derive(Debug, Deserialize, Serialize, Facet)]
struct EntityUrl<'a> {
    #[serde(borrow)]
    urls: Vec<Url<'a>>,
}

#[derive(Debug, Deserialize, Serialize, Facet)]
struct EntityDescription<'a> {
    #[serde(borrow)]
    urls: Vec<Url<'a>>,
}

#[derive(Debug, Deserialize, Serialize, Facet)]
struct Entities<'a> {
    #[serde(borrow)]
    hashtags: Vec<Hashtag<'a>>,
    #[serde(borrow)]
    symbols: Vec<Symbol<'a>>,
    #[serde(borrow)]
    urls: Vec<Url<'a>>,
    #[serde(borrow)]
    user_mentions: Vec<UserMention<'a>>,
    #[serde(default, borrow)]
    #[facet(default)]
    media: Option<Vec<Media<'a>>>,
}

#[derive(Debug, Deserialize, Serialize, Facet)]
struct Hashtag<'a> {
    #[serde(borrow)]
    text: Cow<'a, str>,
    indices: Vec<u64>,
}

#[derive(Debug, Deserialize, Serialize, Facet)]
struct Symbol<'a> {
    #[serde(borrow)]
    text: Cow<'a, str>,
    indices: Vec<u64>,
}

#[derive(Debug, Deserialize, Serialize, Facet)]
struct Url<'a> {
    #[serde(borrow)]
    url: Cow<'a, str>,
    #[serde(borrow)]
    expanded_url: Cow<'a, str>,
    #[serde(borrow)]
    display_url: Cow<'a, str>,
    indices: Vec<u64>,
}

#[derive(Debug, Deserialize, Serialize, Facet)]
struct UserMention<'a> {
    #[serde(borrow)]
    screen_name: Cow<'a, str>,
    #[serde(borrow)]
    name: Cow<'a, str>,
    id: u64,
    #[serde(borrow)]
    id_str: Cow<'a, str>,
    indices: Vec<u64>,
}

#[derive(Debug, Deserialize, Serialize, Facet)]
struct Media<'a> {
    id: u64,
    #[serde(borrow)]
    id_str: Cow<'a, str>,
    indices: Vec<u64>,
    #[serde(borrow)]
    media_url: Cow<'a, str>,
    #[serde(borrow)]
    media_url_https: Cow<'a, str>,
    #[serde(borrow)]
    url: Cow<'a, str>,
    #[serde(borrow)]
    display_url: Cow<'a, str>,
    #[serde(borrow)]
    expanded_url: Cow<'a, str>,
    #[serde(rename = "type", borrow)]
    #[facet(rename = "type")]
    media_type: Cow<'a, str>,
    #[serde(borrow)]
    sizes: Sizes<'a>,
    #[serde(default)]
    #[facet(default)]
    source_status_id: Option<u64>,
    #[serde(default, borrow)]
    #[facet(default)]
    source_status_id_str: Option<Cow<'a, str>>,
}

#[derive(Debug, Deserialize, Serialize, Facet)]
struct Sizes<'a> {
    #[serde(borrow)]
    medium: Size<'a>,
    #[serde(borrow)]
    small: Size<'a>,
    #[serde(borrow)]
    thumb: Size<'a>,
    #[serde(borrow)]
    large: Size<'a>,
}

#[derive(Debug, Deserialize, Serialize, Facet)]
struct Size<'a> {
    w: u64,
    h: u64,
    #[serde(borrow)]
    resize: Cow<'a, str>,
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
// Data loading - convert JSON fixtures to YAML
// =============================================================================

static CITM_YAML: LazyLock<String> = LazyLock::new(|| {
    let json_str = &*facet_json_classics::CITM_CATALOG;
    let data: CitmCatalog = serde_json::from_str(json_str).expect("Failed to parse citm JSON");
    serde_yaml::to_string(&data).expect("Failed to serialize citm to YAML")
});

static TWITTER_YAML: LazyLock<String> = LazyLock::new(|| {
    let json_str = &*facet_json_classics::TWITTER;
    let data: Twitter = serde_json::from_str(json_str).expect("Failed to parse twitter JSON");
    serde_yaml::to_string(&data).expect("Failed to serialize twitter to YAML")
});

static CANADA_YAML: LazyLock<String> = LazyLock::new(|| {
    let json_str = &*facet_json_classics::CANADA;
    let data: FeatureCollection =
        serde_json::from_str(json_str).expect("Failed to parse canada JSON");
    serde_yaml::to_string(&data).expect("Failed to serialize canada to YAML")
});

// =============================================================================
// Benchmarks - citm
// =============================================================================

#[divan::bench]
fn citm_serde_yaml(bencher: Bencher) {
    let data = &*CITM_YAML;
    bencher.bench(|| {
        let result: CitmCatalog = black_box(serde_yaml::from_str(black_box(data)).unwrap());
        black_box(result)
    });
}

#[divan::bench]
fn citm_facet_yaml(bencher: Bencher) {
    let data = &*CITM_YAML;
    bencher.bench(|| {
        let result: CitmCatalog = black_box(facet_yaml::from_str(black_box(data)).unwrap());
        black_box(result)
    });
}

// =============================================================================
// Benchmarks - twitter
// =============================================================================

#[divan::bench]
fn twitter_serde_yaml(bencher: Bencher) {
    let data = &*TWITTER_YAML;
    bencher.bench(|| {
        let result: Twitter = black_box(serde_yaml::from_str(black_box(data)).unwrap());
        black_box(result)
    });
}

#[divan::bench]
fn twitter_facet_yaml(bencher: Bencher) {
    let data = &*TWITTER_YAML;
    bencher.bench(|| {
        let result: Twitter = black_box(facet_yaml::from_str_borrowed(black_box(data)).unwrap());
        black_box(result)
    });
}

// =============================================================================
// Benchmarks - canada
// =============================================================================

#[divan::bench]
fn canada_serde_yaml(bencher: Bencher) {
    let data = &*CANADA_YAML;
    bencher.bench(|| {
        let result: FeatureCollection = black_box(serde_yaml::from_str(black_box(data)).unwrap());
        black_box(result)
    });
}

#[divan::bench]
fn canada_facet_yaml(bencher: Bencher) {
    let data = &*CANADA_YAML;
    bencher.bench(|| {
        let result: FeatureCollection = black_box(facet_yaml::from_str(black_box(data)).unwrap());
        black_box(result)
    });
}
