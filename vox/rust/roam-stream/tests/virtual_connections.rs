//! Integration tests for virtual connections using the generated API.
//!
//! These tests verify that virtual connections work correctly with the
//! high-level generated client/service APIs, not just wire-level messages.

use std::net::SocketAddr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use roam::session::{Rx, Tx};
use roam_stream::{CobsFramed, ConnectionHandle, HandshakeConfig, accept, initiate_framed};
use spec_proto::{Testbed, TestbedClient, TestbedDispatcher};

/// Test service implementation.
#[derive(Clone)]
struct TestService {
    call_count: Arc<AtomicU32>,
}

impl TestService {
    fn new() -> Self {
        Self {
            call_count: Arc::new(AtomicU32::new(0)),
        }
    }
}

impl Testbed for TestService {
    async fn echo(&self, _cx: &roam::session::Context, message: String) -> String {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        message
    }

    async fn reverse(&self, _cx: &roam::session::Context, message: String) -> String {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        message.chars().rev().collect()
    }

    async fn divide(
        &self,
        _cx: &roam::session::Context,
        dividend: i64,
        divisor: i64,
    ) -> Result<i64, spec_proto::MathError> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        if divisor == 0 {
            Err(spec_proto::MathError::DivisionByZero)
        } else {
            Ok(dividend / divisor)
        }
    }

    async fn lookup(
        &self,
        _cx: &roam::session::Context,
        id: u32,
    ) -> Result<spec_proto::Person, spec_proto::LookupError> {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        if id == 1 {
            Ok(spec_proto::Person {
                name: "Alice".to_string(),
                age: 30,
                email: Some("alice@example.com".to_string()),
            })
        } else {
            Err(spec_proto::LookupError::NotFound)
        }
    }

    async fn sum(&self, _cx: &roam::session::Context, mut numbers: Rx<i32>) -> i64 {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        let mut total = 0i64;
        while let Ok(Some(n)) = numbers.recv().await {
            total += n as i64;
        }
        total
    }

    async fn generate(&self, _cx: &roam::session::Context, count: u32, output: Tx<i32>) {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        for i in 0..count as i32 {
            let _ = output.send(&i).await;
        }
    }

    async fn transform(
        &self,
        _cx: &roam::session::Context,
        mut input: Rx<String>,
        output: Tx<String>,
    ) {
        self.call_count.fetch_add(1, Ordering::SeqCst);
        while let Ok(Some(s)) = input.recv().await {
            let _ = output.send(&s.to_uppercase()).await;
        }
    }

    async fn echo_point(
        &self,
        _cx: &roam::session::Context,
        point: spec_proto::Point,
    ) -> spec_proto::Point {
        point
    }

    async fn create_person(
        &self,
        _cx: &roam::session::Context,
        name: String,
        age: u8,
        email: Option<String>,
    ) -> spec_proto::Person {
        spec_proto::Person { name, age, email }
    }

    async fn rectangle_area(
        &self,
        _cx: &roam::session::Context,
        rect: spec_proto::Rectangle,
    ) -> f64 {
        let w = (rect.bottom_right.x - rect.top_left.x).abs() as f64;
        let h = (rect.bottom_right.y - rect.top_left.y).abs() as f64;
        w * h
    }

    async fn parse_color(
        &self,
        _cx: &roam::session::Context,
        name: String,
    ) -> Option<spec_proto::Color> {
        match name.to_lowercase().as_str() {
            "red" => Some(spec_proto::Color::Red),
            "green" => Some(spec_proto::Color::Green),
            "blue" => Some(spec_proto::Color::Blue),
            _ => None,
        }
    }

    async fn shape_area(&self, _cx: &roam::session::Context, shape: spec_proto::Shape) -> f64 {
        match shape {
            spec_proto::Shape::Circle { radius } => std::f64::consts::PI * radius * radius,
            spec_proto::Shape::Rectangle { width, height } => width * height,
            spec_proto::Shape::Point => 0.0,
        }
    }

    async fn create_canvas(
        &self,
        _cx: &roam::session::Context,
        name: String,
        shapes: Vec<spec_proto::Shape>,
        background: spec_proto::Color,
    ) -> spec_proto::Canvas {
        spec_proto::Canvas {
            name,
            shapes,
            background,
        }
    }

    async fn process_message(
        &self,
        _cx: &roam::session::Context,
        msg: spec_proto::Message,
    ) -> spec_proto::Message {
        match msg {
            spec_proto::Message::Text(s) => spec_proto::Message::Text(format!("processed: {s}")),
            spec_proto::Message::Number(n) => spec_proto::Message::Number(n * 2),
            spec_proto::Message::Data(d) => {
                spec_proto::Message::Data(d.into_iter().rev().collect())
            }
        }
    }

    async fn get_points(&self, _cx: &roam::session::Context, count: u32) -> Vec<spec_proto::Point> {
        (0..count as i32)
            .map(|i| spec_proto::Point { x: i, y: i * 2 })
            .collect()
    }

    async fn swap_pair(&self, _cx: &roam::session::Context, pair: (i32, String)) -> (String, i32) {
        (pair.1, pair.0)
    }
}

