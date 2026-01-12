# Phase 002: styx-parse (Lexer)

Low-level lexer that produces tokens with spans from Styx source text.

## Deliverables

- `crates/styx-parse/src/lib.rs` - Crate root
- `crates/styx-parse/src/lexer.rs` - Tokenizer
- `crates/styx-parse/src/token.rs` - Token types and spans
- `crates/styx-parse/src/error.rs` - Lexer errors

## Token Types

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TokenKind {
    // Structural
    LBrace,      // {
    RBrace,      // }
    LParen,      // (
    RParen,      // )
    Comma,       // ,
    Eq,          // =
    At,          // @

    // Scalars
    BareScalar,      // unquoted text
    QuotedScalar,    // "..." with escapes
    RawScalar,       // r#"..."#
    HeredocStart,    // <<DELIM
    HeredocContent,  // lines of heredoc
    HeredocEnd,      // closing DELIM

    // Comments
    LineComment,     // // ...
    DocComment,      // /// ...

    // Whitespace (may be significant for separator detection)
    Whitespace,      // spaces, tabs
    Newline,         // \n or \r\n

    // Special
    Eof,
    Error,
}
```

## Span Tracking

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: u32,  // byte offset
    pub end: u32,    // byte offset (exclusive)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Token<'src> {
    pub kind: TokenKind,
    pub span: Span,
    pub text: &'src str,  // slice of source
}
```

## Lexer API

```rust
pub struct Lexer<'src> {
    source: &'src str,
    pos: usize,
    // ...
}

impl<'src> Lexer<'src> {
    pub fn new(source: &'src str) -> Self;
}

impl<'src> Iterator for Lexer<'src> {
    type Item = Token<'src>;
    fn next(&mut self) -> Option<Self::Item>;
}
```

## Implementation Details

### Character Classification

```rust
fn is_bare_char(c: char) -> bool {
    // Cannot contain: {}(),"=@ or whitespace
    !matches!(c, '{' | '}' | '(' | ')' | ',' | '"' | '=' | '@')
        && !c.is_whitespace()
}
```

### Escape Sequence Handling

For quoted scalars, the lexer identifies the full token including escapes.
Escape validation and conversion happens at a higher layer (parser or tree builder).

The lexer should:
- Recognize valid escape sequences: `\\`, `\"`, `\n`, `\r`, `\t`, `\0`, `\uXXXX`, `\u{...}`
- Report invalid escapes as errors with specific spans

### Heredoc State Machine

```
State: Normal
  See "<<" → State: HeredocOpen, collect delimiter
  
State: HeredocOpen
  Collect [A-Z][A-Z0-9_]* as delimiter
  Emit HeredocStart token
  State: HeredocBody
  
State: HeredocBody
  Collect lines until line matches delimiter
  Emit HeredocContent for each line (or single token for all content)
  When delimiter found → Emit HeredocEnd, State: Normal
```

### Raw String State Machine

```
State: Normal
  See 'r' followed by '#'* and '"' → State: RawString
  Count opening '#' marks
  
State: RawString
  Collect until '"' followed by same number of '#'
  Emit RawScalar token
  State: Normal
```

### Comment Detection

Comments require preceding whitespace (or start of line/file):
- `/` at start of line/file, or after whitespace → check for `//` or `///`
- `/` elsewhere → part of bare scalar

## Error Recovery

The lexer should:
- Never panic on malformed input
- Emit `Error` tokens for unrecognized sequences
- Continue lexing after errors
- Provide helpful error spans

## Testing

- Unit tests for each token type
- Tests for escape sequences (valid and invalid)
- Tests for heredocs with various delimiters
- Tests for raw strings with 0, 1, 2+ `#` marks
- Tests for comment detection (line start, after whitespace, not after text)
- Fuzz testing for robustness
