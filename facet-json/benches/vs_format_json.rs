//! Benchmark comparing facet_json vs facet_format_json vs serde_json serialization performance.
//!
//! Uses standard JSON benchmark corpus files (brotli-compressed) plus synthetic
//! benchmarks that exercise specific code paths.
//!
//! Also includes facet_json::cranelift (JIT-compiled) for deserialization.
//! Also includes facet_format::jit (format-agnostic JIT) for deserialization.

use divan::{Bencher, black_box};
use facet::Facet;
use facet_format::jit as format_jit;
use facet_format_json::JsonParser;
use serde::{Deserialize, Serialize};
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

#[derive(Facet, Serialize, Deserialize, Debug, Clone)]
struct TwitterResponseSparse {
    statuses: Vec<StatusSparse>,
}

#[derive(Facet, Serialize, Deserialize, Debug, Clone)]
struct StatusSparse {
    id: u64,
    text: String,
    user: UserSparse,
    retweet_count: u32,
    favorite_count: u32,
}

#[derive(Facet, Serialize, Deserialize, Debug, Clone)]
struct UserSparse {
    id: u64,
    screen_name: String,
    followers_count: u32,
}

// =============================================================================
// Canada: GeoJSON structure (number-heavy)
// =============================================================================

#[derive(Facet, Serialize, Deserialize, Debug, Clone)]
struct Canada {
    #[facet(rename = "type")]
    #[serde(rename = "type")]
    type_: String,
    features: Vec<Feature>,
}

#[derive(Facet, Serialize, Deserialize, Debug, Clone)]
struct Feature {
    #[facet(rename = "type")]
    #[serde(rename = "type")]
    type_: String,
    properties: Properties,
    geometry: Geometry,
}

#[derive(Facet, Serialize, Deserialize, Debug, Clone)]
struct Properties {
    name: String,
}

#[derive(Facet, Serialize, Deserialize, Debug, Clone)]
struct Geometry {
    #[facet(rename = "type")]
    #[serde(rename = "type")]
    type_: String,
    coordinates: Vec<Vec<Vec<f64>>>,
}

// =============================================================================
// Flatten benchmark types (defined at top level due to derive macro limitations)
// =============================================================================

// Auth variants for flatten benchmarks
#[derive(Facet, Serialize, Deserialize, Debug, Clone, PartialEq)]
struct AuthPassword {
    password: String,
}

#[derive(Facet, Serialize, Deserialize, Debug, Clone, PartialEq)]
struct AuthToken {
    token: String,
    token_expiry: u64,
}

#[derive(Facet, Serialize, Deserialize, Debug, Clone, PartialEq)]
#[repr(C)]
enum AuthMethod {
    Password(AuthPassword),
    Token(AuthToken),
}

// Transport variants
#[derive(Facet, Serialize, Deserialize, Debug, Clone, PartialEq)]
struct TransportTcp {
    tcp_port: u16,
}

#[derive(Facet, Serialize, Deserialize, Debug, Clone, PartialEq)]
struct TransportUnix {
    socket_path: String,
}

#[derive(Facet, Serialize, Deserialize, Debug, Clone, PartialEq)]
#[repr(C)]
enum Transport {
    Tcp(TransportTcp),
    Unix(TransportUnix),
}

// Storage variants
#[derive(Facet, Serialize, Deserialize, Debug, Clone, PartialEq)]
struct StorageLocal {
    local_path: String,
}

#[derive(Facet, Serialize, Deserialize, Debug, Clone, PartialEq)]
struct StorageRemote {
    remote_url: String,
}

#[derive(Facet, Serialize, Deserialize, Debug, Clone, PartialEq)]
#[repr(C)]
enum Storage {
    Local(StorageLocal),
    Remote(StorageRemote),
}

// Logging variants
#[derive(Facet, Serialize, Deserialize, Debug, Clone, PartialEq)]
struct LogFile {
    log_path: String,
}

#[derive(Facet, Serialize, Deserialize, Debug, Clone, PartialEq)]
struct LogStdout {
    log_color: bool,
}

#[derive(Facet, Serialize, Deserialize, Debug, Clone, PartialEq)]
#[repr(C)]
enum Logging {
    File(LogFile),
    Stdout(LogStdout),
}

