//! Benchmark parsing classic JSON benchmarks converted to TOML.
//!
//! The JSON fixtures are loaded, parsed, serialized to TOML, then we benchmark
//! deserializing that TOML data.

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
    #[serde(default)]
    in_reply_to_status_id: Option<u64>,
    #[serde(default)]
    in_reply_to_status_id_str: Option<&'a str>,
    #[serde(default)]
    in_reply_to_user_id: Option<u64>,
    #[serde(default)]
    in_reply_to_user_id_str: Option<&'a str>,
    #[serde(default)]
    in_reply_to_screen_name: Option<&'a str>,
    #[serde(borrow)]
    user: User<'a>,
    #[serde(default)]
    geo: Option<()>,
    #[serde(default)]
    coordinates: Option<()>,
    #[serde(default)]
    place: Option<()>,
    #[serde(default)]
    contributors: Option<()>,
    retweet_count: u64,
    favorite_count: u64,
    #[serde(borrow)]
    entities: Entities<'a>,
    favorited: bool,
    retweeted: bool,
    lang: &'a str,
    #[serde(default, borrow)]
    retweeted_status: Option<Box<Status<'a>>>,
    #[serde(default)]
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
// Owned Twitter types for TOML serde (can't do zero-copy deserialization)
// =============================================================================

mod owned {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Deserialize, Serialize)]
    pub struct Twitter {
        pub statuses: Vec<Status>,
        pub search_metadata: SearchMetadata,
    }

    #[derive(Debug, Deserialize, Serialize)]
    pub struct SearchMetadata {
        pub completed_in: f64,
        pub max_id: u64,
        pub max_id_str: String,
        pub next_results: String,
        pub query: String,
        pub refresh_url: String,
        pub count: u64,
        pub since_id: u64,
        pub since_id_str: String,
    }

    #[derive(Debug, Deserialize, Serialize)]
    pub struct Status {
        pub metadata: Metadata,
        pub created_at: String,
        pub id: u64,
        pub id_str: String,
        pub text: String,
        pub source: String,
        pub truncated: bool,
        pub in_reply_to_status_id: Option<u64>,
        pub in_reply_to_status_id_str: Option<String>,
        pub in_reply_to_user_id: Option<u64>,
        pub in_reply_to_user_id_str: Option<String>,
        pub in_reply_to_screen_name: Option<String>,
        pub user: User,
        pub geo: Option<()>,
        pub coordinates: Option<()>,
        pub place: Option<()>,
        pub contributors: Option<()>,
        pub retweet_count: u64,
        pub favorite_count: u64,
        pub entities: Entities,
        pub favorited: bool,
        pub retweeted: bool,
        pub lang: String,
        #[serde(default)]
        pub retweeted_status: Option<Box<Status>>,
        #[serde(default)]
        pub possibly_sensitive: Option<bool>,
    }

    #[derive(Debug, Deserialize, Serialize)]
    pub struct Metadata {
        pub result_type: String,
        pub iso_language_code: String,
    }

    #[derive(Debug, Deserialize, Serialize)]
    pub struct User {
        pub id: u64,
        pub id_str: String,
        pub name: String,
        pub screen_name: String,
        pub location: String,
        pub description: String,
        pub url: Option<String>,
        pub entities: UserEntities,
        pub protected: bool,
        pub followers_count: u64,
        pub friends_count: u64,
        pub listed_count: u64,
        pub created_at: String,
        pub favourites_count: u64,
        pub utc_offset: Option<i64>,
        pub time_zone: Option<String>,
        pub geo_enabled: bool,
        pub verified: bool,
        pub statuses_count: u64,
        pub lang: String,
        pub contributors_enabled: bool,
        pub is_translator: bool,
        pub is_translation_enabled: bool,
        pub profile_background_color: String,
        pub profile_background_image_url: String,
        pub profile_background_image_url_https: String,
        pub profile_background_tile: bool,
        pub profile_image_url: String,
        pub profile_image_url_https: String,
        #[serde(default)]
        pub profile_banner_url: Option<String>,
        pub profile_link_color: String,
        pub profile_sidebar_border_color: String,
        pub profile_sidebar_fill_color: String,
        pub profile_text_color: String,
        pub profile_use_background_image: bool,
        pub default_profile: bool,
        pub default_profile_image: bool,
        pub following: bool,
        pub follow_request_sent: bool,
        pub notifications: bool,
    }

    #[derive(Debug, Deserialize, Serialize)]
    pub struct UserEntities {
        #[serde(default)]
        pub url: Option<EntityUrl>,
        pub description: EntityDescription,
    }

    #[derive(Debug, Deserialize, Serialize)]
    pub struct EntityUrl {
        pub urls: Vec<Url>,
    }

    #[derive(Debug, Deserialize, Serialize)]
    pub struct EntityDescription {
        pub urls: Vec<Url>,
    }

    #[derive(Debug, Deserialize, Serialize)]
    pub struct Entities {
        pub hashtags: Vec<Hashtag>,
        pub symbols: Vec<Symbol>,
        pub urls: Vec<Url>,
        pub user_mentions: Vec<UserMention>,
        #[serde(default)]
        pub media: Option<Vec<Media>>,
    }

    #[derive(Debug, Deserialize, Serialize)]
    pub struct Hashtag {
        pub text: String,
        pub indices: Vec<u64>,
    }

    #[derive(Debug, Deserialize, Serialize)]
    pub struct Symbol {
        pub text: String,
        pub indices: Vec<u64>,
    }

    #[derive(Debug, Deserialize, Serialize)]
    pub struct Url {
        pub url: String,
        pub expanded_url: String,
        pub display_url: String,
        pub indices: Vec<u64>,
    }

    #[derive(Debug, Deserialize, Serialize)]
    pub struct UserMention {
        pub screen_name: String,
        pub name: String,
        pub id: u64,
        pub id_str: String,
        pub indices: Vec<u64>,
    }

    #[derive(Debug, Deserialize, Serialize)]
    pub struct Media {
        pub id: u64,
        pub id_str: String,
        pub indices: Vec<u64>,
        pub media_url: String,
        pub media_url_https: String,
        pub url: String,
        pub display_url: String,
        pub expanded_url: String,
        #[serde(rename = "type")]
        pub media_type: String,
        pub sizes: Sizes,
        #[serde(default)]
        pub source_status_id: Option<u64>,
        #[serde(default)]
        pub source_status_id_str: Option<String>,
    }

    #[derive(Debug, Deserialize, Serialize)]
    pub struct Sizes {
        pub medium: Size,
        pub small: Size,
        pub thumb: Size,
        pub large: Size,
    }

    #[derive(Debug, Deserialize, Serialize)]
    pub struct Size {
        pub w: u64,
        pub h: u64,
        pub resize: String,
    }
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
// Data loading - convert JSON fixtures to TOML
// =============================================================================

