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

/// Extension attributes for args namespace.
///
/// Users import `use facet_args as args;` to use `#[facet(args::positional)]` etc.
pub mod attrs {
    use facet_core::{AnyStaticRef, LiteralKind, Token};

    // Marker struct for positional attribute
    #[doc(hidden)]
    #[allow(non_camel_case_types)]
    pub struct positional {
        _private: (),
    }

    #[doc(hidden)]
    pub fn positional(_args: &[Token]) -> AnyStaticRef {
        static __UNIT: () = ();
        &__UNIT
    }

    // Marker struct for short attribute
    #[doc(hidden)]
    #[allow(non_camel_case_types)]
    pub struct short {
        _private: (),
    }

    /// The short attribute function parses `= 'c'` or `= "c"` and returns the character.
    /// Returns `Option<char>` - None if no argument, Some(c) if a character was specified.
    ///
    /// Note: This leaks a small amount of memory per field with a short attribute.
    /// This is acceptable since it's bounded by the number of fields in the program.
    #[doc(hidden)]
    pub fn short(args: &[Token]) -> AnyStaticRef {
        // Parse `= 'c'` or `= "c"` syntax
        let result: Option<char> = parse_short_args(args);
        // Leak the result to get 'static lifetime - this is fine since each field
        // has at most one short attribute and SHAPE is computed once at startup
        Box::leak(Box::new(result))
    }

    fn parse_short_args(args: &[Token]) -> Option<char> {
        let mut iter = args.iter();

        // Skip the '=' if present
        if let Some(Token::Punct { ch: '=', .. }) = iter.next() {
            // Look for a literal
            if let Some(Token::Literal { text, kind, .. }) = iter.next() {
                match kind {
                    LiteralKind::Char => {
                        // 'c' -> c
                        let inner = text.trim_start_matches('\'').trim_end_matches('\'');
                        return inner.chars().next();
                    }
                    LiteralKind::String => {
                        // "c" -> c
                        let inner = text.trim_start_matches('"').trim_end_matches('"');
                        return inner.chars().next();
                    }
                    _ => return None,
                }
            }
        }
        None
    }

    // Marker struct for named attribute
    #[doc(hidden)]
    #[allow(non_camel_case_types)]
    pub struct named {
        _private: (),
    }

    #[doc(hidden)]
    pub fn named(_args: &[Token]) -> AnyStaticRef {
        static __UNIT: () = ();
        &__UNIT
    }

    // Validation machinery
    #[doc(hidden)]
    pub struct ValidAttr<A>(::core::marker::PhantomData<A>);

    #[doc(hidden)]
    #[diagnostic::on_unimplemented(
        message = "`{Self}` is not a recognized args attribute",
        label = "unknown attribute",
        note = "valid attributes are: `positional`, `short`, `named`"
    )]
    pub trait IsValidAttr {}

    #[doc(hidden)]
    pub const fn __check_attr<A>()
    where
        ValidAttr<A>: IsValidAttr,
    {
    }

    #[doc(hidden)]
    impl IsValidAttr for ValidAttr<positional> {}
    #[doc(hidden)]
    impl IsValidAttr for ValidAttr<short> {}
    #[doc(hidden)]
    impl IsValidAttr for ValidAttr<named> {}
}
