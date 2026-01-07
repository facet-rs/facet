//! Test consumer crate that uses generated code from build.rs.

// Include the generated code
include!(concat!(env!("OUT_DIR"), "/generated.rs"));

#[cfg(test)]
mod tests {
    use super::calculator::*;

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
        use roam_stream::{Message, Server};
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

    // NOTE: duplicate-detection requires concurrent request handling to test.
    // With synchronous processing, requests complete before the next is received.
    // See r[unary.request-id.duplicate-detection] in the spec.

    /// r[verify streaming.id.zero-reserved] - Stream ID 0 is reserved.
    #[tokio::test]
    async fn stream_id_zero_rejected() {
        use roam_stream::{Message, Server};
        use std::time::Duration;
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_handle = tokio::spawn(async move {
            let server = Server::new();
            let mut conn = server.accept(&listener).await.unwrap();
            let dispatcher = CalculatorDispatcher::new(TestCalculator);
            conn.run(&dispatcher).await
        });

        let client = Server::new();
        let mut conn = client.connect(&addr.to_string()).await.unwrap();

        // Send Data with stream_id = 0 (reserved, protocol violation)
        conn.io()
            .send(&Message::Data {
                stream_id: 0,
                payload: vec![1, 2, 3],
            })
            .await
            .unwrap();

        // Server should send Goodbye with zero-reserved reason
        let msg = conn
            .io()
            .recv_timeout(Duration::from_secs(1))
            .await
            .unwrap();
        match msg {
            Some(Message::Goodbye { reason }) => {
                assert!(
                    reason.contains("zero-reserved"),
                    "expected zero-reserved, got: {reason}"
                );
            }
            other => panic!("expected Goodbye, got {other:?}"),
        }

        let _ = server_handle.await;
    }

    /// r[verify message.hello.enforcement] - Exceeding negotiated payload limit triggers Goodbye.
    /// r[verify streaming.data.size-limit] - Stream data bounded by max_payload_size.
    #[tokio::test]
    async fn oversized_stream_data_rejected() {
        use roam_stream::{Message, Server};
        use std::time::Duration;
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_handle = tokio::spawn(async move {
            let server = Server::new();
            let mut conn = server.accept(&listener).await.unwrap();
            let dispatcher = CalculatorDispatcher::new(TestCalculator);
            conn.run(&dispatcher).await
        });

        let client = Server::new();
        let mut conn = client.connect(&addr.to_string()).await.unwrap();

        // Default max_payload_size is 1MB. Send Data larger than that.
        // Use stream_id = 1 (valid odd ID for initiator streams).
        // Use 1MB + 1 byte to be just over the limit
        let oversized_payload = vec![0u8; 1024 * 1024 + 1];
        conn.io()
            .send(&Message::Data {
                stream_id: 1,
                payload: oversized_payload,
            })
            .await
            .unwrap();

        // Server should send Goodbye with hello.enforcement reason (payload exceeded)
        // r[impl message.hello.enforcement] uses this reason for payload limit violations
        let msg = conn
            .io()
            .recv_timeout(Duration::from_secs(1))
            .await
            .unwrap();
        match msg {
            Some(Message::Goodbye { reason }) => {
                assert!(
                    reason.contains("hello.enforcement"),
                    "expected hello.enforcement (payload limit), got: {reason}"
                );
            }
            other => panic!("expected Goodbye, got {other:?}"),
        }

        let _ = server_handle.await;
    }

    /// r[verify message.goodbye.receive] - Connection closes gracefully on Goodbye.
    #[tokio::test]
    async fn goodbye_closes_connection() {
        use roam_stream::{ConnectionError, Message, Server};
        use std::time::Duration;
        use tokio::net::TcpListener;

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server_handle = tokio::spawn(async move {
            let server = Server::new();
            let mut conn = server.accept(&listener).await.unwrap();
            let dispatcher = CalculatorDispatcher::new(TestCalculator);
            conn.run(&dispatcher).await
        });

        let client = Server::new();
        let mut conn = client.connect(&addr.to_string()).await.unwrap();

        // Send Goodbye
        conn.io()
            .send(&Message::Goodbye {
                reason: "client-initiated-close".into(),
            })
            .await
            .unwrap();

        // Wait for server to process Goodbye and terminate
        let server_result = server_handle.await.unwrap();

        // Server should return Err(ConnectionError::Closed) when it receives Goodbye
        match server_result {
            Err(ConnectionError::Closed) => {
                // Expected: server received Goodbye and closed
            }
            Ok(()) => {
                // Also acceptable: connection closed cleanly (EOF before Goodbye processed)
            }
            Err(e) => panic!("unexpected error: {e:?}"),
        }

        // Connection should be closed - recv returns None
        let msg = conn
            .io()
            .recv_timeout(Duration::from_secs(1))
            .await
            .unwrap();
        assert!(
            msg.is_none(),
            "expected connection close (None), got {msg:?}"
        );
    }

    /// r[verify message.hello.enforcement] - Non-Hello before Hello is rejected.
    #[tokio::test]
    async fn non_hello_before_hello_rejected() {
        use roam_stream::{CobsFramed, Message};
        use std::time::Duration;
        use tokio::net::{TcpListener, TcpStream};

        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        // Client sends Request before Hello - protocol violation!
        let client_handle = tokio::spawn(async move {
            let stream = TcpStream::connect(addr).await.unwrap();
            let mut io = CobsFramed::new(stream);

            // Send Request before Hello - protocol violation!
            io.send(&Message::Request {
                request_id: 1,
                method_id: 0,
                metadata: vec![],
                payload: vec![],
            })
            .await
            .unwrap();

            // Server sends Hello first (as part of accept), then sees our non-Hello and sends Goodbye
            let mut saw_hello = false;
            let mut saw_goodbye = false;
            let mut goodbye_reason = String::new();

            for _ in 0..3 {
                match io.recv_timeout(Duration::from_secs(1)).await.unwrap() {
                    Some(Message::Hello(_)) => saw_hello = true,
                    Some(Message::Goodbye { reason }) => {
                        saw_goodbye = true;
                        goodbye_reason = reason;
                        break;
                    }
                    None => break,
                    other => panic!("unexpected message: {other:?}"),
                }
            }

            (saw_hello, saw_goodbye, goodbye_reason)
        });

        // Server side: accept and run
        let server_handle = tokio::spawn(async move {
            use roam_stream::Server;
            let server = Server::new();
            server.accept(&listener).await
        });

        // Client should see Hello then Goodbye
        let (saw_hello, saw_goodbye, reason) = client_handle.await.unwrap();
        assert!(saw_hello, "expected to receive server's Hello");
        assert!(saw_goodbye, "expected to receive Goodbye");
        assert!(
            reason.contains("hello.ordering"),
            "expected hello.ordering, got: {reason}"
        );

        // Server should have returned error
        let server_result = server_handle.await.unwrap();
        assert!(server_result.is_err());
    }
}
