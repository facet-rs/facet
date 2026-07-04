use std::collections::{BTreeMap, BTreeSet};

use vix::engine::Engine;
use vix::exec::ExecEvent;
use vix::fetch::FakeFetchBackend;
use vix::oracle::{Event, Oracle, Payload, Value};

const LUA_URL: &str = "https://www.lua.org/ftp/lua-5.4.8.tar.gz";
const LUA_ARCHIVE_BYTES: &[u8] = b"lua-5.4.8 fixture archive";

fn sample(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/../playgrounds/snark/src/bundled/vix/samples/{name}",
        env!("CARGO_MANIFEST_DIR"),
    ))
    .expect("read sample")
}

fn sample_fixture(name: &str) -> String {
    std::fs::read_to_string(format!(
        "{}/../playgrounds/snark/src/bundled/vix/samples/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR"),
    ))
    .expect("read sample fixture")
}

fn lua_fetch_backend() -> FakeFetchBackend {
    FakeFetchBackend::new().with_archive(
        LUA_URL,
        LUA_ARCHIVE_BYTES,
        vix::exec::Tree::of(&[
            ("lua-5.4.8/src/lua.h", "// lua.h api"),
            (
                "lua-5.4.8/src/lua.c",
                "#include \"lua.h\"\n// interpreter main",
            ),
            ("lua-5.4.8/src/lapi.c", "#include \"lua.h\"\n// api impl"),
            ("lua-5.4.8/src/lauxlib.c", "#include \"lua.h\"\n// aux lib"),
            (
                "lua-5.4.8/src/luac.c",
                "#include \"lua.h\"\n// compiler main",
            ),
        ]),
    )
}

fn artifact_object(path: &str) -> Value {
    Value::Variant {
        enum_name: "Artifact".into(),
        index: 0,
        name: "Object".into(),
        payload: Payload::Tuple(vec![Value::Path(path.into())]),
    }
}

fn target() -> Value {
    Value::Struct {
        name: "Target".into(),
        fields: vec![("os".into(), Value::Str("linux-x86_64".into()))],
    }
}

fn windows_target() -> Value {
    Value::Struct {
        name: "Target".into(),
        fields: vec![(
            "os".into(),
            Value::Variant {
                enum_name: "Os".into(),
                index: 2,
                name: "Windows".into(),
                payload: Payload::Unit,
            },
        )],
    }
}

fn miss_multiset(events: &[Event]) -> BTreeMap<String, usize> {
    let mut out = BTreeMap::new();
    for event in events {
        if let Event::Miss { func, .. } = event {
            *out.entry(func.clone()).or_insert(0) += 1;
        }
    }
    out
}

fn finished_multiset(events: &[Event]) -> BTreeMap<(String, String), usize> {
    let mut out = BTreeMap::new();
    for event in events {
        if let Event::Finished { command, event, .. } = event {
            let class = match event {
                ExecEvent::Tier1Hit => "tier1-hit".to_string(),
                ExecEvent::Tier2Cutoff { .. } => "tier2-cutoff".to_string(),
                ExecEvent::Ran => "ran".to_string(),
                ExecEvent::Joined => "joined".to_string(),
            };
            *out.entry((command.clone(), class)).or_insert(0) += 1;
        }
    }
    out
}

fn finished_output_paths(events: &[Event]) -> BTreeSet<String> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::Finished { outputs, .. } => Some(outputs),
            _ => None,
        })
        .flat_map(|outputs| outputs.iter().map(|(path, _)| path.clone()))
        .collect()
}

fn created_set(events: &[Event]) -> BTreeSet<(String, Vec<String>)> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::Created { command, argv, .. } => Some((command.clone(), argv.clone())),
            _ => None,
        })
        .collect()
}

fn created_count(events: &[Event]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, Event::Created { .. }))
        .count()
}

fn observation_keys(events: &[Event]) -> BTreeSet<String> {
    events
        .iter()
        .filter_map(|event| match event {
            Event::Observation { key, .. } => Some(key.clone()),
            _ => None,
        })
        .collect()
}

fn scheduled_count(events: &[Event]) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, Event::Scheduled { .. }))
        .count()
}

#[derive(Debug)]
struct EvalOutcome {
    value: Value,
    events: Vec<Event>,
    journal: BTreeMap<String, Value>,
}

