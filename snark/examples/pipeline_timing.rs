//! Phase-by-phase timing of the full grammar pipeline for one grammar, native release.
//! Records where the time actually goes: grammar.js -> JSON (boa) -> decode -> validate
//! -> lexical -> normalize -> prepare -> parse table -> runtime plan (compiled lexer) ->
//! parse. Compare the table-build line against everything else.
//!
//! Usage: cargo run --release -p snark --features json-import --example pipeline_timing \
//!          -- [GRAMMAR_JS] [SAMPLE]

use std::{env, path::PathBuf, time::Instant};

use snark::{
    grammar::RawGrammarJson,
    lexical::LexicalFacts,
    parser::{ParseTable, ParserGrammar, RuntimeParser},
    validated::ValidatedGrammar,
};

fn main() {
    let repo = env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .and_then(|p| p.parent().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    let grammar_js = env::args_os().nth(1).map(PathBuf::from).unwrap_or_else(|| {
        repo.join("playgrounds/snark/src/bundled/gingembre/grammar.js")
    });
    let sample = env::args_os().nth(2).map(PathBuf::from).unwrap_or_else(|| {
        repo.join("playgrounds/snark/src/bundled/gingembre/samples/blog-index.html")
    });
    let input = std::fs::read_to_string(&sample).unwrap_or_default();

    macro_rules! phase {
        ($label:expr, $body:expr) => {{
            let t = Instant::now();
            let value = $body;
            (t.elapsed().as_secs_f64() * 1000.0, value)
        }};
    }

    let (t_emit, grammar_json) =
        phase!("emit", snark_dsl::emit_with_boa(&grammar_js).expect("emit grammar.js -> json"));
    let json_bytes = grammar_json.len();
    let (t_decode, raw) = phase!(
        "decode",
        RawGrammarJson::from_tree_sitter_json_str(&grammar_json).expect("decode json")
    );
    let (t_validate, validated) =
        phase!("validate", ValidatedGrammar::from_raw(&raw).expect("validate"));
    let (t_lexical, lexical) = phase!("lexical", LexicalFacts::from_grammar(&validated));
    let (t_normalize, normalized) = phase!(
        "normalize",
        ParserGrammar::normalize_from_validated(&validated, &lexical).expect("normalize")
    );
    let (t_prepare, parser) = phase!(
        "prepare",
        normalized.prepare_productions_for_items().expect("prepare productions")
    );
    let (t_table, table) =
        phase!("table", ParseTable::from_grammar(&parser).expect("build parse table"));
    let (t_plan, runtime) = phase!(
        "plan",
        RuntimeParser::new(&validated, &parser, &table).expect("runtime plan + compiled lexer")
    );
    // Measure each entry point the way the playground actually calls them.
    // Playground: parse_compact_with_report (strict) first, fall back to
    // parse_recovering_compact_with_report only on Err.
    let t0 = Instant::now();
    let strict = runtime.parse_compact_with_report(&input);
    let t_strict = t0.elapsed().as_secs_f64() * 1000.0;
    let strict_ok = strict.is_ok();

    let t0 = Instant::now();
    let _ = runtime.parse_recovering_compact_with_report(&input);
    let t_rec_compact = t0.elapsed().as_secs_f64() * 1000.0;

    let t0 = Instant::now();
    let _ = runtime.parse_recovering_with_report(&input);
    let t_rec_full = t0.elapsed().as_secs_f64() * 1000.0;

    let prepare_total = t_emit + t_decode + t_validate + t_lexical + t_normalize + t_prepare
        + t_table + t_plan;

    println!("grammar: {}", grammar_js.display());
    println!("sample:  {} ({} bytes)", sample.display(), input.len());
    println!("grammar.json: {json_bytes} bytes\n");

    let rows = [
        ("grammar.js -> json (boa)", t_emit),
        ("decode json -> RawGrammarJson", t_decode),
        ("validate -> ValidatedGrammar", t_validate),
        ("lexical facts", t_lexical),
        ("normalize -> ParserGrammar", t_normalize),
        ("prepare productions", t_prepare),
        ("** ParseTable::from_grammar", t_table),
        ("runtime plan (compiled lexer)", t_plan),
    ];
    println!("---- one-time prepare ----");
    for (label, ms) in rows {
        let pct = if prepare_total > 0.0 { ms / prepare_total * 100.0 } else { 0.0 };
        println!("  {label:<32} {ms:>9.1} ms  {pct:>5.1}%");
    }
    println!("  {:<32} {:>9.1} ms", "prepare TOTAL", prepare_total);
    println!("\n---- steady state (parse, same input, 3 entry points) ----");
    println!(
        "  {:<40} {:>11.3} ms   (strict ok? {})",
        "parse_compact_with_report [playground 1st]", t_strict, strict_ok
    );
    println!(
        "  {:<40} {:>11.3} ms",
        "parse_recovering_compact_with_report [fb]", t_rec_compact
    );
    println!(
        "  {:<40} {:>11.3} ms",
        "parse_recovering_with_report [my bench]", t_rec_full
    );
}
