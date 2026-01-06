# TypeScript (new)

This directory is for the new TypeScript implementation and tooling.

Legacy TypeScript code lives in `typescript-legacy/`.

## Subject

The compliance suite runs a per-language **subject** (implementation under test) via `SUBJECT_CMD`.

For TypeScript, run the Node subject with Node v22 type stripping:

```bash
# Generate bindings used by the subject (METHOD_ID, etc.)
cargo xtask codegen --typescript

# Run the compliance suite against the TypeScript subject
SUBJECT_CMD='node --experimental-strip-types typescript/subject/subject.ts' \
  cargo nextest run -p spec-tests
```
