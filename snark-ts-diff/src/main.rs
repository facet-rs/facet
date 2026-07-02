//! Parse-throughput bench for the snark Weavy runtime.
//!
//! Two modes:
//!
//!   # single input, prepare once, parse N times, report best (min) ms
//!   cargo run --release -p snark-ts-diff -- <grammar.js|grammar.json> <input-file> [iters]
//!
//!   # recovering parse, prepare once, parse N times, report best (min) ms
//!   cargo run --release -p snark-ts-diff -- recover <grammar.js|grammar.json> <input-file> [iters]
//!
//!   # lowering/JIT readiness for one grammar
//!   cargo run --release -p snark-ts-diff -- readiness <grammar.js|grammar.json>
//!
//!   # size ladder: prepare once, sweep JSON of growing object counts, print
//!   # a table of ms + bytes/ms + ratio-vs-previous. The `x_prev` column is the
//!   # tell: object counts double each row, so a LINEAR parser holds ~2.0 and a
//!   # QUADRATIC one climbs toward ~4.0 (and bytes/ms halves).
//!   cargo run --release -p snark-ts-diff -- ladder <grammar.js> [max_objects]
//!
//! Fixtures are generated with facet-json (never hand-emitted) as `[{"k":0,
//! "v":"x0"},…]`, which the bundled `jsonb` grammar accepts.

use std::process::Command;
use std::time::Instant;
use std::{env, fs, path::Path, path::PathBuf};

use facet::Facet;
use snark::{
    grammar::RawGrammarJson,
    lexical::LexicalFacts,
    lower::weavy::{
        WeavyParseError, WeavyParsePlan, WeavyParseReport,
        parse_prepared_weavy_recovering_with_report_and_scanner, parse_prepared_weavy_with_report,
    },
    parser::{ParseTable, ParserGrammar, TreeEvent},
    validated::ValidatedGrammar,
};

/// One prepared grammar: everything the parse entrypoint needs, built once so
/// the timed loop measures only parsing, never grammar preparation.
struct Prepared {
    parser: ParserGrammar,
    table: ParseTable,
    plan: WeavyParsePlan,
}

fn load_grammar_json(grammar_path: &str) -> String {
    let path = Path::new(grammar_path);
    if path
        .extension()
        .is_some_and(|extension| extension == "json")
    {
        return fs::read_to_string(path).expect("read grammar.json");
    }
    snark_dsl::emit_with_boa(path).expect("emit grammar.js")
}

fn prepare(grammar_path: &str) -> Prepared {
    let json = load_grammar_json(grammar_path);
    let raw = RawGrammarJson::from_tree_sitter_json_str(&json).expect("import");
    let validated = ValidatedGrammar::from_raw(&raw).expect("validate");
    let lexical = LexicalFacts::from_grammar(&validated);
    let normalized =
        ParserGrammar::normalize_from_validated(&validated, &lexical).expect("normalize");
    let parser = normalized.prepare_productions_for_items().expect("prepare");
    let table = ParseTable::from_grammar(&parser).expect("table");
    let plan = WeavyParsePlan::new(&validated, &parser, &table).expect("plan");
    Prepared {
        parser,
        table,
        plan,
    }
}

/// Profile grammar preparation, phase by phase, then loop `ParseTable::from_grammar`
/// so a sampler (stax) can attach to the table build in isolation.
fn run_tablebench(grammar_path: &str, iters: usize) {
    let t = Instant::now();
    let json = load_grammar_json(grammar_path);
    println!(
        "load grammar json: {:.1} ms",
        t.elapsed().as_secs_f64() * 1000.0
    );
    let raw = RawGrammarJson::from_tree_sitter_json_str(&json).expect("import");
    let validated = ValidatedGrammar::from_raw(&raw).expect("validate");
    let lexical = LexicalFacts::from_grammar(&validated);
    let t = Instant::now();
    let normalized =
        ParserGrammar::normalize_from_validated(&validated, &lexical).expect("normalize");
    let parser = normalized.prepare_productions_for_items().expect("prepare");
    println!(
        "normalize + prepare productions: {:.1} ms  ({} productions)",
        t.elapsed().as_secs_f64() * 1000.0,
        parser.productions().len()
    );
    println!("looping ParseTable::from_grammar {iters}x (attach stax now) …");
    let mut best = f64::INFINITY;
    for i in 0..iters.max(1) {
        let start = Instant::now();
        let table = ParseTable::from_grammar(&parser).expect("table");
        let ms = start.elapsed().as_secs_f64() * 1000.0;
        best = best.min(ms);
        std::hint::black_box(&table);
        println!("  table build iter {i}: {ms:.1} ms (min {best:.1} ms)");
    }
}

