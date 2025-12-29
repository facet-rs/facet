//! Synthetic tests for JIT correctness with nested structures.
//!
//! These tests progressively increase complexity to find where the JIT breaks.

#![cfg(feature = "jit")]

use facet::Facet;
use facet_format::jit as format_jit;
use facet_json::JsonParser;

/// Helper macro to deserialize with JIT (Tier-2)
macro_rules! jit_deserialize {
    ($ty:ty, $json:expr) => {{
        let result = format_jit::deserialize_with_format_jit_fallback::<$ty, _>(JsonParser::new(
            $json.as_bytes(),
        ));
        result.expect("JIT deserialization failed")
    }};
}

// =============================================================================
// Level 0: Flat structs (baseline)
// =============================================================================

#[derive(Debug, PartialEq, Facet)]
struct FlatSimple {
    a: u64,
    b: u64,
}

#[derive(Debug, PartialEq, Facet)]
struct FlatWithString {
    id: u64,
    name: String,
}

#[derive(Debug, PartialEq, Facet)]
struct FlatMixed {
    first: u64,
    second: String,
    third: u32,
}

#[test]
fn test_flat_simple() {
    let json = r#"{"a": 1, "b": 2}"#;
    let result: FlatSimple = jit_deserialize!(FlatSimple, json);
    assert_eq!(result, FlatSimple { a: 1, b: 2 });
}

#[test]
fn test_flat_with_string() {
    let json = r#"{"id": 42, "name": "hello"}"#;
    let result: FlatWithString = jit_deserialize!(FlatWithString, json);
    assert_eq!(
        result,
        FlatWithString {
            id: 42,
            name: "hello".to_string()
        }
    );
}

#[test]
fn test_flat_mixed() {
    let json = r#"{"first": 100, "second": "test", "third": 200}"#;
    let result: FlatMixed = jit_deserialize!(FlatMixed, json);
    assert_eq!(
        result,
        FlatMixed {
            first: 100,
            second: "test".to_string(),
            third: 200
        }
    );
}

// =============================================================================
// Level 1: Nested structs
// =============================================================================

#[derive(Debug, PartialEq, Facet)]
struct Inner {
    x: i64,
    y: i64,
}

#[derive(Debug, PartialEq, Facet)]
struct Outer {
    id: u64,
    inner: Inner,
}

#[derive(Debug, PartialEq, Facet)]
struct OuterWithString {
    id: u64,
    inner: Inner,
    name: String,
}

#[test]
fn test_nested_simple() {
    let json = r#"{"id": 1, "inner": {"x": 10, "y": 20}}"#;
    let result: Outer = jit_deserialize!(Outer, json);
    assert_eq!(
        result,
        Outer {
            id: 1,
            inner: Inner { x: 10, y: 20 }
        }
    );
}

#[test]
fn test_nested_with_string() {
    let json = r#"{"id": 1, "inner": {"x": 10, "y": 20}, "name": "test"}"#;
    let result: OuterWithString = jit_deserialize!(OuterWithString, json);
    assert_eq!(
        result,
        OuterWithString {
            id: 1,
            inner: Inner { x: 10, y: 20 },
            name: "test".to_string(),
        }
    );
}

// =============================================================================
// Level 2: Vec of scalars
// =============================================================================

#[derive(Debug, PartialEq, Facet)]
struct WithVecU64 {
    id: u64,
    values: Vec<u64>,
}

#[derive(Debug, PartialEq, Facet)]
struct WithVecString {
    id: u64,
    names: Vec<String>,
}

#[test]
fn test_vec_u64() {
    let json = r#"{"id": 1, "values": [10, 20, 30]}"#;
    let result: WithVecU64 = jit_deserialize!(WithVecU64, json);
    assert_eq!(
        result,
        WithVecU64 {
            id: 1,
            values: vec![10, 20, 30]
        }
    );
}

#[test]
fn test_vec_string() {
    let json = r#"{"id": 1, "names": ["alice", "bob", "charlie"]}"#;
    let result: WithVecString = jit_deserialize!(WithVecString, json);
    assert_eq!(
        result,
        WithVecString {
            id: 1,
            names: vec![
                "alice".to_string(),
                "bob".to_string(),
                "charlie".to_string()
            ]
        }
    );
}

