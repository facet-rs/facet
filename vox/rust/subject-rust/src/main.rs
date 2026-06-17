//! Rust subject binary for the vox compliance suite.

use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::time::Duration;

use facet_value::{VObject, VString, Value};
use spec_proto::{
    BridgeResponsiveImageInfo, Color, DodecaTemplateCall, EcosystemBridgePayload, MathError,
    StyxLspPosition, TestbedClient, TestbedDispatcher,
};
use subject_rust::{
    TestbedService, sample_dibs_create_request, sample_dibs_create_response,
    sample_dibs_delete_request, sample_dibs_get_request, sample_dibs_list_request,
    sample_dibs_list_response, sample_dibs_logs, sample_dibs_migrate_request,
    sample_dibs_migrate_result, sample_dibs_migration_status, sample_dibs_migration_status_request,
    sample_dibs_row_one, sample_dibs_schema, sample_dibs_update_request,
    sample_dibs_update_response, sample_dodeca_asset_processing_fixture,
    sample_dodeca_code_execution_result, sample_dodeca_data_content, sample_dodeca_data_format,
    sample_dodeca_dead_link_target, sample_dodeca_devtools_event, sample_dodeca_edit_list,
    sample_dodeca_edit_load, sample_dodeca_edit_preview, sample_dodeca_edit_read,
    sample_dodeca_edit_save, sample_dodeca_edit_save_req, sample_dodeca_edit_upload,
    sample_dodeca_edit_upload_req, sample_dodeca_eval_result, sample_dodeca_execute_samples_input,
    sample_dodeca_html_process_input, sample_dodeca_html_process_result,
    sample_dodeca_image_processor_fixture, sample_dodeca_load_data_result,
    sample_dodeca_markdown_content, sample_dodeca_markdown_source_path,
    sample_dodeca_open_source_result, sample_dodeca_parse_result, sample_dodeca_scope_entries,
    sample_dodeca_search_indexer_fixture, sample_dodeca_small_cell_services_fixture,
    sample_helix_pulse_bundle, sample_helix_pulse_bundle_fields, sample_helix_pulses,
    sample_helix_stream_metrics, sample_helix_trace_service_surface, sample_helix_verify_evidence,
    sample_hotmeal_apply_patches_result, sample_hotmeal_live_reload_events, sample_hotmeal_route,
    sample_stax_flamegraph_update, sample_stax_flamegraph_updates,
    sample_stax_linux_broker_control_fixture, sample_stax_macos_batches, sample_stax_macos_config,
    sample_stax_macos_record_summary, sample_stax_view_params, sample_styx_lsp_code_action_params,
    sample_styx_lsp_code_actions, sample_styx_lsp_completion_params, sample_styx_lsp_completions,
    sample_styx_lsp_definition_params, sample_styx_lsp_diagnostic_params,
    sample_styx_lsp_diagnostics, sample_styx_lsp_get_document_params,
    sample_styx_lsp_get_schema_params, sample_styx_lsp_get_source_params,
    sample_styx_lsp_get_subtree_params, sample_styx_lsp_hover_params, sample_styx_lsp_hover_result,
    sample_styx_lsp_initialize_params, sample_styx_lsp_initialize_result,
    sample_styx_lsp_inlay_hint_params, sample_styx_lsp_inlay_hints, sample_styx_lsp_locations,
    sample_styx_lsp_offset_to_position_params, sample_styx_lsp_position_to_offset_params,
    sample_styx_lsp_schema_info, sample_styx_lsp_source, sample_styx_value,
    sample_tracey_api_config, sample_tracey_bad_config_pattern_request,
    sample_tracey_config_pattern_request, sample_tracey_file_request, sample_tracey_file_response,
    sample_tracey_forward_response, sample_tracey_health_response, sample_tracey_hover_info,
    sample_tracey_lsp_code_actions, sample_tracey_lsp_code_lens, sample_tracey_lsp_completions,
    sample_tracey_lsp_content, sample_tracey_lsp_document_request, sample_tracey_lsp_inlay_hints,
    sample_tracey_lsp_inlay_hints_request, sample_tracey_lsp_locations,
    sample_tracey_lsp_position_request, sample_tracey_lsp_references_request,
    sample_tracey_lsp_rename_request, sample_tracey_lsp_semantic_tokens, sample_tracey_lsp_symbols,
    sample_tracey_lsp_text_edits, sample_tracey_lsp_workspace_diagnostics,
    sample_tracey_prepare_rename_result, sample_tracey_query_request,
    sample_tracey_reload_response, sample_tracey_reverse_response, sample_tracey_rule_info,
    sample_tracey_search_results, sample_tracey_spec_content_response, sample_tracey_stale_request,
    sample_tracey_stale_response, sample_tracey_status_response, sample_tracey_uncovered_response,
    sample_tracey_unmapped_request, sample_tracey_unmapped_response,
    sample_tracey_untested_request, sample_tracey_untested_response, sample_tracey_update_error,
    sample_tracey_update_file_range_conflict_request, sample_tracey_update_file_range_request,
    sample_tracey_updates, sample_tracey_validate_request, sample_tracey_validation_result,
    tracey_rule_id,
};
use tracing::info;
use vox_core::initiator;
use vox_stream::{local_link_source, tcp_link_source};

const DEFAULT_SUBJECT_INACTIVITY_TIMEOUT_SECS: u64 = 60;
const SUBJECT_RUNTIME_STACK_BYTES: usize = 32 * 1024 * 1024;

fn main() -> Result<(), String> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let rt = tokio::runtime::Builder::new_multi_thread()
        .thread_stack_size(SUBJECT_RUNTIME_STACK_BYTES)
        .enable_all()
        .build()
        .map_err(|e| format!("failed to create tokio runtime: {e}"))?;

    let mode = std::env::var("SUBJECT_MODE").unwrap_or_else(|_| "server".to_string());
    let task = match mode.as_str() {
        "server" => Ok(rt.spawn(run_with_subject_timeout("server", connect_and_serve()))),
        "client" => Ok(rt.spawn(run_with_subject_timeout("client", run_client()))),
        "server-listen" => Ok(rt.spawn(run_with_subject_timeout(
            "server-listen",
            listen_and_serve(),
        ))),
        other => Err(format!("unknown SUBJECT_MODE: {other}")),
    }?;
    rt.block_on(task)
        .map_err(|e| format!("subject {mode} task failed: {e}"))?
}

// r[impl hosted.subject.lifecycle]
fn subject_inactivity_timeout() -> Option<Duration> {
    let secs = std::env::var("SUBJECT_INACTIVITY_TIMEOUT_SECS")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(DEFAULT_SUBJECT_INACTIVITY_TIMEOUT_SECS);
    if secs == 0 {
        None
    } else {
        Some(Duration::from_secs(secs))
    }
}

// r[impl hosted.subject.lifecycle]
async fn run_with_subject_timeout<F>(mode: &str, future: F) -> Result<(), String>
where
    F: Future<Output = Result<(), String>>,
{
    let Some(timeout) = subject_inactivity_timeout() else {
        return future.await;
    };

    tokio::select! {
        result = future => result,
        _ = tokio::time::sleep(timeout) => {
            Err(format!("subject {mode} timed out after {timeout:?} without exiting"))
        }
    }
}

/// Bind a TCP listener, announce the address to stdout (for the harness to read),
/// accept one connection, and serve the Testbed service on it.
///
/// Used by cross-language harness tests where another subject acts as the client.
async fn listen_and_serve() -> Result<(), String> {
    use tokio::net::TcpListener;
    use vox_core::acceptor_on;

    let listen_port: u16 = std::env::var("LISTEN_PORT")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(0);

    let listener = TcpListener::bind(("127.0.0.1", listen_port))
        .await
        .map_err(|e| format!("bind: {e}"))?;
    let addr = listener
        .local_addr()
        .map_err(|e| format!("local_addr: {e}"))?;

    // Signal readiness — the harness reads this line from stdout.
    println!("LISTEN_ADDR=127.0.0.1:{}", addr.port());
    info!("server-listen mode: bound to {addr}");

    let (stream, _) = listener
        .accept()
        .await
        .map_err(|e| format!("accept: {e}"))?;
    stream.set_nodelay(true).ok();

    let connection = acceptor_on(vox_stream::StreamLink::tcp(stream))
        .on_lane(TestbedDispatcher::new(TestbedService))
        .establish_connection()
        .await
        .map_err(|e| format!("handshake: {e}"))?;

    // r[impl hosted.subject.lifecycle]
    connection.closed().await;
    connection.shutdown().ok();
    Ok(())
}

async fn connect_and_serve() -> Result<(), String> {
    let addr = std::env::var("PEER_ADDR").map_err(|_| "PEER_ADDR env var not set".to_string())?;
    info!("connecting to {addr}");
    let dispatcher = TestbedDispatcher::new(TestbedService);
    let (scheme, host) = match addr.split_once("://") {
        Some((scheme, host)) => (scheme, host.to_string()),
        None => ("tcp", addr.clone()),
    };

    let connection = match scheme {
        "tcp" => initiator(tcp_link_source(host))
            .on_lane(dispatcher.clone())
            .establish_connection()
            .await
            .map_err(|e| format!("handshake failed: {e}"))?,
        "local" => initiator(local_link_source(host))
            .on_lane(dispatcher.clone())
            .establish_connection()
            .await
            .map_err(|e| format!("handshake failed: {e}"))?,
        _ => return Err(format!("unsupported PEER_ADDR scheme: {scheme}")),
    };

    // r[impl hosted.subject.lifecycle]
    connection.closed().await;
    connection.shutdown().ok();
    Ok(())
}

