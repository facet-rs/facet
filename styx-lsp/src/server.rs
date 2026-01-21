//! LSP server implementation

use std::collections::HashMap;
use std::sync::Arc;

use styx_cst::{Parse, parse};
use styx_tree::Value;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use crate::extensions::{ExtensionManager, ExtensionResult, get_extension_info};
use crate::schema_hints::find_matching_hint;
use crate::schema_validation::{
    find_object_at_offset, find_schema_declaration, find_tagged_context_at_offset,
    get_document_fields, get_error_span, get_schema_fields, get_schema_fields_at_path,
    load_document_schema, resolve_schema, validate_against_schema,
};
use crate::semantic_tokens::{compute_semantic_tokens, semantic_token_legend};
use styx_lsp_ext as ext;

/// Document state tracked by the server
pub struct DocumentState {
    /// Document content
    pub content: String,
    /// Parsed CST
    pub parse: Parse,
    /// Parsed tree (for schema validation)
    pub tree: Option<Value>,
    /// Document version
    #[allow(dead_code)]
    pub version: i32,
}

/// Shared document map type
pub type DocumentMap = Arc<RwLock<HashMap<Url, DocumentState>>>;

/// Information about a blocked LSP extension.
struct BlockedExtensionInfo {
    /// The schema ID that has the extension.
    schema_id: String,
    /// The command that needs to be allowed.
    command: String,
}

/// The Styx language server
pub struct StyxLanguageServer {
    /// LSP client for sending notifications
    client: Client,
    /// Open documents
    documents: DocumentMap,
    /// Extension manager
    extensions: Arc<ExtensionManager>,
}

impl StyxLanguageServer {
    pub fn new(client: Client) -> Self {
        let documents: DocumentMap = Arc::new(RwLock::new(HashMap::new()));
        Self {
            client,
            documents: documents.clone(),
            extensions: Arc::new(ExtensionManager::new(documents)),
        }
    }

    /// Check if the document's schema has an LSP extension and spawn it if allowed.
    ///
    /// Returns information about blocked extensions if not allowed.
    async fn check_for_extension(&self, tree: &Value, uri: &Url) -> Option<BlockedExtensionInfo> {
        // Try to load the schema
        let Ok(schema) = load_document_schema(tree, uri) else {
            return None;
        };

        // Check if schema has an LSP extension
        let Some(ext_info) = get_extension_info(&schema) else {
            return None;
        };

        tracing::info!(
            schema_id = %ext_info.schema_id,
            launch = ?ext_info.config.launch,
            "Document schema has LSP extension"
        );

        // Try to spawn the extension (will check allowlist internally)
        match self
            .extensions
            .get_or_spawn(&ext_info.schema_id, &ext_info.config, uri.as_str())
            .await
        {
            ExtensionResult::Running => None,
            ExtensionResult::NotAllowed { command } => Some(BlockedExtensionInfo {
                schema_id: ext_info.schema_id,
                command,
            }),
            ExtensionResult::Failed => None,
        }
    }

    /// Publish diagnostics for a document
    async fn publish_diagnostics(
        &self,
        uri: Url,
        content: &str,
        parsed: &Parse,
        tree: Option<&Value>,
        version: i32,
        blocked_extension: Option<BlockedExtensionInfo>,
    ) {
        let mut diagnostics = self.compute_diagnostics(&uri, content, parsed, tree);

        // Add diagnostic for blocked extension if applicable
        if let Some(blocked) = blocked_extension {
            if let Some(tree) = tree {
                if let Some(range) = find_schema_declaration_range(tree, content) {
                    diagnostics.push(Diagnostic {
                        range,
                        severity: Some(DiagnosticSeverity::INFORMATION),
                        code: None,
                        code_description: None,
                        source: Some("styx-extension".to_string()),
                        message: format!(
                            "LSP extension '{}' is not allowed. Use the code action to allow it.",
                            blocked.command
                        ),
                        related_information: None,
                        tags: None,
                        data: Some(serde_json::json!({
                            "type": "allow_extension",
                            "schema_id": blocked.schema_id,
                            "command": blocked.command,
                        })),
                    });
                }
            }
        }

        self.client
            .publish_diagnostics(uri, diagnostics, Some(version))
            .await;
    }

    /// Compute diagnostics for document content
    fn compute_diagnostics(
        &self,
        uri: &Url,
        content: &str,
        parsed: &Parse,
        tree: Option<&Value>,
    ) -> Vec<Diagnostic> {
        let mut diagnostics = Vec::new();

        // Phase 1: Parse errors
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

        // Phase 2: CST validation (duplicate keys, mixed separators)
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

        // Phase 3: Schema validation
        if let Some(tree) = tree {
            // Only validate if there's a schema declaration
            if let Ok(schema) = resolve_schema(tree, uri) {
                // Create related_information linking to schema
                let schema_location = Some(DiagnosticRelatedInformation {
                    location: Location {
                        uri: schema.uri.clone(),
                        range: Range::default(),
                    },
                    message: format!("schema: {}", schema.uri),
                });

                match validate_against_schema(tree, uri) {
                    Ok(result) => {
                        // Add validation errors
                        for error in &result.errors {
                            // Use the span directly from the error if available
                            let range = if let Some(span) = error.span {
                                Range {
                                    start: offset_to_position(content, span.start as usize),
                                    end: offset_to_position(content, span.end as usize),
                                }
                            } else {
                                // Fallback: try to find by path, or point to start of document
                                if let Some((start, end)) = get_error_span(tree, &error.path) {
                                    Range {
                                        start: offset_to_position(content, start),
                                        end: offset_to_position(content, end),
                                    }
                                } else {
                                    Range {
                                        start: Position::new(0, 0),
                                        end: Position::new(0, 1),
                                    }
                                }
                            };

                            // Store quickfix data for code actions
                            let data = error.quickfix_data();

                            diagnostics.push(Diagnostic {
                                range,
                                severity: Some(DiagnosticSeverity::ERROR),
                                code: None,
                                code_description: None,
                                source: Some("styx-schema".to_string()),
                                message: error.diagnostic_message(),
                                related_information: schema_location.clone().map(|loc| vec![loc]),
                                tags: None,
                                data,
                            });
                        }

                        // Add validation warnings
                        for warning in &result.warnings {
                            // Use the span directly from the warning if available
                            let range = if let Some(span) = warning.span {
                                Range {
                                    start: offset_to_position(content, span.start as usize),
                                    end: offset_to_position(content, span.end as usize),
                                }
                            } else {
                                Range {
                                    start: Position::new(0, 0),
                                    end: Position::new(0, 1),
                                }
                            };

                            diagnostics.push(Diagnostic {
                                range,
                                severity: Some(DiagnosticSeverity::WARNING),
                                code: None,
                                code_description: None,
                                source: Some("styx-schema".to_string()),
                                message: warning.message.clone(),
                                related_information: schema_location.clone().map(|loc| vec![loc]),
                                tags: None,
                                data: None,
                            });
                        }
                    }
                    Err(e) => {
                        // Schema loading error
                        diagnostics.push(Diagnostic {
                            range: Range {
                                start: Position::new(0, 0),
                                end: Position::new(0, 1),
                            },
                            severity: Some(DiagnosticSeverity::ERROR),
                            code: None,
                            code_description: None,
                            source: Some("styx-schema".to_string()),
                            message: e,
                            related_information: None,
                            tags: None,
                            data: None,
                        });
                    }
                }
            }
        }

        // Phase 4: Schema hint suggestions
        // If no schema declaration but file matches a known pattern, suggest adding one
        if let Some(tree) = tree
            && find_schema_declaration(tree).is_none()
            && let Some(hint_match) = find_matching_hint(uri)
        {
            // Create data for the code action
            let data = serde_json::json!({
                "type": "add_schema",
                "declaration": hint_match.schema_declaration(),
                "tool": hint_match.tool_name,
            });

            diagnostics.push(Diagnostic {
                range: Range {
                    start: Position::new(0, 0),
                    end: Position::new(0, 0),
                },
                severity: Some(DiagnosticSeverity::HINT),
                code: Some(NumberOrString::String("missing-schema".to_string())),
                code_description: None,
                source: Some("styx-hints".to_string()),
                message: format!(
                    "This file matches the {} pattern. Add @schema declaration?",
                    hint_match.description()
                ),
                related_information: None,
                tags: None,
                data: Some(data),
            });
        }

        diagnostics
    }
}

