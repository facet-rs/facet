# Editor Extensions

**Status:** TODO  
**Priority:** Medium  
**Depends on:** LSP completion

## Overview

Editor-specific integrations beyond the LSP.

## Zed (zed-styx)

**Current state:** Working tree-sitter integration, LSP configured.

Remaining:
- Ensure extension published to Zed extension registry
- Test all LSP features work correctly
- Add extension icon

## VS Code (vscode-styx)

**Current state:** Not started.

Needs:
- TextMate grammar (can derive from tree-sitter)
- LSP client configuration
- Extension manifest (package.json)
- Syntax highlighting theme integration
- Publish to VS Code marketplace

## Neovim (nvim-styx)

**Current state:** Not started.

Needs:
- Tree-sitter parser integration (nvim-treesitter)
- LSP configuration snippet for lspconfig
- Syntax highlighting queries
- Documentation for setup

## Implementation Order

1. Polish Zed extension (lowest effort, already working)
2. VS Code extension (largest user base)
3. Neovim support (enthusiast audience)
