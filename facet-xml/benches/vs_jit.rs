//! Benchmark comparing facet-xml with/without JIT deserialization.
//!
//! This demonstrates the format-agnostic JIT: the same JIT code that works for JSON
//! also works for XML, because both produce the same ParseEvent stream.

use divan::{Bencher, black_box};
use facet::Facet;
use facet_format::FormatDeserializer;
use facet_format::jit as format_jit;
use facet_xml::{XmlParser, to_vec};
use std::sync::LazyLock;

fn main() {
    divan::main();
}

// =============================================================================
// Simple struct - JIT-compatible (flat struct with scalar fields)
// =============================================================================

#[derive(Facet, Debug, Clone)]
struct SimpleRecord {
    id: u64,
    score: f64,
    count: i64,
    active: bool,
    name: String,
}

mod simple_struct {
    use super::*;

    static DATA: LazyLock<SimpleRecord> = LazyLock::new(|| SimpleRecord {
        id: 12345,
        score: 98.6,
        count: -42,
        active: true,
        name: "test_record".into(),
    });

    static XML: LazyLock<Vec<u8>> = LazyLock::new(|| to_vec(&*DATA).unwrap());

    #[divan::bench]
    fn facet_xml_deserialize(bencher: Bencher) {
        let xml = &*XML;
        bencher.bench(|| {
            let parser = XmlParser::new(black_box(xml));
            let mut de = FormatDeserializer::new_owned(parser);
            black_box(de.deserialize_root::<SimpleRecord>())
        });
    }

    #[divan::bench]
    fn facet_xml_jit_deserialize(bencher: Bencher) {
        let xml = &*XML;
        bencher.bench(|| {
            let parser = XmlParser::new(black_box(xml));
            black_box(format_jit::deserialize_with_fallback::<SimpleRecord, _>(
                parser,
            ))
        });
    }
}

// =============================================================================
// Nested struct - NOT JIT-compatible, exercises fallback
// =============================================================================

#[derive(Facet, Debug, Clone)]
struct Outer {
    id: u64,
    inner: Inner,
}

#[derive(Facet, Debug, Clone)]
struct Inner {
    name: String,
    value: f64,
}

mod nested_struct {
    use super::*;

    static DATA: LazyLock<Outer> = LazyLock::new(|| Outer {
        id: 42,
        inner: Inner {
            name: "nested".into(),
            value: 2.5,
        },
    });

    static XML: LazyLock<Vec<u8>> = LazyLock::new(|| to_vec(&*DATA).unwrap());

    #[divan::bench]
    fn facet_xml_deserialize(bencher: Bencher) {
        let xml = &*XML;
        bencher.bench(|| {
            let parser = XmlParser::new(black_box(xml));
            let mut de = FormatDeserializer::new_owned(parser);
            black_box(de.deserialize_root::<Outer>())
        });
    }

    #[divan::bench]
    fn facet_xml_jit_with_fallback(bencher: Bencher) {
        let xml = &*XML;
        bencher.bench(|| {
            let parser = XmlParser::new(black_box(xml));
            // JIT not compatible, will fall back to reflection
            black_box(format_jit::deserialize_with_fallback::<Outer, _>(parser))
        });
    }
}

// =============================================================================
// Multiple simple records - many flat structs in a list
// =============================================================================

mod many_simple_structs {
    use super::*;

    fn make_data() -> Vec<SimpleRecord> {
        (0..100)
            .map(|i| SimpleRecord {
                id: i,
                score: i as f64 * 1.5,
                count: -(i as i64),
                active: i % 2 == 0,
                name: format!("record_{i}"),
            })
            .collect()
    }

    static DATA: LazyLock<Vec<SimpleRecord>> = LazyLock::new(make_data);
    static XML: LazyLock<Vec<u8>> = LazyLock::new(|| to_vec(&*DATA).unwrap());

