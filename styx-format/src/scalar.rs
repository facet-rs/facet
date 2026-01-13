//! Scalar handling utilities for Styx.
//!
//! Provides functions for determining how scalars should be represented
//! (bare, quoted, raw, heredoc) and for escaping/unescaping string content.

use std::borrow::Cow;

/// Check if a string can be written as a bare scalar.
///
/// A bare scalar is valid when:
/// 1. It's not empty
/// 2. It doesn't start with characters that look like other syntax (`//`, `r#`, `<<`)
/// 3. It doesn't contain special characters: `{}(),"=@` or whitespace
pub fn can_be_bare(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    // Cannot start with characters that look like other syntax
    if s.starts_with("//") || s.starts_with("r#") || s.starts_with("<<") {
        return false;
    }
    // Cannot contain special characters
    !s.chars()
        .any(|c| matches!(c, '{' | '}' | '(' | ')' | ',' | '"' | '=' | '@') || c.is_whitespace())
}

/// Count escape sequences needed for a quoted string.
pub fn count_escapes(s: &str) -> usize {
    s.chars()
        .filter(|c| matches!(c, '"' | '\\' | '\n' | '\r' | '\t'))
        .count()
}

/// Count newlines in a string.
pub fn count_newlines(s: &str) -> usize {
    s.chars().filter(|&c| c == '\n').count()
}

/// Escape a string for quoted output.
///
/// Returns the escaped content (without surrounding quotes).
pub fn escape_quoted(s: &str) -> Cow<'_, str> {
    // Check if any escapes needed
    if !s
        .chars()
        .any(|c| matches!(c, '"' | '\\' | '\n' | '\r' | '\t') || c.is_ascii_control())
    {
        return Cow::Borrowed(s);
    }

    let mut result = String::with_capacity(s.len() + 8);
    for c in s.chars() {
        match c {
            '"' => result.push_str("\\\""),
            '\\' => result.push_str("\\\\"),
            '\n' => result.push_str("\\n"),
            '\r' => result.push_str("\\r"),
            '\t' => result.push_str("\\t"),
            c if c.is_ascii_control() => {
                // Write as \u{XXXX}
                let code = c as u32;
                result.push_str(&format!("\\u{{{code:04x}}}"));
            }
            c => result.push(c),
        }
    }
    Cow::Owned(result)
}

/// Unescape a quoted string.
///
/// The input should be the content between quotes (without the surrounding quotes).
pub fn unescape_quoted(s: &str) -> Cow<'_, str> {
    // Check if any escapes present
    if !s.contains('\\') {
        return Cow::Borrowed(s);
    }

    // Process escapes
    let mut result = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();

    while let Some(c) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some('n') => result.push('\n'),
                Some('r') => result.push('\r'),
                Some('t') => result.push('\t'),
                Some('\\') => result.push('\\'),
                Some('"') => result.push('"'),
                Some('0') => result.push('\0'),
                Some('u') => {
                    // Handle \u{XXXX} or \uXXXX
                    if chars.peek() == Some(&'{') {
                        chars.next(); // consume '{'
                        let mut hex = String::new();
                        while let Some(&c) = chars.peek() {
                            if c == '}' {
                                chars.next();
                                break;
                            }
                            hex.push(chars.next().unwrap());
                        }
                        if let Ok(code) = u32::from_str_radix(&hex, 16) {
                            if let Some(ch) = char::from_u32(code) {
                                result.push(ch);
                            }
                        }
                    } else {
                        // \uXXXX format (4 hex digits)
                        let mut hex = String::new();
                        for _ in 0..4 {
                            if let Some(&c) = chars.peek() {
                                if c.is_ascii_hexdigit() {
                                    hex.push(chars.next().unwrap());
                                } else {
                                    break;
                                }
                            }
                        }
                        if let Ok(code) = u32::from_str_radix(&hex, 16) {
                            if let Some(ch) = char::from_u32(code) {
                                result.push(ch);
                            }
                        }
                    }
                }
                Some(c) => {
                    // Unknown escape - keep as-is
                    result.push('\\');
                    result.push(c);
                }
                None => {
                    result.push('\\');
                }
            }
        } else {
            result.push(c);
        }
    }

    Cow::Owned(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_can_be_bare() {
        // These should be bare
        assert!(can_be_bare("localhost"));
        assert!(can_be_bare("8080"));
        assert!(can_be_bare("hello-world"));
        assert!(can_be_bare("https://example.com/path"));
        assert!(can_be_bare("true"));
        assert!(can_be_bare("false"));

        // These must be quoted
        assert!(!can_be_bare("")); // empty
        assert!(!can_be_bare("hello world")); // space
        assert!(!can_be_bare("{braces}")); // braces
        assert!(!can_be_bare("(parens)")); // parens
        assert!(!can_be_bare("key=value")); // equals
        assert!(!can_be_bare("@tag")); // at sign
        assert!(!can_be_bare("//comment")); // looks like comment
        assert!(!can_be_bare("r#raw")); // looks like raw string
        assert!(!can_be_bare("<<HERE")); // looks like heredoc
    }

    #[test]
    fn test_escape_quoted() {
        assert_eq!(escape_quoted("hello"), "hello");
        assert_eq!(escape_quoted("hello world"), "hello world");
        assert_eq!(escape_quoted("say \"hi\""), "say \\\"hi\\\"");
        assert_eq!(escape_quoted("line1\nline2"), "line1\\nline2");
        assert_eq!(escape_quoted("path\\to\\file"), "path\\\\to\\\\file");
    }

    #[test]
    fn test_unescape_quoted() {
        assert_eq!(unescape_quoted("hello"), "hello");
        assert_eq!(unescape_quoted("say \\\"hi\\\""), "say \"hi\"");
        assert_eq!(unescape_quoted("line1\\nline2"), "line1\nline2");
        assert_eq!(unescape_quoted("path\\\\to\\\\file"), "path\\to\\file");
        assert_eq!(unescape_quoted("tab\\there"), "tab\there");
    }

    #[test]
    fn test_roundtrip() {
        let cases = ["hello", "hello world", "say \"hi\"", "line1\nline2", "a\\b"];

        for case in cases {
            let escaped = escape_quoted(case);
            let unescaped = unescape_quoted(&escaped);
            assert_eq!(unescaped, case, "roundtrip failed for: {case:?}");
        }
    }
}
