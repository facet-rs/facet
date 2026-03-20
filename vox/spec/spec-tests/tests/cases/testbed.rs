use roam::RoamError;
use spec_proto::{Color, LookupError, MathError, Message, Point, Rectangle, Shape, Tag};
use spec_tests::harness::{
    SubjectSpec, accept_subject_spec, run_async, run_subject_client_scenario,
};

// r[verify call.initiate]
// r[verify call.complete]
// r[verify call.lifecycle.single-response]
// r[verify call.lifecycle.ordering]
// r[verify transport.message.binary]
pub fn run_rpc_echo_roundtrip(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child) = accept_subject_spec(spec).await?;
        let resp = client
            .echo("hello".to_string())
            .await
            .map_err(|e| format!("echo: {e:?}"))?;
        if resp != "hello" {
            return Err(format!("expected \"hello\", got {:?}", resp));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify call.error.user]
pub fn run_rpc_user_error_roundtrip(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child) = accept_subject_spec(spec).await?;
        let result = client.divide(10, 0).await;
        match result {
            Err(RoamError::User(MathError::DivisionByZero)) => {}
            Ok(resp) => {
                return Err(format!(
                    "expected Err(User(DivisionByZero)), got Ok({})",
                    resp
                ));
            }
            Err(other) => {
                return Err(format!(
                    "expected Err(User(DivisionByZero)), got Err({other:?})"
                ));
            }
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify call.pipelining.allowed]
// r[verify call.pipelining.independence]
// r[verify core.call]
// r[verify core.call.request-id]
pub fn run_rpc_pipelining_multiple_requests(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child) = accept_subject_spec(spec).await?;
        let (r1, r2, r3) = tokio::join!(
            client.echo("first".to_string()),
            client.echo("second".to_string()),
            client.echo("third".to_string()),
        );
        if r1.map_err(|e| format!("{e:?}"))? != "first" {
            return Err("pipelining: first response wrong".to_string());
        }
        if r2.map_err(|e| format!("{e:?}"))? != "second" {
            return Err("pipelining: second response wrong".to_string());
        }
        if r3.map_err(|e| format!("{e:?}"))? != "third" {
            return Err("pipelining: third response wrong".to_string());
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify call.initiate]
// r[verify call.complete]
pub fn run_rpc_reverse_roundtrip(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child) = accept_subject_spec(spec).await?;
        let resp = client
            .reverse("hello".to_string())
            .await
            .map_err(|e| format!("reverse: {e:?}"))?;
        if resp != "olleh" {
            return Err(format!("expected \"olleh\", got {:?}", resp));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify call.error.user]
pub fn run_rpc_lookup_user_error(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child) = accept_subject_spec(spec).await?;
        let result = client.lookup(999).await;
        match result {
            Err(RoamError::User(err)) => {
                // Any lookup error is acceptable — key thing is it's a user error
                let _ = err;
            }
            Ok(resp) => {
                return Err(format!("expected Err(User(...)), got Ok({resp:?})"));
            }
            Err(other) => {
                return Err(format!("expected Err(User(...)), got Err({other:?})"));
            }
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify call.initiate]
// r[verify encoding.struct]
pub fn run_rpc_complex_struct_echo(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child) = accept_subject_spec(spec).await?;
        let point = Point { x: 3, y: 7 };
        let resp = client
            .echo_point(point.clone())
            .await
            .map_err(|e| format!("echo_point: {e:?}"))?;
        if resp != point {
            return Err(format!("expected {point:?}, got {resp:?}"));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.option]
pub fn run_rpc_optional_field(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child) = accept_subject_spec(spec).await?;
        // Test with Some email
        let p1 = client
            .create_person(
                "Alice".to_string(),
                30,
                Some("alice@example.com".to_string()),
            )
            .await
            .map_err(|e| format!("create_person with email: {e:?}"))?;
        if p1.name != "Alice" || p1.age != 30 || p1.email.as_deref() != Some("alice@example.com") {
            return Err(format!("create_person with email: got {p1:?}"));
        }
        // Test with None email
        let p2 = client
            .create_person("Bob".to_string(), 25, None)
            .await
            .map_err(|e| format!("create_person without email: {e:?}"))?;
        if p2.name != "Bob" || p2.age != 25 || p2.email.is_some() {
            return Err(format!("create_person without email: got {p2:?}"));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct.nested]
