# Compliance Suite

**Status:** TODO  
**Priority:** High

## Problem

As Styx grows (language bindings, alternative implementations), we need a way to verify parsers produce identical results.

## Goals

1. **Canonical tree format** — Styx itself (dogfooding), compact, versioned
2. **Test corpus** — Edge cases, tricky syntax, real-world examples
3. **Golden vectors** — Expected parse trees for each corpus file
4. **Test runner** — Python script that parses all files and outputs all trees in one invocation (avoids interpreter warmup)

## Design

### Canonical Output Format

The tree format is... Styx! Version `2026-01-16`.

```bash
styx @tree --format styx file.styx
```

```styx
@ compliance/tree.schema.styx
version 2026-01-16

root @object{
    entries (
        @entry{
            key @scalar{text name, kind bare, span (0 4)}
            value @scalar{text hello, kind bare, span (5 10)}
        }
    )
    span (0 10)
}
```

Spans are always included (start, end byte offsets).

### Test Corpus Structure

```
compliance/
├── README.md
├── tree.schema.styx          # Schema for tree output format
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
│   │   ├── empty.tree.styx
│   │   ├── single-entry.tree.styx
│   │   └── ...
│   └── ...
└── runner.py                 # Single Python script, parses all, outputs all
```

### Invalid File Testing

For `08-invalid/`, golden files contain expected error info:

```styx
@ compliance/error.schema.styx
version 2026-01-16

valid @false
errors (
    @error{
        code E0001
        message_contains "mixed separators"
        span (10 15)
    }
)
```

### Runner

Single Python script to avoid interpreter warmup:

```bash
python compliance/runner.py compliance/corpus/ > output.styx
# Compare against concatenated golden files
```

Or for a specific implementation:

```bash
python compliance/runner.py --impl ./my-styx-parser compliance/corpus/
```

## Implementation Plan

1. [ ] Define tree schema (`compliance/tree.schema.styx`)
2. [ ] Add `styx @tree --format styx` flag  
3. [ ] Create initial corpus (~50 files covering basics)
4. [ ] Generate golden files from reference implementation
5. [ ] Write `runner.py`
6. [ ] Add to CI
7. [ ] Document how other implementations can run the suite
8. [ ] Expand corpus over time (fuzzing finds edge cases → add to corpus)

## Future

- Schema validation compliance testing (separate suite)
