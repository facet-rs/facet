#[macro_use]
extern crate afl;

use facet::Facet;
use std::collections::HashMap;
use std::fmt::Debug;
use std::sync::OnceLock;

#[derive(Facet, Debug, PartialEq)]
struct OraclePoint {
    x: i32,
    y: i32,
}

#[derive(Facet, Debug, PartialEq)]
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

#[derive(Facet, Debug, PartialEq)]
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

#[derive(Facet, Debug, PartialEq)]
struct OraclePerson {
    name: String,
    age: u32,
    favorite: Option<String>,
    scores: Vec<u16>,
}

#[derive(Facet, Debug, PartialEq)]
struct OracleMaybeScores {
    scores: Vec<Option<u16>>,
}

#[derive(Facet, Debug, PartialEq)]
struct OracleMapHolder {
    names: HashMap<String, String>,
    points: HashMap<String, OraclePoint>,
}

type OraclePointList = Vec<OraclePoint>;
type OracleStringMap = HashMap<String, String>;

fn main() {
    prewarm_shape_bank();

    fuzz!(|data: &[u8]| {
        let (selector, json) = split_selector(data);
        assert_shape_bank_slice_parity(selector, json);
    });
}

fn prewarm_shape_bank() {
    assert_shape_bank_slice_parity(0, br#"{"x":1,"y":2}"#);
    assert_shape_bank_slice_parity(
        1,
        br#"{"a":1,"b":2,"c":3,"d":4,"e":-5,"f":-6,"g":-7,"h":-8,"k":true,"l":11.5}"#,
    );
    assert_shape_bank_slice_parity(2, br#"{"a":1,"d":4,"l":11.5}"#);
    assert_shape_bank_slice_parity(
        3,
        br#"{"name":"Ada","age":37,"favorite":null,"scores":[1,2,3]}"#,
    );
    assert_shape_bank_slice_parity(4, br#"{"scores":[1,null,2,null]}"#);
    assert_shape_bank_slice_parity(5, br#"[{"x":1,"y":2},{"x":3,"y":4}]"#);
    assert_shape_bank_slice_parity(6, br#"{"same":"first","other":"second"}"#);
    assert_shape_bank_slice_parity(
        7,
        br#"{"names":{"main":"floor"},"points":{"origin":{"x":0,"y":0}}}"#,
    );
}

fn split_selector(data: &[u8]) -> (u8, &[u8]) {
    if let [selector @ b'0'..=b'9', b'\n', json @ ..] = data {
        return (*selector - b'0', json);
    }

    (data.first().copied().unwrap_or(0), data)
}

fn assert_shape_bank_slice_parity(selector: u8, input: &[u8]) {
    match selector % 8 {
        0 => assert_slice_parity(input, point_plan(), point_jit_plan()),
        1 => assert_slice_parity(input, wide_plan(), wide_jit_plan()),
        2 => assert_slice_parity(input, defaults_plan(), defaults_jit_plan()),
        3 => assert_slice_parity(input, person_plan(), person_jit_plan()),
        4 => assert_slice_parity(input, maybe_scores_plan(), maybe_scores_jit_plan()),
        5 => assert_slice_parity(input, point_list_plan(), point_list_jit_plan()),
        6 => assert_slice_parity(input, string_map_plan(), string_map_jit_plan()),
        _ => assert_slice_parity(input, map_holder_plan(), map_holder_jit_plan()),
    }
}

fn assert_slice_parity<T>(
    input: &[u8],
    plan: &facet_json::JsonWeavyPlan<T>,
    jit_plan: &facet_json::JsonWeavyPlan<T>,
) where
    T: facet::Facet<'static> + Debug + PartialEq,
{
    let baseline = facet_json::from_slice::<T>(input);
    assert_result_parity("weavy", &baseline, plan.from_slice(input));
    assert_result_parity("weavy-jit-requested", &baseline, jit_plan.from_slice(input));
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
