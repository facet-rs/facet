use core::ptr::NonNull;

use crate::{
    Def, Facet, Field, FieldFlags, KnownPointer, PointerDef, PointerFlags, PointerVTable, PtrConst,
    Repr, StructKind, StructType, Type, UserType, value_vtable,
};

unsafe impl<'a, T: Facet<'a>> Facet<'a> for core::ptr::NonNull<T> {
    const SHAPE: &'static crate::Shape = &const {
        crate::Shape::builder_for_sized::<Self>()
            .type_identifier("NonNull")
            .vtable(value_vtable!(core::ptr::NonNull<T>, |f, _opts| write!(
                f,
                "{}",
                Self::SHAPE.type_identifier
            )))
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: T::SHAPE,
            }])
            .ty(Type::User(UserType::Struct(StructType {
                repr: Repr::transparent(),
                kind: StructKind::Struct,
                fields: &const {
                    [Field::builder()
                        .name("pointer")
                        .shape(|| <*mut T>::SHAPE)
                        .offset(0)
                        .flags(FieldFlags::EMPTY)
                        .build()]
                },
            })))
            .def(Def::Pointer(
                PointerDef::builder()
                    .pointee(T::SHAPE)
                    .flags(PointerFlags::EMPTY)
                    .known(KnownPointer::NonNull)
                    .vtable(
                        &const {
                            PointerVTable::builder()
                                .borrow_fn(|this| {
                                    let ptr = unsafe { this.get::<Self>() };
                                    PtrConst::new(NonNull::from(ptr))
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
            .build()
    };
}
