# @bearcove/codemirror-lang-styx

[Styx](https://styx.bearcove.eu) language support for [CodeMirror 6](https://codemirror.net/).

## Installation

```bash
npm install @bearcove/codemirror-lang-styx
```

## Usage

```typescript
import { EditorView, basicSetup } from "codemirror";
import { styx } from "@bearcove/codemirror-lang-styx";

new EditorView({
  doc: `// Example Styx config
server {
  host localhost
  port 8080
  tls {
    enabled true
    cert "/etc/ssl/cert.pem"
  }
}`,
  extensions: [basicSetup, styx()],
  parent: document.getElementById("editor")!,
});
```

## Features

- Syntax highlighting for all Styx constructs
- Code folding for objects, sequences, and heredocs
- Auto-closing brackets and quotes
- Basic autocompletion for common schema tags
- Comment toggling support

## Development

```bash
# Install dependencies
npm install

# Build the grammar and bundle
npm run build

# Watch mode for development
npm run dev

# Run tests
npm test
```

## License

MIT