/// Best (min) parse time in ms over `iters` runs, after one warm-up.
fn best_parse_ms(p: &Prepared, input: &str, iters: usize) -> f64 {
    let _ = parse_prepared_weavy_with_report(&p.plan, &p.parser, &p.table, input);
    let mut best_ms = f64::INFINITY;
    for _ in 0..iters.max(1) {
        let start = Instant::now();
        let _ = parse_prepared_weavy_with_report(&p.plan, &p.parser, &p.table, input);
        best_ms = best_ms.min(start.elapsed().as_secs_f64() * 1000.0);
    }
    best_ms
}

fn recover_once(p: &Prepared, input: &str) -> Result<WeavyParseReport, WeavyParseError> {
    parse_prepared_weavy_recovering_with_report_and_scanner(
        &p.plan, &p.parser, &p.table, input, None,
    )
}

/// Best (min) recovering parse time in ms over `iters` runs, after one warm-up.
fn best_recover_ms(p: &Prepared, input: &str, iters: usize) -> Result<f64, WeavyParseError> {
    let _ = recover_once(p, input)?;
    let mut best_ms = f64::INFINITY;
    for _ in 0..iters.max(1) {
        let start = Instant::now();
        let _ = recover_once(p, input)?;
        best_ms = best_ms.min(start.elapsed().as_secs_f64() * 1000.0);
    }
    Ok(best_ms)
}

fn error_counts(report: &WeavyParseReport) -> (usize, usize) {
    report
        .tree_events()
        .iter()
        .fold((0, 0), |(errors, missing), event| match event {
            TreeEvent::Error { .. } => (errors + 1, missing),
            TreeEvent::Missing { .. } => (errors, missing + 1),
            _ => (errors, missing),
        })
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

/// Generate a real tree-sitter parser for `grammar_path` in a scratch dir and
/// return it, or `None` if the `tree-sitter` CLI is missing / generate fails.
/// The reference is tree-sitter's OUTPUT/behaviour, never its generated `.c`.
fn tree_sitter_setup(grammar_path: &str) -> Option<PathBuf> {
    if Path::new(grammar_path)
        .extension()
        .is_some_and(|extension| extension != "js")
    {
        return None;
    }
    let ok = Command::new("tree-sitter")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !ok {
        return None;
    }
    let dir = env::temp_dir().join("snark-ts-diff-ladder");
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).ok()?;
    fs::copy(grammar_path, dir.join("grammar.js")).ok()?;
    let out = Command::new("tree-sitter")
        .arg("generate")
        .current_dir(&dir)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    Some(dir)
}

/// Best (min) tree-sitter parse time in ms for `input`, via `parse --time`
/// (internal parse duration, excludes process startup + parser load). Warms
/// once so the on-demand C-parser compile isn't charged to the measurement.
fn tree_sitter_best_ms(dir: &Path, input: &str, iters: usize) -> Option<f64> {
    let file = dir.join("in.json");
    fs::write(&file, input).ok()?;
    let run = || -> Option<f64> {
        let out = Command::new("tree-sitter")
            .args(["parse", "in.json", "--quiet", "--time"])
            .current_dir(dir)
            .output()
            .ok()?;
        let text = String::from_utf8_lossy(&out.stdout);
        text.lines()
            .rev()
            .find_map(|line| line.split("Parse:").nth(1))
            .and_then(|rest| rest.split("ms").next())
            .and_then(|ms| ms.trim().parse::<f64>().ok())
    };
    run(); // warm
    let mut best = f64::INFINITY;
    for _ in 0..iters.max(1) {
        if let Some(ms) = run() {
            best = best.min(ms);
        }
    }
    best.is_finite().then_some(best)
}

