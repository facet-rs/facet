//! In-process LSP integration tests.
//!
//! These tests use tower-lsp's in-process testing capabilities to verify
//! the LSP server behavior without spawning subprocesses or parsing protocols.

use futures::StreamExt;
use serde_json::{Value, json};
use tower::Service;
use tower_lsp::LspService;
use tower_lsp::jsonrpc::Request;

use styx_lsp::StyxLanguageServer;

/// Regression test: ensure CST catches "too many atoms" errors
#[test]
fn test_cst_catches_too_many_atoms_error() {
    // This is a minimal reproduction of the issue found in queries.styx
    // The error is "unexpected atom after value (entry has too many atoms)"
    // in `join admin_user {on {session.user_id admin_user.id}}`
    // where `admin_user` and `{on {...}}` are both values (only one allowed in objects)
    let content = r#"ValidateSession @query{
    params {session_id @string}
    from session
    join admin_user {on {session.user_id admin_user.id}}
    where {session.id $session_id}
    first true
    select {session.user_id, admin_user.email, session.expires_at}
}"#;

    let parsed = styx_cst::parse(content);
    let errors = parsed.errors();

    // CST should now catch this error
    assert!(
        !errors.is_empty(),
        "CST should catch 'too many atoms' error"
    );
    assert!(
        errors[0].message.contains("too many atoms"),
        "Error message should mention 'too many atoms': {}",
        errors[0].message
    );
}

/// Test that CST errors are properly reported as diagnostics to the client.
/// This is a regression test for the queries.styx bug.
#[tokio::test]
async fn test_cst_error_reported_as_diagnostic() {
    // This document has a "too many atoms" error that CST now catches
    let content = r#"ValidateSession @query{
    params {session_id @string}
    from session
    join admin_user {on {session.user_id admin_user.id}}
    where {session.id $session_id}
    first true
    select {session.user_id, admin_user.email, session.expires_at}
}"#;

    // Verify CST catches this error
    let cst = styx_cst::parse(content);
    assert!(
        !cst.errors().is_empty(),
        "CST should catch the 'too many atoms' error"
    );

    // Now test that LSP reports it
    let (mut service, socket) = LspService::new(StyxLanguageServer::new);
    let (mut notifications, _responses) = socket.split();

    let (tx, mut rx) = tokio::sync::mpsc::channel::<tower_lsp::jsonrpc::Request>(32);
    let drain_task = tokio::spawn(async move {
        while let Some(notification) = notifications.next().await {
            let _ = tx.send(notification).await;
        }
    });

    // Initialize
    let init_request = make_request(
        1,
        "initialize",
        json!({
            "processId": null,
            "capabilities": {},
            "rootUri": null
        }),
    );
    let _: Option<tower_lsp::jsonrpc::Response> = service.call(init_request).await.unwrap();
    let _ = service
        .call(make_notification("initialized", json!({})))
        .await;

    // Open the document
    let did_open = make_notification(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": "file:///cst_error.styx",
                "languageId": "styx",
                "version": 1,
                "text": content
            }
        }),
    );
    let _ = service.call(did_open).await;

    // Wait for diagnostics
    let mut found_diagnostics = false;
    let mut diagnostic_message = String::new();

    let timeout = tokio::time::sleep(tokio::time::Duration::from_secs(1));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            Some(notification) = rx.recv() => {
                if notification.method() == "textDocument/publishDiagnostics" {
                    if let Some(params) = notification.params() {
                        if params.get("uri").and_then(|u| u.as_str()) == Some("file:///cst_error.styx") {
                            if let Some(diagnostics) = params.get("diagnostics").and_then(|d| d.as_array()) {
                                if !diagnostics.is_empty() {
                                    found_diagnostics = true;
                                    diagnostic_message = diagnostics[0]
                                        .get("message")
                                        .and_then(|m| m.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                }
                            }
                            break;
                        }
                    }
                }
            }
            _ = &mut timeout => {
                break;
            }
        }
    }

    drain_task.abort();

    assert!(
        found_diagnostics,
        "LSP should report CST error as diagnostic"
    );
    assert!(
        diagnostic_message.contains("too many atoms"),
        "Diagnostic should mention the error, got: {}",
        diagnostic_message
    );
}

/// Helper to create a JSON-RPC request
fn make_request(id: i64, method: &str, params: Value) -> Request {
    let req = json!({
        "jsonrpc": "2.0",
        "id": id,
        "method": method,
        "params": params
    });
    serde_json::from_value(req).expect("valid request")
}

/// Helper to create a JSON-RPC notification (no id)
fn make_notification(method: &str, params: Value) -> Request {
    let req = json!({
        "jsonrpc": "2.0",
        "method": method,
        "params": params
    });
    serde_json::from_value(req).expect("valid notification")
}