#[tower_lsp::async_trait]
impl LanguageServer for StyxLanguageServer {
    async fn initialize(&self, _params: InitializeParams) -> Result<InitializeResult> {
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
                // Document links for schema references
                document_link_provider: Some(DocumentLinkOptions {
                    resolve_provider: Some(false),
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                }),
                // Go to definition
                definition_provider: Some(OneOf::Left(true)),
                // Hover information
                hover_provider: Some(HoverProviderCapability::Simple(true)),
                // Auto-completion
                completion_provider: Some(CompletionOptions {
                    trigger_characters: Some(vec![" ".into(), "\n".into()]),
                    resolve_provider: Some(false),
                    ..Default::default()
                }),
                // Code actions (quick fixes)
                code_action_provider: Some(CodeActionProviderCapability::Simple(true)),
                // Find all references
                references_provider: Some(OneOf::Left(true)),
                // Inlay hints
                inlay_hint_provider: Some(OneOf::Left(true)),
                // Document formatting
                document_formatting_provider: Some(OneOf::Left(true)),
                // On-type formatting (for auto-indent on Enter)
                document_on_type_formatting_provider: Some(DocumentOnTypeFormattingOptions {
                    first_trigger_character: "\n".to_string(),
                    more_trigger_character: None,
                }),
                // Document symbols (outline)
                document_symbol_provider: Some(OneOf::Left(true)),
                // Execute command (for code action commands)
                execute_command_provider: Some(ExecuteCommandOptions {
                    commands: vec!["styx.allowExtension".to_string()],
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                }),
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
            .log_message(
                MessageType::INFO,
                format!(
                    "Styx language server initialized (PID: {})",
                    std::process::id()
                ),
            )
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let uri = params.text_document.uri;
        let content = params.text_document.text;
        let version = params.text_document.version;

        // Parse the document (CST)
        let parsed = parse(&content);

        // Parse into tree for schema validation
        let tree = styx_tree::parse(&content).ok();

        // Check for LSP extension in schema
        let blocked_extension = if let Some(ref tree) = tree {
            self.check_for_extension(tree, &uri).await
        } else {
            None
        };

        // Publish diagnostics
        self.publish_diagnostics(
            uri.clone(),
            &content,
            &parsed,
            tree.as_ref(),
            version,
            blocked_extension,
        )
        .await;

        // Store document
        {
            let mut docs = self.documents.write().await;
            docs.insert(
                uri,
                DocumentState {
                    content,
                    parse: parsed,
                    tree,
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

            // Parse the document (CST)
            let parsed = parse(&content);

            // Parse into tree for schema validation
            let tree = styx_tree::parse(&content).ok();

            // Check for LSP extension in schema (might have changed)
            let blocked_extension = if let Some(ref tree) = tree {
                self.check_for_extension(tree, &uri).await
            } else {
                None
            };

            // Publish diagnostics
            self.publish_diagnostics(
                uri.clone(),
                &content,
                &parsed,
                tree.as_ref(),
                version,
                blocked_extension,
            )
            .await;

            // Update stored document
            {
                let mut docs = self.documents.write().await;
                docs.insert(
                    uri,
                    DocumentState {
                        content,
                        parse: parsed,
                        tree,
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

    async fn document_link(&self, params: DocumentLinkParams) -> Result<Option<Vec<DocumentLink>>> {
        let uri = params.text_document.uri;

        let docs = self.documents.read().await;
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };

        let Some(tree) = &doc.tree else {
            return Ok(None);
        };

        let mut links = Vec::new();

        // Find schema declaration and create a link for it
        if let Some(range) = find_schema_declaration_range(tree, &doc.content)
            && let Ok(schema) = resolve_schema(tree, &uri)
        {
            links.push(DocumentLink {
                range,
                target: Some(schema.uri.clone()),
                tooltip: Some(format!("Open schema: {}", schema.uri)),
                data: None,
            });
        }

        Ok(Some(links))
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let docs = self.documents.read().await;
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };

        let Some(tree) = &doc.tree else {
            return Ok(None);
        };

        let offset = position_to_offset(&doc.content, position);

        // Try to resolve the schema for this document
        let resolved = resolve_schema(tree, &uri).ok();

        // Case 1: On the schema declaration line - jump to schema file
        if let Some(range) = find_schema_declaration_range(tree, &doc.content)
            && position >= range.start
            && position <= range.end
            && let Some(ref schema) = resolved
        {
            return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                uri: schema.uri.clone(),
                range: Range::default(),
            })));
        }

        // Case 2: On a field name in a doc - jump to schema definition
        if let Some(field_name) = find_field_key_at_offset(tree, offset)
            && let Some(ref schema) = resolved
            && let Some(field_range) = find_field_in_schema_source(&schema.source, &field_name)
        {
            return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                uri: schema.uri.clone(),
                range: field_range,
            })));
        }

        // Case 3: In a schema file - jump to first open doc that uses this field
        // Check if this looks like a schema file (has "schema" and "meta" blocks)
        if is_schema_file(tree)
            && let Some(field_name) = find_field_key_at_offset(tree, offset)
        {
            // Search open documents for one that uses this schema
            for (doc_uri, doc_state) in docs.iter() {
                if doc_uri == &uri {
                    continue; // Skip the schema file itself
                }
                if let Some(ref doc_tree) = doc_state.tree {
                    // Check if this doc references our schema
                    if let Ok(doc_schema) = resolve_schema(doc_tree, doc_uri)
                        && doc_schema.uri == uri
                    {
                        // This doc uses our schema - find the field
                        if let Some(field_range) =
                            find_field_in_doc(doc_tree, &field_name, &doc_state.content)
                        {
                            return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                                uri: doc_uri.clone(),
                                range: field_range,
                            })));
                        }
                    }
                }
            }
        }

        Ok(None)
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;

        let docs = self.documents.read().await;
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };

        let Some(tree) = &doc.tree else {
            return Ok(None);
        };

        let offset = position_to_offset(&doc.content, position);

        // Try to resolve the schema for this document
        let resolved = resolve_schema(tree, &uri).ok();

        // Case 1: Hover on schema declaration
        if let Some(range) = find_schema_declaration_range(tree, &doc.content)
            && position >= range.start
            && position <= range.end
            && let Some(ref schema) = resolved
        {
            let content = format!(
                "**Schema**: `{}`\n\nClick to open schema.",
                schema.uri.as_str()
            );
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: content,
                }),
                range: Some(range),
            }));
        }

        // Case 2: Try extension for domain-specific hover (takes priority over schema hover)
        if let Ok(schema_file) = load_document_schema(tree, &uri) {
            let schema_id = &schema_file.meta.id;
            tracing::debug!(%schema_id, "Trying extension for hover");
            if let Some(client) = self.extensions.get_client(schema_id).await {
                tracing::debug!("Got extension client, calling hover");
                let field_path = find_field_path_at_offset(tree, offset).unwrap_or_default();
                let context_obj = find_object_at_offset(tree, offset);
                let tagged_context = find_tagged_context_at_offset(tree, offset);

                let ext_params = ext::HoverParams {
                    document_uri: uri.to_string(),
                    cursor: ext::Cursor {
                        line: position.line,
                        character: position.character,
                        offset: offset as u32,
                    },
                    path: field_path,
                    context: context_obj.map(|c| Value {
                        tag: None,
                        payload: Some(styx_tree::Payload::Object(c.object)),
                        span: None,
                    }),
                    tagged_context,
                };

                match client.hover(ext_params).await {
                    Ok(Some(result)) => {
                        tracing::debug!(contents = %result.contents, "Extension returned hover");
                        let range = result.range.map(|r| Range {
                            start: Position {
                                line: r.start.line,
                                character: r.start.character,
                            },
                            end: Position {
                                line: r.end.line,
                                character: r.end.character,
                            },
                        });
                        return Ok(Some(Hover {
                            contents: HoverContents::Markup(MarkupContent {
                                kind: MarkupKind::Markdown,
                                value: result.contents,
                            }),
                            range,
                        }));
                    }
                    Ok(None) => {
                        tracing::debug!("Extension returned None for hover");
                        // Extension returned None, fall through to schema-based hover
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Extension hover failed");
                    }
                }
            }
        }

        // Case 3: Fallback to schema-based field hover
        if let Some(field_path) = find_field_path_at_offset(tree, offset)
            && let Some(ref schema) = resolved
        {
            let path_refs: Vec<&str> = field_path.iter().map(|s| s.as_str()).collect();

            if let Some(field_info) = get_field_info_from_schema(&schema.source, &path_refs) {
                // Create a link to the field in the schema
                let field_name = field_path.last().map(|s| s.as_str()).unwrap_or("");
                let field_range = find_field_in_schema_source(&schema.source, field_name);
                let schema_link = {
                    let mut link_uri = schema.uri.clone();
                    if let Some(range) = field_range {
                        let line = range.start.line + 1;
                        let col = range.start.character + 1;
                        link_uri.set_fragment(Some(&format!("L{}:{}", line, col)));
                    }
                    Some(link_uri)
                };

                let content = format_field_hover(
                    &field_path,
                    &field_info,
                    schema.uri.as_str(),
                    schema_link.as_ref(),
                );
                return Ok(Some(Hover {
                    contents: HoverContents::Markup(MarkupContent {
                        kind: MarkupKind::Markdown,
                        value: content,
                    }),
                    range: None,
                }));
            }
        }

        Ok(None)
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let docs = self.documents.read().await;
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };

        let Some(tree) = &doc.tree else {
            return Ok(None);
        };

        // Get resolved schema
        let Ok(schema) = resolve_schema(tree, &uri) else {
            return Ok(None);
        };

        // Find the object context at cursor position for context-aware completion
        let offset = position_to_offset(&doc.content, position);
        let context = find_object_at_offset(tree, offset);
        let path = context.as_ref().map(|c| c.path.as_slice()).unwrap_or(&[]);

        tracing::debug!(?offset, ?path, "completion: finding fields at path");

        // Parse the schema to properly resolve type references and enum variants
        let schema_fields: Vec<(String, String)> = if let Ok(schema_file) =
            facet_styx::from_str::<facet_styx::SchemaFile>(&schema.source)
        {
            let fields = get_schema_fields_at_path(&schema_file, path);
            tracing::debug!(
                count = fields.len(),
                ?path,
                "schema fields from parsed SchemaFile"
            );
            fields
                .into_iter()
                .map(|f| {
                    // Build type string from schema info
                    let type_str = if f.optional {
                        format!("@optional({})", schema_to_type_str(&f.schema))
                    } else if let Some(default) = &f.default_value {
                        format!("@default({} {})", default, schema_to_type_str(&f.schema))
                    } else {
                        schema_to_type_str(&f.schema)
                    };
                    (f.name, type_str)
                })
                .collect()
        } else {
            tracing::debug!("Failed to parse schema, falling back to source-based lookup");
            // Fallback to source-based lookup if parsing fails
            get_schema_fields_from_source_at_path(&schema.source, path)
        };
        let existing_fields = context
            .as_ref()
            .map(|c| {
                c.object
                    .entries
                    .iter()
                    .filter_map(|e| e.key.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_else(|| get_existing_fields(tree));

        // Get current word being typed for fuzzy matching and text_edit range
        let word_info = get_word_range_at_position(&doc.content, position);
        let current_word = word_info.as_ref().map(|(w, _)| w.clone());
        // Range for text_edit - either replace the word being typed, or insert at cursor
        let edit_range = word_info.map(|(_, r)| r).unwrap_or_else(|| Range {
            start: position,
            end: position,
        });

        // Filter out existing fields
        let available_fields: Vec<_> = schema_fields
            .into_iter()
            .filter(|(name, _)| !existing_fields.contains(name))
            .collect();

        // If there are too many fields, apply filtering
        const MAX_COMPLETIONS: usize = 50;
        let filtered_fields = if available_fields.len() > MAX_COMPLETIONS {
            if let Some(ref word) = current_word {
                // Filter by prefix or similarity
                let mut scored: Vec<_> = available_fields
                    .into_iter()
                    .filter_map(|(name, type_str)| {
                        let name_lower = name.to_lowercase();
                        let word_lower = word.to_lowercase();

                        // Exact prefix match gets highest priority
                        if name_lower.starts_with(&word_lower) {
                            return Some((name, type_str, 0));
                        }

                        // Contains match
                        if name_lower.contains(&word_lower) {
                            return Some((name, type_str, 1));
                        }

                        // Fuzzy match using Levenshtein distance
                        let dist = levenshtein(&word_lower, &name_lower);
                        if dist <= 3 && dist < word.len().max(2) {
                            return Some((name, type_str, 2 + dist));
                        }

                        None
                    })
                    .collect();

                // Sort by score and take top MAX_COMPLETIONS
                scored.sort_by_key(|(_, _, score)| *score);
                scored
                    .into_iter()
                    .take(MAX_COMPLETIONS)
                    .map(|(name, type_str, _)| (name, type_str))
                    .collect()
            } else {
                // No word typed - just show required fields first, up to limit
                let mut sorted = available_fields;
                sorted.sort_by_key(|(_, type_str)| {
                    if type_str.starts_with("@optional") || type_str.starts_with("@default") {
                        1
                    } else {
                        0
                    }
                });
                sorted.into_iter().take(MAX_COMPLETIONS).collect()
            }
        } else {
            available_fields
        };

        // Build completion items from schema
        let mut items: Vec<CompletionItem> = filtered_fields
            .into_iter()
            .map(|(name, type_str)| {
                let is_optional =
                    type_str.starts_with("@optional") || type_str.starts_with("@default");

                // Add "did you mean" label modifier for fuzzy matches
                let label_details = current_word.as_ref().and_then(|word| {
                    let word_lower = word.to_lowercase();
                    let name_lower = name.to_lowercase();
                    if !name_lower.starts_with(&word_lower) && !word.is_empty() {
                        let dist = levenshtein(&word_lower, &name_lower);
                        if dist <= 3 && dist > 0 {
                            return Some(CompletionItemLabelDetails {
                                detail: Some(" (did you mean?)".to_string()),
                                description: None,
                            });
                        }
                    }
                    None
                });

                CompletionItem {
                    label: name.clone(),
                    label_details,
                    kind: Some(CompletionItemKind::FIELD),
                    detail: Some(type_str),
                    text_edit: Some(CompletionTextEdit::Edit(TextEdit {
                        range: edit_range,
                        new_text: format!("{} ", name),
                    })),
                    filter_text: Some(name.clone()),
                    sort_text: Some(if is_optional {
                        format!("1{}", name) // Optional fields sort after required
                    } else {
                        format!("0{}", name) // Required fields first
                    }),
                    ..Default::default()
                }
            })
            .collect();

        // Try to get completions from extension
        if let Ok(schema_file) = load_document_schema(tree, &uri) {
            let schema_id = &schema_file.meta.id;
            if let Some(client) = self.extensions.get_client(schema_id).await {
                let tagged_context = find_tagged_context_at_offset(tree, offset);
                let ext_params = ext::CompletionParams {
                    document_uri: uri.to_string(),
                    cursor: ext::Cursor {
                        line: position.line,
                        character: position.character,
                        offset: offset as u32,
                    },
                    path: path.iter().map(|s| s.to_string()).collect(),
                    prefix: current_word.clone().unwrap_or_default(),
                    context: context.map(|c| Value {
                        tag: None,
                        payload: Some(styx_tree::Payload::Object(c.object.clone())),
                        span: None,
                    }),
                    tagged_context,
                };

                match client.completions(ext_params).await {
                    Ok(ext_items) => {
                        tracing::debug!(count = ext_items.len(), "Got completions from extension");
                        for item in ext_items {
                            items.push(convert_ext_completion(item, edit_range));
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Extension completion failed");
                    }
                }
            }
        }

        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        let mut actions = Vec::new();

        // Process each diagnostic to generate code actions (quickfixes)
        for diag in params.context.diagnostics {
            // Handle schema hint diagnostics (add @schema declaration)
            if diag.source.as_deref() == Some("styx-hints") {
                if let Some(data) = &diag.data
                    && let Some(fix_type) = data.get("type").and_then(|v| v.as_str())
                    && fix_type == "add_schema"
                    && let Some(declaration) = data.get("declaration").and_then(|v| v.as_str())
                {
                    let tool_name = data
                        .get("tool")
                        .and_then(|v| v.as_str())
                        .unwrap_or("schema");

                    // Action 1: Add the suggested schema declaration
                    {
                        let edit = TextEdit {
                            range: Range {
                                start: Position::new(0, 0),
                                end: Position::new(0, 0),
                            },
                            new_text: format!("{}\n\n", declaration),
                        };

                        let mut changes = std::collections::HashMap::new();
                        changes.insert(uri.clone(), vec![edit]);

                        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                            title: format!("Add {} schema declaration", tool_name),
                            kind: Some(CodeActionKind::QUICKFIX),
                            diagnostics: Some(vec![diag.clone()]),
                            edit: Some(WorkspaceEdit {
                                changes: Some(changes),
                                ..Default::default()
                            }),
                            is_preferred: Some(true),
                            ..Default::default()
                        }));
                    }

                    // Action 2: Stop reminding (insert @schema @ to explicitly disable)
                    {
                        let edit = TextEdit {
                            range: Range {
                                start: Position::new(0, 0),
                                end: Position::new(0, 0),
                            },
                            new_text: "@schema @\n\n".to_string(),
                        };

                        let mut changes = std::collections::HashMap::new();
                        changes.insert(uri.clone(), vec![edit]);

                        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                            title: "Don't use a schema for this file".to_string(),
                            kind: Some(CodeActionKind::QUICKFIX),
                            diagnostics: Some(vec![diag.clone()]),
                            edit: Some(WorkspaceEdit {
                                changes: Some(changes),
                                ..Default::default()
                            }),
                            is_preferred: Some(false),
                            ..Default::default()
                        }));
                    }
                }
                continue;
            }

            // Handle extension allowlist diagnostics
            if diag.source.as_deref() == Some("styx-extension") {
                if let Some(data) = &diag.data
                    && let Some(fix_type) = data.get("type").and_then(|v| v.as_str())
                    && fix_type == "allow_extension"
                    && let Some(command) = data.get("command").and_then(|v| v.as_str())
                {
                    // Code action that triggers the execute_command to allow the extension
                    actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                        title: format!("Allow LSP extension '{}'", command),
                        kind: Some(CodeActionKind::QUICKFIX),
                        diagnostics: Some(vec![diag.clone()]),
                        command: Some(Command {
                            title: format!("Allow LSP extension '{}'", command),
                            command: "styx.allowExtension".to_string(),
                            arguments: Some(vec![serde_json::json!({
                                "command": command,
                            })]),
                        }),
                        is_preferred: Some(true),
                        ..Default::default()
                    }));
                }
                continue;
            }

            // Only process styx-schema diagnostics below
            if diag.source.as_deref() != Some("styx-schema") {
                continue;
            }

            // Check for quickfix data
            if let Some(data) = &diag.data
                && let Some(fix_type) = data.get("type").and_then(|v| v.as_str())
                && fix_type == "rename_field"
                && let (Some(from), Some(to)) = (
                    data.get("from").and_then(|v| v.as_str()),
                    data.get("to").and_then(|v| v.as_str()),
                )
            {
                // Create a text edit to rename the field
                let edit = TextEdit {
                    range: diag.range,
                    new_text: to.to_string(),
                };

                let mut changes = std::collections::HashMap::new();
                changes.insert(uri.clone(), vec![edit]);

                actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                    title: format!("Rename '{}' to '{}'", from, to),
                    kind: Some(CodeActionKind::QUICKFIX),
                    diagnostics: Some(vec![diag.clone()]),
                    edit: Some(WorkspaceEdit {
                        changes: Some(changes),
                        ..Default::default()
                    }),
                    is_preferred: Some(true),
                    ..Default::default()
                }));
            }
        }

        // Add schema-based refactoring actions
        let docs = self.documents.read().await;
        if let Some(doc) = docs.get(&uri)
            && let Some(ref tree) = doc.tree
        {
            // Try to load the schema
            if let Ok(schema_file) = load_document_schema(tree, &uri) {
                // Find the object at cursor position
                let cursor_offset = position_to_offset(&doc.content, params.range.start);
                let object_ctx = find_object_at_offset(tree, cursor_offset);

                // Get schema fields for the current context (root or nested)
                let (schema_fields, existing_fields, context_name) =
                    if let Some(ref ctx) = object_ctx {
                        let fields = get_schema_fields_at_path(&schema_file, &ctx.path);
                        let existing: Vec<String> = ctx
                            .object
                            .entries
                            .iter()
                            .filter_map(|e| e.key.as_str().map(String::from))
                            .collect();
                        let name = if ctx.path.is_empty() {
                            String::new()
                        } else {
                            format!(" in '{}'", ctx.path.join("."))
                        };
                        (fields, existing, name)
                    } else {
                        let fields = get_schema_fields(&schema_file);
                        let existing = get_document_fields(tree);
                        (fields, existing, String::new())
                    };

                // Find missing fields
                let missing_required: Vec<_> = schema_fields
                    .iter()
                    .filter(|f| !f.optional && !existing_fields.contains(&f.name))
                    .collect();

                let missing_optional: Vec<_> = schema_fields
                    .iter()
                    .filter(|f| f.optional && !existing_fields.contains(&f.name))
                    .collect();

                // Find insert position and indentation within the current object
                let insert_info = if let Some(ref ctx) = object_ctx {
                    if let Some(span) = ctx.span {
                        // Find insertion point within this object
                        let obj_content = &doc.content[span.start as usize..span.end as usize];
                        let obj_insert = find_field_insert_position(obj_content);
                        // Adjust position to be relative to document start
                        let obj_start_pos = offset_to_position(&doc.content, span.start as usize);
                        InsertPosition {
                            position: Position {
                                line: obj_start_pos.line + obj_insert.position.line,
                                character: if obj_insert.position.line == 0 {
                                    obj_start_pos.character + obj_insert.position.character
                                } else {
                                    obj_insert.position.character
                                },
                            },
                            indent: obj_insert.indent,
                        }
                    } else {
                        find_field_insert_position(&doc.content)
                    }
                } else {
                    find_field_insert_position(&doc.content)
                };

                // Action: Fill required fields
                if !missing_required.is_empty() {
                    let new_text = generate_fields_text(&missing_required, &insert_info.indent);
                    let edit = TextEdit {
                        range: Range {
                            start: insert_info.position,
                            end: insert_info.position,
                        },
                        new_text,
                    };

                    let mut changes = std::collections::HashMap::new();
                    changes.insert(uri.clone(), vec![edit]);

                    actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                        title: format!(
                            "Fill {} required field{}{}",
                            missing_required.len(),
                            if missing_required.len() == 1 { "" } else { "s" },
                            context_name
                        ),
                        kind: Some(CodeActionKind::REFACTOR),
                        edit: Some(WorkspaceEdit {
                            changes: Some(changes),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }));
                }

                // Action: Fill all fields (required + optional)
                let all_missing: Vec<_> = schema_fields
                    .iter()
                    .filter(|f| !existing_fields.contains(&f.name))
                    .collect();

                if !all_missing.is_empty() && !missing_optional.is_empty() {
                    let new_text = generate_fields_text(&all_missing, &insert_info.indent);
                    let edit = TextEdit {
                        range: Range {
                            start: insert_info.position,
                            end: insert_info.position,
                        },
                        new_text,
                    };

                    let mut changes = std::collections::HashMap::new();
                    changes.insert(uri.clone(), vec![edit]);

                    actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                        title: format!(
                            "Fill all {} field{}{}",
                            all_missing.len(),
                            if all_missing.len() == 1 { "" } else { "s" },
                            context_name
                        ),
                        kind: Some(CodeActionKind::REFACTOR),
                        edit: Some(WorkspaceEdit {
                            changes: Some(changes),
                            ..Default::default()
                        }),
                        ..Default::default()
                    }));
                }

                // Action: Reorder fields to match schema (only at root level for now)
                if object_ctx
                    .as_ref()
                    .map(|c| c.path.is_empty())
                    .unwrap_or(true)
                {
                    let root_fields = get_schema_fields(&schema_file);
                    if let Some(reorder_edit) =
                        generate_reorder_edit(tree, &root_fields, &doc.content)
                    {
                        let mut changes = std::collections::HashMap::new();
                        changes.insert(uri.clone(), vec![reorder_edit]);

                        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                            title: "Reorder fields to match schema".to_string(),
                            kind: Some(CodeActionKind::SOURCE_ORGANIZE_IMPORTS),
                            edit: Some(WorkspaceEdit {
                                changes: Some(changes),
                                ..Default::default()
                            }),
                            ..Default::default()
                        }));
                    }
                }
            }

            // Separator toggle actions (don't need schema)
            let cursor_offset = position_to_offset(&doc.content, params.range.start);
            if let Some(ctx) = find_object_at_offset(tree, cursor_offset) {
                // Only offer toggle for non-root objects with entries
                if let Some(span) = ctx.span
                    && !ctx.object.entries.is_empty()
                {
                    let context_name = if ctx.path.is_empty() {
                        "object".to_string()
                    } else {
                        format!("'{}'", ctx.path.last().unwrap_or(&"object".to_string()))
                    };

                    match ctx.object.separator {
                        styx_tree::Separator::Newline => {
                            // Offer to convert to comma-separated (inline)
                            if let Some(edit) = generate_separator_toggle_edit(
                                &ctx.object,
                                span,
                                &doc.content,
                                styx_tree::Separator::Comma,
                            ) {
                                let mut changes = std::collections::HashMap::new();
                                changes.insert(uri.clone(), vec![edit]);

                                actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                                    title: format!(
                                        "Convert {} to inline (comma-separated)",
                                        context_name
                                    ),
                                    kind: Some(CodeActionKind::REFACTOR),
                                    edit: Some(WorkspaceEdit {
                                        changes: Some(changes),
                                        ..Default::default()
                                    }),
                                    ..Default::default()
                                }));
                            }
                        }
                        styx_tree::Separator::Comma => {
                            // Offer to convert to newline-separated (multiline)
                            if let Some(edit) = generate_separator_toggle_edit(
                                &ctx.object,
                                span,
                                &doc.content,
                                styx_tree::Separator::Newline,
                            ) {
                                let mut changes = std::collections::HashMap::new();
                                changes.insert(uri.clone(), vec![edit]);

                                actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                                    title: format!(
                                        "Convert {} to multiline (newline-separated)",
                                        context_name
                                    ),
                                    kind: Some(CodeActionKind::REFACTOR),
                                    edit: Some(WorkspaceEdit {
                                        changes: Some(changes),
                                        ..Default::default()
                                    }),
                                    ..Default::default()
                                }));
                            }
                        }
                    }
                }
            }
        }

        if actions.is_empty() {
            Ok(None)
        } else {
            Ok(Some(actions))
        }
    }

    async fn execute_command(
        &self,
        params: ExecuteCommandParams,
    ) -> Result<Option<serde_json::Value>> {
        match params.command.as_str() {
            "styx.allowExtension" => {
                // Extract the command to allow from the arguments
                if let Some(arg) = params.arguments.first() {
                    if let Some(command) = arg.get("command").and_then(|v| v.as_str()) {
                        tracing::info!(command, "Allowing LSP extension");
                        self.extensions.allow(command.to_string()).await;

                        // Notify the user
                        self.client
                            .log_message(
                                MessageType::INFO,
                                format!("Allowed LSP extension: {}", command),
                            )
                            .await;

                        // Re-publish diagnostics for all open documents to clear the warning
                        // and trigger extension spawning
                        let docs = self.documents.read().await;
                        for (uri, doc) in docs.iter() {
                            let blocked_extension = if let Some(ref tree) = doc.tree {
                                self.check_for_extension(tree, uri).await
                            } else {
                                None
                            };
                            self.publish_diagnostics(
                                uri.clone(),
                                &doc.content,
                                &doc.parse,
                                doc.tree.as_ref(),
                                doc.version,
                                blocked_extension,
                            )
                            .await;
                        }

                        // Request inlay hint refresh so hints appear immediately
                        let _ = self.client.inlay_hint_refresh().await;
                    }
                }
                Ok(None)
            }
            _ => {
                tracing::warn!(command = %params.command, "Unknown command");
                Ok(None)
            }
        }
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let docs = self.documents.read().await;
        let doc = match docs.get(&uri) {
            Some(doc) => doc,
            None => return Ok(None),
        };

        let Some(tree) = &doc.tree else {
            return Ok(None);
        };

        let offset = position_to_offset(&doc.content, position);

        // Find what field we're on
        let Some(field_name) = find_field_key_at_offset(tree, offset) else {
            return Ok(None);
        };

        let mut locations = Vec::new();

        // Check if we're in a schema file
        if is_schema_file(tree) {
            // We're in a schema - find all docs that use this field
            for (doc_uri, doc_state) in docs.iter() {
                if doc_uri == &uri {
                    continue;
                }
                if let Some(ref doc_tree) = doc_state.tree {
                    // Check if this doc references our schema
                    if let Ok(doc_schema) = resolve_schema(doc_tree, doc_uri)
                        && doc_schema.uri == uri
                    {
                        // This doc uses our schema - find the field usage
                        if let Some(range) =
                            find_field_in_doc(doc_tree, &field_name, &doc_state.content)
                        {
                            locations.push(Location {
                                uri: doc_uri.clone(),
                                range,
                            });
                        }
                    }
                }
            }
        } else {
            // We're in a doc - find the schema definition and other docs using this field
            if let Ok(schema) = resolve_schema(tree, &uri) {
                // Add the schema definition location
                if let Some(field_range) = find_field_in_schema_source(&schema.source, &field_name)
                {
                    locations.push(Location {
                        uri: schema.uri.clone(),
                        range: field_range,
                    });

                    // Find other docs using the same schema
                    for (doc_uri, doc_state) in docs.iter() {
                        if let Some(ref doc_tree) = doc_state.tree
                            && let Ok(doc_schema) = resolve_schema(doc_tree, doc_uri)
                            && doc_schema.uri == schema.uri
                        {
                            // This doc uses the same schema
                            if let Some(range) =
                                find_field_in_doc(doc_tree, &field_name, &doc_state.content)
                            {
                                locations.push(Location {
                                    uri: doc_uri.clone(),
                                    range,
                                });
                            }
                        }
                    }
                }
            }
        }

        if locations.is_empty() {
            Ok(None)
        } else {
            Ok(Some(locations))
        }
    }

    async fn inlay_hint(&self, params: InlayHintParams) -> Result<Option<Vec<InlayHint>>> {
        let uri = params.text_document.uri;

        let docs = self.documents.read().await;
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };

        let Some(tree) = &doc.tree else {
            return Ok(None);
        };

        let mut hints = Vec::new();

        // Check for schema declaration
        if let Some(range) = find_schema_declaration_range(tree, &doc.content)
            && let Ok(schema) = resolve_schema(tree, &uri)
        {
            // Extract meta info from schema
            if let Some(meta) = get_schema_meta(&schema.source) {
                // Show schema name/description as inlay hint after the schema path
                // First line of description is the short desc
                let short_desc = meta
                    .description
                    .as_ref()
                    .and_then(|d| d.lines().next().map(|s| s.trim().to_string()));

                // Build the label: "— short desc (version)" or just "— short desc"
                let label = match (&short_desc, &meta.version) {
                    (Some(desc), Some(ver)) => format!(" — {} ({})", desc, ver),
                    (Some(desc), None) => format!(" — {}", desc),
                    (None, Some(ver)) => format!(" — v{}", ver),
                    (None, None) => String::new(),
                };

                if !label.is_empty() {
                    // Build tooltip with full description and id
                    let tooltip = {
                        let mut parts = Vec::new();
                        if let Some(id) = &meta.id {
                            parts.push(format!("Schema ID: {}", id));
                        }
                        // Show full description if it's multi-line
                        if let Some(desc) = &meta.description
                            && desc.contains('\n')
                        {
                            parts.push(String::new()); // blank line
                            parts.push(desc.clone());
                        }
                        if parts.is_empty() {
                            None
                        } else {
                            Some(InlayHintTooltip::String(parts.join("\n")))
                        }
                    };

                    hints.push(InlayHint {
                        position: range.end,
                        label: InlayHintLabel::String(label),
                        kind: Some(InlayHintKind::TYPE),
                        text_edits: None,
                        tooltip,
                        padding_left: Some(false),
                        padding_right: Some(true),
                        data: None,
                    });
                }
            }
        }

        // Try to get inlay hints from extension
        if let Ok(schema_file) = load_document_schema(tree, &uri) {
            let schema_id = &schema_file.meta.id;
            tracing::debug!(%schema_id, "Trying extension for inlay hints");
            if let Some(client) = self.extensions.get_client(schema_id).await {
                tracing::debug!("Got extension client, calling inlay_hints");
                let ext_params = ext::InlayHintParams {
                    document_uri: uri.to_string(),
                    range: ext::Range {
                        start: ext::Position {
                            line: params.range.start.line,
                            character: params.range.start.character,
                        },
                        end: ext::Position {
                            line: params.range.end.line,
                            character: params.range.end.character,
                        },
                    },
                    context: Some(tree.clone()),
                };

                match client.inlay_hints(ext_params).await {
                    Ok(ext_hints) => {
                        tracing::debug!(count = ext_hints.len(), "Got inlay hints from extension");
                        for hint in ext_hints {
                            // Convert position - extension uses byte offsets, we need line/character
                            let position =
                                offset_to_position(&doc.content, hint.position.character as usize);
                            hints.push(InlayHint {
                                position,
                                label: InlayHintLabel::String(hint.label),
                                kind: hint.kind.map(|k| match k {
                                    ext::InlayHintKind::Type => InlayHintKind::TYPE,
                                    ext::InlayHintKind::Parameter => InlayHintKind::PARAMETER,
                                }),
                                text_edits: None,
                                tooltip: None,
                                padding_left: Some(hint.padding_left),
                                padding_right: Some(hint.padding_right),
                                data: None,
                            });
                        }
                    }
                    Err(e) => {
                        tracing::warn!(error = %e, "Extension inlay hints failed");
                    }
                }
            }
        }

        if hints.is_empty() {
            Ok(None)
        } else {
            Ok(Some(hints))
        }
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document.uri;

        let docs = self.documents.read().await;
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };

        // Only format if document parsed successfully
        if doc.tree.is_none() {
            return Ok(None);
        }

        // Build indent string from editor preferences
        let indent = if params.options.insert_spaces {
            " ".repeat(params.options.tab_size as usize)
        } else {
            "\t".to_string()
        };

        // Format the document using CST formatter (preserves comments)
        let options = styx_format::FormatOptions::default().indent(
            // Leak the string since FormatOptions expects &'static str
            // This is fine since we're not going to format millions of times
            Box::leak(indent.into_boxed_str()),
        );

        let formatted = styx_format::format_source(&doc.content, options);

        // Only return an edit if the content changed
        if formatted == doc.content {
            return Ok(None);
        }

        // Replace the entire document
        let lines: Vec<&str> = doc.content.lines().collect();
        let last_line = lines.len().saturating_sub(1);
        let last_char = lines.last().map(|l| l.len()).unwrap_or(0);

        Ok(Some(vec![TextEdit {
            range: Range {
                start: Position {
                    line: 0,
                    character: 0,
                },
                end: Position {
                    line: last_line as u32,
                    character: last_char as u32,
                },
            },
            new_text: formatted,
        }]))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let uri = params.text_document.uri;

        let docs = self.documents.read().await;
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };

        let Some(tree) = &doc.tree else {
            return Ok(None);
        };

        let symbols = collect_document_symbols(tree, &doc.content);

        if symbols.is_empty() {
            Ok(None)
        } else {
            Ok(Some(DocumentSymbolResponse::Nested(symbols)))
        }
    }

    async fn on_type_formatting(
        &self,
        params: DocumentOnTypeFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;

        let docs = self.documents.read().await;
        let Some(doc) = docs.get(&uri) else {
            return Ok(None);
        };

        // Only handle newline character
        if params.ch != "\n" {
            return Ok(None);
        }

        // Convert position to byte offset
        let offset = position_to_offset(&doc.content, position);

        // Find the nesting depth at this offset using the CST
        let depth = find_nesting_depth_cst(&doc.parse, offset);

        // Build indent string from editor preferences
        let indent_unit = if params.options.insert_spaces {
            " ".repeat(params.options.tab_size as usize)
        } else {
            "\t".to_string()
        };

        // Build the indentation string based on nesting depth
        let indent_str = indent_unit.repeat(depth);

        // Insert indentation at the start of the current line
        Ok(Some(vec![TextEdit {
            range: Range {
                start: Position {
                    line: position.line,
                    character: 0,
                },
                end: Position {
                    line: position.line,
                    character: 0,
                },
            },
            new_text: indent_str,
        }]))
    }
}

