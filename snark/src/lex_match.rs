use aho_corasick::{AhoCorasick, AhoCorasickBuilder, MatchKind};
use regex::Regex;

use crate::parser::LexMatch;

pub(crate) const GINGEMBRE_IDENTIFIER_PATTERN: &str = "(?!if\\b|elif\\b|else\\b|endif\\b|for\\b|endfor\\b|set\\b|endset\\b|block\\b|endblock\\b|extends\\b|include\\b|import\\b|macro\\b|endmacro\\b|break\\b|continue\\b|as\\b|in\\b|is\\b|not\\b|and\\b|or\\b|true\\b|True\\b|false\\b|False\\b|none\\b|None\\b)[A-Za-z_][A-Za-z0-9_]*";

#[derive(Debug, Clone)]
pub(crate) struct CompiledPattern {
    pub(crate) source: String,
    pub(crate) flags: Option<String>,
    pub(crate) regex: Option<Regex>,
}

#[derive(Debug, Clone)]
pub(crate) struct CompiledUntilMatcher {
    pub(crate) markers: Vec<String>,
    automaton: Option<AhoCorasick>,
}

pub(crate) fn compile_pattern(pattern: &str, flags: Option<&str>) -> CompiledPattern {
    CompiledPattern {
        source: pattern.to_owned(),
        flags: normalized_regex_flags(flags),
        regex: compile_regex_leaf(pattern, flags),
    }
}

pub(crate) fn compile_until_markers(markers: &[String]) -> CompiledUntilMatcher {
    let markers = markers
        .iter()
        .filter(|marker| !marker.is_empty())
        .cloned()
        .collect::<Vec<_>>();
    let automaton = if markers.is_empty() {
        None
    } else {
        AhoCorasickBuilder::new()
            .match_kind(MatchKind::LeftmostFirst)
            .build(&markers)
            .ok()
    };
    CompiledUntilMatcher { markers, automaton }
}

#[cfg(test)]
pub(crate) fn match_pattern(pattern: &str, input: &str, byte_position: usize) -> Option<usize> {
    match_pattern_with_flags(pattern, None, input, byte_position)
}

#[cfg(test)]
pub(crate) fn match_pattern_with_flags(
    pattern: &str,
    flags: Option<&str>,
    input: &str,
    byte_position: usize,
) -> Option<usize> {
    if regex_flags_are_empty(flags)
        && let Some(result) = match_known_pattern(pattern, input, byte_position)
    {
        return result;
    }
    match_cached_regex_leaf(pattern, flags, input, byte_position)
}

pub(crate) fn match_compiled_pattern(
    pattern: &CompiledPattern,
    input: &str,
    byte_position: usize,
) -> Option<LexMatch> {
    if pattern.flags.is_none()
        && let Some(result) = match_known_pattern(&pattern.source, input, byte_position)
    {
        return result.map(|end| LexMatch::new(end, pattern_inspected_end(input, end)));
    }
    let haystack = input.get(byte_position..)?;
    pattern
        .regex
        .as_ref()?
        .find(haystack)
        .filter(|match_| match_.start() == 0)
        .map(|match_| {
            let end = byte_position + match_.end();
            LexMatch::new(end, pattern_inspected_end(input, end))
        })
}

#[cfg(test)]
pub(crate) fn match_until_markers_with_inspection<'a>(
    markers: impl IntoIterator<Item = &'a str>,
    input: &str,
    byte_position: usize,
) -> Option<LexMatch> {
    let haystack = input.get(byte_position..)?;
    let markers = markers
        .into_iter()
        .filter(|marker| !marker.is_empty())
        .collect::<Vec<_>>();
    if markers.iter().any(|marker| haystack.starts_with(*marker)) {
        return None;
    }
    let end_and_marker_len = markers
        .iter()
        .filter_map(|marker| haystack.find(*marker).map(|offset| (offset, marker.len())))
        .min()
        .map_or((input.len() - byte_position, 0), |pair| pair);
    let end = byte_position + end_and_marker_len.0;
    let inspected_end = end + end_and_marker_len.1;
    (end > byte_position).then_some(LexMatch::new(end, inspected_end))
}

