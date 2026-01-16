# Auto-Completion

## Goal

Provide intelligent completions based on schema context.

## Use Cases

### 1. Field name completion

```styx
@ server.schema.styx

na|  ← cursor here
```

Shows:
- `name` - The server's display name (@string)
- ~~`port`~~ - already present
- `enabled` - Whether server is enabled (@optional(@bool))
- `host` - Server hostname (@optional(@string))

### 2. Enum variant completion

```styx
log_level @|  ← cursor here
```

If schema says `log_level @enum{ debug info warn error }`, shows:
- `@debug`
- `@info`
- `@warn`
- `@error`

### 3. Boolean completion

```styx
enabled |  ← cursor here (schema expects @bool)
```

Shows:
- `true`
- `false`

### 4. Type completion (in schema files)

```styx
schema {
    @ @object{
        config @|  ← cursor here
    }
}
```

Shows:
- `@string`
- `@int`
- `@bool`
- `@object`
- `@seq`
- `@optional`
- `@ServerConfig` (custom type defined in this schema)

## LSP Protocol

Implement `textDocument/completion`:

```typescript
interface CompletionParams {
    textDocument: TextDocumentIdentifier;
    position: Position;
    context?: CompletionContext;
}

interface CompletionItem {
    label: string;
    kind?: CompletionItemKind;
    detail?: string;
    documentation?: string | MarkupContent;
    insertText?: string;
    insertTextFormat?: InsertTextFormat;
}
```

## Implementation

### 1. Completion handler

```rust
async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;
    
    let docs = self.documents.lock().await;
    let doc = docs.get(&uri)?;
    
    let offset = position_to_offset(&doc.content, position);
    let context = analyze_completion_context(&doc.tree, &doc.content, offset);
    
    let items = match context {
        CompletionContext::FieldName { parent_path, existing_fields } => {
            self.complete_field_names(&doc, &uri, &parent_path, &existing_fields).await?
        }
        CompletionContext::FieldValue { field_path } => {
            self.complete_field_value(&doc, &uri, &field_path).await?
        }
        CompletionContext::Tag => {
            self.complete_tag(&doc, &uri).await?
        }
        CompletionContext::SchemaType => {
            self.complete_schema_type(&doc).await?
        }
        CompletionContext::None => vec![],
    };
    
    Ok(Some(CompletionResponse::Array(items)))
}
```

### 2. Analyze completion context

```rust
enum CompletionContext {
    /// Completing a field name at the start of a line
    FieldName {
        parent_path: String,
        existing_fields: Vec<String>,
    },
    /// Completing a field value
    FieldValue {
        field_path: String,
    },
    /// Completing after @
    Tag,
    /// Completing a schema type (in schema files)
    SchemaType,
    /// No completion available
    None,
}

fn analyze_completion_context(tree: &Value, content: &str, offset: usize) -> CompletionContext {
    // Look at what's before the cursor
    let line_start = content[..offset].rfind('\n').map(|i| i + 1).unwrap_or(0);
    let line_prefix = &content[line_start..offset];
    
    // At start of line or after whitespace only → field name
    if line_prefix.trim().is_empty() {
        let (parent_path, existing) = find_completion_scope(tree, offset);
        return CompletionContext::FieldName {
            parent_path,
            existing_fields: existing,
        };
    }
    
    // After @ → tag completion
    if line_prefix.ends_with('@') {
        return CompletionContext::Tag;
    }
    
    // After field name and space → value completion
    if let Some(field) = extract_field_name(line_prefix) {
        return CompletionContext::FieldValue {
            field_path: field,
        };
    }
    
    CompletionContext::None
}
```

### 3. Complete field names

