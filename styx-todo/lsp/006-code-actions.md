# Code Actions (Quick Fixes)

## Goal

Provide automated fixes for common issues and refactoring actions.

## Use Cases

### 1. Fix unknown field (typo)

When error says "unknown field 'enbled', did you mean 'enabled'?":

```
Quick fix: Rename to 'enabled'
```

### 2. Add missing required field

When error says "missing required field 'name'":

```
Quick fix: Add 'name' field
```

Inserts:
```styx
name ""
```

### 3. Remove unknown field

When error says "unknown field 'foo'":

```
Quick fix: Remove 'foo' field
```

### 4. Convert to optional

When a field might not always be present:

```
Refactor: Make field optional in schema
```

### 5. Extract inline schema

When document has inline schema:

```
Refactor: Extract schema to file
```

## LSP Protocol

Implement `textDocument/codeAction`:

```typescript
interface CodeActionParams {
    textDocument: TextDocumentIdentifier;
    range: Range;
    context: CodeActionContext;
}

interface CodeAction {
    title: string;
    kind?: CodeActionKind;
    diagnostics?: Diagnostic[];
    isPreferred?: boolean;
    edit?: WorkspaceEdit;
    command?: Command;
}
```

## Implementation

### 1. Code action handler

```rust
async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
    let uri = params.text_document.uri;
    let range = params.range;
    let diagnostics = params.context.diagnostics;
    
    let docs = self.documents.lock().await;
    let doc = docs.get(&uri)?;
    
    let mut actions = Vec::new();
    
    // Generate quick fixes for diagnostics
    for diag in &diagnostics {
        actions.extend(self.quick_fixes_for_diagnostic(&doc, &uri, diag).await?);
    }
    
    // Generate refactoring actions for selection
    actions.extend(self.refactoring_actions(&doc, &uri, range).await?);
    
    Ok(Some(actions))
}
```

### 2. Quick fix: Rename typo

```rust
fn fix_typo(
    &self,
    doc: &DocumentState,
    uri: &Url,
    diag: &Diagnostic,
    wrong: &str,
    correct: &str,
) -> Option<CodeAction> {
    // Find the field in the document
    let span = find_field_key_span(&doc.tree, wrong)?;
    
    Some(CodeAction {
        title: format!("Rename to '{}'", correct),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diag.clone()]),
        is_preferred: Some(true),
        edit: Some(WorkspaceEdit {
            changes: Some(hashmap! {
                uri.clone() => vec![TextEdit {
                    range: span_to_range(span, &doc.content),
                    new_text: correct.to_string(),
                }]
            }),
            ..Default::default()
        }),
        ..Default::default()
    })
}
```

### 3. Quick fix: Add missing field

```rust
fn fix_missing_field(
    &self,
    doc: &DocumentState,
    uri: &Url,
    diag: &Diagnostic,
    field: &str,
    field_schema: &Schema,
) -> Option<CodeAction> {
    // Find where to insert (after last field in object)
    let insert_pos = find_insertion_point(&doc.tree, &doc.content)?;
    
    // Generate default value based on type
    let default_value = default_value_for_schema(field_schema);
    
    Some(CodeAction {
        title: format!("Add '{}' field", field),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diag.clone()]),
        is_preferred: Some(false),
        edit: Some(WorkspaceEdit {
            changes: Some(hashmap! {
                uri.clone() => vec![TextEdit {
                    range: Range {
                        start: insert_pos,
                        end: insert_pos,
                    },
                    new_text: format!("{} {}\n", field, default_value),
                }]
            }),
            ..Default::default()
        }),
        ..Default::default()
    })
}

fn default_value_for_schema(schema: &Schema) -> String {
    match schema {
        Schema::String(_) => "\"\"".into(),
        Schema::Int(_) => "0".into(),
        Schema::Float(_) => "0.0".into(),
        Schema::Bool => "false".into(),
        Schema::Object(_) => "{}".into(),
        Schema::Seq(_) => "[]".into(),
        Schema::Optional(_) => "null".into(),  // Or omit?
        _ => "".into(),
    }
}
```

