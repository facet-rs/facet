use facet::Facet;
use proptest::prelude::*;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::fmt::Debug;
use std::sync::OnceLock;

#[derive(Facet, Clone, Debug, Eq, Ord, PartialEq, PartialOrd)]
struct OraclePoint {
    x: i32,
    y: i32,
}

#[derive(Facet, Clone, Debug, PartialEq)]
struct OracleWide {
    a: u8,
    b: u16,
    c: u32,
    d: u64,
    e: i8,
    f: i16,
    g: i32,
    h: i64,
    k: bool,
    l: f64,
}

#[derive(Facet, Clone, Debug, PartialEq)]
struct OracleDefaults {
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
    k: bool,
    #[facet(default)]
    l: f64,
}

#[derive(Facet, Clone, Debug, PartialEq)]
struct OraclePerson {
    name: String,
    age: u32,
    favorite: Option<String>,
    scores: Vec<u16>,
}

#[derive(Facet, Clone, Debug, PartialEq)]
struct OracleMaybeScores {
    scores: Vec<Option<u16>>,
}

#[derive(Facet, Clone, Debug, PartialEq)]
struct OracleMapHolder {
    names: HashMap<String, String>,
    points: HashMap<String, OraclePoint>,
}

#[derive(Facet, Clone, Debug, PartialEq)]
struct OracleSetHolder {
    ordered: BTreeSet<String>,
    hashed: HashSet<u16>,
    maybe: BTreeSet<Option<u16>>,
    points: BTreeSet<OraclePoint>,
}

type OraclePointList = Vec<OraclePoint>;
type OracleStringMap = HashMap<String, String>;

fn point_plan() -> &'static facet_json::JsonWeavyPlan<OraclePoint> {
    static PLAN: OnceLock<facet_json::JsonWeavyPlan<OraclePoint>> = OnceLock::new();
    PLAN.get_or_init(|| facet_json::JsonWeavyPlan::<OraclePoint>::build().unwrap())
}

fn point_jit_plan() -> &'static facet_json::JsonWeavyPlan<OraclePoint> {
    static PLAN: OnceLock<facet_json::JsonWeavyPlan<OraclePoint>> = OnceLock::new();
    PLAN.get_or_init(|| facet_json::JsonWeavyPlan::<OraclePoint>::build_jit().unwrap())
}

fn wide_plan() -> &'static facet_json::JsonWeavyPlan<OracleWide> {
    static PLAN: OnceLock<facet_json::JsonWeavyPlan<OracleWide>> = OnceLock::new();
    PLAN.get_or_init(|| facet_json::JsonWeavyPlan::<OracleWide>::build().unwrap())
}

fn wide_jit_plan() -> &'static facet_json::JsonWeavyPlan<OracleWide> {
    static PLAN: OnceLock<facet_json::JsonWeavyPlan<OracleWide>> = OnceLock::new();
    PLAN.get_or_init(|| facet_json::JsonWeavyPlan::<OracleWide>::build_jit().unwrap())
}

fn defaults_plan() -> &'static facet_json::JsonWeavyPlan<OracleDefaults> {
    static PLAN: OnceLock<facet_json::JsonWeavyPlan<OracleDefaults>> = OnceLock::new();
    PLAN.get_or_init(|| facet_json::JsonWeavyPlan::<OracleDefaults>::build().unwrap())
}

fn defaults_jit_plan() -> &'static facet_json::JsonWeavyPlan<OracleDefaults> {
    static PLAN: OnceLock<facet_json::JsonWeavyPlan<OracleDefaults>> = OnceLock::new();
    PLAN.get_or_init(|| facet_json::JsonWeavyPlan::<OracleDefaults>::build_jit().unwrap())
}

fn person_plan() -> &'static facet_json::JsonWeavyPlan<OraclePerson> {
    static PLAN: OnceLock<facet_json::JsonWeavyPlan<OraclePerson>> = OnceLock::new();
    PLAN.get_or_init(|| facet_json::JsonWeavyPlan::<OraclePerson>::build().unwrap())
}

fn person_jit_plan() -> &'static facet_json::JsonWeavyPlan<OraclePerson> {
    static PLAN: OnceLock<facet_json::JsonWeavyPlan<OraclePerson>> = OnceLock::new();
    PLAN.get_or_init(|| facet_json::JsonWeavyPlan::<OraclePerson>::build_jit().unwrap())
}

