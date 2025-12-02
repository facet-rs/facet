use core::{cmp::Ordering, ptr::NonNull};

use crate::{
    Def, Facet, PtrConst, ResultDef, ResultVTable, Shape, Type, UserType, shape_util, value_vtable,
};

unsafe impl<'a, T: Facet<'a>, E: Facet<'a>> Facet<'a> for Result<T, E> {
    const SHAPE: &'static Shape = &const {
        Shape::builder_for_sized::<Self>()
            .vtable({
                let mut vtable = value_vtable!(core::result::Result<T, E>, |f, opts| {
                    write!(f, "{}", Self::SHAPE.type_identifier)?;
                    if let Some(opts) = opts.for_children() {
                        write!(f, "<")?;
                        (T::SHAPE.vtable.type_name())(f, opts)?;
                        write!(f, ", ")?;
                        (E::SHAPE.vtable.type_name())(f, opts)?;
                        write!(f, ">")?;
                    } else {
                        write!(f, "<…>")?;
                    }
                    Ok(())
                });

                {
                    let vtable_sized = &mut vtable;
                    vtable_sized.debug = if T::SHAPE.is_debug() && E::SHAPE.is_debug() {
                        Some(|this, f| {
                            let this = unsafe { this.get::<Self>() };
                            match this {
                                Ok(value) => f
                                    .debug_tuple("Ok")
                                    .field(&shape_util::Debug {
                                        ptr: PtrConst::new(value.into()),
                                        f: T::SHAPE.vtable.debug.unwrap(),
                                    })
                                    .finish(),
                                Err(err) => f
                                    .debug_tuple("Err")
                                    .field(&shape_util::Debug {
                                        ptr: PtrConst::new(err.into()),
                                        f: E::SHAPE.vtable.debug.unwrap(),
                                    })
                                    .finish(),
                            }
                        })
                    } else {
                        None
                    };

                    vtable_sized.partial_eq =
                        if T::SHAPE.is_partial_eq() && E::SHAPE.is_partial_eq() {
                            Some(|a, b| unsafe {
                                let a = a.get::<Self>();
                                let b = b.get::<Self>();
                                match (a, b) {
                                    (Ok(a), Ok(b)) => T::SHAPE.vtable.partial_eq.unwrap()(
                                        PtrConst::new(a.into()),
                                        PtrConst::new(b.into()),
                                    ),
                                    (Err(a), Err(b)) => E::SHAPE.vtable.partial_eq.unwrap()(
                                        PtrConst::new(a.into()),
                                        PtrConst::new(b.into()),
                                    ),
                                    _ => false,
                                }
                            })
                        } else {
                            None
                        };

                    vtable_sized.partial_ord =
                        if T::SHAPE.is_partial_ord() && E::SHAPE.is_partial_ord() {
                            Some(|a, b| unsafe {
                                let a = a.get::<Self>();
                                let b = b.get::<Self>();
                                match (a, b) {
                                    (Ok(a), Ok(b)) => T::SHAPE.vtable.partial_ord.unwrap()(
                                        PtrConst::new(a.into()),
                                        PtrConst::new(b.into()),
                                    ),
                                    (Err(a), Err(b)) => E::SHAPE.vtable.partial_ord.unwrap()(
                                        PtrConst::new(a.into()),
                                        PtrConst::new(b.into()),
                                    ),
                                    (Ok(_), Err(_)) => Some(Ordering::Greater),
                                    (Err(_), Ok(_)) => Some(Ordering::Less),
                                }
                            })
                        } else {
                            None
                        };

                    vtable_sized.ord = if T::SHAPE.is_ord() && E::SHAPE.is_ord() {
                        Some(|a, b| unsafe {
                            let a = a.get::<Self>();
                            let b = b.get::<Self>();
                            match (a, b) {
                                (Ok(a), Ok(b)) => T::SHAPE.vtable.ord.unwrap()(
                                    PtrConst::new(a.into()),
                                    PtrConst::new(b.into()),
                                ),
                                (Err(a), Err(b)) => E::SHAPE.vtable.ord.unwrap()(
                                    PtrConst::new(a.into()),
                                    PtrConst::new(b.into()),
                                ),
                                (Ok(_), Err(_)) => Ordering::Greater,
                                (Err(_), Ok(_)) => Ordering::Less,
                            }
                        })
                    } else {
                        None
                    };

                    vtable_sized.hash = if T::SHAPE.is_hash() && E::SHAPE.is_hash() {
                        Some(|this, hasher| unsafe {
                            use core::hash::Hash;
                            let this = this.get::<Self>();
                            match this {
                                Ok(value) => {
                                    (
                                        0u8,
                                        shape_util::Hash {
                                            ptr: PtrConst::new(value.into()),
                                            f: T::SHAPE.vtable.hash.unwrap(),
                                        },
                                    )
                                        .hash(&mut { hasher });
                                }
                                Err(err) => {
                                    (
                                        1u8,
                                        shape_util::Hash {
                                            ptr: PtrConst::new(err.into()),
                                            f: E::SHAPE.vtable.hash.unwrap(),
                                        },
                                    )
                                        .hash(&mut { hasher });
                                }
                            }
                        })
                    } else {
                        None
                    };
                }

                vtable
            })
            .type_identifier("Result")
            .type_params(&[
                crate::TypeParam {
                    name: "T",
                    shape: T::SHAPE,
                },
                crate::TypeParam {
                    name: "E",
                    shape: E::SHAPE,
                },
            ])
            // Result's layout is complex and depends on T and E, so we treat it as Opaque
            // and rely on the ResultDef vtable for interaction
            .ty(Type::User(UserType::Opaque))
            .def(Def::Result(
                ResultDef::builder()
                    .t(T::SHAPE)
                    .e(E::SHAPE)
                    .vtable(
                        const {
                            &ResultVTable::builder()
                                .is_ok(|result| unsafe { result.get::<Result<T, E>>().is_ok() })
                                .get_ok(|result| unsafe {
                                    result
                                        .get::<Result<T, E>>()
                                        .as_ref()
                                        .ok()
                                        .map(|t| PtrConst::new(NonNull::from(t)))
                                })
                                .get_err(|result| unsafe {
                                    result
                                        .get::<Result<T, E>>()
                                        .as_ref()
                                        .err()
                                        .map(|e| PtrConst::new(NonNull::from(e)))
                                })
                                .init_ok(|result, value| unsafe {
                                    result.put(Result::<T, E>::Ok(value.read::<T>()))
                                })
                                .init_err(|result, value| unsafe {
                                    result.put(Result::<T, E>::Err(value.read::<E>()))
                                })
                                .build()
                        },
                    )
                    .build(),
            ))
            .build()
    };
}
