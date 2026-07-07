use std::env;
use std::hint::black_box;
use std::time::{Duration, Instant};

use vix::machine::driver::Lane;
use vix::machine::Machine;

const ID: i64 = 0;
const PLUS: i64 = 1;
const EOF: i64 = 2;

const ACCEPT: i64 = 0;
const REDUCE_ID: i64 = 101;
const REDUCE_PLUS_ID: i64 = 102;
const ERROR: i64 = 999;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    All,
    Rust,
    VixInterp,
    VixJit,
    VixUnrolledInterp,
    VixUnrolledJit,
    ArrayControl,
}

#[derive(Clone, Debug)]
struct Args {
    terms: usize,
    runs: i64,
    mode: Mode,
    force_molten_copy: Option<bool>,
    array_pushes: usize,
    array_burst: usize,
    array_pops: usize,
    array_runs: i64,
}

#[derive(Clone, Debug)]
struct BenchResult {
    elapsed: Duration,
    checksum_sum: i64,
}

fn main() -> Result<(), String> {
    let args = parse_args(env::args().skip(1))?;
    if args.mode == Mode::ArrayControl {
        return run_array_control(&args);
    }

    let expected = i64::try_from(args.terms).map_err(|_| "terms do not fit in Int")?;
    let actions = lr_action_count(args.terms);
    let tokens = token_count(args.terms);

    println!("terms={}", args.terms);
    println!("tokens={tokens}");
    println!("lr_actions={actions}");
    println!("runs={}", args.runs);
    println!("expected_checksum={expected}");
    println!(
        "force_molten_copy={}",
        args.force_molten_copy
            .map(|value| value.to_string())
            .unwrap_or_else(|| "runtime-default".to_string())
    );

    let rust = if matches!(
        args.mode,
        Mode::All | Mode::Rust | Mode::VixUnrolledInterp | Mode::VixUnrolledJit
    ) {
        let result = bench_rust(args.terms, args.runs, expected);
        print_result("rust", &result, args.runs, actions);
        Some(result)
    } else {
        None
    };

    if matches!(args.mode, Mode::All | Mode::VixInterp) {
        let result = bench_vix(
            args.terms,
            args.runs,
            expected,
            Lane::Interp,
            args.force_molten_copy,
        )?;
        print_result("vix_interp", &result, args.runs, actions);
        if let Some(rust) = &rust {
            println!(
                "factor_vix_interp_vs_rust={:.3}",
                duration_ratio(result.elapsed, rust.elapsed)
            );
        }
    }

    if matches!(args.mode, Mode::All | Mode::VixJit) {
        #[cfg(feature = "jit")]
        {
            let result = bench_vix(
                args.terms,
                args.runs,
                expected,
                Lane::Jit,
                args.force_molten_copy,
            )?;
            print_result("vix_jit", &result, args.runs, actions);
            if let Some(rust) = &rust {
                println!(
                    "factor_vix_jit_vs_rust={:.3}",
                    duration_ratio(result.elapsed, rust.elapsed)
                );
            }
        }
        #[cfg(not(feature = "jit"))]
        return Err("vix was built without the jit feature".to_string());
    }

    if matches!(args.mode, Mode::VixUnrolledInterp) {
        let result = bench_vix_unrolled(
            args.terms,
            args.runs,
            expected,
            Lane::Interp,
            args.force_molten_copy,
        )?;
        print_result("vix_unrolled_interp", &result, args.runs, actions);
        if let Some(rust) = &rust {
            println!(
                "factor_vix_unrolled_interp_vs_rust={:.3}",
                duration_ratio(result.elapsed, rust.elapsed)
            );
        }
    }

    if matches!(args.mode, Mode::VixUnrolledJit) {
        #[cfg(feature = "jit")]
        {
            let result = bench_vix_unrolled(
                args.terms,
                args.runs,
                expected,
                Lane::Jit,
                args.force_molten_copy,
            )?;
            print_result("vix_unrolled_jit", &result, args.runs, actions);
            if let Some(rust) = &rust {
                println!(
                    "factor_vix_unrolled_jit_vs_rust={:.3}",
                    duration_ratio(result.elapsed, rust.elapsed)
                );
            }
        }
        #[cfg(not(feature = "jit"))]
        return Err("vix was built without the jit feature".to_string());
    }

    Ok(())
}

fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Args, String> {
    let mut out = Args {
        terms: 50_000,
        runs: 1,
        mode: Mode::All,
        force_molten_copy: None,
        array_pushes: 1024,
        array_burst: 32,
        array_pops: 16,
        array_runs: 10,
    };

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--terms" => {
                out.terms = parse_next(&mut args, "--terms")?;
                if out.terms == 0 {
                    return Err("--terms must be positive".to_string());
                }
            }
            "--tokens" => {
                let requested: usize = parse_next(&mut args, "--tokens")?;
                if requested < 2 {
                    return Err("--tokens must be at least 2".to_string());
                }
                out.terms = requested / 2;
                if token_count(out.terms) != requested {
                    return Err(
                        "--tokens must be even: this grammar uses ID (+ ID)* EOF".to_string()
                    );
                }
            }
            "--runs" => {
                out.runs = parse_next(&mut args, "--runs")?;
                if out.runs <= 0 {
                    return Err("--runs must be positive".to_string());
                }
            }
            "--array-pushes" => {
                out.array_pushes = parse_next(&mut args, "--array-pushes")?;
                if out.array_pushes == 0 {
                    return Err("--array-pushes must be positive".to_string());
                }
            }
            "--array-burst" => {
                out.array_burst = parse_next(&mut args, "--array-burst")?;
                if out.array_burst == 0 {
                    return Err("--array-burst must be positive".to_string());
                }
            }
            "--array-pops" => {
                out.array_pops = parse_next(&mut args, "--array-pops")?;
            }
            "--array-runs" => {
                out.array_runs = parse_next(&mut args, "--array-runs")?;
                if out.array_runs <= 0 {
                    return Err("--array-runs must be positive".to_string());
                }
            }
            "--mode" => {
                let value: String = parse_next(&mut args, "--mode")?;
                out.mode = match value.as_str() {
                    "all" => Mode::All,
                    "rust" => Mode::Rust,
                    "vix-interp" => Mode::VixInterp,
                    "vix-jit" => Mode::VixJit,
                    "vix-unrolled-interp" => Mode::VixUnrolledInterp,
                    "vix-unrolled-jit" => Mode::VixUnrolledJit,
                    "array-control" => Mode::ArrayControl,
                    other => {
                        return Err(format!(
                            "unknown --mode `{other}` (expected all, rust, vix-interp, vix-jit, vix-unrolled-interp, vix-unrolled-jit, array-control)"
                        ));
                    }
                };
            }
            "--force-molten-copy" => out.force_molten_copy = Some(true),
            "--molten-reuse" => out.force_molten_copy = Some(false),
            "--help" | "-h" => return Err(help_text()),
            other => return Err(format!("unknown argument `{other}`\n{}", help_text())),
        }
    }

    Ok(out)
}

fn parse_next<T: std::str::FromStr>(
    args: &mut impl Iterator<Item = String>,
    name: &str,
) -> Result<T, String> {
    let value = args.next().ok_or_else(|| format!("{name} needs a value"))?;
    value
        .parse()
        .map_err(|_| format!("could not parse `{value}` for {name}"))
}

fn help_text() -> String {
    "usage: lr_loop_bench [--tokens N|--terms N] [--runs N] [--mode all|rust|vix-interp|vix-jit|vix-unrolled-interp|vix-unrolled-jit|array-control] [--force-molten-copy|--molten-reuse] [--array-pushes N] [--array-burst N] [--array-pops N] [--array-runs N]".to_string()
}

fn print_result(name: &str, result: &BenchResult, runs: i64, actions: usize) {
    let total_actions = actions as f64 * runs as f64;
    let seconds = result.elapsed.as_secs_f64();
    println!("{name}_ns={}", result.elapsed.as_nanos());
    println!("{name}_checksum_sum={}", result.checksum_sum);
    println!("{name}_actions_per_s={:.3}", total_actions / seconds);
    println!(
        "{name}_ns_per_action={:.3}",
        result.elapsed.as_nanos() as f64 / total_actions
    );
}

