use core::{fmt, hash::Hash};

use crate::{
    Facet, HasherProxy, MarkerTraits, PointerType, Shape, Type, TypeParam, VTableView,
    ValuePointerType, ValueVTable,
};

macro_rules! impl_facet_for_pointer {
    ($variant:ident: $type:ty => $shape:expr => $vtable_builder:expr => $($ptrkind:tt)+) => {
        unsafe impl<'a, T: Facet<'a> + ?Sized> Facet<'a> for $type {
            const VTABLE: &'static ValueVTable = &const {
                $vtable_builder
                    .type_name(|f, opts| {
                        if let Some(opts) = opts.for_children() {
                            write!(f, stringify!($($ptrkind)+, " "))?;
                            (T::VTABLE.type_name)(f, opts)
                        } else {
                            write!(f, stringify!($($ptrkind)+, " ⋯"))
                        }
                    })
                    .build()
            };

            const SHAPE: &'static Shape = &const {
                $shape
                    .type_params(&[TypeParam {
                        name: "T",
                        shape: || T::SHAPE,
                    }])
                    .ty(Type::Pointer(PointerType::Raw(ValuePointerType {
                        mutable: false,
                        wide: ::core::mem::size_of::<$($ptrkind)* ()>() != ::core::mem::size_of::<Self>(),
                        target: || T::SHAPE,
                    })))
                    .build()
            };
        }
    };
    (*$mutability:tt) => {
        impl_facet_for_pointer!(
            Raw: *$mutability T
                => Shape::builder_for_sized::<Self>()
                    .inner(|| T::SHAPE)
                => ValueVTable::builder::<Self>()
                    .marker_traits(
                        MarkerTraits::EQ
                            .union(MarkerTraits::COPY)
                            .union(MarkerTraits::UNPIN),
                    )
                    .debug(|data, f| fmt::Debug::fmt(data, f))
                    .clone_into(|src, dst| unsafe { dst.put(src.clone()) })
                    .eq(|left, right| left.cast::<()>().eq(&right.cast::<()>()))
                    .partial_ord(|&left, &right| {
                        left.cast::<()>().partial_cmp(&right.cast::<()>())
                    })
                    .ord(|&left, &right| left.cast::<()>().cmp(&right.cast::<()>()))
                    .hash(|value, hasher_this, hasher_write_fn| {
                        value.hash(&mut unsafe {
                            HasherProxy::new(hasher_this, hasher_write_fn)
                        })
                    })
                => *$mutability
        );
    };
    (@ $builder:expr => &$($mutability:tt)?) => {
        impl_facet_for_pointer!(
            Reference: &'a $($mutability)? T
                => Shape::builder_for_sized::<Self>()
                => {
                    let mut builder = $builder;

                    if T::VTABLE.default_in_place.is_some() {
                        builder = builder.default_in_place(|value| {
                            (<VTableView<&$($mutability)? T>>::of_deref().default_in_place().unwrap())(value)
                        });
                    }

                    if T::VTABLE.debug.is_some() {
                        builder = builder.debug(|value, f| {
                            (<VTableView<&$($mutability)? T>>::of_deref().debug().unwrap())(value, f)
                        });
                    }

                    if T::VTABLE.display.is_some() {
                        builder = builder.display(|value, f| {
                            (<VTableView<&$($mutability)? T>>::of_deref().display().unwrap())(value, f)
                        });
                    }

                    if T::VTABLE.eq.is_some() {
                        builder = builder.eq(|a, b| {
                            (<VTableView<&$($mutability)? T>>::of_deref().eq().unwrap())(a, b)
                        });
                    }

                    if T::VTABLE.ord.is_some() {
                        builder = builder.ord(|a, b| {
                            (<VTableView<&$($mutability)? T>>::of_deref().ord().unwrap())(a, b)
                        });
                    }

                    if T::VTABLE.partial_ord.is_some() {
                        builder = builder.partial_ord(|a, b| {
                            (<VTableView<&$($mutability)? T>>::of_deref().partial_ord().unwrap())(a, b)
                        });
                    }

                    if T::VTABLE.hash.is_some() {
                        builder = builder.hash(|value, state, hasher| {
                            (<VTableView<&$($mutability)? T>>::of_deref().hash().unwrap())(value, state, hasher)
                        });
                    }

                    builder
                }
                => &$($mutability)?
        );
    };
    (&) => {
        impl_facet_for_pointer!(@ ValueVTable::builder::<Self>()
            .marker_traits(
                MarkerTraits::UNPIN
                    .union(MarkerTraits::COPY)
            )
            .clone_into(|src, dst| unsafe { dst.put(core::ptr::read(src)) })
        => &);
    };
    (&mut) => {
        impl_facet_for_pointer!(@ ValueVTable::builder::<Self>()
            .marker_traits(
                MarkerTraits::UNPIN
            )
        => &mut
        );
    };
}

impl_facet_for_pointer!(*const);
impl_facet_for_pointer!(*mut);
impl_facet_for_pointer!(&mut);
impl_facet_for_pointer!(&);