pub fn run_rpc_nested_struct(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child) = accept_subject_spec(spec).await?;
        let rect = Rectangle {
            top_left: Point { x: 0, y: 10 },
            bottom_right: Point { x: 5, y: 0 },
            label: Some("test".to_string()),
        };
        let area = client
            .rectangle_area(rect)
            .await
            .map_err(|e| format!("rectangle_area: {e:?}"))?;
        // area = |x2-x1| * |y2-y1| = 5 * 10 = 50
        if (area - 50.0_f64).abs() > 1e-9 {
            return Err(format!("expected area 50.0, got {area}"));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.option.return]
pub fn run_rpc_option_return(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child) = accept_subject_spec(spec).await?;
        // Known color
        let color = client
            .parse_color("red".to_string())
            .await
            .map_err(|e| format!("parse_color red: {e:?}"))?;
        if color != Some(Color::Red) {
            return Err(format!("expected Some(Red), got {color:?}"));
        }
        // Unknown color → None
        let none = client
            .parse_color("purple".to_string())
            .await
            .map_err(|e| format!("parse_color unknown: {e:?}"))?;
        if none.is_some() {
            return Err(format!("expected None for unknown color, got {none:?}"));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.enum.struct-variants]
pub fn run_rpc_enum_struct_variants(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child) = accept_subject_spec(spec).await?;
        let area_circle = client
            .shape_area(Shape::Circle { radius: 1.0 })
            .await
            .map_err(|e| format!("shape_area circle: {e:?}"))?;
        // π * r² ≈ 3.14159
        if (area_circle - std::f64::consts::PI).abs() > 1e-6 {
            return Err(format!("circle area: expected ~pi, got {area_circle}"));
        }
        let area_rect = client
            .shape_area(Shape::Rectangle {
                width: 3.0,
                height: 4.0,
            })
            .await
            .map_err(|e| format!("shape_area rect: {e:?}"))?;
        if (area_rect - 12.0_f64).abs() > 1e-9 {
            return Err(format!("rect area: expected 12.0, got {area_rect}"));
        }
        let area_point = client
            .shape_area(Shape::Point)
            .await
            .map_err(|e| format!("shape_area point: {e:?}"))?;
        if (area_point - 0.0_f64).abs() > 1e-9 {
            return Err(format!("point area: expected 0.0, got {area_point}"));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct.nested]
