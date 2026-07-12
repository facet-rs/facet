//! Arborium-compatible tree-sitter grammar crate for the vix language.
//!
//! The grammar SOURCE OF TRUTH is `playgrounds/snark/src/bundled/vix/grammar.js`
//! (snark evaluates it directly; snark's corpus is the semantic oracle). This
//! crate vendors the tree-sitter-generated C parser for that grammar so that
//! arborium consumers (dodeca coverage/highlighting, editors) can parse vix
//! without node or the tree-sitter CLI.
//!
//! Regenerate after editing grammar.js:
//!
//! ```sh
//! cd $(mktemp -d)
//! cp <repo>/playgrounds/snark/src/bundled/vix/grammar.js .
//! tree-sitter generate grammar.js
//! cp src/{parser.c,grammar.json,node-types.json} <repo>/arborium-vix/grammar/src/
//! cp src/tree_sitter/*.h <repo>/arborium-vix/grammar/src/tree_sitter/
//! ```
//!
//! Register with arborium (per arborium's EXTERNAL_GRAMMAR_CRATES.md):
//!
//! ```ignore
//! let grammar = Arc::new(CompiledGrammar::new(GrammarConfig {
//!     language: arborium_vix::language().into(),
//!     highlights_query: arborium_vix::HIGHLIGHTS_QUERY,
//!     injections_query: arborium_vix::INJECTIONS_QUERY,
//!     locals_query: arborium_vix::LOCALS_QUERY,
//! })?);
//! store.insert("vix", grammar);
//! ```

use tree_sitter_language::LanguageFn;

unsafe extern "C" {
    fn tree_sitter_vix() -> *const ();
}

/// The tree-sitter language for vix.
pub const fn language() -> LanguageFn {
    unsafe { LanguageFn::from_raw(tree_sitter_vix) }
}

/// Highlight query, vendored from the snark bundle (same provenance as the grammar).
pub const HIGHLIGHTS_QUERY: &str = include_str!("../queries/highlights.scm");
/// Vix has no injections query yet.
pub const INJECTIONS_QUERY: &str = "";
/// Vix has no locals query yet.
pub const LOCALS_QUERY: &str = "";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_vix_without_errors() {
        let mut parser = tree_sitter::Parser::new();
        parser
            .set_language(&language().into())
            .expect("load vix grammar");

        // Exercises the shapes that make vix vix: operator fn names (spaceship),
        // match with struct-literal-free scrutinee, map literals, path literals,
        // and both comment forms (they are what dodeca coverage extracts).
        let source = r#"// line comment with a ref: r[impl vix.smoke]
/// doc comment
fn <=>(self: Rank, other: Rank) -> Ordering {
    // NOTE: `self.n <=> other.n` (spaceship in EXPRESSION position) does not
    // parse in today's grammar — operator fn NAMES parse, operator invocation
    // is a lang-spec feature. Update this body when the language grows it.
    match self.n < other.n {
        true => Ordering::Less,
        false => Ordering::Greater,
    }
}

fn stored_state(state: State) -> State {
    let values: Map<String, State> = {};
    values.insert("state", state).get("state").unwrap()
}

fn pick(index: Index, pkg: Int) -> Step {
    match index.packages.len() == 0 {
        true => Step::Pass { state: state, changed: false },
        false => Step::Conflict(conflict_info(state, pkg, no_clause_id())),
    }
}
"#;
        let tree = parser.parse(source, None).expect("parse");
        let root = tree.root_node();
        assert_eq!(root.kind(), "source_file");
        assert!(
            !root.has_error(),
            "real vix shapes must parse cleanly: {}",
            root.to_sexp()
        );
    }
}
