use core::fmt;

use crate::{
    Def, Facet, KnownPointer, MarkerTraits, PointerDef, PointerFlags, PointerType, PointerVTable,
    PtrConst, PtrConstWide, Shape, Type, TypeParam, VTableView, ValuePointerType, ValueVTable,
};

macro_rules! impl_for_ref {
    ($($modifiers:tt)*) => {
        unsafe impl<'a, T: Facet<'a>> Facet<'a> for &'a $($modifiers)* T {
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
                                view.display().unwrap()(*value, f)
                            })
                        } else {
                            None
                        }
                    })
                    .debug(|| {
                        if T::VTABLE.has_debug() {
                            Some(|value, f| {
                                let view = VTableView::<T>::of();
                                view.debug().unwrap()(*value, f)
                            })
                        } else {
                            None
                        }
                    })
                    .clone_into(|| {
                        if stringify!($($modifiers)*).is_empty() {
                            Some(|src, dst| unsafe { dst.put(core::ptr::read(src)) })
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

            const SHAPE: &'static Shape<'static> = &const {
                Shape::builder_for_sized::<Self>()
                    .type_identifier("&")
                    .type_params(&[TypeParam {
                        name: "T",
                        shape: || T::SHAPE,
                    }])
                    .ty({
                        let vpt = ValuePointerType {
                            mutable: !stringify!($($modifiers)*).is_empty(),
                            wide: false,
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
                                            PtrConst::new(*ptr).into()
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

macro_rules! impl_for_string_ref {
    ($($modifiers:tt)*) => {
        unsafe impl<'a> Facet<'a> for &'a $($modifiers)* str {
            const VTABLE: &'static ValueVTable = &const {
                ValueVTable::builder::<Self>()
                    .marker_traits(|| {
                        let mut marker_traits = MarkerTraits::COPY.union(MarkerTraits::UNPIN);
                        if str::SHAPE.vtable.marker_traits().contains(MarkerTraits::EQ) {
                            marker_traits = marker_traits.union(MarkerTraits::EQ);
                        }
                        if str::SHAPE
                            .vtable
                            .marker_traits()
                            .contains(MarkerTraits::SYNC)
                        {
                            marker_traits = marker_traits
                                .union(MarkerTraits::SEND)
                                .union(MarkerTraits::SYNC);
                        }
                        if str::SHAPE
                            .vtable
                            .marker_traits()
                            .contains(MarkerTraits::REF_UNWIND_SAFE)
                        {
                            marker_traits = marker_traits
                                .union(MarkerTraits::UNWIND_SAFE)
                                .union(MarkerTraits::REF_UNWIND_SAFE);
                        }

                        marker_traits
                    })
                    .display(|| Some(fmt::Display::fmt))
                    .debug(|| Some(fmt::Debug::fmt))
                    .clone_into(|| {
                        if stringify!($($modifiers)*).is_empty() {
                            Some(|src, dst| unsafe { dst.put(core::ptr::read(src)) })
                        } else {
                            None
                        }
                    })
                    .type_name(|f, _opts| {
                        if stringify!($($modifiers)*).is_empty() {
                            write!(f, "&str")
                        } else {
                            write!(f, "&mut str")
                        }
                    })
                    .build()
            };

            const SHAPE: &'static Shape<'static> = &const {
                Shape::builder_for_sized::<Self>()
                    .type_identifier("&_")
                    .type_params(&[TypeParam {
                        name: "T",
                        shape: || str::SHAPE,
                    }])
                    .ty({
                        let vpt = ValuePointerType {
                            mutable: !stringify!($($modifiers)*).is_empty(),
                            wide: true, // string slices are always wide (fat pointers)
                            target: || str::SHAPE,
                        };

                        Type::Pointer(PointerType::Reference(vpt))
                    })
                    .build()
            };
        }
    };
}

impl_for_string_ref!();
impl_for_string_ref!(mut);

macro_rules! impl_for_slice_ref {
    ($($modifiers:tt)*) => {
        unsafe impl<'a, U: Facet<'a>> Facet<'a> for &'a $($modifiers)* [U] {
            const VTABLE: &'static ValueVTable = &const {
                ValueVTable::builder::<Self>()
                    .marker_traits(|| {
                        let mut marker_traits = MarkerTraits::COPY.union(MarkerTraits::UNPIN);
                        if <[U]>::SHAPE
                            .vtable
                            .marker_traits()
                            .contains(MarkerTraits::EQ)
                        {
                            marker_traits = marker_traits.union(MarkerTraits::EQ);
                        }
                        if <[U]>::SHAPE
                            .vtable
                            .marker_traits()
                            .contains(MarkerTraits::SYNC)
                        {
                            marker_traits = marker_traits
                                .union(MarkerTraits::SEND)
                                .union(MarkerTraits::SYNC);
                        }
                        if <[U]>::SHAPE
                            .vtable
                            .marker_traits()
                            .contains(MarkerTraits::REF_UNWIND_SAFE)
                        {
                            marker_traits = marker_traits
                                .union(MarkerTraits::UNWIND_SAFE)
                                .union(MarkerTraits::REF_UNWIND_SAFE);
                        }

                        marker_traits
                    })
                    .debug(|| {
                        if <[U]>::VTABLE.has_debug() {
                            Some(|value, f| {
                                let view = VTableView::<[U]>::of();
                                view.debug().unwrap()(*value, f)
                            })
                        } else {
                            None
                        }
                    })
                    .clone_into(|| Some(|src, dst| unsafe { dst.put(core::ptr::read(src)) }))
                    .type_name(|f, opts| {
                        if stringify!($($modifiers)*).is_empty() {
                            if let Some(opts) = opts.for_children() {
                                write!(f, "&[")?;
                                (<U>::VTABLE.type_name())(f, opts)?;
                                write!(f, "]")
                            } else {
                                write!(f, "&⋯")
                            }
                        } else {
                            if let Some(opts) = opts.for_children() {
                                write!(f, "&mut [")?;
                                (<U>::VTABLE.type_name())(f, opts)?;
                                write!(f, "]")
                            } else {
                                write!(f, "&mut ⋯")
                            }
                        }
                    })
                    .build()
            };

            const SHAPE: &'static Shape<'static> = &const {
                Shape::builder_for_sized::<Self>()
                    .type_identifier("&[_]")
                    .type_params(&[TypeParam {
                        name: "T",
                        shape: || <[U]>::SHAPE,
                    }])
                    .ty({
                        let vpt = ValuePointerType {
                            mutable: !stringify!($($modifiers)*).is_empty(),
                            wide: true, // slice references are always wide (fat pointers)
                            target: || <[U]>::SHAPE,
                        };

                        Type::Pointer(PointerType::Reference(vpt))
                    })
                    .def(Def::Pointer(
                        PointerDef::builder()
                            .pointee(|| <[U]>::SHAPE)
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
                                            // `this` is a PtrConst pointing to our slice reference (&[U] or &mut [U])
                                            // We get a reference to our slice reference, so we have &&[U] or &&mut [U]
                                            let ptr: && $($modifiers)* [U] = unsafe { this.get::<Self>() };

                                            // Dereference once to get the actual slice reference: &[U] or &mut [U]
                                            // This is the wide pointer we want to return (contains ptr + length)
                                            // Note: Even for &mut [U], we can coerce to &[U] for borrowing
                                            let s: &[U] = *ptr;

                                            // Convert the slice reference to a raw pointer (*const [U])
                                            // The &raw const operator creates a raw pointer from a place expression
                                            // without going through a reference first, preserving the wide pointer
                                            PtrConstWide::new(&raw const *s).into()
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

impl_for_slice_ref!();
impl_for_slice_ref!(mut);