#[derive(Debug)]
struct ExpectedContract {
    value: Value,
    finished: BTreeMap<(String, String), usize>,
    finished_paths: BTreeSet<String>,
    scheduled: usize,
    created: usize,
    observations: BTreeSet<String>,
    journal: BTreeMap<String, Value>,
}

fn engine_outcome(
    source: &str,
    func: &str,
    args: &[(&str, Value)],
    fetch_backend: Option<FakeFetchBackend>,
) -> EvalOutcome {
    let mut engine = Engine::load(source).expect("engine load");
    if let Some(fetch_backend) = fetch_backend {
        engine = engine.with_fetch_backend(fetch_backend);
    }
    let value = engine.call(func, args).expect("engine call");
    EvalOutcome {
        value,
        events: engine.events(),
        journal: engine.journal(),
    }
}

fn assert_contract(actual: EvalOutcome, expected: ExpectedContract) {
    assert_eq!(actual.value, expected.value);
    assert_eq!(
        finished_multiset(&actual.events),
        expected.finished,
        "Finished command/event-class multiset differs\nactual={:?}",
        actual.events
    );
    assert_eq!(
        finished_output_paths(&actual.events),
        expected.finished_paths,
        "Finished output paths differ\nactual={:?}",
        actual.events
    );
    assert_eq!(
        scheduled_count(&actual.events),
        expected.scheduled,
        "Scheduled count differs\nactual={:?}",
        actual.events
    );
    assert_eq!(
        created_count(&actual.events),
        expected.created,
        "Created count differs\nactual={:?}",
        actual.events
    );
    assert_eq!(
        observation_keys(&actual.events),
        expected.observations,
        "Observation key set differs\nactual={:?}",
        actual.events
    );
    assert_eq!(actual.journal, expected.journal, "journal pins differ");
}

fn str_value(s: &str) -> Value {
    Value::Str(s.into())
}

fn tree_value(entries: &[(&str, &str)]) -> Value {
    Value::Tree(vix::exec::Tree::of(entries))
}

fn cc_value(fingerprint: &str) -> Value {
    Value::Struct {
        name: "Cc".into(),
        fields: vec![("fingerprint".into(), str_value(fingerprint))],
    }
}

fn ar_value(fingerprint: &str) -> Value {
    Value::Struct {
        name: "Ar".into(),
        fields: vec![("fingerprint".into(), str_value(fingerprint))],
    }
}

fn empty_contract(value: Value) -> ExpectedContract {
    ExpectedContract {
        value,
        finished: BTreeMap::new(),
        finished_paths: BTreeSet::new(),
        scheduled: 0,
        created: 0,
        observations: BTreeSet::new(),
        journal: BTreeMap::new(),
    }
}

fn toolchain_windows_contract() -> ExpectedContract {
    const CC_PIN: &str = "acquire:Cc:1c17ad644df20b15";
    const AR_PIN: &str = "acquire:Ar:1c17ad644df20b15";
    ExpectedContract {
        value: Value::Struct {
            name: "Toolchain".into(),
            fields: vec![
                ("cc".into(), cc_value(CC_PIN)),
                ("ar".into(), ar_value(AR_PIN)),
                ("opt".into(), Value::Int(1)),
                (
                    "env".into(),
                    Value::Map(BTreeMap::from([
                        (str_value("CFLAGS"), str_value("-O2")),
                        (str_value("LDFLAGS"), str_value("-lm")),
                    ])),
                ),
            ],
        },
        finished: BTreeMap::new(),
        finished_paths: BTreeSet::new(),
        scheduled: 0,
        created: 0,
        observations: BTreeSet::from([AR_PIN.into(), CC_PIN.into()]),
        journal: BTreeMap::from([
            (AR_PIN.into(), ar_value(AR_PIN)),
            (CC_PIN.into(), cc_value(CC_PIN)),
        ]),
    }
}

