//! Benchmark for facet-solver with pathological cartesian product cases.
//!
//! - 2 enums × 4 variants each = 16 configs
//! - 3 enums × 4 variants each = 64 configs
//! - 4 enums × 4 variants each = 256 configs
//! - 5 enums × 4 variants each = 1024 configs

use divan::{Bencher, black_box};
use facet::Facet;
use facet_solver::{Schema, Solver};

fn main() {
    divan::main();
}

// Helper enums with 4 variants each

#[allow(dead_code)]
#[derive(Facet)]
#[repr(u8)]
enum Enum1 {
    A1 { field_a1: String },
    B1 { field_b1: String },
    C1 { field_c1: String },
    D1 { field_d1: String },
}

#[allow(dead_code)]
#[derive(Facet)]
#[repr(u8)]
enum Enum2 {
    A2 { field_a2: String },
    B2 { field_b2: String },
    C2 { field_c2: String },
    D2 { field_d2: String },
}

#[allow(dead_code)]
#[derive(Facet)]
#[repr(u8)]
enum Enum3 {
    A3 { field_a3: String },
    B3 { field_b3: String },
    C3 { field_c3: String },
    D3 { field_d3: String },
}

#[allow(dead_code)]
#[derive(Facet)]
#[repr(u8)]
enum Enum4 {
    A4 { field_a4: String },
    B4 { field_b4: String },
    C4 { field_c4: String },
    D4 { field_d4: String },
}

#[allow(dead_code)]
#[derive(Facet)]
#[repr(u8)]
enum Enum5 {
    A5 { field_a5: String },
    B5 { field_b5: String },
    C5 { field_c5: String },
    D5 { field_d5: String },
}

// Types with increasing numbers of flattened enums

#[derive(Facet)]
struct TwoEnums {
    #[facet(flatten)]
    e1: Enum1,
    #[facet(flatten)]
    e2: Enum2,
}

#[derive(Facet)]
struct ThreeEnums {
    #[facet(flatten)]
    e1: Enum1,
    #[facet(flatten)]
    e2: Enum2,
    #[facet(flatten)]
    e3: Enum3,
}

#[derive(Facet)]
struct FourEnums {
    #[facet(flatten)]
    e1: Enum1,
    #[facet(flatten)]
    e2: Enum2,
    #[facet(flatten)]
    e3: Enum3,
    #[facet(flatten)]
    e4: Enum4,
}

#[derive(Facet)]
struct FiveEnums {
    #[facet(flatten)]
    e1: Enum1,
    #[facet(flatten)]
    e2: Enum2,
    #[facet(flatten)]
    e3: Enum3,
    #[facet(flatten)]
    e4: Enum4,
    #[facet(flatten)]
    e5: Enum5,
}

// Schema building benchmarks

#[divan::bench(args = [16, 64, 256, 1024])]
fn schema_build(bencher: Bencher, configs: u32) {
    match configs {
        16 => bencher.bench(|| Schema::build(TwoEnums::SHAPE)),
        64 => bencher.bench(|| Schema::build(ThreeEnums::SHAPE)),
        256 => bencher.bench(|| Schema::build(FourEnums::SHAPE)),
        1024 => bencher.bench(|| Schema::build(FiveEnums::SHAPE)),
        _ => unreachable!(),
    }
}

// Solver benchmarks

#[divan::bench(args = [16, 64, 256, 1024])]
fn incremental_solver(bencher: Bencher, configs: u32) {
    match configs {
        16 => {
            let schema = Schema::build(TwoEnums::SHAPE).unwrap();
            bencher.bench(|| {
                let mut solver = Solver::new(black_box(&schema));
                solver.see_key(black_box("field_a1"));
                solver.see_key(black_box("field_a2"));
            });
        }
        64 => {
            let schema = Schema::build(ThreeEnums::SHAPE).unwrap();
            bencher.bench(|| {
                let mut solver = Solver::new(black_box(&schema));
                solver.see_key(black_box("field_a1"));
                solver.see_key(black_box("field_a2"));
                solver.see_key(black_box("field_a3"));
            });
        }
        256 => {
            let schema = Schema::build(FourEnums::SHAPE).unwrap();
            bencher.bench(|| {
                let mut solver = Solver::new(black_box(&schema));
                solver.see_key(black_box("field_a1"));
                solver.see_key(black_box("field_a2"));
                solver.see_key(black_box("field_a3"));
                solver.see_key(black_box("field_a4"));
            });
        }
        1024 => {
            let schema = Schema::build(FiveEnums::SHAPE).unwrap();
            bencher.bench(|| {
                let mut solver = Solver::new(black_box(&schema));
                solver.see_key(black_box("field_a1"));
                solver.see_key(black_box("field_a2"));
                solver.see_key(black_box("field_a3"));
                solver.see_key(black_box("field_a4"));
                solver.see_key(black_box("field_a5"));
            });
        }
        _ => unreachable!(),
    }
}
