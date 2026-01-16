# Document Links

## Goal

Make schema references clickable in the editor.

## Use Cases

### 1. Schema declaration

```styx
@ server.schema.styx
  ^^^^^^^^^^^^^^^^^^
  clickable link
```

Clicking opens `server.schema.styx`.

### 2. Type references in schemas

```styx
schema {
    @ @object{
        config @ServerConfig
               ^^^^^^^^^^^^^
               clickable link to ServerConfig definition
    }
}
```

## LSP Protocol

Implement `textDocument/documentLink`:

```typescript
interface DocumentLinkParams {
    textDocument: TextDocumentIdentifier;
}

interface DocumentLink {
    range: Range;
    target?: URI;
    tooltip?: string;
    data?: any;  // For resolve
}
```

Optionally implement `documentLink/resolve` for lazy resolution.

## Implementation

### 1. Document link handler

```rust
async fn document_link(&self, params: DocumentLinkParams) -> Result<Option<Vec<DocumentLink>>> {
    let uri = params.text_document.uri;
    
    let docs = self.documents.lock().await;
    let doc = docs.get(&uri)?;
    
    let mut links = Vec::new();
    
    // Find schema declaration
    if let Some((schema_ref, range)) = find_schema_declaration_with_range(&doc.tree, &doc.content) {
        if let SchemaRef::External(path) = schema_ref {
            if let Some(resolved) = resolve_schema_path(&path, &uri) {
                links.push(DocumentLink {
                    range,
                    target: Some(Url::from_file_path(resolved).ok()?),
                    tooltip: Some(format!("Open schema: {}", path)),
                    data: None,
                });
            }
        }
    }
    
    // Find type references (in schema files)
    if is_schema_file(&uri) {
        links.extend(find_type_reference_links(&doc.tree, &doc.content, &uri));
    }
    
    Ok(Some(links))
}
```

### 2. Find schema declaration with range

```rust
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
            }
        }
    }
    
    None
}
```

### 3. Detect schema files

```rust
fn is_schema_file(uri: &Url) -> bool {
    uri.path().ends_with(".schema.styx")
}
```

### 4. Find type references in schemas

```rust
fn find_type_reference_links(tree: &Value, content: &str, uri: &Url) -> Vec<DocumentLink> {
    let mut links = Vec::new();
    
    // Walk the tree looking for type references like @ServerConfig
    visit_tree(tree, |value| {
        if let Some(tag) = &value.tag {
            // Check if it's a type reference (not a builtin)
            if !is_builtin_type(&tag.name) {
                if let Some(span) = value.span {
                    // This is a reference to a custom type
                    links.push(DocumentLink {
                        range: span_to_range(span, content),
                        target: None,  // Will resolve later
                        tooltip: Some(format!("Go to @{} definition", tag.name)),
                        data: Some(json!({ "type": tag.name.clone() })),
                    });
                }
            }
        }
    });
    
    links
}

fn is_builtin_type(name: &str) -> bool {
    matches!(name, 
        "string" | "int" | "float" | "bool" | "any" |
        "object" | "seq" | "map" | "optional" | "enum" |
        "union" | "flatten" | "default" | "deprecated"
    )
}
```

### 5. Resolve handler (optional)

For lazy resolution of type reference targets:

```rust
async fn document_link_resolve(&self, link: DocumentLink) -> Result<DocumentLink> {
    if let Some(data) = link.data {
        if let Some(type_name) = data.get("type").and_then(|v| v.as_str()) {
            // Find where this type is defined
            if let Some(location) = find_type_definition(type_name) {
                return Ok(DocumentLink {
                    target: Some(location.uri),
                    ..link
                });
            }
        }
    }
    Ok(link)
}
```

### 6. Advertise capability

```rust
ServerCapabilities {
    document_link_provider: Some(DocumentLinkOptions {
        resolve_provider: Some(true),
        work_done_progress_options: Default::default(),
    }),
    ...
}
```

## Visual Appearance

In most editors, document links:
- Show as underlined text
- Change cursor to pointer on hover
- Show tooltip on hover
- Open target on Ctrl+Click (or Cmd+Click)

## Files to Modify

1. `crates/styx-lsp/src/server.rs` - Add document_link handler
2. `crates/styx-lsp/src/links.rs` - New file for link detection logic

## Testing

1. Open document with `@ schema.styx`
2. Hover over schema path → shows tooltip
3. Ctrl+Click → opens schema file
4. In schema file, Ctrl+Click on type reference → jumps to definition

## Future Enhancements

- Links in inline schemas
- Links to external URL schemas (`@ https://example.com/schema.styx`)
- Preview on hover (peek definition)
