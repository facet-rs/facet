# 014a - Parser Diagnostics

**Status**: In Progress  
**Parent**: 014-TODO-diagnostics.md

## Goal

Add ariadne-based error rendering to `styx-parse` for nice parser error messages.

## Implementation

1. Add `ariadne` dependency to `styx-parse`
2. Add `render()` method to `BuildError` 
3. Add snapshot tests using `insta`

## Files to modify

- `crates/styx-parse/Cargo.toml` - add ariadne, insta
- `crates/styx-parse/src/lib.rs` - export render functionality
- `crates/styx-parse/src/diagnostic.rs` - new file for rendering
- `crates/styx-parse/src/snapshots/` - snapshot test outputs

## Spec coverage

Parser diagnostics from `r[diagnostic.parser.*]`:
- [ ] `r[diagnostic.parser.unexpected]`
- [ ] `r[diagnostic.parser.unclosed]`
- [ ] `r[diagnostic.parser.escape]`
- [ ] `r[diagnostic.parser.unterminated-string]`
- [ ] `r[diagnostic.parser.unterminated-heredoc]`
- [ ] `r[diagnostic.parser.heredoc-delimiter-length]`
- [ ] `r[diagnostic.parser.heredoc-indent]`
- [ ] `r[diagnostic.parser.comment-whitespace]`
- [ ] `r[diagnostic.parser.duplicate-key]`
- [ ] `r[diagnostic.parser.mixed-separators]`
- [ ] `r[diagnostic.parser.sequence-comma]`
- [ ] `r[diagnostic.parser.attr-in-sequence]`
- [ ] `r[diagnostic.parser.trailing-content]`
