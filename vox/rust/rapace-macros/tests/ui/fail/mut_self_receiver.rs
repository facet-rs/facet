//! Test that the macro rejects methods with `&mut self` receiver.
//!
//! RPC services must use `&self` (not `&mut self`) because:
//! 1. Multiple requests must be handled concurrently
//! 2. The server wraps the service in Arc, which only provides &T
//! 3. Interior mutability (Mutex, RwLock) should be used for mutable state

#[rapace_macros::service]
trait Counter {
    async fn increment(&mut self) -> u32;
}

fn main() {}
