//! Tests for the JIT-compiled JSON deserializer.

use super::from_str;
use facet::Facet;

#[derive(Facet, Debug, PartialEq)]
struct Point {
    x: f64,
    y: f64,
}

#[derive(Facet, Debug, PartialEq)]
struct Person {
    age: u64,
    score: i64,
    active: bool,
}

#[derive(Facet, Debug, PartialEq)]
struct Mixed {
    a: f64,
    b: i64,
    c: u64,
    d: bool,
    e: f64,
}

#[test]
fn test_empty_input() {
    let result: Result<Point, _> = from_str("");
    assert!(result.is_err());
}

#[test]
fn test_garbage_input() {
    let result: Result<Point, _> = from_str("abc");
    assert!(result.is_err());
}

#[test]
fn test_point_basic() {
    let json = r#"{"x": 1.0, "y": 2.0}"#;
    let point: Point = from_str(json).unwrap();
    assert_eq!(point, Point { x: 1.0, y: 2.0 });
}

#[test]
fn test_point_integers_as_floats() {
    let json = r#"{"x": 42, "y": -17}"#;
    let point: Point = from_str(json).unwrap();
    assert_eq!(point, Point { x: 42.0, y: -17.0 });
}

#[test]
fn test_point_scientific_notation() {
    let json = r#"{"x": 1.5e10, "y": -2.5e-3}"#;
    let point: Point = from_str(json).unwrap();
    assert_eq!(
        point,
        Point {
            x: 1.5e10,
            y: -2.5e-3
        }
    );
}

#[test]
fn test_point_with_whitespace() {
    let json = r#"  {  "x"  :  1.0  ,  "y"  :  2.0  }  "#;
    let point: Point = from_str(json).unwrap();
    assert_eq!(point, Point { x: 1.0, y: 2.0 });
}

#[test]
fn test_point_reversed_fields() {
    let json = r#"{"y": 2.0, "x": 1.0}"#;
    let point: Point = from_str(json).unwrap();
    assert_eq!(point, Point { x: 1.0, y: 2.0 });
}

#[test]
fn test_point_unknown_fields_ignored() {
    let json = r#"{"x": 1.0, "z": 999, "y": 2.0, "extra": "ignored"}"#;
    let point: Point = from_str(json).unwrap();
    assert_eq!(point, Point { x: 1.0, y: 2.0 });
}

#[test]
fn test_person_integers_and_bool() {
    let json = r#"{"age": 30, "score": -100, "active": true}"#;
    let person: Person = from_str(json).unwrap();
    assert_eq!(
        person,
        Person {
            age: 30,
            score: -100,
            active: true
        }
    );
}

#[test]
fn test_person_bool_false() {
    let json = r#"{"age": 0, "score": 0, "active": false}"#;
    let person: Person = from_str(json).unwrap();
    assert_eq!(
        person,
        Person {
            age: 0,
            score: 0,
            active: false
        }
    );
}

#[test]
fn test_mixed_types() {
    let json = r#"{"a": 1.5, "b": -42, "c": 100, "d": true, "e": 3.125}"#;
    let mixed: Mixed = from_str(json).unwrap();
    assert_eq!(
        mixed,
        Mixed {
            a: 1.5,
            b: -42,
            c: 100,
            d: true,
            e: 3.125
        }
    );
}

#[test]
fn test_empty_object_with_defaults() {
    #[derive(Facet, Debug)]
    struct Empty {}

    let json = r#"{}"#;
    let _empty: Empty = from_str(json).unwrap();
}

#[test]
fn test_matches_facet_json() {
    // Verify our results match facet-json's interpreter
    let json = r#"{"x": 123.456, "y": -789.012}"#;

    let jit_result: Point = from_str(json).unwrap();
    let interp_result: Point = crate::from_str(json).unwrap();

    assert_eq!(jit_result, interp_result);
}

// Vec<f64> tests
#[derive(Facet, Debug, PartialEq)]
struct Coordinates {
    coords: Vec<f64>,
}

#[test]
fn test_vec_f64_basic() {
    let json = r#"{"coords": [1.0, 2.0, 3.0]}"#;
    let result: Coordinates = from_str(json).unwrap();
    assert_eq!(result.coords, vec![1.0, 2.0, 3.0]);
}

#[test]
fn test_vec_f64_empty() {
    let json = r#"{"coords": []}"#;
    let result: Coordinates = from_str(json).unwrap();
    assert_eq!(result.coords, Vec::<f64>::new());
}

#[test]
fn test_vec_f64_single() {
    let json = r#"{"coords": [42.5]}"#;
    let result: Coordinates = from_str(json).unwrap();
    assert_eq!(result.coords, vec![42.5]);
}

