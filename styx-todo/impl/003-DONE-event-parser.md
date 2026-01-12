# Phase 003: styx-parse (Event Parser)

Event-based parser that consumes tokens and emits semantic events.

## Deliverables

- `crates/styx-parse/src/parser.rs` - Event parser
- `crates/styx-parse/src/event.rs` - Event types
- `crates/styx-parse/src/callback.rs` - Callback trait for consumers

## Event Types

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Event<'src> {
    // Document boundaries
    DocumentStart,
    DocumentEnd,

    // Objects
    ObjectStart {
        span: Span,
        separator: Separator,
    },
    ObjectEnd {
        span: Span,
    },

    // Sequences
    SequenceStart {
        span: Span,
    },
    SequenceEnd {
        span: Span,
    },

    // Entry structure (within objects)
    EntryStart,
    Key {
        span: Span,
        value: Cow<'src, str>,  // after escape processing
        kind: ScalarKind,
    },
    EntryEnd,

    // Values
    Scalar {
        span: Span,
        value: Cow<'src, str>,  // after escape processing
        kind: ScalarKind,
    },
    Unit {
        span: Span,
    },

    // Tags
    TagStart {
        span: Span,
        name: &'src str,
    },
    TagEnd,

    // Comments (optional, for tools that care)
    Comment {
        span: Span,
        text: &'src str,
    },
    DocComment {
        span: Span,
        text: &'src str,
    },

    // Errors
    Error {
        span: Span,
        kind: ParseErrorKind,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Separator {
    Newline,
    Comma,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarKind {
    Bare,
    Quoted,
    Raw,
    Heredoc,
}
```

## Callback Trait

```rust
pub trait ParseCallback<'src> {
    /// Called for each event. Return `false` to stop parsing early.
    fn event(&mut self, event: Event<'src>) -> bool;
}

// Convenience: collect all events
impl<'src> ParseCallback<'src> for Vec<Event<'src>> {
    fn event(&mut self, event: Event<'src>) -> bool {
        self.push(event);
        true
    }
}
```

## Parser API

```rust
pub struct Parser<'src> {
    lexer: Peekable<Lexer<'src>>,
    // state for entry accumulation, separator detection, etc.
}

impl<'src> Parser<'src> {
    pub fn new(source: &'src str) -> Self;
    
    /// Parse and emit events to callback
    pub fn parse<C: ParseCallback<'src>>(self, callback: &mut C);
    
    /// Convenience: parse and collect all events
    pub fn parse_to_vec(self) -> Vec<Event<'src>>;
}
```

## Parsing Logic

### Document Level

```
parse_document:
    emit DocumentStart
    while not EOF:
        parse_entry (top-level, implicit object)
    emit DocumentEnd
```

### Entry Parsing

An entry is 1+ atoms. The interpretation depends on context and atom count:

```
parse_entry:
    emit EntryStart
    atoms = collect atoms until entry boundary
    
    if in object context:
        if atoms.len() == 1:
            emit Key(atoms[0])
            emit Unit  // implicit value
        else if atoms.len() == 2:
            emit Key(atoms[0])
            emit_value(atoms[1])
        else:  // atoms.len() > 2
            // Nested key path: a b c → key=a, value={b c}
            emit Key(atoms[0])
            // Recursively handle remaining as nested entry
            emit ObjectStart(implicit)
            parse_entry_from_atoms(atoms[1..])
            emit ObjectEnd
    else:  // in sequence context, or top-level value
        for atom in atoms:
            emit_value(atom)
    
    emit EntryEnd
```

### Object Parsing

```
parse_object:
    consume '{'
    emit ObjectStart
    
    // Detect separator mode from first entry boundary
    separator = detect_separator()
    
    while not '}':
        parse_entry
        consume separator (newline or comma)
    
    consume '}'
    emit ObjectEnd
```

### Sequence Parsing

```
parse_sequence:
    consume '('
    emit SequenceStart
    
    while not ')':
        parse_atom  // emit as value
        consume whitespace
    
    consume ')'
    emit SequenceEnd
```

### Tag Parsing

```
parse_tag:
    consume '@'
    if followed by tag name char:
        name = consume tag name
        emit TagStart(name)
        if has payload:
            parse_tag_payload
        emit TagEnd
    else:
        emit Unit  // bare @
```

### Separator Detection

Within an object, the first entry boundary determines the mode:
- If comma follows first entry → Comma mode
- If newline follows first entry → Newline mode
- Empty object → doesn't matter

Mixed separators are an error.

## Escape Processing

The parser (not lexer) handles escape sequence conversion:

```rust
fn unescape(s: &str, kind: ScalarKind) -> Result<Cow<str>, EscapeError> {
    match kind {
        ScalarKind::Bare | ScalarKind::Raw | ScalarKind::Heredoc => {
            Ok(Cow::Borrowed(s))  // no escapes
        }
        ScalarKind::Quoted => {
            unescape_quoted(s)
        }
    }
}
```

## Error Handling

The parser should:
- Emit `Event::Error` for parse errors
- Continue parsing after errors where possible (error recovery)
- Track nested structure to avoid cascading errors

Common errors:
- Unexpected token
- Unclosed object/sequence
- Mixed separators
- Invalid escape sequence
- Duplicate keys (detected at this level? or higher?)

## Testing

- Unit tests for each construct
- Integration tests with example files
- Error recovery tests
- Round-trip property: parse → events → (phase 004) tree → serialize ≈ original
