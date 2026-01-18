+++
title = "Editor Integration"
weight = 1
+++

Styx has first-class editor support through LSP and tree-sitter.

## Zed

The Zed extension is built into the repository:

```bash
cd editors/zed-styx
cargo build --release
```

Install via Zed's extension browser (coming soon to the extension gallery).

## VS Code

Install from the [VS Code Marketplace](https://marketplace.visualstudio.com/items?itemName=bearcove.styx-lang):

```bash
code --install-extension bearcove.styx-lang
```

Or search for "Styx Configuration Language" in the Extensions view.

### Features

- **Syntax highlighting** with key/value distinction
- **Language server integration** via `styx @lsp` (diagnostics, hover, completions)
- **Heredoc language injection** for 45+ languages — SQL, JavaScript, Rust, Python, HTML, YAML, and more are syntax-highlighted inside heredocs:

```styx
migration <<SQL,sql
CREATE TABLE users (
    id SERIAL PRIMARY KEY,
    name VARCHAR(255) NOT NULL
);
SQL

script <<BASH,bash
#!/bin/bash
echo "Hello from Styx!"
BASH
```

- Bracket matching and auto-closing
- Comment toggling (`Ctrl+/` / `Cmd+/`)

### Configuration

| Setting | Default | Description |
|---------|---------|-------------|
| `styx.server.path` | `"styx"` | Path to the styx executable |
| `styx.trace.server` | `"off"` | LSP trace level: `"off"`, `"messages"`, `"verbose"` |

### Supported Heredoc Languages

The extension provides syntax highlighting for heredocs with language hints:

| Category | Languages |
|----------|-----------|
| Systems | `c`, `cpp`, `rust`, `go`, `zig` |
| Shell | `bash`, `sh`, `zsh`, `fish`, `powershell` |
| Web | `javascript`, `typescript`, `tsx`, `html`, `css`, `scss`, `svelte`, `vue` |
| Backend | `python`, `ruby`, `java`, `kotlin`, `swift`, `php`, `elixir` |
| Functional | `clojure`, `haskell`, `ocaml`, `scala` |
| Data | `sql`, `graphql`, `julia`, `r` |
| Config | `json`, `yaml`, `toml`, `xml`, `ini`, `dockerfile`, `nginx`, `hcl` |
| Other | `markdown`, `lua`, `perl`, `diff`, `nix`, `makefile` |

### Building from Source

```bash
cd editors/vscode-styx
pnpm install
pnpm run compile
```

Press F5 to launch the Extension Development Host.

## Neovim

Add the tree-sitter parser:

```lua
local parser_config = require("nvim-treesitter.parsers").get_parser_configs()
parser_config.styx = {
  install_info = {
    url = "https://github.com/bearcove/styx",
    files = { "crates/tree-sitter-styx/src/parser.c", "crates/tree-sitter-styx/src/scanner.c" },
    location = "crates/tree-sitter-styx",
  },
}
```

Configure LSP:

```lua
local configs = require("lspconfig.configs")
configs.styx = {
  default_config = {
    cmd = { "styx", "@lsp" },
    filetypes = { "styx" },
    root_dir = require("lspconfig").util.root_pattern(".git"),
  },
}
require("lspconfig").styx.setup({})
```

## Helix

Add to `~/.config/helix/languages.toml`:

```toml
[[language]]
name = "styx"
scope = "source.styx"
file-types = ["styx"]
comment-token = "//"
language-servers = ["styx-lsp"]

[[grammar]]
name = "styx"
source = { git = "https://github.com/bearcove/styx", subpath = "crates/tree-sitter-styx" }

[language-server.styx-lsp]
command = "styx"
args = ["@lsp"]
```

Then fetch and build:

```bash
hx --grammar fetch && hx --grammar build
```

## Emacs

```elisp
(use-package styx-mode
  :load-path "/path/to/styx/editors/emacs-styx"
  :mode "\\.styx\\'"
  :hook (styx-mode . eglot-ensure))
```

LSP works automatically with eglot (Emacs 29+) or lsp-mode.

## Kakoune

```bash
ln -s /path/to/styx/editors/kakoune-styx/styx.kak ~/.config/kak/autoload/
```

Add to `kak-lsp.toml`:

```toml
[language.styx]
filetypes = ["styx"]
command = "styx"
args = ["@lsp"]
```

## Sublime Text

Copy to Packages folder:

```bash
mkdir -p ~/Library/Application\ Support/Sublime\ Text/Packages/Styx
cp /path/to/styx/editors/sublime-styx/* ~/Library/Application\ Support/Sublime\ Text/Packages/Styx/
ln -s /path/to/styx/editors/shared/textmate/styx.tmLanguage.json ~/Library/Application\ Support/Sublime\ Text/Packages/Styx/
```

For LSP, install the [LSP package](https://packagecontrol.io/packages/LSP) and configure:

```json
{
  "clients": {
    "styx": {
      "enabled": true,
      "command": ["styx", "@lsp"],
      "selector": "source.styx"
    }
  }
}
```

## Kate / KDE

Copy syntax definition:

```bash
mkdir -p ~/.local/share/katepart5/syntax/
cp /path/to/styx/editors/kate-styx/styx.xml ~/.local/share/katepart5/syntax/
```

Configure LSP in Settings → Configure Kate → LSP Client → User Server Settings:

```json
{
  "servers": {
    "styx": {
      "command": ["styx", "@lsp"],
      "highlightingModeRegex": "^Styx$"
    }
  }
}
```

## JetBrains IDEs

Build and install:

```bash
cd editors/jetbrains-styx
./gradlew buildPlugin
```

Install from `build/distributions/jetbrains-styx-*.zip` via Settings → Plugins → Install from Disk.

## nano

```bash
mkdir -p ~/.nano
cp /path/to/styx/editors/nano-styx/styx.nanorc ~/.nano/
echo 'include ~/.nano/styx.nanorc' >> ~/.nanorc
```

Note: nano doesn't support LSP, only syntax highlighting.

## Other Editors

Any editor with LSP support can use:

```bash
styx @lsp
```

The server communicates over stdio using the standard Language Server Protocol.
