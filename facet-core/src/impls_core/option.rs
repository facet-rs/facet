use core::mem::MaybeUninit;

use crate::{
    Def, EnumRepr, EnumType, Facet, Field, FieldFlags, OptionDef, OptionVTable, PtrConst, PtrMut,
    PtrUninit, Repr, Shape, StructKind, StructType, TryBorrowInnerError, TryFromError,
    TryIntoInnerError, Type, TypedPtrUninit, UserType, VTableView, ValueVTable, Variant,
    value_vtable,
};
unsafe impl<'a, T: Facet<'a>> Facet<'a> for Option<T> {
    const VTABLE: &'static ValueVTable = &const {
        // Define the functions for transparent conversion between Option<T> and T
        unsafe fn try_from<'a, 'shape, 'src, 'dst, T: Facet<'a>>(
            src_ptr: PtrConst<'src>,
            src_shape: &'shape Shape<'shape>,
            dst: PtrUninit<'dst>,
        ) -> Result<PtrMut<'dst>, TryFromError<'shape>> {
            if src_shape.id != T::SHAPE.id {
                return Err(TryFromError::UnsupportedSourceShape {
                    src_shape,
                    expected: &[T::SHAPE],
                });
            }
            let t = unsafe { src_ptr.read::<T>() };
            let option = Some(t);
            Ok(unsafe { dst.put(option) })
        }

        unsafe fn try_into_inner<'a, 'src, 'dst, T: Facet<'a>>(
            src_ptr: PtrMut<'src>,
            dst: PtrUninit<'dst>,
        ) -> Result<PtrMut<'dst>, TryIntoInnerError> {
            let option = unsafe { src_ptr.read::<Option<T>>() };
            match option {
                Some(t) => Ok(unsafe { dst.put(t) }),
                None => Err(TryIntoInnerError::Unavailable),
            }
        }

        unsafe fn try_borrow_inner<'a, 'src, T: Facet<'a>>(
            src_ptr: PtrConst<'src>,
        ) -> Result<PtrConst<'src>, TryBorrowInnerError> {
            let option = unsafe { src_ptr.get::<Option<T>>() };
            match option {
                Some(t) => Ok(PtrConst::new(t)),
                None => Err(TryBorrowInnerError::Unavailable),
            }
        }

        let mut vtable = value_vtable!(core::option::Option<T>, |f, opts| {
            write!(f, "{}", Self::SHAPE.type_identifier)?;
            if let Some(opts) = opts.for_children() {
                write!(f, "<")?;
                (T::SHAPE.vtable.type_name())(f, opts)?;
                write!(f, ">")?;
            } else {
                write!(f, "<…>")?;
            }
            Ok(())
        });

        {
            let vtable_sized = vtable.sized_mut().unwrap();
            vtable_sized.debug = || {
                if T::SHAPE.is_debug() {
                    Some(|this, f| {
                        let this = unsafe { this.get::<Self>() };
                        if let Some(value) = &this {
                            write!(f, "Some(")?;
                            (<VTableView<T>>::of().debug().unwrap())(value, f)?;
                            write!(f, ")")?;
                        } else {
                            write!(f, "None")?;
                        }
                        Ok(())
                    })
                } else {
                    None
                }
            };

            vtable_sized.parse = || {
                if T::SHAPE.is_from_str() {
                    Some(|str, target| {
                        let mut t = MaybeUninit::<T>::uninit();
                        let parse = <VTableView<T>>::of().parse().unwrap();
                        let _res = (parse)(str, TypedPtrUninit::new(t.as_mut_ptr()))?;
                        // res points to t so we can't drop it yet. the option is not initialized though
                        unsafe {
                            target.put(Some(t.assume_init()));
                            Ok(target.assume_init())
                        }
                    })
                } else {
                    None
                }
            };

            vtable_sized.try_from = || Some(try_from::<T>);
            vtable_sized.try_into_inner = || Some(try_into_inner::<T>);
            vtable_sized.try_borrow_inner = || Some(try_borrow_inner::<T>);
        }

        vtable
    };

    const SHAPE: &'static Shape<'static> = &const {
        // Function to return inner type's shape
        fn inner_shape<'a, T: Facet<'a>>() -> &'static Shape<'static> {
            T::SHAPE
        }

        Shape::builder_for_sized::<Self>()
            .type_identifier("Option")
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: || T::SHAPE,
            }])
            .ty(Type::User(
                // Null-Pointer-Optimization - we verify that this Option variant has no
                // discriminant.
                //
                // See: https://doc.rust-lang.org/std/option/index.html#representation
                if core::mem::size_of::<T>() == core::mem::size_of::<Option<T>>()
                    && core::mem::size_of::<T>() <= core::mem::size_of::<usize>()
                {
                    UserType::Enum(EnumType {
                        repr: Repr::default(),
                        enum_repr: EnumRepr::RustNPO,
                        variants: &const {
                            [
                                Variant::builder()
                                    .name("None")
                                    .discriminant(0)
                                    .data(
                                        StructType::builder()
                                            .repr(Repr::default())
                                            .kind(StructKind::Unit)
                                            .build(),
                                    )
                                    .build(),
                                Variant::builder()
                                    .name("Some")
                                    .discriminant(0)
                                    .data(
                                        StructType::builder()
                                            .repr(Repr::default())
                                            .kind(StructKind::TupleStruct)
                                            .fields(
                                                &const {
                                                    [Field::builder()
                                                        .name("0")
                                                        .shape(T::SHAPE)
                                                        .offset(0)
                                                        .flags(FieldFlags::EMPTY)
                                                        .build()]
                                                },
                                            )
                                            .build(),
                                    )
                                    .build(),
                            ]
                        },
                    })
                } else {
                    UserType::Opaque
                },
            ))
            .def(Def::Option(
                OptionDef::builder()
                    .t(T::SHAPE)
                    .vtable(
                        const {
                            &OptionVTable::builder()
                                .is_some(|option| unsafe { option.get::<Option<T>>().is_some() })
                                .get_value(|option| unsafe {
                                    option
                                        .get::<Option<T>>()
                                        .as_ref()
                                        .map(|t| PtrConst::new(t as *const T))
                                })
                                .init_some(|option, value| unsafe {
                                    option.put(Option::Some(value.read::<T>()))
                                })
                                .init_none(|option| unsafe { option.put(<Option<T>>::None) })
                                .replace_with(|option, value| unsafe {
                                    let option = option.as_mut::<Option<T>>();
                                    match value {
                                        Some(value) => option.replace(value.read::<T>()),
                                        None => option.take(),
                                    };
                                })
                                .build()
                        },
                    )
                    .build(),
            ))
            .inner(inner_shape::<T>)
            .build()
    };
}
