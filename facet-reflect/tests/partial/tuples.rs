use facet_reflect::Partial;
use facet_testhelpers::test;

#[cfg(not(miri))]
macro_rules! assert_snapshot {
    ($($tt:tt)*) => {
        insta::assert_snapshot!($($tt)*);
    };
}
#[cfg(miri)]
macro_rules! assert_snapshot {
    ($($tt:tt)*) => {{ let _ = $($tt)*; }};
}

#[test]
fn build_empty_tuple() {
    // Test building ()
    let partial: Partial<'_, '_> = Partial::alloc::<()>().unwrap();
    partial.build().unwrap();
}

#[test]
fn build_single_empty_tuple() {
    // Test building (())
    let mut partial: Partial<'_, '_> = Partial::alloc::<((),)>().unwrap();

    // Field 0 is of type ()
    partial = partial.begin_nth_field(0).unwrap();
    // Now we're working with type (), which has no fields
    partial = partial.end().unwrap();

    let single_empty = partial.build().unwrap().materialize::<((),)>().unwrap();
    assert_eq!(single_empty, ((),));
}

#[test]
fn build_double_empty_tuple() {
    // Test building ((()),)
    let mut partial: Partial<'_, '_> = Partial::alloc::<(((),),)>().unwrap();

    // Field 0 is of type (())
    partial = partial.begin_nth_field(0).unwrap();

    // Now we're in (()) - field 0 is of type ()
    partial = partial.begin_nth_field(0).unwrap();
    // Now we're working with type (), which has no fields
    partial = partial.end().unwrap();

    // End the (()) field
    partial = partial.end().unwrap();

    let double_empty = partial.build().unwrap().materialize::<(((),),)>().unwrap();
    assert_eq!(double_empty, (((),),));
}

#[test]
fn build_mixed_tuple() {
    // Test building (String, i32)
    let mut partial: Partial<'_, '_> = Partial::alloc::<(String, i32)>().unwrap();

    partial = partial.begin_nth_field(0).unwrap();
    partial = partial.set("Hello".to_string()).unwrap();
    partial = partial.end().unwrap();

    partial = partial.begin_nth_field(1).unwrap();
    partial = partial.set(42i32).unwrap();
    partial = partial.end().unwrap();

    let mixed = partial
        .build()
        .unwrap()
        .materialize::<(String, i32)>()
        .unwrap();
    assert_eq!(mixed, ("Hello".to_string(), 42));
}

#[test]
fn build_nested_tuple() {
    // Test building ((String, i32), bool)
    let mut partial: Partial<'_, '_> = Partial::alloc::<((String, i32), bool)>().unwrap();

    // Field 0 is of type (String, i32)
    partial = partial.begin_nth_field(0).unwrap();

    partial = partial.begin_nth_field(0).unwrap();
    partial = partial.set("World".to_string()).unwrap();
    partial = partial.end().unwrap();

    partial = partial.begin_nth_field(1).unwrap();
    partial = partial.set(99i32).unwrap();
    partial = partial.end().unwrap();

    partial = partial.end().unwrap();

    // Field 1 is of type bool
    partial = partial.begin_nth_field(1).unwrap();
    partial = partial.set(true).unwrap();
    partial = partial.end().unwrap();

    let nested = partial
        .build()
        .unwrap()
        .materialize::<((String, i32), bool)>()
        .unwrap();
    assert_eq!(nested, (("World".to_string(), 99), true));
}

#[test]
fn test_issue_691_tuple_too_few_fields() {
    // This test verifies that issue #691 is fixed: attempting to build a tuple
    // with too few fields should return an error, not cause unsoundness.
    // The original issue showed that with the old Wip API, building a tuple
    // with insufficient fields could lead to accessing uninitialized memory.

    // Test case 1: 2-element tuple with only 1 field initialized
    let mut partial: Partial<'_, '_> = Partial::alloc::<(String, String)>().unwrap();
    partial = partial.begin_nth_field(0).unwrap();
    partial = partial.set("a".to_string()).unwrap();
    partial = partial.end().unwrap();
    // Should fail because we didn't initialize the second field
    assert_snapshot!(partial.build().unwrap_err());
}

