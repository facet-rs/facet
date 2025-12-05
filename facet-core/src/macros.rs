use crate::TypeNameFn;

/// Creates a `ValueVTable` for a given type.
///
/// This macro generates a `ValueVTable` with implementations for various traits
/// (Display, Debug, PartialEq, PartialOrd, Ord, Hash) if they are implemented for the given type.
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
/// For simple, non-generic types that don't need custom formatting, prefer `type_name_fn::<T>()`
/// which uses `core::any::type_name::<T>()` and avoids emitting another formatting closure.
///
/// This cannot be used for a generic type because the `impls!` thing depends on type bounds.
/// If you have a generic type, you need to do specialization yourself, like we do for slices,
/// arrays, etc. — essentially, this macro is only useful for 1) scalars, 2) inside a derive macro
#[macro_export]
macro_rules! value_vtable {
    ($type_name:ty, $type_name_fn:expr $(,)?) => {
        const {
            $crate::ValueVTable::builder($type_name_fn)
                .drop_in_place($crate::ValueVTable::drop_in_place_for::<$type_name>())
                .display_opt($crate::vtable_fmt!({
                    if $crate::spez::impls!($type_name: core::fmt::Display) {
                        Some(|data, f| {
                            let data = unsafe { data.get::<$type_name>() };
                            use $crate::spez::*;
                            (&&Spez(data)).spez_display(f)
                        })
                    } else {
                        None
                    }
                }))
                .debug_opt($crate::vtable_fmt!({
                    if $crate::spez::impls!($type_name: core::fmt::Debug) {
                        Some(|data, f| {
                            let data = unsafe { data.get::<$type_name>() };
                            use $crate::spez::*;
                            (&&Spez(data)).spez_debug(f)
                        })
                    } else {
                        None
                    }
                }))
                .default_in_place_opt({
                    if $crate::spez::impls!($type_name: core::default::Default) {
                        Some(|target| unsafe {
                            use $crate::spez::*;
                            (&&SpezEmpty::<$type_name>::SPEZ).spez_default_in_place(target)
                        })
                    } else {
                        None
                    }
                })
                .clone_into_opt({
                    if $crate::spez::impls!($type_name: core::clone::Clone) {
                        Some(|src, dst| unsafe {
                            use $crate::spez::*;
                            let src = src.get::<$type_name>();
                            (&&Spez(src)).spez_clone_into(dst)
                        })
                    } else {
                        None
                    }
                })
                .partial_eq_opt($crate::vtable_cmp!({
                    if $crate::spez::impls!($type_name: core::cmp::PartialEq) {
                        Some(|left, right| {
                            let left = unsafe { left.get::<$type_name>() };
                            let right = unsafe { right.get::<$type_name>() };
                            use $crate::spez::*;
                            (&&Spez(left)).spez_partial_eq(&&Spez(right))
                        })
                    } else {
                        None
                    }
                }))
                .partial_ord_opt($crate::vtable_cmp!({
                    if $crate::spez::impls!($type_name: core::cmp::PartialOrd) {
                        Some(|left, right| {
                            let left = unsafe { left.get::<$type_name>() };
                            let right = unsafe { right.get::<$type_name>() };
                            use $crate::spez::*;
                            (&&Spez(left)).spez_partial_cmp(&&Spez(right))
                        })
                    } else {
                        None
                    }
                }))
                .ord_opt($crate::vtable_cmp!({
                    if $crate::spez::impls!($type_name: core::cmp::Ord) {
                        Some(|left, right| {
                            let left = unsafe { left.get::<$type_name>() };
                            let right = unsafe { right.get::<$type_name>() };
                            use $crate::spez::*;
                            (&&Spez(left)).spez_cmp(&&Spez(right))
                        })
                    } else {
                        None
                    }
                }))
                .hash_opt($crate::vtable_hash!({
                    if $crate::spez::impls!($type_name: core::hash::Hash) {
                        Some(|value, hasher| {
                            let value = unsafe { value.get::<$type_name>() };
                            use $crate::spez::*;
                            (&&Spez(value)).spez_hash(&mut { hasher })
                        })
                    } else {
                        None
                    }
                }))
                .parse_opt({
                    if $crate::spez::impls!($type_name: core::str::FromStr) {
                        Some(|s, target| {
                            use $crate::spez::*;
                            unsafe { (&&SpezEmpty::<$type_name>::SPEZ).spez_parse(s, target) }
                        })
                    } else {
                        None
                    }
                })
                .build()
        }
    };
}

/// Default type-name formatter using `core::any::type_name::<T>()`.
///
/// This is useful for non-generic scalars or when generic parameter pretty-printing
/// isn't needed; it avoids emitting a fresh formatting closure per type.
#[inline(always)]
pub const fn type_name_fn<T>() -> TypeNameFn {
    |f, _opts| ::core::fmt::Write::write_str(f, ::core::any::type_name::<T>())
}