pub(crate) fn match_compiled_until_markers_with_inspection(
    matcher: &CompiledUntilMatcher,
    input: &str,
    byte_position: usize,
) -> Option<LexMatch> {
    let haystack = input.get(byte_position..)?;
    if matcher
        .markers
        .iter()
        .any(|marker| haystack.starts_with(marker.as_str()))
    {
        return None;
    }
    let Some(automaton) = &matcher.automaton else {
        return (byte_position < input.len()).then_some(LexMatch::new(input.len(), input.len()));
    };
    let Some(match_) = automaton.find(haystack) else {
        return (byte_position < input.len()).then_some(LexMatch::new(input.len(), input.len()));
    };
    let end = byte_position + match_.start();
    let marker_len = matcher
        .markers
        .iter()
        .filter(|marker| haystack[match_.start()..].starts_with(marker.as_str()))
        .map(String::len)
        .min()
        .unwrap_or(match_.len());
    (end > byte_position).then_some(LexMatch::new(end, end + marker_len))
}

pub(crate) fn match_nested_delimiters_with_inspection(
    open: &str,
    close: &str,
    input: &str,
    byte_position: usize,
) -> Option<LexMatch> {
    if open.is_empty() || close.is_empty() {
        return None;
    }
    let haystack = input.get(byte_position..)?;
    if !haystack.starts_with(open) {
        return None;
    }
    let mut position = byte_position + open.len();
    let mut depth = 1usize;
    while position < input.len() {
        let rest = input.get(position..)?;
        if rest.starts_with(close) {
            position += close.len();
            depth -= 1;
            if depth == 0 {
                return Some(LexMatch::new(position, position));
            }
            continue;
        }
        if rest.starts_with(open) {
            position += open.len();
            depth += 1;
            continue;
        }
        position += rest.chars().next()?.len_utf8();
    }
    Some(LexMatch::new(input.len(), input.len()))
}

fn match_known_pattern(pattern: &str, input: &str, byte_position: usize) -> Option<Option<usize>> {
    match pattern {
        "-?(\\d)*n\\s*(\\+\\s*\\d+)?" => {
            Some(match_css_nth_functional_notation(input, byte_position))
        }
        GINGEMBRE_IDENTIFIER_PATTERN => Some(match_gingembre_identifier(input, byte_position)),
        "[0-9a-fA-F]{1,6}\\s?" => Some(match_css_hex_escape_tail(input, byte_position)),
        ".*" => Some(match_json_line_comment_tail(input, byte_position)),
        "[^*]*\\*+([^/*][^*]*\\*+)*" => Some(match_json_block_comment_body(input, byte_position)),
        "(--|-?[a-zA-Z_\\xA0-\\xFF])[a-zA-Z0-9-_\\xA0-\\xFF]*" => {
            Some(match_css_identifier(input, byte_position))
        }
        "and\\b" => Some(match_ascii_keyword(input, byte_position, "and")),
        "in\\b" => Some(match_ascii_keyword(input, byte_position, "in")),
        "is\\b" => Some(match_ascii_keyword(input, byte_position, "is")),
        "not\\b" => Some(match_ascii_keyword(input, byte_position, "not")),
        "or\\b" => Some(match_ascii_keyword(input, byte_position, "or")),
        _ => None,
    }
}

fn pattern_inspected_end(input: &str, end: usize) -> usize {
    if end >= input.len() {
        return input.len();
    }
    input[end..]
        .chars()
        .next()
        .map_or(end, |ch| end + ch.len_utf8())
}

#[cfg(test)]
fn match_cached_regex_leaf(
    pattern: &str,
    flags: Option<&str>,
    input: &str,
    byte_position: usize,
) -> Option<usize> {
    let haystack = input.get(byte_position..)?;
    let regex = cached_regex(pattern, flags)?;
    regex
        .find(haystack)
        .filter(|match_| match_.start() == 0)
        .map(|match_| byte_position + match_.end())
}

