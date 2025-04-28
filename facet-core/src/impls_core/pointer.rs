use core::{fmt, hash::Hash};

use crate::{
    Facet, HasherProxy, MarkerTraits, PointerType, Shape, Type, TypeParam, ValuePointerType,
    ValueVTable,
};

macro_rules! impl_facet_for_pointer {
    ($variant:ident: $type:ty => $shape:expr => $vtable:expr => $($ptrkind:tt)+) => {
        unsafe impl<'a, T: Facet<'a> + ?Sized> Facet<'a> for $type {
            const SHAPE: &'static Shape = &const {
                $shape
                    .type_params(&[TypeParam {
                        name: "T",
                        shape: || T::SHAPE,
                    }])
                    .vtable(
                        const {
                            &$vtable
                                .type_name(|f, opts| {
                                    if let Some(opts) = opts.for_children() {
                                        write!(f, stringify!($($ptrkind)+, " "))?;
                                        (T::SHAPE.vtable.type_name)(f, opts)
                                    } else {
                                        write!(f, stringify!($($ptrkind)+, " ⋯"))
                                    }
                                })
                                .build()
                        },
                    )
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
    (&$($mutability:tt)?) => {
        impl_facet_for_pointer!(
            Reference: &'a $($mutability)? T
                => Shape::builder_for_sized::<Self>()
                => ValueVTable::builder::<Self>()
                    .marker_traits(
                        MarkerTraits::COPY
                            .union(MarkerTraits::UNPIN),
                    )
                => &$($mutability)?
        );
    };
}

impl_facet_for_pointer!(*const);
impl_facet_for_pointer!(*mut);
impl_facet_for_pointer!(&mut);
impl_facet_for_pointer!(&);
