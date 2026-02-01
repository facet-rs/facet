use facet::Facet;
use trame::{Op, Partial, ReflectErrorKind};

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
    let mut value = TwoStrings {
        a: String::from("first_a"),
        b: String::from("first_b"),
    };
    partial.apply(&[Op::set().imm(&mut value)]).unwrap();
    std::mem::forget(value); // We moved ownership

    // Step 2: Set field 0 with a new String - this should drop the old "first_a"
    let mut new_a = String::from("second_a");
    partial.apply(&[Op::set().at(0).imm(&mut new_a)]).unwrap();
    std::mem::forget(new_a);

    // Step 3: Trigger an error by setting the whole struct with Default
    // (TwoStrings doesn't implement Default, so this will fail)
    // This should NOT double-free "first_a" (already dropped in step 2)
    let err = partial.apply(&[Op::set().default()]).unwrap_err();
    assert!(matches!(err.kind, ReflectErrorKind::NoDefault { .. }));

    // The partial is now poisoned, which is fine - but we shouldn't have UB
}

// Regression test for double-free bug found by AFL fuzzer.
// When overwriting a field that was previously set individually (not via whole-struct Imm),
// we drop the old value but don't mark it incomplete. Then if the operation fails,
// poison() -> uninit() sees the field as complete and tries to drop it again.
#[test]
fn overwrite_field_then_fail_no_double_free() {
    #[derive(Debug, Facet)]
    struct HasBox {
        value: Box<i32>,
        other: i32,
    }

    let mut partial = Partial::alloc::<HasBox>().unwrap();

    // Step 1: Set field 0 (Box<i32>) via Imm
    let mut boxed = Box::new(42i32);
    partial.apply(&[Op::set().at(0).imm(&mut boxed)]).unwrap();
    std::mem::forget(boxed); // Ownership transferred

    // Step 2: Try to overwrite field 0 with Default - this should:
    // 1. Drop the old Box<i32>
    // 2. Try to call default (Box<i32> has no default without T: Default)
    // 3. Fail and poison the partial
    // The bug was: after dropping the old value, we didn't mark the field incomplete,
    // so poison() -> uninit() would try to drop it again (use-after-free).
    let err = partial.apply(&[Op::set().at(0).default()]).unwrap_err();
    assert!(matches!(err.kind, ReflectErrorKind::NoDefault { .. }));

    // If we get here without crashing, the bug is fixed
}

#[derive(Debug, PartialEq, Facet)]
struct Point {
    x: i32,
    y: i32,
}

#[test]
fn set_struct_fields() {
    let mut partial = Partial::alloc::<Point>().unwrap();

    let mut x = 10i32;
    let mut y = 20i32;
    partial
        .apply(&[Op::set().at(0).imm(&mut x), Op::set().at(1).imm(&mut y)])
        .unwrap();

    let result: Point = partial.build().unwrap();
    assert_eq!(result, Point { x: 10, y: 20 });
}

#[test]
fn build_with_incomplete_children() {
    let mut partial = Partial::alloc::<Point>().unwrap();

    // Only set one field
    let mut x = 10i32;
    partial.apply(&[Op::set().at(0).imm(&mut x)]).unwrap();

    // Try to build - should fail because y is not initialized
    let err = partial.build::<Point>().unwrap_err();
    assert!(matches!(err.kind, ReflectErrorKind::NotInitialized));
}

#[test]
fn field_index_out_of_bounds() {
    let mut partial = Partial::alloc::<Point>().unwrap();

    let mut value = 10i32;
    // Point only has 2 fields (indices 0 and 1), try index 5
    let err = partial
        .apply(&[Op::set().at(5).imm(&mut value)])
        .unwrap_err();
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

    let mut value = 10u32;
    // u32 is not a struct, can't navigate into fields
    let err = partial
        .apply(&[Op::set().at(0).imm(&mut value)])
        .unwrap_err();
    assert!(matches!(err.kind, ReflectErrorKind::NotAStruct));
}

#[test]
fn multi_level_path_sets_nested_field() {
    #[derive(Debug, PartialEq, Facet)]
    struct Outer {
        inner: Point,
    }

    let mut partial = Partial::alloc::<Outer>().unwrap();

    // Set outer.inner.x with path at(0).at(0) - creates intermediate frame
    let mut x = 10i32;
    partial.apply(&[Op::set().at(0).at(0).imm(&mut x)]).unwrap();

    // Now we're inside the `inner` frame, set y
    let mut y = 20i32;
    partial.apply(&[Op::set().at(1).imm(&mut y)]).unwrap();

    // End inner frame, back to Outer (which is now complete)
    partial.apply(&[Op::end()]).unwrap();

    let result: Outer = partial.build().unwrap();
    assert_eq!(
        result,
        Outer {
            inner: Point { x: 10, y: 20 }
        }
    );
}

