use core::alloc::Layout;

use camino::{Utf8Path, Utf8PathBuf};

use crate::{
    ConstTypeId, Def, Facet, ScalarAffinity, ScalarDef, Shape, value_vtable, value_vtable_inner,
};

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
            .vtable(
                &const {
                    let mut vtable = value_vtable_inner!((), |f, _opts| write!(f, "Utf8PathBuf"));
                    vtable.parse =
                        Some(|s, target| Ok(unsafe { target.put(Utf8Path::new(s).to_owned()) }));
                    vtable
                },
            )
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
