# Phase 004: styx-tree (Document Tree)

High-level document tree built from parser events. Provides a convenient API for reading and manipulating Styx documents.

## Deliverables

- `crates/styx-tree/src/lib.rs` - Crate root, main types
- `crates/styx-tree/src/value.rs` - Value enum
- `crates/styx-tree/src/object.rs` - Object type
- `crates/styx-tree/src/sequence.rs` - Sequence type
- `crates/styx-tree/src/builder.rs` - Tree builder from events
- `crates/styx-tree/src/access.rs` - Path-based access API

## Core Types

```rust
/// A Styx value
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// Scalar text (from any scalar syntax)
    Scalar(Scalar),
    
    /// Unit value (@)
    Unit,
    
    /// Tagged value (@name payload)
    Tagged(Tagged),
    
    /// Sequence (a b c)
    Sequence(Sequence),
    
    /// Object {key value, ...}
    Object(Object),
}

#[derive(Debug, Clone, PartialEq)]
pub struct Scalar {
    pub text: String,
    pub kind: ScalarKind,
    pub span: Option<Span>,  // None if programmatically constructed
}

#[derive(Debug, Clone, PartialEq)]
pub struct Tagged {
    pub tag: String,
    pub payload: Option<Box<Value>>,
    pub span: Option<Span>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Sequence {
    pub items: Vec<Value>,
    pub span: Option<Span>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Object {
    pub entries: Vec<Entry>,
    pub separator: Separator,
    pub span: Option<Span>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub key: Value,  // usually Scalar, but can be Unit or Tagged
    pub value: Value,
    pub doc_comment: Option<String>,
}
```

## Builder from Events

```rust
pub struct TreeBuilder {
    stack: Vec<BuilderFrame>,
    root: Option<Value>,
}

enum BuilderFrame {
    Object {
        entries: Vec<Entry>,
        separator: Separator,
        span: Span,
        pending_key: Option<Value>,
        pending_doc_comment: Option<String>,
    },
    Sequence {
        items: Vec<Value>,
        span: Span,
    },
    Tag {
        name: String,
        span: Span,
    },
    Entry,
}

impl<'src> ParseCallback<'src> for TreeBuilder {
    fn event(&mut self, event: Event<'src>) -> bool {
        match event {
            Event::ObjectStart { span, separator } => {
                self.stack.push(BuilderFrame::Object { ... });
            }
            Event::ObjectEnd { span } => {
                let frame = self.stack.pop();
                let obj = Object::from(frame);
                self.push_value(Value::Object(obj));
            }
            // ... etc
        }
        true
    }
}

impl TreeBuilder {
    pub fn new() -> Self;
    pub fn finish(self) -> Result<Value, BuildError>;
}
```

## Convenience Parsing

```rust
/// Parse source into a document tree
pub fn parse(source: &str) -> Result<Value, ParseError> {
    let parser = Parser::new(source);
    let mut builder = TreeBuilder::new();
    parser.parse(&mut builder);
    builder.finish()
}
```

## Access API

Path-based access for easy querying:

```rust
impl Value {
    /// Get a value by path. Path segments are separated by `.`
    /// Use `[n]` for sequence indexing.
    /// Returns None if path doesn't exist or type mismatches.
    pub fn get(&self, path: &str) -> Option<&Value>;
    
    /// Mutable access
    pub fn get_mut(&mut self, path: &str) -> Option<&mut Value>;
    
    /// Get as string (for scalars)
    pub fn as_str(&self) -> Option<&str>;
    
    /// Get as object
    pub fn as_object(&self) -> Option<&Object>;
    
    /// Get as sequence
    pub fn as_sequence(&self) -> Option<&Sequence>;
    
    /// Check if unit
    pub fn is_unit(&self) -> bool;
    
    /// Get tag name if tagged
    pub fn tag(&self) -> Option<&str>;
}

impl Object {
    /// Get entry by key
    pub fn get(&self, key: &str) -> Option<&Value>;
    
    /// Iterate entries
    pub fn iter(&self) -> impl Iterator<Item = (&Value, &Value)>;
    
    /// Check for key
    pub fn contains_key(&self, key: &str) -> bool;
}

impl Sequence {
    /// Get by index
    pub fn get(&self, index: usize) -> Option<&Value>;
    
    /// Length
    pub fn len(&self) -> usize;
    
    /// Iterate
    pub fn iter(&self) -> impl Iterator<Item = &Value>;
}
```

## Serialization (to Styx text)

```rust
impl Value {
    /// Serialize to Styx text
    pub fn to_styx(&self) -> String;
    
    /// Serialize with formatting options
    pub fn to_styx_formatted(&self, options: &FormatOptions) -> String;
}

pub struct FormatOptions {
    pub indent: &'static str,  // "  " or "\t"
    pub prefer_comma_separator: bool,
    pub prefer_bare_scalars: bool,
}
```

## Document Type

For documents that are implicitly an object at root:

```rust
pub struct Document {
    pub root: Object,
    pub leading_comments: Vec<String>,
}

impl Document {
    pub fn parse(source: &str) -> Result<Self, ParseError>;
    pub fn get(&self, path: &str) -> Option<&Value>;
}
```

## Testing

- Round-trip tests: parse → tree → serialize → parse → compare
- Access API tests with various paths
- Error handling for malformed documents
- Span preservation tests
