//! Benchmark comparing facet_json vs facet_format_json for slices.

use divan::{Bencher, black_box};
use facet::Facet;

fn main() {
    divan::main();
}

#[derive(Facet, Clone)]
struct Item {
    id: u64,
    name: String,
    value: f64,
}

fn make_items(n: usize) -> Vec<Item> {
    (0..n)
        .map(|i| Item {
            id: i as u64,
            name: format!("item_{}", i),
            value: i as f64 * 1.5,
        })
        .collect()
}

#[divan::bench(consts = [10, 100, 1000])]
fn facet_json_serialize<const N: usize>(bencher: Bencher) {
    let items = make_items(N);
    bencher.bench_local(|| black_box(facet_json::to_string(&items)));
}

#[divan::bench(consts = [10, 100, 1000])]
fn facet_format_json_serialize<const N: usize>(bencher: Bencher) {
    let items = make_items(N);
    bencher.bench_local(|| black_box(facet_format_json::to_string(&items)));
}

#[divan::bench(consts = [10, 100, 1000])]
fn facet_json_deserialize<const N: usize>(bencher: Bencher) {
    let items = make_items(N);
    let json = facet_json::to_string(&items);
    bencher.bench_local(|| black_box(facet_json::from_str::<Vec<Item>>(&json)));
}

#[divan::bench(consts = [10, 100, 1000])]
fn facet_format_json_deserialize<const N: usize>(bencher: Bencher) {
    let items = make_items(N);
    let json = facet_format_json::to_string(&items).unwrap();
    bencher.bench_local(|| black_box(facet_format_json::from_str::<Vec<Item>>(&json)));
}