fn lua_contract() -> ExpectedContract {
    const CC_PIN: &str = "acquire:Cc:391c555cf0975f9c";
    const AR_PIN: &str = "acquire:Ar:391c555cf0975f9c";
    const FETCH_PIN: &str = "fetch:https://www.lua.org/ftp/lua-5.4.8.tar.gz:sha256:f5c9123295667d2cc0841c03490f04d6e66d0eac5e440ab386a944eec30e64d7";
    ExpectedContract {
        value: tree_value(&[("lua", "obj(da0b3249eab2761b)")]),
        finished: BTreeMap::from([(("cc".into(), "ran".into()), 3)]),
        finished_paths: BTreeSet::from(["lapi.o".into(), "lauxlib.o".into(), "lua".into()]),
        scheduled: 5,
        created: 5,
        observations: BTreeSet::from([AR_PIN.into(), CC_PIN.into(), FETCH_PIN.into()]),
        journal: BTreeMap::from([
            (AR_PIN.into(), ar_value(AR_PIN)),
            (CC_PIN.into(), cc_value(CC_PIN)),
            (
                FETCH_PIN.into(),
                str_value("f5c9123295667d2cc0841c03490f04d6e66d0eac5e440ab386a944eec30e64d7"),
            ),
        ]),
    }
}

fn linux_cc_journal() -> BTreeMap<String, Value> {
    const CC_PIN: &str = "acquire:Cc:391c555cf0975f9c";
    BTreeMap::from([(CC_PIN.into(), cc_value(CC_PIN))])
}

fn linux_cc_observations() -> BTreeSet<String> {
    BTreeSet::from(["acquire:Cc:391c555cf0975f9c".into()])
}

fn merge_contract(
    value_entry: (&str, &str),
    finished: BTreeMap<(String, String), usize>,
    finished_paths: BTreeSet<String>,
    scheduled: usize,
) -> ExpectedContract {
    ExpectedContract {
        value: tree_value(&[value_entry]),
        finished,
        finished_paths,
        scheduled,
        created: 2,
        observations: linux_cc_observations(),
        journal: linux_cc_journal(),
    }
}

fn hit_count(events: &[Event], name: &str) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, Event::Hit { func, .. } if func == name))
        .count()
}

fn miss_count(events: &[Event], name: &str) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, Event::Miss { func, .. } if func == name))
        .count()
}

fn assert_misses_subset(engine: &[Event], oracle: &[Event]) {
    let engine = miss_multiset(engine);
    let oracle = miss_multiset(oracle);
    for (func, engine_count) in engine {
        let oracle_count = oracle.get(&func).copied().unwrap_or(0);
        assert!(
            engine_count <= oracle_count,
            "engine miss count for {func}: {engine_count}, oracle: {oracle_count}"
        );
    }
}

fn assert_created_subset(engine: &[Event], oracle: &[Event]) {
    let engine = created_set(engine);
    let oracle = created_set(oracle);
    for created in &engine {
        assert!(
            oracle.contains(created),
            "engine created run absent from oracle: {created:?}\nengine={engine:?}\noracle={oracle:?}"
        );
    }
}

fn assert_full_contract(source: &str, func: &str, args: &[(&str, Value)], exact_created: bool) {
    let backend = lua_fetch_backend();
    let oracle = Oracle::load(source)
        .expect("oracle load")
        .with_fetch_backend(backend.clone());
    let mut engine = Engine::load(source)
        .expect("engine load")
        .with_fetch_backend(backend);
    let oracle_value = oracle.call(func, args).expect("oracle call");
    let engine_value = engine.call(func, args).expect("engine call");
    assert_eq!(engine_value, oracle_value);
    let engine_events = engine.events();
    let oracle_events = oracle.events();
    assert_eq!(
        finished_multiset(&engine_events),
        finished_multiset(&oracle_events),
        "Finished command/event-class multiset differs\nengine={engine_events:?}\noracle={oracle_events:?}"
    );
    assert_eq!(
        scheduled_count(&engine_events),
        scheduled_count(&oracle_events),
        "Scheduled counts differ\nengine={engine_events:?}\noracle={oracle_events:?}"
    );
    assert_eq!(
        observation_keys(&engine_events),
        observation_keys(&oracle_events),
        "Observation key sets differ\nengine={engine_events:?}\noracle={oracle_events:?}"
    );
    assert_eq!(engine.journal(), oracle.journal(), "journal pins differ");
    assert_misses_subset(&engine_events, &oracle_events);
    if exact_created {
        assert_eq!(
            created_set(&engine_events),
            created_set(&oracle_events),
            "Created sets differ\nengine={engine_events:?}\noracle={oracle_events:?}"
        );
    } else {
        assert_created_subset(&engine_events, &oracle_events);
    }
}

