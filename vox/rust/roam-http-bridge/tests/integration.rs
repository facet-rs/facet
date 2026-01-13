//! Integration tests for the HTTP bridge.
//!
//! This tests the full flow:
//! 1. Start a roam server with Testbed service
//! 2. Start an HTTP bridge connected to it
//! 3. Make HTTP requests to the bridge
//! 4. Verify responses

use std::net::SocketAddr;

use axum::Router;
use futures_util::{SinkExt, StreamExt};
use roam_http_bridge::{BridgeRouter, GenericBridgeService};
use roam_stream::{Connector, HandshakeConfig, NoDispatcher, accept, connect};
use spec_proto::{
    LookupError, MathError, Person, Testbed, TestbedDispatcher, testbed_service_detail,
};
use tokio::net::{TcpListener, TcpStream};
use tokio_tungstenite::{connect_async, tungstenite};

/// Simple Testbed implementation for testing.
#[derive(Clone)]
struct TestbedImpl;

impl Testbed for TestbedImpl {
    async fn echo(&self, message: String) -> String {
        message
    }

    async fn reverse(&self, message: String) -> String {
        message.chars().rev().collect()
    }

    async fn divide(&self, dividend: i64, divisor: i64) -> Result<i64, MathError> {
        if divisor == 0 {
            Err(MathError::DivisionByZero)
        } else {
            Ok(dividend / divisor)
        }
    }

    async fn lookup(&self, id: u32) -> Result<Person, LookupError> {
        match id {
            1 => Ok(Person {
                name: "Alice".to_string(),
                age: 30,
                email: Some("alice@example.com".to_string()),
            }),
            _ => Err(LookupError::NotFound),
        }
    }

    async fn sum(&self, mut numbers: roam_session::Rx<i32>) -> i64 {
        let mut total: i64 = 0;
        while let Some(n) = numbers.recv().await.ok().flatten() {
            total += n as i64;
        }
        total
    }

    async fn generate(&self, count: u32, output: roam_session::Tx<i32>) {
        for i in 0..count as i32 {
            let _ = output.send(&i).await;
        }
    }

    async fn transform(
        &self,
        mut input: roam_session::Rx<String>,
        output: roam_session::Tx<String>,
    ) {
        while let Some(s) = input.recv().await.ok().flatten() {
            let _ = output.send(&s.to_uppercase()).await;
        }
    }

    async fn echo_point(&self, point: spec_proto::Point) -> spec_proto::Point {
        point
    }

    async fn create_person(
        &self,
        name: String,
        age: u8,
        email: Option<String>,
    ) -> spec_proto::Person {
        spec_proto::Person { name, age, email }
    }

    async fn rectangle_area(&self, rect: spec_proto::Rectangle) -> f64 {
        let width = (rect.bottom_right.x - rect.top_left.x).abs() as f64;
        let height = (rect.bottom_right.y - rect.top_left.y).abs() as f64;
        width * height
    }

    async fn parse_color(&self, name: String) -> Option<spec_proto::Color> {
        match name.to_lowercase().as_str() {
            "red" => Some(spec_proto::Color::Red),
            "green" => Some(spec_proto::Color::Green),
            "blue" => Some(spec_proto::Color::Blue),
            _ => None,
        }
    }

    async fn shape_area(&self, shape: spec_proto::Shape) -> f64 {
        match shape {
            spec_proto::Shape::Circle { radius } => std::f64::consts::PI * radius * radius,
            spec_proto::Shape::Rectangle { width, height } => width * height,
            spec_proto::Shape::Point => 0.0,
        }
    }

    async fn create_canvas(
        &self,
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

    async fn process_message(&self, msg: spec_proto::Message) -> spec_proto::Message {
        msg
    }

    async fn get_points(&self, count: u32) -> Vec<spec_proto::Point> {
        (0..count as i32)
            .map(|i| spec_proto::Point { x: i, y: i * 2 })
            .collect()
    }

    async fn swap_pair(&self, pair: (i32, String)) -> (String, i32) {
        (pair.1, pair.0)
    }
}

/// Start a roam server and return the address.
async fn start_roam_server() -> (SocketAddr, tokio::task::JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let handle = tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let dispatcher = TestbedDispatcher::new(TestbedImpl);

            tokio::spawn(async move {
                let (handle, driver) = accept(stream, HandshakeConfig::default(), dispatcher)
                    .await
                    .unwrap();
                let _ = handle;
                let _ = driver.run().await;
            });
        }
    });

    (addr, handle)
}

