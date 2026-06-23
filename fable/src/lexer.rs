//! Lossless lexer for Fable source.

use crate::SyntaxKind;

/// A single token with the exact source slice it covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lexeme<'src> {
    pub kind: SyntaxKind,
    pub text: &'src str,
}

/// Tokenise `src` into a lossless stream of lexemes.
#[must_use]
pub fn lex(src: &str) -> Vec<Lexeme<'_>> {
    let mut out = Vec::new();
    let mut pos = 0;

    while pos < src.len() {
        let (kind, len) = lex_one(src, pos);
        out.push(Lexeme {
            kind,
            text: &src[pos..pos + len],
        });
        pos += len;
    }

    out
}

fn lex_one(src: &str, pos: usize) -> (SyntaxKind, usize) {
    let s = &src[pos..];
    let first = s.as_bytes()[0];

    if first.is_ascii_whitespace() {
        let len = s.bytes().take_while(u8::is_ascii_whitespace).count();
        return (SyntaxKind::Whitespace, len);
    }

    if s.starts_with("//") {
        let len = s.find('\n').unwrap_or(s.len());
        return (SyntaxKind::Comment, len);
    }

    if s.starts_with("/*") {
        return (SyntaxKind::Comment, block_comment_len(s));
    }

    if first == b'_' || first.is_ascii_alphabetic() {
        let len = s
            .bytes()
            .take_while(|b| *b == b'_' || b.is_ascii_alphanumeric())
            .count();
        return (keyword_or_ident(&s[..len]), len);
    }

    if first.is_ascii_digit() {
        let mut len = s.bytes().take_while(u8::is_ascii_digit).count();
        let mut is_float = false;
        if s[len..].starts_with('.')
            && s[len + 1..]
                .bytes()
                .next()
                .is_some_and(|b| b.is_ascii_digit())
        {
            is_float = true;
            len += 1;
            len += s[len..].bytes().take_while(u8::is_ascii_digit).count();
        }
        return (
            if is_float {
                SyntaxKind::Float
            } else {
                SyntaxKind::Int
            },
            len,
        );
    }

    if first == b'"' || first == b'\'' {
        return (SyntaxKind::Str, string_len(s, first));
    }

    for (text, kind) in MULTI_OPS {
        if s.starts_with(text) {
            return (*kind, text.len());
        }
    }

    if let Some(kind) = single_op(first) {
        return (kind, 1);
    }

    (SyntaxKind::Error, next_char_len(src, pos))
}

fn block_comment_len(src: &str) -> usize {
    let mut pos = 2;
    let mut depth = 1usize;

    while pos < src.len() {
        if src[pos..].starts_with("/*") {
            depth += 1;
            pos += 2;
        } else if src[pos..].starts_with("*/") {
            depth -= 1;
            pos += 2;
            if depth == 0 {
                return pos;
            }
        } else {
            pos += next_char_len(src, pos);
        }
    }

    src.len()
}

fn string_len(src: &str, quote: u8) -> usize {
    let mut pos = 1;
    let bytes = src.as_bytes();

    while pos < bytes.len() {
        if bytes[pos] == b'\\' && pos + 1 < bytes.len() {
            pos += 2;
            continue;
        }
        if bytes[pos] == quote {
            return pos + 1;
        }
        pos += next_char_len(src, pos);
    }

    src.len()
}

const MULTI_OPS: &[(&str, SyntaxKind)] = &[
    ("==", SyntaxKind::EqEq),
    ("!=", SyntaxKind::Neq),
    ("<=", SyntaxKind::Le),
    (">=", SyntaxKind::Ge),
];

fn single_op(byte: u8) -> Option<SyntaxKind> {
    Some(match byte {
        b'.' => SyntaxKind::Dot,
        b',' => SyntaxKind::Comma,
        b';' => SyntaxKind::Semicolon,
        b'(' => SyntaxKind::LParen,
        b')' => SyntaxKind::RParen,
        b'[' => SyntaxKind::LBracket,
        b']' => SyntaxKind::RBracket,
        b'{' => SyntaxKind::LBrace,
        b'}' => SyntaxKind::RBrace,
        b'=' => SyntaxKind::Assign,
        b'+' => SyntaxKind::Plus,
        b'-' => SyntaxKind::Minus,
        b'<' => SyntaxKind::Lt,
        b'>' => SyntaxKind::Gt,
        _ => return None,
    })
}

fn keyword_or_ident(word: &str) -> SyntaxKind {
    match word {
        "if" => SyntaxKind::IfKw,
        "else" => SyntaxKind::ElseKw,
        "and" => SyntaxKind::AndKw,
        "or" => SyntaxKind::OrKw,
        "not" => SyntaxKind::NotKw,
        "true" => SyntaxKind::True,
        "false" => SyntaxKind::False,
        "null" | "none" => SyntaxKind::Null,
        _ => SyntaxKind::Ident,
    }
}

fn next_char_len(src: &str, pos: usize) -> usize {
    src[pos..].chars().next().map_or(1, char::len_utf8)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_lossless(src: &str) -> Vec<Lexeme<'_>> {
        let tokens = lex(src);
        let joined: String = tokens.iter().map(|token| token.text).collect();
        assert_eq!(joined, src);
        tokens
    }

    fn kinds(src: &str) -> Vec<SyntaxKind> {
        assert_lossless(src)
            .into_iter()
            .map(|token| token.kind)
            .collect()
    }

    #[test]
    fn lexes_assignment_path_and_literals() {
        use SyntaxKind::*;
        assert_eq!(
            kinds(r#"root.user.name = "Ada";"#),
            [
                Ident, Dot, Ident, Dot, Ident, Whitespace, Assign, Whitespace, Str, Semicolon
            ]
        );
    }

    #[test]
    fn lexes_if_expression_with_index_and_comment() {
        use SyntaxKind::*;
        assert_eq!(
            kinds("if root.users[0].age >= 18 { // ok\nroot.adult = true }"),
            [
                IfKw, Whitespace, Ident, Dot, Ident, LBracket, Int, RBracket, Dot, Ident,
                Whitespace, Ge, Whitespace, Int, Whitespace, LBrace, Whitespace, Comment,
                Whitespace, Ident, Dot, Ident, Whitespace, Assign, Whitespace, True, Whitespace,
                RBrace
            ]
        );
    }

    #[test]
    fn lexes_nested_block_comments_losslessly() {
        use SyntaxKind::*;
        assert_eq!(
            kinds("a /* outer /* inner */ done */ b"),
            [Ident, Whitespace, Comment, Whitespace, Ident]
        );
    }
}
