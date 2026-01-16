//! LSP server implementation

use std::collections::HashMap;
use std::sync::Arc;

use styx_cst::{Parse, parse};
use styx_tree::Value;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::*;
use tower_lsp::{Client, LanguageServer, LspService, Server};

use crate::schema_validation::{
    SchemaRef, find_object_at_offset, get_document_fields, get_error_span, get_schema_fields,
    get_schema_fields_at_path, load_document_schema, resolve_schema_path, validate_against_schema,
};
use crate::semantic_tokens::{compute_semantic_tokens, semantic_token_legend};

/// Document state tracked by the server
struct DocumentState {
    /// Document content
    #[allow(dead_code)]
    content: String,
    /// Parsed CST
    parse: Parse,
    /// Parsed tree (for schema validation)
    #[allow(dead_code)]
    tree: Option<Value>,
    /// Document version
    #[allow(dead_code)]
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
    async fn publish_diagnostics(
        &self,
        uri: Url,
        content: &str,
        parsed: &Parse,
        tree: Option<&Value>,
        version: i32,
    ) {
        let diagnostics = self.compute_diagnostics(&uri, content, parsed, tree);
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
            if let Some((schema_ref, _)) = find_schema_declaration_with_range(tree, content) {
                // Try to resolve schema for related_information
                let schema_location = if let SchemaRef::External(ref path) = schema_ref {
                    resolve_schema_path(path, uri).and_then(|resolved| {
                        Url::from_file_path(&resolved).ok().map(|schema_uri| {
                            DiagnosticRelatedInformation {
                                location: Location {
                                    uri: schema_uri,
                                    range: Range::default(),
                                },
                                message: format!("schema defined in {}", path),
                            }
                        })
                    })
                } else {
                    None
                };

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
                // Document symbols (outline)
                document_symbol_provider: Some(OneOf::Left(true)),
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

        // Parse the document (CST)
        let parsed = parse(&content);

        // Parse into tree for schema validation
        let tree = styx_tree::parse(&content).ok();

        // Publish diagnostics
        self.publish_diagnostics(uri.clone(), &content, &parsed, tree.as_ref(), version)
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

            // Publish diagnostics
            self.publish_diagnostics(uri.clone(), &content, &parsed, tree.as_ref(), version)
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
        if let Some((schema_ref, range)) = find_schema_declaration_with_range(tree, &doc.content)
            && let SchemaRef::External(path) = schema_ref
            && let Some(resolved) = resolve_schema_path(&path, &uri)
            && let Ok(target_uri) = Url::from_file_path(&resolved)
        {
            links.push(DocumentLink {
                range,
                target: Some(target_uri),
                tooltip: Some(format!("Open schema: {}", path)),
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

        // Case 1: On the schema declaration line - jump to schema file
        if let Some((schema_ref, range)) = find_schema_declaration_with_range(tree, &doc.content) {
            // Check if cursor is within the schema declaration range
            if position >= range.start
                && position <= range.end
                && let SchemaRef::External(path) = schema_ref
                && let Some(resolved) = resolve_schema_path(&path, &uri)
                && let Ok(target_uri) = Url::from_file_path(&resolved)
            {
                return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                    uri: target_uri,
                    range: Range::default(),
                })));
            }
        }

        // Case 2: On a field name in a doc - jump to schema definition
        if let Some(field_name) = find_field_key_at_offset(tree, offset) {
            // Load the schema file
            let schema_decl = find_schema_declaration_with_range(tree, &doc.content);

            if let Some((SchemaRef::External(schema_path), _)) = schema_decl
                && let Some(resolved) = resolve_schema_path(&schema_path, &uri)
                && let Ok(schema_source) = std::fs::read_to_string(&resolved)
                && let Some(field_range) = find_field_in_schema_source(&schema_source, &field_name)
                && let Ok(target_uri) = Url::from_file_path(&resolved)
            {
                return Ok(Some(GotoDefinitionResponse::Scalar(Location {
                    uri: target_uri,
                    range: field_range,
                })));
            }
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
                    if let Some((SchemaRef::External(schema_path), _)) =
                        find_schema_declaration_with_range(doc_tree, &doc_state.content)
                        && let Some(resolved) = resolve_schema_path(&schema_path, doc_uri)
                        && let Ok(schema_uri) = Url::from_file_path(&resolved)
                        && schema_uri == uri
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

        // Case 1: Hover on schema declaration
        if let Some((schema_ref, range)) = find_schema_declaration_with_range(tree, &doc.content)
            && position >= range.start
            && position <= range.end
            && let SchemaRef::External(path) = schema_ref
        {
            let content = format!("**Schema**: `{}`\n\nClick to open schema file.", path);
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: content,
                }),
                range: Some(range),
            }));
        }

        // Case 2: Hover on field name
        if let Some(field_path) = find_field_path_at_offset(tree, offset)
            && let Some((SchemaRef::External(schema_path), _)) =
                find_schema_declaration_with_range(tree, &doc.content)
            && let Some(resolved) = resolve_schema_path(&schema_path, &uri)
            && let Ok(schema_source) = std::fs::read_to_string(&resolved)
        {
            // Convert path to &[&str] for the lookup
            let path_refs: Vec<&str> = field_path.iter().map(|s| s.as_str()).collect();

            if let Some(field_info) = get_field_info_from_schema(&schema_source, &path_refs) {
                // Create a file:// URI for the schema link with line number
                // Use the last field in the path for finding the definition location
                let field_name = field_path.last().map(|s| s.as_str()).unwrap_or("");
                let field_range = find_field_in_schema_source(&schema_source, field_name);
                let schema_link = if let Ok(mut schema_uri) = Url::from_file_path(&resolved) {
                    // Add line/column fragment for jumping to the field definition
                    if let Some(range) = field_range {
                        // LSP positions are 0-based, but URI fragments are typically 1-based
                        let line = range.start.line + 1;
                        let col = range.start.character + 1;
                        schema_uri.set_fragment(Some(&format!("L{}:{}", line, col)));
                    }
                    Some(schema_uri)
                } else {
                    None
                };
                let content = format_field_hover(
                    &field_path,
                    &field_info,
                    &schema_path,
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

        // Get schema fields
        let Some((SchemaRef::External(schema_path), _)) =
            find_schema_declaration_with_range(tree, &doc.content)
        else {
            return Ok(None);
        };

        let Some(resolved) = resolve_schema_path(&schema_path, &uri) else {
            return Ok(None);
        };

        let Ok(schema_source) = std::fs::read_to_string(&resolved) else {
            return Ok(None);
        };

        let schema_fields = get_schema_fields_from_source(&schema_source);
        let existing_fields = get_existing_fields(tree);

        // Get current word being typed for fuzzy matching
        let current_word = get_word_at_position(&doc.content, position);

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

        // Build completion items
        let items: Vec<CompletionItem> = filtered_fields
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
                    insert_text: Some(format!("{} ", name)),
                    sort_text: Some(if is_optional {
                        format!("1{}", name) // Optional fields sort after required
                    } else {
                        format!("0{}", name) // Required fields first
                    }),
                    ..Default::default()
                }
            })
            .collect();

        Ok(Some(CompletionResponse::Array(items)))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let uri = params.text_document.uri;
        let mut actions = Vec::new();

        // Process each diagnostic to generate code actions (quickfixes)
        for diag in params.context.diagnostics {
            // Only process our diagnostics
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
                    if let Some((SchemaRef::External(schema_path), _)) =
                        find_schema_declaration_with_range(doc_tree, &doc_state.content)
                        && let Some(resolved) = resolve_schema_path(&schema_path, doc_uri)
                        && let Ok(schema_uri) = Url::from_file_path(&resolved)
                        && schema_uri == uri
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
            let schema_decl = find_schema_declaration_with_range(tree, &doc.content);

            if let Some((SchemaRef::External(schema_path), _)) = schema_decl
                && let Some(resolved) = resolve_schema_path(&schema_path, &uri)
            {
                // Add the schema definition location
                if let Ok(schema_source) = std::fs::read_to_string(&resolved)
                    && let Some(field_range) =
                        find_field_in_schema_source(&schema_source, &field_name)
                    && let Ok(schema_uri) = Url::from_file_path(&resolved)
                {
                    locations.push(Location {
                        uri: schema_uri.clone(),
                        range: field_range,
                    });

                    // Find other docs using the same schema
                    for (doc_uri, doc_state) in docs.iter() {
                        if let Some(ref doc_tree) = doc_state.tree
                            && let Some((SchemaRef::External(other_schema), _)) =
                                find_schema_declaration_with_range(doc_tree, &doc_state.content)
                            && let Some(other_resolved) =
                                resolve_schema_path(&other_schema, doc_uri)
                            && let Ok(other_uri) = Url::from_file_path(&other_resolved)
                            && other_uri == schema_uri
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
        if let Some((SchemaRef::External(schema_path), range)) =
            find_schema_declaration_with_range(tree, &doc.content)
            && let Some(resolved) = resolve_schema_path(&schema_path, &uri)
            && let Ok(schema_source) = std::fs::read_to_string(&resolved)
        {
            // Extract meta info from schema
            if let Some(meta) = get_schema_meta(&schema_source) {
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

/// Find the schema declaration and its range in the source
fn find_schema_declaration_with_range(tree: &Value, content: &str) -> Option<(SchemaRef, Range)> {
    let obj = tree.as_object()?;

    for entry in &obj.entries {
        if entry.key.is_unit() {
            if let Some(path) = entry.value.as_str() {
                let span = entry.value.span?;
                let range = Range {
                    start: offset_to_position(content, span.start as usize),
                    end: offset_to_position(content, span.end as usize),
                };
                return Some((SchemaRef::External(path.to_string()), range));
            } else if entry.value.as_object().is_some() {
                let span = entry.value.span?;
                let range = Range {
                    start: offset_to_position(content, span.start as usize),
                    end: offset_to_position(content, span.end as usize),
                };
                return Some((SchemaRef::Inline(entry.value.clone()), range));
            }
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

    for entry in &obj.entries {
        if entry.key.as_str() == Some("schema") {
            return get_field_info_in_object(&entry.value, field_path);
        }
    }

    None
}

/// Recursively get field info from an object value, following a path
fn get_field_info_in_object(value: &Value, path: &[&str]) -> Option<FieldInfo> {
    if path.is_empty() {
        return None;
    }

    let field_name = path[0];
    let remaining_path = &path[1..];

    // Try to get the object - handle various wrappings
    let obj = extract_object_from_value(value)?;

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
                return get_field_info_in_object(&entry.value, remaining_path);
            }
        }

        // Check unit key entries (root schema)
        if entry.key.is_unit()
            && let Some(found) = get_field_info_in_object(&entry.value, path)
        {
            return Some(found);
        }
    }

    None
}

/// Extract an object from a value, unwrapping wrappers like @optional, @default, etc.
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
fn format_breadcrumb(path: &[String]) -> String {
    let mut result = String::from("@");
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
    schema_path: &str,
    schema_uri: Option<&Url>,
) -> String {
    let mut content = String::new();

    // Breadcrumb path
    let breadcrumb = format_breadcrumb(field_path);
    content.push_str(&format!("`{}`\n\n", breadcrumb));

    // Type annotation
    content.push_str(&format!("**Type:** `{}`\n", field_info.type_str));

    // Doc comment (rendered as markdown)
    if let Some(doc) = &field_info.doc_comment {
        content.push('\n');
        content.push_str(doc);
        content.push('\n');
    }

    content.push('\n');

    // Schema source link
    if let Some(uri) = schema_uri {
        content.push_str(&format!("Defined in [{}]({})", schema_path, uri));
    } else {
        content.push_str(&format!("Defined in {}", schema_path));
    }

    content
}

/// Get all fields defined in a schema
fn get_schema_fields_from_source(schema_source: &str) -> Vec<(String, String)> {
    let mut fields = Vec::new();

    let Ok(tree) = styx_tree::parse(schema_source) else {
        return fields;
    };

    let Some(obj) = tree.as_object() else {
        return fields;
    };

    for entry in &obj.entries {
        if entry.key.as_str() == Some("schema") {
            collect_fields_from_object(&entry.value, schema_source, &mut fields);
        }
    }

    fields
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

/// Get the word being typed at the cursor position
fn get_word_at_position(content: &str, position: Position) -> Option<String> {
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
        Some(word.to_string())
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_field_in_schema_source() {
        let schema_source = r#"meta {
  name "test"
}
schema {
  @ @object {
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
        assert_eq!(format_breadcrumb(&[]), "@");
        assert_eq!(format_breadcrumb(&["logging".to_string()]), "@ › logging");
        assert_eq!(
            format_breadcrumb(&["logging".to_string(), "format".to_string()]),
            "@ › logging › format"
        );
        assert_eq!(
            format_breadcrumb(&[
                "logging".to_string(),
                "format".to_string(),
                "timestamp".to_string()
            ]),
            "@ › logging › format › timestamp"
        );
        // With index
        assert_eq!(
            format_breadcrumb(&["items".to_string(), "0".to_string(), "name".to_string()]),
            "@ › items › 0 › name"
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
}
