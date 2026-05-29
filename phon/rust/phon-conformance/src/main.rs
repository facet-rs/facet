//! Generates the cross-language conformance corpus under `conformance/cases/`.
//!
//! Run with `cargo run -p phon-conformance`. It overwrites the corpus, so a
//! clean working tree afterward means nothing drifted. See
//! `conformance/README.md` for the corpus format and the oracle workflow.

fn main() {
    let cases = phon_conformance::cases_dir();
    // TODO: build the canonical schema set, compute each SchemaId, encode each
    // sample value in self-describing and compact modes, and write the case
    // files plus a facet-json manifest under `cases`. Blocked on the phon
    // encode/identity implementation landing.
    let _ = cases;
}
