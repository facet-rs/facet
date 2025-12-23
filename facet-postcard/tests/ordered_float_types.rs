use facet::Facet;
use facet_postcard::{from_slice, to_vec};
use ordered_float::{NotNan, OrderedFloat};
use std::f32;
use std::f64;

#[test]
fn test_ordered_float_f32_roundtrip() {
    facet_testhelpers::setup();

    let val = OrderedFloat(f32::consts::PI);
    let bytes = to_vec(&val).unwrap();
    let decoded: OrderedFloat<f32> = from_slice(&bytes).unwrap();
    assert_eq!(val, decoded);
}

#[test]
fn test_ordered_float_f64_roundtrip() {
    facet_testhelpers::setup();

    let val = OrderedFloat(f64::consts::PI);
    let bytes = to_vec(&val).unwrap();
    let decoded: OrderedFloat<f64> = from_slice(&bytes).unwrap();
    assert_eq!(val, decoded);
}

#[test]
fn test_ordered_float_zero() {
    facet_testhelpers::setup();

    let val = OrderedFloat(0.0f32);
    let bytes = to_vec(&val).unwrap();
    let decoded: OrderedFloat<f32> = from_slice(&bytes).unwrap();
    assert_eq!(val, decoded);
}

#[test]
fn test_ordered_float_negative() {
    facet_testhelpers::setup();

    let val = OrderedFloat(-123.456f64);
    let bytes = to_vec(&val).unwrap();
    let decoded: OrderedFloat<f64> = from_slice(&bytes).unwrap();
    assert_eq!(val, decoded);
}

#[test]
fn test_ordered_float_inf() {
    facet_testhelpers::setup();

    let val = OrderedFloat(f32::INFINITY);
    let bytes = to_vec(&val).unwrap();
    let decoded: OrderedFloat<f32> = from_slice(&bytes).unwrap();
    assert_eq!(val, decoded);
}

#[test]
fn test_ordered_float_neg_inf() {
    facet_testhelpers::setup();

    let val = OrderedFloat(f64::NEG_INFINITY);
    let bytes = to_vec(&val).unwrap();
    let decoded: OrderedFloat<f64> = from_slice(&bytes).unwrap();
    assert_eq!(val, decoded);
}

#[test]
fn test_ordered_float_in_struct() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct Point {
        x: OrderedFloat<f32>,
        y: OrderedFloat<f32>,
    }

    let original = Point {
        x: OrderedFloat(10.5),
        y: OrderedFloat(-5.25),
    };

    let bytes = to_vec(&original).unwrap();
    let decoded: Point = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_ordered_float_in_option() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct WithOptional {
        value: Option<OrderedFloat<f64>>,
    }

    // Test Some variant
    let original = WithOptional {
        value: Some(OrderedFloat(42.42)),
    };
    let bytes = to_vec(&original).unwrap();
    let decoded: WithOptional = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);

    // Test None variant
    let original = WithOptional { value: None };
    let bytes = to_vec(&original).unwrap();
    let decoded: WithOptional = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_ordered_float_in_vec() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct Measurements {
        values: Vec<OrderedFloat<f64>>,
    }

    let original = Measurements {
        values: vec![OrderedFloat(1.1), OrderedFloat(2.2), OrderedFloat(3.3)],
    };

    let bytes = to_vec(&original).unwrap();
    let decoded: Measurements = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_notnan_f32_roundtrip() {
    facet_testhelpers::setup();

    let val = NotNan::new(f32::consts::PI).unwrap();
    let bytes = to_vec(&val).unwrap();
    let decoded: NotNan<f32> = from_slice(&bytes).unwrap();
    assert_eq!(val, decoded);
}

#[test]
fn test_notnan_f64_roundtrip() {
    facet_testhelpers::setup();

    let val = NotNan::new(f64::consts::PI).unwrap();
    let bytes = to_vec(&val).unwrap();
    let decoded: NotNan<f64> = from_slice(&bytes).unwrap();
    assert_eq!(val, decoded);
}

#[test]
fn test_notnan_zero() {
    facet_testhelpers::setup();

    let val = NotNan::new(0.0f32).unwrap();
    let bytes = to_vec(&val).unwrap();
    let decoded: NotNan<f32> = from_slice(&bytes).unwrap();
    assert_eq!(val, decoded);
}

#[test]
fn test_notnan_negative() {
    facet_testhelpers::setup();

    let val = NotNan::new(-123.456f64).unwrap();
    let bytes = to_vec(&val).unwrap();
    let decoded: NotNan<f64> = from_slice(&bytes).unwrap();
    assert_eq!(val, decoded);
}

