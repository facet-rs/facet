use crate::{Facet, Opaque, Shape, VarianceDesc};

// Opaque<T> is a lifetime boundary; require 'static to prevent lifetime laundering
// through reflection. See issue #1563 for details.
unsafe impl<'facet, T: 'static> Facet<'facet> for Opaque<T> {
    const SHAPE: &'static Shape = &const {
        Shape::builder_for_sized::<Opaque<T>>("Opaque")
            .decl_id_prim()
            .variance(VarianceDesc::INVARIANT)
            .build()
    };
}
