#![deny(unsafe_code)]

pub mod decode;
pub mod deserialize;
pub mod encode;
pub mod error;
pub mod plan;
pub mod serialize;

pub use deserialize::{from_slice, from_slice_identity};
pub use error::{DeserializeError, SerializeError, TranslationError, TranslationErrorKind};
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

    // ---- Translation plan tests ----

    /// Helper: extract schemas for a type and build a registry.
    fn schemas_and_registry(
        shape: &'static facet_core::Shape,
    ) -> (Vec<roam_schema::Schema>, roam_schema::SchemaRegistry) {
        let schemas = roam_schema::extract_schemas(shape);
        let registry = roam_schema::build_registry(&schemas);
        (schemas, registry)
    }

    // r[verify schema.translation.skip-unknown]
    #[test]
    fn translation_remote_has_extra_field() {
        // Remote has fields [x, y, z], local only has [x, z]
        #[derive(Facet, Debug)]
        struct RemotePoint {
            x: f64,
            y: f64,
            z: f64,
        }

        #[derive(Facet, Debug, PartialEq)]
        struct LocalPoint {
            x: f64,
            z: f64,
        }

        let (schemas, registry) = schemas_and_registry(RemotePoint::SHAPE);
        let remote_root = schemas.last().unwrap();
        let plan = build_plan(remote_root, LocalPoint::SHAPE, &registry).unwrap();

        // Serialize with remote type
        let remote_val = RemotePoint {
            x: 1.0,
            y: 2.0,
            z: 3.0,
        };
        let bytes = to_vec(&remote_val).unwrap();

        // Deserialize with plan into local type — y should be skipped
        let local_val: LocalPoint = from_slice(&bytes, &plan, &registry).unwrap();
        assert_eq!(local_val, LocalPoint { x: 1.0, z: 3.0 });
    }

    // r[verify schema.translation.fill-defaults]
    #[test]
    fn translation_remote_missing_field_with_default() {
        // Remote has [x], local has [x, y] where y has a default
        #[derive(Facet, Debug)]
        struct RemotePoint {
            x: f64,
        }

        #[derive(Facet, Debug, PartialEq)]
        struct LocalPoint {
            x: f64,
            #[facet(default)]
            y: f64,
        }

        let (schemas, registry) = schemas_and_registry(RemotePoint::SHAPE);
        let remote_root = schemas.last().unwrap();
        let plan = build_plan(remote_root, LocalPoint::SHAPE, &registry).unwrap();

        let remote_val = RemotePoint { x: 42.0 };
        let bytes = to_vec(&remote_val).unwrap();

        let local_val: LocalPoint = from_slice(&bytes, &plan, &registry).unwrap();
        assert_eq!(local_val, LocalPoint { x: 42.0, y: 0.0 });
    }

    // r[verify schema.translation.field-matching]
    // r[verify schema.errors.early-detection]
    // r[verify schema.errors.missing-required]
    #[test]
    fn translation_missing_required_field_errors() {
        // Remote has [x], local has [x, y] where y is required (no default)
        #[derive(Facet, Debug)]
        struct RemotePoint {
            x: f64,
        }

        #[derive(Facet, Debug)]
        struct LocalPoint {
            x: f64,
            y: f64,
        }

        let (schemas, registry) = schemas_and_registry(RemotePoint::SHAPE);
        let remote_root = schemas.last().unwrap();
        let result = build_plan(remote_root, LocalPoint::SHAPE, &registry);

        assert!(result.is_err());
        let err = result.unwrap_err();
        match &err.kind {
            error::TranslationErrorKind::MissingRequiredField { field_name, .. } => {
                assert_eq!(field_name, "y");
            }
            other => panic!("expected MissingRequiredField, got {other:?}"),
        }
        // Verify error message includes useful context
        let msg = err.to_string();
        assert!(msg.contains("y"), "error should name the field: {msg}");
        assert!(
            msg.contains("missing required"),
            "error should say missing required: {msg}"
        );
    }

    // r[verify schema.translation.reorder]
    #[test]
    fn translation_field_reorder() {
        // Remote has [b, a], local has [a, b] — different field order
        #[derive(Facet, Debug)]
        struct RemotePair {
            b: String,
            a: u32,
        }

        #[derive(Facet, Debug, PartialEq)]
        struct LocalPair {
            a: u32,
            b: String,
        }

        let (schemas, registry) = schemas_and_registry(RemotePair::SHAPE);
        let remote_root = schemas.last().unwrap();
        let plan = build_plan(remote_root, LocalPair::SHAPE, &registry).unwrap();

        let remote_val = RemotePair {
            b: "hello".to_string(),
            a: 42,
        };
        let bytes = to_vec(&remote_val).unwrap();

        let local_val: LocalPair = from_slice(&bytes, &plan, &registry).unwrap();
        assert_eq!(
            local_val,
            LocalPair {
                a: 42,
                b: "hello".to_string()
            }
        );
    }

    // r[verify schema.translation.skip-unknown]
    #[test]
    fn translation_skip_complex_field() {
        // Remote has an extra Vec<String> field that local doesn't have
        #[derive(Facet, Debug)]
        struct RemoteMsg {
            id: u32,
            tags: Vec<String>,
            name: String,
        }

        #[derive(Facet, Debug, PartialEq)]
        struct LocalMsg {
            id: u32,
            name: String,
        }

        let (schemas, registry) = schemas_and_registry(RemoteMsg::SHAPE);
        let remote_root = schemas.last().unwrap();
        let plan = build_plan(remote_root, LocalMsg::SHAPE, &registry).unwrap();

        let remote_val = RemoteMsg {
            id: 99,
            tags: vec!["a".into(), "bb".into(), "ccc".into()],
            name: "test".to_string(),
        };
        let bytes = to_vec(&remote_val).unwrap();

        let local_val: LocalMsg = from_slice(&bytes, &plan, &registry).unwrap();
        assert_eq!(
            local_val,
            LocalMsg {
                id: 99,
                name: "test".to_string()
            }
        );
    }

    // r[verify schema.translation.serialization-unchanged]
    // r[verify schema.translation.type-compat]
    #[test]
    fn translation_identity_plan_matches_direct() {
        // Identity plan should produce the same result as from_slice_identity
        #[derive(Facet, Debug, PartialEq)]
        struct Point {
            x: f64,
            y: f64,
        }

        let val = Point { x: 1.0, y: 2.0 };
        let bytes = to_vec(&val).unwrap();

        let direct: Point = from_slice_identity(&bytes).unwrap();

        let (schemas, registry) = schemas_and_registry(Point::SHAPE);
        let remote_root = schemas.last().unwrap();
        let plan = build_plan(remote_root, Point::SHAPE, &registry).unwrap();
        let translated: Point = from_slice(&bytes, &plan, &registry).unwrap();

        assert_eq!(direct, translated);
    }

    #[test]
    fn translation_combined_add_remove_reorder() {
        // Remote: [a, b, c] → Local: [c, d, a] (b removed, d added with default, c/a reordered)
        #[derive(Facet, Debug)]
        struct Remote {
            a: u32,
            b: String,
            c: bool,
        }

        #[derive(Facet, Debug, PartialEq)]
        struct Local {
            c: bool,
            #[facet(default)]
            d: u64,
            a: u32,
        }

        let (schemas, registry) = schemas_and_registry(Remote::SHAPE);
        let remote_root = schemas.last().unwrap();
        let plan = build_plan(remote_root, Local::SHAPE, &registry).unwrap();

        let remote_val = Remote {
            a: 42,
            b: "dropped".to_string(),
            c: true,
        };
        let bytes = to_vec(&remote_val).unwrap();

        let local_val: Local = from_slice(&bytes, &plan, &registry).unwrap();
        assert_eq!(
            local_val,
            Local {
                c: true,
                d: 0, // default
                a: 42,
            }
        );
    }

    // r[verify schema.errors.content]
    #[test]
    fn translation_error_includes_context() {
        // Verify errors include remote type ID, local type name, field name
        #[derive(Facet, Debug)]
        struct RemoteOuter {
            inner: u32,
        }

        // LocalOuter has a required field 'missing' that remote doesn't have
        #[derive(Facet, Debug)]
        struct LocalOuter {
            inner: u32,
            missing: String,
        }

        let (schemas, registry) = schemas_and_registry(RemoteOuter::SHAPE);
        let remote_root = schemas.last().unwrap();
        let err = build_plan(remote_root, LocalOuter::SHAPE, &registry).unwrap_err();

        let msg = err.to_string();
        // Should include local type name
        assert!(
            msg.contains("LocalOuter"),
            "error should include local type name: {msg}"
        );
        // Should include missing field name
        assert!(
            msg.contains("missing"),
            "error should include field name: {msg}"
        );
        // Should include "missing required"
        assert!(
            msg.contains("missing required"),
            "error should describe the incompatibility: {msg}"
        );
        // Remote type ID should be present
        assert!(
            !format!("{:?}", err.remote_type_id).is_empty(),
            "error should have a remote_type_id"
        );
    }

    // r[verify schema.errors.type-mismatch]
    #[test]
    fn translation_error_kind_mismatch() {
        // Remote is a struct, but we try to translate into a primitive shape
        #[derive(Facet, Debug)]
        struct RemoteStruct {
            x: u32,
        }

        let (schemas, registry) = schemas_and_registry(RemoteStruct::SHAPE);
        let remote_root = schemas.last().unwrap();
        let err = build_plan(remote_root, <u32 as Facet>::SHAPE, &registry).unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("struct"), "error should mention struct: {msg}");
    }

    // r[verify schema.translation.enum]
    // r[verify schema.translation.enum.missing-variant]
    #[test]
    fn translation_enum_variant_added() {
        // Remote has [A, B], local has [A, B, C] — C never sent, that's fine
        #[derive(Facet, Debug)]
        #[repr(u8)]
        enum RemoteCmd {
            Start,
            Stop,
        }

        #[derive(Facet, Debug, PartialEq)]
        #[repr(u8)]
        enum LocalCmd {
            Start,
            Stop,
            Restart,
        }

        let (schemas, registry) = schemas_and_registry(RemoteCmd::SHAPE);
        let remote_root = schemas.last().unwrap();
        let plan = build_plan(remote_root, LocalCmd::SHAPE, &registry).unwrap();

        let bytes = to_vec(&RemoteCmd::Stop).unwrap();
        let result: LocalCmd = from_slice(&bytes, &plan, &registry).unwrap();
        assert_eq!(result, LocalCmd::Stop);
    }

    // r[verify schema.translation.enum.unknown-variant]
    // r[verify schema.errors.unknown-variant-runtime]
    #[test]
    fn translation_enum_unknown_variant_errors_at_runtime() {
        // Remote has [A, B, C], local has [A, B] — receiving C should error
        #[derive(Facet, Debug)]
        #[repr(u8)]
        enum RemoteCmd {
            Start,
            Stop,
            Restart,
        }

        #[derive(Facet, Debug, PartialEq)]
        #[repr(u8)]
        enum LocalCmd {
            Start,
            Stop,
        }

        let (schemas, registry) = schemas_and_registry(RemoteCmd::SHAPE);
        let remote_root = schemas.last().unwrap();
        let plan = build_plan(remote_root, LocalCmd::SHAPE, &registry).unwrap();

        // Sending Start/Stop works
        let bytes = to_vec(&RemoteCmd::Start).unwrap();
        let result: LocalCmd = from_slice(&bytes, &plan, &registry).unwrap();
        assert_eq!(result, LocalCmd::Start);

        // Sending Restart (index 2) should fail at runtime
        let bytes = to_vec(&RemoteCmd::Restart).unwrap();
        let result: Result<LocalCmd, _> = from_slice(&bytes, &plan, &registry);
        assert!(result.is_err());
    }
}