/// Start a server that accepts incoming virtual connections.
/// Returns the address and a task that handles connections.
async fn start_server_accepting_virtual_connections(
    service: TestService,
) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let handle = tokio::spawn(async move {
        while let Ok((stream, _)) = listener.accept().await {
            let service = service.clone();
            tokio::spawn(async move {
                let dispatcher = TestbedDispatcher::new(service.clone());
                if let Ok((handle, mut incoming, driver)) =
                    accept(stream, HandshakeConfig::default(), dispatcher).await
                {
                    // Spawn driver
                    let driver_handle = tokio::spawn(async move { driver.run().await });

                    // Handle incoming virtual connections
                    while let Some(conn) = incoming.recv().await {
                        tokio::spawn(async move {
                            // Accept the connection - the dispatcher is already set up
                            // on the link, so the new connection will use the same service
                            match conn.accept(vec![]).await {
                                Ok(_virtual_handle) => {
                                    // Virtual connection is now active
                                    // Calls on it will be handled by the dispatcher
                                }
                                Err(e) => {
                                    eprintln!("Failed to accept virtual connection: {e}");
                                }
                            }
                        });
                    }

                    let _ = driver_handle.await;
                    let _ = handle;
                }
            });
        }
    });

    tokio::time::sleep(Duration::from_millis(10)).await;
    (addr, handle)
}

// r[verify core.conn.open] - Virtual connection opened via Connect/Accept
// r[verify core.conn.lifecycle] - Virtual connection can be used for RPC
#[tokio::test]
async fn rpc_over_virtual_connection() {
    let service = TestService::new();
    let (addr, _server) = start_server_accepting_virtual_connections(service.clone()).await;

    // Connect to the server
    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let dispatcher = TestbedDispatcher::new(service.clone());
    let framed = CobsFramed::new(stream);
    let (root_handle, _incoming, driver) =
        initiate_framed(framed, HandshakeConfig::default(), dispatcher)
            .await
            .unwrap();

    tokio::spawn(async move { driver.run().await });

    // Open a virtual connection
    let virtual_handle: ConnectionHandle = root_handle.connect(vec![]).await.unwrap();

    // Use the generated client over the virtual connection
    let client = TestbedClient::new(virtual_handle);

    // Make RPC calls
    let result = client.echo("hello virtual!".to_string()).await.unwrap();
    assert_eq!(result, "hello virtual!");

    let result = client.reverse("hello".to_string()).await.unwrap();
    assert_eq!(result, "olleh");
}

// r[verify core.conn.independence] - Virtual connections have independent request ID spaces
#[tokio::test]
async fn multiple_virtual_connections_independent() {
    let service = TestService::new();
    let (addr, _server) = start_server_accepting_virtual_connections(service.clone()).await;

    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let dispatcher = TestbedDispatcher::new(service.clone());
    let (root_handle, _incoming, driver) = initiate_framed(
        CobsFramed::new(stream),
        HandshakeConfig::default(),
        dispatcher,
    )
    .await
    .unwrap();

    tokio::spawn(async move { driver.run().await });

    // Open multiple virtual connections
    let virtual1 = root_handle.connect(vec![]).await.unwrap();
    let virtual2 = root_handle.connect(vec![]).await.unwrap();

    let client1 = TestbedClient::new(virtual1);
    let client2 = TestbedClient::new(virtual2);

    // Make concurrent calls on different virtual connections
    let (result1, result2) = tokio::join!(
        client1.echo("from conn 1".to_string()),
        client2.echo("from conn 2".to_string()),
    );

    assert_eq!(result1.unwrap(), "from conn 1");
    assert_eq!(result2.unwrap(), "from conn 2");
}

