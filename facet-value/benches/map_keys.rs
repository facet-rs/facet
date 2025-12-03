use divan::{Bencher, black_box};
use facet_value::{VObject, Value};
use kstring::KString;
use std::collections::HashMap;

fn main() {
    divan::main();
}

// --- Insert benchmarks (short keys) ----------------------------------------------------------

#[divan::bench(args = [16, 64, 256, 512])]
fn insert_vobject_short_keys(bencher: Bencher, entries: usize) {
    run_insert_bench(
        bencher,
        entries,
        KeyShape::InlineFriendly,
        MapFlavor::FacetObject,
    );
}

#[divan::bench(args = [16, 64, 256, 512])]
fn insert_string_map_short_keys(bencher: Bencher, entries: usize) {
    run_insert_bench(
        bencher,
        entries,
        KeyShape::InlineFriendly,
        MapFlavor::StdString,
    );
}

#[divan::bench(args = [16, 64, 256, 512])]
fn insert_kstring_map_short_keys(bencher: Bencher, entries: usize) {
    run_insert_bench(
        bencher,
        entries,
        KeyShape::InlineFriendly,
        MapFlavor::KString,
    );
}

// --- Insert benchmarks (long keys) -----------------------------------------------------------

#[divan::bench(args = [16, 64, 256, 512])]
fn insert_vobject_long_keys(bencher: Bencher, entries: usize) {
    run_insert_bench(bencher, entries, KeyShape::HeapOnly, MapFlavor::FacetObject);
}

#[divan::bench(args = [16, 64, 256, 512])]
fn insert_string_map_long_keys(bencher: Bencher, entries: usize) {
    run_insert_bench(bencher, entries, KeyShape::HeapOnly, MapFlavor::StdString);
}

#[divan::bench(args = [16, 64, 256, 512])]
fn insert_kstring_map_long_keys(bencher: Bencher, entries: usize) {
    run_insert_bench(bencher, entries, KeyShape::HeapOnly, MapFlavor::KString);
}

// --- Collect benchmarks (Vec<(key, value)> -> map) ------------------------------------------

#[divan::bench(args = [16, 64, 256, 512])]
fn collect_vobject_short_keys(bencher: Bencher, entries: usize) {
    run_collect_bench(
        bencher,
        entries,
        KeyShape::InlineFriendly,
        MapFlavor::FacetObject,
    );
}

#[divan::bench(args = [16, 64, 256, 512])]
fn collect_string_map_short_keys(bencher: Bencher, entries: usize) {
    run_collect_bench(
        bencher,
        entries,
        KeyShape::InlineFriendly,
        MapFlavor::StdString,
    );
}

#[divan::bench(args = [16, 64, 256, 512])]
fn collect_kstring_map_short_keys(bencher: Bencher, entries: usize) {
    run_collect_bench(
        bencher,
        entries,
        KeyShape::InlineFriendly,
        MapFlavor::KString,
    );
}

#[divan::bench(args = [16, 64, 256, 512])]
fn collect_vobject_long_keys(bencher: Bencher, entries: usize) {
    run_collect_bench(bencher, entries, KeyShape::HeapOnly, MapFlavor::FacetObject);
}

#[divan::bench(args = [16, 64, 256, 512])]
fn collect_string_map_long_keys(bencher: Bencher, entries: usize) {
    run_collect_bench(bencher, entries, KeyShape::HeapOnly, MapFlavor::StdString);
}

#[divan::bench(args = [16, 64, 256, 512])]
fn collect_kstring_map_long_keys(bencher: Bencher, entries: usize) {
    run_collect_bench(bencher, entries, KeyShape::HeapOnly, MapFlavor::KString);
}

#[derive(Copy, Clone)]
enum KeyShape {
    InlineFriendly,
    HeapOnly,
}

#[derive(Copy, Clone)]
enum MapFlavor {
    FacetObject,
    StdString,
    KString,
}

fn run_insert_bench(bencher: Bencher, entries: usize, shape: KeyShape, flavor: MapFlavor) {
    let keys = build_keys(entries, shape);
    bencher.bench(move || match flavor {
        MapFlavor::FacetObject => {
            let mut object = VObject::with_capacity(entries);
            for (idx, key) in keys.iter().enumerate() {
                object.insert(key.as_str(), Value::from(idx as i64));
            }
            black_box(object);
        }
        MapFlavor::StdString => {
            let mut map = HashMap::with_capacity(entries);
            for (idx, key) in keys.iter().enumerate() {
                map.insert(key.clone(), Value::from(idx as i64));
            }
            black_box(map);
        }
        MapFlavor::KString => {
            let mut map = HashMap::with_capacity(entries);
            for (idx, key) in keys.iter().enumerate() {
                map.insert(KString::from(key.clone()), Value::from(idx as i64));
            }
            black_box(map);
        }
    });
}

fn run_collect_bench(bencher: Bencher, entries: usize, shape: KeyShape, flavor: MapFlavor) {
    let keys = build_keys(entries, shape);
    bencher.bench(move || match flavor {
        MapFlavor::FacetObject => {
            let value: Value = keys
                .iter()
                .enumerate()
                .map(|(idx, key)| (key.as_str(), Value::from(idx as i64)))
                .collect();
            black_box(value);
        }
        MapFlavor::StdString => {
            let map: HashMap<String, Value> = keys
                .iter()
                .enumerate()
                .map(|(idx, key)| (key.clone(), Value::from(idx as i64)))
                .collect();
            black_box(map);
        }
        MapFlavor::KString => {
            let map: HashMap<KString, Value> = keys
                .iter()
                .enumerate()
                .map(|(idx, key)| (KString::from(key.clone()), Value::from(idx as i64)))
                .collect();
            black_box(map);
        }
    });
}

fn build_keys(entries: usize, shape: KeyShape) -> Vec<String> {
    match shape {
        KeyShape::InlineFriendly => (0..entries).map(|idx| format!("k{:03}", idx)).collect(),
        KeyShape::HeapOnly => (0..entries)
            .map(|idx| format!("very-long-key-{idx:08}-suffix"))
            .collect(),
    }
}
