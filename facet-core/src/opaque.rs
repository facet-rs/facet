use crate::{ConstTypeId, Def, ScalarAffinity, ScalarDef, value_vtable};
use crate::{Facet, Shape};
use core::alloc::Layout;

/// Helper type for opaque members
#[repr(transparent)]
pub struct Opaque<T>(T);

unsafe impl<'a, T: 'a> Facet<'a> for Opaque<T> {
    const SHAPE: &'static Shape = &const {
        Shape::builder()
            .id(ConstTypeId::of::<Self>())
            .layout(Layout::new::<Self>())
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(ScalarAffinity::opaque().build())
                    .build(),
            ))
            // Since T is opaque and could be anything, we can't provide much functionality.
            // Using `()` for the vtable like PhantomData.
            .vtable(&const { value_vtable!((), |f, _opts| write!(f, "Opaque")) })
            .build()
    };
}