fn run_ladder(grammar_path: &str, max_objects: u64) {
    let p = prepare(grammar_path);
    let ts_dir = tree_sitter_setup(grammar_path);
    if ts_dir.is_none() {
        eprintln!("note: `tree-sitter` CLI unavailable — snark-only ladder");
    }
    println!(
        "{:>8} {:>10} {:>12} {:>7} {:>12} {:>7} {:>10}",
        "objects", "bytes", "snark_ms", "snk_x", "ts_ms", "ts_x", "snark/ts"
    );
    let counts = [250u64, 500, 1000, 2000, 4000, 8000, 16000, 32000];
    let (mut prev_snark, mut prev_ts): (Option<f64>, Option<f64>) = (None, None);
    for &n in &counts {
        if n > max_objects {
            break;
        }
        let input = gen_json(n);
        let bytes = input.len();
        let iters = iters_for(bytes);

        let snark_ms = best_parse_ms(&p, &input, iters);
        let snk_x = prev_snark.map(|prev| snark_ms / prev).unwrap_or(0.0);

        let ts_ms = ts_dir
            .as_deref()
            .and_then(|dir| tree_sitter_best_ms(dir, &input, iters.min(10)));
        let ts_x = match (ts_ms, prev_ts) {
            (Some(cur), Some(prev)) if prev > 0.0 => cur / prev,
            _ => 0.0,
        };
        let ratio = ts_ms.map(|ts| snark_ms / ts).unwrap_or(0.0);

        let ts_ms_s = ts_ms
            .map(|v| format!("{v:.3}"))
            .unwrap_or_else(|| "-".into());
        println!(
            "{n:>8} {bytes:>10} {snark_ms:>12.3} {snk_x:>7.2} {ts_ms_s:>12} {ts_x:>7.2} {ratio:>10.0}"
        );
        prev_snark = Some(snark_ms);
        prev_ts = ts_ms;
    }
}

fn run_readiness(grammar_path: &str) {
    let p = prepare(grammar_path);
    let analysis = p.plan.analysis();
    let readiness = &analysis.readiness;
    let lexer = &readiness.lexer;
    println!("grammar: {grammar_path}");
    println!(
        "parser: neutral_ops={} snark_intrinsics={} lexer_graph={} sink={} dialect={} host_barriers={} opaque={} host_calls={} stencils_needed={} native_copy_patch_jit_available={}",
        readiness.neutral_weavy_op_count,
        readiness.snark_intrinsic_count,
        readiness.lexer_graph_intrinsic_count,
        readiness.sink_op_intrinsic_count,
        readiness.dialect_op_intrinsic_count,
        readiness.host_call_barrier_intrinsic_count,
        readiness.opaque_intrinsic_count,
        readiness.host_call_intrinsic_count,
        readiness.needs_snark_stencils(),
        readiness.native_copy_patch_jit_available
    );
    println!(
        "lexer: modes={} terminals={} literal_sets={}/{} pattern_sets={}/{} dfa_sets={}/{} leaf_rematch={} known_patterns={} regex_automata={} rust_regex_fallback={} unsupported_patterns={} unsupported_terminals={} unsupported_symbols={} external_scanners={}",
        analysis.lexer.mode_count,
        analysis.lexer.terminal_count,
        lexer.merged_literal_set_count,
        lexer.merged_literal_terminal_count,
        lexer.merged_pattern_set_count,
        lexer.merged_pattern_terminal_count,
        lexer.merged_pattern_dfa_set_count,
        lexer.merged_pattern_dfa_terminal_count,
        lexer.merged_pattern_leaf_rematch_terminal_count,
        lexer.known_pattern_count,
        lexer.regex_automata_count,
        lexer.rust_regex_fallback_count,
        lexer.unsupported_pattern_count,
        lexer.unsupported_terminal_count,
        lexer.unsupported_symbol_count,
        lexer.external_scanner_candidate_count
    );
    println!(
        "visibility: parser={} lexer={} full={} neutral_only={}",
        readiness.is_parser_fully_visible(),
        lexer.is_fully_visible(),
        readiness.is_fully_visible(),
        readiness.is_neutral_weavy_only()
    );
    if analysis.lexer.op_counts.is_empty() {
        println!("lexer_ops: none");
    } else {
        println!("lexer_ops:");
        for (kind, count) in &analysis.lexer.op_counts {
            println!("  {kind:?}: {count}");
        }
    }
    if readiness.barrier_summaries.is_empty() {
        println!("barriers: none");
    } else {
        println!("barriers:");
        for summary in &readiness.barrier_summaries {
            println!("  {:?}: {}", summary.barrier, summary.count);
        }
    }
    if readiness.snark_stencil_summaries.is_empty() {
        println!("stencil_descriptors: none");
    } else {
        println!("stencil_descriptors:");
        for summary in &readiness.snark_stencil_summaries {
            println!(
                "  {}.{} domain={:?} lowering={:?} family={:?} execution={:?} effect_order={:?} may_fail={} may_allocate={} calls_user_code={} opaque={} resources={:?} typed_memory={:?} state={:?} count={}",
                summary.descriptor.dialect,
                summary.descriptor.name,
                summary.domain,
                summary.lowering,
                summary.stencil.family,
                summary.stencil.execution,
                summary.effect.ordering,
                summary.effect.may_fail,
                summary.effect.may_allocate,
                summary.effect.calls_user_code,
                summary.effect.opaque,
                summary.effect.resources,
                summary.effect.typed_memory,
                summary.stencil.state,
                summary.count
            );
        }
    }
    if readiness.snark_stencil_family_summaries.is_empty() {
        println!("stencil_families: none");
    } else {
        println!("stencil_families:");
        for summary in &readiness.snark_stencil_family_summaries {
            println!(
                "  {:?}/{:?}: {}",
                summary.family, summary.execution, summary.count
            );
        }
    }
    if readiness.snark_stencil_state_summaries.is_empty() {
        println!("stencil_state: none");
    } else {
        println!("stencil_state:");
        for summary in &readiness.snark_stencil_state_summaries {
            println!("  {:?}: {}", summary.state, summary.count);
        }
    }
}

