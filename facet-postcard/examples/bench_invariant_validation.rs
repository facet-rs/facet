//! Quick benchmark for decode scaling on Vec<u8>.
//!
//! Usage:
//!   cargo run -p facet-postcard --release --example bench_invariant_validation
//!   cargo run -p facet-postcard --release --example bench_invariant_validation -- 100000 64,256,1024

use std::time::Instant;

use std::hint::black_box;

fn parse_sizes(arg: Option<&str>) -> Vec<usize> {
    match arg {
        Some(csv) => csv
            .split(',')
            .filter_map(|s| s.trim().parse::<usize>().ok())
            .filter(|n| *n > 0)
            .collect(),
        None => vec![64, 256, 1024, 4096, 16384, 65536],
    }
}

fn bench_owned(encoded: &[u8], iterations: usize) -> std::time::Duration {
    let start = Instant::now();
    for _ in 0..iterations {
        let value: Vec<u8> = black_box(facet_postcard::from_slice(black_box(encoded)).unwrap());
        black_box(value);
    }
    start.elapsed()
}

fn bench_borrowed(encoded: &[u8], iterations: usize) -> std::time::Duration {
    let start = Instant::now();
    for _ in 0..iterations {
        let value: Vec<u8> =
            black_box(facet_postcard::from_slice_borrowed(black_box(encoded)).unwrap());
        black_box(value);
    }
    start.elapsed()
}

fn ns_per_iter(duration: std::time::Duration, iterations: usize) -> f64 {
    duration.as_nanos() as f64 / iterations as f64
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let iterations = args
        .get(1)
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(20_000);
    let sizes = parse_sizes(args.get(2).map(String::as_str));

    eprintln!(
        "iterations per size: {iterations}, sizes: {}",
        sizes
            .iter()
            .map(|n| n.to_string())
            .collect::<Vec<_>>()
            .join(",")
    );
    println!("size,owned_ns,borrowed_ns,borrowed_over_owned");

    for size in sizes {
        let data: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();
        let encoded = postcard::to_allocvec(&data).unwrap();

        // correctness checks before timing
        let owned_check: Vec<u8> = facet_postcard::from_slice(&encoded).unwrap();
        assert_eq!(owned_check, data, "owned decode failed for size={size}");
        let borrowed_check: Vec<u8> = facet_postcard::from_slice_borrowed(&encoded).unwrap();
        assert_eq!(
            borrowed_check, data,
            "borrowed decode failed for size={size}"
        );

        let owned = bench_owned(&encoded, iterations);
        let borrowed = bench_borrowed(&encoded, iterations);

        let owned_ns = ns_per_iter(owned, iterations);
        let borrowed_ns = ns_per_iter(borrowed, iterations);
        let ratio = borrowed_ns / owned_ns;

        println!("{size},{owned_ns:.2},{borrowed_ns:.2},{ratio:.3}");
    }
}
