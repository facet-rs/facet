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
    ($($tt:tt)*) => {
        /* no-op under miri */
    };
}

#[test]
fn build_empty_tuple() {
    // Test building ()
    let mut partial = Partial::alloc::<()>()?;
    partial.build()?;
}

#[test]
fn build_single_empty_tuple() {
    // Test building (())
    let mut partial = Partial::alloc::<((),)>()?;

    // Field 0 is of type ()
    partial.begin_nth_field(0)?;
    // Now we're working with type (), which has no fields
    partial.end()?;

    let single_empty = *partial.build()?;
    assert_eq!(single_empty, ((),));
}

#[test]
fn build_double_empty_tuple() {
    // Test building ((()),)
    let mut partial = Partial::alloc::<(((),),)>()?;

    // Field 0 is of type (())
    partial.begin_nth_field(0)?;

    // Now we're in (()) - field 0 is of type ()
    partial.begin_nth_field(0)?;
    // Now we're working with type (), which has no fields
    partial.end()?;

    // End the (()) field
    partial.end()?;

    let double_empty = *partial.build()?;
    assert_eq!(double_empty, (((),),));
}

#[test]
fn build_mixed_tuple() {
    // Test building (String, i32)
    let mut partial = Partial::alloc::<(String, i32)>()?;

    partial.begin_nth_field(0)?;
    partial.set("Hello".to_string())?;
    partial.end()?;

    partial.begin_nth_field(1)?;
    partial.set(42i32)?;
    partial.end()?;

    let mixed = *partial.build()?;
    assert_eq!(mixed, ("Hello".to_string(), 42));
}

#[test]
fn build_nested_tuple() {
    // Test building ((String, i32), bool)
    let mut partial = Partial::alloc::<((String, i32), bool)>()?;

    // Field 0 is of type (String, i32)
    partial.begin_nth_field(0)?;

    partial.begin_nth_field(0)?;
    partial.set("World".to_string())?;
    partial.end()?;

    partial.begin_nth_field(1)?;
    partial.set(99i32)?;
    partial.end()?;

    partial.end()?;

    // Field 1 is of type bool
    partial.begin_nth_field(1)?;
    partial.set(true)?;
    partial.end()?;

    let nested = *partial.build()?;
    assert_eq!(nested, (("World".to_string(), 99), true));
}

#[test]
fn test_issue_691_tuple_too_few_fields() {
    // This test verifies that issue #691 is fixed: attempting to build a tuple
    // with too few fields should return an error, not cause unsoundness.
    // The original issue showed that with the old Wip API, building a tuple
    // with insufficient fields could lead to accessing uninitialized memory.

    // Test case 1: 2-element tuple with only 1 field initialized
    let mut partial = Partial::alloc::<(String, String)>()?;
    partial.begin_nth_field(0)?;
    partial.set("a".to_string())?;
    partial.end()?;
    // Should fail because we didn't initialize the second field
    assert_snapshot!(partial.build().unwrap_err());
}

#[test]
fn test_issue_691_3_tuple_missing_field() {
    // Test case 2: 3-element tuple with only 2 fields initialized
    let mut partial = Partial::alloc::<(String, i32, bool)>()?;
    partial.begin_nth_field(0)?;
    partial.set("hello".to_string())?;
    partial.end()?;
    partial.begin_nth_field(1)?;
    partial.set(42)?;
    partial.end()?;
    // Should fail because we didn't initialize the third field
    assert_snapshot!(partial.build().unwrap_err());
}

#[test]
fn test_issue_691_nested_tuple_incomplete() {
    // Test case 3: Nested tuple with inner tuple not fully initialized
    let mut partial = Partial::alloc::<((String, i32), bool)>()?;
    partial.begin_nth_field(0)?;
    partial.begin_nth_field(0)?;
    partial.set("nested".to_string())?;
    partial.end()?;
    // We didn't set the i32 field of the inner tuple
    // The error should occur when we try to end the inner tuple frame
    assert_snapshot!(partial.end().unwrap_err());
}

#[test]
fn test_issue_691_valid_tuples() {
    // Test case 4: Empty tuple should work (no fields to initialize)
    let mut partial = Partial::alloc::<()>()?;
    let result = partial.build();
    assert!(result.is_ok(), "Building empty tuple should succeed");

    // Test case 5: Single-element tuple with field initialized should work
    let mut partial = Partial::alloc::<(String,)>()?;
    partial.begin_nth_field(0)?;
    partial.set("single".to_string())?;
    partial.end()?;
    let result = partial.build();
    assert!(
        result.is_ok(),
        "Building single-element tuple with field initialized should succeed"
    );
}
