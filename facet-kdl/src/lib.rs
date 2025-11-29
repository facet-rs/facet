#![warn(missing_docs)]
#![allow(clippy::result_large_err)]
#![doc = include_str!("../README.md")]

mod deserialize;
mod error;
mod serialize;

// Re-export span types from facet-reflect
pub use facet_reflect::{Span, Spanned};

// Re-export error types
pub use error::{KdlError, KdlErrorKind};

// Re-export deserialization
pub use deserialize::from_str;

// Re-export serialization
pub use serialize::{to_string, to_writer};

// KDL extension attributes for use with #[facet(kdl::attr)] syntax.
//
// After importing `use facet_kdl as kdl;`, users can write:
//   #[facet(kdl::child)]
//   #[facet(kdl::children)]
//   #[facet(kdl::property)]
//   #[facet(kdl::argument)]
//   #[facet(kdl::arguments)]
//   #[facet(kdl::node_name)]

/// Dispatcher macro for KDL extension attributes.
/// This is called by the derive macro to resolve attribute names.
#[macro_export]
#[doc(hidden)]
macro_rules! __attr {
    (child { $($tt:tt)* }) => { $crate::__child!{ $($tt)* } };
    (children { $($tt:tt)* }) => { $crate::__children!{ $($tt)* } };
    (property { $($tt:tt)* }) => { $crate::__property!{ $($tt)* } };
    (argument { $($tt:tt)* }) => { $crate::__argument!{ $($tt)* } };
    (arguments { $($tt:tt)* }) => { $crate::__arguments!{ $($tt)* } };
    (node_name { $($tt:tt)* }) => { $crate::__node_name!{ $($tt)* } };

    // Unknown attribute: use path resolution to get the error span on the unknown identifier.
    // The module name contains the valid options as a hint.
    ($unknown:ident $($tt:tt)*) => {
        $crate::valid_kdl_attrs_are::child_or_children_or_property_or_argument_or_arguments_or_node_name::$unknown
    };
}

/// This module exists only to produce helpful error messages for unknown kdl attributes.
#[doc(hidden)]
pub mod valid_kdl_attrs_are {
    #[doc(hidden)]
    pub mod child_or_children_or_property_or_argument_or_arguments_or_node_name {}
}

/// Marks a field as a KDL child node.
#[macro_export]
#[doc(hidden)]
macro_rules! __child {
    // Field with type, no args
    { $field:ident : $ty:ty } => {{
        static __UNIT: () = ();
        ::facet::ExtensionAttr::new("kdl", "child", &__UNIT)
    }};
    // Field with type and args (not expected, but handle gracefully)
    { $field:ident : $ty:ty | $($args:tt)+ } => {{
        ::core::compile_error!("kdl::child does not accept arguments")
    }};
    // Container level (no field)
    { } => {{
        static __UNIT: () = ();
        ::facet::ExtensionAttr::new("kdl", "child", &__UNIT)
    }};
    // Container level with args
    { | $($args:tt)+ } => {{
        ::core::compile_error!("kdl::child does not accept arguments")
    }};
}

/// Marks a field as collecting multiple KDL children.
#[macro_export]
#[doc(hidden)]
macro_rules! __children {
    { $field:ident : $ty:ty } => {{
        static __UNIT: () = ();
        ::facet::ExtensionAttr::new("kdl", "children", &__UNIT)
    }};
    { $field:ident : $ty:ty | $($args:tt)+ } => {{
        ::core::compile_error!("kdl::children does not accept arguments")
    }};
    { } => {{
        static __UNIT: () = ();
        ::facet::ExtensionAttr::new("kdl", "children", &__UNIT)
    }};
    { | $($args:tt)+ } => {{
        ::core::compile_error!("kdl::children does not accept arguments")
    }};
}

/// Marks a field as a KDL property (key=value).
#[macro_export]
#[doc(hidden)]
macro_rules! __property {
    { $field:ident : $ty:ty } => {{
        static __UNIT: () = ();
        ::facet::ExtensionAttr::new("kdl", "property", &__UNIT)
    }};
    { $field:ident : $ty:ty | $($args:tt)+ } => {{
        ::core::compile_error!("kdl::property does not accept arguments")
    }};
    { } => {{
        static __UNIT: () = ();
        ::facet::ExtensionAttr::new("kdl", "property", &__UNIT)
    }};
    { | $($args:tt)+ } => {{
        ::core::compile_error!("kdl::property does not accept arguments")
    }};
}

/// Marks a field as a KDL positional argument.
#[macro_export]
#[doc(hidden)]
macro_rules! __argument {
    { $field:ident : $ty:ty } => {{
        static __UNIT: () = ();
        ::facet::ExtensionAttr::new("kdl", "argument", &__UNIT)
    }};
    { $field:ident : $ty:ty | $($args:tt)+ } => {{
        ::core::compile_error!("kdl::argument does not accept arguments")
    }};
    { } => {{
        static __UNIT: () = ();
        ::facet::ExtensionAttr::new("kdl", "argument", &__UNIT)
    }};
    { | $($args:tt)+ } => {{
        ::core::compile_error!("kdl::argument does not accept arguments")
    }};
}

/// Marks a field as collecting all KDL positional arguments.
#[macro_export]
#[doc(hidden)]
macro_rules! __arguments {
    { $field:ident : $ty:ty } => {{
        static __UNIT: () = ();
        ::facet::ExtensionAttr::new("kdl", "arguments", &__UNIT)
    }};
    { $field:ident : $ty:ty | $($args:tt)+ } => {{
        ::core::compile_error!("kdl::arguments does not accept arguments")
    }};
    { } => {{
        static __UNIT: () = ();
        ::facet::ExtensionAttr::new("kdl", "arguments", &__UNIT)
    }};
    { | $($args:tt)+ } => {{
        ::core::compile_error!("kdl::arguments does not accept arguments")
    }};
}

/// Marks a field as storing the KDL node name.
#[macro_export]
#[doc(hidden)]
macro_rules! __node_name {
    { $field:ident : $ty:ty } => {{
        static __UNIT: () = ();
        ::facet::ExtensionAttr::new("kdl", "node_name", &__UNIT)
    }};
    { $field:ident : $ty:ty | $($args:tt)+ } => {{
        ::core::compile_error!("kdl::node_name does not accept arguments")
    }};
    { } => {{
        static __UNIT: () = ();
        ::facet::ExtensionAttr::new("kdl", "node_name", &__UNIT)
    }};
    { | $($args:tt)+ } => {{
        ::core::compile_error!("kdl::node_name does not accept arguments")
    }};
}
