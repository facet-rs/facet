//! Test that the macro rejects methods with owned `self` receiver.
//!
//! RPC services must use `&self` to allow concurrent request handling.

#[rapace_macros::service]
trait Calculator {
    async fn add(self, a: i32, b: i32) -> i32;
}

fn main() {}