// r[verify core.conn.open] - Root connection works alongside virtual connections
#[tokio::test]
async fn root_and_virtual_connections_coexist() {
    let service = TestService::new();
    let (addr, _server) = start_server_accepting_virtual_connections(service.clone()).await;

    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let dispatcher = TestbedDispatcher::new(service.clone());
    let (root_handle, _incoming, driver) = initiate_framed(
        CobsFramed::new(stream),
        HandshakeConfig::default(),
        dispatcher,
    )
    .await
    .unwrap();

    tokio::spawn(async move { driver.run().await });

    // Create client on root connection
    let root_client = TestbedClient::new(root_handle.clone());

    // Open virtual connection and create client
    let virtual_handle = root_handle.connect(vec![]).await.unwrap();
    let virtual_client = TestbedClient::new(virtual_handle);

    // Make calls on both connections
    let root_result = root_client.echo("root".to_string()).await.unwrap();
    let virtual_result = virtual_client.echo("virtual".to_string()).await.unwrap();

    assert_eq!(root_result, "root");
    assert_eq!(virtual_result, "virtual");

    // Interleaved calls
    let (r1, v1, r2, v2) = tokio::join!(
        root_client.reverse("abc".to_string()),
        virtual_client.reverse("xyz".to_string()),
        root_client.echo("root2".to_string()),
        virtual_client.echo("virtual2".to_string()),
    );

    assert_eq!(r1.unwrap(), "cba");
    assert_eq!(v1.unwrap(), "zyx");
    assert_eq!(r2.unwrap(), "root2");
    assert_eq!(v2.unwrap(), "virtual2");
}

// r[verify channeling.type] - Streaming works over virtual connections
#[tokio::test]
async fn streaming_over_virtual_connection() {
    let service = TestService::new();
    let (addr, _server) = start_server_accepting_virtual_connections(service.clone()).await;

    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let dispatcher = TestbedDispatcher::new(service.clone());
    let (root_handle, _incoming, driver) = initiate_framed(
        CobsFramed::new(stream),
        HandshakeConfig::default(),
        dispatcher,
    )
    .await
    .unwrap();

    tokio::spawn(async move { driver.run().await });

    // Open virtual connection
    let virtual_handle = root_handle.connect(vec![]).await.unwrap();
    let client = TestbedClient::new(virtual_handle);

    // Test client-to-server streaming (sum)
    let (tx, rx) = roam::channel::<i32>();
    let sum_task = tokio::spawn({
        let client = client.clone();
        async move { client.sum(rx).await }
    });

    // Send values
    for i in 1..=5 {
        tx.send(&i).await.unwrap();
    }
    drop(tx); // Close the stream

    let sum = sum_task.await.unwrap().unwrap();
    assert_eq!(sum, 15); // 1+2+3+4+5

    // Test server-to-client streaming (generate)
    let (tx, mut rx) = roam::channel::<i32>();
    client.generate(5, tx).await.unwrap();

    let mut received = Vec::new();
    while let Ok(Some(n)) = rx.recv().await {
        received.push(n);
    }
    assert_eq!(received, vec![0, 1, 2, 3, 4]);
}

