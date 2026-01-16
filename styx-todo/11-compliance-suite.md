# Compliance Suite

**Status:** TODO  
**Priority:** High

## Problem

As Styx grows (language bindings, alternative implementations), we need a way to verify parsers produce identical results. Currently `styx @tree` output is debug-oriented, not standardized.

## Goals

1. **Canonical tree format** — JSON or similar, machine-readable, versioned
2. **Test corpus** — Edge cases, tricky syntax, real-world examples
3. **Golden vectors** — Expected parse trees for each corpus file
4. **Test runner** — Compare implementation output against golden files

## Design

### Canonical Output Format

Replace debug `@tree` output with structured JSON:

```bash
styx @tree --format json file.styx
```

```json
{
  "version": "1.0",
  "root": {
    "type": "object",
    "entries": [
      {
        "key": {"type": "scalar", "text": "name", "kind": "bare"},
        "value": {"type": "scalar", "text": "hello", "kind": "bare"}
      }
    ]
  }
}
```

Key decisions:
- Include span information? (useful for LSP testing, but makes diffs noisy)
- Include raw vs interpreted scalar values?
- How to represent tags, sequences, attributes?

### Test Corpus Structure

```
compliance/
├── README.md
├── corpus/
│   ├── 00-basic/
│   │   ├── empty.styx
│   │   ├── single-entry.styx
│   │   ├── multiple-entries.styx
│   │   └── ...
│   ├── 01-scalars/
│   │   ├── bare.styx
│   │   ├── quoted.styx
│   │   ├── quoted-escapes.styx
│   │   ├── raw.styx
│   │   ├── raw-hashes.styx
│   │   ├── heredoc.styx
│   │   ├── heredoc-lang-hint.styx
│   │   └── ...
│   ├── 02-objects/
│   │   ├── empty.styx
│   │   ├── newline-sep.styx
│   │   ├── comma-sep.styx
│   │   ├── nested.styx
│   │   └── ...
│   ├── 03-sequences/
│   ├── 04-tags/
│   ├── 05-attributes/
│   ├── 06-comments/
│   ├── 07-edge-cases/
│   │   ├── unicode.styx
│   │   ├── deeply-nested.styx
│   │   ├── large-heredoc.styx
│   │   └── ...
│   └── 08-invalid/
│       ├── mixed-separators.styx
│       ├── unclosed-brace.styx
│       └── ...
├── golden/
│   ├── 00-basic/
│   │   ├── empty.json
│   │   ├── single-entry.json
│   │   └── ...
│   └── ...
└── runner/
    ├── run.sh              # Simple bash runner
    └── compliance.rs       # Rust test harness
```

### Invalid File Testing

For `08-invalid/`, golden files contain expected error info:

```json
{
  "valid": false,
  "errors": [
    {
      "code": "E0001",
      "message_contains": "mixed separators",
      "span": {"start": 10, "end": 15}
    }
  ]
}
```

### Runner Options

1. **Bash script** — Portable, calls `styx @tree --format json`, diffs against golden
2. **Rust harness** — Integrated into `cargo test`, snapshot testing with insta
3. **Both** — Rust for CI, bash for other implementations to use

## Implementation Plan

1. [ ] Define canonical JSON tree format (spec it out)
2. [ ] Add `styx @tree --format json` flag
3. [ ] Create initial corpus (~50 files covering basics)
4. [ ] Generate golden files from reference implementation
5. [ ] Add compliance runner to CI
6. [ ] Document how other implementations can run the suite
7. [ ] Expand corpus over time (fuzzing finds edge cases → add to corpus)

## Open Questions

- Should spans be optional/configurable? (`--include-spans`)
- How strict on whitespace/formatting in golden files?
- Versioning strategy when tree format changes?
- Should we test schema validation too, or just parsing?
