//! Benchmark comparing TypePlan creation overhead vs reuse.
//!
//! This measures whether reusing a TypePlan across multiple deserializations
//! provides meaningful performance benefits.
//!
//! Run with:
//!   cargo bench -p facet-json --bench typeplan_reuse

use divan::{Bencher, black_box};
use facet::Facet;
use facet_format::MetaSource;
use facet_json::JsonParser;
use facet_reflect::TypePlan;
use serde::de::DeserializeOwned;

fn main() {
    divan::main();
}

// =============================================================================
// Test types of varying complexity
// =============================================================================

/// Simple flat struct - baseline
#[derive(Debug, Facet, serde::Deserialize)]
struct Point {
    x: i32,
    y: i32,
}

/// Medium complexity with nested types
#[derive(Debug, Facet, serde::Deserialize)]
struct Person {
    name: String,
    age: u32,
    email: Option<String>,
    scores: Vec<i32>,
}

/// Complex nested struct
#[derive(Debug, Facet, serde::Deserialize)]
struct Company {
    name: String,
    employees: Vec<Employee>,
    headquarters: Address,
}

#[derive(Debug, Facet, serde::Deserialize)]
struct Employee {
    id: u64,
    name: String,
    department: String,
    salary: f64,
}

#[derive(Debug, Facet, serde::Deserialize)]
struct Address {
    street: String,
    city: String,
    country: String,
    zip: String,
}

/// Tiny float-heavy struct
#[derive(Debug, Facet, serde::Deserialize)]
struct FloatPoint {
    x: f64,
    y: f64,
    z: f64,
}

/// Numeric arrays and mixed scalar values
#[derive(Debug, Facet, serde::Deserialize)]
struct SensorFrame {
    id: u64,
    temperature: f64,
    pressure: f32,
    samples: Vec<f64>,
    deltas: Vec<i32>,
    flags: Vec<bool>,
}

/// Wide scalar record, deliberately over the old 8-field inline ledger limit
#[derive(Debug, Facet, serde::Deserialize)]
struct WideScalars {
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

/// Scalar record with defaults, for missing-field synthesis.
#[derive(Debug, Facet, serde::Deserialize)]
struct DefaultScalars {
    #[facet(default)]
    #[serde(default)]
    a: u8,
    #[facet(default)]
    #[serde(default)]
    b: u16,
    #[facet(default)]
    #[serde(default)]
    c: u32,
    #[facet(default)]
    #[serde(default)]
    d: u64,
    #[facet(default)]
    #[serde(default)]
    e: i8,
    #[facet(default)]
    #[serde(default)]
    f: i16,
    #[facet(default)]
    #[serde(default)]
    g: bool,
    #[facet(default)]
    #[serde(default)]
    h: f64,
}

#[derive(Debug, Facet, serde::Deserialize)]
#[facet(tag = "kind")]
#[serde(tag = "kind")]
#[repr(u8)]
enum InternalTaggedEvent {
    Created { id: u64, name: String, active: bool },
    Deleted { id: u64, reason: String },
}

impl InternalTaggedEvent {
    fn checksum(&self) -> usize {
        match self {
            Self::Created { id, name, active } => *id as usize ^ name.len() ^ usize::from(*active),
            Self::Deleted { id, reason } => *id as usize ^ reason.len(),
        }
    }
}

#[derive(Debug, Facet, serde::Deserialize)]
#[facet(tag = "kind", content = "data")]
#[serde(tag = "kind", content = "data")]
#[repr(u8)]
enum AdjacentTaggedEvent {
    Started,
    Message { id: u64, text: String },
    Pair(i32, i32),
}

impl AdjacentTaggedEvent {
    fn checksum(&self) -> usize {
        match self {
            Self::Started => 0,
            Self::Message { id, text } => *id as usize ^ text.len(),
            Self::Pair(a, b) => (*a as usize).wrapping_add(*b as usize),
        }
    }
}

#[derive(Debug, Facet, serde::Deserialize)]
#[facet(untagged)]
#[serde(untagged)]
#[repr(u8)]
enum UntaggedEvent {
    Point { x: i32, y: i32 },
    Text(String),
    Count(u64),
    Flag(bool),
}

impl UntaggedEvent {
    fn checksum(&self) -> usize {
        match self {
            Self::Point { x, y } => (*x as usize).wrapping_add(*y as usize),
            Self::Text(value) => value.len(),
            Self::Count(value) => *value as usize,
            Self::Flag(value) => usize::from(*value),
        }
    }
}

// =============================================================================
// Test data
// =============================================================================

const POINT_JSON: &str = r#"{"x": 10, "y": 20}"#;
const POINT_LIST_JSON: &str =
    r#"[{"x":10,"y":20},{"x":30,"y":40},{"x":50,"y":60},{"x":70,"y":80}]"#;

const PERSON_JSON: &str = r#"{
    "name": "Alice",
    "age": 30,
    "email": "alice@example.com",
    "scores": [95, 87, 92, 88, 91]
}"#;

