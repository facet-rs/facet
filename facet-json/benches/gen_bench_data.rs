//! Generate JSON data files for benchmarks
//!
//! Run with: cargo run --bin gen_bench_data -p facet-json

use facet::Facet;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

fn main() {
    let data_dir = Path::new("facet-json/benches/data");
    fs::create_dir_all(data_dir);

    // Booleans - 10,000 alternating
    let booleans: Vec<bool> = (0..10000).map(|i| i % 2 == 0).collect();
    let json = facet_json::to_string(&booleans);
    fs::write(data_dir.join("booleans.json"), json);
    println!("✅ Generated booleans.json ({} items)", booleans.len());

    // Integers - 1,000 multiplied
    let integers: Vec<u64> = (0..1000).map(|i| i * 12345678901234).collect();
    let json = facet_json::to_string(&integers);
    fs::write(data_dir.join("integers.json"), json);
    println!("✅ Generated integers.json ({} items)", integers.len());

    // Floats - 1,000 multiplied
    let floats: Vec<f64> = (0..1000).map(|i| i as f64 * 1.23456789).collect();
    let json = facet_json::to_string(&floats);
    fs::write(data_dir.join("floats.json"), json);
    println!("✅ Generated floats.json ({} items)", floats.len());

    // Short strings - 1,000 items, ~10 chars each
    let short_strings: Vec<String> = (0..1000).map(|i| format!("str_{:06}", i)).collect();
    let json = facet_json::to_string(&short_strings);
    fs::write(data_dir.join("short_strings.json"), json);
    println!(
        "✅ Generated short_strings.json ({} items)",
        short_strings.len()
    );

    // Long strings - 100 items, 1000 chars each
    let long_strings: Vec<String> = (0..100)
        .map(|i| "x".repeat(1000) + &format!("_{}", i))
        .collect();
    let json = facet_json::to_string(&long_strings);
    fs::write(data_dir.join("long_strings.json"), json);
    println!(
        "✅ Generated long_strings.json ({} items)",
        long_strings.len()
    );

    // Escaped strings - 1,000 items with various escapes
    let escaped_strings: Vec<String> = (0..1000)
        .map(|i| format!("line_{}\nwith\ttabs\tand \"quotes\" and \\backslashes\\", i))
        .collect();
    let json = facet_json::to_string(&escaped_strings);
    fs::write(data_dir.join("escaped_strings.json"), json);
    println!(
        "✅ Generated escaped_strings.json ({} items)",
        escaped_strings.len()
    );

    // Hashmaps - 1,000 entries
    let hashmaps: HashMap<String, u64> = (0..1000).map(|i| (format!("key_{}", i), i * 2)).collect();
    let json = facet_json::to_string(&hashmaps);
    fs::write(data_dir.join("hashmaps.json"), json);
    println!("✅ Generated hashmaps.json ({} entries)", hashmaps.len());

    // Nested structs - Vec<Outer> with 3-level deep nesting (500 items)
    #[derive(Facet)]
    struct NestedOuter {
        id: u64,
        inner: NestedInner,
    }

    #[derive(Facet)]
    struct NestedInner {
        name: String,
        value: f64,
        deep: NestedDeep,
    }

    #[derive(Facet)]
    struct NestedDeep {
        flag: bool,
        count: u32,
    }

    let nested_data: Vec<NestedOuter> = (0..500)
        .map(|i| NestedOuter {
            id: i,
            inner: NestedInner {
                name: format!("name_{}", i),
                value: i as f64 * 1.5,
                deep: NestedDeep {
                    flag: i % 2 == 0,
                    count: i as u32 * 10,
                },
            },
        })
        .collect();
    let json = facet_json::to_string(&nested_data);
    fs::write(data_dir.join("nested_structs.json"), json);
    println!(
        "✅ Generated nested_structs.json ({} items)",
        nested_data.len()
    );

    // Options - Vec<MaybeData> with Option fields (500 items)
    #[derive(Facet)]
    struct OptionsMaybeData {
        required: u64,
        optional_string: Option<String>,
        optional_number: Option<f64>,
    }

    let options_data: Vec<OptionsMaybeData> = (0..500)
        .map(|i| OptionsMaybeData {
            required: i,
            optional_string: if i % 2 == 0 {
                Some(format!("str_{}", i))
            } else {
                None
            },
            optional_number: if i % 3 == 0 { Some(i as f64) } else { None },
        })
        .collect();
    let json = facet_json::to_string(&options_data);
    fs::write(data_dir.join("options.json"), json);
    println!("✅ Generated options.json ({} items)", options_data.len());

    println!("\n🎉 All benchmark data generated!");
    println!("   Total: 9 data files");
}
