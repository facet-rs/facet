# Developing Styx

## Running Tests

```bash
# Run all tests
cargo nextest run

# Run tests for a specific crate
cargo nextest run -p styx-format
```

## Property Testing

The formatter uses [proptest](https://proptest-rs.github.io/proptest/) to find edge cases through fuzz testing. Property tests are in `crates/styx-format/src/cst_format.rs`.

Two invariants are tested:

1. **Semantics preservation** - formatting must not change the document's meaning (tree equality ignoring spans)
2. **Idempotence** - `format(format(x)) == format(x)`

Run with more cases to find rare bugs:

```bash
PROPTEST_CASES=5000 cargo nextest run -p styx-format proptests
```

When proptest finds a failing case, it saves it to `proptest-regressions/` for deterministic replay.

## Installing the CLI

```bash
cargo xtask install
```

This builds a release binary, copies it to `~/.cargo/bin/styx`, and codesigns it on macOS.

## Publishing npm Packages

The Styx playgrounds and editor integrations are published to npm. These packages are loaded via [esm.sh](https://esm.sh) CDN in the docs site.

### Prerequisites

```bash
# For WASM builds
rustup target add wasm32-unknown-unknown
cargo install wasm-bindgen-cli

# npm login (if not already logged in)
npm login
```

### Packages

| Package | Location | Description |
|---------|----------|-------------|
| `@bearcove/styx-wasm` | `crates/styx-wasm` | WASM bindings for parser |
| `@bearcove/codemirror-lang-styx` | `editors/codemirror-styx` | CodeMirror 6 language support |
| `@bearcove/monaco-lang-styx` | `editors/monaco-styx` | Monaco editor language support |

### Publishing

Each package has `prepublishOnly` scripts that build automatically:

```bash
# Publish all packages
cd crates/styx-wasm && pnpm publish
cd editors/codemirror-styx && pnpm publish
cd editors/monaco-styx && pnpm publish
```

After publishing, the docs site playgrounds will automatically use the new versions via esm.sh (may take a few minutes for CDN cache).

### Version Bumps

Update the version in each `package.json` before publishing. The playground templates in `docs/templates/` reference specific versions in esm.sh URLs - update those too:

```js
// docs/templates/monaco.html, codemirror.html
import { ... } from 'https://esm.sh/@bearcove/styx-wasm@0.1.0';
```