#[test]
fn multi_level_path_three_levels() {
    #[derive(Debug, PartialEq, Facet)]
    struct Outer {
        middle: Middle,
    }

    #[derive(Debug, PartialEq, Facet)]
    struct Middle {
        inner: Point,
    }

    let mut partial = Partial::alloc::<Outer>().unwrap();

    // Set outer.middle.inner.x with a 3-level path
    let mut x = 100i32;
    partial
        .apply(&[Op::set().at(0).at(0).at(0).imm(&mut x)])
        .unwrap();

    // Now in Point frame, set y
    let mut y = 200i32;
    partial.apply(&[Op::set().at(1).imm(&mut y)]).unwrap();

    // build() auto-navigates to root
    let result: Outer = partial.build().unwrap();
    assert_eq!(
        result,
        Outer {
            middle: Middle {
                inner: Point { x: 100, y: 200 }
            }
        }
    );
}

#[test]
fn field_type_mismatch() {
    let mut partial = Partial::alloc::<Point>().unwrap();

    // Try to set a String into an i32 field
    let mut value = String::from("hello");
    let err = partial
        .apply(&[Op::set().at(0).imm(&mut value)])
        .unwrap_err();
    assert!(matches!(err.kind, ReflectErrorKind::ShapeMismatch { .. }));
}

