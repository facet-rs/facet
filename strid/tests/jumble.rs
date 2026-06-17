#![deny(unsafe_code)]

use std::{borrow::Cow, convert::Infallible, fmt};

use strid::braid;

mod common;

#[braid]
pub struct Basic;

#[braid(ref_name = "SomeRefName")]
pub struct CustomRefName;

#[braid(validator = "ValidatedBuf")]
pub struct ExternallyValidated;

#[braid(ref_name = "SomeValidatedRefName", validator = "ValidatedBuf")]
pub struct ValidatedWithCustomRefName;

#[braid(serde)]
pub struct Orange;

#[braid]
pub struct OrangeWithNamedField {
    id: String,
}

#[test]
fn internal_access_to_named_ref_field_compile_test() {
    let x = OrangeWithNamedFieldRef::from_static("thing");
    let _ = &x.id;
}

#[braid(
    serde,
    validator,
    ref_doc = "A reference to a cool new orange, that isn't yours!"
)]
pub struct ValidatedBuf;

#[derive(Debug, PartialEq, Eq)]
pub struct InvalidData;

impl std::fmt::Display for InvalidData {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str("found 4-byte UTF-8 codepoints")
    }
}

impl From<Infallible> for InvalidData {
    #[inline(always)]
    fn from(x: Infallible) -> Self {
        match x {}
    }
}

impl std::error::Error for InvalidData {}

impl strid::Validator for ValidatedBuf {
    type Error = InvalidData;
    fn validate(s: &str) -> Result<(), Self::Error> {
        if s.chars().any(|c| c.len_utf8() > 3) {
            Err(InvalidData)
        } else {
            Ok(())
        }
    }
}

#[braid(
    serde,
    normalizer,
    ref_doc = "A reference to a cool new orange, that isn't yours!"
)]
pub struct NormalizedBuf;

impl strid::Validator for NormalizedBuf {
    type Error = InvalidData;

    fn validate(raw: &str) -> Result<(), Self::Error> {
        if raw.chars().any(|c| c.len_utf8() > 3 || c == ' ') {
            Err(InvalidData)
        } else {
            Ok(())
        }
    }
}

impl strid::Normalizer for NormalizedBuf {
    fn normalize(s: &str) -> Result<Cow<'_, str>, Self::Error> {
        if s.chars().any(|c| c.len_utf8() > 3) {
            Err(InvalidData)
        } else if s.contains(' ') {
            Ok(Cow::Owned(s.replace(' ', "")))
        } else {
            Ok(Cow::Borrowed(s))
        }
    }
}

#[braid(clone = "omit", debug = "omit", display = "omit")]
pub struct CustomImpls;

impl fmt::Debug for CustomImpls {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("Owned Debug")
    }
}

impl fmt::Display for CustomImpls {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("Owned Display")
    }
}

impl fmt::Debug for CustomImplsRef {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("Borrowed Debug")
    }
}

impl fmt::Display for CustomImplsRef {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("Borrowed Display")
    }
}

#[braid(debug = "owned", display = "owned")]
pub struct DelegatedImpls;

impl fmt::Debug for DelegatedImplsRef {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("Borrowed Debug")
    }
}

impl fmt::Display for DelegatedImplsRef {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str("Borrowed Display")
    }
}

#[braid(debug = "owned", display = "owned")]
pub struct Secret;

impl fmt::Debug for SecretRef {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if f.alternate() {
            f.write_str("\"")?;
            let max_len = f.width().unwrap_or(10);
            if max_len <= 1 {
                f.write_str("…")?;
            } else {
                match self.0.char_indices().nth(max_len - 2) {
                    Some((idx, c)) if idx + c.len_utf8() < self.0.len() => {
                        f.write_str(&self.0[0..idx + c.len_utf8()])?;
                        f.write_str("…")?;
                    }
                    _ => {
                        f.write_str(&self.0)?;
                    }
                }
            }
            f.write_str("\"")
        } else {
            f.write_str("***SECRET***")
        }
    }
}

impl fmt::Display for SecretRef {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        if f.alternate() {
            f.write_str(&self.0)
        } else {
            f.write_str("***SECRET***")
        }
    }
}

mod tests {
    use super::*;

    #[test]
    fn check_custom_no_impl_clone() {
        static_assertions::assert_not_impl_any!(CustomImpls: Clone);
    }

    #[test]
    fn check_custom_debug() {
        let v = CustomImpls::from_static("");
        let vref: &CustomImplsRef = &v;
        assert_eq!("Owned Debug", format!("{:?}", v));
        assert_eq!("Borrowed Debug", format!("{:?}", vref));
    }

    #[test]
    fn check_custom_display() {
        let v = CustomImpls::from_static("");
        let vref: &CustomImplsRef = &v;
        assert_eq!("Owned Display", format!("{}", v));
        assert_eq!("Borrowed Display", format!("{}", vref));
    }

    #[test]
    fn check_delegated_impl_clone() {
        static_assertions::assert_impl_all!(DelegatedImpls: Clone);
    }

    #[test]
    fn check_delegated_debug() {
        let v = DelegatedImpls::from_static("");
        let vref: &DelegatedImplsRef = &v;
        assert_eq!("Borrowed Debug", format!("{:?}", v));
        assert_eq!("Borrowed Debug", format!("{:?}", vref));
    }

    #[test]
    fn check_delegated_display() {
        let v = DelegatedImpls::from_static("");
        let vref: &DelegatedImplsRef = &v;
        assert_eq!("Borrowed Display", format!("{}", v));
        assert_eq!("Borrowed Display", format!("{}", vref));
    }

    #[test]
    fn check_secret_impl_clone() {
        static_assertions::assert_impl_all!(Secret: Clone);
    }

    #[test]
    fn check_secret_debug() {
        let v = Secret::from_static("my secret is bananas");
        let vref: &SecretRef = &v;
        assert_eq!("***SECRET***", format!("{:?}", v));
        assert_eq!("\"my secret…\"", format!("{:#?}", v));
        assert_eq!("\"…\"", format!("{:#1?}", v));
        assert_eq!("\"my secret is…\"", format!("{:#13?}", v));
        assert_eq!("\"my secret is banana…\"", format!("{:#20?}", v));
        assert_eq!("\"my secret is bananas\"", format!("{:#21?}", v));

        assert_eq!("***SECRET***", format!("{:?}", vref));
        assert_eq!("\"my secret…\"", format!("{:#?}", vref));
        assert_eq!("\"…\"", format!("{:#1?}", vref));
        assert_eq!("\"my secret is…\"", format!("{:#13?}", vref));
        assert_eq!("\"my secret is banana…\"", format!("{:#20?}", vref));
        assert_eq!("\"my secret is bananas\"", format!("{:#21?}", vref));
    }

    #[test]
    fn check_secret_display() {
        let v = Secret::from_static("my secret is bananas");
        let vref: &SecretRef = &v;
        assert_eq!("***SECRET***", format!("{}", v));
        assert_eq!("my secret is bananas", format!("{:#}", v));
        assert_eq!("***SECRET***", format!("{}", vref));
        assert_eq!("my secret is bananas", format!("{:#}", vref));
    }
}