const COMPANY_JSON: &str = r#"{
    "name": "Acme Corp",
    "employees": [
        {"id": 1, "name": "Alice", "department": "Engineering", "salary": 120000.0},
        {"id": 2, "name": "Bob", "department": "Sales", "salary": 90000.0},
        {"id": 3, "name": "Charlie", "department": "Engineering", "salary": 115000.0}
    ],
    "headquarters": {
        "street": "123 Main St",
        "city": "San Francisco",
        "country": "USA",
        "zip": "94102"
    }
}"#;

const FLOAT_POINT_JSON: &str = r#"{"x": 12.5, "y": -0.03125, "z": 9000.125}"#;
const FLOAT_POINT_LIST_JSON: &str = r#"[
    {"x": 12.5, "y": -0.03125, "z": 9000.125},
    {"x": -4.25, "y": 2.5, "z": 0.125},
    {"x": 100.0, "y": -200.5, "z": 300.75},
    {"x": 0.0, "y": 1.0, "z": -1.0}
]"#;

const SENSOR_FRAME_JSON: &str = r#"{
    "id": 9001,
    "temperature": 21.75,
    "pressure": 1013.25,
    "samples": [0.5, 1.25, 2.5, 5.0, 10.0, 20.0, 40.0, 80.0],
    "deltas": [-3, -1, 0, 1, 3, 5, 8, 13],
    "flags": [true, false, true, true, false, false, true, false]
}"#;

const WIDE_SCALARS_JSON: &str = r#"{
    "a": 1,
    "b": 2,
    "c": 3,
    "d": 4,
    "e": -5,
    "f": -6,
    "g": -7,
    "h": -8,
    "i": 9,
    "j": -10,
    "k": true,
    "l": 11.5
}"#;

const WIDE_SCALARS_LIST_JSON: &str = r#"[
    {"a":1,"b":2,"c":3,"d":4,"e":-5,"f":-6,"g":-7,"h":-8,"i":9,"j":-10,"k":true,"l":11.5},
    {"a":12,"b":13,"c":14,"d":15,"e":-16,"f":-17,"g":-18,"h":-19,"i":20,"j":-21,"k":false,"l":22.5},
    {"a":23,"b":24,"c":25,"d":26,"e":-27,"f":-28,"g":-29,"h":-30,"i":31,"j":-32,"k":true,"l":33.5}
]"#;

const WIDE_SCALARS_OUT_OF_ORDER_JSON: &str = r#"{
    "l": 11.5,
    "k": true,
    "j": -10,
    "i": 9,
    "h": -8,
    "g": -7,
    "f": -6,
    "e": -5,
    "d": 4,
    "c": 3,
    "b": 2,
    "a": 1
}"#;

const WIDE_SCALARS_LIST_OUT_OF_ORDER_JSON: &str = r#"[
    {"a":1,"b":2,"c":3,"d":4,"e":-5,"f":-6,"g":-7,"h":-8,"i":9,"j":-10,"k":true,"l":11.5},
    {"l":22.5,"k":false,"j":-21,"i":20,"h":-19,"g":-18,"f":-17,"e":-16,"d":15,"c":14,"b":13,"a":12},
    {"a":23,"b":24,"c":25,"d":26,"e":-27,"f":-28,"g":-29,"h":-30,"i":31,"j":-32,"k":true,"l":33.5}
]"#;

const WIDE_SCALARS_SKIPPED_UNKNOWN_JSON: &str = r#"{
    "a": 1,
    "unknown_scalar": 12345,
    "b": 2,
    "unknown_array": [1, 2, 3, 4],
    "c": 3,
    "unknown_object": {"nested": true, "items": [1, 2, 3]},
    "d": 4,
    "e": -5,
    "f": -6,
    "g": -7,
    "h": -8,
    "i": 9,
    "j": -10,
    "k": true,
    "l": 11.5
}"#;

