//! Profile vec_u32 parsing

fn main() {
    // Generate test data
    let data: Vec<u32> = (0..1000)
        .map(|i| match i % 4 {
            0 => i as u32,
            1 => (i as u32) * 100,
            2 => (i as u32) * 10000,
            _ => (i as u32) * 1000000,
        })
        .collect();

    let encoded = postcard::to_allocvec(&data).unwrap();

    // Correctness checks (assert-before-bench pattern)
    let facet_result: Vec<u32> = facet_postcard::from_slice(&encoded).unwrap();
    assert_eq!(facet_result, data, "facet correctness check failed");
    let postcard_result: Vec<u32> = postcard::from_bytes(&encoded).unwrap();
    assert_eq!(postcard_result, data, "postcard correctness check failed");

    let args: Vec<String> = std::env::args().collect();
    let which = args.get(1).map(|s| s.as_str()).unwrap_or("facet");
    let iterations = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(100000);

    match which {
        "facet" => {
            eprintln!("Running facet JIT for {} iterations", iterations);
            for _ in 0..iterations {
                let _: Vec<u32> = std::hint::black_box(
                    facet_postcard::from_slice(std::hint::black_box(&encoded)).unwrap(),
                );
            }
        }
        "postcard" => {
            eprintln!("Running postcard for {} iterations", iterations);
            for _ in 0..iterations {
                let _: Vec<u32> = std::hint::black_box(
                    postcard::from_bytes(std::hint::black_box(&encoded)).unwrap(),
                );
            }
        }
        _ => {
            eprintln!("Usage: profile_vec_u32 [facet|postcard] [iterations]");
        }
    }
}
