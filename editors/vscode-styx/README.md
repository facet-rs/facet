# Styx for VS Code

Styx language support for Visual Studio Code.

## Features

- Syntax highlighting
- Language server integration (diagnostics, hover, completions)
- Bracket matching and auto-closing
- Comment toggling

## Requirements

The Styx CLI must be installed and available in your PATH:

```bash
cargo install styx-cli
```

Or configure a custom path in settings:

```json
{
  "styx.server.path": "/path/to/styx"
}
```

## Extension Settings

- `styx.server.path`: Path to the styx executable (default: `"styx"`)
- `styx.trace.server`: Trace communication with language server (`"off"`, `"messages"`, `"verbose"`)

## Development

```bash
cd editors/vscode-styx
npm install
npm run compile
```

Then press F5 in VS Code to launch the extension in a new window.

## Packaging

```bash
npm run package
```

This creates a `.vsix` file you can install locally or publish to the marketplace.
