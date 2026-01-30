use facet::Facet;
use facet_reflect2::{Op, Partial, ReflectErrorKind};

#[derive(Debug, PartialEq, Facet)]
struct Point {
    x: i32,
    y: i32,
}

#[test]
fn set_struct_fields() {
    let mut partial = Partial::alloc::<Point>().unwrap();

    let x = 10i32;
    let y = 20i32;
    partial
        .apply(&[Op::set().at(0).mov(&x), Op::set().at(1).mov(&y)])
        .unwrap();

    let result: Point = partial.build().unwrap();
    assert_eq!(result, Point { x: 10, y: 20 });
}

#[test]
fn build_with_incomplete_children() {
    let mut partial = Partial::alloc::<Point>().unwrap();

    // Only set one field
    let x = 10i32;
    partial.apply(&[Op::set().at(0).mov(&x)]).unwrap();

    // Try to build - should fail because y is not initialized
    let err = partial.build::<Point>().unwrap_err();
    assert!(matches!(err.kind, ReflectErrorKind::NotInitialized));
}

#[test]
fn field_index_out_of_bounds() {
    let mut partial = Partial::alloc::<Point>().unwrap();

    let value = 10i32;
    // Point only has 2 fields (indices 0 and 1), try index 5
    let err = partial.apply(&[Op::set().at(5).mov(&value)]).unwrap_err();
    assert!(matches!(
        err.kind,
        ReflectErrorKind::FieldIndexOutOfBounds {
            index: 5,
            field_count: 2
        }
    ));
}

#[test]
fn set_field_on_non_struct() {
    let mut partial = Partial::alloc::<u32>().unwrap();

    let value = 10u32;
    // u32 is not a struct, can't navigate into fields
    let err = partial.apply(&[Op::set().at(0).mov(&value)]).unwrap_err();
    assert!(matches!(err.kind, ReflectErrorKind::NotAStruct));
}

#[test]
fn multi_level_path_not_supported() {
    #[derive(Facet)]
    struct Outer {
        inner: Point,
    }

    let mut partial = Partial::alloc::<Outer>().unwrap();

    let value = 10i32;
    // Try to set outer.inner.x with path [0, 0] - multi-level not yet supported
    let err = partial
        .apply(&[Op::set().at(0).at(0).mov(&value)])
        .unwrap_err();
    assert!(matches!(
        err.kind,
        ReflectErrorKind::MultiLevelPathNotSupported { depth: 2 }
    ));
}

#[test]
fn field_type_mismatch() {
    let mut partial = Partial::alloc::<Point>().unwrap();

    // Try to set a String into an i32 field
    let value = String::from("hello");
    let err = partial.apply(&[Op::set().at(0).mov(&value)]).unwrap_err();
    assert!(matches!(err.kind, ReflectErrorKind::ShapeMismatch { .. }));
}

#[test]
fn set_struct_fields_with_at_path() {
    let mut partial = Partial::alloc::<Point>().unwrap();

    let x = 10i32;
    let y = 20i32;
    // Use at_path instead of at().at()
    partial
        .apply(&[
            Op::set().at_path(&[0]).mov(&x),
            Op::set().at_path(&[1]).mov(&y),
        ])
        .unwrap();

    let result: Point = partial.build().unwrap();
    assert_eq!(result, Point { x: 10, y: 20 });
}

#[derive(Debug, Facet)]
struct TwoStrings {
    a: String,
    b: String,
}

#[test]
fn drop_partially_initialized_struct() {
    // Partially initialize a struct with Drop fields, then drop without build
    let mut partial = Partial::alloc::<TwoStrings>().unwrap();

    let a = String::from("first");
    partial.apply(&[Op::set().at(0).mov(&a)]).unwrap();
    std::mem::forget(a);

    // Drop without setting field b - must clean up field a
    drop(partial);
}

#[test]
fn build_fails_then_drops_partial_struct() {
    // Same scenario but via build() returning error
    let mut partial = Partial::alloc::<TwoStrings>().unwrap();

    let a = String::from("will be cleaned up");
    partial.apply(&[Op::set().at(0).mov(&a)]).unwrap();
    std::mem::forget(a);

    // build() fails because b is not set, then Drop cleans up a
    let err = partial.build::<TwoStrings>().unwrap_err();
    assert!(matches!(err.kind, ReflectErrorKind::NotInitialized));
}

