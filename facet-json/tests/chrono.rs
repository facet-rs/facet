use chrono::{DateTime, FixedOffset, NaiveDate, NaiveDateTime, NaiveTime, TimeZone, Utc};
use facet::Facet;
use facet_json::{from_str, to_string};
use facet_testhelpers::test;

#[test]
fn read_chrono_datetime_utc() {
    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        created_at: DateTime<Utc>,
    }

    let json = r#"{"created_at":"2023-01-15T12:34:56Z"}"#;

    let s: FooBar = from_str(json)?;
    assert_eq!(
        s,
        FooBar {
            created_at: Utc.with_ymd_and_hms(2023, 1, 15, 12, 34, 56).unwrap(),
        }
    );
}

#[test]
fn read_chrono_datetime_fixed_offset() {
    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        created_at: DateTime<FixedOffset>,
    }

    let json = r#"{"created_at":"2023-01-15T12:34:56+07:00"}"#;

    let s: FooBar = from_str(json)?;
    let expected_offset = FixedOffset::east_opt(7 * 3600).unwrap();
    assert_eq!(
        s,
        FooBar {
            created_at: expected_offset
                .with_ymd_and_hms(2023, 1, 15, 12, 34, 56)
                .unwrap(),
        }
    );
}

#[test]
fn read_chrono_naive_datetime() {
    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        created_at: NaiveDateTime,
    }

    let json = r#"{"created_at":"2023-01-15T12:34:56"}"#;

    let s: FooBar = from_str(json)?;
    assert_eq!(
        s,
        FooBar {
            created_at: NaiveDate::from_ymd_opt(2023, 1, 15)
                .unwrap()
                .and_hms_opt(12, 34, 56)
                .unwrap(),
        }
    );
}

#[test]
fn read_chrono_naive_date() {
    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        birth_date: NaiveDate,
    }

    let json = r#"{"birth_date":"2023-01-15"}"#;

    let s: FooBar = from_str(json)?;
    assert_eq!(
        s,
        FooBar {
            birth_date: NaiveDate::from_ymd_opt(2023, 1, 15).unwrap(),
        }
    );
}

#[test]
fn read_chrono_naive_time() {
    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        alarm_time: NaiveTime,
    }

    let json = r#"{"alarm_time":"12:34:56"}"#;

    let s: FooBar = from_str(json)?;
    assert_eq!(
        s,
        FooBar {
            alarm_time: NaiveTime::from_hms_opt(12, 34, 56).unwrap(),
        }
    );
}

#[test]
fn write_chrono_datetime_utc() {
    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        created_at: DateTime<Utc>,
    }

    let value = FooBar {
        created_at: Utc.with_ymd_and_hms(2023, 1, 15, 12, 34, 56).unwrap(),
    };

    let json = to_string(&value);
    assert_eq!(json, r#"{"created_at":"2023-01-15T12:34:56Z"}"#);
}

#[test]
fn write_chrono_datetime_fixed_offset() {
    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        created_at: DateTime<FixedOffset>,
    }

    let offset = FixedOffset::east_opt(7 * 3600).unwrap();
    let value = FooBar {
        created_at: offset.with_ymd_and_hms(2023, 1, 15, 12, 34, 56).unwrap(),
    };

    let json = to_string(&value);
    assert_eq!(json, r#"{"created_at":"2023-01-15T12:34:56+07:00"}"#);
}

#[test]
fn write_chrono_naive_datetime() {
    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        created_at: NaiveDateTime,
    }

    let value = FooBar {
        created_at: NaiveDate::from_ymd_opt(2023, 1, 15)
            .unwrap()
            .and_hms_opt(12, 34, 56)
            .unwrap(),
    };

    let json = to_string(&value);
    assert_eq!(json, r#"{"created_at":"2023-01-15T12:34:56"}"#);
}

#[test]
fn write_chrono_naive_date() {
    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        birth_date: NaiveDate,
    }

    let value = FooBar {
        birth_date: NaiveDate::from_ymd_opt(2023, 1, 15).unwrap(),
    };

    let json = to_string(&value);
    assert_eq!(json, r#"{"birth_date":"2023-01-15"}"#);
}

#[test]
fn write_chrono_naive_time() {
    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        alarm_time: NaiveTime,
    }

    let value = FooBar {
        alarm_time: NaiveTime::from_hms_opt(12, 34, 56).unwrap(),
    };

    let json = to_string(&value);
    assert_eq!(json, r#"{"alarm_time":"12:34:56"}"#);
}

#[test]
fn read_chrono_optional_datetime() {
    #[derive(Facet, Debug, PartialEq)]
    struct FooBar {
        created_at: Option<DateTime<Utc>>,
        updated_at: Option<DateTime<Utc>>,
    }

    let json = r#"{"created_at":"2023-01-15T12:34:56Z","updated_at":null}"#;

    let s: FooBar = from_str(json)?;
    assert_eq!(
        s,
        FooBar {
            created_at: Some(Utc.with_ymd_and_hms(2023, 1, 15, 12, 34, 56).unwrap()),
            updated_at: None,
        }
    );
}

#[test]
fn chrono_in_vec() {
    #[derive(Facet, Debug, PartialEq)]
    struct Events {
        timestamps: Vec<DateTime<Utc>>,
    }

    let json = r#"{"timestamps":["2023-01-15T12:34:56Z","2023-02-20T10:00:00Z"]}"#;

    let events: Events = from_str(json)?;
    assert_eq!(events.timestamps.len(), 2);
    assert_eq!(
        events.timestamps[0],
        Utc.with_ymd_and_hms(2023, 1, 15, 12, 34, 56).unwrap()
    );
    assert_eq!(
        events.timestamps[1],
        Utc.with_ymd_and_hms(2023, 2, 20, 10, 0, 0).unwrap()
    );

    let serialized = to_string(&events);
    assert_eq!(serialized, json);
}