/// Find the nesting depth (number of objects/sequences) containing the given offset.
///
/// Uses the CST for accurate position information. Counts OBJECT and SEQUENCE
/// nodes in the ancestor chain from the given offset.
fn find_nesting_depth_cst(parse: &styx_cst::Parse, offset: usize) -> usize {
    use styx_cst::{SyntaxKind, TextSize, TokenAtOffset};

    let root = parse.syntax();
    let offset = TextSize::new(offset as u32);

    // Find the token at this offset
    let token = match root.token_at_offset(offset) {
        TokenAtOffset::None => return 0,
        TokenAtOffset::Single(t) => {
            // If we're exactly at the end of a closing delimiter, we're semantically outside
            if matches!(t.kind(), SyntaxKind::R_BRACE | SyntaxKind::R_PAREN)
                && t.text_range().end() == offset
            {
                // We're at the end of a closing brace - count depth excluding this container
                let mut depth: usize = 0;
                let mut node = t.parent();
                while let Some(n) = node {
                    if matches!(n.kind(), SyntaxKind::OBJECT | SyntaxKind::SEQUENCE) {
                        depth += 1;
                    }
                    node = n.parent();
                }
                // Subtract 1 because we're outside the innermost container
                return depth.saturating_sub(1);
            }
            t
        }
        TokenAtOffset::Between(left, right) => {
            // When between two tokens, choose based on what makes semantic sense:
            // - If left is a closing delimiter (} or )), cursor is OUTSIDE, prefer right
            // - Otherwise prefer left (e.g., after newline inside a block)
            match left.kind() {
                SyntaxKind::R_BRACE | SyntaxKind::R_PAREN => right,
                _ => left,
            }
        }
    };

    // Walk up the ancestor chain, counting OBJECT and SEQUENCE nodes
    let mut depth = 0;
    let mut node = token.parent();

    while let Some(n) = node {
        match n.kind() {
            SyntaxKind::OBJECT | SyntaxKind::SEQUENCE => {
                depth += 1;
            }
            _ => {}
        }
        node = n.parent();
    }

    depth
}

