use core::{fmt, hash::Hash};

use crate::{
    Facet, HasherProxy, MarkerTraits, PointerType, Shape, Type, TypeParam, ValuePointerType,
    ValueVTable,
};

macro_rules! impl_facet_for_pointer {
    ($variant:ident: $type:ty => $shape:expr => $vtable_builder:expr => $ptr_type:ident, $mutable:expr) => {
        unsafe impl<'a, T: Facet<'a> + ?Sized> Facet<'a> for $type {
            const VTABLE: &'static ValueVTable = &const {
                $vtable_builder
                    .type_name(|f, opts| {
                        if let Some(opts) = opts.for_children() {
                            if stringify!($ptr_type) == "Raw" {
                                if $mutable {
                                    write!(f, "*mut ")?;
                                } else {
                                    write!(f, "*const ")?;
                                }
                            } else {
                                write!(f, "&")?;
                                if $mutable {
                                    write!(f, "mut ")?;
                                }
                            }
                            (T::VTABLE.type_name)(f, opts)
                        } else {
                            if stringify!($ptr_type) == "Raw" {
                                if $mutable {
                                    write!(f, "*mut ⋯")
                                } else {
                                    write!(f, "*const ⋯")
                                }
                            } else {
                                write!(f, "&")?;
                                if $mutable {
                                    write!(f, "mut ⋯")
                                } else {
                                    write!(f, "⋯")
                                }
                            }
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
                    .ty({
                        let is_wide =
                            ::core::mem::size_of::<$type>() != ::core::mem::size_of::<*const ()>();
                        let vpt = ValuePointerType {
                            mutable: $mutable,
                            wide: is_wide,
                            target: || T::SHAPE,
                        };

                        Type::Pointer(PointerType::$ptr_type(vpt))
                    })
                    .build()
            };
        }
    };
}

// *const pointers
impl_facet_for_pointer!(
    Raw: *const T
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
        => Raw, false
);

// *mut pointers
impl_facet_for_pointer!(
    Raw: *mut T
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
        => Raw, true
);

// &T references
impl_facet_for_pointer!(
    Reference: &'a T
        => Shape::builder_for_sized::<Self>()
        => ValueVTable::builder::<Self>()
            .marker_traits(
                MarkerTraits::UNPIN
                    .union(MarkerTraits::COPY)
            )
            .clone_into(|src, dst| unsafe { dst.put(core::ptr::read(src)) })
        => Reference, false
);

// &mut T references
impl_facet_for_pointer!(
    Reference: &'a mut T
        => Shape::builder_for_sized::<Self>()
        => ValueVTable::builder::<Self>()
            .marker_traits(
                MarkerTraits::UNPIN
            )
        => Reference, true
);
