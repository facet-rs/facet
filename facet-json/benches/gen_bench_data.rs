//! Generate JSON data files for array benchmarks
//!
//! Run with: cargo run --bin gen_bench_data -p facet-json

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

    println!("\n🎉 All array benchmark data generated!");
    println!("   Data dir: {}", data_dir.display());
}
