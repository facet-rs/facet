//! Diagnostics over Rapace - Demo Binary
//!
//! This example demonstrates streaming diagnostics where:
//! - The **host** sends a source file to analyze
//! - The **cell** streams back diagnostic findings

use std::sync::Arc;

use rapace::{AnyTransport, RpcSession};
use tokio_stream::StreamExt;

use rapace_diagnostics_over_rapace::{DiagnosticsClient, DiagnosticsImpl, DiagnosticsServer};

fn main() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async_main());
}

async fn async_main() {
    println!("=== Diagnostics over Rapace Demo ===\n");

    // Create a transport pair (in-memory for demo)
    let (host_transport, cell_transport) = AnyTransport::mem_pair();

    // ========== CELL SIDE ==========
    // Use DiagnosticsServer::serve() which handles the frame loop
    let server = DiagnosticsServer::new(DiagnosticsImpl);
    let _cell_handle = tokio::spawn(server.serve(cell_transport));

    // ========== HOST SIDE ==========
    // Create RpcSession and client (transports are cheap to clone)
    let session = Arc::new(RpcSession::new(host_transport));
    let session_clone = session.clone();
    tokio::spawn(async move { session_clone.run().await });
    let client = DiagnosticsClient::new(session);

    // ========== ANALYZE A SOURCE FILE ==========
    let source = r#"
// This is a sample source file
fn main() {
    // TODO: implement actual logic
    println!("Hello, world!");

    // FIXME: this is broken
    let x = 42;

    // NOTE: this is just for testing
    let y = x + 1;

    // Another TODO here
    process(y);
}

fn process(n: i32) {
    // TODO: add error handling
    println!("Processing: {}", n);
}
"#;

    println!("--- Analyzing source file ---\n");
    println!("Source:\n{}", source);
    println!("\n--- Diagnostics stream ---\n");

    // Call analyze and consume the stream
    let mut stream = client
        .analyze("example.rs".to_string(), source.as_bytes().to_vec())
        .await
        .expect("analyze call failed");

    let mut count = 0;
    while let Some(result) = stream.next().await {
        match result {
            Ok(diag) => {
                count += 1;
                let icon = match diag.severity.as_str() {
                    "info" => "i",
                    "warning" => "!",
                    "error" => "X",
                    _ => "?",
                };
                println!(
                    "[{icon}] example.rs:{}: {} - {} ({code})",
                    diag.line,
                    diag.column,
                    diag.message,
                    icon = icon,
                    code = diag.code
                );
            }
            Err(e) => {
                eprintln!("Error receiving diagnostic: {:?}", e);
                break;
            }
        }
    }

    println!("\n--- Summary ---");
    println!("Total diagnostics: {}", count);

    println!("\n=== Demo Complete ===");
}

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use rapace::transport::StreamTransport;
    use rapace_diagnostics_over_rapace::Diagnostic;
    use std::time::Duration;
    use tokio::net::{TcpListener, TcpStream};

    /// Helper to run diagnostics scenario with any transport.
    async fn run_scenario(
        host_transport: AnyTransport,
        cell_transport: AnyTransport,
        source: &str,
    ) -> Vec<Diagnostic> {
        // Cell side - use DiagnosticsServer::serve()
        let server = DiagnosticsServer::new(DiagnosticsImpl);
        let cell_handle = tokio::spawn(server.serve(cell_transport));

        // Host side - create RpcSession and DiagnosticsClient
        let session = Arc::new(RpcSession::new(host_transport));
        let session_clone = session.clone();
        tokio::spawn(async move { session_clone.run().await });
        let client = DiagnosticsClient::new(session.clone());

        let mut stream = client
            .analyze("test.rs".to_string(), source.as_bytes().to_vec())
            .await
            .expect("analyze call failed");

        // Collect all diagnostics
        let mut diagnostics = Vec::new();
        while let Some(result) = stream.next().await {
            match result {
                Ok(diag) => diagnostics.push(diag),
                Err(e) => panic!("Stream error: {:?}", e),
            }
        }

        // Cleanup
        tokio::time::sleep(Duration::from_millis(50)).await;
        session.close();
        cell_handle.abort();

        diagnostics
    }

    const TEST_SOURCE: &str = r#"
