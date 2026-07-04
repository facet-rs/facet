#![forbid(unsafe_code)]
//! Vix language server.
//!
//! Layering note: this server speaks to the Rust binder and parser today. The
//! vix/fable-hosted query engines can swap in behind the small internal
//! `LanguageQueries` trait later; that replacement is the self-hosting demo,
//! not a protocol rewrite.

use std::collections::BTreeMap;
use std::env;
use std::fmt;
use std::io::{BufRead, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use facet::Facet;
use facet_lsp::framing::{
    FrameError, IncomingMessage, RawNotificationMessage, ResponseMessage, RpcId, read_message,
    write_frame,
};
use facet_lsp::position::LineIndex;
use facet_lsp::semantic::{AbsoluteSemanticToken, encode_semantic_tokens};
use facet_lsp::types::{
    Diagnostic, DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DocumentHighlight, Hover, InitializeParams, InitializeResult, Location, MarkupContent,
    Position, PublishDiagnosticsParams, Range, ReferenceParams, SemanticTokens,
    SemanticTokensLegend, SemanticTokensOptions, SemanticTokensParams, ServerCapabilities,
    ServerInfo, TextDocumentPositionParams, TextEdit, WorkspaceEdit,
};
use tracing::{debug, info, warn};
use tracing_appender::rolling::{RollingFileAppender, Rotation};
use tracing_subscriber::prelude::*;
use vix::binder::{self, Bindings, SymbolId};
use vix::support::Span;
use vix::{ParseError, VixParser};

/// Run the server over framed JSON-RPC streams.
pub fn run_stdio() -> Result<(), ServerError> {
    init_tracing();
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut input = std::io::BufReader::new(stdin.lock());
    let mut output = stdout.lock();
    let mut server = Server::new(RustLanguageQueries::new());
    server.run(&mut input, &mut output)
}

/// Initialize compact tracing to stderr and a daily rolling log file.
pub fn init_tracing() {
    static INIT: OnceLock<()> = OnceLock::new();
    INIT.get_or_init(|| {
        let filter = tracing_subscriber::EnvFilter::try_from_env("VIX_LSP_LOG_LEVEL")
            .or_else(|_| tracing_subscriber::EnvFilter::try_from_env("VIX_LSP_LOG_FILTER"))
            .unwrap_or_else(|_| "info".into());
        let stderr_layer = tracing_subscriber::fmt::layer()
            .compact()
            .with_ansi(false)
            .with_writer(std::io::stderr);
        let mut file_error = None;
        let file_layer = match rolling_log_appender() {
            Ok(appender) => Some(
                tracing_subscriber::fmt::layer()
                    .compact()
                    .with_ansi(false)
                    .with_writer(appender),
            ),
            Err(err) => {
                file_error = Some(err.to_string());
                None
            }
        };

        let subscriber = tracing_subscriber::registry()
            .with(filter)
            .with(stderr_layer)
            .with(file_layer);
        if tracing::subscriber::set_global_default(subscriber).is_ok()
            && let Some(err) = file_error
        {
            warn!(error = %err, "failed to initialize vix-lsp file logging");
        }
    });
}

fn rolling_log_appender() -> Result<RollingFileAppender, tracing_appender::rolling::InitError> {
    RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("vix-lsp.log")
        .max_log_files(log_retention())
        .build(log_dir())
}

fn log_dir() -> PathBuf {
    if let Some(dir) = env::var_os("VIX_LSP_LOG_DIR") {
        return PathBuf::from(dir);
    }
    if let Some(path) = env::var_os("VIX_LSP_LOG") {
        let path = PathBuf::from(path);
        return if path.extension().is_some() {
            path.parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .unwrap_or_else(|| Path::new("."))
                .to_path_buf()
        } else {
            path
        };
    }
    PathBuf::from("/tmp/vix-lsp")
}

fn log_retention() -> usize {
    env::var("VIX_LSP_LOG_RETENTION")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(7)
}

/// Server errors.
#[derive(Debug)]
pub enum ServerError {
    /// Framing or JSON failed.
    Frame(FrameError),
    /// Method dispatch failed.
    Dispatch(String),
}

impl fmt::Display for ServerError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Frame(err) => write!(f, "{err}"),
            Self::Dispatch(err) => f.write_str(err),
        }
    }
}