#[test]
fn test_issue_691_3_tuple_missing_field() {
    // Test case 2: 3-element tuple with only 2 fields initialized
    let mut partial: Partial<'_, '_> = Partial::alloc::<(String, i32, bool)>().unwrap();
    partial = partial.begin_nth_field(0).unwrap();
    partial = partial.set("hello".to_string()).unwrap();
    partial = partial.end().unwrap();
    partial = partial.begin_nth_field(1).unwrap();
    partial = partial.set(42).unwrap();
    partial = partial.end().unwrap();
    // Should fail because we didn't initialize the third field
    assert_snapshot!(partial.build().unwrap_err());
}

#[test]
fn test_issue_691_nested_tuple_incomplete() {
    // Test case 3: Nested tuple with inner tuple not fully initialized
    let mut partial: Partial<'_, '_> = Partial::alloc::<((String, i32), bool)>().unwrap();
    partial = partial.begin_nth_field(0).unwrap();
    partial = partial.begin_nth_field(0).unwrap();
    partial = partial.set("nested".to_string()).unwrap();
    partial = partial.end().unwrap();
    // We didn't set the i32 field of the inner tuple
    // The error should occur when we try to end the inner tuple frame
    let err = match partial.end() {
        Ok(_) => panic!("expected error"),
        Err(e) => e,
    };
    assert_snapshot!(err);
}

#[test]
fn test_issue_691_valid_tuples() {
    // Test case 4: Empty tuple should work (no fields to initialize)
    let partial: Partial<'_, '_> = Partial::alloc::<()>().unwrap();
    let result = partial.build();
    assert!(result.is_ok(), "Building empty tuple should succeed");

    // Test case 5: Single-element tuple with field initialized should work
    let mut partial: Partial<'_, '_> = Partial::alloc::<(String,)>().unwrap();
    partial = partial.begin_nth_field(0).unwrap();
    partial = partial.set("single".to_string()).unwrap();
    partial = partial.end().unwrap();
    let result = partial.build();
    assert!(
        result.is_ok(),
        "Building single-element tuple with field initialized should succeed"
    );
}

// =============================================================================
// Tests migrated from src/partial/tests.rs
// =============================================================================

use facet_testhelpers::IPanic;

#[test]
fn tuple_basic() -> Result<(), IPanic> {
    let value = Partial::alloc::<(i32, String)>()?
        .set_nth_field(0, 42i32)?
        .set_nth_field(1, "hello".to_string())?
        .build()?
        .materialize::<(i32, String)>()?;
    assert_eq!(value, (42, "hello".to_string()));
    Ok(())
}

#[test]
fn tuple_mixed_types() -> Result<(), IPanic> {
    let value = Partial::alloc::<(u8, bool, f64, String)>()?
        .set_nth_field(2, 56.124f64)?
        .set_nth_field(0, 255u8)?
        .set_nth_field(3, "world".to_string())?
        .set_nth_field(1, true)?
        .build()?
        .materialize::<(u8, bool, f64, String)>()?;
    assert_eq!(value, (255u8, true, 56.124f64, "world".to_string()));
    Ok(())
}

#[test]
fn tuple_nested() -> Result<(), IPanic> {
    let value = Partial::alloc::<((i32, i32), String)>()?
        .begin_nth_field(0)?
        .set_nth_field(0, 1i32)?
        .set_nth_field(1, 2i32)?
        .end()?
        .set_nth_field(1, "nested".to_string())?
        .build()?
        .materialize::<((i32, i32), String)>()?;
    assert_eq!(value, ((1, 2), "nested".to_string()));
    Ok(())
}

#[test]
fn tuple_empty() -> Result<(), IPanic> {
    Partial::alloc::<()>()?
        .set(())?
        .build()?
        .materialize::<()>()?;
    Ok(())
}