#[test]
fn test_notnan_in_struct() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct SafePoint {
        x: NotNan<f32>,
        y: NotNan<f32>,
    }

    let original = SafePoint {
        x: NotNan::new(10.5).unwrap(),
        y: NotNan::new(-5.25).unwrap(),
    };

    let bytes = to_vec(&original).unwrap();
    let decoded: SafePoint = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_notnan_in_option() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct WithOptional {
        value: Option<NotNan<f64>>,
    }

    // Test Some variant
    let original = WithOptional {
        value: Some(NotNan::new(42.42).unwrap()),
    };
    let bytes = to_vec(&original).unwrap();
    let decoded: WithOptional = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);

    // Test None variant
    let original = WithOptional { value: None };
    let bytes = to_vec(&original).unwrap();
    let decoded: WithOptional = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_notnan_in_vec() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct SafeMeasurements {
        values: Vec<NotNan<f64>>,
    }

    let original = SafeMeasurements {
        values: vec![
            NotNan::new(1.1).unwrap(),
            NotNan::new(2.2).unwrap(),
            NotNan::new(3.3).unwrap(),
        ],
    };

    let bytes = to_vec(&original).unwrap();
    let decoded: SafeMeasurements = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_ordered_float_and_notnan_together() {
    facet_testhelpers::setup();

    #[derive(Facet, PartialEq, Debug)]
    struct Mixed {
        ordered: OrderedFloat<f32>,
        not_nan: NotNan<f32>,
    }

    let original = Mixed {
        ordered: OrderedFloat(1.23),
        not_nan: NotNan::new(4.56).unwrap(),
    };

    let bytes = to_vec(&original).unwrap();
    let decoded: Mixed = from_slice(&bytes).unwrap();
    assert_eq!(original, decoded);
}

#[test]
fn test_bare_ordered_float_f32() {
    facet_testhelpers::setup();

    let val = OrderedFloat(42.0f32);
    let bytes = to_vec(&val).unwrap();
    let decoded: OrderedFloat<f32> = from_slice(&bytes).unwrap();
    assert_eq!(val, decoded);
}

#[test]
fn test_bare_ordered_float_f64() {
    facet_testhelpers::setup();

    let val = OrderedFloat(42.0f64);
    let bytes = to_vec(&val).unwrap();
    let decoded: OrderedFloat<f64> = from_slice(&bytes).unwrap();
    assert_eq!(val, decoded);
}

#[test]
fn test_bare_notnan_f32() {
    facet_testhelpers::setup();

    let val = NotNan::new(42.0f32).unwrap();
    let bytes = to_vec(&val).unwrap();
    let decoded: NotNan<f32> = from_slice(&bytes).unwrap();
    assert_eq!(val, decoded);
}

#[test]
fn test_bare_notnan_f64() {
    facet_testhelpers::setup();

    let val = NotNan::new(42.0f64).unwrap();
    let bytes = to_vec(&val).unwrap();
    let decoded: NotNan<f64> = from_slice(&bytes).unwrap();
    assert_eq!(val, decoded);
}

#[test]
fn test_ordered_float_serialization_size_f32() {
    facet_testhelpers::setup();

    let val = OrderedFloat(f32::consts::PI);
    let bytes = to_vec(&val).unwrap();
    // Should be exactly 4 bytes (same as f32)
    assert_eq!(bytes.len(), 4);
}

#[test]
fn test_ordered_float_serialization_size_f64() {
    facet_testhelpers::setup();

    let val = OrderedFloat(f64::consts::PI);
    let bytes = to_vec(&val).unwrap();
    // Should be exactly 8 bytes (same as f64)
    assert_eq!(bytes.len(), 8);
}

#[test]
fn test_notnan_serialization_size_f32() {
    facet_testhelpers::setup();

    let val = NotNan::new(f32::consts::PI).unwrap();
    let bytes = to_vec(&val).unwrap();
    // Should be exactly 4 bytes (same as f32)
    assert_eq!(bytes.len(), 4);
}

#[test]
fn test_notnan_serialization_size_f64() {
    facet_testhelpers::setup();

    let val = NotNan::new(f64::consts::PI).unwrap();
    let bytes = to_vec(&val).unwrap();
    // Should be exactly 8 bytes (same as f64)
    assert_eq!(bytes.len(), 8);
}
