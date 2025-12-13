use std::{borrow::Cow, convert::Infallible, error, fmt};

use strid::braid;

#[derive(Debug)]
pub enum InvalidString {
    EmptyString,
    InvalidCharacter,
}

impl fmt::Display for InvalidString {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::EmptyString => f.write_str("string cannot be empty"),
            Self::InvalidCharacter => f.write_str("string contains invalid uppercase character"),
        }
    }
}

impl From<Infallible> for InvalidString {
    #[inline(always)]
    fn from(x: Infallible) -> Self {
        match x {}
    }
}

impl error::Error for InvalidString {}

/// A non-empty [`String`] normalized to lowercase
#[braid(
    serde,
    normalizer,
    ref_name = "LowerStr",
    ref_doc = "A borrowed reference to a non-empty, lowercase string"
)]
pub struct LowerString;

impl strid::Validator for LowerString {
    type Error = InvalidString;

    fn validate(raw: &str) -> Result<(), Self::Error> {
        if raw.is_empty() {
            Err(InvalidString::EmptyString)
        } else if raw.chars().any(|c| c.is_uppercase()) {
            Err(InvalidString::InvalidCharacter)
        } else {
            Ok(())
        }
    }
}

impl strid::Normalizer for LowerString {
    fn normalize(s: &str) -> Result<Cow<'_, str>, Self::Error> {
        if s.is_empty() {
            Err(InvalidString::EmptyString)
        } else if s.contains(|c: char| c.is_uppercase()) {
            Ok(Cow::Owned(s.to_lowercase()))
        } else {
            Ok(Cow::Borrowed(s))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn owned_handles_already_normal() {
        let x = LowerString::from_static("testing");
        assert_eq!(x.as_str(), "testing");
    }

    #[test]
    fn owned_handles_valid_non_normal() {
        let x = LowerString::from_static("TestIng");
        assert_eq!(x.as_str(), "testing");
    }

    #[test]
    fn owned_rejects_invalid() {
        let x = LowerString::new("".to_owned());
        assert!(x.is_err());
    }

    #[test]
    fn ref_handles_already_normal() {
        let x = LowerStr::from_str("testing").unwrap();
        assert!(matches!(x, Cow::Borrowed(_)));
        assert_eq!(x.as_str(), "testing");
    }

    #[test]
    fn from_static_ref_handles_already_normal() {
        let x = LowerStr::from_static("testing");
        assert_eq!(x.as_str(), "testing");
    }

    #[test]
    fn ref_handles_valid_non_normal() {
        let x = LowerStr::from_str("TestIng").unwrap();
        assert!(matches!(x, Cow::Owned(_)));
        assert_eq!(x.as_str(), "testing");
    }

    #[test]
    #[should_panic]
    fn static_ref_handles_panics_on_non_normal() {
        LowerStr::from_static("TestIng");
    }

    fn needs_ref(_: &LowerStr) {}
    fn needs_owned(_: LowerString) {}

    #[test]
    fn ref_as_ref_already_normal() {
        let cow = LowerStr::from_str("testing").unwrap();
        let borrowed = cow.as_ref();
        needs_ref(borrowed);
    }

    #[test]
    fn ref_as_ref_valid_non_normal() {
        let cow = LowerStr::from_str("TestIng").unwrap();
        let borrowed = cow.as_ref();
        needs_ref(borrowed);
    }

    #[test]
    fn ref_to_owned_already_normal() {
        let owned = LowerStr::from_str("testing").unwrap().into_owned();
        needs_owned(owned);
    }

    #[test]
    fn ref_to_owned_valid_non_normal() {
        let owned = LowerStr::from_str("TestIng").unwrap().into_owned();
        needs_owned(owned);
    }

    #[test]
    fn ref_rejects_invalid() {
        let x = LowerStr::from_str("");
        assert!(x.is_err());
    }

    #[test]
    fn ref_norm_handles_already_normal() {
        let x = LowerStr::from_normalized_str("testing").unwrap();
        assert_eq!(x.as_str(), "testing");
    }

    #[test]
    fn ref_norm_rejects_valid_non_normal() {
        let x = LowerStr::from_normalized_str("TestIng");
        assert!(x.is_err());
    }

    #[test]
    fn ref_norm_rejects_invalid() {
        let x = LowerStr::from_normalized_str("");
        assert!(x.is_err());
    }

    #[allow(dead_code)]
    struct Bar<'a> {
        foo: std::borrow::Cow<'a, LowerStr>,
    }

    #[test]
    fn owned_as_cow() {
        let owned = LowerString::new("ORANGE".to_owned()).unwrap();
        let _bar = Bar { foo: owned.into() };
    }

    #[test]
    fn borrowed_as_cow() {
        let borrowed = LowerStr::from_normalized_str("orange").unwrap();
        let _bar = Bar {
            foo: borrowed.into(),
        };
    }

    #[test]
    fn owned_as_ref_borrowed() {
        let owned = LowerString::new("ORANGE".to_owned()).unwrap();
        let _reference: &LowerStr = owned.as_ref();
    }

    #[test]
    fn owned_as_ref_str() {
        let owned = LowerString::new("ORANGE".to_owned()).unwrap();
        let _reference: &str = owned.as_ref();
    }

    #[test]
    fn borrowed_as_ref_str() {
        let owned = LowerStr::from_normalized_str("orange").unwrap();
        let _reference: &str = owned.as_ref();
    }
}
