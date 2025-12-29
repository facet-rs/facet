#![cfg(feature = "jiff02")]

use facet::Facet;
use facet_postcard_legacy::{from_slice, to_vec};
use jiff::{Timestamp, Zoned, civil::DateTime};

#[test]
fn test_zoned_roundtrip() {
    facet_testhelpers::setup();

    let zoned = Zoned::now();
    let bytes = to_vec(&zoned).unwrap();
    let decoded: Zoned = from_slice(&bytes).unwrap();
    assert_eq!(zoned, decoded);
}

#[test]
fn test_zoned_specific_timezone() {
    facet_testhelpers::setup();

    // Parse a specific timestamp with timezone
    let zoned: Zoned = "2024-12-23T15:30:00-05:00[America/New_York]"
        .parse()
        .unwrap();
    let bytes = to_vec(&zoned).unwrap();
    let decoded: Zoned = from_slice(&bytes).unwrap();
    assert_eq!(zoned, decoded);
}

#[test]
fn test_zoned_in_struct() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct Event {
        timestamp: Zoned,
        name: String,
    }

    let original = Event {
        timestamp: Zoned::now(),
        name: "test event".to_string(),
    };

    let bytes = to_vec(&original).unwrap();
    let decoded: Event = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_zoned_in_option() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct WithOptionalTime {
        time: Option<Zoned>,
    }

    // Test Some variant
    let original = WithOptionalTime {
        time: Some(Zoned::now()),
    };
    let bytes = to_vec(&original).unwrap();
    let decoded: WithOptionalTime = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);

    // Test None variant
    let original = WithOptionalTime { time: None };
    let bytes = to_vec(&original).unwrap();
    let decoded: WithOptionalTime = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_timestamp_roundtrip() {
    facet_testhelpers::setup();

    let ts = Timestamp::now();
    let bytes = to_vec(&ts).unwrap();
    let decoded: Timestamp = from_slice(&bytes).unwrap();
    assert_eq!(ts, decoded);
}

#[test]
fn test_timestamp_unix_epoch() {
    facet_testhelpers::setup();

    let ts = Timestamp::UNIX_EPOCH;
    let bytes = to_vec(&ts).unwrap();
    let decoded: Timestamp = from_slice(&bytes).unwrap();
    assert_eq!(ts, decoded);
}

#[test]
fn test_timestamp_in_struct() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct LogEntry {
        timestamp: Timestamp,
        message: String,
    }

    let original = LogEntry {
        timestamp: Timestamp::now(),
        message: "log message".to_string(),
    };

    let bytes = to_vec(&original).unwrap();
    let decoded: LogEntry = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_datetime_roundtrip() {
    facet_testhelpers::setup();

    let dt: DateTime = "2024-12-23T15:30:00".parse().unwrap();
    let bytes = to_vec(&dt).unwrap();
    let decoded: DateTime = from_slice(&bytes).unwrap();
    assert_eq!(dt, decoded);
}

#[test]
fn test_datetime_in_struct() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct Appointment {
        scheduled: DateTime,
        title: String,
    }

    let original = Appointment {
        scheduled: "2024-12-25T10:00:00".parse().unwrap(),
        title: "Meeting".to_string(),
    };

    let bytes = to_vec(&original).unwrap();
    let decoded: Appointment = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_datetime_in_vec() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct Schedule {
        times: Vec<DateTime>,
    }

    let original = Schedule {
        times: vec![
            "2024-12-23T09:00:00".parse().unwrap(),
            "2024-12-23T14:00:00".parse().unwrap(),
            "2024-12-23T17:30:00".parse().unwrap(),
        ],
    };

    let bytes = to_vec(&original).unwrap();
    let decoded: Schedule = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_multiple_jiff_types() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct TimeData {
        zoned: Zoned,
        timestamp: Timestamp,
        datetime: DateTime,
    }

    let original = TimeData {
        zoned: Zoned::now(),
        timestamp: Timestamp::now(),
        datetime: "2024-12-23T12:00:00".parse().unwrap(),
    };

    let bytes = to_vec(&original).unwrap();
    let decoded: TimeData = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_bare_zoned() {
    facet_testhelpers::setup();

    let zoned = Zoned::now();
    let bytes = to_vec(&zoned).unwrap();
    let decoded: Zoned = from_slice(&bytes).unwrap();
    assert_eq!(zoned, decoded);
}

#[test]
fn test_bare_timestamp() {
    facet_testhelpers::setup();

    let ts = Timestamp::now();
    let bytes = to_vec(&ts).unwrap();
    let decoded: Timestamp = from_slice(&bytes).unwrap();
    assert_eq!(ts, decoded);
}

#[test]
fn test_bare_datetime() {
    facet_testhelpers::setup();

    let dt: DateTime = "2024-12-23T15:30:00".parse().unwrap();
    let bytes = to_vec(&dt).unwrap();
    let decoded: DateTime = from_slice(&bytes).unwrap();
    assert_eq!(dt, decoded);
}
