use crate::{Facet, Shape};

#[doc(hidden)]
pub const fn shape_of<'a, TStruct, TField: Facet<'a>>(
    _f: &dyn Fn(&TStruct) -> &TField,
) -> &'static Shape {
    TField::SHAPE
}

#[doc(hidden)]
pub const fn shape_of_opaque<'a, TStruct, TField>(
    _f: &dyn Fn(&TStruct) -> &TField,
) -> &'static Shape
where
    Opaque<TField>: Facet<'a>,
{
    Opaque::<TField>::SHAPE
}

/// Helper type for opaque members
pub struct Opaque<T>(T);

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
        &$crate::value_vtable_inner!($type_name, $type_name_fn)
    };
}

/// Creates a `ValueVTable` for a given type, see [`value_vtable!`] for more details.
#[macro_export]
macro_rules! value_vtable_inner {
    ($type_name:ty, $type_name_fn:expr) => {
        const {
            let mut builder = $crate::ValueVTable::builder()
                .type_name($type_name_fn)
                .drop_in_place(|data| unsafe { data.drop_in_place::<$type_name>() });

            if $crate::spez::impls!($type_name: core::fmt::Display) {
                builder = builder.display(|data, f| {
                    use $crate::spez::*;
                    (&&Spez(unsafe { data.get::<$type_name>() })).spez_display(f)
                });
            }

            if $crate::spez::impls!($type_name: core::fmt::Debug) {
                builder = builder.debug(|data, f| {
                    use $crate::spez::*;
                    (&&Spez(unsafe { data.get::<$type_name>() })).spez_debug(f)
                });
            }

            if $crate::spez::impls!($type_name: core::default::Default) {
                builder = builder.default_in_place(|target| {
                    use $crate::spez::*;
                    unsafe { (&&SpezEmpty::<$type_name>::SPEZ).spez_default_in_place(target) }
                });
            }

            if $crate::spez::impls!($type_name: core::clone::Clone) {
                builder = builder.clone_into(|src, dst| {
                    use $crate::spez::*;
                    unsafe { (&&Spez(src.get::<$type_name>())).spez_clone_into(dst) }
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
                    (&&Spez(unsafe { left.get::<$type_name>() }))
                        .spez_eq(&&Spez(unsafe { right.get::<$type_name>() }))
                });
            }

            if $crate::spez::impls!($type_name: core::cmp::PartialOrd) {
                builder = builder.partial_ord(|left, right| {
                    use $crate::spez::*;
                    (&&Spez(unsafe { left.get::<$type_name>() }))
                        .spez_partial_cmp(&&Spez(unsafe { right.get::<$type_name>() }))
                });
            }

            if $crate::spez::impls!($type_name: core::cmp::Ord) {
                builder = builder.ord(|left, right| {
                    use $crate::spez::*;
                    (&&Spez(unsafe { left.get::<$type_name>() }))
                        .spez_cmp(&&Spez(unsafe { right.get::<$type_name>() }))
                });
            }

            if $crate::spez::impls!($type_name: core::hash::Hash) {
                builder = builder.hash(|value, hasher_this, hasher_write_fn| {
                    use $crate::spez::*;
                    use $crate::HasherProxy;
                    (&&Spez(unsafe { value.get::<$type_name>() }))
                        .spez_hash(&mut unsafe { HasherProxy::new(hasher_this, hasher_write_fn) })
                });
            }

            if $crate::spez::impls!($type_name: core::str::FromStr) {
                builder = builder.parse(|s, target| {
                    use $crate::spez::*;
                    let res = unsafe { (&&SpezEmpty::<$type_name>::SPEZ).spez_parse(s, target) };
                    res.map(|_| unsafe { target.assume_init() })
                });
            }

            builder.build()
        }
    };
}