static CITM_TOML: LazyLock<String> = LazyLock::new(|| {
    let json_str = &*facet_json_classics::CITM_CATALOG;
    let data: CitmCatalog = serde_json::from_str(json_str).expect("Failed to parse citm JSON");
    toml::to_string(&data).expect("Failed to serialize citm to TOML")
});

static TWITTER_TOML: LazyLock<String> = LazyLock::new(|| {
    let json_str = &*facet_json_classics::TWITTER;
    let data: owned::Twitter =
        serde_json::from_str(json_str).expect("Failed to parse twitter JSON");
    toml::to_string(&data).expect("Failed to serialize twitter to TOML")
});

static CANADA_TOML: LazyLock<String> = LazyLock::new(|| {
    let json_str = &*facet_json_classics::CANADA;
    let data: FeatureCollection =
        serde_json::from_str(json_str).expect("Failed to parse canada JSON");
    toml::to_string(&data).expect("Failed to serialize canada to TOML")
});

// =============================================================================
// Benchmarks - citm
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

// =============================================================================
// Benchmarks - twitter
// =============================================================================

#[divan::bench]
fn twitter_toml_serde(bencher: Bencher) {
    let data = &*TWITTER_TOML;
    bencher.bench(|| {
        let result: owned::Twitter = black_box(toml::from_str(black_box(data)).unwrap());
        black_box(result)
    });
}

#[divan::bench]
fn twitter_facet_toml(bencher: Bencher) {
    let data = &*TWITTER_TOML;
    bencher.bench(|| {
        let result: Twitter = black_box(facet_toml::from_str_borrowed(black_box(data)).unwrap());
        black_box(result)
    });
}

// =============================================================================
// Benchmarks - canada
// =============================================================================

#[divan::bench]
fn canada_toml_serde(bencher: Bencher) {
    let data = &*CANADA_TOML;
    bencher.bench(|| {
        let result: FeatureCollection = black_box(toml::from_str(black_box(data)).unwrap());
        black_box(result)
    });
}

#[divan::bench]
fn canada_facet_toml(bencher: Bencher) {
    let data = &*CANADA_TOML;
    bencher.bench(|| {
        let result: FeatureCollection = black_box(facet_toml::from_str(black_box(data)).unwrap());
        black_box(result)
    });
}
