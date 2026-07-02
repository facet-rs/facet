//! Benchmark the prepared Weavy runtime path on the same grammar + input, separating
//! one-time setup from fresh-plan and warm-plan per-parse cost.
//!
//! Usage: cargo run --release -p snark --features json-import \
//!          --example weavy_bench -- [GRAMMAR_JS] [INPUT_FILE] [ITERS] [all|tree|resolved|report|recovering|hostcalls]

use std::{
    collections::BTreeMap,
    env,
    path::PathBuf,
    time::{Duration, Instant},
};

use snark::{
    grammar::RawGrammarJson,
    lexical::LexicalFacts,
    lower::weavy::{
        SnarkStencilProfile, WeavyLexerExecutionStats, WeavyParseError, WeavyParsePlan,
        WeavyParseReport, WeavySnarkExecutionStats, WeavySnarkProfileStencilReadiness,
        parse_prepared_weavy_recovering_with_report_and_scanner,
        parse_prepared_weavy_resolved_tree_and_scanner, parse_prepared_weavy_tree_and_scanner,
        parse_prepared_weavy_with_report_and_scanner,
    },
    parser::{ParseTable, ParserGrammar},
    validated::ValidatedGrammar,
};
use weavy::{RunStats, ir::lowered_analysis};

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
use snark::lower::weavy::parse_prepared_weavy_hostcalls_with_report_and_scanner;

fn ms(d: std::time::Duration) -> f64 {
    d.as_secs_f64() * 1000.0
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BenchMode {
    All,
    StrictTree,
    StrictResolvedTree,
    StrictReport,
    Recovering,
    #[cfg(all(
        feature = "jit",
        any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64")
        )
    ))]
    HostCallsReport,
}

impl BenchMode {
    fn from_arg(arg: Option<String>) -> Self {
        match arg.as_deref() {
            None | Some("all") => Self::All,
            Some("strict" | "tree") => Self::StrictTree,
            Some("resolved" | "resolved-tree") => Self::StrictResolvedTree,
            Some("report" | "strict-report") => Self::StrictReport,
            Some("recovering") => Self::Recovering,
            Some("hostcalls" | "hostcalls-report") => Self::hostcalls_report_mode(),
            Some(other) => {
                panic!(
                    "unknown benchmark mode {other:?}; expected all|tree|resolved|report|recovering|hostcalls"
                )
            }
        }
    }

    #[cfg(all(
        feature = "jit",
        any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64")
        )
    ))]
    const fn hostcalls_report_mode() -> Self {
        Self::HostCallsReport
    }

    #[cfg(not(all(
        feature = "jit",
        any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64")
        )
    )))]
    fn hostcalls_report_mode() -> Self {
        panic!("hostcalls benchmark mode requires the snark jit feature on a supported target")
    }

    const fn runs_strict_tree_fresh(self) -> bool {
        matches!(self, Self::All)
    }

    const fn runs_strict_tree_warm(self) -> bool {
        matches!(self, Self::All | Self::StrictTree)
    }

    const fn runs_strict_resolved_tree_warm(self) -> bool {
        matches!(self, Self::All | Self::StrictResolvedTree)
    }

    const fn runs_strict_report_warm(self) -> bool {
        matches!(self, Self::All | Self::StrictReport)
    }

    const fn runs_recovering_warm(self) -> bool {
        matches!(self, Self::All | Self::Recovering)
    }

    #[cfg(all(
        feature = "jit",
        any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64")
        )
    ))]
    const fn runs_hostcalls_report_warm(self) -> bool {
        matches!(self, Self::All | Self::HostCallsReport)
    }
}

#[derive(Clone, Debug, Default)]
struct BenchTotals {
    duration: Duration,
    stats: RunStats,
    lexer_stats: WeavyLexerExecutionStats,
    snark_stats: WeavySnarkExecutionStats,
    execution_lanes: BTreeMap<String, usize>,
    successes: usize,
    failures: usize,
    runner_samples: usize,
}

fn add_run_stats(total: &mut RunStats, next: RunStats) {
    total.step_count += next.step_count;
    total.inline_call_count += next.inline_call_count;
    total.block_call_count += next.block_call_count;
    total.return_count += next.return_count;
    total.continuation_resume_count += next.continuation_resume_count;
    total.max_frame_depth = total.max_frame_depth.max(next.max_frame_depth);
}

fn add_lexer_execution_stats(
    total: &mut WeavyLexerExecutionStats,
    next: &WeavyLexerExecutionStats,
) {
    total.lex_call_count += next.lex_call_count;
    total.direct_set_cache_hit_count += next.direct_set_cache_hit_count;
    total.direct_set_cache_miss_count += next.direct_set_cache_miss_count;
    for (kind, count) in &next.stencil_executions {
        *total.stencil_executions.entry(*kind).or_default() += count;
    }
}

