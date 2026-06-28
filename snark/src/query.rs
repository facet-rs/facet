//! Imported Tree-sitter query files.

use std::collections::BTreeSet;

use facet::Facet;

use crate::source::SourceFile;

/// Raw Tree-sitter query source.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct QuerySource(pub String);

impl QuerySource {
    /// Quoted anonymous node literals referenced by this query source.
    ///
    /// This is an oracle/import fact, not a query evaluator. Predicate forms
    /// such as `(#match? @capture "regex")` are skipped structurally.
    pub fn anonymous_node_literals(&self) -> BTreeSet<String> {
        anonymous_node_literals(&self.0)
    }
}

/// Quoted anonymous node literals referenced by a Tree-sitter query source.
pub fn anonymous_node_literals(query: &str) -> BTreeSet<String> {
    let mut scanner = QueryScanner::new(query);
    let mut contexts = vec![QueryContext::Root];
    let mut literals = BTreeSet::new();
    while let Some(token) = scanner.next_token() {
        match token {
            QueryToken::OpenParen => contexts.push(QueryContext::Form {
                seen_head: false,
                predicate: false,
            }),
            QueryToken::CloseParen => {
                if contexts.len() > 1 {
                    contexts.pop();
                }
            }
            QueryToken::OpenBracket => contexts.push(QueryContext::List),
            QueryToken::CloseBracket => {
                if contexts.len() > 1 {
                    contexts.pop();
                }
            }
            QueryToken::Symbol(symbol) => {
                if let Some(QueryContext::Form {
                    seen_head,
                    predicate,
                }) = contexts.last_mut()
                {
                    if !*seen_head {
                        *predicate = symbol.starts_with('#');
                        *seen_head = true;
                    }
                }
            }
            QueryToken::String(literal) => {
                if let Some(QueryContext::Form { seen_head, .. }) = contexts.last_mut() {
                    *seen_head = true;
                }
                if !contexts.iter().any(QueryContext::is_predicate) {
                    literals.insert(literal);
                }
            }
        }
    }
    literals
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum QueryContext {
    Root,
    List,
    Form { seen_head: bool, predicate: bool },
}

impl QueryContext {
    const fn is_predicate(&self) -> bool {
        matches!(
            self,
            Self::Form {
                predicate: true,
                ..
            }
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum QueryToken {
    OpenParen,
    CloseParen,
    OpenBracket,
    CloseBracket,
    String(String),
    Symbol(String),
}

struct QueryScanner<'a> {
    source: &'a str,
    index: usize,
}

impl<'a> QueryScanner<'a> {
    const fn new(source: &'a str) -> Self {
        Self { source, index: 0 }
    }

    fn next_token(&mut self) -> Option<QueryToken> {
        self.skip_ws_and_comments();
        let ch = self.peek_char()?;
        match ch {
            '(' => {
                self.index += ch.len_utf8();
                Some(QueryToken::OpenParen)
            }
            ')' => {
                self.index += ch.len_utf8();
                Some(QueryToken::CloseParen)
            }
            '[' => {
                self.index += ch.len_utf8();
                Some(QueryToken::OpenBracket)
            }
            ']' => {
                self.index += ch.len_utf8();
                Some(QueryToken::CloseBracket)
            }
            '"' => Some(QueryToken::String(self.string_token())),
            _ => Some(QueryToken::Symbol(self.symbol_token())),
        }
    }

    fn skip_ws_and_comments(&mut self) {
        loop {
            let Some(ch) = self.peek_char() else {
                return;
            };
            if ch.is_whitespace() {
                self.index += ch.len_utf8();
                continue;
            }
            if ch == ';' {
                while let Some(ch) = self.peek_char() {
                    self.index += ch.len_utf8();
                    if ch == '\n' {
                        break;
                    }
                }
                continue;
            }
            return;
        }
    }

    fn string_token(&mut self) -> String {
        debug_assert_eq!(self.peek_char(), Some('"'));
        self.index += '"'.len_utf8();
        let mut value = String::new();
        let mut escaped = false;
        while let Some(ch) = self.peek_char() {
            self.index += ch.len_utf8();
            if escaped {
                value.push(ch);
                escaped = false;
                continue;
            }
            match ch {
                '\\' => escaped = true,
                '"' => break,
                _ => value.push(ch),
            }
        }
        value
    }

    fn symbol_token(&mut self) -> String {
        let start = self.index;
        while let Some(ch) = self.peek_char() {
            if ch.is_whitespace() || matches!(ch, '(' | ')' | '[' | ']' | '"') || ch == ';' {
                break;
            }
            self.index += ch.len_utf8();
        }
        self.source[start..self.index].to_owned()
    }

    fn peek_char(&self) -> Option<char> {
        self.source.get(self.index..)?.chars().next()
    }
}

/// Well-known Tree-sitter query categories.
#[derive(Debug, Clone, Copy, Facet, PartialEq, Eq)]
#[repr(u8)]
pub enum WellKnownQuery {
    /// Highlight query.
    Highlights,
    /// Locals query.
    Locals,
    /// Injections query.
    Injections,
    /// Tags query.
    Tags,
}

impl WellKnownQuery {
    /// Default filename used by Tree-sitter packages.
    pub const fn filename(self) -> &'static str {
        match self {
            Self::Highlights => "highlights.scm",
            Self::Locals => "locals.scm",
            Self::Injections => "injections.scm",
            Self::Tags => "tags.scm",
        }
    }
}

/// Imported query files. Unknown query files are preserved.
#[derive(Debug, Clone, Default, Facet, PartialEq, Eq)]
pub struct QueryBundle {
    /// Query source files with category resolution.
    pub files: Vec<QueryFile>,
}

/// Imported query source file with Tree-sitter category metadata.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct QueryFile {
    /// Well-known category, when this file was resolved through category semantics.
    pub category: Option<WellKnownQuery>,
    /// Whether the file came from `tree-sitter.json` rather than fallback discovery.
    pub configured: bool,
    /// Query source file.
    pub source: SourceFile<QuerySource>,
}

impl QueryBundle {
    /// Get a well-known query file by default filename.
    pub fn well_known(&self, query: WellKnownQuery) -> Option<&SourceFile<QuerySource>> {
        self.files
            .iter()
            .find(|file| file.category == Some(query))
            .map(|file| &file.source)
    }

    /// Iterate well-known query files in configured order.
    pub fn well_known_files(
        &self,
        query: WellKnownQuery,
    ) -> impl Iterator<Item = &SourceFile<QuerySource>> {
        self.files
            .iter()
            .filter(move |file| file.category == Some(query))
            .map(|file| &file.source)
    }

    /// Iterate all query files.
    pub fn iter(&self) -> impl Iterator<Item = &SourceFile<QuerySource>> {
        self.files.iter().map(|file| &file.source)
    }

    /// Iterate all query files with category metadata.
    pub fn iter_files(&self) -> impl Iterator<Item = &QueryFile> {
        self.files.iter()
    }
}

#[cfg(test)]
mod tests {
    use super::{QuerySource, anonymous_node_literals};

    #[test]
    fn extracts_query_anonymous_node_literals() {
        let query = r##"
          "~" @operator
          ["#" "," "."] @punctuation.delimiter
          (("and") @keyword)
          ("\"" @punctuation.delimiter)
          ((property_name) @variable
            (#match? @variable "^--"))
          ; comments can contain "ignored" strings
        "##;

        let literals = anonymous_node_literals(query);

        assert!(literals.contains("~"));
        assert!(literals.contains("#"));
        assert!(literals.contains(","));
        assert!(literals.contains("."));
        assert!(literals.contains("and"));
        assert!(literals.contains("\""));
        assert!(!literals.contains("^--"));
        assert!(!literals.contains("ignored"));
    }

    #[test]
    fn query_source_reports_anonymous_node_literals() {
        let source = QuerySource(r#""@media" @keyword"#.to_owned());

        assert!(source.anonymous_node_literals().contains("@media"));
    }
}
