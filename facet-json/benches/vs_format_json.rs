//! Benchmark comparing facet_json vs facet_format_json serialization performance.
//!
//! Uses standard JSON benchmark corpus files (brotli-compressed) plus synthetic
//! benchmarks that exercise specific code paths.

use divan::{Bencher, black_box};
use facet::Facet;
use std::collections::HashMap;
use std::sync::LazyLock;

fn main() {
    divan::main();
}

// =============================================================================
// Corpus loading (brotli-compressed)
// =============================================================================

fn decompress_brotli(compressed: &[u8]) -> String {
    let mut decompressed = Vec::new();
    brotli::BrotliDecompress(&mut std::io::Cursor::new(compressed), &mut decompressed).unwrap();
    String::from_utf8(decompressed).unwrap()
}

static TWITTER_JSON: LazyLock<String> =
    LazyLock::new(|| decompress_brotli(include_bytes!("corpus/twitter.json.br")));

static CANADA_JSON: LazyLock<String> =
    LazyLock::new(|| decompress_brotli(include_bytes!("corpus/canada.json.br")));

// =============================================================================
// Twitter: Sparse struct definitions
// =============================================================================

#[derive(Facet, Debug, Clone)]
struct TwitterResponseSparse {
    statuses: Vec<StatusSparse>,
}

#[derive(Facet, Debug, Clone)]
struct StatusSparse {
    id: u64,
    text: String,
    user: UserSparse,
    retweet_count: u32,
    favorite_count: u32,
}

#[derive(Facet, Debug, Clone)]
struct UserSparse {
    id: u64,
    screen_name: String,
    followers_count: u32,
}

// =============================================================================
// Canada: GeoJSON structure (number-heavy)
// =============================================================================

#[derive(Facet, Debug, Clone)]
struct Canada {
    #[facet(rename = "type")]
    type_: String,
    features: Vec<Feature>,
}

#[derive(Facet, Debug, Clone)]
struct Feature {
    #[facet(rename = "type")]
    type_: String,
    properties: Properties,
    geometry: Geometry,
}

#[derive(Facet, Debug, Clone)]
struct Properties {
    name: String,
}

#[derive(Facet, Debug, Clone)]
struct Geometry {
    #[facet(rename = "type")]
    type_: String,
    coordinates: Vec<Vec<Vec<f64>>>,
}

// =============================================================================
// Twitter benchmarks
// =============================================================================

mod twitter {
    use super::*;

    static DATA: LazyLock<TwitterResponseSparse> =
        LazyLock::new(|| facet_json::from_str(&TWITTER_JSON).unwrap());

    #[divan::bench]
    fn facet_json_serialize(bencher: Bencher) {
        let data = &*DATA;
        bencher.bench(|| black_box(facet_json::to_string(black_box(data))));
    }

    #[divan::bench]
    fn facet_format_json_serialize(bencher: Bencher) {
        let data = &*DATA;
        bencher.bench(|| black_box(facet_format_json::to_string(black_box(data))));
    }

    #[divan::bench]
    fn facet_json_deserialize(bencher: Bencher) {
        let json = &*TWITTER_JSON;
        bencher.bench(|| {
            let result: TwitterResponseSparse = facet_json::from_str(black_box(json)).unwrap();
            black_box(result)
        });
    }

    #[divan::bench]
    fn facet_format_json_deserialize(bencher: Bencher) {
        let json = &*TWITTER_JSON;
        bencher.bench(|| {
            let result: TwitterResponseSparse =
                facet_format_json::from_str(black_box(json)).unwrap();
            black_box(result)
        });
    }
}

// =============================================================================
// Canada benchmarks (number-heavy GeoJSON)
// =============================================================================

mod canada {
    use super::*;

    static DATA: LazyLock<Canada> = LazyLock::new(|| facet_json::from_str(&CANADA_JSON).unwrap());

    #[divan::bench]
    fn facet_json_serialize(bencher: Bencher) {
        let data = &*DATA;
        bencher.bench(|| black_box(facet_json::to_string(black_box(data))));
    }

    #[divan::bench]
    fn facet_format_json_serialize(bencher: Bencher) {
        let data = &*DATA;
        bencher.bench(|| black_box(facet_format_json::to_string(black_box(data))));
    }

    #[divan::bench]
    fn facet_json_deserialize(bencher: Bencher) {
        let json = &*CANADA_JSON;
        bencher.bench(|| {
            let result: Canada = facet_json::from_str(black_box(json)).unwrap();
            black_box(result)
        });
    }

    #[divan::bench]
    fn facet_format_json_deserialize(bencher: Bencher) {
        let json = &*CANADA_JSON;
        bencher.bench(|| {
            let result: Canada = facet_format_json::from_str(black_box(json)).unwrap();
            black_box(result)
        });
    }
}

// =============================================================================
// Synthetic benchmarks - exercise specific code paths
// =============================================================================

/// Pure integer arrays - tests number formatting (itoa)
mod integers {
    use super::*;

