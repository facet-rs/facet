use facet_macros_parse::DocInner;

#[derive(Debug)]
pub enum UnescapeError {
    UnexpectedBackslash { character_index: usize },
}

impl std::fmt::Display for UnescapeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            UnescapeError::UnexpectedBackslash { character_index } => {
                write!(f, "Unexpected backslash at index {}", character_index)
            }
        }
    }
}

pub fn unescape(doc_attr: &DocInner) -> Result<String, UnescapeError> {
    let s = doc_attr.value.as_str();
    let mut out = String::with_capacity(s.len());
    let mut chars = s.char_indices();

    while let Some((i, c)) = chars.next() {
        if c == '\\' {
            match chars.next() {
                Some((_, '\\')) => out.push('\\'),
                Some((_, '"')) => out.push('"'),
                Some((_, '\'')) => out.push('\''),
                Some((_, _)) | None => {
                    return Err(UnescapeError::UnexpectedBackslash { character_index: i });
                }
            }
        } else {
            out.push(c);
        }
    }
    Ok(out)
}
