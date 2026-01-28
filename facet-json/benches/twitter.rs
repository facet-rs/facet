//! Benchmark parsing twitter.json from simdjson.
//!
//! Run with:
//!   TWITTER_PATH=/path/to/twitter.json cargo bench -p facet-json --bench twitter
//!
//! Download the file from:
//!   https://raw.githubusercontent.com/simdjson/simdjson/master/jsonexamples/twitter.json

use divan::{Bencher, black_box};
use facet::Facet;
use serde::Deserialize;
use std::borrow::Cow;
use std::sync::LazyLock;

fn main() {
    divan::main();
}

// =============================================================================
// Types for twitter.json
// =============================================================================

#[derive(Debug, Deserialize, Facet)]
struct Twitter<'a> {
    #[serde(borrow)]
    statuses: Vec<Status<'a>>,
    #[serde(borrow)]
    search_metadata: SearchMetadata<'a>,
}

#[derive(Debug, Deserialize, Facet)]
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

#[derive(Debug, Deserialize, Facet)]
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

#[derive(Debug, Deserialize, Facet)]
struct Metadata<'a> {
    result_type: &'a str,
    iso_language_code: &'a str,
}

#[derive(Debug, Deserialize, Facet)]
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

#[derive(Debug, Deserialize, Facet)]
struct UserEntities<'a> {
    #[serde(default, borrow)]
    #[facet(default)]
    url: Option<EntityUrl<'a>>,
    #[serde(borrow)]
    description: EntityDescription<'a>,
}

#[derive(Debug, Deserialize, Facet)]
struct EntityUrl<'a> {
    #[serde(borrow)]
    urls: Vec<Url<'a>>,
}

#[derive(Debug, Deserialize, Facet)]
struct EntityDescription<'a> {
    #[serde(borrow)]
    urls: Vec<Url<'a>>,
}

#[derive(Debug, Deserialize, Facet)]
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

#[derive(Debug, Deserialize, Facet)]
struct Hashtag<'a> {
    #[serde(borrow)]
    text: Cow<'a, str>,
    indices: Vec<u64>,
}

#[derive(Debug, Deserialize, Facet)]
struct Symbol<'a> {
    #[allow(dead_code)]
    #[serde(borrow)]
    text: Cow<'a, str>,
    #[allow(dead_code)]
    indices: Vec<u64>,
}

#[derive(Debug, Deserialize, Facet)]
struct Url<'a> {
    url: &'a str,
    expanded_url: &'a str,
    display_url: &'a str,
    indices: Vec<u64>,
}

#[derive(Debug, Deserialize, Facet)]
struct UserMention<'a> {
    screen_name: &'a str,
    #[serde(borrow)]
    name: Cow<'a, str>,
    id: u64,
    id_str: &'a str,
    indices: Vec<u64>,
}

#[derive(Debug, Deserialize, Facet)]
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

#[derive(Debug, Deserialize, Facet)]
struct Sizes {
    medium: Size,
    small: Size,
    thumb: Size,
    large: Size,
}

#[derive(Debug, Deserialize, Facet)]
struct Size {
    w: u64,
    h: u64,
    resize: String,
}

