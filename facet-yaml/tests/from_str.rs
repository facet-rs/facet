use facet::Facet;

#[derive(Debug, Facet, PartialEq)]
struct Person {
    name: String,
    age: u64,
}

#[test]
fn test_deserialize_person() {
    let yaml = r#"
            name: Alice
            age: 30
        "#;

    let person: Person = facet_yaml::from_str(yaml).expect("Failed to parse YAML");
    assert_eq!(
        person,
        Person {
            name: "Alice".to_string(),
            age: 30
        }
    );
}

#[cfg(feature = "ulid")]
#[test]
fn transparent_ulid() -> eyre::Result<()> {
    use ulid::Ulid;

    let yaml = r#"
        01F8MECHREJA5K66K6902DN5B3
    "#;

    // Test deserializing into Ulid
    let ulid: Ulid = facet_yaml::from_str(yaml)?;
    assert_eq!(ulid, Ulid::from_string("01F8MECHREJA5K66K6902DN5B3")?);

    // Test direct usage of Ulid
    let direct_ulid = Ulid::from_string("01BX5ZZKBKACTAV9WEVGEMMVS0")?;
    assert_eq!(direct_ulid.to_string(), "01BX5ZZKBKACTAV9WEVGEMMVS0");

    Ok(())
}
