use facet_reflect::Partial;
use facet_testhelpers::test;

#[test]
fn build_empty_tuple() {
    // Test building ()
    let mut partial = Partial::alloc::<()>()?;
    let empty_tuple = *partial.build()?;
    assert_eq!(empty_tuple, ());
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
