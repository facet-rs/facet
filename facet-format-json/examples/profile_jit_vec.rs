//! Profile facet-format JIT Vec deserialization
//!
//! Tests both Tier-1 (shape JIT over events) and Tier-2 (format JIT direct byte parsing).
//!
//! Run with: cargo run --release --example profile_jit_vec --features jit

use facet_format::FormatJitParser;
use facet_format::jit;
use facet_format_json::JsonParser;

fn main() {
    let data: Vec<bool> = (0..1024).map(|i| i % 2 == 0).collect();
    let json = serde_json::to_vec(&data).unwrap();

    println!("Input: {} bools, {} bytes of JSON", data.len(), json.len());

    // Test Tier-2 (format JIT)
    println!("\n--- Tier-2 Format JIT (direct byte parsing) ---");
    {
        println!(
            "  is_format_jit_compatible: {}",
            jit::is_format_jit_compatible::<Vec<bool>>()
        );

        let mut parser = JsonParser::new(&json);
        println!("  jit_pos: {:?}", parser.jit_pos());
        println!("  jit_input len: {}", parser.jit_input().len());

        match jit::try_deserialize_format::<Vec<bool>, JsonParser<'_>>(&mut parser) {
            Some(Ok(result)) => {
                println!("Tier-2 success! Got {} bools", result.len());
                assert_eq!(result, data);
            }
            Some(Err(e)) => {
                println!("Tier-2 error: {:?}", e);
            }
            None => {
                println!("Tier-2 not available (type not Tier-2 compatible or compilation failed)");
            }
        }
    }

    // Test Tier-1 (shape JIT)
    println!("\n--- Tier-1 Shape JIT (events + shape) ---");
    {
        let mut parser = JsonParser::new(&json);
        match jit::try_deserialize::<Vec<bool>, JsonParser<'_>>(&mut parser) {
            Some(Ok(result)) => {
                println!("Tier-1 success! Got {} bools", result.len());
                assert_eq!(result, data);
            }
            Some(Err(e)) => {
                println!("Tier-1 error: {:?}", e);
            }
            None => {
                println!("Tier-1 not available (type not JIT compatible)");
            }
        }
    }

    // Benchmark Tier-2
    println!("\n--- Benchmarking Tier-2 (10,000 iterations) ---");
    let start = std::time::Instant::now();
    for _ in 0..10_000 {
        let mut parser = JsonParser::new(&json);
        let result = jit::try_deserialize_format::<Vec<bool>, JsonParser<'_>>(&mut parser);
        std::hint::black_box(result);
    }
    let tier2_time = start.elapsed();
    println!(
        "Tier-2: {:?} ({:.2} ns/iter)",
        tier2_time,
        tier2_time.as_nanos() as f64 / 10_000.0
    );

    // Benchmark Tier-1
    println!("\n--- Benchmarking Tier-1 (10,000 iterations) ---");
    let start = std::time::Instant::now();
    for _ in 0..10_000 {
        let mut parser = JsonParser::new(&json);
        let result = jit::try_deserialize::<Vec<bool>, JsonParser<'_>>(&mut parser);
        std::hint::black_box(result);
    }
    let tier1_time = start.elapsed();
    println!(
        "Tier-1: {:?} ({:.2} ns/iter)",
        tier1_time,
        tier1_time.as_nanos() as f64 / 10_000.0
    );

    // Speedup
    let speedup = tier1_time.as_nanos() as f64 / tier2_time.as_nanos() as f64;
    println!("\n--- Speedup: {:.2}x ---", speedup);
}
