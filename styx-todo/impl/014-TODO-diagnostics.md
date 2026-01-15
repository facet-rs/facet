# 014 - Diagnostics

**Status**: TODO  
**Spec**: `docs/content/spec/diagnostics.md`  
**Priority**: High - current error messages are unusable

## Problem

Current error messages are horrendous default facet-format output with no spans:

```
error: type mismatch: expected struct start or sequence start for map, got Scalar(Unit)
```

Should be (per spec):

```
error: expected object, found unit
  --> config.styx:2:10
   |
 2 |   server @
   |          ^ expected object
   |
   = help: use braces for object: server {host localhost}
```

## Implementation

### Crate: `styx-diagnostic`

```
crates/styx-diagnostic/
├── Cargo.toml
├── src/
│   ├── lib.rs           # Diagnostic type, DiagnosticBag
│   ├── render.rs        # ariadne rendering
│   └── codes.rs         # Error codes (optional)
```

### Dependencies

```toml
[dependencies]
ariadne = "0.4"
```

### Core Types

```rust
use styx_parse::Span;

pub enum Level {
    Error,
    Warning,
    Note,
}

pub struct Label {
    pub span: Span,
    pub message: String,
    pub primary: bool,  // ^^^^ vs ----
}

pub struct Diagnostic {
    pub level: Level,
    pub message: String,
    pub labels: Vec<Label>,
    pub notes: Vec<String>,
    pub help: Option<String>,
}

impl Diagnostic {
    pub fn error(message: impl Into<String>) -> Self { ... }
    pub fn warning(message: impl Into<String>) -> Self { ... }
    
    pub fn with_label(self, span: Span, message: impl Into<String>) -> Self { ... }
    pub fn with_secondary(self, span: Span, message: impl Into<String>) -> Self { ... }
    pub fn with_note(self, note: impl Into<String>) -> Self { ... }
    pub fn with_help(self, help: impl Into<String>) -> Self { ... }
}
```

### Rendering with ariadne

```rust
use ariadne::{Report, ReportKind, Label, Source, ColorGenerator};

impl Diagnostic {
    pub fn render(&self, filename: &str, source: &str) -> String {
        let mut colors = ColorGenerator::new();
        
        let kind = match self.level {
            Level::Error => ReportKind::Error,
            Level::Warning => ReportKind::Warning,
            Level::Note => ReportKind::Advice,
        };
        
        let mut report = Report::build(kind, filename, self.labels[0].span.start)
            .with_message(&self.message);
        
        for label in &self.labels {
            let color = colors.next();
            let ariadne_label = if label.primary {
                Label::new((filename, label.span.start..label.span.end))
                    .with_message(&label.message)
                    .with_color(color)
            } else {
                Label::new((filename, label.span.start..label.span.end))
                    .with_message(&label.message)
                    .with_color(color)
                    .with_order(-1)  // secondary labels render first
            };
            report = report.with_label(ariadne_label);
        }
        
        for note in &self.notes {
            report = report.with_note(note);
        }
        
        if let Some(help) = &self.help {
            report = report.with_help(help);
        }
        
        let mut output = Vec::new();
        report.finish()
            .write((filename, Source::from(source)), &mut output)
            .unwrap();
        String::from_utf8(output).unwrap()
    }
}
```

## Integration Points

### 1. Parser errors (styx-parse)

`BuildError` already has spans via `ParseErrorKind`. Need to convert to `Diagnostic`:

```rust
impl From<&BuildError> for Diagnostic {
    fn from(err: &BuildError) -> Self {
        // diagnostic[impl parser.*]
        match &err.kind {
            ParseErrorKind::DuplicateKey => {
                Diagnostic::error("duplicate key")
                    .with_label(err.span, "duplicate key")
                    // TODO: need first definition span
            }
            ParseErrorKind::MixedSeparators => {
                Diagnostic::error("mixed separators in object")
                    .with_label(err.span, "comma here")
                    .with_help("use either commas or newlines, not both")
            }
            // ... etc
        }
    }
}
```

### 2. Deserializer errors (facet-styx)

Good news: `StyxError` already has `span: Option<Span>` and `StyxParser` tracks
`current_span`. The infrastructure is there, we just need to:

1. Add a method to `StyxError` that renders with ariadne given source text
2. Wire that up in the CLI

### 3. Schema validation errors (styx-schema)

`ValidationResult` has `errors: Vec<ValidationError>` but `ValidationError` is just a string.
Need to add spans:

```rust
pub struct ValidationError {
    pub message: String,
    pub span: Option<Span>,
    pub schema_span: Option<Span>,  // where the constraint came from
}
```

### 4. CLI integration

```rust
fn run_validation(...) -> Result<(), CliError> {
    // ...
    if !result.is_valid() {
        for error in &result.errors {
            let diag = Diagnostic::from(error);
            eprintln!("{}", diag.render(&filename, &source));
        }
        return Err(CliError::Validation(...));
    }
}
```

## Spec Coverage

Need to implement diagnostics for all `r[diagnostic.*]` rules:

### Parser (styx-parse)
- [ ] `r[diagnostic.parser.unexpected]` - Unexpected token
- [ ] `r[diagnostic.parser.unclosed]` - Unclosed delimiter
- [ ] `r[diagnostic.parser.escape]` - Invalid escape sequence
- [ ] `r[diagnostic.parser.unterminated-string]` - Unterminated string
- [ ] `r[diagnostic.parser.unterminated-heredoc]` - Unterminated heredoc
- [ ] `r[diagnostic.parser.heredoc-delimiter-length]` - Heredoc delimiter too long
- [ ] `r[diagnostic.parser.heredoc-indent]` - Heredoc indentation error
- [ ] `r[diagnostic.parser.comment-whitespace]` - Comment without whitespace
- [ ] `r[diagnostic.parser.duplicate-key]` - Duplicate key
- [ ] `r[diagnostic.parser.mixed-separators]` - Mixed separators
- [ ] `r[diagnostic.parser.sequence-comma]` - Comma in sequence
- [ ] `r[diagnostic.parser.attr-in-sequence]` - Attribute in sequence
- [ ] `r[diagnostic.parser.trailing-content]` - Trailing content after root

### Deserializer (facet-styx)
- [ ] `r[diagnostic.deser.invalid-value]` - Invalid value for type
- [ ] `r[diagnostic.deser.enum-invalid]` - Enum not a tagged value
- [ ] `r[diagnostic.deser.unknown-variant]` - Unknown enum variant
- [ ] `r[diagnostic.deser.missing-field]` - Missing required field
- [ ] `r[diagnostic.deser.unknown-field]` - Unknown field
- [ ] `r[diagnostic.deser.expected-object]` - Expected object
- [ ] `r[diagnostic.deser.expected-sequence]` - Expected sequence

## Testing

Each diagnostic should have a test that verifies:
1. The error is detected
2. The span points to the right location
3. The message matches the spec format

```rust
#[test]
fn test_duplicate_key_diagnostic() {
    let source = "a 1\na 2";
    let result = parse(source);
    let diag = result.unwrap_err().to_diagnostic();
    
    assert_eq!(diag.level, Level::Error);
    assert!(diag.message.contains("duplicate key"));
    assert_eq!(diag.labels[0].span, Span { start: 4, end: 5 }); // second 'a'
    // TODO: secondary label for first definition
}
```

## Priority Order

1. **Parser errors** - Already have spans, just need rendering
2. **CLI integration** - Wire up ariadne rendering
3. **Schema validation** - Add spans to ValidationError
4. **Deserializer errors** - Hardest, needs span tracking in facet-styx
