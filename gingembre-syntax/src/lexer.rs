//! Lossless lexer for gingembre templates.
//!
//! Every byte of the source is covered by exactly one [`Lexeme`] (concatenating their
//! text reproduces the input), so the cstree tree is lossless. Whitespace inside code,
//! comments, and the `{%- -%}` trim dashes are all preserved as tokens; trimming is a
//! *lowering* concern, not a lexing one.

use crate::SyntaxKind;

/// A single token: its kind and the exact source slice it covers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Lexeme<'src> {
    pub kind: SyntaxKind,
    pub text: &'src str,
}

#[derive(Clone, Copy)]
enum Mode {
    /// Outside delimiters — raw template text.
    Text,
    /// Inside `{{ … }}` or `{% … %}`.
    Code,
}

/// Tokenise `src` into a lossless stream of lexemes.
pub fn lex(src: &str) -> Vec<Lexeme<'_>> {
    let mut out = Vec::new();
    let bytes = src.as_bytes();
    let mut pos = 0;
    let mut mode = Mode::Text;

    let starts_with = |pos: usize, s: &str| src[pos..].starts_with(s);

    while pos < bytes.len() {
        match mode {
            Mode::Text => {
                // A comment is text-level: lex the whole `{# … #}` (with nesting) as one
                // token and stay in text mode.
                if starts_with(pos, "{#") {
                    let end = lex_comment_end(src, pos);
                    out.push(Lexeme {
                        kind: SyntaxKind::Comment,
                        text: &src[pos..end],
                    });
                    pos = end;
                    continue;
                }
                // An opening delimiter ends the current text run.
                if let Some((kind, len)) = open_delim(src, pos) {
                    out.push(Lexeme {
                        kind,
                        text: &src[pos..pos + len],
                    });
                    pos += len;
                    mode = Mode::Code;
                    continue;
                }
                // Otherwise accumulate text until the next `{{` / `{%` / `{#`.
                let start = pos;
                while pos < bytes.len() {
                    if starts_with(pos, "{{") || starts_with(pos, "{%") || starts_with(pos, "{#") {
                        break;
                    }
                    pos += next_char_len(src, pos);
                }
                out.push(Lexeme {
                    kind: SyntaxKind::Text,
                    text: &src[start..pos],
                });
            }
            Mode::Code => {
                // Closing delimiter returns to text mode.
                if let Some((kind, len)) = close_delim(src, pos) {
                    out.push(Lexeme {
                        kind,
                        text: &src[pos..pos + len],
                    });
                    pos += len;
                    mode = Mode::Text;
                    continue;
                }
                let (kind, len) = lex_code_token(src, pos);
                out.push(Lexeme {
                    kind,
                    text: &src[pos..pos + len],
                });
                pos += len;
            }
        }
    }

    out
}

/// Match an opening delimiter at `pos`, returning `(kind, byte_len)`. Trim variants
/// (`{{-`/`{%-`) are three bytes.
fn open_delim(src: &str, pos: usize) -> Option<(SyntaxKind, usize)> {
    let s = &src[pos..];
    if s.starts_with("{{-") {
        Some((SyntaxKind::OpenExprTrim, 3))
    } else if s.starts_with("{{") {
        Some((SyntaxKind::OpenExpr, 2))
    } else if s.starts_with("{%-") {
        Some((SyntaxKind::OpenStmtTrim, 3))
    } else if s.starts_with("{%") {
        Some((SyntaxKind::OpenStmt, 2))
    } else {
        None
    }
}

/// Match a closing delimiter at `pos`.
fn close_delim(src: &str, pos: usize) -> Option<(SyntaxKind, usize)> {
    let s = &src[pos..];
    if s.starts_with("-}}") {
        Some((SyntaxKind::CloseExprTrim, 3))
    } else if s.starts_with("}}") {
        Some((SyntaxKind::CloseExpr, 2))
    } else if s.starts_with("-%}") {
        Some((SyntaxKind::CloseStmtTrim, 3))
    } else if s.starts_with("%}") {
        Some((SyntaxKind::CloseStmt, 2))
    } else {
        None
    }
}

