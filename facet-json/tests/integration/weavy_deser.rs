use facet::Facet;
use facet_format::DeserializeErrorKind;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt::Debug;
use std::sync::atomic::{AtomicUsize, Ordering};

static DROPPED_LIST_ELEMENTS: AtomicUsize = AtomicUsize::new(0);
static DROPPED_SET_ELEMENTS: AtomicUsize = AtomicUsize::new(0);
static DROPPED_BULK_MAP_VALUES: AtomicUsize = AtomicUsize::new(0);
static DROPPED_DUPLICATE_MAP_VALUES: AtomicUsize = AtomicUsize::new(0);

fn assert_default_weavy_parity<T>(json: &str)
where
    T: Facet<'static> + Debug + PartialEq,
{
    let default = facet_json::from_str::<T>(json);
    let weavy = facet_json::from_str_weavy::<T>(json);

    match (default, weavy) {
        (Ok(default), Ok(weavy)) => assert_eq!(weavy, default),
        (Err(_), Err(_)) => {}
        (Ok(default), Err(err)) => {
            panic!("Weavy rejected input accepted by default: {err:?}; expected {default:?}")
        }
        (Err(err), Ok(weavy)) => {
            panic!("Weavy accepted input rejected by default: {weavy:?}; default error {err:?}")
        }
    }
}

#[derive(Facet, Debug, PartialEq)]
struct Point {
    x: i32,
    y: i32,
}

#[derive(Facet, Debug, PartialEq)]
struct FloatPoint {
    x: f64,
    y: f64,
    z: f64,
}

#[derive(Facet, Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct OrderedPoint {
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
struct MapHolder {
    names: HashMap<String, String>,
    buckets: HashMap<String, Vec<u64>>,
    points: HashMap<String, Point>,
}

#[derive(Facet, Debug, PartialEq)]
struct IntegerMapKeys {
    i8s: BTreeMap<i8, String>,
    i32s: BTreeMap<i32, String>,
    u16s: BTreeMap<u16, String>,
    u128s: BTreeMap<u128, String>,
}

#[derive(Facet, Debug, PartialEq)]
struct SetHolder {
    names: BTreeMap<String, String>,
    ordered: BTreeSet<String>,
    hashed: HashSet<u16>,
    maybe: BTreeSet<Option<u16>>,
    points: BTreeSet<OrderedPoint>,
}

#[derive(Facet, Debug, PartialEq)]
struct DroppyMap {
    items: HashMap<String, BulkMapDroppy>,
}

#[derive(Facet, Debug, Eq, Hash, PartialEq)]
struct SetDroppy {
    value: u8,
}

impl Drop for SetDroppy {
    fn drop(&mut self) {
        DROPPED_SET_ELEMENTS.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Facet, Debug, PartialEq)]
struct DroppySet {
    items: HashSet<SetDroppy>,
}

#[derive(Facet, Debug, PartialEq)]
struct BulkMapDroppy {
    value: u8,
}

impl Drop for BulkMapDroppy {
    fn drop(&mut self) {
        DROPPED_BULK_MAP_VALUES.fetch_add(1, Ordering::SeqCst);
    }
}

#[derive(Facet, Debug, PartialEq)]
struct DuplicateMapDroppy {
    value: u8,
}

