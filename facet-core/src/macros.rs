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

            if $crate::spez::impls!($type_name: core::fmt::Display) {
                builder = builder.display(|data, f| {
                    use $crate::spez::*;
                    (&&Spez(data)).spez_display(f)
                });
            }

            if $crate::spez::impls!($type_name: core::fmt::Debug) {
                builder = builder.debug(|data, f| {
                    use $crate::spez::*;
                    (&&Spez(data)).spez_debug(f)
                });
            }

            if $crate::spez::impls!($type_name: core::default::Default) {
                builder = builder.default_in_place(|target| unsafe {
                    use $crate::spez::*;
                    (&&SpezEmpty::<$type_name>::SPEZ).spez_default_in_place(target.into()).as_mut()
                });
            }

            if $crate::spez::impls!($type_name: core::clone::Clone) {
                builder = builder.clone_into(|src, dst| unsafe {
                    use $crate::spez::*;
                    (&&Spez(src)).spez_clone_into(dst.into()).as_mut()
                });
            }

            {
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
                builder = builder.marker_traits(traits);
            }

            if $crate::spez::impls!($type_name: core::cmp::PartialEq) {
                builder = builder.eq(|left, right| {
                    use $crate::spez::*;
                    (&&Spez(left))
                        .spez_eq(&&Spez(right))
                });
            }

            if $crate::spez::impls!($type_name: core::cmp::PartialOrd) {
                builder = builder.partial_ord(|left, right| {
                    use $crate::spez::*;
                    (&&Spez(left))
                        .spez_partial_cmp(&&Spez(right))
                });
            }

            if $crate::spez::impls!($type_name: core::cmp::Ord) {
                builder = builder.ord(|left, right| {
                    use $crate::spez::*;
                    (&&Spez(left))
                        .spez_cmp(&&Spez(right))
                });
            }

            if $crate::spez::impls!($type_name: core::hash::Hash) {
                builder = builder.hash(|value, hasher_this, hasher_write_fn| {
                    use $crate::spez::*;
                    use $crate::HasherProxy;
                    (&&Spez(value))
                        .spez_hash(&mut unsafe { HasherProxy::new(hasher_this, hasher_write_fn) })
                });
            }

            if $crate::spez::impls!($type_name: core::str::FromStr) {
                builder = builder.parse(|s, target| {
                    use $crate::spez::*;
                    let res = unsafe { (&&SpezEmpty::<$type_name>::SPEZ).spez_parse(s, target.into()) };
                    res.map(|res| unsafe { res.as_mut() })
                });
                            }

            builder.build()
        }
    };
}