#[test]
fn test_vec_f64_with_whitespace() {
    let json = r#"{"coords": [ 1.0 , 2.0 , 3.0 ]}"#;
    let result: Coordinates = from_str(json).unwrap();
    assert_eq!(result.coords, vec![1.0, 2.0, 3.0]);
}

#[test]
fn test_vec_f64_scientific() {
    let json = r#"{"coords": [1e10, -2.5e-3, 0.0]}"#;
    let result: Coordinates = from_str(json).unwrap();
    assert_eq!(result.coords, vec![1e10, -2.5e-3, 0.0]);
}

#[test]
fn test_vec_f64_matches_facet_json() {
    let json = r#"{"coords": [1.0, 2.0, 3.0, 4.0, 5.0]}"#;
    let jit_result: Coordinates = from_str(json).unwrap();
    let interp_result: Coordinates = crate::from_str(json).unwrap();
    assert_eq!(jit_result, interp_result);
}

// Nested Vec tests
#[derive(Facet, Debug, PartialEq)]
struct Matrix {
    data: Vec<Vec<f64>>,
}

#[derive(Facet, Debug, PartialEq)]
struct Tensor {
    data: Vec<Vec<Vec<f64>>>,
}

#[test]
fn test_vec_vec_f64() {
    let json = r#"{"data": [[1.0, 2.0], [3.0, 4.0, 5.0]]}"#;
    let result: Matrix = from_str(json).unwrap();
    assert_eq!(result.data, vec![vec![1.0, 2.0], vec![3.0, 4.0, 5.0]]);
}

#[test]
fn test_vec_vec_f64_empty() {
    let json = r#"{"data": []}"#;
    let result: Matrix = from_str(json).unwrap();
    assert_eq!(result.data, Vec::<Vec<f64>>::new());
}

#[test]
fn test_vec_vec_f64_matches_facet_json() {
    let json = r#"{"data": [[1.0, 2.0], [3.0], [4.0, 5.0, 6.0]]}"#;
    let jit_result: Matrix = from_str(json).unwrap();
    let interp_result: Matrix = crate::from_str(json).unwrap();
    assert_eq!(jit_result, interp_result);
}

#[test]
fn test_vec_vec_vec_f64() {
    let json = r#"{"data": [[[1.0, 2.0], [3.0]], [[4.0]]]}"#;
    let result: Tensor = from_str(json).unwrap();
    assert_eq!(
        result.data,
        vec![vec![vec![1.0, 2.0], vec![3.0]], vec![vec![4.0]]]
    );
}

#[test]
fn test_vec_vec_vec_f64_matches_facet_json() {
    let json = r#"{"data": [[[1.0, 2.0]], [[3.0, 4.0], [5.0]]]}"#;
    let jit_result: Tensor = from_str(json).unwrap();
    let interp_result: Tensor = crate::from_str(json).unwrap();
    assert_eq!(jit_result, interp_result);
}

// String tests
#[derive(Facet, Debug, PartialEq)]
struct Named {
    name: String,
}

#[test]
fn test_string_basic() {
    let json = r#"{"name": "hello"}"#;
    let result: Named = from_str(json).unwrap();
    assert_eq!(result.name, "hello");
}

#[test]
fn test_string_with_spaces() {
    let json = r#"{"name": "hello world"}"#;
    let result: Named = from_str(json).unwrap();
    assert_eq!(result.name, "hello world");
}

#[test]
fn test_string_empty() {
    let json = r#"{"name": ""}"#;
    let result: Named = from_str(json).unwrap();
    assert_eq!(result.name, "");
}

#[test]
fn test_string_with_newline_escape() {
    let json = r#"{"name": "hello\nworld"}"#;
    let result: Named = from_str(json).unwrap();
    assert_eq!(result.name, "hello\nworld");
}

#[test]
fn test_string_with_tab_escape() {
    let json = r#"{"name": "hello\tworld"}"#;
    let result: Named = from_str(json).unwrap();
    assert_eq!(result.name, "hello\tworld");
}

#[test]
fn test_string_with_quote_escape() {
    let json = r#"{"name": "say \"hello\""}"#;
    let result: Named = from_str(json).unwrap();
    assert_eq!(result.name, "say \"hello\"");
}

#[test]
fn test_string_with_unicode_escape() {
    let json = r#"{"name": "letter \u0041"}"#;
    let result: Named = from_str(json).unwrap();
    assert_eq!(result.name, "letter A");
}

#[test]
fn test_string_with_multiple_escapes() {
    let json = r#"{"name": "Hello\nWorld\t\"quoted\"\u0041"}"#;
    let result: Named = from_str(json).unwrap();
    assert_eq!(result.name, "Hello\nWorld\t\"quoted\"A");
}