fn duration_ratio(candidate: Duration, baseline: Duration) -> f64 {
    candidate.as_secs_f64() / baseline.as_secs_f64()
}

fn token_count(terms: usize) -> usize {
    terms * 2
}

fn lr_action_count(terms: usize) -> usize {
    terms * 3
}

fn bench_rust(terms: usize, runs: i64, expected: i64) -> BenchResult {
    let tokens = reversed_tokens(terms);
    let start = Instant::now();
    let mut checksum_sum = 0;
    for seed in 0..runs {
        let value = rust_parse(&tokens, seed);
        assert_eq!(value, expected);
        checksum_sum += black_box(value);
    }
    BenchResult {
        elapsed: start.elapsed(),
        checksum_sum,
    }
}

fn bench_vix(
    terms: usize,
    runs: i64,
    expected: i64,
    lane: Lane,
    force_molten_copy: Option<bool>,
) -> Result<BenchResult, String> {
    let source = vix_source(terms);
    let mut machine = Machine::load_with_lane(&source, lane)?;
    if let Some(force) = force_molten_copy {
        machine.set_force_molten_copy(force);
    }
    let tokens = machine.demand_i64("tokens", vec![])?;
    let start = Instant::now();
    let mut checksum_sum = 0;
    for seed in 0..runs {
        let value = machine.demand_i64("parse_entry", vec![tokens, seed])?;
        if value != expected {
            return Err(format!(
                "{lane:?} checksum mismatch: got {value}, expected {expected}"
            ));
        }
        checksum_sum += black_box(value);
    }
    Ok(BenchResult {
        elapsed: start.elapsed(),
        checksum_sum,
    })
}

fn bench_vix_unrolled(
    terms: usize,
    runs: i64,
    expected: i64,
    lane: Lane,
    force_molten_copy: Option<bool>,
) -> Result<BenchResult, String> {
    let source = vix_unrolled_source(terms);
    let mut machine = Machine::load_with_lane(&source, lane)?;
    if let Some(force) = force_molten_copy {
        machine.set_force_molten_copy(force);
    }
    let tokens = machine.demand_i64("tokens", vec![])?;
    let start = Instant::now();
    let mut checksum_sum = 0;
    for seed in 0..runs {
        let value = machine.demand_i64("parse_entry", vec![tokens, seed])?;
        if value != expected {
            return Err(format!(
                "{lane:?} unrolled checksum mismatch: got {value}, expected {expected}"
            ));
        }
        checksum_sum += black_box(value);
    }
    Ok(BenchResult {
        elapsed: start.elapsed(),
        checksum_sum,
    })
}