/// Find the byte offset just past the matching `#}` for a comment starting at `pos`
/// (which is at `{#`). Comments nest. An unterminated comment runs to EOF.
fn lex_comment_end(src: &str, pos: usize) -> usize {
    let mut i = pos + 2; // past `{#`
    let mut depth = 1usize;
    let bytes = src.as_bytes();
    while i < bytes.len() {
        if src[i..].starts_with("#}") {
            depth -= 1;
            i += 2;
            if depth == 0 {
                return i;
            }
        } else if src[i..].starts_with("{#") {
            depth += 1;
            i += 2;
        } else {
            i += next_char_len(src, i);
        }
    }
    src.len()
}

/// Lex one token in code mode at `pos`, returning `(kind, byte_len)`.
fn lex_code_token(src: &str, pos: usize) -> (SyntaxKind, usize) {
    let s = &src[pos..];
    let first = s.as_bytes()[0];

    // Whitespace run.
    if first.is_ascii_whitespace() {
        let len = s.bytes().take_while(|b| b.is_ascii_whitespace()).count();
        return (SyntaxKind::Whitespace, len);
    }

    // Identifier / keyword.
    if first == b'_' || first.is_ascii_alphabetic() {
        let len = s
            .bytes()
            .take_while(|b| *b == b'_' || b.is_ascii_alphanumeric())
            .count();
        return (keyword_or_ident(&s[..len]), len);
    }

    // Number (int or float). A leading `-` is lexed as the Minus operator, not part of
    // the number, so the parser handles unary minus uniformly.
    if first.is_ascii_digit() {
        let mut len = s.bytes().take_while(|b| b.is_ascii_digit()).count();
        let mut is_float = false;
        if s[len..].starts_with('.')
            && s[len + 1..]
                .bytes()
                .next()
                .is_some_and(|b| b.is_ascii_digit())
        {
            is_float = true;
            len += 1; // dot
            len += s[len..].bytes().take_while(|b| b.is_ascii_digit()).count();
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

    // String literal (single or double quoted, with backslash escapes).
    if first == b'"' || first == b'\'' {
        let quote = first;
        let mut i = 1;
        let b = s.as_bytes();
        while i < b.len() {
            if b[i] == b'\\' && i + 1 < b.len() {
                i += 2;
                continue;
            }
            if b[i] == quote {
                i += 1;
                return (SyntaxKind::Str, i);
            }
            i += next_char_len(s, i);
        }
        return (SyntaxKind::Str, b.len()); // unterminated → to end of code
    }

    // Multi-char operators first, then single-char.
    for (text, kind) in MULTI_OPS {
        if s.starts_with(text) {
            return (*kind, text.len());
        }
    }
    if let Some(kind) = single_op(first) {
        return (kind, 1);
    }

    // Anything else: one error char.
    (SyntaxKind::Error, next_char_len(src, pos))
}

const MULTI_OPS: &[(&str, SyntaxKind)] = &[
    ("::", SyntaxKind::ColonColon),
    ("**", SyntaxKind::StarStar),
    ("//", SyntaxKind::SlashSlash),
    ("==", SyntaxKind::EqEq),
    ("!=", SyntaxKind::Neq),
    ("<=", SyntaxKind::Le),
    (">=", SyntaxKind::Ge),
];

fn single_op(b: u8) -> Option<SyntaxKind> {
    Some(match b {
        b'.' => SyntaxKind::Dot,
        b',' => SyntaxKind::Comma,
        b':' => SyntaxKind::Colon,
        b'|' => SyntaxKind::Pipe,
        b'?' => SyntaxKind::Question,
        b'(' => SyntaxKind::LParen,
        b')' => SyntaxKind::RParen,
        b'[' => SyntaxKind::LBracket,
        b']' => SyntaxKind::RBracket,
        b'{' => SyntaxKind::LBrace,
        b'}' => SyntaxKind::RBrace,
        b'=' => SyntaxKind::Assign,
        b'+' => SyntaxKind::Plus,
        b'-' => SyntaxKind::Minus,
        b'*' => SyntaxKind::Star,
        b'/' => SyntaxKind::Slash,
        b'%' => SyntaxKind::Percent,
        b'~' => SyntaxKind::Tilde,
        b'<' => SyntaxKind::Lt,
        b'>' => SyntaxKind::Gt,
        _ => return None,
    })
}

fn keyword_or_ident(word: &str) -> SyntaxKind {
    match word {
        "if" => SyntaxKind::IfKw,
        "elif" => SyntaxKind::ElifKw,
        "else" => SyntaxKind::ElseKw,
        "endif" => SyntaxKind::EndifKw,
        "for" => SyntaxKind::ForKw,
        "endfor" => SyntaxKind::EndforKw,
        "set" => SyntaxKind::SetKw,
        "endset" => SyntaxKind::EndsetKw,
        "block" => SyntaxKind::BlockKw,
        "endblock" => SyntaxKind::EndblockKw,
        "extends" => SyntaxKind::ExtendsKw,
        "include" => SyntaxKind::IncludeKw,
        "import" => SyntaxKind::ImportKw,
        "macro" => SyntaxKind::MacroKw,
        "endmacro" => SyntaxKind::EndmacroKw,
        "break" => SyntaxKind::BreakKw,
        "continue" => SyntaxKind::ContinueKw,
        "as" => SyntaxKind::AsKw,
        "in" => SyntaxKind::InKw,
        "is" => SyntaxKind::IsKw,
        "not" => SyntaxKind::NotKw,
        "and" => SyntaxKind::AndKw,
        "or" => SyntaxKind::OrKw,
        "true" | "True" => SyntaxKind::True,
        "false" | "False" => SyntaxKind::False,
        "none" | "None" => SyntaxKind::NoneKw,
        _ => SyntaxKind::Ident,
    }
}

fn next_char_len(src: &str, pos: usize) -> usize {
    src[pos..].chars().next().map_or(1, char::len_utf8)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The lexer is lossless: the concatenation of all lexeme texts is the input.
    fn assert_lossless(src: &str) -> Vec<Lexeme<'_>> {
        let toks = lex(src);
        let joined: String = toks.iter().map(|t| t.text).collect();
        assert_eq!(joined, src, "lexer dropped/added bytes");
        toks
    }

    fn kinds(src: &str) -> Vec<SyntaxKind> {
        assert_lossless(src).iter().map(|t| t.kind).collect()
    }

    #[test]
    fn plain_text() {
        assert_eq!(kinds("hello world"), [SyntaxKind::Text]);
    }

    #[test]
    fn interpolation_with_field_and_filter() {
        use SyntaxKind::*;
        assert_eq!(
            kinds("a {{ page.title | upper }} b"),
            [
                Text, OpenExpr, Whitespace, Ident, Dot, Ident, Whitespace, Pipe, Whitespace, Ident,
                Whitespace, CloseExpr, Text
            ]
        );
    }

    #[test]
    fn statement_with_trim_and_keywords() {
        use SyntaxKind::*;
        assert_eq!(
            kinds("{%- if x is not defined -%}"),
            [
                OpenStmtTrim,
                Whitespace,
                IfKw,
                Whitespace,
                Ident,
                Whitespace,
                IsKw,
                Whitespace,
                NotKw,
                Whitespace,
                Ident,
                Whitespace,
                CloseStmtTrim
            ]
        );
    }

    #[test]
    fn comment_is_one_token_and_nests() {
        use SyntaxKind::*;
        assert_eq!(
            kinds("a {# outer {# inner #} still #} b"),
            [Text, Comment, Text]
        );
    }

    #[test]
    fn numbers_strings_operators_optional() {
        use SyntaxKind::*;
        assert_eq!(
            kinds(r#"{{ a[:3] ~ "x" // 2 + b? }}"#),
            [
                OpenExpr, Whitespace, Ident, LBracket, Colon, Int, RBracket, Whitespace, Tilde,
                Whitespace, Str, Whitespace, SlashSlash, Whitespace, Int, Whitespace, Plus,
                Whitespace, Ident, Question, Whitespace, CloseExpr
            ]
        );
    }

    #[test]
    fn macro_dotted_call() {
        use SyntaxKind::*;
        assert_eq!(
            kinds("{{ macros.youtube_embed(id, alt=x) }}"),
            [
                OpenExpr, Whitespace, Ident, Dot, Ident, LParen, Ident, Comma, Whitespace, Ident,
                Assign, Ident, RParen, Whitespace, CloseExpr
            ]
        );
    }
}