impl std::error::Error for ServerError {}

impl From<FrameError> for ServerError {
    fn from(value: FrameError) -> Self {
        Self::Frame(value)
    }
}

/// Internal query interface for the current Rust implementation and future
/// hosted engines.
#[doc(hidden)]
pub trait LanguageQueries {
    fn analyze(&mut self, source: &str) -> Result<Analysis, String>;
    fn highlights(&mut self, source: &str) -> Result<Vec<Highlight>, String>;
}

/// Rust-backed Vix queries.
pub struct RustLanguageQueries {
    parser: VixParser,
}

impl RustLanguageQueries {
    /// Construct Rust-backed queries.
    pub fn new() -> Self {
        Self {
            parser: VixParser::new(),
        }
    }
}

impl Default for RustLanguageQueries {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageQueries for RustLanguageQueries {
    fn analyze(&mut self, source: &str) -> Result<Analysis, String> {
        let file = self
            .parser
            .parse(source)
            .map_err(|err| parse_message(&err))?;
        Ok(Analysis {
            bindings: binder::bind(&file),
        })
    }

    fn highlights(&mut self, source: &str) -> Result<Vec<Highlight>, String> {
        self.parser
            .highlights(source)
            .map_err(|err| parse_message(&err))?
            .into_iter()
            .map(|(capture, start, end)| {
                Ok(Highlight {
                    capture,
                    span: Span { start, end },
                })
            })
            .collect()
    }
}

fn parse_message(err: &ParseError) -> String {
    err.message.clone()
}

#[doc(hidden)]
pub struct Analysis {
    bindings: Bindings,
}

#[doc(hidden)]
#[derive(Clone, Debug)]
pub struct Highlight {
    capture: String,
    span: Span,
}

#[derive(Clone, Debug)]
struct Document {
    text: String,
    index: LineIndex,
}

impl Document {
    fn new(text: String) -> Self {
        let index = LineIndex::new(&text);
        Self { text, index }
    }
}

/// Stateful Vix LSP server.
pub struct Server<Q> {
    queries: Q,
    documents: BTreeMap<String, Document>,
}

impl<Q: LanguageQueries> Server<Q> {
    /// Construct a server from a query implementation.
    pub fn new(queries: Q) -> Self {
        Self {
            queries,
            documents: BTreeMap::new(),
        }
    }

    /// Process framed input until EOF.
    pub fn run(
        &mut self,
        input: &mut impl BufRead,
        output: &mut impl Write,
    ) -> Result<(), ServerError> {
        while let Some(message) = read_message(input)? {
            let frames = self.handle_message(message)?;
            for frame in frames {
                write_frame(output, &frame)?;
            }
        }
        Ok(())
    }

