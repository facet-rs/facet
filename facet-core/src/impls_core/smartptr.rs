use crate::{
    BaseRepr, Def, Facet, Field, FieldFlags, KnownSmartPointer, PtrConst, Repr, SmartPointerDef,
    SmartPointerFlags, SmartPointerVTable, StructDef, StructKind, Type, UserSubtype, UserType,
    value_vtable,
};

unsafe impl<'a, T: Facet<'a>> Facet<'a> for core::ptr::NonNull<T> {
    const SHAPE: &'static crate::Shape = &const {
        crate::Shape::builder_for_sized::<Self>()
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: || T::SHAPE,
            }])
            .ty(Type::User(UserType {
                repr: Repr {
                    base: BaseRepr::Transparent,
                    packed: false,
                    align: None,
                },
                subtype: UserSubtype::Struct(StructDef {
                    kind: StructKind::Struct,
                    fields: &const {
                        [Field::builder()
                            .name("pointer")
                            .shape(|| <*mut T>::SHAPE)
                            .offset(0)
                            .flags(FieldFlags::EMPTY)
                            .build()]
                    },
                }),
            }))
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
            .vtable(
                &const { value_vtable!(core::ptr::NonNull<T>, |f, _opts| write!(f, "NonNull")) },
            )
            .build()
    };
}
