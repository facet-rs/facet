//! Scannerless parser smoke milestone for a small Tree-sitter grammar subset.
//!
//! This module is deliberately not Snark's runtime parser contract. It exists
//! to exercise a tiny scannerless grammar shape while the production path is
//! built as Snark grammar validation followed by Weavy lowering.

use std::fmt;

use facet::Facet;

use crate::{
    grammar::{RawGrammarJson, RawRuleJson},
    runtime_input::{ByteOffset, ByteRange},
};

/// Parser construction or parse failure.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct ParseError {
    /// Error kind.
    pub kind: ParseErrorKind,
    /// Byte offset where the error was detected.
    pub offset: ByteOffset,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} at byte {}", self.kind, self.offset.get())
    }
}

impl std::error::Error for ParseError {}

/// Parser construction or parse failure kind.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
#[repr(u8)]
pub enum ParseErrorKind {
    /// Grammar had no start rule.
    MissingStartRule,
    /// Scannerless parser was asked to parse a grammar with externals.
    ExternalScannerRequired,
    /// A symbol reference could not be resolved.
    UnknownSymbol {
        /// Missing symbol name.
        name: String,
    },
    /// A rule kind is not supported by this parser milestone.
    UnsupportedRule {
        /// Rule kind label.
        kind: &'static str,
    },
    /// A regex pattern is outside the small scannerless subset.
    UnsupportedPattern {
        /// Pattern source.
        pattern: String,
    },
    /// Input did not match the start rule.
    NoMatch,
    /// Input remained after parsing the start rule.
    TrailingInput,
}

impl fmt::Display for ParseErrorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::MissingStartRule => f.write_str("grammar has no start rule"),
            Self::ExternalScannerRequired => {
                f.write_str("grammar requires external scanner support")
            }
            Self::UnknownSymbol { name } => write!(f, "unknown symbol `{name}`"),
            Self::UnsupportedRule { kind } => write!(f, "unsupported rule kind `{kind}`"),
            Self::UnsupportedPattern { pattern } => {
                write!(f, "unsupported pattern `{pattern}`")
            }
            Self::NoMatch => f.write_str("input did not match start rule"),
            Self::TrailingInput => f.write_str("input remained after parse"),
        }
    }
}

/// Scannerless smoke parser for the initial grammar subset.
#[derive(Debug)]
pub struct ScannerlessParser<'grammar> {
    grammar: &'grammar RawGrammarJson,
    start_rule: String,
}

impl<'grammar> ScannerlessParser<'grammar> {
    /// Build a scannerless smoke parser from raw Tree-sitter grammar JSON.
    pub fn new(grammar: &'grammar RawGrammarJson) -> Result<Self, ParseError> {
        if !grammar.externals.is_empty() {
            return Err(ParseError {
                kind: ParseErrorKind::ExternalScannerRequired,
                offset: ByteOffset::new(0),
            });
        }
        let Some((start_rule, _)) = grammar.start_rule() else {
            return Err(ParseError {
                kind: ParseErrorKind::MissingStartRule,
                offset: ByteOffset::new(0),
            });
        };
        Ok(Self {
            grammar,
            start_rule: start_rule.as_str().to_owned(),
        })
    }

    /// Parse UTF-8 source text and return a named-node syntax tree.
    pub fn parse(&self, input: &str) -> Result<SyntaxNode, ParseError> {
        let start = self.skip_extras(input, 0)?;
        let Some((node, position)) = self.parse_named_rule(&self.start_rule, input, start)? else {
            return Err(ParseError {
                kind: ParseErrorKind::NoMatch,
                offset: ByteOffset::new(start as u32),
            });
        };
        let end = self.skip_extras(input, position)?;
        if end != input.len() {
            return Err(ParseError {
                kind: ParseErrorKind::TrailingInput,
                offset: ByteOffset::new(end as u32),
            });
        }
        Ok(node)
    }

    fn parse_named_rule(
        &self,
        name: &str,
        input: &str,
        position: usize,
    ) -> Result<Option<(SyntaxNode, usize)>, ParseError> {
        let rule = self.grammar.rule(name).ok_or_else(|| {
            self.error(
                position,
                ParseErrorKind::UnknownSymbol {
                    name: name.to_owned(),
                },
            )
        })?;
        let start = self.skip_extras(input, position)?;
        let Some((children, end)) = self.parse_rule(rule, input, start, true)? else {
            return Ok(None);
        };
        if name.starts_with('_') {
            return Ok(Some((SyntaxNode::anonymous(children, start, end)?, end)));
        }
        Ok(Some((
            SyntaxNode::new(name.to_owned(), start, end, children)?,
            end,
        )))
    }

