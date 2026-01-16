# Hover Information

## Goal

Show helpful information when hovering over elements in a Styx document.

## Use Cases

### 1. Hover over field name

```
port 8080
^^^^^
```

Shows:
```markdown
**port** `@int`

The port number the server listens on.

Constraints:
- minimum: 1
- maximum: 65535

Defined in [server.schema.styx:15:3](file:///path/to/server.schema.styx)
```

### 2. Hover over schema declaration

```
@ server.schema.styx
  ^^^^^^^^^^^^^^^^^^
```

Shows:
```markdown
**Schema**: server.schema.styx

Server configuration schema for the web application.

Version: 2026-01-16
```

### 3. Hover over tag

```
enabled @true
        ^^^^^
```

Shows:
```markdown
**@true** - Boolean literal

Type: `@bool`
```

### 4. Hover over type in schema

```
port @int(min: 1, max: 65535)
     ^^^^^^^^^^^^^^^^^^^^^^^^
```

Shows:
```markdown
**@int** - Integer type

Constraints:
- min: 1
- max: 65535
```

## LSP Protocol

Implement `textDocument/hover`:

```typescript
interface HoverParams {
    textDocument: TextDocumentIdentifier;
    position: Position;
}

interface Hover {
    contents: MarkupContent;
    range?: Range;
}
```

## Implementation

### 1. Hover handler

```rust
async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;
    
    let docs = self.documents.lock().await;
    let doc = docs.get(&uri)?;
    
    let offset = position_to_offset(&doc.content, position);
    let node = find_node_at_offset(&doc.tree, offset)?;
    
    let hover_content = match node {
        NodeAtOffset::FieldKey(name) => {
            self.hover_for_field(&doc, &uri, &name).await?
        }
        NodeAtOffset::SchemaDeclaration(path) => {
            self.hover_for_schema(&uri, &path).await?
        }
        NodeAtOffset::TagName(tag) => {
            self.hover_for_tag(&tag)?
        }
        _ => return Ok(None),
    };
    
    Ok(Some(Hover {
        contents: MarkupContent {
            kind: MarkupKind::Markdown,
            value: hover_content,
        },
        range: None,
    }))
}
```

### 2. Field hover content

```rust
async fn hover_for_field(&self, doc: &DocumentState, uri: &Url, field: &str) -> Option<String> {
    let schema = load_schema_for_document(&doc.tree, uri).ok()?;
    let field_schema = schema.find_field(field)?;
    
    let mut content = String::new();
    
    // Field name and type
    writeln!(content, "**{}** `{}`", field, schema_type_display(&field_schema.schema));
    writeln!(content);
    
    // Doc comment if available
    if let Some(doc) = &field_schema.doc {
        writeln!(content, "{}", doc);
        writeln!(content);
    }
    
    // Constraints
    if let Some(constraints) = format_constraints(&field_schema.schema) {
        writeln!(content, "**Constraints:**");
        writeln!(content, "{}", constraints);
        writeln!(content);
    }
    
    // Location link
    if let Some(loc) = &field_schema.location {
        writeln!(content, "Defined in [{}:{}:{}]({})",
            loc.file.file_name().unwrap_or_default().to_string_lossy(),
            loc.line,
            loc.column,
            Url::from_file_path(&loc.file).ok()?
        );
    }
    
    Some(content)
}
```

### 3. Format schema type for display

```rust
fn schema_type_display(schema: &Schema) -> String {
    match schema {
        Schema::String(None) => "@string".into(),
        Schema::String(Some(c)) => {
            let mut parts = vec!["@string"];
            if c.min_len.is_some() || c.max_len.is_some() {
                // Add constraint info
            }
            parts.join("")
        }
        Schema::Int(None) => "@int".into(),
        Schema::Int(Some(c)) => format!("@int(min: {}, max: {})", 
            c.min.map(|n| n.to_string()).unwrap_or("∞".into()),
            c.max.map(|n| n.to_string()).unwrap_or("∞".into())
        ),
        Schema::Bool => "@bool".into(),
        Schema::Optional(inner) => format!("@optional({})", schema_type_display(&inner.0.0)),
        Schema::Object(_) => "@object".into(),
        Schema::Seq(s) => format!("@seq({})", schema_type_display(&s.0.0)),
        Schema::Type { name: Some(n) } => format!("@{}", n),
        Schema::Type { name: None } => "@".into(),
        _ => "unknown".into(),
    }
}
```

### 4. Format constraints

```rust
fn format_constraints(schema: &Schema) -> Option<String> {
    match schema {
        Schema::Int(Some(c)) => {
            let mut lines = Vec::new();
            if let Some(min) = c.min {
                lines.push(format!("- minimum: {}", min));
            }
            if let Some(max) = c.max {
                lines.push(format!("- maximum: {}", max));
            }
            if lines.is_empty() { None } else { Some(lines.join("\n")) }
        }
        Schema::String(Some(c)) => {
            let mut lines = Vec::new();
            if let Some(min) = c.min_len {
                lines.push(format!("- min length: {}", min));
            }
            if let Some(max) = c.max_len {
                lines.push(format!("- max length: {}", max));
            }
            if let Some(pattern) = &c.pattern {
                lines.push(format!("- pattern: `{}`", pattern));
            }
            if lines.is_empty() { None } else { Some(lines.join("\n")) }
        }
        _ => None,
    }
}
```

### 5. Advertise capability

```rust
ServerCapabilities {
    hover_provider: Some(HoverProviderCapability::Simple(true)),
    ...
}
```

## Schema Doc Comments

Need to support doc comments in schemas:

```styx
schema {
    @ @object{
        /// The server's display name
        name @string
        
        /// Port number to listen on
        /// Must be between 1 and 65535
        port @int(min: 1, max: 65535)
    }
}
```

This requires:
1. CST support for doc comments (may already exist)
2. Preserving doc comments during schema parsing
3. Associating comments with fields

## Files to Modify

1. `crates/styx-lsp/src/server.rs` - Add hover handler
2. `crates/styx-lsp/src/hover.rs` - New file for hover logic
3. `crates/styx-schema/src/types.rs` - Add doc field to schema types
4. `crates/styx-schema/src/parse.rs` - Extract doc comments

## Testing

1. Hover over field name → shows type and docs
2. Hover over schema declaration → shows schema info
3. Hover over constrained type → shows constraints
4. Test markdown rendering in Zed

## Future Enhancements

- Hover for enum variants showing all options
- Hover for union types showing alternatives
- Quick fixes from hover (e.g., "Convert to @optional")
- Show example values
