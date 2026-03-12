//! Profile twitter.json parsing (for callgrind, etc.)
//!
//! cargo build --profile profiling -p facet-json --example profile_twitter
//! valgrind --tool=callgrind ./target/profiling/examples/profile_twitter

use facet::Facet;
use facet_json::from_str;
use std::hint::black_box;

#[derive(Debug, Facet)]
struct Twitter {
    statuses: Vec<Status>,
    search_metadata: SearchMetadata,
}

#[derive(Debug, Facet)]
struct SearchMetadata {
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

#[derive(Debug, Facet)]
struct Status {
    metadata: Metadata,
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
    user: User,
    geo: Option<()>,
    coordinates: Option<()>,
    place: Option<()>,
    contributors: Option<()>,
    retweet_count: u64,
    favorite_count: u64,
    entities: Entities,
    favorited: bool,
    retweeted: bool,
    lang: String,
    #[facet(default, recursive_type)]
    retweeted_status: Option<Box<Status>>,
    #[facet(default)]
    possibly_sensitive: Option<bool>,
}

#[derive(Debug, Facet)]
struct Metadata {
    result_type: String,
    iso_language_code: String,
}

#[derive(Debug, Facet)]
struct User {
    id: u64,
    id_str: String,
    name: String,
    screen_name: String,
    location: String,
    description: String,
    url: Option<String>,
    entities: UserEntities,
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
    #[facet(default)]
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

#[derive(Debug, Facet)]
struct UserEntities {
    #[facet(default)]
    url: Option<EntityUrl>,
    description: EntityDescription,
}

#[derive(Debug, Facet)]
struct EntityUrl {
    urls: Vec<Url>,
}

#[derive(Debug, Facet)]
struct EntityDescription {
    urls: Vec<Url>,
}

#[derive(Debug, Facet)]
struct Entities {
    hashtags: Vec<Hashtag>,
    symbols: Vec<Symbol>,
    urls: Vec<Url>,
    user_mentions: Vec<UserMention>,
    #[facet(default)]
    media: Option<Vec<Media>>,
}

#[derive(Debug, Facet)]
struct Hashtag {
    text: String,
    indices: Vec<u64>,
}

#[derive(Debug, Facet)]
struct Symbol {
    text: String,
    indices: Vec<u64>,
}

#[derive(Debug, Facet)]
struct Url {
    url: String,
    expanded_url: String,
    display_url: String,
    indices: Vec<u64>,
}

#[derive(Debug, Facet)]
struct UserMention {
    screen_name: String,
    name: String,
    id: u64,
    id_str: String,
    indices: Vec<u64>,
}

#[derive(Debug, Facet)]
struct Media {
    id: u64,
    id_str: String,
    indices: Vec<u64>,
    media_url: String,
    media_url_https: String,
    url: String,
    display_url: String,
    expanded_url: String,
    #[facet(rename = "type")]
    media_type: String,
    sizes: Sizes,
    #[facet(default)]
    source_status_id: Option<u64>,
    #[facet(default)]
    source_status_id_str: Option<String>,
}

#[derive(Debug, Facet)]
struct Sizes {
    medium: Size,
    small: Size,
    thumb: Size,
    large: Size,
}

#[derive(Debug, Facet)]
struct Size {
    w: u64,
    h: u64,
    resize: String,
}

fn main() {
    let json = std::fs::read_to_string(concat!(env!("CARGO_MANIFEST_DIR"), "/../../twitter.json"))
        .expect("twitter.json not found");

    for _ in 0..100 {
        let result: Twitter = from_str(&json).unwrap();
        black_box(result);
    }
}
