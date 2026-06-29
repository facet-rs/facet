//! Benchmark the interpreted reduced parser vs the Weavy-lowered path on the same
//! grammar + input, separating one-time setup (table build, runtime/plan construction)
//! from steady-state per-parse cost.
//!
//! Usage: cargo run --release -p snark --features json-import,weavy-lowering \
//!          --example weavy_bench -- [GRAMMAR_JS] [INPUT_FILE] [ITERS]

use std::{env, path::PathBuf, time::Instant};

use snark::{
    grammar::RawGrammarJson,
    lexical::LexicalFacts,
    lower::weavy::{lower_reduced_parser, parse_reduced_with_report},
    parser::{ParseTable, ParserGrammar, RuntimeParser},
    validated::ValidatedGrammar,
};

fn ms(d: std::time::Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

fn main() {
    let repo = env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .and_then(|p| p.parent().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    let grammar_js = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| repo.join("playgrounds/snark/src/bundled/gingembre/grammar.js"));
    let input_file = env::args_os().nth(2).map(PathBuf::from).unwrap_or_else(|| {
        repo.join("playgrounds/snark/src/bundled/gingembre/samples/docs-base.html")
    });
    let iters: usize = env::args()
        .nth(3)
        .and_then(|s| s.parse().ok())
        .unwrap_or(50);

    let input = std::fs::read_to_string(&input_file).expect("input file");
    let grammar_json =
        snark_dsl::emit_with_boa(&grammar_js).expect("grammar.js should emit grammar JSON");
    let raw = RawGrammarJson::from_tree_sitter_json_str(&grammar_json).expect("import");
    let validated = ValidatedGrammar::from_raw(&raw).expect("validate");
    let lexical = LexicalFacts::from_grammar(&validated);
    let parser = ParserGrammar::normalize_from_validated(&validated, &lexical)
        .expect("normalize")
        .prepare_productions_for_items()
        .expect("prepare");

    let t = Instant::now();
    let table = ParseTable::from_grammar(&parser).expect("table");
    let table_build = t.elapsed();

    // ---- interpreted path ----
    let t = Instant::now();
    let interp = RuntimeParser::new(&validated, &parser, &table).expect("runtime");
    let interp_new = t.elapsed();

    let t = Instant::now();
    for _ in 0..iters {
        let rt = RuntimeParser::new(&validated, &parser, &table).expect("runtime");
        let _ = rt.parse_recovering_with_report(&input);
    }
    let interp_cold_total = t.elapsed();

    let t = Instant::now();
    for _ in 0..iters {
        let _ = interp.parse_recovering_with_report(&input);
    }
    let interp_warm_total = t.elapsed();

    // ---- weavy path ----
    let t = Instant::now();
    let plan = lower_reduced_parser(&parser, &table).expect("lower");
    let weavy_lower = t.elapsed();

    let t = Instant::now();
    for _ in 0..iters {
        let _ = parse_reduced_with_report(&plan, &validated, &parser, &table, &input);
    }
    let weavy_total = t.elapsed();

    println!("grammar: {}", grammar_js.display());
    println!("input:   {} ({} bytes)", input_file.display(), input.len());
    println!("iters:   {iters}\n");

    println!("one-time setup:");
    println!("  ParseTable::from_grammar   {:>8.1} ms", ms(table_build));
    println!("  RuntimeParser::new         {:>8.1} ms", ms(interp_new));
    println!("  lower_reduced_parser       {:>8.1} ms", ms(weavy_lower));

    println!("\nper-parse (avg over {iters}):");
    println!(
        "  interpreted, fresh runtime {:>8.3} ms   (what the wasm demo does today)",
        ms(interp_cold_total) / iters as f64
    );
    println!(
        "  interpreted, warm runtime  {:>8.3} ms",
        ms(interp_warm_total) / iters as f64
    );
    println!(
        "  weavy, warm plan           {:>8.3} ms",
        ms(weavy_total) / iters as f64
    );
}