    /// Handle one decoded JSON-RPC message and return outgoing messages.
    pub fn handle_message(
        &mut self,
        message: IncomingMessage,
    ) -> Result<Vec<Outgoing>, ServerError> {
        debug!(
            method = message.method,
            has_id = message.id.is_some(),
            "handling message"
        );
        match message.method.as_str() {
            "initialize" => {
                let id = message.id.clone();
                let _params: InitializeParams = decode_params(&message)?;
                Self::respond(id, Ok(initialize_result()))
            }
            "initialized" => {
                let _params: facet_lsp::types::InitializedParams = decode_params(&message)?;
                info!("initialized");
                Ok(Vec::new())
            }
            "textDocument/didOpen" => {
                let params: DidOpenTextDocumentParams = decode_params(&message)?;
                let uri = params.text_document.uri;
                let text = params.text_document.text;
                self.documents.insert(uri.clone(), Document::new(text));
                Ok(self.publish_diagnostics(uri))
            }
            "textDocument/didChange" => {
                let params: DidChangeTextDocumentParams = decode_params(&message)?;
                let uri = params.text_document.uri;
                if let Some(change) = params.content_changes.into_iter().last() {
                    self.documents
                        .insert(uri.clone(), Document::new(change.text));
                }
                Ok(self.publish_diagnostics(uri))
            }
            "textDocument/didClose" => {
                let params: DidCloseTextDocumentParams = decode_params(&message)?;
                self.documents.remove(&params.text_document.uri);
                Ok(Vec::new())
            }
            "textDocument/definition" => {
                let id = message.id.clone();
                let params: TextDocumentPositionParams = decode_params(&message)?;
                Self::respond(id, self.definition(params))
            }
            "textDocument/references" => {
                let id = message.id.clone();
                let params: ReferenceParams = decode_params(&message)?;
                Self::respond(id, self.references(params))
            }
            "textDocument/documentHighlight" => {
                let id = message.id.clone();
                let params: TextDocumentPositionParams = decode_params(&message)?;
                Self::respond(id, self.document_highlight(params))
            }
            "textDocument/rename" => {
                let id = message.id.clone();
                let params: facet_lsp::types::RenameParams = decode_params(&message)?;
                Self::respond(id, self.rename(params))
            }
            "textDocument/hover" => {
                let id = message.id.clone();
                let params: TextDocumentPositionParams = decode_params(&message)?;
                Self::respond(id, self.hover(params))
            }
            "textDocument/semanticTokens/full" => {
                let id = message.id.clone();
                let params: SemanticTokensParams = decode_params(&message)?;
                Self::respond(id, self.semantic_tokens(params))
            }
            method => {
                warn!(method, "unsupported method");
                if let Some(id) = message.id {
                    Ok(vec![Outgoing::Response(ResponseMessage::error(
                        id,
                        -32601,
                        format!("method not found: {method}"),
                    ))])
                } else {
                    Ok(Vec::new())
                }
            }
        }
    }

    fn respond<T: Facet<'static>>(
        id: Option<RpcId>,
        result: Result<T, ServerError>,
    ) -> Result<Vec<Outgoing>, ServerError> {
        let Some(id) = id else {
            return Err(ServerError::Dispatch(
                "request method was missing JSON-RPC id".to_owned(),
            ));
        };
        match result {
            Ok(result) => Ok(vec![Outgoing::Response(ResponseMessage::result(
                id, &result,
            )?)]),
            Err(err) => Ok(vec![Outgoing::Response(ResponseMessage::error(
                id,
                -32603,
                err.to_string(),
            ))]),
        }
    }

    fn publish_diagnostics(&mut self, uri: String) -> Vec<Outgoing> {
        let diagnostics = self
            .documents
            .get(&uri)
            .map(|doc| diagnostics_for(&mut self.queries, doc))
            .unwrap_or_default();
        let params = PublishDiagnosticsParams { uri, diagnostics };
        match RawNotificationMessage::typed("textDocument/publishDiagnostics", &params) {
            Ok(notification) => vec![Outgoing::Notification(notification)],
            Err(err) => {
                warn!(error = %err, "could not encode diagnostics notification");
                Vec::new()
            }
        }
    }

    fn document(&self, uri: &str) -> Result<&Document, ServerError> {
        self.documents
            .get(uri)
            .ok_or_else(|| ServerError::Dispatch(format!("document not open: {uri}")))
    }

    fn symbol_at(
        &mut self,
        uri: &str,
        position: Position,
    ) -> Result<(SymbolId, Analysis), ServerError> {
        let doc = self.document(uri)?;
        let offset = doc.index.position_to_offset(position).ok_or_else(|| {
            ServerError::Dispatch(format!(
                "position is not a valid UTF-16 boundary: {position:?}"
            ))
        })?;
        let source = doc.text.clone();
        let analysis = self
            .queries
            .analyze(&source)
            .map_err(ServerError::Dispatch)?;
        let symbol = analysis.bindings.symbol_at(offset).ok_or_else(|| {
            ServerError::Dispatch(format!("no Vix symbol at byte offset {offset}"))
        })?;
        Ok((symbol, analysis))
    }