#[test]
fn test_string_matches_facet_json() {
    let json = r#"{"name": "test string"}"#;
    let jit_result: Named = from_str(json).unwrap();
    let interp_result: Named = crate::from_str(json).unwrap();
    assert_eq!(jit_result, interp_result);
}

// Nested struct tests
#[derive(Facet, Debug, PartialEq)]
struct Inner {
    value: f64,
    flag: bool,
}

#[derive(Facet, Debug, PartialEq)]
struct Outer {
    name: String,
    inner: Inner,
}

#[test]
fn test_nested_struct_basic() {
    let json = r#"{"name": "test", "inner": {"value": 42.5, "flag": true}}"#;
    let result: Outer = from_str(json).unwrap();
    assert_eq!(result.name, "test");
    assert_eq!(result.inner.value, 42.5);
    assert!(result.inner.flag);
}

#[test]
fn test_nested_struct_matches_facet_json() {
    let json = r#"{"name": "hello", "inner": {"value": 3.14, "flag": false}}"#;
    let jit_result: Outer = from_str(json).unwrap();
    let interp_result: Outer = crate::from_str(json).unwrap();
    assert_eq!(jit_result, interp_result);
}

// Deeply nested
#[derive(Facet, Debug, PartialEq)]
struct Level3 {
    x: f64,
}

#[derive(Facet, Debug, PartialEq)]
struct Level2 {
    level3: Level3,
}

#[derive(Facet, Debug, PartialEq)]
struct Level1 {
    level2: Level2,
}

#[test]
fn test_deeply_nested_struct() {
    let json = r#"{"level2": {"level3": {"x": 99.9}}}"#;
    let result: Level1 = from_str(json).unwrap();
    assert_eq!(result.level2.level3.x, 99.9);
}

// Vec<Struct> tests
#[derive(Facet, Debug, PartialEq)]
struct Item {
    id: u64,
    value: f64,
}

#[derive(Facet, Debug, PartialEq)]
struct Container {
    items: Vec<Item>,
}

#[test]
fn test_vec_struct_basic() {
    let json = r#"{"items": [{"id": 1, "value": 1.5}, {"id": 2, "value": 2.5}]}"#;
    let result: Container = from_str(json).unwrap();
    assert_eq!(result.items.len(), 2);
    assert_eq!(result.items[0], Item { id: 1, value: 1.5 });
    assert_eq!(result.items[1], Item { id: 2, value: 2.5 });
}

#[test]
fn test_vec_struct_empty() {
    let json = r#"{"items": []}"#;
    let result: Container = from_str(json).unwrap();
    assert_eq!(result.items.len(), 0);
}

#[test]
fn test_vec_struct_single() {
    let json = r#"{"items": [{"id": 42, "value": 3.125}]}"#;
    let result: Container = from_str(json).unwrap();
    assert_eq!(result.items.len(), 1);
    assert_eq!(
        result.items[0],
        Item {
            id: 42,
            value: 3.125
        }
    );
}

#[test]
fn test_vec_struct_matches_facet_json() {
    let json =
        r#"{"items": [{"id": 1, "value": 1.0}, {"id": 2, "value": 2.0}, {"id": 3, "value": 3.0}]}"#;
    let jit_result: Container = from_str(json).unwrap();
    let interp_result: Container = crate::from_str(json).unwrap();
    assert_eq!(jit_result, interp_result);
}

// Complex nested Vec<Struct> with multiple levels
#[derive(Facet, Debug, PartialEq)]
struct NestedItem {
    name: String,
    coords: Vec<f64>,
}

#[derive(Facet, Debug, PartialEq)]
struct NestedContainer {
    items: Vec<NestedItem>,
}

#[test]
fn test_vec_struct_with_nested_fields() {
    let json =
        r#"{"items": [{"name": "a", "coords": [1.0, 2.0]}, {"name": "b", "coords": [3.0]}]}"#;
    let result: NestedContainer = from_str(json).unwrap();
    assert_eq!(result.items.len(), 2);
    assert_eq!(result.items[0].name, "a");
    assert_eq!(result.items[0].coords, vec![1.0, 2.0]);
    assert_eq!(result.items[1].name, "b");
    assert_eq!(result.items[1].coords, vec![3.0]);
}

#[test]
fn test_vec_struct_nested_matches_facet_json() {
    let json = r#"{"items": [{"name": "test", "coords": [1.0, 2.0, 3.0]}]}"#;
    let jit_result: NestedContainer = from_str(json).unwrap();
    let interp_result: NestedContainer = crate::from_str(json).unwrap();
    assert_eq!(jit_result, interp_result);
}

