use chrono::{DateTime, FixedOffset, Local, NaiveDate, NaiveDateTime, NaiveTime, Utc};
use facet::Facet;
use facet_postcard::{from_slice, to_vec};

#[test]
fn test_datetime_utc_roundtrip() {
    facet_testhelpers::setup();

    let dt = Utc::now();
    let bytes = to_vec(&dt).unwrap();
    let decoded: DateTime<Utc> = from_slice(&bytes).unwrap();
    assert_eq!(dt, decoded);
}

#[test]
fn test_datetime_utc_specific() {
    facet_testhelpers::setup();

    let dt: DateTime<Utc> = "2024-12-23T15:30:00Z".parse().unwrap();
    let bytes = to_vec(&dt).unwrap();
    let decoded: DateTime<Utc> = from_slice(&bytes).unwrap();
    assert_eq!(dt, decoded);
}

#[test]
fn test_datetime_utc_in_struct() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct Event {
        timestamp: DateTime<Utc>,
        name: String,
    }

    let original = Event {
        timestamp: Utc::now(),
        name: "test event".to_string(),
    };

    let bytes = to_vec(&original).unwrap();
    let decoded: Event = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_datetime_local_roundtrip() {
    facet_testhelpers::setup();

    let dt = Local::now();
    let bytes = to_vec(&dt).unwrap();
    let decoded: DateTime<Local> = from_slice(&bytes).unwrap();
    assert_eq!(dt, decoded);
}

#[test]
fn test_datetime_fixed_offset_roundtrip() {
    facet_testhelpers::setup();

    let dt: DateTime<FixedOffset> = "2024-12-23T15:30:00-05:00".parse().unwrap();
    let bytes = to_vec(&dt).unwrap();
    let decoded: DateTime<FixedOffset> = from_slice(&bytes).unwrap();
    assert_eq!(dt, decoded);
}

#[test]
fn test_datetime_fixed_offset_in_struct() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct TimedEvent {
        when: DateTime<FixedOffset>,
        what: String,
    }

    let original = TimedEvent {
        when: "2024-12-23T15:30:00+03:00".parse().unwrap(),
        what: "event".to_string(),
    };

    let bytes = to_vec(&original).unwrap();
    let decoded: TimedEvent = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_naive_datetime_roundtrip() {
    facet_testhelpers::setup();

    // Use explicit parsing with format instead of FromStr
    let dt = NaiveDateTime::parse_from_str("2024-12-23 15:30:00", "%Y-%m-%d %H:%M:%S").unwrap();
    let bytes = to_vec(&dt).unwrap();
    let decoded: NaiveDateTime = from_slice(&bytes).unwrap();
    assert_eq!(dt, decoded);
}

#[test]
fn test_naive_datetime_in_struct() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct Schedule {
        time: NaiveDateTime,
    }

    let original = Schedule {
        time: NaiveDateTime::parse_from_str("2024-12-25 10:00:00", "%Y-%m-%d %H:%M:%S").unwrap(),
    };

    let bytes = to_vec(&original).unwrap();
    let decoded: Schedule = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_naive_date_roundtrip() {
    facet_testhelpers::setup();

    let date: NaiveDate = "2024-12-23".parse().unwrap();
    let bytes = to_vec(&date).unwrap();
    let decoded: NaiveDate = from_slice(&bytes).unwrap();
    assert_eq!(date, decoded);
}

#[test]
fn test_naive_date_in_struct() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct DateRecord {
        date: NaiveDate,
        description: String,
    }

    let original = DateRecord {
        date: "2024-12-25".parse().unwrap(),
        description: "Christmas".to_string(),
    };

    let bytes = to_vec(&original).unwrap();
    let decoded: DateRecord = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_naive_time_roundtrip() {
    facet_testhelpers::setup();

    let time: NaiveTime = "15:30:00".parse().unwrap();
    let bytes = to_vec(&time).unwrap();
    let decoded: NaiveTime = from_slice(&bytes).unwrap();
    assert_eq!(time, decoded);
}

#[test]
fn test_naive_time_in_struct() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct Alarm {
        time: NaiveTime,
        label: String,
    }

    let original = Alarm {
        time: "07:30:00".parse().unwrap(),
        label: "Wake up".to_string(),
    };

    let bytes = to_vec(&original).unwrap();
    let decoded: Alarm = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_multiple_chrono_types() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct TimeData {
        utc: DateTime<Utc>,
        date: NaiveDate,
        time: NaiveTime,
    }

    let original = TimeData {
        utc: Utc::now(),
        date: "2024-12-23".parse().unwrap(),
        time: "15:30:00".parse().unwrap(),
    };

    let bytes = to_vec(&original).unwrap();
    let decoded: TimeData = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_chrono_in_option() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct WithOptionalDateTime {
        timestamp: Option<DateTime<Utc>>,
    }

    // Test Some variant
    let original = WithOptionalDateTime {
        timestamp: Some(Utc::now()),
    };
    let bytes = to_vec(&original).unwrap();
    let decoded: WithOptionalDateTime = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);

    // Test None variant
    let original = WithOptionalDateTime { timestamp: None };
    let bytes = to_vec(&original).unwrap();
    let decoded: WithOptionalDateTime = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_chrono_in_vec() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct EventLog {
        timestamps: Vec<DateTime<Utc>>,
    }

    let original = EventLog {
        timestamps: vec![Utc::now(), Utc::now(), Utc::now()],
    };

    let bytes = to_vec(&original).unwrap();
    let decoded: EventLog = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_bare_datetime_utc() {
    facet_testhelpers::setup();

    let dt = Utc::now();
    let bytes = to_vec(&dt).unwrap();
    let decoded: DateTime<Utc> = from_slice(&bytes).unwrap();
    assert_eq!(dt, decoded);
}

#[test]
fn test_bare_naive_date() {
    facet_testhelpers::setup();

    let date: NaiveDate = "2024-12-23".parse().unwrap();
    let bytes = to_vec(&date).unwrap();
    let decoded: NaiveDate = from_slice(&bytes).unwrap();
    assert_eq!(date, decoded);
}

#[test]
fn test_bare_naive_time() {
    facet_testhelpers::setup();

    let time: NaiveTime = "15:30:00".parse().unwrap();
    let bytes = to_vec(&time).unwrap();
    let decoded: NaiveTime = from_slice(&bytes).unwrap();
    assert_eq!(time, decoded);
}