fn run_array_control(args: &Args) -> Result<(), String> {
    let ops = array_control_ops(args.array_pushes, args.array_burst, args.array_pops);
    let expected = array_control_expected(args.array_pushes, args.array_burst, args.array_pops)?;
    println!("array_control_pushes={}", args.array_pushes);
    println!("array_control_burst={}", args.array_burst);
    println!("array_control_pops={}", args.array_pops);
    println!("array_control_ops={ops}");
    println!("array_control_runs={}", args.array_runs);
    println!("array_control_expected={expected}");

    let rust = bench_array_control_rust(
        args.array_pushes,
        args.array_burst,
        args.array_pops,
        args.array_runs,
        expected,
    );
    print_result("array_control_rust", &rust, args.array_runs, ops);

    let source = array_control_source(args.array_pushes, args.array_burst, args.array_pops);
    match args.force_molten_copy {
        Some(true) => {
            let interp =
                bench_array_control_vix(&source, args.array_runs, expected, Lane::Interp, true)?;
            print_result(
                "array_control_vix_interp_copy",
                &interp,
                args.array_runs,
                ops,
            );
            #[cfg(feature = "jit")]
            {
                let jit =
                    bench_array_control_vix(&source, args.array_runs, expected, Lane::Jit, true)?;
                print_result("array_control_vix_jit_copy", &jit, args.array_runs, ops);
            }
        }
        Some(false) => {
            let interp =
                bench_array_control_vix(&source, args.array_runs, expected, Lane::Interp, false)?;
            print_result(
                "array_control_vix_interp_reuse",
                &interp,
                args.array_runs,
                ops,
            );
            #[cfg(feature = "jit")]
            {
                let jit =
                    bench_array_control_vix(&source, args.array_runs, expected, Lane::Jit, false)?;
                print_result("array_control_vix_jit_reuse", &jit, args.array_runs, ops);
            }
        }
        None => {
            let interp_copy =
                bench_array_control_vix(&source, args.array_runs, expected, Lane::Interp, true)?;
            print_result(
                "array_control_vix_interp_copy",
                &interp_copy,
                args.array_runs,
                ops,
            );
            let interp_reuse =
                bench_array_control_vix(&source, args.array_runs, expected, Lane::Interp, false)?;
            print_result(
                "array_control_vix_interp_reuse",
                &interp_reuse,
                args.array_runs,
                ops,
            );
            println!(
                "array_control_interp_copy_vs_reuse={:.3}",
                duration_ratio(interp_copy.elapsed, interp_reuse.elapsed)
            );
            #[cfg(feature = "jit")]
            {
                let jit_copy =
                    bench_array_control_vix(&source, args.array_runs, expected, Lane::Jit, true)?;
                print_result(
                    "array_control_vix_jit_copy",
                    &jit_copy,
                    args.array_runs,
                    ops,
                );
                let jit_reuse =
                    bench_array_control_vix(&source, args.array_runs, expected, Lane::Jit, false)?;
                print_result(
                    "array_control_vix_jit_reuse",
                    &jit_reuse,
                    args.array_runs,
                    ops,
                );
                println!(
                    "array_control_jit_copy_vs_reuse={:.3}",
                    duration_ratio(jit_copy.elapsed, jit_reuse.elapsed)
                );
            }
        }
    }
    Ok(())
}

fn bench_array_control_rust(
    pushes: usize,
    burst: usize,
    pops: usize,
    runs: i64,
    expected: i64,
) -> BenchResult {
    let start = Instant::now();
    let mut checksum_sum = 0;
    for seed in 0..runs {
        let value = rust_array_control(pushes, burst, pops, seed);
        assert_eq!(value, expected);
        checksum_sum += black_box(value);
    }
    BenchResult {
        elapsed: start.elapsed(),
        checksum_sum,
    }
}

fn bench_array_control_vix(
    source: &str,
    runs: i64,
    expected: i64,
    lane: Lane,
    force_copy: bool,
) -> Result<BenchResult, String> {
    let mut machine = Machine::load_with_lane(source, lane)?;
    machine.set_force_molten_copy(force_copy);
    let start = Instant::now();
    let mut checksum_sum = 0;
    for seed in 0..runs {
        let value = machine.demand_i64("main", vec![seed])?;
        if value != expected {
            return Err(format!(
                "{lane:?} array control mismatch: got {value}, expected {expected}"
            ));
        }
        checksum_sum += black_box(value);
    }
    Ok(BenchResult {
        elapsed: start.elapsed(),
        checksum_sum,
    })
}

fn rust_array_control(pushes: usize, burst: usize, pops: usize, seed: i64) -> i64 {
    let mut stack = Vec::new();
    stack.push(0_i64);
    for i in 1..=pushes {
        stack.push(i as i64);
        if i % burst == 0 {
            for _ in 0..pops {
                black_box(stack.pop().expect("control pop has an item"));
            }
        }
    }
    stack.len() as i64 + seed - seed
}

fn array_control_ops(pushes: usize, burst: usize, pops: usize) -> usize {
    pushes + (pushes / burst) * pops
}

fn array_control_expected(pushes: usize, burst: usize, pops: usize) -> Result<i64, String> {
    let checkpoints = pushes / burst;
    let removed = checkpoints
        .checked_mul(pops)
        .ok_or_else(|| "array control pop count overflow".to_string())?;
    let len = 1_usize
        .checked_add(pushes)
        .and_then(|len| len.checked_sub(removed))
        .ok_or_else(|| "array control pops more values than it pushes".to_string())?;
    i64::try_from(len).map_err(|_| "array control final length does not fit Int".to_string())
}

