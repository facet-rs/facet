# Vix real-program corpus

These programs pressure the language with package-index ingestion, Cargo-like
unit construction, and Rodin solving. They are acceptance inputs, not a second
specification. If a program conflicts with the language or runtime spec, record
the conflict in the relevant specification/rung change and update the corpus
deliberately; do not add another gap or adjudication ledger here.

- `cargo_manifest.vix` decodes and models Cargo manifests.
- `crate.vix` constructs Rust compilation units and commands.
- `index.vix` ingests package-index rows.
- `rodin.vix` is the ported solver corpus.
- `package/` sketches the package-manifest seam (two packages, one seam).
- `libpng/` builds zlib and libpng from pinned source and encodes a PNG,
  pressuring the literal/template design against a real C build. Its README
  lists the four gaps that exercise found.

The production Rodin source remains under `rodin/`. This directory is retained
as the larger port/corpus oracle while the new compiler climbs the ratchet.
