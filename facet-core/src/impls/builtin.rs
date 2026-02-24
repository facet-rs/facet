use crate::{Facet, Opaque, OpaqueBorrow, Shape, VarianceDesc};

// Opaque<T> is a lifetime boundary; require 'static to prevent lifetime laundering
// through reflection. See issue #1563 for details.
unsafe impl<'facet, T: 'static> Facet<'facet> for Opaque<T> {
    const SHAPE: &'static Shape = &const {
        Shape::builder_for_sized::<Opaque<T>>("Opaque")
            .variance(VarianceDesc::INVARIANT)
            .build()
    };
}

// OpaqueBorrow<'facet, T> is used by derive-generated field-level `#[facet(opaque)]`
// wrappers so borrowed fields can stay tied to the active Facet lifetime.
unsafe impl<'facet, T: 'facet> Facet<'facet> for OpaqueBorrow<'facet, T> {
    const SHAPE: &'static Shape = &const {
        Shape::builder_for_sized::<OpaqueBorrow<'facet, T>>("Opaque")
            .variance(VarianceDesc::INVARIANT)
            .build()
    };
}