fn maybe_scores_plan() -> &'static facet_json::JsonWeavyPlan<OracleMaybeScores> {
    static PLAN: OnceLock<facet_json::JsonWeavyPlan<OracleMaybeScores>> = OnceLock::new();
    PLAN.get_or_init(|| facet_json::JsonWeavyPlan::<OracleMaybeScores>::build().unwrap())
}

fn maybe_scores_jit_plan() -> &'static facet_json::JsonWeavyPlan<OracleMaybeScores> {
    static PLAN: OnceLock<facet_json::JsonWeavyPlan<OracleMaybeScores>> = OnceLock::new();
    PLAN.get_or_init(|| facet_json::JsonWeavyPlan::<OracleMaybeScores>::build_jit().unwrap())
}

fn point_list_plan() -> &'static facet_json::JsonWeavyPlan<OraclePointList> {
    static PLAN: OnceLock<facet_json::JsonWeavyPlan<OraclePointList>> = OnceLock::new();
    PLAN.get_or_init(|| facet_json::JsonWeavyPlan::<OraclePointList>::build().unwrap())
}

fn point_list_jit_plan() -> &'static facet_json::JsonWeavyPlan<OraclePointList> {
    static PLAN: OnceLock<facet_json::JsonWeavyPlan<OraclePointList>> = OnceLock::new();
    PLAN.get_or_init(|| facet_json::JsonWeavyPlan::<OraclePointList>::build_jit().unwrap())
}

fn string_map_plan() -> &'static facet_json::JsonWeavyPlan<OracleStringMap> {
    static PLAN: OnceLock<facet_json::JsonWeavyPlan<OracleStringMap>> = OnceLock::new();
    PLAN.get_or_init(|| facet_json::JsonWeavyPlan::<OracleStringMap>::build().unwrap())
}

fn string_map_jit_plan() -> &'static facet_json::JsonWeavyPlan<OracleStringMap> {
    static PLAN: OnceLock<facet_json::JsonWeavyPlan<OracleStringMap>> = OnceLock::new();
    PLAN.get_or_init(|| facet_json::JsonWeavyPlan::<OracleStringMap>::build_jit().unwrap())
}

fn map_holder_plan() -> &'static facet_json::JsonWeavyPlan<OracleMapHolder> {
    static PLAN: OnceLock<facet_json::JsonWeavyPlan<OracleMapHolder>> = OnceLock::new();
    PLAN.get_or_init(|| facet_json::JsonWeavyPlan::<OracleMapHolder>::build().unwrap())
}

fn map_holder_jit_plan() -> &'static facet_json::JsonWeavyPlan<OracleMapHolder> {
    static PLAN: OnceLock<facet_json::JsonWeavyPlan<OracleMapHolder>> = OnceLock::new();
    PLAN.get_or_init(|| facet_json::JsonWeavyPlan::<OracleMapHolder>::build_jit().unwrap())
}

fn set_holder_plan() -> &'static facet_json::JsonWeavyPlan<OracleSetHolder> {
    static PLAN: OnceLock<facet_json::JsonWeavyPlan<OracleSetHolder>> = OnceLock::new();
    PLAN.get_or_init(|| facet_json::JsonWeavyPlan::<OracleSetHolder>::build().unwrap())
}

fn set_holder_jit_plan() -> &'static facet_json::JsonWeavyPlan<OracleSetHolder> {
    static PLAN: OnceLock<facet_json::JsonWeavyPlan<OracleSetHolder>> = OnceLock::new();
    PLAN.get_or_init(|| facet_json::JsonWeavyPlan::<OracleSetHolder>::build_jit().unwrap())
}

fn assert_str_parity<T>(
    json: &str,
    plan: &facet_json::JsonWeavyPlan<T>,
    jit_plan: &facet_json::JsonWeavyPlan<T>,
) where
    T: facet::Facet<'static> + Debug + PartialEq,
{
    assert_result_parity(
        "weavy",
        &facet_json::from_str::<T>(json),
        plan.from_str(json),
    );
    assert_result_parity(
        "weavy-jit-requested",
        &facet_json::from_str::<T>(json),
        jit_plan.from_str(json),
    );
}

fn assert_slice_parity<T>(
    input: &[u8],
    plan: &facet_json::JsonWeavyPlan<T>,
    jit_plan: &facet_json::JsonWeavyPlan<T>,
) where
    T: facet::Facet<'static> + Debug + PartialEq,
{
    assert_result_parity(
        "weavy",
        &facet_json::from_slice::<T>(input),
        plan.from_slice(input),
    );
    assert_result_parity(
        "weavy-jit-requested",
        &facet_json::from_slice::<T>(input),
        jit_plan.from_slice(input),
    );
}

