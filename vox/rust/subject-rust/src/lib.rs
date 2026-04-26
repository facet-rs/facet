//! Shared Testbed service implementation — used by the subject binary and
//! by bench_client for in-process FFI benchmarks.

use spec_proto::{
    Canvas, Color, Config, GnarlyPayload, LookupError, MathError, Measurement, Message, Person,
    Point, Profile, Record, Rectangle, Shape, Status, Tag, TaggedPoint, Testbed,
};
use tracing::{debug, error, info, instrument};
use vox::{Rx, Tx};

mod ffi;

#[derive(Clone)]
pub struct TestbedService;

pub async fn stream_retry_probe_values(count: u32, output: Tx<i32>) {
    for i in 0..count as i32 {
        debug!(i, "sending value");
        if let Err(e) = output.send(i).await {
            error!(i, ?e, "send failed");
            break;
        }
    }
    output.close(Default::default()).await.ok();
}

pub async fn stream_post_reply_values(output: Tx<i32>) {
    let _ = moire::task::spawn(async move {
        moire::time::sleep(std::time::Duration::from_millis(10)).await;
        for i in 0..5 {
            debug!(i, "post-reply sending value");
            if let Err(e) = output.send(i).await {
                error!(i, ?e, "post-reply send failed");
                break;
            }
        }
        output.close(Default::default()).await.ok();
    });
}

pub async fn sum_post_reply_values(mut input: Rx<i32>, result: Tx<i64>) {
    let _ = moire::task::spawn(async move {
        let mut total: i64 = 0;
        while let Ok(Some(n)) = input.recv().await {
            let n = n.get();
            debug!(n = *n, total, "post-reply received number");
            total += *n as i64;
        }
        if let Err(e) = result.send(total).await {
            error!(total, ?e, "post-reply result send failed");
        }
        result.close(Default::default()).await.ok();
    });
}

impl Testbed for TestbedService {
    #[instrument(skip(self))]
    async fn echo(&self, message: String) -> String {
        info!("echo called");
        message
    }

    #[instrument(skip(self))]
    async fn reverse(&self, message: String) -> String {
        info!("reverse called");
        message.chars().rev().collect()
    }

    #[instrument(skip(self))]
    async fn divide(&self, dividend: i64, divisor: i64) -> Result<i64, MathError> {
        info!("divide called");
        if divisor == 0 {
            Err(MathError::DivisionByZero)
        } else {
            dividend.checked_div(divisor).ok_or(MathError::Overflow)
        }
    }

    #[instrument(skip(self))]
    async fn lookup(&self, id: u32) -> Result<Person, LookupError> {
        info!("lookup called");
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
            100..=199 => Err(LookupError::AccessDenied),
            _ => Err(LookupError::NotFound),
        }
    }

    #[instrument(skip(self, numbers))]
    async fn sum(&self, mut numbers: Rx<i32>) -> i64 {
        info!("sum called");
        let mut total: i64 = 0;
        while let Ok(Some(n)) = numbers.recv().await {
            let n = n.get();
            debug!(n = *n, total, "received number");
            total += *n as i64;
        }
        info!(total, "sum complete");
        total
    }

    #[instrument(skip(self, output))]
    async fn generate(&self, count: u32, output: Tx<i32>) {
        info!(count, "generate called");
        stream_retry_probe_values(count, output).await;
    }

    #[instrument(skip(self, output))]
    async fn generate_retry_non_idem(&self, count: u32, output: Tx<i32>) {
        info!(count, "generate_retry_non_idem called");
        stream_retry_probe_values(count, output).await;
    }

    #[instrument(skip(self, output))]
    async fn generate_retry_idem(&self, count: u32, output: Tx<i32>) {
        info!(count, "generate_retry_idem called");
        stream_retry_probe_values(count, output).await;
    }

    #[instrument(skip(self, input, output))]
    async fn transform(&self, mut input: Rx<String>, output: Tx<String>) {
        info!("transform called");
        while let Ok(Some(s)) = input.recv().await {
            let s = s.get();
            debug!(s = ?*s, "transforming");
            let _ = output.send(s.clone()).await;
        }
        output.close(Default::default()).await.ok();
    }

    #[instrument(skip(self, output))]
    async fn post_reply_generate(&self, output: Tx<i32>) {
        info!("post_reply_generate called");
        stream_post_reply_values(output).await;
    }

    #[instrument(skip(self, input, result))]
    async fn post_reply_sum(&self, input: Rx<i32>, result: Tx<i64>) {
        info!("post_reply_sum called");
        sum_post_reply_values(input, result).await;
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

    async fn echo_gnarly(&self, payload: GnarlyPayload) -> GnarlyPayload {
        payload
    }

    async fn process_message(&self, msg: Message) -> Message {
        match msg {
            Message::Text(s) => Message::Text(format!("processed: {s}")),
            Message::Number(n) => Message::Number(n * 2),
            Message::Data(d) => Message::Data(d.into_iter().rev().collect()),
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
        let mut total: i64 = 0;
        while let Ok(Some(n)) = numbers.recv().await {
            let n = n.get();
            total += *n as i64;
        }
        total
    }

    async fn generate_large(&self, count: u32, output: Tx<i32>) {
        stream_retry_probe_values(count, output).await;
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