// r[verify encoding.vec]
// r[verify encoding.enum]
pub fn run_rpc_vec_of_structs(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child) = accept_subject_spec(spec).await?;
        let canvas = client
            .create_canvas(
                "test".to_string(),
                vec![
                    Shape::Point,
                    Shape::Circle { radius: 2.0 },
                    Shape::Rectangle {
                        width: 1.0,
                        height: 3.0,
                    },
                ],
                Color::Blue,
            )
            .await
            .map_err(|e| format!("create_canvas: {e:?}"))?;
        if canvas.name != "test" {
            return Err(format!(
                "canvas name: expected 'test', got {:?}",
                canvas.name
            ));
        }
        if canvas.background != Color::Blue {
            return Err(format!(
                "canvas background: expected Blue, got {:?}",
                canvas.background
            ));
        }
        if canvas.shapes.len() != 3 {
            return Err(format!(
                "canvas shapes: expected 3, got {}",
                canvas.shapes.len()
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.enum.newtype-variants]
pub fn run_rpc_enum_newtype_variants(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child) = accept_subject_spec(spec).await?;
        // Text variant — subject prefixes with "processed: "
        let text_out = client
            .process_message(Message::Text("hello".to_string()))
            .await
            .map_err(|e| format!("process_message text: {e:?}"))?;
        match text_out {
            Message::Text(_) => {}
            other => {
                return Err(format!(
                    "process_message text: expected Text, got {other:?}"
                ));
            }
        }
        // Number variant — subject doubles the number
        let num_out = client
            .process_message(Message::Number(21))
            .await
            .map_err(|e| format!("process_message number: {e:?}"))?;
        match num_out {
            Message::Number(n) if n == 42 => {}
            other => {
                return Err(format!(
                    "process_message number: expected Number(42), got {other:?}"
                ));
            }
        }
        // Data variant — subject reverses the bytes
        let data_out = client
            .process_message(Message::Data(vec![1, 2, 3, 4]))
            .await
            .map_err(|e| format!("process_message data: {e:?}"))?;
        match data_out {
            Message::Data(ref bytes) if *bytes == vec![4, 3, 2, 1] => {}
            other => {
                return Err(format!(
                    "process_message data: expected Data([4,3,2,1]), got {other:?}"
                ));
            }
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.vec]
pub fn run_rpc_vec_return(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child) = accept_subject_spec(spec).await?;
        let points = client
            .get_points(3)
            .await
            .map_err(|e| format!("get_points: {e:?}"))?;
        if points.len() != 3 {
            return Err(format!("expected 3 points, got {}", points.len()));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.tuple]
pub fn run_rpc_tuple_type(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child) = accept_subject_spec(spec).await?;
        let (s, n) = client
            .swap_pair((42, "hello".to_string()))
            .await
            .map_err(|e| format!("swap_pair: {e:?}"))?;
        if s != "hello" || n != 42 {
            return Err(format!(
                "swap_pair: expected (\"hello\", 42), got ({s:?}, {n})"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// ============================================================================
// Subject→Harness direction: TypeScript calls Rust's service
// ============================================================================

// r[verify call.initiate]
// r[verify call.complete]
pub fn run_subject_calls_echo(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "echo");
}

// r[verify encoding.enum.struct-variants]
pub fn run_subject_calls_shape_area(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "shape_area");
}

// r[verify encoding.struct.nested]
// r[verify encoding.vec]
pub fn run_subject_calls_create_canvas(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "create_canvas");
}

// r[verify encoding.enum.newtype-variants]
pub fn run_subject_calls_process_message(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "process_message");
}

// ============================================================================
// Additional harness→subject: error variants
// ============================================================================

// r[verify call.error.user]
pub fn run_rpc_divide_overflow(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child) = accept_subject_spec(spec).await?;
        // i64::MIN / -1 overflows
        let result = client.divide(i64::MIN, -1).await;
        match result {
            Err(RoamError::User(MathError::Overflow)) => {}
            Ok(v) => return Err(format!("divide_overflow: expected Overflow, got Ok({v})")),
            Err(other) => return Err(format!("divide_overflow: expected Overflow, got {other:?}")),
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify call.error.user]
pub fn run_rpc_lookup_found(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child) = accept_subject_spec(spec).await?;
        // id=1 → Alice with email
        let alice = client
            .lookup(1)
            .await
            .map_err(|e| format!("lookup 1: {e:?}"))?;
        if alice.name != "Alice" || alice.email.as_deref() != Some("alice@example.com") {
            return Err(format!("lookup 1: unexpected {alice:?}"));
        }
        // id=2 → Bob without email
        let bob = client
            .lookup(2)
            .await
            .map_err(|e| format!("lookup 2: {e:?}"))?;
        if bob.name != "Bob" || bob.email.is_some() {
            return Err(format!("lookup 2: unexpected {bob:?}"));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify call.error.user]
pub fn run_rpc_lookup_access_denied(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child) = accept_subject_spec(spec).await?;
        let result = client.lookup(100).await;
        match result {
            Err(RoamError::User(LookupError::AccessDenied)) => {}
            Ok(v) => return Err(format!("expected AccessDenied, got Ok({v:?})")),
            Err(other) => return Err(format!("expected AccessDenied, got {other:?}")),
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// ============================================================================
// Additional harness→subject: new primitive/type methods
// ============================================================================

// r[verify encoding.bytes]
pub fn run_rpc_echo_bytes(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child) = accept_subject_spec(spec).await?;
        let data = vec![0u8, 1, 127, 128, 255];
        let result = client
            .echo_bytes(data.clone())
            .await
            .map_err(|e| format!("echo_bytes: {e:?}"))?;
        if result != data {
            return Err(format!("echo_bytes: expected {data:?}, got {result:?}"));
        }
        // Empty bytes
        let empty = client
            .echo_bytes(vec![])
            .await
            .map_err(|e| format!("echo_bytes empty: {e:?}"))?;
        if !empty.is_empty() {
            return Err(format!("echo_bytes empty: expected [], got {empty:?}"));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.bool]
pub fn run_rpc_echo_bool(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child) = accept_subject_spec(spec).await?;
        for b in [true, false] {
            let result = client
                .echo_bool(b)
                .await
                .map_err(|e| format!("echo_bool({b}): {e:?}"))?;
            if result != b {
                return Err(format!("echo_bool({b}): expected {b}, got {result}"));
            }
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.u64]
pub fn run_rpc_echo_u64(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child) = accept_subject_spec(spec).await?;
        for n in [0u64, 1, u64::MAX, 1_000_000_000_000] {
            let result = client
                .echo_u64(n)
                .await
                .map_err(|e| format!("echo_u64({n}): {e:?}"))?;
            if result != n {
                return Err(format!("echo_u64({n}): expected {n}, got {result}"));
            }
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.option]
pub fn run_rpc_echo_option_string(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child) = accept_subject_spec(spec).await?;
        let some = client
            .echo_option_string(Some("hello".to_string()))
            .await
            .map_err(|e| format!("echo_option_string Some: {e:?}"))?;
        if some.as_deref() != Some("hello") {
            return Err(format!("echo_option_string Some: got {some:?}"));
        }
        let none = client
            .echo_option_string(None)
            .await
            .map_err(|e| format!("echo_option_string None: {e:?}"))?;
        if none.is_some() {
            return Err(format!("echo_option_string None: got {none:?}"));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct.multi-arg]
pub fn run_rpc_describe_point(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child) = accept_subject_spec(spec).await?;
        let tp = client
            .describe_point("test".to_string(), 5, -3, true)
            .await
            .map_err(|e| format!("describe_point: {e:?}"))?;
        if tp.label != "test" || tp.x != 5 || tp.y != -3 || !tp.active {
            return Err(format!("describe_point: unexpected {tp:?}"));
        }
        let tp2 = client
            .describe_point("far".to_string(), -100, 200, false)
            .await
            .map_err(|e| format!("describe_point 2: {e:?}"))?;
        if tp2.label != "far" || tp2.x != -100 || tp2.y != 200 || tp2.active {
            return Err(format!("describe_point 2: unexpected {tp2:?}"));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.enum.unit-variants]
pub fn run_rpc_all_colors(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child) = accept_subject_spec(spec).await?;
        let colors = client
            .all_colors()
            .await
            .map_err(|e| format!("all_colors: {e:?}"))?;
        if colors != vec![Color::Red, Color::Green, Color::Blue] {
            return Err(format!(
                "all_colors: expected [Red,Green,Blue], got {colors:?}"
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.enum.struct-variants]
pub fn run_rpc_echo_shape(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child) = accept_subject_spec(spec).await?;
        for shape in [
            Shape::Point,
            Shape::Circle { radius: 1.5 },
            Shape::Rectangle {
                width: 3.0,
                height: 4.0,
            },
        ] {
            let result = client
                .echo_shape(shape.clone())
                .await
                .map_err(|e| format!("echo_shape: {e:?}"))?;
            if result != shape {
                return Err(format!("echo_shape: expected {shape:?}, got {result:?}"));
            }
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.enum.unit-variants]
pub fn run_rpc_echo_status(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child) = accept_subject_spec(spec).await?;
        for status in [spec_proto::Status::Active, spec_proto::Status::Inactive] {
            let result = client
                .echo_status_v1(status.clone())
                .await
                .map_err(|e| format!("echo_status_v1: {e:?}"))?;
            if result != status {
                return Err(format!(
                    "echo_status_v1: expected {status:?}, got {result:?}"
                ));
            }
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify encoding.struct]
pub fn run_rpc_echo_tag(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child) = accept_subject_spec(spec).await?;
        let tag = Tag {
            label: "important".to_string(),
            priority: 42,
            note: "do not delete".to_string(),
        };
        let result = client
            .echo_tag_v1(tag.clone())
            .await
            .map_err(|e| format!("echo_tag_v1: {e:?}"))?;
        if result != tag {
            return Err(format!("echo_tag_v1: expected {tag:?}, got {result:?}"));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify call.pipelining.allowed]
pub fn run_rpc_pipelining_10_concurrent(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child) = accept_subject_spec(spec).await?;
        let mut handles = Vec::new();
        for i in 0..10usize {
            let client = client.clone();
            let msg = format!("concurrent-{i}");
            handles.push(tokio::spawn(async move {
                client
                    .echo(msg.clone())
                    .await
                    .map_err(|e| format!("pipelining[{i}]: {e:?}"))
                    .and_then(|r| {
                        if r == msg {
                            Ok(())
                        } else {
                            Err(format!("pipelining[{i}]: expected {msg}, got {r}"))
                        }
                    })
            }));
        }
        for h in handles {
            h.await.map_err(|e| format!("pipelining join: {e}"))??;
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify channeling.flow-control]
pub fn run_rpc_channeling_large_stream(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child) = accept_subject_spec(spec).await?;
        let n: u32 = 100; // well above default initial_credit of 16
        let (tx, mut rx) = roam::channel::<i32>();
        let recv = spec_tests::harness::spawn_loud(async move {
            let mut received = Vec::new();
            while let Ok(Some(v)) = rx.recv().await {
                received.push(*v);
            }
            received
        });
        client
            .generate_large(n, tx)
            .await
            .map_err(|e| format!("generate_large: {e:?}"))?;
        let received = recv.await.map_err(|e| format!("recv: {e}"))?;
        let expected: Vec<i32> = (0..n as i32).collect();
        if received != expected {
            return Err(format!(
                "generate_large: expected {n} sequential items, got {} items",
                received.len()
            ));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// r[verify channeling.flow-control]
pub fn run_rpc_channeling_sum_large(spec: SubjectSpec) {
    run_async(async {
        let (client, mut child) = accept_subject_spec(spec).await?;
        let n: i32 = 100;
        let (tx, rx) = roam::channel::<i32>();
        spec_tests::harness::spawn_loud(async move {
            for i in 0..n {
                tx.send(i).await.unwrap();
            }
            tx.close(Default::default()).await.unwrap();
        });
        let result = client
            .sum_large(rx)
            .await
            .map_err(|e| format!("sum_large: {e:?}"))?;
        let expected: i64 = (0..n as i64).sum();
        if result != expected {
            return Err(format!("sum_large: expected {expected}, got {result}"));
        }
        child.kill().await.ok();
        Ok::<_, String>(())
    })
    .unwrap();
}

// ============================================================================
// Additional subject→harness: full type + error coverage
// ============================================================================

// r[verify call.initiate]
pub fn run_subject_calls_reverse(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "reverse");
}

// r[verify call.error.user]
pub fn run_subject_calls_divide_success(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "divide_success");
}

// r[verify call.error.user]
pub fn run_subject_calls_divide_zero(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "divide_zero");
}

// r[verify call.error.user]
pub fn run_subject_calls_divide_overflow(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "divide_overflow");
}

// r[verify call.error.user]
pub fn run_subject_calls_lookup_found(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "lookup_found");
}

