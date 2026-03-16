#![deny(unsafe_code)]

pub mod decode;
pub mod deserialize;
pub mod encode;
pub mod error;
pub mod plan;
pub mod serialize;

pub use deserialize::{from_slice, from_slice_identity};
pub use error::{DeserializeError, SerializeError, TranslationError};
pub use plan::{EnumTranslationPlan, FieldOp, TranslationPlan, build_identity_plan, build_plan};
pub use serialize::to_vec;

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;

    #[test]
    fn round_trip_u32() {
        let bytes = to_vec(&42u32).unwrap();
        let result: u32 = from_slice_identity(&bytes).unwrap();
        assert_eq!(result, 42);
    }

    #[test]
    fn round_trip_bool() {
        let bytes = to_vec(&true).unwrap();
        let result: bool = from_slice_identity(&bytes).unwrap();
        assert!(result);

        let bytes = to_vec(&false).unwrap();
        let result: bool = from_slice_identity(&bytes).unwrap();
        assert!(!result);
    }

    #[test]
    fn round_trip_string() {
        let s = "hello world".to_string();
        let bytes = to_vec(&s).unwrap();
        let result: String = from_slice_identity(&bytes).unwrap();
        assert_eq!(result, "hello world");
    }

    #[test]
    fn round_trip_empty_string() {
        let s = String::new();
        let bytes = to_vec(&s).unwrap();
        let result: String = from_slice_identity(&bytes).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn round_trip_f64() {
        let v = std::f64::consts::PI;
        let bytes = to_vec(&v).unwrap();
        let result: f64 = from_slice_identity(&bytes).unwrap();
        assert_eq!(result, v);
    }

    #[test]
    fn round_trip_negative_i32() {
        let v: i32 = -12345;
        let bytes = to_vec(&v).unwrap();
        let result: i32 = from_slice_identity(&bytes).unwrap();
        assert_eq!(result, v);
    }

    #[test]
    fn round_trip_struct() {
        #[derive(Facet, Debug, PartialEq)]
        struct Point {
            x: f64,
            y: f64,
        }

        let p = Point { x: 1.5, y: -2.5 };
        let bytes = to_vec(&p).unwrap();
        let result: Point = from_slice_identity(&bytes).unwrap();
        assert_eq!(result, p);
    }

    #[test]
    fn round_trip_enum() {
        #[derive(Facet, Debug, PartialEq)]
        #[repr(u8)]
        enum Color {
            Red,
            Green,
            Blue,
        }

        for (color, expected_disc) in [(Color::Red, 0u8), (Color::Green, 1), (Color::Blue, 2)] {
            let bytes = to_vec(&color).unwrap();
            // Varint discriminant
            assert_eq!(bytes[0], expected_disc);
            let result: Color = from_slice_identity(&bytes).unwrap();
            assert_eq!(result, color);
        }
    }

    #[test]
    fn round_trip_enum_with_payload() {
        #[derive(Facet, Debug, PartialEq)]
        #[repr(u8)]
        enum Shape {
            Circle(f64),
            Rect { w: f64, h: f64 },
            Empty,
        }

        let shapes = vec![
            Shape::Circle(3.14),
            Shape::Rect { w: 10.0, h: 20.0 },
            Shape::Empty,
        ];

        for shape in shapes {
            let bytes = to_vec(&shape).unwrap();
            let result: Shape = from_slice_identity(&bytes).unwrap();
            assert_eq!(result, shape);
        }
    }

    #[test]
    fn round_trip_vec() {
        let v: Vec<u32> = vec![1, 2, 3, 100, 0];
        let bytes = to_vec(&v).unwrap();
        let result: Vec<u32> = from_slice_identity(&bytes).unwrap();
        assert_eq!(result, v);
    }

    #[test]
    fn round_trip_vec_u8() {
        let v: Vec<u8> = vec![0xFF, 0x00, 0x42, 0xAB];
        let bytes = to_vec(&v).unwrap();
        let result: Vec<u8> = from_slice_identity(&bytes).unwrap();
        assert_eq!(result, v);
    }

    #[test]
    fn round_trip_option() {
        let some: Option<u32> = Some(42);
        let none: Option<u32> = None;

        let bytes = to_vec(&some).unwrap();
        let result: Option<u32> = from_slice_identity(&bytes).unwrap();
        assert_eq!(result, some);

        let bytes = to_vec(&none).unwrap();
        let result: Option<u32> = from_slice_identity(&bytes).unwrap();
        assert_eq!(result, none);
    }

    #[test]
    fn round_trip_nested() {
        #[derive(Facet, Debug, PartialEq)]
        struct Inner {
            value: u32,
        }

        #[derive(Facet, Debug, PartialEq)]
        struct Outer {
            name: String,
            inner: Inner,
            tags: Vec<String>,
        }

        let val = Outer {
            name: "test".to_string(),
            inner: Inner { value: 99 },
            tags: vec!["a".into(), "bb".into()],
        };

        let bytes = to_vec(&val).unwrap();
        let result: Outer = from_slice_identity(&bytes).unwrap();
        assert_eq!(result, val);
    }

    #[test]
    fn round_trip_tuple() {
        let val: (u32, String, bool) = (42, "hello".to_string(), true);
        let bytes = to_vec(&val).unwrap();
        let result: (u32, String, bool) = from_slice_identity(&bytes).unwrap();
        assert_eq!(result, val);
    }

    #[test]
    fn cross_compat_with_facet_postcard() {
        // Verify our encoding matches facet-postcard exactly
        let val: u32 = 300;
        let ours = to_vec(&val).unwrap();
        let theirs = facet_postcard::to_vec(&val).unwrap();
        assert_eq!(ours, theirs, "u32 encoding mismatch");

        let val = "hello".to_string();
        let ours = to_vec(&val).unwrap();
        let theirs = facet_postcard::to_vec(&val).unwrap();
        assert_eq!(ours, theirs, "String encoding mismatch");

        let val: i32 = -42;
        let ours = to_vec(&val).unwrap();
        let theirs = facet_postcard::to_vec(&val).unwrap();
        assert_eq!(ours, theirs, "i32 encoding mismatch");

        let val = true;
        let ours = to_vec(&val).unwrap();
        let theirs = facet_postcard::to_vec(&val).unwrap();
        assert_eq!(ours, theirs, "bool encoding mismatch");

        let val: f64 = std::f64::consts::E;
        let ours = to_vec(&val).unwrap();
        let theirs = facet_postcard::to_vec(&val).unwrap();
        assert_eq!(ours, theirs, "f64 encoding mismatch");

        let val: Vec<u32> = vec![1, 2, 3];
        let ours = to_vec(&val).unwrap();
        let theirs = facet_postcard::to_vec(&val).unwrap();
        assert_eq!(ours, theirs, "Vec<u32> encoding mismatch");

        let val: Option<u32> = Some(42);
        let ours = to_vec(&val).unwrap();
        let theirs = facet_postcard::to_vec(&val).unwrap();
        assert_eq!(ours, theirs, "Option<u32> Some encoding mismatch");

        let val: Option<u32> = None;
        let ours = to_vec(&val).unwrap();
        let theirs = facet_postcard::to_vec(&val).unwrap();
        assert_eq!(ours, theirs, "Option<u32> None encoding mismatch");
    }

    #[test]
    fn cross_compat_struct() {
        #[derive(Facet, Debug, PartialEq)]
        struct Point {
            x: f64,
            y: f64,
        }

        let val = Point { x: 1.0, y: 2.0 };
        let ours = to_vec(&val).unwrap();
        let theirs = facet_postcard::to_vec(&val).unwrap();
        assert_eq!(ours, theirs, "Point struct encoding mismatch");

        // Deserialize theirs with ours
        let result: Point = from_slice_identity(&theirs).unwrap();
        assert_eq!(result, val);
    }

    #[test]
    fn cross_compat_enum() {
        #[derive(Facet, Debug, PartialEq)]
        #[repr(u8)]
        enum Color {
            Red,
            Green,
            Blue,
        }

        for color in [Color::Red, Color::Green, Color::Blue] {
            let ours = to_vec(&color).unwrap();
            let theirs = facet_postcard::to_vec(&color).unwrap();
            assert_eq!(ours, theirs, "Color enum encoding mismatch");
        }
    }
}
