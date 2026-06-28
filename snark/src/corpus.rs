//! Imported Tree-sitter corpus and highlight fixture files.

use std::{error::Error, fmt};

use facet::Facet;

use crate::source::SourceFile;

/// Raw Tree-sitter corpus or highlight fixture source.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct CorpusSource(pub String);

/// Imported corpus or highlight fixture.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct CorpusFixture {
    /// Fixture kind.
    pub kind: CorpusKind,
    /// Fixture source file.
    pub source: SourceFile<CorpusSource>,
}

impl CorpusFixture {
    /// Parse this fixture as a Tree-sitter parse corpus file.
    pub fn parse_cases(&self) -> Result<Vec<CorpusCase>, CorpusParseError> {
        if self.kind != CorpusKind::Parse {
            return Err(CorpusParseError::new(CorpusParseErrorKind::WrongKind {
                kind: self.kind,
            }));
        }
        parse_corpus_cases(&self.source.body.0)
    }
}

/// Supported fixture categories.
#[derive(Debug, Clone, Copy, Facet, PartialEq, Eq)]
#[repr(u8)]
pub enum CorpusKind {
    /// Tree-sitter parse corpus fixture from `test/corpus`.
    Parse,
    /// Highlight fixture from `test/highlight` or legacy `test/highlights`.
    Highlight,
}

/// One named Tree-sitter corpus case.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct CorpusCase {
    /// Human-readable case name from the corpus separator header.
    pub name: String,
    /// Tree-sitter corpus attributes declared in the case header.
    #[facet(default)]
    pub attributes: Vec<CorpusAttribute>,
    /// Source input that Tree-sitter parses for this case.
    pub input: String,
    /// Expected Tree-sitter S-expression.
    pub expected: SexpNode,
}

/// Attribute declared in a Tree-sitter corpus case header.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct CorpusAttribute {
    /// Attribute name.
    pub name: String,
    /// Optional attribute value.
    pub value: Option<String>,
}

/// Parsed S-expression node from a Tree-sitter corpus expected tree.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct SexpNode {
    /// Node kind.
    pub kind: String,
    /// Child values.
    #[facet(default)]
    pub children: Vec<SexpChild>,
}

/// Parsed S-expression child value, optionally labeled with a Tree-sitter field.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct SexpChild {
    /// Optional field label.
    pub field: Option<String>,
    /// Child value.
    pub value: SexpValue,
}

/// Parsed S-expression value.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
#[repr(u8)]
pub enum SexpValue {
    /// Nested named node.
    Node(SexpNode),
    /// Atom child, used for anonymous terminals such as `MISSING ";"`.
    Atom(SexpAtom),
}

/// Parsed S-expression atom.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
#[repr(u8)]
pub enum SexpAtom {
    /// Unquoted atom.
    Bare(String),
    /// Quoted anonymous terminal, without surrounding quotes.
    Quoted(String),
}

impl SexpNode {
    /// Render this node back to a normalized S-expression.
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

impl SexpChild {
    fn write_sexp(&self, out: &mut String) {
        if let Some(field) = &self.field {
            out.push_str(field);
            out.push_str(": ");
        }
        self.value.write_sexp(out);
    }
}

impl SexpValue {
    fn write_sexp(&self, out: &mut String) {
        match self {
            Self::Node(node) => node.write_sexp(out),
            Self::Atom(atom) => atom.write_sexp(out),
        }
    }
}

impl SexpAtom {
    fn write_sexp(&self, out: &mut String) {
        match self {
            Self::Bare(atom) => out.push_str(atom),
            Self::Quoted(atom) => write_quoted_atom(atom, out),
        }
    }
}

/// Error while parsing Tree-sitter corpus fixtures.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct CorpusParseError {
    /// Error kind.
    pub kind: CorpusParseErrorKind,
}

impl CorpusParseError {
    fn new(kind: CorpusParseErrorKind) -> Self {
        Self { kind }
    }
}

impl fmt::Display for CorpusParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            CorpusParseErrorKind::WrongKind { kind } => {
                write!(f, "cannot parse {kind:?} fixture as parse corpus")
            }
            CorpusParseErrorKind::MissingCaseName { line } => {
                write!(f, "corpus case at line {line} is missing a name")
            }
            CorpusParseErrorKind::MissingInputSeparator { name } => {
                write!(f, "corpus case `{name}` is missing --- separator")
            }
            CorpusParseErrorKind::MissingExpectedTree { name } => {
                write!(f, "corpus case `{name}` is missing an expected tree")
            }
            CorpusParseErrorKind::Sexp { message, offset } => {
                write!(
                    f,
                    "could not parse expected S-expression at byte {offset}: {message}"
                )
            }
        }
    }
}

