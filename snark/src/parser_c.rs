//! Generated Tree-sitter `parser.c` fact extraction.
//!
//! This module reads the stable tables emitted by Tree-sitter's generator. It
//! is not a general C parser; it extracts the generated machine facts Snark
//! needs before lowering parser behavior into Weavy.

use std::{error::Error, fmt};

use facet::Facet;

/// Raw generated `src/parser.c` source.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct ParserC(pub String);

/// Tree-sitter generated-parser constants.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct ParserConstants {
    /// `LANGUAGE_VERSION`.
    pub language_version: u32,
    /// `STATE_COUNT`.
    pub state_count: u32,
    /// `LARGE_STATE_COUNT`.
    pub large_state_count: u32,
    /// `SYMBOL_COUNT`.
    pub symbol_count: u32,
    /// `ALIAS_COUNT`.
    pub alias_count: u32,
    /// `TOKEN_COUNT`.
    pub token_count: u32,
    /// `EXTERNAL_TOKEN_COUNT`.
    pub external_token_count: u32,
    /// `FIELD_COUNT`.
    pub field_count: u32,
    /// `MAX_ALIAS_SEQUENCE_LENGTH`.
    pub max_alias_sequence_length: u32,
    /// `MAX_RESERVED_WORD_SET_SIZE`.
    pub max_reserved_word_set_size: u32,
    /// `PRODUCTION_ID_COUNT`.
    pub production_id_count: u32,
    /// `SUPERTYPE_COUNT`.
    pub supertype_count: u32,
}

/// Symbol id used by generated `parser.c`.
#[derive(Debug, Clone, Copy, Facet, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ParserSymbolId(u32);

impl ParserSymbolId {
    /// Return the generated symbol id.
    pub const fn get(self) -> u32 {
        self.0
    }
}

/// Symbol metadata from `parser.c`.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct ParserSymbol {
    /// Dense generated symbol id.
    pub id: ParserSymbolId,
    /// C enum identifier such as `sym_stylesheet`.
    pub c_name: String,
    /// Tree-sitter public symbol name.
    pub name: String,
    /// Whether Tree-sitter exposes this symbol in syntax trees.
    pub visible: bool,
    /// Whether Tree-sitter treats this as a named symbol.
    pub named: bool,
}

/// Lex mode selected for one parse state.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct ParserLexMode {
    /// Parse state index.
    pub state: u32,
    /// Generated internal lexer state.
    pub lex_state: u32,
    /// Generated external scanner state, when present.
    pub external_lex_state: Option<u32>,
}

/// Generated parser facts currently extracted from `parser.c`.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct GeneratedParserFacts {
    /// Top-level generated constants.
    pub constants: ParserConstants,
    /// Symbols, including aliases, in generated numeric order.
    pub symbols: Vec<ParserSymbol>,
    /// Lex mode table indexed by parse state.
    pub lex_modes: Vec<ParserLexMode>,
}

impl GeneratedParserFacts {
    /// Extract generated parser facts from `src/parser.c`.
    pub fn from_parser_c(parser_c: &ParserC) -> Result<Self, ParserCFactsError> {
        let source = parser_c.0.as_str();
        let constants = parse_constants(source)?;
        let identifiers = parse_symbol_identifiers(source)?;
        let symbol_names = parse_symbol_names(source)?;
        let metadata = parse_symbol_metadata(source)?;
        let lex_modes = parse_lex_modes(source)?;

        let mut symbols = Vec::with_capacity(symbol_names.len());
        for (c_name, id, name) in symbol_names {
            let metadata = metadata
                .iter()
                .find(|metadata| metadata.c_name == c_name)
                .ok_or_else(|| {
                    ParserCFactsError::missing_table_entry("ts_symbol_metadata", &c_name)
                })?;
            let declared_id = identifiers
                .iter()
                .find(|identifier| identifier.c_name == c_name)
                .map(|identifier| identifier.id)
                .ok_or_else(|| {
                    ParserCFactsError::missing_table_entry("enum ts_symbol_identifiers", &c_name)
                })?;
            if declared_id != id {
                return Err(ParserCFactsError::new(
                    ParserCFactsErrorKind::MismatchedSymbolId {
                        symbol: c_name,
                        enum_id: declared_id,
                        table_id: id,
                    },
                ));
            }
            symbols.push(ParserSymbol {
                id: ParserSymbolId(id),
                c_name,
                name,
                visible: metadata.visible,
                named: metadata.named,
            });
        }
        symbols.sort_by_key(|symbol| symbol.id);

        Ok(Self {
            constants,
            symbols,
            lex_modes,
        })
    }

