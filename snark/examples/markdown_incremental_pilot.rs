use std::{env, hint::black_box, time::{Duration, Instant}};

use snark::{
    grammar::RawGrammarJson,
    lexical::LexicalFacts,
    lower::weavy::{
        WeavyParsePlan, WeavyParseSession,
        parse_prepared_weavy_collecting_reuse_with_report_and_scanner,
    },
    parser::{ParseTable, ParserGrammar, ParserInputEdit, TreeEvent},
    validated::ValidatedGrammar,
};

const GRAMMAR_JS: &str = include_str!("../tests/fixtures/markdown-pilot/grammar.js");

struct PreparedParser {
    parser: ParserGrammar,
    table: ParseTable,
    plan: WeavyParsePlan,
}

impl PreparedParser {
    fn new() -> Self {
        let grammar_json = snark_dsl::emit_source_with_boa(
            GRAMMAR_JS,
            "snark/tests/fixtures/markdown-pilot/grammar.js",
        )
        .expect("markdown pilot grammar emits");
        let raw = RawGrammarJson::from_tree_sitter_json_str(&grammar_json)
            .expect("markdown pilot grammar imports");
        let validated =
            ValidatedGrammar::from_raw(&raw).expect("markdown pilot grammar validates");
        let lexical = LexicalFacts::from_grammar(&validated);
        let parser = ParserGrammar::normalize_from_validated(&validated, &lexical)
            .expect("markdown pilot grammar normalizes")
            .prepare_productions_for_items()
            .expect("markdown pilot grammar prepares productions");
        let table = ParseTable::from_grammar(&parser).expect("markdown pilot builds parse table");
        let plan =
            WeavyParsePlan::new(&validated, &parser, &table).expect("markdown pilot lowers to Weavy");
        Self { parser, table, plan }
    }

    fn full_parse(&self, input: &str) -> snark::lower::weavy::WeavyParseReport {
        parse_prepared_weavy_collecting_reuse_with_report_and_scanner(
            &self.plan,
            &self.parser,
            &self.table,
            input,
            None,
        )
        .expect("full parse succeeds")
    }
}

fn document(paragraphs: usize, edited_word: &str) -> (String, usize) {
    let edit_index = paragraphs / 2;
    let mut source = String::with_capacity(paragraphs * 80);
    let mut edit_start = 0;
    for index in 0..paragraphs {
        if index % 97 == 0 {
            source.push_str("```rust\nfn generated() { println!(\"scanner free\"); }\n```\n\n");
        }
        source.push_str("Paragraph ");
        source.push_str(&index.to_string());
        source.push_str(" keeps enough stable syntax around the local ");
        if index == edit_index {
            edit_start = source.len();
            source.push_str(edited_word);
        } else {
            source.push_str("alpha");
        }
        source.push_str(" edit.\n\n");
    }
    (source, edit_start)
}

fn elapsed_per_iteration(total: Duration, iterations: usize) -> Duration {
    total / u32::try_from(iterations).expect("iteration count fits u32")
}

fn main() {
    let paragraphs = env::args()
        .nth(1)
        .map(|value| value.parse().expect("paragraph count is an integer"))
        .unwrap_or(10_000);
    let iterations = env::args()
        .nth(2)
        .map(|value| value.parse().expect("iteration count is an integer"))
        .unwrap_or(40);
    assert!(paragraphs >= 4, "pilot needs at least four paragraphs");
    assert!(iterations >= 2, "pilot needs at least two iterations");

    let prepared = PreparedParser::new();
    let (alpha, edit_start) = document(paragraphs, "alpha");
    let (bravo, bravo_edit_start) = document(paragraphs, "bravo");
    assert_eq!(edit_start, bravo_edit_start);
    assert_eq!(alpha.len(), bravo.len());

    let mut session = WeavyParseSession::new(&prepared.plan, &prepared.parser, &prepared.table);
    let initial = session.parse(alpha.clone()).expect("initial parse succeeds").clone();
    let edit = ParserInputEdit::new(edit_start, edit_start + 5, edit_start + 5);
    let incremental = session
        .reparse(edit, bravo.clone())
        .expect("incremental reparse succeeds")
        .clone();
    let oracle = prepared.full_parse(&bravo);

    assert_eq!(incremental.tree().to_sexp(), oracle.tree().to_sexp());
    let reused_nodes = incremental
        .tree_events()
        .iter()
        .filter(|event| matches!(event, TreeEvent::ReuseNode { .. }))
        .count();
    assert!(reused_nodes > 0, "incremental reparse must reuse accepted subtrees");

    for iteration in 0..32 {
        let input = if iteration % 2 == 0 { &alpha } else { &bravo };
        black_box(prepared.full_parse(black_box(input)));
    }
    let mut warm_session =
        WeavyParseSession::new(&prepared.plan, &prepared.parser, &prepared.table);
    warm_session.parse(alpha.clone()).expect("warm baseline");
    for iteration in 0..32 {
        let input = if iteration % 2 == 0 { &bravo } else { &alpha };
        black_box(
            warm_session
                .reparse(edit, input.clone())
                .expect("warm incremental parse"),
        );
    }
    let full_start = Instant::now();
    for iteration in 0..iterations {
        let input = if iteration % 2 == 0 { &alpha } else { &bravo };
        black_box(prepared.full_parse(black_box(input)));
    }
    let full_elapsed = full_start.elapsed();

    let mut incremental_session =
        WeavyParseSession::new(&prepared.plan, &prepared.parser, &prepared.table);
    incremental_session
        .parse(alpha.clone())
        .expect("benchmark baseline parse");
    let incremental_start = Instant::now();
    for iteration in 0..iterations {
        let input = if iteration % 2 == 0 { &bravo } else { &alpha };
        black_box(
            incremental_session
                .reparse(edit, input.clone())
                .expect("benchmark incremental reparse"),
        );
    }
    let incremental_elapsed = incremental_start.elapsed();

    let speedup = full_elapsed.as_secs_f64() / incremental_elapsed.as_secs_f64();
    println!("grammar: JavaScript DSL -> Snark facts -> LR/GLR table -> Weavy plan");
    println!("document: {} bytes, {paragraphs} paragraphs", alpha.len());
    println!("incremental execution lane: {:?}", incremental.execution_lane());
    println!(
        "JIT blocks: {} executed, {} fallback",
        incremental.hostcall_stats().executed_blocks,
        incremental.hostcall_stats().fallback_blocks,
    );
    println!("initial tree: {}", initial.tree().to_sexp());
    println!("incremental/full tree equivalence: yes");
    println!("reused nodes after localized edit: {reused_nodes}");
    println!(
        "fresh full parse: {:?}/iteration ({:?} total)",
        elapsed_per_iteration(full_elapsed, iterations),
        full_elapsed,
    );
    println!(
        "incremental reparse: {:?}/iteration ({:?} total)",
        elapsed_per_iteration(incremental_elapsed, iterations),
        incremental_elapsed,
    );
    println!("incremental speedup: {speedup:.2}x");
}