fn array_control_source(pushes: usize, burst: usize, pops: usize) -> String {
    let mut expr = "[0]".to_string();
    for i in 1..=pushes {
        expr = format!("({expr}).push({i})");
        if i % burst == 0 {
            for _ in 0..pops {
                expr = format!("(({expr}).pop()).1");
            }
        }
    }
    format!("pub fn main(seed: Int) -> Int {{\n    ({expr}).len() + seed - seed\n}}\n")
}

fn rust_parse(tokens: &[i64], seed: i64) -> i64 {
    let mut tokens = tokens.to_vec();
    let mut stack = vec![0_i64];
    let mut lookahead = tokens.pop().expect("first token");
    let mut reduces = seed;

    loop {
        let state = *stack.last().expect("state stack is never empty");
        match rust_action(state, lookahead) {
            ACCEPT => return reduces - seed,
            REDUCE_ID => {
                stack.pop().expect("reduce id pops state");
                let prev = *stack.last().expect("goto base for id");
                stack.push(rust_goto(prev));
                reduces += 1;
            }
            REDUCE_PLUS_ID => {
                stack.pop().expect("reduce plus/id pops id state");
                stack.pop().expect("reduce plus/id pops plus state");
                stack.pop().expect("reduce plus/id pops lhs state");
                let prev = *stack.last().expect("goto base for plus/id");
                stack.push(rust_goto(prev));
                reduces += 1;
            }
            ERROR => panic!("parse error at state {state}, token {lookahead}"),
            shift => {
                stack.push(shift);
                lookahead = tokens.pop().expect("shift has next token");
            }
        }
    }
}

fn rust_action(state: i64, token: i64) -> i64 {
    match state {
        0 => match token {
            ID => 2,
            _ => ERROR,
        },
        1 => match token {
            PLUS => 3,
            EOF => ACCEPT,
            _ => ERROR,
        },
        2 => match token {
            PLUS | EOF => REDUCE_ID,
            _ => ERROR,
        },
        3 => match token {
            ID => 4,
            _ => ERROR,
        },
        4 => match token {
            PLUS | EOF => REDUCE_PLUS_ID,
            _ => ERROR,
        },
        _ => ERROR,
    }
}

fn rust_goto(state: i64) -> i64 {
    match state {
        0 => 1,
        _ => ERROR,
    }
}

fn reversed_tokens(terms: usize) -> Vec<i64> {
    let mut tokens = Vec::with_capacity(token_count(terms));
    tokens.push(EOF);
    for term in (0..terms).rev() {
        tokens.push(ID);
        if term > 0 {
            tokens.push(PLUS);
        }
    }
    tokens
}

fn vix_source(terms: usize) -> String {
    let mut source = String::new();
    push_tokens_fn(&mut source, terms);
    source.push_str(
        r#"
fn parse(tokens: [Int], stack: [Int], lookahead: Int, reduces: Int) -> Int {
    let top = stack.pop();
    let state = top.0;
    let action = match state {
        0 => match lookahead {
            0 => 2,
            _ => 999,
        },
        1 => match lookahead {
            1 => 3,
            2 => 0,
            _ => 999,
        },
        2 => match lookahead {
            1 => 101,
            2 => 101,
            _ => 999,
        },
        3 => match lookahead {
            0 => 4,
            _ => 999,
        },
        4 => match lookahead {
            1 => 102,
            2 => 102,
            _ => 999,
        },
        _ => 999,
    };
    match action {
        0 => reduces,
        101 => reduce_id(tokens, top.1, lookahead, reduces),
        102 => reduce_plus_id(tokens, top.1, lookahead, reduces),
        999 => 0 - state - lookahead - reduces,
        _ => shift(tokens, stack, action, reduces),
    }
}

fn shift(tokens: [Int], stack: [Int], action: Int, reduces: Int) -> Int {
    let next = tokens.pop();
    parse(next.1, stack.push(action), next.0, reduces)
}

fn reduce_id(tokens: [Int], base: [Int], lookahead: Int, reduces: Int) -> Int {
    let prev = base.pop();
    let next_state = match prev.0 {
        0 => 1,
        _ => 999,
    };
    parse(tokens, base.push(next_state), lookahead, reduces + 1)
}

fn reduce_plus_id(tokens: [Int], after_top: [Int], lookahead: Int, reduces: Int) -> Int {
    let pop_plus = after_top.pop();
    let pop_lhs = pop_plus.1.pop();
    let base = pop_lhs.1;
    let prev = base.pop();
    let next_state = match prev.0 {
        0 => 1,
        _ => 999,
    };
    parse(tokens, base.push(next_state), lookahead, reduces + 1)
}

pub fn parse_entry(tokens: [Int], seed: Int) -> Int {
    let first = tokens.pop();
    parse(first.1, [0], first.0, seed) - seed
}
"#,
    );
    source
}