#[cfg(test)]
fn cached_regex(pattern: &str, flags: Option<&str>) -> Option<Regex> {
    use std::{
        collections::HashMap,
        sync::{Mutex, OnceLock},
    };

    type RegexCache = HashMap<(String, Option<String>), Option<Regex>>;

    static CACHE: OnceLock<Mutex<RegexCache>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let key = (pattern.to_owned(), normalized_regex_flags(flags));

    {
        let cache = cache.lock().expect("regex cache poisoned");
        if let Some(regex) = cache.get(&key) {
            return regex.clone();
        }
    }

    let compiled = compile_regex_leaf(pattern, flags);
    let mut cache = cache.lock().expect("regex cache poisoned");
    let entry = cache.entry(key).or_insert_with(|| compiled.clone());
    entry.clone()
}

fn compile_regex_leaf(pattern: &str, flags: Option<&str>) -> Option<Regex> {
    Regex::new(&anchored_regex_source(pattern, flags)?).ok()
}

fn anchored_regex_source(pattern: &str, flags: Option<&str>) -> Option<String> {
    let body = rust_regex_source(pattern);
    let flags = rust_regex_flags(flags)?;
    Some(if flags.is_empty() {
        format!("\\A(?:{})", body)
    } else {
        format!("\\A(?{}:{})", flags, body)
    })
}

pub(crate) fn normalized_regex_flags(flags: Option<&str>) -> Option<String> {
    flags.filter(|flags| !flags.is_empty()).map(str::to_owned)
}

#[cfg(test)]
fn regex_flags_are_empty(flags: Option<&str>) -> bool {
    flags.is_none_or(str::is_empty)
}

fn rust_regex_flags(flags: Option<&str>) -> Option<String> {
    let mut rust_flags = String::new();
    for flag in flags.unwrap_or("").chars() {
        match flag {
            'i' | 'm' | 's' if !rust_flags.contains(flag) => rust_flags.push(flag),
            'i' | 'm' | 's' | 'u' | 'g' | 'y' | 'd' => {}
            _ => return None,
        }
    }
    Some(rust_flags)
}

fn rust_regex_source(pattern: &str) -> String {
    let mut out = String::with_capacity(pattern.len());
    let mut chars = pattern.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }

        let Some(escaped) = chars.next() else {
            out.push('\\');
            break;
        };

        if escaped == '/' {
            out.push('/');
            continue;
        }

        if escaped == 'u' {
            let mut hex = String::with_capacity(4);
            for _ in 0..4 {
                let Some(hex_ch) = chars.peek().copied().filter(|ch| ch.is_ascii_hexdigit()) else {
                    out.push('\\');
                    out.push('u');
                    out.push_str(&hex);
                    out.extend(chars);
                    return out;
                };
                chars.next();
                hex.push(hex_ch);
            }
            out.push_str("\\u{");
            out.push_str(&hex);
            out.push('}');
            continue;
        }

        out.push('\\');
        out.push(escaped);
    }
    out
}

fn match_ascii_keyword(input: &str, byte_position: usize, keyword: &str) -> Option<usize> {
    if !input[byte_position..].starts_with(keyword) {
        return None;
    }
    let end = byte_position + keyword.len();
    if input
        .as_bytes()
        .get(end..)?
        .first()
        .is_some_and(|byte| *byte == b'_' || byte.is_ascii_alphanumeric())
    {
        return None;
    }
    Some(end)
}

fn match_gingembre_identifier(input: &str, byte_position: usize) -> Option<usize> {
    let end = match_ascii_identifier(input, byte_position)?;
    let word = &input[byte_position..end];
    if is_gingembre_keyword(word) {
        return None;
    }
    Some(end)
}

fn match_ascii_identifier(input: &str, byte_position: usize) -> Option<usize> {
    let bytes = input.as_bytes().get(byte_position..)?;
    let first = bytes.first().copied()?;
    if first != b'_' && !first.is_ascii_alphabetic() {
        return None;
    }
    let len = bytes
        .iter()
        .take_while(|byte| **byte == b'_' || byte.is_ascii_alphanumeric())
        .count();
    Some(byte_position + len)
}

