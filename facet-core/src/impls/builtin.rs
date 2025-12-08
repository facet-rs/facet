use crate::{Opaque, Shape};

use crate::Facet;

unsafe impl<'facet, T: 'facet> Facet<'facet> for Opaque<T> {
    const SHAPE: &'static Shape =
        &const { Shape::builder_for_sized::<Opaque<T>>("Opaque").build() };
}
