//! Improve and strengthen your strings
//!
//! Strongly-typed APIs reduce errors and confusion over passing around un-typed strings.
//! Braid helps in that endeavor by making it painless to create wrappers around your
//! string values, ensuring that you use them in the right way every time.
//!
//! Examples of the documentation and implementations provided for braids are available
//! below and in the [`strid_examples`] crate documentation.
//!
//! [`strid_examples`]: https://docs.rs/strid_examples/
//!
//! # Usage
//!
//! A braid is created by attaching `#[braid]` to a struct definition. The macro will take
//! care of automatically updating the representation of the struct to wrap a string and
//! generate the borrowed form of the strong type.
//!
//! ```
//! use strid::braid;
//!
//! #[braid]
//! pub struct DatabaseName;
//! ```
//!
//! Braids of custom string types are also supported, so long as they implement a set of
//! expected traits. If not specified, the type named `String` in the current namespace
//! will be used. See the section on [custom string types] for more information.
//!
//! [custom string types]: #custom-string-types
//!
//! ```ignore
//! use strid::braid;
//! use compact_str::CompactString as String;
//!
//! #[braid]
//! pub struct UserId;
//! ```
//!
//! Once created, braids can be passed around as strongly-typed, immutable strings.
//!
//! ```
//! # use strid::braid;
//! #
//! fn take_strong_string(n: DatabaseName) {}
//! fn borrow_strong_string(n: &DatabaseNameRef) {}
//!
//! # #[braid]
//! # pub struct DatabaseName;
//! #
//! let owned = DatabaseName::new(String::from("mongo"));
//! borrow_strong_string(&owned);
//! take_strong_string(owned);
//! ```
//!
//! A braid can also be untyped for use in stringly-typed interfaces.
//!
//! ```
//! # use strid::braid;
//! #
//! fn take_raw_string(s: String) {}
//! fn borrow_raw_str(s: &str) {}
//!
//! # #[braid]
//! # pub struct DatabaseName;
//! #
//! let owned = DatabaseName::new(String::from("mongo"));
//! borrow_raw_str(owned.as_str());
//! take_raw_string(owned.take());
//! ```
//!
//! By default, the name of the borrowed form will be the same as the owned form
//! with `Ref` appended to the end.
//!
//! ```
//! # use strid::braid;
//! #
//! #[braid]
//! pub struct DatabaseName;
//!
//! let owned = DatabaseName::from_static("mongo");
//! let borrowed = DatabaseNameRef::from_static("mongo");
//! # assert_eq!(owned, borrowed);
//! ```
//!
//! If the name ends with `Buf`, however, then the borrowed form will drop the `Buf`, similar
//! to the relationship between
//! [`PathBuf`][std::path::PathBuf] and [`Path`][std::path::Path].
//!
//! [std::path::PathBuf]: https://doc.rust-lang.org/std/path/struct.PathBuf.html
//! [std::path::Path]: https://doc.rust-lang.org/std/path/struct.Path.html
//!
//! ```
//! # use strid::braid;
//! #
//! #[braid]
//! pub struct DatabaseNameBuf;
//!
//! let owned = DatabaseNameBuf::from_static("mongo");
//! let borrowed = DatabaseName::from_static("mongo");
//! # assert_eq!(owned, borrowed);
//! ```
//!
//! If the name ends with `String`, then the borrowed form will use `Str`.
//!
//! ```
//! # use strid::braid;
//! #
//! #[braid]
//! pub struct ConnectionString;
//!
//! let owned = ConnectionString::from_static("mysql://…");
//! let borrowed = ConnectionStr::from_static("mysql://…");
//! # assert_eq!(owned, borrowed);
//! ```
//!
//! If a different name is desired, this behavior can be
//! overridden by specifying the name of the reference type to create using the `ref`
//! parameter.
//!
//! ```
//! # use strid::braid;
//! #
//! #[braid(ref_name = "TempDb")]
//! pub struct DatabaseNameBuf;
//!
//! let owned = DatabaseNameBuf::from_static("mongo");
//! let borrowed = TempDb::from_static("mongo");
//! let to_owned: DatabaseNameBuf = borrowed.to_owned();
//! # assert_eq!(owned, borrowed);
//! ```
//!
//! A default doc comment is added to the borrowed form that refers back to the owned form.
//! If a custom doc comment is desired, the `ref_doc` parameter allows supplying custom
//! documentation.
//!
//! ```
//! # use strid::braid;
//! #
//! #[braid(ref_doc = "A temporary reference to a database name")]
//! pub struct DatabaseName;
//! #
//! # let owned = DatabaseName::from_static("mongo");
//! # let borrowed = DatabaseNameRef::from_static("mongo");
//! # assert_eq!(owned, borrowed);
//! ```
//!
//! Attributes added to the braid will be applied to both the owned and borrowed forms
//! with the exception of `///` and `#[doc = ""]` attributes. To add an attribute to
//! only the owned form, use the `owned_attr` parameter. Similarly, use `ref_attr` to
//! add an attribute to only the borrowed form.
//!
//! ```
//! use strid::braid;
//!
//! #[braid(
//!    owned_attr(must_use = "database name should always be used"),
//!    ref_attr(must_use = "created a reference, but never used it"),
//! )]
//! #[cfg(not(feature = "nightly"))]
//! pub struct DatabaseName;
//! ```
//!
//! # Extensibility
//!
//! The types created by the `braid` macro are placed in the same module where declared.
//! This means additional functionality, including mutations, can be implemented easily.
//!
//! As a basic example, here is a type built to hold Amazon ARNs. The type has been
//! extended to support some mutation and introspection.
//!
//! ```
//! # use strid::braid;
//! #
//! #[braid]
//! pub struct AmazonArnBuf;
//!
//! impl AmazonArnBuf {
//!     /// Append an ARN segment
//!     pub fn add_segment(&mut self, segment: &str) {
//!         self.0.push_str(":");
//!         self.0.push_str(segment);
//!     }
//! }
//!
//! impl AmazonArn {
//!     /// Returns an iterator of all ARN segments
//!     pub fn get_segments(&self) -> std::str::Split<char> {
//!         self.0.split(':')
//!     }
//!
//!     /// Returns the service segment of the ARN
//!     pub fn get_service(&self) -> &str {
//!         self.get_segments().nth(2).unwrap_or("")
//!     }
//! }
//! ```
//!
//! # Encapsulation
//!
//! Because code within the same module where the braid is defined are allowed to
//! access the internal value, you can use a module in order to more strictly
//! enforce encapsulation and limit accessibility that might otherwise violate
//! established invariants. This may be particularly desired when the wrapped type
//! requires [validation](#validation).
//!
//! ```
//! mod amazon_arn {
//!     #[strid::braid]
//!     pub struct AmazonArnBuf;
//!
//!     /* Additional impls that need access to the inner values */
//! #     impl AmazonArn {
//! #         pub fn get_segments(&self) -> std::str::Split<char> {
//! #             self.0.split(':')
//! #         }
//! #
//! #         pub fn get_service(&self) -> &str {
//! #             self.get_segments().nth(2).unwrap_or("")
//! #         }
//! #     }
//! }
//!
//! pub use amazon_arn::{AmazonArnBuf, AmazonArn};
//!
//! # fn main() {
//! let x = AmazonArnBuf::from_static("arn:aws:iam::123456789012:user/Development");
//! assert_eq!("iam", x.get_service());
//! # }
//! ```
//!
//! # Soundness
//!
//! This crate ensures that the `from_str` implementation provided for wrapping
//! borrowed `str` slices does not extend lifetimes.
//!
//! In the example below, we verify that the borrowed `DatabaseNameRef` is unable
//! to escape the lifetime of `data`. The following code snippet will fail to
//! compile, because `data` will go out of scope and be dropped at the end of
//! the block creating `ex_ref`.
//!
//! ```compile_fail
//! # use strid::braid;
//! #
//! # #[braid]
//! # pub struct DatabaseName;
//! #
//! let ex_ref = {
//!     let data = DatabaseName::new("test string");
//!     DatabaseNameRef::from_str(data.as_str())
//! }; // `data` is dropped at this point
//!
//! // Which means that `ex_ref` would be invalid if allowed.
//! println!("{}", ex_ref);
//! ```
//!
//! # Validation
//!
//! Types can be configured to only contain certain values. This can be used to strongly
//! enforce domain type boundaries, thus making invalid values unrepresentable.
//!
//! For example, if you wanted to have a username type that did not accept the `root` user,
//! you have a few options:
//!
//! 1. Pass the username around as a string, validate that it isn't `root` at known entry points.
//! 2. Create a username type and allow creation from a raw string, then validate it just after
//!    creation.
//! 3. Create a strong username type that requires the value to be validated prior to being
//!    creatable.
//!
//! Braided strings give the strongest, third guarantee. The other two methods require constant
//! vigilance to ensure that an unexpected `root` value doesn't sneak in through other backdoors.
//!
//! By default, Rust's module system allows items within the same module to have access to
//! each other's non-public members. If not handled properly, this can lead to unintentionally
//! violating invariants. Thus, for the strongest guarantees, it is recommended to use the module
//! system to further control access to the interior values held by the braided type as
//! described in the section on [encapsulation](#encapsulation).
//!
//! As a convenience, `from_static` functions are provided that accept `&'static str`. For fallible
//! braids and the owned form of normalized braids, this function will panic if the value is not
//! valid. For borrowed form of normalized braids, the function will panic if the value is not
//! normalized.
//!
//! ```
//! # use strid::braid;
//! #
//! #[derive(Debug, PartialEq, Eq)]
//! pub struct InvalidUsername;
//! // Error implementation elided
//! # impl std::fmt::Display for InvalidUsername {
//! #     fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
//! #         f.write_str("invalid username")
//! #     }
//! # }
//! # strid::from_infallible!(InvalidUsername);
//! # impl std::error::Error for InvalidUsername {}
//!
//! #[braid(validator)]
//! pub struct NonRootUsername;
//!
//! impl strid::Validator for NonRootUsername {
//!     type Error = InvalidUsername;
//!     fn validate(s: &str) -> Result<(), Self::Error> {
//!         if s.is_empty() || s.eq_ignore_ascii_case("root") {
//!             Err(InvalidUsername)
//!         } else {
//!             Ok(())
//!         }
//!     }
//! }
//!
//! assert!(NonRootUsername::new("".to_string()).is_err());
//! assert!(NonRootUsername::new("root".to_string()).is_err());
//! assert!(NonRootUsername::new("nobody".to_string()).is_ok());
//!
//! NonRootUsername::from_static("nobody");
//!
//! assert!(NonRootUsernameRef::from_str("").is_err());
//! assert!(NonRootUsernameRef::from_str("root").is_err());
//! assert!(NonRootUsernameRef::from_str("nobody").is_ok());
//!
//! NonRootUsernameRef::from_static("nobody");
//! ```
//!
//! Foreign validators can also be used by specifying the name of the type that
//! implements the validation logic.
//!
//! ```
//! # use strid::braid;
//! #
//! # #[derive(Debug, PartialEq, Eq)]
//! # pub struct InvalidUsername;
//! # // Error implementation elided
//! # impl std::fmt::Display for InvalidUsername {
//! #     fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
//! #         f.write_str("invalid username")
//! #     }
//! # }
//! # strid::from_infallible!(InvalidUsername);
//! # impl std::error::Error for InvalidUsername {}
//! #
//! #[braid(validator = "UsernameValidator")]
//! pub struct NonRootUsername;
//!
//! pub struct UsernameValidator;
//!
//! impl strid::Validator for UsernameValidator {
//!     /* … */
//! #     type Error = InvalidUsername;
//! #     fn validate(s: &str) -> Result<(), Self::Error> {
//! #         if s.is_empty() || s.eq_ignore_ascii_case("root") {
//! #             Err(InvalidUsername)
//! #         } else {
//! #             Ok(())
//! #         }
//! #     }
//! }
//!
//! assert!(NonRootUsername::new("".to_string()).is_err());
//! assert!(NonRootUsername::new("root".to_string()).is_err());
//! assert!(NonRootUsername::new("nobody".to_string()).is_ok());
//!
//! NonRootUsername::from_static("nobody");
//!
//! assert!(NonRootUsernameRef::from_str("").is_err());
//! assert!(NonRootUsernameRef::from_str("root").is_err());
//! assert!(NonRootUsernameRef::from_str("nobody").is_ok());
//!
//! NonRootUsernameRef::from_static("nobody");
//! ```
//!
//! Note: `Validator::Error` is expected to implement `From<Infallible>`. If
//! you haven't implemented this trait, you'll receive an error of the
//! following form:
//!
//! ```text
//! the trait bound `MyError: std::convert::From<std::convert::Infallible>` is not satisfied
//! the trait `std::convert::From<std::convert::Infallible>` is not implemented for `MyError`
//! ```
//!
//! In order to assist in implementing this trait trivially, use the [`from_infallible!()`]
//! helper macro:
//!
//! ```
//! pub struct InvalidUsername;
//!
//! strid::from_infallible!(InvalidUsername);
//! ```
//!
//! This expands to the following code:
//!
//! ```
//! struct InvalidUsername;
//! impl From<core::convert::Infallible> for InvalidUsername {
//!     #[inline(always)]
//!     fn from(x: core::convert::Infallible) -> Self {
//!         match x {}
//!     }
//! }
//! ```
//!
//! ## Normalization
//!
//! Braided strings can also have enforced normalization, which is carried out at the creation
//! boundary. In this case, the `.from_str()` function on the borrowed form will return a
//! [`Cow<Borrowed>`][alloc::borrow::Cow], which can be inspected to determine whether
//! normalization and conversion to an owned value was required. In cases where the incoming
//! value is expected to already be normalized, the `.from_normalized_str()` function can
//! be used. This function will return an error if the value required normalization.
//!
//! Note that when implementing [`Validator`] for a braided type, the `validate` method
//! must ensure that the value is already in normalized form and return an error if it is
//! not.
//!
//! When using `serde` to deserialze directly to the borrowed form, care must be taken, as
//! only already normalized values will be able to be deserialized. If normalization is
//! expected, deserialize into the owned form or `Cow<Borrowed>`.
//!
//! Here is a toy example where the value must not be empty and must be composed of ASCII
//! characters, but that is also normalized to use lowercase ASCII letters.
//!
//! ```
//! # use strid::braid;
//! use std::borrow::Cow;
//!
//! #[derive(Debug, PartialEq, Eq)]
//! pub struct InvalidHeaderName;
//! // Error implementation elided
//! # impl std::fmt::Display for InvalidHeaderName {
//! #     fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
//! #         f.write_str("invalid header name")
//! #     }
//! # }
//! # strid::from_infallible!(InvalidHeaderName);
//! # impl std::error::Error for InvalidHeaderName {}
//!
//! #[braid(normalizer)]
//! pub struct HeaderName;
//!
//! impl strid::Validator for HeaderName {
//!     type Error = InvalidHeaderName;
//!     fn validate(s: &str) -> Result<(), Self::Error> {
//!         if s.is_empty() || !s.is_ascii() || s.as_bytes().iter().any(|&b| b'A' <= b && b <= b'Z') {
//!             Err(InvalidHeaderName)
//!         } else {
//!             Ok(())
//!         }
//!     }
//! }
//!
//! impl strid::Normalizer for HeaderName {
//!     fn normalize(s: &str) -> Result<Cow<str>, Self::Error> {
//!         if s.is_empty() || !s.is_ascii() {
//!             Err(InvalidHeaderName)
//!         } else if s.as_bytes().iter().any(|&b| b'A' <= b && b <= b'Z') {
//!             Ok(Cow::Owned(s.to_ascii_lowercase()))
//!         } else {
//!             Ok(Cow::Borrowed(s))
//!         }
//!     }
//! }
//!
//! assert!(HeaderName::new("".to_string()).is_err());
//! assert_eq!("mixedcase", HeaderName::new("MixedCase".to_string()).unwrap().as_str());
//! assert_eq!("lowercase", HeaderName::new("lowercase".to_string()).unwrap().as_str());
//!
//! assert_eq!("mixedcase", HeaderName::from_static("MixedCase").as_str());
//! assert_eq!("lowercase", HeaderName::from_static("lowercase").as_str());
//!
//! assert!(HeaderNameRef::from_str("").is_err());
//! assert_eq!("mixedcase", HeaderNameRef::from_str("MixedCase").unwrap().as_str());
//! assert_eq!("lowercase", HeaderNameRef::from_str("lowercase").unwrap().as_str());
//!
//! assert!(HeaderNameRef::from_normalized_str("").is_err());
//! assert!(HeaderNameRef::from_normalized_str("MixedCase").is_err());
//! assert_eq!("lowercase", HeaderNameRef::from_normalized_str("lowercase").unwrap().as_str());
//!
//! assert_eq!("lowercase", HeaderNameRef::from_static("lowercase").as_str());
//! ```
//!
//! ## Unchecked creation
//!
//! Where necessary for efficiency, it is possible to bypass the validations on creation through
//! the use of the `.new_unchecked()` or `from_str_unchecked()` functions. These functions are
//! marked as `unsafe`, as they require the caller to assert that they are fulfilling the
//! implicit contract that the value be both valid and in normal form. If either of these
//! constraints are violated, undefined behavior could result when downstream consumers depend
//! on these constraints being upheld.
//!
//! ```compile_fail
//! # use strid::braid;
//! #
//! # #[derive(Debug, PartialEq, Eq)]
//! # pub struct InvalidUsername;
//! # // Error implementation elided
//! # impl std::fmt::Display for InvalidUsername {
//! #     fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
//! #         f.write_str("invalid username")
//! #     }
//! # }
//! # strid::from_infallible!(InvalidUsername);
//! # impl std::error::Error for InvalidUsername {}
//! #
//! # #[braid(validator)]
//! # pub struct NonRootUsername;
//! #
//! # impl strid::Validator for NonRootUsername {
//! #     type Error = InvalidUsername;
//! #     fn validate(s: &str) -> Result<(), Self::Error> {
//! #         if s.is_empty() || s.eq_ignore_ascii_case("root") {
//! #             Err(InvalidUsername)
//! #         } else {
//! #             Ok(())
//! #         }
//! #     }
//! # }
//! #
//! NonRootUsername::new_unchecked("");
//! NonRootUsernameRef::from_str_unchecked("nobody");
//! ```
//!
//! If you find violations of your guarantees, you can look specifically for uses of `unsafe`.
//!
//! ```
//! # use strid::braid;
//! #
//! # #[derive(Debug, PartialEq, Eq)]
//! # pub struct InvalidUsername;
//! # // Error implementation elided
//! # impl std::fmt::Display for InvalidUsername {
//! #     fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
//! #         f.write_str("invalid username")
//! #     }
//! # }
//! # strid::from_infallible!(InvalidUsername);
//! # impl std::error::Error for InvalidUsername {}
//! #
//! # #[braid(validator)]
//! # pub struct NonRootUsername;
//! #
//! # impl strid::Validator for NonRootUsername {
//! #     type Error = InvalidUsername;
//! #     fn validate(s: &str) -> Result<(), Self::Error> {
//! #         if s.is_empty() || s.eq_ignore_ascii_case("root") {
//! #             Err(InvalidUsername)
//! #         } else {
//! #             Ok(())
//! #         }
//! #     }
//! # }
//! #
//! unsafe {
//!     NonRootUsername::new_unchecked(String::from(""));
//!     NonRootUsernameRef::from_str_unchecked("root");
//! }
//! ```
//!
//! # Provided trait impls
//!
//! By default, the following traits will be automatically implemented.
//!
//! For the `Owned` type
//! * [`core::clone::Clone`]
//! * [`core::fmt::Debug`]
//! * [`core::fmt::Display`]
//! * [`core::hash::Hash`]
//! * [`core::cmp::Eq`]
//! * [`core::cmp::Ord`]
//! * [`core::cmp::PartialEq<Owned>`]
//! * [`core::cmp::PartialEq<Borrowed>`]
//! * [`core::cmp::PartialEq<&Borrowed>`]
//! * [`core::cmp::PartialEq<Box<Borrowed>>`]
//! * [`core::cmp::PartialOrd`]
//! * [`core::convert::AsRef<Borrowed>`]
//! * [`core::convert::AsRef<str>`]
//! * [`core::convert::From<&Borrowed>`]
//! * [`core::convert::From<Box<Borrowed>>`]
//! * [`core::convert::From<Cow<Borrowed>>`]
//! * [`core::borrow::Borrow<Borrowed>`]
//! * [`core::str::FromStr`]
//! * [`core::ops::Deref`] where `Target = Borrowed`
//!
//! Additionally, unvalidated owned types implement
//! * [`core::convert::From<String>`]
//! * [`core::convert::From<&str>`]
//!
//! Validated and normalized owned types will instead implement
//! * [`core::convert::TryFrom<String>`]
//! * [`core::convert::TryFrom<&str>`]
//!
//! When normalized, the above conversions will normalize values.
//!
//! For the `Borrowed` type
//! * [`core::fmt::Debug`]
//! * [`core::fmt::Display`]
//! * [`core::hash::Hash`]
//! * [`core::cmp::Eq`]
//! * [`core::cmp::Ord`]
//! * [`core::cmp::PartialEq<Owned>`]
//! * [`core::cmp::PartialEq<Borrowed>`]
//! * [`core::cmp::PartialEq<&Borrowed>`]
//! * [`core::cmp::PartialEq<Box<Borrowed>>`]
//! * [`core::cmp::PartialOrd`]
//! * [`core::convert::From<&Cow<Borrowed>>`]
//! * [`alloc::borrow::ToOwned`] where `Owned = Owned`
//!
//! Additionally, unvalidated borrowed types implement
//! * [`core::convert::From<&str>`]
//!
//! Validated and normalize borrowed types will instead implement
//! * [`core::convert::TryFrom<&str>`]
//!
//! For `Cow<'static, Borrowed>`
//! * [`core::convert::From<Owned>`]
//!
//! For `Cow<Borrowed>`
//! * [`core::convert::From<&Borrowed>`]
//!
//! For `Box<Borrowed>`
//! * [`core::convert::From<Owned>`]
//!
//! The above conversion will fail if the value is not already normalized.
//!
//! Types that are not normalized will additionally implement
//! * [`core::borrow::Borrow<str>`]
//!
//! `Borrow<str>` cannot be implemented for normalized braids because equality and hashing
//! of equivalent braid values will have differing results for equality, which violates the
//! contract implied by the `Borrow` trait.
//!
//! `Deref` to a `str` is explicitly not implemented. This means that an explicit call is
//! required to treat a value as an untyped string, whether `.as_str()`, `.to_string()`, or
//! `.into_string()`
//!
//! ## Omitting `Clone`
//!
//! For some types, it may be desirable to prevent arbitrary cloning of a type. In that case,
//! the `clone` parameter can be used to prevent automatically deriving
//! [`Clone`][core::clone::Clone].
//!
//! ```
//! # use strid::braid;
//! # use static_assertions::assert_not_impl_any;
//! #
//! #[braid(clone = "omit")]
//! pub struct Sensitive;
//!
//! assert_not_impl_any!(Sensitive: Clone);
//! ```
//!
//! ## Custom `Display`, `Debug`, and `PartialOrd`/`Ord` implementations
//!
//! By default, the implementations of [`Display`][core::fmt::Display], [`Debug`][core::fmt::Debug]
//! [`PartialOrd`][core::cmp::PartialOrd], and [`Ord`][core::cmp::Ord]
//! provided by a braid delegate directly to the underlying [`String`][alloc::string::String]
//! or [`str`] types. If a custom implementation is desired, the automatic derivation of these
//! traits can be controlled by the `display`, `debug`, and `ord` parameters. Both of these
//! parameters accept one of `impl`, `owned`, or `omit`. By default, the `impl` derivation
//! mode is used.
//!
//! The modes have the following effects:
//!
//! * `impl`: Format the owned and reference type transparently as the underlying string (slice)
//!   type.
//! * `owned`: Automatically provide an owned implementation that transparently delegates to the
//!   implementation of the borrowed form. The consumer must provide their custom implementation on
//!   the borrowed form.
//! * `omit`: No implementations are provided for the owned or borrowed forms. These must be
//!   implemented by the consumer if they are desired.
//!
//! Note: Omitting a `PartialOrd` and `Ord` implementation will make the braid unable to be
//! used as a key in a `BTreeMap` or `BTreeSet`.
//!
//! As an example:
//!
//! ```
//! # use strid::braid;
//! use std::fmt;
//! #
//! #[braid(clone = "omit", display = "owned", debug = "owned")]
//! pub struct Sensitive;
//!
//! impl fmt::Debug for SensitiveRef {
//!     fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
//!          f.write_str("SENSITIVE")
//!     }
//! }
//!
//! impl fmt::Display for SensitiveRef {
//!     fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
//!          f.write_str("SENSITIVE DISPLAY")
//!     }
//! }
//!
//! let owned = Sensitive::from_static("secret value");
//! assert_eq!("SENSITIVE", format!("{:?}", owned));
//! assert_eq!("SENSITIVE DISPLAY", format!("{}", owned));
//! assert_eq!("secret value", owned.as_str());
//!
//! let borrowed: &SensitiveRef = &owned;
//! assert_eq!("SENSITIVE", format!("{:?}", borrowed));
//! assert_eq!("SENSITIVE DISPLAY", format!("{}", borrowed));
//! assert_eq!("secret value", borrowed.as_str());
//! ```
//!
//! # Serde
//!
//! [`Serialize`] and [`Deserialize`] implementations from the [`serde`] crate
//! can be automatically generated by including `serde` in the argument list for the macro.
//!
//! [`serde`]: https://docs.rs/serde/*/serde/
//! [`Serialize`]: https://docs.rs/serde/*/serde/trait.Serialize.html
//! [`Deserialize`]: https://docs.rs/serde/*/serde/trait.Deserialize.html
//!
//! ```
//! # use strid::braid;
//! #
//! #[braid(serde)]
//! pub struct Username;
//!
//! let username = Username::from_static("root");
//! let json = serde_json::to_string(&username).unwrap();
//! let new_username: Username = serde_json::from_str(&json).unwrap();
//! # assert_eq!(username, new_username);
//! ```
//!
//! Such automatic implementations will also properly handle string values that require
//! validation. This automatic validation has the benefit of easing use with _Serde_ while
//! still protecting the integrity of the type.
//!
//! ```
//! # use strid::braid;
//! #
//! #[derive(Debug, PartialEq, Eq)]
//! pub struct InvalidUsername;
//! // Error implementation elided
//! # impl std::fmt::Display for InvalidUsername {
//! #     fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
//! #         f.write_str("invalid username")
//! #     }
//! # }
//! # strid::from_infallible!(InvalidUsername);
//! # impl std::error::Error for InvalidUsername {}
//!
//! #[braid(serde, validator)]
//! pub struct Username;
//!
//! impl strid::Validator for Username {
//!     type Error = InvalidUsername;
//!     fn validate(s: &str) -> Result<(), Self::Error> {
//!         if s.is_empty() || s.eq_ignore_ascii_case("root") {
//!             Err(InvalidUsername)
//!         } else {
//!             Ok(())
//!         }
//!     }
//! }
//!
//! assert!(serde_json::from_str::<Username>("\"\"").is_err());
//! assert!(serde_json::from_str::<Username>("\"root\"").is_err());
//! assert!(serde_json::from_str::<Username>("\"nobody\"").is_ok());
//!
//! assert!(serde_json::from_str::<&UsernameRef>("\"\"").is_err());
//! assert!(serde_json::from_str::<&UsernameRef>("\"root\"").is_err());
//! assert!(serde_json::from_str::<&UsernameRef>("\"nobody\"").is_ok());
//! ```
//!
//! # Custom string types
//!
//! The `braid` macro can be used to define a custom string type that wraps types
//! other than the standard `String`. This allows defining a braid that is backed
//! by a type that offers small-string optimizations, such as [`SmartString`] or
//! [`CompactString`].
//!
//! Functions that expose the inner wrapped type can be made private by adding the
//! `no_expose` parameter to avoid leaking the type in the public interface.
//!
//! [`SmartString`]: https://docs.rs/smartstring/*/smartstring/struct.SmartString.html
//! [`CompactString`]: https://docs.rs/compact_str/*/compact_str/struct.CompactString.html
//!
//! ```ignore
//! # use strid::braid;
//! use compact_str::CompactString;
//! use smartstring::{SmartString, LazyCompact};
//!
//! #[braid(no_expose)]
//! pub struct UserId(CompactString);
//!
//! #[braid(no_expose)]
//! pub struct AltUserId(SmartString<LazyCompact>);
//! ```
//!
//! It can also be used to wrap a [`ByteString`], which is a string backed by
//! [`Bytes`], which may be useful if the type is primarily used in contexts
//! where a zero-copy implementation is preferred.
//!
//! [`ByteString`]: https://docs.rs/bytestring/*/bytestring/struct.ByteString.html
//! [`Bytes`]: https://docs.rs/bytes/*/bytes/struct.Bytes.html
//!
//! ```ignore
//! # use strid::braid;
//! use bytestring::ByteString;
//!
//! #[braid]
//! pub struct ZeroCopyIdentifier(ByteString);
//! ```
//!
//! ## Requirements
//!
//! In order to be used as a custom string type, the type must implement the
//! following traits:
//!
//! * [`core::clone::Clone`] (unless `clone` is `omit`)
//! * [`core::fmt::Debug`] (unless `debug` is `omit`)
//! * [`core::fmt::Display`] (unless `display` is `omit`)
//! * [`core::cmp::Eq`]
//! * [`core::cmp::PartialEq`]
//! * [`core::hash::Hash`]
//! * [`core::cmp::Ord`] (unless `ord` is `omit`)
//! * [`core::cmp::PartialOrd`] (unless `ord` is `omit`)
//! * [`serde::Serialize`] (unless `serde` is `omit`)
//! * [`serde::Deserialize`] (unless `serde` is `omit`)
//! * [`core::convert::From<&str>`]
//! * [`core::convert::From<Box<str>>`]
//! * [`core::convert::AsRef<str>`]
//! * [`core::convert::Into<String>`]
//!
//! [`serde::Serialize`]: https://docs.rs/serde/*/serde/trait.Serialize.html
//! [`serde::Deserialize`]: https://docs.rs/serde/*/serde/trait.Deserialize.html
//!
//! # `no_std` support
//!
//! Braids can be implemented in `no_std` environments with `alloc`. By adding the
//! `no_std` parameter to the macro, all impls will reference the `core` or `alloc`
//! crates instead of the `std` crate, as appropriate.
//!
//! ```
//! extern crate alloc;
//!
//! use strid::braid;
//! use alloc::string::String;
//!
//! #[braid(no_std)]
//! pub struct NoStdLibWrapper;
//! #
//! # fn main() {}
//! ```
//!
//! In environments without an allocator, `braid_ref` can be used to create a
//! reference-only braid. In order to remove the `alloc` dependency in `strid`,
//! specify `default-features = "false"` in the `Cargo.toml` file.
//!
//! ```
//! use strid::braid_ref;
//!
//! #[braid_ref(no_std)]
//! pub struct NoStdValue;
//! #
//! # fn main() {}
//! ```
//!
//! # Safety
//!
//! Braid uses limited `unsafe` in order to be able to reinterpret string slices
//! (`&str`) as the borrowed form. Because this functionality is provided as a
//! macro, using the `#![forbid(unsafe_code)]` lint level on a crate that generates
//! braids will result in compiler errors. Instead, the crate can be annotated with
//! `#![deny(unsafe_code)]`, which allows for overrides as appropriate. The functions
//! that require `unsafe` to work correctly are annotated with `#[allow(unsafe_code)]`,
//! and all usages of unsafe that the macro generates are annotated with `SAFETY`
//! code comments.
//!
//! If strict adherence to forbid unsafe code is required, then the types can be
//! segregated into an accessory crate without the prohibition, and then consumed
//! safely from crates that otherwise forbid unsafe code.

