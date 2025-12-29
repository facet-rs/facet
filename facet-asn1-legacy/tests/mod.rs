use facet::Facet;
use facet_asn1_legacy::{deserialize_der, to_vec_der};
use facet_testhelpers::test;

#[derive(Debug, Clone, Facet, PartialEq, Eq)]
#[facet(type_tag = "IA5String", transparent)]
struct IA5String(String);

impl From<String> for IA5String {
    fn from(value: String) -> Self {
        Self(value)
    }
}

// FooQuestion ::= SEQUENCE {
//   trackingNumber INTEGER,
//   question       IA5String
// }
#[derive(Debug, Facet, PartialEq, Eq)]
struct FooQuestion {
    tracking_number: i8,
    question: IA5String,
}

#[test]
fn test_deserialize_foo_question() {
    let der: [u8; 21] = [
        0x30, 0x13, 0x02, 0x01, 0x05, 0x16, 0x0e, 0x41, 0x6e, 0x79, 0x62, 0x6f, 0x64, 0x79, 0x20,
        0x74, 0x68, 0x65, 0x72, 0x65, 0x3f,
    ];
    let question: FooQuestion = deserialize_der(&der).unwrap();
    assert_eq!(
        question,
        FooQuestion {
            tracking_number: 5,
            question: String::from("Anybody there?").into(),
        }
    );
}

#[test]
fn test_serialize_foo_question() {
    let question = FooQuestion {
        tracking_number: 5,
        question: String::from("Anybody there?").into(),
    };
    let der = to_vec_der(&question).unwrap();
    let expected_der: [u8; 21] = [
        0x30, 0x13, 0x02, 0x01, 0x05, 0x16, 0x0e, 0x41, 0x6e, 0x79, 0x62, 0x6f, 0x64, 0x79, 0x20,
        0x74, 0x68, 0x65, 0x72, 0x65, 0x3f,
    ];
    assert_eq!(&expected_der[..], &der[..]);
}

#[derive(Debug, Facet, Clone, Copy, PartialEq, Eq)]
#[facet(type_tag = "0", transparent)]
struct OptionalIntZero(Option<i32>);

impl From<Option<i32>> for OptionalIntZero {
    fn from(value: Option<i32>) -> Self {
        Self(value)
    }
}

#[derive(Debug, Facet, Clone, Copy, PartialEq, Eq)]
#[facet(type_tag = "1", transparent)]
struct OptionalIntOne(Option<i32>);

impl From<Option<i32>> for OptionalIntOne {
    fn from(value: Option<i32>) -> Self {
        Self(value)
    }
}

// Point ::= SEQUENCE {
//   x [0] INTEGER OPTIONAL,
//   y [1] INTEGER OPTIONAL,
// }
#[derive(Debug, Facet, PartialEq, Eq)]
struct Point {
    x: OptionalIntZero,
    y: OptionalIntOne,
}

#[test]
fn test_deserialize_point_x() {
    let der: [u8; 5] = [0x30, 0x03, 0x80, 0x01, 0x09];
    let point: Point = deserialize_der(&der).unwrap();
    assert_eq!(
        point,
        Point {
            x: Some(9).into(),
            y: None.into(),
        }
    );
}

#[test]
fn test_deserialize_point_y() {
    let der: [u8; 5] = [0x30, 0x03, 0x81, 0x01, 0x09];
    let point: Point = deserialize_der(&der).unwrap();
    assert_eq!(
        point,
        Point {
            x: None.into(),
            y: Some(9).into(),
        }
    );
}

#[test]
fn test_deserialize_point_x_y() {
    let der: [u8; 8] = [0x30, 0x06, 0x80, 0x01, 0x09, 0x81, 0x01, 0x09];
    let point: Point = deserialize_der(&der).unwrap();
    assert_eq!(
        point,
        Point {
            x: Some(9).into(),
            y: Some(9).into(),
        }
    );
}

#[test]
fn test_serialize_point_x() {
    let point = Point {
        x: Some(9).into(),
        y: None.into(),
    };
    let der = to_vec_der(&point).unwrap();
    let expected_der = [0x30, 0x03, 0x80, 0x01, 0x09];
    assert_eq!(&expected_der[..], &der[..]);
}

