//! Benchmark the prepared Weavy runtime path on the same grammar + input, separating
//! one-time setup from fresh-plan and warm-plan per-parse cost.
//!
//! Usage: cargo run --release -p snark --features json-import,weavy-lowering \
//!          --example weavy_bench -- [GRAMMAR_JS] [INPUT_FILE] [ITERS] [all|strict|recovering]

use std::{env, path::PathBuf, time::Instant};

use snark::{
    grammar::RawGrammarJson,
    lexical::LexicalFacts,
    lower::weavy::{
        WeavyParsePlan, parse_prepared_weavy_recovering_with_report_and_scanner,
        parse_prepared_weavy_with_report_and_scanner,
    },
    parser::{ParseTable, ParserGrammar},
    validated::ValidatedGrammar,
};

fn ms(d: std::time::Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BenchMode {
    All,
    Strict,
    Recovering,
}

impl BenchMode {
    fn from_arg(arg: Option<String>) -> Self {
        match arg.as_deref() {
            None | Some("all") => Self::All,
            Some("strict") => Self::Strict,
            Some("recovering") => Self::Recovering,
            Some(other) => {
                panic!("unknown benchmark mode {other:?}; expected all|strict|recovering")
            }
        }
    }

    const fn runs_strict_fresh(self) -> bool {
        matches!(self, Self::All)
    }

    const fn runs_strict_warm(self) -> bool {
        matches!(self, Self::All | Self::Strict)
    }

    const fn runs_recovering_warm(self) -> bool {
        matches!(self, Self::All | Self::Recovering)
    }
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
    let mode = BenchMode::from_arg(env::args().nth(4));

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

    let t = Instant::now();
    let plan = WeavyParsePlan::new(&validated, &parser, &table).expect("weavy parse plan");
    let plan_new = t.elapsed();

    let strict_fresh_plan_total = mode.runs_strict_fresh().then(|| {
        let t = Instant::now();
        for _ in 0..iters {
            let plan = WeavyParsePlan::new(&validated, &parser, &table).expect("weavy parse plan");
            let _ = parse_prepared_weavy_with_report_and_scanner(
                &plan, &validated, &parser, &table, &input, None,
            );
        }
        t.elapsed()
    });

    let strict_warm_plan_total = mode.runs_strict_warm().then(|| {
        let t = Instant::now();
        for _ in 0..iters {
            let _ = parse_prepared_weavy_with_report_and_scanner(
                &plan, &validated, &parser, &table, &input, None,
            );
        }
        t.elapsed()
    });

    let recovering_warm_plan_total = mode.runs_recovering_warm().then(|| {
        let t = Instant::now();
        for _ in 0..iters {
            let _ = parse_prepared_weavy_recovering_with_report_and_scanner(
                &plan, &validated, &parser, &table, &input, None,
            );
        }
        t.elapsed()
    });

    println!("grammar: {}", grammar_js.display());
    println!("input:   {} ({} bytes)", input_file.display(), input.len());
    println!("iters:   {iters}\n");
    println!("mode:    {mode:?}\n");

    println!("one-time setup:");
    println!("  ParseTable::from_grammar   {:>8.1} ms", ms(table_build));
    println!("  WeavyParsePlan::new      {:>8.1} ms", ms(plan_new));

    println!("\nper-parse (avg over {iters}):");
    if let Some(total) = strict_fresh_plan_total {
        println!(
            "  weavy strict, fresh plan   {:>8.3} ms",
            ms(total) / iters as f64
        );
    }
    if let Some(total) = strict_warm_plan_total {
        println!(
            "  weavy strict, warm plan    {:>8.3} ms",
            ms(total) / iters as f64
        );
    }
    if let Some(total) = recovering_warm_plan_total {
        println!(
            "  weavy recovering, warm     {:>8.3} ms",
            ms(total) / iters as f64
        );
    }
}
