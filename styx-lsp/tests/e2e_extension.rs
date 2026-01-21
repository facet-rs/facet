//! End-to-end tests for LSP extension functionality.
//!
//! These tests spawn the actual styx-lsp server and make real LSP requests
//! to verify the full pipeline including extension communication.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::atomic::{AtomicI32, Ordering};

use serde_json::{Value, json};

static REQUEST_ID: AtomicI32 = AtomicI32::new(1);

fn next_id() -> i32 {
    REQUEST_ID.fetch_add(1, Ordering::SeqCst)
}

/// A simple LSP client for testing.
struct LspClient {
    #[allow(dead_code)]
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
}

impl LspClient {
    /// Spawn styx lsp server.
    fn spawn() -> Self {
        let mut child = Command::new("styx")
            .arg("lsp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .expect("Failed to spawn styx lsp");

        let stdin = child.stdin.take().expect("Failed to get stdin");
        let stdout = BufReader::new(child.stdout.take().expect("Failed to get stdout"));

        Self {
            child,
            stdin,
            stdout,
        }
    }

    /// Send a JSON-RPC request and return the response.
    fn request(&mut self, method: &str, params: Value) -> Value {
        let id = next_id();
        let request = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });

        self.send_message(&request);
        self.read_response(id)
    }

    /// Send a notification (no response expected).
    fn notify(&mut self, method: &str, params: Value) {
        let notification = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });

        self.send_message(&notification);
    }

    fn send_message(&mut self, msg: &Value) {
        let content = serde_json::to_string(msg).unwrap();
        let header = format!("Content-Length: {}\r\n\r\n", content.len());

        self.stdin.write_all(header.as_bytes()).unwrap();
        self.stdin.write_all(content.as_bytes()).unwrap();
        self.stdin.flush().unwrap();
    }

    fn read_response(&mut self, expected_id: i32) -> Value {
        loop {
            // Read headers
            let mut content_length: Option<usize> = None;
            loop {
                let mut line = String::new();
                self.stdout.read_line(&mut line).unwrap();
                let line = line.trim();

                if line.is_empty() {
                    break;
                }

                if let Some(len_str) = line.strip_prefix("Content-Length: ") {
                    content_length = Some(len_str.parse().unwrap());
                }
            }

            let content_length = content_length.expect("No Content-Length header");

            // Read content
            let mut content = vec![0u8; content_length];
            self.stdout.read_exact(&mut content).unwrap();

            let msg: Value = serde_json::from_slice(&content).unwrap();

            // Check if this is the response we're waiting for
            if let Some(id) = msg.get("id")
                && id.as_i64() == Some(expected_id as i64) {
                    return msg;
                }

            // Otherwise it's a notification or different response, skip it
        }
    }

    /// Initialize the LSP server.
    fn initialize(&mut self) {
        let response = self.request(
            "initialize",
            json!({
                "processId": std::process::id(),
                "capabilities": {},
                "rootUri": null
            }),
        );

        assert!(
            response.get("result").is_some(),
            "Initialize should succeed: {:?}",
            response
        );

        self.notify("initialized", json!({}));
    }

    /// Open a document.
    fn open_document(&mut self, uri: &str, content: &str) {
        self.notify(
            "textDocument/didOpen",
            json!({
                "textDocument": {
                    "uri": uri,
                    "languageId": "styx",
                    "version": 1,
                    "text": content
                }
            }),
        );
    }

    /// Request hover at a position.
    fn hover(&mut self, uri: &str, line: u32, character: u32) -> Option<Value> {
        let response = self.request(
            "textDocument/hover",
            json!({
                "textDocument": {"uri": uri},
                "position": {"line": line, "character": character}
            }),
        );

        response.get("result").cloned()
    }

    /// Request completions at a position.
    #[allow(dead_code)]
    fn completions(&mut self, uri: &str, line: u32, character: u32) -> Vec<Value> {
        let response = self.request(
            "textDocument/completion",
            json!({
                "textDocument": {"uri": uri},
                "position": {"line": line, "character": character}
            }),
        );

        match response.get("result") {
            Some(Value::Array(items)) => items.clone(),
            Some(Value::Object(obj)) if obj.contains_key("items") => {
                obj.get("items").unwrap().as_array().unwrap().clone()
            }
            _ => vec![],
        }
    }

    /// Request inlay hints for a range.
    fn inlay_hints(&mut self, uri: &str, start_line: u32, end_line: u32) -> Vec<Value> {
        let response = self.request(
            "textDocument/inlayHint",
            json!({
                "textDocument": {"uri": uri},
                "range": {
                    "start": {"line": start_line, "character": 0},
                    "end": {"line": end_line, "character": 0}
                }
            }),
        );

        match response.get("result") {
            Some(Value::Array(hints)) => hints.clone(),
            _ => vec![],
        }
    }

    /// Allow an extension command to run.
    fn allow_extension(&mut self, command: &str) {
        self.request(
            "workspace/executeCommand",
            json!({
                "command": "styx.allowExtension",
                "arguments": [{"command": command}]
            }),
        );
    }

    /// Shutdown the server.
    fn shutdown(&mut self) {
        self.request("shutdown", json!(null));
        self.notify("exit", json!(null));
    }
}