async fn run_client() -> Result<(), String> {
    let addr = std::env::var("PEER_ADDR").map_err(|_| "PEER_ADDR env var not set".to_string())?;
    let scenario = std::env::var("CLIENT_SCENARIO").unwrap_or_else(|_| "echo".to_string());
    info!("client mode: connecting to {addr}, scenario={scenario}");

    let client = initiator(tcp_link_source(addr))
        .on_lane(TestbedDispatcher::new(TestbedService))
        .establish::<TestbedClient>()
        .await
        .map_err(|e| format!("handshake failed: {e}"))?;

    match scenario.as_str() {
        "echo" => {
            let result = client
                .echo("hello from client".to_string())
                .await
                .map_err(|e| format!("echo failed: {e:?}"))?;
            info!("echo result: {result}");
        }
        "reverse" => {
            let result = client
                .reverse("hello".to_string())
                .await
                .map_err(|e| format!("reverse failed: {e:?}"))?;
            if result != "olleh" {
                return Err(format!("reverse: expected 'olleh', got {result:?}"));
            }
            info!("reverse result: {result}");
        }
        "divide_success" => {
            let result = client
                .divide(10, 3)
                .await
                .map_err(|e| format!("divide_success failed: {e:?}"))?;
            if result != 3 {
                return Err(format!("divide_success: expected 3, got {result}"));
            }
            info!("divide_success result: {result}");
        }
        "divide_zero" => {
            match client.divide(10, 0).await {
                Err(vox::VoxError::User(error)) if *error == MathError::DivisionByZero => {}
                other => {
                    return Err(format!(
                        "divide_zero: expected DivisionByZero, got {other:?}"
                    ));
                }
            }
            info!("divide_zero: got expected DivisionByZero error");
        }
        "divide_overflow" => {
            match client.divide(i64::MIN, -1).await {
                Err(vox::VoxError::User(error)) if *error == MathError::Overflow => {}
                other => return Err(format!("divide_overflow: expected Overflow, got {other:?}")),
            }
            info!("divide_overflow: got expected Overflow error");
        }
        "lookup_found" => {
            let p = client
                .lookup(1)
                .await
                .map_err(|e| format!("lookup_found failed: {e:?}"))?;
            if p.name != "Alice" {
                return Err(format!("lookup_found: expected Alice, got {p:?}"));
            }
            info!("lookup_found: {p:?}");
        }
        "lookup_found_no_email" => {
            let p = client
                .lookup(2)
                .await
                .map_err(|e| format!("lookup_found_no_email failed: {e:?}"))?;
            if p.name != "Bob" || p.email.is_some() {
                return Err(format!(
                    "lookup_found_no_email: expected Bob with no email, got {p:?}"
                ));
            }
            info!("lookup_found_no_email: {p:?}");
        }
        "lookup_not_found" => {
            match client.lookup(999).await {
                Err(vox::VoxError::User(error)) if *error == spec_proto::LookupError::NotFound => {}
                other => {
                    return Err(format!(
                        "lookup_not_found: expected NotFound, got {other:?}"
                    ));
                }
            }
            info!("lookup_not_found: got expected NotFound error");
        }
        "lookup_access_denied" => {
            match client.lookup(100).await {
                Err(vox::VoxError::User(error))
                    if *error == spec_proto::LookupError::AccessDenied => {}
                other => {
                    return Err(format!(
                        "lookup_access_denied: expected AccessDenied, got {other:?}"
                    ));
                }
            }
            info!("lookup_access_denied: got expected AccessDenied error");
        }
        "echo_point" => {
            let pt = spec_proto::Point { x: 42, y: -7 };
            let result = client
                .echo_point(pt.clone())
                .await
                .map_err(|e| format!("echo_point failed: {e:?}"))?;
            if result != pt {
                return Err(format!("echo_point: expected {pt:?}, got {result:?}"));
            }
            info!("echo_point OK");
        }
        "create_person" => {
            let p = client
                .create_person("Dave".to_string(), 40, Some("dave@example.com".to_string()))
                .await
                .map_err(|e| format!("create_person failed: {e:?}"))?;
            if p.name != "Dave" || p.age != 40 || p.email.as_deref() != Some("dave@example.com") {
                return Err(format!("create_person: unexpected {p:?}"));
            }
            info!("create_person OK: {p:?}");
        }
        "rectangle_area" => {
            use spec_proto::{Point, Rectangle};
            let area = client
                .rectangle_area(Rectangle {
                    top_left: Point { x: 0, y: 10 },
                    bottom_right: Point { x: 5, y: 0 },
                    label: None,
                })
                .await
                .map_err(|e| format!("rectangle_area failed: {e:?}"))?;
            if (area - 50.0_f64).abs() > 1e-9 {
                return Err(format!("rectangle_area: expected 50.0, got {area}"));
            }
            info!("rectangle_area: {area}");
        }
        "parse_color" => {
            for (name, expected) in [
                ("red", Color::Red),
                ("green", Color::Green),
                ("blue", Color::Blue),
            ] {
                match client.parse_color(name.to_string()).await {
                    Ok(Some(c)) if c == expected => {}
                    other => return Err(format!("parse_color {name}: unexpected {other:?}")),
                }
            }
            match client.parse_color("purple".to_string()).await {
                Ok(None) => {}
                other => return Err(format!("parse_color purple: expected None, got {other:?}")),
            }
            info!("parse_color: all variants OK");
        }
        "get_points" => {
            let pts = client
                .get_points(5)
                .await
                .map_err(|e| format!("get_points failed: {e:?}"))?;
            if pts.len() != 5 {
                return Err(format!("get_points: expected 5, got {}", pts.len()));
            }
            info!("get_points: {} points", pts.len());
        }
        "swap_pair" => {
            let (s, n) = client
                .swap_pair((99, "hello".to_string()))
                .await
                .map_err(|e| format!("swap_pair failed: {e:?}"))?;
            if s != "hello" || n != 99 {
                return Err(format!(
                    "swap_pair: expected ('hello', 99), got ({s:?}, {n})"
                ));
            }
            info!("swap_pair OK");
        }
        "echo_bytes" => {
            let data = vec![1u8, 2, 3, 255, 0, 128];
            let result = client
                .echo_bytes(data.clone())
                .await
                .map_err(|e| format!("echo_bytes failed: {e:?}"))?;
            if result != data {
                return Err(format!("echo_bytes: expected {data:?}, got {result:?}"));
            }
            info!("echo_bytes OK");
        }
        "echo_bool" => {
            for b in [true, false] {
                let result = client
                    .echo_bool(b)
                    .await
                    .map_err(|e| format!("echo_bool({b}) failed: {e:?}"))?;
                if result != b {
                    return Err(format!("echo_bool: expected {b}, got {result}"));
                }
            }
            info!("echo_bool OK");
        }
        "echo_u64" => {
            for n in [0u64, 1, u64::MAX, 1_000_000_000_000] {
                let result = client
                    .echo_u64(n)
                    .await
                    .map_err(|e| format!("echo_u64({n}) failed: {e:?}"))?;
                if result != n {
                    return Err(format!("echo_u64: expected {n}, got {result}"));
                }
            }
            info!("echo_u64 OK");
        }
        "echo_option_string" => {
            match client.echo_option_string(Some("hello".to_string())).await {
                Ok(Some(s)) if s == "hello" => {}
                other => return Err(format!("echo_option_string Some: got {other:?}")),
            }
            match client.echo_option_string(None).await {
                Ok(None) => {}
                other => return Err(format!("echo_option_string None: got {other:?}")),
            }
            info!("echo_option_string OK");
        }
        "describe_point" => {
            let result = client
                .describe_point("origin".to_string(), 0, 0, true)
                .await
                .map_err(|e| format!("describe_point failed: {e:?}"))?;
            if result.label != "origin" || result.x != 0 || result.y != 0 || !result.active {
                return Err(format!("describe_point: unexpected {result:?}"));
            }
            info!("describe_point OK: {result:?}");
        }
        "all_colors" => {
            let colors = client
                .all_colors()
                .await
                .map_err(|e| format!("all_colors failed: {e:?}"))?;
            if colors.len() != 3 {
                return Err(format!("all_colors: expected 3, got {}", colors.len()));
            }
            if colors[0] != Color::Red || colors[1] != Color::Green || colors[2] != Color::Blue {
                return Err(format!("all_colors: unexpected order {colors:?}"));
            }
            info!("all_colors OK");
        }
        "echo_shape" => {
            for shape in [
                spec_proto::Shape::Point,
                #[allow(clippy::approx_constant)]
                spec_proto::Shape::Circle { radius: 3.14 },
                spec_proto::Shape::Rectangle {
                    width: 2.0,
                    height: 5.0,
                },
            ] {
                let result = client
                    .echo_shape(shape.clone())
                    .await
                    .map_err(|e| format!("echo_shape failed: {e:?}"))?;
                if result != shape {
                    return Err(format!("echo_shape: expected {shape:?}, got {result:?}"));
                }
            }
            info!("echo_shape OK (all 3 variants)");
        }
        "pipelining" => {
            let mut handles = Vec::new();
            for i in 0..10usize {
                let client = client.clone();
                let msg = format!("msg{i}");
                handles.push(tokio::spawn(async move {
                    client
                        .echo(msg.clone())
                        .await
                        .map_err(|e| format!("pipelining echo {i}: {e:?}"))
                        .and_then(|r| {
                            if r == msg {
                                Ok(r)
                            } else {
                                Err(format!("pipelining: expected {msg}, got {r}"))
                            }
                        })
                }));
            }
            for h in handles {
                h.await.map_err(|e| format!("pipelining join: {e}"))??;
            }
            info!("pipelining OK (10 concurrent echo calls)");
        }
        "sum_large" => {
            let (tx, rx) = vox::channel::<i32>();
            let n: i32 = 100;
            let send_task = tokio::spawn(async move {
                for i in 0..n {
                    tx.send(i).await.ok();
                }
                tx.close(Default::default()).await.ok();
            });
            let result = client
                .sum_large(rx)
                .await
                .map_err(|e| format!("sum_large failed: {e:?}"))?;
            send_task.await.ok();
            let expected: i64 = (0..n as i64).sum();
            if result != expected {
                return Err(format!("sum_large: expected {expected}, got {result}"));
            }
            info!("sum_large OK: {result}");
        }
        "generate_large" => {
            let (tx, mut rx) = vox::channel::<i32>();
            let n: u32 = 100;
            let recv_task = tokio::spawn(async move {
                let mut received = Vec::new();
                while let Ok(Some(v)) = rx.recv().await {
                    let v = v.get();
                    received.push(*v);
                }
                received
            });
            client
                .generate_large(n, tx)
                .await
                .map_err(|e| format!("generate_large failed: {e:?}"))?;
            let received = recv_task.await.map_err(|e| format!("recv task: {e}"))?;
            if received.len() != n as usize {
                return Err(format!(
                    "generate_large: expected {n} items, got {}",
                    received.len()
                ));
            }
            let expected: Vec<i32> = (0..n as i32).collect();
            if received != expected {
                return Err(format!(
                    "generate_large: expected sequential, got {received:?}"
                ));
            }
            info!("generate_large OK: {} items", received.len());
        }
        "sum_client_to_server" => {
            let (tx, rx) = vox::channel::<i32>();
            let send_task = tokio::spawn(async move {
                for n in [1i32, 2, 3, 4, 5] {
                    tx.send(n).await.unwrap();
                }
                tx.close(Default::default()).await.unwrap();
            });
            let result = client
                .sum(rx)
                .await
                .map_err(|e| format!("sum_client_to_server failed: {e:?}"))?;
            send_task.await.ok();
            if result != 15 {
                return Err(format!("sum_client_to_server: expected 15, got {result}"));
            }
            info!("sum_client_to_server OK: {result}");
        }
        "transform_bidi" => {
            let (input_tx, input_rx) = vox::channel::<String>();
            let (output_tx, mut output_rx) = vox::channel::<String>();
            let messages = vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()];
            let msgs_clone = messages.clone();
            let send_task = tokio::spawn(async move {
                for msg in msgs_clone {
                    input_tx.send(msg).await.unwrap();
                }
                input_tx.close(Default::default()).await.unwrap();
            });
            let recv_task = tokio::spawn(async move {
                let mut received = Vec::new();
                while let Ok(Some(s)) = output_rx.recv().await {
                    let s = s.get();
                    received.push(s.clone());
                }
                received
            });
            client
                .transform(input_rx, output_tx)
                .await
                .map_err(|e| format!("transform_bidi failed: {e:?}"))?;
            send_task.await.ok();
            let received = recv_task.await.map_err(|e| format!("recv: {e}"))?;
            if received != messages {
                return Err(format!(
                    "transform_bidi: expected {messages:?}, got {received:?}"
                ));
            }
            info!("transform_bidi OK");
        }
        "dodeca_byte_tunnel" => {
            let (inbound_tx, inbound_rx) = vox::channel::<Vec<u8>>();
            let (outbound_tx, mut outbound_rx) = vox::channel::<Vec<u8>>();
            let chunks = vec![vec![0, 1, 2, 3], vec![], vec![255, 254, 253]];
            let expected = chunks.clone();
            let send_task = tokio::spawn(async move {
                for chunk in chunks {
                    inbound_tx.send(chunk).await.unwrap();
                }
                inbound_tx.close(Default::default()).await.unwrap();
            });
            let recv_task = tokio::spawn(async move {
                let mut received = Vec::new();
                while let Ok(Some(chunk)) = outbound_rx.recv().await {
                    received.push(chunk.get().clone());
                }
                received
            });
            client
                .dodeca_byte_tunnel(inbound_rx, outbound_tx)
                .await
                .map_err(|e| format!("dodeca_byte_tunnel failed: {e:?}"))?;
            send_task.await.ok();
            let received = recv_task.await.map_err(|e| format!("recv: {e}"))?;
            if received != expected {
                return Err(format!(
                    "dodeca_byte_tunnel: expected {expected:?}, got {received:?}"
                ));
            }
            info!("dodeca_byte_tunnel OK");
        }
        "dodeca_devtools_lsp" => {
            let (client_tx, client_rx) = vox::channel::<String>();
            let (server_tx, mut server_rx) = vox::channel::<String>();
            let chunks = vec![
                "Content-Length: 37\r\n\r\n{\"jsonrpc\":\"2.0\",\"id\":1}".to_string(),
                "{\"method\":\"textDocument/didOpen\"}".to_string(),
            ];
            let expected = chunks
                .iter()
                .map(|chunk| format!("lsp:{chunk}"))
                .collect::<Vec<_>>();
            let send_task = tokio::spawn(async move {
                for chunk in chunks {
                    client_tx.send(chunk).await.unwrap();
                }
                client_tx.close(Default::default()).await.unwrap();
            });
            let recv_task = tokio::spawn(async move {
                let mut received = Vec::new();
                while let Ok(Some(chunk)) = server_rx.recv().await {
                    received.push(chunk.get().clone());
                }
                received
            });
            client
                .dodeca_devtools_lsp("editor-token".to_string(), client_rx, server_tx)
                .await
                .map_err(|e| format!("dodeca_devtools_lsp failed: {e:?}"))?;
            send_task.await.ok();
            let received = recv_task.await.map_err(|e| format!("recv: {e}"))?;
            if received != expected {
                return Err(format!(
                    "dodeca_devtools_lsp: expected {expected:?}, got {received:?}"
                ));
            }
            info!("dodeca_devtools_lsp OK");
        }
        "dibs_list" => {
            let expected = sample_dibs_list_response();
            let result = client
                .dibs_list(sample_dibs_list_request())
                .await
                .map_err(|e| format!("dibs_list failed: {e:?}"))?;
            if result != expected {
                return Err(format!("dibs_list: expected {expected:?}, got {result:?}"));
            }
            info!("dibs_list OK");
        }
        "dibs_schema" => {
            let expected = sample_dibs_schema();
            let result = client
                .dibs_schema()
                .await
                .map_err(|e| format!("dibs_schema failed: {e:?}"))?;
            if result != expected {
                return Err(format!(
                    "dibs_schema: expected {expected:?}, got {result:?}"
                ));
            }
            info!("dibs_schema OK");
        }
        "dibs_get" => {
            let expected = Some(sample_dibs_row_one());
            let result = client
                .dibs_get(sample_dibs_get_request())
                .await
                .map_err(|e| format!("dibs_get failed: {e:?}"))?;
            if result != expected {
                return Err(format!("dibs_get: expected {expected:?}, got {result:?}"));
            }
            info!("dibs_get OK");
        }
        "dibs_create" => {
            let expected = sample_dibs_create_response();
            let result = client
                .dibs_create(sample_dibs_create_request())
                .await
                .map_err(|e| format!("dibs_create failed: {e:?}"))?;
            if result != expected {
                return Err(format!(
                    "dibs_create: expected {expected:?}, got {result:?}"
                ));
            }
            info!("dibs_create OK");
        }
        "dibs_update" => {
            let expected = sample_dibs_update_response();
            let result = client
                .dibs_update(sample_dibs_update_request())
                .await
                .map_err(|e| format!("dibs_update failed: {e:?}"))?;
            if result != expected {
                return Err(format!(
                    "dibs_update: expected {expected:?}, got {result:?}"
                ));
            }
            info!("dibs_update OK");
        }
        "dibs_delete" => {
            let result = client
                .dibs_delete(sample_dibs_delete_request())
                .await
                .map_err(|e| format!("dibs_delete failed: {e:?}"))?;
            if result != 1 {
                return Err(format!("dibs_delete: expected 1, got {result}"));
            }
            info!("dibs_delete OK");
        }
        "dibs_migration_status" => {
            let expected = sample_dibs_migration_status();
            let result = client
                .dibs_migration_status(sample_dibs_migration_status_request())
                .await
                .map_err(|e| format!("dibs_migration_status failed: {e:?}"))?;
            if result != expected {
                return Err(format!(
                    "dibs_migration_status: expected {expected:?}, got {result:?}"
                ));
            }
            info!("dibs_migration_status OK");
        }
        "dibs_migrate" => {
            let (log_tx, mut log_rx) = vox::channel::<spec_proto::DibsMigrationLog>();
            let expected_logs = sample_dibs_logs();
            let recv_task = tokio::spawn(async move {
                let mut logs = Vec::new();
                while let Ok(Some(log)) = log_rx.recv().await {
                    logs.push(log.get().clone());
                }
                logs
            });
            let result = client
                .dibs_migrate(sample_dibs_migrate_request(), log_tx)
                .await
                .map_err(|e| format!("dibs_migrate failed: {e:?}"))?;
            let logs = recv_task.await.map_err(|e| format!("logs recv: {e}"))?;
            let expected_result = sample_dibs_migrate_result();
            if result != expected_result {
                return Err(format!(
                    "dibs_migrate: expected {expected_result:?}, got {result:?}"
                ));
            }
            if logs != expected_logs {
                return Err(format!(
                    "dibs_migrate logs: expected {expected_logs:?}, got {logs:?}"
                ));
            }
            info!("dibs_migrate OK");
        }
        "post_reply_generate" => {
            let (tx, mut rx) = vox::channel::<i32>();

            client
                .post_reply_generate(tx)
                .await
                .map_err(|e| format!("post_reply_generate failed: {e:?}"))?;

            let mut received = Vec::new();
            while let Ok(Some(n)) = rx.recv().await {
                let n = n.get();
                received.push(*n);
            }

            let expected: Vec<i32> = (0..5).collect();
            if received != expected {
                return Err(format!(
                    "post_reply_generate: expected {expected:?}, got {received:?}"
                ));
            }
            info!("post_reply_generate OK");
        }
        "post_reply_sum" => {
            let (input_tx, input_rx) = vox::channel::<i32>();
            let (result_tx, mut result_rx) = vox::channel::<i64>();

            tokio::spawn(async move {
                for n in [1i32, 2, 3, 4, 5] {
                    input_tx.send(n).await.unwrap();
                }
                input_tx.close(Default::default()).await.unwrap();
            });

            client
                .post_reply_sum(input_rx, result_tx)
                .await
                .map_err(|e| format!("post_reply_sum failed: {e:?}"))?;

            let total = match result_rx.recv().await {
                Ok(Some(total)) => *total.get(),
                Ok(None) => {
                    return Err("post_reply_sum: result channel closed without a value".to_string());
                }
                Err(e) => return Err(format!("post_reply_sum result recv failed: {e}")),
            };

            if total != 15 {
                return Err(format!("post_reply_sum: expected 15, got {total}"));
            }

            match result_rx.recv().await {
                Ok(None) => {}
                Ok(Some(extra)) => {
                    return Err(format!(
                        "post_reply_sum: expected result channel close, got extra value {}",
                        *extra.get()
                    ));
                }
                Err(e) => return Err(format!("post_reply_sum result close recv failed: {e}")),
            }

            info!("post_reply_sum OK");
        }
        "shape_area" => {
            use spec_proto::Shape;
            let area = client
                .shape_area(Shape::Rectangle {
                    width: 3.0,
                    height: 4.0,
                })
                .await
                .map_err(|e| format!("shape_area failed: {e:?}"))?;
            if (area - 12.0_f64).abs() > 1e-9 {
                return Err(format!("shape_area: expected 12.0, got {area}"));
            }
            info!("shape_area result: {area}");
        }
        "create_canvas" => {
            use spec_proto::{Color, Shape};
            let canvas = client
                .create_canvas(
                    "enum-canvas".to_string(),
                    vec![Shape::Point, Shape::Circle { radius: 2.5 }],
                    Color::Green,
                )
                .await
                .map_err(|e| format!("create_canvas failed: {e:?}"))?;
            if canvas.name != "enum-canvas" {
                return Err(format!(
                    "create_canvas: expected name 'enum-canvas', got {:?}",
                    canvas.name
                ));
            }
            if canvas.background != Color::Green {
                return Err(format!(
                    "create_canvas: expected Green background, got {:?}",
                    canvas.background
                ));
            }
            if canvas.shapes.len() != 2 {
                return Err(format!(
                    "create_canvas: expected 2 shapes, got {}",
                    canvas.shapes.len()
                ));
            }
            info!("create_canvas result OK");
        }
        "process_message" => {
            use spec_proto::Message;
            let result = client
                .process_message(Message::Data(vec![1, 2, 3, 4]))
                .await
                .map_err(|e| format!("process_message failed: {e:?}"))?;
            match &result {
                Message::Data(bytes) if bytes == &vec![4, 3, 2, 1] => {}
                other => {
                    return Err(format!("process_message: unexpected result {other:?}"));
                }
            }
            info!("process_message result OK");
        }
        "echo_tree" => {
            let tree = spec_proto::Tree {
                value: 1,
                children: vec![
                    spec_proto::Tree {
                        value: 2,
                        children: vec![],
                    },
                    spec_proto::Tree {
                        value: 3,
                        children: vec![spec_proto::Tree {
                            value: 4,
                            children: vec![],
                        }],
                    },
                ],
            };
            let result = client
                .echo_tree(tree.clone())
                .await
                .map_err(|e| format!("echo_tree failed: {e:?}"))?;
            if result != tree {
                return Err(format!("echo_tree: expected {tree:?}, got {result:?}"));
            }
            info!("echo_tree OK");
        }
        "echo_ecosystem_bridge" => {
            let payload = sample_ecosystem_bridge_payload();
            let result = client
                .echo_ecosystem_bridge(payload.clone())
                .await
                .map_err(|e| format!("echo_ecosystem_bridge failed: {e:?}"))?;
            if result != payload {
                return Err(format!(
                    "echo_ecosystem_bridge: expected {payload:?}, got {result:?}"
                ));
            }
            info!("echo_ecosystem_bridge OK");
        }
        "echo_dodeca_template_call" => {
            let payload = sample_dodeca_template_call();
            let result = client
                .echo_dodeca_template_call(payload.clone())
                .await
                .map_err(|e| format!("echo_dodeca_template_call failed: {e:?}"))?;
            if result != payload {
                return Err(format!(
                    "echo_dodeca_template_call: expected {payload:?}, got {result:?}"
                ));
            }
            info!("echo_dodeca_template_call OK");
        }
        "dodeca_html_process" => {
            let input = sample_dodeca_html_process_input();
            let expected = sample_dodeca_html_process_result();
            let result = client
                .dodeca_html_process(input)
                .await
                .map_err(|e| format!("dodeca_html_process failed: {e:?}"))?;
            if result != expected {
                return Err(format!(
                    "dodeca_html_process: expected {expected:?}, got {result:?}"
                ));
            }
            info!("dodeca_html_process OK");
        }
        "dodeca_execute_code_samples" => {
            let input = sample_dodeca_execute_samples_input();
            let expected = sample_dodeca_code_execution_result();
            let result = client
                .dodeca_execute_code_samples(input)
                .await
                .map_err(|e| format!("dodeca_execute_code_samples failed: {e:?}"))?;
            if result != expected {
                return Err(format!(
                    "dodeca_execute_code_samples: expected {expected:?}, got {result:?}"
                ));
            }
            info!("dodeca_execute_code_samples OK");
        }
        "dodeca_load_data" => {
            let expected = sample_dodeca_load_data_result();
            let result = client
                .dodeca_load_data(sample_dodeca_data_content(), sample_dodeca_data_format())
                .await
                .map_err(|e| format!("dodeca_load_data failed: {e:?}"))?;
            if result != expected {
                return Err(format!(
                    "dodeca_load_data: expected {expected:?}, got {result:?}"
                ));
            }
            info!("dodeca_load_data OK");
        }
        "dodeca_parse_and_render" => {
            let expected = sample_dodeca_parse_result();
            let result = client
                .dodeca_parse_and_render(
                    sample_dodeca_markdown_source_path(),
                    sample_dodeca_markdown_content(),
                    true,
                )
                .await
                .map_err(|e| format!("dodeca_parse_and_render failed: {e:?}"))?;
            if result != expected {
                return Err(format!(
                    "dodeca_parse_and_render: expected {expected:?}, got {result:?}"
                ));
            }
            info!("dodeca_parse_and_render OK");
        }
        "echo_dodeca_image_processor_fixture" => {
            let payload = sample_dodeca_image_processor_fixture();
            let result = client
                .echo_dodeca_image_processor_fixture(payload.clone())
                .await
                .map_err(|e| format!("echo_dodeca_image_processor_fixture failed: {e:?}"))?;
            if result != payload {
                return Err(format!(
                    "echo_dodeca_image_processor_fixture: expected {payload:?}, got {result:?}"
                ));
            }
            info!("echo_dodeca_image_processor_fixture OK");
        }
        "echo_dodeca_search_indexer_fixture" => {
            let payload = sample_dodeca_search_indexer_fixture();
            let result = client
                .echo_dodeca_search_indexer_fixture(payload.clone())
                .await
                .map_err(|e| format!("echo_dodeca_search_indexer_fixture failed: {e:?}"))?;
            if result != payload {
                return Err(format!(
                    "echo_dodeca_search_indexer_fixture: expected {payload:?}, got {result:?}"
                ));
            }
            info!("echo_dodeca_search_indexer_fixture OK");
        }
        "echo_dodeca_asset_processing_fixture" => {
            let payload = sample_dodeca_asset_processing_fixture();
            let result = client
                .echo_dodeca_asset_processing_fixture(payload.clone())
                .await
                .map_err(|e| format!("echo_dodeca_asset_processing_fixture failed: {e:?}"))?;
            if result != payload {
                return Err(format!(
                    "echo_dodeca_asset_processing_fixture: expected {payload:?}, got {result:?}"
                ));
            }
            info!("echo_dodeca_asset_processing_fixture OK");
        }
        "echo_dodeca_devtools_event" => {
            let payload = sample_dodeca_devtools_event();
            let result = client
                .echo_dodeca_devtools_event(payload.clone())
                .await
                .map_err(|e| format!("echo_dodeca_devtools_event failed: {e:?}"))?;
            if result != payload {
                return Err(format!(
                    "echo_dodeca_devtools_event: expected {payload:?}, got {result:?}"
                ));
            }
            info!("echo_dodeca_devtools_event OK");
        }
        "echo_dodeca_small_cell_services_fixture" => {
            let payload = sample_dodeca_small_cell_services_fixture();
            let result = client
                .echo_dodeca_small_cell_services_fixture(payload.clone())
                .await
                .map_err(|e| format!("echo_dodeca_small_cell_services_fixture failed: {e:?}"))?;
            if result != payload {
                return Err(format!(
                    "echo_dodeca_small_cell_services_fixture: expected {payload:?}, got {result:?}"
                ));
            }
            info!("echo_dodeca_small_cell_services_fixture OK");
        }
        "dodeca_devtools_get_scope" => {
            let expected = sample_dodeca_scope_entries();
            let result = client
                .dodeca_devtools_get_scope(Some(vec!["page".to_string()]))
                .await
                .map_err(|e| format!("dodeca_devtools_get_scope failed: {e:?}"))?;
            if result != expected {
                return Err(format!(
                    "dodeca_devtools_get_scope: expected {expected:?}, got {result:?}"
                ));
            }
            info!("dodeca_devtools_get_scope OK");
        }
        "dodeca_devtools_eval" => {
            let expected = sample_dodeca_eval_result();
            let result = client
                .dodeca_devtools_eval("snap-devtools-42".to_string(), "page.title".to_string())
                .await
                .map_err(|e| format!("dodeca_devtools_eval failed: {e:?}"))?;
            if result != expected {
                return Err(format!(
                    "dodeca_devtools_eval: expected {expected:?}, got {result:?}"
                ));
            }
            info!("dodeca_devtools_eval OK");
        }
        "dodeca_devtools_open_dead_link" => {
            let expected = sample_dodeca_open_source_result();
            let result = client
                .dodeca_devtools_open_dead_link(
                    "/guide/".to_string(),
                    sample_dodeca_dead_link_target(),
                )
                .await
                .map_err(|e| format!("dodeca_devtools_open_dead_link failed: {e:?}"))?;
            if result != expected {
                return Err(format!(
                    "dodeca_devtools_open_dead_link: expected {expected:?}, got {result:?}"
                ));
            }
            info!("dodeca_devtools_open_dead_link OK");
        }
        "dodeca_devtools_edit_load" => {
            let expected = sample_dodeca_edit_load();
            let result = client
                .dodeca_devtools_edit_load("editor-token".to_string(), "/guide/".to_string())
                .await
                .map_err(|e| format!("dodeca_devtools_edit_load failed: {e:?}"))?;
            if result != expected {
                return Err(format!(
                    "dodeca_devtools_edit_load: expected {expected:?}, got {result:?}"
                ));
            }
            info!("dodeca_devtools_edit_load OK");
        }
        "dodeca_devtools_edit_preview" => {
            let expected = sample_dodeca_edit_preview();
            let result = client
                .dodeca_devtools_edit_preview(
                    "editor-token".to_string(),
                    "content/guide.md".to_string(),
                    "# Guide\n\nUpdated from browser.".to_string(),
                )
                .await
                .map_err(|e| format!("dodeca_devtools_edit_preview failed: {e:?}"))?;
            if result != expected {
                return Err(format!(
                    "dodeca_devtools_edit_preview: expected {expected:?}, got {result:?}"
                ));
            }
            info!("dodeca_devtools_edit_preview OK");
        }
        "dodeca_devtools_edit_save" => {
            let expected = sample_dodeca_edit_save();
            let result = client
                .dodeca_devtools_edit_save(
                    "editor-token".to_string(),
                    sample_dodeca_edit_save_req(),
                )
                .await
                .map_err(|e| format!("dodeca_devtools_edit_save failed: {e:?}"))?;
            if result != expected {
                return Err(format!(
                    "dodeca_devtools_edit_save: expected {expected:?}, got {result:?}"
                ));
            }
            info!("dodeca_devtools_edit_save OK");
        }
        "dodeca_devtools_edit_upload" => {
            let expected = sample_dodeca_edit_upload();
            let result = client
                .dodeca_devtools_edit_upload(
                    "editor-token".to_string(),
                    sample_dodeca_edit_upload_req(),
                )
                .await
                .map_err(|e| format!("dodeca_devtools_edit_upload failed: {e:?}"))?;
            if result != expected {
                return Err(format!(
                    "dodeca_devtools_edit_upload: expected {expected:?}, got {result:?}"
                ));
            }
            info!("dodeca_devtools_edit_upload OK");
        }
        "dodeca_devtools_edit_read" => {
            let expected = sample_dodeca_edit_read();
            let result = client
                .dodeca_devtools_edit_read(
                    "editor-token".to_string(),
                    "file:///workspace/content/guide.md".to_string(),
                )
                .await
                .map_err(|e| format!("dodeca_devtools_edit_read failed: {e:?}"))?;
            if result != expected {
                return Err(format!(
                    "dodeca_devtools_edit_read: expected {expected:?}, got {result:?}"
                ));
            }
            info!("dodeca_devtools_edit_read OK");
        }
        "dodeca_devtools_edit_list" => {
            let expected = sample_dodeca_edit_list();
            let result = client
                .dodeca_devtools_edit_list("editor-token".to_string())
                .await
                .map_err(|e| format!("dodeca_devtools_edit_list failed: {e:?}"))?;
            if result != expected {
                return Err(format!(
                    "dodeca_devtools_edit_list: expected {expected:?}, got {result:?}"
                ));
            }
            info!("dodeca_devtools_edit_list OK");
        }
        "echo_styx_value" => {
            let value = sample_styx_value();
            let result = client
                .echo_styx_value(value.clone())
                .await
                .map_err(|e| format!("echo_styx_value failed: {e:?}"))?;
            if result != value {
                return Err(format!(
                    "echo_styx_value: expected {value:?}, got {result:?}"
                ));
            }
            info!("echo_styx_value OK");
        }
        "styx_lsp_initialize" => {
            let expected = sample_styx_lsp_initialize_result();
            let result = client
                .styx_lsp_initialize(sample_styx_lsp_initialize_params())
                .await
                .map_err(|e| format!("styx_lsp_initialize failed: {e:?}"))?;
            if result != expected {
                return Err(format!(
                    "styx_lsp_initialize: expected {expected:?}, got {result:?}"
                ));
            }
            info!("styx_lsp_initialize OK");
        }
        "styx_lsp_completions" => {
            let expected = sample_styx_lsp_completions();
            let result = client
                .styx_lsp_completions(sample_styx_lsp_completion_params())
                .await
                .map_err(|e| format!("styx_lsp_completions failed: {e:?}"))?;
            if result != expected {
                return Err(format!(
                    "styx_lsp_completions: expected {expected:?}, got {result:?}"
                ));
            }
            info!("styx_lsp_completions OK");
        }
        "styx_lsp_hover" => {
            let expected = Some(sample_styx_lsp_hover_result());
            let result = client
                .styx_lsp_hover(sample_styx_lsp_hover_params())
                .await
                .map_err(|e| format!("styx_lsp_hover failed: {e:?}"))?;
            if result != expected {
                return Err(format!(
                    "styx_lsp_hover: expected {expected:?}, got {result:?}"
                ));
            }
            info!("styx_lsp_hover OK");
        }
        "styx_lsp_inlay_hints" => {
            let expected = sample_styx_lsp_inlay_hints();
            let result = client
                .styx_lsp_inlay_hints(sample_styx_lsp_inlay_hint_params())
                .await
                .map_err(|e| format!("styx_lsp_inlay_hints failed: {e:?}"))?;
            if result != expected {
                return Err(format!(
                    "styx_lsp_inlay_hints: expected {expected:?}, got {result:?}"
                ));
            }
            info!("styx_lsp_inlay_hints OK");
        }
        "styx_lsp_diagnostics" => {
            let expected = sample_styx_lsp_diagnostics();
            let result = client
                .styx_lsp_diagnostics(sample_styx_lsp_diagnostic_params())
                .await
                .map_err(|e| format!("styx_lsp_diagnostics failed: {e:?}"))?;
            if result != expected {
                return Err(format!(
                    "styx_lsp_diagnostics: expected {expected:?}, got {result:?}"
                ));
            }
            info!("styx_lsp_diagnostics OK");
        }
        "styx_lsp_code_actions" => {
            let expected = sample_styx_lsp_code_actions();
            let result = client
                .styx_lsp_code_actions(sample_styx_lsp_code_action_params())
                .await
                .map_err(|e| format!("styx_lsp_code_actions failed: {e:?}"))?;
            if result != expected {
                return Err(format!(
                    "styx_lsp_code_actions: expected {expected:?}, got {result:?}"
                ));
            }
            info!("styx_lsp_code_actions OK");
        }
        "styx_lsp_definition" => {
            let expected = sample_styx_lsp_locations();
            let result = client
                .styx_lsp_definition(sample_styx_lsp_definition_params())
                .await
                .map_err(|e| format!("styx_lsp_definition failed: {e:?}"))?;
            if result != expected {
                return Err(format!(
                    "styx_lsp_definition: expected {expected:?}, got {result:?}"
                ));
            }
            info!("styx_lsp_definition OK");
        }
        "styx_lsp_shutdown" => {
            client
                .styx_lsp_shutdown()
                .await
                .map_err(|e| format!("styx_lsp_shutdown failed: {e:?}"))?;
            info!("styx_lsp_shutdown OK");
        }
        "styx_host_get_subtree" => {
            let expected = Some(sample_styx_value());
            let result = client
                .styx_host_get_subtree(sample_styx_lsp_get_subtree_params())
                .await
                .map_err(|e| format!("styx_host_get_subtree failed: {e:?}"))?;
            if result != expected {
                return Err(format!(
                    "styx_host_get_subtree: expected {expected:?}, got {result:?}"
                ));
            }
            info!("styx_host_get_subtree OK");
        }
        "styx_host_get_document" => {
            let expected = Some(sample_styx_value());
            let result = client
                .styx_host_get_document(sample_styx_lsp_get_document_params())
                .await
                .map_err(|e| format!("styx_host_get_document failed: {e:?}"))?;
            if result != expected {
                return Err(format!(
                    "styx_host_get_document: expected {expected:?}, got {result:?}"
                ));
            }
            info!("styx_host_get_document OK");
        }
        "styx_host_get_source" => {
            let expected = Some(sample_styx_lsp_source());
            let result = client
                .styx_host_get_source(sample_styx_lsp_get_source_params())
                .await
                .map_err(|e| format!("styx_host_get_source failed: {e:?}"))?;
            if result != expected {
                return Err(format!(
                    "styx_host_get_source: expected {expected:?}, got {result:?}"
                ));
            }
            info!("styx_host_get_source OK");
        }
        "styx_host_get_schema" => {
            let expected = Some(sample_styx_lsp_schema_info());
            let result = client
                .styx_host_get_schema(sample_styx_lsp_get_schema_params())
                .await
                .map_err(|e| format!("styx_host_get_schema failed: {e:?}"))?;
            if result != expected {
                return Err(format!(
                    "styx_host_get_schema: expected {expected:?}, got {result:?}"
                ));
            }
            info!("styx_host_get_schema OK");
        }
        "styx_host_offset_to_position" => {
            let expected = Some(StyxLspPosition {
                line: 0,
                character: 16,
            });
            let result = client
                .styx_host_offset_to_position(sample_styx_lsp_offset_to_position_params())
                .await
                .map_err(|e| format!("styx_host_offset_to_position failed: {e:?}"))?;
            if result != expected {
                return Err(format!(
                    "styx_host_offset_to_position: expected {expected:?}, got {result:?}"
                ));
            }
            info!("styx_host_offset_to_position OK");
        }
        "styx_host_position_to_offset" => {
            let expected = Some(16);
            let result = client
                .styx_host_position_to_offset(sample_styx_lsp_position_to_offset_params())
                .await
                .map_err(|e| format!("styx_host_position_to_offset failed: {e:?}"))?;
            if result != expected {
                return Err(format!(
                    "styx_host_position_to_offset: expected {expected:?}, got {result:?}"
                ));
            }
            info!("styx_host_position_to_offset OK");
        }
        "stax_flamegraph" => {
            let params = sample_stax_view_params();
            let expected = sample_stax_flamegraph_update(&params);
            let result = client
                .stax_flamegraph(params)
                .await
                .map_err(|e| format!("stax_flamegraph failed: {e:?}"))?;
            if result != expected {
                return Err(format!(
                    "stax_flamegraph: expected {expected:?}, got {result:?}"
                ));
            }
            info!("stax_flamegraph OK");
        }
        "echo_stax_flamegraph_update" => {
            let params = sample_stax_view_params();
            let update = sample_stax_flamegraph_update(&params);
            let result = client
                .echo_stax_flamegraph_update(update.clone())
                .await
                .map_err(|e| format!("echo_stax_flamegraph_update failed: {e:?}"))?;
            if result != update {
                return Err(format!(
                    "echo_stax_flamegraph_update: expected {update:?}, got {result:?}"
                ));
            }
            info!("echo_stax_flamegraph_update OK");
        }
        "stax_subscribe_flamegraph_updates" => {
            let (update_tx, mut update_rx) = vox::channel::<spec_proto::StaxFlamegraphUpdate>();
            let expected = sample_stax_flamegraph_updates();
            client
                .stax_subscribe_flamegraph_updates(update_tx)
                .await
                .map_err(|e| format!("stax_subscribe_flamegraph_updates failed: {e:?}"))?;
            let mut updates = Vec::new();
            while let Ok(Some(update)) = update_rx.recv().await {
                updates.push(update.get().clone());
            }
            if updates != expected {
                return Err(format!(
                    "stax_subscribe_flamegraph_updates: expected {expected:?}, got {updates:?}"
                ));
            }
            info!("stax_subscribe_flamegraph_updates OK");
        }
        "echo_stax_linux_broker_control" => {
            let fixture = sample_stax_linux_broker_control_fixture();
            let result = client
                .echo_stax_linux_broker_control(fixture.clone())
                .await
                .map_err(|e| format!("echo_stax_linux_broker_control failed: {e:?}"))?;
            if result != fixture {
                return Err(format!(
                    "echo_stax_linux_broker_control: expected {fixture:?}, got {result:?}"
                ));
            }
            info!("echo_stax_linux_broker_control OK");
        }
        "stax_macos_record" => {
            let (batch_tx, mut batch_rx) = vox::channel::<spec_proto::StaxMacKdBufBatch>();
            let expected_batches = sample_stax_macos_batches();
            let recv_task = tokio::spawn(async move {
                let mut batches = Vec::new();
                while let Ok(Some(batch)) = batch_rx.recv().await {
                    batches.push(batch.get().clone());
                }
                batches
            });
            let result = client
                .stax_macos_record(sample_stax_macos_config(), batch_tx)
                .await
                .map_err(|e| format!("stax_macos_record failed: {e:?}"))?;
            let batches = recv_task
                .await
                .map_err(|e| format!("macos batches recv: {e}"))?;
            let expected_summary = sample_stax_macos_record_summary();
            if result != expected_summary {
                return Err(format!(
                    "stax_macos_record: expected {expected_summary:?}, got {result:?}"
                ));
            }
            if batches != expected_batches {
                return Err(format!(
                    "stax_macos_record batches: expected {expected_batches:?}, got {batches:?}"
                ));
            }
            info!("stax_macos_record OK");
        }
        "echo_hotmeal_live_reload_event" => {
            for event in sample_hotmeal_live_reload_events() {
                let result = client
                    .echo_hotmeal_live_reload_event(event.clone())
                    .await
                    .map_err(|e| format!("echo_hotmeal_live_reload_event failed: {e:?}"))?;
                if result != event {
                    return Err(format!(
                        "echo_hotmeal_live_reload_event: expected {event:?}, got {result:?}"
                    ));
                }
            }
            info!("echo_hotmeal_live_reload_event OK");
        }
        "echo_hotmeal_apply_patches_result" => {
            let payload = sample_hotmeal_apply_patches_result();
            let result = client
                .echo_hotmeal_apply_patches_result(payload.clone())
                .await
                .map_err(|e| format!("echo_hotmeal_apply_patches_result failed: {e:?}"))?;
            if result != payload {
                return Err(format!(
                    "echo_hotmeal_apply_patches_result: expected {payload:?}, got {result:?}"
                ));
            }
            info!("echo_hotmeal_apply_patches_result OK");
        }
        "hotmeal_live_reload_subscribe" => {
            client
                .hotmeal_live_reload_subscribe(sample_hotmeal_route())
                .await
                .map_err(|e| format!("hotmeal_live_reload_subscribe failed: {e:?}"))?;
            info!("hotmeal_live_reload_subscribe OK");
        }
        "hotmeal_live_reload_on_event" => {
            for event in sample_hotmeal_live_reload_events() {
                client
                    .hotmeal_live_reload_on_event(event.clone())
                    .await
                    .map_err(|e| format!("hotmeal_live_reload_on_event failed: {e:?}"))?;
            }
            info!("hotmeal_live_reload_on_event OK");
        }
        "echo_helix_stream_metrics" => {
            let metrics = sample_helix_stream_metrics();
            let result = client
                .echo_helix_stream_metrics(metrics.clone())
                .await
                .map_err(|e| format!("echo_helix_stream_metrics failed: {e:?}"))?;
            if result != metrics {
                return Err(format!(
                    "echo_helix_stream_metrics: expected {metrics:?}, got {result:?}"
                ));
            }
            info!("echo_helix_stream_metrics OK");
        }
        "echo_helix_verify_evidence" => {
            let digest = sample_helix_verify_evidence();
            let result = client
                .echo_helix_verify_evidence(digest.clone())
                .await
                .map_err(|e| format!("echo_helix_verify_evidence failed: {e:?}"))?;
            if result != digest {
                return Err(format!(
                    "echo_helix_verify_evidence: expected {digest:?}, got {result:?}"
                ));
            }
            info!("echo_helix_verify_evidence OK");
        }
        "helix_subscribe_pulses" => {
            let (pulse_tx, mut pulse_rx) = vox::channel::<spec_proto::HelixPulseAvailable>();
            let expected = sample_helix_pulses();
            let recv_task = tokio::spawn(async move {
                let mut pulses = Vec::new();
                while let Ok(Some(pulse)) = pulse_rx.recv().await {
                    pulses.push(*pulse.get());
                }
                pulses
            });
            client
                .helix_subscribe_pulses(pulse_tx)
                .await
                .map_err(|e| format!("helix_subscribe_pulses failed: {e:?}"))?;
            let pulses = recv_task.await.map_err(|e| format!("pulse recv: {e}"))?;
            if pulses != expected {
                return Err(format!(
                    "helix_subscribe_pulses: expected {expected:?}, got {pulses:?}"
                ));
            }
            info!("helix_subscribe_pulses OK");
        }
        "helix_pulse_bundle" => {
            let expected = sample_helix_pulse_bundle();
            let result = client
                .helix_pulse_bundle(
                    spec_proto::HelixSchedulerPulseId(102),
                    sample_helix_pulse_bundle_fields(),
                )
                .await
                .map_err(|e| format!("helix_pulse_bundle failed: {e:?}"))?;
            if result != expected {
                return Err(format!(
                    "helix_pulse_bundle: expected {expected:?}, got {result:?}"
                ));
            }
            info!("helix_pulse_bundle OK");
        }
        "helix_trace_service_surface" => {
            let expected = sample_helix_trace_service_surface();
            let result = client
                .helix_trace_service_surface()
                .await
                .map_err(|e| format!("helix_trace_service_surface failed: {e:?}"))?;
            if result != expected {
                return Err(format!(
                    "helix_trace_service_surface: expected {expected:?}, got {result:?}"
                ));
            }
            info!("helix_trace_service_surface OK");
        }
        "tracey_status" => {
            let expected = sample_tracey_status_response();
            let result = client
                .tracey_status()
                .await
                .map_err(|e| format!("tracey_status failed: {e:?}"))?;
            if result != expected {
                return Err(format!(
                    "tracey_status: expected {expected:?}, got {result:?}"
                ));
            }
            info!("tracey_status OK");
        }
        "tracey_core_control" => {
            let uncovered = client
                .tracey_uncovered(sample_tracey_query_request())
                .await
                .map_err(|e| format!("tracey_uncovered failed: {e:?}"))?;
            if uncovered != sample_tracey_uncovered_response() {
                return Err(format!("tracey_uncovered: got {uncovered:?}"));
            }

            let untested = client
                .tracey_untested(sample_tracey_untested_request())
                .await
                .map_err(|e| format!("tracey_untested failed: {e:?}"))?;
            if untested != sample_tracey_untested_response() {
                return Err(format!("tracey_untested: got {untested:?}"));
            }

            let stale = client
                .tracey_stale(sample_tracey_stale_request())
                .await
                .map_err(|e| format!("tracey_stale failed: {e:?}"))?;
            if stale != sample_tracey_stale_response() {
                return Err(format!("tracey_stale: got {stale:?}"));
            }

            let unmapped = client
                .tracey_unmapped(sample_tracey_unmapped_request())
                .await
                .map_err(|e| format!("tracey_unmapped failed: {e:?}"))?;
            if unmapped != sample_tracey_unmapped_response() {
                return Err(format!("tracey_unmapped: got {unmapped:?}"));
            }

            let config = client
                .tracey_config()
                .await
                .map_err(|e| format!("tracey_config failed: {e:?}"))?;
            if config != sample_tracey_api_config() {
                return Err(format!("tracey_config: got {config:?}"));
            }

            client
                .tracey_vfs_open("src/lib.rs".to_string(), sample_tracey_lsp_content())
                .await
                .map_err(|e| format!("tracey_vfs_open failed: {e:?}"))?;
            client
                .tracey_vfs_change(
                    "src/lib.rs".to_string(),
                    "// r[verify rpc.channel.direct-args]\n".to_string(),
                )
                .await
                .map_err(|e| format!("tracey_vfs_change failed: {e:?}"))?;
            client
                .tracey_vfs_close("src/lib.rs".to_string())
                .await
                .map_err(|e| format!("tracey_vfs_close failed: {e:?}"))?;

            let reload = client
                .tracey_reload()
                .await
                .map_err(|e| format!("tracey_reload failed: {e:?}"))?;
            if reload != sample_tracey_reload_response() {
                return Err(format!("tracey_reload: got {reload:?}"));
            }

            let version = client
                .tracey_version()
                .await
                .map_err(|e| format!("tracey_version failed: {e:?}"))?;
            if version != 13 {
                return Err(format!("tracey_version: got {version}"));
            }

            let health = client
                .tracey_health()
                .await
                .map_err(|e| format!("tracey_health failed: {e:?}"))?;
            if health != sample_tracey_health_response() {
                return Err(format!("tracey_health: got {health:?}"));
            }

            client
                .tracey_shutdown()
                .await
                .map_err(|e| format!("tracey_shutdown failed: {e:?}"))?;

            info!("tracey_core_control OK");
        }
        "tracey_rule" => {
            let result = client
                .tracey_rule(tracey_rule_id("rpc.channel.direct-args", 1))
                .await
                .map_err(|e| format!("tracey_rule known failed: {e:?}"))?;
            if result != Some(sample_tracey_rule_info()) {
                return Err(format!("tracey_rule known: got {result:?}"));
            }
            let missing = client
                .tracey_rule(tracey_rule_id("missing.rule", 1))
                .await
                .map_err(|e| format!("tracey_rule missing failed: {e:?}"))?;
            if missing.is_some() {
                return Err(format!(
                    "tracey_rule missing: expected None, got {missing:?}"
                ));
            }
            info!("tracey_rule OK");
        }
        "tracey_dashboard" => {
            let forward = client
                .tracey_forward("vox".to_string(), "rust".to_string())
                .await
                .map_err(|e| format!("tracey_forward failed: {e:?}"))?;
            if forward != Some(sample_tracey_forward_response()) {
                return Err(format!("tracey_forward: got {forward:?}"));
            }
            let missing_forward = client
                .tracey_forward("missing".to_string(), "rust".to_string())
                .await
                .map_err(|e| format!("tracey_forward missing failed: {e:?}"))?;
            if missing_forward.is_some() {
                return Err(format!(
                    "tracey_forward missing: expected None, got {missing_forward:?}"
                ));
            }

            let reverse = client
                .tracey_reverse("vox".to_string(), "rust".to_string())
                .await
                .map_err(|e| format!("tracey_reverse failed: {e:?}"))?;
            if reverse != Some(sample_tracey_reverse_response()) {
                return Err(format!("tracey_reverse: got {reverse:?}"));
            }

            let file = client
                .tracey_file(sample_tracey_file_request())
                .await
                .map_err(|e| format!("tracey_file failed: {e:?}"))?;
            if file != Some(sample_tracey_file_response()) {
                return Err(format!("tracey_file: got {file:?}"));
            }

            let spec_content = client
                .tracey_spec_content("vox".to_string(), "rust".to_string())
                .await
                .map_err(|e| format!("tracey_spec_content failed: {e:?}"))?;
            if spec_content != Some(sample_tracey_spec_content_response()) {
                return Err(format!("tracey_spec_content: got {spec_content:?}"));
            }

            let search = client
                .tracey_search("channel".to_string(), 10)
                .await
                .map_err(|e| format!("tracey_search failed: {e:?}"))?;
            if search != sample_tracey_search_results() {
                return Err(format!("tracey_search: got {search:?}"));
            }

            client
                .tracey_update_file_range(sample_tracey_update_file_range_request())
                .await
                .map_err(|e| format!("tracey_update_file_range ok failed: {e:?}"))?;
            match client
                .tracey_update_file_range(sample_tracey_update_file_range_conflict_request())
                .await
            {
                Err(vox::VoxError::User(error)) if *error == sample_tracey_update_error() => {}
                Ok(()) => {
                    return Err(
                        "tracey_update_file_range conflict: expected user error".to_string()
                    );
                }
                Err(other) => {
                    return Err(format!(
                        "tracey_update_file_range conflict: expected user error, got {other:?}"
                    ));
                }
            }

            client
                .tracey_config_add_exclude(sample_tracey_config_pattern_request())
                .await
                .map_err(|e| format!("tracey_config_add_exclude ok failed: {e:?}"))?;
            match client
                .tracey_config_add_exclude(sample_tracey_bad_config_pattern_request())
                .await
            {
                Err(vox::VoxError::User(error)) if error.as_str() == "invalid pattern" => {}
                Ok(()) => {
                    return Err(
                        "tracey_config_add_exclude bad pattern: expected user error".to_string()
                    );
                }
                Err(other) => {
                    return Err(format!(
                        "tracey_config_add_exclude bad pattern: expected user error, got {other:?}"
                    ));
                }
            }
            client
                .tracey_config_add_include(sample_tracey_config_pattern_request())
                .await
                .map_err(|e| format!("tracey_config_add_include failed: {e:?}"))?;

            info!("tracey_dashboard OK");
        }
        "tracey_validate" => {
            let expected = sample_tracey_validation_result();
            let result = client
                .tracey_validate(sample_tracey_validate_request())
                .await
                .map_err(|e| format!("tracey_validate failed: {e:?}"))?;
            if result != expected {
                return Err(format!(
                    "tracey_validate: expected {expected:?}, got {result:?}"
                ));
            }
            info!("tracey_validate OK");
        }
        "tracey_lsp_surface" => {
            let test_file = client
                .tracey_is_test_file("spec/spec-tests/tests/cases/testbed.rs".to_string())
                .await
                .map_err(|e| format!("tracey_is_test_file true failed: {e:?}"))?;
            if !test_file {
                return Err("tracey_is_test_file: expected true for tests path".to_string());
            }
            let source_file = client
                .tracey_is_test_file("src/lib.rs".to_string())
                .await
                .map_err(|e| format!("tracey_is_test_file false failed: {e:?}"))?;
            if source_file {
                return Err("tracey_is_test_file: expected false for source path".to_string());
            }

            let hover = client
                .tracey_lsp_hover(sample_tracey_lsp_position_request())
                .await
                .map_err(|e| format!("tracey_lsp_hover failed: {e:?}"))?;
            if hover != Some(sample_tracey_hover_info()) {
                return Err(format!("tracey_lsp_hover: got {hover:?}"));
            }

            let definition = client
                .tracey_lsp_definition(sample_tracey_lsp_position_request())
                .await
                .map_err(|e| format!("tracey_lsp_definition failed: {e:?}"))?;
            if definition != sample_tracey_lsp_locations() {
                return Err(format!("tracey_lsp_definition: got {definition:?}"));
            }

            let implementation = client
                .tracey_lsp_implementation(sample_tracey_lsp_position_request())
                .await
                .map_err(|e| format!("tracey_lsp_implementation failed: {e:?}"))?;
            if implementation != sample_tracey_lsp_locations() {
                return Err(format!("tracey_lsp_implementation: got {implementation:?}"));
            }

            let references = client
                .tracey_lsp_references(sample_tracey_lsp_references_request())
                .await
                .map_err(|e| format!("tracey_lsp_references failed: {e:?}"))?;
            if references != sample_tracey_lsp_locations() {
                return Err(format!("tracey_lsp_references: got {references:?}"));
            }

            let completions = client
                .tracey_lsp_completions(sample_tracey_lsp_position_request())
                .await
                .map_err(|e| format!("tracey_lsp_completions failed: {e:?}"))?;
            if completions != sample_tracey_lsp_completions() {
                return Err(format!("tracey_lsp_completions: got {completions:?}"));
            }

            let document_symbols = client
                .tracey_lsp_document_symbols(sample_tracey_lsp_document_request())
                .await
                .map_err(|e| format!("tracey_lsp_document_symbols failed: {e:?}"))?;
            if document_symbols != sample_tracey_lsp_symbols() {
                return Err(format!(
                    "tracey_lsp_document_symbols: got {document_symbols:?}"
                ));
            }

            let workspace_symbols = client
                .tracey_lsp_workspace_symbols("rpc.channel".to_string())
                .await
                .map_err(|e| format!("tracey_lsp_workspace_symbols failed: {e:?}"))?;
            if workspace_symbols != sample_tracey_lsp_symbols() {
                return Err(format!(
                    "tracey_lsp_workspace_symbols: got {workspace_symbols:?}"
                ));
            }

            let semantic_tokens = client
                .tracey_lsp_semantic_tokens(sample_tracey_lsp_document_request())
                .await
                .map_err(|e| format!("tracey_lsp_semantic_tokens failed: {e:?}"))?;
            if semantic_tokens != sample_tracey_lsp_semantic_tokens() {
                return Err(format!(
                    "tracey_lsp_semantic_tokens: got {semantic_tokens:?}"
                ));
            }

            let code_lens = client
                .tracey_lsp_code_lens(sample_tracey_lsp_document_request())
                .await
                .map_err(|e| format!("tracey_lsp_code_lens failed: {e:?}"))?;
            if code_lens != sample_tracey_lsp_code_lens() {
                return Err(format!("tracey_lsp_code_lens: got {code_lens:?}"));
            }

            let inlay_hints = client
                .tracey_lsp_inlay_hints(sample_tracey_lsp_inlay_hints_request())
                .await
                .map_err(|e| format!("tracey_lsp_inlay_hints failed: {e:?}"))?;
            if inlay_hints != sample_tracey_lsp_inlay_hints() {
                return Err(format!("tracey_lsp_inlay_hints: got {inlay_hints:?}"));
            }

            let prepare_rename = client
                .tracey_lsp_prepare_rename(sample_tracey_lsp_position_request())
                .await
                .map_err(|e| format!("tracey_lsp_prepare_rename failed: {e:?}"))?;
            if prepare_rename != Some(sample_tracey_prepare_rename_result()) {
                return Err(format!("tracey_lsp_prepare_rename: got {prepare_rename:?}"));
            }

            let text_edits = client
                .tracey_lsp_rename(sample_tracey_lsp_rename_request())
                .await
                .map_err(|e| format!("tracey_lsp_rename failed: {e:?}"))?;
            if text_edits != sample_tracey_lsp_text_edits() {
                return Err(format!("tracey_lsp_rename: got {text_edits:?}"));
            }

            let code_actions = client
                .tracey_lsp_code_actions(sample_tracey_lsp_position_request())
                .await
                .map_err(|e| format!("tracey_lsp_code_actions failed: {e:?}"))?;
            if code_actions != sample_tracey_lsp_code_actions() {
                return Err(format!("tracey_lsp_code_actions: got {code_actions:?}"));
            }

            let highlights = client
                .tracey_lsp_document_highlight(sample_tracey_lsp_position_request())
                .await
                .map_err(|e| format!("tracey_lsp_document_highlight failed: {e:?}"))?;
            if highlights != sample_tracey_lsp_locations() {
                return Err(format!("tracey_lsp_document_highlight: got {highlights:?}"));
            }

            info!("tracey_lsp_surface OK");
        }
        "tracey_lsp_workspace_diagnostics" => {
            let expected = sample_tracey_lsp_workspace_diagnostics();
            let result = client
                .tracey_lsp_workspace_diagnostics()
                .await
                .map_err(|e| format!("tracey_lsp_workspace_diagnostics failed: {e:?}"))?;
            if result != expected {
                return Err(format!(
                    "tracey_lsp_workspace_diagnostics: expected {expected:?}, got {result:?}"
                ));
            }
            info!("tracey_lsp_workspace_diagnostics OK");
        }
        "tracey_subscribe_updates" => {
            let (update_tx, mut update_rx) = vox::channel::<spec_proto::TraceyDataUpdate>();
            let expected = sample_tracey_updates();
            let recv_task = tokio::spawn(async move {
                let mut updates = Vec::new();
                while let Ok(Some(update)) = update_rx.recv().await {
                    updates.push(update.get().clone());
                }
                updates
            });
            client
                .tracey_subscribe_updates(update_tx)
                .await
                .map_err(|e| format!("tracey_subscribe_updates failed: {e:?}"))?;
            let updates = recv_task.await.map_err(|e| format!("update recv: {e}"))?;
            if updates != expected {
                return Err(format!(
                    "tracey_subscribe_updates: expected {expected:?}, got {updates:?}"
                ));
            }
            info!("tracey_subscribe_updates OK");
        }
        other => return Err(format!("unknown CLIENT_SCENARIO: {other}")),
    }

    if let Some(connection) = client.connection.as_ref() {
        connection.shutdown().ok();
    }
    tokio::time::timeout(Duration::from_secs(1), client.caller.closed())
        .await
        .ok();
    Ok(())
}