#[test]
fn set_struct_fields_with_at_path() {
    let mut partial = Partial::alloc::<Point>().unwrap();

    let mut x = 10i32;
    let mut y = 20i32;
    // Use at_path instead of at().at()
    partial
        .apply(&[
            Op::set().at_path(&[0]).imm(&mut x),
            Op::set().at_path(&[1]).imm(&mut y),
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

    let mut a = String::from("first");
    partial.apply(&[Op::set().at(0).imm(&mut a)]).unwrap();
    std::mem::forget(a);

    // Drop without setting field b - must clean up field a
    drop(partial);
}

#[test]
fn build_fails_then_drops_partial_struct() {
    // Same scenario but via build() returning error
    let mut partial = Partial::alloc::<TwoStrings>().unwrap();

    let mut a = String::from("will be cleaned up");
    partial.apply(&[Op::set().at(0).imm(&mut a)]).unwrap();
    std::mem::forget(a);

    // build() fails because b is not set, then Drop cleans up a
    let err = partial.build::<TwoStrings>().unwrap_err();
    assert!(matches!(err.kind, ReflectErrorKind::NotInitialized));
}

#[test]
fn set_field_wrong_type_poisons_partial() {
    let mut partial = Partial::alloc::<Point>().unwrap();

    // Set field 0
    let mut x = 10i32;
    partial.apply(&[Op::set().at(0).imm(&mut x)]).unwrap();

    // Try to set field 1 with wrong type - should fail and poison the Partial
    let mut wrong = String::from("oops");
    let err = partial
        .apply(&[Op::set().at(1).imm(&mut wrong)])
        .unwrap_err();
    assert!(matches!(err.kind, ReflectErrorKind::ShapeMismatch { .. }));

    // After an error, the Partial is poisoned - any further operations should fail
    let mut y = 20i32;
    let err = partial.apply(&[Op::set().at(1).imm(&mut y)]).unwrap_err();
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

    let mut x = 10i32;
    partial
        .apply(&[
            Op::set().at(0).imm(&mut x),
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
    partial.apply(&[Op::set().at(0).stage()]).unwrap();

    // Now we're in the inner frame - set its fields
    let mut x = 10i32;
    let mut y = 20i32;
    partial
        .apply(&[Op::set().at(0).imm(&mut x), Op::set().at(1).imm(&mut y)])
        .unwrap();

    // End the inner frame
    partial.apply(&[Op::end()]).unwrap();

    // Set the outer extra field
    let mut extra = 99i32;
    partial.apply(&[Op::set().at(1).imm(&mut extra)]).unwrap();

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
    partial.apply(&[Op::set().at(0).stage()]).unwrap();

    // Only set one field of inner
    let mut x = 10i32;
    partial.apply(&[Op::set().at(0).imm(&mut x)]).unwrap();

    // Try to end - should fail because inner.y (field 1) is not set
    let err = partial.apply(&[Op::end()]).unwrap_err();
    assert!(matches!(
        err.kind,
        ReflectErrorKind::MissingRequiredField { index: 1 }
    ));
}

#[test]
fn build_at_empty_path_fails() {
    let mut partial = Partial::alloc::<Point>().unwrap();

    let err = partial.apply(&[Op::set().stage()]).unwrap_err();
    assert!(matches!(err.kind, ReflectErrorKind::BuildAtEmptyPath));
}

#[test]
fn build_box_containing_struct() {
    let mut partial = Partial::alloc::<Box<Point>>().unwrap();

    // Use Build to enter the box (allocate the inner memory)
    partial.apply(&[Op::set().stage()]).unwrap();

    // Set inner struct fields
    let mut x = 10i32;
    let mut y = 20i32;
    partial
        .apply(&[Op::set().at(0).imm(&mut x), Op::set().at(1).imm(&mut y)])
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
    partial.apply(&[Op::set().stage()]).unwrap();

    // Set inner struct fields
    let mut x = 100i32;
    let mut y = 200i32;
    partial
        .apply(&[Op::set().at(0).imm(&mut x), Op::set().at(1).imm(&mut y)])
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
    partial.apply(&[Op::set().stage()]).unwrap();

    // Set inner struct fields
    let mut x = 1000i32;
    let mut y = 2000i32;
    partial
        .apply(&[Op::set().at(0).imm(&mut x), Op::set().at(1).imm(&mut y)])
        .unwrap();

    // End - this calls new_into_fn to create the actual Arc
    partial.apply(&[Op::end()]).unwrap();

    let result: Arc<Point> = partial.build().unwrap();
    assert_eq!(*result, Point { x: 1000, y: 2000 });
}

#[test]
fn root_path_navigates_to_root() {
    let mut partial = Partial::alloc::<Outer>().unwrap();

    // Enter inner struct
    partial.apply(&[Op::set().at(0).stage()]).unwrap();

    // Set both fields of inner (must be complete before navigating away)
    let mut x = 10i32;
    let mut y = 20i32;
    partial
        .apply(&[Op::set().at(0).imm(&mut x), Op::set().at(1).imm(&mut y)])
        .unwrap();

    // Use root() path to navigate back to root and set outer.extra
    // This ends the inner frame (which is now complete) and sets extra
    let mut extra = 42i32;
    partial
        .apply(&[Op::set().root().at(1).imm(&mut extra)])
        .unwrap();

    // We should now be at root with both fields complete
    let result: Outer = partial.build().unwrap();
    assert_eq!(
        result,
        Outer {
            inner: Point { x: 10, y: 20 },
            extra: 42
        }
    );
}

#[test]
fn root_path_fails_with_incomplete_frame() {
    let mut partial = Partial::alloc::<Outer>().unwrap();

    // Enter inner struct
    partial.apply(&[Op::set().at(0).stage()]).unwrap();

    // Set only one field of inner (incomplete)
    let mut x = 10i32;
    partial.apply(&[Op::set().at(0).imm(&mut x)]).unwrap();

    // Try to use root() - should fail because inner.y (field 1) is not set
    let mut extra = 42i32;
    let err = partial
        .apply(&[Op::set().root().at(1).imm(&mut extra)])
        .unwrap_err();
    assert!(matches!(
        err.kind,
        ReflectErrorKind::MissingRequiredField { index: 1 }
    ));
}

// Struct with a default field
#[derive(Debug, PartialEq, Facet)]
struct StructWithDefault {
    required: i32,
    #[facet(default)]
    optional: i32, // defaults to 0
}

#[test]
fn end_applies_defaults_for_missing_fields() {
    let mut partial = Partial::alloc::<StructWithDefault>().unwrap();

    // Only set the required field
    let mut required = 42i32;
    partial
        .apply(&[Op::set().at(0).imm(&mut required)])
        .unwrap();

    // build() should succeed - optional gets its default (0)
    let result: StructWithDefault = partial.build().unwrap();
    assert_eq!(
        result,
        StructWithDefault {
            required: 42,
            optional: 0
        }
    );
}

// Struct with Option field (auto-defaults to None)
#[derive(Debug, PartialEq, Facet)]
struct StructWithOption {
    required: i32,
    optional: Option<String>,
}

#[test]
fn end_applies_none_for_option_fields() {
    let mut partial = Partial::alloc::<StructWithOption>().unwrap();

    // Only set the required field
    let mut required = 42i32;
    partial
        .apply(&[Op::set().at(0).imm(&mut required)])
        .unwrap();

    // build() should succeed - optional gets None
    let result: StructWithOption = partial.build().unwrap();
    assert_eq!(
        result,
        StructWithOption {
            required: 42,
            optional: None
        }
    );
}

// Struct with custom default expression
#[derive(Debug, PartialEq, Facet)]
struct StructWithCustomDefault {
    required: i32,
    #[facet(default = 999)]
    custom: i32,
}

#[test]
fn end_applies_custom_defaults() {
    let mut partial = Partial::alloc::<StructWithCustomDefault>().unwrap();

    // Only set the required field
    let mut required = 42i32;
    partial
        .apply(&[Op::set().at(0).imm(&mut required)])
        .unwrap();

    // build() should succeed - custom gets 999
    let result: StructWithCustomDefault = partial.build().unwrap();
    assert_eq!(
        result,
        StructWithCustomDefault {
            required: 42,
            custom: 999
        }
    );
}
