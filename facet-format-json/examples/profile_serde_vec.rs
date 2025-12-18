//! Profile serde_json Vec deserialization
//! Run with: cargo run --release --example profile_serde_vec

fn main() {
    let data: Vec<bool> = (0..1024).map(|i| i % 2 == 0).collect();
    let json = serde_json::to_vec(&data).unwrap();

    println!("Input: {} bools, {} bytes of JSON", data.len(), json.len());

    // Warmup
    for _ in 0..100 {
        let result: Vec<bool> = serde_json::from_slice(&json).unwrap();
        std::hint::black_box(result);
    }

    println!("\n--- Benchmarking serde_json (10,000 iterations) ---");
    let start = std::time::Instant::now();
    for _ in 0..10_000 {
        let result: Vec<bool> = serde_json::from_slice(&json).unwrap();
        std::hint::black_box(result);
    }
    let elapsed = start.elapsed();
    println!(
        "serde_json: {:?} ({:.2} ns/iter)",
        elapsed,
        elapsed.as_nanos() as f64 / 10_000.0
    );
}
