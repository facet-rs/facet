use criterion::{Criterion, black_box, criterion_group, criterion_main};
use facet::Facet;
use facet_pretty::{FacetPretty, PrettyPrinter};

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
    Tuple(i32, bool),
    Struct { a: u8, b: String },
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

// --- Benchmark Functions ---

fn bench_simple_struct(c: &mut Criterion) {
    let facet_val = Simple {
        a: 123,
        b: "hello world".to_string(),
    };
    let facet_printer = PrettyPrinter::new().with_colors(false);

    let mut group = c.benchmark_group("Simple Struct Formatting");

    group.bench_function("facet-pretty", |b| {
        b.iter(|| {
            black_box(facet_printer.format(black_box(&facet_val)));
        })
    });

    group.bench_function("derive(Debug) + pretty {:#?}", |b| {
        b.iter(|| {
            black_box(format!("{:#?}", black_box(&facet_val)));
        })
    });

    group.finish();
}

fn bench_nested_struct(c: &mut Criterion) {
    let facet_val = Outer {
        inner: Inner { x: 3.14159 },
        name: "outer".to_string(),
        count: 42,
    };
    let facet_printer = PrettyPrinter::new().with_colors(false);

    let mut group = c.benchmark_group("Nested Struct Formatting");

    group.bench_function("facet-pretty", |b| {
        b.iter(|| {
            black_box(facet_printer.format(black_box(&facet_val)));
        })
    });

    group.bench_function("derive(Debug) + pretty {:#?}", |b| {
        b.iter(|| {
            black_box(format!("{:#?}", black_box(&facet_val)));
        })
    });

    group.finish();
}

fn bench_enum(c: &mut Criterion) {
    let facet_unit = Enum::Unit;
    let facet_tuple = Enum::Tuple(10, true);
    let facet_struct = Enum::Struct {
        a: 5,
        b: "enum struct".to_string(),
    };

    let facet_printer = PrettyPrinter::new().with_colors(false);

    let mut group = c.benchmark_group("Enum Formatting");

    group.bench_function("facet-pretty (Unit)", |b| {
        b.iter(|| {
            black_box(facet_printer.format(black_box(&facet_unit)));
        })
    });
    group.bench_function("derive(Debug) + pretty {:#?} (Unit)", |b| {
        b.iter(|| {
            black_box(format!("{:#?}", black_box(&facet_unit)));
        })
    });

    group.bench_function("facet-pretty (Tuple)", |b| {
        b.iter(|| {
            black_box(facet_printer.format(black_box(&facet_tuple)));
        })
    });
    group.bench_function("derive(Debug) + pretty {:#?} (Tuple)", |b| {
        b.iter(|| {
            black_box(format!("{:#?}", black_box(&facet_tuple)));
        })
    });

    group.bench_function("facet-pretty (Struct)", |b| {
        b.iter(|| {
            black_box(facet_printer.format(black_box(&facet_struct)));
        })
    });
    group.bench_function("derive(Debug) + pretty {:#?} (Struct)", |b| {
        b.iter(|| {
            black_box(format!("{:#?}", black_box(&facet_struct)));
        })
    });

    group.finish();
}

fn bench_complex_struct(c: &mut Criterion) {
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

    let mut group = c.benchmark_group("Complex Struct Formatting");

    group.bench_function("facet-pretty", |b| {
        b.iter(|| {
            black_box(facet_printer.format(black_box(&facet_val)));
        })
    });

    group.bench_function("derive(Debug) + pretty {:#?}", |b| {
        b.iter(|| {
            black_box(format!("{:#?}", black_box(&facet_val)));
        })
    });

    group.finish();
}

// --- Criterion Setup ---
criterion_group!(
    benches,
    bench_simple_struct,
    bench_nested_struct,
    bench_enum,
    bench_complex_struct,
);
criterion_main!(benches);
