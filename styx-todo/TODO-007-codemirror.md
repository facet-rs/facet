# TODO-007: CodeMirror Language Support & Web Playground

## Status
TODO

## Description
Create a CodeMirror language package for Styx and a web playground with full LSP support.

## Part 1: Basic CodeMirror Support

### Reference
- https://codemirror.net/examples/lang-package/

### Content
- Lezer grammar (CodeMirror's parser generator)
- Syntax highlighting theme
- Language data (comments, brackets, etc.)
- npm package for distribution (`@bearcove/codemirror-lang-styx` or similar)

## Part 2: Web Playground with LSP

### Architecture
1. **CodeMirror 6** with the lezer grammar for syntax highlighting
2. **codemirror-languageserver** extension for LSP integration
   - https://github.com/FurqanSoftware/codemirror-languageserver
   - See issue #20 for Web Worker approach: https://github.com/FurqanSoftware/codemirror-languageserver/issues/20
   - Someone already built `PostMessageWorkerTransport` — use `languageServerWithTransport` instead of `languageServer`
3. **Styx LSP compiled to WASM**, running in a Web Worker
4. **WorkerTransport** shim that bridges postMessage ↔ LSP JSON-RPC (prior art in issue #20)

### Why Web Worker?
Can't run server-side LSP for a public playground (scaling, cost). Instead:
- Compile `styx-lsp` to WASM via `wasm32-unknown-unknown` or `wasm32-wasi`
- Run in a Web Worker so it doesn't block the main thread
- Custom transport that looks like WebSocket but uses `postMessage`

### Transport Shim
```typescript
// Instead of WebSocketTransport, something like:
class WorkerTransport {
  private worker: Worker;

  constructor(wasmUrl: string) {
    this.worker = new Worker(wasmUrl);
  }

  send(message: string) {
    this.worker.postMessage(message);
  }

  onMessage(callback: (msg: string) => void) {
    this.worker.onmessage = (e) => callback(e.data);
  }
}
```

### LSP Features in Playground
- Diagnostics (syntax errors, schema validation errors)
- Autocomplete
- Hover documentation
- Maybe: schema picker (select from example schemas)

## Notes
This enables Styx syntax support anywhere CodeMirror is used, and gives us a zero-install way for people to try Styx with full tooling.