// =============================================================================
// Level 3: Vec of structs (THIS IS LIKELY WHERE THE BUG IS)
// =============================================================================

#[derive(Debug, PartialEq, Facet, Clone)]
struct Item {
    id: u64,
    name: String,
}

#[derive(Debug, PartialEq, Facet)]
struct WithVecStruct {
    items: Vec<Item>,
}

#[derive(Debug, PartialEq, Facet)]
struct WithVecStructAndMore {
    prefix: String,
    items: Vec<Item>,
    suffix: u64,
}

#[test]
fn test_vec_struct_single() {
    let json = r#"{"items": [{"id": 1, "name": "first"}]}"#;
    let result: WithVecStruct = jit_deserialize!(WithVecStruct, json);
    assert_eq!(
        result,
        WithVecStruct {
            items: vec![Item {
                id: 1,
                name: "first".to_string()
            }]
        }
    );
}

#[test]
fn test_vec_struct_multiple() {
    let json = r#"{"items": [{"id": 1, "name": "first"}, {"id": 2, "name": "second"}]}"#;
    let result: WithVecStruct = jit_deserialize!(WithVecStruct, json);
    assert_eq!(
        result,
        WithVecStruct {
            items: vec![
                Item {
                    id: 1,
                    name: "first".to_string()
                },
                Item {
                    id: 2,
                    name: "second".to_string()
                },
            ]
        }
    );
}

#[test]
fn test_vec_struct_with_surrounding_fields() {
    let json = r#"{"prefix": "hello", "items": [{"id": 1, "name": "first"}], "suffix": 42}"#;
    let result: WithVecStructAndMore = jit_deserialize!(WithVecStructAndMore, json);
    assert_eq!(
        result,
        WithVecStructAndMore {
            prefix: "hello".to_string(),
            items: vec![Item {
                id: 1,
                name: "first".to_string()
            }],
            suffix: 42,
        }
    );
}

// =============================================================================
// Level 4: Deeper nesting - struct with Vec of structs that have strings
// =============================================================================

#[derive(Debug, PartialEq, Facet, Clone)]
struct User {
    id: u64,
    screen_name: String,
    followers_count: u32,
}

#[derive(Debug, PartialEq, Facet, Clone)]
struct Status {
    id: u64,
    text: String,
    user: User,
    retweet_count: u32,
    favorite_count: u32,
}

#[derive(Debug, PartialEq, Facet)]
struct Response {
    statuses: Vec<Status>,
}

#[test]
fn test_twitter_like_single_status() {
    let json = r#"{
        "statuses": [{
            "id": 123,
            "text": "Hello world",
            "user": {
                "id": 456,
                "screen_name": "testuser",
                "followers_count": 100
            },
            "retweet_count": 5,
            "favorite_count": 10
        }]
    }"#;
    let result: Response = jit_deserialize!(Response, json);

    assert_eq!(result.statuses.len(), 1);
    assert_eq!(result.statuses[0].id, 123);
    assert_eq!(result.statuses[0].text, "Hello world");
    assert_eq!(result.statuses[0].user.id, 456);
    assert_eq!(result.statuses[0].user.screen_name, "testuser");
    assert_eq!(result.statuses[0].user.followers_count, 100);
    assert_eq!(result.statuses[0].retweet_count, 5);
    assert_eq!(result.statuses[0].favorite_count, 10);
}

#[test]
fn test_twitter_like_multiple_statuses() {
    let json = r#"{
        "statuses": [
            {
                "id": 1,
                "text": "First tweet",
                "user": {"id": 100, "screen_name": "user1", "followers_count": 50},
                "retweet_count": 1,
                "favorite_count": 2
            },
            {
                "id": 2,
                "text": "Second tweet",
                "user": {"id": 200, "screen_name": "user2", "followers_count": 75},
                "retweet_count": 3,
                "favorite_count": 4
            }
        ]
    }"#;
    let result: Response = jit_deserialize!(Response, json);

    assert_eq!(result.statuses.len(), 2);

    assert_eq!(result.statuses[0].id, 1);
    assert_eq!(result.statuses[0].text, "First tweet");
    assert_eq!(result.statuses[0].user.screen_name, "user1");

    assert_eq!(result.statuses[1].id, 2);
    assert_eq!(result.statuses[1].text, "Second tweet");
    assert_eq!(result.statuses[1].user.screen_name, "user2");
}