#[test]
fn test_serialize_point_y() {
    let point = Point {
        x: None.into(),
        y: Some(9).into(),
    };
    let der = to_vec_der(&point).unwrap();
    let expected_der = [0x30, 0x03, 0x81, 0x01, 0x09];
    assert_eq!(&expected_der[..], &der[..]);
}

#[test]
fn test_serialize_point_x_y() {
    let point = Point {
        x: Some(9).into(),
        y: Some(9).into(),
    };
    let der = to_vec_der(&point).unwrap();
    let expected_der: [u8; 8] = [0x30, 0x06, 0x80, 0x01, 0x09, 0x81, 0x01, 0x09];
    assert_eq!(&expected_der[..], &der[..]);
}

// ImplicitString ::= [5] IMPLICIT UTF8String
#[derive(Debug, Facet, PartialEq, Eq)]
#[facet(type_tag = "5", transparent)]
struct ImplicitString(String);

// ExplciitString ::= [5] EXPLICIT UTF8String
#[derive(Debug, Facet, PartialEq, Eq)]
#[facet(type_tag = "5")]
struct ExplicitString(String);

#[test]
fn test_deserialize_implicit_string() {
    let der: [u8; 4] = [0x85, 0x02, 0x68, 0x69];
    let implicit_string: ImplicitString = deserialize_der(&der).unwrap();
    assert_eq!(implicit_string, ImplicitString(String::from("hi")));
}

#[test]
fn test_serialize_implicit_string() {
    let implicit_string = ImplicitString("hi".to_owned());
    let der = to_vec_der(&implicit_string).unwrap();
    let expected_der: [u8; 4] = [0x85, 0x02, 0x68, 0x69];
    assert_eq!(&expected_der[..], &der[..]);
}

#[test]
fn test_deserialize_explicit_string() {
    let der: [u8; 6] = [0xA5, 0x04, 0x0C, 0x02, 0x68, 0x69];
    let explicit_string: ExplicitString = deserialize_der(&der).unwrap();
    assert_eq!(explicit_string, ExplicitString(String::from("hi")));
}

#[test]
fn test_serialize_explicit_string() {
    let explicit_string = ExplicitString("hi".to_owned());
    let der = to_vec_der(&explicit_string).unwrap();
    let expected_der: [u8; 6] = [0xA5, 0x04, 0x0C, 0x02, 0x68, 0x69];
    assert_eq!(&expected_der[..], &der[..]);
}

// NullStruct ::= NULL
#[derive(Debug, Facet, PartialEq, Eq)]
struct NullStruct;

#[test]
fn test_deserialize_null() {
    let der: [u8; 2] = [0x05, 0x00];
    let null_struct: NullStruct = deserialize_der(&der).unwrap();
    assert_eq!(null_struct, NullStruct);
}

#[test]
fn test_serialize_null() {
    let null_struct = NullStruct;
    let der = to_vec_der(&null_struct).unwrap();
    let expected_der: [u8; 2] = [0x05, 0x00];
    assert_eq!(&expected_der[..], &der[..]);
}

#[test]
fn test_deserialize_octet_string() {
    let der: [u8; 6] = [0x04, 0x04, 0x00, 0x01, 0x02, 0x03];
    let octet_string: Vec<u8> = deserialize_der(&der).unwrap();
    assert_eq!(octet_string, vec![0x00, 0x01, 0x02, 0x03]);
}

#[test]
fn test_serialize_octet_string() {
    let octet_string: Vec<u8> = vec![0x00, 0x01, 0x02, 0x03];
    let der = to_vec_der(&octet_string).unwrap();
    let expected_der: [u8; 6] = [0x04, 0x04, 0x00, 0x01, 0x02, 0x03];
    assert_eq!(&expected_der[..], &der);
}

// Division ::= CHOICE {
//   r-and-d [1] IMPLICIT SEQUENCE {
//     labID INTEGER,
//     currentProject IA5String,
//   }
//   unassigned [2] IMPLICIT NULL
// }
#[derive(Debug, Facet, PartialEq, Eq)]
#[repr(u8)]
enum Division {
    RAndD {
        lab_id: i64,
        current_project: IA5String,
    } = 1,
    Unassigned = 2,
}

#[test]
fn test_deserialize_choice_sequence() {
    let der: [u8; 11] = [
        0x81, 0x09, 0x02, 0x01, 0x30, 0x16, 0x04, 0x44, 0x58, 0x2D, 0x37,
    ];
    let division: Division = deserialize_der(&der).unwrap();
    assert_eq!(
        division,
        Division::RAndD {
            lab_id: 48,
            current_project: String::from("DX-7").into(),
        }
    );
}