impl Error for CorpusParseError {}

/// Error kind while parsing Tree-sitter corpus fixtures.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
#[repr(u8)]
pub enum CorpusParseErrorKind {
    /// Fixture was not a parse corpus file.
    WrongKind {
        /// Actual fixture kind.
        kind: CorpusKind,
    },
    /// A separator header did not contain a case name.
    MissingCaseName {
        /// One-based line number of the separator.
        line: usize,
    },
    /// A case did not contain the `---` input/expected separator.
    MissingInputSeparator {
        /// Case name.
        name: String,
    },
    /// A case did not contain an expected S-expression.
    MissingExpectedTree {
        /// Case name.
        name: String,
    },
    /// Expected tree S-expression parsing failed.
    Sexp {
        /// Error message.
        message: String,
        /// Byte offset inside the expected S-expression.
        offset: usize,
    },
}

fn parse_corpus_cases(source: &str) -> Result<Vec<CorpusCase>, CorpusParseError> {
    let lines = source.lines().collect::<Vec<_>>();
    let mut cases = Vec::new();
    let mut index = 0;
    while index < lines.len() {
        if !is_separator(lines[index]) {
            index += 1;
            continue;
        }
        let separator_line = index + 1;
        index += 1;

        let mut name_lines = Vec::new();
        while index < lines.len() && !is_separator(lines[index]) {
            name_lines.push(lines[index]);
            index += 1;
        }
        if index >= lines.len() {
            return Err(CorpusParseError::new(
                CorpusParseErrorKind::MissingCaseName {
                    line: separator_line,
                },
            ));
        }
        let name = name_lines.join("\n").trim().to_owned();
        if name.is_empty() {
            return Err(CorpusParseError::new(
                CorpusParseErrorKind::MissingCaseName {
                    line: separator_line,
                },
            ));
        }
        let (name, attributes) = parse_case_header(&name);
        index += 1;

        let mut input_lines = Vec::new();
        while index < lines.len()
            && !is_separator(lines[index])
            && !is_input_separator(lines[index])
        {
            input_lines.push(lines[index]);
            index += 1;
        }
        if index >= lines.len() || is_separator(lines[index]) {
            return Err(CorpusParseError::new(
                CorpusParseErrorKind::MissingInputSeparator { name },
            ));
        }
        index += 1;

        let mut expected_lines = Vec::new();
        while index < lines.len() && !is_separator(lines[index]) {
            expected_lines.push(lines[index]);
            index += 1;
        }
        let input = input_lines.join("\n");
        let expected = expected_lines.join("\n");
        let expected = expected.trim();
        if expected.is_empty() {
            return Err(CorpusParseError::new(
                CorpusParseErrorKind::MissingExpectedTree { name },
            ));
        }
        cases.push(CorpusCase {
            name,
            attributes,
            input: trim_blank_edges(&input).to_owned(),
            expected: parse_sexp(expected)?,
        });
    }
    Ok(cases)
}

fn parse_case_header(header: &str) -> (String, Vec<CorpusAttribute>) {
    let mut name_lines = Vec::new();
    let mut attributes = Vec::new();
    for line in header.lines() {
        let trimmed = line.trim();
        if let Some(attribute) = parse_attribute_line(trimmed) {
            attributes.push(attribute);
        } else {
            name_lines.push(line);
        }
    }
    (name_lines.join("\n").trim().to_owned(), attributes)
}

fn parse_attribute_line(line: &str) -> Option<CorpusAttribute> {
    let body = line.strip_prefix(':')?;
    let (name, value) = body
        .split_once(' ')
        .or_else(|| body.split_once('\t'))
        .map_or((body, None), |(name, value)| (name, Some(value.trim())));
    let name = name.trim();
    if name.is_empty() {
        return None;
    }
    Some(CorpusAttribute {
        name: name.to_owned(),
        value: value.filter(|value| !value.is_empty()).map(str::to_owned),
    })
}

fn trim_blank_edges(input: &str) -> &str {
    input.trim_matches('\n')
}

