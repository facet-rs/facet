# Phase 006c: Attribute Syntax

Implement attribute syntax per `r[attr.syntax]`, `r[attr.values]`, `r[attr.atom]`.

## Spec References

> r[attr.syntax]
> Attribute syntax `key=value` creates an object entry.
> The `=` has no spaces around it.
> Attribute keys MUST be bare scalars.
>
> ```styx
> server host=localhost port=8080
> ```
> Equivalent to:
> ```styx
> server {host localhost, port 8080}
> ```

> r[attr.values]
> Attribute values may be bare scalars, quoted scalars, sequences, or objects.
>
> ```styx
> config name=app tags=(web prod) opts={verbose true}
> ```

> r[attr.atom]
> Multiple attributes combine into a single object atom.
>
> ```styx
> host=localhost port=8080
> ```
> Equivalent to:
> ```styx
> {host localhost, port 8080}
> ```

> r[entry.keypath.attributes]
> Key paths compose naturally with attribute syntax.
>
> ```styx
> spec selector matchLabels app=web tier=frontend
> ```
> Equivalent to:
> ```styx
> spec {selector {matchLabels {app web, tier frontend}}}
> ```

## Current State

The lexer produces `TokenKind::Eq` for `=`, but the parser ignores it entirely.

## Required Changes

### 1. Add attribute parsing in `collect_entry_atoms()`

When we see `BareScalar` followed immediately by `=`, it starts an attribute sequence:

```rust
fn collect_entry_atoms(&mut self) -> Vec<Atom<'src>> {
    let mut atoms = Vec::new();

    loop {
        self.skip_whitespace();

        let Some(token) = self.peek() else { break };

        match token.kind {
            // Entry boundaries
            TokenKind::Newline | TokenKind::Comma | TokenKind::Eof => break,
            TokenKind::RBrace | TokenKind::RParen => break,
            TokenKind::LineComment | TokenKind::DocComment => break,

            // Check for attribute: bare_scalar immediately followed by =
            TokenKind::BareScalar => {
                let scalar_end = token.span.end;
                let scalar_text = token.text;
                
                // Peek ahead to see if = follows immediately
                // Need to check without consuming
                if self.is_attribute_start() {
                    atoms.push(self.parse_attributes());
                } else {
                    atoms.push(self.parse_scalar_atom());
                }
            }

            // ... rest unchanged
        }
    }

    atoms
}

// [impl r[attr.syntax]]
fn is_attribute_start(&mut self) -> bool {
    // Save position, peek ahead
    // BareScalar immediately followed by Eq (no whitespace)
    let Some(token) = self.peek() else { return false };
    if token.kind != TokenKind::BareScalar { return false }
    
    let scalar_end = token.span.end;
    
    // Look at next token without consuming
    // This requires peeking 2 ahead...
    // May need to restructure
    false // placeholder
}

// [impl r[attr.syntax]] [impl r[attr.values]] [impl r[attr.atom]]
fn parse_attributes(&mut self) -> Atom<'src> {
    let start = self.peek().unwrap().span.start;
    let mut attrs: Vec<(String, Atom<'src>)> = Vec::new();

    loop {
        self.skip_whitespace();
        
        let Some(key_token) = self.peek() else { break };
        if key_token.kind != TokenKind::BareScalar { break }
        
        let key_end = key_token.span.end;
        let key = self.advance().unwrap();
        
        // Check for = immediately after key
        let Some(eq_token) = self.peek_raw() else { 
            // Not an attribute, put key back? Or error?
            // Actually we already consumed it... need different approach
            break;
        };
        
        if eq_token.kind != TokenKind::Eq || eq_token.span.start != key_end {
            // Not an attribute
            break;
        }
        
        self.advance(); // consume =
        let eq_end = eq_token.span.end;
        
        // Value must immediately follow = (no whitespace)
        let Some(val_token) = self.peek_raw() else {
            // Error: missing value after =
            break;
        };
        
        if val_token.span.start != eq_end {
            // Error: whitespace after =
            break;
        }
        
        // Parse value
        let value = match val_token.kind {
            TokenKind::BareScalar | TokenKind::QuotedScalar | TokenKind::RawScalar => {
                self.parse_scalar_atom()
            }
            TokenKind::LParen => self.parse_sequence_atom(),
            TokenKind::LBrace => self.parse_object_atom(),
            _ => break, // Error: invalid attribute value
        };
        
        attrs.push((key.text.to_string(), value));
    }

    let end = attrs.last().map(|(_, a)| a.span.end).unwrap_or(start);

    Atom {
        span: Span { start, end },
        kind: ScalarKind::Bare,
        content: AtomContent::Attributes(attrs),
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
    Attributes(Vec<(String, Atom<'src>)>),  // NEW
}
```

### 3. Update `emit_atom_as_value()` for attributes

```rust
// [impl r[attr.atom]]
AtomContent::Attributes(attrs) => {
    // Emit as comma-separated object
    if !callback.event(Event::ObjectStart { 
        span: atom.span, 
        separator: Separator::Comma 
    }) {
        return false;
    }
    
    for (key, value) in attrs {
        if !callback.event(Event::EntryStart) { return false }
        if !callback.event(Event::Key { 
            span: value.span, // approximate
            value: Cow::Borrowed(key), 
            kind: ScalarKind::Bare 
        }) { return false }
        if !self.emit_atom_as_value(value, callback) { return false }
        if !callback.event(Event::EntryEnd) { return false }
    }
    
    callback.event(Event::ObjectEnd { span: atom.span })
}
```

## Alternative: Restructure Lexer

Could add a compound token `TokenKind::Attribute` that the lexer produces when it sees `bare=value`. This would simplify parser logic.

## Test Cases

```rust
// [verify r[attr.syntax]]
#[test]
fn test_simple_attribute() {
    let events = parse("server host=localhost");
    // key=server, value={host: localhost}
}

// [verify r[attr.values]]
#[test]
fn test_attribute_values() {
    let events = parse("config name=app tags=(a b) opts={x 1}");
}

// [verify r[attr.atom]]
#[test]
fn test_multiple_attributes() {
    let events = parse("host=localhost port=8080");
    // Single object with two entries
}

// [verify r[entry.keypath.attributes]]  
#[test]
fn test_keypath_with_attributes() {
    let events = parse("spec selector matchLabels app=web tier=frontend");
    // Nested: spec.selector.matchLabels = {app: web, tier: frontend}
}

#[test]
fn test_attribute_no_spaces() {
    // Spaces around = are NOT allowed
    let events = parse("x = y"); // This is key=x, value==, then y? Or error?
    // Per spec: = with spaces is not attribute syntax
}
```

## Tracey Annotations

- `// [impl r[attr.syntax]]` on attribute key parsing
- `// [impl r[attr.values]]` on attribute value parsing  
- `// [impl r[attr.atom]]` on attribute-to-object conversion
- `// [impl r[entry.keypath.attributes]]` on keypath + attribute composition
