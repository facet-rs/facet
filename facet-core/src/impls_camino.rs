use core::alloc::Layout;

use camino::{Utf8Path, Utf8PathBuf};

use crate::{ConstTypeId, Def, Facet, ScalarAffinity, ScalarDef, Shape, value_vtable};

unsafe impl Facet for Utf8PathBuf {
    const SHAPE: &'static Shape = &const {
        Shape::builder()
            .id(ConstTypeId::of::<Self>())
            .layout(Layout::new::<Self>())
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(ScalarAffinity::path().build())
                    .build(),
            ))
            .vtable(value_vtable!((), |f, _opts| write!(f, "Utf8PathBuf")))
            .build()
    };
}

unsafe impl Facet for &Utf8Path {
    const SHAPE: &'static Shape = &const {
        Shape::builder()
            .id(ConstTypeId::of::<Self>())
            .layout(Layout::new::<Self>())
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(ScalarAffinity::path().build())
                    .build(),
            ))
            .vtable(value_vtable!((), |f, _opts| write!(f, "Utf8Path")))
            .build()
    };
}
