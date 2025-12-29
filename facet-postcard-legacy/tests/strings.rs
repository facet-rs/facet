//! Tests for string types

use eyre::Result;
use facet::Facet;
use facet_postcard_legacy::{from_slice, to_vec};
use postcard::to_allocvec as postcard_to_vec;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;

#[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
struct StringWrapper {
    value: String,
}

#[test]
fn test_empty_string() -> Result<()> {
    facet_testhelpers::setup();
    let wrapper = StringWrapper {
        value: String::new(),
    };
    let facet_bytes = to_vec(&wrapper)?;
    let postcard_bytes = postcard_to_vec(&wrapper)?;
    assert_eq!(facet_bytes, postcard_bytes);

    let decoded: StringWrapper = from_slice(&facet_bytes)?;
    assert_eq!(wrapper, decoded);
    Ok(())
}

#[test]
fn test_ascii_string() -> Result<()> {
    facet_testhelpers::setup();
    let wrapper = StringWrapper {
        value: "Hello, World!".to_string(),
    };
    let facet_bytes = to_vec(&wrapper)?;
    let postcard_bytes = postcard_to_vec(&wrapper)?;
    assert_eq!(facet_bytes, postcard_bytes);

    let decoded: StringWrapper = from_slice(&facet_bytes)?;
    assert_eq!(wrapper, decoded);
    Ok(())
}

#[test]
fn test_unicode_string() -> Result<()> {
    facet_testhelpers::setup();
    let wrapper = StringWrapper {
        value: "こんにちは世界 🦀 Привет мир".to_string(),
    };
    let facet_bytes = to_vec(&wrapper)?;
    let postcard_bytes = postcard_to_vec(&wrapper)?;
    assert_eq!(facet_bytes, postcard_bytes);

    let decoded: StringWrapper = from_slice(&facet_bytes)?;
    assert_eq!(wrapper, decoded);
    Ok(())
}

#[test]
fn test_string_with_null() -> Result<()> {
    facet_testhelpers::setup();
    let wrapper = StringWrapper {
        value: "hello\0world".to_string(),
    };
    let facet_bytes = to_vec(&wrapper)?;
    let postcard_bytes = postcard_to_vec(&wrapper)?;
    assert_eq!(facet_bytes, postcard_bytes);

    let decoded: StringWrapper = from_slice(&facet_bytes)?;
    assert_eq!(wrapper, decoded);
    Ok(())
}

#[test]
fn test_string_with_escapes() -> Result<()> {
    facet_testhelpers::setup();
    let wrapper = StringWrapper {
        value: "line1\nline2\ttab\rcarriage".to_string(),
    };
    let facet_bytes = to_vec(&wrapper)?;
    let postcard_bytes = postcard_to_vec(&wrapper)?;
    assert_eq!(facet_bytes, postcard_bytes);

    let decoded: StringWrapper = from_slice(&facet_bytes)?;
    assert_eq!(wrapper, decoded);
    Ok(())
}

#[test]
fn test_long_string() -> Result<()> {
    facet_testhelpers::setup();
    // Test string longer than 127 bytes (requires 2-byte varint length)
    let wrapper = StringWrapper {
        value: "a".repeat(1000),
    };
    let facet_bytes = to_vec(&wrapper)?;
    let postcard_bytes = postcard_to_vec(&wrapper)?;
    assert_eq!(facet_bytes, postcard_bytes);

    let decoded: StringWrapper = from_slice(&facet_bytes)?;
    assert_eq!(wrapper, decoded);
    Ok(())
}

#[test]
fn test_very_long_string() -> Result<()> {
    facet_testhelpers::setup();
    // Test string longer than 16383 bytes (requires 3-byte varint length)
    let wrapper = StringWrapper {
        value: "🦀".repeat(5000), // 4 bytes * 5000 = 20000 bytes
    };
    let facet_bytes = to_vec(&wrapper)?;
    let postcard_bytes = postcard_to_vec(&wrapper)?;
    assert_eq!(facet_bytes, postcard_bytes);

    let decoded: StringWrapper = from_slice(&facet_bytes)?;
    assert_eq!(wrapper, decoded);
    Ok(())
}

// Test Cow<str>
mod cow_str_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct CowWrapper<'a> {
        #[serde(borrow)]
        value: Cow<'a, str>,
    }

    #[test]
    fn test_cow_owned() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = CowWrapper {
            value: Cow::Owned("hello owned".to_string()),
        };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: CowWrapper<'static> = from_slice(&facet_bytes)?;
        assert_eq!(wrapper.value, decoded.value);
        Ok(())
    }

    #[test]
    fn test_cow_borrowed() -> Result<()> {
        facet_testhelpers::setup();
        let s = "hello borrowed";
        let wrapper = CowWrapper {
            value: Cow::Borrowed(s),
        };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);
        Ok(())
    }
}

// Test multiple strings in a struct
mod multi_string_tests {
    use super::*;

    #[derive(Debug, PartialEq, Facet, Serialize, Deserialize)]
    struct MultiString {
        first: String,
        second: String,
        third: String,
    }

    #[test]
    fn test_multiple_strings() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = MultiString {
            first: "hello".to_string(),
            second: "world".to_string(),
            third: "!".to_string(),
        };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: MultiString = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[test]
    fn test_multiple_empty_strings() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = MultiString {
            first: String::new(),
            second: String::new(),
            third: String::new(),
        };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: MultiString = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }
}

// Test string edge cases
mod edge_cases {
    use super::*;

    #[test]
    fn test_single_char_string() -> Result<()> {
        facet_testhelpers::setup();
        let wrapper = StringWrapper {
            value: "a".to_string(),
        };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: StringWrapper = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[test]
    fn test_127_byte_string() -> Result<()> {
        facet_testhelpers::setup();
        // Exactly at the boundary where varint encoding changes
        let wrapper = StringWrapper {
            value: "a".repeat(127),
        };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: StringWrapper = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[test]
    fn test_128_byte_string() -> Result<()> {
        facet_testhelpers::setup();
        // Just past the boundary
        let wrapper = StringWrapper {
            value: "a".repeat(128),
        };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: StringWrapper = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }

    #[test]
    fn test_all_printable_ascii() -> Result<()> {
        facet_testhelpers::setup();
        let all_printable: String = (32u8..=126u8).map(|b| b as char).collect();
        let wrapper = StringWrapper {
            value: all_printable,
        };
        let facet_bytes = to_vec(&wrapper)?;
        let postcard_bytes = postcard_to_vec(&wrapper)?;
        assert_eq!(facet_bytes, postcard_bytes);

        let decoded: StringWrapper = from_slice(&facet_bytes)?;
        assert_eq!(wrapper, decoded);
        Ok(())
    }
}
