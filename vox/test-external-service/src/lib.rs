//! Test crate to verify external services can use auto-generated ServiceDispatch wrappers.
//!
//! This validates the fix for issue #100.

/// A simple test service defined in an external crate (not rapace-cell)
#[rapace::service]
pub trait Calculator {
    async fn add(&self, a: i32, b: i32) -> i32;
}

/// Implementation of Calculator
pub struct CalculatorImpl;

impl Calculator for CalculatorImpl {
    async fn add(&self, a: i32, b: i32) -> i32 {
        a + b
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rapace_cell::{DispatcherBuilder, ServiceDispatch};

    #[test]
    fn calculator_has_into_dispatch() {
        // Create a CalculatorServer
        let server = CalculatorServer::new(CalculatorImpl);

        // The auto-generated into_dispatch() method should be available
        let dispatch = server.into_dispatch();

        // Verify it implements ServiceDispatch
        let method_ids = dispatch.method_ids();
        assert!(
            !method_ids.is_empty(),
            "Should have method IDs for Calculator service"
        );
    }

    #[test]
    fn calculator_dispatch_type_exists() {
        // Verify that CalculatorDispatch type was auto-generated
        let server = CalculatorServer::new(CalculatorImpl);
        let _dispatch: CalculatorDispatch<_> = server.into_dispatch();
    }

    #[test]
    fn can_build_dispatcher_with_external_service() {
        // This is the key test - we should be able to use DispatcherBuilder
        // with a service defined outside of rapace-cell
        let server = CalculatorServer::new(CalculatorImpl);
        let buffer_pool = rapace::BufferPool::new();

        let _dispatcher = DispatcherBuilder::new()
            .add_service(server.into_dispatch())
            .build(buffer_pool);

        // If we got here, the wrapper was successfully generated!
    }
}