#![warn(
    missing_docs,
    unused_import_braces,
    unused_imports,
    unused_qualifications
)]
#![deny(
    missing_debug_implementations,
    trivial_casts,
    trivial_numeric_casts,
    unused_must_use
)]
#![deny(unsafe_code)]
#![no_std]

#[cfg(feature = "alloc")]
extern crate alloc;

/// A validator that can verify a given input is valid given certain preconditions
///
/// If the type can be normalized, this implementation should also validate that
/// the value is _already in normalized form_.
pub trait Validator {
    /// The error produced when the string is invalid
    ///
    /// `TryFrom<String>::Error` for the wrapped type must be convertable into this
    /// error type. In most cases, this conversion is infallible, and so the error
    /// type needs to implement `From<Infallible>`. See the [`from_infallible!()`]
    /// helper macro to quickly implement this for your error type.
    type Error;

    /// Validates a string according to a predetermined set of rules
    ///
    /// # Errors
    ///
    /// Returns an error if the string is invalid or not in normalized form.
    fn validate(raw: &str) -> Result<(), Self::Error>;
}

/// A normalizer that can verify a given input is valid
/// and performs necessary normalization
#[cfg(feature = "alloc")]
pub trait Normalizer: Validator {
    /// Validates and normalizes the borrowed input
    ///
    /// # Errors
    ///
    /// Returns an error if the string is invalid and cannot be normalized.
    fn normalize(raw: &str) -> Result<::alloc::borrow::Cow<'_, str>, Self::Error>;
}

/// Utility macro for easily defining `From<Infallible>` for a given type.
///
/// # Example
///
/// ```
/// use core::convert::Infallible;
/// use strid::from_infallible;
///
/// pub struct MyType;
///
/// from_infallible!(MyType);
///
/// let result: Result<(), Infallible> = Ok(());
/// let my_result: Result<(), MyType> = result.map_err(MyType::from);
/// ```
#[macro_export]
macro_rules! from_infallible {
    ($ty:ty) => {
        impl ::core::convert::From<::core::convert::Infallible> for $ty {
            // This should always be trivially inlineable as it allows
            // the compiler to eliminate code paths from this unreachable path.
            #[inline(always)]
            fn from(x: ::core::convert::Infallible) -> Self {
                match x {}
            }
        }
    };
}

pub use facet;
pub use strid_macros::{braid, braid_ref};
