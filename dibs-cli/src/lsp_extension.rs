//! LSP extension for Styx.
//!
//! When invoked with `dibs lsp-extension`, this provides domain-specific
//! intelligence (completions, hover, diagnostics) for dibs query files.
//!
//! This connects to the user's db crate service (same as the TUI) to fetch
//! the actual schema, rather than using dummy tables.

use crate::config;
use crate::service::{self, ServiceConnection};
use dibs_proto::{SchemaInfo, TableInfo};
use roam_session::HandshakeConfig;
use roam_stream::CobsFramed;
use std::path::Path;
use std::sync::Arc;
use styx_lsp_ext::{
    Capability, CodeAction, CodeActionParams, CompletionItem, CompletionKind, CompletionParams,
    DefinitionParams, Diagnostic, DiagnosticParams, DiagnosticSeverity, HoverParams, HoverResult,
    InitializeParams, InitializeResult, InlayHint, InlayHintKind, InlayHintParams, Location,
    OffsetToPositionParams, Position, Range, StyxLspExtension, StyxLspExtensionDispatcher,
    StyxLspHostClient,
};
use tokio::io::{stdin, stdout};
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

/// Run the LSP extension, communicating over stdin/stdout.
pub async fn run() {
    // Set up logging to stderr (stdout is for roam protocol)
    // Use plain format without ANSI colors since this goes to editor logs
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .with_ansi(false)
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

    // Create the extension - we'll set the host client after handshake
    let extension = DibsExtension::new();
    let dispatcher = StyxLspExtensionDispatcher::new(extension.clone());

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
    let host_client = StyxLspHostClient::new(handle);

    // Store the host client in the extension so it can call back for offset_to_position
    extension.set_host(host_client).await;

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

/// Internal state that gets populated during initialize.
struct ExtensionState {
    /// The schema fetched from the service.
    schema: SchemaInfo,
    /// The service connection (kept alive).
    #[allow(dead_code)]
    connection: ServiceConnection,
}

/// The dibs LSP extension implementation.
#[derive(Clone)]
struct DibsExtension {
    /// State populated during initialize. None until then.
    state: Arc<RwLock<Option<ExtensionState>>>,
    /// The host client for calling back to the LSP.
    host: Arc<RwLock<Option<StyxLspHostClient>>>,
}

impl DibsExtension {
    fn new() -> Self {
        Self {
            state: Arc::new(RwLock::new(None)),
            host: Arc::new(RwLock::new(None)),
        }
    }

    /// Set the host client after handshake.
    async fn set_host(&self, host: StyxLspHostClient) {
        *self.host.write().await = Some(host);
    }

    /// Convert a byte offset to a Position using the host's offset_to_position.
    async fn offset_to_position(&self, document_uri: &str, offset: u32) -> Position {
        let host = self.host.read().await;
        if let Some(host) = host.as_ref()
            && let Ok(Some(pos)) = host
                .offset_to_position(OffsetToPositionParams {
                    document_uri: document_uri.to_string(),
                    offset,
                })
                .await
        {
            return pos;
        }
        // Fallback: use offset as character on line 0
        Position {
            line: 0,
            character: offset,
        }
    }

    /// Convert a styx span to an LSP range using proper line numbers.
    async fn span_to_range(&self, document_uri: &str, span: &styx_tree::Span) -> Range {
        let start = self.offset_to_position(document_uri, span.start).await;
        let end = self.offset_to_position(document_uri, span.end).await;
        Range { start, end }
    }

    /// Find a $param reference at the given cursor offset.
    /// Returns the param name (without the $) if found.
    fn find_param_at_offset(&self, value: &styx_tree::Value, offset: usize) -> Option<String> {
        // Check if this value is a scalar that looks like $param
        if let Some(text) = value.as_str() {
            if let Some(span) = &value.span {
                let start = span.start as usize;
                let end = span.end as usize;
                if offset >= start && offset <= end {
                    if let Some(param_name) = text.strip_prefix('$') {
                        return Some(param_name.to_string());
                    }
                }
            }
        }

        // Recurse into object entries
        if let Some(styx_tree::Payload::Object(obj)) = &value.payload {
            for entry in &obj.entries {
                // Check the value of each entry
                if let Some(name) = self.find_param_at_offset(&entry.value, offset) {
                    return Some(name);
                }
            }
        }

        // Recurse into sequences
        if let Some(styx_tree::Payload::Sequence(seq)) = &value.payload {
            for item in &seq.items {
                if let Some(name) = self.find_param_at_offset(item, offset) {
                    return Some(name);
                }
            }
        }

        None
    }

    /// Collect all $param references in a value tree.
    fn collect_param_refs(&self, value: &styx_tree::Value) -> Vec<String> {
        let mut params = Vec::new();
        self.collect_param_refs_inner(value, &mut params);
        params
    }

    fn collect_param_refs_inner(&self, value: &styx_tree::Value, params: &mut Vec<String>) {
        // Check if this value is a scalar that looks like $param
        if let Some(text) = value.as_str() {
            if let Some(param_name) = text.strip_prefix('$') {
                params.push(param_name.to_string());
            }
        }

        // Recurse into object entries (but skip the "params" block itself)
        if let Some(styx_tree::Payload::Object(obj)) = &value.payload {
            for entry in &obj.entries {
                // Skip the params declaration block
                if entry.key.as_str() != Some("params") {
                    self.collect_param_refs_inner(&entry.key, params);
                    self.collect_param_refs_inner(&entry.value, params);
                }
            }
        }

        // Recurse into sequences
        if let Some(styx_tree::Payload::Sequence(seq)) = &value.payload {
            for item in &seq.items {
                self.collect_param_refs_inner(item, params);
            }
        }
    }

    /// Get the schema, returning an empty schema if not initialized.
    async fn schema(&self) -> SchemaInfo {
        let state = self.state.read().await;
        state
            .as_ref()
            .map(|s| s.schema.clone())
            .unwrap_or_else(|| SchemaInfo { tables: vec![] })
    }

    /// Get completions for table names.
    async fn table_completions(&self, prefix: &str) -> Vec<CompletionItem> {
        let schema = self.schema().await;
        schema
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

    /// Collect diagnostics from a value tree.
    async fn collect_diagnostics(
        &self,
        document_uri: &str,
        value: &styx_tree::Value,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        let schema = self.schema().await;

        // Handle tagged objects (@query, @update, @delete, @insert, @upsert)
        // Note: @rel blocks are handled separately by lint_relation()
        if let Some(tag) = &value.tag {
            let tag_name = tag.name.as_str();

            // Skip @rel - handled by lint_relation() called from parent
            if tag_name == "rel" {
                return;
            }

            if let Some(styx_tree::Payload::Object(obj)) = &value.payload {
                // Collect info about the query structure
                let mut has_limit = false;
                let mut has_offset = false;
                let mut has_order_by = false;
                let mut has_where = false;
                let mut has_first = false;
                let mut offset_span: Option<&styx_tree::Span> = None;
                let mut limit_span: Option<&styx_tree::Span> = None;
                let mut first_span: Option<&styx_tree::Span> = None;
                let mut declared_params: Vec<(String, Option<String>, Option<styx_tree::Span>)> =
                    Vec::new(); // (name, type, span)
                let mut table_name = None;

                for entry in &obj.entries {
                    let key = entry.key.as_str().unwrap_or("");
                    match key {
                        "limit" => {
                            has_limit = true;
                            limit_span = entry.key.span.as_ref();
                        }
                        "offset" => {
                            has_offset = true;
                            offset_span = entry.key.span.as_ref();
                        }
                        "order-by" => has_order_by = true,
                        "where" => has_where = true,
                        "first" => {
                            has_first = true;
                            first_span = entry.key.span.as_ref();
                        }
                        "from" | "into" | "table" => {
                            if let Some(name) = entry.value.as_str() {
                                if !schema.tables.iter().any(|t| t.name == name) {
                                    if let Some(span) = &entry.value.span {
                                        diagnostics.push(Diagnostic {
                                            range: self.span_to_range(document_uri, span).await,
                                            severity: DiagnosticSeverity::Error,
                                            message: format!("Unknown table '{}'", name),
                                            source: Some("dibs".to_string()),
                                            code: Some("unknown-table".to_string()),
                                            data: None,
                                        });
                                    }
                                } else {
                                    table_name = Some(name.to_string());
                                }
                            }
                        }
                        "params" => {
                            // Collect declared param names and types
                            if let Some(styx_tree::Payload::Object(params_obj)) =
                                &entry.value.payload
                            {
                                for param_entry in &params_obj.entries {
                                    if let Some(name) = param_entry.key.as_str() {
                                        // Get the type tag (e.g., @string, @int)
                                        let param_type =
                                            param_entry.value.tag.as_ref().map(|t| t.name.clone());
                                        let span = param_entry.key.span.clone();
                                        declared_params.push((name.to_string(), param_type, span));
                                    }
                                }
                            }
                        }
                        _ => {}
                    }
                }

                // Lint: OFFSET without LIMIT
                if has_offset && !has_limit {
                    if let Some(span) = offset_span {
                        diagnostics.push(Diagnostic {
                            range: self.span_to_range(document_uri, span).await,
                            severity: DiagnosticSeverity::Warning,
                            message: "'offset' without 'limit' - did you forget limit?".to_string(),
                            source: Some("dibs".to_string()),
                            code: Some("offset-without-limit".to_string()),
                            data: None,
                        });
                    }
                }

                // Lint: LIMIT without ORDER BY (for @query only)
                if tag_name == "query" && has_limit && !has_order_by {
                    if let Some(span) = limit_span {
                        diagnostics.push(Diagnostic {
                            range: self.span_to_range(document_uri, span).await,
                            severity: DiagnosticSeverity::Warning,
                            message: "'limit' without 'order-by' returns arbitrary rows"
                                .to_string(),
                            source: Some("dibs".to_string()),
                            code: Some("limit-without-order-by".to_string()),
                            data: None,
                        });
                    }
                }

                // Lint: first without ORDER BY (for @query only)
                if tag_name == "query" && has_first && !has_order_by {
                    if let Some(span) = first_span {
                        diagnostics.push(Diagnostic {
                            range: self.span_to_range(document_uri, span).await,
                            severity: DiagnosticSeverity::Warning,
                            message: "'first' without 'order-by' returns arbitrary row".to_string(),
                            source: Some("dibs".to_string()),
                            code: Some("first-without-order-by".to_string()),
                            data: None,
                        });
                    }
                }

                // Lint: @update/@delete without WHERE
                if matches!(tag_name, "update" | "delete") && !has_where {
                    if let Some(span) = &tag.span {
                        diagnostics.push(Diagnostic {
                            range: self.span_to_range(document_uri, span).await,
                            severity: DiagnosticSeverity::Error,
                            message: format!(
                                "@{} without 'where' affects all rows - add 'where' or 'all true'",
                                tag_name
                            ),
                            source: Some("dibs".to_string()),
                            code: Some("mutation-without-where".to_string()),
                            data: None,
                        });
                    }
                }

                // Lint: unused params - collect used params and compare
                if !declared_params.is_empty() {
                    let used_params = self.collect_param_refs(value);
                    for (param_name, _param_type, param_span) in &declared_params {
                        if !used_params.contains(param_name) {
                            if let Some(span) = param_span {
                                diagnostics.push(Diagnostic {
                                    range: self.span_to_range(document_uri, span).await,
                                    severity: DiagnosticSeverity::Warning,
                                    message: format!(
                                        "param '{}' is declared but never used",
                                        param_name
                                    ),
                                    source: Some("dibs".to_string()),
                                    code: Some("unused-param".to_string()),
                                    data: None,
                                });
                            }
                        }
                    }
                }

                // Get table info for soft delete checks
                let table_info = table_name
                    .as_ref()
                    .and_then(|name| schema.tables.iter().find(|t| &t.name == name));

                // Check if table has deleted_at column (soft delete pattern)
                let has_deleted_at_column = table_info
                    .map(|t| t.columns.iter().any(|c| c.name == "deleted_at"))
                    .unwrap_or(false);

                // Check if where clause filters on deleted_at
                let filters_deleted_at = obj.entries.iter().any(|entry| {
                    if entry.key.as_str() == Some("where") {
                        self.where_filters_deleted_at(&entry.value)
                    } else {
                        false
                    }
                });

                // Lint: query on soft-delete table without deleted_at filter
                if tag_name == "query" && has_deleted_at_column && !filters_deleted_at {
                    // Find the "from" entry for span
                    for entry in &obj.entries {
                        if entry.key.as_str() == Some("from") {
                            if let Some(span) = &entry.value.span {
                                diagnostics.push(Diagnostic {
                                    range: self.span_to_range(document_uri, span).await,
                                    severity: DiagnosticSeverity::Warning,
                                    message: format!(
                                        "query on '{}' doesn't filter 'deleted_at' - add 'deleted_at @null' to exclude soft-deleted rows",
                                        table_name.as_deref().unwrap_or("table")
                                    ),
                                    source: Some("dibs".to_string()),
                                    code: Some("missing-deleted-at-filter".to_string()),
                                    data: None,
                                });
                            }
                            break;
                        }
                    }
                }

                // Lint: @delete on table with deleted_at (should use soft delete)
                if tag_name == "delete" && has_deleted_at_column {
                    if let Some(span) = &tag.span {
                        diagnostics.push(Diagnostic {
                            range: self.span_to_range(document_uri, span).await,
                            severity: DiagnosticSeverity::Warning,
                            message: format!(
                                "@delete on table with 'deleted_at' column - consider soft delete with @update instead"
                            ),
                            source: Some("dibs".to_string()),
                            code: Some("hard-delete-on-soft-delete-table".to_string()),
                            data: None,
                        });
                    }
                }

                // Validate column references if we have a valid table
                if tag_name == "query" {
                    if let Some(table) = table_info {
                        for entry in &obj.entries {
                            let key = entry.key.as_str().unwrap_or("");
                            if matches!(key, "select" | "where" | "order-by" | "group-by") {
                                self.validate_columns(
                                    document_uri,
                                    &entry.value,
                                    table,
                                    diagnostics,
                                )
                                .await;
                            }
                            // Type check param usages in where clause
                            if key == "where" {
                                self.validate_param_types(
                                    document_uri,
                                    &entry.value,
                                    table,
                                    &declared_params,
                                    diagnostics,
                                )
                                .await;
                            }
                        }
                    }
                }

                // Recurse into select to find @rel blocks
                for entry in &obj.entries {
                    if entry.key.as_str() == Some("select") {
                        Box::pin(self.collect_diagnostics(document_uri, &entry.value, diagnostics))
                            .await;
                    }
                }
            }

            // Don't recurse into tagged blocks - we've handled them
            return;
        }

        // Recurse into children (and handle @rel blocks)
        if let Some(styx_tree::Payload::Object(obj)) = &value.payload {
            for entry in &obj.entries {
                // Check if this entry's value is a @rel for relation-specific linting
                if let Some(tag) = &entry.value.tag {
                    if tag.name == "rel" {
                        self.lint_relation(document_uri, &entry.value, diagnostics)
                            .await;
                    }
                }
                Box::pin(self.collect_diagnostics(document_uri, &entry.value, diagnostics)).await;
            }
        } else if let Some(obj) = value.as_object() {
            for entry in &obj.entries {
                Box::pin(self.collect_diagnostics(document_uri, &entry.value, diagnostics)).await;
            }
        }

        if let Some(styx_tree::Payload::Sequence(seq)) = &value.payload {
            for item in &seq.items {
                Box::pin(self.collect_diagnostics(document_uri, item, diagnostics)).await;
            }
        }
    }

    /// Validate column references in a select/where/etc block.
    async fn validate_columns(
        &self,
        document_uri: &str,
        value: &styx_tree::Value,
        table: &TableInfo,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if let Some(styx_tree::Payload::Object(obj)) = &value.payload {
            for entry in &obj.entries {
                // Skip entries that are relations (@rel) - they're not column names
                if entry.value.tag.as_ref().map(|t| t.name.as_str()) == Some("rel") {
                    continue;
                }

                if let Some(col_name) = entry.key.as_str()
                    && !table.columns.iter().any(|c| c.name == col_name)
                {
                    // Unknown column
                    if let Some(span) = &entry.key.span {
                        diagnostics.push(Diagnostic {
                            range: self.span_to_range(document_uri, span).await,
                            severity: DiagnosticSeverity::Error,
                            message: format!(
                                "Unknown column '{}' in table '{}'",
                                col_name, table.name
                            ),
                            source: Some("dibs".to_string()),
                            code: Some("unknown-column".to_string()),
                            data: None,
                        });
                    }
                }
            }
        }
    }

    /// Validate param types against column types in a where clause.
    async fn validate_param_types(
        &self,
        document_uri: &str,
        where_value: &styx_tree::Value,
        table: &TableInfo,
        declared_params: &[(String, Option<String>, Option<styx_tree::Span>)],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if let Some(styx_tree::Payload::Object(obj)) = &where_value.payload {
            for entry in &obj.entries {
                let col_name = entry.key.as_str().unwrap_or("");
                let column = table.columns.iter().find(|c| c.name == col_name);

                if let Some(column) = column {
                    // Check for param usage in this entry's value
                    // Pattern 1: `column $param` (direct)
                    // Pattern 2: `column @op($param)` (with operator)
                    self.check_param_type_in_value(
                        document_uri,
                        &entry.value,
                        column,
                        declared_params,
                        diagnostics,
                    )
                    .await;
                }
            }
        }
    }

    /// Check param type compatibility in a value (handles both direct and operator patterns).
    async fn check_param_type_in_value(
        &self,
        document_uri: &str,
        value: &styx_tree::Value,
        column: &dibs_proto::ColumnInfo,
        declared_params: &[(String, Option<String>, Option<styx_tree::Span>)],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Check if value is a $param reference
        if let Some(text) = value.as_str() {
            if let Some(param_name) = text.strip_prefix('$') {
                self.emit_type_mismatch_if_needed(
                    document_uri,
                    value.span.as_ref(),
                    param_name,
                    column,
                    declared_params,
                    diagnostics,
                )
                .await;
            }
        }

        // Check if value has a tag like @eq, @ilike, etc. with param argument
        if value.tag.is_some() {
            // Check sequence payload for params (e.g., @eq($param))
            if let Some(styx_tree::Payload::Sequence(seq)) = &value.payload {
                for item in &seq.items {
                    if let Some(text) = item.as_str() {
                        if let Some(param_name) = text.strip_prefix('$') {
                            self.emit_type_mismatch_if_needed(
                                document_uri,
                                item.span.as_ref(),
                                param_name,
                                column,
                                declared_params,
                                diagnostics,
                            )
                            .await;
                        }
                    }
                }
            }
        }
    }

    /// Emit a type mismatch diagnostic if param type doesn't match column type.
    async fn emit_type_mismatch_if_needed(
        &self,
        document_uri: &str,
        span: Option<&styx_tree::Span>,
        param_name: &str,
        column: &dibs_proto::ColumnInfo,
        declared_params: &[(String, Option<String>, Option<styx_tree::Span>)],
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        // Find the param's declared type
        let param_info = declared_params
            .iter()
            .find(|(name, _, _)| name == param_name);

        if let Some((_, Some(param_type), _)) = param_info {
            if !self.types_compatible(param_type, &column.sql_type) {
                if let Some(span) = span {
                    diagnostics.push(Diagnostic {
                        range: self.span_to_range(document_uri, span).await,
                        severity: DiagnosticSeverity::Error,
                        message: format!(
                            "type mismatch: param '{}' is @{} but column '{}' is {}",
                            param_name, param_type, column.name, column.sql_type
                        ),
                        source: Some("dibs".to_string()),
                        code: Some("param-type-mismatch".to_string()),
                        data: None,
                    });
                }
            }
        }
    }

    /// Check if a param type is compatible with a SQL column type.
    fn types_compatible(&self, param_type: &str, sql_type: &str) -> bool {
        match param_type {
            "string" => matches!(
                sql_type.to_uppercase().as_str(),
                "TEXT" | "VARCHAR" | "CHAR" | "CHARACTER VARYING"
            ),
            "int" => matches!(
                sql_type.to_uppercase().as_str(),
                "INT" | "INTEGER" | "BIGINT" | "SMALLINT" | "INT4" | "INT8" | "INT2"
            ),
            "bool" | "boolean" => matches!(sql_type.to_uppercase().as_str(), "BOOLEAN" | "BOOL"),
            "float" => matches!(
                sql_type.to_uppercase().as_str(),
                "FLOAT" | "DOUBLE" | "REAL" | "NUMERIC" | "DECIMAL" | "FLOAT4" | "FLOAT8"
            ),
            // For unrecognized param types, assume compatible (could be custom)
            _ => true,
        }
    }

    /// Check if a where clause filters on deleted_at.
    fn where_filters_deleted_at(&self, where_value: &styx_tree::Value) -> bool {
        if let Some(styx_tree::Payload::Object(obj)) = &where_value.payload {
            for entry in &obj.entries {
                if entry.key.as_str() == Some("deleted_at") {
                    return true;
                }
            }
        }
        false
    }

    /// Lint a @rel block for relation-specific issues.
    async fn lint_relation(
        &self,
        document_uri: &str,
        rel_value: &styx_tree::Value,
        diagnostics: &mut Vec<Diagnostic>,
    ) {
        if let Some(styx_tree::Payload::Object(obj)) = &rel_value.payload {
            let mut has_first = false;
            let mut has_order_by = false;
            let mut first_span: Option<&styx_tree::Span> = None;

            for entry in &obj.entries {
                let key = entry.key.as_str().unwrap_or("");
                match key {
                    "first" => {
                        has_first = true;
                        first_span = entry.key.span.as_ref();
                    }
                    "order-by" => has_order_by = true,
                    _ => {}
                }
            }

            // Lint: first without order-by in relation
            if has_first && !has_order_by {
                if let Some(span) = first_span {
                    diagnostics.push(Diagnostic {
                        range: self.span_to_range(document_uri, span).await,
                        severity: DiagnosticSeverity::Warning,
                        message: "'first' in @rel without 'order-by' returns arbitrary row"
                            .to_string(),
                        source: Some("dibs".to_string()),
                        code: Some("rel-first-without-order-by".to_string()),
                        data: None,
                    });
                }
            }
        }
    }

    /// Collect inlay hints from a value tree.
    async fn collect_inlay_hints(
        &self,
        document_uri: &str,
        value: &styx_tree::Value,
        hints: &mut Vec<InlayHint>,
    ) {
        let schema = self.schema().await;

        // If this is a tagged @query or @rel object, look for column references
        if let Some(tag) = &value.tag
            && (tag.name == "query" || tag.name == "rel")
        {
            if let Some(styx_tree::Payload::Object(obj)) = &value.payload {
                // Find the table from "from" field
                let table_name = obj.entries.iter().find_map(|e| {
                    if e.key.as_str() == Some("from") {
                        e.value.as_str().map(|s| s.to_string())
                    } else {
                        None
                    }
                });

                if let Some(table_name) = table_name {
                    // Find the table in schema
                    if let Some(table) = schema.tables.iter().find(|t| t.name == table_name) {
                        // Look for select/where/order_by entries and add hints
                        for entry in &obj.entries {
                            let key = entry.key.as_str().unwrap_or("");
                            if matches!(key, "select" | "where" | "order_by" | "group_by") {
                                self.add_column_hints(document_uri, &entry.value, table, hints)
                                    .await;
                            }
                        }
                    }
                }

                // Continue recursing to find nested @rel blocks in select
                for entry in &obj.entries {
                    Box::pin(self.collect_inlay_hints(document_uri, &entry.value, hints)).await;
                }
            }
            return;
        }

        // Recurse into children - but only through one path to avoid double-visiting
        if let Some(styx_tree::Payload::Object(obj)) = &value.payload {
            for entry in &obj.entries {
                Box::pin(self.collect_inlay_hints(document_uri, &entry.value, hints)).await;
            }
        } else if let Some(obj) = value.as_object() {
            // Only use as_object() if payload wasn't an object
            for entry in &obj.entries {
                Box::pin(self.collect_inlay_hints(document_uri, &entry.value, hints)).await;
            }
        }

        if let Some(styx_tree::Payload::Sequence(seq)) = &value.payload {
            for item in &seq.items {
                Box::pin(self.collect_inlay_hints(document_uri, item, hints)).await;
            }
        }
    }

    /// Add inlay hints for column references in a select/where/etc block.
    async fn add_column_hints(
        &self,
        document_uri: &str,
        value: &styx_tree::Value,
        table: &TableInfo,
        hints: &mut Vec<InlayHint>,
    ) {
        // The value should be an object with column names as keys
        if let Some(styx_tree::Payload::Object(obj)) = &value.payload {
            for entry in &obj.entries {
                if let Some(col_name) = entry.key.as_str() {
                    // Skip if entry value has an explicit type annotation (colon followed by type)
                    // or if it's a @rel block (nested relation)
                    if entry.value.tag.as_ref().is_some_and(|t| t.name == "rel") {
                        // Skip @rel blocks - they're handled separately via recursion
                        continue;
                    }

                    // Skip if there's already a value (explicit type annotation like "id: BIGINT")
                    if entry.value.as_str().is_some() {
                        continue;
                    }

                    // Find the column in the table
                    if let Some(col) = table.columns.iter().find(|c| c.name == col_name) {
                        // Get the position at the end of the column name
                        if let Some(span) = &entry.key.span {
                            let position = self.offset_to_position(document_uri, span.end).await;

                            hints.push(InlayHint {
                                position,
                                label: format!(": {}", col.sql_type),
                                kind: Some(InlayHintKind::Type),
                                padding_left: false,
                                padding_right: false,
                            });
                        }
                    }
                }
            }
        }
    }

    /// Generate hover content for a column.
    async fn column_hover(&self, col_name: &str, table_name: &str) -> Option<HoverResult> {
        let schema = self.schema().await;
        let table = schema.tables.iter().find(|t| t.name == table_name)?;
        let col = table.columns.iter().find(|c| c.name == col_name)?;

        let mut content = format!("**Column `{}.{}`**\n\n", table.name, col.name);

        content.push_str(&format!("**Type:** `{}`\n\n", col.sql_type));

        let mut constraints = Vec::new();
        if col.primary_key {
            constraints.push("PRIMARY KEY".to_string());
        }
        if col.unique {
            constraints.push("UNIQUE".to_string());
        }
        if !col.nullable {
            constraints.push("NOT NULL".to_string());
        }
        if col.auto_generated {
            constraints.push("AUTO GENERATED".to_string());
        }
        if let Some(ref default) = col.default {
            constraints.push(format!("DEFAULT {}", default));
        }

        if !constraints.is_empty() {
            content.push_str("**Constraints:**\n");
            for c in constraints {
                content.push_str(&format!("- {}\n", c));
            }
        }

        Some(HoverResult {
            contents: content,
            range: None,
        })
    }

    /// Generate hover content for a table.
    fn table_hover(table: &TableInfo) -> HoverResult {
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
                col.sql_type,
                constraints.join(", ")
            ));
        }

        HoverResult {
            contents: content,
            range: None,
        }
    }

    /// Find the table name from a context value by looking for a "from" field.
    fn find_table_in_context(context: &styx_tree::Value) -> Option<String> {
        debug!(
            tag = ?context.tag,
            has_payload = context.payload.is_some(),
            "find_table_in_context"
        );

        // Look for a "from" field in the context object
        if let Some(obj) = context.as_object() {
            debug!(entries = obj.entries.len(), "checking as_object");
            for entry in &obj.entries {
                let key = entry.key.as_str();
                debug!(?key, "checking entry");
                if key == Some("from") {
                    let table = entry.value.as_str().map(|s| s.to_string());
                    debug!(?table, "found from field");
                    return table;
                }
            }
        }

        // Also check inside tagged payloads (e.g., @query{...})
        if let Some(styx_tree::Payload::Object(obj)) = &context.payload {
            debug!(entries = obj.entries.len(), "checking payload object");
            for entry in &obj.entries {
                let key = entry.key.as_str();
                debug!(?key, "checking payload entry");
                if key == Some("from") {
                    let table = entry.value.as_str().map(|s| s.to_string());
                    debug!(?table, "found from in payload");
                    return table;
                }
            }
        }

        debug!("no table found");
        None
    }

    /// Get completions for column names of a specific table.
    async fn column_completions(&self, table_name: &str, prefix: &str) -> Vec<CompletionItem> {
        let schema = self.schema().await;
        let Some(table) = schema.tables.iter().find(|t| t.name == table_name) else {
            return Vec::new();
        };

        table
            .columns
            .iter()
            .filter(|c| c.name.starts_with(prefix) || prefix.is_empty())
            .map(|c| {
                let mut detail = c.sql_type.to_string();
                if c.primary_key {
                    detail.push_str(" PK");
                }
                if !c.nullable {
                    detail.push_str(" NOT NULL");
                }

                CompletionItem {
                    label: c.name.clone(),
                    detail: Some(detail),
                    documentation: c.doc.clone(),
                    kind: Some(CompletionKind::Field),
                    sort_text: None,
                    insert_text: None,
                }
            })
            .collect()
    }

    /// Get completions for query structure fields (from, select, where, etc.)
    fn query_field_completions(&self, prefix: &str, is_rel: bool) -> Vec<CompletionItem> {
        use facet_styx::{Documented, ObjectKey, ObjectSchema, Schema, SchemaFile};

        // Generate schema string from the QueryFile type
        let schema_str = facet_styx::schema_from_type::<dibs_query_schema::QueryFile>();

        // Parse it back into a SchemaFile
        let Ok(schema_file) = facet_styx::from_str::<SchemaFile>(&schema_str) else {
            return Vec::new();
        };

        let type_name = if is_rel { "Relation" } else { "Query" };

        let Some(Schema::Object(ObjectSchema(fields))) =
            schema_file.schema.get(&Some(type_name.to_string()))
        else {
            return Vec::new();
        };

        fields
            .iter()
            .filter_map(|(key, _schema): (&Documented<ObjectKey>, &Schema)| {
                let name = key.name()?;
                if name.starts_with(prefix) || prefix.is_empty() {
                    let doc = key.doc().map(|lines: &[String]| lines.join("\n"));
                    Some(CompletionItem {
                        label: name.to_string(),
                        detail: None,
                        documentation: doc,
                        kind: Some(CompletionKind::Keyword),
                        sort_text: None,
                        insert_text: Some(format!("{} ", name)),
                    })
                } else {
                    None
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

        // Try to connect to the service and fetch the schema
        let schema = match connect_and_fetch_schema(&params.document_uri).await {
            Ok(state) => {
                let schema_tables = state.schema.tables.len();
                info!(
                    tables = schema_tables,
                    "Connected to service, fetched schema"
                );
                let mut guard = self.state.write().await;
                *guard = Some(state);
                schema_tables
            }
            Err(e) => {
                error!(error = %e, "Failed to connect to service");
                0
            }
        };

        InitializeResult {
            name: "dibs".to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            capabilities: if schema > 0 {
                vec![
                    Capability::Completions,
                    Capability::Hover,
                    Capability::Diagnostics,
                    Capability::InlayHints,
                    Capability::Definition,
                ]
            } else {
                // No schema = no capabilities
                vec![]
            },
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

        // Get the last segment and the second-to-last (parent)
        let last = params.path.last().map(|s| s.as_str()).unwrap_or("");
        let parent = if params.path.len() >= 2 {
            params.path.get(params.path.len() - 2).map(|s| s.as_str())
        } else {
            None
        };

        // Check if the last segment is a partial input (same as prefix)
        // In that case, use the parent to determine context
        let context_key = if !params.prefix.is_empty() && last == params.prefix {
            parent.unwrap_or(last)
        } else {
            last
        };

        match context_key {
            // Inside a @query or @rel block - offer query structure fields
            "@query" => self.query_field_completions(&params.prefix, false),
            "@rel" => self.query_field_completions(&params.prefix, true),

            // Table references
            "from" | "table" | "join" => self.table_completions(&params.prefix).await,

            // Column references - need to know which table
            "select" | "where" | "order_by" | "group_by" => {
                // Try tagged_context first (the @query block) - most reliable
                if let Some(tagged) = &params.tagged_context
                    && let Some(table_name) = Self::find_table_in_context(tagged)
                {
                    return self.column_completions(&table_name, &params.prefix).await;
                }

                // Fallback to direct context
                if let Some(context) = &params.context
                    && let Some(table_name) = Self::find_table_in_context(context)
                {
                    return self.column_completions(&table_name, &params.prefix).await;
                }

                // Last resort: return all columns from all tables
                let schema = self.schema().await;
                let mut items = Vec::new();
                for table in &schema.tables {
                    items.extend(self.column_completions(&table.name, &params.prefix).await);
                }
                items
            }

            _ => Vec::new(),
        }
    }

    async fn hover(&self, params: HoverParams) -> Option<HoverResult> {
        debug!(
            path = ?params.path,
            context = ?params.context,
            tagged_context = ?params.tagged_context,
            "Hover request"
        );

        // Try to provide hover info for table/column names
        if params.path.is_empty() {
            return None;
        }

        let last = params.path.last()?;
        let schema = self.schema().await;

        // Try to find the table from tagged_context first (the @query block)
        // This is the most reliable way to get context
        let table_from_tagged = params
            .tagged_context
            .as_ref()
            .and_then(Self::find_table_in_context);

        // If the last path segment is "from", "table", or "join", we're hovering over a table reference
        if matches!(last.as_str(), "from" | "table" | "join")
            && let Some(ref table_name) = table_from_tagged
            && let Some(table) = schema.tables.iter().find(|t| t.name == *table_name)
        {
            return Some(Self::table_hover(table));
        }

        // Check if we're hovering over a table name directly
        if let Some(table) = schema.tables.iter().find(|t| t.name == *last) {
            return Some(Self::table_hover(table));
        }

        // Check if we're hovering over a column name - use tagged_context to find the table
        if let Some(table_name) = table_from_tagged
            && let Some(result) = self.column_hover(last, &table_name).await
        {
            return Some(result);
        }

        // Fallback: try the direct context
        if let Some(context) = &params.context
            && let Some(table_name) = Self::find_table_in_context(context)
            && let Some(result) = self.column_hover(last, &table_name).await
        {
            return Some(result);
        }

        None
    }

    async fn inlay_hints(&self, params: InlayHintParams) -> Vec<InlayHint> {
        debug!(range = ?params.range, "Inlay hints request");

        let mut hints = Vec::new();

        // We need the context to find column references and their types
        let Some(context) = params.context else {
            debug!("No context provided for inlay hints");
            return hints;
        };

        debug!(
            has_tag = context.tag.is_some(),
            has_payload = context.payload.is_some(),
            "Inlay hints context"
        );

        // Find all @query blocks and add type hints for columns
        self.collect_inlay_hints(&params.document_uri, &context, &mut hints)
            .await;

        debug!(count = hints.len(), "Returning inlay hints");
        hints
    }

    async fn diagnostics(&self, params: DiagnosticParams) -> Vec<Diagnostic> {
        debug!("Diagnostics request");

        let mut diagnostics = Vec::new();

        // Find all @query blocks and validate references
        self.collect_diagnostics(&params.document_uri, &params.tree, &mut diagnostics)
            .await;

        diagnostics
    }

    async fn code_actions(&self, _params: CodeActionParams) -> Vec<CodeAction> {
        // Not implemented yet
        Vec::new()
    }

    async fn definition(&self, params: DefinitionParams) -> Vec<Location> {
        debug!(path = ?params.path, cursor = ?params.cursor, "Definition request");

        // We support definition for:
        // 1. $param references → jump to param declaration in same query
        // 2. Table names → could jump to Rust struct (needs source locations in schema)
        // 3. Column names → could jump to column in struct (needs source locations)

        let Some(tagged_context) = &params.tagged_context else {
            return Vec::new();
        };

        // Try to find a $param reference at the cursor position
        let cursor_offset = params.cursor.offset as usize;

        if let Some(param_name) = self.find_param_at_offset(tagged_context, cursor_offset) {
            debug!(%param_name, "Found param reference at cursor");

            // The tagged_context might be the query itself (if cursor is in a non-tagged area)
            // or a nested tag like @eq. We need to get the query block.
            // The path's first element is the query name.
            let query_value =
                if tagged_context.tag.as_ref().map(|t| t.name.as_str()) == Some("query") {
                    // tagged_context is already the query
                    Some(tagged_context.clone())
                } else if !params.path.is_empty() {
                    // Fetch the query subtree from the host
                    let host = self.host.read().await;
                    if let Some(host) = host.as_ref() {
                        match host
                            .get_subtree(styx_lsp_ext::GetSubtreeParams {
                                document_uri: params.document_uri.clone(),
                                path: vec![params.path[0].clone()],
                            })
                            .await
                        {
                            Ok(Some(v)) => Some(v),
                            Ok(None) => {
                                debug!("get_subtree returned None");
                                None
                            }
                            Err(e) => {
                                debug!(%e, "get_subtree failed");
                                None
                            }
                        }
                    } else {
                        debug!("No host client available");
                        None
                    }
                } else {
                    None
                };

            if let Some(query_value) = query_value {
                // Find the params block in the query
                if let Some(obj) = query_value.as_object() {
                    for entry in &obj.entries {
                        if entry.key.as_str() == Some("params") {
                            // Found the params block - look for our param
                            if let Some(styx_tree::Payload::Object(params_obj)) =
                                &entry.value.payload
                            {
                                for param_entry in &params_obj.entries {
                                    if param_entry.key.as_str() == Some(&param_name) {
                                        debug!(%param_name, "Found param declaration");
                                        // Found it! Return its location
                                        if let Some(span) = &param_entry.key.span {
                                            let range = self
                                                .span_to_range(&params.document_uri, span)
                                                .await;
                                            return vec![Location {
                                                uri: params.document_uri.clone(),
                                                range,
                                            }];
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        Vec::new()
    }

    async fn shutdown(&self) {
        info!("Shutdown requested");
    }
}

/// Connect to the service and fetch the schema.
///
/// This uses the same mechanism as the TUI:
/// 1. Find `.config/dibs.styx` starting from the document's directory
/// 2. Connect to the service specified in the config
/// 3. Fetch the schema via RPC
async fn connect_and_fetch_schema(document_uri: &str) -> Result<ExtensionState, String> {
    // Parse the URI to get the file path
    let path = if let Some(stripped) = document_uri.strip_prefix("file://") {
        Path::new(stripped)
    } else {
        Path::new(document_uri)
    };

    // Get the directory containing the document
    let dir = path.parent().ok_or("Document has no parent directory")?;

    info!(dir = %dir.display(), "Looking for config starting from document directory");

    // Load the config
    let (cfg, config_path) =
        config::load_from(dir).map_err(|e| format!("Failed to load config: {}", e))?;

    info!(config_path = %config_path.display(), "Found config");

    // Change to the config directory so relative paths work
    let config_dir = config_path
        .parent()
        .and_then(|p| p.parent())
        .ok_or("Config path has no parent")?;

    std::env::set_current_dir(config_dir)
        .map_err(|e| format!("Failed to change directory: {}", e))?;

    info!(cwd = %config_dir.display(), "Changed working directory");

    // Connect to the service
    let connection = service::connect_to_service(&cfg)
        .await
        .map_err(|e| format!("Failed to connect to service: {}", e))?;

    info!("Connected to service");

    // Fetch the schema
    let client = connection.client();
    let schema_info = client
        .schema()
        .await
        .map_err(|e| format!("Failed to fetch schema: {}", e))?;

    info!(tables = schema_info.tables.len(), "Fetched schema");

    Ok(ExtensionState {
        schema: schema_info,
        connection,
    })
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_query_field_completions_from_schema() {
        // Generate schema from QueryFile type
        let schema_str = facet_styx::schema_from_type::<dibs_query_schema::QueryFile>();

        // Parse it back
        let schema_file: facet_styx::SchemaFile =
            facet_styx::from_str(&schema_str).expect("should parse schema");

        // Verify Query type exists and has expected fields
        let query_schema = schema_file
            .schema
            .get(&Some("Query".to_string()))
            .expect("should have Query type");

        if let facet_styx::Schema::Object(facet_styx::ObjectSchema(fields)) = query_schema {
            let field_names: Vec<_> = fields.keys().filter_map(|k| k.name()).collect();

            assert!(
                field_names.contains(&"from"),
                "Query should have 'from' field"
            );
            assert!(
                field_names.contains(&"select"),
                "Query should have 'select' field"
            );
            assert!(
                field_names.contains(&"where"),
                "Query should have 'where' field"
            );
            // Field is named with hyphen in styx (order-by), not underscore
            assert!(
                field_names.contains(&"order-by"),
                "Query should have 'order-by' field, got: {:?}",
                field_names
            );
        } else {
            panic!("Query should be an object schema");
        }

        // Verify Relation type exists
        let relation_schema = schema_file
            .schema
            .get(&Some("Relation".to_string()))
            .expect("should have Relation type");

        if let facet_styx::Schema::Object(facet_styx::ObjectSchema(fields)) = relation_schema {
            let field_names: Vec<_> = fields.keys().filter_map(|k| k.name()).collect();

            assert!(
                field_names.contains(&"from"),
                "Relation should have 'from' field"
            );
        } else {
            panic!("Relation should be an object schema");
        }
    }
}
