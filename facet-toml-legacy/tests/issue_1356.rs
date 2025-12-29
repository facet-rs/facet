// SPDX-FileCopyrightText: 2024 Facet Maintainers
// SPDX-License-Identifier: MIT OR Apache-2.0

//! Test for issue #1356: facet-toml should handle multiline strings in nested tables

use facet::Facet;

#[derive(Debug, Facet)]
struct Config {
    #[facet(rename = "mediaType")]
    media_type: String,
    datasets: Vec<Dataset>,
}

#[derive(Debug, Facet)]
struct Dataset {
    name: String,
    #[facet(rename = "ds-type")]
    ds_type: String,
    #[facet(rename = "text-lines")]
    text_lines: Vec<String>,
    #[facet(default)]
    tests: Option<Tests>,
    #[facet(default)]
    second: Option<Second>,
    #[facet(default)]
    features: Option<Features>,
}

#[derive(Debug, Facet)]
struct Tests {
    #[facet(rename = "bind-dump-zonefile", default)]
    bind_dump_zonefile: Option<BindDumpZonefile>,
    #[facet(default)]
    queries: Vec<Query>,
}

#[derive(Debug, Facet)]
struct BindDumpZonefile {
    lines: Vec<String>,
}

#[derive(Debug, Facet)]
struct Query {
    qtype: String,
    qname: String,
    #[facet(default)]
    rcode: Option<String>,
    #[facet(default)]
    data: Option<Vec<QueryData>>,
}

#[derive(Debug, Facet)]
struct QueryData {
    rtype: String,
    data: String,
    #[facet(default)]
    serial: Option<u64>,
}

#[derive(Debug, Facet)]
struct Second {
    name: String,
    #[facet(rename = "ds-type")]
    ds_type: String,
}

#[derive(Debug, Facet)]
struct Features {
    #[facet(rename = "rec-generic-caa")]
    rec_generic_caa: String,
}

#[test]
fn test_issue_1356_full_file() {
    let toml_content = include_str!("issue_1356_defs.toml");

    let result: Result<Config, _> = facet_toml_legacy::from_str(toml_content);

    match &result {
        Ok(_config) => {
            // Success!
        }
        Err(e) => {
            eprintln!("Failed to parse TOML: {}", e);
        }
    }

    assert!(
        result.is_ok(),
        "Should successfully parse the full defs.toml file"
    );
}

#[test]
fn test_issue_1356_minimal_multiline() {
    // Minimal test case with multiline strings in nested table
    let toml = r#"
mediaType = "test"

[[datasets]]
name = "minimal"
ds-type = "ip4tset"
text-lines = [
  "192.168.13.0",
]

[datasets.tests.bind-dump-zonefile]
lines = [
  "$ORIGIN test-rbldnsd.example.com.",
  "$TTL 2100",
  "0.13.168.192 A 127.0.0.2",
]
"#;

    let result: Result<Config, _> = facet_toml_legacy::from_str(toml);
    assert!(
        result.is_ok(),
        "Should parse nested table with string arrays: {result:?}"
    );
}

#[test]
fn test_issue_1356_multiline_with_escapes() {
    // Test with escaped quotes in multiline strings
    let toml = r#"
mediaType = "test"

[[datasets]]
name = "address-and-text"
ds-type = "ip4tset"
text-lines = [
  ":255.255.255.252:127.0.3.1:This is:\"something:\"and stuff",
  "192.168.13.0",
]

[datasets.tests.bind-dump-zonefile]
lines = [
  "$ORIGIN test-rbldnsd.example.com.",
  "$TTL 2100",
  "0.13.168.192 A 255.255.255.252",
  " TXT \"127.0.3.1:This is:\\\"something:\\\"and stuff\"",
]
"#;

    let result: Result<Config, _> = facet_toml_legacy::from_str(toml);

    if let Ok(config) = &result {
        assert_eq!(
            config.datasets[0].text_lines[0],
            ":255.255.255.252:127.0.3.1:This is:\"something:\"and stuff"
        );
        assert_eq!(
            config.datasets[0]
                .tests
                .as_ref()
                .unwrap()
                .bind_dump_zonefile
                .as_ref()
                .unwrap()
                .lines[3],
            " TXT \"127.0.3.1:This is:\\\"something:\\\"and stuff\""
        );
    }

    assert!(
        result.is_ok(),
        "Should parse escaped quotes in strings: {result:?}"
    );
}
