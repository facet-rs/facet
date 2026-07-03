//! Composition probes: the language-level memo and the exec-level two-tier
//! cache firing TOGETHER — outputs-as-mounts, chained execs, and the lua
//! sketch building end to end inside the oracle.

use vix::exec::{ExecEvent, Tree};
use vix::oracle::{Event, Oracle, Value};

fn lua_source() -> String {
    std::fs::read_to_string(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../playgrounds/snark/src/bundled/vix/samples/lua.vix"
    ))
    .expect("read lua.vix corpus")
}

fn target() -> Value {
    Value::Struct {
        name: "Target".into(),
        fields: vec![("os".into(), Value::Str("linux-x86_64".into()))],
    }
}

#[test]
fn lua_builds_end_to_end() {
    let oracle = Oracle::load(&lua_source()).expect("load");
    let out = oracle.call("lua", &[("target", target())]).unwrap();

    // The pipeline: fetch -> extract -> subtree -> glob/filter -> 2 unit
    // compiles (lapi, lauxlib) -> collect-merge -> ar -> main compile -> link.
    let Value::Tree(bin) = &out else {
        panic!("lua() returns a Tree, got {out:?}");
    };
    assert!(
        bin.entries.contains_key("lua"),
        "the linked binary: {bin:?}"
    );
    assert!(bin.entries["lua"].starts_with("obj("));

    let events = oracle.events();
    let spawns = events
        .iter()
        .filter(|e| matches!(e, Event::Created { .. }))
        .count();
    let execs: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::Finished { command, event, .. } => Some((command.as_str(), event.clone())),
            _ => None,
        })
        .collect();
    // DEMAND-TRUTHFUL accounting, three moments per run: 5 CREATED
    // (3 compiles + ar + link); 5 SCHEDULED (every run was demanded — the
    // archive and main's object by path PROJECTION, the rest by identity);
    // only 3 FINISHED logged — projection doesn't force, so projected-only
    // runs never log completion.
    assert_eq!(spawns, 5, "{events:?}");
    let scheduled = events
        .iter()
        .filter(|e| matches!(e, Event::Scheduled { .. }))
        .count();
    assert_eq!(scheduled, 5, "{events:?}");
    assert_eq!(execs.len(), 3, "{execs:?}");
    assert!(
        execs
            .iter()
            .all(|(c, e)| *c == "cc" && *e == ExecEvent::Ran)
    );

    // Warm: the WHOLE build is one fn-level memo hit; no exec even consulted.
    let before = oracle.events().len();
    let again = oracle.call("lua", &[("target", target())]).unwrap();
    assert_eq!(out, again);
    let warm = &oracle.events()[before..];
    assert_eq!(warm.len(), 1, "{warm:?}");
    assert!(
        matches!(&warm[0], Event::Hit { func, .. } if func == "lua"),
        "{warm:?}"
    );
}

#[test]
fn fn_memo_and_exec_tiers_compose() {
    // The seam composing: a fn-level memo MISS (an argument tree changed)
    // resolving to an exec-level tier-2 CUTOFF (nothing the compile read
    // changed) — the language cache and the exec cache each doing their half.
    let src = r#"
use vix::{Tree, Path, Target};
use caps::Cc;

fn get_cc(target: Target) -> Cc {
    Cc::acquire(target)
}

fn object(cc: Cc, src: Tree, unit: Path) -> Tree {
    cc! { -O2 -I {src} -c {src / unit} -o {unit.with_ext("o")} }
}
"#;
    let oracle = Oracle::load(src).expect("load");
    let cc = oracle.call("get_cc", &[("target", target())]).unwrap();

    let tree_v1 = Value::Tree(Tree::of(&[
        ("lapi.c", "#include \"lua.h\"\n// api impl"),
        ("lua.h", "// the api"),
        ("README", "docs, never read by cc"),
    ]));
    let unit = Value::Path("lapi.c".into());

    let first = oracle
        .call(
            "object",
            &[("cc", cc.clone()), ("src", tree_v1), ("unit", unit.clone())],
        )
        .unwrap();

    // Edit the UNREAD README: the src tree VALUE changes, so the fn memo key
    // changes (miss) — but the exec read-set still verifies (cutoff).
    let tree_v2 = Value::Tree(Tree::of(&[
        ("lapi.c", "#include \"lua.h\"\n// api impl"),
        ("lua.h", "// the api"),
        ("README", "docs, EDITED, still never read"),
    ]));
    let before = oracle.events().len();
    let second = oracle
        .call(
            "object",
            &[("cc", cc.clone()), ("src", tree_v2), ("unit", unit.clone())],
        )
        .unwrap();

    assert_eq!(first, second, "same object, no recompile");
    let warm = &oracle.events()[before..];
    assert!(
        matches!(&warm[0], Event::Miss { func, .. } if func == "object"),
        "fn memo misses (the tree value changed): {warm:?}"
    );
    assert!(
        warm.iter().any(|e| matches!(
            e,
            Event::Finished { command, event: ExecEvent::Tier2Cutoff { .. }, .. } if command == "cc"
        )),
        "exec tier-2 cuts off (nothing read changed): {warm:?}"
    );

    // Edit the READ header: everything must rerun.
    let tree_v3 = Value::Tree(Tree::of(&[
        ("lapi.c", "#include \"lua.h\"\n// api impl"),
        ("lua.h", "// the api CHANGED"),
        ("README", "docs, EDITED, still never read"),
    ]));
    let before = oracle.events().len();
    let third = oracle
        .call("object", &[("cc", cc), ("src", tree_v3), ("unit", unit)])
        .unwrap();
    assert_ne!(first, third, "new header, new object");
    let warm = &oracle.events()[before..];
    assert!(
        warm.iter().any(|e| matches!(
            e,
            Event::Finished {
                event: ExecEvent::Ran,
                ..
            }
        )),
        "{warm:?}"
    );
}
