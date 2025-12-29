use facet::Facet;
use facet_reflect::{Poke, ReflectError};

#[test]
fn poke_pod_struct() {
    #[derive(Debug, Facet, PartialEq)]
    #[facet(pod)]
    struct Point {
        x: i32,
        y: i32,
    }

    let mut point = Point { x: 1, y: 2 };
    let poke = Poke::new(&mut point);

    assert_eq!(poke.shape(), Point::SHAPE);
}

#[test]
fn poke_non_pod_struct_can_be_created() {
    // Poke::new() works on any type - it's set_field that requires POD
    #[derive(Debug, Facet)]
    struct NotPod {
        x: i32,
    }

    let mut value = NotPod { x: 42 };
    let poke = Poke::new(&mut value);

    assert_eq!(poke.shape(), NotPod::SHAPE);
}

#[test]
fn poke_non_pod_struct_set_field_fails() {
    #[derive(Debug, Facet)]
    struct NotPod {
        x: i32,
    }

    let mut value = NotPod { x: 42 };
    let poke = Poke::new(&mut value);
    let mut poke_struct = poke.into_struct().unwrap();

    // Setting a field on a non-POD struct should fail
    let result = poke_struct.set_field_by_name("x", 100i32);
    assert!(matches!(result, Err(ReflectError::NotPod { .. })));
}

#[test]
fn poke_non_pod_struct_wholesale_replace_works() {
    #[derive(Debug, Facet, PartialEq)]
    struct NotPod {
        x: i32,
    }

    let mut value = NotPod { x: 42 };
    let mut poke = Poke::new(&mut value);

    // Wholesale replacement always works
    poke.set(NotPod { x: 100 }).unwrap();
    assert_eq!(value.x, 100);
}

#[test]
fn poke_primitive() {
    let mut x: i32 = 42;
    let mut poke = Poke::new(&mut x);

    assert_eq!(*poke.get::<i32>().unwrap(), 42);

    poke.set(100i32).unwrap();
    assert_eq!(x, 100);
}

#[test]
fn poke_get_mut() {
    let mut x: i32 = 42;
    let mut poke = Poke::new(&mut x);

    *poke.get_mut::<i32>().unwrap() = 99;
    assert_eq!(x, 99);
}

#[test]
fn poke_wrong_type_fails() {
    let mut x: i32 = 42;
    let poke = Poke::new(&mut x);

    let result = poke.get::<u32>();
    assert!(matches!(result, Err(ReflectError::WrongShape { .. })));
}

#[test]
fn poke_set_wrong_type_fails() {
    let mut x: i32 = 42;
    let mut poke = Poke::new(&mut x);

    let result = poke.set(42u32);
    assert!(matches!(result, Err(ReflectError::WrongShape { .. })));
}

#[test]
fn poke_string_wholesale_replace() {
    // String is not POD, but wholesale replacement works
    let mut s = String::from("hello");
    let mut poke = Poke::new(&mut s);

    poke.set(String::from("world")).unwrap();
    assert_eq!(s, "world");
}

#[test]
fn poke_as_peek() {
    #[derive(Debug, Facet, PartialEq)]
    #[facet(pod)]
    struct Point {
        x: i32,
        y: i32,
    }

    let mut point = Point { x: 10, y: 20 };
    let poke = Poke::new(&mut point);

    let peek = poke.as_peek();
    assert_eq!(peek.shape(), Point::SHAPE);
}
