#[derive(Debug)]
struct ShapeLike {
    next: Option<&'static ShapeLike>,
}

impl FacetLike for () {
    const SHAPE_LIKE: &'static ShapeLike = const {
        static EMPTY_TUPLE_SHAPE: ShapeLike = {
            ShapeLike {
                next: Some(&EMPTY_TUPLE_SHAPE),
            }
        };

        &EMPTY_TUPLE_SHAPE
    };
}

trait FacetLike {
    const SHAPE_LIKE: &'static ShapeLike;
}

#[test]
fn cyclic_shape() {
    assert!(<()>::SHAPE_LIKE.next.is_some());
}
