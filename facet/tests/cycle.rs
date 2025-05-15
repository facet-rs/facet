use facet::Facet;
use std::sync::Arc;

trait FacetLike {
    const SHAPE_LIKE: &'static ShapeLike;
}

#[derive(Debug)]
enum ShapeLike {
    Simple {
        only: &'static ShapeLike,
    },
    Double {
        left: &'static ShapeLike,
        right: &'static ShapeLike,
    },
}

/////////////////////////////////////////////////////////////////////////////

static EMPTY_TUPLE_SHAPE: ShapeLike = ShapeLike::Simple {
    only: &EMPTY_TUPLE_SHAPE,
};

impl FacetLike for () {
    const SHAPE_LIKE: &'static ShapeLike = &EMPTY_TUPLE_SHAPE;
}

/////////////////////////////////////////////////////////////////////////////

impl<T: FacetLike> FacetLike for Vec<T> {
    const SHAPE_LIKE: &'static ShapeLike = &ShapeLike::Simple {
        only: <T as FacetLike>::SHAPE_LIKE,
    };
}

impl<T: FacetLike> FacetLike for Arc<T> {
    const SHAPE_LIKE: &'static ShapeLike = &ShapeLike::Simple {
        only: <T as FacetLike>::SHAPE_LIKE,
    };
}

/////////////////////////////////////////////////////////////////////////////

#[derive(Facet)]
struct Recursive {
    next: Option<Arc<Recursive>>,
}

static RECURSIVE_TUPLE_SHAPE: ShapeLike = ShapeLike::Simple {
    only: &RECURSIVE_TUPLE_SHAPE,
};

impl FacetLike for Recursive {
    const SHAPE_LIKE: &'static ShapeLike = &RECURSIVE_TUPLE_SHAPE;
}

/////////////////////////////////////////////////////////////////////////////

#[derive(Facet)]
struct Recursive2<T> {
    next: Arc<Recursive2<T>>,
    other: T,
}

impl<T: FacetLike> FacetLike for Recursive2<T> {
    const SHAPE_LIKE: &'static ShapeLike = &ShapeLike::Double {
        left: <Arc<Recursive2<T>> as FacetLike>::SHAPE_LIKE,
        right: T::SHAPE_LIKE,
    };
}

/////////////////////////////////////////////////////////////////////////////

#[test]
fn cyclic_shape() {
    let _ = <Recursive2<Recursive2<()>>>::SHAPE_LIKE;
}