// =============================================================================
// Level 5: Stress test with many items
// =============================================================================

#[test]
fn test_many_items_in_vec() {
    // Generate JSON with 100 items
    let mut items_json = String::from("[");
    for i in 0..100 {
        if i > 0 {
            items_json.push(',');
        }
        items_json.push_str(&format!(r#"{{"id": {}, "name": "item{}"}}"#, i, i));
    }
    items_json.push(']');

    let json = format!(r#"{{"items": {}}}"#, items_json);
    let result: WithVecStruct = jit_deserialize!(WithVecStruct, &json);

    assert_eq!(result.items.len(), 100);
    for (i, item) in result.items.iter().enumerate() {
        assert_eq!(item.id, i as u64, "item {} has wrong id", i);
        assert_eq!(item.name, format!("item{}", i), "item {} has wrong name", i);
    }
}

// =============================================================================
// Level 7: Stress test with skipped fields (like Twitter)
// =============================================================================

#[derive(Debug, PartialEq, Facet)]
struct UserWithExtras {
    id: u64,
    screen_name: String,
    followers_count: u32,
}

#[derive(Debug, PartialEq, Facet)]
struct StatusWithExtras {
    id: u64,
    text: String,
    user: UserWithExtras,
    retweet_count: u32,
    favorite_count: u32,
}

#[derive(Debug, PartialEq, Facet)]
struct ResponseWithExtras {
    statuses: Vec<StatusWithExtras>,
}

#[test]
fn test_twitter_like_with_skipped_fields() {
    // Simulate Twitter-like JSON with many extra fields that get skipped
    let json = r#"{
        "statuses": [
            {
                "metadata": {"result_type": "recent"},
                "created_at": "Sun Aug 31 00:29:15 +0000 2014",
                "id": 12345,
                "id_str": "12345",
                "text": "Hello\nWorld",
                "source": "<a href=\"http://example.com\">App</a>",
                "truncated": false,
                "in_reply_to_status_id": null,
                "user": {
                    "id": 100,
                    "id_str": "100",
                    "name": "Test User",
                    "screen_name": "testuser",
                    "location": "NYC",
                    "description": "A test user",
                    "followers_count": 500,
                    "friends_count": 200
                },
                "retweet_count": 10,
                "favorite_count": 20,
                "entities": {"hashtags": [], "urls": []},
                "favorited": false,
                "retweeted": false
            },
            {
                "metadata": {"result_type": "recent"},
                "created_at": "Sun Aug 31 00:30:00 +0000 2014",
                "id": 12346,
                "id_str": "12346",
                "text": "Second tweet\nwith newlines",
                "source": "<a href=\"http://example.com\">App</a>",
                "truncated": false,
                "in_reply_to_status_id": null,
                "user": {
                    "id": 101,
                    "id_str": "101",
                    "name": "Another User",
                    "screen_name": "anotheruser",
                    "location": "LA",
                    "description": "Another test",
                    "followers_count": 1000,
                    "friends_count": 300
                },
                "retweet_count": 5,
                "favorite_count": 15,
                "entities": {"hashtags": [], "urls": []},
                "favorited": true,
                "retweeted": false
            }
        ],
        "search_metadata": {"count": 2}
    }"#;

    let result: ResponseWithExtras = jit_deserialize!(ResponseWithExtras, json);

    assert_eq!(result.statuses.len(), 2);

    assert_eq!(result.statuses[0].id, 12345);
    assert_eq!(result.statuses[0].text, "Hello\nWorld");
    assert_eq!(result.statuses[0].user.screen_name, "testuser");
    assert_eq!(result.statuses[0].user.followers_count, 500);

    assert_eq!(result.statuses[1].id, 12346);
    assert_eq!(result.statuses[1].text, "Second tweet\nwith newlines");
    assert_eq!(result.statuses[1].user.screen_name, "anotheruser");
}

#[test]
fn test_many_statuses_with_extras() {
    // Generate many statuses with extra fields
    let mut statuses_json = String::from("[");
    for i in 0..100 {
        if i > 0 {
            statuses_json.push(',');
        }
        statuses_json.push_str(&format!(
            r#"{{
            "metadata": {{"type": "recent"}},
            "extra_field_1": "skip me",
            "id": {},
            "extra_field_2": {{"nested": "object"}},
            "text": "Tweet number {} with newline\nhere",
            "extra_field_3": [1, 2, 3],
            "user": {{
                "id": {},
                "extra_user_field": "also skipped",
                "screen_name": "user{}",
                "more_extras": null,
                "followers_count": {}
            }},
            "retweet_count": {},
            "favorite_count": {},
            "trailing_extra": true
        }}"#,
            i,
            i,
            i + 1000,
            i,
            (i * 10) % 1000,
            i % 100,
            i % 50
        ));
    }
    statuses_json.push(']');

    let json = format!(
        r#"{{"statuses": {}, "extra_root": "ignored"}}"#,
        statuses_json
    );
    let result: ResponseWithExtras = jit_deserialize!(ResponseWithExtras, &json);

    assert_eq!(result.statuses.len(), 100);
    for (i, status) in result.statuses.iter().enumerate() {
        assert_eq!(status.id, i as u64, "status {} has wrong id", i);
        assert_eq!(
            status.text,
            format!("Tweet number {} with newline\nhere", i),
            "status {} has wrong text",
            i
        );
        assert_eq!(
            status.user.id,
            (i + 1000) as u64,
            "status {} user has wrong id",
            i
        );
        assert_eq!(
            status.user.screen_name,
            format!("user{}", i),
            "status {} user has wrong screen_name",
            i
        );
    }
}

