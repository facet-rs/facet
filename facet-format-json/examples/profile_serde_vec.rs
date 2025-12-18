//! Profile serde_json Vec deserialization
//! Run with: valgrind --tool=callgrind ./target/release/examples/profile_serde_vec

fn main() {
    let data: Vec<bool> = (0..1024).map(|i| i % 2 == 0).collect();
    let json = serde_json::to_vec(&data).unwrap();

    for _ in 0..10_000 {
        let result: Vec<bool> = serde_json::from_slice(&json).unwrap();
        std::hint::black_box(result);
    }
}