/// Connector for the bridge client.
struct BridgeConnector {
    addr: SocketAddr,
}

impl Connector for BridgeConnector {
    type Transport = TcpStream;

    async fn connect(&self) -> std::io::Result<TcpStream> {
        TcpStream::connect(self.addr).await
    }
}

/// Connect to the roam server and return a connection handle.
async fn connect_to_roam(addr: SocketAddr) -> roam_stream::Client<BridgeConnector, NoDispatcher> {
    let connector = BridgeConnector { addr };
    connect(connector, HandshakeConfig::default(), NoDispatcher)
}

/// Start the HTTP bridge server.
async fn start_bridge_server(
    roam_client: roam_stream::Client<BridgeConnector, NoDispatcher>,
) -> (SocketAddr, tokio::task::JoinHandle<()>) {
    // Get a handle from the client
    let handle = roam_client.handle().await.unwrap();

    // Leak the service detail to get a 'static reference
    let detail: &'static _ = Box::leak(Box::new(testbed_service_detail()));
    let service = GenericBridgeService::new(handle, detail);

    let bridge_router = BridgeRouter::new().service(service).build();

    let app = Router::new().nest("/api", bridge_router);

    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();

    let handle = tokio::spawn(async move {
        axum::serve(listener, app).await.unwrap();
    });

    (addr, handle)
}