#[test]
fn test_unicode_emoji_content() {
    // Test with actual unicode/emoji content like Twitter data
    let json = r#"{
        "statuses": [
            {
                "id": 1,
                "text": "@aym0566x \n\n名前:前田あゆみ\n第一印象:なんか怖っ！\n今の印象:とりあえずキモい。噛み合わない\n好きなところ:ぶすでキモいとこ😋✨✨",
                "user": {"id": 100, "screen_name": "ayuu0123", "followers_count": 262},
                "retweet_count": 0,
                "favorite_count": 0
            },
            {
                "id": 2,
                "text": "RT @thsc782_407: #LEDカツカツ選手権\n漢字一文字ぶんのスペースに「ハウステンボス」を収める狂気",
                "user": {"id": 200, "screen_name": "nekonekomikan", "followers_count": 100},
                "retweet_count": 58,
                "favorite_count": 0
            },
            {
                "id": 3,
                "text": "思い出:んーーー、ありすぎ😊❤️\nLINE交換できる？:あぁ……ごめん✋\nトプ画をみて:照れますがな😘✨\n一言:お前は一生もんのダチ💖",
                "user": {"id": 300, "screen_name": "testuser", "followers_count": 50},
                "retweet_count": 10,
                "favorite_count": 20
            }
        ]
    }"#;

    let result: ResponseWithExtras = jit_deserialize!(ResponseWithExtras, json);
    assert_eq!(result.statuses.len(), 3);
    assert_eq!(result.statuses[0].user.screen_name, "ayuu0123");
    assert_eq!(result.statuses[1].user.screen_name, "nekonekomikan");
    assert!(result.statuses[0].text.contains("😋"));
}

