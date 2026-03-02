//! Browser tests for roam Rust/Wasm client.
//!
//! This crate only compiles for wasm32 target. Build with:
//! ```
//! wasm-pack build --target web rust/wasm-browser-tests
//! ```

#![cfg(target_arch = "wasm32")]

use roam_core::{BareConduit, Driver, initiator};
use roam_types::{MessageFamily, Parity};
use roam_websocket::WsLink;
use spec_proto::{Color, LookupError, MathError, Message, Point, Rectangle, Shape, TestbedClient};
use wasm_bindgen::prelude::*;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);

    #[wasm_bindgen(js_namespace = console)]
    fn error(s: &str);
}

macro_rules! console_log {
    ($($t:tt)*) => (log(&format!($($t)*)))
}

macro_rules! console_error {
    ($($t:tt)*) => (error(&format!($($t)*)))
}

#[wasm_bindgen]
pub struct TestResults {
    results: Vec<TestResult>,
}

struct TestResult {
    name: String,
    passed: bool,
    error: Option<String>,
}

#[wasm_bindgen]
impl TestResults {
    #[wasm_bindgen(getter)]
    pub fn count(&self) -> usize {
        self.results.len()
    }

    pub fn get_name(&self, index: usize) -> Option<String> {
        self.results.get(index).map(|r| r.name.clone())
    }

    pub fn get_passed(&self, index: usize) -> bool {
        self.results.get(index).is_some_and(|r| r.passed)
    }

    pub fn get_error(&self, index: usize) -> Option<String> {
        self.results
            .get(index)
            .and_then(|r| r.error.as_ref().cloned())
    }

    pub fn all_passed(&self) -> bool {
        self.results.iter().all(|r| r.passed)
    }
}

/// Run all tests against a WebSocket server at the given URL.
#[wasm_bindgen]
pub async fn run_tests(ws_url: &str) -> TestResults {
    let mut results = Vec::new();

    console_log!("Connecting to {ws_url}...");

    let link = match WsLink::connect(ws_url).await {
        Ok(l) => l,
        Err(e) => {
            console_error!("Failed to connect: {e:?}");
            results.push(TestResult {
                name: "connect".into(),
                passed: false,
                error: Some(format!("{e:?}")),
            });
            return TestResults { results };
        }
    };

    console_log!("Connected! Performing handshake...");

    let conduit = BareConduit::<MessageFamily, _>::new(link);
    let (mut session, handle, _sh) = match initiator(conduit).establish().await {
        Ok(result) => result,
        Err(e) => {
            console_error!("Handshake failed: {e:?}");
            results.push(TestResult {
                name: "handshake".into(),
                passed: false,
                error: Some(format!("{e:?}")),
            });
            return TestResults { results };
        }
    };

    console_log!("Handshake complete.");

    let mut driver = Driver::new(handle, (), Parity::Odd);
    let caller = driver.caller();

    wasm_bindgen_futures::spawn_local(async move {
        session.run().await;
    });
    wasm_bindgen_futures::spawn_local(async move {
        driver.run().await;
    });

    let client = TestbedClient::new(caller);

    // Run echo tests
    run_echo_tests(&client, &mut results).await;

    // Run complex type tests
    run_complex_tests(&client, &mut results).await;

    // Run fallible tests
    run_fallible_tests(&client, &mut results).await;

    let passed = results.iter().filter(|r| r.passed).count();
    let total = results.len();
    console_log!("Tests complete: {passed}/{total} passed");

    TestResults { results }
}

async fn run_echo_tests(
    client: &TestbedClient<impl roam_types::Caller>,
    results: &mut Vec<TestResult>,
) {
    // Test: echo
    console_log!("Testing echo...");
    match client.echo("Hello from Rust Wasm!".into()).await {
        Ok(result) if result == "Hello from Rust Wasm!" => {
            results.push(TestResult {
                name: "echo".into(),
                passed: true,
                error: None,
            });
        }
        Ok(result) => {
            results.push(TestResult {
                name: "echo".into(),
                passed: false,
                error: Some(format!(
                    "expected 'Hello from Rust Wasm!', got '{}'",
                    result
                )),
            });
        }
        Err(e) => {
            console_error!("echo failed: {e:?}");
            results.push(TestResult {
                name: "echo".into(),
                passed: false,
                error: Some(format!("{e:?}")),
            });
        }
    }

    // Test: reverse
    console_log!("Testing reverse...");
    match client.reverse("Hello".into()).await {
        Ok(result) if result == "olleH" => {
            results.push(TestResult {
                name: "reverse".into(),
                passed: true,
                error: None,
            });
        }
        Ok(result) => {
            results.push(TestResult {
                name: "reverse".into(),
                passed: false,
                error: Some(format!("expected 'olleH', got '{}'", result)),
            });
        }
        Err(e) => {
            console_error!("reverse failed: {e:?}");
            results.push(TestResult {
                name: "reverse".into(),
                passed: false,
                error: Some(format!("{e:?}")),
            });
        }
    }
}

