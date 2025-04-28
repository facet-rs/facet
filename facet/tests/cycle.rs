use facet::Facet;
use std::sync::Arc;

#[derive(Facet)]
struct Recursive {
    next: Option<Arc<Recursive>>,
}

#[derive(Debug)]
struct ShapeLike {
    next: Option<fn() -> &'static ShapeLike>,
}

impl FacetLike for () {
    const SHAPE_LIKE: &'static ShapeLike = &const {
        ShapeLike {
            next: Some(|| Self::SHAPE_LIKE),
        }
    };
}

impl<T: FacetLike> FacetLike for Vec<T> {
    const SHAPE_LIKE: &'static ShapeLike = &ShapeLike {
        next: Some(|| T::SHAPE_LIKE),
    };
}

trait FacetLike {
    const SHAPE_LIKE: &'static ShapeLike;
}

#[test]
fn cyclic_shape() {
    assert!(<()>::SHAPE_LIKE.next.is_some());
}