impl Drop for DuplicateMapDroppy {
    fn drop(&mut self) {
        DROPPED_DUPLICATE_MAP_VALUES.fetch_add(1, Ordering::SeqCst);
    }
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
#[repr(u8)]
enum ExternalUnit {
    Active,
    Inactive,
}

#[derive(Facet, Debug, PartialEq)]
#[repr(u8)]
enum ExternalRenamed {
    #[facet(rename = "enabled")]
    Active,
    #[facet(rename = "disabled")]
    Inactive,
}

#[derive(Facet, Debug, PartialEq)]
#[repr(u8)]
enum ExternalNewtype {
    Empty,
    Some(i32),
    Point(Point),
}

#[derive(Facet, Debug, PartialEq)]
#[repr(u8)]
enum ExternalTuple {
    Empty,
    Pair(String, i32),
    Triple(bool, f64, String),
}

#[derive(Facet, Debug, PartialEq)]
#[repr(u8)]
enum ExternalStruct {
    Empty,
    Record {
        name: String,
        age: u8,
        favorite: Option<String>,
    },
}

#[derive(Facet, Debug, PartialEq)]
#[repr(u8)]
enum ExternalDroppy {
    Item { item: Droppy, tail: u8 },
}

#[derive(Facet, Debug, PartialEq)]
#[repr(u8)]
enum ExternalTupleDroppy {
    Pair(Droppy, u8),
}

#[derive(Facet, Debug, PartialEq)]
#[facet(rename_all = "kebab-case")]
#[repr(u8)]
enum ExternalOther {
    Null,
    Gt(Vec<String>),
    #[facet(other)]
    EqBare(Option<String>),
}

#[derive(Facet, Debug, PartialEq)]
#[facet(tag = "type")]
#[repr(u8)]
enum InternalShape {
    Circle { radius: f64 },
    Rectangle { width: f64, height: f64 },
}

#[derive(Facet, Debug, PartialEq)]
#[facet(tag = "status")]
#[repr(u8)]
enum InternalUnit {
    Active,
    Inactive,
}

#[derive(Facet, Debug, PartialEq)]
#[facet(tag = "kind", rename_all = "snake_case")]
#[repr(u8)]
enum InternalRenamed {
    UserCreated { user_id: u64 },
    UserDeleted { user_id: u64 },
}

#[derive(Facet, Debug, PartialEq)]
#[facet(tag = "kind", content = "data")]
#[repr(u8)]
enum AdjacentValue {
    Start,
    Str(String),
    Pair(i32, i32),
    Block { text: String, level: u8 },
}

#[derive(Facet, Debug, PartialEq)]
#[facet(tag = "kind", content = "data", rename_all = "snake_case")]
#[repr(u8)]
enum AdjacentRenamed {
    CreateUser { name: String },
    DeleteUser { id: u64 },
}

#[derive(Facet, Debug, PartialEq)]
#[facet(tag = "kind")]
#[repr(u8)]
enum InternalDroppy {
    Item { item: Droppy, tail: u8 },
}

#[derive(Facet, Debug, PartialEq)]
#[facet(tag = "kind", content = "data")]
#[repr(u8)]
enum AdjacentDroppy {
    Item { item: Droppy, tail: u8 },
}

#[derive(Facet, Debug, PartialEq)]
#[facet(tag = "kind", content = "data")]
#[repr(u8)]
enum AdjacentTupleDroppy {
    Pair(Droppy, u8),
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
fn weavy_deserializes_hash_maps() {
    let got: MapHolder = facet_json::from_str_weavy(
        r#"{
            "names":{"100":"main","200":"balcony","escaped\u005fkey":"decoded"},
            "buckets":{"topic":[10,20,30],"empty":[]},
            "points":{"origin":{"x":0,"y":0},"target":{"y":7,"x":5}}
        }"#,
    )
    .unwrap();

