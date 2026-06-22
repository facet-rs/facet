use core::hash::{Hash, Hasher};

use divan::{Bencher, black_box};
use facet::Facet;
use facet_hash::HashPlan;
use facet_reflect::Peek;

fn main() {
    divan::main();
}

#[derive(Debug, Facet, Hash)]
struct Point {
    x: i32,
    y: i32,
}

#[derive(Debug, Facet, Hash)]
struct Person {
    name: String,
    age: u32,
    email: Option<String>,
    scores: Vec<i32>,
}

#[derive(Debug, Facet)]
struct FloatPoint {
    x: f64,
    y: f64,
    z: f64,
}

#[derive(Debug, Facet)]
struct Company {
    name: String,
    employees: Vec<Employee>,
    headquarters: Address,
}

#[derive(Debug, Facet)]
struct Employee {
    id: u64,
    name: String,
    department: String,
    salary: f64,
}

#[derive(Debug, Facet)]
struct Address {
    street: String,
    city: String,
    country: String,
    zip: String,
}

#[divan::bench]
fn point_native_hash(bencher: Bencher<'_, '_>) {
    let value = Point { x: 123, y: -456 };
    bencher.bench_local(|| {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        black_box(&value).hash(&mut hasher);
        black_box(hasher.finish())
    });
}

#[divan::bench]
fn point_value_plan_reused(bencher: Bencher<'_, '_>) {
    let plan = HashPlan::<Point>::build().unwrap();
    let value = Point { x: 123, y: -456 };
    bencher.bench_local(|| black_box(plan.hash64(black_box(&value)).unwrap()));
}

#[divan::bench]
fn point_value_plan_build_each_time(bencher: Bencher<'_, '_>) {
    let value = Point { x: 123, y: -456 };
    bencher.bench_local(|| black_box(facet_hash::hash64(black_box(&value)).unwrap()));
}

#[divan::bench]
fn point_structural_plan_reused(bencher: Bencher<'_, '_>) {
    let plan = HashPlan::<Point>::build_structural().unwrap();
    let value = Point { x: 123, y: -456 };
    bencher.bench_local(|| black_box(plan.hash64(black_box(&value)).unwrap()));
}

#[divan::bench]
fn point_peek_structural_hash(bencher: Bencher<'_, '_>) {
    let value = Point { x: 123, y: -456 };
    bencher.bench_local(|| {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        Peek::new(black_box(&value)).structural_hash(&mut hasher);
        black_box(hasher.finish())
    });
}

#[divan::bench]
fn person_native_hash(bencher: Bencher<'_, '_>) {
    let value = person();
    bencher.bench_local(|| {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        black_box(&value).hash(&mut hasher);
        black_box(hasher.finish())
    });
}

#[divan::bench]
fn person_value_plan_reused(bencher: Bencher<'_, '_>) {
    let plan = HashPlan::<Person>::build().unwrap();
    let value = person();
    bencher.bench_local(|| black_box(plan.hash64(black_box(&value)).unwrap()));
}

#[divan::bench]
fn person_structural_plan_reused(bencher: Bencher<'_, '_>) {
    let plan = HashPlan::<Person>::build_structural().unwrap();
    let value = person();
    bencher.bench_local(|| black_box(plan.hash64(black_box(&value)).unwrap()));
}

#[divan::bench]
fn person_peek_structural_hash(bencher: Bencher<'_, '_>) {
    let value = person();
    bencher.bench_local(|| {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        Peek::new(black_box(&value)).structural_hash(&mut hasher);
        black_box(hasher.finish())
    });
}

#[divan::bench]
fn float_value_plan_reused(bencher: Bencher<'_, '_>) {
    let plan = HashPlan::<FloatPoint>::build().unwrap();
    let value = FloatPoint {
        x: 1.25,
        y: -9.5,
        z: f64::NAN,
    };
    bencher.bench_local(|| black_box(plan.hash64(black_box(&value)).unwrap()));
}

#[divan::bench]
fn float_peek_structural_hash(bencher: Bencher<'_, '_>) {
    let value = FloatPoint {
        x: 1.25,
        y: -9.5,
        z: f64::NAN,
    };
    bencher.bench_local(|| {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        Peek::new(black_box(&value)).structural_hash(&mut hasher);
        black_box(hasher.finish())
    });
}

#[divan::bench]
fn company_value_plan_reused(bencher: Bencher<'_, '_>) {
    let plan = HashPlan::<Company>::build().unwrap();
    let value = company();
    bencher.bench_local(|| black_box(plan.hash64(black_box(&value)).unwrap()));
}

#[divan::bench]
fn company_structural_plan_reused(bencher: Bencher<'_, '_>) {
    let plan = HashPlan::<Company>::build_structural().unwrap();
    let value = company();
    bencher.bench_local(|| black_box(plan.hash64(black_box(&value)).unwrap()));
}

#[divan::bench]
fn company_peek_structural_hash(bencher: Bencher<'_, '_>) {
    let value = company();
    bencher.bench_local(|| {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        Peek::new(black_box(&value)).structural_hash(&mut hasher);
        black_box(hasher.finish())
    });
}

fn person() -> Person {
    Person {
        name: "Ada Lovelace".to_owned(),
        age: 36,
        email: Some("ada@example.test".to_owned()),
        scores: vec![1, 1, 2, 3, 5, 8, 13],
    }
}

fn company() -> Company {
    Company {
        name: "Analytical Engines Ltd".to_owned(),
        employees: vec![
            Employee {
                id: 1,
                name: "Ada".to_owned(),
                department: "math".to_owned(),
                salary: 123_456.75,
            },
            Employee {
                id: 2,
                name: "Grace".to_owned(),
                department: "compiler".to_owned(),
                salary: 234_567.25,
            },
        ],
        headquarters: Address {
            street: "1 Difference Lane".to_owned(),
            city: "London".to_owned(),
            country: "UK".to_owned(),
            zip: "N1".to_owned(),
        },
    }
}
