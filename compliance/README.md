# Styx Compliance Suite

Test corpus and golden vectors for verifying Styx parser implementations.

## Structure

```
compliance/
├── README.md           # This file
├── format.md           # S-expression output format specification
├── golden.sexp         # Expected output for all corpus files
└── corpus/
    ├── 00-basic/       # Basic parsing
    ├── 01-scalars/     # Scalar types (bare, quoted, raw, heredoc)
    ├── 02-objects/     # Object syntax
    ├── 03-sequences/   # Sequence syntax
    ├── 04-tags/        # Tagged values
    ├── 05-comments/    # Comments
    ├── 06-edge-cases/  # Unicode, whitespace, edge cases
    └── 07-invalid/     # Files that should fail to parse
```

## Running

Each implementation provides its own compliance runner. The runner:
1. Parses all `.styx` files in `corpus/` (sorted alphabetically)
2. Outputs S-expression trees per `format.md`
3. Compares against `golden.sexp`

### Rust (reference implementation)

```bash
# Generate output
find compliance/corpus -name "*.styx" | sort | while read f; do
  styx @tree --format sexp "$f"
done > output.sexp

# Compare
diff -u compliance/golden.sexp output.sexp
```

### Other implementations

Your parser should produce identical output to `golden.sexp`. See `format.md` for the exact S-expression format.

## Regenerating golden.sexp

If you change the reference implementation:

```bash
cd /path/to/styx
find compliance/corpus -name "*.styx" | sort | while read f; do
  cargo run --quiet --bin styx -- @tree --format sexp "$f"
done > compliance/golden.sexp
```

## Format Version

The current format version is `2026-01-16`. See `format.md` for details.
