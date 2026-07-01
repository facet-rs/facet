//! Steady-state parse-throughput bench: prepare once, parse the input N times,
//! report the best (min) parse time. Compare against `tree-sitter parse --time`.
//!
//!   cargo run --release -p snark-ts-diff -- <grammar.js> <input-file> [iters]

use std::time::Instant;
use std::{env, fs, path::Path};

use snark::{
    grammar::RawGrammarJson,
    lexical::LexicalFacts,
    lower::weavy::{RuntimeWeavyPlan, parse_prepared_runtime_with_report},
    parser::{ParseTable, ParserGrammar},
    validated::ValidatedGrammar,
};

fn main() {
    let args: Vec<String> = env::args().collect();
    let grammar_path = args.get(1).expect("usage: <grammar.js> <input> [iters]");
    let input = fs::read_to_string(args.get(2).expect("input file")).expect("read input");
    let iters: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(30);

    // Prepare once (emit + tables + weavy plan) — excluded from the parse timing.
    let json = snark_dsl::emit_with_boa(Path::new(grammar_path)).expect("emit");
    let raw = RawGrammarJson::from_tree_sitter_json_str(&json).expect("import");
    let validated = ValidatedGrammar::from_raw(&raw).expect("validate");
    let lexical = LexicalFacts::from_grammar(&validated);
    let normalized =
        ParserGrammar::normalize_from_validated(&validated, &lexical).expect("normalize");
    let parser = normalized.prepare_productions_for_items().expect("prepare");
    let table = ParseTable::from_grammar(&parser).expect("table");
    let plan = RuntimeWeavyPlan::new(&validated, &parser, &table).expect("plan");

    if let Err(e) = parse_prepared_runtime_with_report(&plan, &validated, &parser, &table, &input) {
        eprintln!("parse failed: {e:?}");
        std::process::exit(1);
    }

    let mut best_ms = f64::INFINITY;
    for _ in 0..iters {
        let start = Instant::now();
        let _ = parse_prepared_runtime_with_report(&plan, &validated, &parser, &table, &input);
        best_ms = best_ms.min(start.elapsed().as_secs_f64() * 1000.0);
    }

    let bytes = input.len();
    println!(
        "snark weavy parse: min {best_ms:.2} ms over {iters} iters, {bytes} bytes, {:.0} bytes/ms",
        bytes as f64 / best_ms
    );
}
