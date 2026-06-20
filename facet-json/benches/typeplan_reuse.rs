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

// =============================================================================
// Test data
// =============================================================================

const POINT_JSON: &str = r#"{"x": 10, "y": 20}"#;

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

/// VM path, including TypePlan build and lowering
#[divan::bench]
fn point_vm(bencher: Bencher) {
    let json = POINT_JSON;
    bencher.bench(|| {
        let result: Point = black_box(facet_json::from_str_vm(black_box(json)).unwrap());
        black_box(result)
    });
}

/// VM path with explicit plan reuse
#[divan::bench]
fn point_reused_vm_plan(bencher: Bencher) {
    let json = POINT_JSON;
    let plan = facet_json::JsonVmPlan::<Point>::build().unwrap();

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

/// VM path, including TypePlan build and lowering
#[divan::bench]
fn person_vm(bencher: Bencher) {
    let json = PERSON_JSON;
    bencher.bench(|| {
        let result: Person = black_box(facet_json::from_str_vm(black_box(json)).unwrap());
        black_box(result)
    });
}

/// VM path with explicit plan reuse
#[divan::bench]
fn person_reused_vm_plan(bencher: Bencher) {
    let json = PERSON_JSON;
    let plan = facet_json::JsonVmPlan::<Person>::build().unwrap();

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

/// VM path, including TypePlan build and lowering
#[divan::bench]
fn company_vm(bencher: Bencher) {
    let json = COMPANY_JSON;
    bencher.bench(|| {
        let result: Company = black_box(facet_json::from_str_vm(black_box(json)).unwrap());
        black_box(result)
    });
}

/// VM path with explicit plan reuse
#[divan::bench]
fn company_reused_vm_plan(bencher: Bencher) {
    let json = COMPANY_JSON;
    let plan = facet_json::JsonVmPlan::<Company>::build().unwrap();

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

/// 1000 deserializations through the VM path
#[divan::bench]
fn batch_1000_vm(bencher: Bencher) {
    let json = PERSON_JSON;
    bencher.bench(|| {
        for _ in 0..1000 {
            let result: Person = facet_json::from_str_vm(black_box(json)).unwrap();
            black_box(result);
        }
    });
}

/// 1000 deserializations reusing the same VM plan
#[divan::bench]
fn batch_1000_reused_vm_plan(bencher: Bencher) {
    let json = PERSON_JSON;
    let plan = facet_json::JsonVmPlan::<Person>::build().unwrap();

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
