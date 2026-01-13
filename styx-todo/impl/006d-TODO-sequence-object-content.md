# Phase 006d: Sequence and Object Content Parsing

Fix sequence and object content to actually parse their contents instead of emitting empty containers.

## Spec References

> r[sequence.syntax]
> Sequences use `(` `)` delimiters. Empty sequences `()` are valid.
> Elements are separated by whitespace (spaces, tabs, or newlines).
> Commas are NOT allowed.

> r[sequence.elements]
> Elements may be any atom type.

> r[object.syntax]
> Objects use `{` `}` delimiters. Empty objects `{}` are valid.

## Current State

`parse_object_atom()` and `parse_sequence_atom()` just count braces/parens and return empty containers:

```rust
fn parse_object_atom(&mut self) -> Atom<'src> {
    let open = self.advance().unwrap(); // consume '{'
    // ... just counts braces ...
    Atom {
        content: AtomContent::Object,  // No content!
    }
}
```

And `emit_atom_as_value()`:
```rust
AtomContent::Object => {
    // For now, emit as empty object  // BUG!
    callback.event(Event::ObjectStart { ... })
    callback.event(Event::ObjectEnd { ... })
}
```

## Required Changes

### Option A: Store source slice, re-parse later

Store the source range and re-parse when emitting:

```rust
enum AtomContent<'src> {
    // ...
    Object(&'src str),    // Source slice of object content
    Sequence(&'src str),  // Source slice of sequence content
}

fn parse_object_atom(&mut self) -> Atom<'src> {
    let open = self.advance().unwrap();
    let content_start = self.pos;
    
    // Find matching brace
    let mut depth = 1;
    while depth > 0 {
        // ... existing logic ...
    }
    
    let content = &self.source[content_start..close_pos];
    
    Atom {
        span: ...,
        content: AtomContent::Object(content),
    }
}
```

Then in `emit_atom_as_value()`, create a sub-parser for the content.

### Option B: Parse inline (preferred)

Parse the content immediately and store the parsed entries:

```rust
enum AtomContent<'src> {
    // ...
    Object(Vec<(Atom<'src>, Atom<'src>)>),  // entries
    Sequence(Vec<Atom<'src>>),               // elements
}

// [impl r[object.syntax]]
fn parse_object_atom(&mut self) -> Atom<'src> {
    let open = self.advance().unwrap(); // consume '{'
    let start_span = open.span;
    
    let mut entries = Vec::new();
    let mut separator = None;
    
    loop {
        self.skip_whitespace();
        
        let Some(token) = self.peek() else { break };
        
        match token.kind {
            TokenKind::RBrace => {
                let close = self.advance().unwrap();
                return Atom {
                    span: Span { start: start_span.start, end: close.span.end },
                    kind: ScalarKind::Bare,
                    content: AtomContent::Object { entries, separator: separator.unwrap_or(Separator::Newline) },
                };
            }
            TokenKind::Newline => {
                if separator == Some(Separator::Comma) {
                    // Error: mixed separators
                }
                separator = Some(Separator::Newline);
                self.advance();
            }
            TokenKind::Comma => {
                if separator == Some(Separator::Newline) {
                    // Error: mixed separators
                }
                separator = Some(Separator::Comma);
                self.advance();
            }
            TokenKind::LineComment | TokenKind::DocComment => {
                self.advance(); // skip comments inside objects
            }
            _ => {
                // Parse entry atoms
                let entry_atoms = self.collect_entry_atoms_in_object();
                if !entry_atoms.is_empty() {
                    let key = entry_atoms[0].clone();
                    let value = if entry_atoms.len() > 1 {
                        entry_atoms[1].clone()
                    } else {
                        Atom { span: key.span, kind: ScalarKind::Bare, content: AtomContent::Unit }
                    };
                    entries.push((key, value));
                }
            }
        }
    }
    
    // Unclosed object
    Atom {
        span: Span { start: start_span.start, end: self.pos },
        kind: ScalarKind::Bare,
        content: AtomContent::Object { entries, separator: separator.unwrap_or(Separator::Newline) },
    }
}

// [impl r[sequence.syntax]] [impl r[sequence.elements]]
fn parse_sequence_atom(&mut self) -> Atom<'src> {
    let open = self.advance().unwrap(); // consume '('
    let start_span = open.span;
    
    let mut elements = Vec::new();
    
    loop {
        self.skip_whitespace_and_newlines(); // sequences allow both
        
        let Some(token) = self.peek() else { break };
        
        match token.kind {
            TokenKind::RParen => {
                let close = self.advance().unwrap();
                return Atom {
                    span: Span { start: start_span.start, end: close.span.end },
                    kind: ScalarKind::Bare,
                    content: AtomContent::Sequence(elements),
                };
            }
            TokenKind::Comma => {
                // Error: commas not allowed in sequences
                self.advance();
            }
            TokenKind::LineComment | TokenKind::DocComment => {
                self.advance();
            }
            _ => {
                // Parse single element (not entry, just one atom)
                let elem = self.parse_single_atom();
                if let Some(e) = elem {
                    elements.push(e);
                }
            }
        }
    }
    
    // Unclosed sequence
    Atom {
        span: Span { start: start_span.start, end: self.pos },
        kind: ScalarKind::Bare,
        content: AtomContent::Sequence(elements),
    }
}

fn parse_single_atom(&mut self) -> Option<Atom<'src>> {
    let Some(token) = self.peek() else { return None };
    
    match token.kind {
        TokenKind::BareScalar | TokenKind::QuotedScalar | TokenKind::RawScalar | TokenKind::HeredocStart => {
            Some(self.parse_scalar_atom())
        }
        TokenKind::LBrace => Some(self.parse_object_atom()),
        TokenKind::LParen => Some(self.parse_sequence_atom()),
        TokenKind::At => Some(self.parse_tag_or_unit()),
        _ => None,
    }
}
```

