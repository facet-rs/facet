use facet::Facet;
use facet_json::{from_str, to_string};

#[derive(Debug, PartialEq, Facet, Default)]
#[repr(u8)]
enum Speed {
    #[default]
    Fast,
    Slow,
}

#[derive(Debug, PartialEq, Facet, Default)]
struct InnerData {
    speed: Option<Speed>,
    count: Option<u32>,
}

// Test struct with flatten - similar to PersistedPortConfig
#[derive(Debug, PartialEq, Facet)]
struct PortConfig {
    dev_port: u32,
    #[facet(flatten)]
    data: InnerData,
}

#[test]
fn test_option_enum_flatten_null_roundtrip() {
    let config = PortConfig {
        dev_port: 42,
        data: InnerData::default(), // speed: None, count: None
    };
    
    let json = to_string(&config).unwrap();
    println!("Serialized: {}", json);
    
    let deserialized: PortConfig = from_str(&json).unwrap();
    assert_eq!(config, deserialized);
}

#[test]
fn test_option_enum_flatten_with_value_roundtrip() {
    let config = PortConfig {
        dev_port: 42,
        data: InnerData {
            speed: Some(Speed::Slow),
            count: Some(100),
        },
    };
    
    let json = to_string(&config).unwrap();
    println!("Serialized: {}", json);
    
    let deserialized: PortConfig = from_str(&json).unwrap();
    assert_eq!(config, deserialized);
}