// 2-enum config (2×2 = 4 configurations)
#[derive(Facet, Serialize, Deserialize, Debug, Clone, PartialEq)]
struct Config2Enums {
    name: String,
    #[facet(flatten)]
    #[serde(flatten)]
    auth: AuthMethod,
    #[facet(flatten)]
    #[serde(flatten)]
    transport: Transport,
}

// 4-enum config (2^4 = 16 configurations)
#[derive(Facet, Serialize, Deserialize, Debug, Clone, PartialEq)]
struct Config4Enums {
    name: String,
    #[facet(flatten)]
    #[serde(flatten)]
    auth: AuthMethod,
    #[facet(flatten)]
    #[serde(flatten)]
    transport: Transport,
    #[facet(flatten)]
    #[serde(flatten)]
    storage: Storage,
    #[facet(flatten)]
    #[serde(flatten)]
    logging: Logging,
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
    fn serde_json_serialize(bencher: Bencher) {
        let data = &*DATA;
        bencher.bench(|| black_box(serde_json::to_string(black_box(data)).unwrap()));
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
    fn facet_json_cranelift_deserialize(bencher: Bencher) {
        let json = &*TWITTER_JSON;
        bencher.bench(|| {
            let result: TwitterResponseSparse =
                facet_json::cranelift::from_str(black_box(json)).unwrap();
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

    #[divan::bench]
    fn facet_format_jit_deserialize(bencher: Bencher) {
        let json = TWITTER_JSON.as_bytes();
        bencher.bench(|| {
            let parser = JsonParser::new(black_box(json));
            let result: TwitterResponseSparse =
                format_jit::deserialize_with_fallback(parser).unwrap();
            black_box(result)
        });
    }

    #[divan::bench]
    fn serde_json_deserialize(bencher: Bencher) {
        let json = &*TWITTER_JSON;
        bencher.bench(|| {
            let result: TwitterResponseSparse = serde_json::from_str(black_box(json)).unwrap();
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
    fn serde_json_serialize(bencher: Bencher) {
        let data = &*DATA;
        bencher.bench(|| black_box(serde_json::to_string(black_box(data)).unwrap()));
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
    fn facet_json_cranelift_deserialize(bencher: Bencher) {
        let json = &*CANADA_JSON;
        bencher.bench(|| {
            let result: Canada = facet_json::cranelift::from_str(black_box(json)).unwrap();
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

    #[divan::bench]
    fn serde_json_deserialize(bencher: Bencher) {
        let json = &*CANADA_JSON;
        bencher.bench(|| {
            let result: Canada = serde_json::from_str(black_box(json)).unwrap();
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
    fn serde_json_serialize(bencher: Bencher) {
        bencher.bench(|| black_box(serde_json::to_string(black_box(&*DATA)).unwrap()));
    }

    #[divan::bench]
    fn facet_json_deserialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_json::from_str::<Vec<u64>>(black_box(&*JSON))));
    }

    #[divan::bench]
    fn facet_json_cranelift_deserialize(bencher: Bencher) {
        bencher.bench(|| {
            black_box(facet_json::cranelift::from_str::<Vec<u64>>(black_box(
                &*JSON,
            )))
        });
    }

    #[divan::bench]
    fn facet_format_json_deserialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_format_json::from_str::<Vec<u64>>(black_box(&*JSON))));
    }

    #[divan::bench]
    fn serde_json_deserialize(bencher: Bencher) {
        bencher.bench(|| black_box(serde_json::from_str::<Vec<u64>>(black_box(&*JSON)).unwrap()));
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
    fn serde_json_serialize(bencher: Bencher) {
        bencher.bench(|| black_box(serde_json::to_string(black_box(&*DATA)).unwrap()));
    }

    #[divan::bench]
    fn facet_json_deserialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_json::from_str::<Vec<f64>>(black_box(&*JSON))));
    }

    #[divan::bench]
    fn facet_json_cranelift_deserialize(bencher: Bencher) {
        bencher.bench(|| {
            black_box(facet_json::cranelift::from_str::<Vec<f64>>(black_box(
                &*JSON,
            )))
        });
    }

    #[divan::bench]
    fn facet_format_json_deserialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_format_json::from_str::<Vec<f64>>(black_box(&*JSON))));
    }

    #[divan::bench]
    fn serde_json_deserialize(bencher: Bencher) {
        bencher.bench(|| black_box(serde_json::from_str::<Vec<f64>>(black_box(&*JSON)).unwrap()));
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
    fn serde_json_serialize(bencher: Bencher) {
        bencher.bench(|| black_box(serde_json::to_string(black_box(&*DATA)).unwrap()));
    }

    #[divan::bench]
    fn facet_json_deserialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_json::from_str::<Vec<String>>(black_box(&*JSON))));
    }

    #[divan::bench]
    fn facet_json_cranelift_deserialize(bencher: Bencher) {
        bencher.bench(|| {
            black_box(facet_json::cranelift::from_str::<Vec<String>>(black_box(
                &*JSON,
            )))
        });
    }

    #[divan::bench]
    fn facet_format_json_deserialize(bencher: Bencher) {
        bencher.bench(|| {
            black_box(facet_format_json::from_str::<Vec<String>>(black_box(
                &*JSON,
            )))
        });
    }

    #[divan::bench]
    fn serde_json_deserialize(bencher: Bencher) {
        bencher
            .bench(|| black_box(serde_json::from_str::<Vec<String>>(black_box(&*JSON)).unwrap()));
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
    fn serde_json_serialize(bencher: Bencher) {
        bencher.bench(|| black_box(serde_json::to_string(black_box(&*DATA)).unwrap()));
    }

    #[divan::bench]
    fn facet_json_deserialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_json::from_str::<Vec<String>>(black_box(&*JSON))));
    }

    #[divan::bench]
    fn facet_json_cranelift_deserialize(bencher: Bencher) {
        bencher.bench(|| {
            black_box(facet_json::cranelift::from_str::<Vec<String>>(black_box(
                &*JSON,
            )))
        });
    }

    #[divan::bench]
    fn facet_format_json_deserialize(bencher: Bencher) {
        bencher.bench(|| {
            black_box(facet_format_json::from_str::<Vec<String>>(black_box(
                &*JSON,
            )))
        });
    }

    #[divan::bench]
    fn serde_json_deserialize(bencher: Bencher) {
        bencher
            .bench(|| black_box(serde_json::from_str::<Vec<String>>(black_box(&*JSON)).unwrap()));
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
    fn serde_json_serialize(bencher: Bencher) {
        bencher.bench(|| black_box(serde_json::to_string(black_box(&*DATA)).unwrap()));
    }

    #[divan::bench]
    fn facet_json_deserialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_json::from_str::<Vec<String>>(black_box(&*JSON))));
    }

    #[divan::bench]
    fn facet_json_cranelift_deserialize(bencher: Bencher) {
        bencher.bench(|| {
            black_box(facet_json::cranelift::from_str::<Vec<String>>(black_box(
                &*JSON,
            )))
        });
    }

    #[divan::bench]
    fn facet_format_json_deserialize(bencher: Bencher) {
        bencher.bench(|| {
            black_box(facet_format_json::from_str::<Vec<String>>(black_box(
                &*JSON,
            )))
        });
    }

    #[divan::bench]
    fn serde_json_deserialize(bencher: Bencher) {
        bencher
            .bench(|| black_box(serde_json::from_str::<Vec<String>>(black_box(&*JSON)).unwrap()));
    }
}