### Update `emit_atom_as_value()`

```rust
AtomContent::Object { entries, separator } => {
    if !callback.event(Event::ObjectStart { span: atom.span, separator }) {
        return false;
    }
    
    for (key, value) in entries {
        if !callback.event(Event::EntryStart) { return false }
        if !callback.event(Event::Key { 
            span: key.span, 
            value: self.process_scalar_value(&key), 
            kind: key.kind 
        }) { return false }
        if !self.emit_atom_as_value(&value, callback) { return false }
        if !callback.event(Event::EntryEnd) { return false }
    }
    
    callback.event(Event::ObjectEnd { span: atom.span })
}

AtomContent::Sequence(elements) => {
    if !callback.event(Event::SequenceStart { span: atom.span }) {
        return false;
    }
    
    for elem in elements {
        if !self.emit_atom_as_value(&elem, callback) { return false }
    }
    
    callback.event(Event::SequenceEnd { span: atom.span })
}
```

## Test Cases

```rust
// [verify r[object.syntax]]
#[test]
fn test_nested_object() {
    let events = parse("outer {inner {x 1}}");
    // Should have nested ObjectStart/ObjectEnd events
    let obj_starts = events.iter().filter(|e| matches!(e, Event::ObjectStart { .. })).count();
    assert_eq!(obj_starts, 2);
}

// [verify r[sequence.syntax]] [verify r[sequence.elements]]
#[test]
fn test_sequence_elements() {
    let events = parse("items (a b c)");
    let scalars: Vec<_> = events.iter().filter_map(|e| match e {
        Event::Scalar { value, .. } => Some(value.as_ref()),
        _ => None,
    }).collect();
    assert!(scalars.contains(&"a"));
    assert!(scalars.contains(&"b"));
    assert!(scalars.contains(&"c"));
}

#[test]
fn test_nested_sequences() {
    let events = parse("matrix ((1 2) (3 4))");
    let seq_starts = events.iter().filter(|e| matches!(e, Event::SequenceStart { .. })).count();
    assert_eq!(seq_starts, 3); // outer + 2 inner
}

#[test]
fn test_sequence_no_commas() {
    // Commas in sequences should error (or be treated as bare scalars?)
    let events = parse("items (a, b, c)");
    // Check for error or specific behavior
}

#[test]
fn test_object_in_sequence() {
    let events = parse("servers ({host a} {host b})");
    // Sequence containing objects
}
```

## Tracey Annotations

- `// [impl r[sequence.syntax]]` on sequence parsing
- `// [impl r[sequence.elements]]` on element parsing
- `// [impl r[object.syntax]]` on object parsing
