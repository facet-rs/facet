use facet::Facet;
use facet_reflect::{Poke, ReflectErrorKind};

#[test]
fn poke_struct_field_by_name() {
    #[derive(Debug, Facet, PartialEq)]
    #[facet(pod)]
    struct Point {
        x: i32,
        y: i32,
    }

    let mut point = Point { x: 1, y: 2 };
    let poke = Poke::new(&mut point);
    let mut poke_struct = poke.into_struct().expect("Point is a struct");

    // Modify x field
    poke_struct.set_field_by_name("x", 100i32).unwrap();

    assert_eq!(point.x, 100);
    assert_eq!(point.y, 2);
}

#[test]
fn poke_struct_field_by_index() {
    #[derive(Debug, Facet, PartialEq)]
    #[facet(pod)]
    struct Point {
        x: i32,
        y: i32,
    }

    let mut point = Point { x: 1, y: 2 };
    let poke = Poke::new(&mut point);
    let mut poke_struct = poke.into_struct().expect("Point is a struct");

    // Modify y field (index 1)
    poke_struct.set_field(1, 200i32).unwrap();

    assert_eq!(point.x, 1);
    assert_eq!(point.y, 200);
}

#[test]
fn poke_struct_get_field_poke() {
    #[derive(Debug, Facet, PartialEq)]
    #[facet(pod)]
    struct Point {
        x: i32,
        y: i32,
    }

    let mut point = Point { x: 1, y: 2 };
    {
        let poke = Poke::new(&mut point);
        let mut poke_struct = poke.into_struct().expect("Point is a struct");

        // Get mutable access to x field
        let mut field_poke = poke_struct.field_by_name("x").unwrap();
        field_poke.set(42i32).unwrap();
    }

    assert_eq!(point.x, 42);
}

#[test]
fn poke_struct_wrong_field_type() {
    #[derive(Debug, Facet, PartialEq)]
    #[facet(pod)]
    struct Point {
        x: i32,
        y: i32,
    }

    let mut point = Point { x: 1, y: 2 };
    let poke = Poke::new(&mut point);
    let mut poke_struct = poke.into_struct().expect("Point is a struct");

    // Try to set i32 field with u32
    let result = poke_struct.set_field_by_name("x", 100u32);
    assert!(
        matches!(result, Err(ref err) if matches!(err.kind, ReflectErrorKind::WrongShape { .. }))
    );
}

#[test]
fn poke_struct_no_such_field() {
    #[derive(Debug, Facet, PartialEq)]
    #[facet(pod)]
    struct Point {
        x: i32,
        y: i32,
    }

    let mut point = Point { x: 1, y: 2 };
    let poke = Poke::new(&mut point);
    let mut poke_struct = poke.into_struct().expect("Point is a struct");

    let result = poke_struct.set_field_by_name("z", 100i32);
    assert!(
        matches!(result, Err(ref err) if matches!(err.kind, ReflectErrorKind::FieldError { .. }))
    );
}

#[test]
fn poke_struct_field_index_out_of_bounds() {
    #[derive(Debug, Facet, PartialEq)]
    #[facet(pod)]
    struct Point {
        x: i32,
        y: i32,
    }

    let mut point = Point { x: 1, y: 2 };
    let poke = Poke::new(&mut point);
    let mut poke_struct = poke.into_struct().expect("Point is a struct");

    let result = poke_struct.set_field(99, 100i32);
    assert!(
        matches!(result, Err(ref err) if matches!(err.kind, ReflectErrorKind::FieldError { .. }))
    );
}

#[test]
fn poke_struct_peek_field() {
    #[derive(Debug, Facet, PartialEq)]
    #[facet(pod)]
    struct Point {
        x: i32,
        y: i32,
    }

    let mut point = Point { x: 42, y: 99 };
    let poke = Poke::new(&mut point);
    let poke_struct = poke.into_struct().expect("Point is a struct");

    // Get read-only view of field
    let x_peek = poke_struct.peek_field_by_name("x").unwrap();
    assert_eq!(*x_peek.get::<i32>().unwrap(), 42);
}

#[test]
fn poke_struct_as_peek_struct() {
    #[derive(Debug, Facet, PartialEq)]
    #[facet(pod)]
    struct Point {
        x: i32,
        y: i32,
    }

    let mut point = Point { x: 10, y: 20 };
    let poke = Poke::new(&mut point);
    let poke_struct = poke.into_struct().expect("Point is a struct");

    let peek_struct = poke_struct.as_peek_struct();
    assert_eq!(peek_struct.field_count(), 2);
}

#[test]
fn poke_struct_with_string_field() {
    // POD struct can have non-POD fields like String
    // The POD check is on the parent struct, not the field type
    #[derive(Debug, Facet, PartialEq)]
    #[facet(pod)]
    struct Person {
        name: String,
        age: u32,
    }

    let mut person = Person {
        name: "Alice".to_string(),
        age: 30,
    };

    let poke = Poke::new(&mut person);
    let mut poke_struct = poke.into_struct().expect("Person is a struct");

    poke_struct
        .set_field_by_name("name", "Bob".to_string())
        .unwrap();
    poke_struct.set_field_by_name("age", 25u32).unwrap();

    assert_eq!(person.name, "Bob");
    assert_eq!(person.age, 25);
}

#[test]
fn poke_struct_with_non_pod_field_in_pod_parent() {
    // The field type doesn't need to be POD - only the parent struct
    #[derive(Debug, Facet, PartialEq)]
    struct Inner {
        value: i32,
    }

    #[derive(Debug, Facet, PartialEq)]
    #[facet(pod)]
    struct Outer {
        inner: Inner,
    }

    let mut outer = Outer {
        inner: Inner { value: 42 },
    };

    let poke = Poke::new(&mut outer);
    let mut poke_struct = poke.into_struct().expect("Outer is a struct");

    // Can replace the Inner field even though Inner is not POD
    poke_struct
        .set_field_by_name("inner", Inner { value: 100 })
        .unwrap();

    assert_eq!(outer.inner.value, 100);
}
