# Phase 006f: Separator Mode Enforcement

Enforce that objects use exactly one separator mode per `r[object.separators]`.

## Spec Reference

> r[object.separators]
> Entries are separated by newlines or commas. Duplicate keys are forbidden.
> An object MUST use exactly one separator mode:
>
> - **newline-separated**: entries separated by newlines; commas forbidden
> - **comma-separated**: entries separated by commas; newlines forbidden
>
> Comma-separated objects are single-line (except for heredoc content).

## Current State

The parser's `skip_whitespace_and_newlines()` is called unconditionally, allowing mixing:

```styx
{a 1, b 2
c 3}
```

This should be an error but is currently accepted.

## Required Changes

### 1. Track separator mode during object parsing

```rust
// [impl r[object.separators]]
fn parse_object_atom(&mut self) -> Atom<'src> {
    let open = self.advance().unwrap();
    let start_span = open.span;
    
    let mut entries = Vec::new();
    let mut separator_mode: Option<Separator> = None;
    let mut seen_keys = HashSet::new();
    
    loop {
        // Only skip whitespace (not newlines) until we know the mode
        self.skip_whitespace();
        
        let Some(token) = self.peek() else { break };
        
        match token.kind {
            TokenKind::RBrace => {
                let close = self.advance().unwrap();
                return Atom {
                    span: Span { start: start_span.start, end: close.span.end },
                    kind: ScalarKind::Bare,
                    content: AtomContent::Object { 
                        entries, 
                        separator: separator_mode.unwrap_or(Separator::Newline) 
                    },
                };
            }
            
            TokenKind::Newline => {
                match separator_mode {
                    None => separator_mode = Some(Separator::Newline),
                    Some(Separator::Newline) => { /* OK */ }
                    Some(Separator::Comma) => {
                        // Error: mixed separators
                        self.errors.push(ParseError {
                            span: token.span,
                            kind: ParseErrorKind::MixedSeparators,
                            message: "newline in comma-separated object".into(),
                        });
                    }
                }
                self.advance();
                // In newline mode, consume consecutive newlines
                while matches!(self.peek(), Some(t) if t.kind == TokenKind::Newline) {
                    self.advance();
                }
            }
            
            TokenKind::Comma => {
                match separator_mode {
                    None => separator_mode = Some(Separator::Comma),
                    Some(Separator::Comma) => { /* OK */ }
                    Some(Separator::Newline) => {
                        // Error: mixed separators
                        self.errors.push(ParseError {
                            span: token.span,
                            kind: ParseErrorKind::MixedSeparators,
                            message: "comma in newline-separated object".into(),
                        });
                    }
                }
                self.advance();
            }
            
            TokenKind::LineComment | TokenKind::DocComment => {
                // Comments are allowed in newline-separated objects
                // In comma-separated, comments would span to EOL which breaks single-line rule
                if separator_mode == Some(Separator::Comma) {
                    self.errors.push(ParseError {
                        span: token.span,
                        kind: ParseErrorKind::InvalidToken,
                        message: "comments not allowed in comma-separated objects".into(),
                    });
                }
                self.advance();
            }
            
            _ => {
                // Parse entry
                let entry_atoms = self.collect_entry_atoms_in_object();
                // ... rest of entry parsing ...
            }
        }
    }
    
    // Unclosed object error
    // ...
}
```

### 2. Validate first separator determines mode

The first separator seen (comma or newline after first entry) determines the mode. All subsequent separators must match.

### 3. Emit errors for violations

```rust
pub enum ParseErrorKind {
    // ... existing ...
    /// Mixed separators in object (some commas, some newlines).
    MixedSeparators,
    /// Invalid token in context.
    InvalidToken,
}
```

### 4. Also enforce at document root

The document root is an implicit object, so the same rules apply:

```rust
fn parse_entries<C: ParseCallback<'src>>(&mut self, callback: &mut C, closing: Option<TokenKind>) {
    let mut separator_mode: Option<Separator> = None;
    
    // ... same logic as object parsing for tracking separators ...
}
```

## Edge Cases

### Heredoc content exception

> Comma-separated objects are single-line (except for heredoc content).

```styx
{script <<EOF
echo hello
EOF, name foo}
```

This is valid even though there are newlines - they're inside the heredoc.

### Empty objects

```styx
{}
```

No separator mode is determined (both are valid for empty objects).

### Single entry

```styx
{a 1}
```

No separator needed, either mode is valid.

## Test Cases

```rust
// [verify r[object.separators]]

#[test]
fn test_newline_separated() {
    let events = parse("{a 1\nb 2\nc 3}");
    assert!(events.iter().any(|e| matches!(e, Event::ObjectStart { separator: Separator::Newline, .. })));
    assert!(!events.iter().any(|e| matches!(e, Event::Error { .. })));
}

#[test]
fn test_comma_separated() {
    let events = parse("{a 1, b 2, c 3}");
    assert!(events.iter().any(|e| matches!(e, Event::ObjectStart { separator: Separator::Comma, .. })));
    assert!(!events.iter().any(|e| matches!(e, Event::Error { .. })));
}

#[test]
fn test_mixed_separators_error() {
    let events = parse("{a 1, b 2\nc 3}");
    assert!(events.iter().any(|e| matches!(e, Event::Error { kind: ParseErrorKind::MixedSeparators, .. })));
}

#[test]
fn test_mixed_separators_error_reverse() {
    let events = parse("{a 1\nb 2, c 3}");
    assert!(events.iter().any(|e| matches!(e, Event::Error { kind: ParseErrorKind::MixedSeparators, .. })));
}

#[test]
fn test_empty_object_ok() {
    let events = parse("{}");
    assert!(!events.iter().any(|e| matches!(e, Event::Error { .. })));
}

#[test]
fn test_single_entry_ok() {
    let events = parse("{a 1}");
    assert!(!events.iter().any(|e| matches!(e, Event::Error { .. })));
}

#[test]
fn test_heredoc_in_comma_object() {
    let events = parse("{script <<EOF\necho hi\nEOF, name foo}");
    // Newlines inside heredoc don't count
    assert!(!events.iter().any(|e| matches!(e, Event::Error { kind: ParseErrorKind::MixedSeparators, .. })));
}

#[test]
fn test_document_root_mixed_error() {
    let events = parse("a 1, b 2\nc 3");
    assert!(events.iter().any(|e| matches!(e, Event::Error { kind: ParseErrorKind::MixedSeparators, .. })));
}
```

## Tracey Annotations

- `// [impl r[object.separators]]` on separator mode tracking and validation
