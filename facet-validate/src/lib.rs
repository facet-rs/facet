//! Validation attributes for facet.
//!
//! This crate provides validation attributes that can be used with the `#[facet(...)]` syntax.
//! Validators are run during deserialization, providing errors with spans that point to the
//! problematic JSON location.
//!
//! # Example
//!
//! ```ignore
//! use facet::Facet;
//!
//! #[derive(Facet)]
//! pub struct Product {
//!     #[facet(validate::min_length = 1, validate::max_length = 100)]
//!     pub title: String,
//!
//!     #[facet(validate::min = 0)]
//!     pub price: i64,
//!
//!     #[facet(validate::email)]
//!     pub contact_email: String,
//!
//!     #[facet(validate::custom = validate_currency)]
//!     pub currency: String,
//! }
//!
//! fn validate_currency(s: &str) -> Result<(), String> {
//!     match s {
//!         "USD" | "EUR" | "GBP" => Ok(()),
//!         _ => Err(format!("invalid currency code: {}", s)),
//!     }
//! }
//! ```
//!
//! # Built-in Validators
//!
//! | Validator | Syntax | Applies To |
//! |-----------|--------|------------|
//! | `min` | `validate::min = 0` | numbers |
//! | `max` | `validate::max = 100` | numbers |
//! | `min_length` | `validate::min_length = 1` | String, Vec, slices |
//! | `max_length` | `validate::max_length = 100` | String, Vec, slices |
//! | `email` | `validate::email` | String |
//! | `url` | `validate::url` | String |
//! | `regex` | `validate::regex = r"..."` | String |
//! | `contains` | `validate::contains = "foo"` | String |
//! | `custom` | `validate::custom = fn_name` | any |

#![warn(missing_docs)]

use regex::Regex;
use std::sync::LazyLock;

// Re-export the validator function type for use in custom validators
pub use facet_core::ValidatorFn;

/// Validates that a string is a valid email address.
///
/// Uses a simple regex pattern that catches most common cases.
pub fn is_valid_email(s: &str) -> bool {
    static EMAIL_REGEX: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$").unwrap());
    EMAIL_REGEX.is_match(s)
}

/// Validates that a string is a valid URL.
///
/// Uses a simple regex pattern that catches most common cases.
pub fn is_valid_url(s: &str) -> bool {
    static URL_REGEX: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^https?://[^\s/$.?#].[^\s]*$").unwrap());
    URL_REGEX.is_match(s)
}

/// Validates that a string matches a regex pattern.
pub fn matches_pattern(s: &str, pattern: &str) -> bool {
    match Regex::new(pattern) {
        Ok(re) => re.is_match(s),
        Err(_) => false,
    }
}

// Define the validation attribute grammar
facet::define_attr_grammar! {
    ns "validate";
    crate_path ::facet_validate;

    /// Validation attributes for facet fields.
    ///
    /// These attributes can be used with `#[facet(validate::...)]` syntax.
    pub enum Attr {
        /// Minimum numeric value constraint.
        ///
        /// Usage: `#[facet(validate::min = 0)]`
        #[target(field)]
        Min(i64),

        /// Maximum numeric value constraint.
        ///
        /// Usage: `#[facet(validate::max = 100)]`
        #[target(field)]
        Max(i64),

        /// Minimum length constraint for strings and collections.
        ///
        /// Usage: `#[facet(validate::min_length = 1)]`
        #[target(field)]
        MinLength(usize),

        /// Maximum length constraint for strings and collections.
        ///
        /// Usage: `#[facet(validate::max_length = 100)]`
        #[target(field)]
        MaxLength(usize),

        /// Email format validation.
        ///
        /// Usage: `#[facet(validate::email)]`
        #[target(field)]
        Email,

        /// URL format validation.
        ///
        /// Usage: `#[facet(validate::url)]`
        #[target(field)]
        Url,

        /// Regex pattern validation.
        ///
        /// Usage: `#[facet(validate::regex = r"^[A-Z]{2}$")]`
        #[target(field)]
        Regex(&'static str),

        /// String contains validation.
        ///
        /// Usage: `#[facet(validate::contains = "foo")]`
        #[target(field)]
        Contains(&'static str),

        /// Custom validator function.
        ///
        /// The function must have signature `fn(&T) -> Result<(), String>`.
        ///
        /// Usage: `#[facet(validate::custom = my_validator)]`
        #[target(field)]
        Custom(validator ValidatorFn),
    }
}