/// Test using the exact same types as the benchmark (same #[derive])
#[test]
fn test_twitter_benchmark_types() {
    // Use ResponseWithExtras which matches TwitterResponseSparse structure
    let json = r#"{
        "statuses": [
            {
                "metadata": {"result_type": "recent", "iso_language_code": "ja"},
                "created_at": "Sun Aug 31 00:29:15 +0000 2014",
                "id": 505874924095815681,
                "id_str": "505874924095815681",
                "text": "@aym0566x \n\n名前:前田あゆみ\n第一印象:なんか怖っ！\n今の印象:とりあえずキモい。噛み合わない\n好きなところ:ぶすでキモいとこ😋✨✨\n思い出:んーーー、ありすぎ😊❤️\nLINE交換できる？:あぁ……ごめん✋\nトプ画をみて:照れますがな😘✨\n一言:お前は一生もんのダチ💖",
                "source": "<a href=\"http://twitter.com/download/iphone\" rel=\"nofollow\">Twitter for iPhone</a>",
                "truncated": false,
                "in_reply_to_status_id": null,
                "in_reply_to_status_id_str": null,
                "in_reply_to_user_id": 866260188,
                "in_reply_to_user_id_str": "866260188",
                "in_reply_to_screen_name": "aym0566x",
                "user": {
                    "id": 1186275104,
                    "id_str": "1186275104",
                    "name": "AYUMI",
                    "screen_name": "ayuu0123",
                    "location": "",
                    "description": "元野球部マネージャー❤︎…最高の夏をありがとう…❤︎",
                    "url": null,
                    "entities": {"description": {"urls": []}},
                    "protected": false,
                    "followers_count": 262,
                    "friends_count": 252,
                    "listed_count": 0,
                    "created_at": "Sat Feb 16 13:40:25 +0000 2013",
                    "favourites_count": 235,
                    "utc_offset": null,
                    "time_zone": null,
                    "geo_enabled": false,
                    "verified": false,
                    "statuses_count": 1769,
                    "lang": "en",
                    "contributors_enabled": false,
                    "is_translator": false,
                    "is_translation_enabled": false,
                    "profile_background_color": "C0DEED",
                    "profile_background_image_url": "http://abs.twimg.com/images/themes/theme1/bg.png"
                },
                "geo": null,
                "coordinates": null,
                "place": null,
                "contributors": null,
                "retweet_count": 0,
                "favorite_count": 0,
                "entities": {"hashtags": [], "symbols": [], "urls": [], "user_mentions": []},
                "favorited": false,
                "retweeted": false,
                "lang": "ja"
            }
        ],
        "search_metadata": {"count": 1}
    }"#;

    let result: ResponseWithExtras = jit_deserialize!(ResponseWithExtras, json);
    assert_eq!(result.statuses.len(), 1);
    assert_eq!(result.statuses[0].id, 505874924095815681);
    assert_eq!(result.statuses[0].user.screen_name, "ayuu0123");
    assert_eq!(result.statuses[0].user.followers_count, 262);
}

// =============================================================================
// Level 6: Field order matters - test alphabetical vs declaration order
// =============================================================================

/// Fields declared in non-alphabetical order
#[derive(Debug, PartialEq, Facet)]
struct NonAlphabetical {
    zebra: u64,    // z comes last alphabetically but first in declaration
    apple: String, // a comes first alphabetically but second in declaration
    middle: u32,   // m is in the middle
}

#[test]
fn test_non_alphabetical_field_order() {
    // JSON keys in declaration order
    let json = r#"{"zebra": 1, "apple": "test", "middle": 42}"#;
    let result: NonAlphabetical = jit_deserialize!(NonAlphabetical, json);
    assert_eq!(
        result,
        NonAlphabetical {
            zebra: 1,
            apple: "test".to_string(),
            middle: 42
        }
    );
}

#[test]
fn test_non_alphabetical_json_in_alpha_order() {
    // JSON keys in alphabetical order (different from declaration order)
    let json = r#"{"apple": "test", "middle": 42, "zebra": 1}"#;
    let result: NonAlphabetical = jit_deserialize!(NonAlphabetical, json);
    assert_eq!(
        result,
        NonAlphabetical {
            zebra: 1,
            apple: "test".to_string(),
            middle: 42
        }
    );
}

/// Nested struct with non-alphabetical fields inside a Vec
#[derive(Debug, PartialEq, Facet, Clone)]
struct NonAlphaItem {
    zz_last: u64,
    aa_first: String,
}

#[derive(Debug, PartialEq, Facet)]
struct VecOfNonAlpha {
    items: Vec<NonAlphaItem>,
}

#[test]
fn test_vec_of_non_alphabetical_structs() {
    let json =
        r#"{"items": [{"zz_last": 1, "aa_first": "one"}, {"zz_last": 2, "aa_first": "two"}]}"#;
    let result: VecOfNonAlpha = jit_deserialize!(VecOfNonAlpha, json);

    assert_eq!(result.items.len(), 2);
    assert_eq!(result.items[0].zz_last, 1);
    assert_eq!(result.items[0].aa_first, "one");
    assert_eq!(result.items[1].zz_last, 2);
    assert_eq!(result.items[1].aa_first, "two");
}