    fn make_data() -> Vec<u64> {
        (0..1000).map(|i| i * 12345678901234).collect()
    }

    static DATA: LazyLock<Vec<u64>> = LazyLock::new(make_data);
    static JSON: LazyLock<String> = LazyLock::new(|| facet_json::to_string(&*DATA));

    #[divan::bench]
    fn facet_json_serialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_json::to_string(black_box(&*DATA))));
    }

    #[divan::bench]
    fn facet_format_json_serialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_format_json::to_string(black_box(&*DATA))));
    }

    #[divan::bench]
    fn facet_json_deserialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_json::from_str::<Vec<u64>>(black_box(&*JSON))));
    }

    #[divan::bench]
    fn facet_format_json_deserialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_format_json::from_str::<Vec<u64>>(black_box(&*JSON))));
    }
}

/// Pure float arrays - tests number formatting (ryu)
mod floats {
    use super::*;

    fn make_data() -> Vec<f64> {
        (0..1000)
            .map(|i| (i as f64) * 1.23456789012345e10)
            .collect()
    }

    static DATA: LazyLock<Vec<f64>> = LazyLock::new(make_data);
    static JSON: LazyLock<String> = LazyLock::new(|| facet_json::to_string(&*DATA));

    #[divan::bench]
    fn facet_json_serialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_json::to_string(black_box(&*DATA))));
    }

    #[divan::bench]
    fn facet_format_json_serialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_format_json::to_string(black_box(&*DATA))));
    }

    #[divan::bench]
    fn facet_json_deserialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_json::from_str::<Vec<f64>>(black_box(&*JSON))));
    }

    #[divan::bench]
    fn facet_format_json_deserialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_format_json::from_str::<Vec<f64>>(black_box(&*JSON))));
    }
}

/// Short strings - tests string escaping fast path
mod short_strings {
    use super::*;

    fn make_data() -> Vec<String> {
        (0..1000).map(|i| format!("item_{i}")).collect()
    }

    static DATA: LazyLock<Vec<String>> = LazyLock::new(make_data);
    static JSON: LazyLock<String> = LazyLock::new(|| facet_json::to_string(&*DATA));

    #[divan::bench]
    fn facet_json_serialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_json::to_string(black_box(&*DATA))));
    }

    #[divan::bench]
    fn facet_format_json_serialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_format_json::to_string(black_box(&*DATA))));
    }

    #[divan::bench]
    fn facet_json_deserialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_json::from_str::<Vec<String>>(black_box(&*JSON))));
    }

    #[divan::bench]
    fn facet_format_json_deserialize(bencher: Bencher) {
        bencher.bench(|| {
            black_box(facet_format_json::from_str::<Vec<String>>(black_box(
                &*JSON,
            )))
        });
    }
}

/// Long strings - tests string handling with larger payloads
mod long_strings {
    use super::*;

    fn make_data() -> Vec<String> {
        (0..100)
            .map(|i| format!("This is a much longer string number {i} with more content to process and test the string handling performance of both implementations"))
            .collect()
    }

    static DATA: LazyLock<Vec<String>> = LazyLock::new(make_data);
    static JSON: LazyLock<String> = LazyLock::new(|| facet_json::to_string(&*DATA));

    #[divan::bench]
    fn facet_json_serialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_json::to_string(black_box(&*DATA))));
    }

    #[divan::bench]
    fn facet_format_json_serialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_format_json::to_string(black_box(&*DATA))));
    }

    #[divan::bench]
    fn facet_json_deserialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_json::from_str::<Vec<String>>(black_box(&*JSON))));
    }

    #[divan::bench]
    fn facet_format_json_deserialize(bencher: Bencher) {
        bencher.bench(|| {
            black_box(facet_format_json::from_str::<Vec<String>>(black_box(
                &*JSON,
            )))
        });
    }
}

/// Strings with escapes - tests escape handling
mod escaped_strings {
    use super::*;

    fn make_data() -> Vec<String> {
        (0..100)
            .map(|i| format!("Line {i}\twith\ttabs\nand \"quotes\" and \\backslashes\\"))
            .collect()
    }

    static DATA: LazyLock<Vec<String>> = LazyLock::new(make_data);
    static JSON: LazyLock<String> = LazyLock::new(|| facet_json::to_string(&*DATA));

    #[divan::bench]
    fn facet_json_serialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_json::to_string(black_box(&*DATA))));
    }

    #[divan::bench]
    fn facet_format_json_serialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_format_json::to_string(black_box(&*DATA))));
    }

    #[divan::bench]
    fn facet_json_deserialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_json::from_str::<Vec<String>>(black_box(&*JSON))));
    }

    #[divan::bench]
    fn facet_format_json_deserialize(bencher: Bencher) {
        bencher.bench(|| {
            black_box(facet_format_json::from_str::<Vec<String>>(black_box(
                &*JSON,
            )))
        });
    }
}

/// Nested structs - tests struct traversal overhead
mod nested_structs {
    use super::*;

