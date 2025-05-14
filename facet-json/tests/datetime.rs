use facet::Facet;
use facet_json::{from_str, to_string};
use facet_testhelpers::test;
use jiff::{Timestamp, Zoned, civil::DateTime};
use time::OffsetDateTime;

#[test]
fn read_time_datetime() {
    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        created_at: OffsetDateTime,
    }

    use time::macros::datetime;

    let json = r#"{"created_at":"2023-01-15T12:34:56Z"}"#;

    let s: FooBar = from_str(json)?;
    assert_eq!(
        s,
        FooBar {
            created_at: datetime!(2023-01-15 12:34:56 UTC),
        }
    );
}

#[test]
#[cfg(not(miri))] // I don't think we can read time zones from miri, the test just fails
fn read_jiff_zoned() {
    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        created_at: Zoned,
    }

    let json = r#"{"created_at":"2023-12-31T18:30:00+07:00[Asia/Ho_Chi_Minh]"}"#;

    let s: FooBar = from_str(json)?;

    assert_eq!(
        s,
        FooBar {
            created_at: jiff::civil::date(2023, 12, 31)
                .at(18, 30, 0, 0)
                .in_tz("Asia/Ho_Chi_Minh")?
        }
    );
}

#[test]
fn read_jiff_timestamp() {
    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        created_at: Timestamp,
    }

    let json = r#"{"created_at":"2023-12-31T11:30:00Z"}"#;

    let s: FooBar = from_str(json)?;

    assert_eq!(
        s,
        FooBar {
            created_at: "2023-12-31T18:30:00+07:00[Asia/Ho_Chi_Minh]".parse()?,
        }
    );
}

#[test]
fn read_jiff_datetime() {
    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        created_at: DateTime,
    }

    let json = r#"{"created_at":"2024-06-19T15:22:45"}"#;

    let s: FooBar = from_str(json)?;

    assert_eq!(
        s,
        FooBar {
            created_at: "2024-06-19T15:22:45".parse()?,
        }
    );
}

#[test]
fn write_time_datetime() {
    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        created_at: OffsetDateTime,
    }

    use time::macros::datetime;

    let value = FooBar {
        created_at: datetime!(2023-01-15 12:34:56 UTC),
    };

    let json = to_string(&value);
    assert_eq!(json, r#"{"created_at":"2023-01-15T12:34:56Z"}"#);
}

#[test]
#[cfg(not(miri))] // I don't think we can read time zones from miri, the test just fails
fn write_jiff_zoned() {
    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        created_at: Zoned,
    }

    let value = FooBar {
        created_at: jiff::civil::date(2023, 12, 31)
            .at(18, 30, 0, 0)
            .in_tz("Asia/Ho_Chi_Minh")?,
    };

    let json = to_string(&value);
    assert_eq!(
        json,
        r#"{"created_at":"2023-12-31T18:30:00+07:00[Asia/Ho_Chi_Minh]"}"#
    );
}

#[test]
fn write_jiff_timestamp() {
    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        created_at: Timestamp,
    }

    let value = FooBar {
        created_at: "2023-12-31T18:30:00+07:00[Asia/Ho_Chi_Minh]".parse()?,
    };

    let json = to_string(&value);
    assert_eq!(json, r#"{"created_at":"2023-12-31T11:30:00Z"}"#);
}

#[test]
fn write_jiff_datetime() {
    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        created_at: DateTime,
    }

    let value = FooBar {
        created_at: "2024-06-19T15:22:45".parse()?,
    };

    let json = to_string(&value);
    assert_eq!(json, r#"{"created_at":"2024-06-19T15:22:45"}"#);
}
