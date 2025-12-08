use crate::{ConstTypeId, Def, Facet, Shape, ShapeBuilder, vtable_direct};

unsafe impl Facet<'_> for ConstTypeId {
    const SHAPE: &'static Shape = &const {
        // ConstTypeId implements Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash
        // but NOT Display or Default

        const VTABLE: crate::VTableDirect = vtable_direct!(ConstTypeId =>
            Debug,
            Hash,
            PartialEq,
            PartialOrd,
            Ord,
        );

        ShapeBuilder::for_sized::<ConstTypeId>("ConstTypeId")
            .def(Def::Scalar)
            .vtable_direct(&VTABLE)
            .eq()
            .copy()
            .send()
            .sync()
            .build()
    };
}

unsafe impl Facet<'_> for core::any::TypeId {
    const SHAPE: &'static Shape = &const {
        // TypeId implements Debug, Clone, Copy, PartialEq, Eq, Hash
        // but NOT Display, Default, PartialOrd, or Ord

        const VTABLE: crate::VTableDirect = vtable_direct!(core::any::TypeId =>
            Debug,
            Hash,
            PartialEq,
        );

        ShapeBuilder::for_sized::<core::any::TypeId>("TypeId")
            .def(Def::Scalar)
            .vtable_direct(&VTABLE)
            .eq()
            .copy()
            .send()
            .sync()
            .build()
    };
}