/// Collect document symbols recursively from a value tree
fn collect_document_symbols(value: &Value, content: &str) -> Vec<DocumentSymbol> {
    let mut symbols = Vec::new();

    let Some(obj) = value.as_object() else {
        return symbols;
    };

    for entry in &obj.entries {
        // Skip the @ (schema declaration)
        if entry.key.is_unit() {
            continue;
        }

        let Some(name) = entry.key.as_str() else {
            continue;
        };

        let Some(key_span) = entry.key.span else {
            continue;
        };

        let Some(val_span) = entry.value.span else {
            continue;
        };

        // Get the value text for the detail
        let val_text = &content[val_span.start as usize..val_span.end as usize];

        // Determine the symbol kind based on the value type
        let (kind, detail, children): (SymbolKind, Option<String>, Vec<DocumentSymbol>) =
            if let Some(nested_obj) = entry.value.as_object() {
                // It's an object - recurse
                let nested_value = Value {
                    tag: entry.value.tag.clone(),
                    payload: Some(styx_tree::Payload::Object(nested_obj.clone())),
                    span: entry.value.span,
                };
                let children = collect_document_symbols(&nested_value, content);
                (SymbolKind::OBJECT, None, children)
            } else if entry.value.as_sequence().is_some() {
                (SymbolKind::ARRAY, Some("array".to_string()), Vec::new())
            } else if entry.value.as_str().is_some() {
                (SymbolKind::STRING, Some(val_text.to_string()), Vec::new())
            } else if entry.value.is_unit() {
                (SymbolKind::NULL, Some("@".to_string()), Vec::new())
            } else if entry.value.tag.is_some() {
                // Tagged value
                (SymbolKind::VARIABLE, Some(val_text.to_string()), Vec::new())
            } else {
                // Number, bool, or other scalar - just show the text
                (SymbolKind::CONSTANT, Some(val_text.to_string()), Vec::new())
            };

        let selection_range = Range {
            start: offset_to_position(content, key_span.start as usize),
            end: offset_to_position(content, key_span.end as usize),
        };

        let range = Range {
            start: offset_to_position(content, key_span.start as usize),
            end: offset_to_position(content, val_span.end as usize),
        };

        #[allow(deprecated)]
        symbols.push(DocumentSymbol {
            name: name.to_string(),
            detail,
            kind,
            tags: None,
            deprecated: None,
            range,
            selection_range,
            children: if children.is_empty() {
                None
            } else {
                Some(children)
            },
        });
    }

    symbols
}

/// Find the schema declaration range in the source
fn find_schema_declaration_range(tree: &Value, content: &str) -> Option<Range> {
    let obj = tree.as_object()?;

    for entry in &obj.entries {
        if entry.key.is_schema_tag() {
            let span = entry.value.span?;
            return Some(Range {
                start: offset_to_position(content, span.start as usize),
                end: offset_to_position(content, span.end as usize),
            });
        }
    }

    None
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

/// Convert LSP Position to byte offset
fn position_to_offset(content: &str, position: Position) -> usize {
    let mut current_line = 0u32;
    let mut current_col = 0u32;

    for (i, ch) in content.char_indices() {
        if current_line == position.line && current_col == position.character {
            return i;
        }
        if ch == '\n' {
            if current_line == position.line {
                // Position is past end of line
                return i;
            }
            current_line += 1;
            current_col = 0;
        } else {
            current_col += 1;
        }
    }

    content.len()
}

/// Find the field key at a given offset in the tree (returns just the immediate field name)
fn find_field_key_at_offset(tree: &Value, offset: usize) -> Option<String> {
    // Use the path-based function and return just the last element
    find_field_path_at_offset(tree, offset).and_then(|path| path.last().cloned())
}

/// A path segment - either a field name or a sequence index
#[derive(Debug, Clone)]
enum PathSegment {
    Field(String),
    Index(usize),
}

impl PathSegment {
    fn as_str(&self) -> String {
        match self {
            PathSegment::Field(name) => name.clone(),
            PathSegment::Index(i) => i.to_string(),
        }
    }
}

/// Find the field path at the given offset (e.g., ["logging", "format", "timestamp"] or ["items", "0", "name"])
fn find_field_path_at_offset(tree: &Value, offset: usize) -> Option<Vec<String>> {
    find_path_segments_at_offset(tree, offset)
        .map(|segments| segments.iter().map(|s| s.as_str()).collect())
}

/// Find path segments at the given offset, including sequence indices
fn find_path_segments_at_offset(tree: &Value, offset: usize) -> Option<Vec<PathSegment>> {
    find_path_in_value(tree, offset)
}

/// Recursively find path segments in a value
fn find_path_in_value(value: &Value, offset: usize) -> Option<Vec<PathSegment>> {
    // Check if we're in an object
    if let Some(obj) = value.as_object() {
        for entry in &obj.entries {
            // Check if cursor is on the key
            if let Some(span) = entry.key.span {
                let start = span.start as usize;
                let end = span.end as usize;
                if offset >= start
                    && offset < end
                    && let Some(key) = entry.key.as_str()
                {
                    return Some(vec![PathSegment::Field(key.to_string())]);
                }
            }
            // Check if cursor is within this entry's value
            if let Some(span) = entry.value.span {
                let start = span.start as usize;
                let end = span.end as usize;
                if offset >= start && offset < end {
                    // Recurse into the value
                    if let Some(mut nested_path) = find_path_in_value(&entry.value, offset) {
                        // Prepend current key to the path
                        if let Some(key) = entry.key.as_str() {
                            nested_path.insert(0, PathSegment::Field(key.to_string()));
                        }
                        return Some(nested_path);
                    }
                    // We're on the value but not in a nested field - return this key
                    if let Some(key) = entry.key.as_str() {
                        return Some(vec![PathSegment::Field(key.to_string())]);
                    }
                }
            }
        }
    }

    // Check if we're in a sequence
    if let Some(seq) = value.as_sequence() {
        for (index, item) in seq.items.iter().enumerate() {
            if let Some(span) = item.span {
                let start = span.start as usize;
                let end = span.end as usize;
                if offset >= start && offset < end {
                    // Recurse into the sequence item
                    if let Some(mut nested_path) = find_path_in_value(item, offset) {
                        nested_path.insert(0, PathSegment::Index(index));
                        return Some(nested_path);
                    }
                    // We're on this item but not deeper
                    return Some(vec![PathSegment::Index(index)]);
                }
            }
        }
    }

    None
}

/// Find a field definition in schema source by field name
fn find_field_in_schema_source(schema_source: &str, field_name: &str) -> Option<Range> {
    // Parse the schema file
    let tree = styx_tree::parse(schema_source).ok()?;

    // Navigate to schema.@ (the root schema definition)
    let obj = tree.as_object()?;

    for entry in &obj.entries {
        if entry.key.as_str() == Some("schema") {
            // Found schema block
            // Structure: schema { @ @object { name @string } }
            //   - entry.value is an Object containing the unit entry
            //   - get unit entry -> value is Object containing @object entry
            //   - get @object entry -> value is Object containing fields
            if let Some(schema_obj) = entry.value.as_object() {
                // Get the unit entry (@)
                if let Some(unit_value) = schema_obj.get_unit() {
                    // unit_value should be @object{...} - an object with @object key
                    if let Some(inner_obj) = unit_value.as_object() {
                        // This object has an @object entry
                        for inner_entry in &inner_obj.entries {
                            if inner_entry.key.tag_name() == Some("object") {
                                // Found @object - its value is the fields object
                                return find_field_in_object(
                                    &inner_entry.value,
                                    schema_source,
                                    field_name,
                                );
                            }
                        }
                    } else if unit_value.tag_name() == Some("object") {
                        // Direct @object value (no space before brace)
                        return find_field_in_object(unit_value, schema_source, field_name);
                    }
                }
            }
            // Fallback to original approach
            return find_field_in_object(&entry.value, schema_source, field_name);
        }
    }

    None
}

/// Recursively find a field in an object value
fn find_field_in_object(value: &Value, source: &str, field_name: &str) -> Option<Range> {
    // Check if it's a tagged value like @object{...}
    let obj = if let Some(obj) = value.as_object() {
        obj
    } else if value.tag.is_some() {
        // Tagged value - check if payload is an object
        match &value.payload {
            Some(styx_tree::Payload::Object(obj)) => obj,
            _ => return None,
        }
    } else {
        return None;
    };

    for entry in &obj.entries {
        if entry.key.as_str() == Some(field_name) {
            // Found the field!
            if let Some(span) = entry.key.span {
                return Some(Range {
                    start: offset_to_position(source, span.start as usize),
                    end: offset_to_position(source, span.end as usize),
                });
            }
        }

        // Check if this is the root definition (@) which contains the object schema
        if entry.key.is_unit()
            && let Some(found) = find_field_in_object(&entry.value, source, field_name)
        {
            return Some(found);
        }

        // Recurse into nested objects
        if let Some(found) = find_field_in_object(&entry.value, source, field_name) {
            return Some(found);
        }
    }

    None
}

/// Schema metadata extracted from the meta block
struct SchemaMeta {
    id: Option<String>,
    version: Option<String>,
    description: Option<String>,
}

/// Extract metadata from a schema file's meta block
fn get_schema_meta(schema_source: &str) -> Option<SchemaMeta> {
    let tree = styx_tree::parse(schema_source).ok()?;
    let obj = tree.as_object()?;

    for entry in &obj.entries {
        if entry.key.as_str() == Some("meta") {
            let meta_obj = entry.value.as_object()?;

            let id = meta_obj
                .get("id")
                .and_then(|v| v.as_str())
                .map(String::from);
            let version = meta_obj
                .get("version")
                .and_then(|v| v.as_str())
                .map(String::from);
            let description = meta_obj
                .get("description")
                .and_then(|v| v.as_str())
                .map(String::from);

            return Some(SchemaMeta {
                id,
                version,
                description,
            });
        }
    }

    None
}

/// Get the schema type for a field from schema source
/// Information about a field from the schema
struct FieldInfo {
    /// The type annotation (e.g., "@string", "@optional(@bool)")
    type_str: String,
    /// Doc comment if present
    doc_comment: Option<String>,
}

fn get_field_info_from_schema(schema_source: &str, field_path: &[&str]) -> Option<FieldInfo> {
    let tree = styx_tree::parse(schema_source).ok()?;
    let obj = tree.as_object()?;

    // Find the schema block
    let schema_value = obj
        .entries
        .iter()
        .find(|e| e.key.as_str() == Some("schema"))
        .map(|e| &e.value)?;

    get_field_info_in_schema(schema_value, field_path)
}

/// Recursively get field info from a schema value, following a path.
/// The schema_block is the full "schema { ... }" block for resolving type references.
fn get_field_info_in_schema(schema_block: &Value, path: &[&str]) -> Option<FieldInfo> {
    let schema_obj = schema_block.as_object()?;

    // Find the root schema (@ entry)
    let root = schema_obj
        .entries
        .iter()
        .find(|e| e.key.is_unit())
        .map(|e| &e.value)?;

    get_field_info_in_type(root, path, schema_obj)
}

/// Recursively get field info from a type value, following a path.
/// schema_defs contains all type definitions for resolving references like @Hint.
fn get_field_info_in_type(
    value: &Value,
    path: &[&str],
    schema_defs: &styx_tree::Object,
) -> Option<FieldInfo> {
    if path.is_empty() {
        return None;
    }

    let field_name = path[0];
    let remaining_path = &path[1..];

    // Check if value is a @seq type - if so, skip numeric indices and recurse into element type
    if let Some(seq_element_type) = extract_seq_element_type(value) {
        // If the path segment is a numeric index, skip it and continue into the element type
        if field_name.parse::<usize>().is_ok() {
            // Resolve type reference if needed (e.g., @seq(@Spec) -> look up Spec)
            let resolved = resolve_type_reference(seq_element_type, schema_defs);
            return get_field_info_in_type(resolved, remaining_path, schema_defs);
        }
    }

    // Check if value is a @map type - if so, any key is valid and returns the value type
    if let Some(map_value_type) = extract_map_value_type(value) {
        if remaining_path.is_empty() {
            // We're hovering on a map key - return the value type
            return Some(FieldInfo {
                type_str: format_type_concise(map_value_type),
                doc_comment: None, // Map keys don't have individual doc comments
            });
        } else {
            // Need to go deeper into the map value type
            // If it's a type reference like @Hint, resolve it first
            let resolved = resolve_type_reference(map_value_type, schema_defs);
            return get_field_info_in_type(resolved, remaining_path, schema_defs);
        }
    }

    // Check if value is an @object{@KeyType @ValueType} (catch-all pattern)
    // This handles schemas like `@ @object{@string @Decl}` where any string key is valid
    if let Some(catchall_value_type) = extract_object_catchall_value_type(value) {
        if remaining_path.is_empty() {
            // We're hovering on a catch-all key - return the value type
            return Some(FieldInfo {
                type_str: format_type_concise(catchall_value_type),
                doc_comment: None, // Catch-all keys don't have individual doc comments
            });
        } else {
            // Need to go deeper into the catch-all value type
            let resolved = resolve_type_reference(catchall_value_type, schema_defs);
            return get_field_info_in_type(resolved, remaining_path, schema_defs);
        }
    }

    // Check if value is an @enum type - if so, search all variants for the field
    // This handles cases like `Decl @enum{query @Query}` where we need to look
    // inside variant types (like @Query) to find nested fields (like `from`)
    if let Some(enum_obj) = extract_enum_object(value) {
        // Search all enum variants for the field
        for entry in &enum_obj.entries {
            // Resolve the variant's value type and recurse
            let resolved = resolve_type_reference(&entry.value, schema_defs);
            if let Some(info) = get_field_info_in_type(resolved, path, schema_defs) {
                return Some(info);
            }
        }
        return None;
    }

    // Try to get the object - handle various wrappings (also resolves type refs)
    let obj = extract_object_from_value_with_defs(value, schema_defs)?;

    for entry in &obj.entries {
        if entry.key.as_str() == Some(field_name) {
            if remaining_path.is_empty() {
                // This is the target field
                return Some(FieldInfo {
                    type_str: format_type_concise(&entry.value),
                    doc_comment: entry.doc_comment.clone(),
                });
            } else {
                // Need to go deeper - unwrap wrappers like @optional
                return get_field_info_in_type(&entry.value, remaining_path, schema_defs);
            }
        }

        // Check unit key entries (root schema)
        if entry.key.is_unit()
            && let Some(found) = get_field_info_in_type(&entry.value, path, schema_defs)
        {
            return Some(found);
        }
    }

    None
}

/// Resolve a type reference like @Hint to its definition in the schema.
/// Returns the original value if it's not a type reference.
fn resolve_type_reference<'a>(value: &'a Value, schema_defs: &'a styx_tree::Object) -> &'a Value {
    // Check if this is a type reference (tag with no payload or unit payload)
    if let Some(tag) = &value.tag {
        let is_type_ref = match &value.payload {
            None => true,
            Some(styx_tree::Payload::Scalar(s)) if s.text.is_empty() => true,
            _ => false,
        };

        if is_type_ref {
            // Look for a definition with this name in schema_defs
            for entry in &schema_defs.entries {
                if entry.key.as_str() == Some(&tag.name) {
                    return &entry.value;
                }
            }
        }
    }
    value
}

/// Extract the element type from a @seq type.
/// For @seq(@Spec), returns @Spec.
/// Also handles @optional(@seq(@T)) by unwrapping the optional first.
fn extract_seq_element_type(value: &Value) -> Option<&Value> {
    let tag = value.tag.as_ref()?;

    // Direct @seq(@T)
    if tag.name == "seq" {
        let seq = value.as_sequence()?;
        return seq.items.first();
    }

    // Handle @optional(@seq(@T)) - unwrap the optional and check inside
    if (tag.name == "optional" || tag.name == "default")
        && let Some(seq) = value.as_sequence()
    {
        for item in &seq.items {
            if let Some(element_type) = extract_seq_element_type(item) {
                return Some(element_type);
            }
        }
    }

    None
}

/// Extract the value type from a @map type.
/// For @map(@string @Hint), returns the @Hint value.
/// For @map(@V), returns @V (single-arg map uses value as both key and value).
fn extract_map_value_type(value: &Value) -> Option<&Value> {
    let tag = value.tag.as_ref()?;
    if tag.name != "map" {
        return None;
    }

    // @map has a sequence payload: @map(@K @V) or @map(@V)
    let seq = value.as_sequence()?;
    match seq.items.len() {
        1 => Some(&seq.items[0]), // @map(@V) - single type used as value
        2 => Some(&seq.items[1]), // @map(@K @V) - second is value type
        _ => None,
    }
}

/// Extract the inner object from an `@enum{...}` type.
///
/// In Styx schemas, `@enum{variant1 @Type1, variant2 @Type2}` defines an enum
/// with named variants. Each entry in the object is a variant name -> type mapping.
///
/// Returns Some(object) if the value is an @enum with object payload, None otherwise.
fn extract_enum_object(value: &Value) -> Option<&styx_tree::Object> {
    let tag = value.tag.as_ref()?;
    if tag.name != "enum" {
        return None;
    }
    value.as_object()
}

/// Extract the catch-all value type from an `@object{@KeyType @ValueType}` pattern.
///
/// In Styx schemas, `@object{@string @Decl}` means "an object where any string key
/// maps to a value of type @Decl". The key entry has a tagged key (like `@string`)
/// rather than a scalar key.
///
/// Returns Some(value_type) if the object has exactly one entry with a tagged key
/// (indicating a catch-all pattern), None otherwise.
fn extract_object_catchall_value_type(value: &Value) -> Option<&Value> {
    // Must be @object{...}
    let tag = value.tag.as_ref()?;
    if tag.name != "object" {
        return None;
    }

    let obj = value.as_object()?;

    // Catch-all pattern: exactly one entry with a tagged key (like @string)
    if obj.entries.len() == 1 {
        let entry = &obj.entries[0];
        // Key must be tagged (e.g., @string), not a scalar
        if entry.key.tag.is_some() && entry.key.payload.is_none() {
            return Some(&entry.value);
        }
    }

    None
}

/// Extract an object from a value, unwrapping wrappers like @optional, @default, etc.
/// Also resolves type references using schema_defs.
fn extract_object_from_value_with_defs<'a>(
    value: &'a Value,
    schema_defs: &'a styx_tree::Object,
) -> Option<&'a styx_tree::Object> {
    // First resolve if it's a type reference
    let resolved = resolve_type_reference(value, schema_defs);

    // Direct object
    if let Some(obj) = resolved.as_object() {
        return Some(obj);
    }

    // Tagged object like @object{...}
    if resolved.tag.is_some() {
        match &resolved.payload {
            Some(styx_tree::Payload::Object(obj)) => return Some(obj),
            // Handle @optional(@object{...}) or @optional(@TypeRef) - tag with sequence payload
            Some(styx_tree::Payload::Sequence(seq)) => {
                // Look for object inside the sequence (recursively resolving type refs)
                for item in &seq.items {
                    if let Some(obj) = extract_object_from_value_with_defs(item, schema_defs) {
                        return Some(obj);
                    }
                }
            }
            _ => {}
        }
    }

    None
}

