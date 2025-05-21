use crate::{Facet, Opaque, Shape};

#[doc(hidden)]
pub const fn shape_of<'a, TStruct, TField: Facet<'a>>(
    _f: &dyn Fn(&TStruct) -> &TField,
) -> &'static Shape<'static> {
    TField::SHAPE
}

#[doc(hidden)]
pub const fn shape_of_opaque<'a, TStruct, TField>(
    _f: &dyn Fn(&TStruct) -> &TField,
) -> &'static Shape<'static>
where
    Opaque<TField>: Facet<'a>,
{
    Opaque::<TField>::SHAPE
}

/// Creates a `ValueVTable` for a given type.
///
/// This macro generates a `ValueVTable` with implementations for various traits
/// (Display, Debug, PartialEq, Eq, PartialOrd, Ord, Hash) if they are implemented for the given type.
///
/// # Arguments
///
/// * `$type_name:ty` - The type for which to create the `ValueVTable`.
/// * `$type_name_fn:expr` - A function that writes the type name to a formatter.
///
/// # Example
///
/// ```
/// use facet_core::value_vtable;
/// use core::fmt::{self, Formatter};
/// use facet_core::TypeNameOpts;
///
/// let vtable = value_vtable!(String, |f: &mut Formatter<'_>, _opts: TypeNameOpts| write!(f, "String"));
/// ```
///
/// This cannot be used for a generic type because the `impls!` thing depends on type bounds.
/// If you have a generic type, you need to do specialization yourself, like we do for slices,
/// arrays, etc. — essentially, this macro is only useful for 1) scalars, 2) inside a derive macro
#[macro_export]
macro_rules! value_vtable {
    ($type_name:ty, $type_name_fn:expr) => {
        const {
            let mut builder = $crate::ValueVTable::builder::<$type_name>()
                .type_name($type_name_fn);

            builder = builder.display(|| {
                if $crate::spez::impls!($type_name: core::fmt::Display) {
                    Some(|data, f| {
                        use $crate::spez::*;
                        (&&Spez(data)).spez_display(f)
                    })
                } else {
                    None
                }
            });

            builder = builder.debug(|| {
                if $crate::spez::impls!($type_name: core::fmt::Debug) {
                    Some(|data, f| {
                        use $crate::spez::*;
                        (&&Spez(data)).spez_debug(f)
                    })
                } else {
                    None
                }
            });

            builder = builder.default_in_place(|| {
                if $crate::spez::impls!($type_name: core::default::Default) {
                    Some(|target| unsafe {
                        use $crate::spez::*;
                        (&&SpezEmpty::<$type_name>::SPEZ).spez_default_in_place(target.into()).as_mut()
                    })
                } else {
                    None
                }
            });

            builder = builder.clone_into(|| {
                if $crate::spez::impls!($type_name: core::clone::Clone) {
                    Some(|src, dst| unsafe {
                        use $crate::spez::*;
                        (&&Spez(src)).spez_clone_into(dst.into()).as_mut()
                    })
                } else {
                    None
                }
            });

            builder = builder.marker_traits(|| {
                let mut traits = $crate::MarkerTraits::empty();
                if $crate::spez::impls!($type_name: core::cmp::Eq) {
                    traits = traits.union($crate::MarkerTraits::EQ);
                }
                if $crate::spez::impls!($type_name: core::marker::Send) {
                    traits = traits.union($crate::MarkerTraits::SEND);
                }
                if $crate::spez::impls!($type_name: core::marker::Sync) {
                    traits = traits.union($crate::MarkerTraits::SYNC);
                }
                if $crate::spez::impls!($type_name: core::marker::Copy) {
                    traits = traits.union($crate::MarkerTraits::COPY);
                }
                if $crate::spez::impls!($type_name: core::marker::Unpin) {
                    traits = traits.union($crate::MarkerTraits::UNPIN);
                }

                traits
            });

            builder = builder.eq(|| {
                if $crate::spez::impls!($type_name: core::cmp::PartialEq) {
                    Some(|left, right| {
                        use $crate::spez::*;
                        (&&Spez(left))
                            .spez_eq(&&Spez(right))
                    })
                } else {
                    None
                }
            });

            builder = builder.partial_ord(|| {
                if $crate::spez::impls!($type_name: core::cmp::PartialOrd) {
                    Some(|left, right| {
                        use $crate::spez::*;
                        (&&Spez(left))
                            .spez_partial_cmp(&&Spez(right))
                    })
                } else {
                    None
                }
            });

            builder = builder.ord(|| {
                if $crate::spez::impls!($type_name: core::cmp::Ord) {
                    Some(|left, right| {
                        use $crate::spez::*;
                        (&&Spez(left))
                            .spez_cmp(&&Spez(right))
                    })
                } else {
                    None
                }
            });

            builder = builder.hash(|| {
                if $crate::spez::impls!($type_name: core::hash::Hash) {
                    Some(|value, hasher_this, hasher_write_fn| {
                        use $crate::spez::*;
                        use $crate::HasherProxy;
                        (&&Spez(value))
                            .spez_hash(&mut unsafe { HasherProxy::new(hasher_this, hasher_write_fn) })
                    })
                } else {
                    None
                }
            });

            builder = builder.parse(|| {
                if $crate::spez::impls!($type_name: core::str::FromStr) {
                    Some(|s, target| {
                        use $crate::spez::*;
                        let res = unsafe { (&&SpezEmpty::<$type_name>::SPEZ).spez_parse(s, target.into()) };
                        res.map(|res| unsafe { res.as_mut() })
                    })
                } else {
                    None
                }
            });

            builder.build()
        }
    };
}

