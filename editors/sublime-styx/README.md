# Styx for Sublime Text

Styx syntax highlighting for Sublime Text.

## Installation

### Manual

1. Open Sublime Text
2. Go to `Preferences > Browse Packages...`
3. Create a `Styx` folder
4. Copy the contents of this directory into it
5. Symlink or copy the TextMate grammar:
   ```bash
   ln -s /path/to/styx/editors/shared/textmate/styx.tmLanguage.json Styx.tmLanguage.json
   ```

### Package Control

Not yet available. Coming soon.

## Features

- Syntax highlighting for `.styx` files
- Comment toggling with `Cmd+/` / `Ctrl+/`

## LSP Support

For LSP support, install [LSP](https://packagecontrol.io/packages/LSP) and configure:

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
