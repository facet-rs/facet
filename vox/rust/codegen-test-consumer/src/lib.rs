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

    // Test that we can implement the handler trait
    struct TestCalculator;

    #[allow(clippy::manual_async_fn)]
    impl CalculatorHandler for TestCalculator {
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
            _output: Push<u32>,
        ) -> impl std::future::Future<Output = Result<(), Box<dyn std::error::Error + Send + Sync>>> + Send
        {
            async move { Ok(()) } // Stub implementation
        }
    }

    #[test]
    fn can_create_dispatcher() {
        let _dispatcher = CalculatorDispatcher::new(TestCalculator);
        // Dispatcher implements ServiceDispatcher trait
    }

    /// Test that Rust-generated client can talk to Rust-generated server over TCP.
    #[tokio::test]
    async fn rust_to_rust_tcp_roundtrip() {
        use roam::__private::facet_postcard;
        use roam_tcp::{Message, Server};
        use std::time::Duration;
        use tokio::net::TcpListener;

        // 1. Bind listener on random port
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // 2. Spawn server task with CalculatorDispatcher
        let server_handle = tokio::spawn(async move {
            let server = Server::new();
            let mut conn = server.accept(&listener).await.unwrap();
            let dispatcher = CalculatorDispatcher::new(TestCalculator);
            conn.run(&dispatcher).await
        });

        // 3. Connect as client
        let client = Server::new();
        let mut conn = client.connect(&addr.to_string()).await.unwrap();

        // 4. Test add(2, 3) = 5
        let payload = facet_postcard::to_vec(&(2i32, 3i32)).unwrap();
        conn.io()
            .send(&Message::Request {
                request_id: 1,
                method_id: method_id::ADD,
                metadata: vec![],
                payload,
            })
            .await
            .unwrap();

        let resp = conn
            .io()
            .recv_timeout(Duration::from_secs(1))
            .await
            .unwrap()
            .unwrap();
        let Message::Response {
            request_id,
            payload,
            ..
        } = resp
        else {
            panic!("expected Response, got {resp:?}")
        };
        assert_eq!(request_id, 1);
        // Response is CallResult<T, Never> = Result<T, RoamError<Never>> per spec r[unary.response.encoding]
        let result: CallResult<i32, Never> = facet_postcard::from_slice(&payload).unwrap();
        assert_eq!(result, Ok(5));

        // 5. Test multiply(4, 7) = 28
        let payload = facet_postcard::to_vec(&(4i32, 7i32)).unwrap();
        conn.io()
            .send(&Message::Request {
                request_id: 2,
                method_id: method_id::MULTIPLY,
                metadata: vec![],
                payload,
            })
            .await
            .unwrap();

        let resp = conn
            .io()
            .recv_timeout(Duration::from_secs(1))
            .await
            .unwrap()
            .unwrap();
        let Message::Response {
            request_id,
            payload,
            ..
        } = resp
        else {
            panic!("expected Response, got {resp:?}")
        };
        assert_eq!(request_id, 2);
        let result: CallResult<i32, Never> = facet_postcard::from_slice(&payload).unwrap();
        assert_eq!(result, Ok(28));

        // 6. Drop connection - server will see clean shutdown
        drop(conn);
        let _ = server_handle.await;
    }
}
