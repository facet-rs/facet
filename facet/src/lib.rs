#![cfg_attr(not(feature = "std"), no_std)]
#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![doc = include_str!("../README.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(docsrs, feature(builtin_syntax))]
#![cfg_attr(docsrs, feature(prelude_import))]
#![cfg_attr(docsrs, allow(internal_features))]

pub use facet_core::*;

pub use facet_macros::*;

#[cfg(feature = "reflect")]
pub use facet_reflect::*;

pub mod hacking;

/// Built-in facet attributes.
///
/// These attributes are used with the `#[facet(...)]` syntax without a namespace prefix.
/// For example: `#[facet(sensitive)]`, `#[facet(rename = "name")]`, `#[facet(skip)]`.
///
/// Function-based attributes like `default`, `skip_serializing_if`, and `invariants`
/// store type-erased function pointers. The `proxy` attribute stores a Shape reference
/// for custom serialization/deserialization via TryFrom conversions.
pub mod builtin {
    // Re-export function pointer types for grammar variants
    pub use crate::DefaultInPlaceFn;
    pub use crate::InvariantsFn;
    pub use crate::SkipSerializingIfFn;

    // Generate built-in attribute grammar.
    // Uses empty namespace "" for built-in facet attributes.
    // The `builtin;` flag tells the generator this is inside the facet crate itself,
    // so definition-time code uses `crate::` instead of `::facet::`.
    crate::define_attr_grammar! {
        builtin;
        ns "";
        crate_path ::facet::builtin;

        /// Built-in facet attribute types.
        ///
        /// These represent the runtime-queryable built-in attributes.
        /// Attributes with function pointers store the actual function reference.
        pub enum Attr {
            /// Marks a field as containing sensitive data that should be redacted in debug output.
            ///
            /// Usage: `#[facet(sensitive)]`
            Sensitive,

            /// Marks a container as opaque - its inner fields don't need to implement Facet.
            ///
            /// Usage: `#[facet(opaque)]`
            Opaque,

            /// Marks a container as transparent - de/serialization is forwarded to the inner type.
            /// Used for newtype patterns.
            ///
            /// Usage: `#[facet(transparent)]`
            Transparent,

            /// Marks a field to be flattened into its parent structure.
            ///
            /// Usage: `#[facet(flatten)]`
            Flatten,

            /// Marks a field as a child node (for hierarchical formats like KDL/XML).
            ///
            /// Usage: `#[facet(child)]`
            Child,

            /// Denies unknown fields during deserialization.
            ///
            /// Usage: `#[facet(deny_unknown_fields)]`
            DenyUnknownFields,

            /// Uses the default value when the field is missing during deserialization.
            /// Stores a function pointer that produces the default value in-place.
            ///
            /// Usage: `#[facet(default)]` (uses Default trait) or `#[facet(default = expr)]`
            Default(make_t),

            /// Skips both serialization and deserialization of this field.
            ///
            /// Usage: `#[facet(skip)]`
            Skip,

            /// Skips serialization of this field.
            ///
            /// Usage: `#[facet(skip_serializing)]`
            SkipSerializing,

            /// Conditionally skips serialization based on a predicate function.
            /// Stores a type-erased function pointer: `fn(PtrConst) -> bool`.
            ///
            /// Usage: `#[facet(skip_serializing_if = is_empty)]`
            SkipSerializingIf(predicate SkipSerializingIfFn),

            /// Skips deserialization of this field (uses default value).
            ///
            /// Usage: `#[facet(skip_deserializing)]`
            SkipDeserializing,

            /// For enums: variants are serialized without a discriminator tag.
            ///
            /// Usage: `#[facet(untagged)]`
            Untagged,

            /// Renames a field or variant during serialization/deserialization.
            ///
            /// Usage: `#[facet(rename = "new_name")]`
            Rename(&'static str),

            /// Renames all fields/variants using a case conversion rule.
            ///
            /// Usage: `#[facet(rename_all = "camelCase")]`
            ///
            /// Supported rules: camelCase, snake_case, PascalCase, SCREAMING_SNAKE_CASE,
            /// kebab-case, SCREAMING-KEBAB-CASE
            RenameAll(&'static str),

            /// For internally/adjacently tagged enums: the field name for the tag.
            ///
            /// Usage: `#[facet(tag = "type")]`
            Tag(&'static str),

            /// For adjacently tagged enums: the field name for the content.
            ///
            /// Usage: `#[facet(content = "data")]`
            Content(&'static str),

            /// Identifies the type with a tag for self-describing formats.
            ///
            /// Usage: `#[facet(type_tag = "com.example.MyType")]`
            TypeTag(&'static str),

            /// Type invariant validation function.
            /// Stores a type-erased function pointer: `fn(PtrConst) -> bool`.
            ///
            /// Usage: `#[facet(invariants = validate_fn)]`
            Invariants(predicate InvariantsFn),

            /// Proxy type for serialization and deserialization.
            /// The proxy type must implement `TryFrom<ProxyType> for FieldType` (for deserialization)
            /// and `TryFrom<&FieldType> for ProxyType` (for serialization).
            ///
            /// Usage: `#[facet(proxy = MyProxyType)]`
            Proxy(shape_type),
        }
    }

    // Manual Facet impl for Attr since we can't use the derive macro inside the facet crate.
    // This is a simplified opaque implementation.
    unsafe impl crate::Facet<'_> for Attr {
        const SHAPE: &'static crate::Shape = &const {
            crate::Shape::builder_for_sized::<Self>()
                .vtable(crate::value_vtable!(Self, |_f, _o| {
                    core::fmt::Result::Ok(())
                }))
                .type_identifier("facet::builtin::Attr")
                .ty(crate::Type::User(crate::UserType::Opaque))
                .build()
        };
    }
}

pub use static_assertions;

/// Define an attribute grammar with type-safe parsing.
///
/// This macro generates:
/// - The attribute types (enum + structs)
/// - A `__parse_attr!` macro for parsing attribute tokens
/// - Re-exports for the necessary proc-macros
///
/// # Example
///
/// ```ignore
/// facet::define_attr_grammar! {
///     pub enum Attr {
///         /// Skip this field entirely
///         Skip,
///         /// Rename to a different name
///         Rename(&'static str),
///         /// Database column configuration
///         Column(Column),
///     }
///
///     pub struct Column {
///         /// Override the database column name
///         pub name: Option<&'static str>,
///         /// Mark as primary key
///         pub primary_key: bool,
///     }
/// }
/// ```
///
/// This generates an `Attr` enum and `Column` struct with the specified fields,
/// along with a `__parse_attr!` macro that can parse attribute syntax like:
///
/// - `skip` → `Attr::Skip`
/// - `rename("users")` → `Attr::Rename("users")`
/// - `column(name = "user_id", primary_key)` → `Attr::Column(Column { name: Some("user_id"), primary_key: true })`
///
/// # Supported Field Types
///
/// | Grammar Type | Rust Type | Syntax |
/// |--------------|-----------|--------|
/// | `bool` | `bool` | `flag` or `flag = true` |
/// | `&'static str` | `&'static str` | `name = "value"` |
/// | `Option<&'static str>` | `Option<&'static str>` | `name = "value"` (optional) |
/// | `Option<bool>` | `Option<bool>` | `flag = true` (optional) |
#[macro_export]
macro_rules! define_attr_grammar {
    ($($grammar:tt)*) => {
        $crate::__make_parse_attr! { $($grammar)* }
    };
}
