# Phase 006b: Tag Payloads

Implement tag payload parsing per `r[tag.payload]`.

## Spec Reference

> r[tag.payload]
> A tag MAY be immediately followed (no whitespace) by a payload:
>
> | Follows `@tag` | Result |
> |----------------|--------|
> | `{...}` | tagged object |
> | `(...)` | tagged sequence |
> | `"..."`, `r#"..."#`, `<<HEREDOC` | tagged scalar |
> | `@` | tagged unit (explicit) |
> | *(nothing)* | tagged unit (implicit) |

## Current State

`parse_tag_or_unit_atom()` in `parser.rs` only parses the tag name, ignoring any payload.

## Required Changes

### 1. Update `parse_tag_or_unit_atom()` → `parse_tag_or_unit()`

```rust
// [impl r[tag.payload]]
fn parse_tag_or_unit(&mut self) -> Atom<'src> {
    let at = self.advance().unwrap(); // consume '@'
    let start_span = at.span;

    // Check for tag name (must immediately follow @, no whitespace)
    let Some(token) = self.peek_raw() else {
        // Just @ (unit)
        return Atom { span: start_span, kind: ScalarKind::Bare, content: AtomContent::Unit };
    };

    if token.kind != TokenKind::BareScalar || token.span.start != start_span.end {
        // Just @ (unit) - either not a scalar or has whitespace gap
        return Atom { span: start_span, kind: ScalarKind::Bare, content: AtomContent::Unit };
    }

    let name_token = self.advance().unwrap();
    let tag_name = name_token.text;
    let name_end = name_token.span.end;

    // Check for payload (must immediately follow tag name, no whitespace)
    let payload = self.parse_tag_payload(name_end);

    let end_span = payload.as_ref().map(|p| p.span.end).unwrap_or(name_end);

    Atom {
        span: Span { start: start_span.start, end: end_span },
        kind: ScalarKind::Bare,
        content: AtomContent::Tag { name: tag_name, payload: payload.map(Box::new) },
    }
}

fn parse_tag_payload(&mut self, after_name: u32) -> Option<Atom<'src>> {
    let Some(token) = self.peek_raw() else {
        return None; // implicit unit
    };

    // Payload must immediately follow tag name (no whitespace)
    if token.span.start != after_name {
        return None; // implicit unit
    }

    match token.kind {
        TokenKind::LBrace => Some(self.parse_object_atom()),
        TokenKind::LParen => Some(self.parse_sequence_atom()),
        TokenKind::QuotedScalar | TokenKind::RawScalar | TokenKind::HeredocStart => {
            Some(self.parse_scalar_atom())
        }
        TokenKind::At => {
            // Explicit tagged unit: @tag@
            let at = self.advance().unwrap();
            Some(Atom {
                span: at.span,
                kind: ScalarKind::Bare,
                content: AtomContent::Unit,
            })
        }
        _ => None, // implicit unit
    }
}
```

### 2. Update `AtomContent` enum

```rust
enum AtomContent<'src> {
    Scalar(&'src str),
    Heredoc(String),
    Unit,
    Tag { name: &'src str, payload: Option<Box<Atom<'src>>> },
    Object,
    Sequence,
}
```

### 3. Update `emit_atom_as_value()` for tags with payloads

```rust
AtomContent::Tag { name, payload } => {
    if !callback.event(Event::TagStart { span: atom.span, name }) {
        return false;
    }
    if let Some(payload) = payload {
        if !self.emit_atom_as_value(&payload, callback) {
            return false;
        }
    }
    // If no payload, implicit unit (no event needed, TagEnd implies it)
    callback.event(Event::TagEnd)
}
```

## Test Cases

```rust
// [verify r[tag.payload]]
#[test]
fn test_tagged_object() {
    let events = parse("result @err{message x}");
    assert!(events.iter().any(|e| matches!(e, Event::TagStart { name: "err", .. })));
    assert!(events.iter().any(|e| matches!(e, Event::ObjectStart { .. })));
}

#[test]
fn test_tagged_sequence() {
    let events = parse("color @rgb(255 128 0)");
    assert!(events.iter().any(|e| matches!(e, Event::TagStart { name: "rgb", .. })));
    assert!(events.iter().any(|e| matches!(e, Event::SequenceStart { .. })));
}

#[test]
fn test_tagged_scalar() {
    let events = parse(r#"name @nickname"Bob""#);
    assert!(events.iter().any(|e| matches!(e, Event::TagStart { name: "nickname", .. })));
    assert!(events.iter().any(|e| matches!(e, Event::Scalar { value, .. } if value == "Bob")));
}

#[test]
fn test_tagged_explicit_unit() {
    let events = parse("nothing @empty@");
    assert!(events.iter().any(|e| matches!(e, Event::TagStart { name: "empty", .. })));
    assert!(events.iter().any(|e| matches!(e, Event::Unit { .. })));
}

#[test]
fn test_tagged_implicit_unit() {
    let events = parse("status @ok");
    assert!(events.iter().any(|e| matches!(e, Event::TagStart { name: "ok", .. })));
    // No Unit event - implicit
}

#[test]
fn test_tag_whitespace_gap() {
    // Whitespace between tag and payload = separate atoms
    let events = parse("x @tag {a b}");
    // @tag is unit, {a b} is separate object value
}
```

## Tracey Annotations

Add to implementation:
- `// [impl r[tag.payload]]` on `parse_tag_payload()` and related code

Add to tests:
- `// [verify r[tag.payload]]` on test functions
