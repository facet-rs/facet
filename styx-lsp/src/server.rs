//! LSP server implementation

use std::collections::HashMap;
use std::sync::Arc;

use styx_cst::{Parse, parse};
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use crate::semantic_tokens::{compute_semantic_tokens, semantic_token_legend};

/// Document state tracked by the server
struct DocumentState {
    /// Document content
    content: String,
    /// Parsed CST
    parse: Parse,
    /// Document version
    version: i32,
}

/// The Styx language server
pub struct StyxLanguageServer {
    /// LSP client for sending notifications
    client: Client,
    /// Open documents
    documents: Arc<RwLock<HashMap<Url, DocumentState>>>,
}

impl StyxLanguageServer {
    pub fn new(client: Client) -> Self {
        Self {
            client,
            documents: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Publish diagnostics for a document
    async fn publish_diagnostics(&self, uri: Url, content: &str, parsed: &Parse, version: i32) {
        let diagnostics = self.compute_diagnostics(content, parsed);
        self.client
            .publish_diagnostics(uri, diagnostics, Some(version))
            .await;
    }

    /// Compute diagnostics for document content
    fn compute_diagnostics(&self, content: &str, parsed: &Parse) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Convert parse errors to diagnostics
        for error in parsed.errors() {
            let range = Range {
                start: offset_to_position(content, error.offset as usize),
                end: offset_to_position(content, error.offset as usize + 1),
            };

            diagnostics.push(Diagnostic {
                range,
                severity: Some(DiagnosticSeverity::ERROR),
                code: None,
                code_description: None,
                source: Some("styx".to_string()),
                message: error.message.clone(),
                related_information: None,
                tags: None,
                data: None,
            });
        }

        // Run validation for semantic errors (duplicate keys, mixed separators)
        let validation_diagnostics = styx_cst::validate(&parsed.syntax());
        for diag in validation_diagnostics {
            let range = Range {
                start: offset_to_position(content, diag.range.start().into()),
                end: offset_to_position(content, diag.range.end().into()),
            };

            let severity = match diag.severity {
                styx_cst::Severity::Error => DiagnosticSeverity::ERROR,
                styx_cst::Severity::Warning => DiagnosticSeverity::WARNING,
                styx_cst::Severity::Hint => DiagnosticSeverity::HINT,
            };

            diagnostics.push(Diagnostic {
                range,
                severity: Some(severity),
                code: None,
                code_description: None,
                source: Some("styx".to_string()),
                message: diag.message,
                related_information: None,
                tags: None,
                data: None,
            });
        }

        diagnostics
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for StyxLanguageServer {
    async fn initialize(&self, _: InitializeParams) -> Result<InitializeResult> {
        Ok(InitializeResult {
            capabilities: ServerCapabilities {
                // Full document sync - we get the whole document on each change
                text_document_sync: Some(TextDocumentSyncCapability::Kind(
                    TextDocumentSyncKind::FULL,
                )),
                // Semantic tokens for highlighting
                semantic_tokens_provider: Some(
                    SemanticTokensServerCapabilities::SemanticTokensOptions(
                        SemanticTokensOptions {
                            work_done_progress_options: WorkDoneProgressOptions::default(),
                            legend: semantic_token_legend(),
                            range: Some(false),
                            full: Some(SemanticTokensFullOptions::Bool(true)),
                        },
                    ),
                ),
                ..Default::default()
            },
            server_info: Some(ServerInfo {
                name: "styx-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _: InitializedParams) {
        self.client
            .log_message(MessageType::INFO, "Styx language server initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let content = params.text_document.text;
        let version = params.text_document.version;

        // Parse the document
        let parsed = parse(&content);

        // Publish diagnostics
        self.publish_diagnostics(uri.clone(), &content, &parsed, version)
            .await;

        // Store document
        {
            let mut docs = self.documents.write().await;
            docs.insert(
                uri,
                DocumentState {
                    content,
                    parse: parsed,
                    version,
                },
            );
        }
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;

        // With FULL sync, we get the entire document content
        if let Some(change) = params.content_changes.into_iter().next() {
            let content = change.text;

            // Parse the document
            let parsed = parse(&content);

            // Publish diagnostics
            self.publish_diagnostics(uri.clone(), &content, &parsed, version)
                .await;

            // Update stored document
            {
                let mut docs = self.documents.write().await;
                docs.insert(
                    uri,
                    DocumentState {
                        content,
                        parse: parsed,
                        version,
                    },
                );
            }
        }
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;

        // Remove document
        {
            let mut docs = self.documents.write().await;
            docs.remove(&uri);
        }

        // Clear diagnostics
        self.client.publish_diagnostics(uri, vec![], None).await;
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let uri = params.text_document.uri;

        let docs = self.documents.read().await;
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };

        let tokens = compute_semantic_tokens(&doc.parse);

        Ok(Some(SemanticTokensResult::Tokens(SemanticTokens {
            result_id: None,
            data: tokens,
        })))
    }
}

/// Convert byte offset to LSP Position
fn offset_to_position(content: &str, offset: usize) -> Position {
    let mut line = 0u32;
    let mut col = 0u32;

    for (i, ch) in content.char_indices() {
        if i >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }

    Position::new(line, col)
}

/// Run the LSP server on stdin/stdout
pub async fn run() -> eyre::Result<()> {
    // Set up logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with_writer(std::io::stderr)
        .init();

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(StyxLanguageServer::new);
    Server::new(stdin, stdout, socket).serve(service).await;

    Ok(())
}