    /// Get a symbol by generated id.
    pub fn symbol_by_id(&self, id: u32) -> Option<&ParserSymbol> {
        self.symbols.iter().find(|symbol| symbol.id.get() == id)
    }

    /// Get a symbol by generated C enum identifier.
    pub fn symbol_by_c_name(&self, c_name: &str) -> Option<&ParserSymbol> {
        self.symbols.iter().find(|symbol| symbol.c_name == c_name)
    }
}

/// Error while extracting generated parser facts.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
pub struct ParserCFactsError {
    /// Error kind.
    pub kind: ParserCFactsErrorKind,
}

impl ParserCFactsError {
    fn new(kind: ParserCFactsErrorKind) -> Self {
        Self { kind }
    }

    fn missing_constant(name: &'static str) -> Self {
        Self::new(ParserCFactsErrorKind::MissingConstant { name })
    }

    fn missing_section(name: &'static str) -> Self {
        Self::new(ParserCFactsErrorKind::MissingSection { name })
    }

    fn missing_table_entry(table: &'static str, key: &str) -> Self {
        Self::new(ParserCFactsErrorKind::MissingTableEntry {
            table,
            key: key.to_owned(),
        })
    }
}

impl fmt::Display for ParserCFactsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ParserCFactsErrorKind::MissingConstant { name } => {
                write!(f, "generated parser.c is missing constant {name}")
            }
            ParserCFactsErrorKind::InvalidConstant { name, value } => {
                write!(
                    f,
                    "generated parser.c constant {name} has invalid value `{value}`"
                )
            }
            ParserCFactsErrorKind::MissingSection { name } => {
                write!(f, "generated parser.c is missing section {name}")
            }
            ParserCFactsErrorKind::MalformedLine { section, line } => {
                write!(
                    f,
                    "generated parser.c section {section} has malformed line `{line}`"
                )
            }
            ParserCFactsErrorKind::MissingTableEntry { table, key } => {
                write!(f, "generated parser.c table {table} has no entry for {key}")
            }
            ParserCFactsErrorKind::MismatchedSymbolId {
                symbol,
                enum_id,
                table_id,
            } => write!(
                f,
                "generated parser.c symbol {symbol} has enum id {enum_id} but table id {table_id}"
            ),
        }
    }
}

impl Error for ParserCFactsError {}

