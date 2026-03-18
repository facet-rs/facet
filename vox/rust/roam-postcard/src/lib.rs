#![deny(unsafe_code)]

#[allow(unsafe_code)]
pub mod raw;

pub mod decode;
pub mod deserialize;
pub mod encode;
pub mod error;
pub mod plan;
pub mod scatter;
pub mod serialize;

pub use deserialize::{
    deserialize_into, from_slice, from_slice_borrowed, from_slice_borrowed_with_plan,
    from_slice_with_plan,
};
pub use error::{DeserializeError, SerializeError, TranslationError, TranslationErrorKind};
pub use plan::{
    EnumTranslationPlan, FieldOp, PlanInput, SchemaSet, TranslationPlan, build_identity_plan,
    build_plan,
};
pub use raw::opaque_encoded_borrowed;
pub use scatter::{ScatterPlan, Segment, peek_to_scatter_plan};
pub use serialize::to_vec;

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;

    #[test]
    fn round_trip_u32() {
        let bytes = to_vec(&42u32).unwrap();
        let result: u32 = from_slice(&bytes).unwrap();
        assert_eq!(result, 42);
    }

    #[test]
    fn round_trip_bool() {
        let bytes = to_vec(&true).unwrap();
        let result: bool = from_slice(&bytes).unwrap();
        assert!(result);

        let bytes = to_vec(&false).unwrap();
        let result: bool = from_slice(&bytes).unwrap();
        assert!(!result);
    }

    #[test]
    fn round_trip_string() {
        let s = "hello world".to_string();
        let bytes = to_vec(&s).unwrap();
        let result: String = from_slice(&bytes).unwrap();
        assert_eq!(result, "hello world");
    }

    #[test]
    fn round_trip_empty_string() {
        let s = String::new();
        let bytes = to_vec(&s).unwrap();
        let result: String = from_slice(&bytes).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn round_trip_f64() {
        let v = std::f64::consts::PI;
        let bytes = to_vec(&v).unwrap();
        let result: f64 = from_slice(&bytes).unwrap();
        assert_eq!(result, v);
    }

    #[test]
    fn round_trip_negative_i32() {
        let v: i32 = -12345;
        let bytes = to_vec(&v).unwrap();
        let result: i32 = from_slice(&bytes).unwrap();
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
        let result: Point = from_slice(&bytes).unwrap();
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
            let result: Color = from_slice(&bytes).unwrap();
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
            Shape::Circle(std::f64::consts::PI),
            Shape::Rect { w: 10.0, h: 20.0 },
            Shape::Empty,
        ];

        for shape in shapes {
            let bytes = to_vec(&shape).unwrap();
            let result: Shape = from_slice(&bytes).unwrap();
            assert_eq!(result, shape);
        }
    }

    #[test]
    fn round_trip_vec() {
        let v: Vec<u32> = vec![1, 2, 3, 100, 0];
        let bytes = to_vec(&v).unwrap();
        let result: Vec<u32> = from_slice(&bytes).unwrap();
        assert_eq!(result, v);
    }

    #[test]
    fn round_trip_vec_u8() {
        let v: Vec<u8> = vec![0xFF, 0x00, 0x42, 0xAB];
        let bytes = to_vec(&v).unwrap();
        let result: Vec<u8> = from_slice(&bytes).unwrap();
        assert_eq!(result, v);
    }

    #[test]
    fn round_trip_option() {
        let some: Option<u32> = Some(42);
        let none: Option<u32> = None;

        let bytes = to_vec(&some).unwrap();
        let result: Option<u32> = from_slice(&bytes).unwrap();
        assert_eq!(result, some);

        let bytes = to_vec(&none).unwrap();
        let result: Option<u32> = from_slice(&bytes).unwrap();
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
        let result: Outer = from_slice(&bytes).unwrap();
        assert_eq!(result, val);
    }

    #[test]
    fn round_trip_tuple() {
        let val: (u32, String, bool) = (42, "hello".to_string(), true);
        let bytes = to_vec(&val).unwrap();
        let result: (u32, String, bool) = from_slice(&bytes).unwrap();
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
        let result: Point = from_slice(&theirs).unwrap();
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

    #[test]
    fn scatter_plan_matches_to_vec() {
        #[derive(Facet, Debug, PartialEq)]
        struct Msg {
            id: u32,
            name: String,
            tags: Vec<String>,
        }

        let val = Msg {
            id: 42,
            name: "hello".to_string(),
            tags: vec!["a".into(), "bb".into()],
        };

        let direct = to_vec(&val).unwrap();

        let peek = facet_reflect::Peek::new(&val);
        let plan = peek_to_scatter_plan(peek).unwrap();
        assert_eq!(plan.total_size(), direct.len());

        let mut scattered = vec![0u8; plan.total_size()];
        plan.write_into(&mut scattered);
        assert_eq!(scattered, direct);
    }

    #[test]
    fn scatter_plan_segment_structure() {
        #[derive(Facet)]
        struct Borrowed<'a> {
            id: u32,
            name: &'a str,
            data: &'a [u8],
        }

        let val = Borrowed {
            id: 42,
            name: "hello",
            data: &[1, 2, 3, 4, 5],
        };

        let peek = facet_reflect::Peek::new(&val);
        let plan = peek_to_scatter_plan(peek).unwrap();

        // Verify output matches to_vec
        let direct = to_vec(&val).unwrap();
        let mut scattered = vec![0u8; plan.total_size()];
        plan.write_into(&mut scattered);
        assert_eq!(scattered, direct);

        // Check segment structure: staged bytes should merge when contiguous,
        // and borrowed fields should appear as Reference segments.
        let segments = plan.segments();
        let staged_count = segments
            .iter()
            .filter(|s| matches!(s, Segment::Staged { .. }))
            .count();
        let ref_count = segments
            .iter()
            .filter(|s| matches!(s, Segment::Reference { .. }))
            .count();

        // Both &str and &[u8] produce Reference segments (zero-copy).
        assert_eq!(
            ref_count, 2,
            "expected 2 reference segments for &str and &[u8]"
        );
        // Staged segments merge when contiguous. References break the merge chain:
        // staged(id + name len), ref(name), staged(data len), ref(data) = 2 staged.
        assert_eq!(staged_count, 2, "expected 2 staged segments");

        eprintln!("segments ({} total):", segments.len());
        for (i, seg) in segments.iter().enumerate() {
            match seg {
                Segment::Staged { offset, len } => {
                    eprintln!("  [{i}] Staged: offset={offset}, len={len}");
                }
                Segment::Reference { bytes } => {
                    eprintln!("  [{i}] Reference: len={}", bytes.len());
                }
            }
        }
    }

    #[test]
    fn round_trip_borrowed_identity() {
        #[derive(Facet, Debug, PartialEq)]
        struct Msg {
            id: u32,
            name: String,
        }

        let val = Msg {
            id: 42,
            name: "hello".to_string(),
        };
        let bytes = to_vec(&val).unwrap();
        let result: Msg = from_slice_borrowed(&bytes).unwrap();
        assert_eq!(result, val);
    }

    // ---- Translation plan tests ----

    #[derive(Debug)]
    struct PlanResult {
        plan: TranslationPlan,
        remote: SchemaSet,
        #[allow(dead_code)]
        local: SchemaSet,
    }

    /// Helper: build a translation plan from remote and local shapes.
    fn plan_for(
        remote_shape: &'static facet_core::Shape,
        local_shape: &'static facet_core::Shape,
    ) -> Result<PlanResult, error::TranslationError> {
        let remote = SchemaSet::from_extracted(roam_types::extract_schemas(remote_shape));
        let local = SchemaSet::from_extracted(roam_types::extract_schemas(local_shape));
        let plan = build_plan(&PlanInput {
            remote: &remote,
            local: &local,
        })?;
        Ok(PlanResult {
            plan,
            remote,
            local,
        })
    }

    // r[verify schema.translation.skip-unknown]
    #[test]
    fn translation_remote_has_extra_field() {
        // Remote has fields [x, y, z], local only has [x, z]
        mod remote {
            use facet::Facet;
            #[derive(Facet, Debug)]
            pub struct Point {
                pub x: f64,
                pub y: f64,
                pub z: f64,
            }
        }

        mod local {
            use facet::Facet;
            #[derive(Facet, Debug, PartialEq)]
            pub struct Point {
                pub x: f64,
                pub z: f64,
            }
        }

        let r = plan_for(remote::Point::SHAPE, local::Point::SHAPE).unwrap();

        // Serialize with remote type
        let remote_val = remote::Point {
            x: 1.0,
            y: 2.0,
            z: 3.0,
        };
        let bytes = to_vec(&remote_val).unwrap();

        // Deserialize with plan into local type — y should be skipped
        let local_val: local::Point =
            from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(local_val, local::Point { x: 1.0, z: 3.0 });
    }

    // r[verify schema.translation.fill-defaults]
    #[test]
    fn translation_remote_missing_field_with_default() {
        // Remote has [x], local has [x, y] where y has a default
        mod remote {
            use facet::Facet;
            #[derive(Facet, Debug)]
            pub struct Point {
                pub x: f64,
            }
        }

        mod local {
            use facet::Facet;
            #[derive(Facet, Debug, PartialEq)]
            pub struct Point {
                pub x: f64,
                #[facet(default)]
                pub y: f64,
            }
        }

        let r = plan_for(remote::Point::SHAPE, local::Point::SHAPE).unwrap();

        let remote_val = remote::Point { x: 42.0 };
        let bytes = to_vec(&remote_val).unwrap();

        let local_val: local::Point =
            from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(local_val, local::Point { x: 42.0, y: 0.0 });
    }

    // r[verify schema.translation.field-matching]
    // r[verify schema.errors.early-detection]
    // r[verify schema.errors.missing-required]
    #[test]
    fn translation_missing_required_field_errors() {
        // Remote has [x], local has [x, y] where y is required (no default)
        mod remote {
            use facet::Facet;
            #[derive(Facet, Debug)]
            pub struct Point {
                pub x: f64,
            }
        }

        mod local {
            use facet::Facet;
            #[derive(Facet, Debug)]
            pub struct Point {
                pub x: f64,
                pub y: f64,
            }
        }

        let result = plan_for(remote::Point::SHAPE, local::Point::SHAPE);

        assert!(result.is_err());
        let err = result.unwrap_err();
        match &*err.kind {
            error::TranslationErrorKind::MissingRequiredField { field, .. } => {
                assert_eq!(field.name, "y");
            }
            other => panic!("expected MissingRequiredField, got {other:?}"),
        }
        // Verify error message includes useful context
        let msg = err.to_string();
        assert!(msg.contains("y"), "error should name the field: {msg}");
        assert!(
            msg.contains("required field") && msg.contains("missing"),
            "error should say missing required: {msg}"
        );
    }

    // r[verify schema.translation.reorder]
    #[test]
    fn translation_field_reorder() {
        // Remote has [b, a], local has [a, b] — different field order
        mod remote {
            use facet::Facet;
            #[derive(Facet, Debug)]
            pub struct Pair {
                pub b: String,
                pub a: u32,
            }
        }

        mod local {
            use facet::Facet;
            #[derive(Facet, Debug, PartialEq)]
            pub struct Pair {
                pub a: u32,
                pub b: String,
            }
        }

        let r = plan_for(remote::Pair::SHAPE, local::Pair::SHAPE).unwrap();

        let remote_val = remote::Pair {
            b: "hello".to_string(),
            a: 42,
        };
        let bytes = to_vec(&remote_val).unwrap();

        let local_val: local::Pair =
            from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(
            local_val,
            local::Pair {
                a: 42,
                b: "hello".to_string()
            }
        );
    }

    // r[verify schema.translation.skip-unknown]
    #[test]
    fn translation_skip_complex_field() {
        // Remote has an extra Vec<String> field that local doesn't have
        mod remote {
            use facet::Facet;
            #[derive(Facet, Debug)]
            pub struct Msg {
                pub id: u32,
                pub tags: Vec<String>,
                pub name: String,
            }
        }

        mod local {
            use facet::Facet;
            #[derive(Facet, Debug, PartialEq)]
            pub struct Msg {
                pub id: u32,
                pub name: String,
            }
        }

        let r = plan_for(remote::Msg::SHAPE, local::Msg::SHAPE).unwrap();

        let remote_val = remote::Msg {
            id: 99,
            tags: vec!["a".into(), "bb".into(), "ccc".into()],
            name: "test".to_string(),
        };
        let bytes = to_vec(&remote_val).unwrap();

        let local_val: local::Msg =
            from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(
            local_val,
            local::Msg {
                id: 99,
                name: "test".to_string()
            }
        );
    }

    // r[verify schema.translation.serialization-unchanged]
    // r[verify schema.translation.type-compat]
    #[test]
    fn translation_identity_plan_matches_direct() {
        // Identity plan should produce the same result as from_slice
        #[derive(Facet, Debug, PartialEq)]
        struct Point {
            x: f64,
            y: f64,
        }

        let val = Point { x: 1.0, y: 2.0 };
        let bytes = to_vec(&val).unwrap();

        let direct: Point = from_slice(&bytes).unwrap();

        let r = plan_for(Point::SHAPE, Point::SHAPE).unwrap();
        let translated: Point = from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();

        assert_eq!(direct, translated);
    }

    #[test]
    fn translation_combined_add_remove_reorder() {
        // Remote: [a, b, c] → Local: [c, d, a] (b removed, d added with default, c/a reordered)
        mod remote {
            use facet::Facet;
            #[derive(Facet, Debug)]
            pub struct Combo {
                pub a: u32,
                pub b: String,
                pub c: bool,
            }
        }

        mod local {
            use facet::Facet;
            #[derive(Facet, Debug, PartialEq)]
            pub struct Combo {
                pub c: bool,
                #[facet(default)]
                pub d: u64,
                pub a: u32,
            }
        }

        let r = plan_for(remote::Combo::SHAPE, local::Combo::SHAPE).unwrap();

        let remote_val = remote::Combo {
            a: 42,
            b: "dropped".to_string(),
            c: true,
        };
        let bytes = to_vec(&remote_val).unwrap();

        let local_val: local::Combo =
            from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(
            local_val,
            local::Combo {
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
        mod remote {
            use facet::Facet;
            #[derive(Facet, Debug)]
            pub struct Outer {
                pub inner: u32,
            }
        }

        // local::Outer has a required field 'missing' that remote doesn't have
        mod local {
            use facet::Facet;
            #[derive(Facet, Debug)]
            pub struct Outer {
                pub inner: u32,
                pub missing: String,
            }
        }

        let err = plan_for(remote::Outer::SHAPE, local::Outer::SHAPE).unwrap_err();

        let msg = err.to_string();
        // Should include local type name
        assert!(
            msg.contains("Outer"),
            "error should include local type name: {msg}"
        );
        // Should include missing field name
        assert!(
            msg.contains("missing"),
            "error should include field name: {msg}"
        );
        // Should include "required field" and "missing"
        assert!(
            msg.contains("required field") && msg.contains("missing"),
            "error should describe the incompatibility: {msg}"
        );
        // Error should have useful context
        assert!(
            !format!("{err}").is_empty(),
            "error Display should produce output"
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

        let err = plan_for(RemoteStruct::SHAPE, <u32 as Facet>::SHAPE).unwrap_err();

        let msg = err.to_string();
        assert!(msg.contains("struct"), "error should mention struct: {msg}");
    }

    // r[verify schema.translation.enum]
    // r[verify schema.translation.enum.missing-variant]
    #[test]
    fn translation_enum_variant_added() {
        // Remote has [A, B], local has [A, B, C] — C never sent, that's fine
        mod remote {
            use facet::Facet;
            #[derive(Facet, Debug)]
            #[repr(u8)]
            pub enum Cmd {
                Start,
                Stop,
            }
        }

        mod local {
            use facet::Facet;
            #[derive(Facet, Debug, PartialEq)]
            #[repr(u8)]
            pub enum Cmd {
                Start,
                Stop,
                Restart,
            }
        }

        let r = plan_for(remote::Cmd::SHAPE, local::Cmd::SHAPE).unwrap();

        let bytes = to_vec(&remote::Cmd::Stop).unwrap();
        let result: local::Cmd = from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(result, local::Cmd::Stop);
    }

    // r[verify schema.translation.enum.unknown-variant]
    // r[verify schema.errors.unknown-variant-runtime]
    #[test]
    fn translation_enum_unknown_variant_errors_at_runtime() {
        // Remote has [A, B, C], local has [A, B] — receiving C should error
        mod remote {
            use facet::Facet;
            #[derive(Facet, Debug)]
            #[repr(u8)]
            pub enum Cmd {
                Start,
                Stop,
                Restart,
            }
        }

        mod local {
            use facet::Facet;
            #[derive(Facet, Debug, PartialEq)]
            #[repr(u8)]
            pub enum Cmd {
                Start,
                Stop,
            }
        }

        let r = plan_for(remote::Cmd::SHAPE, local::Cmd::SHAPE).unwrap();

        // Sending Start/Stop works
        let bytes = to_vec(&remote::Cmd::Start).unwrap();
        let result: local::Cmd = from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(result, local::Cmd::Start);

        // Sending Restart (index 2) should fail at runtime
        let bytes = to_vec(&remote::Cmd::Restart).unwrap();
        let result: Result<local::Cmd, _> =
            from_slice_with_plan(&bytes, &r.plan, &r.remote.registry);
        assert!(result.is_err());
    }

    #[test]
    fn translation_result_ok_round_trip() {
        type T = Result<String, u32>;
        let r = plan_for(T::SHAPE, T::SHAPE).unwrap();
        let bytes = to_vec(&Ok::<_, u32>("hello".to_string())).unwrap();
        let result: T = from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(result, Ok("hello".to_string()));
    }

    #[test]
    fn translation_result_err_round_trip() {
        type T = Result<String, u32>;
        let r = plan_for(T::SHAPE, T::SHAPE).unwrap();
        let bytes = to_vec(&Err::<String, _>(42u32)).unwrap();
        let result: T = from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(result, Err(42));
    }

    #[test]
    fn translation_result_with_enum_error() {
        #[derive(Facet, Debug, PartialEq)]
        #[repr(u8)]
        enum MyError {
            NotFound,
            Forbidden(String),
        }

        type T = Result<u32, MyError>;
        let r = plan_for(T::SHAPE, T::SHAPE).unwrap();

        let result: T = from_slice_with_plan(
            &to_vec(&Ok::<u32, MyError>(42u32)).unwrap(),
            &r.plan,
            &r.remote.registry,
        )
        .unwrap();
        assert_eq!(result, Ok(42));

        let result: T = from_slice_with_plan(
            &to_vec(&Err::<u32, _>(MyError::Forbidden("nope".into()))).unwrap(),
            &r.plan,
            &r.remote.registry,
        )
        .unwrap();
        assert_eq!(result, Err(MyError::Forbidden("nope".into())));
    }

    #[test]
    fn translation_result_with_roam_error_shape() {
        use roam_types::RoamError;
        use std::convert::Infallible;

        type T = Result<String, RoamError<Infallible>>;
        let r = plan_for(T::SHAPE, T::SHAPE).unwrap();
        let bytes = to_vec(&Ok::<_, RoamError<Infallible>>("hello".to_string())).unwrap();
        let result: T = from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(result.unwrap(), "hello");
    }

    #[test]
    fn translation_result_with_roam_error_err_variant() {
        use roam_types::RoamError;
        use std::convert::Infallible;

        type T = Result<String, RoamError<Infallible>>;
        let r = plan_for(T::SHAPE, T::SHAPE).unwrap();
        let bytes = to_vec(&Err::<String, _>(RoamError::<Infallible>::InvalidPayload(
            "bad data".into(),
        )))
        .unwrap();
        let result: T = from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        match result {
            Err(RoamError::InvalidPayload(msg)) => assert_eq!(msg, "bad data"),
            other => panic!("expected InvalidPayload, got {other:?}"),
        }
    }

    #[test]
    fn translation_nested_struct_in_result() {
        #[derive(Facet, Debug, PartialEq)]
        struct Payload {
            id: u32,
            name: String,
        }

        type T = Result<Payload, u32>;
        let r = plan_for(T::SHAPE, T::SHAPE).unwrap();
        let bytes = to_vec(&Ok::<_, u32>(Payload {
            id: 1,
            name: "test".into(),
        }))
        .unwrap();
        let result: T = from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(
            result,
            Ok(Payload {
                id: 1,
                name: "test".into()
            })
        );
    }

    #[test]
    fn translation_enum_newtype_variant() {
        #[derive(Facet, Debug, PartialEq)]
        #[repr(u8)]
        enum Message {
            Text(String),
            Number(i64),
            Data(Vec<u8>),
        }

        let r = plan_for(Message::SHAPE, Message::SHAPE).unwrap();
        for val in [
            Message::Text("hello".into()),
            Message::Number(42),
            Message::Data(vec![1, 2, 3]),
        ] {
            let bytes = to_vec(&val).unwrap();
            let result: Message =
                from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
            assert_eq!(result, val);
        }
    }

    #[test]
    fn opaque_round_trip() {
        use facet::{
            Facet, FacetOpaqueAdapter, OpaqueDeserialize, OpaqueSerialize, PtrConst, Shape,
        };
        use std::marker::PhantomData;

        // A minimal opaque adapter for testing.
        #[derive(Debug, Facet)]
        #[repr(u8)]
        #[facet(opaque = TestPayloadAdapter, traits(Debug))]
        enum TestPayload<'a> {
            Outgoing {
                ptr: PtrConst,
                shape: &'static Shape,
                _lt: PhantomData<&'a ()>,
            },
            Incoming(&'a [u8]),
        }

        struct TestPayloadAdapter;

        impl FacetOpaqueAdapter for TestPayloadAdapter {
            type Error = String;
            type SendValue<'a> = TestPayload<'a>;
            type RecvValue<'de> = TestPayload<'de>;

            fn serialize_map(value: &Self::SendValue<'_>) -> OpaqueSerialize {
                match value {
                    TestPayload::Outgoing { ptr, shape, .. } => {
                        OpaqueSerialize { ptr: *ptr, shape }
                    }
                    TestPayload::Incoming(bytes) => crate::opaque_encoded_borrowed(bytes),
                }
            }

            fn deserialize_build<'de>(
                input: OpaqueDeserialize<'de>,
            ) -> Result<Self::RecvValue<'de>, Self::Error> {
                match input {
                    OpaqueDeserialize::Borrowed(bytes) => Ok(TestPayload::Incoming(bytes)),
                    OpaqueDeserialize::Owned(_) => Err("must be borrowed".into()),
                }
            }
        }

        // A struct with an opaque field, mimicking RequestCall.
        #[derive(Debug, Facet)]
        struct TestCall<'a> {
            id: u32,
            payload: TestPayload<'a>,
        }

        // 1. Serialize with Outgoing payload (non-passthrough)
        let val: u32 = 42;
        let call = TestCall {
            id: 7,
            payload: TestPayload::Outgoing {
                ptr: PtrConst::new((&val as *const u32).cast::<u8>()),
                shape: <u32 as Facet>::SHAPE,
                _lt: PhantomData,
            },
        };

        let our_bytes = to_vec(&call).unwrap();
        eprintln!("outgoing - roam:  {:02x?}", our_bytes);
        // Opaque values use u32le length prefix (not varint), so our encoding
        // intentionally differs from facet-postcard here.

        // 2. Deserialize back (payload becomes Incoming)
        let round_tripped: TestCall<'_> = from_slice_borrowed(&our_bytes).unwrap();
        let payload_bytes = match round_tripped.payload {
            TestPayload::Incoming(b) => b,
            _ => panic!("expected incoming"),
        };
        let result: u32 = from_slice(payload_bytes).unwrap();
        assert_eq!(result, 42, "payload should contain 42");

        // 3. Re-serialize with Incoming payload (passthrough)
        let reserialized = to_vec(&round_tripped).unwrap();
        eprintln!("incoming - roam:  {:02x?}", reserialized);
        assert_eq!(
            reserialized, our_bytes,
            "re-serialized incoming must match original outgoing"
        );
    }

    #[test]
    fn opaque_vec_u8_round_trip() {
        use facet::{
            Facet, FacetOpaqueAdapter, OpaqueDeserialize, OpaqueSerialize, PtrConst, Shape,
        };
        use std::marker::PhantomData;

        #[derive(Debug, Facet)]
        #[repr(u8)]
        #[facet(opaque = TestPayloadAdapter2, traits(Debug))]
        enum TestPayload2<'a> {
            Outgoing {
                ptr: PtrConst,
                shape: &'static Shape,
                _lt: PhantomData<&'a ()>,
            },
            Incoming(&'a [u8]),
        }

        struct TestPayloadAdapter2;

        impl FacetOpaqueAdapter for TestPayloadAdapter2 {
            type Error = String;
            type SendValue<'a> = TestPayload2<'a>;
            type RecvValue<'de> = TestPayload2<'de>;

            fn serialize_map(value: &Self::SendValue<'_>) -> OpaqueSerialize {
                match value {
                    TestPayload2::Outgoing { ptr, shape, .. } => {
                        OpaqueSerialize { ptr: *ptr, shape }
                    }
                    TestPayload2::Incoming(bytes) => crate::opaque_encoded_borrowed(bytes),
                }
            }

            fn deserialize_build<'de>(
                input: OpaqueDeserialize<'de>,
            ) -> Result<Self::RecvValue<'de>, Self::Error> {
                match input {
                    OpaqueDeserialize::Borrowed(bytes) => Ok(TestPayload2::Incoming(bytes)),
                    OpaqueDeserialize::Owned(_) => Err("must be borrowed".into()),
                }
            }
        }

        #[derive(Debug, Facet)]
        struct TestCall2<'a> {
            id: u32,
            payload: TestPayload2<'a>,
        }

        // Test with Vec<u8> payload (like the blob stress test)
        let blob = vec![0u8; 32];
        let call = TestCall2 {
            id: 7,
            payload: TestPayload2::Outgoing {
                ptr: PtrConst::new((&blob as *const Vec<u8>).cast::<u8>()),
                shape: <Vec<u8> as Facet>::SHAPE,
                _lt: PhantomData,
            },
        };

        // Step 1: Serialize
        let encoded = to_vec(&call).unwrap();
        eprintln!(
            "encoded ({} bytes): {:02x?}",
            encoded.len(),
            &encoded[..encoded.len().min(40)]
        );

        // Step 2: Deserialize back (payload becomes Incoming)
        let round_tripped: TestCall2<'_> = from_slice_borrowed(&encoded).unwrap();
        let payload_bytes = match &round_tripped.payload {
            TestPayload2::Incoming(b) => *b,
            _ => panic!("expected incoming"),
        };
        eprintln!(
            "payload_bytes ({} bytes): {:02x?}",
            payload_bytes.len(),
            &payload_bytes[..payload_bytes.len().min(40)]
        );

        // Step 3: Deserialize the payload as Vec<u8>
        let result: Vec<u8> = from_slice(payload_bytes).unwrap();
        assert_eq!(result.len(), 32, "should get back 32 bytes");
        assert_eq!(result, blob, "round-trip should preserve Vec<u8> content");

        // Step 4: Re-serialize with Incoming payload (passthrough)
        let reserialized = to_vec(&round_tripped).unwrap();
        assert_eq!(reserialized, encoded, "re-serialized must match original");

        // Step 5: Full round-trip again
        let final_trip: TestCall2<'_> = from_slice_borrowed(&reserialized).unwrap();
        let final_bytes = match &final_trip.payload {
            TestPayload2::Incoming(b) => *b,
            _ => panic!("expected incoming"),
        };
        let final_result: Vec<u8> = from_slice(final_bytes).unwrap();
        assert_eq!(
            final_result, blob,
            "double round-trip should preserve content"
        );
    }
}
