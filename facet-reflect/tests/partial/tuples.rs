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
    let mut partial = Partial::alloc::<()>().unwrap();
    partial.build().unwrap();
}

#[test]
fn build_single_empty_tuple() {
    // Test building (())
    let mut partial = Partial::alloc::<((),)>().unwrap();

    // Field 0 is of type ()
    partial.begin_nth_field(0).unwrap();
    // Now we're working with type (), which has no fields
    partial.end().unwrap();

    let single_empty = *partial.build().unwrap();
    assert_eq!(single_empty, ((),));
}

#[test]
fn build_double_empty_tuple() {
    // Test building ((()),)
    let mut partial = Partial::alloc::<(((),),)>().unwrap();

    // Field 0 is of type (())
    partial.begin_nth_field(0).unwrap();

    // Now we're in (()) - field 0 is of type ()
    partial.begin_nth_field(0).unwrap();
    // Now we're working with type (), which has no fields
    partial.end().unwrap();

    // End the (()) field
    partial.end().unwrap();

    let double_empty = *partial.build().unwrap();
    assert_eq!(double_empty, (((),),));
}

#[test]
fn build_mixed_tuple() {
    // Test building (String, i32)
    let mut partial = Partial::alloc::<(String, i32)>().unwrap();

    partial.begin_nth_field(0).unwrap();
    partial.set("Hello".to_string()).unwrap();
    partial.end().unwrap();

    partial.begin_nth_field(1).unwrap();
    partial.set(42i32).unwrap();
    partial.end().unwrap();

    let mixed = *partial.build().unwrap();
    assert_eq!(mixed, ("Hello".to_string(), 42));
}

#[test]
fn build_nested_tuple() {
    // Test building ((String, i32), bool)
    let mut partial = Partial::alloc::<((String, i32), bool)>().unwrap();

    // Field 0 is of type (String, i32)
    partial.begin_nth_field(0).unwrap();

    partial.begin_nth_field(0).unwrap();
    partial.set("World".to_string()).unwrap();
    partial.end().unwrap();

    partial.begin_nth_field(1).unwrap();
    partial.set(99i32).unwrap();
    partial.end().unwrap();

    partial.end().unwrap();

    // Field 1 is of type bool
    partial.begin_nth_field(1).unwrap();
    partial.set(true).unwrap();
    partial.end().unwrap();

    let nested = *partial.build().unwrap();
    assert_eq!(nested, (("World".to_string(), 99), true));
}

#[test]
fn test_issue_691_tuple_too_few_fields() {
    // This test verifies that issue #691 is fixed: attempting to build a tuple
    // with too few fields should return an error, not cause unsoundness.
    // The original issue showed that with the old Wip API, building a tuple
    // with insufficient fields could lead to accessing uninitialized memory.

    // Test case 1: 2-element tuple with only 1 field initialized
    let mut partial = Partial::alloc::<(String, String)>().unwrap();
    partial.begin_nth_field(0).unwrap();
    partial.set("a".to_string()).unwrap();
    partial.end().unwrap();
    // Should fail because we didn't initialize the second field
    assert_snapshot!(partial.build().unwrap_err());
}

#[test]
fn test_issue_691_3_tuple_missing_field() {
    // Test case 2: 3-element tuple with only 2 fields initialized
    let mut partial = Partial::alloc::<(String, i32, bool)>().unwrap();
    partial.begin_nth_field(0).unwrap();
    partial.set("hello".to_string()).unwrap();
    partial.end().unwrap();
    partial.begin_nth_field(1).unwrap();
    partial.set(42).unwrap();
    partial.end().unwrap();
    // Should fail because we didn't initialize the third field
    assert_snapshot!(partial.build().unwrap_err());
}

#[test]
fn test_issue_691_nested_tuple_incomplete() {
    // Test case 3: Nested tuple with inner tuple not fully initialized
    let mut partial = Partial::alloc::<((String, i32), bool)>().unwrap();
    partial.begin_nth_field(0).unwrap();
    partial.begin_nth_field(0).unwrap();
    partial.set("nested".to_string()).unwrap();
    partial.end().unwrap();
    // We didn't set the i32 field of the inner tuple
    // The error should occur when we try to end the inner tuple frame
    assert_snapshot!(partial.end().unwrap_err());
}

#[test]
fn test_issue_691_valid_tuples() {
    // Test case 4: Empty tuple should work (no fields to initialize)
    let mut partial = Partial::alloc::<()>().unwrap();
    let result = partial.build();
    assert!(result.is_ok(), "Building empty tuple should succeed");

    // Test case 5: Single-element tuple with field initialized should work
    let mut partial = Partial::alloc::<(String,)>().unwrap();
    partial.begin_nth_field(0).unwrap();
    partial.set("single".to_string()).unwrap();
    partial.end().unwrap();
    let result = partial.build();
    assert!(
        result.is_ok(),
        "Building single-element tuple with field initialized should succeed"
    );
}
