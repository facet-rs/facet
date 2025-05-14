use eyre::Result;
use facet::Facet;
use facet_testhelpers::setup;

/**
 * This test verifies that Facet can properly serialize and deserialize
 * enum struct variants.
 */

#[test]
fn enum_struct_variants() -> Result<()> {
    setup();

    // Struct variants
    #[derive(Debug, Facet, PartialEq)]
    #[repr(C)]
    #[allow(dead_code)]
    enum Message {
        Good { time: i32 },
        Bad { code: i32 },
    }

    // Test struct variant with primitive fields (no strings)
    let good = Message::Good { time: 800 };

    let json_good = facet_json::to_string(&good);
    assert_eq!(json_good, r#"{"Good":{"time":800}}"#);

    // Test struct variant deserialization
    let deserialized_good: Message =
        facet_json::from_str(&json_good).map_err(|e| eyre::eyre!("{}", e))?;
    assert_eq!(deserialized_good, good);

    Ok(())
}