fn sample_ecosystem_bridge_payload() -> EcosystemBridgePayload {
    EcosystemBridgePayload {
        html: "<main><img src=\"/hero.png\"></main>".to_string(),
        path_map: BTreeMap::from([("/old.css".to_string(), "/assets/new.css".to_string())]),
        known_routes: BTreeSet::from(["/".to_string(), "/guide/".to_string()]),
        image_variants: BTreeMap::from([(
            "/hero.png".to_string(),
            BridgeResponsiveImageInfo {
                jxl_srcset: vec![("/hero-640.jxl".to_string(), 640)],
                webp_srcset: vec![("/hero-640.webp".to_string(), 640)],
            },
        )]),
        blobs: vec![vec![0, 1, 2, 3, 255], vec![]],
    }
}

fn sample_dynamic_template_object() -> Value {
    let mut object = VObject::new();
    object.insert(VString::new("sidebar"), Value::from(true));
    object.insert(VString::new("title"), Value::from("Phon migration"));
    object.insert(VString::new("count"), Value::from(42i64));
    object.into()
}

fn sample_dodeca_template_call() -> DodecaTemplateCall {
    DodecaTemplateCall {
        context_id: "ctx-docs".to_string(),
        name: "render-card".to_string(),
        args: vec![sample_dynamic_template_object(), Value::from("docs")],
        kwargs: vec![("path".to_string(), Value::from("/guide/"))],
    }
}
