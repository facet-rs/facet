//! Test that container-level extension attributes work alongside derive plugins.
//!
//! Bug repro: when #[facet(derive(X))] is present, container-level extension
//! attributes from other #[facet(...)] annotations are being dropped.

use facet::Facet;
use facet_default as _;
use facet_testattrs as testattrs;

// WITHOUT derive: extension attribute IS captured
#[derive(Facet)]
#[facet(testattrs::positional)]
struct WithoutDerive {
    x: i32,
}

// WITH derive: extension attribute is DROPPED (BUG)
#[derive(Facet)]
#[facet(derive(Default))]
#[facet(testattrs::positional)]
struct WithDerive {
    x: i32,
}

#[test]
fn test_extension_attr_without_derive() {
    let shape = WithoutDerive::SHAPE;
    let attr = shape
        .attributes
        .iter()
        .find(|a| a.ns == Some("testattrs") && a.key == "positional");
    assert!(
        attr.is_some(),
        "Extension attribute should be present without derive"
    );
}

#[test]
fn test_extension_attr_with_derive() {
    let shape = WithDerive::SHAPE;
    let attr = shape
        .attributes
        .iter()
        .find(|a| a.ns == Some("testattrs") && a.key == "positional");
    // BUG: This currently fails because the attribute is dropped when derive is present
    assert!(
        attr.is_some(),
        "Extension attribute should be present WITH derive (currently broken)"
    );
}