const WIDE_SCALARS_LIST_SKIPPED_UNKNOWN_JSON: &str = r#"[
    {"a":1,"b":2,"c":3,"d":4,"e":-5,"f":-6,"g":-7,"h":-8,"i":9,"j":-10,"k":true,"l":11.5},
    {"a":12,"unknown_scalar":12345,"b":13,"unknown_array":[1,2,3,4],"c":14,"unknown_object":{"nested":true,"items":[1,2,3]},"d":15,"e":-16,"f":-17,"g":-18,"h":-19,"i":20,"j":-21,"k":false,"l":22.5},
    {"a":23,"b":24,"c":25,"d":26,"e":-27,"f":-28,"g":-29,"h":-30,"i":31,"j":-32,"k":true,"l":33.5}
]"#;

const DEFAULT_SCALARS_MISSING_JSON: &str = r#"{
    "a": 1,
    "d": 4,
    "h": 11.5
}"#;

const INTERNAL_TAGGED_JSON: &str = r#"{
    "kind": "Created",
    "id": 42,
    "name": "alice",
    "active": true
}"#;

const ADJACENT_TAGGED_JSON: &str = r#"{
    "kind": "Message",
    "data": {
        "id": 42,
        "text": "hello"
    }
}"#;

const UNTAGGED_EVENT_JSON: &str = r#"{"x": 10, "y": 20}"#;