async fn run_complex_tests(
    client: &TestbedClient<impl roam_types::Caller>,
    results: &mut Vec<TestResult>,
) {
    // Test: echo_point
    console_log!("Testing echo_point...");
    let point = Point { x: 42, y: -17 };
    match client.echo_point(point.clone()).await {
        Ok(result) if result == point => {
            results.push(TestResult {
                name: "echo_point".into(),
                passed: true,
                error: None,
            });
        }
        Ok(result) => {
            results.push(TestResult {
                name: "echo_point".into(),
                passed: false,
                error: Some(format!("expected {point:?}, got {:?}", result)),
            });
        }
        Err(e) => {
            results.push(TestResult {
                name: "echo_point".into(),
                passed: false,
                error: Some(format!("{e:?}")),
            });
        }
    }

    // Test: create_person
    console_log!("Testing create_person...");
    match client
        .create_person("Alice".into(), 30, Some("alice@example.com".into()))
        .await
    {
        Ok(result)
            if result.name == "Alice"
                && result.age == 30
                && result.email.as_deref() == Some("alice@example.com") =>
        {
            results.push(TestResult {
                name: "create_person".into(),
                passed: true,
                error: None,
            });
        }
        Ok(result) => {
            results.push(TestResult {
                name: "create_person".into(),
                passed: false,
                error: Some(format!("unexpected person: {:?}", result)),
            });
        }
        Err(e) => {
            results.push(TestResult {
                name: "create_person".into(),
                passed: false,
                error: Some(format!("{e:?}")),
            });
        }
    }

    // Test: rectangle_area
    console_log!("Testing rectangle_area...");
    let rect = Rectangle {
        top_left: Point { x: 0, y: 0 },
        bottom_right: Point { x: 10, y: 5 },
        label: None,
    };
    match client.rectangle_area(rect).await {
        Ok(result) if (result - 50.0).abs() < 0.001 => {
            results.push(TestResult {
                name: "rectangle_area".into(),
                passed: true,
                error: None,
            });
        }
        Ok(result) => {
            results.push(TestResult {
                name: "rectangle_area".into(),
                passed: false,
                error: Some(format!("expected 50.0, got {}", result)),
            });
        }
        Err(e) => {
            results.push(TestResult {
                name: "rectangle_area".into(),
                passed: false,
                error: Some(format!("{e:?}")),
            });
        }
    }

    // Test: parse_color
    console_log!("Testing parse_color...");
    match client.parse_color("red".into()).await {
        Ok(result) if matches!(result, Some(Color::Red)) => {
            results.push(TestResult {
                name: "parse_color".into(),
                passed: true,
                error: None,
            });
        }
        Ok(result) => {
            results.push(TestResult {
                name: "parse_color".into(),
                passed: false,
                error: Some(format!("expected Some(Red), got {:?}", result)),
            });
        }
        Err(e) => {
            results.push(TestResult {
                name: "parse_color".into(),
                passed: false,
                error: Some(format!("{e:?}")),
            });
        }
    }

    // Test: shape_area (Circle)
    console_log!("Testing shape_area (Circle)...");
    match client.shape_area(Shape::Circle { radius: 2.0 }).await {
        Ok(result) if (result - std::f64::consts::PI * 4.0).abs() < 0.001 => {
            results.push(TestResult {
                name: "shape_area_circle".into(),
                passed: true,
                error: None,
            });
        }
        Ok(result) => {
            results.push(TestResult {
                name: "shape_area_circle".into(),
                passed: false,
                error: Some(format!(
                    "expected {}, got {}",
                    std::f64::consts::PI * 4.0,
                    result
                )),
            });
        }
        Err(e) => {
            results.push(TestResult {
                name: "shape_area_circle".into(),
                passed: false,
                error: Some(format!("{e:?}")),
            });
        }
    }

    // Test: shape_area (Rectangle)
    console_log!("Testing shape_area (Rectangle)...");
    match client
        .shape_area(Shape::Rectangle {
            width: 3.0,
            height: 4.0,
        })
        .await
    {
        Ok(result) if (result - 12.0).abs() < 0.001 => {
            results.push(TestResult {
                name: "shape_area_rectangle".into(),
                passed: true,
                error: None,
            });
        }
        Ok(result) => {
            results.push(TestResult {
                name: "shape_area_rectangle".into(),
                passed: false,
                error: Some(format!("expected 12.0, got {}", result)),
            });
        }
        Err(e) => {
            results.push(TestResult {
                name: "shape_area_rectangle".into(),
                passed: false,
                error: Some(format!("{e:?}")),
            });
        }
    }

    // Test: get_points
    console_log!("Testing get_points...");
    match client.get_points(3).await {
        Ok(result)
            if result.len() == 3
                && result[0] == Point { x: 0, y: 0 }
                && result[1] == Point { x: 1, y: 2 }
                && result[2] == Point { x: 2, y: 4 } =>
        {
            results.push(TestResult {
                name: "get_points".into(),
                passed: true,
                error: None,
            });
        }
        Ok(result) => {
            results.push(TestResult {
                name: "get_points".into(),
                passed: false,
                error: Some(format!("unexpected points: {:?}", result)),
            });
        }
        Err(e) => {
            results.push(TestResult {
                name: "get_points".into(),
                passed: false,
                error: Some(format!("{e:?}")),
            });
        }
    }

    // Test: swap_pair
    console_log!("Testing swap_pair...");
    match client.swap_pair((42, "hello".into())).await {
        Ok(result) if result.0 == "hello" && result.1 == 42 => {
            results.push(TestResult {
                name: "swap_pair".into(),
                passed: true,
                error: None,
            });
        }
        Ok(result) => {
            results.push(TestResult {
                name: "swap_pair".into(),
                passed: false,
                error: Some(format!("expected (\"hello\", 42), got {:?}", result)),
            });
        }
        Err(e) => {
            results.push(TestResult {
                name: "swap_pair".into(),
                passed: false,
                error: Some(format!("{e:?}")),
            });
        }
    }

    // Test: process_message (Text)
    console_log!("Testing process_message (Text)...");
    match client.process_message(Message::Text("hello".into())).await {
        Ok(result) if matches!(&result, Message::Text(s) if s == "Processed: hello") => {
            results.push(TestResult {
                name: "process_message_text".into(),
                passed: true,
                error: None,
            });
        }
        Ok(result) => {
            results.push(TestResult {
                name: "process_message_text".into(),
                passed: false,
                error: Some(format!(
                    "expected Text(\"Processed: hello\"), got {:?}",
                    result
                )),
            });
        }
        Err(e) => {
            results.push(TestResult {
                name: "process_message_text".into(),
                passed: false,
                error: Some(format!("{e:?}")),
            });
        }
    }

    // Test: process_message (Number)
    console_log!("Testing process_message (Number)...");
    match client.process_message(Message::Number(21)).await {
        Ok(result) if matches!(&result, Message::Number(n) if *n == 42) => {
            results.push(TestResult {
                name: "process_message_number".into(),
                passed: true,
                error: None,
            });
        }
        Ok(result) => {
            results.push(TestResult {
                name: "process_message_number".into(),
                passed: false,
                error: Some(format!("expected Number(42), got {:?}", result)),
            });
        }
        Err(e) => {
            results.push(TestResult {
                name: "process_message_number".into(),
                passed: false,
                error: Some(format!("{e:?}")),
            });
        }
    }
}

