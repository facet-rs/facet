use crate::{
    Def, Facet, KnownPointer, MarkerTraits, PointerDef, PointerFlags, PointerType, PointerVTable,
    PtrConst, Shape, Type, TypeParam, VTableView, ValuePointerType, ValueVTable,
};

macro_rules! impl_for_ref {
    ($($modifiers:tt)*) => {
        unsafe impl<'a, T: ?Sized + Facet<'a>> Facet<'a> for &'a $($modifiers)* T {
            const VTABLE: &'static ValueVTable = &const {
                ValueVTable::builder::<Self>()
                    .marker_traits(|| {
                        let mut marker_traits = if stringify!($($modifiers)*).is_empty() {
                            MarkerTraits::COPY.union(MarkerTraits::UNPIN)
                        } else {
                            MarkerTraits::UNPIN
                        };
                        if T::SHAPE.vtable.marker_traits().contains(MarkerTraits::EQ) {
                            marker_traits = marker_traits.union(MarkerTraits::EQ);
                        }
                        if T::SHAPE.vtable.marker_traits().contains(MarkerTraits::SYNC) {
                            marker_traits = marker_traits
                                .union(MarkerTraits::SEND)
                                .union(MarkerTraits::SYNC);
                        }
                        if T::SHAPE
                            .vtable
                            .marker_traits()
                            .contains(MarkerTraits::REF_UNWIND_SAFE)
                        {
                            marker_traits = marker_traits.union(MarkerTraits::REF_UNWIND_SAFE);
                            if stringify!($($modifiers)*).is_empty() {
                                marker_traits = marker_traits.union(MarkerTraits::UNWIND_SAFE);
                            }
                        }

                        marker_traits
                    })
                    .display(|| {
                        if T::VTABLE.has_display() {
                            Some(|value, f| {
                                let view = VTableView::<T>::of();
                                view.display().unwrap()((&**value.get()).into(), f)
                            })
                        } else {
                            None
                        }
                    })
                    .debug(|| {
                        if T::VTABLE.has_debug() {
                            Some(|value, f| {
                                let view = VTableView::<T>::of();
                                view.debug().unwrap()((&**value.get()).into(), f)
                            })
                        } else {
                            None
                        }
                    })
                    .clone_into(|| {
                        if stringify!($($modifiers)*).is_empty() {
                            Some(|src, dst| unsafe { dst.put(core::ptr::read(src.as_ptr())).into() })
                        } else {
                            None
                        }
                    })
                    .type_name(|f, opts| {
                        if stringify!($($modifiers)*).is_empty() {
                            if let Some(opts) = opts.for_children() {
                                write!(f, "&")?;
                                (T::VTABLE.type_name())(f, opts)
                            } else {
                                write!(f, "&⋯")
                            }
                        } else {
                            if let Some(opts) = opts.for_children() {
                                write!(f, "&mut ")?;
                                (T::VTABLE.type_name())(f, opts)
                            } else {
                                write!(f, "&mut ⋯")
                            }
                        }
                    })
                    .build()
            };

            const SHAPE: &'static Shape = &const {
                Shape::builder_for_sized::<Self>()
                    .type_identifier("&")
                    .type_params(&[TypeParam {
                        name: "T",
                        shape: || T::SHAPE,
                    }])
                    .ty({
                        let vpt = ValuePointerType {
                            mutable: !stringify!($($modifiers)*).is_empty(),
                            wide: size_of::<*const T>() != size_of::<*const ()>(),
                            target: || T::SHAPE,
                        };

                        Type::Pointer(PointerType::Reference(vpt))
                    })
                    .def(Def::Pointer(
                        PointerDef::builder()
                            .pointee(|| T::SHAPE)
                            .flags(PointerFlags::EMPTY)
                            .known(if stringify!($($modifiers)*).is_empty() {
                                KnownPointer::SharedReference
                            } else {
                                KnownPointer::ExclusiveReference
                            })
                            .vtable(
                                &const {
                                    PointerVTable::builder()
                                        .borrow_fn(|this| {
                                            let ptr: && $($modifiers)* T = unsafe { this.get::<Self>() };
                                            let ptr: &T = *ptr;
                                            PtrConst::new(core::ptr::NonNull::from(ptr)).into()
                                        })
                                        .build()
                                },
                            )
                            .build(),
                    ))
                    .build()
            };
        }
    };
}

impl_for_ref!();
impl_for_ref!(mut);