#[test]
fn test_vec_struct_with_escaped_strings() {
    let json = r#"{"items": [{"name": "hello\nworld", "coords": [1.0]}, {"name": "tab\there", "coords": [2.0]}]}"#;
    let result: NestedContainer = from_str(json).unwrap();
    assert_eq!(result.items.len(), 2);
    assert_eq!(result.items[0].name, "hello\nworld");
    assert_eq!(result.items[1].name, "tab\there");
}

// Twitter-like structure for testing
#[derive(Facet, Debug, PartialEq)]
struct TweetUser {
    id: u64,
    screen_name: String,
}

#[derive(Facet, Debug, PartialEq)]
struct Tweet {
    id: u64,
    text: String,
    user: TweetUser,
}

#[derive(Facet, Debug, PartialEq)]
struct TweetContainer {
    statuses: Vec<Tweet>,
}

#[test]
fn test_twitter_like_structure() {
    let json = r#"{"statuses": [{"id": 123, "text": "Hello\nWorld", "user": {"id": 456, "screen_name": "test"}}]}"#;
    let result: TweetContainer = from_str(json).unwrap();
    assert_eq!(result.statuses.len(), 1);
    assert_eq!(result.statuses[0].id, 123);
    assert_eq!(result.statuses[0].text, "Hello\nWorld");
    assert_eq!(result.statuses[0].user.screen_name, "test");
}

#[test]
fn test_twitter_like_multiple() {
    let json = r#"{"statuses": [
        {"id": 1, "text": "First\ntweet", "user": {"id": 10, "screen_name": "user1"}},
        {"id": 2, "text": "Second\ttweet", "user": {"id": 20, "screen_name": "user2"}},
        {"id": 3, "text": "Third \"quoted\"", "user": {"id": 30, "screen_name": "user3"}}
    ]}"#;
    let result: TweetContainer = from_str(json).unwrap();
    assert_eq!(result.statuses.len(), 3);
    assert_eq!(result.statuses[0].text, "First\ntweet");
    assert_eq!(result.statuses[1].text, "Second\ttweet");
    assert_eq!(result.statuses[2].text, "Third \"quoted\"");
}

#[test]
fn test_twitter_like_with_extra_fields() {
    // Twitter JSON has many extra fields we need to skip
    let json = r#"{"statuses": [
        {
            "metadata": {"result_type": "recent"},
            "created_at": "Sun Aug 31",
            "id": 123,
            "id_str": "123",
            "text": "Hello\nWorld with \"quotes\"",
            "source": "<a href=\"test\">Test</a>",
            "truncated": false,
            "in_reply_to_status_id": null,
            "user": {
                "id": 456,
                "id_str": "456",
                "name": "Test User",
                "screen_name": "testuser",
                "location": "Here",
                "description": "A test\nuser",
                "url": null,
                "followers_count": 100
            },
            "retweet_count": 5,
            "favorite_count": 10,
            "entities": {"hashtags": [], "urls": []},
            "favorited": false,
            "retweeted": false
        }
    ]}"#;
    let result: TweetContainer = from_str(json).unwrap();
    assert_eq!(result.statuses.len(), 1);
    assert_eq!(result.statuses[0].id, 123);
    assert_eq!(result.statuses[0].text, "Hello\nWorld with \"quotes\"");
    assert_eq!(result.statuses[0].user.id, 456);
    assert_eq!(result.statuses[0].user.screen_name, "testuser");
}

// =============================================================================
// HashMap fallback tests
// =============================================================================

#[derive(Facet, Debug, PartialEq)]
struct WithHashMap {
    data: std::collections::HashMap<String, i64>,
}

#[test]
fn test_hashmap_fallback() {
    // Types containing HashMap should fall back to the interpreter
    let json = r#"{"data": {"foo": 42, "bar": 100}}"#;

    // This should use the interpreter fallback, not the JIT
    let result: WithHashMap = super::from_str_with_fallback(json).unwrap();

    assert_eq!(result.data.get("foo"), Some(&42));
    assert_eq!(result.data.get("bar"), Some(&100));
    assert_eq!(result.data.len(), 2);
}

#[derive(Facet, Debug, PartialEq)]
struct NestedWithHashMap {
    id: u64,
    inner: WithHashMap,
}

#[test]
fn test_nested_hashmap_fallback() {
    // Nested structs containing HashMap should also fall back
    let json = r#"{"id": 123, "inner": {"data": {"x": 1, "y": 2, "z": 3}}}"#;

    let result: NestedWithHashMap = super::from_str_with_fallback(json).unwrap();

    assert_eq!(result.id, 123);
    assert_eq!(result.inner.data.get("x"), Some(&1));
    assert_eq!(result.inner.data.get("y"), Some(&2));
    assert_eq!(result.inner.data.get("z"), Some(&3));
}
