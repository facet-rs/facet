use std::hint::black_box;
use std::time::{Duration, Instant};

use vix::machine::driver::ValueStore;

const DEFAULT_RUNS: usize = 8;
const DEFAULT_STEPS: usize = 4096;

fn main() -> Result<(), String> {
    let runs = arg_usize("--runs", DEFAULT_RUNS)?;
    let steps = arg_usize("--steps", DEFAULT_STEPS)?;
    let mode = arg_string("--mode").unwrap_or_else(|| "all".to_string());

    println!("cdcl_flesh_bench steps={steps} runs={runs} mode={mode}");
    match mode.as_str() {
        "all" => {
            let naive = measure(runs, || naive_copy(steps))?;
            let reuse = measure(runs, || reuse_incremental(steps))?;
            let rust_vec = measure(runs, || rust_vec_ceiling(steps))?;
            print_result("naive-copy", &naive);
            print_result("reuse-incremental", &reuse);
            print_result("rust-vec", &rust_vec);
        }
        "naive" => print_result("naive-copy", &measure(runs, || naive_copy(steps))?),
        "reuse" => print_result(
            "reuse-incremental",
            &measure(runs, || reuse_incremental(steps))?,
        ),
        "vec" => print_result("rust-vec", &measure(runs, || rust_vec_ceiling(steps))?),
        other => {
            return Err(format!(
                "unknown --mode `{other}`; expected all, naive, reuse, or vec"
            ));
        }
    }

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

fn arg_string(flag: &str) -> Option<String> {
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        if arg == flag {
            return args.next();
        }
    }
    None
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

fn naive_copy(steps: usize) -> Result<(), String> {
    let schema_refs = ["Int".to_string()];
    let mut store = ValueStore::default();
    let mut trail = Vec::with_capacity(steps);
    let mut last = 0;
    for step in 0..steps {
        trail.push(i64::try_from(step).expect("step fits i64"));
        last = store
            .alloc_array_words_for_bench("Int", trail.clone(), &schema_refs)?
            .0;
    }
    black_box(last);
    Ok(())
}

fn reuse_incremental(steps: usize) -> Result<(), String> {
    let schema_refs = ["Int".to_string()];
    let mut store = ValueStore::default();
    let mut handle = store
        .alloc_array_words_for_bench("Int", Vec::new(), &schema_refs)?
        .0;
    for step in 0..steps {
        handle = store
            .alloc_array_words_append_for_bench(
                handle,
                "Int",
                i64::try_from(step).expect("step fits i64"),
                &schema_refs,
            )?
            .0;
    }
    black_box(handle);
    Ok(())
}

fn rust_vec_ceiling(steps: usize) -> Result<(), String> {
    let mut trail = Vec::with_capacity(steps);
    for step in 0..steps {
        trail.push(i64::try_from(step).expect("step fits i64"));
    }
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
