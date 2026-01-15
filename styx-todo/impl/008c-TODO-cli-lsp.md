# Phase 008c: styx @lsp (Language Server)

Entry point for the LSP server (see 009-TODO-lsp.md for full spec).

## Usage

```bash
styx @lsp                 # start LSP on stdin/stdout
styx @lsp --stdio         # explicit stdio mode
styx @lsp --tcp 9000      # TCP mode on port
```

## Implementation

This is just the CLI entry point. The actual LSP implementation
is in `crates/styx-lsp/` (see 009-TODO-lsp.md).

```rust
Command::Lsp { .. } => {
    styx_lsp::run()?;
}
```