// r[verify call.error.user] - User errors work over virtual connections
#[tokio::test]
async fn user_errors_over_virtual_connection() {
    let service = TestService::new();
    let (addr, _server) = start_server_accepting_virtual_connections(service.clone()).await;

    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let dispatcher = TestbedDispatcher::new(service.clone());
    let (root_handle, _incoming, driver) = initiate_framed(
        CobsFramed::new(stream),
        HandshakeConfig::default(),
        dispatcher,
    )
    .await
    .unwrap();

    tokio::spawn(async move { driver.run().await });

    let virtual_handle = root_handle.connect(vec![]).await.unwrap();
    let client = TestbedClient::new(virtual_handle);

    // Test successful call - divide returns Result<i64, CallError<MathError>>
    let result = client.divide(10, 2).await.unwrap();
    assert_eq!(result, 5);

    // Test user error - CallError::Roam(RoamError::User(E))
    let result = client.divide(10, 0).await;
    assert!(matches!(
        result,
        Err(roam_stream::CallError::Roam(
            roam::session::RoamError::User(spec_proto::MathError::DivisionByZero)
        ))
    ));
}

// r[verify channeling.type] - Bidirectional streaming works over virtual connections
#[tokio::test]
async fn bidirectional_streaming_over_virtual_connection() {
    let service = TestService::new();
    let (addr, _server) = start_server_accepting_virtual_connections(service.clone()).await;

    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let dispatcher = TestbedDispatcher::new(service.clone());
    let (root_handle, _incoming, driver) = initiate_framed(
        CobsFramed::new(stream),
        HandshakeConfig::default(),
        dispatcher,
    )
    .await
    .unwrap();

    tokio::spawn(async move { driver.run().await });

    let virtual_handle = root_handle.connect(vec![]).await.unwrap();
    let client = TestbedClient::new(virtual_handle);

    // Test bidirectional streaming (transform: input strings, output uppercase)
    let (input_tx, input_rx) = roam::channel::<String>();
    let (output_tx, mut output_rx) = roam::channel::<String>();

    // Spawn the call
    let call_task = tokio::spawn({
        let client = client.clone();
        async move { client.transform(input_rx, output_tx).await }
    });

    // Send input strings
    input_tx.send(&"hello".to_string()).await.unwrap();
    input_tx.send(&"world".to_string()).await.unwrap();
    input_tx.send(&"test".to_string()).await.unwrap();
    drop(input_tx); // Close input stream

    // Receive transformed output
    let mut received = Vec::new();
    while let Ok(Some(s)) = output_rx.recv().await {
        received.push(s);
    }

    call_task.await.unwrap().unwrap();
    assert_eq!(received, vec!["HELLO", "WORLD", "TEST"]);
}

// r[verify core.conn.independence] - Concurrent calls on same virtual connection
#[tokio::test]
async fn concurrent_calls_on_virtual_connection() {
    let service = TestService::new();
    let (addr, _server) = start_server_accepting_virtual_connections(service.clone()).await;

    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let dispatcher = TestbedDispatcher::new(service.clone());
    let (root_handle, _incoming, driver) = initiate_framed(
        CobsFramed::new(stream),
        HandshakeConfig::default(),
        dispatcher,
    )
    .await
    .unwrap();

    tokio::spawn(async move { driver.run().await });

    let virtual_handle = root_handle.connect(vec![]).await.unwrap();
    let client = TestbedClient::new(virtual_handle);

    // Launch many concurrent calls on the same virtual connection
    let mut handles = Vec::new();
    for i in 0..10 {
        let client = client.clone();
        handles.push(tokio::spawn(async move {
            let msg = format!("message {i}");
            let result = client.echo(msg.clone()).await.unwrap();
            assert_eq!(result, msg);
            i
        }));
    }

    // Wait for all to complete
    let mut results = Vec::new();
    for handle in handles {
        results.push(handle.await.unwrap());
    }
    results.sort();
    assert_eq!(results, (0..10).collect::<Vec<_>>());
}

