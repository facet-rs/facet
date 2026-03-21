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
pub use plan::{FieldOp, PlanInput, SchemaSet, TranslationPlan, build_identity_plan, build_plan};
pub use raw::opaque_encoded_borrowed;
pub use scatter::{ScatterPlan, Segment, peek_to_scatter_plan};
pub use serialize::to_vec;

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;

    /// Serialize `value` via both `to_vec` and `peek_to_scatter_plan`,
    /// assert the bytes are identical, then deserialize via `from_slice_borrowed`
    /// and assert the result equals the original.
    fn round_trip<T>(value: &T)
    where
        for<'de> T: Facet<'de> + std::fmt::Debug + PartialEq,
    {
        // Path 1: direct serialization
        let direct_bytes = to_vec(value).unwrap();

        // Path 2: scatter plan
        let peek = facet_reflect::Peek::new(value);
        let plan = peek_to_scatter_plan(peek).unwrap();
        let mut scatter_bytes = vec![0u8; plan.total_size()];
        plan.write_into(&mut scatter_bytes);

        // Both paths must produce identical bytes
        assert_eq!(
            direct_bytes, scatter_bytes,
            "to_vec and scatter plan produced different bytes for {:?}",
            value
        );

        // Also test to_io_slices produces the same content
        let io_slices = plan.to_io_slices();
        let mut io_bytes = Vec::new();
        for slice in &io_slices {
            io_bytes.extend_from_slice(slice);
        }
        assert_eq!(
            direct_bytes, io_bytes,
            "to_io_slices produced different bytes for {:?}",
            value
        );

        // Also verify staging() is accessible
        let _ = plan.staging();

        // Deserialize and assert equality
        let result: T = from_slice_borrowed(&direct_bytes).unwrap();
        assert_eq!(&result, value, "round-trip mismatch for {:?}", value);
    }

    #[test]
    fn round_trip_u32() {
        round_trip(&42u32);
    }

    #[test]
    fn round_trip_bool() {
        round_trip(&true);
        round_trip(&false);
    }

    #[test]
    fn round_trip_string() {
        round_trip(&"hello world".to_string());
    }

    #[test]
    fn round_trip_empty_string() {
        round_trip(&String::new());
    }

    #[test]
    fn round_trip_f64() {
        round_trip(&std::f64::consts::PI);
    }

    #[test]
    fn round_trip_negative_i32() {
        round_trip(&-12345i32);
    }

    #[test]
    fn round_trip_struct() {
        #[derive(Facet, Debug, PartialEq)]
        struct Point {
            x: f64,
            y: f64,
        }

        round_trip(&Point { x: 1.5, y: -2.5 });
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

        round_trip(&Color::Red);
        round_trip(&Color::Green);
        round_trip(&Color::Blue);
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

        round_trip(&Shape::Circle(std::f64::consts::PI));
        round_trip(&Shape::Rect { w: 10.0, h: 20.0 });
        round_trip(&Shape::Empty);
    }

    #[test]
    fn round_trip_vec() {
        round_trip(&vec![1u32, 2, 3, 100, 0]);
    }

    #[test]
    fn round_trip_vec_u8() {
        round_trip(&vec![0xFFu8, 0x00, 0x42, 0xAB]);
    }

    #[test]
    fn round_trip_option() {
        round_trip(&Some(42u32));
        round_trip(&None::<u32>);
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

        round_trip(&Outer {
            name: "test".to_string(),
            inner: Inner { value: 99 },
            tags: vec!["a".into(), "bb".into()],
        });
    }

    #[test]
    fn round_trip_tuple() {
        round_trip(&(42u32, "hello".to_string(), true));
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
    fn scatter_plan_small_blobs_get_staged() {
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

        // Small borrowed fields (< 4K) are copied into staging, not kept as references.
        let segments = plan.segments();
        let ref_count = segments
            .iter()
            .filter(|s| matches!(s, Segment::Reference { .. }))
            .count();
        assert_eq!(
            ref_count, 0,
            "small borrowed fields should be staged, not referenced"
        );
        // Everything merges into one staged segment.
        assert_eq!(
            segments.len(),
            1,
            "all small data should coalesce into 1 segment"
        );
    }

    #[test]
    fn scatter_plan_large_blobs_get_referenced() {
        #[derive(Facet)]
        struct BigPayload<'a> {
            id: u32,
            data: &'a [u8],
        }

        let big_blob = vec![0xABu8; 8192];
        let val = BigPayload {
            id: 1,
            data: &big_blob,
        };

        let peek = facet_reflect::Peek::new(&val);
        let plan = peek_to_scatter_plan(peek).unwrap();

        // Verify output matches to_vec
        let direct = to_vec(&val).unwrap();
        let mut scattered = vec![0u8; plan.total_size()];
        plan.write_into(&mut scattered);
        assert_eq!(scattered, direct);

        // Large blob (>= 4K) should be a Reference segment (zero-copy).
        let segments = plan.segments();
        let ref_count = segments
            .iter()
            .filter(|s| matches!(s, Segment::Reference { .. }))
            .count();
        assert_eq!(ref_count, 1, "large blob should be a Reference segment");
        // staged(id + data len), ref(data) = 2 segments total.
        assert_eq!(segments.len(), 2);
    }

    #[test]
    fn scatter_staged_segments_coalesce() {
        // A struct with no borrowed fields — all structural bytes should merge
        // into a single staged segment.
        #[derive(Facet)]
        struct AllScalar {
            a: u32,
            b: u32,
            c: bool,
            d: u64,
        }

        let val = AllScalar {
            a: 1,
            b: 2,
            c: true,
            d: 999,
        };

        let peek = facet_reflect::Peek::new(&val);
        let plan = peek_to_scatter_plan(peek).unwrap();

        let segments = plan.segments();
        assert_eq!(
            segments.len(),
            1,
            "all-scalar struct should produce exactly 1 coalesced staged segment, got {segments:?}"
        );
        assert!(
            matches!(segments[0], Segment::Staged { .. }),
            "single segment should be Staged"
        );

        // Verify output still matches to_vec
        let direct = to_vec(&val).unwrap();
        let mut scattered = vec![0u8; plan.total_size()];
        plan.write_into(&mut scattered);
        assert_eq!(scattered, direct);
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
        let remote = SchemaSet::from_extracted(
            roam_types::extract_schemas(remote_shape).expect("schema extraction"),
        );
        let local = SchemaSet::from_extracted(
            roam_types::extract_schemas(local_shape).expect("schema extraction"),
        );
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

    #[test]
    fn translation_bytes_and_list_u8_are_compatible() {
        let remote = SchemaSet::from_schemas(vec![
            roam_types::Schema {
                id: roam_types::SchemaHash(1),
                type_params: vec![],
                kind: roam_types::SchemaKind::Primitive {
                    primitive_type: roam_types::PrimitiveType::U8,
                },
            },
            roam_types::Schema {
                id: roam_types::SchemaHash(2),
                type_params: vec![],
                kind: roam_types::SchemaKind::List {
                    element: roam_types::TypeRef::concrete(roam_types::SchemaHash(1)),
                },
            },
        ]);
        let local = SchemaSet::from_extracted(
            roam_types::extract_schemas(<Vec<u8> as Facet>::SHAPE).expect("schema extraction"),
        );
        let plan = build_plan(&PlanInput {
            remote: &remote,
            local: &local,
        })
        .expect("list<u8> and bytes should be translation-compatible");
        assert!(matches!(plan, TranslationPlan::Identity));

        let bytes = to_vec(&vec![1u8, 2, 3, 4]).expect("serialize remote bytes");
        let result: Vec<u8> =
            from_slice_with_plan(&bytes, &plan, &remote.registry).expect("translate byte buffer");
        assert_eq!(result, vec![1, 2, 3, 4]);
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

    // ========================================================================
    // skip_value tests — exercise decode::skip_value for every SchemaKind
    // ========================================================================

    #[test]
    fn skip_struct_field() {
        // Remote: { id: u32, extra: Point, name: String }
        // Local:  { id: u32, name: String }
        // The Point struct must be skipped entirely.
        mod remote {
            use facet::Facet;
            #[derive(Facet, Debug)]
            pub struct Msg {
                pub id: u32,
                pub extra: Point,
                pub name: String,
            }
            #[derive(Facet, Debug)]
            pub struct Point {
                pub x: f64,
                pub y: f64,
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
        let bytes = to_vec(&remote::Msg {
            id: 1,
            extra: remote::Point {
                #[allow(clippy::approx_constant)]
                x: 3.14,
                y: 2.72,
            },
            name: "hi".into(),
        })
        .unwrap();
        let result: local::Msg = from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(
            result,
            local::Msg {
                id: 1,
                name: "hi".into()
            }
        );
    }

    #[test]
    fn skip_enum_field() {
        mod remote {
            use facet::Facet;
            #[derive(Facet, Debug)]
            pub struct Msg {
                pub id: u32,
                pub status: Status,
                pub name: String,
            }
            #[derive(Facet, Debug)]
            #[repr(u8)]
            pub enum Status {
                Active = 0,
                Inactive = 1,
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
        let bytes = to_vec(&remote::Msg {
            id: 5,
            status: remote::Status::Inactive,
            name: "test".into(),
        })
        .unwrap();
        let result: local::Msg = from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(
            result,
            local::Msg {
                id: 5,
                name: "test".into()
            }
        );
    }

    #[test]
    fn skip_option_field() {
        mod remote {
            use facet::Facet;
            #[derive(Facet, Debug)]
            pub struct Msg {
                pub id: u32,
                pub maybe: Option<String>,
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

        // Test with Some
        let r = plan_for(remote::Msg::SHAPE, local::Msg::SHAPE).unwrap();
        let bytes = to_vec(&remote::Msg {
            id: 1,
            maybe: Some("present".into()),
            name: "hi".into(),
        })
        .unwrap();
        let result: local::Msg = from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(
            result,
            local::Msg {
                id: 1,
                name: "hi".into()
            }
        );

        // Test with None
        let bytes = to_vec(&remote::Msg {
            id: 2,
            maybe: None,
            name: "bye".into(),
        })
        .unwrap();
        let result: local::Msg = from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(
            result,
            local::Msg {
                id: 2,
                name: "bye".into()
            }
        );
    }

    #[test]
    fn skip_map_field() {
        mod remote {
            use facet::Facet;
            #[derive(Facet, Debug)]
            pub struct Msg {
                pub id: u32,
                pub meta: std::collections::HashMap<String, u32>,
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
        let mut meta = std::collections::HashMap::new();
        meta.insert("a".into(), 1);
        meta.insert("b".into(), 2);
        let bytes = to_vec(&remote::Msg {
            id: 10,
            meta,
            name: "x".into(),
        })
        .unwrap();
        let result: local::Msg = from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(
            result,
            local::Msg {
                id: 10,
                name: "x".into()
            }
        );
    }

    #[test]
    fn skip_array_field() {
        mod remote {
            use facet::Facet;
            #[derive(Facet, Debug)]
            pub struct Msg {
                pub id: u32,
                pub coords: [f64; 3],
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
        let bytes = to_vec(&remote::Msg {
            id: 7,
            coords: [1.0, 2.0, 3.0],
            name: "y".into(),
        })
        .unwrap();
        let result: local::Msg = from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(
            result,
            local::Msg {
                id: 7,
                name: "y".into()
            }
        );
    }

    #[test]
    fn skip_tuple_field() {
        mod remote {
            use facet::Facet;
            #[derive(Facet, Debug)]
            pub struct Msg {
                pub id: u32,
                pub pair: (String, u64),
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
        let bytes = to_vec(&remote::Msg {
            id: 3,
            pair: ("hello".into(), 999),
            name: "z".into(),
        })
        .unwrap();
        let result: local::Msg = from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(
            result,
            local::Msg {
                id: 3,
                name: "z".into()
            }
        );
    }

    #[test]
    fn skip_enum_with_newtype_payload() {
        mod remote {
            use facet::Facet;
            #[derive(Facet, Debug)]
            pub struct Msg {
                pub id: u32,
                pub shape: Shape,
                pub name: String,
            }
            #[derive(Facet, Debug)]
            #[repr(u8)]
            #[allow(dead_code)]
            pub enum Shape {
                Circle(f64) = 0,
                Rect { w: f64, h: f64 } = 1,
                Empty = 2,
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

        // Skip a newtype variant
        let bytes = to_vec(&remote::Msg {
            id: 1,
            shape: remote::Shape::Circle(5.0),
            name: "a".into(),
        })
        .unwrap();
        let result: local::Msg = from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(
            result,
            local::Msg {
                id: 1,
                name: "a".into()
            }
        );

        // Skip a struct variant
        let bytes = to_vec(&remote::Msg {
            id: 2,
            shape: remote::Shape::Rect { w: 3.0, h: 4.0 },
            name: "b".into(),
        })
        .unwrap();
        let result: local::Msg = from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(
            result,
            local::Msg {
                id: 2,
                name: "b".into()
            }
        );

        // Skip a unit variant
        let bytes = to_vec(&remote::Msg {
            id: 3,
            shape: remote::Shape::Empty,
            name: "c".into(),
        })
        .unwrap();
        let result: local::Msg = from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(
            result,
            local::Msg {
                id: 3,
                name: "c".into()
            }
        );
    }

    // ========================================================================
    // Tuple plan tests
    // ========================================================================

    #[test]
    fn translation_tuple_identity() {
        let r = plan_for(
            <(u32, String) as Facet>::SHAPE,
            <(u32, String) as Facet>::SHAPE,
        )
        .unwrap();
        let bytes = to_vec(&(42u32, "hello".to_string())).unwrap();
        let result: (u32, String) =
            from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(result, (42, "hello".to_string()));
    }

    #[test]
    fn translation_nested_unary_tuple_identity() {
        let r = plan_for(
            <((i32, String),) as Facet>::SHAPE,
            <((i32, String),) as Facet>::SHAPE,
        )
        .unwrap();
        let bytes = to_vec(&((42i32, "hello".to_string()),)).unwrap();
        let result: ((i32, String),) =
            from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(result, ((42, "hello".to_string()),));
    }

    #[test]
    fn translation_tuple_length_mismatch_errors() {
        let result = plan_for(
            <(u32, String) as Facet>::SHAPE,
            <(u32, String, bool) as Facet>::SHAPE,
        );
        assert!(result.is_err(), "tuple length mismatch should error");
        let err = result.unwrap_err();
        assert!(
            format!("{err}").contains("tuple length"),
            "error should mention tuple length: {err}"
        );
    }

    // ========================================================================
    // Name mismatch test
    // ========================================================================

    #[test]
    fn translation_struct_name_mismatch_errors() {
        mod a {
            use facet::Facet;
            #[derive(Facet, Debug)]
            pub struct Foo {
                pub x: u32,
            }
        }
        mod b {
            use facet::Facet;
            #[derive(Facet, Debug)]
            pub struct Bar {
                pub x: u32,
            }
        }

        let result = plan_for(a::Foo::SHAPE, b::Bar::SHAPE);
        assert!(result.is_err(), "name mismatch should error");
        let err = result.unwrap_err();
        assert!(
            format!("{err}").contains("name mismatch") || format!("{err}").contains("NameMismatch"),
            "error should mention name mismatch: {err}"
        );
    }

    // ========================================================================
    // from_schemas constructor test
    // ========================================================================

    #[test]
    fn schema_set_from_schemas_works() {
        let schemas = roam_types::extract_schemas(<Vec<u32> as Facet>::SHAPE)
            .expect("schema extraction")
            .schemas;
        let set = SchemaSet::from_schemas(schemas);
        assert!(
            matches!(set.root.kind, roam_types::SchemaKind::List { .. }),
            "root should be List"
        );
    }

    // ========================================================================
    // Decode error path tests
    // ========================================================================

    #[test]
    fn decode_eof_on_empty_input() {
        let result: Result<u32, _> = from_slice(&[]);
        assert!(result.is_err());
    }

    #[test]
    fn decode_invalid_option_tag() {
        // Option is encoded as 0x00 (None) or 0x01 (Some). 0x02 is invalid.
        // Encode: struct with one Option<u32> field
        // Manually craft bytes: option tag = 0x02
        let result: Result<Option<u32>, _> = from_slice(&[0x02]);
        assert!(result.is_err());
    }

    #[test]
    fn decode_invalid_bool() {
        // Bool is 0x00 or 0x01. 0x02 is invalid.
        let result: Result<bool, _> = from_slice(&[0x02]);
        assert!(result.is_err());
    }

    // ========================================================================
    // deserialize_into with planned deserialization
    // ========================================================================

    #[test]
    fn deserialize_into_with_plan() {
        mod remote {
            use facet::Facet;
            #[derive(Facet, Debug)]
            pub struct Point {
                pub y: f64,
                pub x: f64,
            }
        }
        mod local {
            use facet::Facet;
            #[derive(Facet, Debug, PartialEq)]
            pub struct Point {
                pub x: f64,
                pub y: f64,
            }
        }

        let r = plan_for(remote::Point::SHAPE, local::Point::SHAPE).unwrap();
        let bytes = to_vec(&remote::Point { y: 2.0, x: 1.0 }).unwrap();

        // Use the deserialize_into entry point directly with a Partial
        let partial = facet_reflect::Partial::alloc_owned::<local::Point>().unwrap();
        let partial =
            deserialize_into::<false>(partial, &bytes, &r.plan, &r.remote.registry).unwrap();
        let heap = partial.build().unwrap();
        let value: local::Point = heap.materialize().unwrap();
        assert_eq!(value, local::Point { x: 1.0, y: 2.0 });
    }

    // ========================================================================
    // Enum variant mapping deserialization
    // ========================================================================

    #[test]
    fn translation_enum_struct_variant_with_plan() {
        mod remote {
            use facet::Facet;
            #[derive(Facet, Debug)]
            #[repr(u8)]
            #[allow(dead_code)]
            pub enum Shape {
                Circle { radius: f64 } = 0,
                Rect { w: f64, h: f64 } = 1,
            }
        }
        mod local {
            use facet::Facet;
            #[derive(Facet, Debug, PartialEq)]
            #[repr(u8)]
            pub enum Shape {
                Circle { radius: f64 } = 0,
                Rect { w: f64, h: f64 } = 1,
                Triangle { base: f64, height: f64 } = 2,
            }
        }

        let r = plan_for(remote::Shape::SHAPE, local::Shape::SHAPE).unwrap();

        let bytes = to_vec(&remote::Shape::Rect { w: 3.0, h: 4.0 }).unwrap();
        let result: local::Shape =
            from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(result, local::Shape::Rect { w: 3.0, h: 4.0 });
    }

    #[test]
    fn translation_enum_tuple_variant() {
        mod remote {
            use facet::Facet;
            #[derive(Facet, Debug)]
            #[repr(u8)]
            #[allow(dead_code)]
            pub enum Msg {
                Pair(u32, String) = 0,
                Single(u32) = 1,
            }
        }
        mod local {
            use facet::Facet;
            #[derive(Facet, Debug, PartialEq)]
            #[repr(u8)]
            pub enum Msg {
                Pair(u32, String) = 0,
                Single(u32) = 1,
            }
        }

        let r = plan_for(remote::Msg::SHAPE, local::Msg::SHAPE).unwrap();
        let bytes = to_vec(&remote::Msg::Pair(42, "hi".into())).unwrap();
        let result: local::Msg = from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(result, local::Msg::Pair(42, "hi".into()));
    }

    // ========================================================================
    // Round-trip tests for structural types (Array, Map, Set)
    // ========================================================================

    #[test]
    fn round_trip_array() {
        round_trip(&[1u32, 2, 3, 4]);
    }

    #[test]
    fn round_trip_hashmap() {
        let mut val = std::collections::HashMap::new();
        val.insert("one".to_string(), 1u32);
        round_trip(&val);
    }

    #[test]
    fn round_trip_btreemap() {
        let mut val = std::collections::BTreeMap::new();
        val.insert("alpha".to_string(), 1u32);
        val.insert("beta".to_string(), 2);
        val.insert("gamma".to_string(), 3);
        round_trip(&val);
    }

    #[test]
    fn round_trip_unit_struct() {
        #[derive(Facet, Debug, PartialEq)]
        struct Empty;

        round_trip(&Empty);
    }

    // ========================================================================
    // Planned deserialization with container-type fields
    // ========================================================================

    #[test]
    fn translation_struct_with_array_field() {
        mod remote {
            use facet::Facet;
            #[derive(Facet, Debug)]
            pub struct Data {
                pub coords: [f64; 3],
                pub label: String,
            }
        }
        mod local {
            use facet::Facet;
            #[derive(Facet, Debug, PartialEq)]
            pub struct Data {
                pub label: String,
                pub coords: [f64; 3],
            }
        }

        let r = plan_for(remote::Data::SHAPE, local::Data::SHAPE).unwrap();
        let bytes = to_vec(&remote::Data {
            coords: [1.0, 2.0, 3.0],
            label: "pt".into(),
        })
        .unwrap();
        let result: local::Data =
            from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(
            result,
            local::Data {
                label: "pt".into(),
                coords: [1.0, 2.0, 3.0],
            }
        );
    }

    #[test]
    fn translation_struct_with_map_field() {
        mod remote {
            use facet::Facet;
            #[derive(Facet, Debug)]
            pub struct Data {
                pub meta: std::collections::HashMap<String, u32>,
                pub id: u32,
            }
        }
        mod local {
            use facet::Facet;
            #[derive(Facet, Debug, PartialEq)]
            pub struct Data {
                pub id: u32,
                pub meta: std::collections::HashMap<String, u32>,
            }
        }

        let r = plan_for(remote::Data::SHAPE, local::Data::SHAPE).unwrap();
        let mut meta = std::collections::HashMap::new();
        meta.insert("x".into(), 10);
        let bytes = to_vec(&remote::Data {
            meta: meta.clone(),
            id: 5,
        })
        .unwrap();
        let result: local::Data =
            from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(result, local::Data { id: 5, meta });
    }

    #[test]
    fn translation_struct_with_pointer_field() {
        // Box<T> is a pointer type — tests the Pointer deserialization branch
        mod remote {
            use facet::Facet;
            #[derive(Facet, Debug)]
            pub struct Data {
                #[allow(clippy::box_collection)]
                pub inner: Box<String>,
                pub id: u32,
            }
        }
        mod local {
            use facet::Facet;
            #[derive(Facet, Debug, PartialEq)]
            pub struct Data {
                pub id: u32,
                #[allow(clippy::box_collection)]
                pub inner: Box<String>,
            }
        }

        let r = plan_for(remote::Data::SHAPE, local::Data::SHAPE).unwrap();
        let bytes = to_vec(&remote::Data {
            inner: Box::new("hello".into()),
            id: 7,
        })
        .unwrap();
        let result: local::Data =
            from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(
            result,
            local::Data {
                id: 7,
                inner: Box::new("hello".into()),
            }
        );
    }

    // ========================================================================
    // Transparent wrapper deserialization
    // ========================================================================

    #[test]
    fn round_trip_transparent_wrapper() {
        #[derive(Facet, Debug, PartialEq)]
        #[facet(transparent)]
        struct Wrapper(u32);

        round_trip(&Wrapper(42));
    }

    #[test]
    fn translation_struct_field_reorder_with_nested_struct() {
        // Tests that planned deserialization handles nested structs
        // through the transparent wrapper / inner deserialization path
        mod remote {
            use facet::Facet;
            #[derive(Facet, Debug)]
            pub struct Outer {
                pub inner: Inner,
                pub id: u32,
            }
            #[derive(Facet, Debug)]
            pub struct Inner {
                pub value: String,
            }
        }
        mod local {
            use facet::Facet;
            #[derive(Facet, Debug, PartialEq)]
            pub struct Outer {
                pub id: u32,
                pub inner: Inner,
            }
            #[derive(Facet, Debug, PartialEq)]
            pub struct Inner {
                pub value: String,
            }
        }

        let r = plan_for(remote::Outer::SHAPE, local::Outer::SHAPE).unwrap();
        let bytes = to_vec(&remote::Outer {
            inner: remote::Inner {
                value: "hello".into(),
            },
            id: 42,
        })
        .unwrap();
        let result: local::Outer =
            from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(
            result,
            local::Outer {
                id: 42,
                inner: local::Inner {
                    value: "hello".into(),
                },
            }
        );
    }

    // ========================================================================
    // 128-bit integer round trips
    // ========================================================================

    #[test]
    fn round_trip_u128() {
        round_trip(&u128::MAX);
    }

    #[test]
    fn round_trip_i128() {
        round_trip(&i128::MIN);
    }

    #[test]
    fn round_trip_u128_zero() {
        round_trip(&0u128);
    }

    // ========================================================================
    // Skip enum with tuple variant payload
    // ========================================================================

    #[test]
    fn skip_enum_tuple_variant_field() {
        mod remote {
            use facet::Facet;
            #[derive(Facet, Debug)]
            pub struct Msg {
                pub id: u32,
                pub ev: Event,
                pub name: String,
            }
            #[derive(Facet, Debug)]
            #[repr(u8)]
            #[allow(dead_code)]
            pub enum Event {
                Click(u32, u32) = 0,
                Key(String) = 1,
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

        // Skip a tuple variant
        let bytes = to_vec(&remote::Msg {
            id: 1,
            ev: remote::Event::Click(10, 20),
            name: "a".into(),
        })
        .unwrap();
        let result: local::Msg = from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(
            result,
            local::Msg {
                id: 1,
                name: "a".into()
            }
        );

        // Skip a newtype variant
        let bytes = to_vec(&remote::Msg {
            id: 2,
            ev: remote::Event::Key("enter".into()),
            name: "b".into(),
        })
        .unwrap();
        let result: local::Msg = from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(
            result,
            local::Msg {
                id: 2,
                name: "b".into()
            }
        );
    }

    // ========================================================================
    // Skip unit and char primitive types
    // ========================================================================

    #[test]
    fn round_trip_char() {
        round_trip(&'🦀');
    }

    #[test]
    fn round_trip_unit() {
        round_trip(&());
    }

    #[test]
    fn skip_unit_field() {
        mod remote {
            use facet::Facet;
            #[derive(Facet, Debug)]
            pub struct Msg {
                pub id: u32,
                pub nothing: (),
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
        let bytes = to_vec(&remote::Msg {
            id: 1,
            nothing: (),
            name: "x".into(),
        })
        .unwrap();
        let result: local::Msg = from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(
            result,
            local::Msg {
                id: 1,
                name: "x".into()
            }
        );
    }

    #[test]
    fn skip_char_field() {
        mod remote {
            use facet::Facet;
            #[derive(Facet, Debug)]
            pub struct Msg {
                pub id: u32,
                pub ch: char,
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
        let bytes = to_vec(&remote::Msg {
            id: 1,
            ch: 'Z',
            name: "y".into(),
        })
        .unwrap();
        let result: local::Msg = from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(
            result,
            local::Msg {
                id: 1,
                name: "y".into()
            }
        );
    }

    // ========================================================================
    // Skip u128/i128 fields
    // ========================================================================

    #[test]
    fn skip_u128_field() {
        mod remote {
            use facet::Facet;
            #[derive(Facet, Debug)]
            pub struct Msg {
                pub id: u32,
                pub big: u128,
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
        let bytes = to_vec(&remote::Msg {
            id: 1,
            big: u128::MAX,
            name: "z".into(),
        })
        .unwrap();
        let result: local::Msg = from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(
            result,
            local::Msg {
                id: 1,
                name: "z".into()
            }
        );
    }

    // ========================================================================
    // Borrowed deserialization with plan
    // ========================================================================

    #[test]
    fn borrowed_deserialization_with_plan() {
        mod remote {
            use facet::Facet;
            #[derive(Facet, Debug)]
            pub struct Msg<'a> {
                pub extra: u32,
                pub name: &'a str,
            }
        }
        mod local {
            use facet::Facet;
            #[derive(Facet, Debug, PartialEq)]
            pub struct Msg<'a> {
                pub name: &'a str,
            }
        }

        let r = plan_for(remote::Msg::SHAPE, local::Msg::SHAPE).unwrap();
        let bytes = to_vec(&remote::Msg {
            extra: 99,
            name: "hello",
        })
        .unwrap();
        let result: local::Msg =
            from_slice_borrowed_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(result, local::Msg { name: "hello" });
    }

    // ========================================================================
    // Deserialize Set type
    // ========================================================================

    #[test]
    fn round_trip_hashset() {
        use std::collections::HashSet;
        // Single element to avoid non-deterministic iteration order
        let mut val = HashSet::new();
        val.insert(42u32);
        round_trip(&val);
    }

    // ========================================================================
    // Skip f32 and i128 fields (exercises specific skip_primitive branches)
    // ========================================================================

    #[test]
    fn skip_f32_field() {
        mod remote {
            use facet::Facet;
            #[derive(Facet, Debug)]
            pub struct Msg {
                pub id: u32,
                pub temp: f32,
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
        let bytes = to_vec(&remote::Msg {
            id: 1,
            #[allow(clippy::approx_constant)]
            temp: 3.14,
            name: "x".into(),
        })
        .unwrap();
        let result: local::Msg = from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(
            result,
            local::Msg {
                id: 1,
                name: "x".into()
            }
        );
    }

    #[test]
    fn skip_i128_field() {
        mod remote {
            use facet::Facet;
            #[derive(Facet, Debug)]
            pub struct Msg {
                pub id: u32,
                pub big: i128,
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
        let bytes = to_vec(&remote::Msg {
            id: 1,
            big: i128::MIN,
            name: "y".into(),
        })
        .unwrap();
        let result: local::Msg = from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(
            result,
            local::Msg {
                id: 1,
                name: "y".into()
            }
        );
    }

    // ========================================================================
    // Truncated input (EOF errors)
    // ========================================================================

    #[test]
    fn truncated_struct_input_errors() {
        #[derive(Facet, Debug)]
        struct Msg {
            id: u32,
            name: String,
        }

        // Serialize a valid struct, then truncate the bytes
        let bytes = to_vec(&Msg {
            id: 42,
            name: "hello world".into(),
        })
        .unwrap();
        // Truncate to just the first byte (the varint for id)
        let truncated = &bytes[..1];
        let result: Result<Msg, _> = from_slice(truncated);
        assert!(result.is_err(), "truncated input should error");
    }

    #[test]
    fn truncated_list_input_errors() {
        // Serialize a Vec, truncate mid-stream
        let bytes = to_vec(&vec![1u32, 2, 3, 4, 5]).unwrap();
        let truncated = &bytes[..2]; // length prefix + partial data
        let result: Result<Vec<u32>, _> = from_slice(truncated);
        assert!(result.is_err(), "truncated list should error");
    }

    // ========================================================================
    // Round trip f32 (exercises f32-specific serialization)
    // ========================================================================

    #[test]
    fn round_trip_f32() {
        round_trip(
            #[allow(clippy::approx_constant)]
            &3.14f32,
        );
    }

    // ========================================================================
    // Slice-like types
    // ========================================================================

    #[test]
    fn round_trip_boxed_str() {
        round_trip(&Box::<str>::from("hello"));
    }

    // ========================================================================
    // Proxy type deserialization
    // ========================================================================

    #[test]
    fn proxy_type_round_trip() {
        // Proxied<T> is serialized as its proxy type (u32), then convert_in
        // builds the actual value. This exercises the proxy deserialization path.
        #[derive(Debug, PartialEq, Facet)]
        #[facet(proxy = u32)]
        struct Proxied {
            inner: u32,
        }

        impl From<u32> for Proxied {
            fn from(v: u32) -> Self {
                Proxied { inner: v }
            }
        }

        impl From<&Proxied> for u32 {
            fn from(p: &Proxied) -> Self {
                p.inner
            }
        }

        // Serialize the proxy value (u32)
        let bytes = to_vec(&42u32).unwrap();
        // Deserialize as Proxied — should go through proxy path
        let result: Proxied = from_slice(&bytes).unwrap();
        assert_eq!(result, Proxied { inner: 42 });
    }

    #[test]
    fn proxy_type_in_struct_round_trip() {
        // Test that a struct containing a proxied field round-trips correctly.
        #[derive(Debug, PartialEq, Facet)]
        #[facet(proxy = u32)]
        struct Proxied {
            inner: u32,
        }

        impl From<u32> for Proxied {
            fn from(v: u32) -> Self {
                Proxied { inner: v }
            }
        }

        impl From<&Proxied> for u32 {
            fn from(p: &Proxied) -> Self {
                p.inner
            }
        }

        #[derive(Debug, PartialEq, Facet)]
        struct Msg {
            name: String,
            value: Proxied,
        }

        let msg = Msg {
            name: "test".into(),
            value: Proxied { inner: 99 },
        };
        let bytes = to_vec(&msg).unwrap();
        let result: Msg = from_slice(&bytes).unwrap();
        assert_eq!(result, msg);
    }

    // ========================================================================
    // Borrowed slice round-trips (exercises scatter Reference segments)
    // ========================================================================

    #[test]
    fn round_trip_struct_with_bytes_field() {
        #[derive(Facet, Debug, PartialEq)]
        struct Msg {
            id: u32,
            data: Vec<u8>,
        }

        round_trip(&Msg {
            id: 42,
            data: vec![1, 2, 3, 4, 5],
        });
    }

    #[test]
    fn round_trip_struct_with_large_borrowed_bytes() {
        // Large &[u8] (>4096) — should become a Reference segment in scatter
        #[derive(Facet, Debug, PartialEq)]
        struct Msg<'a> {
            id: u32,
            data: &'a [u8],
        }

        let data = vec![0xABu8; 8192];
        let msg = Msg { id: 1, data: &data };
        let direct = to_vec(&msg).unwrap();

        let peek = facet_reflect::Peek::new(&msg);
        let plan = peek_to_scatter_plan(peek).unwrap();

        // With large data, we should have a Reference segment
        let has_reference = plan
            .segments()
            .iter()
            .any(|s| matches!(s, scatter::Segment::Reference { .. }));
        assert!(
            has_reference,
            "large borrowed bytes should produce a Reference segment"
        );

        let mut scatter_bytes = vec![0u8; plan.total_size()];
        plan.write_into(&mut scatter_bytes);
        assert_eq!(direct, scatter_bytes);

        // Also verify io_slices
        let io_slices = plan.to_io_slices();
        let mut io_bytes = Vec::new();
        for slice in &io_slices {
            io_bytes.extend_from_slice(slice);
        }
        assert_eq!(direct, io_bytes);

        let result: Msg = from_slice_borrowed(&direct).unwrap();
        assert_eq!(result.id, 1);
        assert_eq!(result.data, &data[..]);
    }

    #[test]
    fn round_trip_large_string() {
        // Large string (>4096) to hit the Reference threshold in scatter
        let text = "x".repeat(8192);
        round_trip(&text);
    }

    // ========================================================================
    // Error Display coverage
    // ========================================================================

    #[test]
    fn deserialize_error_display_coverage() {
        use error::DeserializeError;

        // Exercise Display for each DeserializeError variant
        let errors: Vec<DeserializeError> = vec![
            DeserializeError::UnexpectedEof { pos: 42 },
            DeserializeError::VarintOverflow { pos: 10 },
            DeserializeError::InvalidBool { pos: 5, got: 0x02 },
            DeserializeError::InvalidUtf8 { pos: 20 },
            DeserializeError::InvalidOptionTag { pos: 3, got: 0xFF },
            DeserializeError::InvalidEnumDiscriminant {
                pos: 0,
                index: 99,
                variant_count: 3,
            },
            DeserializeError::UnsupportedType("SomeType".into()),
            DeserializeError::ReflectError("something went wrong".into()),
            DeserializeError::UnknownVariant { remote_index: 7 },
            DeserializeError::TrailingBytes { pos: 10, len: 20 },
            DeserializeError::Custom("custom error".into()),
            DeserializeError::protocol("protocol violation"),
        ];

        for err in &errors {
            let msg = format!("{err}");
            assert!(!msg.is_empty(), "error display should not be empty");
        }
    }

    #[test]
    fn serialize_error_display_coverage() {
        use error::SerializeError;

        let errors: Vec<SerializeError> = vec![
            SerializeError::UnsupportedType("BadType".into()),
            SerializeError::ReflectError("reflect fail".into()),
        ];

        for err in &errors {
            let msg = format!("{err}");
            assert!(!msg.is_empty());
        }
    }

    #[test]
    fn translation_error_display_coverage() {
        use roam_types::{
            FieldSchema, Schema, SchemaHash, SchemaKind, TypeRef, VariantPayload, VariantSchema,
        };

        let dummy_struct = Schema {
            id: SchemaHash(1),
            type_params: vec![],
            kind: SchemaKind::Struct {
                name: "Foo".into(),
                fields: vec![FieldSchema {
                    name: "x".into(),
                    type_ref: TypeRef::concrete(SchemaHash(2)),
                    required: true,
                }],
            },
        };
        let dummy_enum = Schema {
            id: SchemaHash(3),
            type_params: vec![],
            kind: SchemaKind::Enum {
                name: "Bar".into(),
                variants: vec![],
            },
        };
        let dummy_prim = Schema {
            id: SchemaHash(4),
            type_params: vec![],
            kind: SchemaKind::Primitive {
                primitive_type: roam_types::PrimitiveType::U32,
            },
        };

        let errors: Vec<TranslationError> = vec![
            TranslationError::new(TranslationErrorKind::NameMismatch {
                remote: dummy_struct.clone(),
                local: dummy_enum.clone(),
                remote_rust: "Dummy".into(),
                local_rust: "Dummy".into(),
            }),
            TranslationError::new(TranslationErrorKind::KindMismatch {
                remote: dummy_struct.clone(),
                local: dummy_prim.clone(),
                remote_rust: "Dummy".into(),
                local_rust: "u32".into(),
            }),
            TranslationError::new(TranslationErrorKind::MissingRequiredField {
                field: FieldSchema {
                    name: "missing".into(),
                    type_ref: TypeRef::concrete(SchemaHash(5)),
                    required: true,
                },
                remote_struct: dummy_struct.clone(),
            }),
            TranslationError::new(TranslationErrorKind::IncompatibleVariantPayload {
                remote_variant: VariantSchema {
                    name: "V".into(),
                    index: 0,
                    payload: VariantPayload::Unit,
                },
                local_variant: VariantSchema {
                    name: "V".into(),
                    index: 0,
                    payload: VariantPayload::Newtype {
                        type_ref: TypeRef::concrete(SchemaHash(6)),
                    },
                },
            }),
            TranslationError::new(TranslationErrorKind::SchemaNotFound {
                type_id: SchemaHash(99),
                side: error::SchemaSide::Remote,
            }),
            TranslationError::new(TranslationErrorKind::TupleLengthMismatch {
                remote: dummy_prim.clone(),
                local: dummy_prim.clone(),
                remote_rust: "(u32, u32)".into(),
                local_rust: "(u32, u32, u32)".into(),
                remote_len: 2,
                local_len: 3,
            }),
            TranslationError::new(TranslationErrorKind::UnresolvedVar {
                name: "T".into(),
                side: error::SchemaSide::Local,
            }),
        ];

        for err in &errors {
            let msg = format!("{err}");
            assert!(
                !msg.is_empty(),
                "translation error display should not be empty"
            );
        }

        // Also test with path prefix
        let with_path = TranslationError::new(TranslationErrorKind::SchemaNotFound {
            type_id: SchemaHash(1),
            side: error::SchemaSide::Remote,
        })
        .with_path_prefix(error::PathSegment::Field("foo".into()));
        let msg = format!("{with_path}");
        assert!(msg.contains(".foo"), "should contain path: {msg}");
    }

    // ========================================================================
    // Failing tests: translation plans don't recurse into containers
    // ========================================================================

    #[test]
    fn translation_through_vec_skips_extra_field() {
        // Remote B has an extra field that local B doesn't have.
        // The translation is inside a Vec — the plan must recurse into
        // the Vec's element type to produce the skip.
        mod remote {
            use facet::Facet;
            #[derive(Facet, Debug)]
            pub struct A {
                pub items: Vec<B>,
            }
            #[derive(Facet, Debug)]
            pub struct B {
                pub x: u32,
                pub extra: String,
            }
        }
        mod local {
            use facet::Facet;
            #[derive(Facet, Debug, PartialEq)]
            pub struct A {
                pub items: Vec<B>,
            }
            #[derive(Facet, Debug, PartialEq)]
            pub struct B {
                pub x: u32,
            }
        }

        let r = plan_for(remote::A::SHAPE, local::A::SHAPE).unwrap();
        let bytes = to_vec(&remote::A {
            items: vec![
                remote::B {
                    x: 1,
                    extra: "a".into(),
                },
                remote::B {
                    x: 2,
                    extra: "bb".into(),
                },
                remote::B {
                    x: 3,
                    extra: "ccc".into(),
                },
            ],
        })
        .unwrap();
        let result: local::A = from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(
            result,
            local::A {
                items: vec![local::B { x: 1 }, local::B { x: 2 }, local::B { x: 3 }],
            }
        );
    }

    #[test]
    fn translation_through_option_skips_extra_field() {
        // Translation change is inside an Option<B>.
        // A trailing field `id` after the Option exposes cursor misalignment.
        mod remote {
            use facet::Facet;
            #[derive(Facet, Debug)]
            pub struct A {
                pub maybe: Option<B>,
                pub id: u32,
            }
            #[derive(Facet, Debug)]
            pub struct B {
                pub x: u32,
                pub extra: String,
            }
        }
        mod local {
            use facet::Facet;
            #[derive(Facet, Debug, PartialEq)]
            pub struct A {
                pub maybe: Option<B>,
                pub id: u32,
            }
            #[derive(Facet, Debug, PartialEq)]
            pub struct B {
                pub x: u32,
            }
        }

        let r = plan_for(remote::A::SHAPE, local::A::SHAPE).unwrap();
        let bytes = to_vec(&remote::A {
            maybe: Some(remote::B {
                x: 42,
                extra: "gone".into(),
            }),
            id: 99,
        })
        .unwrap();
        let result: local::A = from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(
            result,
            local::A {
                maybe: Some(local::B { x: 42 }),
                id: 99,
            }
        );
    }

    #[test]
    fn translation_through_result_skips_extra_field() {
        // Translation change is inside a Result<B, String>.
        // Trailing field `id` exposes cursor misalignment.
        mod remote {
            use facet::Facet;
            #[derive(Facet, Debug)]
            pub struct A {
                pub result: Result<B, String>,
                pub id: u32,
            }
            #[derive(Facet, Debug)]
            pub struct B {
                pub x: u32,
                pub extra: String,
            }
        }
        mod local {
            use facet::Facet;
            #[derive(Facet, Debug, PartialEq)]
            pub struct A {
                pub result: Result<B, String>,
                pub id: u32,
            }
            #[derive(Facet, Debug, PartialEq)]
            pub struct B {
                pub x: u32,
            }
        }

        let r = plan_for(remote::A::SHAPE, local::A::SHAPE).unwrap();
        let bytes = to_vec(&remote::A {
            result: Ok(remote::B {
                x: 99,
                extra: "dropped".into(),
            }),
            id: 77,
        })
        .unwrap();
        let result: local::A = from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(
            result,
            local::A {
                result: Ok(local::B { x: 99 }),
                id: 77,
            }
        );
    }

    #[test]
    fn translation_through_map_value_skips_extra_field() {
        // Translation change is in the value type of a HashMap.
        // Trailing field `id` exposes cursor misalignment.
        mod remote {
            use facet::Facet;
            #[derive(Facet, Debug)]
            pub struct A {
                pub map: std::collections::HashMap<String, B>,
                pub id: u32,
            }
            #[derive(Facet, Debug)]
            pub struct B {
                pub x: u32,
                pub extra: String,
            }
        }
        mod local {
            use facet::Facet;
            #[derive(Facet, Debug, PartialEq)]
            pub struct A {
                pub map: std::collections::HashMap<String, B>,
                pub id: u32,
            }
            #[derive(Facet, Debug, PartialEq)]
            pub struct B {
                pub x: u32,
            }
        }

        let r = plan_for(remote::A::SHAPE, local::A::SHAPE).unwrap();
        let mut remote_map = std::collections::HashMap::new();
        remote_map.insert(
            "key".into(),
            remote::B {
                x: 7,
                extra: "nope".into(),
            },
        );
        let bytes = to_vec(&remote::A {
            map: remote_map,
            id: 55,
        })
        .unwrap();
        let result: local::A = from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        let mut expected_map = std::collections::HashMap::new();
        expected_map.insert("key".into(), local::B { x: 7 });
        assert_eq!(
            result,
            local::A {
                map: expected_map,
                id: 55,
            }
        );
    }

    #[test]
    fn translation_nested_three_levels_deep() {
        // Change is 3 levels deep: A -> Vec<B> -> C (C has extra field).
        mod remote {
            use facet::Facet;
            #[derive(Facet, Debug)]
            pub struct A {
                pub items: Vec<B>,
            }
            #[derive(Facet, Debug)]
            pub struct B {
                pub inner: C,
            }
            #[derive(Facet, Debug)]
            pub struct C {
                pub x: u32,
                pub extra: String,
            }
        }
        mod local {
            use facet::Facet;
            #[derive(Facet, Debug, PartialEq)]
            pub struct A {
                pub items: Vec<B>,
            }
            #[derive(Facet, Debug, PartialEq)]
            pub struct B {
                pub inner: C,
            }
            #[derive(Facet, Debug, PartialEq)]
            pub struct C {
                pub x: u32,
            }
        }

        let r = plan_for(remote::A::SHAPE, local::A::SHAPE).unwrap();
        let bytes = to_vec(&remote::A {
            items: vec![remote::B {
                inner: remote::C {
                    x: 42,
                    extra: "deep".into(),
                },
            }],
        })
        .unwrap();
        let result: local::A = from_slice_with_plan(&bytes, &r.plan, &r.remote.registry).unwrap();
        assert_eq!(
            result,
            local::A {
                items: vec![local::B {
                    inner: local::C { x: 42 },
                }],
            }
        );
    }

    // ========================================================================
    // Memory safety: borrowed vs owned deserialization
    // ========================================================================

    #[test]
    fn non_borrowed_into_borrowed_str_must_fail() {
        // Deserializing into a type with &str using from_slice (BORROW=false)
        // must fail — the returned value would contain a dangling reference
        // to the input bytes which are not guaranteed to outlive the value.
        #[derive(Facet, Debug, PartialEq)]
        struct Msg<'a> {
            name: &'a str,
        }

        let bytes = to_vec(&"hello".to_string()).unwrap();
        // from_slice uses BORROW=false — it cannot produce borrowed references
        let result: Result<Msg<'_>, _> = from_slice(&bytes);
        assert!(
            result.is_err(),
            "from_slice (non-borrowed) into &str should fail, got: {:?}",
            result,
        );
    }

    #[test]
    fn borrowed_into_borrowed_str_must_succeed() {
        // from_slice_borrowed (BORROW=true) into &str must succeed —
        // the returned value borrows directly from the input.
        #[derive(Facet, Debug, PartialEq)]
        struct Msg<'a> {
            name: &'a str,
        }

        let bytes = to_vec(&"hello".to_string()).unwrap();
        let result: Msg<'_> = from_slice_borrowed(&bytes).unwrap();
        assert_eq!(result.name, "hello");
    }

    #[test]
    fn non_borrowed_cow_str_returns_owned() {
        // from_slice (BORROW=false) into Cow<str> must succeed and return
        // Cow::Owned — it can't borrow, so it clones into an owned String.
        use std::borrow::Cow;

        let bytes = to_vec(&"hello".to_string()).unwrap();
        let result: Cow<'_, str> = from_slice(&bytes).unwrap();
        assert_eq!(&*result, "hello");
        assert!(
            matches!(result, Cow::Owned(_)),
            "from_slice into Cow<str> should return Owned, got Borrowed",
        );
    }

    #[test]
    fn borrowed_cow_str_returns_borrowed() {
        // from_slice_borrowed (BORROW=true) into Cow<str> must succeed and
        // return Cow::Borrowed — it can borrow directly from the input.
        use std::borrow::Cow;

        let bytes = to_vec(&"hello".to_string()).unwrap();
        let result: Cow<'_, str> = from_slice_borrowed(&bytes).unwrap();
        assert_eq!(&*result, "hello");
        assert!(
            matches!(result, Cow::Borrowed(_)),
            "from_slice_borrowed into Cow<str> should return Borrowed, got Owned",
        );
    }

    // ========================================================================
    // Cow<str> round-trip
    // ========================================================================

    #[test]
    fn round_trip_cow_str() {
        use std::borrow::Cow;
        let val: Cow<'_, str> = Cow::Owned("hello cow".to_string());
        let bytes = to_vec(&val).unwrap();
        let result: Cow<'_, str> = from_slice_borrowed(&bytes).unwrap();
        assert_eq!(result, val);
    }

    #[test]
    fn round_trip_struct_with_cow_str() {
        use std::borrow::Cow;
        #[derive(Facet, Debug, PartialEq)]
        struct Msg<'a> {
            name: Cow<'a, str>,
            id: u32,
        }

        let msg = Msg {
            name: Cow::Owned("test".to_string()),
            id: 42,
        };
        let bytes = to_vec(&msg).unwrap();
        let result: Msg<'_> = from_slice_borrowed(&bytes).unwrap();
        assert_eq!(result, msg);
    }

    // ========================================================================
    // All integer types round-trip
    // ========================================================================

    #[test]
    fn round_trip_u8() {
        round_trip(&0u8);
        round_trip(&255u8);
    }

    #[test]
    fn round_trip_u16() {
        round_trip(&0u16);
        round_trip(&u16::MAX);
    }

    #[test]
    fn round_trip_u64() {
        round_trip(&0u64);
        round_trip(&u64::MAX);
    }

    #[test]
    fn round_trip_i8() {
        round_trip(&0i8);
        round_trip(&i8::MIN);
        round_trip(&i8::MAX);
    }

    #[test]
    fn round_trip_i16() {
        round_trip(&0i16);
        round_trip(&i16::MIN);
        round_trip(&i16::MAX);
    }

    #[test]
    fn round_trip_i32() {
        round_trip(&0i32);
        round_trip(&i32::MIN);
        round_trip(&i32::MAX);
    }

    #[test]
    fn round_trip_i64() {
        round_trip(&0i64);
        round_trip(&i64::MIN);
        round_trip(&i64::MAX);
    }

    // ========================================================================
    // Slice round-trip
    // ========================================================================

    #[test]
    fn round_trip_borrowed_slice_u32() {
        // &[u32] should serialize like Vec<u32> and deserialize back
        let data: Vec<u32> = vec![10, 20, 30];
        let bytes = to_vec(&data).unwrap();
        let result: Vec<u32> = from_slice_borrowed(&bytes).unwrap();
        assert_eq!(result, data);
    }

    #[test]
    fn round_trip_borrowed_slice_in_struct() {
        #[derive(Facet, Debug, PartialEq)]
        struct Msg<'a> {
            data: &'a [u8],
            id: u32,
        }

        let data = [1u8, 2, 3, 4, 5];
        let msg = Msg {
            data: &data,
            id: 42,
        };
        let bytes = to_vec(&msg).unwrap();
        let result: Msg<'_> = from_slice_borrowed(&bytes).unwrap();
        assert_eq!(result.id, 42);
        assert_eq!(result.data, &data);
    }
}
