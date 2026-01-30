use facet_core::{Facet, PtrConst};
use facet_reflect2::{Move, Op, Partial, ReflectErrorKind, Source};

#[test]
fn set_u32_twice() {
    let mut partial = Partial::alloc::<u32>().unwrap();

    let value1 = 42u32;
    partial.apply(&[Op::set().mov(&value1)]).unwrap();

    // Set again with a different value - should drop the previous one
    let value2 = 99u32;
    partial.apply(&[Op::set().mov(&value2)]).unwrap();

    let result: u32 = partial.build().unwrap();
    assert_eq!(result, 99);
}

#[test]
fn set_string_twice() {
    let mut partial = Partial::alloc::<String>().unwrap();

    let value1 = String::from("hello");
    partial.apply(&[Op::set().mov(&value1)]).unwrap();
    std::mem::forget(value1);

    // Set again - this should drop "hello" before writing "world"
    let value2 = String::from("world");
    partial.apply(&[Op::set().mov(&value2)]).unwrap();
    std::mem::forget(value2);

    let result: String = partial.build().unwrap();
    assert_eq!(result, "world");
}

#[test]
fn set_u32() {
    let mut partial = Partial::alloc::<u32>().unwrap();

    let value = 42u32;
    partial.apply(&[Op::set().mov(&value)]).unwrap();

    let result: u32 = partial.build().unwrap();
    assert_eq!(result, 42);
}

#[test]
fn set_wrong_type() {
    let mut partial = Partial::alloc::<u32>().unwrap();

    // Try to set a String into a u32 slot
    let value = String::from("hello");
    let err = partial.apply(&[Op::set().mov(&value)]).unwrap_err();

    assert!(matches!(err.kind, ReflectErrorKind::ShapeMismatch { .. }));
}

#[test]
fn set_with_raw_move() {
    let mut partial = Partial::alloc::<u64>().unwrap();

    let value = 123u64;
    // Use the unsafe Move::new constructor with raw pointer and shape
    let mov = unsafe { Move::new(PtrConst::new(&value), u64::SHAPE) };
    partial
        .apply(&[Op::Set {
            path: Default::default(),
            source: Source::Move(mov),
        }])
        .unwrap();

    let result: u64 = partial.build().unwrap();
    assert_eq!(result, 123);
}

#[test]
fn set_zst() {
    let mut partial = Partial::alloc::<()>().unwrap();

    let value = ();
    partial.apply(&[Op::set().mov(&value)]).unwrap();

    let result: () = partial.build().unwrap();
    assert_eq!(result, ());
}