fn bench_fresh_typeplan<T>(bencher: Bencher, json: &'static str)
where
    T: Facet<'static>,
{
    bencher.bench(|| {
        let result: T = black_box(facet_json::from_str(black_box(json)).unwrap());
        black_box(result)
    });
}

fn bench_reused_typeplan<T>(bencher: Bencher, json: &'static str)
where
    T: Facet<'static>,
{
    use facet_format::FormatDeserializer;

    let plan = TypePlan::<T>::build().unwrap();

    bencher.bench(|| {
        let partial = plan.partial_owned().unwrap();
        let mut parser = JsonParser::<true>::new(black_box(json.as_bytes()));
        let mut de = FormatDeserializer::new_owned(&mut parser);
        let partial = de
            .deserialize_into(partial, MetaSource::FromEvents)
            .unwrap();
        let result: T = partial.build().unwrap().materialize().unwrap();
        black_box(result)
    });
}

fn bench_weavy_reused_plan<T>(bencher: Bencher, json: &'static str)
where
    T: Facet<'static>,
{
    let plan = facet_json::JsonWeavyPlan::<T>::build().unwrap();

    bencher.bench(|| {
        let result: T = black_box(plan.from_str(black_box(json)).unwrap());
        black_box(result)
    });
}

fn bench_weavy_jit_reused_plan<T>(bencher: Bencher, json: &'static str)
where
    T: Facet<'static>,
{
    let plan = facet_json::JsonWeavyPlan::<T>::build_jit().unwrap();

    bencher.bench(|| {
        let result: T = black_box(plan.from_str(black_box(json)).unwrap());
        black_box(result)
    });
}

fn bench_serde_json<T>(bencher: Bencher, json: &'static str)
where
    T: DeserializeOwned,
{
    bencher.bench(|| {
        let result: T = black_box(serde_json::from_str(black_box(json)).unwrap());
        black_box(result)
    });
}

fn bench_reused_typeplan_observed<T>(
    bencher: Bencher,
    json: &'static str,
    observe: impl Fn(&T) -> usize + Sync,
) where
    T: Facet<'static>,
{
    use facet_format::FormatDeserializer;

    let plan = TypePlan::<T>::build().unwrap();

    bencher.bench(|| {
        let partial = plan.partial_owned().unwrap();
        let mut parser = JsonParser::<true>::new(black_box(json.as_bytes()));
        let mut de = FormatDeserializer::new_owned(&mut parser);
        let partial = de
            .deserialize_into(partial, MetaSource::FromEvents)
            .unwrap();
        let result: T = partial.build().unwrap().materialize().unwrap();
        black_box(observe(&result));
        black_box(result)
    });
}

fn bench_weavy_reused_plan_observed<T>(
    bencher: Bencher,
    json: &'static str,
    observe: impl Fn(&T) -> usize + Sync,
) where
    T: Facet<'static>,
{
    let plan = facet_json::JsonWeavyPlan::<T>::build().unwrap();

    bencher.bench(|| {
        let result: T = black_box(plan.from_str(black_box(json)).unwrap());
        black_box(observe(&result));
        black_box(result)
    });
}

fn bench_weavy_jit_reused_plan_observed<T>(
    bencher: Bencher,
    json: &'static str,
    observe: impl Fn(&T) -> usize + Sync,
) where
    T: Facet<'static>,
{
    let plan = facet_json::JsonWeavyPlan::<T>::build_jit().unwrap();

    bencher.bench(|| {
        let result: T = black_box(plan.from_str(black_box(json)).unwrap());
        black_box(observe(&result));
        black_box(result)
    });
}

fn bench_serde_json_observed<T>(
    bencher: Bencher,
    json: &'static str,
    observe: impl Fn(&T) -> usize + Sync,
) where
    T: DeserializeOwned,
{
    bencher.bench(|| {
        let result: T = black_box(serde_json::from_str(black_box(json)).unwrap());
        black_box(observe(&result));
        black_box(result)
    });
}

// =============================================================================
// Benchmarks - Point (simple)
// =============================================================================

/// Fresh TypePlan each iteration - current default behavior
#[divan::bench]
fn point_fresh_typeplan(bencher: Bencher) {
    let json = POINT_JSON;
    bencher.bench(|| {
        let result: Point = black_box(facet_json::from_str(black_box(json)).unwrap());
        black_box(result)
    });
}

/// Reuse TypePlan across iterations
#[divan::bench]
fn point_reused_typeplan(bencher: Bencher) {
    use facet_format::FormatDeserializer;

    let json = POINT_JSON;
    let plan = TypePlan::<Point>::build().unwrap();

    bencher.bench(|| {
        let partial = plan.partial_owned().unwrap();
        let mut parser = JsonParser::<true>::new(black_box(json.as_bytes()));
        let mut de = FormatDeserializer::new_owned(&mut parser);
        let partial = de
            .deserialize_into(partial, MetaSource::FromEvents)
            .unwrap();
        let result: Point = partial.build().unwrap().materialize().unwrap();
        black_box(result)
    });
}

/// Reuse Weavy JSON plan across iterations
#[divan::bench]
fn point_weavy_reused_plan(bencher: Bencher) {
    let json = POINT_JSON;
    let plan = facet_json::JsonWeavyPlan::<Point>::build().unwrap();

    bencher.bench(|| {
        let result: Point = black_box(plan.from_str(black_box(json)).unwrap());
        black_box(result)
    });
}

/// Reuse Weavy JSON plan across iterations with JIT requested
#[divan::bench]
fn point_weavy_jit_reused_plan(bencher: Bencher) {
    let json = POINT_JSON;
    let plan = facet_json::JsonWeavyPlan::<Point>::build_jit().unwrap();

    bencher.bench(|| {
        let result: Point = black_box(plan.from_str(black_box(json)).unwrap());
        black_box(result)
    });
}

/// serde_json path
#[divan::bench]
fn point_serde_json(bencher: Bencher) {
    let json = POINT_JSON;
    bencher.bench(|| {
        let result: Point = black_box(serde_json::from_str(black_box(json)).unwrap());
        black_box(result)
    });
}

/// serde_json path from bytes
#[divan::bench]
fn point_serde_json_from_slice(bencher: Bencher) {
    let json = POINT_JSON.as_bytes();
    bencher.bench(|| {
        let result: Point = black_box(serde_json::from_slice(black_box(json)).unwrap());
        black_box(result)
    });
}

// =============================================================================
// Benchmarks - Vec<Point> (ordered scalar struct list)
// =============================================================================

#[divan::bench]
fn point_list_weavy_reused_plan(bencher: Bencher) {
    bench_weavy_reused_plan::<Vec<Point>>(bencher, POINT_LIST_JSON);
}

#[divan::bench]
fn point_list_weavy_jit_reused_plan(bencher: Bencher) {
    bench_weavy_jit_reused_plan::<Vec<Point>>(bencher, POINT_LIST_JSON);
}

#[divan::bench]
fn point_list_serde_json(bencher: Bencher) {
    bench_serde_json::<Vec<Point>>(bencher, POINT_LIST_JSON);
}

// =============================================================================
// Benchmarks - Person (medium)
// =============================================================================

/// Fresh TypePlan each iteration
#[divan::bench]
fn person_fresh_typeplan(bencher: Bencher) {
    let json = PERSON_JSON;
    bencher.bench(|| {
        let result: Person = black_box(facet_json::from_str(black_box(json)).unwrap());
        black_box(result)
    });
}

/// Reuse TypePlan across iterations
#[divan::bench]
fn person_reused_typeplan(bencher: Bencher) {
    use facet_format::FormatDeserializer;

    let json = PERSON_JSON;
    let plan = TypePlan::<Person>::build().unwrap();

    bencher.bench(|| {
        let partial = plan.partial_owned().unwrap();
        let mut parser = JsonParser::<true>::new(black_box(json.as_bytes()));
        let mut de = FormatDeserializer::new_owned(&mut parser);
        let partial = de
            .deserialize_into(partial, MetaSource::FromEvents)
            .unwrap();
        let result: Person = partial.build().unwrap().materialize().unwrap();
        black_box(result)
    });
}

/// Reuse Weavy JSON plan across iterations
#[divan::bench]
fn person_weavy_reused_plan(bencher: Bencher) {
    let json = PERSON_JSON;
    let plan = facet_json::JsonWeavyPlan::<Person>::build().unwrap();

    bencher.bench(|| {
        let result: Person = black_box(plan.from_str(black_box(json)).unwrap());
        black_box(result)
    });
}

/// serde_json path
#[divan::bench]
fn person_serde_json(bencher: Bencher) {
    let json = PERSON_JSON;
    bencher.bench(|| {
        let result: Person = black_box(serde_json::from_str(black_box(json)).unwrap());
        black_box(result)
    });
}

/// serde_json path from bytes
#[divan::bench]
fn person_serde_json_from_slice(bencher: Bencher) {
    let json = PERSON_JSON.as_bytes();
    bencher.bench(|| {
        let result: Person = black_box(serde_json::from_slice(black_box(json)).unwrap());
        black_box(result)
    });
}

// =============================================================================
// Benchmarks - Company (complex)
// =============================================================================

/// Fresh TypePlan each iteration
#[divan::bench]
fn company_fresh_typeplan(bencher: Bencher) {
    let json = COMPANY_JSON;
    bencher.bench(|| {
        let result: Company = black_box(facet_json::from_str(black_box(json)).unwrap());
        black_box(result)
    });
}

/// Reuse TypePlan across iterations
#[divan::bench]
fn company_reused_typeplan(bencher: Bencher) {
    use facet_format::FormatDeserializer;

    let json = COMPANY_JSON;
    let plan = TypePlan::<Company>::build().unwrap();

    bencher.bench(|| {
        let partial = plan.partial_owned().unwrap();
        let mut parser = JsonParser::<true>::new(black_box(json.as_bytes()));
        let mut de = FormatDeserializer::new_owned(&mut parser);
        let partial = de
            .deserialize_into(partial, MetaSource::FromEvents)
            .unwrap();
        let result: Company = partial.build().unwrap().materialize().unwrap();
        black_box(result)
    });
}

/// Reuse Weavy JSON plan across iterations
#[divan::bench]
fn company_weavy_reused_plan(bencher: Bencher) {
    let json = COMPANY_JSON;
    let plan = facet_json::JsonWeavyPlan::<Company>::build().unwrap();

    bencher.bench(|| {
        let result: Company = black_box(plan.from_str(black_box(json)).unwrap());
        black_box(result)
    });
}

/// serde_json path
#[divan::bench]
fn company_serde_json(bencher: Bencher) {
    let json = COMPANY_JSON;
    bencher.bench(|| {
        let result: Company = black_box(serde_json::from_str(black_box(json)).unwrap());
        black_box(result)
    });
}

/// serde_json path from bytes
#[divan::bench]
fn company_serde_json_from_slice(bencher: Bencher) {
    let json = COMPANY_JSON.as_bytes();
    bencher.bench(|| {
        let result: Company = black_box(serde_json::from_slice(black_box(json)).unwrap());
        black_box(result)
    });
}

// =============================================================================
// Benchmarks - FloatPoint (float-heavy tiny struct)
// =============================================================================

#[divan::bench]
fn float_point_fresh_typeplan(bencher: Bencher) {
    bench_fresh_typeplan::<FloatPoint>(bencher, FLOAT_POINT_JSON);
}

#[divan::bench]
fn float_point_reused_typeplan(bencher: Bencher) {
    bench_reused_typeplan::<FloatPoint>(bencher, FLOAT_POINT_JSON);
}

#[divan::bench]
fn float_point_weavy_reused_plan(bencher: Bencher) {
    bench_weavy_reused_plan::<FloatPoint>(bencher, FLOAT_POINT_JSON);
}

#[divan::bench]
fn float_point_weavy_jit_reused_plan(bencher: Bencher) {
    bench_weavy_jit_reused_plan::<FloatPoint>(bencher, FLOAT_POINT_JSON);
}

#[divan::bench]
fn float_point_serde_json(bencher: Bencher) {
    bench_serde_json::<FloatPoint>(bencher, FLOAT_POINT_JSON);
}

// =============================================================================
// Benchmarks - Vec<FloatPoint> (float-heavy scalar struct list)
// =============================================================================

#[divan::bench]
fn float_point_list_weavy_reused_plan(bencher: Bencher) {
    bench_weavy_reused_plan::<Vec<FloatPoint>>(bencher, FLOAT_POINT_LIST_JSON);
}

#[divan::bench]
fn float_point_list_weavy_jit_reused_plan(bencher: Bencher) {
    bench_weavy_jit_reused_plan::<Vec<FloatPoint>>(bencher, FLOAT_POINT_LIST_JSON);
}

#[divan::bench]
fn float_point_list_serde_json(bencher: Bencher) {
    bench_serde_json::<Vec<FloatPoint>>(bencher, FLOAT_POINT_LIST_JSON);
}

// =============================================================================
// Benchmarks - SensorFrame (numeric arrays)
// =============================================================================

#[divan::bench]
fn sensor_frame_fresh_typeplan(bencher: Bencher) {
    bench_fresh_typeplan::<SensorFrame>(bencher, SENSOR_FRAME_JSON);
}

#[divan::bench]
fn sensor_frame_reused_typeplan(bencher: Bencher) {
    bench_reused_typeplan::<SensorFrame>(bencher, SENSOR_FRAME_JSON);
}

#[divan::bench]
fn sensor_frame_weavy_reused_plan(bencher: Bencher) {
    bench_weavy_reused_plan::<SensorFrame>(bencher, SENSOR_FRAME_JSON);
}

#[divan::bench]
fn sensor_frame_serde_json(bencher: Bencher) {
    bench_serde_json::<SensorFrame>(bencher, SENSOR_FRAME_JSON);
}

// =============================================================================
// Benchmarks - WideScalars (wide scalar record)
// =============================================================================

#[divan::bench]
fn wide_scalars_fresh_typeplan(bencher: Bencher) {
    bench_fresh_typeplan::<WideScalars>(bencher, WIDE_SCALARS_JSON);
}

#[divan::bench]
fn wide_scalars_reused_typeplan(bencher: Bencher) {
    bench_reused_typeplan::<WideScalars>(bencher, WIDE_SCALARS_JSON);
}

#[divan::bench]
fn wide_scalars_weavy_reused_plan(bencher: Bencher) {
    bench_weavy_reused_plan::<WideScalars>(bencher, WIDE_SCALARS_JSON);
}

#[divan::bench]
fn wide_scalars_weavy_jit_reused_plan(bencher: Bencher) {
    bench_weavy_jit_reused_plan::<WideScalars>(bencher, WIDE_SCALARS_JSON);
}

#[divan::bench]
fn wide_scalars_serde_json(bencher: Bencher) {
    bench_serde_json::<WideScalars>(bencher, WIDE_SCALARS_JSON);
}

#[divan::bench]
fn wide_scalars_out_of_order_weavy_reused_plan(bencher: Bencher) {
    bench_weavy_reused_plan::<WideScalars>(bencher, WIDE_SCALARS_OUT_OF_ORDER_JSON);
}

#[divan::bench]
fn wide_scalars_out_of_order_weavy_jit_reused_plan(bencher: Bencher) {
    bench_weavy_jit_reused_plan::<WideScalars>(bencher, WIDE_SCALARS_OUT_OF_ORDER_JSON);
}

#[divan::bench]
fn wide_scalars_out_of_order_serde_json(bencher: Bencher) {
    bench_serde_json::<WideScalars>(bencher, WIDE_SCALARS_OUT_OF_ORDER_JSON);
}

#[divan::bench]
fn wide_scalars_skipped_unknown_weavy_reused_plan(bencher: Bencher) {
    bench_weavy_reused_plan::<WideScalars>(bencher, WIDE_SCALARS_SKIPPED_UNKNOWN_JSON);
}

#[divan::bench]
fn wide_scalars_skipped_unknown_weavy_jit_reused_plan(bencher: Bencher) {
    bench_weavy_jit_reused_plan::<WideScalars>(bencher, WIDE_SCALARS_SKIPPED_UNKNOWN_JSON);
}

#[divan::bench]
fn wide_scalars_skipped_unknown_serde_json(bencher: Bencher) {
    bench_serde_json::<WideScalars>(bencher, WIDE_SCALARS_SKIPPED_UNKNOWN_JSON);
}

// =============================================================================
// Benchmarks - Vec<WideScalars> (wide scalar struct list)
// =============================================================================

#[divan::bench]
fn wide_scalars_list_weavy_reused_plan(bencher: Bencher) {
    bench_weavy_reused_plan::<Vec<WideScalars>>(bencher, WIDE_SCALARS_LIST_JSON);
}

#[divan::bench]
fn wide_scalars_list_weavy_jit_reused_plan(bencher: Bencher) {
    bench_weavy_jit_reused_plan::<Vec<WideScalars>>(bencher, WIDE_SCALARS_LIST_JSON);
}

#[divan::bench]
fn wide_scalars_list_serde_json(bencher: Bencher) {
    bench_serde_json::<Vec<WideScalars>>(bencher, WIDE_SCALARS_LIST_JSON);
}

#[divan::bench]
fn wide_scalars_list_out_of_order_weavy_reused_plan(bencher: Bencher) {
    bench_weavy_reused_plan::<Vec<WideScalars>>(bencher, WIDE_SCALARS_LIST_OUT_OF_ORDER_JSON);
}

#[divan::bench]
fn wide_scalars_list_out_of_order_weavy_jit_reused_plan(bencher: Bencher) {
    bench_weavy_jit_reused_plan::<Vec<WideScalars>>(bencher, WIDE_SCALARS_LIST_OUT_OF_ORDER_JSON);
}

#[divan::bench]
fn wide_scalars_list_out_of_order_serde_json(bencher: Bencher) {
    bench_serde_json::<Vec<WideScalars>>(bencher, WIDE_SCALARS_LIST_OUT_OF_ORDER_JSON);
}

#[divan::bench]
fn wide_scalars_list_skipped_unknown_weavy_reused_plan(bencher: Bencher) {
    bench_weavy_reused_plan::<Vec<WideScalars>>(bencher, WIDE_SCALARS_LIST_SKIPPED_UNKNOWN_JSON);
}

#[divan::bench]
fn wide_scalars_list_skipped_unknown_weavy_jit_reused_plan(bencher: Bencher) {
    bench_weavy_jit_reused_plan::<Vec<WideScalars>>(
        bencher,
        WIDE_SCALARS_LIST_SKIPPED_UNKNOWN_JSON,
    );
}

#[divan::bench]
fn wide_scalars_list_skipped_unknown_serde_json(bencher: Bencher) {
    bench_serde_json::<Vec<WideScalars>>(bencher, WIDE_SCALARS_LIST_SKIPPED_UNKNOWN_JSON);
}

// =============================================================================
// Benchmarks - Missing scalar fields with defaults
// =============================================================================

#[divan::bench]
fn default_scalars_missing_weavy_reused_plan(bencher: Bencher) {
    bench_weavy_reused_plan::<DefaultScalars>(bencher, DEFAULT_SCALARS_MISSING_JSON);
}

#[divan::bench]
fn default_scalars_missing_weavy_jit_reused_plan(bencher: Bencher) {
    bench_weavy_jit_reused_plan::<DefaultScalars>(bencher, DEFAULT_SCALARS_MISSING_JSON);
}

#[divan::bench]
fn default_scalars_missing_serde_json(bencher: Bencher) {
    bench_serde_json::<DefaultScalars>(bencher, DEFAULT_SCALARS_MISSING_JSON);
}

// =============================================================================
// Benchmarks - tagged enums
// =============================================================================

#[divan::bench]
fn internal_tagged_reused_typeplan(bencher: Bencher) {
    bench_reused_typeplan_observed::<InternalTaggedEvent>(bencher, INTERNAL_TAGGED_JSON, |value| {
        value.checksum()
    });
}

#[divan::bench]
fn internal_tagged_weavy_reused_plan(bencher: Bencher) {
    bench_weavy_reused_plan_observed::<InternalTaggedEvent>(
        bencher,
        INTERNAL_TAGGED_JSON,
        |value| value.checksum(),
    );
}

#[divan::bench]
fn internal_tagged_weavy_jit_reused_plan(bencher: Bencher) {
    bench_weavy_jit_reused_plan_observed::<InternalTaggedEvent>(
        bencher,
        INTERNAL_TAGGED_JSON,
        |value| value.checksum(),
    );
}

#[divan::bench]
fn internal_tagged_serde_json(bencher: Bencher) {
    bench_serde_json_observed::<InternalTaggedEvent>(bencher, INTERNAL_TAGGED_JSON, |value| {
        value.checksum()
    });
}

#[divan::bench]
fn adjacent_tagged_reused_typeplan(bencher: Bencher) {
    bench_reused_typeplan_observed::<AdjacentTaggedEvent>(bencher, ADJACENT_TAGGED_JSON, |value| {
        value.checksum()
    });
}

#[divan::bench]
fn adjacent_tagged_weavy_reused_plan(bencher: Bencher) {
    bench_weavy_reused_plan_observed::<AdjacentTaggedEvent>(
        bencher,
        ADJACENT_TAGGED_JSON,
        |value| value.checksum(),
    );
}

#[divan::bench]
fn adjacent_tagged_weavy_jit_reused_plan(bencher: Bencher) {
    bench_weavy_jit_reused_plan_observed::<AdjacentTaggedEvent>(
        bencher,
        ADJACENT_TAGGED_JSON,
        |value| value.checksum(),
    );
}

#[divan::bench]
fn adjacent_tagged_serde_json(bencher: Bencher) {
    bench_serde_json_observed::<AdjacentTaggedEvent>(bencher, ADJACENT_TAGGED_JSON, |value| {
        value.checksum()
    });
}

// =============================================================================
// Benchmarks - untagged enums
// =============================================================================

#[divan::bench]
fn untagged_event_reused_typeplan(bencher: Bencher) {
    bench_reused_typeplan_observed::<UntaggedEvent>(bencher, UNTAGGED_EVENT_JSON, |value| {
        value.checksum()
    });
}

#[divan::bench]
fn untagged_event_weavy_reused_plan(bencher: Bencher) {
    bench_weavy_reused_plan_observed::<UntaggedEvent>(bencher, UNTAGGED_EVENT_JSON, |value| {
        value.checksum()
    });
}

#[divan::bench]
fn untagged_event_weavy_jit_reused_plan(bencher: Bencher) {
    bench_weavy_jit_reused_plan_observed::<UntaggedEvent>(bencher, UNTAGGED_EVENT_JSON, |value| {
        value.checksum()
    });
}

#[divan::bench]
fn untagged_event_serde_json(bencher: Bencher) {
    bench_serde_json_observed::<UntaggedEvent>(bencher, UNTAGGED_EVENT_JSON, |value| {
        value.checksum()
    });
}

// =============================================================================
// Batch benchmarks - 1000 iterations to amplify TypePlan overhead
// =============================================================================

/// 1000 deserializations with fresh TypePlan each time
#[divan::bench]
fn batch_1000_fresh_typeplan(bencher: Bencher) {
    let json = PERSON_JSON;
    bencher.bench(|| {
        for _ in 0..1000 {
            let result: Person = facet_json::from_str(black_box(json)).unwrap();
            black_box(result);
        }
    });
}

/// 1000 deserializations reusing the same TypePlan
#[divan::bench]
fn batch_1000_reused_typeplan(bencher: Bencher) {
    use facet_format::FormatDeserializer;

    let json = PERSON_JSON;
    let plan = TypePlan::<Person>::build().unwrap();

    bencher.bench(|| {
        for _ in 0..1000 {
            let partial = plan.partial_owned().unwrap();
            let mut parser = JsonParser::<true>::new(black_box(json.as_bytes()));
            let mut de = FormatDeserializer::new_owned(&mut parser);
            let partial = de
                .deserialize_into(partial, MetaSource::FromEvents)
                .unwrap();
            let result: Person = partial.build().unwrap().materialize().unwrap();
            black_box(result);
        }
    });
}

/// 1000 deserializations through a reused Weavy JSON plan
#[divan::bench]
fn batch_1000_weavy_reused_plan(bencher: Bencher) {
    let json = PERSON_JSON;
    let plan = facet_json::JsonWeavyPlan::<Person>::build().unwrap();

    bencher.bench(|| {
        for _ in 0..1000 {
            let result: Person = plan.from_str(black_box(json)).unwrap();
            black_box(result);
        }
    });
}

/// 1000 deserializations through serde_json
#[divan::bench]
fn batch_1000_serde_json(bencher: Bencher) {
    let json = PERSON_JSON;

    bencher.bench(|| {
        for _ in 0..1000 {
            let result: Person = serde_json::from_str(black_box(json)).unwrap();
            black_box(result);
        }
    });
}

/// 1000 deserializations through serde_json from bytes
#[divan::bench]
fn batch_1000_serde_json_from_slice(bencher: Bencher) {
    let json = PERSON_JSON.as_bytes();

    bencher.bench(|| {
        for _ in 0..1000 {
            let result: Person = serde_json::from_slice(black_box(json)).unwrap();
            black_box(result);
        }
    });
}
