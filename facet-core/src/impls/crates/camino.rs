#![cfg(feature = "camino")]

use camino::{Utf8Path, Utf8PathBuf};

use crate::{
    Def, Facet, Shape, ShapeBuilder, Type, UserType, VTableDirect, VTableIndirect, vtable_direct,
    vtable_indirect,
};

unsafe impl Facet<'_> for Utf8PathBuf {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = vtable_direct!(Utf8PathBuf =>
            Display,
            Debug,
            Hash,
            PartialEq,
            PartialOrd,
            Ord,
            FromStr,
        );

        ShapeBuilder::for_sized::<Utf8PathBuf>("Utf8PathBuf")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_direct(&VTABLE)
            .eq()
            .send()
            .sync()
            .build()
    };
}

unsafe impl Facet<'_> for Utf8Path {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableIndirect = vtable_indirect!(Utf8Path =>
            Display,
            Debug,
            Hash,
            PartialEq,
            PartialOrd,
            Ord,
        );

        ShapeBuilder::for_unsized::<Utf8Path>("Utf8Path")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_indirect(&VTABLE)
            .eq()
            .send()
            .sync()
            .build()
    };
}
