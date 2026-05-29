//! Cross-language conformance corpus: shared definitions for the generator and
//! the loaders.
//!
//! Rust is the source of truth (see `conformance/README.md`). This crate's
//! binary writes the corpus; its tests load it back and verify Rust round-trips
//! every case. Swift and TypeScript load the same `cases/` directory and check
//! that they encode identical bytes and compute identical `SchemaId`s.
//!
//! Spec: `docs/content/spec.md` — "Schema identity" is the linchpin this corpus
//! protects.

use std::path::{Path, PathBuf};

/// The committed corpus directory, relative to the repository root.
pub const CASES_DIR: &str = "conformance/cases";

/// Resolve the corpus directory from this crate's location
/// (`<repo>/rust/phon-conformance` -> `<repo>/conformance/cases`).
pub fn cases_dir() -> PathBuf {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(2)
        .expect("crate is two levels under the repo root");
    repo_root.join(CASES_DIR)
}
