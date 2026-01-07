//! Test consumer crate that uses generated code from build.rs.

// Include the generated code
include!(concat!(env!("OUT_DIR"), "/generated.rs"));

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn method_ids_generated() {
        // Verify method ID constants are generated
        assert_ne!(method_id::ADD, 0);
        assert_ne!(method_id::MULTIPLY, 0);
        // They should be different
        assert_ne!(method_id::ADD, method_id::MULTIPLY);
    }

    // Test that we can implement the server trait
    struct TestCalculator;

    #[allow(clippy::manual_async_fn)]
    impl CalculatorServer for TestCalculator {
        fn add(
            &self,
            a: i32,
            b: i32,
        ) -> impl std::future::Future<
            Output = Result<i32, Box<dyn std::error::Error + Send + Sync>>,
        > + Send {
            async move { Ok(a + b) }
        }

        fn multiply(
            &self,
            a: i32,
            b: i32,
        ) -> impl std::future::Future<
            Output = Result<i32, Box<dyn std::error::Error + Send + Sync>>,
        > + Send {
            async move { Ok(a * b) }
        }

        fn sum_stream(
            &self,
            _numbers: Pull<i32>,
        ) -> impl std::future::Future<
            Output = Result<i64, Box<dyn std::error::Error + Send + Sync>>,
        > + Send {
            async move { Ok(0) } // Stub implementation
        }

        fn range(
            &self,
            _count: u32,
        ) -> impl std::future::Future<
            Output = Result<Push<u32>, Box<dyn std::error::Error + Send + Sync>>,
        > + Send {
            async move { Err("not implemented".into()) } // Stub implementation
        }
    }

    #[test]
    fn can_create_dispatcher() {
        let _dispatcher = CalculatorDispatcher::new(TestCalculator);
        // Dispatcher implements ServiceDispatcher trait
    }
}
