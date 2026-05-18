//! A gallery of diff renderings, shown through the same layout renderer
//! that `assert_same!` now uses (`DiffReport::render_ansi_rust`).
//!
//! Run with: `cargo run --example gallery`

use std::collections::{BTreeMap, HashMap, HashSet};

use facet::Facet;
use rediff::{check_same_report, check_sameish_report};

/// Print a titled diff exactly as an `assert_same!` failure would show it.
fn show<'a, T: Facet<'a>>(title: &str, a: &'a T, b: &'a T) {
    println!("\n\x1b[1m── {title} ──\x1b[0m");
    let report = check_same_report(a, b);
    match report.diff() {
        Some(d) => println!("{}", d.render_ansi_rust()),
        None => println!("(structurally same)"),
    }
}

/// Same, for two *different* types (the `assert_sameish!` path).
fn show_ish<'a, T: Facet<'a>, U: Facet<'a>>(title: &str, a: &'a T, b: &'a U) {
    println!("\n\x1b[1m── {title} ──\x1b[0m");
    let report = check_sameish_report(a, b);
    match report.diff() {
        Some(d) => println!("{}", d.render_ansi_rust()),
        None => println!("(structurally same)"),
    }
}

fn main() {
    scalars();
    nested();
    enums();
    options();
    sequences();
    maps_and_sets();
    tuples();
    collapsing();
    cross_type();
    strings();
}

// ───────────────────────────── scalars ─────────────────────────────

fn scalars() {
    #[derive(Facet)]
    struct Server {
        host: String,
        port: u16,
        tls: bool,
        weight: f64,
    }

    show(
        "one field changed",
        &Server {
            host: "rustweek.org".into(),
            port: 443,
            tls: true,
            weight: 1.0,
        },
        &Server {
            host: "rustweek.org".into(),
            port: 8443,
            tls: true,
            weight: 1.0,
        },
    );

    show(
        "several fields changed",
        &Server {
            host: "old.example".into(),
            port: 80,
            tls: false,
            weight: 1.0,
        },
        &Server {
            host: "new.example".into(),
            port: 443,
            tls: true,
            weight: 2.5,
        },
    );
}

// ───────────────────────────── nested ──────────────────────────────

fn nested() {
    #[derive(Facet)]
    struct Inner {
        retries: u8,
        timeout_ms: u32,
    }
    #[derive(Facet)]
    struct Outer {
        name: String,
        inner: Inner,
    }

    show(
        "nested struct, one deep field changed",
        &Outer {
            name: "client".into(),
            inner: Inner {
                retries: 3,
                timeout_ms: 1000,
            },
        },
        &Outer {
            name: "client".into(),
            inner: Inner {
                retries: 3,
                timeout_ms: 5000,
            },
        },
    );
}

// ────────────────────────────── enums ──────────────────────────────

fn enums() {
    #[derive(Facet)]
    #[repr(u8)]
    #[allow(dead_code)] // fields are read via reflection, not by rustc
    enum State {
        Idle,
        Running { pid: u32, cpu: f32 },
        Failed(String),
    }
    #[derive(Facet)]
    struct Job {
        id: u64,
        state: State,
    }

    show(
        "enum: same variant, payload changed",
        &Job {
            id: 1,
            state: State::Running { pid: 100, cpu: 0.2 },
        },
        &Job {
            id: 1,
            state: State::Running { pid: 100, cpu: 0.9 },
        },
    );

    show(
        "enum: variant changed",
        &Job {
            id: 1,
            state: State::Running { pid: 100, cpu: 0.2 },
        },
        &Job {
            id: 1,
            state: State::Failed("oom".into()),
        },
    );

    show(
        "enum: unit → unit",
        &Job {
            id: 2,
            state: State::Idle,
        },
        &Job {
            id: 2,
            state: State::Failed("panic".into()),
        },
    );
}

// ───────────────────────────── options ─────────────────────────────

fn options() {
    #[derive(Facet)]
    struct Config {
        name: String,
        proxy: Option<String>,
        limit: Option<u32>,
    }

    show(
        "Option: None → Some",
        &Config {
            name: "c".into(),
            proxy: None,
            limit: Some(10),
        },
        &Config {
            name: "c".into(),
            proxy: Some("127.0.0.1:8080".into()),
            limit: Some(10),
        },
    );

    show(
        "Option: Some → None",
        &Config {
            name: "c".into(),
            proxy: Some("127.0.0.1:8080".into()),
            limit: Some(10),
        },
        &Config {
            name: "c".into(),
            proxy: None,
            limit: Some(10),
        },
    );

    show(
        "Option: Some(x) → Some(y)",
        &Config {
            name: "c".into(),
            proxy: None,
            limit: Some(10),
        },
        &Config {
            name: "c".into(),
            proxy: None,
            limit: Some(99),
        },
    );
}

// ──────────────────────────── sequences ────────────────────────────