fn is_separator(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.len() >= 3 && trimmed.chars().all(|ch| ch == '=')
}

fn is_input_separator(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.len() >= 3 && trimmed.chars().all(|ch| ch == '-')
}

fn parse_sexp(source: &str) -> Result<SexpNode, CorpusParseError> {
    let mut parser = SexpParser::new(source);
    let node = parser.parse_node()?;
    parser.skip_ws();
    if !parser.is_eof() {
        return Err(parser.error("trailing tokens after S-expression"));
    }
    Ok(node)
}

struct SexpParser<'source> {
    source: &'source str,
    position: usize,
}

impl<'source> SexpParser<'source> {
    fn new(source: &'source str) -> Self {
        Self {
            source,
            position: 0,
        }
    }

    fn parse_node(&mut self) -> Result<SexpNode, CorpusParseError> {
        self.skip_ws();
        self.expect_byte(b'(')?;
        let kind = self.parse_node_kind()?;
        let mut children = Vec::new();
        loop {
            self.skip_ws();
            if self.consume_byte(b')') {
                break;
            }
            let field = self.parse_field_label()?;
            let value = self.parse_value()?;
            children.push(SexpChild { field, value });
        }
        Ok(SexpNode { kind, children })
    }

    fn parse_node_kind(&mut self) -> Result<String, CorpusParseError> {
        match self.parse_atom()? {
            SexpAtom::Bare(kind) => Ok(kind),
            SexpAtom::Quoted(_) => Err(self.error("expected node kind")),
        }
    }

    fn parse_value(&mut self) -> Result<SexpValue, CorpusParseError> {
        self.skip_ws();
        if self.peek_byte() == Some(b'(') {
            return Ok(SexpValue::Node(self.parse_node()?));
        }
        Ok(SexpValue::Atom(self.parse_atom()?))
    }

    fn parse_field_label(&mut self) -> Result<Option<String>, CorpusParseError> {
        let checkpoint = self.position;
        let Some(atom) = self.try_parse_bare_atom()? else {
            return Ok(None);
        };
        if let SexpAtom::Bare(atom) = atom
            && let Some(field) = atom.strip_suffix(':')
        {
            self.skip_ws();
            if !matches!(self.peek_byte(), None | Some(b')')) {
                return Ok(Some(field.to_owned()));
            }
        }
        self.position = checkpoint;
        Ok(None)
    }

    fn try_parse_bare_atom(&mut self) -> Result<Option<SexpAtom>, CorpusParseError> {
        self.skip_ws();
        if self.is_eof() || matches!(self.peek_byte(), Some(b'(' | b')' | b'"')) {
            return Ok(None);
        }
        Ok(Some(self.parse_atom()?))
    }

    fn parse_atom(&mut self) -> Result<SexpAtom, CorpusParseError> {
        self.skip_ws();
        if self.peek_byte() == Some(b'"') {
            return self.parse_quoted_atom();
        }
        let start = self.position;
        while let Some(byte) = self.peek_byte() {
            if byte.is_ascii_whitespace() || matches!(byte, b'(' | b')') {
                break;
            }
            self.position += 1;
        }
        if self.position == start {
            return Err(self.error("expected atom"));
        }
        Ok(SexpAtom::Bare(self.source[start..self.position].to_owned()))
    }

    fn parse_quoted_atom(&mut self) -> Result<SexpAtom, CorpusParseError> {
        self.expect_byte(b'"')?;
        let mut out = String::new();
        while let Some(ch) = self.peek_char() {
            self.position += ch.len_utf8();
            match ch {
                '"' => return Ok(SexpAtom::Quoted(out)),
                '\\' => {
                    let Some(escaped) = self.peek_char() else {
                        return Err(self.error("unterminated escape"));
                    };
                    self.position += escaped.len_utf8();
                    out.push('\\');
                    out.push(escaped);
                }
                _ => out.push(ch),
            }
        }
        Err(self.error("unterminated quoted atom"))
    }

    fn expect_byte(&mut self, expected: u8) -> Result<(), CorpusParseError> {
        self.skip_ws();
        if self.consume_byte(expected) {
            Ok(())
        } else {
            Err(self.error(format!("expected `{}`", char::from(expected))))
        }
    }

    fn consume_byte(&mut self, expected: u8) -> bool {
        if self.peek_byte() == Some(expected) {
            self.position += 1;
            true
        } else {
            false
        }
    }

