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
    SchemaRef, find_schema_declaration, get_error_span, resolve_schema_path,
    validate_against_schema,
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
            if find_schema_declaration(tree).is_some() {
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

                            diagnostics.push(Diagnostic {
                                range,
                                severity: Some(DiagnosticSeverity::ERROR),
                                code: None,
                                code_description: None,
                                source: Some("styx-schema".to_string()),
                                message: error.message.clone(),
                                related_information: None,
                                tags: None,
                                data: None,
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
                                related_information: None,
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

        // Case 2: On a field name - jump to schema definition
        if let Some(field_name) = find_field_key_at_offset(tree, offset) {
            // Load the schema file
            if let Some((SchemaRef::External(schema_path), _)) =
                find_schema_declaration_with_range(tree, &doc.content)
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
        if let Some(field_name) = find_field_key_at_offset(tree, offset)
            && let Some((SchemaRef::External(schema_path), _)) =
                find_schema_declaration_with_range(tree, &doc.content)
            && let Some(resolved) = resolve_schema_path(&schema_path, &uri)
            && let Ok(schema_source) = std::fs::read_to_string(&resolved)
            && let Some(type_str) = get_field_type_from_schema(&schema_source, &field_name)
        {
            let content = format_field_hover(&field_name, &type_str, &schema_path);
            return Ok(Some(Hover {
                contents: HoverContents::Markup(MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: content,
                }),
                range: None,
            }));
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

        let schema_fields = get_schema_fields(&schema_source);
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

/// Find the field key at a given offset in the tree
fn find_field_key_at_offset(tree: &Value, offset: usize) -> Option<String> {
    let obj = tree.as_object()?;

    for entry in &obj.entries {
        if let Some(span) = entry.key.span {
            let start = span.start as usize;
            let end = span.end as usize;
            if offset >= start && offset < end {
                return entry.key.as_str().map(String::from);
            }
        }
        // Also check if we're on the value side and return the key
        if let Some(span) = entry.value.span {
            let start = span.start as usize;
            let end = span.end as usize;
            if offset >= start && offset < end {
                // Recurse into nested objects
                if let Some(nested) = entry.value.as_object()
                    && let Some(found) = find_field_key_at_offset(
                        &Value {
                            tag: None,
                            payload: Some(styx_tree::Payload::Object(nested.clone())),
                            span: entry.value.span,
                        },
                        offset,
                    )
                {
                    return Some(found);
                }
                // We're on the value, return the key name
                return entry.key.as_str().map(String::from);
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
            // Found schema block, look for the field inside
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

/// Get the schema type for a field from schema source
fn get_field_type_from_schema(schema_source: &str, field_name: &str) -> Option<String> {
    let tree = styx_tree::parse(schema_source).ok()?;
    let obj = tree.as_object()?;

    for entry in &obj.entries {
        if entry.key.as_str() == Some("schema") {
            return get_field_type_in_object(&entry.value, schema_source, field_name);
        }
    }

    None
}

/// Recursively get field type from an object value
fn get_field_type_in_object(value: &Value, source: &str, field_name: &str) -> Option<String> {
    let obj = if let Some(obj) = value.as_object() {
        obj
    } else if value.tag.is_some() {
        match &value.payload {
            Some(styx_tree::Payload::Object(obj)) => obj,
            _ => return None,
        }
    } else {
        return None;
    };

    for entry in &obj.entries {
        if entry.key.as_str() == Some(field_name) {
            // Found the field - get its type from the value span
            if let Some(span) = entry.value.span {
                let type_str = &source[span.start as usize..span.end as usize];
                return Some(type_str.trim().to_string());
            }
        }

        if entry.key.is_unit()
            && let Some(found) = get_field_type_in_object(&entry.value, source, field_name)
        {
            return Some(found);
        }

        if let Some(found) = get_field_type_in_object(&entry.value, source, field_name) {
            return Some(found);
        }
    }

    None
}

/// Format a hover message for a field
fn format_field_hover(field_name: &str, type_str: &str, schema_path: &str) -> String {
    let mut content = String::new();

    content.push_str(&format!("**{}** `{}`\n", field_name, type_str));
    content.push('\n');
    content.push_str(&format!("Defined in [{}]({})\n", schema_path, schema_path));

    content
}

/// Get all fields defined in a schema
fn get_schema_fields(schema_source: &str) -> Vec<(String, String)> {
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