fn add_snark_execution_stats(
    total: &mut WeavySnarkExecutionStats,
    next: &WeavySnarkExecutionStats,
) {
    total.intrinsic_count += next.intrinsic_count;
    for (descriptor, count) in &next.descriptor_executions {
        *total.descriptor_executions.entry(*descriptor).or_default() += count;
    }
    for (domain, count) in &next.domain_executions {
        *total.domain_executions.entry(*domain).or_default() += count;
    }
    for (family, count) in &next.family_executions {
        *total.family_executions.entry(*family).or_default() += count;
    }
}

fn bench_parse<F>(iters: usize, mut parse: F) -> BenchTotals
where
    F: FnMut() -> Result<WeavyParseReport, WeavyParseError>,
{
    let t = Instant::now();
    let mut totals = BenchTotals::default();
    for _ in 0..iters {
        match parse() {
            Ok(report) => {
                totals.successes += 1;
                totals.runner_samples += 1;
                add_run_stats(&mut totals.stats, report.stats());
                add_lexer_execution_stats(&mut totals.lexer_stats, report.lexer_stats());
                add_snark_execution_stats(&mut totals.snark_stats, report.snark_stats());
                *totals
                    .execution_lanes
                    .entry(format!("{:?}", report.execution_lane()))
                    .or_default() += 1;
            }
            Err(_) => {
                totals.failures += 1;
            }
        }
    }
    totals.duration = t.elapsed();
    totals
}

fn bench_tree_parse<F, T>(iters: usize, mut parse: F) -> BenchTotals
where
    F: FnMut() -> Result<T, WeavyParseError>,
{
    let t = Instant::now();
    let mut totals = BenchTotals::default();
    for _ in 0..iters {
        match parse() {
            Ok(_) => {
                totals.successes += 1;
            }
            Err(_) => {
                totals.failures += 1;
            }
        }
    }
    totals.duration = t.elapsed();
    totals
}

fn average_count(total: usize, divisor: usize) -> f64 {
    if divisor == 0 {
        0.0
    } else {
        total as f64 / divisor as f64
    }
}

