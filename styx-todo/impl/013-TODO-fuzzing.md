# Phase 013: Fuzzing

Add fuzz testing to catch infinite loops, panics, and other edge cases.

## Priority: HIGH

The parser was found to have an infinite loop when parsing unclosed heredocs.
This was only discovered by chance during testing - a fuzzer would have caught it immediately.

## Tool: proptest

Use [`proptest`](https://lib.rs/crates/proptest) for property-based testing. It integrates
well with cargo test, doesn't require nightly, and provides excellent shrinking.

## Targets

1. **Lexer** (`styx-parse::lexer`)
   - Feed arbitrary strings, ensure no panics or infinite loops
   - Test edge cases: unclosed strings, unclosed heredocs, malformed input

2. **Parser** (`styx-parse::parser`)
   - Feed arbitrary strings, ensure parsing terminates
   - Structured generation of valid Styx syntax for deeper testing

3. **Document Tree** (`styx-tree`)
   - Round-trip testing: parse -> serialize -> parse should be equivalent

## Known Issues Found

- Unclosed heredoc causes infinite loop in lexer (found 2026-01-13)
  - Input: `<<EOF\nkey\nEOF value` (delimiter not on its own line)
  - The heredoc never terminates, lexer loops forever
  - **FIXED**: Lexer now clears heredoc_state on EOF

## Implementation

```toml
# In styx-parse/Cargo.toml
[dev-dependencies]
proptest = "1"
```

```rust
// In styx-parse/src/lexer.rs or tests/fuzz.rs
use proptest::prelude::*;

proptest! {
    #[test]
    fn lexer_never_panics(input in "\\PC*") {
        let mut lexer = Lexer::new(&input);
        // Must terminate - iterator is bounded by input length + 1 (EOF)
        for token in &mut lexer {
            // Just consume all tokens
            let _ = token;
        }
    }

    #[test]
    fn lexer_terminates_on_heredocs(input in "<<[A-Z]+\n(.|\\n)*") {
        let mut lexer = Lexer::new(&input);
        let mut count = 0;
        loop {
            let token = lexer.next_token();
            if token.kind == TokenKind::Eof {
                break;
            }
            count += 1;
            // Sanity bound - input of length N should not produce more than 2N tokens
            assert!(count <= input.len() * 2 + 10);
        }
    }

    #[test]
    fn parser_never_panics(input in "\\PC*") {
        let _ = parse(&input);
    }
}
```

## Structured Generation

For deeper testing, generate valid Styx documents:

```rust
fn arb_scalar() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-zA-Z_][a-zA-Z0-9_]*",  // bare scalar
        "\"[^\"\\\\]*\"",          // simple quoted
        "\\d+",                    // numeric
    ]
}

fn arb_entry() -> impl Strategy<Value = String> {
    (arb_scalar(), arb_value()).prop_map(|(k, v)| format!("{} {}", k, v))
}
```

## Tracey Annotations

- `// [impl r[fuzz.lexer]]` - lexer fuzz target
- `// [impl r[fuzz.parser]]` - parser fuzz target
