//! Ratchet-facing Vix parser.
//!
//! The legacy parser remains exported from the crate root while existing
//! consumers migrate. New compiler work starts here and never enters the
//! legacy machine.

use snark::{
    grammar::RawGrammarJson,
    lexical::LexicalFacts,
    lower::weavy::{WeavyParsePlan, parse_prepared_weavy_with_report},
    parser::{ParseTable, ParserGrammar},
    validated::ValidatedGrammar,
};

use crate::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticPayload, Diagnostics};
use crate::support::Span;

/// Grammar-derived ratchet AST and resolved-CST lowering.
pub mod ast {
    include!(concat!(env!("OUT_DIR"), "/vix_surface_ast.rs"));
}

pub const GRAMMAR_JSON: &str = include_str!(concat!(env!("OUT_DIR"), "/vix_surface_grammar.json"));

/// Prepared Snark parser for the authoritative Vix surface.
pub struct SurfaceParser {
    parser: ParserGrammar,
    table: ParseTable,
    plan: WeavyParsePlan,
}

impl SurfaceParser {
    #[must_use]
    pub fn new() -> Self {
        let raw = RawGrammarJson::from_tree_sitter_json_str(GRAMMAR_JSON)
            .expect("embedded Vix surface grammar imports");
        let validated =
            ValidatedGrammar::from_raw(&raw).expect("embedded Vix surface grammar validates");
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .expect("embedded Vix surface grammar normalizes")
            .prepare_productions_for_items()
            .expect("embedded Vix surface grammar prepares productions");
        let table = ParseTable::from_grammar(&parser)
            .expect("embedded Vix surface grammar builds parse tables");
        let plan = WeavyParsePlan::new(&validated, &parser, &table)
            .expect("embedded Vix surface grammar builds a Weavy parse plan");
        Self {
            parser,
            table,
            plan,
        }
    }

    /// Parse one source file into the generated AST.
    ///
    /// r[impl lang.diagnostics.typed]
    pub fn parse(&self, source: &str) -> Result<ast::SourceFile, Diagnostics> {
        if let Some(span) = unsupported_generic_call_span(source) {
            return Err(Diagnostics::one(Diagnostic {
                code: DiagnosticCode::ParseRejected,
                primary: span,
                labels: Vec::new(),
                payload: DiagnosticPayload::Parse {
                    detail: "generic call type arguments are not part of the Vix surface"
                        .to_owned(),
                },
            }));
        }
        let whole_source = Span {
            start: 0,
            end: u32::try_from(source.len()).unwrap_or(u32::MAX),
        };
        let report =
            parse_prepared_weavy_with_report(&self.plan, &self.parser, &self.table, source)
                .map_err(|error| {
                    Diagnostics::one(Diagnostic {
                        code: DiagnosticCode::ParseRejected,
                        primary: whole_source,
                        labels: Vec::new(),
                        payload: DiagnosticPayload::Parse {
                            detail: format!("{error:?}"),
                        },
                    })
                })?;
        let resolved = report
            .accepted_resolved_tree(&self.parser, source)
            .ok_or_else(|| {
                Diagnostics::one(Diagnostic {
                    code: DiagnosticCode::ParseRejected,
                    primary: whole_source,
                    labels: Vec::new(),
                    payload: DiagnosticPayload::Parse {
                        detail: "parser produced no accepted tree".to_owned(),
                    },
                })
            })?;
        Ok(ast::lower_source_file(&resolved))
    }
}

impl Default for SurfaceParser {
    fn default() -> Self {
        Self::new()
    }
}

fn unsupported_generic_call_span(source: &str) -> Option<Span> {
    let bytes = source.as_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'"' => index = skip_string(bytes, index),
            b'/' if bytes.get(index + 1) == Some(&b'/') => {
                index = skip_line_comment(bytes, index);
            }
            byte if is_identifier_start(byte) => {
                let start = index;
                index += 1;
                while bytes
                    .get(index)
                    .is_some_and(|byte| is_identifier_continue(*byte))
                {
                    index += 1;
                }
                if bytes.get(index) == Some(&b'<')
                    && !is_allowed_type_application(&bytes[start..index])
                    && let Some(end) = generic_call_end(bytes, index)
                {
                    return Some(Span {
                        start: u32::try_from(start).unwrap_or(u32::MAX),
                        end: u32::try_from(end).unwrap_or(u32::MAX),
                    });
                }
            }
            _ => index += 1,
        }
    }
    None
}

fn is_allowed_type_application(identifier: &[u8]) -> bool {
    matches!(identifier, b"try_json_decode" | b"try_toml_decode")
}

fn generic_call_end(bytes: &[u8], lt: usize) -> Option<usize> {
    let mut depth = 1usize;
    let mut index = lt + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'<' => depth = depth.checked_add(1)?,
            b'>' => {
                depth = depth.checked_sub(1)?;
                if depth == 0 {
                    return (bytes.get(index + 1) == Some(&b'(')).then_some(index + 2);
                }
            }
            b'"' => index = skip_string(bytes, index).saturating_sub(1),
            b'\n' | b';' | b'{' | b'}' => return None,
            _ => {}
        }
        index += 1;
    }
    None
}

fn skip_string(bytes: &[u8], start: usize) -> usize {
    let mut index = start + 1;
    while index < bytes.len() {
        match bytes[index] {
            b'\\' => index = (index + 2).min(bytes.len()),
            b'"' => return index + 1,
            _ => index += 1,
        }
    }
    bytes.len()
}

fn skip_line_comment(bytes: &[u8], start: usize) -> usize {
    let mut index = start + 2;
    while index < bytes.len() && bytes[index] != b'\n' {
        index += 1;
    }
    index
}

fn is_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_identifier_continue(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}
