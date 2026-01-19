# TODO-009: Documentation

## Status
TODO

## Description
Complete the documentation site with all referenced pages.

## Pages to Complete

### /tools/editor.md
- [ ] Zed extension installation instructions
- [ ] VS Code extension (needs to be built first)
- [ ] Neovim setup (tree-sitter + nvim-lspconfig)
- [ ] Generic LSP setup instructions
- [ ] Screenshots of each editor

### /tools/cli.md
- [ ] Actual CLI commands (verify against implementation)
- [ ] GitHub Actions example for CI
- [ ] Examples with real output
- [ ] Schema validation examples

### /playground (custom template)
- [ ] Build CodeMirror integration (see TODO-007)
- [ ] Create `playground.html` template in Dodeca
- [ ] Embed WASM LSP
- [ ] Add example schemas to choose from

### /guides/integrate.md
- [ ] Complete Rust example with error handling
- [ ] Schema validation example
- [ ] JavaScript/TypeScript bindings (needs implementation)
- [ ] Python bindings (needs implementation)
- [ ] Go bindings (needs implementation)

### /learn/primer.md
- [ ] Review and update for current syntax
- [ ] Add interactive examples (link to playground)

## Homepage Demos (Offensively Nice Tooling section)

- [ ] Editor integration screenshot/gif
- [ ] Terminal recording of CLI validation (asciinema?)
- [ ] Link to playground once built

## Dependencies
- TODO-007: CodeMirror (for playground)
- TODO-005: Language bindings (for integration guide)
- Actual CLI implementation
- Actual LSP implementation
