use facet::Facet;
use facet_reflect2::{Op, Partial, ReflectErrorKind};

// Regression test for double-free bug found by AFL fuzzer.
// When setting a field on an already-INIT struct/tuple, we drop the old field
// but don't clear INIT. Then if an error occurs and we poison(), uninit() sees
// INIT and tries to drop the whole struct again - double-free.
#[test]
fn set_field_on_init_struct_then_error_no_double_free() {
    #[derive(Debug, Facet)]
    struct TwoStrings {
        a: String,
        b: String,
    }

    let mut partial = Partial::alloc::<TwoStrings>().unwrap();

    // Step 1: Set the whole struct via Imm
    let value = TwoStrings {
        a: String::from("first_a"),
        b: String::from("first_b"),
    };
    partial.apply(&[Op::set().imm(&value)]).unwrap();
    std::mem::forget(value); // We moved ownership

    // Step 2: Set field 0 with a new String - this should drop the old "first_a"
    let new_a = String::from("second_a");
    partial.apply(&[Op::set().at(0).imm(&new_a)]).unwrap();
    std::mem::forget(new_a);

    // Step 3: Trigger an error by setting the whole struct with Default
    // (TwoStrings doesn't implement Default, so this will fail)
    // This should NOT double-free "first_a" (already dropped in step 2)
    let err = partial.apply(&[Op::set().default()]).unwrap_err();
    assert!(matches!(err.kind, ReflectErrorKind::NoDefault { .. }));

    // The partial is now poisoned, which is fine - but we shouldn't have UB
}

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
        .apply(&[Op::set().at(0).imm(&x), Op::set().at(1).imm(&y)])
        .unwrap();

    let result: Point = partial.build().unwrap();
    assert_eq!(result, Point { x: 10, y: 20 });
}

#[test]
fn build_with_incomplete_children() {
    let mut partial = Partial::alloc::<Point>().unwrap();

    // Only set one field
    let x = 10i32;
    partial.apply(&[Op::set().at(0).imm(&x)]).unwrap();

    // Try to build - should fail because y is not initialized
    let err = partial.build::<Point>().unwrap_err();
    assert!(matches!(err.kind, ReflectErrorKind::NotInitialized));
}

#[test]
fn field_index_out_of_bounds() {
    let mut partial = Partial::alloc::<Point>().unwrap();

    let value = 10i32;
    // Point only has 2 fields (indices 0 and 1), try index 5
    let err = partial.apply(&[Op::set().at(5).imm(&value)]).unwrap_err();
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
    let err = partial.apply(&[Op::set().at(0).imm(&value)]).unwrap_err();
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
        .apply(&[Op::set().at(0).at(0).imm(&value)])
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
    let err = partial.apply(&[Op::set().at(0).imm(&value)]).unwrap_err();
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
            Op::set().at_path(&[0]).imm(&x),
            Op::set().at_path(&[1]).imm(&y),
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
    partial.apply(&[Op::set().at(0).imm(&a)]).unwrap();
    std::mem::forget(a);

    // Drop without setting field b - must clean up field a
    drop(partial);
}

#[test]
fn build_fails_then_drops_partial_struct() {
    // Same scenario but via build() returning error
    let mut partial = Partial::alloc::<TwoStrings>().unwrap();

    let a = String::from("will be cleaned up");
    partial.apply(&[Op::set().at(0).imm(&a)]).unwrap();
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
    partial.apply(&[Op::set().at(0).imm(&x)]).unwrap();

    // Try to set field 1 with wrong type - should fail and poison the Partial
    let wrong = String::from("oops");
    let err = partial.apply(&[Op::set().at(1).imm(&wrong)]).unwrap_err();
    assert!(matches!(err.kind, ReflectErrorKind::ShapeMismatch { .. }));

    // After an error, the Partial is poisoned - any further operations should fail
    let y = 20i32;
    let err = partial.apply(&[Op::set().at(1).imm(&y)]).unwrap_err();
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
            Op::set().at(0).imm(&x),
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
        .apply(&[Op::set().at(0).imm(&x), Op::set().at(1).imm(&y)])
        .unwrap();

    // End the inner frame
    partial.apply(&[Op::end()]).unwrap();

    // Set the outer extra field
    let extra = 99i32;
    partial.apply(&[Op::set().at(1).imm(&extra)]).unwrap();

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
    partial.apply(&[Op::set().at(0).imm(&x)]).unwrap();

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

#[test]
fn build_box_containing_struct() {
    let mut partial = Partial::alloc::<Box<Point>>().unwrap();

    // Use Build to enter the box (allocate the inner memory)
    partial.apply(&[Op::set().build()]).unwrap();

    // Set inner struct fields
    let x = 10i32;
    let y = 20i32;
    partial
        .apply(&[Op::set().at(0).imm(&x), Op::set().at(1).imm(&y)])
        .unwrap();

    // End the box frame
    partial.apply(&[Op::end()]).unwrap();

    let result: Box<Point> = partial.build().unwrap();
    assert_eq!(*result, Point { x: 10, y: 20 });
}

#[test]
fn build_rc_containing_struct() {
    use std::rc::Rc;

    let mut partial = Partial::alloc::<Rc<Point>>().unwrap();

    // Use Build to enter the Rc (allocate staging memory)
    partial.apply(&[Op::set().build()]).unwrap();

    // Set inner struct fields
    let x = 100i32;
    let y = 200i32;
    partial
        .apply(&[Op::set().at(0).imm(&x), Op::set().at(1).imm(&y)])
        .unwrap();

    // End - this calls new_into_fn to create the actual Rc
    partial.apply(&[Op::end()]).unwrap();

    let result: Rc<Point> = partial.build().unwrap();
    assert_eq!(*result, Point { x: 100, y: 200 });
}

#[test]
fn build_arc_containing_struct() {
    use std::sync::Arc;

    let mut partial = Partial::alloc::<Arc<Point>>().unwrap();

    // Use Build to enter the Arc (allocate staging memory)
    partial.apply(&[Op::set().build()]).unwrap();

    // Set inner struct fields
    let x = 1000i32;
    let y = 2000i32;
    partial
        .apply(&[Op::set().at(0).imm(&x), Op::set().at(1).imm(&y)])
        .unwrap();

    // End - this calls new_into_fn to create the actual Arc
    partial.apply(&[Op::end()]).unwrap();

    let result: Arc<Point> = partial.build().unwrap();
    assert_eq!(*result, Point { x: 1000, y: 2000 });
}