#[test]
fn test_serialize_choice_sequence() {
    let division = Division::RAndD {
        lab_id: 48,
        current_project: String::from("DX-7").into(),
    };
    let der = to_vec_der(&division).unwrap();
    let expected_der: [u8; 11] = [
        0x81, 0x09, 0x02, 0x01, 0x30, 0x16, 0x04, 0x44, 0x58, 0x2D, 0x37,
    ];
    assert_eq!(&expected_der[..], &der[..]);
}

#[test]
fn test_deserialize_choice_null() {
    let der: [u8; 2] = [0x82, 0x00];
    let division: Division = deserialize_der(&der).unwrap();
    assert_eq!(division, Division::Unassigned);
}

#[test]
fn test_serialize_choice_null() {
    let division = Division::Unassigned;
    let der = to_vec_der(&division).unwrap();
    let expected_der: [u8; 2] = [0x82, 0x00];
    assert_eq!(&expected_der[..], &der[..]);
}

const INTEGER_TESTS: [(i64, &[u8]); 6] = [
    (0, &[0x02, 0x01, 0x00]),
    (127, &[0x02, 0x01, 0x7F]),
    (128, &[0x02, 0x02, 0x00, 0x80]),
    (256, &[0x02, 0x02, 0x01, 0x00]),
    (-128, &[0x02, 0x01, 0x80]),
    (-129, &[0x02, 0x02, 0xFF, 0x7F]),
];

#[test]
fn test_deserialize_integer_values() {
    for (value, der) in INTEGER_TESTS {
        let i: i64 = deserialize_der(der).unwrap();
        assert_eq!(i, value);
    }
}

#[test]
fn test_serialize_integer_values() {
    for (i, expected_der) in INTEGER_TESTS {
        let der = to_vec_der(&i).unwrap();
        assert_eq!(expected_der, &der[..]);
    }
}

const FLOAT_TESTS: [(f64, &[u8]); 15] = [
    (0.0, &[0x09, 0x00]),
    (f64::INFINITY, &[0x09, 0x01, 0b01000000]),
    (f64::NEG_INFINITY, &[0x09, 0x01, 0b01000001]),
    (f64::NAN, &[0x09, 0x01, 0b01000010]),
    (-0.0, &[0x09, 0x01, 0b01000011]),
    (1.0, &[0x09, 0x03, 0b10000000, 0x00, 0x01]),
    (2.0, &[0x09, 0x03, 0b10000000, 0x01, 0x01]),
    (8192.0, &[0x09, 0x03, 0b10000000, 0x0D, 0x01]),
    (
        f64::from_bits(0x4FF0000000000000),
        &[0x09, 0x04, 0b10000001, 0x01, 0x00, 0x01],
    ),
    (3.0, &[0x09, 0x03, 0b10000000, 0x00, 0x03]),
    (1.99609375, &[0x09, 0x04, 0b10000000, 0xF8, 0x01, 0xFF]),
    (15.96875, &[0x09, 0x04, 0b10000000, 0xFB, 0x01, 0xFF]),
    (f64::from_bits(0b1), &[0x09, 0x00]),
    (
        f64::from_bits(0xC00FFFFFFFFFFFFF),
        &[
            0x09, 0x09, 0b11000000, 0xCD, 0x1F, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF,
        ],
    ),
    (
        f64::from_bits(0x7FE0000000000001),
        &[
            0x09, 0x0A, 0b10000001, 0x03, 0xCB, 0x10, 0x00, 0x00, 0x00, 0x00, 0x00, 0x01,
        ],
    ),
];

#[test]
fn test_deserialize_real_values() {
    for (value, der) in FLOAT_TESTS {
        let i: f64 = deserialize_der(der).unwrap();
        if i.is_nan() && value.is_nan() {
            continue;
        }
        if value.is_subnormal() && i == 0.0 {
            continue;
        }
        assert_eq!(i, value);
    }
}

#[test]
fn test_serialize_real_values() {
    for (i, expected_der) in FLOAT_TESTS {
        let der = to_vec_der(&i).unwrap();
        assert_eq!(expected_der, &der[..]);
    }
}