    fn definition(
        &mut self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<Location>, ServerError> {
        let uri = params.text_document.uri;
        let (symbol, analysis) = self.symbol_at(&uri, params.position)?;
        let def = analysis.bindings.symbol(symbol).def;
        let doc = self.document(&uri)?;
        Ok(Some(Location {
            uri,
            range: span_range(doc, def)?,
        }))
    }

    fn references(&mut self, params: ReferenceParams) -> Result<Vec<Location>, ServerError> {
        let uri = params.text_document.uri;
        let include_decl = params.context.include_declaration;
        let (symbol, analysis) = self.symbol_at(&uri, params.position)?;
        let spans = if include_decl {
            analysis.bindings.occurrences(symbol)
        } else {
            analysis.bindings.references(symbol)
        };
        let doc = self.document(&uri)?;
        spans
            .into_iter()
            .map(|span| {
                Ok(Location {
                    uri: uri.clone(),
                    range: span_range(doc, span)?,
                })
            })
            .collect()
    }

    fn document_highlight(
        &mut self,
        params: TextDocumentPositionParams,
    ) -> Result<Vec<DocumentHighlight>, ServerError> {
        let uri = params.text_document.uri;
        let (symbol, analysis) = self.symbol_at(&uri, params.position)?;
        let doc = self.document(&uri)?;
        analysis
            .bindings
            .occurrences(symbol)
            .into_iter()
            .map(|span| {
                Ok(DocumentHighlight {
                    range: span_range(doc, span)?,
                    kind: Some(1),
                })
            })
            .collect()
    }

    fn rename(
        &mut self,
        params: facet_lsp::types::RenameParams,
    ) -> Result<WorkspaceEdit, ServerError> {
        let uri = params.text_document.uri;
        let (symbol, analysis) = self.symbol_at(&uri, params.position)?;
        let doc = self.document(&uri)?;
        let edits = analysis
            .bindings
            .rename_edits(symbol, &params.new_name)
            .into_iter()
            .map(|(span, new_text)| {
                Ok(TextEdit {
                    range: span_range(doc, span)?,
                    new_text,
                })
            })
            .collect::<Result<Vec<_>, ServerError>>()?;
        let mut changes = BTreeMap::new();
        changes.insert(uri, edits);
        Ok(WorkspaceEdit { changes })
    }

    fn hover(&mut self, params: TextDocumentPositionParams) -> Result<Option<Hover>, ServerError> {
        let uri = params.text_document.uri;
        let (symbol, analysis) = self.symbol_at(&uri, params.position)?;
        let doc = self.document(&uri)?;
        let symbol = analysis.bindings.symbol(symbol);
        Ok(Some(Hover {
            contents: MarkupContent {
                kind: "plaintext".to_owned(),
                value: format!("{:?} {}", symbol.kind, symbol.name),
            },
            range: Some(span_range(doc, symbol.def)?),
        }))
    }

    fn semantic_tokens(
        &mut self,
        params: SemanticTokensParams,
    ) -> Result<SemanticTokens, ServerError> {
        let uri = params.text_document.uri;
        let doc = self.document(&uri)?;
        let source = doc.text.clone();
        let highlights = self
            .queries
            .highlights(&source)
            .map_err(ServerError::Dispatch)?;
        let doc = self.document(&uri)?;
        let mut tokens = Vec::new();
        for highlight in highlights {
            let Some(mapping) = semantic_mapping(&highlight.capture) else {
                continue;
            };
            let Some(range) = doc.index.range(highlight.span.start, highlight.span.end) else {
                continue;
            };
            let Some(length) = doc
                .index
                .utf16_len(highlight.span.start, highlight.span.end)
            else {
                continue;
            };
            tokens.push(AbsoluteSemanticToken {
                line: range.start.line,
                start_character: range.start.character,
                length,
                token_type: mapping.token_type,
                token_modifiers: mapping.modifiers,
            });
        }
        Ok(SemanticTokens {
            data: encode_semantic_tokens(&tokens),
        })
    }
}

fn decode_params<T: Facet<'static>>(message: &IncomingMessage) -> Result<T, ServerError> {
    let params = message
        .params
        .as_ref()
        .ok_or_else(|| ServerError::Dispatch(format!("missing params for {}", message.method)))?;
    facet_json::from_str(params.as_str()).map_err(|err| ServerError::Dispatch(err.to_string()))
}

fn diagnostics_for(queries: &mut impl LanguageQueries, doc: &Document) -> Vec<Diagnostic> {
    match queries.analyze(&doc.text) {
        Ok(_) => Vec::new(),
        Err(message) => vec![Diagnostic {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: 0,
                    character: 0,
                },
            },
            severity: Some(1),
            source: Some("vix".to_owned()),
            message,
        }],
    }
}

