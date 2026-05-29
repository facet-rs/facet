//! Generates the cross-language conformance corpus under `conformance/cases/`.
//!
//! Run with `cargo run -p phon-conformance`. It clears and rewrites the corpus,
//! so a clean working tree afterward means nothing drifted. See
//! `conformance/README.md` for the format and the oracle workflow.

use std::error::Error;
use std::fs;

use phon_conformance::{cases, cases_dir, resolve_case};
use phon_schema::schema_to_bytes;

fn main() -> Result<(), Box<dyn Error>> {
    tracing_subscriber::fmt::init();

    let dir = cases_dir();
    fs::create_dir_all(&dir)?;

    // Clear stale case directories so renamed/removed cases don't linger.
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            fs::remove_dir_all(entry.path())?;
        }
    }

    let mut schema_count = 0usize;
    let all = cases();
    for case in &all {
        let case_dir = dir.join(&case.name);
        fs::create_dir_all(&case_dir)?;
        for ls in resolve_case(case) {
            let bytes = schema_to_bytes(&ls.schema);
            fs::write(case_dir.join(format!("{}.phon", ls.label)), &bytes)?;
            schema_count += 1;
            tracing::debug!(
                case = %case.name,
                label = %ls.label,
                id = %ls.schema.id,
                bytes = bytes.len(),
                "wrote schema"
            );
        }
    }

    tracing::info!(
        cases = all.len(),
        schemas = schema_count,
        dir = %dir.display(),
        "wrote conformance corpus"
    );
    Ok(())
}
