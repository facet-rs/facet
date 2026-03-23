//! Browser tests for vox in-process transport (Rust WASM acceptor).
//!
//! This crate only compiles for wasm32 target. Build with:
//! ```
//! wasm-pack build --target web rust/wasm-inprocess-tests
//! ```

#![cfg(target_arch = "wasm32")]

use spec_proto::{
    Canvas, Color, Config, LookupError, MathError, Measurement, Message, Person, Point, Profile,
    Record, Rectangle, Shape, Status, Tag, TaggedPoint, Testbed, TestbedClient, TestbedDispatcher,
};
use vox_core::acceptor_on;
use vox_inprocess::JsInProcessLink;
use vox_types::{Rx, Tx};
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

    async fn sum(&self, mut numbers: Rx<i32>) -> i64 {
        let mut total = 0_i64;
        while let Ok(Some(n)) = numbers.recv().await {
            total += *n as i64;
        }
        total
    }

    async fn generate(&self, count: u32, output: Tx<i32>) {
        for value in 0..count {
            let _ = output.send(value as i32).await;
        }
        let _ = output.close(Default::default()).await;
    }

    async fn generate_retry_non_idem(&self, count: u32, output: Tx<i32>) {
        self.generate(count, output).await;
    }

    async fn generate_retry_idem(&self, count: u32, output: Tx<i32>) {
        self.generate(count, output).await;
    }

    async fn transform(&self, mut input: Rx<String>, output: Tx<String>) {
        while let Ok(Some(item)) = input.recv().await {
            let _ = output.send(item.to_uppercase()).await;
        }
        let _ = output.close(Default::default()).await;
    }

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

    async fn echo_bytes(&self, data: Vec<u8>) -> Vec<u8> {
        data
    }

    async fn echo_bool(&self, b: bool) -> bool {
        b
    }

    async fn echo_u64(&self, n: u64) -> u64 {
        n
    }

    async fn echo_option_string(&self, s: Option<String>) -> Option<String> {
        s
    }

    async fn sum_large(&self, mut numbers: Rx<i32>) -> i64 {
        let mut total = 0_i64;
        while let Ok(Some(n)) = numbers.recv().await {
            total += *n as i64;
        }
        total
    }

    async fn generate_large(&self, count: u32, output: Tx<i32>) {
        self.generate(count, output).await;
    }

    async fn all_colors(&self) -> Vec<Color> {
        vec![Color::Red, Color::Green, Color::Blue]
    }

    async fn describe_point(&self, label: String, x: i32, y: i32, active: bool) -> TaggedPoint {
        TaggedPoint {
            label,
            x,
            y,
            active,
        }
    }

    async fn echo_shape(&self, shape: Shape) -> Shape {
        shape
    }

    async fn echo_status_v1(&self, status: Status) -> Status {
        status
    }

    async fn echo_tag_v1(&self, tag: Tag) -> Tag {
        tag
    }

    async fn echo_profile(&self, profile: Profile) -> Profile {
        profile
    }

    async fn echo_record(&self, record: Record) -> Record {
        record
    }

    async fn echo_status(&self, status: Status) -> Status {
        status
    }

    async fn echo_tag(&self, tag: Tag) -> Tag {
        tag
    }

    async fn echo_measurement(&self, m: Measurement) -> Measurement {
        m
    }

    async fn echo_config(&self, c: Config) -> Config {
        c
    }
}

/// Start a vox acceptor (server) using the in-process transport.
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