    assert_eq!(got.names["100"], "main");
    assert_eq!(got.names["200"], "balcony");
    assert_eq!(got.names["escaped_key"], "decoded");
    assert_eq!(got.buckets["topic"], vec![10, 20, 30]);
    assert_eq!(got.buckets["empty"], Vec::<u64>::new());
    assert_eq!(got.points["origin"], Point { x: 0, y: 0 });
    assert_eq!(got.points["target"], Point { x: 5, y: 7 });
}

#[test]
fn weavy_hash_map_duplicate_keys_keep_latest_value() {
    let got: HashMap<String, String> =
        facet_json::from_str_weavy(r#"{"same":"first","same":"second"}"#).unwrap();

    assert_eq!(got.len(), 1);
    assert_eq!(got["same"], "second");
}

#[test]
fn weavy_deserializes_integer_map_keys_at_every_width() {
    assert_default_weavy_parity::<IntegerMapKeys>(
        r#"{
            "i8s": {"-3": "a"},
            "i32s": {"100000": "b"},
            "u16s": {"65535": "c"},
            "u128s": {"340282366920938463463374607431768211455": "d"}
        }"#,
    );
}

#[test]
fn weavy_rejects_out_of_range_integer_map_key() {
    let err = facet_json::from_str_weavy::<BTreeMap<i8, String>>(r#"{"300": "x"}"#)
        .unwrap_err()
        .to_string();
    assert!(err.contains("valid integer for map key"), "got: {err}");
}

#[test]
fn weavy_deserializes_sets() {
    assert_default_weavy_parity::<SetHolder>(
        r#"{
            "names": {"alpha": "a"},
            "ordered": ["beta", "alpha", "alpha"],
            "hashed": [2, 1, 2],
            "maybe": [null, 3, null],
            "points": [{"x": 1, "y": 2}, {"x": 0, "y": 0}, {"x": 1, "y": 2}]
        }"#,
    );
}

#[test]
fn weavy_drops_set_values_after_later_element_error() {
    DROPPED_SET_ELEMENTS.store(0, Ordering::SeqCst);

    let err = facet_json::from_str_weavy::<DroppySet>(r#"{"items":[{"value":1},{"value":"bad"}]}"#)
        .unwrap_err();

    assert!(matches!(
        err.kind,
        DeserializeErrorKind::UnexpectedToken { .. }
            | DeserializeErrorKind::InvalidValue { .. }
            | DeserializeErrorKind::NumberOutOfRange { .. }
    ));
    assert_eq!(DROPPED_SET_ELEMENTS.load(Ordering::SeqCst), 1);
}

#[test]
fn weavy_hash_map_duplicate_keys_drop_replaced_value() {
    DROPPED_DUPLICATE_MAP_VALUES.store(0, Ordering::SeqCst);

    {
        let got: HashMap<String, DuplicateMapDroppy> =
            facet_json::from_str_weavy(r#"{"same":{"value":1},"same":{"value":2}}"#).unwrap();

        assert_eq!(got.len(), 1);
        assert_eq!(got["same"].value, 2);
        assert_eq!(DROPPED_DUPLICATE_MAP_VALUES.load(Ordering::SeqCst), 1);
    }

    assert_eq!(DROPPED_DUPLICATE_MAP_VALUES.load(Ordering::SeqCst), 2);
}

#[test]
fn weavy_drops_map_values_after_later_value_error() {
    DROPPED_BULK_MAP_VALUES.store(0, Ordering::SeqCst);

    let err = facet_json::from_str_weavy::<DroppyMap>(
        r#"{"items":{"kept":{"value":1},"bad":{"value":"not a u8"}}}"#,
    )
    .unwrap_err();

    assert!(matches!(
        err.kind,
        DeserializeErrorKind::UnexpectedToken { .. }
            | DeserializeErrorKind::InvalidValue { .. }
            | DeserializeErrorKind::NumberOutOfRange { .. }
    ));
    assert_eq!(DROPPED_BULK_MAP_VALUES.load(Ordering::SeqCst), 1);
}

fn native_jit_expected() -> bool {
    cfg!(all(
        feature = "jit",
        any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64")
        )
    ))
}

#[test]
fn weavy_jit_plan_uses_native_for_root_scalar_struct_when_available() {
    let plan = facet_json::JsonWeavyPlan::<Point>::build_jit().unwrap();
    let native_available = native_jit_expected();

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
    } else if !native_jit_expected() {
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
    } else if !native_jit_expected() {
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
fn weavy_jit_ordered_scalar_struct_replays_after_i32_cursor_mismatch() {
    let got: Point = facet_json::from_str_weavy_jit(r#"{"x":1,"y":"2"}"#).unwrap();
    assert_eq!(got, Point { x: 1, y: 2 });
}

#[test]
fn weavy_jit_plan_uses_native_for_root_scalar_struct_list_when_available() {
    let plan = facet_json::JsonWeavyPlan::<Vec<Point>>::build_jit().unwrap();
    let report = plan.jit_fallback_report();
    let native_available = native_jit_expected();

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
fn weavy_jit_ordered_wide_scalars_accept_numeric_strings() {
    let plan = facet_json::JsonWeavyPlan::<WideScalarStruct>::build_jit().unwrap();
    let got = plan
        .from_str(
            r#"{"a":"1","b":2,"c":"3","d":4,"e":"-5","f":-6,"g":"-7","h":-8,"i":"9","j":-10,"k":true,"l":"11.5"}"#,
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
fn weavy_jit_ordered_float_scalars_replay_after_string_mismatch() {
    let plan = facet_json::JsonWeavyPlan::<FloatPoint>::build_jit().unwrap();
    let got = plan
        .from_str(r#"{"x":12.5,"y":"-0.03125","z":9000.125}"#)
        .unwrap();

    assert_eq!(
        got,
        FloatPoint {
            x: 12.5,
            y: -0.03125,
            z: 9000.125,
        }
    );
}

#[test]
fn weavy_jit_scalar_struct_list_handles_ordered_float_scalars() {
    let plan = facet_json::JsonWeavyPlan::<Vec<FloatPoint>>::build_jit().unwrap();
    let got = plan
        .from_str(r#"[{"x":12.5,"y":-0.03125,"z":9000.125},{"x":-4.25,"y":2.5,"z":0.125}]"#)
        .unwrap();

    assert_eq!(
        got,
        vec![
            FloatPoint {
                x: 12.5,
                y: -0.03125,
                z: 9000.125,
            },
            FloatPoint {
                x: -4.25,
                y: 2.5,
                z: 0.125,
            },
        ]
    );
}

#[test]
fn weavy_jit_scalar_struct_list_replays_after_float_string_mismatch() {
    let plan = facet_json::JsonWeavyPlan::<Vec<FloatPoint>>::build_jit().unwrap();
    let got = plan
        .from_str(r#"[{"x":12.5,"y":-0.03125,"z":9000.125},{"x":-4.25,"y":"2.5","z":0.125}]"#)
        .unwrap();

    assert_eq!(
        got,
        vec![
            FloatPoint {
                x: 12.5,
                y: -0.03125,
                z: 9000.125,
            },
            FloatPoint {
                x: -4.25,
                y: 2.5,
                z: 0.125,
            },
        ]
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
fn default_and_weavy_reject_duplicate_defaulted_fields() {
    let json = r#"{"a":1,"a":2}"#;

    let err = facet_json::from_str::<WideDefaultStruct>(json).unwrap_err();
    assert!(matches!(
        err.kind,
        DeserializeErrorKind::DuplicateField { ref field, .. } if field == "a"
    ));

    let err = facet_json::from_str_weavy::<WideDefaultStruct>(json).unwrap_err();
    assert!(matches!(
        err.kind,
        DeserializeErrorKind::DuplicateField { ref field, .. } if field == "a"
    ));
}

#[test]
fn default_and_weavy_reject_out_of_range_numeric_narrowing() {
    let json = r#"{"a":1111}"#;

    let err = facet_json::from_str::<WideDefaultStruct>(json).unwrap_err();
    assert!(matches!(
        err.kind,
        DeserializeErrorKind::NumberOutOfRange {
            target_type: "u8",
            ..
        }
    ));

    let err = facet_json::from_str_weavy::<WideDefaultStruct>(json).unwrap_err();
    assert!(matches!(
        err.kind,
        DeserializeErrorKind::NumberOutOfRange {
            target_type: "u8",
            ..
        }
    ));
}

#[test]
fn default_and_weavy_accept_integer_values_for_string_maps() {
    let json = r#"{"a":1,"b":2}"#;

    let default: HashMap<String, String> = facet_json::from_str(json).unwrap();
    let weavy: HashMap<String, String> = facet_json::from_str_weavy(json).unwrap();

    assert_eq!(default["a"], "1");
    assert_eq!(default["b"], "2");
    assert_eq!(weavy, default);
}

#[test]
fn default_and_weavy_accept_float_values_for_owned_strings() {
    let json = r#"{"l":00.5,"wide":10000000000000001.5}"#;

    let default: HashMap<String, String> = facet_json::from_str(json).unwrap();
    let weavy: HashMap<String, String> = facet_json::from_str_weavy(json).unwrap();

    assert_eq!(default["l"], "0.5");
    assert_eq!(default["wide"], "10000000000000002");
    assert_eq!(weavy, default);
}

#[test]
fn default_and_weavy_default_null_scalar_values() {
    let point: Point = facet_json::from_str(r#"{"x":null,"y":2}"#).unwrap();
    assert_eq!(point, Point { x: 0, y: 2 });
    assert_default_weavy_parity::<Point>(r#"{"x":null,"y":2}"#);

    let person: Person =
        facet_json::from_str(r#"{"name":null,"age":37,"favorite":null,"scores":[]}"#).unwrap();
    assert_eq!(
        person,
        Person {
            name: String::new(),
            age: 37,
            favorite: None,
            scores: Vec::new(),
        }
    );
    assert_default_weavy_parity::<Person>(r#"{"name":null,"age":37,"favorite":null,"scores":[]}"#);
}

#[test]
fn default_and_weavy_match_fuzzer_invalid_unknown_values() {
    assert_default_weavy_parity::<WideDefaultStruct>("\n{\"scores\":[1,,null,2,null]}");
    assert_default_weavy_parity::<WideDefaultStruct>("\n{\"scores\":[1,null,2,,null]}");
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
fn weavy_defaults_absent_option_field() {
    let person: Person =
        facet_json::from_str_weavy(r#"{"name":"Ada","age":37,"scores":[]}"#).unwrap();
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
fn default_and_weavy_require_absent_vec_fields() {
    let json = r#"{"name":"Ada","age":37}"#;

    let err = facet_json::from_str::<Person>(json).unwrap_err();
    assert!(matches!(
        err.kind,
        DeserializeErrorKind::MissingField {
            field: "scores",
            ..
        }
    ));

    let err = facet_json::from_str_weavy::<Person>(json).unwrap_err();
    assert!(matches!(
        err.kind,
        DeserializeErrorKind::MissingField {
            field: "scores",
            ..
        }
    ));
}

#[test]
fn default_and_weavy_require_absent_map_fields() {
    let json = r#"{"names":{}}"#;

    let err = facet_json::from_str::<MapHolder>(json).unwrap_err();
    assert!(matches!(
        err.kind,
        DeserializeErrorKind::MissingField {
            field: "buckets",
            ..
        }
    ));

    let err = facet_json::from_str_weavy::<MapHolder>(json).unwrap_err();
    assert!(matches!(
        err.kind,
        DeserializeErrorKind::MissingField {
            field: "buckets",
            ..
        }
    ));
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
fn weavy_deserializes_external_unit_enums() {
    assert_default_weavy_parity::<ExternalUnit>(r#""Active""#);
    assert_default_weavy_parity::<ExternalUnit>(r#"{"Inactive":{}}"#);
    assert_default_weavy_parity::<ExternalRenamed>(r#""enabled""#);
}

#[test]
fn weavy_deserializes_external_newtype_enums() {
    assert_default_weavy_parity::<ExternalNewtype>(r#"{"Some":99}"#);
    assert_default_weavy_parity::<ExternalNewtype>(r#"{"Point":{"x":10,"y":20}}"#);
}

#[test]
fn weavy_deserializes_external_tuple_enums() {
    assert_default_weavy_parity::<ExternalTuple>(r#"{"Pair":["test",42]}"#);
    assert_default_weavy_parity::<ExternalTuple>(r#"{"Triple":[true,1.5,"ok"]}"#);
    assert_default_weavy_parity::<ExternalTuple>(r#"{"Pair":["test"]}"#);
    assert_default_weavy_parity::<ExternalTuple>(r#"{"Pair":["test",42,true]}"#);
}

#[test]
fn weavy_deserializes_external_struct_enums() {
    assert_default_weavy_parity::<ExternalStruct>(
        r#"{"Record":{"name":"Ada","age":37,"favorite":null}}"#,
    );
    assert_default_weavy_parity::<ExternalStruct>(r#"{"Record":{"age":37,"name":"Ada"}}"#);
}

#[test]
fn weavy_drops_external_struct_variant_fields_after_later_error() {
    DROPPED_LIST_ELEMENTS.store(0, Ordering::SeqCst);

    facet_json::from_str_weavy::<ExternalDroppy>(r#"{"Item":{"item":{"value":1},"tail":"bad"}}"#)
        .unwrap_err();
    assert_eq!(DROPPED_LIST_ELEMENTS.load(Ordering::SeqCst), 1);
}

#[test]
fn weavy_drops_external_tuple_variant_fields_after_later_error() {
    DROPPED_LIST_ELEMENTS.store(0, Ordering::SeqCst);

    facet_json::from_str_weavy::<ExternalTupleDroppy>(r#"{"Pair":[{"value":1},"bad"]}"#)
        .unwrap_err();
    assert_eq!(DROPPED_LIST_ELEMENTS.load(Ordering::SeqCst), 1);
}

#[test]
fn weavy_deserializes_external_other_fallback_enums() {
    assert_default_weavy_parity::<ExternalOther>(r#"{"gt":["$value"]}"#);
    assert_default_weavy_parity::<ExternalOther>(r#"{"custom":"$id"}"#);
    assert_default_weavy_parity::<ExternalOther>(r#"{"eq-bare":"$id"}"#);
    assert_default_weavy_parity::<ExternalOther>(r#""$id""#);
    assert_default_weavy_parity::<ExternalOther>(r#"null"#);
}

#[test]
fn weavy_deserializes_internal_tagged_enums() {
    assert_default_weavy_parity::<InternalShape>(r#"{"type":"Circle","radius":5.0}"#);
    assert_default_weavy_parity::<InternalShape>(r#"{"radius":5.0,"type":"Circle"}"#);
    assert_default_weavy_parity::<InternalShape>(
        r#"{"type":"Rectangle","height":4.0,"width":3.0}"#,
    );
    assert_default_weavy_parity::<InternalUnit>(r#"{"status":"Active"}"#);
    assert_default_weavy_parity::<InternalRenamed>(r#"{"kind":"user_created","user_id":123}"#);
    assert_default_weavy_parity::<InternalRenamed>(r#"{"user_id":456,"kind":"user_deleted"}"#);
    assert_default_weavy_parity::<InternalShape>(r#"{"radius":5.0}"#);
}

#[test]
fn weavy_deserializes_adjacent_tagged_enums() {
    assert_default_weavy_parity::<AdjacentValue>(r#"{"kind":"Start"}"#);
    assert_default_weavy_parity::<AdjacentValue>(r#"{"kind":"Str","data":"hello"}"#);
    assert_default_weavy_parity::<AdjacentValue>(r#"{"data":"hello","kind":"Str"}"#);
    assert_default_weavy_parity::<AdjacentValue>(r#"{"kind":"Pair","data":[10,20]}"#);
    assert_default_weavy_parity::<AdjacentValue>(
        r#"{"kind":"Block","data":{"level":2,"text":"Title"}}"#,
    );
    assert_default_weavy_parity::<AdjacentRenamed>(
        r#"{"kind":"create_user","data":{"name":"alice"}}"#,
    );
    assert_default_weavy_parity::<AdjacentRenamed>(r#"{"data":{"id":123},"kind":"delete_user"}"#);
    assert_default_weavy_parity::<AdjacentValue>(r#"{"kind":"Str"}"#);
}

#[test]
fn weavy_drops_internal_tagged_fields_after_later_error() {
    DROPPED_LIST_ELEMENTS.store(0, Ordering::SeqCst);

    facet_json::from_str_weavy::<InternalDroppy>(
        r#"{"kind":"Item","item":{"value":1},"tail":"bad"}"#,
    )
    .unwrap_err();
    assert_eq!(DROPPED_LIST_ELEMENTS.load(Ordering::SeqCst), 1);
}

#[test]
fn weavy_drops_adjacent_tagged_fields_after_later_error() {
    DROPPED_LIST_ELEMENTS.store(0, Ordering::SeqCst);

    facet_json::from_str_weavy::<AdjacentDroppy>(
        r#"{"kind":"Item","data":{"item":{"value":1},"tail":"bad"}}"#,
    )
    .unwrap_err();
    assert_eq!(DROPPED_LIST_ELEMENTS.load(Ordering::SeqCst), 1);
}

#[test]
fn weavy_drops_adjacent_tuple_fields_after_later_error() {
    DROPPED_LIST_ELEMENTS.store(0, Ordering::SeqCst);

    facet_json::from_str_weavy::<AdjacentTupleDroppy>(
        r#"{"kind":"Pair","data":[{"value":1},"bad"]}"#,
    )
    .unwrap_err();
    assert_eq!(DROPPED_LIST_ELEMENTS.load(Ordering::SeqCst), 1);
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
