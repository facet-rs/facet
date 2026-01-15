//! Parser snapshot tests using datatest-stable and insta.
//!
//! Each `.rs` file in `tests/fixtures/` is parsed and the AST is snapshot-tested.

use roam_macros_parse::{ServiceTrait, parse_trait};
use std::path::Path;

fn test_parse_fixture(path: &Path) -> datatest_stable::Result<()> {
    let content = std::fs::read_to_string(path)?;
    // Normalize CRLF to LF for consistent byte spans across platforms
    let content = content.replace("\r\n", "\n");
    let tokens: proc_macro2::TokenStream = content
        .parse()
        .map_err(|e| format!("failed to tokenize {}: {}", path.display(), e))?;

    let parsed: ServiceTrait =
        parse_trait(&tokens).map_err(|e| format!("failed to parse {}: {}", path.display(), e))?;

    // Use the file stem as the snapshot name
    let name = path.file_stem().unwrap().to_str().unwrap();

    insta::with_settings!({
        description => &content,
        omit_expression => true,
        snapshot_path => path.parent().unwrap().join("snapshots"),
    }, {
        insta::assert_debug_snapshot!(name, parsed);
    });

    Ok(())
}

datatest_stable::harness! {
    { test = test_parse_fixture, root = "tests/fixtures", pattern = r"\.rs$" },
}
