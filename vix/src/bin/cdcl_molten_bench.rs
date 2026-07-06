use std::hint::black_box;
use std::time::{Duration, Instant};

use vix::machine::Machine;

const PUSHES: usize = 128;
const BURST: usize = 32;
const POPS: usize = 16;
const RUNS: i64 = 24;

fn main() {
    let source = cdcl_source();
    let naive = run_vix(&source, true);
    let reuse = run_vix(&source, false);
    let rust_vec = run_rust_vec();

    println!("naive_copy_ns={}", naive.as_nanos());
    println!("reuse_ns={}", reuse.as_nanos());
    println!("rust_vec_ns={}", rust_vec.as_nanos());
}

fn run_vix(source: &str, force_copy: bool) -> Duration {
    let mut machine = Machine::load(source).expect("generated CDCL bench source loads");
    machine.set_force_molten_copy(force_copy);
    let start = Instant::now();
    for seed in 0..RUNS {
        let value = machine
            .demand_i64("main", vec![seed])
            .expect("generated CDCL bench runs");
        black_box(value);
    }
    start.elapsed()
}

fn run_rust_vec() -> Duration {
    let start = Instant::now();
    for seed in 0..RUNS {
        let mut trail = Vec::new();
        trail.push(0_i64);
        for i in 1..=PUSHES {
            trail.push(i as i64);
            if i % BURST == 0 {
                for _ in 0..POPS {
                    black_box(trail.pop().expect("checkpoint pop has an item"));
                }
            }
        }
        let value = trail.len() as i64 + seed - seed;
        black_box(value);
    }
    start.elapsed()
}

fn cdcl_source() -> String {
    let mut expr = "[0]".to_string();
    for i in 1..=PUSHES {
        expr = format!("({expr}).push({i})");
        if i % BURST == 0 {
            for _ in 0..POPS {
                expr = format!("(({expr}).pop()).1");
            }
        }
    }
    format!("pub fn main(seed: Int) -> Int {{\n    ({expr}).len() + seed - seed\n}}\n")
}