#[test]
fn eval_vix_contract_is_pinned() {
    assert_contract(
        engine_outcome(&sample("eval.vix"), "demo", &[], None),
        empty_contract(Value::Float(42.0)),
    );
}

#[test]
fn types_vix_contract_is_pinned() {
    let src = sample("types.vix");
    assert_contract(
        engine_outcome(&src, "partials", &[], None),
        empty_contract(Value::Int(42)),
    );
    assert_contract(
        engine_outcome(&src, "depths", &[], None),
        empty_contract(Value::Int(2)),
    );
    assert_contract(
        engine_outcome(&src, "classify", &[("a", artifact_object("lua.o"))], None),
        empty_contract(str_value("the interpreter object")),
    );
    assert_contract(
        engine_outcome(&src, "classify", &[("a", artifact_object("lapi.o"))], None),
        empty_contract(str_value("an object")),
    );
    assert_contract(
        engine_outcome(&src, "toolchain", &[("target", windows_target())], None),
        toolchain_windows_contract(),
    );
}

#[test]
fn lua_vix_contract_is_pinned() {
    assert_contract(
        engine_outcome(
            &sample("lua.vix"),
            "lua",
            &[("target", target())],
            Some(lua_fetch_backend()),
        ),
        lua_contract(),
    );
}

#[test]
fn cargo_toml_projection_contract_is_pinned() {
    let manifest = sample_fixture("Cargo.toml");
    let tree = Value::Tree(vix::exec::Tree::of(&[("Cargo.toml", &manifest)]));
    assert_contract(
        engine_outcome(
            &sample("cargo.vix"),
            "cargo_manifest",
            &[("manifest", tree)],
            None,
        ),
        empty_contract(Value::Tuple(vec![
            str_value("mini-real-crate"),
            str_value("0.3.1"),
            str_value("0.50.0-rc.5"),
        ])),
    );
}

#[test]
fn json_structural_values_contract_is_pinned() {
    let src = r#"
pub fn parse(input: String) -> (String, Int, Bool) {
    let doc = json(input);
    let package = doc.get("package").unwrap();
    (
        package.get("name").unwrap(),
        package.get("version").unwrap(),
        doc.get("publish").unwrap(),
    )
}
"#;
    assert_contract(
        engine_outcome(
            src,
            "parse",
            &[(
                "input",
                Value::Str(
                    r#"{"package":{"name":"mini-real-crate","version":3},"publish":false}"#.into(),
                ),
            )],
            None,
        ),
        empty_contract(Value::Tuple(vec![
            str_value("mini-real-crate"),
            Value::Int(3),
            Value::Bool(false),
        ])),
    );
}

#[test]
fn fetch_without_declared_checksum_contract_is_pinned() {
    const URL: &str = "https://example.org/source.tar.gz";
    const ARCHIVE: &[u8] = b"source fixture archive";
    const EXPECTED_PIN: &str = "45277631a6bcd7e364dcad299eac30f77f2b96e5a4b17666553331e52fcbb231";
    const JOURNAL_KEY: &str = "fetch:https://example.org/source.tar.gz:observed";
    let src = format!(
        r#"
use vix::Tree;

pub fn src_tree(nonce: Int) -> Tree {{
    fetch(url: "{URL}")
}}
"#
    );
    let backend = FakeFetchBackend::new().with_archive(
        URL,
        ARCHIVE,
        vix::exec::Tree::of(&[("src/lib.rs", "pub fn f() {}")]),
    );
    let mut engine = Engine::load(&src)
        .expect("engine load")
        .with_fetch_backend(backend);

    let expected_value = tree_value(&[("src/lib.rs", "pub fn f() {}")]);
    let engine_first = engine
        .call("src_tree", &[("nonce", Value::Int(1))])
        .expect("engine first call");
    let engine_second = engine
        .call("src_tree", &[("nonce", Value::Int(2))])
        .expect("engine second call");

    assert_eq!(engine_first, expected_value);
    assert_eq!(engine_second, expected_value);
    assert_contract(
        EvalOutcome {
            value: engine_second,
            events: engine.events(),
            journal: engine.journal(),
        },
        ExpectedContract {
            value: expected_value,
            finished: BTreeMap::new(),
            finished_paths: BTreeSet::new(),
            scheduled: 0,
            created: 0,
            observations: BTreeSet::from([JOURNAL_KEY.into()]),
            journal: BTreeMap::from([(JOURNAL_KEY.into(), str_value(EXPECTED_PIN))]),
        },
    );
}