/// Nested structs - tests struct traversal overhead
mod nested_structs {
    use super::*;

    #[derive(Facet, Serialize, Deserialize, Clone, Debug)]
    struct Outer {
        id: u64,
        inner: Inner,
    }

    #[derive(Facet, Serialize, Deserialize, Clone, Debug)]
    struct Inner {
        name: String,
        value: f64,
        deep: Deep,
    }

    #[derive(Facet, Serialize, Deserialize, Clone, Debug)]
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
    fn serde_json_serialize(bencher: Bencher) {
        bencher.bench(|| black_box(serde_json::to_string(black_box(&*DATA)).unwrap()));
    }

    #[divan::bench]
    fn facet_json_deserialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_json::from_str::<Vec<Outer>>(black_box(&*JSON))));
    }

    #[divan::bench]
    fn facet_json_cranelift_deserialize(bencher: Bencher) {
        bencher.bench(|| {
            black_box(facet_json::cranelift::from_str_with_fallback::<Vec<Outer>>(
                black_box(&*JSON),
            ))
        });
    }

    #[divan::bench]
    fn facet_format_json_deserialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_format_json::from_str::<Vec<Outer>>(black_box(&*JSON))));
    }

    #[divan::bench]
    fn serde_json_deserialize(bencher: Bencher) {
        bencher.bench(|| black_box(serde_json::from_str::<Vec<Outer>>(black_box(&*JSON)).unwrap()));
    }
}

