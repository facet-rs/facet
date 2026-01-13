# Phase 006g: Miscellaneous Validation

Smaller validation fixes for spec compliance.

## 1. Tag Name Validation (`r[tag.syntax]`)

### Spec

> r[tag.syntax]
> A tag MUST match the pattern `@[A-Za-z_][A-Za-z0-9_.-]*`.

### Current State

No validation - any bare scalar after `@` is accepted as a tag name.

### Fix

```rust
// [impl r[tag.syntax]]
fn is_valid_tag_name(name: &str) -> bool {
    let mut chars = name.chars();
    
    // First char: letter or underscore
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    
    // Rest: alphanumeric, underscore, dot, or hyphen
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-')
}

fn parse_tag_or_unit(&mut self) -> Atom<'src> {
    // ... after getting tag name ...
    
    if !is_valid_tag_name(tag_name) {
        self.errors.push(ParseError {
            span: name_token.span,
            kind: ParseErrorKind::InvalidTagName,
            message: format!("invalid tag name: {}", tag_name),
        });
    }
    
    // ... continue ...
}
```

### Tests

```rust
// [verify r[tag.syntax]]
#[test]
fn test_valid_tag_names() {
    assert!(parse("@foo").iter().all(|e| !matches!(e, Event::Error { .. })));
    assert!(parse("@_private").iter().all(|e| !matches!(e, Event::Error { .. })));
    assert!(parse("@Some.Type").iter().all(|e| !matches!(e, Event::Error { .. })));
    assert!(parse("@my-tag").iter().all(|e| !matches!(e, Event::Error { .. })));
    assert!(parse("@Type123").iter().all(|e| !matches!(e, Event::Error { .. })));
}

#[test]
fn test_invalid_tag_names() {
    // Starts with digit
    assert!(parse("@123").iter().any(|e| matches!(e, Event::Error { kind: ParseErrorKind::InvalidTagName, .. })));
    // Starts with hyphen
    assert!(parse("@-foo").iter().any(|e| matches!(e, Event::Error { kind: ParseErrorKind::InvalidTagName, .. })));
    // Contains invalid char
    assert!(parse("@foo!bar").iter().any(|e| matches!(e, Event::Error { kind: ParseErrorKind::InvalidTagName, .. })));
}
```

---

## 2. Heredoc Delimiter Validation (`r[scalar.heredoc.syntax]`)

### Spec

> r[scalar.heredoc.syntax]
> Heredocs start with `<<DELIMITER` and end with the delimiter on its own line.
> The delimiter MUST match `[A-Z][A-Z0-9_]*` and not exceed 16 characters.

### Current State

Lexer accepts any uppercase/digit/underscore sequence without validating:
- First char must be uppercase letter
- Max 16 characters

### Fix (in lexer.rs)

```rust
// [impl r[scalar.heredoc.syntax]]
fn lex_heredoc_start(&mut self) -> Token<'src> {
    let start = self.pos;

    // Consume `<<`
    self.advance();
    self.advance();

    // Collect delimiter
    let delim_start = self.pos as usize;
    
    // First char MUST be uppercase letter
    match self.peek() {
        Some(c) if c.is_ascii_uppercase() => { self.advance(); }
        _ => {
            // Invalid delimiter - first char not uppercase
            return self.token(TokenKind::Error, start);
        }
    }
    
    // Rest: uppercase, digit, or underscore
    while let Some(c) = self.peek() {
        if c.is_ascii_uppercase() || c.is_ascii_digit() || c == '_' {
            self.advance();
        } else {
            break;
        }
    }
    
    let delimiter = &self.source[delim_start..self.pos as usize];
    
    // Check length <= 16
    if delimiter.len() > 16 {
        // Too long
        return self.token(TokenKind::Error, start);
    }
    
    // ... rest unchanged ...
}
```

### Tests

```rust
// [verify r[scalar.heredoc.syntax]]
#[test]
fn test_valid_heredoc_delimiters() {
    assert!(lex("<<EOF\ntest\nEOF").iter().all(|t| t.kind != TokenKind::Error));
    assert!(lex("<<A\nx\nA").iter().all(|t| t.kind != TokenKind::Error));
    assert!(lex("<<MY_DELIM123\nx\nMY_DELIM123").iter().all(|t| t.kind != TokenKind::Error));
}

#[test]
fn test_heredoc_must_start_uppercase() {
    // Starts with digit
    assert!(lex("<<123").iter().any(|t| t.kind == TokenKind::Error));
    // Starts with underscore
    assert!(lex("<<_FOO").iter().any(|t| t.kind == TokenKind::Error));
}

#[test]
fn test_heredoc_max_16_chars() {
    // 16 chars OK
    assert!(lex("<<ABCDEFGHIJKLMNOP\nx\nABCDEFGHIJKLMNOP").iter().all(|t| t.kind != TokenKind::Error));
    // 17 chars error
    assert!(lex("<<ABCDEFGHIJKLMNOPQ\nx\nABCDEFGHIJKLMNOPQ").iter().any(|t| t.kind == TokenKind::Error));
}
```