    #[derive(Facet, Clone, Debug)]
    struct Outer {
        id: u64,
        inner: Inner,
    }

    #[derive(Facet, Clone, Debug)]
    struct Inner {
        name: String,
        value: f64,
        deep: Deep,
    }

    #[derive(Facet, Clone, Debug)]
    struct Deep {
        flag: bool,
        count: u32,
    }

    fn make_data() -> Vec<Outer> {
        (0..500)
            .map(|i| Outer {
                id: i,
                inner: Inner {
                    name: format!("name_{i}"),
                    value: i as f64 * 1.5,
                    deep: Deep {
                        flag: i % 2 == 0,
                        count: i as u32 * 10,
                    },
                },
            })
            .collect()
    }

    static DATA: LazyLock<Vec<Outer>> = LazyLock::new(make_data);
    static JSON: LazyLock<String> = LazyLock::new(|| facet_json::to_string(&*DATA));

    #[divan::bench]
    fn facet_json_serialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_json::to_string(black_box(&*DATA))));
    }

    #[divan::bench]
    fn facet_format_json_serialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_format_json::to_string(black_box(&*DATA))));
    }

    #[divan::bench]
    fn facet_json_deserialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_json::from_str::<Vec<Outer>>(black_box(&*JSON))));
    }

    #[divan::bench]
    fn facet_format_json_deserialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_format_json::from_str::<Vec<Outer>>(black_box(&*JSON))));
    }
}

/// HashMaps - tests map serialization
mod hashmaps {
    use super::*;

    fn make_data() -> HashMap<String, u64> {
        (0..500).map(|i| (format!("key_{i}"), i * 1000)).collect()
    }

    static DATA: LazyLock<HashMap<String, u64>> = LazyLock::new(make_data);
    static JSON: LazyLock<String> = LazyLock::new(|| facet_json::to_string(&*DATA));

    #[divan::bench]
    fn facet_json_serialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_json::to_string(black_box(&*DATA))));
    }

    #[divan::bench]
    fn facet_format_json_serialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_format_json::to_string(black_box(&*DATA))));
    }

    #[divan::bench]
    fn facet_json_deserialize(bencher: Bencher) {
        bencher.bench(|| {
            black_box(facet_json::from_str::<HashMap<String, u64>>(black_box(
                &*JSON,
            )))
        });
    }

    #[divan::bench]
    fn facet_format_json_deserialize(bencher: Bencher) {
        bencher.bench(|| {
            black_box(facet_format_json::from_str::<HashMap<String, u64>>(
                black_box(&*JSON),
            ))
        });
    }
}

/// Options - tests Option handling
mod options {
    use super::*;

    #[derive(Facet, Clone, Debug)]
    struct MaybeData {
        required: u64,
        optional_string: Option<String>,
        optional_number: Option<f64>,
    }

    fn make_data() -> Vec<MaybeData> {
        (0..500)
            .map(|i| MaybeData {
                required: i,
                optional_string: if i % 2 == 0 {
                    Some(format!("value_{i}"))
                } else {
                    None
                },
                optional_number: if i % 3 == 0 {
                    Some(i as f64 * 1.5)
                } else {
                    None
                },
            })
            .collect()
    }

    static DATA: LazyLock<Vec<MaybeData>> = LazyLock::new(make_data);
    static JSON: LazyLock<String> = LazyLock::new(|| facet_json::to_string(&*DATA));

    #[divan::bench]
    fn facet_json_serialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_json::to_string(black_box(&*DATA))));
    }

    #[divan::bench]
    fn facet_format_json_serialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_format_json::to_string(black_box(&*DATA))));
    }

    #[divan::bench]
    fn facet_json_deserialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_json::from_str::<Vec<MaybeData>>(black_box(&*JSON))));
    }

    #[divan::bench]
    fn facet_format_json_deserialize(bencher: Bencher) {
        bencher.bench(|| {
            black_box(facet_format_json::from_str::<Vec<MaybeData>>(black_box(
                &*JSON,
            )))
        });
    }
}

// TODO: Add enum benchmarks - derive macro has issues in nested mods

/// Booleans - tests bool serialization
mod booleans {
    use super::*;

    fn make_data() -> Vec<bool> {
        (0..1000).map(|i| i % 2 == 0).collect()
    }

    static DATA: LazyLock<Vec<bool>> = LazyLock::new(make_data);
    static JSON: LazyLock<String> = LazyLock::new(|| facet_json::to_string(&*DATA));

    #[divan::bench]
    fn facet_json_serialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_json::to_string(black_box(&*DATA))));
    }

    #[divan::bench]
    fn facet_format_json_serialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_format_json::to_string(black_box(&*DATA))));
    }

    #[divan::bench]
    fn facet_json_deserialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_json::from_str::<Vec<bool>>(black_box(&*JSON))));
    }

    #[divan::bench]
    fn facet_format_json_deserialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_format_json::from_str::<Vec<bool>>(black_box(&*JSON))));
    }
}