    fn parse_rule(
        &self,
        rule: &RawRuleJson,
        input: &str,
        position: usize,
        allow_extras: bool,
    ) -> Result<Option<(Vec<SyntaxChild>, usize)>, ParseError> {
        let position = if allow_extras {
            self.skip_extras(input, position)?
        } else {
            position
        };

        match rule {
            RawRuleJson::Blank => Ok(Some((Vec::new(), position))),
            RawRuleJson::String { value } => {
                if input[position..].starts_with(value) {
                    Ok(Some((Vec::new(), position + value.len())))
                } else {
                    Ok(None)
                }
            }
            RawRuleJson::Pattern { value, .. } => {
                if let Some(end) = match_pattern(value, input, position) {
                    Ok(Some((Vec::new(), end)))
                } else {
                    Ok(None)
                }
            }
            RawRuleJson::Until { markers } => {
                Ok(
                    match_until_markers(markers.iter().map(String::as_str), input, position)
                        .map(|end| (Vec::new(), end)),
                )
            }
            RawRuleJson::Nested { open, close } => {
                Ok(match_nested_delimiters(open, close, input, position)
                    .map(|end| (Vec::new(), end)))
            }
            RawRuleJson::AutoClose { .. } => Ok(None),
            RawRuleJson::Symbol { name } => {
                let Some((node, end)) = self.parse_named_rule(name, input, position)? else {
                    return Ok(None);
                };
                Ok(Some((node.into_children(), end)))
            }
            RawRuleJson::Choice { members } => {
                for member in members {
                    if let Some(result) = self.parse_rule(member, input, position, allow_extras)? {
                        return Ok(Some(result));
                    }
                }
                Ok(None)
            }
            RawRuleJson::Field { name, content } => {
                let Some((children, end)) =
                    self.parse_rule(content, input, position, allow_extras)?
                else {
                    return Ok(None);
                };
                Ok(Some((
                    children
                        .into_iter()
                        .map(|child| child.with_field(name.clone()))
                        .collect(),
                    end,
                )))
            }
            RawRuleJson::Seq { members } => {
                let mut position = position;
                let mut children = Vec::new();
                for member in members {
                    let Some((mut member_children, end)) =
                        self.parse_rule(member, input, position, true)?
                    else {
                        return Ok(None);
                    };
                    position = end;
                    children.append(&mut member_children);
                }
                Ok(Some((children, position)))
            }
            RawRuleJson::Repeat { content } => {
                let mut position = position;
                let mut children = Vec::new();
                while let Some((mut item_children, end)) =
                    self.parse_rule(content, input, position, true)?
                {
                    if end == position {
                        break;
                    }
                    position = end;
                    children.append(&mut item_children);
                }
                Ok(Some((children, position)))
            }
            RawRuleJson::Repeat1 { content } => {
                let Some((mut children, mut position)) =
                    self.parse_rule(content, input, position, true)?
                else {
                    return Ok(None);
                };
                while let Some((mut item_children, end)) =
                    self.parse_rule(content, input, position, true)?
                {
                    if end == position {
                        break;
                    }
                    position = end;
                    children.append(&mut item_children);
                }
                Ok(Some((children, position)))
            }
            RawRuleJson::Token { content }
            | RawRuleJson::ImmediateToken { content }
            | RawRuleJson::Prec { content, .. }
            | RawRuleJson::PrecLeft { content, .. }
            | RawRuleJson::PrecRight { content, .. }
            | RawRuleJson::PrecDynamic { content, .. }
            | RawRuleJson::Reserved { content, .. } => {
                self.parse_rule(content, input, position, allow_extras)
            }
            RawRuleJson::Alias {
                content,
                named,
                value,
            } => {
                let Some((children, end)) =
                    self.parse_rule(content, input, position, allow_extras)?
                else {
                    return Ok(None);
                };
                if *named && children.is_empty() {
                    let node = SyntaxNode::new(value.clone(), position, end, Vec::new())?;
                    Ok(Some((node.into_children(), end)))
                } else {
                    Ok(Some((children, end)))
                }
            }
        }
    }

    fn skip_extras(&self, input: &str, position: usize) -> Result<usize, ParseError> {
        let mut position = position;
        loop {
            let mut consumed = false;
            for extra in &self.grammar.extras {
                let Some((_children, end)) = self.parse_extra(extra, input, position)? else {
                    continue;
                };
                if end > position {
                    position = end;
                    consumed = true;
                    break;
                }
            }
            if !consumed {
                return Ok(position);
            }
        }
    }

