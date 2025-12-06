use core::ptr::NonNull;

use crate::{
    Def, Facet, Field, FieldFlags, KnownPointer, PointerDef, PointerFlags, PointerVTable, PtrConst,
    Repr, ShapeBuilder, ShapeRef, StructKind, StructType, Type, UserType, value_vtable,
};

unsafe impl<'a, T: Facet<'a>> Facet<'a> for core::ptr::NonNull<T> {
    const SHAPE: &'static crate::Shape = &const {
        ShapeBuilder::for_sized::<Self>(
            |f, _opts| write!(f, "{}", Self::SHAPE.type_identifier),
            "NonNull",
        )
        .vtable(value_vtable!(core::ptr::NonNull<T>, |f, _opts| write!(
            f,
            "{}",
            Self::SHAPE.type_identifier
        )))
        .ty(Type::User(UserType::Struct(StructType {
            repr: Repr::transparent(),
            kind: StructKind::Struct,
            fields: &const {
                [Field {
                    name: "pointer",
                    shape: ShapeRef::Static(<*mut T>::SHAPE),
                    offset: 0,
                    flags: FieldFlags::empty(),
                    rename: None,
                    alias: None,
                    attributes: &[],
                    doc: &[],
                }]
            },
        })))
        .def(Def::Pointer(PointerDef {
            vtable: &const {
                PointerVTable {
                    borrow_fn: Some(|this| {
                        let ptr = unsafe { this.get::<Self>() };
                        PtrConst::new(NonNull::from(ptr))
                    }),
                    new_into_fn: Some(|this, ptr| {
                        let ptr = unsafe { ptr.read::<*mut T>() };
                        let non_null = unsafe { core::ptr::NonNull::new_unchecked(ptr) };
                        unsafe { this.put(non_null) }
                    }),
                    ..PointerVTable::new()
                }
            },
            pointee: Some(T::SHAPE),
            weak: None,
            strong: None,
            flags: PointerFlags::EMPTY,
            known: Some(KnownPointer::NonNull),
        }))
        .type_params(&[crate::TypeParam {
            name: "T",
            shape: T::SHAPE,
        }])
        .build()
    };
}
