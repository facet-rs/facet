# TODO-012: Schema Cache Implementation

## Status
TODO

## Description
Implement the schema cache as specified in `docs/content/tools/schema-distribution.md`.

## Cache Location
- Linux/macOS: `$XDG_CACHE_HOME/styx/schemas/` (default: `~/.cache/styx/schemas/`)
- Windows: `%LOCALAPPDATA%\styx\cache\schemas\`

## Cache Structure
```
~/.cache/styx/schemas/
├── embedded/
│   └── <cli-name>/
│       └── <content-hash>.styx    # extracted from binary
├── crates/
│   └── <crate-name>/
│       └── <full-version>/
│           └── schema.styx        # fetched from crates.io
└── resolve.json                   # maps major versions to resolved full versions
```

## Implementation Tasks

### 1. Cache directory utilities
Create `crates/styx-cache/src/lib.rs` (or add to existing crate):
- `fn cache_dir() -> PathBuf` - returns platform-appropriate cache directory
- `fn embedded_cache_path(cli_name: &str, content_hash: &str) -> PathBuf`
- `fn crate_cache_path(crate_name: &str, version: &str) -> PathBuf`

### 2. Embedded schema caching
When extracting from a binary:
1. Compute SHA-256 of content, truncate to 16 hex chars
2. Write to `embedded/<cli-name>/<hash>.styx`
3. Return the cache file path (as `file://` URI)

### 3. Update LSP to use cache
In `crates/styx-lsp/src/schema_validation.rs`:
- Modify `extract_embedded_schema_source()` to cache
- Return `file://` URI pointing to cache instead of `styx-embedded://`

### 4. Resolution tracking (for crates)
`resolve.json` format:
```json
{
  "myapp-config": {
    "1": { "resolved": "1.2.3", "timestamp": "2025-01-19T12:00:00Z" }
  }
}
```

## Dependencies
- `dirs` crate for platform directories
- `sha2` crate for content hashing

## Notes
- Cache is append-only for embedded schemas (hash-addressed)
- Old entries can be pruned (not accessed in 30 days)
- Crate schemas are immutable once cached