#[test]
fn engine_tunnels_path_demand_through_merge() {
    assert_contract(
        engine_outcome(
            &sample("merge-demand.vix"),
            "selected",
            &[("target", target())],
            None,
        ),
        merge_contract(
            ("wanted.o", "obj(9259fea8a69f1945)"),
            BTreeMap::new(),
            BTreeSet::new(),
            1,
        ),
    );
}

#[test]
fn engine_falls_left_after_right_merge_absence_is_known() {
    assert_contract(
        engine_outcome(
            &sample("merge-demand.vix"),
            "fallback",
            &[("target", target())],
            None,
        ),
        merge_contract(
            ("wanted.o", "obj(9259fea8a69f1945)"),
            BTreeMap::from([(("cc".into(), "ran".into()), 1)]),
            BTreeSet::from(["right.o".into()]),
            2,
        ),
    );
}

#[test]
fn engine_refines_subtree_chain_through_merge() {
    assert_contract(
        engine_outcome(
            &sample("merge-demand.vix"),
            "subtree_chain",
            &[("target", target())],
            None,
        ),
        merge_contract(
            ("wanted.o", "obj(9259fea8a69f1945)"),
            BTreeMap::new(),
            BTreeSet::new(),
            1,
        ),
    );
}

// A5 freezes the oracle and recursive engine. These parity tests remain only
// until vix::machine reaches corpus parity, then die with the frozen evaluators.
mod frozen_parity {
    use super::*;

    #[test]
    fn engine_matches_oracle_on_eval_vix() {
        assert_full_contract(&sample("eval.vix"), "demo", &[], false);
    }

    #[test]
    fn engine_matches_oracle_on_types_vix() {
        let src = sample("types.vix");
        assert_full_contract(&src, "partials", &[], false);
        assert_full_contract(&src, "depths", &[], false);
        assert_full_contract(&src, "classify", &[("a", artifact_object("lua.o"))], false);
        assert_full_contract(&src, "classify", &[("a", artifact_object("lapi.o"))], false);
        assert_full_contract(&src, "toolchain", &[("target", windows_target())], false);
    }

    #[test]
    fn engine_matches_oracle_on_lua_vix_exec_seam() {
        let src = sample("lua.vix");
        let backend = lua_fetch_backend();
        let oracle = Oracle::load(&src)
            .expect("oracle load")
            .with_fetch_backend(backend.clone());
        let mut engine = Engine::load(&src)
            .expect("engine load")
            .with_fetch_backend(backend);

        let oracle_value = oracle
            .call("lua", &[("target", target())])
            .expect("oracle call");
        let engine_value = engine
            .call("lua", &[("target", target())])
            .expect("engine call");
        assert_eq!(engine_value, oracle_value);

        let engine_events = engine.events();
        let oracle_events = oracle.events();
        assert_eq!(
            finished_multiset(&engine_events),
            finished_multiset(&oracle_events)
        );
        assert_eq!(scheduled_count(&engine_events), 5, "{engine_events:?}");
        assert_eq!(scheduled_count(&oracle_events), 5, "{oracle_events:?}");
        assert_eq!(created_set(&engine_events), created_set(&oracle_events));
        assert_eq!(
            observation_keys(&engine_events),
            observation_keys(&oracle_events)
        );
        assert_eq!(engine.journal(), oracle.journal());
        assert_misses_subset(&engine_events, &oracle_events);
    }

    #[test]
    fn engine_matches_oracle_on_cargo_toml_projection() {
        let manifest = sample_fixture("Cargo.toml");
        let tree = Value::Tree(vix::exec::Tree::of(&[("Cargo.toml", &manifest)]));
        assert_full_contract(
            &sample("cargo.vix"),
            "cargo_manifest",
            &[("manifest", tree)],
            false,
        );
    }

    #[test]
    fn engine_matches_oracle_on_json_structural_values() {
        let src = r#"
pub fn parse(input: String) -> (String, Int, Bool) {
    let doc = json(input);
    let package = doc.get("package").unwrap();
    (
        package.get("name").unwrap(),
        package.get("version").unwrap(),
        doc.get("publish").unwrap(),
    )
}
"#;
        assert_full_contract(
            src,
            "parse",
            &[(
                "input",
                Value::Str(
                    r#"{"package":{"name":"mini-real-crate","version":3},"publish":false}"#.into(),
                ),
            )],
            false,
        );
    }