```rust
async fn complete_field_names(
    &self,
    doc: &DocumentState,
    uri: &Url,
    parent_path: &str,
    existing: &[String],
) -> Result<Vec<CompletionItem>> {
    let schema = load_schema_for_document(&doc.tree, uri)?;
    let object_schema = schema.find_object_at_path(parent_path)?;
    
    let mut items = Vec::new();
    
    for (name, field_schema) in &object_schema.fields {
        let Some(name) = name else { continue };  // Skip @ field
        
        // Skip already-present fields
        if existing.contains(name) {
            continue;
        }
        
        let type_display = schema_type_display(&field_schema.schema);
        let is_required = !matches!(field_schema.schema, 
            Schema::Optional(_) | Schema::Default(_));
        
        items.push(CompletionItem {
            label: name.clone(),
            kind: Some(CompletionItemKind::FIELD),
            detail: Some(type_display),
            documentation: field_schema.doc.as_ref().map(|d| {
                MarkupContent {
                    kind: MarkupKind::Markdown,
                    value: d.clone(),
                }
            }),
            insert_text: Some(format!("{} ", name)),  // Add space after
            sort_text: Some(if is_required {
                format!("0{}", name)  // Required fields first
            } else {
                format!("1{}", name)
            }),
            ..Default::default()
        });
    }
    
    Ok(items)
}
```

### 4. Complete field values

```rust
async fn complete_field_value(
    &self,
    doc: &DocumentState,
    uri: &Url,
    field_path: &str,
) -> Result<Vec<CompletionItem>> {
    let schema = load_schema_for_document(&doc.tree, uri)?;
    let field_schema = schema.find_field(field_path)?;
    
    match &field_schema.schema {
        Schema::Bool => Ok(vec![
            CompletionItem {
                label: "true".into(),
                kind: Some(CompletionItemKind::KEYWORD),
                ..Default::default()
            },
            CompletionItem {
                label: "false".into(),
                kind: Some(CompletionItemKind::KEYWORD),
                ..Default::default()
            },
        ]),
        Schema::Enum(enum_schema) => {
            Ok(enum_schema.variants.keys().map(|v| {
                CompletionItem {
                    label: format!("@{}", v),
                    kind: Some(CompletionItemKind::ENUM_MEMBER),
                    ..Default::default()
                }
            }).collect())
        }
        Schema::Optional(inner) => {
            // Recurse into optional's inner type
            self.complete_for_schema(&inner.0.0).await
        }
        _ => Ok(vec![]),
    }
}
```

### 5. Complete schema types

```rust
async fn complete_schema_type(&self, doc: &DocumentState) -> Result<Vec<CompletionItem>> {
    let mut items = vec![
        // Builtin types
        completion_item("@string", "String type"),
        completion_item("@int", "Integer type"),
        completion_item("@float", "Floating point type"),
        completion_item("@bool", "Boolean type"),
        completion_item("@any", "Any type"),
        completion_item("@object", "Object type"),
        completion_item("@seq", "Sequence type"),
        completion_item("@map", "Map type"),
        completion_item("@optional", "Optional wrapper"),
        completion_item("@enum", "Enumeration type"),
        completion_item("@union", "Union type"),
        completion_item("@default", "Default value wrapper"),
        completion_item("@deprecated", "Deprecation wrapper"),
    ];
    
    // Add custom types defined in this schema
    if let Some(tree) = &doc.tree {
        for type_name in find_custom_types(tree) {
            items.push(CompletionItem {
                label: format!("@{}", type_name),
                kind: Some(CompletionItemKind::CLASS),
                detail: Some("Custom type".into()),
                ..Default::default()
            });
        }
    }
    
    Ok(items)
}
```

### 6. Advertise capability

```rust
ServerCapabilities {
    completion_provider: Some(CompletionOptions {
        trigger_characters: Some(vec!["@".into(), " ".into()]),
        resolve_provider: Some(true),
        ..Default::default()
    }),
    ...
}
```

## Snippet Support

For complex types, use snippets:

```rust
CompletionItem {
    label: "@object".into(),
    insert_text: Some("@object{\n\t$1\n}".into()),
    insert_text_format: Some(InsertTextFormat::SNIPPET),
    ..Default::default()
}
```

## Files to Modify

1. `crates/styx-lsp/src/server.rs` - Add completion handler
2. `crates/styx-lsp/src/completion.rs` - New file for completion logic

## Testing

1. Start typing field name → shows available fields
2. Type `@` → shows available types/variants
3. After boolean field → shows true/false
4. After enum field → shows variants
5. Verify sorting (required fields first)
6. Verify filtering (existing fields hidden)

## Future Enhancements

- Fuzzy matching
- Completion for nested objects
- Completion for schema meta fields
- Import completions for external types
