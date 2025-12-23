use facet::Facet;
use facet_postcard::{from_slice, to_vec};
use ulid::Ulid;
use uuid::Uuid;

#[test]
fn test_uuid_v4_roundtrip() {
    facet_testhelpers::setup();

    let uuid = Uuid::new_v4();
    let bytes = to_vec(&uuid).unwrap();
    let decoded: Uuid = from_slice(&bytes).unwrap();
    assert_eq!(uuid, decoded);
}

#[test]
fn test_uuid_nil() {
    facet_testhelpers::setup();

    let uuid = Uuid::nil();
    let bytes = to_vec(&uuid).unwrap();
    let decoded: Uuid = from_slice(&bytes).unwrap();
    assert_eq!(uuid, decoded);
}

#[test]
fn test_uuid_max() {
    facet_testhelpers::setup();

    let uuid = Uuid::from_bytes([0xFF; 16]);
    let bytes = to_vec(&uuid).unwrap();
    let decoded: Uuid = from_slice(&bytes).unwrap();
    assert_eq!(uuid, decoded);
}

#[test]
fn test_uuid_in_struct() {
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
fn test_uuid_in_option() {
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
fn test_uuid_in_vec() {
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

#[test]
fn test_uuid_serialization_size() {
    facet_testhelpers::setup();

    let uuid = Uuid::new_v4();
    let bytes = to_vec(&uuid).unwrap();
    // UUID should be exactly 16 bytes (no length prefix for opaque scalar)
    assert_eq!(bytes.len(), 16);
}

#[test]
fn test_ulid_roundtrip() {
    facet_testhelpers::setup();

    let ulid = Ulid::new();
    let bytes = to_vec(&ulid).unwrap();
    let decoded: Ulid = from_slice(&bytes).unwrap();
    assert_eq!(ulid, decoded);
}

#[test]
fn test_ulid_nil() {
    facet_testhelpers::setup();

    let ulid = Ulid::nil();
    let bytes = to_vec(&ulid).unwrap();
    let decoded: Ulid = from_slice(&bytes).unwrap();
    assert_eq!(ulid, decoded);
}

#[test]
fn test_ulid_in_struct() {
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
fn test_ulid_in_option() {
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
fn test_ulid_serialization_size() {
    facet_testhelpers::setup();

    let ulid = Ulid::new();
    let bytes = to_vec(&ulid).unwrap();
    // ULID should be exactly 16 bytes
    assert_eq!(bytes.len(), 16);
}

#[test]
fn test_ulid_ordering_preserved() {
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

#[test]
fn test_uuid_and_ulid_together() {
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

#[test]
fn test_bare_uuid() {
    facet_testhelpers::setup();

    let uuid = Uuid::new_v4();
    let bytes = to_vec(&uuid).unwrap();
    let decoded: Uuid = from_slice(&bytes).unwrap();
    assert_eq!(uuid, decoded);
}

#[test]
fn test_bare_ulid() {
    facet_testhelpers::setup();

    let ulid = Ulid::new();
    let bytes = to_vec(&ulid).unwrap();
    let decoded: Ulid = from_slice(&bytes).unwrap();
    assert_eq!(ulid, decoded);
}