fn main() {
    let args: Vec<String> = env::args().collect();

    if args.get(1).map(|s| s == "recover").unwrap_or(false) {
        let grammar_path = args
            .get(2)
            .expect("usage: recover <grammar.js|grammar.json> <input> [iters]");
        let input = fs::read_to_string(args.get(3).expect("input file")).expect("read input");
        let iters: usize = args.get(4).and_then(|s| s.parse().ok()).unwrap_or(30);
        let p = prepare(grammar_path);
        let report = match recover_once(&p, &input) {
            Ok(report) => report,
            Err(error) => {
                eprintln!("recovering parse failed: {error:?}");
                std::process::exit(1);
            }
        };
        let (errors, missing) = error_counts(&report);
        let best_ms = match best_recover_ms(&p, &input, iters) {
            Ok(best_ms) => best_ms,
            Err(error) => {
                eprintln!("recovering parse failed during timing: {error:?}");
                std::process::exit(1);
            }
        };
        let bytes = input.len();
        println!(
            "snark weavy recovering parse: min {best_ms:.2} ms over {iters} iters, {bytes} bytes, {:.0} bytes/ms",
            bytes as f64 / best_ms
        );
        println!(
            "accepted={} failed={} max_live={} errors={} missing={}",
            report.accepted_count(),
            report.failure_count(),
            report.max_live_versions(),
            errors,
            missing
        );
        return;
    }

    if args.get(1).map(|s| s == "readiness").unwrap_or(false) {
        let grammar_path = args
            .get(2)
            .expect("usage: readiness <grammar.js|grammar.json>");
        run_readiness(grammar_path);
        return;
    }

    if args.get(1).map(|s| s == "tablebench").unwrap_or(false) {
        let grammar_path = args
            .get(2)
            .expect("usage: tablebench <grammar.js|grammar.json> [iters]");
        let iters: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(50);
        run_tablebench(grammar_path, iters);
        return;
    }

    if args.get(1).map(|s| s == "gen").unwrap_or(false) {
        let n: u64 = args
            .get(2)
            .and_then(|s| s.parse().ok())
            .expect("usage: gen <objects> <out-file>");
        let out = args.get(3).expect("usage: gen <objects> <out-file>");
        fs::write(out, gen_json(n)).expect("write fixture");
        return;
    }

    if args.get(1).map(|s| s == "gennest").unwrap_or(false) {
        let depth: usize = args
            .get(2)
            .and_then(|s| s.parse().ok())
            .expect("usage: gennest <depth> <out-file>");
        let out = args.get(3).expect("usage: gennest <depth> <out-file>");
        // Structural stress fixture: depth-D nested single-child arrays with a
        // string at the center — `[[[…"x"…]]]`. Pure reduce DEPTH, no wide
        // repeat, contrasting the flat `gen` fixture (pure repeat WIDTH). If
        // this scales linearly while flat goes super-linear, the reduce O(n^2)
        // is repeat-width (hidden-rule re-flatten), not depth.
        let mut s = String::with_capacity(depth * 2 + 3);
        for _ in 0..depth {
            s.push('[');
        }
        s.push_str("\"x\"");
        for _ in 0..depth {
            s.push(']');
        }
        fs::write(out, s).expect("write fixture");
        return;
    }

    if args.get(1).map(|s| s == "ladder").unwrap_or(false) {
        let grammar_path = args
            .get(2)
            .expect("usage: ladder <grammar.js|grammar.json> [max_objects]");
        let max_objects: u64 = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(8000);
        run_ladder(grammar_path, max_objects);
        return;
    }

    let grammar_path = args
        .get(1)
        .expect("usage: <grammar.js|grammar.json> <input> [iters]");
    let input = fs::read_to_string(args.get(2).expect("input file")).expect("read input");
    let iters: usize = args.get(3).and_then(|s| s.parse().ok()).unwrap_or(30);

    let p = prepare(grammar_path);
    if let Err(e) = parse_prepared_weavy_with_report(&p.plan, &p.parser, &p.table, &input) {
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
