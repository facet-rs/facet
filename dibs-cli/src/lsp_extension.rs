//! LSP extension for Styx.
//!
//! When invoked with `dibs lsp-extension`, this provides domain-specific
//! intelligence (completions, hover, diagnostics) for dibs query files.

use roam_session::HandshakeConfig;
use roam_stream::CobsFramed;
use styx_lsp_ext::{
    Capability, CodeAction, CodeActionParams, CompletionItem, CompletionKind, CompletionParams,
    Diagnostic, DiagnosticParams, HoverParams, HoverResult, InlayHint, InlayHintParams,
    InitializeParams, InitializeResult, StyxLspExtension, StyxLspExtensionDispatcher,
    StyxLspHostClient,
};
use tokio::io::{stdin, stdout};
use tracing::{debug, info, warn};

/// Run the LSP extension, communicating over stdin/stdout.
pub async fn run() {
    // Set up logging to stderr (stdout is for roam protocol)
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("dibs=debug".parse().unwrap()),
        )
        .init();

    info!("dibs LSP extension starting");

    // Wrap stdin/stdout in COBS framing for roam
    let stdio = StdioStream::new();
    let framed = CobsFramed::new(stdio);

    // Accept the roam handshake (we're the responder)
    let handshake_config = HandshakeConfig::default();

    // Create a placeholder dispatcher - we'll set the real host client after handshake
    let extension = DibsExtension::new();
    let dispatcher = StyxLspExtensionDispatcher::new(extension);

    let (handle, _incoming, driver) =
        match roam_session::accept_framed(framed, handshake_config, dispatcher).await {
            Ok(result) => result,
            Err(e) => {
                warn!(error = %e, "Failed roam handshake");
                return;
            }
        };

    debug!("Roam session established");

    // The handle can be used to call back to the host via StyxLspHostClient
    let _host_client = StyxLspHostClient::new(handle);

    // Run the driver until the connection closes
    if let Err(e) = driver.run().await {
        warn!(error = %e, "Session driver error");
    }

    info!("dibs LSP extension shutting down");
}

/// Duplex stream over stdin/stdout.
struct StdioStream {
    stdin: tokio::io::Stdin,
    stdout: tokio::io::Stdout,
}

impl StdioStream {
    fn new() -> Self {
        Self {
            stdin: stdin(),
            stdout: stdout(),
        }
    }
}

impl tokio::io::AsyncRead for StdioStream {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.stdin).poll_read(cx, buf)
    }
}

impl tokio::io::AsyncWrite for StdioStream {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.stdout).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.stdout).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.stdout).poll_shutdown(cx)
    }
}

/// The dibs LSP extension implementation.
#[derive(Clone)]
struct DibsExtension {
    schema: dibs::Schema,
}

impl DibsExtension {
    fn new() -> Self {
        // Collect the schema from registered tables
        let schema = dibs::Schema::collect();
        Self { schema }
    }

    /// Get completions for table names.
    fn table_completions(&self, prefix: &str) -> Vec<CompletionItem> {
        self.schema
            .tables
            .iter()
            .filter(|t| t.name.starts_with(prefix) || prefix.is_empty())
            .map(|t| CompletionItem {
                label: t.name.clone(),
                detail: Some(format!("{} columns", t.columns.len())),
                documentation: t.doc.clone(),
                kind: Some(CompletionKind::Type),
                sort_text: None,
                insert_text: None,
            })
            .collect()
    }

    /// Get completions for column names of a specific table.
    fn column_completions(&self, table_name: &str, prefix: &str) -> Vec<CompletionItem> {
        let Some(table) = self.schema.tables.iter().find(|t| t.name == table_name) else {
            return Vec::new();
        };

        table
            .columns
            .iter()
            .filter(|c| c.name.starts_with(prefix) || prefix.is_empty())
            .map(|c| {
                let mut detail = c.pg_type.to_string();
                if c.primary_key {
                    detail.push_str(" PK");
                }
                if !c.nullable {
                    detail.push_str(" NOT NULL");
                }

                CompletionItem {
                    label: c.name.clone(),
                    detail: Some(detail),
                    documentation: None,
                    kind: Some(CompletionKind::Field),
                    sort_text: None,
                    insert_text: None,
                }
            })
            .collect()
    }
}

impl StyxLspExtension for DibsExtension {
    async fn initialize(&self, params: InitializeParams) -> InitializeResult {
        info!(
            schema_id = %params.schema_id,
            document_uri = %params.document_uri,
            "Initializing dibs extension"
        );

        InitializeResult {
            name: "dibs".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            capabilities: vec![
                Capability::Completions,
                Capability::Hover,
                Capability::Diagnostics,
            ],
        }
    }

    async fn completions(&self, params: CompletionParams) -> Vec<CompletionItem> {
        debug!(path = ?params.path, prefix = %params.prefix, "Completion request");

        // Determine what kind of completions to provide based on path
        // The path tells us where in the document tree we are
        // e.g., ["AllProducts", "@query", "from"] means we're at the "from" field

        if params.path.is_empty() {
            // At root level - no completions
            return Vec::new();
        }

        let last = params.path.last().map(|s| s.as_str()).unwrap_or("");

        match last {
            // Table references
            "from" | "table" | "join" => self.table_completions(&params.prefix),

            // Column references - need to know which table
            "select" | "where" | "order_by" | "group_by" => {
                // Try to find a "from" or "table" in the context to determine the table
                // For now, return all columns from all tables as a fallback
                let mut items = Vec::new();
                for table in &self.schema.tables {
                    items.extend(self.column_completions(&table.name, &params.prefix));
                }
                items
            }

            _ => Vec::new(),
        }
    }

    async fn hover(&self, params: HoverParams) -> Option<HoverResult> {
        debug!(path = ?params.path, "Hover request");

        // Try to provide hover info for table/column names
        if params.path.is_empty() {
            return None;
        }

        // Check if we're hovering over a table name
        let last = params.path.last()?;
        if let Some(table) = self.schema.tables.iter().find(|t| t.name == *last) {
            let mut content = format!("**Table `{}`**\n\n", table.name);

            if let Some(doc) = &table.doc {
                content.push_str(doc);
                content.push_str("\n\n");
            }

            content.push_str("| Column | Type | Constraints |\n");
            content.push_str("|--------|------|-------------|\n");

            for col in &table.columns {
                let mut constraints = Vec::new();
                if col.primary_key {
                    constraints.push("PK");
                }
                if col.unique {
                    constraints.push("UNIQUE");
                }
                if !col.nullable {
                    constraints.push("NOT NULL");
                }

                content.push_str(&format!(
                    "| {} | {} | {} |\n",
                    col.name,
                    col.pg_type,
                    constraints.join(", ")
                ));
            }

            return Some(HoverResult {
                contents: content,
                range: None,
            });
        }

        None
    }

    async fn inlay_hints(&self, _params: InlayHintParams) -> Vec<InlayHint> {
        // Not implemented yet
        Vec::new()
    }

    async fn diagnostics(&self, _params: DiagnosticParams) -> Vec<Diagnostic> {
        // Not implemented yet - could validate table/column references
        Vec::new()
    }

    async fn code_actions(&self, _params: CodeActionParams) -> Vec<CodeAction> {
        // Not implemented yet
        Vec::new()
    }

    async fn shutdown(&self) {
        info!("Shutdown requested");
    }
}
