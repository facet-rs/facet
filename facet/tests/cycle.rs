#[derive(Debug)]
struct ShapeLike {
    next: Option<&'static ShapeLike>,
}

static EMPTY_TUPLE_SHAPE: ShapeLike = {
    ShapeLike {
        next: Some(&EMPTY_TUPLE_SHAPE),
    }
};

impl FacetLike for () {
    const SHAPE_LIKE: &'static ShapeLike = &EMPTY_TUPLE_SHAPE;
}

trait FacetLike {
    const SHAPE_LIKE: &'static ShapeLike;
}

#[test]
fn cyclic_shape() {
    let shape = &EMPTY_TUPLE_SHAPE;
    assert!(shape.next.is_some());

    eprintln!("ShapeLike2: {:?}", <()>::SHAPE_LIKE);
}
