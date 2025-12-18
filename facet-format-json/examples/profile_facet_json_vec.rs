//! Profile facet-json JIT Vec deserialization
//! Run with: valgrind --tool=callgrind ./target/release/examples/profile_facet_json_vec

fn main() {
    let data: Vec<bool> = (0..1024).map(|i| i % 2 == 0).collect();
    let json = serde_json::to_string(&data).unwrap();

    for _ in 0..10_000 {
        let result: Vec<bool> = facet_json::cranelift::from_str(&json).unwrap();
        std::hint::black_box(result);
    }
}