// r[verify call.error.user]
pub fn run_subject_calls_lookup_found_no_email(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "lookup_found_no_email");
}

// r[verify call.error.user]
pub fn run_subject_calls_lookup_not_found(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "lookup_not_found");
}

// r[verify call.error.user]
pub fn run_subject_calls_lookup_access_denied(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "lookup_access_denied");
}

// r[verify encoding.struct]
pub fn run_subject_calls_echo_point(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "echo_point");
}

// r[verify encoding.struct]
pub fn run_subject_calls_create_person(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "create_person");
}

// r[verify encoding.struct.nested]
pub fn run_subject_calls_rectangle_area(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "rectangle_area");
}

// r[verify encoding.option.return]
pub fn run_subject_calls_parse_color(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "parse_color");
}

// r[verify encoding.vec]
pub fn run_subject_calls_get_points(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "get_points");
}

// r[verify encoding.tuple]
pub fn run_subject_calls_swap_pair(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "swap_pair");
}

// r[verify encoding.bytes]
pub fn run_subject_calls_echo_bytes(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "echo_bytes");
}

// r[verify encoding.bool]
pub fn run_subject_calls_echo_bool(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "echo_bool");
}

// r[verify encoding.u64]
pub fn run_subject_calls_echo_u64(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "echo_u64");
}

