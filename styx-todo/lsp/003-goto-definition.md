# Go to Definition

## Goal

Allow jumping from document fields to their schema definitions.

## Use Cases

1. **Field name → Schema field definition**
   - Click on `port` in document → jump to `port @int` in schema

2. **Schema reference → Schema file**
   - Click on `@ server.schema.styx` → open that file

3. **Type reference → Type definition**
   - Click on `@ServerConfig` in schema → jump to its definition

## LSP Protocol

Implement `textDocument/definition`:

```typescript
interface DefinitionParams {
    textDocument: TextDocumentIdentifier;
    position: Position;
}

// Response: Location | Location[] | null
```

## Implementation

### 1. Track schema locations during parsing

Need to store source locations in the parsed schema. Options:

**Option A: Add spans to Schema types**
```rust
pub struct ObjectSchema {
    pub fields: IndexMap<Option<String>, SchemaField>,
}

pub struct SchemaField {
    pub schema: Schema,
    pub span: Option<Span>,  // Location in schema file
    pub doc: Option<String>, // Doc comment
}
```

**Option B: Build a separate location index**
```rust
pub struct SchemaIndex {
    /// Map from field path to location
    field_locations: HashMap<String, SchemaLocation>,
    /// Map from type name to location  
    type_locations: HashMap<String, SchemaLocation>,
}
```

Option B is less invasive but requires a second pass.

### 2. LSP handler

```rust
async fn goto_definition(
    &self,
    params: GotoDefinitionParams,
) -> Result<Option<GotoDefinitionResponse>> {
    let uri = params.text_document_position_params.text_document.uri;
    let position = params.text_document_position_params.position;
    
    let docs = self.documents.lock().await;
    let doc = docs.get(&uri)?;
    
    // Find what's at the cursor position
    let offset = position_to_offset(&doc.content, position);
    
    // Case 1: On the schema declaration line (@ path)
    if let Some(schema_path) = get_schema_declaration_at(&doc.tree, offset) {
        let resolved = resolve_schema_path(&schema_path, &uri)?;
        return Ok(Some(GotoDefinitionResponse::Scalar(Location {
            uri: Url::from_file_path(resolved).ok()?,
            range: Range::default(), // Start of file
        })));
    }
    
    // Case 2: On a field name
    if let Some(field_name) = get_field_name_at(&doc.tree, offset) {
        // Load schema and find field definition
        let schema_file = load_schema_for_document(&doc.tree, &uri)?;
        if let Some(location) = schema_file.find_field_location(&field_name) {
            return Ok(Some(GotoDefinitionResponse::Scalar(location)));
        }
    }
    
    Ok(None)
}
```

### 3. Helper: Find element at position

```rust
/// Find the CST node at a given offset
fn find_node_at_offset(tree: &Value, offset: usize) -> Option<NodeAtOffset> {
    // Walk the tree to find which node contains the offset
    // Return info about what kind of node it is
}

enum NodeAtOffset {
    SchemaDeclaration(String),  // The @ path value
    FieldKey(String),           // A field name like "port"
    FieldValue,                 // The value part
    TagName(String),            // A tag like @true
}
```

### 4. Advertise capability

```rust
ServerCapabilities {
    definition_provider: Some(OneOf::Left(true)),
    ...
}
```

## Files to Modify

1. `crates/styx-schema/src/types.rs` - Add span tracking to schema types
2. `crates/styx-schema/src/parse.rs` - Capture spans during schema parsing
3. `crates/styx-lsp/src/server.rs` - Implement `goto_definition` handler
4. `crates/styx-lsp/src/navigation.rs` - New file for navigation helpers

## Challenges

### Schema Location Tracking

The schema is currently parsed via `facet_styx::from_str()` which loses location info. Options:

1. **Parse schema specially for LSP** - Use raw styx_tree parse + manual extraction
2. **Add span tracking to facet-styx** - More work but cleaner
3. **Re-parse schema file to find definitions** - Search for field names in schema source

Option 3 is a pragmatic middle ground:
```rust
fn find_field_in_schema_source(schema_source: &str, field_name: &str) -> Option<Location> {
    // Parse the schema file's CST
    let tree = styx_tree::parse(schema_source).ok()?;
    // Navigate to schema.@ and find the field
    // Return its span
}
```

## Testing

1. Open document with `@ schema.styx`
2. Ctrl+Click on field name → should jump to schema
3. Ctrl+Click on `@ schema.styx` → should open schema file
4. Test with nested fields: `server.tls.cert`

## Future Enhancements

- Go to definition for type references (`@ServerConfig`)
- Find all references (reverse lookup)
- Peek definition (inline preview)