// =============================================================================
// Owned types for serde (serde_json doesn't zero-copy borrow as easily)
// =============================================================================

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct TwitterOwned {
    statuses: Vec<StatusOwned>,
    search_metadata: SearchMetadataOwned,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct SearchMetadataOwned {
    completed_in: f64,
    max_id: u64,
    max_id_str: String,
    next_results: String,
    query: String,
    refresh_url: String,
    count: u64,
    since_id: u64,
    since_id_str: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct StatusOwned {
    metadata: MetadataOwned,
    created_at: String,
    id: u64,
    id_str: String,
    text: String,
    source: String,
    truncated: bool,
    in_reply_to_status_id: Option<u64>,
    in_reply_to_status_id_str: Option<String>,
    in_reply_to_user_id: Option<u64>,
    in_reply_to_user_id_str: Option<String>,
    in_reply_to_screen_name: Option<String>,
    user: UserOwned,
    geo: Option<()>,
    coordinates: Option<()>,
    place: Option<()>,
    contributors: Option<()>,
    retweet_count: u64,
    favorite_count: u64,
    entities: EntitiesOwned,
    favorited: bool,
    retweeted: bool,
    lang: String,
    #[serde(default)]
    retweeted_status: Option<Box<StatusOwned>>,
    #[serde(default)]
    possibly_sensitive: Option<bool>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct MetadataOwned {
    result_type: String,
    iso_language_code: String,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct UserOwned {
    id: u64,
    id_str: String,
    name: String,
    screen_name: String,
    location: String,
    description: String,
    url: Option<String>,
    entities: UserEntitiesOwned,
    protected: bool,
    followers_count: u64,
    friends_count: u64,
    listed_count: u64,
    created_at: String,
    favourites_count: u64,
    utc_offset: Option<i64>,
    time_zone: Option<String>,
    geo_enabled: bool,
    verified: bool,
    statuses_count: u64,
    lang: String,
    contributors_enabled: bool,
    is_translator: bool,
    is_translation_enabled: bool,
    profile_background_color: String,
    profile_background_image_url: String,
    profile_background_image_url_https: String,
    profile_background_tile: bool,
    profile_image_url: String,
    profile_image_url_https: String,
    #[serde(default)]
    profile_banner_url: Option<String>,
    profile_link_color: String,
    profile_sidebar_border_color: String,
    profile_sidebar_fill_color: String,
    profile_text_color: String,
    profile_use_background_image: bool,
    default_profile: bool,
    default_profile_image: bool,
    following: bool,
    follow_request_sent: bool,
    notifications: bool,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct UserEntitiesOwned {
    #[serde(default)]
    url: Option<EntityUrlOwned>,
    description: EntityDescriptionOwned,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct EntityUrlOwned {
    urls: Vec<UrlOwned>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct EntityDescriptionOwned {
    urls: Vec<UrlOwned>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct EntitiesOwned {
    hashtags: Vec<HashtagOwned>,
    symbols: Vec<SymbolOwned>,
    urls: Vec<UrlOwned>,
    user_mentions: Vec<UserMentionOwned>,
    #[serde(default)]
    media: Option<Vec<MediaOwned>>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct HashtagOwned {
    text: String,
    indices: Vec<u64>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct SymbolOwned {
    #[allow(dead_code)]
    text: String,
    #[allow(dead_code)]
    indices: Vec<u64>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct UrlOwned {
    url: String,
    expanded_url: String,
    display_url: String,
    indices: Vec<u64>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct UserMentionOwned {
    screen_name: String,
    name: String,
    id: u64,
    id_str: String,
    indices: Vec<u64>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
struct MediaOwned {
    id: u64,
    id_str: String,
    indices: Vec<u64>,
    media_url: String,
    media_url_https: String,
    url: String,
    display_url: String,
    expanded_url: String,
    #[serde(rename = "type")]
    media_type: String,
    sizes: Sizes,
    #[serde(default)]
    source_status_id: Option<u64>,
    #[serde(default)]
    source_status_id_str: Option<String>,
}

// =============================================================================
// Data loading
// =============================================================================

static JSON_STR: LazyLock<String> = LazyLock::new(|| {
    let path = std::env::var("TWITTER_PATH").unwrap_or_else(|_| {
        // Default: look in parent of workspace root
        concat!(env!("CARGO_MANIFEST_DIR"), "/../../twitter.json").to_string()
    });
    std::fs::read_to_string(&path).unwrap_or_else(|e| {
        panic!(
            "Failed to read {path}: {e}\n\
             Download from: https://raw.githubusercontent.com/simdjson/simdjson/master/jsonexamples/twitter.json"
        )
    })
});

// =============================================================================
// Benchmarks
// =============================================================================

#[divan::bench]
fn serde_json(bencher: Bencher) {
    let data = &*JSON_STR;
    bencher.bench(|| {
        let result: TwitterOwned = black_box(serde_json::from_str(black_box(data)).unwrap());
        black_box(result)
    });
}

#[divan::bench]
fn facet_json(bencher: Bencher) {
    let data = &*JSON_STR;
    bencher.bench(|| {
        let result: Twitter = black_box(facet_json::from_str_borrowed(black_box(data)).unwrap());
        black_box(result)
    });
}
