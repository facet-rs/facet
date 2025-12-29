use facet::Facet;
use facet_reflect::{Poke, ReflectError};

#[derive(Debug, Facet, PartialEq)]
#[facet(pod)]
#[repr(u8)]
enum SimpleEnum {
    Unit,
    #[allow(dead_code)]
    Tuple(u32),
    #[allow(dead_code)]
    Struct {
        a: u8,
        b: String,
    },
}

#[test]
fn poke_enum_into_enum() {
    let mut value = SimpleEnum::Unit;
    let poke = Poke::new(&mut value);

    let poke_enum = poke.into_enum().expect("SimpleEnum is an enum");
    assert_eq!(poke_enum.variant_name_active().unwrap(), "Unit");
}

#[test]
fn poke_enum_variant_info() {
    let mut value = SimpleEnum::Tuple(42);
    let poke = Poke::new(&mut value);
    let poke_enum = poke.into_enum().unwrap();

    assert_eq!(poke_enum.variant_count(), 3);
    assert_eq!(poke_enum.variant_name(0), Some("Unit"));
    assert_eq!(poke_enum.variant_name(1), Some("Tuple"));
    assert_eq!(poke_enum.variant_name(2), Some("Struct"));
}

#[test]
fn poke_enum_field_access() {
    let mut value = SimpleEnum::Tuple(42);
    let poke = Poke::new(&mut value);
    let mut poke_enum = poke.into_enum().unwrap();

    // Get field
    let field = poke_enum.field(0).unwrap().unwrap();
    assert_eq!(*field.get::<u32>().unwrap(), 42);
}

#[test]
fn poke_enum_set_field_pod() {
    let mut value = SimpleEnum::Tuple(42);
    {
        let poke = Poke::new(&mut value);
        let mut poke_enum = poke.into_enum().unwrap();
        poke_enum.set_field(0, 100u32).unwrap();
    }

    match value {
        SimpleEnum::Tuple(v) => assert_eq!(v, 100),
        _ => panic!("Expected Tuple variant"),
    }
}

#[test]
fn poke_enum_set_field_by_name() {
    let mut value = SimpleEnum::Struct {
        a: 1,
        b: "hello".to_string(),
    };
    {
        let poke = Poke::new(&mut value);
        let mut poke_enum = poke.into_enum().unwrap();
        poke_enum.set_field_by_name("a", 99u8).unwrap();
        poke_enum
            .set_field_by_name("b", "world".to_string())
            .unwrap();
    }

    match value {
        SimpleEnum::Struct { a, b } => {
            assert_eq!(a, 99);
            assert_eq!(b, "world");
        }
        _ => panic!("Expected Struct variant"),
    }
}

#[test]
fn poke_enum_non_pod_fails() {
    #[derive(Debug, Facet, PartialEq)]
    #[repr(u8)]
    enum NotPod {
        #[allow(dead_code)]
        Value(u32),
    }

    let mut value = NotPod::Value(42);
    let poke = Poke::new(&mut value);
    let mut poke_enum = poke.into_enum().unwrap();

    // Setting field on non-POD should fail
    let result = poke_enum.set_field(0, 100u32);
    assert!(matches!(result, Err(ReflectError::NotPod { .. })));
}

#[test]
fn poke_enum_wrong_field_type() {
    let mut value = SimpleEnum::Tuple(42);
    let poke = Poke::new(&mut value);
    let mut poke_enum = poke.into_enum().unwrap();

    // Try to set u32 field with i32
    let result = poke_enum.set_field(0, 100i32);
    assert!(matches!(result, Err(ReflectError::WrongShape { .. })));
}

#[test]
fn poke_enum_field_index_out_of_bounds() {
    let mut value = SimpleEnum::Tuple(42);
    let poke = Poke::new(&mut value);
    let mut poke_enum = poke.into_enum().unwrap();

    // Tuple variant only has one field (index 0)
    let result = poke_enum.set_field(99, 100u32);
    assert!(matches!(result, Err(ReflectError::FieldError { .. })));
}

#[test]
fn poke_enum_no_such_field() {
    let mut value = SimpleEnum::Struct {
        a: 1,
        b: "hello".to_string(),
    };
    let poke = Poke::new(&mut value);
    let mut poke_enum = poke.into_enum().unwrap();

    let result = poke_enum.set_field_by_name("nonexistent", 100u32);
    assert!(matches!(result, Err(ReflectError::FieldError { .. })));
}

#[test]
fn poke_enum_peek_field() {
    let mut value = SimpleEnum::Tuple(42);
    let poke = Poke::new(&mut value);
    let poke_enum = poke.into_enum().unwrap();

    let peek = poke_enum.peek_field(0).unwrap().unwrap();
    assert_eq!(*peek.get::<u32>().unwrap(), 42);
}

#[test]
fn poke_enum_as_peek_enum() {
    let mut value = SimpleEnum::Unit;
    let poke = Poke::new(&mut value);
    let poke_enum = poke.into_enum().unwrap();

    let peek_enum = poke_enum.as_peek_enum();
    assert_eq!(peek_enum.variant_count(), 3);
    assert_eq!(peek_enum.variant_name_active().unwrap(), "Unit");
}

#[test]
fn poke_enum_into_inner() {
    let mut value = SimpleEnum::Unit;
    let poke = Poke::new(&mut value);
    let poke_enum = poke.into_enum().unwrap();

    let poke_back = poke_enum.into_inner();
    assert_eq!(poke_back.shape(), SimpleEnum::SHAPE);
}

#[test]
fn poke_not_enum_fails() {
    #[derive(Debug, Facet)]
    struct NotAnEnum {
        x: i32,
    }

    let mut value = NotAnEnum { x: 42 };
    let poke = Poke::new(&mut value);

    let result = poke.into_enum();
    assert!(matches!(result, Err(ReflectError::WasNotA { .. })));
}
