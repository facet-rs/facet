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
    pub use crate::TruthyFn;

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
        ///
        /// Attributes annotated with `#[storage(flag)]` are stored in `FieldFlags` for O(1) access.
        /// Attributes annotated with `#[storage(field)]` are stored in dedicated `Field` struct fields.
        /// Attributes without `#[storage(...)]` are stored in the `attributes` slice (O(n) lookup).
        /// <https://facet.rs/guide/attributes/>
        pub enum Attr {
            /// Marks a field as containing sensitive data that should be redacted in debug output.
            ///
            /// Usage: `#[facet(sensitive)]`
            #[storage(flag)]
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
            #[storage(flag)]
            Flatten,

            /// Marks a field as a child node (for hierarchical formats like KDL/XML).
            ///
            /// Usage: `#[facet(child)]`
            #[storage(flag)]
            Child,

            /// Denies unknown fields during deserialization.
            ///
            /// Usage: `#[facet(deny_unknown_fields)]`
            DenyUnknownFields,

            /// Uses the default value when the field is missing during deserialization.
            /// Stores a function pointer that produces the default value in-place.
            ///
            /// Usage: `#[facet(default)]` (uses Default trait) or `#[facet(default = expr)]`
            ///
            /// When no explicit value is given (`#[facet(default)]`), the Rust field type's
            /// `Default::default()` is used. This requires the field type to implement Default.
            /// For opaque fields, this uses the underlying Rust type's Default, not the
            /// Facet shape's default.
            ///
            /// Note: The HAS_DEFAULT flag is also set when this attribute is present.
            Default(make_t or $ty::default()),

            /// Skips both serialization and deserialization of this field.
            ///
            /// Usage: `#[facet(skip)]`
            #[storage(flag)]
            Skip,

            /// Skips serialization of this field.
            ///
            /// Usage: `#[facet(skip_serializing)]`
            #[storage(flag)]
            SkipSerializing,

            /// Conditionally skips serialization based on a predicate function.
            /// Stores a type-erased function pointer: `fn(PtrConst) -> bool`.
            ///
            /// Usage: `#[facet(skip_serializing_if = is_empty)]`
            SkipSerializingIf(predicate SkipSerializingIfFn),

            /// Skips serialization unless the value is truthy.
            /// Uses the type's registered truthiness predicate when available.
            ///
            /// Usage: `#[facet(skip_unless_truthy)]`
            SkipUnlessTruthy,

            /// Skips deserialization of this field (uses default value).
            ///
            /// Usage: `#[facet(skip_deserializing)]`
            #[storage(flag)]
            SkipDeserializing,

            /// For enums: variants are serialized without a discriminator tag.
            ///
            /// Usage: `#[facet(untagged)]`
            Untagged,

            /// Serializes/Deserializers enum to/from integer based on variant discriminant.
            ///
            /// Usage: `#[facet(is_numeric)]`
            IsNumeric,

            /// Renames a field or variant during serialization/deserialization.
            ///
            /// Usage: `#[facet(rename = "new_name")]`
            #[storage(field)]
            Rename(&'static str),

            /// Renames all fields/variants using a case conversion rule.
            ///
            /// Usage: `#[facet(rename_all = "camelCase")]`
            ///
            /// Supported rules: camelCase, snake_case, PascalCase, SCREAMING_SNAKE_CASE,
            /// kebab-case, SCREAMING-KEBAB-CASE
            RenameAll(&'static str),

            /// Aliases a field or variant during deserialization.
            ///
            /// Usage: `#[facet(alias = "additional_name")]`
            ///
            /// Allows for deserializing a field from either the alias or the original name.
            #[storage(field)]
            Alias(&'static str),

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

            /// Declares the truthiness predicate for this container type.
            /// Stores a type-erased function pointer: `fn(PtrConst) -> bool`.
            ///
            /// Usage: `#[facet(truthy = Self::is_truthy)]`
            Truthy(predicate TruthyFn),

            /// Applies `skip_unless_truthy` to every field in the container.
            ///
            /// Usage: `#[facet(skip_all_unless_truthy)]`
            SkipAllUnlessTruthy,

            /// Proxy type for serialization and deserialization.
            /// The proxy type must implement `TryFrom<ProxyType> for FieldType` (for deserialization)
            /// and `TryFrom<&FieldType> for ProxyType` (for serialization).
            ///
            /// Usage: `#[facet(proxy = MyProxyType)]`
            Proxy(shape_type),

            /// Marks a field as having a recursive type that needs lazy shape resolution.
            ///
            /// Use this on fields where the type recursively contains the parent type,
            /// such as `Vec<Self>`, `Box<Self>`, `Option<Arc<Self>>`, etc.
            ///
            /// Without this attribute, such recursive types would cause a compile-time cycle.
            /// With this attribute, the field's shape is resolved lazily via a closure.
            ///
            /// Usage: `#[facet(recursive_type)]`
            ///
            /// # Example
            ///
            /// ```ignore
            /// #[derive(Facet)]
            /// struct Node {
            ///     value: i32,
            ///     #[facet(recursive_type)]
            ///     children: Vec<Node>,
            /// }
            /// ```
            RecursiveType,

            // Note: `traits(...)` and `auto_traits` are compile-time-only directives
            // processed by the derive macro. They are not stored as runtime attributes.
            // See DeclaredTraits in facet-macros-impl/src/parsed.rs for their handling.
        }
    }

    // Manual Facet impl for Attr since we can't use the derive macro inside the facet crate.
    // This is a simplified opaque implementation.
    unsafe impl crate::Facet<'_> for Attr {
        const SHAPE: &'static crate::Shape = &crate::Shape {
            id: crate::Shape::id_of::<Self>(),
            layout: crate::Shape::layout_of::<Self>(),
            vtable: crate::VTableErased::Direct(&crate::VTableDirect::empty()),
            type_ops: None,
            marker_traits: crate::MarkerTraits::empty(),
            type_identifier: "facet::builtin::Attr",
            ty: crate::Type::User(crate::UserType::Opaque),
            def: crate::Def::Undefined,
            type_params: &[],
            doc: &[],
            attributes: &[],
            type_tag: None,
            inner: None,
            builder_shape: None,
            type_name: None,
            proxy: None,
            variance: crate::Variance::COVARIANT,
            flags: crate::ShapeFlags::empty(),
            tag: None,
            content: None,
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
