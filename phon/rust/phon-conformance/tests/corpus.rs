//! The Rust side of the conformance oracle.
//!
//! Reads the committed corpus and checks, for every case:
//!  - **golden**: the bytes the current code generates equal the committed bytes
//!    (so changing the codec or a case without regenerating fails here);
//!  - **round-trip**: each committed file decodes to the expected schema and
//!    re-encodes to the same bytes;
//!  - **identity**: recomputing each schema's id from the decoded batch (its own
//!    identity hash) reproduces the id baked into the bytes — the exact check the
//!    Swift and TypeScript loaders will run against the same files.

use std::fs;

use phon_conformance::{cases, cases_dir, resolve_case};
use phon_schema::{Schema, resolve_ids, schema_from_bytes, schema_to_bytes};

#[test]
fn corpus_is_golden_and_self_consistent() {
    let mut checked = 0usize;
    for case in cases() {
        let case_dir = cases_dir().join(&case.name);
        let resolved = resolve_case(&case);

        let mut decoded_batch: Vec<Schema> = Vec::new();
        for ls in &resolved {
            let path = case_dir.join(format!("{}.phon", ls.label));
            let committed = fs::read(&path).unwrap_or_else(|e| {
                panic!(
                    "missing corpus file {}: {e}; run `cargo run -p phon-conformance`",
                    path.display()
                )
            });

            // golden: the current code reproduces the committed bytes exactly.
            assert_eq!(
                schema_to_bytes(&ls.schema),
                committed,
                "{}/{}: bytes drifted — regenerate the corpus",
                case.name,
                ls.label
            );

            // round-trip: committed bytes decode to the expected schema and back.
            let decoded = schema_from_bytes(&committed)
                .unwrap_or_else(|e| panic!("{}/{}: decode failed: {e}", case.name, ls.label));
            assert_eq!(
                decoded, ls.schema,
                "{}/{}: decoded schema differs",
                case.name, ls.label
            );
            assert_eq!(
                schema_to_bytes(&decoded),
                committed,
                "{}/{}: re-encode differs from committed bytes",
                case.name,
                ls.label
            );

            decoded_batch.push(decoded);
        }

        // identity: recompute every id from the decoded batch and confirm it
        // matches the id baked into the bytes (what other languages must match).
        let recomputed = resolve_ids(decoded_batch.clone());
        for (decoded, recomputed) in decoded_batch.iter().zip(&recomputed) {
            assert_eq!(
                decoded.id, recomputed.id,
                "{}: recomputed SchemaId differs from the stated id",
                case.name
            );
            checked += 1;
        }
    }
    assert!(checked > 0, "corpus is empty");
}