    fn parse_extra(
        &self,
        rule: &RawRuleJson,
        input: &str,
        position: usize,
    ) -> Result<Option<(Vec<SyntaxChild>, usize)>, ParseError> {
        match rule {
            RawRuleJson::String { value } => {
                if input[position..].starts_with(value) {
                    Ok(Some((Vec::new(), position + value.len())))
                } else {
                    Ok(None)
                }
            }
            RawRuleJson::Pattern { value, .. } => {
                Ok(match_pattern(value, input, position).map(|end| (Vec::new(), end)))
            }
            RawRuleJson::Until { markers } => {
                Ok(
                    match_until_markers(markers.iter().map(String::as_str), input, position)
                        .map(|end| (Vec::new(), end)),
                )
            }
            RawRuleJson::Nested { open, close } => {
                Ok(match_nested_delimiters(open, close, input, position)
                    .map(|end| (Vec::new(), end)))
            }
            RawRuleJson::AutoClose { .. } => Ok(None),
            RawRuleJson::Token { content } | RawRuleJson::ImmediateToken { content } => {
                self.parse_extra(content, input, position)
            }
            other => self.parse_rule(other, input, position, false),
        }
    }

    fn error(&self, position: usize, kind: ParseErrorKind) -> ParseError {
        ParseError {
            kind,
            offset: ByteOffset::new(position as u32),
        }
    }
}

/// Named-node syntax tree.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct SyntaxNode {
    kind: String,
    range: ByteRange,
    children: Vec<SyntaxChild>,
}

impl SyntaxNode {
    fn new(
        kind: String,
        start: usize,
        end: usize,
        children: Vec<SyntaxChild>,
    ) -> Result<Self, ParseError> {
        Ok(Self {
            kind,
            range: byte_range(start, end)?,
            children,
        })
    }

    fn anonymous(children: Vec<SyntaxChild>, start: usize, end: usize) -> Result<Self, ParseError> {
        Self::new(String::new(), start, end, children)
    }

    fn into_children(self) -> Vec<SyntaxChild> {
        if self.kind.is_empty() {
            self.children
        } else {
            vec![SyntaxChild {
                field: None,
                node: self,
            }]
        }
    }

    /// Node kind.
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// Node byte range.
    pub const fn range(&self) -> ByteRange {
        self.range
    }

    /// Named children.
    pub fn children(&self) -> &[SyntaxChild] {
        &self.children
    }

    /// Render the named-node tree as a reduced Tree-sitter-style S-expression.
    pub fn to_sexp(&self) -> String {
        let mut out = String::new();
        self.write_sexp(&mut out);
        out
    }

    fn write_sexp(&self, out: &mut String) {
        out.push('(');
        out.push_str(&self.kind);
        for child in &self.children {
            out.push(' ');
            child.write_sexp(out);
        }
        out.push(')');
    }
}

/// Child syntax node with optional field name.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct SyntaxChild {
    field: Option<String>,
    node: SyntaxNode,
}

impl SyntaxChild {
    fn with_field(mut self, field: String) -> Self {
        self.field = Some(field);
        self
    }

    /// Optional Tree-sitter field name.
    pub fn field(&self) -> Option<&str> {
        self.field.as_deref()
    }

    /// Child node.
    pub const fn node(&self) -> &SyntaxNode {
        &self.node
    }

    fn write_sexp(&self, out: &mut String) {
        if let Some(field) = &self.field {
            out.push_str(field);
            out.push_str(": ");
        }
        self.node.write_sexp(out);
    }
}

fn byte_range(start: usize, end: usize) -> Result<ByteRange, ParseError> {
    ByteRange::new(ByteOffset::new(start as u32), ByteOffset::new(end as u32)).map_err(|_| {
        ParseError {
            kind: ParseErrorKind::TrailingInput,
            offset: ByteOffset::new(start as u32),
        }
    })
}

fn match_until_markers<'a>(
    markers: impl IntoIterator<Item = &'a str>,
    input: &str,
    position: usize,
) -> Option<usize> {
    let haystack = input.get(position..)?;
    let markers = markers
        .into_iter()
        .filter(|marker| !marker.is_empty())
        .collect::<Vec<_>>();
    if markers.iter().any(|marker| haystack.starts_with(*marker)) {
        return None;
    }
    let end = markers
        .iter()
        .filter_map(|marker| haystack.find(*marker))
        .min()
        .map_or(input.len(), |offset| position + offset);
    (end > position).then_some(end)
}