fn push_tokens_fn(source: &mut String, terms: usize) {
    source.push_str("pub fn tokens() -> [Int] {\n    [");
    for (index, token) in reversed_tokens(terms).into_iter().enumerate() {
        if index > 0 {
            source.push_str(", ");
        }
        if index > 0 && index % 32 == 0 {
            source.push_str("\n     ");
        }
        source.push_str(&token.to_string());
    }
    source.push_str(
        r#"]
}
"#,
    );
}

fn vix_unrolled_source(terms: usize) -> String {
    let mut source = String::new();
    push_tokens_fn(&mut source, terms);
    source.push_str(
        r#"
pub fn parse_entry(tokens: [Int], seed: Int) -> Int {
    let tokens = tokens.pop().1;
    let stack = [0];
    let reduces = seed;
    let stack = stack.push(2);
    let tokens = tokens.pop().1;
    let stack = stack.pop().1;
    let stack = stack.push(1);
    let reduces = reduces + 1;
"#,
    );
    for _ in 1..terms {
        source.push_str(
            r#"    let stack = stack.push(3);
    let tokens = tokens.pop().1;
    let stack = stack.push(4);
    let tokens = tokens.pop().1;
    let stack = stack.pop().1;
    let stack = stack.pop().1;
    let stack = stack.pop().1;
    let stack = stack.push(1);
    let reduces = reduces + 1;
"#,
        );
    }
    source.push_str(
        r#"    let stack_len = stack.len();
    let token_len = tokens.len();
    reduces - seed + stack_len - 2 + token_len
}
"#,
    );
    source
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_and_vix_match_small_lr_stream() {
        let terms = 16;
        let expected = i64::try_from(terms).unwrap();
        assert_eq!(rust_parse(&reversed_tokens(terms), 7), expected);

        let source = vix_source(terms);
        for lane in test_lanes() {
            let mut machine = Machine::load_with_lane(&source, lane).unwrap();
            let tokens = machine.demand_i64("tokens", vec![]).unwrap();
            assert_eq!(
                machine.demand_i64("parse_entry", vec![tokens, 7]).unwrap(),
                expected,
                "{lane:?}"
            );
        }
    }

    #[test]
    fn fresh_temporary_array_control_matches_rust() {
        let pushes = 16;
        let burst = 8;
        let pops = 4;
        let expected = array_control_expected(pushes, burst, pops).unwrap();
        assert_eq!(rust_array_control(pushes, burst, pops, 3), expected);

        let source = array_control_source(pushes, burst, pops);
        for lane in test_lanes() {
            for force_copy in [true, false] {
                let result =
                    bench_array_control_vix(&source, 2, expected, lane, force_copy).unwrap();
                assert_eq!(result.checksum_sum, expected * 2, "{lane:?}/{force_copy}");
            }
        }
    }

    #[test]
    fn unrolled_named_rebind_lr_stream_matches_rust() {
        let terms = 8;
        let expected = i64::try_from(terms).unwrap();
        let source = vix_unrolled_source(terms);
        for lane in test_lanes() {
            let mut machine = Machine::load_with_lane(&source, lane).unwrap();
            machine.set_force_molten_copy(false);
            let tokens = machine.demand_i64("tokens", vec![]).unwrap();
            assert_eq!(
                machine.demand_i64("parse_entry", vec![tokens, 7]).unwrap(),
                expected,
                "{lane:?}"
            );
        }
    }

    fn test_lanes() -> Vec<Lane> {
        let lanes = vec![Lane::Interp];
        #[cfg(feature = "jit")]
        let lanes = {
            let mut lanes = lanes;
            lanes.push(Lane::Jit);
            lanes
        };
        lanes
    }
}
