//! Diagnostics over Rapace - Streaming RPC Example
//!
//! This example demonstrates server-streaming RPC where:
//! - The **host** sends a source file to analyze
//! - The **plugin** streams back diagnostic findings (lints, warnings, errors)
//!
//! This pattern is useful for IDE integration, build systems, and linter tools
//! where a plugin can provide detailed analysis streamed incrementally.

use rapace::Streaming;

// ============================================================================
// Facet Types
// ============================================================================

/// A single diagnostic finding from code analysis.
#[derive(Debug, Clone, PartialEq, Eq, facet::Facet)]
pub struct Diagnostic {
    /// Diagnostic code (e.g., "TODO001", "FIXME001")
    pub code: String,
    /// Human-readable message
    pub message: String,
    /// Severity: "info", "warning", "error"
    pub severity: String,
    /// 1-based line number
    pub line: u32,
    /// 1-based column number
    pub column: u32,
}

// ============================================================================
// Diagnostics Service
// ============================================================================

/// Service for analyzing source code and streaming back diagnostics.
///
/// The plugin implements this, the host calls it via RPC.
#[allow(async_fn_in_trait)]
#[rapace::service]
pub trait Diagnostics {
    /// Analyze a source file and stream back diagnostics.
    ///
    /// # Arguments
    /// * `path` - Path to the file (for context in diagnostics)
    /// * `contents` - The source file contents as bytes (UTF-8)
    ///
    /// # Returns
    /// A stream of diagnostic findings
    async fn analyze(&self, path: String, contents: Vec<u8>) -> Streaming<crate::Diagnostic>;
}

// ============================================================================
// Plugin Side: DiagnosticsImpl
// ============================================================================

/// Plugin-side implementation of the Diagnostics service.
///
/// This is a simple analyzer that:
/// - Reports INFO for lines containing "NOTE"
/// - Reports WARNING for lines containing "TODO"
/// - Reports ERROR for lines containing "FIXME"
#[derive(Clone)]
pub struct DiagnosticsImpl;

impl Diagnostics for DiagnosticsImpl {
    async fn analyze(&self, path: String, contents: Vec<u8>) -> Streaming<Diagnostic> {
        let (tx, rx) = tokio::sync::mpsc::channel(16);

        // Parse contents as UTF-8 and convert to owned String
        let source = String::from_utf8_lossy(&contents).into_owned();

        // Spawn task to emit diagnostics
        tokio::spawn(async move {
            for (line_idx, line) in source.lines().enumerate() {
                let line_num = (line_idx + 1) as u32;

                // Check for NOTE
                if let Some(col) = line.find("NOTE") {
                    let diag = Diagnostic {
                        code: "NOTE001".to_string(),
                        message: format!("Note comment found in {}", path),
                        severity: "info".to_string(),
                        line: line_num,
                        column: (col + 1) as u32,
                    };
                    if tx.send(Ok(diag)).await.is_err() {
                        return; // Client disconnected
                    }
                }

                // Check for TODO
                if let Some(col) = line.find("TODO") {
                    let diag = Diagnostic {
                        code: "TODO001".to_string(),
                        message: format!("TODO comment found in {}", path),
                        severity: "warning".to_string(),
                        line: line_num,
                        column: (col + 1) as u32,
                    };
                    if tx.send(Ok(diag)).await.is_err() {
                        return;
                    }
                }

                // Check for FIXME
                if let Some(col) = line.find("FIXME") {
                    let diag = Diagnostic {
                        code: "FIXME001".to_string(),
                        message: format!("FIXME comment found in {}", path),
                        severity: "error".to_string(),
                        line: line_num,
                        column: (col + 1) as u32,
                    };
                    if tx.send(Ok(diag)).await.is_err() {
                        return;
                    }
                }
            }
            // Stream ends when tx is dropped
        });

        Box::pin(tokio_stream::wrappers::ReceiverStream::new(rx))
    }
}
