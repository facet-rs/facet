//! Processes string escapes and unescapes them for docstrings.

use crate::DocInner;

/// Errors that can occur while unescaping a string
#[derive(Debug, PartialEq, Eq)]
pub enum UnescapeError {
    /// An illegal character was found following a backslash (e.g., `\a`)
    IllegalCharacterFollowingBackslash {
        /// Index of the backslash in the original string
        character_index: usize,
        /// The illegal character found
        found: char,
        /// The original string being unescaped
        string: String,
    },
    /// The string ended unexpectedly after a backslash character
    UnexpectedEofFollowingBackslash {
        /// Index of the backslash in the original string
        character_index: usize,
        /// The original string being unescaped
        string: String,
    },
    /// Invalid hex digit in \xNN escape
    InvalidHexEscape {
        /// Index of the escape start in the original string
        character_index: usize,
        /// The original string being unescaped
        string: String,
    },
    /// Invalid unicode escape \u{...}
    InvalidUnicodeEscape {
        /// Index of the escape start in the original string
        character_index: usize,
        /// The original string being unescaped
        string: String,
    },
}

impl std::fmt::Display for UnescapeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UnescapeError::IllegalCharacterFollowingBackslash {
                character_index,
                found,
                string,
            } => {
                write!(
                    f,
                    "Illegal character following a backslash at index {character_index} in {string:?}: found '{found}'"
                )
            }
            UnescapeError::UnexpectedEofFollowingBackslash {
                character_index,
                string,
            } => {
                write!(
                    f,
                    "Unexpected end of file following a backslash at index {character_index} in {string:?}"
                )
            }
            UnescapeError::InvalidHexEscape {
                character_index,
                string,
            } => {
                write!(
                    f,
                    "Invalid hex escape at index {character_index} in {string:?}"
                )
            }
            UnescapeError::InvalidUnicodeEscape {
                character_index,
                string,
            } => {
                write!(
                    f,
                    "Invalid unicode escape at index {character_index} in {string:?}"
                )
            }
        }
    }
}

/// Unescapes a doc attribute string.
pub fn unescape(doc_attr: &DocInner) -> Result<String, UnescapeError> {
    unescape_inner(doc_attr.value.as_str())
}

/// Parse exactly 2 hex digits from the iterator.
fn parse_hex_escape(
    chars: &mut std::iter::Peekable<impl Iterator<Item = (usize, char)>>,
    escape_start: usize,
    s: &str,
) -> Result<char, UnescapeError> {
    let mut value = 0u8;
    for _ in 0..2 {
        match chars.next() {
            Some((_, c)) if c.is_ascii_hexdigit() => {
                value = value * 16 + c.to_digit(16).unwrap() as u8;
            }
            _ => {
                return Err(UnescapeError::InvalidHexEscape {
                    character_index: escape_start,
                    string: s.to_string(),
                });
            }
        }
    }
    Ok(value as char)
}

/// Parse a unicode escape \u{NNNN} from the iterator.
fn parse_unicode_escape(
    chars: &mut std::iter::Peekable<impl Iterator<Item = (usize, char)>>,
    escape_start: usize,
    s: &str,
) -> Result<char, UnescapeError> {
    // Expect opening brace
    match chars.next() {
        Some((_, '{')) => {}
        _ => {
            return Err(UnescapeError::InvalidUnicodeEscape {
                character_index: escape_start,
                string: s.to_string(),
            });
        }
    }

    let mut value = 0u32;
    let mut digit_count = 0;

    loop {
        match chars.next() {
            Some((_, '}')) => break,
            Some((_, c)) if c.is_ascii_hexdigit() => {
                digit_count += 1;
                if digit_count > 6 {
                    return Err(UnescapeError::InvalidUnicodeEscape {
                        character_index: escape_start,
                        string: s.to_string(),
                    });
                }
                value = value * 16 + c.to_digit(16).unwrap();
            }
            _ => {
                return Err(UnescapeError::InvalidUnicodeEscape {
                    character_index: escape_start,
                    string: s.to_string(),
                });
            }
        }
    }

    if digit_count == 0 {
        return Err(UnescapeError::InvalidUnicodeEscape {
            character_index: escape_start,
            string: s.to_string(),
        });
    }

    char::from_u32(value).ok_or_else(|| UnescapeError::InvalidUnicodeEscape {
        character_index: escape_start,
        string: s.to_string(),
    })
}

