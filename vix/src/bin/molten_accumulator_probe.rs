use std::env;

use vix::machine::{Machine, RenderedValue};

const SOURCE: &str = r#"
pub fn seed() -> [Int] {
    [0]
}

pub fn grow(n: Int, acc: [Int]) -> [Int] {
    match n {
        0 => acc,
        _ => grow(n - 1, acc.push(n)),
    }
}
"#;

fn main() {
    let n = env::args()
        .nth(1)
        .expect("usage: molten_accumulator_probe <n>")
        .parse::<i64>()
        .expect("n must fit i64");
    let render = !env::args().any(|arg| arg == "--no-render");
    let mut machine = Machine::load(SOURCE).expect("probe source loads");
    let seed = machine.demand_i64("seed", vec![]).expect("seed runs");
    machine.clear_trace();
    let result = machine.demand_i64("grow", vec![n, seed]).expect("grow runs");
    println!("n={n}");
    println!("store_len={}", machine.store_len());
    println!("trace_len={}", machine.trace().len());
    let (
        molten_entries,
        molten_array_words,
        molten_carried_hashes,
        molten_array_entries,
        molten_refs_gt_one,
        molten_max_refs,
    ) = machine.molten_debug_counts();
    println!("molten_entries={molten_entries}");
    println!("molten_array_entries={molten_array_entries}");
    println!("molten_array_words={molten_array_words}");
    println!("molten_carried_hashes={molten_carried_hashes}");
    println!("molten_refs_gt_one={molten_refs_gt_one}");
    println!("molten_max_refs={molten_max_refs}");
    if render {
        let RenderedValue::Array { items, .. } = machine
            .render_result("grow", result)
            .expect("grow result renders")
        else {
            panic!("grow did not render as an Array");
        };
        println!("len={}", items.len());
    } else {
        println!("result_word={result}");
    }
}
