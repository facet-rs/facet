use core::{cmp::Ordering, hash::Hash, mem::MaybeUninit, ptr::NonNull};

use crate::{
    Def, EnumRepr, EnumType, Facet, Field, OptionDef, OptionVTable, PtrConst, PtrMut, PtrUninit,
    Repr, Shape, ShapeBuilder, ShapeRef, StructKind, StructType, TryBorrowInnerError, TryFromError,
    TryIntoInnerError, Type, TypeParam, TypedPtrUninit, UserType, VTableView, Variant, shape_util,
    value_vtable,
};
unsafe impl<'a, T: Facet<'a>> Facet<'a> for Option<T> {
    const SHAPE: &'static Shape = &const {
        let vtable = {
            // Define the functions for transparent conversion between Option<T> and T
            unsafe fn try_from<'a, 'src, 'dst, T: Facet<'a>>(
                src_ptr: PtrConst<'src>,
                src_shape: &'static Shape,
                dst: PtrUninit<'dst>,
            ) -> Result<PtrMut<'dst>, TryFromError> {
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
                    Some(t) => Ok(PtrConst::new(NonNull::from(t))),
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
                let vtable_sized = &mut vtable;

                vtable_sized.format.debug = if T::SHAPE.is_debug() {
                    Some(|this, f| {
                        let this = unsafe { this.get::<Self>() };
                        if let Some(value) = &this {
                            f.debug_tuple("Some")
                                .field(&shape_util::Debug {
                                    ptr: PtrConst::new(value.into()),
                                    f: T::SHAPE.vtable.format.debug.unwrap(),
                                })
                                .finish()
                        } else {
                            write!(f, "None")
                        }
                    })
                } else {
                    None
                };

                vtable_sized.hash.hash = if T::SHAPE.is_hash() {
                    Some(|this, hasher| unsafe {
                        let this = this.get::<Self>();
                        this.as_ref()
                            .map(|this| shape_util::Hash {
                                ptr: PtrConst::new(this.into()),
                                f: T::SHAPE.vtable.hash.hash.unwrap(),
                            })
                            .hash(&mut { hasher });
                    })
                } else {
                    None
                };

                vtable_sized.cmp.partial_eq = if T::SHAPE.is_partial_eq() {
                    Some(|a, b| unsafe {
                        let a = a.get::<Self>();
                        let b = b.get::<Self>();
                        match (a, b) {
                            (None, None) => true,
                            (Some(a), Some(b)) => T::SHAPE.vtable.cmp.partial_eq.unwrap()(
                                PtrConst::new(a.into()),
                                PtrConst::new(b.into()),
                            ),
                            _ => false,
                        }
                    })
                } else {
                    None
                };

                vtable_sized.cmp.partial_ord = if T::SHAPE.is_partial_ord() {
                    Some(|a, b| unsafe {
                        let a = a.get::<Self>();
                        let b = b.get::<Self>();
                        match (a, b) {
                            (None, None) => Some(Ordering::Equal),
                            (None, Some(_)) => Some(Ordering::Less),
                            (Some(_), None) => Some(Ordering::Greater),
                            (Some(a), Some(b)) => T::SHAPE.vtable.cmp.partial_ord.unwrap()(
                                PtrConst::new(a.into()),
                                PtrConst::new(b.into()),
                            ),
                        }
                    })
                } else {
                    None
                };

                vtable_sized.cmp.ord = if T::SHAPE.is_ord() {
                    Some(|a, b| unsafe {
                        let a = a.get::<Self>();
                        let b = b.get::<Self>();
                        match (a, b) {
                            (None, None) => Ordering::Equal,
                            (None, Some(_)) => Ordering::Less,
                            (Some(_), None) => Ordering::Greater,
                            (Some(a), Some(b)) => T::SHAPE.vtable.cmp.ord.unwrap()(
                                PtrConst::new(a.into()),
                                PtrConst::new(b.into()),
                            ),
                        }
                    })
                } else {
                    None
                };

                vtable_sized.parse = {
                    if T::SHAPE.is_from_str() {
                        Some(|str, target| {
                            let mut t = MaybeUninit::<T>::uninit();
                            let parse = <VTableView<T>>::of().parse().unwrap();
                            let _res =
                                (parse)(str, TypedPtrUninit::new(NonNull::from(&mut t).cast()))?;
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

                vtable_sized.try_from = Some(try_from::<T>);
                vtable_sized.try_into_inner = Some(try_into_inner::<T>);
                vtable_sized.try_borrow_inner = Some(try_borrow_inner::<T>);
            }

            vtable
        };

        ShapeBuilder::for_sized::<Self>(
            |f, opts| {
                write!(f, "{}", Self::SHAPE.type_identifier)?;
                if let Some(opts) = opts.for_children() {
                    write!(f, "<")?;
                    (T::SHAPE.vtable.type_name())(f, opts)?;
                    write!(f, ">")?;
                } else {
                    write!(f, "<…>")?;
                }
                Ok(())
            },
            "Option",
        )
        .vtable(vtable)
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
                            Variant {
                                name: "None",
                                discriminant: Some(0),
                                attributes: &[],
                                data: StructType {
                                    repr: Repr::default(),
                                    kind: StructKind::Unit,
                                    fields: &[],
                                },
                                doc: &[],
                            },
                            Variant {
                                name: "Some",
                                discriminant: Some(0),
                                attributes: &[],
                                data: StructType {
                                    repr: Repr::default(),
                                    kind: StructKind::TupleStruct,
                                    fields: &const {
                                        [Field {
                                            name: "0",
                                            shape: ShapeRef::Static(T::SHAPE),
                                            offset: 0,
                                            attributes: &[],
                                            doc: &[],
                                        }]
                                    },
                                },
                                doc: &[],
                            },
                        ]
                    },
                })
            } else {
                UserType::Opaque
            },
        ))
        .def(Def::Option(OptionDef::new(
            &const {
                OptionVTable::new(
                    |option| unsafe { option.get::<Option<T>>().is_some() },
                    |option| unsafe {
                        option
                            .get::<Option<T>>()
                            .as_ref()
                            .map(|t| PtrConst::new(NonNull::from(t)))
                    },
                    |option, value| unsafe { option.put(Option::Some(value.read::<T>())) },
                    |option| unsafe { option.put(<Option<T>>::None) },
                    |option, value| unsafe {
                        let option = option.as_mut::<Option<T>>();
                        match value {
                            Some(value) => option.replace(value.read::<T>()),
                            None => option.take(),
                        };
                    },
                )
            },
            T::SHAPE,
        )))
        .type_params(&[TypeParam {
            name: "T",
            shape: T::SHAPE,
        }])
        .inner(T::SHAPE)
        .build()
    };
}
