//! Browser tests for roam in-process transport (Rust WASM acceptor).
//!
//! This crate only compiles for wasm32 target. Build with:
//! ```
//! wasm-pack build --target web rust/wasm-inprocess-tests
//! ```

#![cfg(target_arch = "wasm32")]

use roam_core::acceptor_on;
use roam_inprocess::JsInProcessLink;
use roam_types::{Rx, Tx};
use spec_proto::{
    Canvas, Color, LookupError, MathError, Message, Person, Point, Rectangle, Shape, Testbed,
    TestbedClient, TestbedDispatcher,
};
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

#[derive(Clone)]
struct TestbedService;

impl Testbed for TestbedService {
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
            2 => Ok(Person {
                name: "Bob".to_string(),
                age: 25,
                email: None,
            }),
            3 => Ok(Person {
                name: "Charlie".to_string(),
                age: 35,
                email: Some("charlie@example.com".to_string()),
            }),
            _ => Err(LookupError::NotFound),
        }
    }

    // Channel methods (sum, generate, transform) are not testable on wasm32
    // because Tx/Rx runtime methods are cfg-gated to non-wasm targets.
    // These stubs satisfy the trait but won't be exercised by tests.

    async fn sum(&self, _numbers: Rx<i32>) -> i64 {
        0
    }

    async fn generate(&self, _count: u32, _output: Tx<i32>) {}

    async fn generate_retry_non_idem(&self, _count: u32, _output: Tx<i32>) {}

    async fn generate_retry_idem(&self, _count: u32, _output: Tx<i32>) {}

    async fn transform(&self, _input: Rx<String>, _output: Tx<String>) {}

    async fn echo_point(&self, point: Point) -> Point {
        point
    }

    async fn create_person(&self, name: String, age: u8, email: Option<String>) -> Person {
        Person { name, age, email }
    }

    async fn rectangle_area(&self, rect: Rectangle) -> f64 {
        let width = (rect.bottom_right.x - rect.top_left.x).abs() as f64;
        let height = (rect.bottom_right.y - rect.top_left.y).abs() as f64;
        width * height
    }

    async fn parse_color(&self, name: String) -> Option<Color> {
        match name.to_lowercase().as_str() {
            "red" => Some(Color::Red),
            "green" => Some(Color::Green),
            "blue" => Some(Color::Blue),
            _ => None,
        }
    }

    async fn shape_area(&self, shape: Shape) -> f64 {
        match shape {
            Shape::Circle { radius } => std::f64::consts::PI * radius * radius,
            Shape::Rectangle { width, height } => width * height,
            Shape::Point => 0.0,
        }
    }

    async fn create_canvas(&self, name: String, shapes: Vec<Shape>, background: Color) -> Canvas {
        Canvas {
            name,
            shapes,
            background,
        }
    }

    async fn process_message(&self, msg: Message) -> Message {
        match msg {
            Message::Text(text) => Message::Text(format!("Processed: {}", text)),
            Message::Number(n) => Message::Number(n * 2),
            Message::Data(data) => Message::Data(data.into_iter().rev().collect()),
        }
    }

    async fn get_points(&self, count: u32) -> Vec<Point> {
        (0..count as i32)
            .map(|i| Point { x: i, y: i * 2 })
            .collect()
    }

    async fn swap_pair(&self, pair: (i32, String)) -> (String, i32) {
        (pair.1, pair.0)
    }
}

/// Start a roam acceptor (server) using the in-process transport.
///
/// Returns a `JsInProcessLink` that JS should wire to an `InProcessTransport`.
/// The acceptor runs in the background via `wasm_bindgen_futures::spawn_local`.
#[wasm_bindgen]
pub fn start_acceptor(on_message: js_sys::Function) -> JsInProcessLink {
    let mut js_link = JsInProcessLink::new(on_message);
    let link = js_link
        .take_link()
        .expect("take_link should succeed on fresh JsInProcessLink");

    wasm_bindgen_futures::spawn_local(async move {
        console_log!("In-process acceptor: starting handshake...");

        match acceptor_on(link)
            .establish::<TestbedClient>(TestbedDispatcher::new(TestbedService))
            .await
        {
            Ok((_root_caller_guard, _sh)) => {
                console_log!("In-process acceptor: session established");
                // Keep the session alive
                std::future::pending::<()>().await;
            }
            Err(e) => {
                console_error!("In-process acceptor: handshake failed: {:?}", e);
            }
        }
    });

    js_link
}