/// Unescapes a string with Rust-style escape sequences.
///
/// Supported escapes:
/// - `\\` -> backslash
/// - `\"` -> double quote
/// - `\'` -> single quote
/// - `\n` -> newline
/// - `\r` -> carriage return
/// - `\t` -> tab
/// - `\0` -> null
/// - `\xNN` -> byte value (2 hex digits, ASCII only)
/// - `\u{NNNNNN}` -> unicode scalar value (1-6 hex digits)
pub fn unescape_inner(s: &str) -> Result<String, UnescapeError> {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.char_indices().peekable();

    while let Some((i, c)) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some((_, '\\')) => out.push('\\'),
                Some((_, '"')) => out.push('"'),
                Some((_, '\'')) => out.push('\''),
                Some((_, 'n')) => out.push('\n'),
                Some((_, 'r')) => out.push('\r'),
                Some((_, 't')) => out.push('\t'),
                Some((_, '0')) => out.push('\0'),
                Some((_, 'x')) => {
                    out.push(parse_hex_escape(&mut chars, i, s)?);
                }
                Some((_, 'u')) => {
                    out.push(parse_unicode_escape(&mut chars, i, s)?);
                }
                Some((_, found)) => {
                    return Err(UnescapeError::IllegalCharacterFollowingBackslash {
                        character_index: i,
                        found,
                        string: s.to_string(),
                    });
                }
                None => {
                    return Err(UnescapeError::UnexpectedEofFollowingBackslash {
                        character_index: i,
                        string: s.to_string(),
                    });
                }
            }
        } else {
            out.push(c);
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unescape_basic() {
        assert_eq!(unescape_inner("hello").unwrap(), "hello");
        assert_eq!(
            unescape_inner(r#"hello \"world\""#).unwrap(),
            r#"hello "world""#
        );
        assert_eq!(
            unescape_inner(r#"hello \'world\'"#).unwrap(),
            "hello 'world'"
        );
        assert_eq!(unescape_inner(r"back\\slash").unwrap(), r"back\slash");
    }

    #[test]
    fn test_unescape_newline() {
        // This is the case from issue #921
        assert_eq!(
            unescape_inner(r"```solidity\nstruct MyStruct { ... }\n```").unwrap(),
            "```solidity\nstruct MyStruct { ... }\n```"
        );
    }

    #[test]
    fn test_unescape_common_escapes() {
        assert_eq!(unescape_inner(r"hello\nworld").unwrap(), "hello\nworld");
        assert_eq!(unescape_inner(r"hello\rworld").unwrap(), "hello\rworld");
        assert_eq!(unescape_inner(r"hello\tworld").unwrap(), "hello\tworld");
        assert_eq!(unescape_inner(r"null\0char").unwrap(), "null\0char");
        assert_eq!(
            unescape_inner(r"line1\nline2\nline3").unwrap(),
            "line1\nline2\nline3"
        );
        assert_eq!(unescape_inner(r"tab\there").unwrap(), "tab\there");
        assert_eq!(unescape_inner(r"cr\rlf").unwrap(), "cr\rlf");
        assert_eq!(unescape_inner(r"crlf\r\n").unwrap(), "crlf\r\n");
    }

    #[test]
    fn test_unescape_hex() {
        assert_eq!(unescape_inner(r"\x41").unwrap(), "A");
        assert_eq!(unescape_inner(r"\x61").unwrap(), "a");
        assert_eq!(unescape_inner(r"\x00").unwrap(), "\0");
        assert_eq!(unescape_inner(r"\x7f").unwrap(), "\x7f");
        assert_eq!(unescape_inner(r"hello\x20world").unwrap(), "hello world");
    }

    #[test]
    fn test_unescape_unicode() {
        assert_eq!(unescape_inner(r"\u{41}").unwrap(), "A");
        assert_eq!(unescape_inner(r"\u{0041}").unwrap(), "A");
        assert_eq!(unescape_inner(r"\u{1F600}").unwrap(), "😀");
        assert_eq!(unescape_inner(r"\u{10FFFF}").unwrap(), "\u{10FFFF}");
        assert_eq!(unescape_inner(r"hello\u{20}world").unwrap(), "hello world");
    }

    #[test]
    fn test_unescape_mixed() {
        assert_eq!(
            unescape_inner(r#"line1\nline2\ttab\\backslash\"quote"#).unwrap(),
            "line1\nline2\ttab\\backslash\"quote"
        );
    }

    #[test]
    fn test_unescape_errors() {
        // Invalid escape character
        assert!(matches!(
            unescape_inner(r"invalid \a escape"),
            Err(UnescapeError::IllegalCharacterFollowingBackslash {
                character_index: 8,
                found: 'a',
                ..
            })
        ));

        // Trailing backslash
        assert!(matches!(
            unescape_inner(r"trailing backslash \"),
            Err(UnescapeError::UnexpectedEofFollowingBackslash {
                character_index: 19,
                ..
            })
        ));

        // Invalid hex escape (not enough digits)
        assert!(matches!(
            unescape_inner(r"\x4"),
            Err(UnescapeError::InvalidHexEscape { .. })
        ));

        // Invalid hex escape (non-hex character)
        assert!(matches!(
            unescape_inner(r"\xGG"),
            Err(UnescapeError::InvalidHexEscape { .. })
        ));

        // Invalid unicode escape (no braces)
        assert!(matches!(
            unescape_inner(r"\u0041"),
            Err(UnescapeError::InvalidUnicodeEscape { .. })
        ));

        // Invalid unicode escape (empty)
        assert!(matches!(
            unescape_inner(r"\u{}"),
            Err(UnescapeError::InvalidUnicodeEscape { .. })
        ));

        // Invalid unicode escape (too many digits)
        assert!(matches!(
            unescape_inner(r"\u{1234567}"),
            Err(UnescapeError::InvalidUnicodeEscape { .. })
        ));

        // Invalid unicode escape (invalid codepoint)
        assert!(matches!(
            unescape_inner(r"\u{FFFFFF}"),
            Err(UnescapeError::InvalidUnicodeEscape { .. })
        ));
    }
}
