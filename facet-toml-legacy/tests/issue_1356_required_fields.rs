// Regression tests for issue #1356: nested table headers with REQUIRED fields
//
// The existing issue_1356.rs uses Option<T> and #[facet(default)] as a workaround.
// This test file uses required fields (no Option, no default) which matches
// Peter's actual code and fails.
//
// The bug: When parsing `[datasets.tests.bind-dump-zonefile]` after `[[datasets]]`,
// we enter the `tests` struct to set `bind_dump_zonefile`, but when we later
// pop frames, the `tests` field on `TestDataset` appears uninitialized because
// the parent tracker never reclaimed it (child's require_full_initialization failed).

use facet::Facet;

#[derive(Debug, Facet)]
pub struct TestBindDumpZonefile {
    pub lines: Vec<String>,
}

#[derive(Debug, Facet)]
pub struct TestQuery {
    pub qtype: String,
    pub qname: String,
}

#[derive(Debug, Facet)]
#[facet(rename_all = "kebab-case")]
pub struct TestTests {
    pub bind_dump_zonefile: TestBindDumpZonefile,
    pub queries: Vec<TestQuery>, // No default! This is required.
}

#[derive(Debug, Facet)]
#[facet(rename_all = "kebab-case")]
pub struct TestDataset {
    pub name: String,
    pub ds_type: String,
    pub text_lines: Vec<String>,
    pub tests: TestTests, // No default! This is required.
}

#[derive(Debug, Facet)]
pub struct TestDefs {
    #[facet(rename = "mediaType")]
    pub media_type: String,
    pub datasets: Vec<TestDataset>,
}

/// Minimal case: table header then array-of-tables for nested struct fields
#[test]
fn test_table_then_array_of_tables() {
    let toml = r#"
mediaType = "test"

[[datasets]]
name = "minimal"
ds-type = "ip4tset"
text-lines = ["192.168.13.0"]

[datasets.tests.bind-dump-zonefile]
lines = ["line1", "line2"]

[[datasets.tests.queries]]
qtype = "SOA"
qname = "@"
"#;
    let result: Result<TestDefs, _> = facet_toml_legacy::from_str(toml);
    assert!(result.is_ok(), "Should parse: {}", result.unwrap_err());

    let defs = result.unwrap();
    assert_eq!(defs.datasets.len(), 1);
    assert_eq!(defs.datasets[0].tests.bind_dump_zonefile.lines.len(), 2);
    assert_eq!(defs.datasets[0].tests.queries.len(), 1);
}

/// Two datasets - the second one triggers the bug when we close the first
#[test]
fn test_two_datasets() {
    let toml = r#"
mediaType = "test"

[[datasets]]
name = "first"
ds-type = "ip4tset"
text-lines = ["192.168.13.0"]

[datasets.tests.bind-dump-zonefile]
lines = ["line1"]

[[datasets.tests.queries]]
qtype = "SOA"
qname = "@"

[[datasets]]
name = "second"
ds-type = "ip4tset"
text-lines = ["10.0.0.1"]

[datasets.tests.bind-dump-zonefile]
lines = ["line2"]

[[datasets.tests.queries]]
qtype = "A"
qname = "test"
"#;
    let result: Result<TestDefs, _> = facet_toml_legacy::from_str(toml);
    assert!(result.is_ok(), "Should parse: {}", result.unwrap_err());

    let defs = result.unwrap();
    assert_eq!(defs.datasets.len(), 2);
}

/// Array-of-tables first, then table header
#[test]
fn test_array_then_table() {
    let toml = r#"
mediaType = "test"

[[datasets]]
name = "minimal"
ds-type = "ip4tset"
text-lines = ["192.168.13.0"]

[[datasets.tests.queries]]
qtype = "SOA"
qname = "@"

[datasets.tests.bind-dump-zonefile]
lines = ["line1", "line2"]
"#;
    let result: Result<TestDefs, _> = facet_toml_legacy::from_str(toml);
    assert!(result.is_ok(), "Should parse: {}", result.unwrap_err());
}
