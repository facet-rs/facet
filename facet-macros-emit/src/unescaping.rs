use facet_macros_parse::DocInner;

#[derive(Debug)]
pub enum UnescapeError {
    IllegalCharacterFollowingBackslash {
        character_index: usize,
        found: char,
        string: String,
    },
    UnexpectedEofFollowingBackslash {
        character_index: usize,
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
        }
    }
}

/// Unescapes a string that has backslashes, double quotes, and single quotes escaped with a preceding backslash.
pub fn unescape(doc_attr: &DocInner) -> Result<String, UnescapeError> {
    unescape_inner(doc_attr.value.as_str())
}

/// Private helper to avoid inappropriate use on strings not from a doc attribute.
/// Exists to make testing easier.
fn unescape_inner(s: &str) -> Result<String, UnescapeError> {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.char_indices();

    while let Some((i, c)) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some((_, '\\')) => out.push('\\'),
                Some((_, '"')) => out.push('"'),
                Some((_, '\'')) => out.push('\''),
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
            r#"hello 'world'"#
        );
        assert_eq!(unescape_inner(r#"back\\slash"#).unwrap(), r#"back\slash"#);
    }

    #[test]
    fn test_unescape_errors() {
        match unescape_inner(r#"invalid \x escape"#) {
            Err(UnescapeError::IllegalCharacterFollowingBackslash {
                character_index,
                found,
                ..
            }) => {
                assert_eq!(character_index, 8);
                assert_eq!(found, 'x');
            }
            _ => panic!("Expected IllegalCharacterFollowingBackslash"),
        }

        match unescape_inner(r#"trailing backslash \"#) {
            Err(UnescapeError::UnexpectedEofFollowingBackslash {
                character_index, ..
            }) => {
                assert_eq!(character_index, 19);
            }
            _ => panic!("Expected UnexpectedEofFollowingBackslash"),
        }
    }
}
