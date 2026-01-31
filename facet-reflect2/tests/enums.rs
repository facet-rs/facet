use facet::Facet;
use facet_reflect2::{Op, Partial, ReflectErrorKind};

#[derive(Debug, PartialEq, Facet)]
#[repr(u8)]
enum Message {
    Quit,
    Move { x: i32, y: i32 },
    Write(String),
}

#[test]
fn enum_unit_variant() {
    let mut partial = Partial::alloc::<Message>().unwrap();

    // Select Quit variant (index 0) with Default
    partial.apply(&[Op::set().at(0).default()]).unwrap();

    let result: Message = partial.build().unwrap();
    assert_eq!(result, Message::Quit);
}

#[test]
fn enum_struct_variant() {
    let mut partial = Partial::alloc::<Message>().unwrap();

    // Select Move variant (index 1) with Build, then set fields
    partial.apply(&[Op::set().at(1).stage()]).unwrap();

    // Inside the variant frame, set x and y
    let mut x = 10i32;
    let mut y = 20i32;
    partial
        .apply(&[Op::set().at(0).imm(&mut x), Op::set().at(1).imm(&mut y)])
        .unwrap();

    // End the variant frame
    partial.apply(&[Op::end()]).unwrap();

    let result: Message = partial.build().unwrap();
    assert_eq!(result, Message::Move { x: 10, y: 20 });
}

#[test]
fn enum_tuple_variant() {
    let mut partial = Partial::alloc::<Message>().unwrap();

    // Select Write variant (index 2) with Move (complete value)
    let mut msg = String::from("hello");
    partial.apply(&[Op::set().at(2).imm(&mut msg)]).unwrap();
    std::mem::forget(msg);

    let result: Message = partial.build().unwrap();
    assert_eq!(result, Message::Write("hello".to_string()));
}

#[test]
fn enum_variant_index_out_of_bounds() {
    let mut partial = Partial::alloc::<Message>().unwrap();

    // Message has 3 variants (0, 1, 2), try index 5
    let err = partial.apply(&[Op::set().at(5).default()]).unwrap_err();
    assert!(matches!(
        err.kind,
        ReflectErrorKind::VariantIndexOutOfBounds {
            index: 5,
            variant_count: 3
        }
    ));
}

// C-style enum (all unit variants)
#[derive(Debug, PartialEq, Facet)]
#[repr(u8)]
enum Color {
    Red,
    Green,
    Blue,
}

#[test]
fn enum_c_style() {
    let mut partial = Partial::alloc::<Color>().unwrap();

    // Select Green variant (index 1)
    partial.apply(&[Op::set().at(1).default()]).unwrap();

    let result: Color = partial.build().unwrap();
    assert_eq!(result, Color::Green);
}

// Enum with explicit discriminants
#[derive(Debug, PartialEq, Facet)]
#[repr(u8)]
enum Status {
    Pending = 1,
    Active = 5,
    Done = 10,
}

#[test]
fn enum_explicit_discriminants() {
    let mut partial = Partial::alloc::<Status>().unwrap();

    // Select Active variant (index 1, discriminant 5)
    partial.apply(&[Op::set().at(1).default()]).unwrap();

    let result: Status = partial.build().unwrap();
    assert_eq!(result, Status::Active);
}

// Nested enum in struct
#[derive(Debug, PartialEq, Facet)]
struct Event {
    id: u32,
    message: Message,
}

#[test]
fn nested_enum_in_struct() {
    let mut partial = Partial::alloc::<Event>().unwrap();

    // Set id
    let mut id = 42u32;
    partial.apply(&[Op::set().at(0).imm(&mut id)]).unwrap();

    // Build message field, select Move variant
    partial.apply(&[Op::set().at(1).stage()]).unwrap();

    // Select variant 1 (Move) inside the message frame
    partial.apply(&[Op::set().at(1).stage()]).unwrap();

    // Set Move's fields
    let mut x = 100i32;
    let mut y = 200i32;
    partial
        .apply(&[Op::set().at(0).imm(&mut x), Op::set().at(1).imm(&mut y)])
        .unwrap();

    // End Move variant frame
    partial.apply(&[Op::end()]).unwrap();

    // End message field frame
    partial.apply(&[Op::end()]).unwrap();

    let result: Event = partial.build().unwrap();
    assert_eq!(
        result,
        Event {
            id: 42,
            message: Message::Move { x: 100, y: 200 }
        }
    );
}

#[test]
fn enum_incomplete_variant_fails() {
    let mut partial = Partial::alloc::<Message>().unwrap();

    // Select Move variant with Build
    partial.apply(&[Op::set().at(1).stage()]).unwrap();

    // Only set x, not y
    let mut x = 10i32;
    partial.apply(&[Op::set().at(0).imm(&mut x)]).unwrap();

    // Try to end - should fail because y is not set
    let err = partial.apply(&[Op::end()]).unwrap_err();
    assert!(matches!(err.kind, ReflectErrorKind::EndWithIncomplete));
}

#[test]
fn drop_partially_initialized_enum() {
    // Partially initialize an enum with a String field, then drop without build
    let mut partial = Partial::alloc::<Message>().unwrap();

    // Select Write variant and set the string
    let mut msg = String::from("will be dropped");
    partial.apply(&[Op::set().at(2).imm(&mut msg)]).unwrap();
    std::mem::forget(msg);

    // Drop without building - must clean up the string
    drop(partial);
}

// ============================================================================
// Bug regression tests (run under Miri to detect memory issues)
// ============================================================================

// Enum with heap-allocated variant for testing memory safety
#[derive(Debug, PartialEq, Facet)]
#[repr(u8)]
enum MaybeBox {
    Empty,
    Boxed(Box<u32>),
    Named { value: Option<String> },
}

#[test]
fn enum_switch_variant_drops_old_value() {
    // Bug: When switching enum variants, the old variant's data must be dropped.
    // After Move of enum at [], FrameKind::Enum { selected } must track the
    // active variant (by reading the discriminant), so subsequent variant
    // switches know what to drop.

    let mut partial = Partial::alloc::<MaybeBox>().unwrap();

    // Move a complete enum value with heap allocation
    let mut value = MaybeBox::Boxed(Box::new(42));
    partial.apply(&[Op::set().imm(&mut value)]).unwrap();
    std::mem::forget(value);

    // Switch to a different variant - this must:
    // 1. Read the discriminant to know Boxed variant is active
    // 2. Drop the Box<u32> before switching variants
    // 3. Write the new discriminant
    partial.apply(&[Op::set().at(0).default()]).unwrap(); // Select Empty

    let result: MaybeBox = partial.build().unwrap();
    assert_eq!(result, MaybeBox::Empty);
}

#[test]
fn enum_reselect_variant_with_wrong_type_fails() {
    // AFL crash case: Set variant 1 with correct type, then try to set
    // variant 1 again with wrong type. Should fail with shape mismatch,
    // not hang or crash.

    let mut partial = Partial::alloc::<MaybeBox>().unwrap();

    // Set variant 1 (Boxed) with a Box<u32>
    let mut boxed = Box::new(42u32);
    partial.apply(&[Op::set().at(1).imm(&mut boxed)]).unwrap();
    std::mem::forget(boxed);

    // Try to set variant 1 again with wrong type (bool instead of Box<u32>)
    let mut b = false;
    let err = partial.apply(&[Op::set().at(1).imm(&mut b)]).unwrap_err();
    assert!(matches!(err.kind, ReflectErrorKind::ShapeMismatch { .. }));
}
