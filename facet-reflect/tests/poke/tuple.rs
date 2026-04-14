use facet::Facet;
use facet_reflect::{Poke, ReflectErrorKind};

#[test]
fn poke_tuple_len_and_field_access() {
    let mut pair: (i32, i32) = (1, 2);
    let poke = Poke::new(&mut pair);
    let tuple = poke.into_tuple().expect("(i32, i32) is a tuple");

    assert_eq!(tuple.len(), 2);
    assert!(!tuple.is_empty());

    let f0 = tuple.field(0).unwrap();
    assert_eq!(*f0.get::<i32>().unwrap(), 1);
    let f1 = tuple.field(1).unwrap();
    assert_eq!(*f1.get::<i32>().unwrap(), 2);

    // Out-of-bounds field access returns None (not an error).
    assert!(tuple.field(99).is_none());
}

#[test]
fn poke_tuple_field_mut_writes_through() {
    let mut pair: (i32, i32) = (1, 2);
    let poke = Poke::new(&mut pair);
    let mut tuple = poke.into_tuple().expect("(i32, i32) is a tuple");

    {
        let mut f1 = tuple.field_mut(1).unwrap();
        f1.set(99i32).unwrap();
    }
    assert_eq!(pair, (1, 99));
}

#[test]
fn poke_tuple_field_mut_out_of_bounds_fails() {
    let mut pair: (i32, i32) = (1, 2);
    let poke = Poke::new(&mut pair);
    let mut tuple = poke.into_tuple().expect("(i32, i32) is a tuple");

    let result = tuple.field_mut(99);
    assert!(matches!(
        result,
        Err(ref err) if matches!(err.kind, ReflectErrorKind::FieldError { .. })
    ));
}

#[test]
fn poke_tuple_set_field_not_pod_fails() {
    // Raw tuples aren't POD by default, so set_field should refuse.
    let mut pair: (i32, i32) = (1, 2);
    let poke = Poke::new(&mut pair);
    let mut tuple = poke.into_tuple().expect("(i32, i32) is a tuple");

    let result = tuple.set_field(0, 42i32);
    assert!(matches!(
        result,
        Err(ref err) if matches!(err.kind, ReflectErrorKind::NotPod { .. })
    ));
}

#[test]
fn poke_tuple_into_inner_round_trips() {
    let mut pair: (i32, i32) = (1, 2);
    let poke = Poke::new(&mut pair);
    let tuple = poke.into_tuple().expect("(i32, i32) is a tuple");

    let poke = tuple.into_inner();
    // After round-trip we can still interrogate the underlying Poke.
    let tuple = poke.into_tuple().unwrap();
    assert_eq!(tuple.len(), 2);
}

#[test]
fn poke_tuple_as_peek_tuple() {
    let mut pair: (i32, i32) = (3, 4);
    let poke = Poke::new(&mut pair);
    let tuple = poke.into_tuple().expect("(i32, i32) is a tuple");

    let peek = tuple.as_peek_tuple();
    assert_eq!(peek.len(), 2);
    assert_eq!(*peek.field(0).unwrap().get::<i32>().unwrap(), 3);
}

#[test]
fn poke_not_a_tuple_fails() {
    #[derive(Debug, Facet, PartialEq)]
    #[facet(pod)]
    struct NamedFields {
        x: i32,
        y: i32,
    }

    let mut v = NamedFields { x: 1, y: 2 };
    let poke = Poke::new(&mut v);
    let result = poke.into_tuple();
    assert!(matches!(
        result,
        Err(ref err) if matches!(err.kind, ReflectErrorKind::WasNotA { .. })
    ));
}
