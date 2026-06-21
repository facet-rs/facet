use facet::Facet;
use facet_format::DeserializeErrorKind;

#[derive(Facet, Debug, PartialEq)]
struct Point {
    x: i32,
    y: i32,
}

#[derive(Facet, Debug, PartialEq)]
struct Person {
    name: String,
    age: u32,
    favorite: Option<String>,
    scores: Vec<u16>,
}

#[derive(Facet, Debug, PartialEq)]
struct MaybeScores {
    scores: Vec<Option<u16>>,
}

#[derive(Facet, Debug, PartialEq)]
struct PointList {
    points: Vec<Point>,
}

#[derive(Facet, Debug, PartialEq)]
struct Node {
    id: u32,
    child: Option<Box<Node>>,
}

#[derive(Facet, Debug, PartialEq)]
struct EscapedFieldName {
    quoted_key: u8,
}

#[test]
fn weavy_deserializes_named_struct_scalars() {
    let point: Point = facet_json::from_str_weavy(r#"{"y":20,"x":10}"#).unwrap();
    assert_eq!(point, Point { x: 10, y: 20 });
}

#[test]
fn weavy_deserializes_escaped_field_names() {
    let expected = EscapedFieldName { quoted_key: 7 };
    let json = r#"{"quoted\u005fkey":7}"#;

    let from_str: EscapedFieldName = facet_json::from_str_weavy(json).unwrap();
    let from_slice: EscapedFieldName = facet_json::from_slice_weavy(json.as_bytes()).unwrap();

    assert_eq!(from_str, expected);
    assert_eq!(from_slice, expected);
}

#[test]
fn weavy_plan_can_be_reused() {
    let plan = facet_json::JsonWeavyPlan::<Point>::build().unwrap();
    let first = plan.from_str(r#"{"x":1,"y":2}"#).unwrap();
    let second = plan.from_str(r#"{"x":3,"y":4}"#).unwrap();
    assert_eq!(first, Point { x: 1, y: 2 });
    assert_eq!(second, Point { x: 3, y: 4 });
}

#[test]
fn weavy_rejects_duplicate_field_after_ordered_match() {
    let err = facet_json::from_str_weavy::<Point>(r#"{"x":1,"y":2,"x":3}"#).unwrap_err();
    assert!(matches!(
        err.kind,
        DeserializeErrorKind::DuplicateField { ref field, .. } if field == "x"
    ));
}

#[test]
fn weavy_deserializes_options_and_lists() {
    let person: Person =
        facet_json::from_str_weavy(r#"{"name":"Ada","age":37,"favorite":null,"scores":[1,2,3]}"#)
            .unwrap();
    assert_eq!(
        person,
        Person {
            name: "Ada".to_owned(),
            age: 37,
            favorite: None,
            scores: vec![1, 2, 3],
        }
    );
}

#[test]
fn weavy_deserializes_null_options_inside_lists() {
    let got: MaybeScores = facet_json::from_str_weavy(r#"{"scores":[1,null,2,null]}"#).unwrap();
    assert_eq!(got.scores, vec![Some(1), None, Some(2), None]);
}

#[test]
fn weavy_deserializes_structs_inside_lists() {
    let got: PointList =
        facet_json::from_str_weavy(r#"{"points":[{"x":1,"y":2},{"x":3,"y":4}]}"#).unwrap();
    assert_eq!(got.points, vec![Point { x: 1, y: 2 }, Point { x: 3, y: 4 }]);
}

#[test]
fn weavy_deserializes_top_level_null_option() {
    let got: Option<u16> = facet_json::from_str_weavy("null").unwrap();
    assert_eq!(got, None);
}

#[test]
fn weavy_defaults_absent_option_and_vec_fields() {
    let person: Person = facet_json::from_str_weavy(r#"{"name":"Ada","age":37}"#).unwrap();
    assert_eq!(
        person,
        Person {
            name: "Ada".to_owned(),
            age: 37,
            favorite: None,
            scores: Vec::new(),
        }
    );
}

#[test]
fn weavy_deserializes_recursive_pointer_shape() {
    let node: Node =
        facet_json::from_str_weavy(r#"{"id":1,"child":{"id":2,"child":null}}"#).unwrap();
    assert_eq!(node.id, 1);
    let child = node.child.as_deref().unwrap();
    assert_eq!(child.id, 2);
    assert!(child.child.is_none());
}

#[test]
fn weavy_stats_report_block_calls_for_recursive_shape() {
    let (_, stats): (Node, _) =
        facet_json::from_str_weavy_with_stats(r#"{"id":1,"child":{"id":2,"child":null}}"#).unwrap();
    assert!(stats.block_call_count >= 3, "{stats:?}");
    assert!(stats.max_frame_depth >= 2, "{stats:?}");
}

#[test]
fn weavy_stats_keep_scalar_fields_and_lists_in_loop() {
    let (_, point_stats): (Point, _) =
        facet_json::from_str_weavy_with_stats(r#"{"x":10,"y":20}"#).unwrap();
    assert_eq!(point_stats.inline_call_count, 0, "{point_stats:?}");

    let (_, stats): (Person, _) = facet_json::from_str_weavy_with_stats(
        r#"{"name":"Ada","age":37,"favorite":null,"scores":[1,2,3]}"#,
    )
    .unwrap();
    assert_eq!(stats.inline_call_count, 0, "{stats:?}");
}