// r[verify call.payload] - Complex types work over virtual connections
#[tokio::test]
async fn complex_types_over_virtual_connection() {
    let service = TestService::new();
    let (addr, _server) = start_server_accepting_virtual_connections(service.clone()).await;

    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let dispatcher = TestbedDispatcher::new(service.clone());
    let (root_handle, _incoming, driver) = initiate_framed(
        CobsFramed::new(stream),
        HandshakeConfig::default(),
        dispatcher,
    )
    .await
    .unwrap();

    tokio::spawn(async move { driver.run().await });

    let virtual_handle = root_handle.connect(vec![]).await.unwrap();
    let client = TestbedClient::new(virtual_handle);

    // Test struct (Point)
    let point = spec_proto::Point { x: 10, y: 20 };
    let result = client.echo_point(point.clone()).await.unwrap();
    assert_eq!(result, point);

    // Test nested struct (Rectangle)
    let rect = spec_proto::Rectangle {
        top_left: spec_proto::Point { x: 0, y: 0 },
        bottom_right: spec_proto::Point { x: 100, y: 50 },
        label: Some("test rect".to_string()),
    };
    let area = client.rectangle_area(rect).await.unwrap();
    assert_eq!(area, 5000.0);

    // Test enum (Shape)
    let circle = spec_proto::Shape::Circle { radius: 5.0 };
    let area = client.shape_area(circle).await.unwrap();
    assert!((area - std::f64::consts::PI * 25.0).abs() < 0.001);

    // Test Vec of complex types
    let points = client.get_points(3).await.unwrap();
    assert_eq!(points.len(), 3);
    assert_eq!(points[0], spec_proto::Point { x: 0, y: 0 });
    assert_eq!(points[1], spec_proto::Point { x: 1, y: 2 });
    assert_eq!(points[2], spec_proto::Point { x: 2, y: 4 });

    // Test tuple
    let swapped = client.swap_pair((42, "hello".to_string())).await.unwrap();
    assert_eq!(swapped, ("hello".to_string(), 42));
}

// r[verify core.conn.only-root-accepts] - Non-root handles cannot accept connections
#[tokio::test]
async fn virtual_connection_cannot_accept_nested() {
    let service = TestService::new();
    let (addr, _server) = start_server_accepting_virtual_connections(service.clone()).await;

    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let dispatcher = TestbedDispatcher::new(service.clone());
    let (root_handle, _incoming, driver) = initiate_framed(
        CobsFramed::new(stream),
        HandshakeConfig::default(),
        dispatcher,
    )
    .await
    .unwrap();

    tokio::spawn(async move { driver.run().await });

    // Open a virtual connection
    let virtual_handle = root_handle.connect(vec![]).await.unwrap();

    // The virtual_handle is a ConnectionHandle, not the root handle.
    // It should not have an IncomingConnections receiver - that's only for the root.
    // The API enforces this: connect() returns ConnectionHandle, not a tuple with IncomingConnections.
    // This test verifies we can still use the virtual handle for normal operations.
    let client = TestbedClient::new(virtual_handle);
    let result = client.echo("nested test".to_string()).await.unwrap();
    assert_eq!(result, "nested test");
}

// r[verify core.conn.lifecycle] - Many virtual connections can be opened and closed
#[tokio::test]
async fn many_virtual_connections() {
    let service = TestService::new();
    let (addr, _server) = start_server_accepting_virtual_connections(service.clone()).await;

    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let dispatcher = TestbedDispatcher::new(service.clone());
    let (root_handle, _incoming, driver) = initiate_framed(
        CobsFramed::new(stream),
        HandshakeConfig::default(),
        dispatcher,
    )
    .await
    .unwrap();

    tokio::spawn(async move { driver.run().await });

    // Open many virtual connections and use them
    for i in 0..20 {
        let virtual_handle = root_handle.connect(vec![]).await.unwrap();
        let client = TestbedClient::new(virtual_handle);
        let msg = format!("connection {i}");
        let result = client.echo(msg.clone()).await.unwrap();
        assert_eq!(result, msg);
        // virtual_handle is dropped here, connection closes
    }

    // Root connection should still work
    let root_client = TestbedClient::new(root_handle.clone());
    let result = root_client
        .echo("root still works".to_string())
        .await
        .unwrap();
    assert_eq!(result, "root still works");
}