// r[verify encoding.option]
pub fn run_subject_calls_echo_option_string(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "echo_option_string");
}

// r[verify encoding.struct.multi-arg]
pub fn run_subject_calls_describe_point(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "describe_point");
}

// r[verify encoding.enum.unit-variants]
pub fn run_subject_calls_all_colors(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "all_colors");
}

// r[verify encoding.enum.struct-variants]
pub fn run_subject_calls_echo_shape(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "echo_shape");
}

// r[verify call.pipelining.allowed]
pub fn run_subject_calls_pipelining(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "pipelining");
}

// r[verify channeling.flow-control]
pub fn run_subject_calls_sum_large(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "sum_large");
}

// r[verify channeling.flow-control]
pub fn run_subject_calls_generate_large(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "generate_large");
}

// r[verify channeling.caller-pov]
pub fn run_subject_calls_sum_client_to_server(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "sum_client_to_server");
}

// r[verify channeling.type]
// r[verify channeling.lifecycle.immediate-data]
pub fn run_subject_calls_transform_bidi(spec: SubjectSpec) {
    run_subject_client_scenario(spec, "transform_bidi");
}

// Cross-language test functions are generated inline by the xtask matrix
// generator directly from the scenario name list — no wrapper functions needed.
// See xtask/src/main.rs `cross_lang_scenarios` for the single source of truth.