/// Similar to `value_vtable!` macro but for `!Sized` types.
#[macro_export]
macro_rules! value_vtable_unsized {
    ($type_name:ty, $type_name_fn:expr) => {
        const {
            let mut builder = $crate::ValueVTable::builder_unsized::<$type_name>()
                .type_name($type_name_fn);

            builder = builder.display(|| {
                if $crate::spez::impls!($type_name: core::fmt::Display) {
                    Some(|data, f| {
                        use $crate::spez::*;
                        (&&Spez(data)).spez_display(f)
                    })
                } else {
                    None
                }
            });

            builder = builder.debug(|| {
                if $crate::spez::impls!($type_name: core::fmt::Debug) {
                    Some(|data, f| {
                        use $crate::spez::*;
                        (&&Spez(data)).spez_debug(f)
                    })
                } else {
                    None
                }
            });

            builder = builder.marker_traits(|| {
                let mut traits = $crate::MarkerTraits::empty();
                if $crate::spez::impls!($type_name: core::cmp::Eq) {
                    traits = traits.union($crate::MarkerTraits::EQ);
                }
                if $crate::spez::impls!($type_name: core::marker::Send) {
                    traits = traits.union($crate::MarkerTraits::SEND);
                }
                if $crate::spez::impls!($type_name: core::marker::Sync) {
                    traits = traits.union($crate::MarkerTraits::SYNC);
                }
                if $crate::spez::impls!($type_name: core::marker::Copy) {
                    traits = traits.union($crate::MarkerTraits::COPY);
                }
                if $crate::spez::impls!($type_name: core::marker::Unpin) {
                    traits = traits.union($crate::MarkerTraits::UNPIN);
                }

                traits
            });

            builder = builder.eq(|| {
                if $crate::spez::impls!($type_name: core::cmp::PartialEq) {
                    Some(|left, right| {
                        use $crate::spez::*;
                        (&&Spez(left))
                            .spez_eq(&&Spez(right))
                    })
                } else {
                    None
                }
            });

            builder = builder.partial_ord(|| {
                if $crate::spez::impls!($type_name: core::cmp::PartialOrd) {
                    Some(|left, right| {
                        use $crate::spez::*;
                        (&&Spez(left))
                            .spez_partial_cmp(&&Spez(right))
                    })
                } else {
                    None
                }
            });

            builder = builder.ord(|| {
                if $crate::spez::impls!($type_name: core::cmp::Ord) {
                    Some(|left, right| {
                        use $crate::spez::*;
                        (&&Spez(left))
                            .spez_cmp(&&Spez(right))
                    })
                } else {
                    None
                }
            });

            builder = builder.hash(|| {
                if $crate::spez::impls!($type_name: core::hash::Hash) {
                    Some(|value, hasher_this, hasher_write_fn| {
                        use $crate::spez::*;
                        use $crate::HasherProxy;
                        (&&Spez(value))
                            .spez_hash(&mut unsafe { HasherProxy::new(hasher_this, hasher_write_fn) })
                    })
                } else {
                    None
                }
            });

            builder.build()
        }
    };
}
