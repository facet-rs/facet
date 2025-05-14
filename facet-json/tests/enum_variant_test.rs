use eyre::Result;
use facet::Facet;
use facet_testhelpers::setup;

/**
 * This test verifies that Facet can properly serialize and deserialize
 * different enum variants, focusing on those that are known to work.
 */

#[test]
fn enum_variants() -> Result<()> {
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

    // Struct variants
    #[derive(Debug, Facet, PartialEq)]
    #[repr(C)]
    #[allow(dead_code)]
    enum Message {
        Good { greeting: String, time: i32 },
        Bad { error: String, code: i32 },
    }

    // Test struct variant serialization
    let good = Message::Good {
        greeting: "Hello, sunshine!".to_string(),
        time: 800,
    };

    let json_good = facet_json::to_string(&good);
    assert_eq!(
        json_good,
        r#"{"Good":{"greeting":"Hello, sunshine!","time":800}}"#
    );

    // Test struct variant deserialization
    let deserialized_good: Message =
        facet_json::from_str(&json_good).map_err(|e| eyre::eyre!("{}", e))?;
    assert_eq!(deserialized_good, good);

    // Tuple variants
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    enum Point {
        X(u64),
        Y(String, bool),
    }

    // Test tuple variant serialization
    let x = Point::X(123);
    let json_x = facet_json::to_string(&x);
    assert_eq!(json_x, r#"{"X":123}"#);

    let y = Point::Y("hello".to_string(), true);
    let json_y = facet_json::to_string(&y);
    assert_eq!(json_y, r#"{"Y":["hello",true]}"#);

    // Test tuple variant deserialization
    let deserialized_x: Point = facet_json::from_str(&json_x).map_err(|e| eyre::eyre!("{}", e))?;
    assert_eq!(deserialized_x, x);

    let deserialized_y: Point = facet_json::from_str(&json_y).map_err(|e| eyre::eyre!("{}", e))?;
    assert_eq!(deserialized_y, y);

    Ok(())
}
