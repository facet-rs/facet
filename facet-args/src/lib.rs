#![warn(missing_docs)]
#![warn(clippy::std_instead_of_core)]
#![warn(clippy::std_instead_of_alloc)]
#![forbid(unsafe_code)]
#![doc = include_str!("../README.md")]

extern crate alloc;

mod format;

pub(crate) mod arg;
pub(crate) mod error;
pub(crate) mod span;

pub use format::from_slice;
pub use format::from_std_args;

// Args extension attributes for use with #[facet(args::attr)] syntax.
//
// After importing `use facet_args as args;`, users can write:
//   #[facet(args::positional)]
//   #[facet(args::short = 'v')]
//   #[facet(args::named)]

/// Dispatcher macro for args extension attributes.
/// This is called by the derive macro to resolve attribute names.
#[macro_export]
#[doc(hidden)]
macro_rules! __attr {
    (positional { $($tt:tt)* }) => { $crate::__positional!{ $($tt)* } };
    (short { $($tt:tt)* }) => { $crate::__short!{ $($tt)* } };
    (named { $($tt:tt)* }) => { $crate::__named!{ $($tt)* } };

    // Unknown attribute: emit a clear compile error with suggestions
    ($unknown:ident $($tt:tt)*) => {
        ::core::compile_error!(::core::concat!(
            "unknown args attribute `", ::core::stringify!($unknown), "`. ",
            "expected one of: positional, short, named"
        ))
    };
}

/// Marks a field as a positional argument.
#[macro_export]
#[doc(hidden)]
macro_rules! __positional {
    { $field:ident : $ty:ty } => {{
        static __UNIT: () = ();
        ::facet::ExtensionAttr::new("args", "positional", &__UNIT)
    }};
    { $field:ident : $ty:ty | $($args:tt)+ } => {{
        ::core::compile_error!("args::positional does not accept arguments")
    }};
    { } => {{
        static __UNIT: () = ();
        ::facet::ExtensionAttr::new("args", "positional", &__UNIT)
    }};
    { | $($args:tt)+ } => {{
        ::core::compile_error!("args::positional does not accept arguments")
    }};
}

/// Specifies a short flag character for the field.
///
/// Usage: `#[facet(args::short = 'v')]`
#[macro_export]
#[doc(hidden)]
macro_rules! __short {
    // Field with short char literal: #[facet(args::short = 'v')]
    { $field:ident : $ty:ty | = $ch:literal } => {
        $crate::__short_impl!($ch)
    };
    // Field without short character (will use default): #[facet(args::short)]
    { $field:ident : $ty:ty } => {{
        static __VAL: ::core::option::Option<char> = ::core::option::Option::None;
        ::facet::ExtensionAttr::new("args", "short", &__VAL)
    }};
    // Container level (no field)
    { } => {{
        static __VAL: ::core::option::Option<char> = ::core::option::Option::None;
        ::facet::ExtensionAttr::new("args", "short", &__VAL)
    }};
    { | = $ch:literal } => {
        $crate::__short_impl!($ch)
    };
    // Invalid syntax
    { $($tt:tt)* } => {{
        ::core::compile_error!("args::short expects `= 'c'` syntax, e.g., #[facet(args::short = 'v')]")
    }};
}

/// Helper macro that extracts first char and creates the ExtensionAttr.
/// Works with both char literals and string literals.
#[macro_export]
#[doc(hidden)]
macro_rules! __short_impl {
    ($ch:literal) => {{
        // For char literals, this is just the char.
        // For string literals, we take the first byte as ASCII.
        const __CHAR: char = {
            // Try to use it as a char first, then as a string
            let bytes: &[u8] = {
                // This trick: if $ch is a char, .as_bytes() won't exist, but we can
                // convert it to a string first. If it's a string, use it directly.
                // Actually, we can use the stringify! trick to handle both.
                const S: &str = ::core::concat!($ch);
                S.as_bytes()
            };
            match bytes {
                [b, ..] => *b as char,
                [] => panic!("args::short value cannot be empty"),
            }
        };
        static __VAL: ::core::option::Option<char> = ::core::option::Option::Some(__CHAR);
        ::facet::ExtensionAttr::new("args", "short", &__VAL)
    }};
}

/// Marks a field as a named argument.
#[macro_export]
#[doc(hidden)]
macro_rules! __named {
    { $field:ident : $ty:ty } => {{
        static __UNIT: () = ();
        ::facet::ExtensionAttr::new("args", "named", &__UNIT)
    }};
    { $field:ident : $ty:ty | $($args:tt)+ } => {{
        ::core::compile_error!("args::named does not accept arguments")
    }};
    { } => {{
        static __UNIT: () = ();
        ::facet::ExtensionAttr::new("args", "named", &__UNIT)
    }};
    { | $($args:tt)+ } => {{
        ::core::compile_error!("args::named does not accept arguments")
    }};
}
