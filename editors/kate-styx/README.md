# Styx for Kate

Styx syntax highlighting and LSP support for [Kate](https://kate-editor.org/) and other KDE editors using KSyntaxHighlighting.

## Installation

### Syntax Highlighting

Copy the syntax definition to your local Kate syntax directory:

```bash
mkdir -p ~/.local/share/katepart5/syntax/
cp /path/to/styx/editors/kate-styx/styx.xml ~/.local/share/katepart5/syntax/
```

Or for system-wide installation (requires root):

```bash
sudo cp styx.xml /usr/share/katepart5/syntax/
```

Restart Kate and `.styx` files should be recognized automatically.

### LSP Support

Kate has built-in LSP support. Configure it in Settings → Configure Kate → LSP Client → User Server Settings:

```json
{
  "servers": {
    "styx": {
      "command": ["styx", "lsp"],
      "rootIndicationFileNames": [".git", "*.styx"],
      "highlightingModeRegex": "^Styx$"
    }
  }
}
```

## Requirements

The Styx CLI must be installed:

```bash
cargo install styx-cli
```

## Features

- Syntax highlighting
- LSP integration (diagnostics, hover, completions)
- Code folding for objects
- Comment toggling

## Other KDE Apps

This syntax definition also works with:

- **KWrite**
- **KDevelop**
- **Any app using KSyntaxHighlighting**

## Contributing Upstream

To add Styx to KSyntaxHighlighting (so it ships with KDE):

1. Fork https://invent.kde.org/frameworks/syntax-highlighting
2. Add `styx.xml` to `data/syntax/`
3. Submit merge request

Docs: https://kate-editor.org/syntax/