### 4. Quick fix: Remove field

```rust
fn fix_remove_field(
    &self,
    doc: &DocumentState,
    uri: &Url,
    diag: &Diagnostic,
    field: &str,
) -> Option<CodeAction> {
    // Find the entire field line
    let (start, end) = find_field_line_range(&doc.tree, &doc.content, field)?;
    
    Some(CodeAction {
        title: format!("Remove '{}' field", field),
        kind: Some(CodeActionKind::QUICKFIX),
        diagnostics: Some(vec![diag.clone()]),
        is_preferred: Some(false),
        edit: Some(WorkspaceEdit {
            changes: Some(hashmap! {
                uri.clone() => vec![TextEdit {
                    range: Range { start, end },
                    new_text: String::new(),
                }]
            }),
            ..Default::default()
        }),
        ..Default::default()
    })
}
```

### 5. Refactoring: Extract schema

```rust
fn refactor_extract_schema(
    &self,
    doc: &DocumentState,
    uri: &Url,
) -> Option<CodeAction> {
    // Only if document has inline schema
    let inline_schema = find_inline_schema(&doc.tree)?;
    
    // Generate schema filename
    let doc_path = uri.to_file_path().ok()?;
    let schema_name = doc_path.file_stem()?.to_string_lossy();
    let schema_path = doc_path.with_extension("schema.styx");
    let schema_uri = Url::from_file_path(&schema_path).ok()?;
    
    // Format the schema
    let schema_content = format_as_schema_file(&inline_schema);
    
    Some(CodeAction {
        title: "Extract schema to file".into(),
        kind: Some(CodeActionKind::REFACTOR_EXTRACT),
        edit: Some(WorkspaceEdit {
            document_changes: Some(DocumentChanges::Operations(vec![
                // Create schema file
                DocumentChangeOperation::Op(ResourceOp::Create(CreateFile {
                    uri: schema_uri.clone(),
                    options: None,
                    annotation_id: None,
                })),
                // Write schema content
                DocumentChangeOperation::Edit(TextDocumentEdit {
                    text_document: OptionalVersionedTextDocumentIdentifier {
                        uri: schema_uri,
                        version: None,
                    },
                    edits: vec![OneOf::Left(TextEdit {
                        range: Range::default(),
                        new_text: schema_content,
                    })],
                }),
                // Update document to reference schema
                DocumentChangeOperation::Edit(TextDocumentEdit {
                    text_document: OptionalVersionedTextDocumentIdentifier {
                        uri: uri.clone(),
                        version: doc.version,
                    },
                    edits: vec![OneOf::Left(TextEdit {
                        range: inline_schema_range,
                        new_text: format!("@ {}.schema.styx", schema_name),
                    })],
                }),
            ])),
            ..Default::default()
        }),
        ..Default::default()
    })
}
```

### 6. Advertise capability

```rust
ServerCapabilities {
    code_action_provider: Some(CodeActionProviderCapability::Options(CodeActionOptions {
        code_action_kinds: Some(vec![
            CodeActionKind::QUICKFIX,
            CodeActionKind::REFACTOR,
            CodeActionKind::REFACTOR_EXTRACT,
        ]),
        resolve_provider: Some(true),
        ..Default::default()
    })),
    ...
}
```

## Files to Modify

1. `crates/styx-lsp/src/server.rs` - Add code_action handler
2. `crates/styx-lsp/src/code_actions.rs` - New file for code action logic

## Testing

1. Typo in field name → quick fix to rename
2. Missing required field → quick fix to add
3. Unknown field → quick fix to remove
4. Inline schema → refactor to extract

## Future Enhancements

- "Add all missing fields" action
- "Sort fields alphabetically" action
- "Convert to multiline/inline" format actions
- "Wrap in @optional" action
- "Generate schema from document" action