    #[test]
    fn engine_matches_oracle_on_fetch_without_declared_checksum() {
        const URL: &str = "https://example.org/source.tar.gz";
        const ARCHIVE: &[u8] = b"source fixture archive";
        const EXPECTED_PIN: &str =
            "45277631a6bcd7e364dcad299eac30f77f2b96e5a4b17666553331e52fcbb231";
        let src = format!(
            r#"
use vix::Tree;

pub fn src_tree(nonce: Int) -> Tree {{
    fetch(url: "{URL}")
}}
"#
        );
        let backend = FakeFetchBackend::new().with_archive(
            URL,
            ARCHIVE,
            vix::exec::Tree::of(&[("src/lib.rs", "pub fn f() {}")]),
        );
        let oracle = Oracle::load(&src)
            .expect("oracle load")
            .with_fetch_backend(backend.clone());
        let mut engine = Engine::load(&src)
            .expect("engine load")
            .with_fetch_backend(backend);

        let oracle_first = oracle
            .call("src_tree", &[("nonce", Value::Int(1))])
            .expect("oracle first call");
        let oracle_second = oracle
            .call("src_tree", &[("nonce", Value::Int(2))])
            .expect("oracle second call");
        let engine_first = engine
            .call("src_tree", &[("nonce", Value::Int(1))])
            .expect("engine first call");
        let engine_second = engine
            .call("src_tree", &[("nonce", Value::Int(2))])
            .expect("engine second call");

        assert_eq!(oracle_first, engine_first);
        assert_eq!(oracle_second, engine_second);
        assert_eq!(oracle_first, oracle_second);
        assert_eq!(engine.journal(), oracle.journal());
        assert_eq!(
            oracle.journal(),
            BTreeMap::from([(
                format!("fetch:{URL}:observed"),
                Value::Str(EXPECTED_PIN.into())
            )])
        );
        assert_eq!(
            observation_keys(&engine.events()),
            observation_keys(&oracle.events())
        );
    }

    #[test]
    fn engine_merge_finished_subset_of_oracle_for_selected() {
        let src = sample("merge-demand.vix");
        let oracle = Oracle::load(&src).expect("oracle load");
        let mut engine = Engine::load(&src).expect("engine load");

        let oracle_value = oracle
            .call("selected", &[("target", target())])
            .expect("oracle call");
        let engine_value = engine
            .call("selected", &[("target", target())])
            .expect("engine call");

        assert_eq!(engine_value, oracle_value);
        assert_eq!(engine.journal(), oracle.journal(), "journal pins differ");

        let oracle_finished = finished_output_paths(&oracle.events());
        let engine_finished = finished_output_paths(&engine.events());
        assert!(
            engine_finished.is_subset(&oracle_finished),
            "engine finished outputs must be an oracle subset\nengine={:?}\noracle={:?}",
            engine.events(),
            oracle.events()
        );
        assert!(
            engine_finished.len() < oracle_finished.len(),
            "expected strict Finished subset\nengine={:?}\noracle={:?}",
            engine.events(),
            oracle.events()
        );
        assert!(
            !engine_finished.contains("left.o"),
            "left object must not finish under engine path demand: {:?}",
            engine.events()
        );
        assert!(
            oracle_finished.contains("left.o"),
            "oracle stays eager and finishes left object: {:?}",
            oracle.events()
        );
        assert!(
            oracle_finished.contains("wanted.o"),
            "oracle finishes right object: {:?}",
            oracle.events()
        );
    }

    #[test]
    fn engine_merge_finished_subset_of_oracle_for_fallback() {
        let src = sample("merge-demand.vix");
        let oracle = Oracle::load(&src).expect("oracle load");
        let mut engine = Engine::load(&src).expect("engine load");

        let oracle_value = oracle
            .call("fallback", &[("target", target())])
            .expect("oracle call");
        let engine_value = engine
            .call("fallback", &[("target", target())])
            .expect("engine call");

        assert_eq!(engine_value, oracle_value);
        let oracle_finished = finished_output_paths(&oracle.events());
        let engine_finished = finished_output_paths(&engine.events());

        assert!(
            engine_finished.contains("right.o"),
            "right candidate must finish before absence is known: {:?}",
            engine.events()
        );
        assert!(
            !engine_finished.contains("wanted.o"),
            "winning left candidate is served by path demand, not full finish: {:?}",
            engine.events()
        );
        assert!(
            oracle_finished.contains("wanted.o") && oracle_finished.contains("right.o"),
            "oracle stays eager: {:?}",
            oracle.events()
        );
    }