---

## 3. Unicode Escape `\uXXXX` (`r[scalar.quoted.escapes]`)

### Spec

> r[scalar.quoted.escapes]
> Quoted scalars use `"..."` and support escape sequences:
> `\\`, `\"`, `\n`, `\r`, `\t`, `\0`, `\uXXXX`, `\u{X...}`.

### Current State

Only `\u{...}` is implemented, not 4-digit `\uXXXX`.

### Fix

```rust
// [impl r[scalar.quoted.escapes]]
fn unescape_quoted(&self, text: &'src str) -> Cow<'src, str> {
    // ... existing setup ...
    
    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                // ... existing cases ...
                
                Some('u') => {
                    match chars.peek() {
                        Some('{') => {
                            // \u{X...} form
                            chars.next(); // consume '{'
                            let mut hex = String::new();
                            while let Some(&c) = chars.peek() {
                                if c == '}' {
                                    chars.next();
                                    break;
                                }
                                hex.push(chars.next().unwrap());
                            }
                            if let Ok(code) = u32::from_str_radix(&hex, 16) {
                                if let Some(ch) = char::from_u32(code) {
                                    result.push(ch);
                                }
                            }
                        }
                        Some(c) if c.is_ascii_hexdigit() => {
                            // \uXXXX form (exactly 4 hex digits)
                            let mut hex = String::with_capacity(4);
                            for _ in 0..4 {
                                if let Some(&c) = chars.peek() {
                                    if c.is_ascii_hexdigit() {
                                        hex.push(chars.next().unwrap());
                                    } else {
                                        break;
                                    }
                                }
                            }
                            if hex.len() == 4 {
                                if let Ok(code) = u32::from_str_radix(&hex, 16) {
                                    if let Some(ch) = char::from_u32(code) {
                                        result.push(ch);
                                    }
                                }
                            } else {
                                // Invalid escape - not enough digits
                                result.push_str("\\u");
                                result.push_str(&hex);
                            }
                        }
                        _ => {
                            // Invalid \u
                            result.push_str("\\u");
                        }
                    }
                }
                
                // ... rest ...
            }
        } else {
            result.push(c);
        }
    }
    
    Cow::Owned(result)
}
```

### Tests

```rust
// [verify r[scalar.quoted.escapes]]
#[test]
fn test_unicode_escape_braces() {
    let events = parse(r#"x "\u{1F600}""#);
    assert!(events.iter().any(|e| matches!(e, Event::Scalar { value, .. } if value == "😀")));
}

#[test]
fn test_unicode_escape_4digit() {
    let events = parse(r#"x "\u0041""#);
    assert!(events.iter().any(|e| matches!(e, Event::Scalar { value, .. } if value == "A")));
}

#[test]
fn test_unicode_escape_4digit_emoji() {
    // Can't represent emoji with \uXXXX (needs surrogate pairs), but can do BMP chars
    let events = parse(r#"x "\u00E9""#); // é
    assert!(events.iter().any(|e| matches!(e, Event::Scalar { value, .. } if value == "é")));
}
```

---

## 4. Comment Positioning (`r[comment.line]`)

### Spec

> r[comment.line]
> Comments MUST either start at the beginning of the file or be preceded by whitespace.

### Current State

The lexer treats `//` as always starting a comment. But `foo//bar` should be parsed as `foo//bar` (bare scalar), not `foo` + comment.

### Note

Actually, the current lexer handles this correctly because:
- `is_bare_scalar_start('/')` returns false (excluded)
- But `is_bare_scalar_char('/')` returns true (allowed after first char)

So `https://example.com` works correctly. The issue would be `foo//comment` where `//` appears in a bare scalar position.

Let me re-check... Actually the lexer excludes `/` from starting a bare scalar but allows it after. So:
- `//comment` → comment (correct)
- `foo` then `//comment` → bare scalar `foo`, then comment (correct)
- `foo//bar` → This is interesting...