fn assert_result_parity<T>(
    backend: &str,
    baseline: &Result<T, facet_json::DeserializeError>,
    candidate: Result<T, facet_json::DeserializeError>,
) where
    T: Debug + PartialEq,
{
    match (baseline, candidate) {
        (Ok(expected), Ok(got)) => {
            assert_eq!(&got, expected, "{backend} decoded a different value")
        }
        (Ok(expected), Err(err)) => {
            panic!("{backend} rejected input accepted by default: {err:?}; expected {expected:?}")
        }
        (Err(_), Ok(got)) => panic!("{backend} accepted input rejected by default: {got:?}"),
        (Err(_), Err(_)) => {}
    }
}

fn small_string() -> impl Strategy<Value = String> {
    prop::string::string_regex("[a-zA-Z0-9_]{0,16}").unwrap()
}

fn small_key() -> impl Strategy<Value = String> {
    prop::string::string_regex("[a-zA-Z][a-zA-Z0-9_]{0,12}").unwrap()
}

fn finite_f64() -> impl Strategy<Value = f64> {
    (-1_000_000i32..=1_000_000).prop_map(|n| f64::from(n) / 8.0)
}

fn point_strategy() -> impl Strategy<Value = OraclePoint> {
    (any::<i32>(), any::<i32>()).prop_map(|(x, y)| OraclePoint { x, y })
}

fn wide_strategy() -> impl Strategy<Value = OracleWide> {
    (
        any::<u8>(),
        any::<u16>(),
        any::<u32>(),
        any::<u64>(),
        any::<i8>(),
        any::<i16>(),
        any::<i32>(),
        any::<i64>(),
        any::<bool>(),
        finite_f64(),
    )
        .prop_map(|(a, b, c, d, e, f, g, h, k, l)| OracleWide {
            a,
            b,
            c,
            d,
            e,
            f,
            g,
            h,
            k,
            l,
        })
}

fn defaults_strategy() -> impl Strategy<Value = OracleDefaults> {
    (
        any::<u8>(),
        any::<u16>(),
        any::<u32>(),
        any::<u64>(),
        any::<i8>(),
        any::<i16>(),
        any::<i32>(),
        any::<i64>(),
        any::<bool>(),
        finite_f64(),
    )
        .prop_map(|(a, b, c, d, e, f, g, h, k, l)| OracleDefaults {
            a,
            b,
            c,
            d,
            e,
            f,
            g,
            h,
            k,
            l,
        })
}

fn person_strategy() -> impl Strategy<Value = OraclePerson> {
    (
        small_string(),
        any::<u32>(),
        prop::option::of(small_string()),
        prop::collection::vec(any::<u16>(), 0..16),
    )
        .prop_map(|(name, age, favorite, scores)| OraclePerson {
            name,
            age,
            favorite,
            scores,
        })
}

fn maybe_scores_strategy() -> impl Strategy<Value = OracleMaybeScores> {
    prop::collection::vec(prop::option::of(any::<u16>()), 0..16)
        .prop_map(|scores| OracleMaybeScores { scores })
}

fn map_holder_strategy() -> impl Strategy<Value = OracleMapHolder> {
    (
        prop::collection::hash_map(small_key(), small_string(), 0..8),
        prop::collection::hash_map(small_key(), point_strategy(), 0..8),
    )
        .prop_map(|(names, points)| OracleMapHolder { names, points })
}

fn set_holder_strategy() -> impl Strategy<Value = OracleSetHolder> {
    (
        prop::collection::btree_set(small_string(), 0..8),
        prop::collection::hash_set(any::<u16>(), 0..8),
        prop::collection::btree_set(prop::option::of(any::<u16>()), 0..8),
        prop::collection::btree_set(point_strategy(), 0..8),
    )
        .prop_map(|(ordered, hashed, maybe, points)| OracleSetHolder {
            ordered,
            hashed,
            maybe,
            points,
        })
}

fn assert_serialized_value_parity<T>(
    value: &T,
    plan: &facet_json::JsonWeavyPlan<T>,
    jit_plan: &facet_json::JsonWeavyPlan<T>,
) where
    T: facet::Facet<'static> + Debug + PartialEq,
{
    let json = facet_json::to_string(value).unwrap();
    assert_str_parity(&json, plan, jit_plan);
}

