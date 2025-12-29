#![cfg(feature = "time")]

use facet::Facet;
use facet_postcard_legacy::{from_slice, to_vec};
use time::{OffsetDateTime, UtcDateTime};

#[test]
fn test_utc_datetime_roundtrip() {
    facet_testhelpers::setup();

    let dt = UtcDateTime::now();
    let bytes = to_vec(&dt).unwrap();
    let decoded: UtcDateTime = from_slice(&bytes).unwrap();
    assert_eq!(dt, decoded);
}

#[test]
fn test_utc_datetime_unix_epoch() {
    facet_testhelpers::setup();

    let dt = UtcDateTime::UNIX_EPOCH;
    let bytes = to_vec(&dt).unwrap();
    let decoded: UtcDateTime = from_slice(&bytes).unwrap();
    assert_eq!(dt, decoded);
}

#[test]
fn test_utc_datetime_in_struct() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct Event {
        timestamp: UtcDateTime,
        name: String,
    }

    let original = Event {
        timestamp: UtcDateTime::now(),
        name: "test event".to_string(),
    };

    let bytes = to_vec(&original).unwrap();
    let decoded: Event = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_utc_datetime_in_option() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct WithOptionalTime {
        time: Option<UtcDateTime>,
    }

    // Test Some variant
    let original = WithOptionalTime {
        time: Some(UtcDateTime::now()),
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
fn test_offset_datetime_roundtrip() {
    facet_testhelpers::setup();

    let dt = OffsetDateTime::now_utc();
    let bytes = to_vec(&dt).unwrap();
    let decoded: OffsetDateTime = from_slice(&bytes).unwrap();
    assert_eq!(dt, decoded);
}

#[test]
fn test_offset_datetime_unix_epoch() {
    facet_testhelpers::setup();

    let dt = OffsetDateTime::UNIX_EPOCH;
    let bytes = to_vec(&dt).unwrap();
    let decoded: OffsetDateTime = from_slice(&bytes).unwrap();
    assert_eq!(dt, decoded);
}

#[test]
fn test_offset_datetime_in_struct() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct LogEntry {
        timestamp: OffsetDateTime,
        message: String,
    }

    let original = LogEntry {
        timestamp: OffsetDateTime::now_utc(),
        message: "log message".to_string(),
    };

    let bytes = to_vec(&original).unwrap();
    let decoded: LogEntry = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_offset_datetime_in_vec() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct TimeLog {
        times: Vec<OffsetDateTime>,
    }

    let original = TimeLog {
        times: vec![
            OffsetDateTime::now_utc(),
            OffsetDateTime::now_utc(),
            OffsetDateTime::now_utc(),
        ],
    };

    let bytes = to_vec(&original).unwrap();
    let decoded: TimeLog = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_multiple_time_types() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct TimeData {
        utc: UtcDateTime,
        offset: OffsetDateTime,
    }

    let original = TimeData {
        utc: UtcDateTime::now(),
        offset: OffsetDateTime::now_utc(),
    };

    let bytes = to_vec(&original).unwrap();
    let decoded: TimeData = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_bare_utc_datetime() {
    facet_testhelpers::setup();

    let dt = UtcDateTime::now();
    let bytes = to_vec(&dt).unwrap();
    let decoded: UtcDateTime = from_slice(&bytes).unwrap();
    assert_eq!(dt, decoded);
}

#[test]
fn test_bare_offset_datetime() {
    facet_testhelpers::setup();

    let dt = OffsetDateTime::now_utc();
    let bytes = to_vec(&dt).unwrap();
    let decoded: OffsetDateTime = from_slice(&bytes).unwrap();
    assert_eq!(dt, decoded);
}

#[test]
fn test_utc_datetime_in_enum() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    #[repr(C)]
    enum Event {
        Scheduled(UtcDateTime),
        Completed { at: UtcDateTime, result: String },
    }

    // Test tuple variant
    let original = Event::Scheduled(UtcDateTime::now());
    let bytes = to_vec(&original).unwrap();
    let decoded: Event = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);

    // Test struct variant
    let original = Event::Completed {
        at: UtcDateTime::now(),
        result: "success".to_string(),
    };
    let bytes = to_vec(&original).unwrap();
    let decoded: Event = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);
}
