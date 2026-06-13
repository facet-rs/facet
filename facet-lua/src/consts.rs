//! Shared Lua syntax tokens and helpers used by both the parser and serializer.

/// Lua `nil` keyword.
pub(crate) const KW_NIL: &[u8] = b"nil";
/// Lua `true` keyword.
pub(crate) const KW_TRUE: &[u8] = b"true";
/// Lua `false` keyword.
pub(crate) const KW_FALSE: &[u8] = b"false";
/// Lua positive infinity literal.
pub(crate) const MATH_HUGE: &[u8] = b"math.huge";
/// Lua NaN literal (`0/0`).
pub(crate) const NAN_LITERAL: &[u8] = b"0/0";

/// Lua reserved keywords, sorted alphabetically for binary search.
pub(crate) const LUA_KEYWORDS: &[&str] = &[
    "and", "break", "do", "else", "elseif", "end", "false", "for", "function", "goto", "if", "in",
    "local", "nil", "not", "or", "repeat", "return", "then", "true", "until", "while",
];

/// Check if a string is a Lua reserved keyword.
pub(crate) fn is_lua_keyword(name: &str) -> bool {
    LUA_KEYWORDS.binary_search(&name).is_ok()
}

/// Check if a string is a valid Lua identifier (alphanumeric/underscore, not a keyword).
pub(crate) fn is_lua_identifier(s: &str) -> bool {
    if s.is_empty() {
        return false;
    }
    let mut chars = s.chars();
    let first = chars.next().unwrap();
    if !first.is_ascii_alphabetic() && first != '_' {
        return false;
    }
    for c in chars {
        if !c.is_ascii_alphanumeric() && c != '_' {
            return false;
        }
    }
    !is_lua_keyword(s)
}