fn assert_shape_bank_slice_parity(selector: u8, input: &[u8]) {
    match selector % 9 {
        0 => assert_slice_parity(input, point_plan(), point_jit_plan()),
        1 => assert_slice_parity(input, wide_plan(), wide_jit_plan()),
        2 => assert_slice_parity(input, defaults_plan(), defaults_jit_plan()),
        3 => assert_slice_parity(input, person_plan(), person_jit_plan()),
        4 => assert_slice_parity(input, maybe_scores_plan(), maybe_scores_jit_plan()),
        5 => assert_slice_parity(input, point_list_plan(), point_list_jit_plan()),
        6 => assert_slice_parity(input, string_map_plan(), string_map_jit_plan()),
        7 => assert_slice_parity(input, map_holder_plan(), map_holder_jit_plan()),
        _ => assert_slice_parity(input, set_holder_plan(), set_holder_jit_plan()),
    }
}

fn split_selector(data: &[u8]) -> (u8, &[u8]) {
    if let [selector @ b'0'..=b'9', b'\n', json @ ..] = data {
        return (*selector - b'0', json);
    }

    (data.first().copied().unwrap_or(0), data)
}

#[test]
fn mur_afl_timeout_replays_match_default_path() {
    for data in [
        &b"6\n{\"a\":1,\"d\":4,\"l\":00.5}\n"[..],
        &b"\n{\"scores\":[1,,null,2,null]}"[..],
        &b"\n{\"scores\":[1,null,2,,null]}"[..],
        &b"2\n{\"a\":1,\"d\":7,\"s\":11.E}"[..],
        &b"2\n{\"a\":1,\"d\":4,\"f\":11. }"[..],
        &b"2\n{\"f\":0,\"d\":2,\"l\":null,\"scl222Bb\":22222}"[..],
        &b"6\n[]a\"/1,\"?\x00:2F\"c\":2F\"c\":4M\"f\":-7\x00\x00\x00\x7f:-4,\"g\"::-4\x00\x80\x00\x00:-8,\"k\"?\x00:2F\"\x00\xf8\xff\xff\":10.}\n"[..],
        &b"6\n{\"f\":0,\"d\":2,\"l\":00.023222222222222222222222}"[..],
    ] {
        let (selector, json) = split_selector(data);
        assert_shape_bank_slice_parity(selector, json);
    }
}

proptest! {
    #![proptest_config(ProptestConfig {
        cases: 96,
        .. ProptestConfig::default()
    })]

    #[test]
    fn generated_point_values_match_all_weavy_backends(value in point_strategy()) {
        assert_serialized_value_parity(&value, point_plan(), point_jit_plan());
    }

    #[test]
    fn generated_wide_scalar_values_match_all_weavy_backends(value in wide_strategy()) {
        assert_serialized_value_parity(&value, wide_plan(), wide_jit_plan());
    }

    #[test]
    fn generated_defaulted_values_match_all_weavy_backends(value in defaults_strategy()) {
        assert_serialized_value_parity(&value, defaults_plan(), defaults_jit_plan());
    }

    #[test]
    fn generated_person_values_match_all_weavy_backends(value in person_strategy()) {
        assert_serialized_value_parity(&value, person_plan(), person_jit_plan());
    }

    #[test]
    fn generated_option_list_values_match_all_weavy_backends(value in maybe_scores_strategy()) {
        assert_serialized_value_parity(&value, maybe_scores_plan(), maybe_scores_jit_plan());
    }

    #[test]
    fn generated_point_lists_match_all_weavy_backends(value in prop::collection::vec(point_strategy(), 0..16)) {
        assert_serialized_value_parity(&value, point_list_plan(), point_list_jit_plan());
    }

    #[test]
    fn generated_string_maps_match_all_weavy_backends(value in prop::collection::hash_map(small_key(), small_string(), 0..8)) {
        assert_serialized_value_parity(&value, string_map_plan(), string_map_jit_plan());
    }

    #[test]
    fn generated_map_holders_match_all_weavy_backends(value in map_holder_strategy()) {
        assert_serialized_value_parity(&value, map_holder_plan(), map_holder_jit_plan());
    }

    #[test]
    fn generated_set_holders_match_all_weavy_backends(value in set_holder_strategy()) {
        assert_serialized_value_parity(&value, set_holder_plan(), set_holder_jit_plan());
    }

    #[test]
    fn arbitrary_bytes_have_success_parity_across_shape_bank(selector in any::<u8>(), input in prop::collection::vec(any::<u8>(), 0..256)) {
        assert_shape_bank_slice_parity(selector, &input);
    }
}
