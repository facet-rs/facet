use facet::Facet;
use facet_reflect2::{Op, Partial};

#[derive(Debug, PartialEq, Facet)]
struct Point {
    x: i32,
    y: i32,
}

#[test]
fn set_struct_fields() {
    let mut partial = Partial::alloc::<Point>().unwrap();

    let x = 10i32;
    let y = 20i32;
    partial
        .apply(&[Op::set().at(0).mov(&x), Op::set().at(1).mov(&y)])
        .unwrap();

    let result: Point = partial.build().unwrap();
    assert_eq!(result, Point { x: 10, y: 20 });
}
