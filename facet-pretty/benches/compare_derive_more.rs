#![allow(clippy::approx_constant)]

use divan::{Bencher, black_box};
use facet::Facet;
use facet_pretty::PrettyPrinter;

// --- Data Structures for Benchmarking ---

// --- Simple Case ---
#[derive(Facet, Debug, Clone)]
struct Simple {
    a: u32,
    b: String,
}

// --- Nested Case ---
#[derive(Facet, Debug, Clone)]
struct Inner {
    x: f64,
}

#[derive(Facet, Debug, Clone)]
struct Outer {
    inner: Inner,
    name: String,
    count: usize,
}

// --- Enum Case ---
#[derive(Facet, Debug, Clone)]
#[repr(u8)]
enum Enum {
    Unit,
    #[allow(dead_code)]
    Tuple(i32, bool),
    #[allow(dead_code)]
    Struct {
        a: u8,
        b: String,
    },
}

// --- Complex Case (Deeper Nesting, Option, Vec) ---
#[derive(Facet, Debug, Clone)]
struct ComplexInner {
    id: u64,
    flag: Option<bool>,
}

#[derive(Facet, Debug, Clone)]
struct ComplexOuter {
    name: String,
    items: Vec<ComplexInner>,
    maybe_value: Option<i128>,
    level: u32,
}

// --- Simple Struct Benchmarks ---

#[divan::bench(name = "Simple Struct - facet-pretty")]
fn bench_simple_struct_facet(bencher: Bencher) {
    let facet_val = Simple {
        a: 123,
        b: "hello world".to_string(),
    };
    let facet_printer = PrettyPrinter::new().with_colors(false);

    bencher.bench(|| black_box(facet_printer.format(black_box(&facet_val))));
}

#[divan::bench(name = "Simple Struct - derive(Debug) + pretty {:#?}")]
fn bench_simple_struct_debug(bencher: Bencher) {
    let facet_val = Simple {
        a: 123,
        b: "hello world".to_string(),
    };

    bencher.bench(|| black_box(format!("{:#?}", black_box(&facet_val))));
}

// --- Nested Struct Benchmarks ---

#[divan::bench(name = "Nested Struct - facet-pretty")]
fn bench_nested_struct_facet(bencher: Bencher) {
    let facet_val = Outer {
        inner: Inner { x: 3.14159 },
        name: "outer".to_string(),
        count: 42,
    };
    let facet_printer = PrettyPrinter::new().with_colors(false);

    bencher.bench(|| black_box(facet_printer.format(black_box(&facet_val))));
}

#[divan::bench(name = "Nested Struct - derive(Debug) + pretty {:#?}")]
fn bench_nested_struct_debug(bencher: Bencher) {
    let facet_val = Outer {
        inner: Inner { x: 3.14159 },
        name: "outer".to_string(),
        count: 42,
    };

    bencher.bench(|| black_box(format!("{:#?}", black_box(&facet_val))));
}

// --- Enum Benchmarks ---

#[divan::bench(name = "Enum (Unit) - facet-pretty")]
fn bench_enum_unit_facet(bencher: Bencher) {
    let facet_unit = Enum::Unit;
    let facet_printer = PrettyPrinter::new().with_colors(false);

    bencher.bench(|| black_box(facet_printer.format(black_box(&facet_unit))));
}

#[divan::bench(name = "Enum (Unit) - derive(Debug) + pretty {:#?}")]
fn bench_enum_unit_debug(bencher: Bencher) {
    let facet_unit = Enum::Unit;

    bencher.bench(|| black_box(format!("{:#?}", black_box(&facet_unit))));
}

#[divan::bench(name = "Enum (Tuple) - facet-pretty")]
fn bench_enum_tuple_facet(bencher: Bencher) {
    let facet_tuple = Enum::Tuple(10, true);
    let facet_printer = PrettyPrinter::new().with_colors(false);

    bencher.bench(|| black_box(facet_printer.format(black_box(&facet_tuple))));
}

#[divan::bench(name = "Enum (Tuple) - derive(Debug) + pretty {:#?}")]
fn bench_enum_tuple_debug(bencher: Bencher) {
    let facet_tuple = Enum::Tuple(10, true);

    bencher.bench(|| black_box(format!("{:#?}", black_box(&facet_tuple))));
}

#[divan::bench(name = "Enum (Struct) - facet-pretty")]
fn bench_enum_struct_facet(bencher: Bencher) {
    let facet_struct = Enum::Struct {
        a: 5,
        b: "enum struct".to_string(),
    };
    let facet_printer = PrettyPrinter::new().with_colors(false);

    bencher.bench(|| black_box(facet_printer.format(black_box(&facet_struct))));
}

#[divan::bench(name = "Enum (Struct) - derive(Debug) + pretty {:#?}")]
fn bench_enum_struct_debug(bencher: Bencher) {
    let facet_struct = Enum::Struct {
        a: 5,
        b: "enum struct".to_string(),
    };

    bencher.bench(|| black_box(format!("{:#?}", black_box(&facet_struct))));
}

// --- Complex Struct Benchmarks ---

#[divan::bench(name = "Complex Struct - facet-pretty")]
fn bench_complex_struct_facet(bencher: Bencher) {
    let facet_val = ComplexOuter {
        name: "complex data structure".to_string(),
        items: vec![
            ComplexInner {
                id: 1,
                flag: Some(true),
            },
            ComplexInner { id: 2, flag: None },
            ComplexInner {
                id: 3,
                flag: Some(false),
            },
        ],
        maybe_value: Some(-1_000_000_000_000_000),
        level: 5,
    };
    let facet_printer = PrettyPrinter::new().with_colors(false);

    bencher.bench(|| black_box(facet_printer.format(black_box(&facet_val))));
}

#[divan::bench(name = "Complex Struct - derive(Debug) + pretty {:#?}")]
fn bench_complex_struct_debug(bencher: Bencher) {
    let facet_val = ComplexOuter {
        name: "complex data structure".to_string(),
        items: vec![
            ComplexInner {
                id: 1,
                flag: Some(true),
            },
            ComplexInner { id: 2, flag: None },
            ComplexInner {
                id: 3,
                flag: Some(false),
            },
        ],
        maybe_value: Some(-1_000_000_000_000_000),
        level: 5,
    };

    bencher.bench(|| black_box(format!("{:#?}", black_box(&facet_val))));
}

fn main() {
    divan::main();
}