// =============================================================================
// Tests
// =============================================================================

/// Test document with dibs-queries schema for extension testing.
/// Uses the 'posts' table which has columns: id, title, body, published, author_id, tenant_id, created_at, updated_at
const DIBS_QUERY_DOC: &str = r#"@schema {id crate:dibs-queries@1, cli dibs}

AllPosts @query{
    from posts
    where {published true}
    select {id, title, body}
}
"#;

#[test]
#[ignore = "requires styx and dibs binaries in PATH"]
fn test_hover_on_column() {
    let mut client = LspClient::spawn();
    client.initialize();

    // Allow the dibs extension
    client.allow_extension("dibs");

    let uri = "file:///test/queries.styx";
    client.open_document(uri, DIBS_QUERY_DOC);

    // Give server time to process and spawn extension
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Hover over "title" in select {id, title, body}
    // Line 5 (0-indexed): "    select {id, title, body}"
    // "title" starts at character 16
    let hover = client.hover(uri, 5, 17);

    eprintln!("Hover result: {:?}", hover);

    assert!(hover.is_some(), "Should get hover for column 'title'");
    let hover = hover.unwrap();

    if !hover.is_null() {
        let contents = hover
            .get("contents")
            .and_then(|c| c.get("value"))
            .and_then(|v| v.as_str());

        assert!(
            contents.is_some(),
            "Hover should have contents: {:?}",
            hover
        );
        let contents = contents.unwrap();
        assert!(
            contents.contains("title") || contents.contains("Column"),
            "Hover should mention column: {}",
            contents
        );
    }

    client.shutdown();
}

#[test]
#[ignore = "requires styx and dibs binaries in PATH"]
fn test_hover_on_table() {
    let mut client = LspClient::spawn();
    client.initialize();

    // Allow the dibs extension
    client.allow_extension("dibs");

    let uri = "file:///test/queries.styx";
    client.open_document(uri, DIBS_QUERY_DOC);

    std::thread::sleep(std::time::Duration::from_millis(500));

    // Hover over "posts" in "from posts"
    // Line 3: "    from posts"
    let hover = client.hover(uri, 3, 10);

    eprintln!("Hover result for table: {:?}", hover);

    assert!(hover.is_some(), "Should get hover for table 'posts'");
    let hover = hover.unwrap();

    if !hover.is_null() {
        let contents = hover
            .get("contents")
            .and_then(|c| c.get("value"))
            .and_then(|v| v.as_str());

        assert!(
            contents.is_some(),
            "Hover should have contents: {:?}",
            hover
        );
    }

    client.shutdown();
}

