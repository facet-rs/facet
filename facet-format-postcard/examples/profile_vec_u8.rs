//! Profile vec_u8 parsing

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let which = args.get(1).map(|s| s.as_str()).unwrap_or("facet");
    let size: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(1000);
    let iterations: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(100000);

    // Generate test data
    let data: Vec<u8> = (0..size).map(|i| (i % 256) as u8).collect();
    let encoded = postcard::to_allocvec(&data).unwrap();

    // Correctness check (assert-before-bench pattern)
    let facet_result: Vec<u8> = facet_format_postcard::from_slice(&encoded).unwrap();
    assert_eq!(facet_result, data, "facet correctness check failed");

    eprintln!(
        "Running {} for {} iterations, size={}",
        which, iterations, size
    );

    match which {
        "facet" => {
            for _ in 0..iterations {
                let _: Vec<u8> = std::hint::black_box(
                    facet_format_postcard::from_slice(std::hint::black_box(&encoded)).unwrap(),
                );
            }
        }
        "postcard" => {
            use serde::Deserialize;
            #[derive(Deserialize)]
            struct B(#[serde(with = "serde_bytes")] Vec<u8>);

            for _ in 0..iterations {
                let _: B = std::hint::black_box(
                    postcard::from_bytes(std::hint::black_box(&encoded)).unwrap(),
                );
            }
        }
        _ => eprintln!("Usage: profile_vec_u8 [facet|postcard] [size] [iterations]"),
    }
}
