//! Regression test for issue #1189: untagged enum with tuple variant
//!
//! This tests that untagged enums with newtype variants wrapping tuples
//! can be deserialized from YAML sequences.
//!
//! See: https://github.com/facet-rs/facet/issues/1189

use facet::Facet;

#[derive(Facet, Clone, Debug, PartialEq)]
pub struct Spec {
    pub data: Vec<Cnt>,
}

#[derive(Facet, Clone, Debug, PartialEq)]
#[facet(untagged)]
#[repr(u8)]
pub enum Cnt {
    Unit(String),
    Weight((String, f64)),
}

/// Test the exact YAML format from issue #1189 (flow style)
#[test]
fn test_issue_1189_flow_style() {
    let yaml = r#"data: [[AGRICIBPAR, 1.0], [BARCLAYLDN, 1.0]]"#;

    let spec: Spec = facet_yaml::from_str(yaml).expect("should deserialize");
    assert_eq!(spec.data.len(), 2);
    assert_eq!(spec.data[0], Cnt::Weight(("AGRICIBPAR".to_string(), 1.0)));
    assert_eq!(spec.data[1], Cnt::Weight(("BARCLAYLDN".to_string(), 1.0)));
}

/// Test block style YAML
#[test]
fn test_issue_1189_block_style() {
    let yaml = r#"
data:
  - [AGRICIBPAR, 1.0]
  - [BARCLAYLDN, 1.0]
"#;

    let spec: Spec = facet_yaml::from_str(yaml).expect("should deserialize");
    assert_eq!(spec.data.len(), 2);
}

/// Test deserializing just the enum directly
#[test]
fn test_issue_1189_enum_directly() {
    let yaml = r#"[AGRICIBPAR, 1.0]"#;

    let cnt: Cnt = facet_yaml::from_str(yaml).expect("should deserialize");
    assert_eq!(cnt, Cnt::Weight(("AGRICIBPAR".to_string(), 1.0)));
}

/// Test with #[repr(C)] as in the original issue
#[derive(Facet, Clone, Debug, PartialEq)]
#[facet(untagged)]
#[repr(C)]
pub enum CntReprC {
    Unit(String),
    Weight((String, f64)),
}

#[derive(Facet, Clone, Debug, PartialEq)]
pub struct SpecReprC {
    pub data: Vec<CntReprC>,
}

#[test]
fn test_issue_1189_repr_c() {
    let yaml = r#"data: [[AGRICIBPAR, 1.0], [BARCLAYLDN, 1.0]]"#;

    let spec: SpecReprC = facet_yaml::from_str(yaml).expect("should deserialize");
    assert_eq!(spec.data.len(), 2);
}

/// Test that scalar variant (Unit) still works
#[test]
fn test_issue_1189_scalar_variant() {
    let yaml = r#"data: [foo, bar]"#;

    let spec: Spec = facet_yaml::from_str(yaml).expect("should deserialize");
    assert_eq!(spec.data.len(), 2);
    assert_eq!(spec.data[0], Cnt::Unit("foo".to_string()));
    assert_eq!(spec.data[1], Cnt::Unit("bar".to_string()));
}

/// Test mixing scalar and tuple variants
#[test]
fn test_issue_1189_mixed_variants() {
    let yaml = r#"
data:
  - foo
  - [bar, 2.5]
  - baz
"#;

    let spec: Spec = facet_yaml::from_str(yaml).expect("should deserialize");
    assert_eq!(spec.data.len(), 3);
    assert_eq!(spec.data[0], Cnt::Unit("foo".to_string()));
    assert_eq!(spec.data[1], Cnt::Weight(("bar".to_string(), 2.5)));
    assert_eq!(spec.data[2], Cnt::Unit("baz".to_string()));
}
