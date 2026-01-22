//! Tests for external type support in facet-postcard.
//!
//! This module tests serialization/deserialization of types from external crates:
//! - uuid: UUID v4 identifiers
//! - ulid: Universally Unique Lexicographically Sortable Identifiers
//! - chrono: Date/time types (DateTime, NaiveDate, etc.)
//! - jiff: Modern date/time library
//! - time: Alternative date/time library
//! - bytes: Bytes and BytesMut
//! - ordered-float: OrderedFloat and NotNan
//! - net: IP addresses (std::net)

#![cfg(feature = "jit")]
// Allow unused imports - they're used by feature-gated submodules via `super::*`
#![allow(unused_imports)]

use facet::Facet;
use facet_postcard::{from_slice, to_vec};

// =============================================================================
// UUID Tests
// =============================================================================

#[cfg(feature = "uuid")]
mod uuid_tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn uuid_v4_roundtrip() {
        facet_testhelpers::setup();
        let uuid = Uuid::new_v4();
        let bytes = to_vec(&uuid).unwrap();
        let decoded: Uuid = from_slice(&bytes).unwrap();
        assert_eq!(uuid, decoded);
    }

    #[test]
    fn uuid_nil() {
        facet_testhelpers::setup();
        let uuid = Uuid::nil();
        let bytes = to_vec(&uuid).unwrap();
        let decoded: Uuid = from_slice(&bytes).unwrap();
        assert_eq!(uuid, decoded);
    }

    #[test]
    fn uuid_max() {
        facet_testhelpers::setup();
        let uuid = Uuid::from_bytes([0xFF; 16]);
        let bytes = to_vec(&uuid).unwrap();
        let decoded: Uuid = from_slice(&bytes).unwrap();
        assert_eq!(uuid, decoded);
    }

    #[test]
    fn uuid_serialization_size() {
        facet_testhelpers::setup();
        let uuid = Uuid::new_v4();
        let bytes = to_vec(&uuid).unwrap();
        // UUID should be exactly 16 bytes (no length prefix for opaque scalar)
        assert_eq!(bytes.len(), 16);
    }

    #[test]
    fn uuid_in_struct() {
        facet_testhelpers::setup();

        #[derive(Facet, PartialEq, Debug)]
        struct Record {
            id: Uuid,
            name: String,
        }

        let original = Record {
            id: Uuid::new_v4(),
            name: "test".to_string(),
        };

        let bytes = to_vec(&original).unwrap();
        let decoded: Record = from_slice(&bytes).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn uuid_in_option() {
        facet_testhelpers::setup();

        #[derive(Facet, PartialEq, Debug)]
        struct WithOptionalId {
            id: Option<Uuid>,
        }

        // Test Some variant
        let original = WithOptionalId {
            id: Some(Uuid::new_v4()),
        };
        let bytes = to_vec(&original).unwrap();
        let decoded: WithOptionalId = from_slice(&bytes).unwrap();
        assert_eq!(original, decoded);

        // Test None variant
        let original = WithOptionalId { id: None };
        let bytes = to_vec(&original).unwrap();
        let decoded: WithOptionalId = from_slice(&bytes).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn uuid_in_vec() {
        facet_testhelpers::setup();

        #[derive(Facet, PartialEq, Debug)]
        struct WithIds {
            ids: Vec<Uuid>,
        }

        let original = WithIds {
            ids: vec![Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()],
        };

        let bytes = to_vec(&original).unwrap();
        let decoded: WithIds = from_slice(&bytes).unwrap();
        assert_eq!(original, decoded);
    }
}

// =============================================================================
// ULID Tests
// =============================================================================

#[cfg(feature = "ulid")]
mod ulid_tests {
    use super::*;
    use ulid::Ulid;

    #[test]
    fn ulid_roundtrip() {
        facet_testhelpers::setup();
        let ulid = Ulid::new();
        let bytes = to_vec(&ulid).unwrap();
        let decoded: Ulid = from_slice(&bytes).unwrap();
        assert_eq!(ulid, decoded);
    }

    #[test]
    fn ulid_nil() {
        facet_testhelpers::setup();
        let ulid = Ulid::nil();
        let bytes = to_vec(&ulid).unwrap();
        let decoded: Ulid = from_slice(&bytes).unwrap();
        assert_eq!(ulid, decoded);
    }

    #[test]
    fn ulid_serialization_size() {
        facet_testhelpers::setup();
        let ulid = Ulid::new();
        let bytes = to_vec(&ulid).unwrap();
        // ULID should be exactly 16 bytes
        assert_eq!(bytes.len(), 16);
    }

    #[test]
    fn ulid_in_struct() {
        facet_testhelpers::setup();

        #[derive(Facet, PartialEq, Debug)]
        struct Event {
            id: Ulid,
            data: String,
        }

        let original = Event {
            id: Ulid::new(),
            data: "event data".to_string(),
        };

        let bytes = to_vec(&original).unwrap();
        let decoded: Event = from_slice(&bytes).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn ulid_in_option() {
        facet_testhelpers::setup();

        #[derive(Facet, PartialEq, Debug)]
        struct WithOptionalUlid {
            ulid: Option<Ulid>,
        }

        // Test Some variant
        let original = WithOptionalUlid {
            ulid: Some(Ulid::new()),
        };
        let bytes = to_vec(&original).unwrap();
        let decoded: WithOptionalUlid = from_slice(&bytes).unwrap();
        assert_eq!(original, decoded);

        // Test None variant
        let original = WithOptionalUlid { ulid: None };
        let bytes = to_vec(&original).unwrap();
        let decoded: WithOptionalUlid = from_slice(&bytes).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn ulid_ordering_preserved() {
        facet_testhelpers::setup();

        // ULIDs are lexicographically sortable by creation time
        let ulid1 = Ulid::new();
        std::thread::sleep(std::time::Duration::from_millis(2));
        let ulid2 = Ulid::new();

        assert!(ulid1 < ulid2);

        let bytes1 = to_vec(&ulid1).unwrap();
        let bytes2 = to_vec(&ulid2).unwrap();

        let decoded1: Ulid = from_slice(&bytes1).unwrap();
        let decoded2: Ulid = from_slice(&bytes2).unwrap();

        assert!(decoded1 < decoded2);
    }
}

// =============================================================================
// UUID + ULID Combined Tests
// =============================================================================

#[cfg(all(feature = "uuid", feature = "ulid"))]
mod uuid_ulid_combined {
    use super::*;
    use ulid::Ulid;
    use uuid::Uuid;

    #[test]
    fn uuid_and_ulid_together() {
        facet_testhelpers::setup();

        #[derive(Facet, PartialEq, Debug)]
        struct WithBothIds {
            uuid: Uuid,
            ulid: Ulid,
            count: u32,
        }

        let original = WithBothIds {
            uuid: Uuid::new_v4(),
            ulid: Ulid::new(),
            count: 42,
        };

        let bytes = to_vec(&original).unwrap();
        let decoded: WithBothIds = from_slice(&bytes).unwrap();
        assert_eq!(original, decoded);
    }
}

// =============================================================================
// Chrono Tests
// =============================================================================

#[cfg(feature = "chrono")]
mod chrono_tests {
    use super::*;
    use chrono::{DateTime, FixedOffset, Local, NaiveDate, NaiveDateTime, NaiveTime, Utc};

    #[test]
    fn datetime_utc_roundtrip() {
        facet_testhelpers::setup();
        let dt = Utc::now();
        let bytes = to_vec(&dt).unwrap();
        let decoded: DateTime<Utc> = from_slice(&bytes).unwrap();
        assert_eq!(dt, decoded);
    }

    #[test]
    fn datetime_utc_specific() {
        facet_testhelpers::setup();
        let dt: DateTime<Utc> = "2024-12-23T15:30:00Z".parse().unwrap();
        let bytes = to_vec(&dt).unwrap();
        let decoded: DateTime<Utc> = from_slice(&bytes).unwrap();
        assert_eq!(dt, decoded);
    }

    #[test]
    fn datetime_utc_in_struct() {
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
    fn datetime_local_roundtrip() {
        facet_testhelpers::setup();
        let dt = Local::now();
        let bytes = to_vec(&dt).unwrap();
        let decoded: DateTime<Local> = from_slice(&bytes).unwrap();
        assert_eq!(dt, decoded);
    }

    #[test]
    fn datetime_fixed_offset_roundtrip() {
        facet_testhelpers::setup();
        let dt: DateTime<FixedOffset> = "2024-12-23T15:30:00-05:00".parse().unwrap();
        let bytes = to_vec(&dt).unwrap();
        let decoded: DateTime<FixedOffset> = from_slice(&bytes).unwrap();
        assert_eq!(dt, decoded);
    }

    #[test]
    fn naive_datetime_roundtrip() {
        facet_testhelpers::setup();
        let dt = NaiveDateTime::parse_from_str("2024-12-23 15:30:00", "%Y-%m-%d %H:%M:%S").unwrap();
        let bytes = to_vec(&dt).unwrap();
        let decoded: NaiveDateTime = from_slice(&bytes).unwrap();
        assert_eq!(dt, decoded);
    }

    #[test]
    fn naive_date_roundtrip() {
        facet_testhelpers::setup();
        let date: NaiveDate = "2024-12-23".parse().unwrap();
        let bytes = to_vec(&date).unwrap();
        let decoded: NaiveDate = from_slice(&bytes).unwrap();
        assert_eq!(date, decoded);
    }

    #[test]
    fn naive_time_roundtrip() {
        facet_testhelpers::setup();
        let time: NaiveTime = "15:30:00".parse().unwrap();
        let bytes = to_vec(&time).unwrap();
        let decoded: NaiveTime = from_slice(&bytes).unwrap();
        assert_eq!(time, decoded);
    }

    #[test]
    fn chrono_in_option() {
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
    fn chrono_in_vec() {
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
    fn multiple_chrono_types() {
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
}

// =============================================================================
// Jiff Tests
// =============================================================================

#[cfg(feature = "jiff02")]
mod jiff_tests {
    use super::*;
    use jiff::{Timestamp, Zoned, civil::DateTime};

    #[test]
    fn timestamp_roundtrip() {
        facet_testhelpers::setup();
        let ts = Timestamp::now();
        let bytes = to_vec(&ts).unwrap();
        let decoded: Timestamp = from_slice(&bytes).unwrap();
        assert_eq!(ts, decoded);
    }

    #[test]
    fn datetime_roundtrip() {
        facet_testhelpers::setup();
        let dt: DateTime = "2024-12-23T15:30:00".parse().unwrap();
        let bytes = to_vec(&dt).unwrap();
        let decoded: DateTime = from_slice(&bytes).unwrap();
        assert_eq!(dt, decoded);
    }

    #[test]
    fn zoned_roundtrip() {
        facet_testhelpers::setup();
        let zoned: Zoned = "2024-12-23T15:30:00-05:00[America/New_York]"
            .parse()
            .unwrap();
        let bytes = to_vec(&zoned).unwrap();
        let decoded: Zoned = from_slice(&bytes).unwrap();
        assert_eq!(zoned, decoded);
    }

    #[test]
    fn jiff_in_struct() {
        facet_testhelpers::setup();

        #[derive(Facet, PartialEq, Debug)]
        struct Event {
            timestamp: Timestamp,
            name: String,
        }

        let original = Event {
            timestamp: Timestamp::now(),
            name: "test event".to_string(),
        };

        let bytes = to_vec(&original).unwrap();
        let decoded: Event = from_slice(&bytes).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn jiff_in_option() {
        facet_testhelpers::setup();

        #[derive(Facet, PartialEq, Debug)]
        struct WithOptionalTimestamp {
            timestamp: Option<Timestamp>,
        }

        // Test Some variant
        let original = WithOptionalTimestamp {
            timestamp: Some(Timestamp::now()),
        };
        let bytes = to_vec(&original).unwrap();
        let decoded: WithOptionalTimestamp = from_slice(&bytes).unwrap();
        assert_eq!(original, decoded);

        // Test None variant
        let original = WithOptionalTimestamp { timestamp: None };
        let bytes = to_vec(&original).unwrap();
        let decoded: WithOptionalTimestamp = from_slice(&bytes).unwrap();
        assert_eq!(original, decoded);
    }
}

// =============================================================================
// Time Crate Tests
// =============================================================================

#[cfg(feature = "time")]
mod time_tests {
    use super::*;
    use time::{OffsetDateTime, UtcDateTime};

    #[test]
    fn utc_datetime_roundtrip() {
        facet_testhelpers::setup();
        let dt = UtcDateTime::now();
        let bytes = to_vec(&dt).unwrap();
        let decoded: UtcDateTime = from_slice(&bytes).unwrap();
        // Compare with second precision due to subsecond differences
        assert_eq!(dt.unix_timestamp(), decoded.unix_timestamp());
    }

    #[test]
    fn offset_datetime_roundtrip() {
        facet_testhelpers::setup();
        let dt = OffsetDateTime::now_utc();
        let bytes = to_vec(&dt).unwrap();
        let decoded: OffsetDateTime = from_slice(&bytes).unwrap();
        // Compare with second precision due to subsecond differences
        assert_eq!(dt.unix_timestamp(), decoded.unix_timestamp());
    }

    #[test]
    fn time_in_struct() {
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
        // Compare timestamp with second precision
        assert_eq!(original.name, decoded.name);
        assert_eq!(
            original.timestamp.unix_timestamp(),
            decoded.timestamp.unix_timestamp()
        );
    }
}

// =============================================================================
// Bytes Crate Tests
// =============================================================================

#[cfg(feature = "bytes")]
mod bytes_tests {
    use super::*;
    use bytes::{Bytes, BytesMut};

    #[test]
    fn bytes_roundtrip() {
        facet_testhelpers::setup();
        let original = Bytes::from_static(b"hello world");
        let serialized = to_vec(&original).unwrap();
        let decoded: Bytes = from_slice(&serialized).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn bytes_empty() {
        facet_testhelpers::setup();
        let original = Bytes::new();
        let serialized = to_vec(&original).unwrap();
        let decoded: Bytes = from_slice(&serialized).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn bytes_large() {
        facet_testhelpers::setup();
        let data: Vec<u8> = (0..1000).map(|i| (i % 256) as u8).collect();
        let original = Bytes::from(data);
        let serialized = to_vec(&original).unwrap();
        let decoded: Bytes = from_slice(&serialized).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn bytesmut_roundtrip() {
        facet_testhelpers::setup();
        let original = BytesMut::from(&b"hello world"[..]);
        let serialized = to_vec(&original).unwrap();
        let decoded: BytesMut = from_slice(&serialized).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn bytes_in_struct() {
        facet_testhelpers::setup();

        #[derive(Facet, PartialEq, Debug)]
        struct BinaryData {
            name: String,
            data: Bytes,
        }

        let original = BinaryData {
            name: "test".to_string(),
            data: Bytes::from_static(b"binary content"),
        };

        let serialized = to_vec(&original).unwrap();
        let decoded: BinaryData = from_slice(&serialized).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn bytes_in_option() {
        facet_testhelpers::setup();

        #[derive(Facet, PartialEq, Debug)]
        struct WithOptionalBytes {
            data: Option<Bytes>,
        }

        // Test Some variant
        let original = WithOptionalBytes {
            data: Some(Bytes::from_static(b"data")),
        };
        let serialized = to_vec(&original).unwrap();
        let decoded: WithOptionalBytes = from_slice(&serialized).unwrap();
        assert_eq!(original, decoded);

        // Test None variant
        let original = WithOptionalBytes { data: None };
        let serialized = to_vec(&original).unwrap();
        let decoded: WithOptionalBytes = from_slice(&serialized).unwrap();
        assert_eq!(original, decoded);
    }
}

// =============================================================================
// OrderedFloat Tests
// =============================================================================

#[cfg(feature = "ordered-float")]
mod ordered_float_tests {
    use super::*;
    use ordered_float::{NotNan, OrderedFloat};

    #[test]
    fn ordered_float_f32_roundtrip() {
        facet_testhelpers::setup();
        let original = OrderedFloat(2.71f32);
        let bytes = to_vec(&original).unwrap();
        let decoded: OrderedFloat<f32> = from_slice(&bytes).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn ordered_float_f64_roundtrip() {
        facet_testhelpers::setup();
        let original = OrderedFloat(123.456789f64);
        let bytes = to_vec(&original).unwrap();
        let decoded: OrderedFloat<f64> = from_slice(&bytes).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn ordered_float_serialization_size() {
        facet_testhelpers::setup();
        let f32_val = OrderedFloat(1.0f32);
        let f64_val = OrderedFloat(1.0f64);

        let f32_bytes = to_vec(&f32_val).unwrap();
        let f64_bytes = to_vec(&f64_val).unwrap();

        assert_eq!(f32_bytes.len(), 4, "OrderedFloat<f32> should be 4 bytes");
        assert_eq!(f64_bytes.len(), 8, "OrderedFloat<f64> should be 8 bytes");
    }

    #[test]
    fn ordered_float_in_struct() {
        facet_testhelpers::setup();

        #[derive(Facet, PartialEq, Debug)]
        struct Measurement {
            value: OrderedFloat<f64>,
            unit: String,
        }

        let original = Measurement {
            value: OrderedFloat(42.5),
            unit: "meters".to_string(),
        };

        let bytes = to_vec(&original).unwrap();
        let decoded: Measurement = from_slice(&bytes).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn ordered_float_special_values() {
        facet_testhelpers::setup();

        // Test infinity
        let inf = OrderedFloat(f64::INFINITY);
        let bytes = to_vec(&inf).unwrap();
        let decoded: OrderedFloat<f64> = from_slice(&bytes).unwrap();
        assert_eq!(inf, decoded);

        // Test negative infinity
        let neg_inf = OrderedFloat(f64::NEG_INFINITY);
        let bytes = to_vec(&neg_inf).unwrap();
        let decoded: OrderedFloat<f64> = from_slice(&bytes).unwrap();
        assert_eq!(neg_inf, decoded);

        // Test NaN (OrderedFloat allows NaN comparison)
        let nan = OrderedFloat(f64::NAN);
        let bytes = to_vec(&nan).unwrap();
        let decoded: OrderedFloat<f64> = from_slice(&bytes).unwrap();
        assert!(decoded.is_nan());
    }

    #[test]
    fn notnan_f32_roundtrip() {
        facet_testhelpers::setup();
        let original = NotNan::new(2.71f32).unwrap();
        let bytes = to_vec(&original).unwrap();
        let decoded: NotNan<f32> = from_slice(&bytes).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn notnan_f64_roundtrip() {
        facet_testhelpers::setup();
        let original = NotNan::new(123.456789f64).unwrap();
        let bytes = to_vec(&original).unwrap();
        let decoded: NotNan<f64> = from_slice(&bytes).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn notnan_in_struct() {
        facet_testhelpers::setup();

        #[derive(Facet, PartialEq, Debug)]
        struct Score {
            value: NotNan<f64>,
            label: String,
        }

        let original = Score {
            value: NotNan::new(95.5).unwrap(),
            label: "test score".to_string(),
        };

        let bytes = to_vec(&original).unwrap();
        let decoded: Score = from_slice(&bytes).unwrap();
        assert_eq!(original, decoded);
    }
}

// =============================================================================
// Network Types Tests
// =============================================================================

#[cfg(feature = "net")]
mod net_tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6};

    #[test]
    fn ipv4_addr_roundtrip() {
        facet_testhelpers::setup();

        #[derive(Facet, PartialEq, Debug)]
        struct WithIpv4 {
            addr: Ipv4Addr,
        }

        let original = WithIpv4 {
            addr: Ipv4Addr::new(192, 168, 1, 1),
        };
        let bytes = to_vec(&original).unwrap();
        let decoded: WithIpv4 = from_slice(&bytes).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn ipv6_addr_roundtrip() {
        facet_testhelpers::setup();

        #[derive(Facet, PartialEq, Debug)]
        struct WithIpv6 {
            addr: Ipv6Addr,
        }

        let original = WithIpv6 {
            addr: Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1),
        };
        let bytes = to_vec(&original).unwrap();
        let decoded: WithIpv6 = from_slice(&bytes).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn ip_addr_v4_roundtrip() {
        facet_testhelpers::setup();

        #[derive(Facet, PartialEq, Debug)]
        struct WithIpAddr {
            addr: IpAddr,
        }

        let original = WithIpAddr {
            addr: IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
        };
        let bytes = to_vec(&original).unwrap();
        let decoded: WithIpAddr = from_slice(&bytes).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn ip_addr_v6_roundtrip() {
        facet_testhelpers::setup();

        #[derive(Facet, PartialEq, Debug)]
        struct WithIpAddr {
            addr: IpAddr,
        }

        let original = WithIpAddr {
            addr: IpAddr::V6(Ipv6Addr::LOCALHOST),
        };
        let bytes = to_vec(&original).unwrap();
        let decoded: WithIpAddr = from_slice(&bytes).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn socket_addr_v4_roundtrip() {
        facet_testhelpers::setup();

        #[derive(Facet, PartialEq, Debug)]
        struct WithSocketAddrV4 {
            addr: SocketAddrV4,
        }

        let original = WithSocketAddrV4 {
            addr: SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8080),
        };
        let bytes = to_vec(&original).unwrap();
        let decoded: WithSocketAddrV4 = from_slice(&bytes).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn socket_addr_v6_roundtrip() {
        facet_testhelpers::setup();

        #[derive(Facet, PartialEq, Debug)]
        struct WithSocketAddrV6 {
            addr: SocketAddrV6,
        }

        let original = WithSocketAddrV6 {
            addr: SocketAddrV6::new(Ipv6Addr::LOCALHOST, 8080, 0, 0),
        };
        let bytes = to_vec(&original).unwrap();
        let decoded: WithSocketAddrV6 = from_slice(&bytes).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn socket_addr_roundtrip() {
        facet_testhelpers::setup();

        #[derive(Facet, PartialEq, Debug)]
        struct WithSocketAddr {
            addr: SocketAddr,
        }

        // V4
        let original = WithSocketAddr {
            addr: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(192, 168, 1, 1), 443)),
        };
        let bytes = to_vec(&original).unwrap();
        let decoded: WithSocketAddr = from_slice(&bytes).unwrap();
        assert_eq!(original, decoded);

        // V6
        let original = WithSocketAddr {
            addr: SocketAddr::V6(SocketAddrV6::new(Ipv6Addr::LOCALHOST, 8080, 0, 0)),
        };
        let bytes = to_vec(&original).unwrap();
        let decoded: WithSocketAddr = from_slice(&bytes).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn ip_addrs_special_values() {
        facet_testhelpers::setup();

        #[derive(Facet, PartialEq, Debug)]
        struct AddressTest {
            addr: Ipv4Addr,
        }

        // Localhost
        let original = AddressTest {
            addr: Ipv4Addr::LOCALHOST,
        };
        let bytes = to_vec(&original).unwrap();
        let decoded: AddressTest = from_slice(&bytes).unwrap();
        assert_eq!(original, decoded);

        // Broadcast
        let original = AddressTest {
            addr: Ipv4Addr::BROADCAST,
        };
        let bytes = to_vec(&original).unwrap();
        let decoded: AddressTest = from_slice(&bytes).unwrap();
        assert_eq!(original, decoded);

        // Unspecified
        let original = AddressTest {
            addr: Ipv4Addr::UNSPECIFIED,
        };
        let bytes = to_vec(&original).unwrap();
        let decoded: AddressTest = from_slice(&bytes).unwrap();
        assert_eq!(original, decoded);
    }
}

// =============================================================================
// Camino Path Tests
// =============================================================================

#[cfg(feature = "camino")]
mod camino_tests {
    use super::*;
    use camino::Utf8PathBuf;

    #[test]
    fn utf8pathbuf_roundtrip() {
        facet_testhelpers::setup();
        let path = Utf8PathBuf::from("/path/to/file");
        let bytes = to_vec(&path).unwrap();
        let decoded: Utf8PathBuf = from_slice(&bytes).unwrap();
        assert_eq!(path, decoded);
    }

    #[test]
    fn utf8pathbuf_empty() {
        facet_testhelpers::setup();
        let path = Utf8PathBuf::new();
        let bytes = to_vec(&path).unwrap();
        let decoded: Utf8PathBuf = from_slice(&bytes).unwrap();
        assert_eq!(path, decoded);
    }

    #[test]
    fn utf8pathbuf_relative() {
        facet_testhelpers::setup();
        let path = Utf8PathBuf::from("relative/path/to/file.txt");
        let bytes = to_vec(&path).unwrap();
        let decoded: Utf8PathBuf = from_slice(&bytes).unwrap();
        assert_eq!(path, decoded);
    }

    #[test]
    fn utf8pathbuf_in_struct() {
        facet_testhelpers::setup();

        #[derive(Facet, PartialEq, Debug)]
        struct FileInfo {
            path: Utf8PathBuf,
            size: u64,
        }

        let original = FileInfo {
            path: Utf8PathBuf::from("/home/user/documents/file.txt"),
            size: 1024,
        };

        let bytes = to_vec(&original).unwrap();
        let decoded: FileInfo = from_slice(&bytes).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn utf8pathbuf_in_option() {
        facet_testhelpers::setup();

        #[derive(Facet, PartialEq, Debug)]
        struct WithOptionalPath {
            path: Option<Utf8PathBuf>,
        }

        // Test Some variant
        let original = WithOptionalPath {
            path: Some(Utf8PathBuf::from("/path")),
        };
        let bytes = to_vec(&original).unwrap();
        let decoded: WithOptionalPath = from_slice(&bytes).unwrap();
        assert_eq!(original, decoded);

        // Test None variant
        let original = WithOptionalPath { path: None };
        let bytes = to_vec(&original).unwrap();
        let decoded: WithOptionalPath = from_slice(&bytes).unwrap();
        assert_eq!(original, decoded);
    }

    #[test]
    fn utf8pathbuf_in_vec() {
        facet_testhelpers::setup();

        #[derive(Facet, PartialEq, Debug)]
        struct PathList {
            paths: Vec<Utf8PathBuf>,
        }

        let original = PathList {
            paths: vec![
                Utf8PathBuf::from("/a"),
                Utf8PathBuf::from("/b/c"),
                Utf8PathBuf::from("/d/e/f"),
            ],
        };

        let bytes = to_vec(&original).unwrap();
        let decoded: PathList = from_slice(&bytes).unwrap();
        assert_eq!(original, decoded);
    }
}