/// Extract an object from a value, unwrapping wrappers like @optional, @default, etc.
/// Simple version without type reference resolution.
fn extract_object_from_value(value: &Value) -> Option<&styx_tree::Object> {
    // Direct object
    if let Some(obj) = value.as_object() {
        return Some(obj);
    }

    // Tagged object like @object{...}
    if value.tag.is_some() {
        match &value.payload {
            Some(styx_tree::Payload::Object(obj)) => return Some(obj),
            // Handle @optional(@object{...}) - tag with sequence payload
            Some(styx_tree::Payload::Sequence(seq)) => {
                // Look for @object inside the sequence
                for item in &seq.items {
                    if let Some(obj) = extract_object_from_value(item) {
                        return Some(obj);
                    }
                }
            }
            _ => {}
        }
    }

    None
}

/// Format a type value concisely (without expanding nested objects)
fn format_type_concise(value: &Value) -> String {
    let mut result = String::new();

    if let Some(tag) = &value.tag {
        result.push('@');
        result.push_str(&tag.name);
    }

    match &value.payload {
        None => {}
        Some(styx_tree::Payload::Scalar(s)) => {
            if value.tag.is_some() {
                result.push('(');
                result.push_str(&s.text);
                result.push(')');
            } else {
                result.push_str(&s.text);
            }
        }
        Some(styx_tree::Payload::Object(obj)) => {
            if value.tag.is_some() {
                // For tagged objects, show @tag{...} or @tag(@inner{...})
                result.push_str("{...}");
            } else {
                // Count fields for a hint
                let field_count = obj.entries.len();
                result.push_str(&format!("{{...}} ({} fields)", field_count));
            }
        }
        Some(styx_tree::Payload::Sequence(seq)) => {
            result.push('(');
            for (i, item) in seq.items.iter().enumerate() {
                if i > 0 {
                    result.push(' ');
                }
                result.push_str(&format_type_concise(item));
            }
            result.push(')');
        }
    }

    if result.is_empty() {
        "@".to_string() // unit
    } else {
        result
    }
}

/// Format a breadcrumb path like `@ › logging › format › timestamp`
/// The `@` can optionally be a link to the schema.
fn format_breadcrumb(path: &[String], schema_link: Option<&str>) -> String {
    let root = match schema_link {
        Some(uri) => format!("[@]({})", uri),
        None => "@".to_string(),
    };
    let mut result = root;
    for segment in path {
        result.push_str(" › ");
        result.push_str(segment);
    }
    result
}

/// Format a hover message for a field
fn format_field_hover(
    field_path: &[String],
    field_info: &FieldInfo,
    _schema_path: &str,
    schema_uri: Option<&Url>,
) -> String {
    let mut content = String::new();

    // Doc comment first (most important - what does this field do?)
    if let Some(doc) = &field_info.doc_comment {
        content.push_str(doc);
        content.push_str("\n\n");
    }

    // Breadcrumb path with @ as schema link and type at the end:
    // [@](schema-uri) › hints › tracey › schema: `@SchemaRef`
    let schema_link = schema_uri.map(|u| u.as_str());
    let breadcrumb = format_breadcrumb(field_path, schema_link);
    content.push_str(&format!("{}: `{}`", breadcrumb, field_info.type_str));

    content
}

/// Convert a Schema to a displayable type string
fn schema_to_type_str(schema: &facet_styx::Schema) -> String {
    use facet_styx::Schema;
    match schema {
        Schema::String(_) => "@string".to_string(),
        Schema::Int(_) => "@int".to_string(),
        Schema::Float(_) => "@float".to_string(),
        Schema::Bool => "@bool".to_string(),
        Schema::Unit => "@unit".to_string(),
        Schema::Any => "@any".to_string(),
        Schema::Type { name: Some(n) } => format!("@{}", n),
        Schema::Type { name: None } => "@type".to_string(),
        Schema::Object(_) => "@object{...}".to_string(),
        Schema::Seq(_) => "@seq(...)".to_string(),
        Schema::Map(_) => "@map(...)".to_string(),
        Schema::Enum(_) => "@enum{...}".to_string(),
        Schema::Union(_) => "@union(...)".to_string(),
        Schema::OneOf(_) => "@oneof(...)".to_string(),
        Schema::Flatten(_) => "@flatten(...)".to_string(),
        Schema::Literal(s) => format!("\"{}\"", s),
        Schema::Optional(opt) => format!("@optional({})", schema_to_type_str(&opt.0.0)),
        Schema::Default(def) => format!("@default(..., {})", schema_to_type_str(&def.0.1)),
        Schema::Deprecated(dep) => format!("@deprecated({})", schema_to_type_str(&dep.0.1)),
    }
}

/// Get schema fields at a specific path (for nested object completion)
fn get_schema_fields_from_source_at_path(
    schema_source: &str,
    path: &[String],
) -> Vec<(String, String)> {
    let mut fields = Vec::new();

    let Ok(tree) = styx_tree::parse(schema_source) else {
        return fields;
    };

    let Some(obj) = tree.as_object() else {
        return fields;
    };

    // Find the "schema" entry
    let schema_entry = obj
        .entries
        .iter()
        .find(|e| e.key.as_str() == Some("schema"));
    let Some(schema_entry) = schema_entry else {
        return fields;
    };

    // Navigate to the object at the given path
    let target_value = if path.is_empty() {
        // Root level - use the schema root
        &schema_entry.value
    } else {
        // Navigate into nested object
        match navigate_schema_path(&schema_entry.value, path) {
            Some(v) => v,
            None => return fields,
        }
    };

    collect_fields_from_object(target_value, schema_source, &mut fields);
    fields
}

/// Navigate to a field in a schema tree by path, returning the field's type value
fn navigate_schema_path<'a>(value: &'a Value, path: &[String]) -> Option<&'a Value> {
    if path.is_empty() {
        return Some(value);
    }

    let field_name = &path[0];
    let remaining = &path[1..];

    // Get the object (unwrapping @object{...} if needed)
    let obj = extract_object_from_value(value)?;

    for entry in &obj.entries {
        // Check named fields
        if entry.key.as_str() == Some(field_name.as_str()) {
            // Found the field - unwrap any wrappers like @optional to get to the inner type
            let inner = unwrap_type_wrappers(&entry.value);
            return navigate_schema_path(inner, remaining);
        }

        // Check unit key (root definition)
        if entry.key.is_unit()
            && let Some(result) = navigate_schema_path(&entry.value, path)
        {
            return Some(result);
        }
    }

    None
}

