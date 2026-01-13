# Phase 006e: Duplicate Key Detection

Implement duplicate key detection per `r[entry.key-equality]`.

## Spec Reference

> r[entry.key-equality]
> To detect duplicate keys, the parser MUST compare keys by their parsed value:
>
> - **Scalar keys** compare equal if their contents are exactly equal after parsing
>   (quoted scalars are compared after escape processing).
> - **Unit keys** compare equal to other unit keys.
> - **Tagged keys** compare equal if both tag name and payload are equal.

Also from `r[object.separators]`:
> Duplicate keys are forbidden.

## Current State

No duplicate key detection. The parser accepts:
```styx
{a 1, a 2, a 3}
```
without any error.

## Required Changes

### 1. Add key comparison infrastructure

```rust
// [impl r[entry.key-equality]]

/// A parsed key for equality comparison.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum KeyValue {
    /// Scalar key (after escape processing).
    Scalar(String),
    /// Unit key (@).
    Unit,
    /// Tagged key.
    Tagged { name: String, payload: Option<Box<KeyValue>> },
}

impl KeyValue {
    fn from_atom(atom: &Atom<'_>, source: &str) -> Self {
        match &atom.content {
            AtomContent::Scalar(text) => {
                // Process escapes for quoted strings
                let processed = match atom.kind {
                    ScalarKind::Quoted => unescape_quoted(text),
                    _ => text.to_string(),
                };
                KeyValue::Scalar(processed)
            }
            AtomContent::Heredoc(content) => KeyValue::Scalar(content.clone()),
            AtomContent::Unit => KeyValue::Unit,
            AtomContent::Tag { name, payload } => KeyValue::Tagged {
                name: name.to_string(),
                payload: payload.as_ref().map(|p| Box::new(KeyValue::from_atom(p, source))),
            },
            // Objects/Sequences as keys would be unusual, treat as their text repr
            AtomContent::Object { .. } => KeyValue::Scalar("{}".into()),
            AtomContent::Sequence(_) => KeyValue::Scalar("()".into()),
            AtomContent::Attributes(_) => KeyValue::Scalar("...".into()),
        }
    }
}
```

### 2. Track keys during object parsing

```rust
use std::collections::HashSet;

fn parse_object_atom(&mut self) -> Atom<'src> {
    let open = self.advance().unwrap();
    let start_span = open.span;
    
    let mut entries = Vec::new();
    let mut seen_keys: HashSet<KeyValue> = HashSet::new();
    let mut separator = None;
    
    loop {
        // ... existing loop structure ...
        
        _ => {
            let entry_atoms = self.collect_entry_atoms_in_object();
            if !entry_atoms.is_empty() {
                let key = entry_atoms[0].clone();
                let key_value = KeyValue::from_atom(&key, self.source);
                
                // [impl r[entry.key-equality]]
                if seen_keys.contains(&key_value) {
                    self.errors.push(ParseError {
                        span: key.span,
                        kind: ParseErrorKind::DuplicateKey,
                        message: format!("duplicate key: {:?}", key_value),
                    });
                } else {
                    seen_keys.insert(key_value);
                }
                
                let value = if entry_atoms.len() > 1 {
                    entry_atoms[1].clone()
                } else {
                    Atom { span: key.span, kind: ScalarKind::Bare, content: AtomContent::Unit }
                };
                entries.push((key, value));
            }
        }
    }
    
    // ...
}
```

### 3. Also check at document root level

The document root is an implicit object, so duplicate keys should be detected there too:

```rust
fn parse_entries<C: ParseCallback<'src>>(&mut self, callback: &mut C, closing: Option<TokenKind>) {
    let mut seen_keys: HashSet<KeyValue> = HashSet::new();
    
    // ... in the loop where we parse entries ...
    
    // After parsing key atom, check for duplicates
    let key_value = KeyValue::from_atom(&key_atom, self.source);
    if seen_keys.contains(&key_value) {
        callback.event(Event::Error {
            span: key_atom.span,
            kind: ParseErrorKind::DuplicateKey,
        });
    } else {
        seen_keys.insert(key_value);
    }
}
```

### 4. Add error kind

```rust
pub enum ParseErrorKind {
    // ... existing ...
    /// Duplicate key in object.
    DuplicateKey,
}
```

## Test Cases

```rust
// [verify r[entry.key-equality]]

#[test]
fn test_duplicate_bare_key() {
    let events = parse("{a 1, a 2}");
    assert!(events.iter().any(|e| matches!(e, Event::Error { kind: ParseErrorKind::DuplicateKey, .. })));
}

#[test]
fn test_duplicate_quoted_key() {
    let events = parse(r#"{"key" 1, "key" 2}"#);
    assert!(events.iter().any(|e| matches!(e, Event::Error { kind: ParseErrorKind::DuplicateKey, .. })));
}

#[test]
fn test_duplicate_key_escape_normalized() {
    // "a\x62" and "ab" should be considered duplicates after escape processing
    let events = parse(r#"{"ab" 1, "a\u{62}" 2}"#);
    assert!(events.iter().any(|e| matches!(e, Event::Error { kind: ParseErrorKind::DuplicateKey, .. })));
}

#[test]
fn test_duplicate_unit_key() {
    let events = parse("{@ 1, @ 2}");
    assert!(events.iter().any(|e| matches!(e, Event::Error { kind: ParseErrorKind::DuplicateKey, .. })));
}

#[test]
fn test_duplicate_tagged_key() {
    let events = parse("{@foo 1, @foo 2}");
    assert!(events.iter().any(|e| matches!(e, Event::Error { kind: ParseErrorKind::DuplicateKey, .. })));
}

#[test]
fn test_different_keys_ok() {
    let events = parse("{a 1, b 2, c 3}");
    assert!(!events.iter().any(|e| matches!(e, Event::Error { .. })));
}

#[test]
fn test_document_root_duplicates() {
    let events = parse("name Alice\nname Bob");
    assert!(events.iter().any(|e| matches!(e, Event::Error { kind: ParseErrorKind::DuplicateKey, .. })));
}
```

## Tracey Annotations

- `// [impl r[entry.key-equality]]` on KeyValue struct and comparison logic
- Add verify annotations to all duplicate key tests
