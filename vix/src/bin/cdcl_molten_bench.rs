use std::collections::HashMap;
use std::hint::black_box;
use std::time::{Duration, Instant};

use vix::machine::Machine;
use vix::machine::driver::ValueStore;

const DEFAULT_RUNS: usize = 8;
const DEFAULT_STEPS: usize = 4096;

fn main() -> Result<(), String> {
    let runs = arg_usize("--runs", DEFAULT_RUNS)?;
    let steps = arg_usize("--steps", DEFAULT_STEPS)?;
    let mode = arg_string("--mode").unwrap_or_else(|| "all".to_string());

    println!("cdcl_molten_bench steps={steps} runs={runs} mode={mode}");
    match mode.as_str() {
        "all" => {
            print_result(
                "store-copy-hash",
                &measure(runs, || store_copy_hash(steps))?,
            );
            print_result(
                "store-hash-only",
                &measure(runs, || store_hash_only(steps))?,
            );
            print_result(
                "store-copy-only",
                &measure(runs, || store_copy_only(steps))?,
            );
            print_result("store-append", &measure(runs, || store_append(steps))?);
            print_result(
                "synthetic-blake3-append",
                &measure(runs, || synthetic_blake3_append(steps))?,
            );
            print_result("rust-vec", &measure(runs, || rust_vec(steps))?);
        }
        "store-copy-hash" => print_result(
            "store-copy-hash",
            &measure(runs, || store_copy_hash(steps))?,
        ),
        "store-hash-only" => print_result(
            "store-hash-only",
            &measure(runs, || store_hash_only(steps))?,
        ),
        "store-copy-only" => print_result(
            "store-copy-only",
            &measure(runs, || store_copy_only(steps))?,
        ),
        "store-append" => print_result("store-append", &measure(runs, || store_append(steps))?),
        "synthetic-blake3-append" => print_result(
            "synthetic-blake3-append",
            &measure(runs, || synthetic_blake3_append(steps))?,
        ),
        "rust-vec" => print_result("rust-vec", &measure(runs, || rust_vec(steps))?),
        "machine-rebind" => print_result(
            "machine-rebind",
            &measure_machine(&machine_rebind_source(steps), runs, false)?,
        ),
        "machine-chain-reuse" => print_result(
            "machine-chain-reuse",
            &measure_machine(&machine_chain_source(steps), runs, false)?,
        ),
        "machine-chain-copy" => print_result(
            "machine-chain-copy",
            &measure_machine(&machine_chain_source(steps), runs, true)?,
        ),
        other => {
            return Err(format!(
                "unknown --mode `{other}`; expected all, store-copy-hash, store-hash-only, store-copy-only, store-append, synthetic-blake3-append, rust-vec, machine-rebind, machine-chain-reuse, or machine-chain-copy"
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

fn measure_machine(source: &str, runs: usize, force_copy: bool) -> Result<Vec<Duration>, String> {
    let mut machine = Machine::load(source).expect("generated CDCL bench source loads");
    machine.set_force_molten_copy(force_copy);
    let mut durations = Vec::with_capacity(runs);
    for seed in 0..runs {
        let start = Instant::now();
        let value = machine
            .demand_i64("main", vec![i64::try_from(seed).expect("seed fits i64")])
            .expect("generated CDCL bench runs");
        black_box(value);
        durations.push(start.elapsed());
    }
    Ok(durations)
}

fn store_copy_hash(steps: usize) -> Result<(), String> {
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

fn store_hash_only(steps: usize) -> Result<(), String> {
    let store = ValueStore::default();
    let mut trail = Vec::with_capacity(steps);
    for step in 0..steps {
        trail.push(i64::try_from(step).expect("step fits i64"));
        black_box(store.hash_array_words_for_bench("Int", &trail));
    }
    Ok(())
}

fn store_copy_only(steps: usize) -> Result<(), String> {
    let mut trail = Vec::with_capacity(steps);
    for step in 0..steps {
        trail.push(i64::try_from(step).expect("step fits i64"));
        black_box(trail.clone());
    }
    Ok(())
}

fn store_append(steps: usize) -> Result<(), String> {
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

fn synthetic_blake3_append(steps: usize) -> Result<(), String> {
    let mut by_content = HashMap::new();
    let mut prefix = blake3_array_empty_hash("Int");
    by_content.insert(prefix, 0usize);
    for step in 0..steps {
        let word = i64::try_from(step).expect("step fits i64");
        let child = blake3_scalar_hash("Int", word);
        prefix = blake3_array_push_hash("Int", prefix, step + 1, child);
        by_content.insert(prefix, step + 1);
    }
    black_box((prefix, by_content));
    Ok(())
}

fn blake3_scalar_hash(schema: &str, word: i64) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vix-scalar-word");
    hasher.update(schema.as_bytes());
    hasher.update(&word.to_le_bytes());
    *hasher.finalize().as_bytes()
}

fn blake3_array_empty_hash(elem_schema: &str) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vix-array-words-empty-v1");
    hasher.update(elem_schema.as_bytes());
    *hasher.finalize().as_bytes()
}

fn blake3_array_push_hash(
    elem_schema: &str,
    prefix_hash: [u8; 32],
    next_len: usize,
    child_hash: [u8; 32],
) -> [u8; 32] {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vix-array-words-push-v1");
    hasher.update(elem_schema.as_bytes());
    hasher.update(
        &i64::try_from(next_len)
            .expect("array length fits i64")
            .to_le_bytes(),
    );
    hasher.update(&prefix_hash);
    hasher.update(&child_hash);
    *hasher.finalize().as_bytes()
}

fn rust_vec(steps: usize) -> Result<(), String> {
    let mut trail = Vec::with_capacity(steps);
    for step in 0..steps {
        trail.push(i64::try_from(step).expect("step fits i64"));
    }
    black_box(trail);
    Ok(())
}

fn machine_rebind_source(steps: usize) -> String {
    let mut source = String::from("pub fn main(seed: Int) -> Int {\n    let trail = [0];\n");
    for step in 1..=steps {
        source.push_str(&format!("    let trail = trail.push({step});\n"));
    }
    source.push_str("    trail.len() + seed - seed\n}\n");
    source
}

fn machine_chain_source(steps: usize) -> String {
    let mut expr = "[0]".to_string();
    for step in 1..=steps {
        expr = format!("({expr}).push({step})");
    }
    format!("pub fn main(seed: Int) -> Int {{\n    ({expr}).len() + seed - seed\n}}\n")
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
