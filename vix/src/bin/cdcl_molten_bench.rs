use std::hint::black_box;
use std::time::{Duration, Instant};

use vix::machine::{Machine, MachineArg, NamedArg};

const DEFAULT_RUNS: usize = 8;
const DEFAULT_STEPS: usize = 2048;

fn main() -> Result<(), String> {
    let runs = arg_usize("--runs", DEFAULT_RUNS)?;
    let steps = arg_usize("--steps", DEFAULT_STEPS)?;

    println!("cdcl_molten_bench steps={steps} runs={runs}");
    print_result(
        "machine-chain-reuse",
        &measure_machine(&machine_chain_source(steps), runs, false)?,
    );
    print_result(
        "machine-chain-copy",
        &measure_machine(&machine_chain_source(steps), runs, true)?,
    );
    print_result(
        "machine-rebind-reuse",
        &measure_machine(&machine_rebind_source(steps), runs, false)?,
    );
    print_result(
        "machine-rebind-copy",
        &measure_machine(&machine_rebind_source(steps), runs, true)?,
    );
    print_result("rust-vec", &measure(runs, || rust_vec(steps))?);

    Ok(())
}

fn arg_usize(flag: &str, default: usize) -> Result<usize, String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == flag {
            let value = args
                .next()
                .ok_or_else(|| format!("{flag} expects a value"))?;
            return value
                .parse()
                .map_err(|err| format!("invalid {flag} value `{value}`: {err}"));
        }
    }
    Ok(default)
}

fn measure<E>(runs: usize, mut run: impl FnMut() -> Result<(), E>) -> Result<Vec<Duration>, E> {
    let mut durations = Vec::with_capacity(runs);
    for _ in 0..runs {
        let start = Instant::now();
        run()?;
        durations.push(start.elapsed());
    }
    Ok(durations)
}

fn measure_machine(source: &str, runs: usize, force_copy: bool) -> Result<Vec<Duration>, String> {
    let mut machine = Machine::load(source).expect("generated CDCL bench source loads");
    machine.set_force_molten_copy(force_copy);
    let mut durations = Vec::with_capacity(runs);
    for seed in 0..runs {
        let start = Instant::now();
        let value = machine
            .call(
                "main",
                &[NamedArg {
                    name: "seed".to_string(),
                    value: MachineArg::Word(i64::try_from(seed).expect("seed fits i64")),
                }],
            )
            .expect("generated CDCL bench runs");
        black_box(value);
        durations.push(start.elapsed());
    }
    Ok(durations)
}

fn machine_chain_source(steps: usize) -> String {
    let mut expr = "[0]".to_string();
    for step in 1..=steps {
        expr = format!("({expr}).push({step})");
    }
    format!("pub fn main(seed: Int) -> [Int] {{\n    ({expr}).push(seed - seed)\n}}\n")
}

fn machine_rebind_source(steps: usize) -> String {
    let mut source = String::from("pub fn main(seed: Int) -> [Int] {\n    let trail = [0];\n");
    for step in 1..=steps {
        source.push_str(&format!("    let trail = trail.push({step});\n"));
    }
    source.push_str("    trail.push(seed - seed)\n}\n");
    source
}

fn rust_vec(steps: usize) -> Result<(), String> {
    let mut trail = Vec::with_capacity(steps + 2);
    trail.push(0_i64);
    for step in 1..=steps {
        trail.push(i64::try_from(step).expect("step fits i64"));
    }
    trail.push(0);
    black_box(trail);
    Ok(())
}

fn print_result(label: &str, durations: &[Duration]) {
    let mut sorted = durations.to_vec();
    sorted.sort_unstable();
    let median = sorted[sorted.len() / 2];
    let total: Duration = durations.iter().copied().sum();
    let mean = total / u32::try_from(durations.len()).expect("run count fits u32");
    println!("{label}: median={} mean={}", fmt(median), fmt(mean));
}

fn fmt(duration: Duration) -> String {
    let nanos = duration.as_nanos();
    if nanos >= 1_000_000 {
        format!("{:.3}ms", nanos as f64 / 1_000_000.0)
    } else if nanos >= 1_000 {
        format!("{:.3}us", nanos as f64 / 1_000.0)
    } else {
        format!("{nanos}ns")
    }
}
