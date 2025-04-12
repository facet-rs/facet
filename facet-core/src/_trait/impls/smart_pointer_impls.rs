use core::alloc::Layout;

use crate::{
    ConstTypeId, Def, Facet, OpaqueConst, SmartPointerDef, SmartPointerFlags, SmartPointerVTable,
    value_vtable,
};

#[cfg(feature = "alloc")]
unsafe impl<T: Facet> Facet for alloc::sync::Arc<T> {
    const SHAPE: &'static crate::Shape = &const {
        crate::Shape::builder()
            .id(ConstTypeId::of::<Self>())
            .layout(Layout::new::<Self>())
            .def(Def::SmartPointer(
                SmartPointerDef::builder()
                    .t(T::SHAPE)
                    .flags(SmartPointerFlags::ATOMIC)
                    .known(Some(crate::KnownSmartPointer::Arc))
                    .vtable(
                        &const {
                            SmartPointerVTable::builder()
                                .borrow_fn(|opaque| {
                                    let ptr = Self::as_ptr(unsafe { opaque.as_ref() });
                                    OpaqueConst::new(ptr)
                                })
                                .new_into_fn(|this, ptr| {
                                    let t = unsafe { ptr.read::<T>() };
                                    let arc = alloc::sync::Arc::new(t);
                                    unsafe { this.put(arc) }
                                })
                                .build()
                        },
                    )
                    .build(),
            ))
            .vtable(value_vtable!(alloc::sync::Arc<T>, |f, _opts| write!(
                f,
                "Arc"
            )))
            .build()
    };
}