fn match_nested_delimiters(open: &str, close: &str, input: &str, position: usize) -> Option<usize> {
    if open.is_empty() || close.is_empty() {
        return None;
    }
    let haystack = input.get(position..)?;
    if !haystack.starts_with(open) {
        return None;
    }
    let mut position = position + open.len();
    let mut depth = 1usize;
    while position < input.len() {
        let rest = input.get(position..)?;
        if rest.starts_with(close) {
            position += close.len();
            depth -= 1;
            if depth == 0 {
                return Some(position);
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
    Some(input.len())
}

fn match_pattern(pattern: &str, input: &str, position: usize) -> Option<usize> {
    if pattern == "\\s" {
        return input[position..]
            .chars()
            .next()
            .filter(|ch| ch.is_whitespace())
            .map(|ch| position + ch.len_utf8());
    }
    if pattern == "\\s+" {
        return match_repeating_class(input, position, |ch| ch.is_whitespace(), true);
    }
    if let Some(class) = pattern
        .strip_prefix('[')
        .and_then(|rest| rest.split_once(']'))
    {
        let (class, suffix) = class;
        if suffix == "+" {
            return match_repeating_class(
                input,
                position,
                |ch| char_class_matches(class, ch),
                true,
            );
        }
        if suffix == "*" {
            return match_repeating_class(
                input,
                position,
                |ch| char_class_matches(class, ch),
                false,
            );
        }
    }
    None
}

fn match_repeating_class(
    input: &str,
    position: usize,
    matches: impl Fn(char) -> bool,
    require_one: bool,
) -> Option<usize> {
    let mut end = position;
    let mut matched = false;
    for ch in input[position..].chars() {
        if !matches(ch) {
            break;
        }
        matched = true;
        end += ch.len_utf8();
    }
    if matched || !require_one {
        Some(end)
    } else {
        None
    }
}

fn char_class_matches(class: &str, ch: char) -> bool {
    let chars = class.chars().collect::<Vec<_>>();
    let mut index = 0;
    while index < chars.len() {
        if index + 2 < chars.len() && chars[index + 1] == '-' {
            if chars[index] <= ch && ch <= chars[index + 2] {
                return true;
            }
            index += 3;
        } else {
            if chars[index] == ch {
                return true;
            }
            index += 1;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINI_CSS_GRAMMAR: &str = r#"{
      "name": "mini_css",
      "rules": {
        "source_file": {
          "type": "REPEAT",
          "content": { "type": "SYMBOL", "name": "rule_set" }
        },
        "rule_set": {
          "type": "SEQ",
          "members": [
            { "type": "SYMBOL", "name": "selector" },
            { "type": "STRING", "value": "{" },
            { "type": "SYMBOL", "name": "declaration" },
            { "type": "STRING", "value": "}" }
          ]
        },
        "selector": { "type": "PATTERN", "value": "[a-z]+" },
        "declaration": {
          "type": "SEQ",
          "members": [
            { "type": "SYMBOL", "name": "property_name" },
            { "type": "STRING", "value": ":" },
            { "type": "SYMBOL", "name": "property_value" },
            { "type": "STRING", "value": ";" }
          ]
        },
        "property_name": { "type": "STRING", "value": "color" },
        "property_value": { "type": "STRING", "value": "red" }
      },
      "extras": [{ "type": "PATTERN", "value": "\\s" }]
    }"#;

    #[test]
    fn scannerless_parser_matches_tiny_tree_sitter_css_shape() {
        let grammar = RawGrammarJson::from_tree_sitter_json_str(MINI_CSS_GRAMMAR).unwrap();
        let parser = ScannerlessParser::new(&grammar).unwrap();

        let tree = parser.parse("a { color: red; }").unwrap();

        assert_eq!(
            tree.to_sexp(),
            "(source_file (rule_set (selector) (declaration (property_name) (property_value))))"
        );
    }

    #[test]
    fn scannerless_parser_rejects_external_scanners() {
        let grammar = RawGrammarJson::from_tree_sitter_json_str(
            r#"{
              "name": "needs_scanner",
              "rules": {
                "source_file": { "type": "SYMBOL", "name": "external_token" }
              },
              "externals": [{ "type": "SYMBOL", "name": "external_token" }]
            }"#,
        )
        .unwrap();

        assert_eq!(
            ScannerlessParser::new(&grammar).unwrap_err().kind,
            ParseErrorKind::ExternalScannerRequired
        );
    }
}
