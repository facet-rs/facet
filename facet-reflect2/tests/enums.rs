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
    partial.apply(&[Op::set().at(1).build()]).unwrap();

    // Inside the variant frame, set x and y
    let x = 10i32;
    let y = 20i32;
    partial
        .apply(&[Op::set().at(0).mov(&x), Op::set().at(1).mov(&y)])
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
    let msg = String::from("hello");
    partial.apply(&[Op::set().at(2).mov(&msg)]).unwrap();
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
    let id = 42u32;
    partial.apply(&[Op::set().at(0).mov(&id)]).unwrap();

    // Build message field, select Move variant
    partial.apply(&[Op::set().at(1).build()]).unwrap();

    // Select variant 1 (Move) inside the message frame
    partial.apply(&[Op::set().at(1).build()]).unwrap();

    // Set Move's fields
    let x = 100i32;
    let y = 200i32;
    partial
        .apply(&[Op::set().at(0).mov(&x), Op::set().at(1).mov(&y)])
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
    partial.apply(&[Op::set().at(1).build()]).unwrap();

    // Only set x, not y
    let x = 10i32;
    partial.apply(&[Op::set().at(0).mov(&x)]).unwrap();

    // Try to end - should fail because y is not set
    let err = partial.apply(&[Op::end()]).unwrap_err();
    assert!(matches!(err.kind, ReflectErrorKind::EndWithIncomplete));
}

#[test]
fn drop_partially_initialized_enum() {
    // Partially initialize an enum with a String field, then drop without build
    let mut partial = Partial::alloc::<Message>().unwrap();

    // Select Write variant and set the string
    let msg = String::from("will be dropped");
    partial.apply(&[Op::set().at(2).mov(&msg)]).unwrap();
    std::mem::forget(msg);

    // Drop without building - must clean up the string
    drop(partial);
}
