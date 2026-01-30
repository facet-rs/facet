use facet_reflect2::{Op, Partial};

#[test]
fn set_u32() {
    let mut partial = Partial::alloc::<u32>().unwrap();

    let value = 42u32;
    partial.apply(&[Op::set().mov(&value)]).unwrap();

    let result: u32 = partial.build().unwrap();
    assert_eq!(result, 42);
}
