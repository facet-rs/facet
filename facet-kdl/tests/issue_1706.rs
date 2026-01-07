use facet::Facet;

/// Issue #1706: facet-kdl unable to roundtrip a simple struct
/// https://github.com/facet-rs/facet/issues/1706
///
/// Simple structs should serialize and deserialize without requiring
/// explicit kdl::* attributes or wrapper structs.

#[derive(Debug, Facet, PartialEq)]
struct Config {
    host: String,
    port: u16,
}

#[test]
fn test_roundtrip() {
    let cfg = Config {
        host: String::from("https://kdl.dev"),
        port: 443,
    };

    let serialized = facet_kdl::to_string(&cfg).unwrap();
    eprintln!("Serialized:\n{}", serialized);

    // This should now work - no wrapper struct needed!
    let deserialized: Config = facet_kdl::from_str(&serialized).unwrap();
    assert_eq!(cfg, deserialized);
}
