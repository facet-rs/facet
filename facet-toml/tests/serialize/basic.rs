use eyre::Result;
use facet::Facet;
use facet_serialize::Serialize;

#[derive(Debug, Facet, PartialEq)]
struct Person {
    name: String,
    age: u64,
}

#[cfg(feature = "alloc")]
#[test]
fn test_serialize_person() -> Result<()> {
    use facet_toml::TomlSerializer;

    facet_testhelpers::setup();

    let person = Person {
        name: "Alice".to_string(),
        age: 30,
    };

    let mut toml = String::new();
    let mut serializer = TomlSerializer::new(&mut toml);
    person.serialize(&mut serializer)?;

    assert_eq!(toml, "name = \"Alice\"\nage = 30\n");

    Ok(())
}
