//! Benchmark parsing classic JSON benchmarks converted to MessagePack binary format.
//!
//! The JSON fixtures are loaded, parsed, serialized to MessagePack, then we benchmark
//! deserializing that binary data.

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
    max_id_str: &'a str,
    next_results: &'a str,
    query: &'a str,
    refresh_url: &'a str,
    count: u64,
    since_id: u64,
    since_id_str: &'a str,
}

#[derive(Debug, Deserialize, Serialize, Facet)]
struct Status<'a> {
    #[serde(borrow)]
    metadata: Metadata<'a>,
    created_at: &'a str,
    id: u64,
    id_str: &'a str,
    #[serde(borrow)]
    text: Cow<'a, str>,
    #[serde(borrow)]
    source: Cow<'a, str>,
    truncated: bool,
    in_reply_to_status_id: Option<u64>,
    in_reply_to_status_id_str: Option<&'a str>,
    in_reply_to_user_id: Option<u64>,
    in_reply_to_user_id_str: Option<&'a str>,
    in_reply_to_screen_name: Option<&'a str>,
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
    lang: &'a str,
    #[serde(default, borrow)]
    #[facet(default)]
    retweeted_status: Option<Box<Status<'a>>>,
    #[serde(default)]
    #[facet(default)]
    possibly_sensitive: Option<bool>,
}

#[derive(Debug, Deserialize, Serialize, Facet)]
struct Metadata<'a> {
    result_type: &'a str,
    iso_language_code: &'a str,
}

#[derive(Debug, Deserialize, Serialize, Facet)]
struct User<'a> {
    id: u64,
    id_str: &'a str,
    #[serde(borrow)]
    name: Cow<'a, str>,
    screen_name: &'a str,
    #[serde(borrow)]
    location: Cow<'a, str>,
    #[serde(borrow)]
    description: Cow<'a, str>,
    url: Option<&'a str>,
    #[serde(borrow)]
    entities: UserEntities<'a>,
    protected: bool,
    followers_count: u64,
    friends_count: u64,
    listed_count: u64,
    created_at: &'a str,
    favourites_count: u64,
    utc_offset: Option<i64>,
    time_zone: Option<&'a str>,
    geo_enabled: bool,
    verified: bool,
    statuses_count: u64,
    lang: &'a str,
    contributors_enabled: bool,
    is_translator: bool,
    is_translation_enabled: bool,
    profile_background_color: &'a str,
    profile_background_image_url: &'a str,
    profile_background_image_url_https: &'a str,
    profile_background_tile: bool,
    profile_image_url: &'a str,
    profile_image_url_https: &'a str,
    #[serde(default)]
    #[facet(default)]
    profile_banner_url: Option<&'a str>,
    profile_link_color: &'a str,
    profile_sidebar_border_color: &'a str,
    profile_sidebar_fill_color: &'a str,
    profile_text_color: &'a str,
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
    url: &'a str,
    expanded_url: &'a str,
    display_url: &'a str,
    indices: Vec<u64>,
}

#[derive(Debug, Deserialize, Serialize, Facet)]
struct UserMention<'a> {
    screen_name: &'a str,
    #[serde(borrow)]
    name: Cow<'a, str>,
    id: u64,
    id_str: &'a str,
    indices: Vec<u64>,
}

#[derive(Debug, Deserialize, Serialize, Facet)]
struct Media<'a> {
    id: u64,
    id_str: &'a str,
    indices: Vec<u64>,
    media_url: &'a str,
    media_url_https: &'a str,
    url: &'a str,
    display_url: &'a str,
    expanded_url: &'a str,
    #[serde(rename = "type")]
    #[facet(rename = "type")]
    media_type: &'a str,
    sizes: Sizes,
    #[serde(default)]
    #[facet(default)]
    source_status_id: Option<u64>,
    #[serde(default)]
    #[facet(default)]
    source_status_id_str: Option<&'a str>,
}

#[derive(Debug, Deserialize, Serialize, Facet)]
struct Sizes {
    medium: Size,
    small: Size,
    thumb: Size,
    large: Size,
}

#[derive(Debug, Deserialize, Serialize, Facet)]
struct Size {
    w: u64,
    h: u64,
    resize: String,
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
// Data loading - convert JSON fixtures to MessagePack
// =============================================================================

static CITM_MSGPACK: LazyLock<Vec<u8>> = LazyLock::new(|| {
    let json_str = &*facet_json_classics::CITM_CATALOG;
    let data: CitmCatalog = serde_json::from_str(json_str).expect("Failed to parse citm JSON");
    rmp_serde::to_vec_named(&data).expect("Failed to serialize citm to msgpack")
});

static TWITTER_MSGPACK: LazyLock<Vec<u8>> = LazyLock::new(|| {
    let json_str = &*facet_json_classics::TWITTER;
    let data: Twitter = serde_json::from_str(json_str).expect("Failed to parse twitter JSON");
    rmp_serde::to_vec_named(&data).expect("Failed to serialize twitter to msgpack")
});

static CANADA_MSGPACK: LazyLock<Vec<u8>> = LazyLock::new(|| {
    let json_str = &*facet_json_classics::CANADA;
    let data: FeatureCollection =
        serde_json::from_str(json_str).expect("Failed to parse canada JSON");
    rmp_serde::to_vec_named(&data).expect("Failed to serialize canada to msgpack")
});

// =============================================================================
// Benchmarks - citm
// =============================================================================

#[divan::bench]
fn citm_rmp_serde(bencher: Bencher) {
    let data = &*CITM_MSGPACK;
    bencher.bench(|| {
        let result: CitmCatalog = black_box(rmp_serde::from_slice(black_box(data)).unwrap());
        black_box(result)
    });
}

#[divan::bench]
fn citm_facet_msgpack(bencher: Bencher) {
    let data = &*CITM_MSGPACK;
    bencher.bench(|| {
        let result: CitmCatalog = black_box(facet_msgpack::from_slice(black_box(data)).unwrap());
        black_box(result)
    });
}

// =============================================================================
// Benchmarks - twitter
// =============================================================================

#[divan::bench]
fn twitter_rmp_serde(bencher: Bencher) {
    let data = &*TWITTER_MSGPACK;
    bencher.bench(|| {
        let result: Twitter = black_box(rmp_serde::from_slice(black_box(data)).unwrap());
        black_box(result)
    });
}

#[divan::bench]
fn twitter_facet_msgpack(bencher: Bencher) {
    let data = &*TWITTER_MSGPACK;
    bencher.bench(|| {
        let result: Twitter =
            black_box(facet_msgpack::from_slice_borrowed(black_box(data)).unwrap());
        black_box(result)
    });
}

// =============================================================================
// Benchmarks - canada
// =============================================================================

#[divan::bench]
fn canada_rmp_serde(bencher: Bencher) {
    let data = &*CANADA_MSGPACK;
    bencher.bench(|| {
        let result: FeatureCollection = black_box(rmp_serde::from_slice(black_box(data)).unwrap());
        black_box(result)
    });
}

#[divan::bench]
fn canada_facet_msgpack(bencher: Bencher) {
    let data = &*CANADA_MSGPACK;
    bencher.bench(|| {
        let result: FeatureCollection =
            black_box(facet_msgpack::from_slice(black_box(data)).unwrap());
        black_box(result)
    });
}
