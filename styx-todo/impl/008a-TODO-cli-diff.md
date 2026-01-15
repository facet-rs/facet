# Phase 008a: styx @diff (Structural Diff)

Schema-aware structural diff between two styx documents.

## Usage

```bash
styx @diff old.styx new.styx
styx @diff old.styx new.styx --schema app.schema  # schema-aware diff
```

## Features

- Show added/removed/changed keys
- Ignore formatting differences (semantic diff)
- Schema-aware: understand type semantics
- Path-based output: `server.host: "localhost" → "0.0.0.0"`

## Output Format

```
server.host: "localhost" → "0.0.0.0"
server.port: (added) 8080
debug: (removed) true
```

## Dependencies

- styx-tree (parsing)
- styx-schema (optional, for schema-aware diff)

## Open Questions

- Color output?
- JSON output format for tooling?
- Exit code: 0 = same, 1 = different, 2 = error?