/// Single nested struct - for JIT testing (no Vec)
mod single_nested_struct {
    use super::*;

    #[derive(Facet, Serialize, Deserialize, Clone, Debug)]
    struct Outer {
        id: u64,
        inner: Inner,
        name: String,
    }

    #[derive(Facet, Serialize, Deserialize, Clone, Debug)]
    struct Inner {
        x: i64,
        y: i64,
    }

    static DATA: LazyLock<Outer> = LazyLock::new(|| Outer {
        id: 42,
        inner: Inner { x: 10, y: 20 },
        name: "test".to_string(),
    });
    static JSON: LazyLock<String> = LazyLock::new(|| facet_json::to_string(&*DATA));

    #[divan::bench]
    fn facet_format_jit_deserialize(bencher: Bencher) {
        bencher.bench(|| {
            black_box(format_jit::deserialize_with_fallback::<Outer, _>(
                facet_format_json::JsonParser::new(black_box(JSON.as_bytes())),
            ))
        });
    }

    #[divan::bench]
    fn facet_format_json_deserialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_format_json::from_str::<Outer>(black_box(&*JSON))));
    }

    #[divan::bench]
    fn facet_json_deserialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_json::from_str::<Outer>(black_box(&*JSON))));
    }

    #[divan::bench]
    fn serde_json_deserialize(bencher: Bencher) {
        bencher.bench(|| black_box(serde_json::from_str::<Outer>(black_box(&*JSON)).unwrap()));
    }
}

/// Simple struct with Options - for JIT testing Option support
mod simple_with_options {
    use super::*;

    #[derive(Facet, Serialize, Deserialize, Clone, Debug)]
    struct WithOptions {
        id: u64,
        maybe_count: Option<i64>,
        maybe_flag: Option<bool>,
        maybe_value: Option<f64>,
    }

    static DATA: LazyLock<WithOptions> = LazyLock::new(|| WithOptions {
        id: 42,
        maybe_count: Some(123),
        maybe_flag: None,
        maybe_value: Some(2.5),
    });
    static JSON: LazyLock<String> = LazyLock::new(|| facet_json::to_string(&*DATA));

    #[divan::bench]
    fn facet_format_jit_deserialize(bencher: Bencher) {
        bencher.bench(|| {
            black_box(format_jit::deserialize_with_fallback::<WithOptions, _>(
                facet_format_json::JsonParser::new(black_box(JSON.as_bytes())),
            ))
        });
    }

    #[divan::bench]
    fn facet_format_json_deserialize(bencher: Bencher) {
        bencher.bench(|| {
            black_box(facet_format_json::from_str::<WithOptions>(black_box(
                &*JSON,
            )))
        });
    }

