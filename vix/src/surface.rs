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
