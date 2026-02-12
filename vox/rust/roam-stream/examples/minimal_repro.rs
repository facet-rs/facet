//! Minimal reproduction of the serialization bug.
//!
//! Run with: RUSTFLAGS="-Z sanitizer=address" cargo +nightly run -Z build-std --target aarch64-unknown-linux-gnu --example minimal_repro

use std::collections::HashMap;

#[derive(facet::Facet, Clone, Debug)]
struct ComplexData {
    id: u64,
    name: String,
    data: Vec<u8>,
    nested: NestedData,
    tags: Vec<String>,
    metadata: HashMap<String, String>,
}

#[derive(facet::Facet, Clone, Debug)]
struct NestedData {
    timestamp: u64,
    values: Vec<f64>,
    flags: Vec<bool>,
}

fn main() {
    println!("Testing facet serialization of complex types...");

    for size in [100, 1024, 10 * 1024, 50 * 1024, 100 * 1024] {
        println!("Testing size: {} bytes", size);

        for i in 0..100 {
            let data = ComplexData {
                id: i,
                name: format!("test-{}", i),
                data: vec![(i % 256) as u8; size],
                nested: NestedData {
                    timestamp: i,
                    values: vec![1.0, 2.0, 3.0, (i as f64) * 0.5],
                    flags: vec![i % 2 == 0, i % 3 == 0],
                },
                tags: vec![format!("tag-{}", i), "test".to_string()],
                metadata: [
                    ("key1".to_string(), "value1".to_string()),
                    ("key2".to_string(), format!("value-{}", i)),
                ]
                .into_iter()
                .collect(),
            };

            // Serialize
            let bytes = facet_postcard::to_vec(&data).expect("serialize failed");

            // Deserialize
            let _decoded: ComplexData =
                facet_postcard::from_slice(&bytes).expect("deserialize failed");
        }

        println!("  ✓ {} iterations completed", 100);
    }

    println!();
    println!("✓ All tests passed!");
}
