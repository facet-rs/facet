#![cfg(feature = "time")]

use facet_testhelpers::test;

#[test]
fn test_deserialize_datetime_utc() {
    use facet::Facet;
    use facet_yaml::from_str;
    use time::OffsetDateTime;

    #[derive(Facet, Debug, PartialEq)]
    struct TimeObject {
        timestamp: OffsetDateTime,
    }

    use time::macros::datetime;

    // Test deserialization
    let yaml_input = "timestamp: 2023-04-15T14:30:25Z";
    let parsed: TimeObject = from_str(yaml_input).unwrap();

    assert_eq!(
        parsed,
        TimeObject {
            timestamp: datetime!(2023-04-15 14:30:25 UTC),
        }
    );
}

#[test]
#[cfg(feature = "time")]
fn test_deserialize_datetime_with_offset() {
    use facet::Facet;
    use facet_yaml::from_str;
    use time::OffsetDateTime;

    #[derive(Facet, Debug, PartialEq)]
    struct TimeObject {
        timestamp: OffsetDateTime,
    }

    use time::macros::datetime;

    // Test with timezone offset
    let yaml_input = "timestamp: 2023-04-15T14:30:25+02:00";
    let parsed: TimeObject = from_str(yaml_input).unwrap();

    assert_eq!(
        parsed,
        TimeObject {
            timestamp: datetime!(2023-04-15 12:30:25 UTC),
        }
    );
}
