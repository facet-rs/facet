use std::convert::Infallible;
use std::time::Duration;

use afl::fuzz;
use roam::Call;
use roam_core::{BareConduit, DriverReplySink, acceptor, initiator, memory_link_pair};
use roam_types::{Handler, MessageFamily, RequestCall, SelfRef};
use spec_proto::{
    Canvas, Color, Config, LookupError, MathError, Measurement, Message, Person, Point, Profile,
    Record, Rectangle, Shape, Status, Tag, Testbed, TestbedClient, TestbedDispatcher,
};

struct NoopHandler;

impl Handler<DriverReplySink> for NoopHandler {
    async fn handle(
        &self,
        _call: SelfRef<RequestCall<'static>>,
        _reply: DriverReplySink,
        _schemas: std::sync::Arc<roam_types::SchemaRecvTracker>,
    ) {
    }
}

#[derive(Clone)]
struct FuzzService;

impl Testbed for FuzzService {
    async fn echo(&self, call: impl Call<String, Infallible>, message: String) {
        call.ok(message).await;
    }

    async fn reverse(&self, call: impl Call<String, Infallible>, message: String) {
        call.ok(message.chars().rev().collect()).await;
    }

    async fn divide(&self, call: impl Call<i64, MathError>, dividend: i64, divisor: i64) {
        if divisor == 0 {
            call.err(MathError::DivisionByZero).await;
        } else {
            call.ok(dividend / divisor).await;
        }
    }

    async fn lookup(&self, call: impl Call<Person, LookupError>, id: u32) {
        match id {
            1 => {
                call.ok(Person {
                    name: "Alice".to_string(),
                    age: 30,
                    email: Some("alice@example.com".to_string()),
                })
                .await
            }
            2 => {
                call.ok(Person {
                    name: "Bob".to_string(),
                    age: 25,
                    email: None,
                })
                .await
            }
            _ => call.err(LookupError::NotFound).await,
        }
    }

    async fn sum(&self, call: impl Call<i64, Infallible>, mut numbers: roam::Rx<i32>) {
        let mut total: i64 = 0;
        while let Ok(Some(n)) = numbers.recv().await {
            total += i64::from(*n);
        }
        call.ok(total).await;
    }

    async fn generate(&self, call: impl Call<(), Infallible>, count: u32, output: roam::Tx<i32>) {
        for i in 0..count as i32 {
            if output.send(i).await.is_err() {
                break;
            }
        }
        let _ = output.close(Default::default()).await;
        call.ok(()).await;
    }

    async fn transform(
        &self,
        call: impl Call<(), Infallible>,
        mut input: roam::Rx<String>,
        output: roam::Tx<String>,
    ) {
        while let Ok(Some(s)) = input.recv().await {
            let _ = output.send(s.clone()).await;
        }
        let _ = output.close(Default::default()).await;
        call.ok(()).await;
    }

    async fn echo_point(&self, call: impl Call<Point, Infallible>, point: Point) {
        call.ok(point).await;
    }

    async fn create_person(
        &self,
        call: impl Call<Person, Infallible>,
        name: String,
        age: u8,
        email: Option<String>,
    ) {
        call.ok(Person { name, age, email }).await;
    }

    async fn rectangle_area(&self, call: impl Call<f64, Infallible>, rect: Rectangle) {
        let width = (rect.bottom_right.x - rect.top_left.x).abs() as f64;
        let height = (rect.bottom_right.y - rect.top_left.y).abs() as f64;
        call.ok(width * height).await;
    }

    async fn parse_color(&self, call: impl Call<Option<Color>, Infallible>, name: String) {
        let color = match name.to_ascii_lowercase().as_str() {
            "red" => Some(Color::Red),
            "green" => Some(Color::Green),
            "blue" => Some(Color::Blue),
            _ => None,
        };
        call.ok(color).await;
    }

    async fn shape_area(&self, call: impl Call<f64, Infallible>, shape: Shape) {
        let area = match shape {
            Shape::Circle { radius } => std::f64::consts::PI * radius * radius,
            Shape::Rectangle { width, height } => width * height,
            Shape::Point => 0.0,
        };
        call.ok(area).await;
    }

    async fn create_canvas(
        &self,
        call: impl Call<Canvas, Infallible>,
        name: String,
        shapes: Vec<Shape>,
        background: Color,
    ) {
        call.ok(Canvas {
            name,
            shapes,
            background,
        })
        .await;
    }

    async fn process_message(&self, call: impl Call<Message, Infallible>, msg: Message) {
        let response = match msg {
            Message::Text(s) => Message::Text(format!("processed: {s}")),
            Message::Number(n) => Message::Number(n.wrapping_mul(2)),
            Message::Data(d) => Message::Data(d.into_iter().rev().collect()),
        };
        call.ok(response).await;
    }

    async fn get_points(&self, call: impl Call<Vec<Point>, Infallible>, count: u32) {
        let points = (0..count as i32)
            .map(|i| Point { x: i, y: i * 2 })
            .collect();
        call.ok(points).await;
    }

