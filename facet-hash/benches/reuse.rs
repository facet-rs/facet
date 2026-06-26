use core::hash::{Hash, Hasher};

use divan::{Bencher, black_box};
use facet::Facet;
use facet_hash::{EqualityPlan, HashPlan};
#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
use facet_hash::{NativeEqualityPlan, NativeHashPlan};
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
struct MixedScalarRuns {
    a: u32,
    point: Point,
    b: u32,
    c: u32,
}

#[derive(Debug, Facet, Hash)]
struct PointArray {
    points: [Point; 2],
    tail: i16,
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

#[derive(Debug, Facet, Hash)]
struct HashCompany {
    name: String,
    employees: Vec<HashEmployee>,
    headquarters: HashAddress,
}

#[derive(Debug, Facet, Hash)]
struct HashEmployee {
    id: u64,
    name: String,
    department: String,
    salary_cents: u64,
}

#[derive(Debug, Facet, Hash)]
struct HashAddress {
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
fn point_equality_plan_equal(bencher: Bencher<'_, '_>) {
    let plan = EqualityPlan::<Point>::build().unwrap();
    let left = Point { x: 123, y: -456 };
    let right = Point { x: 123, y: -456 };
    bencher.bench_local(|| black_box(plan.eq(black_box(&left), black_box(&right)).unwrap()));
}

#[divan::bench]
fn point_equality_plan_different(bencher: Bencher<'_, '_>) {
    let plan = EqualityPlan::<Point>::build().unwrap();
    let left = Point { x: 123, y: -456 };
    let right = Point { x: 123, y: -457 };
    bencher.bench_local(|| black_box(plan.eq(black_box(&left), black_box(&right)).unwrap()));
}

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
#[divan::bench]
fn point_value_native_jit(bencher: Bencher<'_, '_>) {
    let plan = NativeHashPlan::<Point>::build().unwrap();
    let value = Point { x: 123, y: -456 };
    bencher.bench_local(|| black_box(plan.hash64(black_box(&value)).unwrap()));
}

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
#[divan::bench]
fn point_equality_native_jit_equal(bencher: Bencher<'_, '_>) {
    let plan = NativeEqualityPlan::<Point>::build().unwrap();
    let left = Point { x: 123, y: -456 };
    let right = Point { x: 123, y: -456 };
    bencher.bench_local(|| black_box(plan.eq(black_box(&left), black_box(&right)).unwrap()));
}

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
#[divan::bench]
fn point_equality_native_jit_different(bencher: Bencher<'_, '_>) {
    let plan = NativeEqualityPlan::<Point>::build().unwrap();
    let left = Point { x: 123, y: -456 };
    let right = Point { x: 123, y: -457 };
    bencher.bench_local(|| black_box(plan.eq(black_box(&left), black_box(&right)).unwrap()));
}

#[divan::bench]
fn byte_vec_native_hash(bencher: Bencher<'_, '_>) {
    let value = byte_vec();
    bencher.bench_local(|| {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        black_box(&value).hash(&mut hasher);
        black_box(hasher.finish())
    });
}

#[divan::bench]
fn byte_vec_fnv1a64_hash(bencher: Bencher<'_, '_>) {
    let value = byte_vec();
    bencher.bench_local(|| black_box(facet_hash::hash_bytes_fnv1a64(black_box(&value))));
}

#[divan::bench]
fn byte_vec_value_plan_reused(bencher: Bencher<'_, '_>) {
    let plan = HashPlan::<Vec<u8>>::build().unwrap();
    let value = byte_vec();
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
fn mixed_native_hash(bencher: Bencher<'_, '_>) {
    let value = mixed_scalar_runs();
    bencher.bench_local(|| {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        black_box(&value).hash(&mut hasher);
        black_box(hasher.finish())
    });
}

#[divan::bench]
fn mixed_value_plan_reused(bencher: Bencher<'_, '_>) {
    let plan = HashPlan::<MixedScalarRuns>::build().unwrap();
    let value = mixed_scalar_runs();
    bencher.bench_local(|| black_box(plan.hash64(black_box(&value)).unwrap()));
}

#[divan::bench]
fn mixed_equality_plan_equal(bencher: Bencher<'_, '_>) {
    let plan = EqualityPlan::<MixedScalarRuns>::build().unwrap();
    let left = mixed_scalar_runs();
    let right = mixed_scalar_runs();
    bencher.bench_local(|| black_box(plan.eq(black_box(&left), black_box(&right)).unwrap()));
}

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
#[divan::bench]
fn mixed_value_native_jit(bencher: Bencher<'_, '_>) {
    let plan = NativeHashPlan::<MixedScalarRuns>::build().unwrap();
    let value = mixed_scalar_runs();
    bencher.bench_local(|| black_box(plan.hash64(black_box(&value)).unwrap()));
}

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
#[divan::bench]
fn mixed_equality_native_jit_equal(bencher: Bencher<'_, '_>) {
    let plan = NativeEqualityPlan::<MixedScalarRuns>::build().unwrap();
    let left = mixed_scalar_runs();
    let right = mixed_scalar_runs();
    bencher.bench_local(|| black_box(plan.eq(black_box(&left), black_box(&right)).unwrap()));
}

#[divan::bench]
fn point_array_native_hash(bencher: Bencher<'_, '_>) {
    let value = point_array();
    bencher.bench_local(|| {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        black_box(&value).hash(&mut hasher);
        black_box(hasher.finish())
    });
}

#[divan::bench]
fn point_array_value_plan_reused(bencher: Bencher<'_, '_>) {
    let plan = HashPlan::<PointArray>::build().unwrap();
    let value = point_array();
    bencher.bench_local(|| black_box(plan.hash64(black_box(&value)).unwrap()));
}

#[divan::bench]
fn point_array_equality_plan_equal(bencher: Bencher<'_, '_>) {
    let plan = EqualityPlan::<PointArray>::build().unwrap();
    let left = point_array();
    let right = point_array();
    bencher.bench_local(|| black_box(plan.eq(black_box(&left), black_box(&right)).unwrap()));
}

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
#[divan::bench]
fn point_array_value_native_jit(bencher: Bencher<'_, '_>) {
    let plan = NativeHashPlan::<PointArray>::build().unwrap();
    let value = point_array();
    bencher.bench_local(|| black_box(plan.hash64(black_box(&value)).unwrap()));
}

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
#[divan::bench]
fn point_array_equality_native_jit_equal(bencher: Bencher<'_, '_>) {
    let plan = NativeEqualityPlan::<PointArray>::build().unwrap();
    let left = point_array();
    let right = point_array();
    bencher.bench_local(|| black_box(plan.eq(black_box(&left), black_box(&right)).unwrap()));
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
fn float_equality_plan_equal(bencher: Bencher<'_, '_>) {
    let plan = EqualityPlan::<FloatPoint>::build().unwrap();
    let left = FloatPoint {
        x: 1.25,
        y: -9.5,
        z: f64::NAN,
    };
    let right = FloatPoint {
        x: 1.25,
        y: -9.5,
        z: f64::from_bits(f64::NAN.to_bits()),
    };
    bencher.bench_local(|| black_box(plan.eq(black_box(&left), black_box(&right)).unwrap()));
}

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
#[divan::bench]
fn float_value_native_jit(bencher: Bencher<'_, '_>) {
    let plan = NativeHashPlan::<FloatPoint>::build().unwrap();
    let value = FloatPoint {
        x: 1.25,
        y: -9.5,
        z: f64::NAN,
    };
    bencher.bench_local(|| black_box(plan.hash64(black_box(&value)).unwrap()));
}

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
#[divan::bench]
fn float_equality_native_jit_equal(bencher: Bencher<'_, '_>) {
    let plan = NativeEqualityPlan::<FloatPoint>::build().unwrap();
    let left = FloatPoint {
        x: 1.25,
        y: -9.5,
        z: f64::NAN,
    };
    let right = FloatPoint {
        x: 1.25,
        y: -9.5,
        z: f64::from_bits(f64::NAN.to_bits()),
    };
    bencher.bench_local(|| black_box(plan.eq(black_box(&left), black_box(&right)).unwrap()));
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

#[divan::bench]
fn hash_company_native_hash(bencher: Bencher<'_, '_>) {
    let value = hash_company();
    bencher.bench_local(|| {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        black_box(&value).hash(&mut hasher);
        black_box(hasher.finish())
    });
}

#[divan::bench]
fn hash_company_value_plan_reused(bencher: Bencher<'_, '_>) {
    let plan = HashPlan::<HashCompany>::build().unwrap();
    let value = hash_company();
    bencher.bench_local(|| black_box(plan.hash64(black_box(&value)).unwrap()));
}

#[divan::bench]
fn hash_company_structural_plan_reused(bencher: Bencher<'_, '_>) {
    let plan = HashPlan::<HashCompany>::build_structural().unwrap();
    let value = hash_company();
    bencher.bench_local(|| black_box(plan.hash64(black_box(&value)).unwrap()));
}

#[divan::bench]
fn hash_company_peek_structural_hash(bencher: Bencher<'_, '_>) {
    let value = hash_company();
    bencher.bench_local(|| {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        Peek::new(black_box(&value)).structural_hash(&mut hasher);
        black_box(hasher.finish())
    });
}

fn mixed_scalar_runs() -> MixedScalarRuns {
    MixedScalarRuns {
        a: 1,
        point: Point { x: 2, y: 3 },
        b: 4,
        c: 5,
    }
}

fn point_array() -> PointArray {
    PointArray {
        points: [Point { x: 1, y: 2 }, Point { x: 3, y: 4 }],
        tail: -5,
    }
}

fn byte_vec() -> Vec<u8> {
    (0..4096)
        .map(|index| (index as u8).wrapping_mul(31))
        .collect()
}

fn person() -> Person {
    Person {
        name: "Ada Lovelace".to_owned(),
        age: 36,
        email: Some("ada@example.test".to_owned()),
        scores: vec![1, 1, 2, 3, 5, 8, 13],
    }
}

fn hash_company() -> HashCompany {
    HashCompany {
        name: "Analytical Engines Ltd".to_owned(),
        employees: vec![
            HashEmployee {
                id: 1,
                name: "Ada".to_owned(),
                department: "math".to_owned(),
                salary_cents: 12_345_675,
            },
            HashEmployee {
                id: 2,
                name: "Grace".to_owned(),
                department: "compiler".to_owned(),
                salary_cents: 23_456_725,
            },
        ],
        headquarters: HashAddress {
            street: "1 Difference Lane".to_owned(),
            city: "London".to_owned(),
            country: "UK".to_owned(),
            zip: "N1".to_owned(),
        },
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