fn span_range(doc: &Document, span: Span) -> Result<Range, ServerError> {
    doc.index
        .range(span.start, span.end)
        .ok_or_else(|| ServerError::Dispatch(format!("invalid span: {span:?}")))
}

fn initialize_result() -> InitializeResult {
    InitializeResult {
        capabilities: ServerCapabilities {
            text_document_sync: 1,
            definition_provider: true,
            references_provider: true,
            document_highlight_provider: true,
            rename_provider: true,
            hover_provider: true,
            semantic_tokens_provider: SemanticTokensOptions {
                legend: SemanticTokensLegend {
                    token_types: TOKEN_TYPES.iter().map(|s| (*s).to_owned()).collect(),
                    token_modifiers: TOKEN_MODIFIERS.iter().map(|s| (*s).to_owned()).collect(),
                },
                full: true,
            },
        },
        server_info: ServerInfo {
            name: "vix-lsp".to_owned(),
            version: Some(env!("CARGO_PKG_VERSION").to_owned()),
        },
    }
}

const TOKEN_TYPES: &[&str] = &[
    "keyword",
    "function",
    "method",
    "parameter",
    "variable",
    "property",
    "type",
    "enumMember",
    "string",
    "number",
    "comment",
    "operator",
];

const TOKEN_MODIFIERS: &[&str] = &[
    "declaration",
    "documentation",
    "builtin",
    "special",
    "macro",
];

#[derive(Clone, Copy)]
struct TokenMapping {
    token_type: u32,
    modifiers: u32,
}

const fn modifier_bit(index: u32) -> u32 {
    1 << index
}

fn semantic_mapping(capture: &str) -> Option<TokenMapping> {
    let (token_type, modifiers) = match capture {
        "keyword" => (0, 0),
        "function" => (1, modifier_bit(0)),
        "function.call" => (1, 0),
        "function.method" => (2, 0),
        "variable.parameter" => (3, 0),
        "property" => (5, 0),
        "type" => (6, 0),
        "constructor" => (7, 0),
        "string" => (8, 0),
        "string.special" => (8, modifier_bit(3)),
        "string.special.path" => (8, modifier_bit(3)),
        "number" => (9, 0),
        "comment" => (10, 0),
        "comment.documentation" => (10, modifier_bit(1)),
        "operator" => (11, 0),
        "constant" => (4, 0),
        "constant.builtin" => (4, modifier_bit(2)),
        "function.macro" => (1, modifier_bit(4)),
        _ => return None,
    };
    Some(TokenMapping {
        token_type,
        modifiers,
    })
}

/// Outgoing LSP message.
#[derive(Debug, Facet)]
#[facet(untagged)]
#[repr(u8)]
pub enum Outgoing {
    /// Response message.
    Response(ResponseMessage),
    /// Notification message.
    Notification(RawNotificationMessage),
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FakeQueries;

    impl LanguageQueries for FakeQueries {
        fn analyze(&mut self, _source: &str) -> Result<Analysis, String> {
            Err("not used".to_owned())
        }

        fn highlights(&mut self, _source: &str) -> Result<Vec<Highlight>, String> {
            Ok(vec![
                Highlight {
                    capture: "keyword".to_owned(),
                    span: Span { start: 0, end: 2 },
                },
                Highlight {
                    capture: "string.special.path".to_owned(),
                    span: Span { start: 5, end: 10 },
                },
                Highlight {
                    capture: "function".to_owned(),
                    span: Span { start: 3, end: 4 },
                },
            ])
        }
    }

    #[test]
    fn semantic_tokens_encode_multiline_vix_captures() {
        let mut server = Server::new(FakeQueries);
        let uri = "file:///test.vix".to_owned();
        server
            .documents
            .insert(uri.clone(), Document::new("fn f\np\"é\"".to_owned()));
        let got = server
            .semantic_tokens(SemanticTokensParams {
                text_document: facet_lsp::types::TextDocumentIdentifier { uri },
            })
            .expect("tokens");
        assert_eq!(
            got.data,
            vec![
                0, 0, 2, 0, 0, // fn
                0, 3, 1, 1, 1, // f
                1, 0, 4, 8, 8, // p"é", length is 4 UTF-16 units
            ]
        );
    }
}
