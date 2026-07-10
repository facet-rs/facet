use std::collections::{BTreeMap, BTreeSet, HashMap};

use vix::machine::driver::Lane;
use vix::machine::{DriveEvent, Machine};

// r[verify machine.execution.jit-single-feature]
//
// Vix carries no jit feature of its own: whether `Lane::Jit` is exercised
// follows Weavy's single build-time decision, read back at runtime.
pub fn lanes() -> Vec<Lane> {
    let mut lanes = vec![Lane::Interp];
    if weavy::jit::task_lane::available() {
        lanes.push(Lane::Jit);
    }
    lanes
}

pub fn load_with_lane(source: &str, lane: Lane) -> Machine {
    Machine::load_with_lane(source, lane).unwrap_or_else(|err| {
        panic!("loads on {lane:?}: {err}");
    })
}

pub fn load_modules_with_lane(
    root: &str,
    modules: BTreeMap<String, String>,
    lane: Lane,
) -> Machine {
    Machine::load_modules_with_lane(root, modules, lane).unwrap_or_else(|err| {
        panic!("module set loads on {lane:?}: {err}");
    })
}

pub fn modules(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
    entries
        .iter()
        .map(|(path, source)| ((*path).to_string(), (*source).to_string()))
        .collect()
}

pub fn spawned_functions(machine: &Machine) -> BTreeSet<String> {
    event_functions(machine, |event| match event {
        DriveEvent::Spawned { fn_hash } => Some(*fn_hash),
        _ => None,
    })
}

pub fn memo_hit_functions(machine: &Machine) -> BTreeSet<String> {
    event_functions(machine, |event| match event {
        DriveEvent::MemoHit { fn_hash } => Some(*fn_hash),
        _ => None,
    })
}

fn event_functions(
    machine: &Machine,
    pick: impl Fn(&DriveEvent) -> Option<u64>,
) -> BTreeSet<String> {
    let by_hash: HashMap<u64, String> = machine
        .fn_hashes()
        .into_iter()
        .map(|(name, hash)| (hash, name))
        .collect();
    machine
        .trace()
        .iter()
        .filter_map(|event| pick(event).and_then(|hash| by_hash.get(&hash).cloned()))
        .collect()
}

pub fn spawned_count(machine: &Machine, name: &str) -> usize {
    let hash = machine.fn_hash(name).expect("function hash");
    machine
        .trace()
        .iter()
        .filter(|event| matches!(event, DriveEvent::Spawned { fn_hash } if *fn_hash == hash))
        .count()
}

pub fn memo_hit_count(machine: &Machine, name: &str) -> usize {
    let hash = machine.fn_hash(name).expect("function hash");
    machine
        .trace()
        .iter()
        .filter(|event| matches!(event, DriveEvent::MemoHit { fn_hash } if *fn_hash == hash))
        .count()
}