#[tokio::test]
async fn test_diagnostics_published_for_parse_error() {
    // Create the LSP service in-process
    let (mut service, socket) = LspService::new(StyxLanguageServer::new);

    // Split the socket to receive server-to-client notifications
    let (mut notifications, _responses) = socket.split();

    // Spawn a task to drain notifications - this prevents blocking when server sends
    let (tx, mut rx) = tokio::sync::mpsc::channel::<tower_lsp::jsonrpc::Request>(32);
    let drain_task = tokio::spawn(async move {
        while let Some(notification) = notifications.next().await {
            let _ = tx.send(notification).await;
        }
    });

    // Initialize the server
    let init_request = make_request(
        1,
        "initialize",
        json!({
            "processId": null,
            "capabilities": {},
            "rootUri": null
        }),
    );

    let response: Option<tower_lsp::jsonrpc::Response> =
        service.call(init_request).await.expect("initialize failed");
    assert!(response.is_some(), "initialize should return a response");

    // Send initialized notification
    let initialized = make_notification("initialized", json!({}));
    let _ = service.call(initialized).await;

    // Open a document with a parse error ("foo {a b c}" has too many atoms)
    let did_open = make_notification(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": "file:///test.styx",
                "languageId": "styx",
                "version": 1,
                "text": "foo {a b c}"
            }
        }),
    );

    let _ = service.call(did_open).await;

    // Wait for diagnostics notification
    let mut found_diagnostics = false;
    let mut diagnostic_count = 0;
    let mut diagnostic_message = String::new();

    // Give it time to receive the diagnostics
    let timeout = tokio::time::sleep(tokio::time::Duration::from_secs(1));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            Some(notification) = rx.recv() => {
                if notification.method() == "textDocument/publishDiagnostics" {
                    if let Some(params) = notification.params() {
                        if params.get("uri").and_then(|u| u.as_str()) == Some("file:///test.styx") {
                            if let Some(diagnostics) = params.get("diagnostics").and_then(|d| d.as_array()) {
                                diagnostic_count = diagnostics.len();
                                found_diagnostics = true;
                                if !diagnostics.is_empty() {
                                    diagnostic_message = diagnostics[0]
                                        .get("message")
                                        .and_then(|m| m.as_str())
                                        .unwrap_or("")
                                        .to_string();
                                }
                            }
                            break;
                        }
                    }
                }
            }
            _ = &mut timeout => {
                break;
            }
        }
    }

    drain_task.abort();

    assert!(
        found_diagnostics,
        "Server should publish diagnostics for document with parse error"
    );
    assert!(
        diagnostic_count > 0,
        "Should have at least one diagnostic for parse error"
    );
    assert!(
        diagnostic_message.contains("unexpected atom"),
        "Diagnostic should mention 'unexpected atom', got: {}",
        diagnostic_message
    );
}

#[tokio::test]
async fn test_no_diagnostics_for_valid_document() {
    let (mut service, socket) = LspService::new(StyxLanguageServer::new);
    let (mut notifications, _responses) = socket.split();

    // Spawn a task to drain notifications
    let (tx, mut rx) = tokio::sync::mpsc::channel::<tower_lsp::jsonrpc::Request>(32);
    let drain_task = tokio::spawn(async move {
        while let Some(notification) = notifications.next().await {
            let _ = tx.send(notification).await;
        }
    });

    // Initialize
    let init_request = make_request(
        1,
        "initialize",
        json!({
            "processId": null,
            "capabilities": {},
            "rootUri": null
        }),
    );
    let _ = service.call(init_request).await;
    let _ = service
        .call(make_notification("initialized", json!({})))
        .await;

    // Open a valid document
    let did_open = make_notification(
        "textDocument/didOpen",
        json!({
            "textDocument": {
                "uri": "file:///valid.styx",
                "languageId": "styx",
                "version": 1,
                "text": "server {\n    host localhost\n    port 8080\n}"
            }
        }),
    );

    let _ = service.call(did_open).await;

    // Check for diagnostics
    let mut diagnostic_count = 0;
    let mut found_our_doc = false;

    let timeout = tokio::time::sleep(tokio::time::Duration::from_secs(1));
    tokio::pin!(timeout);

    loop {
        tokio::select! {
            Some(notification) = rx.recv() => {
                if notification.method() == "textDocument/publishDiagnostics" {
                    if let Some(params) = notification.params() {
                        if params.get("uri").and_then(|u| u.as_str()) == Some("file:///valid.styx") {
                            found_our_doc = true;
                            if let Some(diagnostics) = params.get("diagnostics").and_then(|d| d.as_array()) {
                                diagnostic_count = diagnostics.len();
                            }
                            break;
                        }
                    }
                }
            }
            _ = &mut timeout => {
                break;
            }
        }
    }

    drain_task.abort();

    assert!(
        found_our_doc,
        "Should have received diagnostics for our document"
    );
    assert_eq!(
        diagnostic_count, 0,
        "Valid document should have no diagnostics"
    );
}
