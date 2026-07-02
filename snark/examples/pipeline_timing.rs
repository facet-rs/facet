//! Phase-by-phase timing of the full grammar pipeline for one grammar, native release.
//! Records where the time actually goes: grammar.js -> JSON (boa) -> decode -> validate
//! -> lexical -> normalize -> prepare -> parse table -> Weavy parse plan ->
//! parse. Compare the table-build line against everything else.
//!
//! Usage: cargo run --release -p snark --features json-import \
//!          --example pipeline_timing -- [GRAMMAR_JS] [SAMPLE]

use std::{env, path::PathBuf, time::Instant};

use snark::{
    grammar::RawGrammarJson,
    lexical::LexicalFacts,
    lower::weavy::{
        WeavyParsePlan, parse_prepared_weavy_recovering_with_report_and_scanner,
        parse_prepared_weavy_tree, parse_prepared_weavy_with_report,
    },
    parser::{ParseTable, ParserGrammar},
    validated::ValidatedGrammar,
};

fn main() {
    let repo = env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .and_then(|p| p.parent().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."));
    let grammar_js = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| repo.join("playgrounds/snark/src/bundled/gingembre/grammar.js"));
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

    let (t_emit, grammar_json) = phase!(
        "emit",
        snark_dsl::emit_with_boa(&grammar_js).expect("emit grammar.js -> json")
    );
    let json_bytes = grammar_json.len();
    let (t_decode, raw) = phase!(
        "decode",
        RawGrammarJson::from_tree_sitter_json_str(&grammar_json).expect("decode json")
    );
    let (t_validate, validated) = phase!(
        "validate",
        ValidatedGrammar::from_raw(&raw).expect("validate")
    );
    let (t_lexical, lexical) = phase!("lexical", LexicalFacts::from_grammar(&validated));
    let (t_normalize, normalized) = phase!(
        "normalize",
        ParserGrammar::normalize_from_validated(&validated, &lexical).expect("normalize")
    );
    let (t_prepare, parser) = phase!(
        "prepare",
        normalized
            .prepare_productions_for_items()
            .expect("prepare productions")
    );
    let (t_table, table) = phase!(
        "table",
        ParseTable::from_grammar(&parser).expect("build parse table")
    );
    let (t_plan, plan) = phase!(
        "plan",
        WeavyParsePlan::new(&validated, &parser, &table).expect("Weavy parse plan")
    );
    let analysis = plan.analysis();
    // Measure the lean tree path separately from the rich report path. The
    // report path is still what diagnostics, recovery, and incremental reuse
    // consumers need; the tree path is the valid-input fast consumer shape.
    let t0 = Instant::now();
    let strict_tree = parse_prepared_weavy_tree(&plan, &parser, &table, &input);
    let t_strict_tree = t0.elapsed().as_secs_f64() * 1000.0;
    let strict_tree_ok = strict_tree.is_ok();

    let t0 = Instant::now();
    let strict_report = parse_prepared_weavy_with_report(&plan, &parser, &table, &input);
    let t_strict_report = t0.elapsed().as_secs_f64() * 1000.0;
    let strict_report_ok = strict_report.is_ok();

    let t0 = Instant::now();
    let _ = parse_prepared_weavy_recovering_with_report_and_scanner(
        &plan, &parser, &table, &input, None,
    );
    let t_recovering = t0.elapsed().as_secs_f64() * 1000.0;

    let prepare_total =
        t_emit + t_decode + t_validate + t_lexical + t_normalize + t_prepare + t_table + t_plan;

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
        ("Weavy parse plan", t_plan),
    ];
    println!("---- one-time prepare ----");
    for (label, ms) in rows {
        let pct = if prepare_total > 0.0 {
            ms / prepare_total * 100.0
        } else {
            0.0
        };
        println!("  {label:<32} {ms:>9.1} ms  {pct:>5.1}%");
    }
    println!("  {:<32} {:>9.1} ms", "prepare TOTAL", prepare_total);
    println!("\n---- steady state (parse, same input) ----");
    println!(
        "  {:<40} {:>11.3} ms   (strict ok? {})",
        "parse_prepared_weavy_tree", t_strict_tree, strict_tree_ok
    );
    println!(
        "  {:<40} {:>11.3} ms   (strict ok? {})",
        "parse_prepared_weavy_with_report", t_strict_report, strict_report_ok
    );
    println!(
        "  {:<40} {:>11.3} ms",
        "parse_prepared_weavy_recovering", t_recovering
    );

    println!("\n---- weavy lowering readiness ----");
    println!(
        "  parser ops: {} neutral, {} snark intrinsics",
        analysis.readiness.neutral_weavy_op_count, analysis.readiness.snark_intrinsic_count
    );
    println!(
        "  intrinsic lanes: {} dialect, {} lexer graph, {} sink, {} host-call barrier",
        analysis.readiness.dialect_op_intrinsic_count,
        analysis.readiness.lexer_graph_intrinsic_count,
        analysis.readiness.sink_op_intrinsic_count,
        analysis.readiness.host_call_barrier_intrinsic_count
    );
    println!(
        "  lexer leaves: {} regex-automata, {} unsupported pattern, {} unsupported terminal, {} unsupported symbol",
        analysis.readiness.lexer.regex_automata_count,
        analysis.readiness.lexer.unsupported_pattern_count,
        analysis.readiness.lexer.unsupported_terminal_count,
        analysis.readiness.lexer.unsupported_symbol_count
    );
    println!(
        "  merged lexer sets: {} literal sets ({} terminals), {} pattern sets ({} terminals)",
        analysis.readiness.lexer.merged_literal_set_count,
        analysis.readiness.lexer.merged_literal_terminal_count,
        analysis.readiness.lexer.merged_pattern_set_count,
        analysis.readiness.lexer.merged_pattern_terminal_count
    );
    println!(
        "  fully visible? parser={} all={} neutral-weavy-only={} needs-snark-stencils={}",
        analysis.readiness.is_parser_fully_visible(),
        analysis.readiness.is_fully_visible(),
        analysis.readiness.is_neutral_weavy_only(),
        analysis.readiness.needs_snark_stencils()
    );
    if analysis.readiness.barrier_summaries.is_empty() {
        println!("  blockers: none");
    } else {
        println!("  blockers:");
        for summary in &analysis.readiness.barrier_summaries {
            println!("    {:?}: {}", summary.barrier, summary.count);
        }
    }

    if !analysis.readiness.snark_stencil_family_summaries.is_empty() {
        println!("  stencil families:");
        for summary in &analysis.readiness.snark_stencil_family_summaries {
            println!(
                "    {:?}/{:?}: {}  state={:?}  effect={:?} fail={} alloc={} user={} opaque={}",
                summary.family,
                summary.execution,
                summary.count,
                summary.state,
                summary.effect.ordering,
                summary.effect.may_fail,
                summary.effect.may_allocate,
                summary.effect.calls_user_code,
                summary.effect.opaque
            );
        }
    }

    if !analysis
        .readiness
        .snark_stencil_execution_summaries
        .is_empty()
    {
        println!("  stencil execution lanes:");
        for summary in &analysis.readiness.snark_stencil_execution_summaries {
            println!(
                "    {:?}: {}  families={:?}  state={:?}  effect={:?} fail={} alloc={} user={} opaque={}",
                summary.execution,
                summary.count,
                summary.families,
                summary.state,
                summary.effect.ordering,
                summary.effect.may_fail,
                summary.effect.may_allocate,
                summary.effect.calls_user_code,
                summary.effect.opaque
            );
        }
    }

    if !analysis.readiness.snark_stencil_state_summaries.is_empty() {
        println!("  stencil state surfaces:");
        for summary in &analysis.readiness.snark_stencil_state_summaries {
            println!("    {:?}: {}", summary.state, summary.count);
        }
    }
}