fn sequences() {
    #[derive(Facet)]
    struct Cluster {
        ports: Vec<u16>,
    }

    show(
        "Vec: element inserted in the middle",
        &Cluster {
            ports: vec![80, 443, 8443],
        },
        &Cluster {
            ports: vec![80, 443, 9000, 8443],
        },
    );

    show(
        "Vec: element removed",
        &Cluster {
            ports: vec![80, 443, 9000, 8443],
        },
        &Cluster {
            ports: vec![80, 443, 8443],
        },
    );

    show(
        "Vec: element value changed",
        &Cluster {
            ports: vec![80, 443, 8443],
        },
        &Cluster {
            ports: vec![80, 8080, 8443],
        },
    );

    #[derive(Facet)]
    struct Route {
        path: String,
        code: u16,
    }
    #[derive(Facet)]
    struct Routes {
        routes: Vec<Route>,
    }

    show(
        "Vec<struct>: one element's field changed",
        &Routes {
            routes: vec![
                Route {
                    path: "/".into(),
                    code: 200,
                },
                Route {
                    path: "/health".into(),
                    code: 204,
                },
            ],
        },
        &Routes {
            routes: vec![
                Route {
                    path: "/".into(),
                    code: 200,
                },
                Route {
                    path: "/health".into(),
                    code: 503,
                },
            ],
        },
    );
}

// ──────────────────────────── maps & sets ──────────────────────────

fn maps_and_sets() {
    #[derive(Facet)]
    struct Routes {
        routes: HashMap<String, u16>,
    }

    show(
        "HashMap: changed + added + removed keys",
        &Routes {
            routes: HashMap::from([
                ("/".into(), 200),
                ("/health".into(), 204),
                ("/admin".into(), 200),
            ]),
        },
        &Routes {
            routes: HashMap::from([
                ("/".into(), 200),
                ("/health".into(), 503),
                ("/metrics".into(), 200),
            ]),
        },
    );

    #[derive(Facet)]
    struct Limits {
        by_tier: BTreeMap<String, u32>,
    }

    show(
        "BTreeMap: one value changed",
        &Limits {
            by_tier: BTreeMap::from([("free".into(), 10), ("pro".into(), 100)]),
        },
        &Limits {
            by_tier: BTreeMap::from([("free".into(), 10), ("pro".into(), 500)]),
        },
    );

    #[derive(Facet)]
    struct Flags {
        enabled: HashSet<String>,
    }

    show(
        "HashSet: members added/removed",
        &Flags {
            enabled: HashSet::from(["a".into(), "b".into()]),
        },
        &Flags {
            enabled: HashSet::from(["b".into(), "c".into()]),
        },
    );
}

// ────────────────────────────── tuples ─────────────────────────────

fn tuples() {
    #[derive(Facet)]
    struct Point(i32, i32, i32);

    show(
        "tuple struct: one slot changed",
        &Point(1, 2, 3),
        &Point(1, 9, 3),
    );

    #[derive(Facet)]
    struct Pair {
        span: (u32, u32),
    }

    show(
        "tuple field: end moved",
        &Pair { span: (10, 20) },
        &Pair { span: (10, 35) },
    );
}

// ──────────────────────────── collapsing ────────────────────────────

fn collapsing() {
    #[derive(Facet)]
    struct Big {
        a: u32,
        b: u32,
        c: u32,
        d: u32,
        e: u32,
        f: u32,
        g: u32,
        h: u32,
    }

    show(
        "many unchanged fields collapse around the one change",
        &Big {
            a: 1,
            b: 2,
            c: 3,
            d: 4,
            e: 5,
            f: 6,
            g: 7,
            h: 8,
        },
        &Big {
            a: 1,
            b: 2,
            c: 3,
            d: 4,
            e: 5,
            f: 6,
            g: 7,
            h: 99,
        },
    );
}

// ──────────────────────────── cross type ────────────────────────────

fn cross_type() {
    #[derive(Facet)]
    struct PersonV1 {
        name: String,
        age: u32,
    }
    #[derive(Facet)]
    struct PersonV2 {
        name: String,
        age: u32,
    }

    show_ish(
        "sameish: two different types, one field differs",
        &PersonV1 {
            name: "Ada".into(),
            age: 30,
        },
        &PersonV2 {
            name: "Ada".into(),
            age: 31,
        },
    );
}

// ────────────────────────────── strings ─────────────────────────────

fn strings() {
    #[derive(Facet)]
    struct Doc {
        title: String,
    }

    show(
        "string changed",
        &Doc {
            title: "Introduction to rediff".into(),
        },
        &Doc {
            title: "Introduction to rediff (revised)".into(),
        },
    );

    // Visually identical, but different Unicode scalars (Cyrylic 'а').
    show(
        "confusable strings (look identical, differ in codepoints)",
        &Doc {
            title: "paypal.com".into(),
        },
        &Doc {
            title: "pаypаl.com".into(),
        },
    );
}
