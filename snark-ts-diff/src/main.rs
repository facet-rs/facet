//! Parse-throughput bench for the snark Weavy runtime.
//!
//! Two modes:
//!
//!   # single input, prepare once, parse N times, report best (min) ms
//!   cargo run --release -p snark-ts-diff -- <grammar.js> <input-file> [iters]
//!
//!   # size ladder: prepare once, sweep JSON of growing object counts, print
//!   # a table of ms + bytes/ms + ratio-vs-previous. The `x_prev` column is the
//!   # tell: object counts double each row, so a LINEAR parser holds ~2.0 and a
//!   # QUADRATIC one climbs toward ~4.0 (and bytes/ms halves).
//!   cargo run --release -p snark-ts-diff -- ladder <grammar.js> [max_objects]
//!
//! Fixtures are generated with facet-json (never hand-emitted) as `[{"k":0,
//! "v":"x0"},…]`, which the bundled `jsonb` grammar accepts.

use std::time::Instant;
use std::{env, fs, path::Path};

use facet::Facet;
use snark::{
    grammar::RawGrammarJson,
    lexical::LexicalFacts,
    lower::weavy::{RuntimeWeavyPlan, parse_prepared_runtime_with_report},
    parser::{ParseTable, ParserGrammar},
    validated::ValidatedGrammar,
};

/// One prepared grammar: everything the parse entrypoint needs, built once so
/// the timed loop measures only parsing, never grammar preparation.
struct Prepared {
    validated: ValidatedGrammar,
    parser: ParserGrammar,
    table: ParseTable,
    plan: RuntimeWeavyPlan,
}

fn prepare(grammar_path: &str) -> Prepared {
    let json = snark_dsl::emit_with_boa(Path::new(grammar_path)).expect("emit");
    let raw = RawGrammarJson::from_tree_sitter_json_str(&json).expect("import");
    let validated = ValidatedGrammar::from_raw(&raw).expect("validate");
    let lexical = LexicalFacts::from_grammar(&validated);
    let normalized =
        ParserGrammar::normalize_from_validated(&validated, &lexical).expect("normalize");
    let parser = normalized.prepare_productions_for_items().expect("prepare");
    let table = ParseTable::from_grammar(&parser).expect("table");
    let plan = RuntimeWeavyPlan::new(&validated, &parser, &table).expect("plan");
    Prepared {
        validated,
        parser,
        table,
        plan,
    }
}

/// Best (min) parse time in ms over `iters` runs, after one warm-up.
fn best_parse_ms(p: &Prepared, input: &str, iters: usize) -> f64 {
    let _ = parse_prepared_runtime_with_report(&p.plan, &p.validated, &p.parser, &p.table, input);
    let mut best_ms = f64::INFINITY;
    for _ in 0..iters.max(1) {
        let start = Instant::now();
        let _ =
            parse_prepared_runtime_with_report(&p.plan, &p.validated, &p.parser, &p.table, input);
        best_ms = best_ms.min(start.elapsed().as_secs_f64() * 1000.0);
    }
    best_ms
}

#[derive(Facet)]
struct Row {
    k: u64,
    v: String,
}

/// `[{"k":0,"v":"x0"},…]` with `n` objects, via facet-json.
fn gen_json(n: u64) -> String {
    let rows: Vec<Row> = (0..n)
        .map(|k| Row {
            k,
            v: format!("x{k}"),
        })
        .collect();
    facet_json::to_string(&rows).expect("serialize fixture json")
}

/// Iterations scaled to input size, so small inputs still get a stable min and
/// large (currently quadratic) inputs don't take forever.
fn iters_for(bytes: usize) -> usize {
    match bytes {
        0..4_000 => 100,
        4_000..16_000 => 30,
        16_000..64_000 => 8,
        64_000..160_000 => 2,
        _ => 1,
    }
}

fn run_ladder(grammar_path: &str, max_objects: u64) {
    let p = prepare(grammar_path);
    println!(
        "{:>8} {:>10} {:>12} {:>12} {:>8}",
        "objects", "bytes", "min_ms", "bytes/ms", "x_prev"
    );
    let counts = [250u64, 500, 1000, 2000, 4000, 8000, 16000, 32000];
    let mut prev_ms: Option<f64> = None;
    for &n in &counts {
        if n > max_objects {
            break;
        }
        let input = gen_json(n);
        let bytes = input.len();
        let ms = best_parse_ms(&p, &input, iters_for(bytes));
        let bpm = bytes as f64 / ms;
        let x_prev = prev_ms.map(|prev| ms / prev).unwrap_or(0.0);
        println!("{n:>8} {bytes:>10} {ms:>12.3} {bpm:>12.0} {x_prev:>8.2}");
        prev_ms = Some(ms);
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.get(1).map(|s| s == "ladder").unwrap_or(false) {
        let grammar_path = args
            .get(2)
            .expect("usage: ladder <grammar.js> [max_objects]");
        let max_objects: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(8000);
        run_ladder(grammar_path, max_objects);
        return;
    }

    let grammar_path = args.get(1).expect("usage: <grammar.js> <input> [iters]");
    let input = fs::read_to_string(args.get(2).expect("input file")).expect("read input");
    let iters: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(30);

    let p = prepare(grammar_path);
    if let Err(e) =
        parse_prepared_runtime_with_report(&p.plan, &p.validated, &p.parser, &p.table, &input)
    {
        eprintln!("parse failed: {e:?}");
        std::process::exit(1);
    }
    let best_ms = best_parse_ms(&p, &input, iters);
    let bytes = input.len();
    println!(
        "snark weavy parse: min {best_ms:.2} ms over {iters} iters, {bytes} bytes, {:.0} bytes/ms",
        bytes as f64 / best_ms
    );
}