    #[divan::bench]
    fn facet_xml_deserialize(bencher: Bencher) {
        let xml = &*XML;
        bencher.bench(|| {
            let parser = XmlParser::new(black_box(xml));
            let mut de = FormatDeserializer::new_owned(parser);
            black_box(de.deserialize_root::<Vec<SimpleRecord>>())
        });
    }

    #[divan::bench]
    fn facet_xml_jit_with_fallback(bencher: Bencher) {
        let xml = &*XML;
        bencher.bench(|| {
            let parser = XmlParser::new(black_box(xml));
            // Vec<SimpleRecord> is not JIT-compatible, will fall back to reflection
            black_box(format_jit::deserialize_with_fallback::<Vec<SimpleRecord>, _>(parser))
        });
    }
}

// =============================================================================
// Primitive types - integers, floats, strings
// =============================================================================

mod integers {
    use super::*;

    fn make_data() -> Vec<u64> {
        (0..100).map(|i| i * 12345678901234).collect()
    }

    static DATA: LazyLock<Vec<u64>> = LazyLock::new(make_data);
    static XML: LazyLock<Vec<u8>> = LazyLock::new(|| to_vec(&*DATA).unwrap());

    #[divan::bench]
    fn facet_xml_deserialize(bencher: Bencher) {
        let xml = &*XML;
        bencher.bench(|| {
            let parser = XmlParser::new(black_box(xml));
            let mut de = FormatDeserializer::new_owned(parser);
            black_box(de.deserialize_root::<Vec<u64>>())
        });
    }

    #[divan::bench]
    fn facet_xml_jit_with_fallback(bencher: Bencher) {
        let xml = &*XML;
        bencher.bench(|| {
            let parser = XmlParser::new(black_box(xml));
            black_box(format_jit::deserialize_with_fallback::<Vec<u64>, _>(parser))
        });
    }
}

mod floats {
    use super::*;

    fn make_data() -> Vec<f64> {
        (0..100).map(|i| (i as f64) * 1.23456789012345e10).collect()
    }

    static DATA: LazyLock<Vec<f64>> = LazyLock::new(make_data);
    static XML: LazyLock<Vec<u8>> = LazyLock::new(|| to_vec(&*DATA).unwrap());

    #[divan::bench]
    fn facet_xml_deserialize(bencher: Bencher) {
        let xml = &*XML;
        bencher.bench(|| {
            let parser = XmlParser::new(black_box(xml));
            let mut de = FormatDeserializer::new_owned(parser);
            black_box(de.deserialize_root::<Vec<f64>>())
        });
    }

    #[divan::bench]
    fn facet_xml_jit_with_fallback(bencher: Bencher) {
        let xml = &*XML;
        bencher.bench(|| {
            let parser = XmlParser::new(black_box(xml));
            black_box(format_jit::deserialize_with_fallback::<Vec<f64>, _>(parser))
        });
    }
}

mod strings {
    use super::*;

    fn make_data() -> Vec<String> {
        (0..100).map(|i| format!("string_value_{i}")).collect()
    }

    static DATA: LazyLock<Vec<String>> = LazyLock::new(make_data);
    static XML: LazyLock<Vec<u8>> = LazyLock::new(|| to_vec(&*DATA).unwrap());

    #[divan::bench]
    fn facet_xml_deserialize(bencher: Bencher) {
        let xml = &*XML;
        bencher.bench(|| {
            let parser = XmlParser::new(black_box(xml));
            let mut de = FormatDeserializer::new_owned(parser);
            black_box(de.deserialize_root::<Vec<String>>())
        });
    }

    #[divan::bench]
    fn facet_xml_jit_with_fallback(bencher: Bencher) {
        let xml = &*XML;
        bencher.bench(|| {
            let parser = XmlParser::new(black_box(xml));
            black_box(format_jit::deserialize_with_fallback::<Vec<String>, _>(
                parser,
            ))
        });
    }
}