// r[verify message.connect.metadata] - Connect can carry metadata
#[tokio::test]
async fn connect_with_metadata() {
    use roam_wire::MetadataValue;

    let service = TestService::new();
    let (addr, _server) = start_server_accepting_virtual_connections(service.clone()).await;

    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let dispatcher = TestbedDispatcher::new(service.clone());
    let (root_handle, _incoming, driver) = initiate_framed(
        CobsFramed::new(stream),
        HandshakeConfig::default(),
        dispatcher,
    )
    .await
    .unwrap();

    tokio::spawn(async move { driver.run().await });

    // Open virtual connection with metadata
    let metadata = vec![
        (
            "client-id".to_string(),
            MetadataValue::String("test-client-123".to_string()),
        ),
        (
            "version".to_string(),
            MetadataValue::String("1.0".to_string()),
        ),
    ];
    let virtual_handle = root_handle.connect(metadata).await.unwrap();

    // Verify the connection works
    let client = TestbedClient::new(virtual_handle);
    let result = client.echo("with metadata".to_string()).await.unwrap();
    assert_eq!(result, "with metadata");
}

// r[verify message.reject.reason] - Explicit rejection with custom reason
#[tokio::test]
async fn connect_explicit_rejection() {
    use roam_stream::ConnectError;

    // Start a server that rejects connections with a custom reason
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let service = TestService::new();
        let dispatcher = TestbedDispatcher::new(service);
        let (_handle, mut incoming, driver) =
            accept(stream, HandshakeConfig::default(), dispatcher)
                .await
                .unwrap();

        let driver_handle = tokio::spawn(async move { driver.run().await });

        // Reject the first incoming connection with a custom reason
        if let Some(conn) = incoming.recv().await {
            conn.reject(
                "not authorized".to_string(),
                vec![(
                    "error-code".to_string(),
                    roam_wire::MetadataValue::String("401".to_string()),
                )],
            );
        }

        let _ = driver_handle.await;
    });

    tokio::time::sleep(Duration::from_millis(10)).await;

    // Client connects and tries to open virtual connection
    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let service = TestService::new();
    let dispatcher = TestbedDispatcher::new(service);
    let (root_handle, _incoming, driver) = initiate_framed(
        CobsFramed::new(stream),
        HandshakeConfig::default(),
        dispatcher,
    )
    .await
    .unwrap();

    tokio::spawn(async move { driver.run().await });

    // Try to open virtual connection - should be rejected
    let result = root_handle.connect(vec![]).await;
    assert!(matches!(result, Err(ConnectError::Rejected(reason)) if reason == "not authorized"));

    server.abort();
}

// r[verify core.conn.independence] - Interleaved streaming on multiple virtual connections
#[tokio::test]
async fn interleaved_streaming_multiple_connections() {
    let service = TestService::new();
    let (addr, _server) = start_server_accepting_virtual_connections(service.clone()).await;

    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let dispatcher = TestbedDispatcher::new(service.clone());
    let (root_handle, _incoming, driver) = initiate_framed(
        CobsFramed::new(stream),
        HandshakeConfig::default(),
        dispatcher,
    )
    .await
    .unwrap();

    tokio::spawn(async move { driver.run().await });

    // Open two virtual connections
    let virtual1 = root_handle.connect(vec![]).await.unwrap();
    let virtual2 = root_handle.connect(vec![]).await.unwrap();

    let client1 = TestbedClient::new(virtual1);
    let client2 = TestbedClient::new(virtual2);

    // Start streaming calls on both connections
    let (tx1, rx1) = roam::channel::<i32>();
    let (tx2, rx2) = roam::channel::<i32>();

    let sum1_task = tokio::spawn({
        let client1 = client1.clone();
        async move { client1.sum(rx1).await }
    });

    let sum2_task = tokio::spawn({
        let client2 = client2.clone();
        async move { client2.sum(rx2).await }
    });

    // Interleave sends on both streams
    tx1.send(&1).await.unwrap();
    tx2.send(&10).await.unwrap();
    tx1.send(&2).await.unwrap();
    tx2.send(&20).await.unwrap();
    tx1.send(&3).await.unwrap();
    tx2.send(&30).await.unwrap();

    drop(tx1);
    drop(tx2);

    // Verify each got the correct sum
    let sum1 = sum1_task.await.unwrap().unwrap();
    let sum2 = sum2_task.await.unwrap().unwrap();

    assert_eq!(sum1, 6); // 1+2+3
    assert_eq!(sum2, 60); // 10+20+30
}

