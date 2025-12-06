//! Benchmark comparing facet_json vs serde_json deserialization performance.
//!
//! Uses the twitter.json corpus from simdjson benchmarks.

use divan::{Bencher, black_box};
use facet::Facet;
use serde::{Deserialize, Serialize};

fn main() {
    divan::main();
}

// =============================================================================
// Corpus loading
// =============================================================================

static TWITTER_JSON: &str = include_str!("corpus/twitter.json");

// =============================================================================
// Sparse struct definitions (only essential fields, rest skipped)
// =============================================================================

/// Sparse Twitter response - only a few key fields
#[derive(Facet, Deserialize, Serialize, Debug)]
struct TwitterResponseSparse {
    statuses: Vec<StatusSparse>,
}

#[derive(Facet, Deserialize, Serialize, Debug)]
struct StatusSparse {
    id: u64,
    text: String,
    user: UserSparse,
    retweet_count: u32,
    favorite_count: u32,
}

#[derive(Facet, Deserialize, Serialize, Debug)]
struct UserSparse {
    id: u64,
    screen_name: String,
    followers_count: u32,
}

// =============================================================================
// Benchmarks: Deserialization into typed structs (sparse)
// =============================================================================

#[divan::bench]
fn facet_sparse_struct(bencher: Bencher) {
    bencher.bench(|| {
        let result: TwitterResponseSparse = facet_json::from_str(black_box(TWITTER_JSON)).unwrap();
        black_box(result)
    });
}

#[divan::bench]
fn serde_sparse_struct(bencher: Bencher) {
    bencher.bench(|| {
        let result: TwitterResponseSparse = serde_json::from_str(black_box(TWITTER_JSON)).unwrap();
        black_box(result)
    });
}

// =============================================================================
// Benchmarks: Deserialization into dynamic Value types
// =============================================================================

#[divan::bench]
fn facet_value(bencher: Bencher) {
    bencher.bench(|| {
        let result: facet_value::Value = facet_json::from_str(black_box(TWITTER_JSON)).unwrap();
        black_box(result)
    });
}

#[divan::bench]
fn serde_value(bencher: Bencher) {
    bencher.bench(|| {
        let result: serde_json::Value = serde_json::from_str(black_box(TWITTER_JSON)).unwrap();
        black_box(result)
    });
}

// =============================================================================
// Benchmarks: Serialization (Value)
// =============================================================================

#[divan::bench]
fn facet_serialize_value(bencher: Bencher) {
    let data: facet_value::Value = facet_json::from_str(TWITTER_JSON).unwrap();
    bencher.bench(|| {
        let result = facet_json::to_string(black_box(&data));
        black_box(result)
    });
}

#[divan::bench]
fn serde_serialize_value(bencher: Bencher) {
    let data: serde_json::Value = serde_json::from_str(TWITTER_JSON).unwrap();
    bencher.bench(|| {
        let result = serde_json::to_string(black_box(&data)).unwrap();
        black_box(result)
    });
}

// =============================================================================
// Benchmarks: Serialization (sparse struct)
// =============================================================================

#[divan::bench]
fn facet_serialize_sparse(bencher: Bencher) {
    let data: TwitterResponseSparse = facet_json::from_str(TWITTER_JSON).unwrap();
    bencher.bench(|| {
        let result = facet_json::to_string(black_box(&data));
        black_box(result)
    });
}

#[divan::bench]
fn serde_serialize_sparse(bencher: Bencher) {
    let data: TwitterResponseSparse = serde_json::from_str(TWITTER_JSON).unwrap();
    bencher.bench(|| {
        let result = serde_json::to_string(black_box(&data)).unwrap();
        black_box(result)
    });
}