/// Unwrap type wrappers like @optional(...) to get to the inner type
fn unwrap_type_wrappers(value: &Value) -> &Value {
    // Check for @optional, @default, etc. which wrap the actual type
    if let Some(tag) = value.tag_name()
        && matches!(tag, "optional" | "default" | "deprecated")
    {
        // The inner type is in the payload
        match &value.payload {
            // @optional(@object{...}) - parenthesized, so it's a sequence with one item
            Some(styx_tree::Payload::Sequence(seq)) => {
                if let Some(first) = seq.items.first() {
                    return unwrap_type_wrappers(first);
                }
            }
            // @optional @object{...} - the object is the payload directly
            Some(styx_tree::Payload::Object(obj)) => {
                // Check for unit entry pattern
                for entry in &obj.entries {
                    if entry.key.is_unit() {
                        return unwrap_type_wrappers(&entry.value);
                    }
                }
            }
            _ => {}
        }
    }
    value
}

/// Collect fields from an object schema
fn collect_fields_from_object(value: &Value, source: &str, fields: &mut Vec<(String, String)>) {
    let obj = if let Some(obj) = value.as_object() {
        obj
    } else if value.tag.is_some() {
        match &value.payload {
            Some(styx_tree::Payload::Object(obj)) => obj,
            _ => return,
        }
    } else {
        return;
    };

    for entry in &obj.entries {
        // If it's a named field (not @), add it
        if let Some(name) = entry.key.as_str()
            && let Some(span) = entry.value.span
        {
            let type_str = source[span.start as usize..span.end as usize].trim();
            fields.push((name.to_string(), type_str.to_string()));
        }

        // If it's the root definition (@), recurse into it
        if entry.key.is_unit() {
            collect_fields_from_object(&entry.value, source, fields);
        }
    }
}

/// Get existing field names in a document
fn get_existing_fields(tree: &Value) -> Vec<String> {
    let mut fields = Vec::new();

    if let Some(obj) = tree.as_object() {
        for entry in &obj.entries {
            if let Some(name) = entry.key.as_str() {
                fields.push(name.to_string());
            }
        }
    }

    fields
}

/// Get the current word being typed and its range in the document.
/// Returns (word, Range) where Range is the span of the word.
fn get_word_range_at_position(content: &str, position: Position) -> Option<(String, Range)> {
    let offset = position_to_offset(content, position);
    if offset == 0 {
        return None;
    }

    // Find the start of the current word by scanning backwards
    let before = &content[..offset];
    let word_start = before
        .rfind(|c: char| c.is_whitespace() || c == '{' || c == '}')
        .map(|i| i + 1)
        .unwrap_or(0);

    let word = &before[word_start..];
    if word.is_empty() {
        None
    } else {
        // Calculate the start position of the word
        let start_position = offset_to_position(content, word_start);
        let range = Range {
            start: start_position,
            end: position,
        };
        Some((word.to_string(), range))
    }
}

/// Compute Levenshtein distance between two strings
fn levenshtein(a: &str, b: &str) -> usize {
    let a_chars: Vec<char> = a.chars().collect();
    let b_chars: Vec<char> = b.chars().collect();
    let m = a_chars.len();
    let n = b_chars.len();

    if m == 0 {
        return n;
    }
    if n == 0 {
        return m;
    }

    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0; n + 1];

    for i in 1..=m {
        curr[0] = i;
        for j in 1..=n {
            let cost = if a_chars[i - 1] == b_chars[j - 1] {
                0
            } else {
                1
            };
            curr[j] = (prev[j] + 1).min(curr[j - 1] + 1).min(prev[j - 1] + cost);
        }
        std::mem::swap(&mut prev, &mut curr);
    }

    prev[n]
}

/// Convert an extension completion item to LSP completion item.
fn convert_ext_completion(item: ext::CompletionItem, edit_range: Range) -> CompletionItem {
    // Use text_edit if insert_text is provided (Zed ignores insert_text)
    let text_edit = item.insert_text.as_ref().map(|text| {
        CompletionTextEdit::Edit(TextEdit {
            range: edit_range,
            new_text: text.clone(),
        })
    });

    CompletionItem {
        label: item.label.clone(),
        kind: item.kind.map(|k| match k {
            ext::CompletionKind::Field => CompletionItemKind::FIELD,
            ext::CompletionKind::Value => CompletionItemKind::VALUE,
            ext::CompletionKind::Keyword => CompletionItemKind::KEYWORD,
            ext::CompletionKind::Type => CompletionItemKind::CLASS,
        }),
        detail: item.detail,
        documentation: item.documentation.map(|d| {
            Documentation::MarkupContent(MarkupContent {
                kind: MarkupKind::Markdown,
                value: d,
            })
        }),
        sort_text: item.sort_text,
        text_edit,
        filter_text: Some(item.label),
        // Extension completions sort after schema completions
        preselect: Some(false),
        ..Default::default()
    }
}

/// Check if a tree looks like a schema file (has "schema" and "meta" blocks)
fn is_schema_file(tree: &Value) -> bool {
    let Some(obj) = tree.as_object() else {
        return false;
    };

    let mut has_schema = false;
    let mut has_meta = false;

    for entry in &obj.entries {
        match entry.key.as_str() {
            Some("schema") => has_schema = true,
            Some("meta") => has_meta = true,
            _ => {}
        }
    }

    has_schema && has_meta
}

/// Find a field usage in a document (not a schema)
fn find_field_in_doc(tree: &Value, field_name: &str, content: &str) -> Option<Range> {
    let obj = tree.as_object()?;

    for entry in &obj.entries {
        if entry.key.as_str() == Some(field_name)
            && let Some(span) = entry.key.span
        {
            return Some(Range {
                start: offset_to_position(content, span.start as usize),
                end: offset_to_position(content, span.end as usize),
            });
        }
    }

    None
}

/// Info about where to insert new fields
struct InsertPosition {
    position: Position,
    indent: String,
}

/// Find the position where new fields should be inserted and the indentation to use.
fn find_field_insert_position(content: &str) -> InsertPosition {
    let lines: Vec<&str> = content.lines().collect();

    // Find the last non-empty, non-comment line
    for (i, line) in lines.iter().enumerate().rev() {
        let trimmed = line.trim();
        if !trimmed.is_empty() && !trimmed.starts_with("//") && !trimmed.starts_with('#') {
            // Detect indentation from this line
            let indent_len = line.len() - line.trim_start().len();
            let indent = line[..indent_len].to_string();

            return InsertPosition {
                position: Position {
                    line: i as u32,
                    character: line.len() as u32,
                },
                indent,
            };
        }
    }

    // Fallback: end of document, no indentation
    InsertPosition {
        position: Position {
            line: lines.len().saturating_sub(1) as u32,
            character: lines.last().map(|l| l.len()).unwrap_or(0) as u32,
        },
        indent: String::new(),
    }
}

/// Generate text for inserting missing fields.
fn generate_fields_text(fields: &[&crate::schema_validation::SchemaField], indent: &str) -> String {
    use crate::schema_validation::generate_placeholder;

    let mut result = String::new();

    for field in fields {
        result.push('\n');
        result.push_str(indent);
        result.push_str(&field.name);
        result.push(' ');

        // Use default value if available, otherwise generate placeholder
        let value = field
            .default_value
            .clone()
            .unwrap_or_else(|| generate_placeholder(&field.schema));
        result.push_str(&value);
    }

    result
}

/// Generate a text edit to toggle an object's separator style.
///
/// This reformats the object content with the new separator style while preserving
/// the indentation context from the surrounding document.
fn generate_separator_toggle_edit(
    object: &styx_tree::Object,
    span: styx_tree::Span,
    content: &str,
    target_separator: styx_tree::Separator,
) -> Option<TextEdit> {
    // Create a new object with the target separator
    let mut new_obj = object.clone();
    new_obj.separator = target_separator;

    // Format the object with braces using the appropriate separator style
    let formatted =
        styx_format::format_object_braced(&new_obj, styx_format::FormatOptions::default());

    // For multiline, we need to adjust indentation to match the context
    let new_text = if target_separator == styx_tree::Separator::Comma {
        // Inline format - use as-is (already has braces)
        formatted.trim().to_string()
    } else {
        // Multiline: adjust indentation to match the document context
        let brace_offset = span.start as usize;
        let line_start = content[..brace_offset]
            .rfind('\n')
            .map(|i| i + 1)
            .unwrap_or(0);

        // Extract just the leading whitespace (not the key name)
        let line_prefix = &content[line_start..brace_offset];
        let base_indent: String = line_prefix
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect();

        // If no base indent needed (root level), use formatted output as-is
        if base_indent.is_empty() {
            formatted.trim().to_string()
        } else {
            // Prepend base_indent to each line (except the first which is just `{`)
            let mut result = String::new();
            for (i, line) in formatted.trim().lines().enumerate() {
                if i == 0 {
                    // First line is the opening brace - no extra indent
                    result.push_str(line);
                } else {
                    // Add base indent before the line's existing indentation
                    result.push_str(&base_indent);
                    result.push_str(line);
                }
                result.push('\n');
            }
            // Remove trailing newline
            result.trim_end().to_string()
        }
    };

    Some(TextEdit {
        range: Range {
            start: offset_to_position(content, span.start as usize),
            end: offset_to_position(content, span.end as usize),
        },
        new_text,
    })
}

/// Generate a text edit to reorder fields to match schema order.
/// Returns None if the fields are already in order or can't be reordered.
fn generate_reorder_edit(
    tree: &Value,
    schema_fields: &[crate::schema_validation::SchemaField],
    content: &str,
) -> Option<TextEdit> {
    let obj = tree.as_object()?;

    // Build schema field order map
    let schema_order: std::collections::HashMap<&str, usize> = schema_fields
        .iter()
        .enumerate()
        .map(|(i, f)| (f.name.as_str(), i))
        .collect();

    // Collect document entries with their positions
    let mut doc_entries: Vec<(Option<usize>, &styx_tree::Entry)> = obj
        .entries
        .iter()
        .map(|e| {
            let order = e
                .key
                .as_str()
                .and_then(|name| schema_order.get(name).copied());
            (order, e)
        })
        .collect();

    // Check if already in order
    let current_order: Vec<Option<usize>> = doc_entries.iter().map(|(o, _)| *o).collect();
    let mut sorted_order = current_order.clone();
    sorted_order.sort_by(|a, b| match (a, b) {
        (Some(x), Some(y)) => x.cmp(y),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    });

    if current_order == sorted_order {
        return None; // Already in order
    }

    // Sort entries by schema order (keep @ declaration first, unknowns last)
    doc_entries.sort_by(|(a_order, a_entry), (b_order, b_entry)| {
        // @ declaration always first
        if a_entry.key.is_unit() {
            return std::cmp::Ordering::Less;
        }
        if b_entry.key.is_unit() {
            return std::cmp::Ordering::Greater;
        }
        // Then by schema order
        match (a_order, b_order) {
            (Some(x), Some(y)) => x.cmp(y),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => std::cmp::Ordering::Equal,
        }
    });

    // Regenerate the document content, preserving original entry text
    let mut new_content = String::new();

    for (i, (_, entry)) in doc_entries.iter().enumerate() {
        if i > 0 {
            new_content.push('\n');
        }

        // Get the full entry text from the original source (key + value)
        // by using the key's start and value's end
        if let (Some(key_span), Some(val_span)) = (entry.key.span, entry.value.span) {
            // Find the start of the line to preserve indentation
            let key_start = key_span.start as usize;
            let line_start = content[..key_start].rfind('\n').map(|i| i + 1).unwrap_or(0);
            let indent = &content[line_start..key_start];

            new_content.push_str(indent);
            new_content.push_str(&content[key_start..val_span.end as usize]);
        }
    }

    // Find the range of the entire document content (excluding leading/trailing whitespace)
    let start_offset = obj.span?.start as usize;
    let end_offset = obj.span?.end as usize;

    Some(TextEdit {
        range: Range {
            start: offset_to_position(content, start_offset),
            end: offset_to_position(content, end_offset),
        },
        new_text: new_content,
    })
}