    #[test]
    fn engine_merge_finished_subset_of_oracle_for_subtree_chain() {
        let src = sample("merge-demand.vix");
        let oracle = Oracle::load(&src).expect("oracle load");
        let mut engine = Engine::load(&src).expect("engine load");

        let oracle_value = oracle
            .call("subtree_chain", &[("target", target())])
            .expect("oracle call");
        let engine_value = engine
            .call("subtree_chain", &[("target", target())])
            .expect("engine call");

        assert_eq!(engine_value, oracle_value);
        let oracle_finished = finished_output_paths(&oracle.events());
        let engine_finished = finished_output_paths(&engine.events());

        assert!(
            !engine_finished.contains("left.o"),
            "left object must not finish when chained projection selects x/wanted.o: {:?}",
            engine.events()
        );
        assert!(
            !engine_finished.contains("x/wanted.o"),
            "selected nested object is served by path demand, not full finish: {:?}",
            engine.events()
        );
        assert!(
            oracle_finished.contains("left.o") && oracle_finished.contains("x/wanted.o"),
            "oracle stays eager across chained projection: {:?}",
            oracle.events()
        );
    }
}

#[test]
fn collect_argument_strictness_is_pinned() {
    let src = r#"
pub fn bad() -> [Int] {
    [2, 1].collect(0)
}

pub fn good() -> [Int] {
    [2, 1].collect()
}
"#;
    let mut engine = Engine::load(src).expect("engine load");

    let engine_err = engine.call("bad", &[]).expect_err("engine rejects args");
    assert_eq!(engine_err, "collect takes no arguments");

    let expected = Value::Array(vec![Value::Int(1), Value::Int(2)]);
    assert_eq!(engine.call("good", &[]).unwrap(), expected);
}

#[test]
fn resolved_tree_missing_path_errors_immediately() {
    let src = r#"
use vix::Tree;

pub fn main(input: Tree) -> Tree {
    input / p"missing.txt"
}
"#;
    let tree = Value::Tree(vix::exec::Tree::of(&[("present.txt", "ok")]));
    let mut engine = Engine::load(src).expect("engine load");

    let engine_err = engine
        .call("main", &[("input", tree)])
        .expect_err("engine missing path");

    assert!(engine_err.contains("missing.txt"), "{engine_err}");
    assert_eq!(
        scheduled_count(&engine.events()),
        0,
        "{:?}",
        engine.events()
    );
    assert_eq!(finished_multiset(&engine.events()), BTreeMap::new());
}

#[test]
fn pending_tree_path_projection_serves_file_without_finish() {
    let src = r#"
use vix::Target;
use caps::Cc;

pub fn main(target: Target) -> Tree {
    let cc = Cc::acquire(target);
    cc! { -o artifact.o } / p"artifact.o"
}
"#;
    let mut engine = Engine::load(src).expect("engine load");

    let engine_value = engine.call("main", &[("target", target())]).unwrap();

    let Value::Tree(tree) = engine_value else {
        panic!("projection should return a tree");
    };
    assert_eq!(
        tree.entries.keys().cloned().collect::<Vec<_>>(),
        vec!["artifact.o".to_string()]
    );
    assert_eq!(
        scheduled_count(&engine.events()),
        1,
        "{:?}",
        engine.events()
    );
    assert_eq!(finished_multiset(&engine.events()), BTreeMap::new());
}

#[test]
fn pending_tree_missing_path_errors_when_producer_finishes() {
    let src = r#"
use vix::Target;
use caps::Cc;

pub fn main(target: Target) -> Tree {
    let cc = Cc::acquire(target);
    cc! { -o artifact.o } / p"never-written.o"
}
"#;
    let mut engine = Engine::load(src).expect("engine load");

    let engine_err = engine
        .call("main", &[("target", target())])
        .expect_err("engine missing produced path");

    assert!(engine_err.contains("never-written.o"), "{engine_err}");
    assert_eq!(
        scheduled_count(&engine.events()),
        1,
        "{:?}",
        engine.events()
    );
    assert_eq!(
        finished_multiset(&engine.events()),
        BTreeMap::from([(("cc".to_string(), "ran".to_string()), 1)])
    );
}

