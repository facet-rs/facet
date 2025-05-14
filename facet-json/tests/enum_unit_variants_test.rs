use eyre::Result;
use facet::Facet;
use facet_testhelpers::setup;

/**
 * This test verifies that Facet can properly serialize and deserialize
 * enum unit variants, which are known to work.
 */

#[test]
fn enum_unit_variants() -> Result<()> {
    setup();

    // Unit variants
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    #[allow(dead_code)]
    enum FontStyle {
        Italic,
        Oblique,
    }

    // Test unit variant serialization/deserialization
    let italic = FontStyle::Italic;
    let json_italic = facet_json::to_string(&italic);
    assert_eq!(json_italic, r#""Italic""#);

    let deserialized_italic: FontStyle =
        facet_json::from_str(&json_italic).map_err(|e| eyre::eyre!("{}", e))?;
    assert_eq!(deserialized_italic, italic);

    Ok(())
}

#[test]
fn enum_tuple_primitive_variants() -> Result<()> {
    setup();

    // Tuple variants with primitive types
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    enum Point {
        X(u64),
        Y(i32, bool),
    }

    // Test tuple variant with a primitive type
    let x = Point::X(123);
    let json_x = facet_json::to_string(&x);
    assert_eq!(json_x, r#"{"X":123}"#);

    let deserialized_x: Point = facet_json::from_str(&json_x).map_err(|e| eyre::eyre!("{}", e))?;
    assert_eq!(deserialized_x, x);

    // Test tuple variant with multiple primitive types
    let y = Point::Y(456, true);
    let json_y = facet_json::to_string(&y);
    assert_eq!(json_y, r#"{"Y":[456,true]}"#);

    let deserialized_y: Point = facet_json::from_str(&json_y).map_err(|e| eyre::eyre!("{}", e))?;
    assert_eq!(deserialized_y, y);

    Ok(())
}
