use std::{convert::Infallible, error, fmt};

use strid::braid;

#[derive(Debug)]
pub enum InvalidScopeToken {
    EmptyString,
    InvalidCharacter { position: usize, value: u8 },
}

impl fmt::Display for InvalidScopeToken {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Self::EmptyString => f.write_str("scope cannot be empty"),
            Self::InvalidCharacter { position, value } => f.write_fmt(format_args!(
                "invalid scope character at position {}: {:02x}",
                position, value
            )),
        }
    }
}

impl From<Infallible> for InvalidScopeToken {
    #[inline(always)]
    fn from(x: Infallible) -> Self {
        match x {}
    }
}

impl error::Error for InvalidScopeToken {}

/// A scope token as defined in RFC6749, Section 3.3
#[braid(serde, validator, ref_doc = "A borrowed reference to a [`ScopeToken`]")]
pub struct ScopeToken;

impl strid::Validator for ScopeToken {
    type Error = InvalidScopeToken;

    fn validate(s: &str) -> Result<(), Self::Error> {
        if s.is_empty() {
            Err(InvalidScopeToken::EmptyString)
        } else if let Some((position, &value)) = s
            .as_bytes()
            .iter()
            .enumerate()
            .find(|&(_, &b)| b <= 0x20 || b == 0x22 || b == 0x5C || 0x7F <= b)
        {
            Err(InvalidScopeToken::InvalidCharacter { position, value })
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use std::borrow::Borrow;

    use super::*;

    #[test]
    fn owned_handles_valid() {
        let x = ScopeToken::from_static("https://crates.io/scopes/publish:crate");
        assert_eq!(x.as_str(), "https://crates.io/scopes/publish:crate");
    }

    #[test]
    fn owned_rejects_empty() {
        let x = ScopeToken::new("".to_owned());
        assert!(matches!(x, Err(InvalidScopeToken::EmptyString)));
    }

    #[test]
    fn owned_rejects_invalid_quote() {
        let x = ScopeToken::new("https://crates.io/scopes/\"publish:crate\"".to_owned());
        assert!(matches!(x, Err(InvalidScopeToken::InvalidCharacter { .. })));
    }

    #[test]
    fn owned_rejects_invalid_control() {
        let x = ScopeToken::new("https://crates.io/scopes/\tpublish:crate".to_owned());
        assert!(matches!(x, Err(InvalidScopeToken::InvalidCharacter { .. })));
    }

    #[test]
    fn owned_rejects_invalid_backslash() {
        let x = ScopeToken::new("https://crates.io/scopes/\\publish:crate".to_owned());
        assert!(matches!(x, Err(InvalidScopeToken::InvalidCharacter { .. })));
    }

    #[test]
    fn owned_rejects_invalid_delete() {
        let x = ScopeToken::new("https://crates.io/scopes/\x7Fpublish:crate".to_owned());
        assert!(matches!(x, Err(InvalidScopeToken::InvalidCharacter { .. })));
    }

    #[test]
    fn owned_rejects_invalid_non_ascii() {
        let x = ScopeToken::new("https://crates.io/scopes/¿publish:crate".to_owned());
        assert!(matches!(x, Err(InvalidScopeToken::InvalidCharacter { .. })));
    }

    #[test]
    fn owned_rejects_invalid_emoji() {
        let x = ScopeToken::new("https://crates.io/scopes/🪤publish:crate".to_owned());
        assert!(matches!(x, Err(InvalidScopeToken::InvalidCharacter { .. })));
    }

    #[test]
    fn ref_handles_valid() {
        let x = ScopeTokenRef::from_static("https://crates.io/scopes/publish:crate");
        assert_eq!(x.as_str(), "https://crates.io/scopes/publish:crate");
    }

    #[test]
    fn ref_rejects_empty() {
        let x = ScopeTokenRef::from_str("");
        assert!(matches!(x, Err(InvalidScopeToken::EmptyString)));
    }

    #[test]
    #[should_panic]
    fn from_static_ref_panics_on_empty() {
        ScopeTokenRef::from_static("");
    }

    #[test]
    #[should_panic]
    fn from_static_owned_panics_on_empty() {
        ScopeToken::from_static("");
    }

    #[test]
    fn ref_rejects_invalid_quote() {
        let x = ScopeTokenRef::from_str("https://crates.io/scopes/\"publish:crate\"");
        assert!(matches!(x, Err(InvalidScopeToken::InvalidCharacter { .. })));
    }

    #[test]
    fn ref_rejects_invalid_control() {
        let x = ScopeTokenRef::from_str("https://crates.io/scopes/\tpublish:crate");
        assert!(matches!(x, Err(InvalidScopeToken::InvalidCharacter { .. })));
    }

    #[test]
    fn ref_rejects_invalid_backslash() {
        let x = ScopeTokenRef::from_str("https://crates.io/scopes/\\publish:crate");
        assert!(matches!(x, Err(InvalidScopeToken::InvalidCharacter { .. })));
    }

    #[test]
    fn ref_rejects_invalid_delete() {
        let x = ScopeTokenRef::from_str("https://crates.io/scopes/\x7Fpublish:crate");
        assert!(matches!(x, Err(InvalidScopeToken::InvalidCharacter { .. })));
    }

    #[test]
    fn ref_rejects_invalid_non_ascii() {
        let x = ScopeTokenRef::from_str("https://crates.io/scopes/¿publish:crate");
        assert!(matches!(x, Err(InvalidScopeToken::InvalidCharacter { .. })));
    }

    #[test]
    fn ref_rejects_invalid_emoji() {
        let x = ScopeTokenRef::from_str("https://crates.io/scopes/🪤publish:crate");
        assert!(matches!(x, Err(InvalidScopeToken::InvalidCharacter { .. })));
    }

    #[allow(dead_code)]
    struct Bar<'a> {
        foo: std::borrow::Cow<'a, ScopeTokenRef>,
    }

    #[test]
    fn owned_as_cow() {
        let owned = ScopeToken::from_static("https://crates.io/scopes/publish:crate");
        let _bar = Bar { foo: owned.into() };
    }

    #[test]
    fn borrowed_as_cow() {
        let borrowed = ScopeTokenRef::from_static("https://crates.io/scopes/publish:crate");
        let _bar = Bar {
            foo: borrowed.into(),
        };
    }

    #[test]
    fn owned_as_ref_borrowed() {
        let owned = ScopeToken::from_static("https://crates.io/scopes/publish:crate");
        let _reference: &ScopeTokenRef = owned.as_ref();
    }

    #[test]
    fn owned_as_ref_str() {
        let owned = ScopeToken::from_static("https://crates.io/scopes/publish:crate");
        let _reference: &str = owned.as_ref();
    }

    #[test]
    fn borrowed_as_ref_str() {
        let owned = ScopeTokenRef::from_static("https://crates.io/scopes/publish:crate");
        let _reference: &str = owned.as_ref();
    }

    #[test]
    fn owned_borrow_borrowed() {
        let owned = ScopeToken::from_static("https://crates.io/scopes/publish:crate");
        let _reference: &ScopeToken = owned.borrow();
    }

    #[test]
    fn owned_borrow_str() {
        let owned = ScopeToken::from_static("https://crates.io/scopes/publish:crate");
        let _reference: &str = owned.borrow();
    }

    #[test]
    fn borrowed_borrow_str() {
        let owned = ScopeTokenRef::from_static("https://crates.io/scopes/publish:crate");
        let _reference: &str = owned.borrow();
    }
}
