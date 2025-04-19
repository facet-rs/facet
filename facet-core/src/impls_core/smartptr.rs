use core::alloc::Layout;

use crate::{
    ConstTypeId, Def, Facet, KnownSmartPointer, PtrConst, SmartPointerDef, SmartPointerFlags,
    SmartPointerVTable, value_vtable,
};

unsafe impl<T: Facet> Facet for core::ptr::NonNull<T> {
    const SHAPE: &'static crate::Shape = &const {
        crate::Shape::builder()
            .id(ConstTypeId::of::<Self>())
            .layout(Layout::new::<Self>())
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: || T::SHAPE,
            }])
            .def(Def::SmartPointer(
                SmartPointerDef::builder()
                    .pointee(T::SHAPE)
                    .flags(SmartPointerFlags::EMPTY)
                    .known(KnownSmartPointer::NonNull)
                    .vtable(
                        &const {
                            SmartPointerVTable::builder()
                                .borrow_fn(|this| {
                                    let ptr = unsafe { this.get::<Self>().as_ptr() };
                                    PtrConst::new(ptr)
                                })
                                .new_into_fn(|this, ptr| {
                                    let ptr = unsafe { ptr.read::<*mut T>() };
                                    let non_null =
                                        unsafe { core::ptr::NonNull::new_unchecked(ptr) };
                                    unsafe { this.put(non_null) }
                                })
                                .build()
                        },
                    )
                    .build(),
            ))
            .vtable(value_vtable!(core::ptr::NonNull<T>, |f, _opts| write!(
                f,
                "NonNull"
            )))
            .build()
    };
}