/// Run the LSP server on stdin/stdout
pub async fn run() -> eyre::Result<()> {
    // Set up logging (no ANSI colors since output goes to stderr for LSP)
    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .with_writer(std::io::stderr)
        .init();

    eprintln!(
        "styx-lsp PID: {} - attach debugger now!",
        std::process::id()
    );

    let stdin = tokio::io::stdin();
    let stdout = tokio::io::stdout();

    let (service, socket) = LspService::new(StyxLanguageServer::new);
    Server::new(stdin, stdout, socket).serve(service).await;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_field_in_schema_source() {
        let schema_source = r#"meta {
  name "test"
}
schema {
  @ @object{
    name @string
    port @int
  }
}"#;

        // Should find 'name' field
        let range = find_field_in_schema_source(schema_source, "name");
        assert!(range.is_some(), "should find 'name' field");

        // Should find 'port' field
        let range = find_field_in_schema_source(schema_source, "port");
        assert!(range.is_some(), "should find 'port' field");

        // Should not find 'unknown' field
        let range = find_field_in_schema_source(schema_source, "unknown");
        assert!(range.is_none(), "should not find 'unknown' field");
    }

    #[test]
    fn test_find_field_in_schema_no_space() {
        // Test @ @object{ ... } without space before brace (actual file format)
        let schema_no_space = r#"schema {
  @ @object{
    name @string
  }
}"#;

        let range = find_field_in_schema_source(schema_no_space, "name");
        assert!(
            range.is_some(),
            "should find 'name' without space before brace"
        );
    }

    #[test]
    fn test_offset_to_position() {
        let content = "line1\nline2\nline3";
        assert_eq!(offset_to_position(content, 0), Position::new(0, 0));
        assert_eq!(offset_to_position(content, 5), Position::new(0, 5));
        assert_eq!(offset_to_position(content, 6), Position::new(1, 0));
        assert_eq!(offset_to_position(content, 12), Position::new(2, 0));
    }

    #[test]
    fn test_levenshtein() {
        assert_eq!(levenshtein("", ""), 0);
        assert_eq!(levenshtein("abc", "abc"), 0);
        assert_eq!(levenshtein("abc", "ab"), 1);
        assert_eq!(levenshtein("port", "prot"), 2);
        assert_eq!(levenshtein("name", "nme"), 1);
    }

    #[test]
    fn test_separator_toggle_to_multiline() {
        // Test converting inline object to multiline
        let content = "logging {level debug, format {timestamp true}}";

        // Parse to get the object
        let tree = styx_tree::parse(content).unwrap();
        let obj = tree.as_object().unwrap();
        let logging_entry = obj.get("logging").unwrap();
        let logging_obj = logging_entry.as_object().unwrap();
        let span = logging_entry.span.unwrap();

        let edit = generate_separator_toggle_edit(
            logging_obj,
            span,
            content,
            styx_tree::Separator::Newline,
        );

        assert!(edit.is_some());
        let edit = edit.unwrap();

        // The indentation should be spaces, not "logging"
        assert!(
            !edit.new_text.contains("logging     "),
            "Should not have key name in indentation, got: {:?}",
            edit.new_text
        );
        assert!(
            edit.new_text.contains("    level"),
            "Should have proper indentation, got: {:?}",
            edit.new_text
        );
    }

    #[test]
    fn test_separator_toggle_to_inline() {
        // Test converting multiline object to inline
        let content = "config {\n    host localhost\n    port 8080\n}";

        let tree = styx_tree::parse(content).unwrap();
        let obj = tree.as_object().unwrap();
        let config_entry = obj.get("config").unwrap();
        let config_obj = config_entry.as_object().unwrap();
        let span = config_entry.span.unwrap();

        let edit =
            generate_separator_toggle_edit(config_obj, span, content, styx_tree::Separator::Comma);

        assert!(edit.is_some());
        let edit = edit.unwrap();

        // Should be inline with commas
        assert!(
            edit.new_text.contains("{host localhost, port 8080}"),
            "Should be inline with commas, got: {:?}",
            edit.new_text
        );
    }

    #[test]
    fn test_separator_toggle_nested_indentation() {
        // Test that nested objects get proper indentation
        let content = "outer {\n    inner {a 1, b 2}\n}";

        let tree = styx_tree::parse(content).unwrap();
        let obj = tree.as_object().unwrap();
        let outer_entry = obj.get("outer").unwrap();
        let outer_obj = outer_entry.as_object().unwrap();
        let inner_entry = outer_obj.get("inner").unwrap();
        let inner_obj = inner_entry.as_object().unwrap();
        let span = inner_entry.span.unwrap();

        let edit =
            generate_separator_toggle_edit(inner_obj, span, content, styx_tree::Separator::Newline);

        assert!(edit.is_some());
        let edit = edit.unwrap();

        // Inner content should be indented relative to "inner" line (4 spaces base + 4 more)
        assert!(
            edit.new_text.contains("        a"),
            "Should have 8 spaces indentation for nested content, got: {:?}",
            edit.new_text
        );
    }

    #[test]
    fn test_separator_toggle_with_nested_multiline_object() {
        // Exact test case: inline object containing a nested multiline object
        let content = "logging {level debug, format {\n    timestamp true\n}}";

        let tree = styx_tree::parse(content).unwrap();
        let obj = tree.as_object().unwrap();
        let logging_entry = obj.get("logging").unwrap();
        let logging_obj = logging_entry.as_object().unwrap();
        let span = logging_entry.span.unwrap();

        let edit = generate_separator_toggle_edit(
            logging_obj,
            span,
            content,
            styx_tree::Separator::Newline,
        );

        assert!(edit.is_some());
        let edit = edit.unwrap();

        // Expected output (at root level, so no base indent):
        let expected = "{\n    level debug\n    format {\n        timestamp true\n    }\n}";

        assert_eq!(
            edit.new_text, expected,
            "Multiline conversion should preserve nested object structure.\nGot:\n{}\n\nExpected:\n{}",
            edit.new_text, expected
        );
    }

    #[test]
    fn test_find_field_path_at_offset() {
        // Test finding path in nested objects
        let content = "logging {\n    format {\n        timestamp true\n    }\n}";
        let tree = styx_tree::parse(content).unwrap();

        // Position on "logging" key (offset 0-7)
        let path = find_field_path_at_offset(&tree, 3);
        assert_eq!(path, Some(vec!["logging".to_string()]));

        // Position on "format" key (inside logging object)
        // "logging {\n    format" - format starts at offset 14
        let path = find_field_path_at_offset(&tree, 16);
        assert_eq!(
            path,
            Some(vec!["logging".to_string(), "format".to_string()])
        );

        // Position on "timestamp" key (inside format object)
        // Find the offset of "timestamp"
        let timestamp_offset = content.find("timestamp").unwrap();
        let path = find_field_path_at_offset(&tree, timestamp_offset + 2);
        assert_eq!(
            path,
            Some(vec![
                "logging".to_string(),
                "format".to_string(),
                "timestamp".to_string()
            ])
        );
    }

    #[test]
    fn test_format_breadcrumb() {
        // Without schema link
        assert_eq!(format_breadcrumb(&[], None), "@");
        assert_eq!(
            format_breadcrumb(&["logging".to_string()], None),
            "@ › logging"
        );
        assert_eq!(
            format_breadcrumb(&["logging".to_string(), "format".to_string()], None),
            "@ › logging › format"
        );

        // With schema link - @ becomes a link
        assert_eq!(
            format_breadcrumb(&[], Some("file:///schema.styx")),
            "[@](file:///schema.styx)"
        );
        assert_eq!(
            format_breadcrumb(&["logging".to_string()], Some("file:///schema.styx")),
            "[@](file:///schema.styx) › logging"
        );
        assert_eq!(
            format_breadcrumb(
                &[
                    "logging".to_string(),
                    "format".to_string(),
                    "timestamp".to_string()
                ],
                Some("file:///schema.styx")
            ),
            "[@](file:///schema.styx) › logging › format › timestamp"
        );
    }

    #[test]
    fn test_get_field_info_from_schema() {
        let schema_source = r#"meta {
    id test
}
schema {
    @ @object{
        name @string
        logging @optional(@object{
            level @string
            format @optional(@object{
                timestamp @optional(@bool)
            })
        })
    }
}"#;

        // Top-level field
        let info = get_field_info_from_schema(schema_source, &["name"]);
        assert!(info.is_some(), "Should find 'name' field");
        let info = info.unwrap();
        assert_eq!(info.type_str, "@string");

        // Nested field in @optional(@object{...})
        let info = get_field_info_from_schema(schema_source, &["logging", "level"]);
        assert!(info.is_some(), "Should find 'logging.level' field");
        let info = info.unwrap();
        assert_eq!(info.type_str, "@string");

        // Deeply nested field
        let info = get_field_info_from_schema(schema_source, &["logging", "format", "timestamp"]);
        assert!(
            info.is_some(),
            "Should find 'logging.format.timestamp' field"
        );
        let info = info.unwrap();
        assert_eq!(info.type_str, "@optional(@bool)");

        // Test map type - hovering on a map key
        let map_schema = r#"meta { id test }
schema {
    @ @object{
        hints @map(@string @Hint)
    }
    Hint @object{
        title @string
        patterns @seq(@string)
    }
}"#;

        // Hovering on a map key like "captain" should return @Hint
        let info = get_field_info_from_schema(map_schema, &["hints", "captain"]);
        assert!(info.is_some(), "Should find map key 'hints.captain'");
        let info = info.unwrap();
        assert_eq!(info.type_str, "@Hint");

        // Hovering on a field inside a map value like "captain.title"
        let info = get_field_info_from_schema(map_schema, &["hints", "captain", "title"]);
        assert!(info.is_some(), "Should find 'hints.captain.title'");
        let info = info.unwrap();
        assert_eq!(info.type_str, "@string");

        // Test nested maps: @map(@string @map(@string @Foo))
        let nested_map_schema = r#"meta { id test }
schema {
    @ @object{
        data @map(@string @map(@string @Item))
    }
    Item @object{
        value @int
    }
}"#;

        // First level map key
        let info = get_field_info_from_schema(nested_map_schema, &["data", "outer"]);
        assert!(info.is_some(), "Should find 'data.outer'");
        assert_eq!(info.unwrap().type_str, "@map(@string @Item)");

        // Second level map key
        let info = get_field_info_from_schema(nested_map_schema, &["data", "outer", "inner"]);
        assert!(info.is_some(), "Should find 'data.outer.inner'");
        assert_eq!(info.unwrap().type_str, "@Item");

        // Field inside nested map value
        let info =
            get_field_info_from_schema(nested_map_schema, &["data", "outer", "inner", "value"]);
        assert!(info.is_some(), "Should find 'data.outer.inner.value'");
        assert_eq!(info.unwrap().type_str, "@int");

        // Test type reference at top level
        let type_ref_schema = r#"meta { id test }
schema {
    @ @object{
        config @Config
    }
    Config @object{
        port @int
        host @string
    }
}"#;

        let info = get_field_info_from_schema(type_ref_schema, &["config", "port"]);
        assert!(info.is_some(), "Should find 'config.port'");
        assert_eq!(info.unwrap().type_str, "@int");

        // Test optional type reference: @optional(@Config)
        let optional_ref_schema = r#"meta { id test }
schema {
    @ @object{
        config @optional(@Config)
    }
    Config @object{
        port @int
    }
}"#;

        let info = get_field_info_from_schema(optional_ref_schema, &["config", "port"]);
        assert!(
            info.is_some(),
            "Should find 'config.port' through @optional(@Config)"
        );
        assert_eq!(info.unwrap().type_str, "@int");

        // Test non-existent field
        let info = get_field_info_from_schema(schema_source, &["nonexistent"]);
        assert!(info.is_none(), "Should not find 'nonexistent' field");

        // Test non-existent nested field
        let info = get_field_info_from_schema(schema_source, &["logging", "nonexistent"]);
        assert!(
            info.is_none(),
            "Should not find 'logging.nonexistent' field"
        );

        // Test @seq type - field inside sequence element
        let seq_schema = r#"meta { id test }
schema {
    @ @object{
        /// List of specifications
        specs @seq(@object{
            /// Name of the spec
            name @string
            /// Prefix for annotations
            prefix @string
        })
    }
}"#;

        // Hovering on a field inside a sequence element (path includes index)
        // Path is ["specs", "0", "name"] where "0" is the sequence index
        let info = get_field_info_from_schema(seq_schema, &["specs", "0", "name"]);
        assert!(
            info.is_some(),
            "Should find 'specs[0].name' field inside @seq element"
        );
        let info = info.unwrap();
        assert_eq!(info.type_str, "@string");
        assert_eq!(info.doc_comment, Some("Name of the spec".to_string()));

        // Test nested @seq with type reference
        let nested_seq_schema = r#"meta { id test }
schema {
    @ @object{
        specs @seq(@Spec)
    }
    Spec @object{
        name @string
        impls @seq(@Impl)
    }
    Impl @object{
        /// Implementation name
        name @string
        include @seq(@string)
    }
}"#;

        // Field in first-level seq element
        let info = get_field_info_from_schema(nested_seq_schema, &["specs", "0", "name"]);
        assert!(info.is_some(), "Should find 'specs[0].name'");
        assert_eq!(info.unwrap().type_str, "@string");

        // Field in nested seq element (specs[0].impls[0].name)
        let info =
            get_field_info_from_schema(nested_seq_schema, &["specs", "0", "impls", "1", "name"]);
        assert!(info.is_some(), "Should find 'specs[0].impls[1].name'");
        let info = info.unwrap();
        assert_eq!(info.type_str, "@string");
        assert_eq!(info.doc_comment, Some("Implementation name".to_string()));

        // Test @optional(@seq(...))
        let optional_seq_schema = r#"meta { id test }
schema {
    @ @object{
        items @optional(@seq(@object{
            value @int
        }))
    }
}"#;

        let info = get_field_info_from_schema(optional_seq_schema, &["items", "0", "value"]);
        assert!(
            info.is_some(),
            "Should find 'items[0].value' through @optional(@seq(...))"
        );
        assert_eq!(info.unwrap().type_str, "@int");

        // Test @seq inside @map: @map(@string @seq(@Item))
        let seq_in_map_schema = r#"meta { id test }
schema {
    @ @object{
        groups @map(@string @seq(@Item))
    }
    Item @object{
        id @int
        label @string
    }
}"#;

        // Map key returns the value type (@seq(@Item))
        let info = get_field_info_from_schema(seq_in_map_schema, &["groups", "mygroup"]);
        assert!(info.is_some(), "Should find map key 'groups.mygroup'");
        assert_eq!(info.unwrap().type_str, "@seq(@Item)");

        // Field inside seq element inside map value
        let info =
            get_field_info_from_schema(seq_in_map_schema, &["groups", "mygroup", "0", "label"]);
        assert!(
            info.is_some(),
            "Should find 'groups.mygroup[0].label' through @map -> @seq"
        );
        assert_eq!(info.unwrap().type_str, "@string");

        // Test @map inside @seq: @seq(@map(@string @Value))
        let map_in_seq_schema = r#"meta { id test }
schema {
    @ @object{
        records @seq(@map(@string @Value))
    }
    Value @object{
        data @string
    }
}"#;

        // Map inside seq element
        let info = get_field_info_from_schema(map_in_seq_schema, &["records", "0", "somekey"]);
        assert!(
            info.is_some(),
            "Should find 'records[0].somekey' through @seq -> @map"
        );
        assert_eq!(info.unwrap().type_str, "@Value");

        // Field inside map value inside seq element
        let info =
            get_field_info_from_schema(map_in_seq_schema, &["records", "0", "somekey", "data"]);
        assert!(
            info.is_some(),
            "Should find 'records[0].somekey.data' through @seq -> @map -> @object"
        );
        assert_eq!(info.unwrap().type_str, "@string");

        // Test @default with @seq: @default([] @seq(@Item))
        let default_seq_schema = r#"meta { id test }
schema {
    @ @object{
        tags @default([] @seq(@object{
            name @string
        }))
    }
}"#;

        let info = get_field_info_from_schema(default_seq_schema, &["tags", "0", "name"]);
        assert!(
            info.is_some(),
            "Should find 'tags[0].name' through @default([] @seq(...))"
        );
        assert_eq!(info.unwrap().type_str, "@string");

        // Test nested sequences: @seq(@seq(@Item))
        let nested_seq_seq_schema = r#"meta { id test }
schema {
    @ @object{
        matrix @seq(@seq(@Cell))
    }
    Cell @object{
        value @int
    }
}"#;

        // First level - hovering on index returns None (we skip indices to get element type)
        let _info = get_field_info_from_schema(nested_seq_seq_schema, &["matrix", "0"]);

        // Second level - inside the inner seq
        let info =
            get_field_info_from_schema(nested_seq_seq_schema, &["matrix", "0", "1", "value"]);
        assert!(
            info.is_some(),
            "Should find 'matrix[0][1].value' through nested @seq"
        );
        assert_eq!(info.unwrap().type_str, "@int");

        // Test @seq with @optional element: @seq(@optional(@Item))
        let seq_optional_element_schema = r#"meta { id test }
