use roam::RoamError;
use spec_proto::{Color, MathError, Message, Point, Rectangle, Shape};
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