    fn skip_ws(&mut self) {
        while let Some(byte) = self.peek_byte() {
            if !byte.is_ascii_whitespace() {
                break;
            }
            self.position += 1;
        }
    }

    fn peek_byte(&self) -> Option<u8> {
        self.source.as_bytes().get(self.position).copied()
    }

    fn peek_char(&self) -> Option<char> {
        self.source[self.position..].chars().next()
    }

    fn is_eof(&self) -> bool {
        self.position >= self.source.len()
    }

    fn error(&self, message: impl Into<String>) -> CorpusParseError {
        CorpusParseError::new(CorpusParseErrorKind::Sexp {
            message: message.into(),
            offset: self.position,
        })
    }
}

fn write_quoted_atom(atom: &str, out: &mut String) {
    out.push('"');
    for ch in atom.chars() {
        match ch {
            '"' | '\\' => {
                out.push('\\');
                out.push(ch);
            }
            _ => out.push(ch),
        }
    }
    out.push('"');
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::source::{PackageRelativePath, SourceFile, SourceId};

    fn fixture(source: &str) -> CorpusFixture {
        CorpusFixture {
            kind: CorpusKind::Parse,
            source: SourceFile {
                id: SourceId::for_test(0),
                path: PackageRelativePath::new("test/corpus/example.txt").unwrap(),
                body: CorpusSource(source.to_owned()),
            },
        }
    }

    #[test]
    fn parses_tree_sitter_corpus_cases() {
        let corpus = fixture(
            "====================\nRule sets\n====================\n\n#some-id {\n  some-property: 5px;\n}\n\n---\n\n(stylesheet\n  (rule_set\n    (selectors (id_selector (id_name)))\n    (block\n      (declaration (property_name) (integer_value (unit))))))\n",
        );

        let cases = corpus.parse_cases().unwrap();

        assert_eq!(cases.len(), 1);
        assert_eq!(cases[0].name, "Rule sets");
        assert!(cases[0].attributes.is_empty());
        assert_eq!(cases[0].input, "#some-id {\n  some-property: 5px;\n}");
        assert_eq!(
            cases[0].expected.to_sexp(),
            "(stylesheet (rule_set (selectors (id_selector (id_name))) (block (declaration (property_name) (integer_value (unit))))))"
        );
    }

    #[test]
    fn parses_field_labeled_sexp_children() {
        let node =
            parse_sexp("(declaration property: (property_name) value: (integer_value))").unwrap();

        assert_eq!(node.children[0].field.as_deref(), Some("property"));
        assert_eq!(node.children[1].field.as_deref(), Some("value"));
    }

    #[test]
    fn parses_long_hyphen_input_separators_and_attributes() {
        let corpus = fixture(
            "====================\nCase name\n:skip\n:platform linux\n====================\n\ninput\n\n--------------------------------------------------------------------------------\n\n(document)\n",
        );

        let cases = corpus.parse_cases().unwrap();

        assert_eq!(cases[0].name, "Case name");
        assert_eq!(
            cases[0].attributes,
            [
                CorpusAttribute {
                    name: "skip".to_owned(),
                    value: None,
                },
                CorpusAttribute {
                    name: "platform".to_owned(),
                    value: Some("linux".to_owned()),
                },
            ]
        );
        assert_eq!(cases[0].input, "input");
    }

    #[test]
    fn parses_atom_children_and_quoted_anonymous_tokens() {
        let node = parse_sexp(r#"(ERROR (MISSING ";") "}")"#).unwrap();

        assert_eq!(node.to_sexp(), r#"(ERROR (MISSING ";") "}")"#);
        assert_eq!(
            node.children[0].value,
            SexpValue::Node(SexpNode {
                kind: "MISSING".to_owned(),
                children: vec![SexpChild {
                    field: None,
                    value: SexpValue::Atom(SexpAtom::Quoted(";".to_owned())),
                }],
            })
        );
        assert_eq!(
            node.children[1].value,
            SexpValue::Atom(SexpAtom::Quoted("}".to_owned()))
        );
    }

    #[test]
    fn preserves_non_ascii_quoted_atoms() {
        let node = parse_sexp(r#"(ERROR "é")"#).unwrap();

        assert_eq!(node.to_sexp(), r#"(ERROR "é")"#);
    }
}
