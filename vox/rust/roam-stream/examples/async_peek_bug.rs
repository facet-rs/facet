//! Reproduce the async + Peek lifetime bug.
//!
//! Run with: RUSTFLAGS="-Z sanitizer=address" cargo +nightly run -Z build-std --target aarch64-unknown-linux-gnu --example async_peek_bug

async fn serialize_and_send(data: facet::Peek<'_, 'static>) -> Vec<u8> {
    // Simulate what send_ok_response does
    let bytes = facet_postcard::peek_to_vec(data).expect("serialize failed");

    // Simulate async send
    tokio::time::sleep(std::time::Duration::from_micros(1)).await;

    bytes
}

async fn process_result(result: Result<Vec<u8>, ()>) {
    if let Ok(ref ok_result) = result {
        // This mimics dispatch_call's pattern
        let peek = facet::Peek::new(ok_result);

        // Call async function that uses the peek
        let _bytes = serialize_and_send(peek).await;
    }
}

#[tokio::main]
async fn main() {
    println!("Testing async + Peek pattern...");

    for i in 0..1000 {
        let big_data = vec![(i % 256) as u8; 100 * 1024];
        let result: Result<Vec<u8>, ()> = Ok(big_data);

        process_result(result).await;

        if i % 100 == 0 {
            println!("  Completed {} iterations", i);
        }
    }

    println!("✓ All tests passed!");
}