#[test]
fn set_field_wrong_type_poisons_partial() {
    let mut partial = Partial::alloc::<Point>().unwrap();

    // Set field 0
    let x = 10i32;
    partial.apply(&[Op::set().at(0).mov(&x)]).unwrap();

    // Try to set field 1 with wrong type - should fail and poison the Partial
    let wrong = String::from("oops");
    let err = partial.apply(&[Op::set().at(1).mov(&wrong)]).unwrap_err();
    assert!(matches!(err.kind, ReflectErrorKind::ShapeMismatch { .. }));

    // After an error, the Partial is poisoned - any further operations should fail
    let y = 20i32;
    let err = partial.apply(&[Op::set().at(1).mov(&y)]).unwrap_err();
    assert!(matches!(err.kind, ReflectErrorKind::Poisoned));
}

#[derive(Debug, Default, PartialEq, Facet)]
struct PointWithDefault {
    x: i32,
    y: i32,
}

#[test]
fn set_struct_field_to_default() {
    let mut partial = Partial::alloc::<PointWithDefault>().unwrap();

    let x = 10i32;
    partial
        .apply(&[
            Op::set().at(0).mov(&x),
            Op::set().at(1).default(), // y gets default value (0)
        ])
        .unwrap();

    let result: PointWithDefault = partial.build().unwrap();
    assert_eq!(result, PointWithDefault { x: 10, y: 0 });
}

#[test]
fn set_whole_struct_to_default() {
    let mut partial = Partial::alloc::<PointWithDefault>().unwrap();

    partial.apply(&[Op::set().default()]).unwrap();

    let result: PointWithDefault = partial.build().unwrap();
    assert_eq!(result, PointWithDefault::default());
}

// A type that derives Facet but not Default
#[derive(Debug, Facet)]
struct NoDefaultType {
    value: i32,
}

#[test]
fn set_default_fails_for_type_without_default() {
    let mut partial = Partial::alloc::<NoDefaultType>().unwrap();

    let err = partial.apply(&[Op::set().default()]).unwrap_err();
    assert!(matches!(err.kind, ReflectErrorKind::NoDefault { .. }));
}

// Nested struct for Build tests
#[derive(Debug, PartialEq, Facet)]
struct Outer {
    inner: Point,
    extra: i32,
}

#[test]
fn build_nested_struct() {
    let mut partial = Partial::alloc::<Outer>().unwrap();

    // Build inner struct incrementally
    partial.apply(&[Op::set().at(0).build()]).unwrap();

    // Now we're in the inner frame - set its fields
    let x = 10i32;
    let y = 20i32;
    partial
        .apply(&[Op::set().at(0).mov(&x), Op::set().at(1).mov(&y)])
        .unwrap();

    // End the inner frame
    partial.apply(&[Op::end()]).unwrap();

    // Set the outer extra field
    let extra = 99i32;
    partial.apply(&[Op::set().at(1).mov(&extra)]).unwrap();

    let result: Outer = partial.build().unwrap();
    assert_eq!(
        result,
        Outer {
            inner: Point { x: 10, y: 20 },
            extra: 99
        }
    );
}

#[test]
fn end_at_root_fails() {
    let mut partial = Partial::alloc::<Point>().unwrap();

    let err = partial.apply(&[Op::end()]).unwrap_err();
    assert!(matches!(err.kind, ReflectErrorKind::EndAtRoot));
}

#[test]
fn end_with_incomplete_fails() {
    let mut partial = Partial::alloc::<Outer>().unwrap();

    // Start building inner
    partial.apply(&[Op::set().at(0).build()]).unwrap();

    // Only set one field of inner
    let x = 10i32;
    partial.apply(&[Op::set().at(0).mov(&x)]).unwrap();

    // Try to end - should fail because inner.y is not set
    let err = partial.apply(&[Op::end()]).unwrap_err();
    assert!(matches!(err.kind, ReflectErrorKind::EndWithIncomplete));
}

#[test]
fn build_at_empty_path_fails() {
    let mut partial = Partial::alloc::<Point>().unwrap();

    let err = partial.apply(&[Op::set().build()]).unwrap_err();
    assert!(matches!(err.kind, ReflectErrorKind::BuildAtEmptyPath));
}