async fn run_fallible_tests(
    client: &TestbedClient<impl roam_types::Caller>,
    results: &mut Vec<TestResult>,
) {
    // Test: divide (success)
    console_log!("Testing divide (success)...");
    match client.divide(10, 2).await {
        Ok(result) if result == 5 => {
            results.push(TestResult {
                name: "divide_success".into(),
                passed: true,
                error: None,
            });
        }
        Ok(result) => {
            results.push(TestResult {
                name: "divide_success".into(),
                passed: false,
                error: Some(format!("expected 5, got {:?}", result)),
            });
        }
        Err(e) => {
            results.push(TestResult {
                name: "divide_success".into(),
                passed: false,
                error: Some(format!("{e:?}")),
            });
        }
    }

    // Test: divide (error - division by zero)
    console_log!("Testing divide (error)...");
    match client.divide(10, 0).await {
        Err(roam_types::RoamError::User(MathError::DivisionByZero)) => {
            results.push(TestResult {
                name: "divide_error".into(),
                passed: true,
                error: None,
            });
        }
        Ok(result) => {
            results.push(TestResult {
                name: "divide_error".into(),
                passed: false,
                error: Some(format!(
                    "expected DivisionByZero error, got Ok({:?})",
                    result
                )),
            });
        }
        Err(e) => {
            results.push(TestResult {
                name: "divide_error".into(),
                passed: false,
                error: Some(format!("expected DivisionByZero, got {e:?}")),
            });
        }
    }

    // Test: lookup (success)
    console_log!("Testing lookup (success)...");
    match client.lookup(1).await {
        Ok(result)
            if result.name == "Alice"
                && result.age == 30
                && result.email.as_deref() == Some("alice@example.com") =>
        {
            results.push(TestResult {
                name: "lookup_success".into(),
                passed: true,
                error: None,
            });
        }
        Ok(result) => {
            results.push(TestResult {
                name: "lookup_success".into(),
                passed: false,
                error: Some(format!("unexpected person: {:?}", result)),
            });
        }
        Err(e) => {
            results.push(TestResult {
                name: "lookup_success".into(),
                passed: false,
                error: Some(format!("{e:?}")),
            });
        }
    }

    // Test: lookup (error - not found)
    console_log!("Testing lookup (error)...");
    match client.lookup(999).await {
        Err(roam_types::RoamError::User(LookupError::NotFound)) => {
            results.push(TestResult {
                name: "lookup_error".into(),
                passed: true,
                error: None,
            });
        }
        Ok(result) => {
            results.push(TestResult {
                name: "lookup_error".into(),
                passed: false,
                error: Some(format!("expected NotFound error, got Ok({:?})", result)),
            });
        }
        Err(e) => {
            results.push(TestResult {
                name: "lookup_error".into(),
                passed: false,
                error: Some(format!("expected NotFound, got {e:?}")),
            });
        }
    }
}