The lexer would see `foo` (bare scalar terminates at... nothing, it continues with `//bar`). Actually no, `/` is allowed in bare scalars, so `foo//bar` would be lexed as one bare scalar `foo//bar`.

Hmm, that might be wrong per the spec. Let me re-read...

> Comments MUST either start at the beginning of the file or be preceded by whitespace.
> ```styx
> url https://example.com  // the :// is not a comment
> ```

OK so `://` inside a URL is not a comment because it's part of the bare scalar. The rule is about `//` being preceded by whitespace (or BOF).

The current implementation seems correct for this case. `foo//bar` is a valid bare scalar (no whitespace before `//`).

But `foo //comment` has whitespace, so `//comment` should be a comment. And indeed the lexer would tokenize as: `foo`, `whitespace`, `//comment`.

This seems OK. Skip this fix.

---

## 5. Doc Comment Validation (`r[comment.doc]`)

### Spec

> r[comment.doc]
> A doc comment not followed by an entry (blank line or EOF) is an error.

### Current State

No validation. Dangling doc comments are silently accepted.

### Fix

Track doc comments and verify they're followed by an entry:

```rust
fn parse_entries<C: ParseCallback<'src>>(&mut self, callback: &mut C, closing: Option<TokenKind>) {
    let mut pending_doc_comment: Option<Span> = None;
    
    loop {
        // ... existing token handling ...
        
        if token.kind == TokenKind::DocComment {
            if pending_doc_comment.is_some() {
                // Consecutive doc comments are OK, they get concatenated
            }
            pending_doc_comment = Some(token.span);
            // ... emit doc comment event ...
            self.skip_whitespace_and_newlines();
            continue;
        }
        
        // If we're about to parse an entry, clear pending doc comment
        if token.kind.is_entry_start() {
            pending_doc_comment = None;
            // ... parse entry ...
        }
        
        // If we hit EOF or closing brace with pending doc comment, error
        if token.kind == TokenKind::Eof || Some(token.kind) == closing {
            if let Some(span) = pending_doc_comment {
                // [impl r[comment.doc]]
                callback.event(Event::Error {
                    span,
                    kind: ParseErrorKind::DanglingDocComment,
                });
            }
            break;
        }
    }
}
```

### Tests

```rust
// [verify r[comment.doc]]
#[test]
fn test_doc_comment_followed_by_entry() {
    let events = parse("/// doc\nfoo bar");
    assert!(!events.iter().any(|e| matches!(e, Event::Error { .. })));
}

#[test]
fn test_doc_comment_at_eof_error() {
    let events = parse("foo bar\n/// dangling");
    assert!(events.iter().any(|e| matches!(e, Event::Error { kind: ParseErrorKind::DanglingDocComment, .. })));
}

#[test]
fn test_doc_comment_followed_by_blank_error() {
    let events = parse("/// doc\n\nfoo bar");
    assert!(events.iter().any(|e| matches!(e, Event::Error { kind: ParseErrorKind::DanglingDocComment, .. })));
}
```

---

## 6. Heredoc Keys Forbidden (`r[entry.keys]`)

### Spec

> r[entry.keys]
> Heredoc scalars are not allowed as keys.

### Fix

Check in entry parsing:

```rust
// After parsing key atom
if key_atom.kind == ScalarKind::Heredoc {
    // [impl r[entry.keys]]
    self.errors.push(ParseError {
        span: key_atom.span,
        kind: ParseErrorKind::InvalidKey,
        message: "heredoc scalars cannot be used as keys".into(),
    });
}
```

### Tests

```rust
// [verify r[entry.keys]]
#[test]
fn test_heredoc_key_error() {
    let events = parse("<<EOF\nkey\nEOF value");
    assert!(events.iter().any(|e| matches!(e, Event::Error { kind: ParseErrorKind::InvalidKey, .. })));
}
```

---

## Error Kinds to Add

```rust
pub enum ParseErrorKind {
    // ... existing ...
    InvalidTagName,
    InvalidKey,
    DanglingDocComment,
}
```

## Tracey Annotations Summary

- `// [impl r[tag.syntax]]` - tag name validation
- `// [impl r[scalar.heredoc.syntax]]` - heredoc delimiter validation
- `// [impl r[scalar.quoted.escapes]]` - unicode escape handling
- `// [impl r[comment.doc]]` - doc comment validation
- `// [impl r[entry.keys]]` - heredoc key rejection
