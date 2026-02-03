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
use dibs_query_schema::{Decl, QueryFile};
use roam_session::HandshakeConfig;
use roam_stream::CobsFramed;
use std::path::Path;
use std::sync::Arc;
use styx_lsp_ext::{
    Capability, CodeAction, CodeActionKind, CodeActionParams, CompletionItem, CompletionKind,
    CompletionParams, DefinitionParams, Diagnostic, DiagnosticParams, DocumentEdit, HoverParams,
    HoverResult, InitializeParams, InitializeResult, InlayHint, InlayHintKind, InlayHintParams,
    Location, OffsetToPositionParams, Position, StyxLspExtension, StyxLspExtensionDispatcher,
    StyxLspHostClient, TextEdit, WorkspaceEdit,
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

    /// Find a $param reference at the given cursor offset.
    /// Returns the param name (without the $) if found.
    fn find_param_at_offset(&self, value: &styx_tree::Value, offset: usize) -> Option<String> {
        // Check if this value is a scalar that looks like $param
        if let Some(text) = value.as_str()
            && let Some(span) = &value.span
        {
            let start = span.start as usize;
            let end = span.end as usize;
            if offset >= start
                && offset <= end
                && let Some(param_name) = text.strip_prefix('$')
            {
                return Some(param_name.to_string());
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

    /// Get the schema, returning an empty schema if not initialized.
    async fn schema(&self) -> SchemaInfo {
        let state = self.state.read().await;
        state
            .as_ref()
            .map(|s| s.schema.clone())
            .unwrap_or_else(|| SchemaInfo { tables: vec![] })
    }

    /// Parse styx content into typed QueryFile.
    /// Returns None if parsing fails (syntax errors are handled by styx-lsp itself).
    fn parse_query_file(content: &str) -> Result<QueryFile, facet_styx::DeserializeError> {
        facet_styx::from_str(content)
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

    /// Collect diagnostics from the content using typed schema.
    async fn collect_diagnostics_typed(
        &self,
        content: &str,
        diagnostics: &mut Vec<Diagnostic>,
    ) -> Result<(), facet_styx::DeserializeError> {
        use crate::lints::{self, LintContext};

        let schema = self.schema().await;

        // Parse content to typed schema
        let query_file = Self::parse_query_file(content)?;

        let mut ctx = LintContext::new(&schema, diagnostics);

        for (_name, decl) in &query_file.0 {
            match decl {
                Decl::Select(query) => {
                    lints::lint_unknown_table_query(query, &mut ctx);
                    lints::lint_pagination_query(query, &mut ctx);
                    lints::lint_unused_params_query(query, &mut ctx);
                    lints::lint_missing_deleted_at_filter(query, &mut ctx);

                    if let Some(from) = &query.from
                        && let Some(table) = ctx.find_table(from.as_str())
                    {
                        if let Some(fields) = &query.fields {
                            lints::lint_empty_select(fields, &mut ctx);
                            lints::lint_unknown_columns_select(fields, table, &mut ctx);
                            lints::lint_relations_in_select(fields, Some(from.as_str()), &mut ctx);
                        }
                        if let Some(where_clause) = &query.where_clause {
                            lints::lint_unknown_columns_where(where_clause, table, &mut ctx);
                            lints::lint_redundant_params_in_where(where_clause, &mut ctx);
                            lints::lint_literal_types_in_where(where_clause, table, &mut ctx);
                            if let Some(params) = &query.params {
                                lints::lint_param_types_in_where(
                                    where_clause,
                                    table,
                                    params,
                                    &mut ctx,
                                );
                            }
                        }
                        if let Some(order_by) = &query.order_by {
                            lints::lint_unknown_columns_order_by(order_by, table, &mut ctx);
                        }
                    }
                }
                Decl::Insert(insert) => {
                    lints::lint_unknown_table_insert(insert, &mut ctx);
                    lints::lint_unused_params_insert(insert, &mut ctx);
                    lints::lint_redundant_params_in_values(&insert.values, &mut ctx);

                    if let Some(table) = ctx.find_table(insert.into.as_str()) {
                        lints::lint_unknown_columns_values(&insert.values, table, &mut ctx);
                    }
                }
                Decl::InsertMany(insert_many) => {
                    lints::lint_unknown_table_insert_many(insert_many, &mut ctx);
                    lints::lint_unused_params_insert_many(insert_many, &mut ctx);
                    lints::lint_redundant_params_in_values(&insert_many.values, &mut ctx);

                    if let Some(table) = ctx.find_table(insert_many.into.as_str()) {
                        lints::lint_unknown_columns_values(&insert_many.values, table, &mut ctx);
                    }
                }
                Decl::Upsert(upsert) => {
                    lints::lint_unknown_table_upsert(upsert, &mut ctx);
                    lints::lint_unused_params_upsert(upsert, &mut ctx);
                    lints::lint_redundant_params_in_values(&upsert.values, &mut ctx);
                    lints::lint_redundant_params_in_conflict_update(
                        &upsert.on_conflict.update,
                        &mut ctx,
                    );

                    if let Some(table) = ctx.find_table(upsert.into.as_str()) {
                        lints::lint_unknown_columns_values(&upsert.values, table, &mut ctx);
                    }
                }
                Decl::UpsertMany(upsert_many) => {
                    lints::lint_unknown_table_upsert_many(upsert_many, &mut ctx);
                    lints::lint_unused_params_upsert_many(upsert_many, &mut ctx);
                    lints::lint_redundant_params_in_values(&upsert_many.values, &mut ctx);
                    lints::lint_redundant_params_in_conflict_update(
                        &upsert_many.on_conflict.update,
                        &mut ctx,
                    );

                    if let Some(table) = ctx.find_table(upsert_many.into.as_str()) {
                        lints::lint_unknown_columns_values(&upsert_many.values, table, &mut ctx);
                    }
                }
                Decl::Update(update) => {
                    lints::lint_unknown_table_update(update, &mut ctx);
                    lints::lint_update_without_where(update, &mut ctx);
                    lints::lint_unused_params_update(update, &mut ctx);
                    lints::lint_redundant_params_in_values(&update.set, &mut ctx);

                    if let Some(table) = ctx.find_table(update.table.as_str()) {
                        lints::lint_unknown_columns_values(&update.set, table, &mut ctx);
                        if let Some(where_clause) = &update.where_clause {
                            lints::lint_unknown_columns_where(where_clause, table, &mut ctx);
                            lints::lint_redundant_params_in_where(where_clause, &mut ctx);
                        }
                    }
                }
                Decl::Delete(delete) => {
                    lints::lint_unknown_table_delete(delete, &mut ctx);
                    lints::lint_delete_without_where(delete, &mut ctx);
                    lints::lint_hard_delete_on_soft_delete_table(delete, &mut ctx);
                    lints::lint_unused_params_delete(delete, &mut ctx);

                    if let Some(table) = ctx.find_table(delete.from.as_str())
                        && let Some(where_clause) = &delete.where_clause
                    {
                        lints::lint_unknown_columns_where(where_clause, table, &mut ctx);
                        lints::lint_redundant_params_in_where(where_clause, &mut ctx);
                    }
                }
            }
        }

        Ok(())
    }

    /// Collect inlay hints from a value tree.
    /// Note: Uses styx_tree::Value because InlayHintParams doesn't include content string.
    async fn collect_inlay_hints(
        &self,
        document_uri: &str,
        value: &styx_tree::Value,
        hints: &mut Vec<InlayHint>,
    ) {
        let schema = self.schema().await;

        // Handle tagged objects (@select, @rel, @insert, @update, @delete, @upsert, @insert-many, @upsert-many)
        if let Some(tag) = &value.tag
            && matches!(
                tag.name.as_str(),
                "select"
                    | "rel"
                    | "insert"
                    | "update"
                    | "delete"
                    | "upsert"
                    | "insert-many"
                    | "upsert-many"
            )
        {
            if let Some(styx_tree::Payload::Object(obj)) = &value.payload {
                // Find the table from "from", "into", or "table" field
                let table_name = obj.entries.iter().find_map(|e| {
                    let key = e.key.as_str();
                    if matches!(key, Some("from") | Some("into") | Some("table")) {
                        e.value.as_str().map(|s| s.to_string())
                    } else {
                        None
                    }
                });

                if let Some(table_name) = table_name {
                    // Find the table in schema
                    if let Some(table) = schema.tables.iter().find(|t| t.name == table_name) {
                        // Look for column reference entries and add hints
                        for entry in &obj.entries {
                            let key = entry.key.as_str().unwrap_or("");
                            if matches!(
                                key,
                                "fields"
                                    | "where"
                                    | "order-by"
                                    | "group-by"
                                    | "values"
                                    | "set"
                                    | "returning"
                            ) {
                                self.add_column_hints(document_uri, &entry.value, table, hints)
                                    .await;
                            }
                            // Handle on-conflict block (has target and update sub-blocks)
                            if key == "on-conflict"
                                && let Some(styx_tree::Payload::Object(conflict_obj)) =
                                    &entry.value.payload
                            {
                                for conflict_entry in &conflict_obj.entries {
                                    let conflict_key = conflict_entry.key.as_str().unwrap_or("");
                                    if matches!(conflict_key, "target" | "update") {
                                        self.add_column_hints(
                                            document_uri,
                                            &conflict_entry.value,
                                            table,
                                            hints,
                                        )
                                        .await;
                                    }
                                }
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
                    // Skip @rel blocks - they're handled separately via recursion
                    if entry.value.tag.as_ref().is_some_and(|t| t.name == "rel") {
                        continue;
                    }

                    // Skip if there's an explicit type annotation like "credential_id: BYTEA"
                    if let Some(val_str) = entry.value.as_str() {
                        let is_type_annotation = val_str
                            .chars()
                            .next()
                            .is_some_and(|c| c.is_ascii_uppercase())
                            && val_str
                                .chars()
                                .all(|c| c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_');
                        if is_type_annotation {
                            continue;
                        }
                    }

                    // Find the column in the table
                    if let Some(col) = table.columns.iter().find(|c| c.name == col_name)
                        && let Some(span) = &entry.key.span
                    {
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

        // Look for a "from", "into", or "table" field in the context object
        if let Some(obj) = context.as_object() {
            debug!(entries = obj.entries.len(), "checking as_object");
            for entry in &obj.entries {
                let key = entry.key.as_str();
                debug!(?key, "checking entry");
                if matches!(key, Some("from") | Some("into") | Some("table")) {
                    let table = entry.value.as_str().map(|s| s.to_string());
                    debug!(?table, "found table field");
                    return table;
                }
            }
        }

        // Also check inside tagged payloads (e.g., @select{...}, @insert{...}, etc.)
        if let Some(styx_tree::Payload::Object(obj)) = &context.payload {
            debug!(entries = obj.entries.len(), "checking payload object");
            for entry in &obj.entries {
                let key = entry.key.as_str();
                debug!(?key, "checking payload entry");
                if matches!(key, Some("from") | Some("into") | Some("table")) {
                    let table = entry.value.as_str().map(|s| s.to_string());
                    debug!(?table, "found table in payload");
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
        let type_name = if is_rel { "Relation" } else { "Query" };
        self.schema_type_completions(prefix, type_name)
    }

    /// Get completions for fields of a specific schema type.
    fn schema_type_completions(&self, prefix: &str, type_name: &str) -> Vec<CompletionItem> {
        use facet_styx::{Documented, ObjectKey, ObjectSchema, Schema, SchemaFile};

        // Generate schema string from the QueryFile type
        let schema_str = facet_styx::schema_from_type::<dibs_query_schema::QueryFile>();

        // Parse it back into a SchemaFile
        let Ok(schema_file) = facet_styx::from_str::<SchemaFile>(&schema_str) else {
            return Vec::new();
        };

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
    async fn initialize(&self, _cx: &roam::Context, params: InitializeParams) -> InitializeResult {
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

    async fn completions(
        &self,
        _cx: &roam::Context,
        params: CompletionParams,
    ) -> Vec<CompletionItem> {
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

            // Inside bulk operation blocks - offer their specific fields
            "@insert-many" => self.schema_type_completions(&params.prefix, "InsertMany"),
            "@upsert-many" => self.schema_type_completions(&params.prefix, "UpsertMany"),

            // Table references
            "from" | "into" | "table" | "join" => self.table_completions(&params.prefix).await,

            // Column references - need to know which table
            "fields" | "where" | "order-by" | "group-by" | "values" | "set" | "returning"
            | "target" | "update" => {
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

    async fn hover(&self, _cx: &roam::Context, params: HoverParams) -> Option<HoverResult> {
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

        // If the last path segment is "from", "into", "table", or "join", we're hovering over a table reference
        if matches!(last.as_str(), "from" | "into" | "table" | "join")
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

    async fn inlay_hints(&self, _cx: &roam::Context, params: InlayHintParams) -> Vec<InlayHint> {
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

    async fn diagnostics(&self, _cx: &roam::Context, params: DiagnosticParams) -> Vec<Diagnostic> {
        debug!("Diagnostics request");

        let mut diagnostics = Vec::new();

        // Use typed schema for diagnostics
        if let Err(e) = self
            .collect_diagnostics_typed(&params.content, &mut diagnostics)
            .await
        {
            let span = e
                .span()
                .copied()
                .unwrap_or(dibs_query_schema::Span { offset: 0, len: 0 });
            diagnostics.push(Diagnostic {
                span: styx_tree::Span {
                    start: span.offset,
                    end: (span.offset + span.len),
                },
                severity: styx_lsp_ext::DiagnosticSeverity::Error,
                message: e.to_string(),
                source: Some("dibs".to_string()),
                code: Some("parse-error".to_string()),
                data: None,
            });
        }

        diagnostics
    }

    async fn code_actions(&self, _cx: &roam::Context, params: CodeActionParams) -> Vec<CodeAction> {
        let mut actions = Vec::new();

        // Offer quick fixes for diagnostics at this range
        for diag in &params.diagnostics {
            if diag.code.as_deref() == Some("redundant-param") {
                // Get the column name from diagnostic data
                if let Some(name) = diag.data.as_ref().and_then(|v| v.as_str()) {
                    actions.push(CodeAction {
                        title: format!("Shorten to '{}'", name),
                        kind: Some(CodeActionKind::QuickFix),
                        edit: Some(WorkspaceEdit {
                            changes: vec![DocumentEdit {
                                uri: params.document_uri.clone(),
                                edits: vec![TextEdit {
                                    span: diag.span,
                                    new_text: String::new(), // Just delete the $param part
                                }],
                            }],
                        }),
                        is_preferred: true,
                    });
                }
            }
        }

        actions
    }

    async fn definition(&self, _cx: &roam::Context, params: DefinitionParams) -> Vec<Location> {
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
                                        if let Some(span) = param_entry.key.span {
                                            return vec![Location {
                                                uri: params.document_uri.clone(),
                                                span,
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

    async fn shutdown(&self, _cx: &roam::Context) {
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
    let connection = service::connect_to_service(&cfg.db)
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
    use facet_styx::SchemaFile;

    #[test]
    fn test_query_schema_roundtrip() {
        // Generate schema from QueryFile type
        let schema_str = facet_styx::schema_from_type::<dibs_query_schema::QueryFile>();

        // Parse it back into a SchemaFile - this validates the schema is well-formed
        let schema_file: SchemaFile = facet_styx::from_str(&schema_str)
            .expect("Generated schema should parse back into SchemaFile");

        // Verify the schema has the expected structure
        assert!(
            schema_file.schema.contains_key(&None),
            "Schema should have root definition"
        );

        // Check that key type definitions exist
        assert!(
            schema_file.schema.contains_key(&Some("Select".to_string())),
            "Schema should contain Select type definition"
        );
        assert!(
            schema_file
                .schema
                .contains_key(&Some("Relation".to_string())),
            "Schema should contain Relation type definition"
        );
        assert!(
            schema_file.schema.contains_key(&Some("Where".to_string())),
            "Schema should contain Where type definition"
        );
    }
}