    #[divan::bench]
    fn facet_json_deserialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_json::from_str::<WithOptions>(black_box(&*JSON))));
    }

    #[cfg(feature = "cranelift")]
    #[divan::bench]
    fn facet_json_cranelift_deserialize(bencher: Bencher) {
        bencher.bench(|| {
            black_box(
                facet_json::cranelift::from_str_with_fallback::<WithOptions>(black_box(&*JSON))
                    .unwrap(),
            )
        });
    }

    #[divan::bench]
    fn serde_json_deserialize(bencher: Bencher) {
        bencher
            .bench(|| black_box(serde_json::from_str::<WithOptions>(black_box(&*JSON)).unwrap()));
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
    fn serde_json_serialize(bencher: Bencher) {
        bencher.bench(|| black_box(serde_json::to_string(black_box(&*DATA)).unwrap()));
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
    fn facet_json_cranelift_deserialize(bencher: Bencher) {
        bencher.bench(|| {
            black_box(facet_json::cranelift::from_str_with_fallback::<
                HashMap<String, u64>,
            >(black_box(&*JSON)))
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

    #[divan::bench]
    fn serde_json_deserialize(bencher: Bencher) {
        bencher.bench(|| {
            black_box(serde_json::from_str::<HashMap<String, u64>>(black_box(&*JSON)).unwrap())
        });
    }
}

/// Options - tests Option handling
mod options {
    use super::*;

    #[derive(Facet, Serialize, Deserialize, Clone, Debug)]
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
    fn serde_json_serialize(bencher: Bencher) {
        bencher.bench(|| black_box(serde_json::to_string(black_box(&*DATA)).unwrap()));
    }

    #[divan::bench]
    fn facet_json_deserialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_json::from_str::<Vec<MaybeData>>(black_box(&*JSON))));
    }

    #[divan::bench]
    fn facet_json_cranelift_deserialize(bencher: Bencher) {
        bencher.bench(|| {
            black_box(facet_json::cranelift::from_str_with_fallback::<
                Vec<MaybeData>,
            >(black_box(&*JSON)))
        });
    }

    #[divan::bench]
    fn facet_format_json_deserialize(bencher: Bencher) {
        bencher.bench(|| {
            black_box(facet_format_json::from_str::<Vec<MaybeData>>(black_box(
                &*JSON,
            )))
        });
    }

    #[divan::bench]
    fn serde_json_deserialize(bencher: Bencher) {
        bencher.bench(|| {
            black_box(serde_json::from_str::<Vec<MaybeData>>(black_box(&*JSON)).unwrap())
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
    fn serde_json_serialize(bencher: Bencher) {
        bencher.bench(|| black_box(serde_json::to_string(black_box(&*DATA)).unwrap()));
    }

    #[divan::bench]
    fn facet_json_deserialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_json::from_str::<Vec<bool>>(black_box(&*JSON))));
    }

    #[divan::bench]
    fn facet_json_cranelift_deserialize(bencher: Bencher) {
        bencher.bench(|| {
            black_box(facet_json::cranelift::from_str::<Vec<bool>>(black_box(
                &*JSON,
            )))
        });
    }

    #[divan::bench]
    fn facet_format_json_deserialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_format_json::from_str::<Vec<bool>>(black_box(&*JSON))));
    }

    #[divan::bench]
    fn serde_json_deserialize(bencher: Bencher) {
        bencher.bench(|| black_box(serde_json::from_str::<Vec<bool>>(black_box(&*JSON)).unwrap()));
    }
}

// =============================================================================
// Simple struct benchmark - exercises the JIT path
// =============================================================================

/// A simple flat struct that IS JIT-compatible (no Vec, no nested structs)
#[derive(Facet, Serialize, Deserialize, Debug, Clone)]
struct SimpleRecord {
    id: u64,
    score: f64,
    count: i64,
    active: bool,
    name: String,
}

mod simple_struct {
    use super::*;

    // Single struct - this IS JIT-compatible
    static DATA: LazyLock<SimpleRecord> = LazyLock::new(|| SimpleRecord {
        id: 12345,
        score: 98.6,
        count: -42,
        active: true,
        name: "test_record".into(),
    });
    static JSON: LazyLock<String> = LazyLock::new(|| facet_json::to_string(&*DATA));

    #[divan::bench]
    fn facet_json_deserialize(bencher: Bencher) {
        let json = &*JSON;
        bencher.bench(|| black_box(facet_json::from_str::<SimpleRecord>(black_box(json))));
    }

    #[divan::bench]
    fn facet_json_cranelift_deserialize(bencher: Bencher) {
        let json = &*JSON;
        bencher.bench(|| {
            black_box(
                facet_json::cranelift::from_str_with_fallback::<SimpleRecord>(black_box(json)),
            )
        });
    }

    #[divan::bench]
    fn facet_format_json_deserialize(bencher: Bencher) {
        let json = &*JSON;
        bencher.bench(|| black_box(facet_format_json::from_str::<SimpleRecord>(black_box(json))));
    }

    #[divan::bench]
    fn facet_format_jit_deserialize(bencher: Bencher) {
        let json = JSON.as_bytes();
        bencher.bench(|| {
            let parser = JsonParser::new(black_box(json));
            black_box(format_jit::deserialize_with_fallback::<SimpleRecord, _>(
                parser,
            ))
        });
    }

    #[divan::bench]
    fn serde_json_deserialize(bencher: Bencher) {
        let json = &*JSON;
        bencher.bench(|| black_box(serde_json::from_str::<SimpleRecord>(black_box(json)).unwrap()));
    }
}