    async fn swap_pair(&self, call: impl Call<(String, i32), Infallible>, pair: (i32, String)) {
        call.ok((pair.1, pair.0)).await;
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

struct Cursor<'a> {
    bytes: &'a [u8],
    idx: usize,
}

impl<'a> Cursor<'a> {
    fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, idx: 0 }
    }

    fn next_u8(&mut self) -> u8 {
        if self.bytes.is_empty() {
            return 0;
        }
        let b = self.bytes[self.idx % self.bytes.len()];
        self.idx = self.idx.wrapping_add(1);
        b
    }

    fn next_u32(&mut self) -> u32 {
        let mut buf = [0u8; 4];
        for b in &mut buf {
            *b = self.next_u8();
        }
        u32::from_le_bytes(buf)
    }

    fn next_i64(&mut self) -> i64 {
        let mut buf = [0u8; 8];
        for b in &mut buf {
            *b = self.next_u8();
        }
        i64::from_le_bytes(buf)
    }

    fn bytes(&mut self, max_len: usize) -> Vec<u8> {
        let len = usize::from(self.next_u8()) % (max_len + 1);
        (0..len).map(|_| self.next_u8()).collect()
    }

    fn string(&mut self, max_len: usize) -> String {
        String::from_utf8_lossy(&self.bytes(max_len)).into_owned()
    }
}

async fn setup_client() -> Option<TestbedClient> {
    let (client_link, server_link) = memory_link_pair(64 * 1024);

    let server_conduit: BareConduit<MessageFamily, _> = BareConduit::new(server_link);
    let client_conduit: BareConduit<MessageFamily, _> = BareConduit::new(client_link);

    let server_task = tokio::spawn(async move {
        let Ok(((), _)) = acceptor(server_conduit)
            .establish::<()>(TestbedDispatcher::new(FuzzService))
            .await
        else {
            return;
        };
    });

    let Ok((client, _)) = initiator(client_conduit)
        .establish::<TestbedClient>(NoopHandler)
        .await
    else {
        return None;
    };

    let _ = server_task.await;
    Some(client)
}

async fn run_case(data: &[u8]) {
    let Some(client) = setup_client().await else {
        return;
    };

    let mut cur = Cursor::new(data);
    let ops = (usize::from(cur.next_u8()) % 24) + 1;

    for _ in 0..ops {
        match cur.next_u8() % 10 {
            0 => {
                let s = cur.string(64);
                let _ = tokio::time::timeout(Duration::from_millis(25), client.echo(s)).await;
            }
            1 => {
                let s = cur.string(64);
                let _ = tokio::time::timeout(Duration::from_millis(25), client.reverse(s)).await;
            }
            2 => {
                let a = cur.next_i64();
                let b = cur.next_i64();
                let _ = tokio::time::timeout(Duration::from_millis(25), client.divide(a, b)).await;
            }
            3 => {
                let id = cur.next_u32();
                let _ = tokio::time::timeout(Duration::from_millis(25), client.lookup(id)).await;
            }
            4 => {
                let payload = cur.bytes(1024);
                if let Ok(Ok(resp)) = tokio::time::timeout(
                    Duration::from_millis(25),
                    client.process_message(Message::Data(payload.clone())),
                )
                .await
                {
                    if let Message::Data(ret) = &resp.ret {
                        let mut expected = payload;
                        expected.reverse();
                        assert_eq!(ret, &expected);
                    }
                }
            }
            5 => {
                let (tx, rx) = roam::channel::<i32>();
                let count = usize::from(cur.next_u8() % 8);
                let mut nums = Vec::with_capacity(count);
                for _ in 0..count {
                    nums.push(i32::from_le_bytes(cur.next_u32().to_le_bytes()));
                }
                tokio::spawn(async move {
                    for n in nums {
                        let _ = tx.send(n).await;
                    }
                    let _ = tx.close(Default::default()).await;
                });
                let _ = tokio::time::timeout(Duration::from_millis(30), client.sum(rx)).await;
            }
            6 => {
                let count = cur.next_u32() % 16;
                let (tx, mut rx) = roam::channel::<i32>();
                let recv_task = tokio::spawn(async move {
                    let mut out = Vec::new();
                    while let Ok(Some(n)) = rx.recv().await {
                        out.push(*n);
                        if out.len() > 32 {
                            break;
                        }
                    }
                    out
                });
                let _ = tokio::time::timeout(Duration::from_millis(40), client.generate(count, tx))
                    .await;
                let _ = tokio::time::timeout(Duration::from_millis(40), recv_task).await;
            }
            7 => {
                let (in_tx, in_rx) = roam::channel::<String>();
                let (out_tx, mut out_rx) = roam::channel::<String>();
                let count = usize::from(cur.next_u8() % 6);
                let mut vals = Vec::new();
                for _ in 0..count {
                    vals.push(cur.string(24));
                }
                tokio::spawn(async move {
                    for s in vals {
                        let _ = in_tx.send(s).await;
                    }
                    let _ = in_tx.close(Default::default()).await;
                });
                let recv_task = tokio::spawn(async move {
                    let mut out = Vec::new();
                    while let Ok(Some(s)) = out_rx.recv().await {
                        out.push(s.clone());
                        if out.len() > 12 {
                            break;
                        }
                    }
                    out
                });
                let _ = tokio::time::timeout(
                    Duration::from_millis(40),
                    client.transform(in_rx, out_tx),
                )
                .await;
                let _ = tokio::time::timeout(Duration::from_millis(40), recv_task).await;
            }
            8 => {
                let point = Point {
                    x: i32::from_le_bytes(cur.next_u32().to_le_bytes()),
                    y: i32::from_le_bytes(cur.next_u32().to_le_bytes()),
                };
                let _ =
                    tokio::time::timeout(Duration::from_millis(25), client.echo_point(point)).await;
            }
            _ => {
                let pair = (
                    i32::from_le_bytes(cur.next_u32().to_le_bytes()),
                    cur.string(32),
                );
                let _ =
                    tokio::time::timeout(Duration::from_millis(25), client.swap_pair(pair)).await;
            }
        }
    }
}

fn main() {
    fuzz!(|data: &[u8]| {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("create runtime");
        rt.block_on(run_case(data));
    });
}