schema {
    @ @object{
        maybe_items @seq(@optional(@Item))
    }
    Item @object{
        name @string
    }
}"#;

        let info =
            get_field_info_from_schema(seq_optional_element_schema, &["maybe_items", "0", "name"]);
        assert!(
            info.is_some(),
            "Should find 'maybe_items[0].name' through @seq(@optional(@Item))"
        );
        assert_eq!(info.unwrap().type_str, "@string");
    }

    #[test]
    fn test_doc_comments_in_schema() {
        let schema_source = r#"meta {
    id test
}
schema {
    @ @object{
        /// The server name
        name @string
    }
}"#;

        // First, let's verify the tree structure
        let tree = styx_tree::parse(schema_source).unwrap();
        let obj = tree.as_object().unwrap();

        // Find the schema entry
        let schema_entry = obj
            .entries
            .iter()
            .find(|e| e.key.as_str() == Some("schema"))
            .unwrap();
        let schema_obj = schema_entry.value.as_object().unwrap();

        // Find the @ entry (unit key)
        let unit_entry = schema_obj.entries.iter().find(|e| e.key.is_unit()).unwrap();

        // The value is @object{...} - check if it's an object with entries
        println!("unit_entry.value tag: {:?}", unit_entry.value.tag);
        println!(
            "unit_entry.value payload: {:?}",
            unit_entry.value.payload.is_some()
        );

        // Navigate into the @object
        if let Some(inner_obj) = unit_entry.value.as_object() {
            for entry in &inner_obj.entries {
                println!(
                    "Inner entry: key={:?}, tag={:?}, doc={:?}",
                    entry.key.as_str(),
                    entry.key.tag_name(),
                    entry.doc_comment
                );
            }
        }

        // Now test via our function
        let info = get_field_info_from_schema(schema_source, &["name"]);
        assert!(info.is_some(), "Should find 'name' field");
        let info = info.unwrap();
        assert_eq!(
            info.doc_comment,
            Some("The server name".to_string()),
            "Doc comment should be extracted"
        );
    }

    #[test]
    fn test_get_completion_fields_at_path() {
        // Schema with nested objects
        let schema_source = r#"meta { id test }
schema {
    @ @object{
        content @string
        output @string
        syntax_highlight @optional(@object{
            light_theme @string
            dark_theme @string
        })
    }
}"#;

        // At root level, should get root fields
        let root_fields = get_schema_fields_from_source_at_path(schema_source, &[]);
        let root_names: Vec<_> = root_fields.iter().map(|(n, _)| n.as_str()).collect();
        assert!(
            root_names.contains(&"content"),
            "Root should have 'content', got: {:?}",
            root_names
        );
        assert!(root_names.contains(&"output"), "Root should have 'output'");
        assert!(
            root_names.contains(&"syntax_highlight"),
            "Root should have 'syntax_highlight'"
        );
        assert!(
            !root_names.contains(&"light_theme"),
            "Root should NOT have 'light_theme' (it's nested)"
        );

        // Inside syntax_highlight, should get its fields
        // Debug: trace the schema structure
        let tree = styx_tree::parse(schema_source).unwrap();
        let obj = tree.as_object().unwrap();
        let schema_entry = obj
            .entries
            .iter()
            .find(|e| e.key.as_str() == Some("schema"))
            .unwrap();
        tracing::debug!(
            "schema_entry.value.tag: {:?}",
            schema_entry.value.tag_name()
        );

        // Look for syntax_highlight in the schema
        if let Some(inner_obj) = extract_object_from_value(&schema_entry.value) {
            for entry in &inner_obj.entries {
                tracing::debug!(
                    key = ?entry.key.as_str(),
                    tag = ?entry.key.tag_name(),
                    is_unit = entry.key.is_unit(),
                    "L1 entry"
                );

                // If unit entry, go deeper
                if entry.key.is_unit() {
                    tracing::debug!(tag = ?entry.value.tag_name(), "unit value");
                    if let Some(l2_obj) = extract_object_from_value(&entry.value) {
                        for l2_entry in &l2_obj.entries {
                            tracing::debug!(
                                key = ?l2_entry.key.as_str(),
                                tag = ?l2_entry.key.tag_name(),
                                "L2 entry"
                            );
                            if l2_entry.key.as_str() == Some("syntax_highlight") {
                                tracing::debug!("found syntax_highlight!");
                                tracing::debug!(tag = ?l2_entry.value.tag_name(), "value");
                                let unwrapped = unwrap_type_wrappers(&l2_entry.value);
                                tracing::debug!(tag = ?unwrapped.tag_name(), "unwrapped");
                            }
                        }
                    }
                }
            }
        }

        let nested_fields =
            get_schema_fields_from_source_at_path(schema_source, &["syntax_highlight".to_string()]);
        let nested_names: Vec<_> = nested_fields.iter().map(|(n, _)| n.as_str()).collect();
        assert!(
            nested_names.contains(&"light_theme"),
            "syntax_highlight should have 'light_theme', got: {:?}",
            nested_names
        );
        assert!(
            nested_names.contains(&"dark_theme"),
            "syntax_highlight should have 'dark_theme'"
        );
        assert!(
            !nested_names.contains(&"content"),
            "syntax_highlight should NOT have 'content' (it's at root)"
        );
    }

    #[test]
    fn test_find_object_context_at_cursor() {
        // Document with cursor inside nested object
        let content = "syntax_highlight {\n    light_theme foo\n    \n}";
        //                                                    ^ cursor here (line 2, after light_theme)
        let tree = styx_tree::parse(content).unwrap();

        // Find offset for the empty line inside the object
        let cursor_offset = content.find("    \n}").unwrap() + 4; // just before the newline

        let ctx = find_object_at_offset(&tree, cursor_offset);
        assert!(ctx.is_some(), "Should find object context at cursor");
        let ctx = ctx.unwrap();
        assert_eq!(
            ctx.path,
            vec!["syntax_highlight".to_string()],
            "Path should be ['syntax_highlight'], got: {:?}",
            ctx.path
        );
    }

    #[test]
    fn test_hover_with_catchall_key_schema() {
        // Schema where root is @object{@string @Decl} - a map with typed catch-all keys
        // This is used by dibs-queries where any string key maps to a @Decl
        let schema_source = r#"meta {id test}
schema {
    Decl @enum{
        /// A query declaration.
        query @Query
    }
    Query @object{
        /// Source table to query from.
        from @optional(@string)
        /// Filter conditions.
        where @optional(@object{@string @string})
    }
    @ @object{@string @Decl}
}"#;

        // Hovering on "AllProducts" in a document like:
        //   AllProducts @query{from product}
        // Should return type info: @Decl
        let info = get_field_info_from_schema(schema_source, &["AllProducts"]);
        assert!(
            info.is_some(),
            "Should find field info for dynamic key 'AllProducts' in catch-all schema"
        );
        let info = info.unwrap();
        // The type should be @Decl (the value type of the catch-all)
        assert!(
            info.type_str.contains("Decl"),
            "Type should reference Decl, got: {}",
            info.type_str
        );

        // Hovering on nested field "from" inside a query:
        //   AllProducts @query{from product}
        // Path would be ["AllProducts", "from"]
        let info = get_field_info_from_schema(schema_source, &["AllProducts", "from"]);
        assert!(
            info.is_some(),
            "Should find field info for 'from' inside a query"
        );
        let info = info.unwrap();
        assert!(
            info.type_str.contains("string"),
            "Type of 'from' should be string, got: {}",
            info.type_str
        );
    }

    /// Test helper: parse content with `⏐` as cursor position marker,
    /// return (content_without_marker, cursor_offset)
    fn parse_cursor(input: &str) -> (String, usize) {
        let cursor_pos = input
            .find('⏐')
            .expect("test input must contain ⏐ cursor marker");
        let content = input.replace('⏐', "");
        (content, cursor_pos)
    }

    #[test]
    fn test_nesting_depth_at_root() {
        // Cursor at root level, outside any braces
        let (content, offset) = parse_cursor("⏐name value");
        let parse = styx_cst::parse(&content);
        assert_eq!(find_nesting_depth_cst(&parse, offset), 0);
    }

    #[test]
    fn test_nesting_depth_after_root_entry() {
        // Cursor at root level after an entry
        let (content, offset) = parse_cursor("name value\n⏐");
        let parse = styx_cst::parse(&content);
        assert_eq!(find_nesting_depth_cst(&parse, offset), 0);
    }

    #[test]
    fn test_nesting_depth_inside_object() {
        // Cursor inside a top-level object
        let (content, offset) = parse_cursor("server {\n    ⏐\n}");
        let parse = styx_cst::parse(&content);
        assert_eq!(find_nesting_depth_cst(&parse, offset), 1);
    }

    #[test]
    fn test_nesting_depth_inside_object_with_content() {
        // Cursor inside object that has content
        let (content, offset) = parse_cursor("server {\n    host localhost\n    ⏐\n}");
        let parse = styx_cst::parse(&content);
        assert_eq!(find_nesting_depth_cst(&parse, offset), 1);
    }

    #[test]
    fn test_nesting_depth_nested_object() {
        // Cursor inside a nested object (depth 2)
        let (content, offset) = parse_cursor("server {\n    tls {\n        ⏐\n    }\n}");
        let parse = styx_cst::parse(&content);
        assert_eq!(find_nesting_depth_cst(&parse, offset), 2);
    }

    #[test]
    fn test_nesting_depth_deeply_nested() {
        // Cursor at depth 3
        let (content, offset) =
            parse_cursor("a {\n    b {\n        c {\n            ⏐\n        }\n    }\n}");
        let parse = styx_cst::parse(&content);
        assert_eq!(find_nesting_depth_cst(&parse, offset), 3);
    }

    #[test]
    fn test_nesting_depth_inside_sequence() {
        // Cursor inside a sequence
        let (content, offset) = parse_cursor("items (\n    ⏐\n)");
        let parse = styx_cst::parse(&content);
        assert_eq!(find_nesting_depth_cst(&parse, offset), 1);
    }

    #[test]
    fn test_nesting_depth_object_inside_sequence() {
        // Cursor inside an object that's inside a sequence
        let (content, offset) = parse_cursor("items (\n    {\n        ⏐\n    }\n)");
        let parse = styx_cst::parse(&content);
        assert_eq!(find_nesting_depth_cst(&parse, offset), 2);
    }

    #[test]
    fn test_nesting_depth_sequence_inside_object() {
        // Cursor inside a sequence that's inside an object
        let (content, offset) = parse_cursor("server {\n    ports (\n        ⏐\n    )\n}");
        let parse = styx_cst::parse(&content);
        assert_eq!(find_nesting_depth_cst(&parse, offset), 2);
    }

    #[test]
    fn test_nesting_depth_between_siblings() {
        // Cursor between two sibling entries at root
        let (content, offset) = parse_cursor("first {\n    a 1\n}\n⏐\nsecond {\n    b 2\n}");
        let parse = styx_cst::parse(&content);
        assert_eq!(find_nesting_depth_cst(&parse, offset), 0);
    }

    #[test]
    fn test_nesting_depth_just_after_open_brace() {
        // Cursor right after opening brace
        let (content, offset) = parse_cursor("server {⏐}");
        let parse = styx_cst::parse(&content);
        assert_eq!(find_nesting_depth_cst(&parse, offset), 1);
    }

    #[test]
    fn test_nesting_depth_just_before_close_brace() {
        // Cursor right before closing brace
        let (content, offset) = parse_cursor("server {\n    host localhost\n⏐}");
        let parse = styx_cst::parse(&content);
        assert_eq!(find_nesting_depth_cst(&parse, offset), 1);
    }

    #[test]
    fn test_nesting_depth_inline_object() {
        // Cursor inside inline object
        let (content, offset) = parse_cursor("server {host localhost, ⏐}");
        let parse = styx_cst::parse(&content);
        assert_eq!(find_nesting_depth_cst(&parse, offset), 1);
    }

    #[test]
    fn test_nesting_depth_tagged_object() {
        // Cursor inside a tagged object like @query{...}
        let (content, offset) = parse_cursor("AllProducts @query{\n    ⏐\n}");
        let parse = styx_cst::parse(&content);
        assert_eq!(find_nesting_depth_cst(&parse, offset), 1);
    }

    #[test]
    fn test_nesting_depth_nested_tagged_objects() {
        // Cursor inside nested tagged objects
        let (content, offset) =
            parse_cursor("AllProducts @query{\n    select {\n        ⏐\n    }\n}");
        let parse = styx_cst::parse(&content);
        assert_eq!(find_nesting_depth_cst(&parse, offset), 2);
    }

    // === Edge case tests ===

    #[test]
    fn test_nesting_depth_empty_object() {
        // Cursor inside empty object
        let (content, offset) = parse_cursor("empty {⏐}");
        let parse = styx_cst::parse(&content);
        assert_eq!(find_nesting_depth_cst(&parse, offset), 1);
    }

    #[test]
    fn test_nesting_depth_empty_sequence() {
        // Cursor inside empty sequence
        let (content, offset) = parse_cursor("empty (⏐)");
        let parse = styx_cst::parse(&content);
        assert_eq!(find_nesting_depth_cst(&parse, offset), 1);
    }

    #[test]
    fn test_nesting_depth_after_closing_brace() {
        // Cursor right after closing brace (back to root)
        let (content, offset) = parse_cursor("server { host localhost }⏐");
        let parse = styx_cst::parse(&content);
        assert_eq!(find_nesting_depth_cst(&parse, offset), 0);
    }

    #[test]
    fn test_nesting_depth_empty_document() {
        // Empty document
        let (content, offset) = parse_cursor("⏐");
        let parse = styx_cst::parse(&content);
        assert_eq!(find_nesting_depth_cst(&parse, offset), 0);
    }

    #[test]
    fn test_nesting_depth_whitespace_only() {
        // Document with only whitespace
        let (content, offset) = parse_cursor("   ⏐   ");
        let parse = styx_cst::parse(&content);
        assert_eq!(find_nesting_depth_cst(&parse, offset), 0);
    }

    #[test]
    fn test_nesting_depth_multiline_sequence_elements() {
        // Cursor between sequence elements
        let (content, offset) = parse_cursor("items (\n    a\n    ⏐\n    b\n)");
        let parse = styx_cst::parse(&content);
        assert_eq!(find_nesting_depth_cst(&parse, offset), 1);
    }

    #[test]
    fn test_nesting_depth_complex_dibs_like() {
        // Real-world-like dibs query structure
        let (content, offset) = parse_cursor(
            r#"AllProducts @query{
    from product
    where {deleted_at @null}
    select {
        id
        handle
        ⏐
    }
}"#,
        );
        let parse = styx_cst::parse(&content);
        assert_eq!(find_nesting_depth_cst(&parse, offset), 2);
    }

    #[test]
    fn test_nesting_depth_inside_where_clause() {
        // Cursor inside where clause object
        let (content, offset) = parse_cursor(
            r#"Query @query{
    where {
        status "published"
        ⏐
    }
}"#,
        );
        let parse = styx_cst::parse(&content);
        assert_eq!(find_nesting_depth_cst(&parse, offset), 2);
    }
}
