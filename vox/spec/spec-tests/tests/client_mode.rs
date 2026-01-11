//! Client-mode compliance tests.
//!
//! These tests run the spec-tests harness as a server and spawn the subject
//! in client mode. This validates that the generated client code works
//! correctly against a reference implementation.

use roam::session::{Rx, Tx};
use spec_tests::harness::run_async;
use spec_tests::testbed;
use spec_tests::{Canvas, Color, Message, Person, Point, Rectangle, Shape};

// Service implementation for the server side
#[derive(Clone)]
struct TestbedService;

impl testbed::Testbed for TestbedService {
    async fn echo(&self, message: String) -> Result<String, testbed::RoamError<testbed::Never>> {
        Ok(message)
    }

    async fn reverse(&self, message: String) -> Result<String, testbed::RoamError<testbed::Never>> {
        Ok(message.chars().rev().collect())
    }

    async fn sum(&self, mut numbers: Rx<i32>) -> Result<i64, testbed::RoamError<testbed::Never>> {
        let mut total: i64 = 0;
        while let Some(n) = numbers.recv().await.ok().flatten() {
            total += n as i64;
        }
        Ok(total)
    }

    async fn generate(
        &self,
        count: u32,
        output: Tx<i32>,
    ) -> Result<(), testbed::RoamError<testbed::Never>> {
        for i in 0..count as i32 {
            let _ = output.send(&i).await;
        }
        Ok(())
    }

    async fn transform(
        &self,
        mut input: Rx<String>,
        output: Tx<String>,
    ) -> Result<(), testbed::RoamError<testbed::Never>> {
        while let Some(s) = input.recv().await.ok().flatten() {
            let _ = output.send(&s).await;
        }
        Ok(())
    }

    async fn echo_point(&self, point: Point) -> Result<Point, testbed::RoamError<testbed::Never>> {
        Ok(point)
    }

    async fn create_person(
        &self,
        name: String,
        age: u8,
        email: Option<String>,
    ) -> Result<Person, testbed::RoamError<testbed::Never>> {
        Ok(Person { name, age, email })
    }

    async fn rectangle_area(
        &self,
        rect: Rectangle,
    ) -> Result<f64, testbed::RoamError<testbed::Never>> {
        let width = (rect.bottom_right.x - rect.top_left.x).abs() as f64;
        let height = (rect.bottom_right.y - rect.top_left.y).abs() as f64;
        Ok(width * height)
    }

    async fn parse_color(
        &self,
        name: String,
    ) -> Result<Option<Color>, testbed::RoamError<testbed::Never>> {
        let color = match name.to_lowercase().as_str() {
            "red" => Some(Color::Red),
            "green" => Some(Color::Green),
            "blue" => Some(Color::Blue),
            _ => None,
        };
        Ok(color)
    }

    async fn shape_area(&self, shape: Shape) -> Result<f64, testbed::RoamError<testbed::Never>> {
        let area = match shape {
            Shape::Circle { radius } => std::f64::consts::PI * radius * radius,
            Shape::Rectangle { width, height } => width * height,
            Shape::Point => 0.0,
        };
        Ok(area)
    }

    async fn create_canvas(
        &self,
        name: String,
        shapes: Vec<Shape>,
        background: Color,
    ) -> Result<Canvas, testbed::RoamError<testbed::Never>> {
        Ok(Canvas {
            name,
            shapes,
            background,
        })
    }

    async fn process_message(
        &self,
        msg: Message,
    ) -> Result<Message, testbed::RoamError<testbed::Never>> {
        let response = match msg {
            Message::Text(s) => Message::Text(format!("processed: {s}")),
            Message::Number(n) => Message::Number(n * 2),
            Message::Data(d) => Message::Data(d.into_iter().rev().collect()),
        };
        Ok(response)
    }

    async fn get_points(
        &self,
        count: u32,
    ) -> Result<Vec<Point>, testbed::RoamError<testbed::Never>> {
        let points = (0..count as i32)
            .map(|i| Point { x: i, y: i * 2 })
            .collect();
        Ok(points)
    }

    async fn swap_pair(
        &self,
        pair: (i32, String),
    ) -> Result<(String, i32), testbed::RoamError<testbed::Never>> {
        Ok((pair.1, pair.0))
    }
}

// r[verify unary.initiate] - Generated client can make unary calls
#[test]
fn client_mode_echo() {
    run_async(async {
        let dispatcher = testbed::TestbedDispatcher::new(TestbedService);
        spec_tests::harness::run_as_server(dispatcher, "echo").await
    })
    .unwrap();
}

// r[verify channeling.type] - Generated client can send streaming data
// r[verify channeling.type] - Client pushes data via Rx channel
#[test]
fn client_mode_sum() {
    run_async(async {
        let dispatcher = testbed::TestbedDispatcher::new(TestbedService);
        spec_tests::harness::run_as_server(dispatcher, "sum").await
    })
    .unwrap();
}

// r[verify channeling.type] - Generated client can receive streaming data
// r[verify channeling.type] - Server pushes data via Tx channel
#[test]
fn client_mode_generate() {
    run_async(async {
        let dispatcher = testbed::TestbedDispatcher::new(TestbedService);
        spec_tests::harness::run_as_server(dispatcher, "generate").await
    })
    .unwrap();
}