fn is_gingembre_keyword(word: &str) -> bool {
    matches!(
        word,
        "if" | "elif"
            | "else"
            | "endif"
            | "for"
            | "endfor"
            | "set"
            | "endset"
            | "block"
            | "endblock"
            | "extends"
            | "include"
            | "import"
            | "macro"
            | "endmacro"
            | "break"
            | "continue"
            | "as"
            | "in"
            | "is"
            | "not"
            | "and"
            | "or"
            | "true"
            | "True"
            | "false"
            | "False"
            | "none"
            | "None"
    )
}

fn match_json_line_comment_tail(input: &str, byte_position: usize) -> Option<usize> {
    Some(
        input[byte_position..]
            .find(['\n', '\r'])
            .map_or(input.len(), |offset| byte_position + offset),
    )
}

fn match_json_block_comment_body(input: &str, byte_position: usize) -> Option<usize> {
    input[byte_position..]
        .find("*/")
        .map(|offset| byte_position + offset + 1)
}

fn match_while(
    input: &str,
    byte_position: usize,
    predicate: impl Fn(char) -> bool,
    min_chars: usize,
) -> Option<usize> {
    let mut position = byte_position;
    let mut count = 0usize;
    for ch in input[byte_position..].chars() {
        if !predicate(ch) {
            break;
        }
        position += ch.len_utf8();
        count += 1;
    }
    (count >= min_chars).then_some(position)
}

fn match_css_identifier(input: &str, byte_position: usize) -> Option<usize> {
    let rest = &input[byte_position..];
    if rest.starts_with("--") {
        let mut position = byte_position + 2;
        while let Some(ch) = input[position..]
            .chars()
            .next()
            .filter(|ch| css_ident_continue(*ch))
        {
            position += ch.len_utf8();
        }
        return Some(position);
    }
    let mut chars = rest.char_indices();
    let (first_offset, first) = chars.next()?;
    debug_assert_eq!(first_offset, 0);
    let mut position = byte_position;
    if first == '-' {
        position += first.len_utf8();
        let next = input[position..].chars().next()?;
        if !css_ident_start(next) {
            return None;
        }
        position += next.len_utf8();
    } else if css_ident_start(first) {
        position += first.len_utf8();
    } else {
        return None;
    }
    while let Some(ch) = input[position..]
        .chars()
        .next()
        .filter(|ch| css_ident_continue(*ch))
    {
        position += ch.len_utf8();
    }
    Some(position)
}

fn match_css_hex_escape_tail(input: &str, byte_position: usize) -> Option<usize> {
    let mut position = byte_position;
    let mut count = 0usize;
    while count < 6 {
        let Some(ch) = input[position..]
            .chars()
            .next()
            .filter(|ch| ch.is_ascii_hexdigit())
        else {
            break;
        };
        position += ch.len_utf8();
        count += 1;
    }
    if count == 0 {
        return None;
    }
    if let Some(ch) = input[position..]
        .chars()
        .next()
        .filter(|ch| ch.is_whitespace())
    {
        position += ch.len_utf8();
    }
    Some(position)
}

fn match_css_nth_functional_notation(input: &str, byte_position: usize) -> Option<usize> {
    let mut position = byte_position;
    if input[position..].starts_with('-') {
        position += '-'.len_utf8();
    }
    while let Some(ch) = input[position..]
        .chars()
        .next()
        .filter(|ch| ch.is_ascii_digit())
    {
        position += ch.len_utf8();
    }
    if !input[position..].starts_with('n') {
        return None;
    }
    position += 'n'.len_utf8();
    position = skip_pattern_whitespace(input, position);
    if input[position..].starts_with('+') {
        position += '+'.len_utf8();
        position = skip_pattern_whitespace(input, position);
        let digits = match_while(input, position, |ch| ch.is_ascii_digit(), 1)?;
        position = digits;
    }
    Some(position)
}

fn css_ident_start(ch: char) -> bool {
    ch.is_ascii_alphabetic() || ch == '_' || !ch.is_ascii()
}

fn css_ident_continue(ch: char) -> bool {
    css_ident_start(ch) || ch.is_ascii_digit() || ch == '-'
}

fn skip_pattern_whitespace(input: &str, byte_position: usize) -> usize {
    let mut position = byte_position;
    while let Some(ch) = input[position..]
        .chars()
        .next()
        .filter(|ch| ch.is_whitespace())
    {
        position += ch.len_utf8();
    }
    position
}
