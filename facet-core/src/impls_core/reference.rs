use crate::{
    Def, Facet, KnownPointer, PointerDef, PointerFlags, PointerType, PointerVTable, PtrConst,
    Shape, ShapeBuilder, Type, TypeParam, TypedPtrConst, VTableView, ValuePointerType, ValueVTable,
};

macro_rules! impl_for_ref {
    ($($modifiers:tt)*) => {
        unsafe impl<'a, T: ?Sized + Facet<'a>> Facet<'a> for &'a $($modifiers)* T {
            const SHAPE: &'static Shape = &const {
                ShapeBuilder::for_sized::<Self>(
                    |f, opts| {
                        if stringify!($($modifiers)*).is_empty() {
                            if let Some(opts) = opts.for_children() {
                                write!(f, "&")?;
                                (T::SHAPE.vtable.type_name())(f, opts)
                            } else {
                                write!(f, "&…")
                            }
                        } else {
                            if let Some(opts) = opts.for_children() {
                                write!(f, "&mut ")?;
                                (T::SHAPE.vtable.type_name())(f, opts)
                            } else {
                                write!(f, "&mut …")
                            }
                        }
                    },
                    "&",
                )
                .drop_in_place(ValueVTable::drop_in_place_for::<Self>())
                .clone_into_opt({
                    if stringify!($($modifiers)*).is_empty() {
                        Some(|src, dst| unsafe { dst.put(core::ptr::read(src.as_ptr::<Self>())).into() })
                    } else {
                        None
                    }
                })
                .display_opt({
                    if T::SHAPE.vtable.has_display() {
                        Some(|value, f| unsafe {
                            let view = VTableView::<T>::of();
                            view.display().unwrap()(TypedPtrConst::from(&**value.get::<Self>()), f)
                        })
                    } else {
                        None
                    }
                })
                .debug_opt({
                    if T::SHAPE.vtable.has_debug() {
                        Some(|value, f| unsafe {
                            let view = VTableView::<T>::of();
                            view.debug().unwrap()(TypedPtrConst::from(&**value.get::<Self>()), f)
                        })
                    } else {
                        None
                    }
                })
                .partial_eq_opt({
                    if T::SHAPE.vtable.has_partial_eq() {
                        Some(|a, b| unsafe {
                            let view = VTableView::<T>::of();
                            view.partial_eq().unwrap()(TypedPtrConst::from(&**a.get::<Self>()), TypedPtrConst::from(&**b.get::<Self>()))
                        })
                    } else {
                        None
                    }
                })
                .partial_ord_opt({
                    if T::SHAPE.vtable.has_partial_ord() {
                        Some(|a, b| unsafe {
                            let view = VTableView::<T>::of();
                            view.partial_ord().unwrap()(TypedPtrConst::from(&**a.get::<Self>()), TypedPtrConst::from(&**b.get::<Self>()))
                        })
                    } else {
                        None
                    }
                })
                .ord_opt({
                    if T::SHAPE.vtable.has_ord() {
                        Some(|a, b| unsafe {
                            let view = VTableView::<T>::of();
                            view.ord().unwrap()(TypedPtrConst::from(&**a.get::<Self>()), TypedPtrConst::from(&**b.get::<Self>()))
                        })
                    } else {
                        None
                    }
                })
                .hash_opt({
                    if T::SHAPE.vtable.has_hash() {
                        Some(|value, hasher| unsafe {
                            let view = VTableView::<T>::of();
                            view.hash().unwrap()(TypedPtrConst::from(&**value.get::<Self>()), hasher)
                        })
                    } else {
                        None
                    }
                })
                .ty({
                    let vpt = ValuePointerType {
                        mutable: !stringify!($($modifiers)*).is_empty(),
                        wide: size_of::<*const T>() != size_of::<*const ()>(),
                        target: T::SHAPE,
                    };

                    Type::Pointer(PointerType::Reference(vpt))
                })
                .def(Def::Pointer(PointerDef {
                    vtable: &const {
                        PointerVTable {
                            borrow_fn: Some(|this| {
                                let ptr: && $($modifiers)* T = unsafe { this.get::<Self>() };
                                let ptr: &T = *ptr;
                                PtrConst::new(core::ptr::NonNull::from(ptr)).into()
                            }),
                            ..PointerVTable::new()
                        }
                    },
                    pointee: Some(T::SHAPE),
                    weak: None,
                    strong: None,
                    flags: PointerFlags::EMPTY,
                    known: Some(if stringify!($($modifiers)*).is_empty() {
                        KnownPointer::SharedReference
                    } else {
                        KnownPointer::ExclusiveReference
                    }),
                }))
                .type_params(&[TypeParam {
                    name: "T",
                    shape: T::SHAPE,
                }])
                .build()
            };
        }
    };
}

impl_for_ref!();
impl_for_ref!(mut);
