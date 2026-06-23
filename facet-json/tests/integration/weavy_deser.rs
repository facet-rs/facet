use facet::Facet;
use facet_format::DeserializeErrorKind;
use std::sync::atomic::{AtomicUsize, Ordering};

static DROPPED_LIST_ELEMENTS: AtomicUsize = AtomicUsize::new(0);

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
struct Droppy {
    value: u8,
}

impl Drop for Droppy {
    fn drop(&mut self) {
        DROPPED_LIST_ELEMENTS.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Facet, Debug, PartialEq)]
struct DroppyList {
    items: Vec<Droppy>,
}

#[derive(Facet, Debug, PartialEq)]
struct DroppyPair {
    item: Droppy,
    tail: u8,
}

#[derive(Facet, Debug, PartialEq)]
struct DroppyPairList {
    items: Vec<DroppyPair>,
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

#[derive(Facet, Debug, PartialEq)]
struct AliasedName {
    #[facet(alias = "old_name")]
    new_name: String,
    count: u8,
}

#[derive(Facet, Debug, PartialEq)]
#[facet(deny_unknown_fields)]
struct StrictPoint {
    x: u8,
}

#[derive(Facet, Debug, PartialEq)]
struct LoosePoint {
    x: u8,
}

#[derive(Facet, Debug, PartialEq)]
struct WideScalarStruct {
    a: u8,
    b: u16,
    c: u32,
    d: u64,
    e: i8,
    f: i16,
    g: i32,
    h: i64,
    i: usize,
    j: isize,
    k: bool,
    l: f64,
}

#[derive(Facet, Debug, PartialEq)]
struct WideDefaultStruct {
    #[facet(default)]
    a: u8,
    #[facet(default)]
    b: u16,
    #[facet(default)]
    c: u32,
    #[facet(default)]
    d: u64,
    #[facet(default)]
    e: i8,
    #[facet(default)]
    f: i16,
    #[facet(default)]
    g: i32,
    #[facet(default)]
    h: i64,
    #[facet(default)]
    i: usize,
    #[facet(default)]
    j: isize,
    #[facet(default)]
    k: bool,
    #[facet(default)]
    l: f64,
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
fn weavy_matches_alias_on_raw_field_key() {
    let got: AliasedName = facet_json::from_str_weavy(r#"{"old_name":"value","count":5}"#).unwrap();

    assert_eq!(
        got,
        AliasedName {
            new_name: "value".to_owned(),
            count: 5,
        }
    );
}

#[test]
fn weavy_reports_unknown_field_after_raw_key_matching() {
    let err = facet_json::from_str_weavy::<StrictPoint>(r#"{"x":1,"extra":2}"#).unwrap_err();
    assert!(matches!(
        err.kind,
        DeserializeErrorKind::UnknownField { ref field, .. } if field == "extra"
    ));
}

#[test]
fn weavy_validates_skipped_unknown_raw_field_key_utf8() {
    let err = facet_json::from_slice_weavy::<LoosePoint>(b"{\"x\":1,\"\xff\":2}").unwrap_err();
    assert!(matches!(err.kind, DeserializeErrorKind::InvalidUtf8 { .. }));
}

#[test]
fn weavy_tiny_scalar_struct_skips_unknown_container_value() {
    let got: LoosePoint =
        facet_json::from_str_weavy(r#"{"x":1,"extra":{"nested":[true,false]}}"#).unwrap();
    assert_eq!(got, LoosePoint { x: 1 });
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
fn weavy_jit_plan_uses_native_for_root_scalar_struct_when_available() {
    let plan = facet_json::JsonWeavyPlan::<Point>::build_jit().unwrap();
    let native_available = cfg!(all(
        feature = "jit",
        target_os = "macos",
        target_arch = "aarch64"
    ));

    assert_eq!(
        plan.execution_mode(),
        facet_json::JsonWeavyExecutionMode::Jit
    );
    assert_eq!(
        plan.active_backend(),
        if native_available {
            facet_json::JsonWeavyActiveBackend::NativeJit
        } else {
            facet_json::JsonWeavyActiveBackend::Interpreter
        }
    );
    assert_eq!(
        facet_json::JsonWeavyPlan::<Point>::native_jit_available(),
        native_available
    );

    let got = plan.from_str(r#"{"x":1,"y":2}"#).unwrap();
    assert_eq!(got, Point { x: 1, y: 2 });

    let report = plan.jit_fallback_report();
    if native_available {
        assert!(report.is_empty(), "{report:?}");
    } else {
        assert!(!report.is_empty(), "{report:?}");
        assert_eq!(report.records[0].path, "$");
        let expected_reason = if !cfg!(feature = "jit") {
            "facet-json was built without its jit feature"
        } else {
            "native JIT is not enabled for this build target"
        };
        assert_eq!(report.records[0].reason, expected_reason);
    }
}

#[test]
fn weavy_jit_plan_reports_fallback_for_unsupported_root_shape() {
    let plan = facet_json::JsonWeavyPlan::<PointList>::build_jit().unwrap();

    assert_eq!(
        plan.active_backend(),
        facet_json::JsonWeavyActiveBackend::Interpreter
    );

    let report = plan.jit_fallback_report();
    assert!(!report.is_empty(), "{report:?}");
    assert_eq!(report.records[0].path, "$");
    let expected_reason = if !cfg!(feature = "jit") {
        "facet-json was built without its jit feature"
    } else if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "native JIT is not enabled for this build target"
    } else {
        "JSON native JIT currently supports root scalar structs or scalar struct lists only"
    };
    assert_eq!(report.records[0].reason, expected_reason);
}

#[test]
fn weavy_jit_plan_reports_fallback_for_defaulted_scalar_struct() {
    let plan = facet_json::JsonWeavyPlan::<WideDefaultStruct>::build_jit().unwrap();

    assert_eq!(
        plan.active_backend(),
        facet_json::JsonWeavyActiveBackend::Interpreter
    );

    let report = plan.jit_fallback_report();
    assert!(!report.is_empty(), "{report:?}");
    assert_eq!(report.records[0].path, "$");
    let expected_reason = if !cfg!(feature = "jit") {
        "facet-json was built without its jit feature"
    } else if !cfg!(all(target_os = "macos", target_arch = "aarch64")) {
        "native JIT is not enabled for this build target"
    } else {
        "JSON native JIT currently supports required scalar struct fields only"
    };
    assert_eq!(report.records[0].reason, expected_reason);
}

#[test]
fn weavy_jit_helpers_deserialize_through_jit_requested_plan_slot() {
    let from_str: Point = facet_json::from_str_weavy_jit(r#"{"x":1,"y":2}"#).unwrap();
    let from_slice: Point = facet_json::from_slice_weavy_jit(br#"{"x":3,"y":4}"#).unwrap();

    assert_eq!(from_str, Point { x: 1, y: 2 });
    assert_eq!(from_slice, Point { x: 3, y: 4 });
}

#[test]
fn weavy_jit_plan_uses_native_for_root_scalar_struct_list_when_available() {
    let plan = facet_json::JsonWeavyPlan::<Vec<Point>>::build_jit().unwrap();
    let report = plan.jit_fallback_report();
    let native_available = cfg!(all(
        feature = "jit",
        target_os = "macos",
        target_arch = "aarch64"
    ));

    assert_eq!(
        plan.active_backend(),
        if native_available {
            facet_json::JsonWeavyActiveBackend::NativeJit
        } else {
            facet_json::JsonWeavyActiveBackend::Interpreter
        },
        "{report:?}"
    );

    let got = plan
        .from_str(r#"[{"x":1,"y":2},{"x":3,"y":4},{"x":5,"y":6}]"#)
        .unwrap();
    assert_eq!(
        got,
        vec![
            Point { x: 1, y: 2 },
            Point { x: 3, y: 4 },
            Point { x: 5, y: 6 },
        ]
    );

    if native_available {
        assert!(report.is_empty(), "{report:?}");
    } else {
        assert!(!report.is_empty(), "{report:?}");
    }
}

#[test]
fn weavy_jit_scalar_struct_handles_ordered_wide_scalars() {
    let plan = facet_json::JsonWeavyPlan::<WideScalarStruct>::build_jit().unwrap();
    let got = plan
        .from_str(
            r#"{"a":1,"b":2,"c":3,"d":4,"e":-5,"f":-6,"g":-7,"h":-8,"i":9,"j":-10,"k":true,"l":11.5}"#,
        )
        .unwrap();

    assert_eq!(
        got,
        WideScalarStruct {
            a: 1,
            b: 2,
            c: 3,
            d: 4,
            e: -5,
            f: -6,
            g: -7,
            h: -8,
            i: 9,
            j: -10,
            k: true,
            l: 11.5,
        }
    );
}

#[test]
fn weavy_jit_scalar_struct_list_handles_ordered_wide_scalars() {
    let plan = facet_json::JsonWeavyPlan::<Vec<WideScalarStruct>>::build_jit().unwrap();
    let got = plan
        .from_str(
            r#"[{"a":1,"b":2,"c":3,"d":4,"e":-5,"f":-6,"g":-7,"h":-8,"i":9,"j":-10,"k":true,"l":11.5},{"a":12,"b":13,"c":14,"d":15,"e":-16,"f":-17,"g":-18,"h":-19,"i":20,"j":-21,"k":false,"l":22.5}]"#,
        )
        .unwrap();

    assert_eq!(
        got,
        vec![
            WideScalarStruct {
                a: 1,
                b: 2,
                c: 3,
                d: 4,
                e: -5,
                f: -6,
                g: -7,
                h: -8,
                i: 9,
                j: -10,
                k: true,
                l: 11.5,
            },
            WideScalarStruct {
                a: 12,
                b: 13,
                c: 14,
                d: 15,
                e: -16,
                f: -17,
                g: -18,
                h: -19,
                i: 20,
                j: -21,
                k: false,
                l: 22.5,
            },
        ]
    );
}

#[test]
fn weavy_jit_scalar_struct_falls_back_for_non_ordered_objects() {
    let plan = facet_json::JsonWeavyPlan::<WideScalarStruct>::build_jit().unwrap();
    let out_of_order = plan
        .from_str(
            r#"{"l":11.5,"k":true,"j":-10,"i":9,"h":-8,"g":-7,"f":-6,"e":-5,"d":4,"c":3,"b":2,"a":1}"#,
        )
        .unwrap();
    let skipped_unknown = plan
        .from_str(
            r#"{"a":1,"unknown_scalar":12345,"b":2,"unknown_array":[1,2,3],"c":3,"unknown_object":{"nested":true},"d":4,"e":-5,"f":-6,"g":-7,"h":-8,"i":9,"j":-10,"k":true,"l":11.5}"#,
        )
        .unwrap();

    let expected = WideScalarStruct {
        a: 1,
        b: 2,
        c: 3,
        d: 4,
        e: -5,
        f: -6,
        g: -7,
        h: -8,
        i: 9,
        j: -10,
        k: true,
        l: 11.5,
    };
    assert_eq!(out_of_order, expected);
    assert_eq!(skipped_unknown, expected);
}

#[test]
fn weavy_jit_scalar_struct_list_falls_back_for_non_ordered_objects() {
    let plan = facet_json::JsonWeavyPlan::<Vec<WideScalarStruct>>::build_jit().unwrap();
    let out_of_order = plan
        .from_str(
            r#"[{"a":1,"b":2,"c":3,"d":4,"e":-5,"f":-6,"g":-7,"h":-8,"i":9,"j":-10,"k":true,"l":11.5},{"l":22.5,"k":false,"j":-21,"i":20,"h":-19,"g":-18,"f":-17,"e":-16,"d":15,"c":14,"b":13,"a":12}]"#,
        )
        .unwrap();
    let skipped_unknown = plan
        .from_str(
            r#"[{"a":1,"b":2,"c":3,"d":4,"e":-5,"f":-6,"g":-7,"h":-8,"i":9,"j":-10,"k":true,"l":11.5},{"a":12,"unknown_scalar":12345,"b":13,"unknown_array":[1,2,3],"c":14,"unknown_object":{"nested":true},"d":15,"e":-16,"f":-17,"g":-18,"h":-19,"i":20,"j":-21,"k":false,"l":22.5}]"#,
        )
        .unwrap();

    let expected = vec![
        WideScalarStruct {
            a: 1,
            b: 2,
            c: 3,
            d: 4,
            e: -5,
            f: -6,
            g: -7,
            h: -8,
            i: 9,
            j: -10,
            k: true,
            l: 11.5,
        },
        WideScalarStruct {
            a: 12,
            b: 13,
            c: 14,
            d: 15,
            e: -16,
            f: -17,
            g: -18,
            h: -19,
            i: 20,
            j: -21,
            k: false,
            l: 22.5,
        },
    ];
    assert_eq!(out_of_order, expected);
    assert_eq!(skipped_unknown, expected);
}

#[test]
fn weavy_jit_scalar_struct_falls_back_for_missing_defaults_and_duplicates() {
    let defaults = facet_json::JsonWeavyPlan::<WideDefaultStruct>::build_jit()
        .unwrap()
        .from_str(r#"{"a":1,"d":4,"l":11.5}"#)
        .unwrap();
    assert_eq!(
        defaults,
        WideDefaultStruct {
            a: 1,
            b: 0,
            c: 0,
            d: 4,
            e: 0,
            f: 0,
            g: 0,
            h: 0,
            i: 0,
            j: 0,
            k: false,
            l: 11.5,
        }
    );

    let err = facet_json::JsonWeavyPlan::<Point>::build_jit()
        .unwrap()
        .from_str(r#"{"x":1,"y":2,"x":3}"#)
        .unwrap_err();
    assert!(matches!(
        err.kind,
        DeserializeErrorKind::DuplicateField { ref field, .. } if field == "x"
    ));

    let err = facet_json::JsonWeavyPlan::<Vec<Point>>::build_jit()
        .unwrap()
        .from_str(r#"[{"x":1,"y":2},{"x":3,"y":4,"x":5}]"#)
        .unwrap_err();
    assert!(matches!(
        err.kind,
        DeserializeErrorKind::DuplicateField { ref field, .. } if field == "x"
    ));
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
fn weavy_deserializes_wide_scalar_struct() {
    let got: WideScalarStruct = facet_json::from_str_weavy(
        r#"{"a":1,"b":2,"c":3,"d":4,"e":-5,"f":-6,"g":-7,"h":-8,"i":9,"j":-10,"k":true,"l":11.5}"#,
    )
    .unwrap();

    assert_eq!(
        got,
        WideScalarStruct {
            a: 1,
            b: 2,
            c: 3,
            d: 4,
            e: -5,
            f: -6,
            g: -7,
            h: -8,
            i: 9,
            j: -10,
            k: true,
            l: 11.5,
        }
    );
}

#[test]
fn weavy_deserializes_wide_scalar_struct_out_of_order() {
    let got: WideScalarStruct = facet_json::from_str_weavy(
        r#"{"l":11.5,"k":true,"j":-10,"i":9,"h":-8,"g":-7,"f":-6,"e":-5,"d":4,"c":3,"b":2,"a":1}"#,
    )
    .unwrap();

    assert_eq!(
        got,
        WideScalarStruct {
            a: 1,
            b: 2,
            c: 3,
            d: 4,
            e: -5,
            f: -6,
            g: -7,
            h: -8,
            i: 9,
            j: -10,
            k: true,
            l: 11.5,
        }
    );
}

#[test]
fn weavy_deserializes_wide_scalar_struct_with_skipped_unknown_fields() {
    let got: WideScalarStruct = facet_json::from_str_weavy(
        r#"{"a":1,"unknown_scalar":12345,"b":2,"unknown_array":[1,2,3],"c":3,"unknown_object":{"nested":true},"d":4,"e":-5,"f":-6,"g":-7,"h":-8,"i":9,"j":-10,"k":true,"l":11.5}"#,
    )
    .unwrap();

    assert_eq!(
        got,
        WideScalarStruct {
            a: 1,
            b: 2,
            c: 3,
            d: 4,
            e: -5,
            f: -6,
            g: -7,
            h: -8,
            i: 9,
            j: -10,
            k: true,
            l: 11.5,
        }
    );
}

#[test]
fn weavy_defaults_missing_wide_scalar_fields() {
    let got: WideDefaultStruct = facet_json::from_str_weavy(r#"{"a":1,"d":4,"l":11.5}"#).unwrap();

    assert_eq!(
        got,
        WideDefaultStruct {
            a: 1,
            b: 0,
            c: 0,
            d: 4,
            e: 0,
            f: 0,
            g: 0,
            h: 0,
            i: 0,
            j: 0,
            k: false,
            l: 11.5,
        }
    );
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
fn weavy_deserializes_numeric_strings_on_raw_scalar_path() {
    let person: Person = facet_json::from_str_weavy(
        r#"{"name":"Ada","age":"37","favorite":null,"scores":["1","2","3"]}"#,
    )
    .unwrap();

    assert_eq!(person.age, 37);
    assert_eq!(person.scores, vec![1, 2, 3]);
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
fn weavy_drops_direct_list_elements_after_later_element_error() {
    DROPPED_LIST_ELEMENTS.store(0, Ordering::SeqCst);

    facet_json::from_str_weavy::<DroppyList>(r#"{"items":[{"value":1},{"value":"nope"}]}"#)
        .unwrap_err();
    assert_eq!(DROPPED_LIST_ELEMENTS.load(Ordering::SeqCst), 1);
}

#[test]
fn weavy_drops_partial_direct_list_struct_element_before_list_buffer() {
    DROPPED_LIST_ELEMENTS.store(0, Ordering::SeqCst);

    facet_json::from_str_weavy::<DroppyPairList>(
        r#"{"items":[{"item":{"value":1},"tail":2},{"item":{"value":3},"tail":"nope"}]}"#,
    )
    .unwrap_err();

    assert_eq!(DROPPED_LIST_ELEMENTS.load(Ordering::SeqCst), 2);
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

    let (_, short_list): (Person, _) = facet_json::from_str_weavy_with_stats(
        r#"{"name":"Ada","age":37,"favorite":null,"scores":[1]}"#,
    )
    .unwrap();
    let (_, long_list): (Person, _) = facet_json::from_str_weavy_with_stats(
        r#"{"name":"Ada","age":37,"favorite":null,"scores":[1,2,3,4,5]}"#,
    )
    .unwrap();
    assert_eq!(
        short_list.block_call_count, long_list.block_call_count,
        "{short_list:?} {long_list:?}"
    );
    assert_eq!(
        short_list.step_count, long_list.step_count,
        "{short_list:?} {long_list:?}"
    );

    let (_, short_option_list): (MaybeScores, _) =
        facet_json::from_str_weavy_with_stats(r#"{"scores":[1]}"#).unwrap();
    let (_, long_option_list): (MaybeScores, _) =
        facet_json::from_str_weavy_with_stats(r#"{"scores":[1,null,2,null,3]}"#).unwrap();
    assert_eq!(
        short_option_list.block_call_count, long_option_list.block_call_count,
        "{short_option_list:?} {long_option_list:?}"
    );
    assert_eq!(
        short_option_list.step_count, long_option_list.step_count,
        "{short_option_list:?} {long_option_list:?}"
    );
}
