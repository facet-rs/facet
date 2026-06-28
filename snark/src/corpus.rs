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
    /// Source input that Tree-sitter parses for this case.
    pub input: String,
    /// Expected Tree-sitter S-expression.
    pub expected: SexpNode,
}

/// Parsed S-expression node from a Tree-sitter corpus expected tree.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct SexpNode {
    /// Node kind.
    pub kind: String,
    /// Child nodes.
    #[facet(default)]
    pub children: Vec<SexpChild>,
}

/// Parsed S-expression child, optionally labeled with a Tree-sitter field.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct SexpChild {
    /// Optional field label.
    pub field: Option<String>,
    /// Child node.
    pub node: SexpNode,
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
        self.node.write_sexp(out);
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
        index += 1;

        let mut body_lines = Vec::new();
        while index < lines.len() && !is_separator(lines[index]) {
            body_lines.push(lines[index]);
            index += 1;
        }
        let body = body_lines.join("\n");
        let Some((input, expected)) = body.split_once("\n---\n") else {
            return Err(CorpusParseError::new(
                CorpusParseErrorKind::MissingInputSeparator { name },
            ));
        };
        let expected = expected.trim();
        if expected.is_empty() {
            return Err(CorpusParseError::new(
                CorpusParseErrorKind::MissingExpectedTree { name },
            ));
        }
        cases.push(CorpusCase {
            name,
            input: trim_blank_edges(input).to_owned(),
            expected: parse_sexp(expected)?,
        });
    }
    Ok(cases)
}

fn trim_blank_edges(input: &str) -> &str {
    input.trim_matches('\n')
}

fn is_separator(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.len() >= 3 && trimmed.chars().all(|ch| ch == '=')
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
        let kind = self.parse_atom()?;
        let mut children = Vec::new();
        loop {
            self.skip_ws();
            if self.consume_byte(b')') {
                break;
            }
            let field = self.parse_field_label()?;
            let node = self.parse_node()?;
            children.push(SexpChild { field, node });
        }
        Ok(SexpNode { kind, children })
    }

    fn parse_field_label(&mut self) -> Result<Option<String>, CorpusParseError> {
        let checkpoint = self.position;
        let Some(atom) = self.try_parse_atom()? else {
            return Ok(None);
        };
        if let Some(field) = atom.strip_suffix(':') {
            self.skip_ws();
            if self.peek_byte() == Some(b'(') {
                return Ok(Some(field.to_owned()));
            }
        }
        self.position = checkpoint;
        Ok(None)
    }

    fn try_parse_atom(&mut self) -> Result<Option<String>, CorpusParseError> {
        self.skip_ws();
        if self.is_eof() || matches!(self.peek_byte(), Some(b'(' | b')')) {
            return Ok(None);
        }
        Ok(Some(self.parse_atom()?))
    }

    fn parse_atom(&mut self) -> Result<String, CorpusParseError> {
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
        Ok(self.source[start..self.position].to_owned())
    }

    fn parse_quoted_atom(&mut self) -> Result<String, CorpusParseError> {
        self.expect_byte(b'"')?;
        let mut out = String::new();
        while let Some(byte) = self.peek_byte() {
            self.position += 1;
            match byte {
                b'"' => return Ok(format!("\"{out}\"")),
                b'\\' => {
                    let Some(escaped) = self.peek_byte() else {
                        return Err(self.error("unterminated escape"));
                    };
                    self.position += 1;
                    out.push('\\');
                    out.push(char::from(escaped));
                }
                _ => out.push(char::from(byte)),
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
}
