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

    let yaml = facet_yaml::to_string(&person)?;

    assert_eq!(yaml, "---\nname: Alice\nage: 30");

    Ok(())
}