#[tokio::test]
async fn test_echo_via_http_bridge() {
    // 1. Start roam server
    let (roam_addr, _server_handle) = start_roam_server().await;

    // Give server time to start
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // 2. Connect to roam server
    let roam_client = connect_to_roam(roam_addr).await;

    // 3. Start HTTP bridge
    let (bridge_addr, _bridge_handle) = start_bridge_server(roam_client).await;

    // Give bridge time to start
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // 4. Make HTTP request
    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://{}/api/Testbed/echo", bridge_addr))
        .header("Content-Type", "application/json")
        .body(r#"["hello world"]"#)
        .send()
        .await
        .unwrap();

    let status = response.status();
    let body_text = response.text().await.unwrap();

    assert_eq!(status, 200, "Body was: {}", body_text);

    // TODO: This test is currently expected to fail until facet-rs/facet#1753 lands
    // (need to_vec_with_shape for Value → typed postcard encoding)
    let body: String = serde_json::from_str(&body_text).unwrap();
    assert_eq!(body, "hello world");
}

#[tokio::test]
async fn test_reverse_via_http_bridge() {
    let (roam_addr, _) = start_roam_server().await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let roam_client = connect_to_roam(roam_addr).await;
    let (bridge_addr, _) = start_bridge_server(roam_client).await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://{}/api/Testbed/reverse", bridge_addr))
        .header("Content-Type", "application/json")
        .body(r#"["hello"]"#)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body: String = response.json().await.unwrap();
    assert_eq!(body, "olleh");
}

#[tokio::test]
async fn test_echo_point_via_http_bridge() {
    let (roam_addr, _) = start_roam_server().await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let roam_client = connect_to_roam(roam_addr).await;
    let (bridge_addr, _) = start_bridge_server(roam_client).await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://{}/api/Testbed/echo_point", bridge_addr))
        .header("Content-Type", "application/json")
        .body(r#"[{"x": 10, "y": 20}]"#)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status(), 200);

    let body: serde_json::Value = response.json().await.unwrap();
    assert_eq!(body["x"], 10);
    assert_eq!(body["y"], 20);
}

#[tokio::test]
async fn test_streaming_method_rejected() {
    let (roam_addr, _) = start_roam_server().await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let roam_client = connect_to_roam(roam_addr).await;
    let (bridge_addr, _) = start_bridge_server(roam_client).await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://{}/api/Testbed/sum", bridge_addr))
        .header("Content-Type", "application/json")
        .body(r#"[]"#)
        .send()
        .await
        .unwrap();

    // r[bridge.json.channels-forbidden] - should be rejected with 400
    assert_eq!(response.status(), 400);
}

#[tokio::test]
async fn test_unknown_method() {
    let (roam_addr, _) = start_roam_server().await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let roam_client = connect_to_roam(roam_addr).await;
    let (bridge_addr, _) = start_bridge_server(roam_client).await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let client = reqwest::Client::new();
    let response = client
        .post(format!("http://{}/api/Testbed/nonexistent", bridge_addr))
        .header("Content-Type", "application/json")
        .body(r#"[]"#)
        .send()
        .await
        .unwrap();

    // Unknown method returns 200 with error JSON (it's a BridgeError for now)
    assert_eq!(response.status(), 200);
}

// ============================================================================
// WebSocket tests
// ============================================================================

/// Test WebSocket connection with correct subprotocol.
///
/// r[bridge.ws.subprotocol]
#[tokio::test]
async fn test_websocket_connect_with_subprotocol() {
    let (roam_addr, _) = start_roam_server().await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let roam_client = connect_to_roam(roam_addr).await;
    let (bridge_addr, _) = start_bridge_server(roam_client).await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Connect with the roam-bridge.v1 subprotocol
    let url = format!("ws://{}/api/@ws", bridge_addr);
    let request = tungstenite::http::Request::builder()
        .uri(&url)
        .header("Sec-WebSocket-Protocol", "roam-bridge.v1")
        .header("Host", bridge_addr.to_string())
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header(
            "Sec-WebSocket-Key",
            tungstenite::handshake::client::generate_key(),
        )
        .body(())
        .unwrap();

    let (ws_stream, response) = connect_async(request).await.unwrap();

    // Verify the connection succeeded
    assert_eq!(response.status(), 101);

    // The server should accept the subprotocol
    assert_eq!(
        response
            .headers()
            .get("Sec-WebSocket-Protocol")
            .map(|v| v.to_str().unwrap()),
        Some("roam-bridge.v1")
    );

    // Close the connection
    let (mut write, _read) = ws_stream.split();
    write.close().await.unwrap();
}

/// Test WebSocket echo RPC call.
///
/// r[bridge.ws.request]
/// r[bridge.ws.response]
#[tokio::test]
async fn test_websocket_echo_rpc() {
    let (roam_addr, _) = start_roam_server().await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let roam_client = connect_to_roam(roam_addr).await;
    let (bridge_addr, _) = start_bridge_server(roam_client).await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    // Connect with the roam-bridge.v1 subprotocol
    let url = format!("ws://{}/api/@ws", bridge_addr);
    let request = tungstenite::http::Request::builder()
        .uri(&url)
        .header("Sec-WebSocket-Protocol", "roam-bridge.v1")
        .header("Host", bridge_addr.to_string())
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header(
            "Sec-WebSocket-Key",
            tungstenite::handshake::client::generate_key(),
        )
        .body(())
        .unwrap();

    let (ws_stream, _) = connect_async(request).await.unwrap();
    let (mut write, mut read) = ws_stream.split();

    // Send an echo request
    // r[bridge.ws.request]
    let request_json = serde_json::json!({
        "type": "request",
        "id": 1,
        "service": "Testbed",
        "method": "echo",
        "args": ["hello websocket"]
    });
    write
        .send(tungstenite::Message::Text(request_json.to_string().into()))
        .await
        .unwrap();

    // Read the response
    // r[bridge.ws.response]
    let response = read.next().await.unwrap().unwrap();
    if let tungstenite::Message::Text(text) = response {
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["type"], "response");
        assert_eq!(json["id"], 1);
        assert_eq!(json["result"], "hello websocket");
    } else {
        panic!("Expected text message, got {:?}", response);
    }

    // Close the connection
    write.close().await.unwrap();
}

/// Test WebSocket reverse RPC call.
#[tokio::test]
async fn test_websocket_reverse_rpc() {
    let (roam_addr, _) = start_roam_server().await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let roam_client = connect_to_roam(roam_addr).await;
    let (bridge_addr, _) = start_bridge_server(roam_client).await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let url = format!("ws://{}/api/@ws", bridge_addr);
    let request = tungstenite::http::Request::builder()
        .uri(&url)
        .header("Sec-WebSocket-Protocol", "roam-bridge.v1")
        .header("Host", bridge_addr.to_string())
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header(
            "Sec-WebSocket-Key",
            tungstenite::handshake::client::generate_key(),
        )
        .body(())
        .unwrap();

    let (ws_stream, _) = connect_async(request).await.unwrap();
    let (mut write, mut read) = ws_stream.split();

    // Send a reverse request
    let request_json = serde_json::json!({
        "type": "request",
        "id": 42,
        "service": "Testbed",
        "method": "reverse",
        "args": ["hello"]
    });
    write
        .send(tungstenite::Message::Text(request_json.to_string().into()))
        .await
        .unwrap();

    // Read the response
    let response = read.next().await.unwrap().unwrap();
    if let tungstenite::Message::Text(text) = response {
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["type"], "response");
        assert_eq!(json["id"], 42);
        assert_eq!(json["result"], "olleh");
    } else {
        panic!("Expected text message, got {:?}", response);
    }

    write.close().await.unwrap();
}

/// Test WebSocket unknown method error.
#[tokio::test]
async fn test_websocket_unknown_method() {
    let (roam_addr, _) = start_roam_server().await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let roam_client = connect_to_roam(roam_addr).await;
    let (bridge_addr, _) = start_bridge_server(roam_client).await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let url = format!("ws://{}/api/@ws", bridge_addr);
    let request = tungstenite::http::Request::builder()
        .uri(&url)
        .header("Sec-WebSocket-Protocol", "roam-bridge.v1")
        .header("Host", bridge_addr.to_string())
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header(
            "Sec-WebSocket-Key",
            tungstenite::handshake::client::generate_key(),
        )
        .body(())
        .unwrap();

    let (ws_stream, _) = connect_async(request).await.unwrap();
    let (mut write, mut read) = ws_stream.split();

    // Send request for unknown method
    let request_json = serde_json::json!({
        "type": "request",
        "id": 1,
        "service": "Testbed",
        "method": "nonexistent",
        "args": []
    });
    write
        .send(tungstenite::Message::Text(request_json.to_string().into()))
        .await
        .unwrap();

    // Read the response - should be an error
    let response = read.next().await.unwrap().unwrap();
    if let tungstenite::Message::Text(text) = response {
        let json: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(json["type"], "response");
        assert_eq!(json["id"], 1);
        // Should have an error field
        assert!(json.get("error").is_some());
    } else {
        panic!("Expected text message, got {:?}", response);
    }

    write.close().await.unwrap();
}

/// Test multiple concurrent WebSocket RPC calls (multiplexing).
///
/// r[bridge.ws.multiplexing]
#[tokio::test]
async fn test_websocket_multiplexing() {
    let (roam_addr, _) = start_roam_server().await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let roam_client = connect_to_roam(roam_addr).await;
    let (bridge_addr, _) = start_bridge_server(roam_client).await;
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;

    let url = format!("ws://{}/api/@ws", bridge_addr);
    let request = tungstenite::http::Request::builder()
        .uri(&url)
        .header("Sec-WebSocket-Protocol", "roam-bridge.v1")
        .header("Host", bridge_addr.to_string())
        .header("Connection", "Upgrade")
        .header("Upgrade", "websocket")
        .header("Sec-WebSocket-Version", "13")
        .header(
            "Sec-WebSocket-Key",
            tungstenite::handshake::client::generate_key(),
        )
        .body(())
        .unwrap();

    let (ws_stream, _) = connect_async(request).await.unwrap();
    let (mut write, mut read) = ws_stream.split();

    // Send multiple requests without waiting for responses
    let request1 = serde_json::json!({
        "type": "request",
        "id": 1,
        "service": "Testbed",
        "method": "echo",
        "args": ["first"]
    });
    let request2 = serde_json::json!({
        "type": "request",
        "id": 2,
        "service": "Testbed",
        "method": "echo",
        "args": ["second"]
    });
    let request3 = serde_json::json!({
        "type": "request",
        "id": 3,
        "service": "Testbed",
        "method": "reverse",
        "args": ["third"]
    });

    write
        .send(tungstenite::Message::Text(request1.to_string().into()))
        .await
        .unwrap();
    write
        .send(tungstenite::Message::Text(request2.to_string().into()))
        .await
        .unwrap();
    write
        .send(tungstenite::Message::Text(request3.to_string().into()))
        .await
        .unwrap();

    // Collect all responses
    let mut responses: std::collections::HashMap<u64, serde_json::Value> =
        std::collections::HashMap::new();

    for _ in 0..3 {
        let response = read.next().await.unwrap().unwrap();
        if let tungstenite::Message::Text(text) = response {
            let json: serde_json::Value = serde_json::from_str(&text).unwrap();
            let id = json["id"].as_u64().unwrap();
            responses.insert(id, json);
        }
    }

    // Verify all responses
    assert_eq!(responses.len(), 3);
    assert_eq!(responses[&1]["result"], "first");
    assert_eq!(responses[&2]["result"], "second");
    assert_eq!(responses[&3]["result"], "driht"); // reversed

    write.close().await.unwrap();
}
