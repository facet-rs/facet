# Styx for Emacs

Major mode for editing Styx configuration files in Emacs.

## Installation

### Manual

Copy `styx-mode.el` to your load path and add to your init file:

```elisp
(require 'styx-mode)
```

### use-package

```elisp
(use-package styx-mode
  :load-path "/path/to/styx/editors/emacs-styx"
  :mode "\\.styx\\'")
```

### straight.el

```elisp
(straight-use-package
 '(styx-mode :type git :host github :repo "bearcove/styx"
             :files ("editors/emacs-styx/*.el")))
```

## LSP Support

### With Eglot (built-in since Emacs 29)

The LSP server is automatically configured. Just open a `.styx` file and run:

```
M-x eglot
```

### With lsp-mode

```elisp
(use-package lsp-mode
  :hook (styx-mode . lsp-deferred)
  :commands lsp)
```

## Requirements

The Styx CLI must be installed:

```bash
cargo install styx-cli
```

## Features

- Syntax highlighting
- Comment toggling with `M-;`
- Automatic indentation
- LSP integration (eglot or lsp-mode)
