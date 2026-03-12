use facet::Facet;
use facet_format::DeserializeErrorKind;
use facet_postcard::{from_slice_borrowed, to_vec};

#[repr(u8)]
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
enum BorrowedValue<'a> {
    Str(&'a str) = 0,
    Bytes(&'a [u8]) = 1,
    U64(u64) = 2,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
struct Entry<'a> {
    key: &'a str,
    value: BorrowedValue<'a>,
    flags: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Facet)]
struct Message<'a> {
    id: u32,
    entries: &'a [Entry<'a>],
}

#[test]
fn issue_2098_borrowed_slice_of_structs_empty() {
    facet_testhelpers::setup();

    let msg = Message {
        id: 1,
        entries: &[],
    };
    let bytes = to_vec(&msg).expect("serialize");
    let decoded: Message<'_> = from_slice_borrowed(&bytes).expect("deserialize");

    assert_eq!(decoded, msg);
}

#[test]
fn issue_2098_borrowed_slice_of_structs_non_empty() {
    facet_testhelpers::setup();

    let entries = [Entry {
        key: "x",
        value: BorrowedValue::Str("hello"),
        flags: 7,
    }];
    let msg = Message {
        id: 1,
        entries: &entries,
    };
    let bytes = to_vec(&msg).expect("serialize");
    let err = from_slice_borrowed::<Message<'_>>(&bytes).expect_err("should fail with clear error");
    assert!(matches!(
        err.kind,
        DeserializeErrorKind::CannotBorrow { .. }
    ));
}