// =============================================================================
// Flatten benchmarks - measures solver overhead for ambiguous configurations
// =============================================================================

/// 2 flattened enums = 4 possible configurations
/// Tests the solver's ability to disambiguate based on field presence
mod flatten_2enums {
    use super::*;

    fn make_data() -> Vec<Config2Enums> {
        // Mix of all 4 configurations
        (0..250)
            .flat_map(|i| {
                vec![
                    Config2Enums {
                        name: format!("service_{}", i * 4),
                        auth: AuthMethod::Password(AuthPassword {
                            password: "secret".into(),
                        }),
                        transport: Transport::Tcp(TransportTcp { tcp_port: 8080 }),
                    },
                    Config2Enums {
                        name: format!("service_{}", i * 4 + 1),
                        auth: AuthMethod::Password(AuthPassword {
                            password: "secret".into(),
                        }),
                        transport: Transport::Unix(TransportUnix {
                            socket_path: "/tmp/sock".into(),
                        }),
                    },
                    Config2Enums {
                        name: format!("service_{}", i * 4 + 2),
                        auth: AuthMethod::Token(AuthToken {
                            token: "abc123".into(),
                            token_expiry: 3600,
                        }),
                        transport: Transport::Tcp(TransportTcp { tcp_port: 9090 }),
                    },
                    Config2Enums {
                        name: format!("service_{}", i * 4 + 3),
                        auth: AuthMethod::Token(AuthToken {
                            token: "xyz789".into(),
                            token_expiry: 7200,
                        }),
                        transport: Transport::Unix(TransportUnix {
                            socket_path: "/var/run/app.sock".into(),
                        }),
                    },
                ]
            })
            .collect()
    }

    static DATA: LazyLock<Vec<Config2Enums>> = LazyLock::new(make_data);
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
    fn serde_json_serialize(bencher: Bencher) {
        bencher.bench(|| black_box(serde_json::to_string(black_box(&*DATA)).unwrap()));
    }

    #[divan::bench]
    fn facet_json_deserialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_json::from_str::<Vec<Config2Enums>>(black_box(&*JSON))));
    }

    #[divan::bench]
    fn facet_format_json_deserialize(bencher: Bencher) {
        bencher.bench(|| {
            black_box(facet_format_json::from_str::<Vec<Config2Enums>>(black_box(
                &*JSON,
            )))
        });
    }

    #[divan::bench]
    fn serde_json_deserialize(bencher: Bencher) {
        bencher.bench(|| {
            black_box(serde_json::from_str::<Vec<Config2Enums>>(black_box(&*JSON)).unwrap())
        });
    }
}

/// 4 flattened enums = 16 possible configurations
/// Tests solver scaling with more configuration combinations
mod flatten_4enums {
    use super::*;