#[test]
fn warm_engine_lua_second_call_is_one_hit() {
    let src = sample("lua.vix");
    let mut engine = Engine::load(&src)
        .expect("engine load")
        .with_fetch_backend(lua_fetch_backend());

    let first = engine.call("lua", &[("target", target())]).unwrap();
    let before = engine.events().len();
    let second = engine.call("lua", &[("target", target())]).unwrap();

    assert_eq!(first, second);
    let warm = &engine.events()[before..];
    assert_eq!(warm.len(), 1, "{warm:?}");
    assert!(
        matches!(&warm[0], Event::Hit { func, .. } if func == "lua"),
        "{warm:?}"
    );
}

#[test]
fn unused_command_binding_is_never_created_by_engine() {
    let src = r#"
use vix::Target;
use caps::Cc;

pub fn main(target: Target) -> Int {
    let cc = Cc::acquire(target);
    let dead = cc! { -o dead };
    7
}
"#;
    let mut engine = Engine::load(src).expect("engine load");

    assert_eq!(
        engine.call("main", &[("target", target())]).unwrap(),
        Value::Int(7)
    );
    assert!(
        created_set(&engine.events()).is_empty(),
        "{:?}",
        engine.events()
    );
    assert_eq!(finished_multiset(&engine.events()), BTreeMap::new());
}

#[test]
fn unused_binding_is_never_demanded() {
    let src = r#"
fn expensive() -> Int {
    41
}

pub fn main() -> Int {
    let x = expensive();
    7
}
"#;
    let mut engine = Engine::load(src).expect("engine load");

    assert_eq!(engine.call("main", &[]).unwrap(), Value::Int(7));
    assert_eq!(miss_count(&engine.events(), "expensive"), 0);
}

#[test]
fn shared_binding_computes_once() {
    let src = r#"
fn f(x: Int) -> Int {
    x + 1
}

pub fn main() -> Int {
    let x = f(20);
    x + x
}
"#;
    let mut engine = Engine::load(src).expect("engine load");

    assert_eq!(engine.call("main", &[]).unwrap(), Value::Int(42));
    let events = engine.events();
    assert_eq!(miss_count(&events, "f"), 1);
    assert_eq!(hit_count(&events, "f"), 0);
}

#[test]
fn unselected_match_arm_never_evaluates() {
    let src = r#"
fn boom() -> Int {
    1 / 0
}

pub fn main() -> Int {
    match 0 {
        0 => 42,
        _ => boom(),
    }
}
"#;
    let mut engine = Engine::load(src).expect("engine load");

    assert_eq!(engine.call("main", &[]).unwrap(), Value::Int(42));
    assert_eq!(miss_count(&engine.events(), "boom"), 0);
}

#[test]
fn memo_hits_across_calls() {
    let src = r#"
fn f(x: Int) -> Int {
    x + 1
}

fn a() -> Int {
    f(20)
}

fn b() -> Int {
    f(20)
}

pub fn main() -> Int {
    a() + b()
}
"#;
    let mut engine = Engine::load(src).expect("engine load");

    assert_eq!(engine.call("main", &[]).unwrap(), Value::Int(42));
    let events = engine.events();
    assert_eq!(miss_count(&events, "f"), 1);
    assert!(hit_count(&events, "f") >= 1, "{events:?}");
}

#[test]
fn oracle_rejects_duplicate_named_arguments() {
    let src = r#"
fn f(x: Int) -> Int {
    x
}

pub fn main() -> Int {
    f(x: 1, x: 2)
}
"#;
    let oracle = Oracle::load(src).expect("oracle load");
    let err = oracle
        .call("main", &[])
        .expect_err("duplicate argument errors");
    assert!(err.contains("duplicate argument `x`"), "{err}");
}

#[test]
fn engine_rejects_duplicate_named_arguments() {
    let src = r#"
fn f(x: Int) -> Int {
    x
}

pub fn main() -> Int {
    f(x: 1, x: 2)
}
"#;
    let mut engine = Engine::load(src).expect("engine load");
    let err = engine
        .call("main", &[])
        .expect_err("duplicate argument errors");
    assert!(err.contains("duplicate argument `x`"), "{err}");
}
