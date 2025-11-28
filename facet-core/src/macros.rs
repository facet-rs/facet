use crate::{Facet, Opaque, Shape};

/// Helper for the derive macro to infer the shape of a struct field.
///
/// This function is never actually called at runtime — it exists purely to let
/// the compiler infer `TField` from a field accessor closure like `|s: &MyStruct| &s.field`.
/// By passing a reference to that closure, the compiler resolves `TField` and we can
/// return `TField::SHAPE` at compile time.
#[doc(hidden)]
pub const fn shape_of<'facet, TStruct, TField: Facet<'facet>>(
    _f: &dyn Fn(&TStruct) -> &TField,
) -> &'static Shape {
    TField::SHAPE
}

/// Helper for the derive macro to infer the shape of an opaque struct field.
///
/// Similar to [`shape_of`], but wraps the field type in [`Opaque`] for types that
/// don't implement `Facet` directly. The closure `|s: &MyStruct| &s.field` lets the
/// compiler infer `TField`, and we return `Opaque::<TField>::SHAPE`.
#[doc(hidden)]
pub const fn shape_of_opaque<'a, TStruct, TField>(
    _f: &dyn Fn(&TStruct) -> &TField,
) -> &'static Shape
where
    Opaque<TField>: Facet<'a>,
{
    Opaque::<TField>::SHAPE
}

/// Helper for the derive macro to infer the source shape for `#[facet(deserialize_with = ...)]`.
///
/// Given a `deserialize_with` function like `fn(&Source) -> Result<Target, E>`, this helper
/// infers `Source` from the function signature and returns `Source::SHAPE`. This shape is
/// stored as a [`FieldAttribute::DeserializeFrom`] so the reflection system knows what
/// intermediate type to construct before calling the conversion function.
#[doc(hidden)]
pub const fn shape_of_deserialize_with_source<'facet, Source: Facet<'facet>, Target>(
    _f: &dyn Fn(&Source) -> Target,
) -> &'static Shape {
    Source::SHAPE
}