// Test file
fn main() {
    // TODO: first todo
    let x = 1;
    // FIXME: broken code
    let y = 2;
    // NOTE: just a note
    let z = 3;
    // TODO: second todo
}
"#;

    fn verify_diagnostics(diagnostics: &[Diagnostic]) {
        // Should have 4 diagnostics: 2 TODOs, 1 FIXME, 1 NOTE
        // TODO on line 4, FIXME on line 6, NOTE on line 8, TODO on line 10
        assert_eq!(
            diagnostics.len(),
            4,
            "Expected 4 diagnostics, got {:?}",
            diagnostics
        );

        // Verify order (should be in line order)
        assert_eq!(diagnostics[0].code, "TODO001");
        assert_eq!(diagnostics[0].line, 4);
        assert_eq!(diagnostics[0].severity, "warning");

        assert_eq!(diagnostics[1].code, "FIXME001");
        assert_eq!(diagnostics[1].line, 6);
        assert_eq!(diagnostics[1].severity, "error");

        assert_eq!(diagnostics[2].code, "NOTE001");
        assert_eq!(diagnostics[2].line, 8);
        assert_eq!(diagnostics[2].severity, "info");

        assert_eq!(diagnostics[3].code, "TODO001");
        assert_eq!(diagnostics[3].line, 10);
        assert_eq!(diagnostics[3].severity, "warning");
    }

    #[tokio_test_lite::test]
    async fn test_mem_transport() {
        let (host_transport, cell_transport) = AnyTransport::mem_pair();
        let diagnostics = run_scenario(host_transport, cell_transport, TEST_SOURCE).await;
        verify_diagnostics(&diagnostics);
    }

    #[tokio_test_lite::test]
    async fn test_stream_transport_tcp() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let accept_task = tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            AnyTransport::new(StreamTransport::new(stream))
        });

        let stream = TcpStream::connect(addr).await.unwrap();
        let host_transport = AnyTransport::new(StreamTransport::new(stream));

        let cell_transport = accept_task.await.unwrap();

        let diagnostics = run_scenario(host_transport, cell_transport, TEST_SOURCE).await;
        verify_diagnostics(&diagnostics);
    }

    #[cfg(unix)]
    #[tokio_test_lite::test]
    async fn test_shm_transport() {
        use rapace::transport::shm::ShmTransport;

        let (host_shm, cell_shm) = ShmTransport::hub_pair().expect("Failed to create hub pair");
        let host_transport = AnyTransport::new(host_shm);
        let cell_transport = AnyTransport::new(cell_shm);

        let diagnostics = run_scenario(host_transport, cell_transport, TEST_SOURCE).await;

        verify_diagnostics(&diagnostics);
    }

    #[tokio_test_lite::test]
    async fn test_empty_source() {
        let (host_transport, cell_transport) = AnyTransport::mem_pair();
        let diagnostics = run_scenario(host_transport, cell_transport, "").await;
        assert!(
            diagnostics.is_empty(),
            "Empty source should produce no diagnostics"
        );
    }

    #[tokio_test_lite::test]
    async fn test_no_issues() {
        let source = "fn main() {\n    println!(\"Hello\");\n}\n";
        let (host_transport, cell_transport) = AnyTransport::mem_pair();
        let diagnostics = run_scenario(host_transport, cell_transport, source).await;
        assert!(
            diagnostics.is_empty(),
            "Clean source should produce no diagnostics"
        );
    }

    #[tokio_test_lite::test]
    async fn test_large_file() {
        // Generate a large file with many TODOs
        let mut source = String::new();
        for i in 0..100 {
            source.push_str(&format!("// Line {} - TODO: item {}\n", i + 1, i));
        }

        let (host_transport, cell_transport) = AnyTransport::mem_pair();
        let diagnostics = run_scenario(host_transport, cell_transport, &source).await;

        assert_eq!(diagnostics.len(), 100, "Should have 100 diagnostics");

        // Verify they're in order
        for (i, diag) in diagnostics.iter().enumerate() {
            assert_eq!(diag.line, (i + 1) as u32);
            assert_eq!(diag.code, "TODO001");
        }
    }
}
