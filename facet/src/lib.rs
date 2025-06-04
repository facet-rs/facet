#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, feature(builtin_syntax))]
#![cfg_attr(docsrs, feature(prelude_import))]
#![cfg_attr(docsrs, allow(internal_features))]

#[cfg(docsrs)]
pub mod sample_generated_code;

pub use facet_core::*;

/// Derive the [`Facet`] trait for structs, tuple structs, and enums.
///
/// Using this macro gives the derived type runtime (and to some extent, const-time) knowledge about the type, also known as ["reflection"](https://en.wikipedia.org/wiki/Reflective_programming).
///
/// This uses unsynn, so it's light, but it _will_ choke on some Rust syntax because...
/// there's a lot of Rust syntax.
///
/// ```rust
/// # use facet::Facet;
/// #[derive(Facet)]
/// struct FooBar {
///     foo: u32,
///     bar: String,
/// }
/// ```
///
/// # Container Attributes
///
/// ```rust
/// # use facet::Facet;
/// #[derive(Facet)]
/// #[facet(rename_all = "kebab-case")]
/// struct FooBar {
/// # }
/// ```
///
/// * `rename_all = ".."` Rename all the fields (if this is a struct) or variants (if this is an enum) according to the given case convention. The possible values are: `"snake_case"`, `"SCREAMING_SNAKE_CASE"`, `"PascalCase"`, `"camelCase"`, `"kebab-case"`, `"SCREAMING-KEBAB-CASE"`.
///
/// * `transparent` Serialize and deserialize a newtype struct exactly the same as if its single field were serialized and deserialized by itself.
///
/// * `deny_unknown_fields` Always throw an error when encountering unknown fields during deserialization. When this attribute is not present unknown fields are ignored.
///
/// * `skip_serializing` Don't allow this type to be serialized.
///
/// * `skip_serializing_if = ".."` Don't allow this type to be serialized if the function returns `true`.
///
/// * `invariants = ".."` Called when doing `Wip::build`. **TODO**
///
/// # Field Attributes
///
/// ```rust
/// # use facet::Facet;
/// #[derive(Facet)]
/// struct FooBar {
///     #[facet(default)]
///     foo: u32,
/// # }
/// ```
///
/// * `rename = ".."` Rename to the given case convention. The possible values are: `"snake_case"`, `"SCREAMING_SNAKE_CASE"`, `"PascalCase"`, `"camelCase"`, `"kebab-case"`, `"SCREAMING-KEBAB-CASE"`.
///
/// * `default` Use the field's value from the container's `Default::default()` implementation when the field is missing during deserializing.
///
/// * `default = ".."` Use the expression when the field is missing during deserializing.
///
/// * `sensitive` Don't show the value in debug outputs.
///
/// * `flatten` Flatten the value's content into the container structure.
///
/// * `child` Mark as child node in a hierarchy. **TODO**
///
/// * `skip_serializing` Ignore when serializing.
///
/// * `skip_serializing_if = ".."` Ignore when serializing if the function returns `true`.
///
/// # Variant Attributes
///
/// ```rust
/// # use facet::Facet;
/// #[derive(Facet)]
/// #[repr(C)]
/// enum FooBar {
///     #[facet(rename = "kebab-case")]
///     Foo(u32),
/// # }
/// ```
///
/// * `rename = ".."` Rename to the given case convention. The possible values are: `"snake_case"`, `"SCREAMING_SNAKE_CASE"`, `"PascalCase"`, `"camelCase"`, `"kebab-case"`, `"SCREAMING-KEBAB-CASE"`.
///
/// * `skip_serializing` Ignore when serializing.
///
/// * `skip_serializing_if = ".."` Ignore when serializing if the function returns `true`.
///
/// # Examples
///
/// **TODO**.
pub use facet_macros::*;

#[cfg(feature = "reflect")]
pub use facet_reflect::*;

pub mod hacking;

pub use static_assertions;