    fn make_data() -> Vec<Config4Enums> {
        // Generate all 16 combinations, repeated to get ~1000 items
        (0..64)
            .flat_map(|i| {
                // All 16 combinations for this batch
                vec![
                    // Password + Tcp + Local + File
                    Config4Enums {
                        name: format!("svc_{}_{}", i, 0),
                        auth: AuthMethod::Password(AuthPassword {
                            password: "secret".into(),
                        }),
                        transport: Transport::Tcp(TransportTcp { tcp_port: 8080 }),
                        storage: Storage::Local(StorageLocal {
                            local_path: "/data".into(),
                        }),
                        logging: Logging::File(LogFile {
                            log_path: "/var/log/app.log".into(),
                        }),
                    },
                    // Password + Tcp + Local + Stdout
                    Config4Enums {
                        name: format!("svc_{}_{}", i, 1),
                        auth: AuthMethod::Password(AuthPassword {
                            password: "secret".into(),
                        }),
                        transport: Transport::Tcp(TransportTcp { tcp_port: 8080 }),
                        storage: Storage::Local(StorageLocal {
                            local_path: "/data".into(),
                        }),
                        logging: Logging::Stdout(LogStdout { log_color: true }),
                    },
                    // Password + Tcp + Remote + File
                    Config4Enums {
                        name: format!("svc_{}_{}", i, 2),
                        auth: AuthMethod::Password(AuthPassword {
                            password: "secret".into(),
                        }),
                        transport: Transport::Tcp(TransportTcp { tcp_port: 8080 }),
                        storage: Storage::Remote(StorageRemote {
                            remote_url: "s3://bucket".into(),
                        }),
                        logging: Logging::File(LogFile {
                            log_path: "/var/log/app.log".into(),
                        }),
                    },
                    // Password + Tcp + Remote + Stdout
                    Config4Enums {
                        name: format!("svc_{}_{}", i, 3),
                        auth: AuthMethod::Password(AuthPassword {
                            password: "secret".into(),
                        }),
                        transport: Transport::Tcp(TransportTcp { tcp_port: 8080 }),
                        storage: Storage::Remote(StorageRemote {
                            remote_url: "s3://bucket".into(),
                        }),
                        logging: Logging::Stdout(LogStdout { log_color: true }),
                    },
                    // Password + Unix + Local + File
                    Config4Enums {
                        name: format!("svc_{}_{}", i, 4),
                        auth: AuthMethod::Password(AuthPassword {
                            password: "secret".into(),
                        }),
                        transport: Transport::Unix(TransportUnix {
                            socket_path: "/tmp/sock".into(),
                        }),
                        storage: Storage::Local(StorageLocal {
                            local_path: "/data".into(),
                        }),
                        logging: Logging::File(LogFile {
                            log_path: "/var/log/app.log".into(),
                        }),
                    },
                    // Password + Unix + Local + Stdout
                    Config4Enums {
                        name: format!("svc_{}_{}", i, 5),
                        auth: AuthMethod::Password(AuthPassword {
                            password: "secret".into(),
                        }),
                        transport: Transport::Unix(TransportUnix {
                            socket_path: "/tmp/sock".into(),
                        }),
                        storage: Storage::Local(StorageLocal {
                            local_path: "/data".into(),
                        }),
                        logging: Logging::Stdout(LogStdout { log_color: true }),
                    },
                    // Password + Unix + Remote + File
                    Config4Enums {
                        name: format!("svc_{}_{}", i, 6),
                        auth: AuthMethod::Password(AuthPassword {
                            password: "secret".into(),
                        }),
                        transport: Transport::Unix(TransportUnix {
                            socket_path: "/tmp/sock".into(),
                        }),
                        storage: Storage::Remote(StorageRemote {
                            remote_url: "s3://bucket".into(),
                        }),
                        logging: Logging::File(LogFile {
                            log_path: "/var/log/app.log".into(),
                        }),
                    },
                    // Password + Unix + Remote + Stdout
                    Config4Enums {
                        name: format!("svc_{}_{}", i, 7),
                        auth: AuthMethod::Password(AuthPassword {
                            password: "secret".into(),
                        }),
                        transport: Transport::Unix(TransportUnix {
                            socket_path: "/tmp/sock".into(),
                        }),
                        storage: Storage::Remote(StorageRemote {
                            remote_url: "s3://bucket".into(),
                        }),
                        logging: Logging::Stdout(LogStdout { log_color: true }),
                    },
                    // Token + Tcp + Local + File
                    Config4Enums {
                        name: format!("svc_{}_{}", i, 8),
                        auth: AuthMethod::Token(AuthToken {
                            token: "token123".into(),
                            token_expiry: 3600,
                        }),
                        transport: Transport::Tcp(TransportTcp { tcp_port: 8080 }),
                        storage: Storage::Local(StorageLocal {
                            local_path: "/data".into(),
                        }),
                        logging: Logging::File(LogFile {
                            log_path: "/var/log/app.log".into(),
                        }),
                    },
                    // Token + Tcp + Local + Stdout
                    Config4Enums {
                        name: format!("svc_{}_{}", i, 9),
                        auth: AuthMethod::Token(AuthToken {
                            token: "token123".into(),
                            token_expiry: 3600,
                        }),
                        transport: Transport::Tcp(TransportTcp { tcp_port: 8080 }),
                        storage: Storage::Local(StorageLocal {
                            local_path: "/data".into(),
                        }),
                        logging: Logging::Stdout(LogStdout { log_color: true }),
                    },
                    // Token + Tcp + Remote + File
                    Config4Enums {
                        name: format!("svc_{}_{}", i, 10),
                        auth: AuthMethod::Token(AuthToken {
                            token: "token123".into(),
                            token_expiry: 3600,
                        }),
                        transport: Transport::Tcp(TransportTcp { tcp_port: 8080 }),
                        storage: Storage::Remote(StorageRemote {
                            remote_url: "s3://bucket".into(),
                        }),
                        logging: Logging::File(LogFile {
                            log_path: "/var/log/app.log".into(),
                        }),
                    },
                    // Token + Tcp + Remote + Stdout
                    Config4Enums {
                        name: format!("svc_{}_{}", i, 11),
                        auth: AuthMethod::Token(AuthToken {
                            token: "token123".into(),
                            token_expiry: 3600,
                        }),
                        transport: Transport::Tcp(TransportTcp { tcp_port: 8080 }),
                        storage: Storage::Remote(StorageRemote {
                            remote_url: "s3://bucket".into(),
                        }),
                        logging: Logging::Stdout(LogStdout { log_color: true }),
                    },
                    // Token + Unix + Local + File
                    Config4Enums {
                        name: format!("svc_{}_{}", i, 12),
                        auth: AuthMethod::Token(AuthToken {
                            token: "token123".into(),
                            token_expiry: 3600,
                        }),
                        transport: Transport::Unix(TransportUnix {
                            socket_path: "/tmp/sock".into(),
                        }),
                        storage: Storage::Local(StorageLocal {
                            local_path: "/data".into(),
                        }),
                        logging: Logging::File(LogFile {
                            log_path: "/var/log/app.log".into(),
                        }),
                    },
                    // Token + Unix + Local + Stdout
                    Config4Enums {
                        name: format!("svc_{}_{}", i, 13),
                        auth: AuthMethod::Token(AuthToken {
                            token: "token123".into(),
                            token_expiry: 3600,
                        }),
                        transport: Transport::Unix(TransportUnix {
                            socket_path: "/tmp/sock".into(),
                        }),
                        storage: Storage::Local(StorageLocal {
                            local_path: "/data".into(),
                        }),
                        logging: Logging::Stdout(LogStdout { log_color: true }),
                    },
                    // Token + Unix + Remote + File
                    Config4Enums {
                        name: format!("svc_{}_{}", i, 14),
                        auth: AuthMethod::Token(AuthToken {
                            token: "token123".into(),
                            token_expiry: 3600,
                        }),
                        transport: Transport::Unix(TransportUnix {
                            socket_path: "/tmp/sock".into(),
                        }),
                        storage: Storage::Remote(StorageRemote {
                            remote_url: "s3://bucket".into(),
                        }),
                        logging: Logging::File(LogFile {
                            log_path: "/var/log/app.log".into(),
                        }),
                    },
                    // Token + Unix + Remote + Stdout
                    Config4Enums {
                        name: format!("svc_{}_{}", i, 15),
                        auth: AuthMethod::Token(AuthToken {
                            token: "token123".into(),
                            token_expiry: 3600,
                        }),
                        transport: Transport::Unix(TransportUnix {
                            socket_path: "/tmp/sock".into(),
                        }),
                        storage: Storage::Remote(StorageRemote {
                            remote_url: "s3://bucket".into(),
                        }),
                        logging: Logging::Stdout(LogStdout { log_color: true }),
                    },
                ]
            })
            .collect()
    }

    static DATA: LazyLock<Vec<Config4Enums>> = LazyLock::new(make_data);
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
    fn serde_json_serialize(bencher: Bencher) {
        bencher.bench(|| black_box(serde_json::to_string(black_box(&*DATA)).unwrap()));
    }

    #[divan::bench]
    fn facet_json_deserialize(bencher: Bencher) {
        bencher.bench(|| black_box(facet_json::from_str::<Vec<Config4Enums>>(black_box(&*JSON))));
    }

    #[divan::bench]
    fn facet_format_json_deserialize(bencher: Bencher) {
        bencher.bench(|| {
            black_box(facet_format_json::from_str::<Vec<Config4Enums>>(black_box(
                &*JSON,
            )))
        });
    }

    #[divan::bench]
    fn serde_json_deserialize(bencher: Bencher) {
        bencher.bench(|| {
            black_box(serde_json::from_str::<Vec<Config4Enums>>(black_box(&*JSON)).unwrap())
        });
    }
}
