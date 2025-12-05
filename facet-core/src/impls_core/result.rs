use core::cmp::Ordering;
use core::hash::Hash;
use core::ptr::NonNull;

use crate::shape_util;
use crate::{Def, Facet, PtrConst, ResultDef, ResultVTable, Shape, ShapeBuilder, Type, UserType};

unsafe impl<'a, T: Facet<'a>, E: Facet<'a>> Facet<'a> for Result<T, E> {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<Self>(
            |f, opts| {
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
            },
            "Result",
        )
        .ty(Type::User(UserType::Opaque))
        .def(Def::Result(ResultDef::new(
            &const {
                ResultVTable::new(
                    |result| unsafe { result.get::<Result<T, E>>().is_ok() },
                    |result| unsafe {
                        result
                            .get::<Result<T, E>>()
                            .as_ref()
                            .ok()
                            .map(|t| PtrConst::new(NonNull::from(t)))
                    },
                    |result| unsafe {
                        result
                            .get::<Result<T, E>>()
                            .as_ref()
                            .err()
                            .map(|e| PtrConst::new(NonNull::from(e)))
                    },
                    |result, value| unsafe { result.put(Result::<T, E>::Ok(value.read::<T>())) },
                    |result, value| unsafe { result.put(Result::<T, E>::Err(value.read::<E>())) },
                )
            },
            T::SHAPE,
            E::SHAPE,
        )))
        .debug_opt(if T::SHAPE.is_debug() && E::SHAPE.is_debug() {
            Some(|this, f| {
                let this = unsafe { this.get::<Self>() };
                match this {
                    Ok(value) => f
                        .debug_tuple("Ok")
                        .field(&shape_util::Debug {
                            ptr: PtrConst::new(value.into()),
                            f: T::SHAPE.vtable.format.debug.unwrap(),
                        })
                        .finish(),
                    Err(err) => f
                        .debug_tuple("Err")
                        .field(&shape_util::Debug {
                            ptr: PtrConst::new(err.into()),
                            f: E::SHAPE.vtable.format.debug.unwrap(),
                        })
                        .finish(),
                }
            })
        } else {
            None
        })
        .partial_eq_opt(if T::SHAPE.is_partial_eq() && E::SHAPE.is_partial_eq() {
            Some(|a, b| unsafe {
                let a = a.get::<Self>();
                let b = b.get::<Self>();
                match (a, b) {
                    (Ok(a), Ok(b)) => T::SHAPE.vtable.cmp.partial_eq.unwrap()(
                        PtrConst::new(a.into()),
                        PtrConst::new(b.into()),
                    ),
                    (Err(a), Err(b)) => E::SHAPE.vtable.cmp.partial_eq.unwrap()(
                        PtrConst::new(a.into()),
                        PtrConst::new(b.into()),
                    ),
                    _ => false,
                }
            })
        } else {
            None
        })
        .partial_ord_opt(if T::SHAPE.is_partial_ord() && E::SHAPE.is_partial_ord() {
            Some(|a, b| unsafe {
                let a = a.get::<Self>();
                let b = b.get::<Self>();
                match (a, b) {
                    (Ok(a), Ok(b)) => T::SHAPE.vtable.cmp.partial_ord.unwrap()(
                        PtrConst::new(a.into()),
                        PtrConst::new(b.into()),
                    ),
                    (Err(a), Err(b)) => E::SHAPE.vtable.cmp.partial_ord.unwrap()(
                        PtrConst::new(a.into()),
                        PtrConst::new(b.into()),
                    ),
                    (Ok(_), Err(_)) => Some(Ordering::Greater),
                    (Err(_), Ok(_)) => Some(Ordering::Less),
                }
            })
        } else {
            None
        })
        .ord_opt(if T::SHAPE.is_ord() && E::SHAPE.is_ord() {
            Some(|a, b| unsafe {
                let a = a.get::<Self>();
                let b = b.get::<Self>();
                match (a, b) {
                    (Ok(a), Ok(b)) => T::SHAPE.vtable.cmp.ord.unwrap()(
                        PtrConst::new(a.into()),
                        PtrConst::new(b.into()),
                    ),
                    (Err(a), Err(b)) => E::SHAPE.vtable.cmp.ord.unwrap()(
                        PtrConst::new(a.into()),
                        PtrConst::new(b.into()),
                    ),
                    (Ok(_), Err(_)) => Ordering::Greater,
                    (Err(_), Ok(_)) => Ordering::Less,
                }
            })
        } else {
            None
        })
        .hash_opt(if T::SHAPE.is_hash() && E::SHAPE.is_hash() {
            Some(|this, hasher| unsafe {
                let this = this.get::<Self>();
                match this {
                    Ok(value) => {
                        (
                            0u8,
                            shape_util::Hash {
                                ptr: PtrConst::new(value.into()),
                                f: T::SHAPE.vtable.hash.hash.unwrap(),
                            },
                        )
                            .hash(&mut { hasher });
                    }
                    Err(err) => {
                        (
                            1u8,
                            shape_util::Hash {
                                ptr: PtrConst::new(err.into()),
                                f: E::SHAPE.vtable.hash.hash.unwrap(),
                            },
                        )
                            .hash(&mut { hasher });
                    }
                }
            })
        } else {
            None
        })
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
        .build()
    };
}
