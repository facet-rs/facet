use bstr::{BStr, BString};
use facet::Facet;

#[test]
fn bstring_serializes_as_byte_array() {
    let value = BString::from(vec![104, 105, 255]);
    let json = facet_json::to_string(&value).unwrap();
    assert_eq!(json, "[104,105,255]");
}

#[test]
fn bstring_round_trips_invalid_utf8() {
    let value: BString = facet_json::from_str("[0,159,146,150,255]").unwrap();
    assert_eq!(value, BString::from(vec![0, 159, 146, 150, 255]));

    let json = facet_json::to_string(&value).unwrap();
    assert_eq!(json, "[0,159,146,150,255]");
}

#[test]
fn bstr_serializes_as_byte_array() {
    let value = BStr::new(&[1, 2, 3, 255]);
    let json = facet_json::to_string(value).unwrap();
    assert_eq!(json, "[1,2,3,255]");
}

#[test]
fn bstring_field_needs_no_proxy() {
    #[derive(Facet, Debug, PartialEq)]
    struct Container {
        data: BString,
    }

    let value: Container = facet_json::from_str(r#"{"data":[65,66,255]}"#).unwrap();
    assert_eq!(
        value,
        Container {
            data: BString::from(vec![65, 66, 255]),
        }
    );
    assert_eq!(
        facet_json::to_string(&value).unwrap(),
        r#"{"data":[65,66,255]}"#
    );
}