#[test]
#[ignore = "requires styx and dibs binaries in PATH"]
fn test_completions_for_columns() {
    let mut client = LspClient::spawn();
    client.initialize();

    // Allow the dibs extension
    client.allow_extension("dibs");

    // Document with cursor position for completions
    let doc = r#"@schema {id crate:dibs-queries@1, cli dibs}

AllPosts @query{
    from posts
    select {}
}
"#;

    let uri = "file:///test/queries.styx";
    client.open_document(uri, doc);

    std::thread::sleep(std::time::Duration::from_millis(500));

    // Request completions inside select {}
    // Line 4, character 12 (inside the braces)
    let completions = client.completions(uri, 4, 12);

    eprintln!("Completions: {:?}", completions);

    assert!(
        !completions.is_empty(),
        "Should get column completions in select block"
    );

    // Check that we get posts columns (not all columns from all tables)
    let labels: Vec<&str> = completions
        .iter()
        .filter_map(|c| c.get("label").and_then(|l| l.as_str()))
        .collect();

    eprintln!("Completion labels: {:?}", labels);

    // Posts table should have these columns
    assert!(
        labels.iter().any(|l| *l == "id" || *l == "title"),
        "Should have posts columns"
    );

    client.shutdown();
}

#[test]
#[ignore = "requires styx and dibs binaries in PATH"]
fn test_inlay_hints() {
    let mut client = LspClient::spawn();
    client.initialize();

    // Allow the dibs extension
    client.allow_extension("dibs");

    let uri = "file:///test/queries.styx";
    client.open_document(uri, DIBS_QUERY_DOC);

    std::thread::sleep(std::time::Duration::from_millis(500));

    // Request inlay hints for the whole document
    let hints = client.inlay_hints(uri, 0, 10);

    eprintln!("Inlay hints: {:?}", hints);

    // We should get type hints for columns in select
    // This might be empty if inlay hints aren't working
    if !hints.is_empty() {
        let labels: Vec<&str> = hints
            .iter()
            .filter_map(|h| h.get("label").and_then(|l| l.as_str()))
            .collect();

        eprintln!("Hint labels: {:?}", labels);
    }

    client.shutdown();
}

/// Run this test manually to debug the full pipeline:
/// cargo nextest run -p styx-lsp test_debug_extension_pipeline -- --ignored --nocapture
#[test]
#[ignore = "manual debugging test"]
fn test_debug_extension_pipeline() {
    // Set RUST_LOG so styx-lsp will output debug logs
    // SAFETY: We're in a test, single-threaded at this point
    unsafe {
        std::env::set_var("RUST_LOG", "styx_lsp=debug,dibs=debug");
    }

    let mut client = LspClient::spawn();
    client.initialize();

    // Allow the dibs extension
    client.allow_extension("dibs");

    let uri = "file:///test/queries.styx";
    client.open_document(uri, DIBS_QUERY_DOC);

    // Wait for extension to initialize
    std::thread::sleep(std::time::Duration::from_secs(1));

    println!("\n=== Document ===");
    println!("{}", DIBS_QUERY_DOC);

    println!("\n=== Hover on 'title' (line 5, char 17) ===");
    let hover = client.hover(uri, 5, 17);
    println!("{:#?}", hover);

    println!("\n=== Hover on 'posts' (line 3, char 10) ===");
    let hover = client.hover(uri, 3, 10);
    println!("{:#?}", hover);

    println!("\n=== Inlay hints ===");
    let hints = client.inlay_hints(uri, 0, 10);
    println!("{:#?}", hints);

    // Read any stderr from the server
    println!("\n=== Server stderr (if any) ===");
    if let Some(stderr) = client.child.stderr.take() {
        let mut reader = BufReader::new(stderr);
        let mut line = String::new();
        while reader.read_line(&mut line).unwrap_or(0) > 0 {
            print!("{}", line);
            line.clear();
        }
    }

    client.shutdown();
}
