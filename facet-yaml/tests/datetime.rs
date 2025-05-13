#[test]
#[cfg(feature = "time")]
fn test_deserialize_datetime_utc() -> eyre::Result<()> {
    use facet::Facet;
    use facet_yaml::from_str;
    use time::OffsetDateTime;

    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    struct TimeObject {
        timestamp: OffsetDateTime,
    }

    use time::macros::datetime;

    // Test deserialization
    let yaml_input = "timestamp: 2023-04-15T14:30:25Z";
    let parsed: TimeObject = from_str(yaml_input)?;

    assert_eq!(
        parsed,
        TimeObject {
            timestamp: datetime!(2023-04-15 14:30:25 UTC),
        }
    );

    Ok(())
}

// Skip serialization test for now since serializer is not implemented
// We'll implement this when the serializer is ready
// #[test]
// #[cfg(feature = "time")]
// fn test_serialize_datetime() -> Result<()> {
//     facet_testhelpers::setup();
//
//     #[derive(Facet, Debug, PartialEq)]
//     struct TimeObject {
//         timestamp: OffsetDateTime,
//     }
//
//     use time::macros::datetime;
//
//     // Test serialization
//     let obj = TimeObject {
//         timestamp: datetime!(2023-04-15 14:30:25 UTC),
//     };
//
//     let yaml = to_string(&obj)?;
//     assert_eq!(yaml, "timestamp: 2023-04-15T14:30:25Z\n");
//
//     Ok(())
// }

#[test]
#[cfg(feature = "time")]
fn test_deserialize_datetime_with_offset() -> eyre::Result<()> {
    use facet::Facet;
    use facet_yaml::from_str;
    use time::OffsetDateTime;

    facet_testhelpers::setup();

    #[derive(Facet, Debug, PartialEq)]
    struct TimeObject {
        timestamp: OffsetDateTime,
    }

    use time::macros::datetime;

    // Test with timezone offset
    let yaml_input = "timestamp: 2023-04-15T14:30:25+02:00";
    let parsed: TimeObject = from_str(yaml_input)?;

    assert_eq!(
        parsed,
        TimeObject {
            timestamp: datetime!(2023-04-15 12:30:25 UTC),
        }
    );

    Ok(())
}
