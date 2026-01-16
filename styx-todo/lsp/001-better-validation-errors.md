# Better Validation Error Messages

## Goal

Make validation errors more actionable by including schema context.

## Current State

```
unknown field 'unknown_field'
```

## Desired State

```
unknown field 'unknown_field'
  valid fields: name, port, enabled, host
  defined in: ServerConfig (server.schema.styx:12:3)
```

With typo detection:
```
unknown field 'enbled'
  did you mean 'enabled'?
  valid fields: name, port, enabled, host
```

## Implementation

### 1. Pass schema context to errors

Update `ValidationError` to optionally include:

```rust
pub struct ValidationError {
    pub path: String,
    pub span: Option<Span>,
    pub kind: ValidationErrorKind,
    pub message: String,
    // NEW:
    pub schema_context: Option<SchemaContext>,
}

pub struct SchemaContext {
    /// Valid field names for UnknownField errors
    pub valid_fields: Option<Vec<String>>,
    /// Schema type name if available
    pub type_name: Option<String>,
    /// Location in schema file
    pub schema_location: Option<SchemaLocation>,
}

pub struct SchemaLocation {
    pub file: PathBuf,
    pub line: u32,
    pub column: u32,
}
```

### 2. Update validate_object to provide context

In `validate.rs`, when creating `UnknownField` error:

```rust
result.error(ValidationError::new(
    &field_path,
    ValidationErrorKind::UnknownField { field: key_display.into() },
    format!("unknown field '{key_display}'"),
)
.with_span(entry.key.span)
.with_schema_context(SchemaContext {
    valid_fields: Some(schema.0.keys().filter_map(|k| k.clone()).collect()),
    type_name: None, // TODO: track type names
    schema_location: None, // TODO: track schema locations
}));
```

### 3. Add typo detection

Use Levenshtein distance or similar:

```rust
fn suggest_similar(unknown: &str, valid: &[String]) -> Option<String> {
    valid.iter()
        .filter_map(|v| {
            let dist = levenshtein(unknown, v);
            if dist <= 2 && dist < unknown.len() / 2 {
                Some((v.clone(), dist))
            } else {
                None
            }
        })
        .min_by_key(|(_, d)| *d)
        .map(|(v, _)| v)
}
```

Consider using the `strsim` crate for string similarity.

### 4. Update ariadne rendering

In `error.rs`, update `build_report` for `UnknownField`:

```rust
ValidationErrorKind::UnknownField { field } => {
    let mut builder = Report::build(ReportKind::Error, filename, range.start)
        .with_message(format!("unknown field '{}'", field))
        .with_label(
            Label::new((filename, range.clone()))
                .with_message("not defined in schema")
                .with_color(Color::Red),
        );
    
    if let Some(ctx) = &self.schema_context {
        if let Some(suggestion) = ctx.suggestion.as_ref() {
            builder = builder.with_help(format!("did you mean '{}'?", suggestion));
        }
        if let Some(valid) = &ctx.valid_fields {
            builder = builder.with_note(format!("valid fields: {}", valid.join(", ")));
        }
        if let Some(loc) = &ctx.schema_location {
            builder = builder.with_note(format!(
                "schema defined at {}:{}:{}",
                loc.file.display(), loc.line, loc.column
            ));
        }
    }
    
    builder
}
```

### 5. LSP: Add related information

In the LSP diagnostic, add a link to the schema:

```rust
Diagnostic {
    message: error.message.clone(),
    related_information: error.schema_context.as_ref().and_then(|ctx| {
        ctx.schema_location.as_ref().map(|loc| vec![
            DiagnosticRelatedInformation {
                location: Location {
                    uri: Url::from_file_path(&loc.file).ok()?,
                    range: Range {
                        start: Position::new(loc.line - 1, loc.column - 1),
                        end: Position::new(loc.line - 1, loc.column + 10),
                    },
                },
                message: format!("schema defines valid fields here"),
            }
        ])
    }).flatten(),
    ...
}
```

## Files to Modify

1. `crates/styx-schema/src/error.rs` - Add SchemaContext, update rendering
2. `crates/styx-schema/src/validate.rs` - Pass context when creating errors
3. `crates/styx-lsp/src/server.rs` - Add related_information to diagnostics
4. `Cargo.toml` - Add `strsim` for typo detection (optional)

## Testing

1. Create test file with typo: `enbled` instead of `enabled`
2. Create test file with unknown field
3. Verify CLI shows helpful context
4. Verify LSP shows clickable link to schema

## Future Enhancements

- Track schema locations during parsing (requires schema parser changes)
- Show doc comments from schema in error messages
- Support `@deprecated` with migration hints
