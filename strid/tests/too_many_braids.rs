use std::{borrow::Cow, convert::Infallible, error, fmt};
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
            .find(|&(_, &b)| b <= 0x20 || b == 0x22 || b == 0x5C)
        {
            Err(InvalidScopeToken::InvalidCharacter { position, value })
        } else {
            Ok(())
        }
    }
}

macro_rules! too_many_braids {
    ($modname:ident) => {
        mod $modname {
            use strid::braid;

            /// A basic example of a wrapper around a [`String`]
            #[braid(
                serde,
                ref_doc = "A borrowed reference to a basic string slice wrapper"
            )]
            pub struct BasicExampleBuf;

            /// A non-empty [`String`] normalized to lowercase
            #[braid(
                serde,
                normalizer = "super::LowerString",
                ref_name = "LowerStr",
                ref_doc = "A borrowed reference to a non-empty, lowercase string"
            )]
            pub struct LowerString;

            /// A scope token as defined in RFC6749, Section 3.3
            #[braid(
                serde,
                validator = "super::ScopeToken",
                ref_doc = "A borrowed reference to a [`ScopeToken`]"
            )]
            pub struct ScopeToken;
        }
    };
}

too_many_braids!(iter0);
too_many_braids!(iter1);
too_many_braids!(iter2);
too_many_braids!(iter3);
too_many_braids!(iter4);
too_many_braids!(iter5);
too_many_braids!(iter6);
too_many_braids!(iter7);
too_many_braids!(iter8);
too_many_braids!(iter9);
// too_many_braids!(iter10);
// too_many_braids!(iter11);
// too_many_braids!(iter12);
// too_many_braids!(iter13);
// too_many_braids!(iter14);
// too_many_braids!(iter15);
// too_many_braids!(iter16);
// too_many_braids!(iter17);
// too_many_braids!(iter18);
// too_many_braids!(iter19);
// too_many_braids!(iter20);
// too_many_braids!(iter21);
// too_many_braids!(iter22);
// too_many_braids!(iter23);
// too_many_braids!(iter24);
// too_many_braids!(iter25);
// too_many_braids!(iter26);
// too_many_braids!(iter27);
// too_many_braids!(iter28);
// too_many_braids!(iter29);
// too_many_braids!(iter30);
// too_many_braids!(iter31);
// too_many_braids!(iter32);
// too_many_braids!(iter33);
// too_many_braids!(iter34);
// too_many_braids!(iter35);
// too_many_braids!(iter36);
// too_many_braids!(iter37);
// too_many_braids!(iter38);
// too_many_braids!(iter39);
// too_many_braids!(iter40);
// too_many_braids!(iter41);
// too_many_braids!(iter42);
// too_many_braids!(iter43);
// too_many_braids!(iter44);
// too_many_braids!(iter45);
// too_many_braids!(iter46);
// too_many_braids!(iter47);
// too_many_braids!(iter48);
// too_many_braids!(iter49);
// too_many_braids!(iter50);
// too_many_braids!(iter51);
// too_many_braids!(iter52);
// too_many_braids!(iter53);
// too_many_braids!(iter54);
// too_many_braids!(iter55);
// too_many_braids!(iter56);
// too_many_braids!(iter57);
// too_many_braids!(iter58);
// too_many_braids!(iter59);
// too_many_braids!(iter60);
// too_many_braids!(iter61);
// too_many_braids!(iter62);
// too_many_braids!(iter63);
// too_many_braids!(iter64);
// too_many_braids!(iter65);
// too_many_braids!(iter66);
// too_many_braids!(iter67);
// too_many_braids!(iter68);
// too_many_braids!(iter69);
// too_many_braids!(iter70);
// too_many_braids!(iter71);
// too_many_braids!(iter72);
// too_many_braids!(iter73);
// too_many_braids!(iter74);
// too_many_braids!(iter75);
// too_many_braids!(iter76);
// too_many_braids!(iter77);
// too_many_braids!(iter78);
// too_many_braids!(iter79);
// too_many_braids!(iter80);
// too_many_braids!(iter81);
// too_many_braids!(iter82);
// too_many_braids!(iter83);
// too_many_braids!(iter84);
// too_many_braids!(iter85);
// too_many_braids!(iter86);
// too_many_braids!(iter87);
// too_many_braids!(iter88);
// too_many_braids!(iter89);
// too_many_braids!(iter90);
// too_many_braids!(iter91);
// too_many_braids!(iter92);
// too_many_braids!(iter93);
// too_many_braids!(iter94);
// too_many_braids!(iter95);
// too_many_braids!(iter96);
// too_many_braids!(iter97);
// too_many_braids!(iter98);
// too_many_braids!(iter99);