/// Helper for the derive macro to infer the target shape for `#[facet(serialize_with = ...)]`.
///
/// Given a `serialize_with` function like `fn(&Source) -> Result<Target, &'static str>`, this
/// helper infers `Target` from the function signature and returns `Target::SHAPE`. This shape
/// is stored as a [`FieldAttribute::SerializeInto`] so the reflection system knows what
/// type to serialize instead of the original field type.
#[doc(hidden)]
pub const fn shape_of_serialize_with_target<'facet, Source, Target: Facet<'facet>>(
    _f: &dyn Fn(&Source) -> Result<Target, &'static str>,
) -> &'static Shape {
    Target::SHAPE
}

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
/// This cannot be used for a generic type because the `impls!` thing depends on type bounds.
/// If you have a generic type, you need to do specialization yourself, like we do for slices,
/// arrays, etc. — essentially, this macro is only useful for 1) scalars, 2) inside a derive macro
#[macro_export]
macro_rules! value_vtable {
    ($type_name:ty, $type_name_fn:expr $(,)?) => {
        const {
            $crate::ValueVTable::builder::<$type_name>()
                .type_name($type_name_fn)
                .display({
                    if $crate::spez::impls!($type_name: core::fmt::Display) {
                        Some(|data: $crate::TypedPtrConst<'_, _>, f| {
                            let data = data.get();
                            use $crate::spez::*;
                            (&&Spez(data)).spez_display(f)
                        })
                    } else {
                        None
                    }
                })
                .debug({
                    if $crate::spez::impls!($type_name: core::fmt::Debug) {
                        Some(|data: $crate::TypedPtrConst<'_, _>, f| {
                            let data = data.get();
                            use $crate::spez::*;
                            (&&Spez(data)).spez_debug(f)
                        })
                    } else {
                        None
                    }
                })
                .default_in_place({
                    if $crate::spez::impls!($type_name: core::default::Default) {
                        Some(|target: $crate::TypedPtrUninit<'_, _>| unsafe {
                            use $crate::spez::*;
                            $crate::TypedPtrMut::new((&&SpezEmpty::<$type_name>::SPEZ).spez_default_in_place(target.into()).as_mut())
                        })
                    } else {
                        None
                    }
                })
                .clone_into({
                    if $crate::spez::impls!($type_name: core::clone::Clone) {
                        Some(|src: $crate::TypedPtrConst<'_, _>, dst: $crate::TypedPtrUninit<'_, _>| unsafe {
                            use $crate::spez::*;
                            let src = src.get();
                            $crate::TypedPtrMut::new((&&Spez(src)).spez_clone_into(dst.into()).as_mut())
                        })
                    } else {
                        None
                    }
                })
                .marker_traits({
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
                    if $crate::spez::impls!($type_name: core::panic::UnwindSafe) {
                        traits = traits.union($crate::MarkerTraits::UNWIND_SAFE);
                    }
                    if $crate::spez::impls!($type_name: core::panic::RefUnwindSafe) {
                        traits = traits.union($crate::MarkerTraits::REF_UNWIND_SAFE);
                    }

                    traits
                })
                .partial_eq({
                    if $crate::spez::impls!($type_name: core::cmp::PartialEq) {
                        Some(|left: $crate::TypedPtrConst<'_, _>, right: $crate::TypedPtrConst<'_, _>| {
                            let left = left.get();
                            let right = right.get();
                            use $crate::spez::*;
                            (&&Spez(left))
                                .spez_partial_eq(&&Spez(right))
                        })
                    } else {
                        None
                    }
                })
                .partial_ord({
                    if $crate::spez::impls!($type_name: core::cmp::PartialOrd) {
                        Some(|left: $crate::TypedPtrConst<'_, _>, right: $crate::TypedPtrConst<'_, _>| {
                            let left = left.get();
                            let right = right.get();
                            use $crate::spez::*;
                            (&&Spez(left))
                                .spez_partial_cmp(&&Spez(right))
                        })
                    } else {
                        None
                    }
                })
                .ord({
                    if $crate::spez::impls!($type_name: core::cmp::Ord) {
                        Some(|left: $crate::TypedPtrConst<'_, _>, right: $crate::TypedPtrConst<'_, _>| {
                            let left = left.get();
                            let right = right.get();
                            use $crate::spez::*;
                            (&&Spez(left))
                                .spez_cmp(&&Spez(right))
                        })
                    } else {
                        None
                    }
                })
                .hash({
                    if $crate::spez::impls!($type_name: core::hash::Hash) {
                        Some(|value: $crate::TypedPtrConst<'_, _>, hasher| {
                            let value = value.get();
                            use $crate::spez::*;
                            (&&Spez(value))
                                .spez_hash(&mut { hasher })
                        })
                    } else {
                        None
                    }
                })
                .parse({
                    if $crate::spez::impls!($type_name: core::str::FromStr) {
                        Some(|s, target: $crate::TypedPtrUninit<'_, _>| {
                            use $crate::spez::*;
                            let res = unsafe { (&&SpezEmpty::<$type_name>::SPEZ).spez_parse(s, target.into()) };
                            res.map(|res| unsafe { $crate::TypedPtrMut::new(res.as_mut()) })
                        })
                    } else {
                        None
                    }
                })
                .build()
        }
    };
}

/// Creates a `ShapeBuilder` for a given type.
#[macro_export]
macro_rules! shape_builder {
    ($type_name:ty $(,)?) => {
        const {
            use $crate::spez::*;
            SpezEmpty::<$type_name>::BUILDER
        }
    };
}

/// Declares a single extension attribute builder for use with `#[facet(namespace::attr)]` syntax.
///
/// **Deprecated**: Use [`define_extension_attrs!`] instead, which provides compile-time
/// validation and better error messages.
#[macro_export]
#[deprecated(since = "0.32.0", note = "use `define_extension_attrs!` instead")]
macro_rules! facet_ext_attr {
    ($attr:ident) => {
        $crate::paste::paste! {
            #[doc(hidden)]
            pub fn [<__facet_build_ $attr>](
                _args: &'static [$crate::Token]
            ) -> &'static (dyn ::core::any::Any + ::core::marker::Send + ::core::marker::Sync) {
                static __UNIT: () = ();
                &__UNIT
            }
        }
    };
}