/// Error kind while extracting generated parser facts.
#[derive(Debug, Clone, Facet, PartialEq, Eq)]
#[repr(u8)]
pub enum ParserCFactsErrorKind {
    /// A generated `#define` was not present.
    MissingConstant {
        /// Constant name.
        name: &'static str,
    },
    /// A generated `#define` value was not a number.
    InvalidConstant {
        /// Constant name.
        name: &'static str,
        /// Raw constant value.
        value: String,
    },
    /// A generated table section was not present.
    MissingSection {
        /// Section name.
        name: &'static str,
    },
    /// A generated table line had an unexpected shape.
    MalformedLine {
        /// Section name.
        section: &'static str,
        /// Raw line.
        line: String,
    },
    /// A generated table missed an entry required by another generated table.
    MissingTableEntry {
        /// Table name.
        table: &'static str,
        /// Missing key.
        key: String,
    },
    /// A symbol id disagreed between generated sections.
    MismatchedSymbolId {
        /// Symbol C enum name.
        symbol: String,
        /// Id from `enum ts_symbol_identifiers`.
        enum_id: u32,
        /// Id from a generated table index.
        table_id: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SymbolIdentifier {
    c_name: String,
    id: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct SymbolMetadata {
    c_name: String,
    visible: bool,
    named: bool,
}

fn parse_constants(source: &str) -> Result<ParserConstants, ParserCFactsError> {
    Ok(ParserConstants {
        language_version: parse_define(source, "LANGUAGE_VERSION")?,
        state_count: parse_define(source, "STATE_COUNT")?,
        large_state_count: parse_define(source, "LARGE_STATE_COUNT")?,
        symbol_count: parse_define(source, "SYMBOL_COUNT")?,
        alias_count: parse_define(source, "ALIAS_COUNT")?,
        token_count: parse_define(source, "TOKEN_COUNT")?,
        external_token_count: parse_define(source, "EXTERNAL_TOKEN_COUNT")?,
        field_count: parse_define(source, "FIELD_COUNT")?,
        max_alias_sequence_length: parse_define(source, "MAX_ALIAS_SEQUENCE_LENGTH")?,
        max_reserved_word_set_size: parse_define(source, "MAX_RESERVED_WORD_SET_SIZE")?,
        production_id_count: parse_define(source, "PRODUCTION_ID_COUNT")?,
        supertype_count: parse_define(source, "SUPERTYPE_COUNT")?,
    })
}

fn parse_define(source: &str, name: &'static str) -> Result<u32, ParserCFactsError> {
    let prefix = format!("#define {name} ");
    let line = source
        .lines()
        .find_map(|line| line.trim().strip_prefix(prefix.as_str()))
        .ok_or_else(|| ParserCFactsError::missing_constant(name))?;
    line.trim().parse::<u32>().map_err(|_| {
        ParserCFactsError::new(ParserCFactsErrorKind::InvalidConstant {
            name,
            value: line.trim().to_owned(),
        })
    })
}

fn parse_symbol_identifiers(source: &str) -> Result<Vec<SymbolIdentifier>, ParserCFactsError> {
    let section = extract_section(source, "enum ts_symbol_identifiers {", "};")
        .ok_or_else(|| ParserCFactsError::missing_section("enum ts_symbol_identifiers"))?;
    let mut identifiers = vec![SymbolIdentifier {
        c_name: "ts_builtin_sym_end".to_owned(),
        id: 0,
    }];
    for raw_line in section.lines() {
        let line = raw_line.trim().trim_end_matches(',');
        if line.is_empty() {
            continue;
        }
        let Some((name, value)) = line.split_once(" = ") else {
            return Err(ParserCFactsError::new(
                ParserCFactsErrorKind::MalformedLine {
                    section: "enum ts_symbol_identifiers",
                    line: raw_line.trim().to_owned(),
                },
            ));
        };
        let id = value.parse::<u32>().map_err(|_| {
            ParserCFactsError::new(ParserCFactsErrorKind::MalformedLine {
                section: "enum ts_symbol_identifiers",
                line: raw_line.trim().to_owned(),
            })
        })?;
        identifiers.push(SymbolIdentifier {
            c_name: name.to_owned(),
            id,
        });
    }
    Ok(identifiers)
}

fn parse_symbol_names(source: &str) -> Result<Vec<(String, u32, String)>, ParserCFactsError> {
    let section = extract_section(
        source,
        "static const char * const ts_symbol_names[] = {",
        "};",
    )
    .ok_or_else(|| ParserCFactsError::missing_section("ts_symbol_names"))?;
    let identifiers = parse_symbol_identifiers(source)?;
    let mut names = Vec::new();
    for raw_line in section.lines() {
        let line = raw_line.trim().trim_end_matches(',');
        if line.is_empty() {
            continue;
        }
        let Some((index, value)) = parse_indexed_assignment(line) else {
            return Err(ParserCFactsError::new(
                ParserCFactsErrorKind::MalformedLine {
                    section: "ts_symbol_names",
                    line: raw_line.trim().to_owned(),
                },
            ));
        };
        let Some(value) = parse_c_string(value) else {
            return Err(ParserCFactsError::new(
                ParserCFactsErrorKind::MalformedLine {
                    section: "ts_symbol_names",
                    line: raw_line.trim().to_owned(),
                },
            ));
        };
        let id = identifiers
            .iter()
            .find(|identifier| identifier.c_name == index)
            .map(|identifier| identifier.id)
            .ok_or_else(|| {
                ParserCFactsError::missing_table_entry("enum ts_symbol_identifiers", index)
            })?;
        names.push((index.to_owned(), id, value));
    }
    Ok(names)
}

fn parse_symbol_metadata(source: &str) -> Result<Vec<SymbolMetadata>, ParserCFactsError> {
    let section = extract_section(
        source,
        "static const TSSymbolMetadata ts_symbol_metadata[] = {",
        "};",
    )
    .ok_or_else(|| ParserCFactsError::missing_section("ts_symbol_metadata"))?;
    let mut metadata = Vec::new();
    let mut lines = section.lines().peekable();
    while let Some(raw_line) = lines.next() {
        let line = raw_line.trim();
        if line.is_empty() {
            continue;
        }
        let Some(c_name) = line
            .strip_prefix('[')
            .and_then(|rest| rest.split_once("] = {"))
            .map(|(name, _)| name.to_owned())
        else {
            return Err(ParserCFactsError::new(
                ParserCFactsErrorKind::MalformedLine {
                    section: "ts_symbol_metadata",
                    line: line.to_owned(),
                },
            ));
        };
        let mut visible = None;
        let mut named = None;
        loop {
            let Some(raw_property) = lines.next() else {
                return Err(ParserCFactsError::new(
                    ParserCFactsErrorKind::MalformedLine {
                        section: "ts_symbol_metadata",
                        line: line.to_owned(),
                    },
                ));
            };
            let property = raw_property.trim().trim_end_matches(',');
            if property == "}" {
                break;
            }
            if let Some(value) = property.strip_prefix(".visible = ") {
                visible = Some(parse_bool(value).ok_or_else(|| {
                    ParserCFactsError::new(ParserCFactsErrorKind::MalformedLine {
                        section: "ts_symbol_metadata",
                        line: raw_property.trim().to_owned(),
                    })
                })?);
            } else if let Some(value) = property.strip_prefix(".named = ") {
                named = Some(parse_bool(value).ok_or_else(|| {
                    ParserCFactsError::new(ParserCFactsErrorKind::MalformedLine {
                        section: "ts_symbol_metadata",
                        line: raw_property.trim().to_owned(),
                    })
                })?);
            }
        }
        metadata.push(SymbolMetadata {
            c_name,
            visible: visible.unwrap_or(false),
            named: named.unwrap_or(false),
        });
    }
    Ok(metadata)
}

fn parse_lex_modes(source: &str) -> Result<Vec<ParserLexMode>, ParserCFactsError> {
    let section = extract_section(
        source,
        "static const TSLexerMode ts_lex_modes[STATE_COUNT] = {",
        "};",
    )
    .ok_or_else(|| ParserCFactsError::missing_section("ts_lex_modes"))?;
    let mut lex_modes = Vec::new();
    for raw_line in section.lines() {
        let line = raw_line.trim().trim_end_matches(',');
        if line.is_empty() {
            continue;
        }
        let Some((index, value)) = parse_indexed_assignment(line) else {
            return Err(ParserCFactsError::new(
                ParserCFactsErrorKind::MalformedLine {
                    section: "ts_lex_modes",
                    line: raw_line.trim().to_owned(),
                },
            ));
        };
        let state = index.parse::<u32>().map_err(|_| {
            ParserCFactsError::new(ParserCFactsErrorKind::MalformedLine {
                section: "ts_lex_modes",
                line: raw_line.trim().to_owned(),
            })
        })?;
        let Some(fields) = value
            .strip_prefix('{')
            .and_then(|rest| rest.strip_suffix('}'))
        else {
            return Err(ParserCFactsError::new(
                ParserCFactsErrorKind::MalformedLine {
                    section: "ts_lex_modes",
                    line: raw_line.trim().to_owned(),
                },
            ));
        };
        let mut lex_state = None;
        let mut external_lex_state = None;
        for field in fields.split(',') {
            let field = field.trim();
            if let Some(value) = field.strip_prefix(".lex_state = ") {
                lex_state = Some(parse_u32_field("ts_lex_modes", raw_line, value)?);
            } else if let Some(value) = field.strip_prefix(".external_lex_state = ") {
                external_lex_state = Some(parse_u32_field("ts_lex_modes", raw_line, value)?);
            }
        }
        let Some(lex_state) = lex_state else {
            return Err(ParserCFactsError::new(
                ParserCFactsErrorKind::MalformedLine {
                    section: "ts_lex_modes",
                    line: raw_line.trim().to_owned(),
                },
            ));
        };
        lex_modes.push(ParserLexMode {
            state,
            lex_state,
            external_lex_state,
        });
    }
    Ok(lex_modes)
}

fn parse_u32_field(
    section: &'static str,
    raw_line: &str,
    value: &str,
) -> Result<u32, ParserCFactsError> {
    value.parse::<u32>().map_err(|_| {
        ParserCFactsError::new(ParserCFactsErrorKind::MalformedLine {
            section,
            line: raw_line.trim().to_owned(),
        })
    })
}

fn extract_section<'a>(source: &'a str, start: &str, end: &str) -> Option<&'a str> {
    let after_start = source.split_once(start)?.1;
    let (section, _) = after_start.split_once(end)?;
    Some(section)
}

fn parse_indexed_assignment(line: &str) -> Option<(&str, &str)> {
    let (index, value) = line.strip_prefix('[')?.split_once("] = ")?;
    Some((index, value))
}

fn parse_bool(value: &str) -> Option<bool> {
    match value {
        "true" => Some(true),
        "false" => Some(false),
        _ => None,
    }
}

fn parse_c_string(value: &str) -> Option<String> {
    let mut chars = value.strip_prefix('"')?.strip_suffix('"')?.chars();
    let mut out = String::new();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        let escaped = chars.next()?;
        match escaped {
            '\\' => out.push('\\'),
            '"' => out.push('"'),
            'n' => out.push('\n'),
            'r' => out.push('\r'),
            't' => out.push('\t'),
            _ => {
                out.push('\\');
                out.push(escaped);
            }
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    const CSS_FIXTURE_PARSER: &str =
        include_str!("../tests/fixtures/packages/tree-sitter-css-reduced/src/parser.c");

    #[test]
    fn extracts_generated_css_parser_facts() {
        let facts =
            GeneratedParserFacts::from_parser_c(&ParserC(CSS_FIXTURE_PARSER.to_owned())).unwrap();

        assert_eq!(facts.constants.language_version, 15);
        assert_eq!(facts.constants.state_count, 442);
        assert_eq!(facts.constants.large_state_count, 2);
        assert_eq!(facts.constants.symbol_count, 142);
        assert_eq!(facts.constants.alias_count, 9);
        assert_eq!(facts.constants.token_count, 75);
        assert_eq!(facts.constants.external_token_count, 3);
        assert_eq!(facts.constants.production_id_count, 17);
        assert_eq!(facts.symbols.len(), 151);
        assert_eq!(facts.lex_modes.len(), 442);

        let stylesheet = facts.symbol_by_c_name("sym_stylesheet").unwrap();
        assert_eq!(stylesheet.id.get(), 75);
        assert_eq!(stylesheet.name, "stylesheet");
        assert!(stylesheet.visible);
        assert!(stylesheet.named);

        let alias = facts.symbol_by_c_name("alias_sym_tag_name").unwrap();
        assert_eq!(alias.id.get(), 150);
        assert_eq!(alias.name, "tag_name");
        assert!(alias.visible);
        assert!(alias.named);

        assert_eq!(facts.lex_modes[0].lex_state, 0);
        assert_eq!(facts.lex_modes[0].external_lex_state, Some(1));
        assert_eq!(facts.lex_modes[14].lex_state, 8);
        assert_eq!(facts.lex_modes[14].external_lex_state, None);
    }

    #[test]
    fn reports_missing_generated_section() {
        let err = GeneratedParserFacts::from_parser_c(&ParserC(
            "#define LANGUAGE_VERSION 15\n".to_owned(),
        ))
        .unwrap_err();

        assert!(matches!(
            err.kind,
            ParserCFactsErrorKind::MissingConstant {
                name: "STATE_COUNT"
            }
        ));
    }
}