fn print_bench_totals(label: &str, totals: &BenchTotals, iters: usize) {
    println!(
        "  {label:<28} {:>8.3} ms  ok {:>4}  fail {:>4}",
        ms(totals.duration) / iters as f64,
        totals.successes,
        totals.failures
    );
    if totals.runner_samples == 0 {
        return;
    }
    println!(
        "      avg runner: steps {:>9.1}  block calls {:>9.1}  returns {:>9.1}  max depth {:>4}",
        average_count(totals.stats.step_count, totals.runner_samples),
        average_count(totals.stats.block_call_count, totals.runner_samples),
        average_count(totals.stats.return_count, totals.runner_samples),
        totals.stats.max_frame_depth
    );
    if !totals.execution_lanes.is_empty() {
        let lanes = totals
            .execution_lanes
            .iter()
            .map(|(lane, count)| format!("{lane}={count}"))
            .collect::<Vec<_>>()
            .join(",");
        println!("      lanes: {lanes}");
    }
    let snark_summaries = totals.snark_stats.family_execution_summaries();
    if let Some(summary) = snark_summaries.first() {
        println!(
            "      avg snark: intrinsics {:>9.1}  dominant {:?}/{:?} {:>9.1}",
            average_count(totals.snark_stats.intrinsic_count, totals.runner_samples),
            summary.family,
            summary.execution,
            average_count(summary.count, totals.runner_samples)
        );
    }
    let lexer_summaries = totals.lexer_stats.stencil_execution_summaries();
    if let Some(summary) = lexer_summaries.first() {
        println!(
            "      avg lexer: calls {:>9.1}  cache hit/miss {:>9.1}/{:<9.1}  dominant {:?} {:>9.1}",
            average_count(totals.lexer_stats.lex_call_count, totals.runner_samples),
            average_count(
                totals.lexer_stats.direct_set_cache_hit_count,
                totals.runner_samples
            ),
            average_count(
                totals.lexer_stats.direct_set_cache_miss_count,
                totals.runner_samples
            ),
            summary.kind,
            average_count(summary.count, totals.runner_samples)
        );
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
    let analysis = lowered_analysis(plan.program().lowered());
    let plan_analysis = plan.analysis();

    let strict_tree_fresh_plan_total = mode.runs_strict_tree_fresh().then(|| {
        bench_tree_parse(iters, || {
            let plan = WeavyParsePlan::new(&validated, &parser, &table).expect("weavy parse plan");
            parse_prepared_weavy_tree_and_scanner(&plan, &parser, &table, &input, None)
        })
    });

    let strict_tree_warm_plan_total = mode.runs_strict_tree_warm().then(|| {
        bench_tree_parse(iters, || {
            parse_prepared_weavy_tree_and_scanner(&plan, &parser, &table, &input, None)
        })
    });

    let strict_resolved_tree_warm_plan_total = mode.runs_strict_resolved_tree_warm().then(|| {
        bench_tree_parse(iters, || {
            parse_prepared_weavy_resolved_tree_and_scanner(&plan, &parser, &table, &input, None)
        })
    });

    let strict_report_warm_plan_total = mode.runs_strict_report_warm().then(|| {
        bench_parse(iters, || {
            parse_prepared_weavy_with_report_and_scanner(&plan, &parser, &table, &input, None)
        })
    });

    let recovering_warm_plan_total = mode.runs_recovering_warm().then(|| {
        bench_parse(iters, || {
            parse_prepared_weavy_recovering_with_report_and_scanner(
                &plan, &parser, &table, &input, None,
            )
        })
    });

    #[cfg(all(
        feature = "jit",
        any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64")
        )
    ))]
    let hostcalls_report_warm_plan_total = mode.runs_hostcalls_report_warm().then(|| {
        bench_parse(iters, || {
            parse_prepared_weavy_hostcalls_with_report_and_scanner(
                &plan, &parser, &table, &input, None,
            )
        })
    });

    println!("grammar: {}", grammar_js.display());
    println!("input:   {} ({} bytes)", input_file.display(), input.len());
    println!("iters:   {iters}\n");
    println!("mode:    {mode:?}\n");

    println!("one-time setup:");
    println!("  ParseTable::from_grammar   {:>8.1} ms", ms(table_build));
    println!("  WeavyParsePlan::new      {:>8.1} ms", ms(plan_new));

    let shape = analysis.program_stats;
    println!("\nlowered program:");
    println!(
        "  blocks {:>6}  ops total/root/blocks {:>6}/{:>4}/{:>6}",
        shape.block_count, shape.total.op_count, shape.root.op_count, shape.blocks.op_count
    );
    println!(
        "  op mix: control {:>6}  intrinsic {:>6}  memory {:>6}  aggregate {:>6}",
        shape.total.control_op_count,
        shape.total.intrinsic_op_count,
        shape.total.memory_op_count,
        shape.total.aggregate_op_count
    );
    println!(
        "  effects: ordered {:>6}  barriers {:>6}  may-fail {:>6}  side {:>6}",
        analysis.effect_stats.total.ordered_count,
        analysis.effect_stats.total.barrier_count,
        analysis.effect_stats.total.may_fail_count,
        analysis.effect_stats.total.side_channel_count
    );
    let readiness = &plan_analysis.readiness;
    println!(
        "  readiness: full {:<5}  parser {:<5}  lexer {:<5}  neutral {:<5}",
        readiness.is_fully_visible(),
        readiness.is_parser_fully_visible(),
        readiness.lexer.is_fully_visible(),
        readiness.is_neutral_weavy_only()
    );
    println!(
        "  weavy op ownership: neutral {:>6}  snark intrinsics {:>6}  parser stencils {:<5}  lexer stencils {:<5}",
        readiness.neutral_weavy_op_count,
        readiness.snark_intrinsic_count,
        readiness.needs_snark_stencils(),
        readiness.needs_lexer_stencils()
    );
    println!(
        "  copy-patch JIT available: {}",
        readiness.copy_patch_jit_available
    );
    println!(
        "  parser lowering: dialect {:>6}  lexer-graph {:>6}  sinks {:>6}  host barriers {:>6}",
        readiness.dialect_op_intrinsic_count,
        readiness.lexer_graph_intrinsic_count,
        readiness.sink_op_intrinsic_count,
        readiness.host_call_barrier_intrinsic_count
    );
    if !readiness.snark_stencil_summaries.is_empty() {
        println!("  snark stencil obligations:");
        for summary in &readiness.snark_stencil_summaries {
            println!(
                "    {:<36} {:<16?} {:<16?} x{}",
                format!(
                    "{}::{}",
                    summary.descriptor.dialect, summary.descriptor.name
                ),
                summary.domain,
                summary.lowering,
                summary.count
            );
        }
    }
    if !readiness.snark_stencil_family_summaries.is_empty() {
        println!("  snark stencil families:");
        for summary in &readiness.snark_stencil_family_summaries {
            println!(
                "    {:<18?} {:<16?} x{}  state={:?}  effect={:?} fail={} alloc={} user={} opaque={}",
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
    if !readiness.lexer_stencil_summaries.is_empty() {
        println!("  lexer graph stencils:");
        for summary in &readiness.lexer_stencil_summaries {
            println!(
                "    {:<18?} {:<16?} x{}  state={:?}",
                summary.kind, summary.execution, summary.count, summary.state
            );
        }
    }
    if !readiness.snark_stencil_execution_summaries.is_empty() {
        println!("  snark stencil execution lanes:");
        for summary in &readiness.snark_stencil_execution_summaries {
            println!(
                "    {:<16?} x{}  families={:?}  state={:?}  effect={:?} fail={} alloc={} user={} opaque={}",
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
    if !readiness.snark_stencil_state_summaries.is_empty() {
        println!("  snark stencil state surfaces:");
        for summary in &readiness.snark_stencil_state_summaries {
            println!("    {:<18?} x{}", summary.state, summary.count);
        }
    }
    let direct_no_trace_profile =
        readiness.snark_stencil_profile(SnarkStencilProfile::DirectNoTrace);
    if direct_no_trace_profile.state_summaries != readiness.snark_stencil_state_summaries {
        print_profile_stencil_readiness("direct no-trace", &direct_no_trace_profile);
    }
    let direct_tree_only_profile =
        readiness.snark_stencil_profile(SnarkStencilProfile::DirectTreeOnly);
    if direct_tree_only_profile.state_summaries != readiness.snark_stencil_state_summaries {
        print_profile_stencil_readiness("direct tree-only", &direct_tree_only_profile);
    }
    println!(
        "  lexer lowering: literal sets {:>4}/{:<4}  pattern sets {:>4}/{:<4}  rematch {:>4}  known {:>4}  regex-auto {:>4}  unsupported {:>4}",
        readiness.lexer.merged_literal_set_count,
        readiness.lexer.merged_literal_terminal_count,
        readiness.lexer.merged_pattern_set_count,
        readiness.lexer.merged_pattern_terminal_count,
        readiness.lexer.merged_pattern_leaf_rematch_terminal_count,
        readiness.lexer.known_pattern_count,
        readiness.lexer.regex_automata_count,
        readiness.lexer.unsupported_pattern_count
            + readiness.lexer.unsupported_terminal_count
            + readiness.lexer.unsupported_symbol_count
    );
    if !readiness.barrier_summaries.is_empty() {
        println!("  lowering barriers:");
        for summary in &readiness.barrier_summaries {
            println!("    {:?}  x{}", summary.barrier, summary.count);
        }
    }
    println!("  intrinsics:");
    for (intrinsic, count) in &analysis.intrinsic_counts {
        println!(
            "    {:<20} {:>6}",
            format!("{}::{}", intrinsic.dialect, intrinsic.name),
            count
        );
    }

    println!("\nper-parse (avg over {iters}):");
    if let Some(totals) = strict_tree_fresh_plan_total {
        print_bench_totals("weavy strict tree, fresh plan", &totals, iters);
    }
    if let Some(totals) = strict_tree_warm_plan_total {
        print_bench_totals("weavy strict tree, warm plan", &totals, iters);
    }
    if let Some(totals) = strict_resolved_tree_warm_plan_total {
        print_bench_totals("weavy strict resolved, warm", &totals, iters);
    }
    if let Some(totals) = strict_report_warm_plan_total {
        print_bench_totals("weavy strict report, warm", &totals, iters);
    }
    if let Some(totals) = recovering_warm_plan_total {
        print_bench_totals("weavy recovering, warm", &totals, iters);
    }
    #[cfg(all(
        feature = "jit",
        any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64")
        )
    ))]
    if let Some(totals) = hostcalls_report_warm_plan_total {
        print_bench_totals("weavy hostcalls report, warm", &totals, iters);
    }
}

fn print_profile_stencil_readiness(label: &str, profile: &WeavySnarkProfileStencilReadiness) {
    if profile.descriptor_summaries.is_empty() {
        return;
    }
    println!("  {label} stencil families:");
    for summary in &profile.family_summaries {
        println!(
            "    {:<18?} {:<16?} x{}  state={:?}  effect={:?} fail={} alloc={} user={} opaque={}",
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
    println!("  {label} stencil execution lanes:");
    for summary in &profile.execution_summaries {
        println!(
            "    {:<16?} x{}  families={:?}  state={:?}  effect={:?} fail={} alloc={} user={} opaque={}",
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
    println!("  {label} stencil state surfaces:");
    for summary in &profile.state_summaries {
        println!("    {:<18?} x{}", summary.state, summary.count);
    }
}
