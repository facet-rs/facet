// facet/tests/derive/raw_identifiers.rs
use facet::Facet;

#[derive(Facet, Debug, PartialEq, Clone)]
struct RawIdentifiers {
    r#type: String,
    r#enum: String,
    r#match: bool,
}

#[test]
fn test_derive_strips_raw_identifier_prefix() {
    let shape = RawIdentifiers::SHAPE;
    let def = shape.def().as_struct().expect("Should be a struct");
    let fields = def.fields();

    assert_eq!(fields.len(), 3);

    assert_eq!(fields[0].name(), "type");
    assert_eq!(fields[0].shape().def().as_str(), Some(())); // Check type is String

    assert_eq!(fields[1].name(), "enum");
    assert_eq!(fields[1].shape().def().as_str(), Some(())); // Check type is String

    assert_eq!(fields[2].name(), "match");
    assert_eq!(fields[2].shape().def().as_bool(), Some(())); // Check type is bool
}
