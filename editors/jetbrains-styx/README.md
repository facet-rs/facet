# Styx for JetBrains IDEs

Styx language support for IntelliJ IDEA, WebStorm, PyCharm, and other JetBrains IDEs.

## Installation

### From JetBrains Marketplace

Coming soon.

### Build from source

```bash
cd editors/jetbrains-styx
gradle wrapper
./gradlew buildPlugin
```

Then install the plugin from `build/distributions/jetbrains-styx-0.1.0.zip`:

1. Open your JetBrains IDE
2. Go to Settings → Plugins → ⚙️ → Install Plugin from Disk...
3. Select the ZIP file

## Requirements

The Styx CLI must be installed:

```bash
cargo install styx-cli
```

## Features

- Syntax highlighting
- LSP integration via [LSP4IJ](https://github.com/redhat-developer/lsp4ij)
- Bracket matching
- Comment toggling

## Development

Open the project in IntelliJ IDEA with the Gradle plugin, then:

```bash
./gradlew runIde
```

This launches a sandboxed IDE with the plugin installed.

## Note

This plugin uses LSP4IJ for language server integration. The lexer and parser
are minimal stubs — all the intelligence comes from the LSP.
