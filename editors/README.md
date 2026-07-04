# Vix Editor Packaging

Build the server once from the repository root:

```sh
cargo build -p vix-lsp --release
```

The binary is written to `target/release/vix-lsp`. Put that directory on `PATH`,
or configure the editor with the absolute path
`/Users/amos/oss/facet-cc/target/release/vix-lsp`.

## Zed

Install the local extension by symlinking this package into Zed's installed
extensions directory:

```sh
mkdir -p ~/.config/zed/extensions/installed
ln -sfn /Users/amos/oss/facet-cc/editors/zed ~/.config/zed/extensions/installed/vix
```

If `target/release` is not on `PATH`, add this to Zed settings:

```json
{
  "lsp": {
    "vix-lsp": {
      "binary": {
        "path": "/Users/amos/oss/facet-cc/target/release/vix-lsp"
      }
    }
  }
}
```

Open a `.vix` file. Zed should start `vix-lsp`; hover, go-to-definition,
references, rename, diagnostics, and semantic tokens come from the LSP server.
Zed shows server stderr in its LSP log view, which is the first place to check
if the server does not attach.

## VSCode

Install dependencies for the local extension:

```sh
cd /Users/amos/oss/facet-cc/editors/vscode
npm install
```

Run it from source with VSCode's extension development host:

```sh
code --extensionDevelopmentPath=/Users/amos/oss/facet-cc/editors/vscode /Users/amos/oss/facet-cc
```

If `target/release` is not on `PATH`, set:

```json
{
  "vix.server.path": "/Users/amos/oss/facet-cc/target/release/vix-lsp"
}
```

## Troubleshooting

`vix-lsp` writes compact tracing to stderr and to a daily rolling file sink.
stdout remains the LSP protocol channel.

The default file directory is `/tmp/vix-lsp`. Log files are named
`vix-lsp.log.YYYY-MM-DD`, and the server keeps 7 files by default.

Configuration environment variables:

- `VIX_LSP_LOG_DIR`: log directory, default `/tmp/vix-lsp`
- `VIX_LSP_LOG_LEVEL`: tracing filter for both sinks, default `info`
- `VIX_LSP_LOG_RETENTION`: maximum retained daily files, default `7`

The Zed extension surfaces stderr through Zed's LSP log view, so check that view
before hunting for files. VSCode surfaces server output in the extension host
logs; the rolling files are the fallback when editor logs do not include enough
context.