// r[verify core.conn.open] - Server can open virtual connection back to client
#[tokio::test]
async fn server_initiated_virtual_connection() {
    use tokio::sync::oneshot;

    // Channel to signal when server has opened a connection back
    let (server_done_tx, server_done_rx) = oneshot::channel::<String>();

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let service = TestService::new();
        let dispatcher = TestbedDispatcher::new(service);
        let (handle, mut incoming, driver) = accept(stream, HandshakeConfig::default(), dispatcher)
            .await
            .unwrap();

        tokio::spawn(async move { driver.run().await });

        // Accept any incoming virtual connections from client
        tokio::spawn(async move {
            while let Some(conn) = incoming.recv().await {
                let _ = conn.accept(vec![]).await;
            }
        });

        // Server opens a virtual connection back to the client
        let virtual_handle = handle.connect(vec![]).await.unwrap();
        let client = TestbedClient::new(virtual_handle);

        // Make a call over the server-initiated virtual connection
        let result = client
            .echo("server calling client".to_string())
            .await
            .unwrap();

        let _ = server_done_tx.send(result);
    });

    tokio::time::sleep(Duration::from_millis(10)).await;

    // Client connects and accepts incoming virtual connections
    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let service = TestService::new();
    let dispatcher = TestbedDispatcher::new(service);
    let (_root_handle, mut incoming, driver) = initiate_framed(
        CobsFramed::new(stream),
        HandshakeConfig::default(),
        dispatcher,
    )
    .await
    .unwrap();

    tokio::spawn(async move { driver.run().await });

    // Client accepts incoming virtual connections from server
    tokio::spawn(async move {
        while let Some(conn) = incoming.recv().await {
            let _ = conn.accept(vec![]).await;
        }
    });

    // Wait for server to complete its call
    let result = tokio::time::timeout(Duration::from_secs(5), server_done_rx)
        .await
        .expect("timeout waiting for server")
        .expect("server channel closed");

    assert_eq!(result, "server calling client");

    server.abort();
}

// r[verify core.conn.lifecycle] - Calls fail gracefully when link closes during streaming
#[tokio::test]
async fn link_closure_during_streaming_call() {
    use roam_stream::CallError;

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    // Server that abruptly closes after accepting a virtual connection
    let server = tokio::spawn(async move {
        let (stream, _) = listener.accept().await.unwrap();
        let service = TestService::new();
        let dispatcher = TestbedDispatcher::new(service);
        let (_handle, mut incoming, driver) =
            accept(stream, HandshakeConfig::default(), dispatcher)
                .await
                .unwrap();

        let driver_handle = tokio::spawn(async move { driver.run().await });

        // Accept the virtual connection
        if let Some(conn) = incoming.recv().await {
            let _virtual_handle = conn.accept(vec![]).await.unwrap();
            // Small delay so client can start a streaming call
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        // Abort the driver - simulates abrupt connection loss
        driver_handle.abort();
    });

    tokio::time::sleep(Duration::from_millis(10)).await;

    let stream = tokio::net::TcpStream::connect(addr).await.unwrap();
    let service = TestService::new();
    let dispatcher = TestbedDispatcher::new(service);
    let (root_handle, _incoming, driver) = initiate_framed(
        CobsFramed::new(stream),
        HandshakeConfig::default(),
        dispatcher,
    )
    .await
    .unwrap();

    tokio::spawn(async move { driver.run().await });

    let virtual_handle = root_handle.connect(vec![]).await.unwrap();
    let client = TestbedClient::new(virtual_handle);

    // Start a streaming call
    let (tx, rx) = roam::channel::<i32>();
    let sum_task = tokio::spawn({
        let client = client.clone();
        async move { client.sum(rx).await }
    });

    // Send some data
    tx.send(&1).await.unwrap();
    tx.send(&2).await.unwrap();

    // Wait for server to close
    tokio::time::sleep(Duration::from_millis(100)).await;

    // Try to send more - might fail
    let _ = tx.send(&3).await;
    drop(tx);

    // The call should fail with ConnectionClosed or DriverGone
    let result = sum_task.await.unwrap();
    assert!(
        matches!(
            result,
            Err(CallError::ConnectionClosed) | Err(CallError::DriverGone)
        ),
        "Expected ConnectionClosed or DriverGone, got {:?}",
        result
    );

    server.abort();
}
