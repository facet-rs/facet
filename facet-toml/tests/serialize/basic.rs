use eyre::Result;
use facet::Facet;

#[derive(Debug, Facet, PartialEq)]
struct Person {
    name: String,
    age: u64,
}

#[cfg(feature = "alloc")]
#[test]
fn test_serialize_person() -> Result<()> {
    facet_testhelpers::setup();

    let person = Person {
        name: "Alice".to_string(),
        age: 30,
    };

    let toml = facet_toml::to_string(&person)?;

    assert_eq!(toml, "name = \"Alice\"\nage = 30\n");

    Ok(())
}
