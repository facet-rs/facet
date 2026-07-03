//! Exec design probes: the two-tier cache keys doing what the synthesis doc
//! claims, against a fake compiler that generates real read-set dynamics
//! (content reads, search-path probes, negative lookups).

use vix::exec::{
    ExecCache, ExecEvent, ExecPlan, FakeCc, Mount, MountedWorld, ReadObservation, ReadSet, Role, Snapshot,
    Tree, verify,
};

const CC_FINGERPRINT: u64 = 0xcc_15_fa_5e;

fn plan() -> ExecPlan {
    ExecPlan {
        argv: vec![
            ("-O2".into(), Role::Flag),
            ("/src/lua.c".into(), Role::Input),
            ("/vendor/include".into(), Role::SearchDir),
            ("/sys/include".into(), Role::SearchDir),
            ("/out/lua.o".into(), Role::Output),
        ],
    }
}

fn src_tree() -> Tree {
    Tree::of(&[
        ("lua.c", "#include \"lauxlib.h\"\nint main() { return 0; }"),
        ("README", "not read by anything"),
    ])
}

fn mounts(src: Tree, vendor: Tree, sys: Tree) -> Vec<Mount> {
    vec![
        Mount { at: "/src".into(), tree: src },
        Mount { at: "/vendor/include".into(), tree: vendor },
        Mount { at: "/sys/include".into(), tree: sys },
    ]
}

/// The header lives in the SECOND search dir — the probe of the first dir is
/// a negative lookup, and it gets PINNED.
fn world() -> Vec<Mount> {
    mounts(
        src_tree(),
        Tree::of(&[("zlib.h", "// unrelated vendored header")]),
        Tree::of(&[("lauxlib.h", "// system lauxlib v1")]),
    )
}

#[test]
fn cold_run_pins_reads_and_negative_lookups() {
    let mut cache = ExecCache::new();
    let out = cache.exec(&plan(), CC_FINGERPRINT, &world(), &FakeCc).unwrap();

    assert_eq!(cache.events, vec![ExecEvent::Ran]);
    assert!(out.outputs.entries.contains_key("/out/lua.o"));

    // The read-set holds: the source, the MISSED probe in /vendor/include,
    // and the hit in /sys/include. The unread README is NOT in it.
    let rs = &out.read_set.entries;
    assert!(matches!(rs.get("/src/lua.c"), Some(ReadObservation::Content(_))));
    assert_eq!(
        rs.get("/vendor/include/lauxlib.h"),
        Some(&ReadObservation::Absent),
        "the miss is an observation too"
    );
    assert!(matches!(
        rs.get("/sys/include/lauxlib.h"),
        Some(ReadObservation::Content(_))
    ));
    assert!(!rs.keys().any(|k| k.contains("README")));
}

#[test]
fn tier1_hits_when_nothing_changed() {
    let mut cache = ExecCache::new();
    cache.exec(&plan(), CC_FINGERPRINT, &world(), &FakeCc).unwrap();
    let again = cache.exec(&plan(), CC_FINGERPRINT, &world(), &FakeCc).unwrap();
    assert_eq!(cache.events, vec![ExecEvent::Ran, ExecEvent::Tier1Hit]);
    assert!(again.outputs.entries.contains_key("/out/lua.o"));
}

#[test]
fn unread_change_cuts_off_at_tier2_the_anti_nix_test() {
    let mut cache = ExecCache::new();
    let first = cache.exec(&plan(), CC_FINGERPRINT, &world(), &FakeCc).unwrap();

    // Touch a file the compile NEVER read: the coarse tier-1 fingerprint of
    // /src changes (Nix would rebuild), but the read-set still verifies —
    // reuse WITHOUT running.
    let mut touched = src_tree();
    touched.entries.insert("README".into(), "edited docs".into());
    let new_world = mounts(
        touched,
        Tree::of(&[("zlib.h", "// unrelated vendored header")]),
        Tree::of(&[("lauxlib.h", "// system lauxlib v1")]),
    );
    let second = cache.exec(&plan(), CC_FINGERPRINT, &new_world, &FakeCc).unwrap();

    // 4 entries: the source, its own-dir probe (C quoted-include semantics),
    // the vendored miss, and the system hit.
    assert!(matches!(cache.events[1], ExecEvent::Tier2Cutoff { verified } if verified == 4));
    assert_eq!(first.outputs, second.outputs);

    // And the cutoff RE-PINNED under the new coarse key: third time is tier-1.
    cache.exec(&plan(), CC_FINGERPRINT, &new_world, &FakeCc).unwrap();
    assert_eq!(cache.events[2], ExecEvent::Tier1Hit);
}

#[test]
fn read_header_change_reruns() {
    let mut cache = ExecCache::new();
    let first = cache.exec(&plan(), CC_FINGERPRINT, &world(), &FakeCc).unwrap();

    let new_world = mounts(
        src_tree(),
        Tree::of(&[("zlib.h", "// unrelated vendored header")]),
        Tree::of(&[("lauxlib.h", "// system lauxlib v2 — CHANGED")]),
    );
    let second = cache.exec(&plan(), CC_FINGERPRINT, &new_world, &FakeCc).unwrap();

    assert_eq!(cache.events, vec![ExecEvent::Ran, ExecEvent::Ran]);
    assert_ne!(first.outputs, second.outputs, "new header, new object");
}

#[test]
fn shadowing_header_diverges_the_pinned_absence() {
    // THE case mtime/depfile systems miss: a NEW header appears EARLIER in
    // the search path. Nothing the first run read changed — but a path it
    // PROBED and found absent now exists. The pinned Absent diverges; rerun
    // picks up the shadowing header and produces a different object.
    let mut cache = ExecCache::new();
    let first = cache.exec(&plan(), CC_FINGERPRINT, &world(), &FakeCc).unwrap();

    let new_world = mounts(
        src_tree(),
        Tree::of(&[
            ("zlib.h", "// unrelated vendored header"),
            ("lauxlib.h", "// VENDORED lauxlib shadows the system one"),
        ]),
        Tree::of(&[("lauxlib.h", "// system lauxlib v1")]),
    );
    let second = cache.exec(&plan(), CC_FINGERPRINT, &new_world, &FakeCc).unwrap();

    assert_eq!(cache.events, vec![ExecEvent::Ran, ExecEvent::Ran]);
    assert_ne!(first.outputs, second.outputs, "the shadow is real input");
    // The new run's read-set no longer contains the vendored miss — it HIT.
    assert!(matches!(
        second.read_set.entries.get("/vendor/include/lauxlib.h"),
        Some(ReadObservation::Content(_))
    ));
}

#[test]
fn capability_change_disables_tier2_reuse() {
    // Same files, different compiler: read-set verification would hold, but
    // the computation identity differs — the candidate index keys on the
    // capability fingerprint, so tier 2 must NOT reuse across it.
    let mut cache = ExecCache::new();
    cache.exec(&plan(), CC_FINGERPRINT, &world(), &FakeCc).unwrap();
    cache.exec(&plan(), 0xdeadbeef, &world(), &FakeCc).unwrap();
    assert_eq!(cache.events, vec![ExecEvent::Ran, ExecEvent::Ran]);
}

#[test]
fn ceiling_is_the_mount_set() {
    // An input outside the mounts is unreadable — the sandbox is the world,
    // not a permission check bolted on after.
    let mut cache = ExecCache::new();
    let escape = ExecPlan {
        argv: vec![
            ("/etc/passwd".into(), Role::Input),
            ("/out/x.o".into(), Role::Output),
        ],
    };
    let err = cache.exec(&escape, CC_FINGERPRINT, &world(), &FakeCc).unwrap_err();
    assert!(err.contains("outside the mounts"), "{err}");
}

#[test]
fn listings_pin_additions_and_deletions() {
    // Directory enumeration is an observation: adding OR removing an entry
    // diverges the pinned listing (the jade.fyi case).
    let tree = Tree::of(&[("a.c", "x"), ("b.c", "y")]);
    let mounts = vec![Mount { at: "/src".into(), tree: tree.clone() }];
    let world = MountedWorld::new(&mounts);

    let mut rs = ReadSet::default();
    let names = world.list("/src").unwrap();
    rs.entries.insert(
        "/src/".into(),
        ReadObservation::Listing({
            use std::hash::{DefaultHasher, Hash, Hasher};
            let mut h = DefaultHasher::new();
            for n in &names {
                n.hash(&mut h);
            }
            h.finish()
        }),
    );
    assert!(verify(&rs, &world));

    // Deletion diverges.
    let mut smaller = tree.clone();
    smaller.entries.remove("b.c");
    let mounts2 = vec![Mount { at: "/src".into(), tree: smaller }];
    assert!(!verify(&rs, &MountedWorld::new(&mounts2)));

    // Addition diverges too.
    let mut bigger = tree;
    bigger.entries.insert("c.c".into(), "z".into());
    let mounts3 = vec![Mount { at: "/src".into(), tree: bigger }];
    assert!(!verify(&rs, &MountedWorld::new(&mounts3)));
}

#[test]
fn normalization_makes_reordered_flags_share_identity() {
    // `cc -c x -O2` == `cc -O2 -c x`: commuting flags sort into one
    // canonical order, so both spellings hit the SAME tier-1 entry. The toy
    // commutativity table stands in for the snark command grammars.
    let a = ExecPlan {
        argv: vec![
            ("-O2".into(), Role::Flag),
            ("-Wall".into(), Role::Flag),
            ("/src/lua.c".into(), Role::Input),
            ("/vendor/include".into(), Role::SearchDir),
            ("/sys/include".into(), Role::SearchDir),
            ("/out/lua.o".into(), Role::Output),
        ],
    };
    let b = ExecPlan {
        argv: vec![
            ("-Wall".into(), Role::Flag),
            ("-O2".into(), Role::Flag),
            ("/src/lua.c".into(), Role::Input),
            ("/vendor/include".into(), Role::SearchDir),
            ("/sys/include".into(), Role::SearchDir),
            ("/out/lua.o".into(), Role::Output),
        ],
    };
    assert_ne!(a.hash(), b.hash(), "byte-shaped hashes differ");
    assert_eq!(a.identity_hash(), b.identity_hash(), "semantic identity agrees");

    let mut cache = ExecCache::new();
    cache.exec(&a, CC_FINGERPRINT, &world(), &FakeCc).unwrap();
    cache.exec(&b, CC_FINGERPRINT, &world(), &FakeCc).unwrap();
    assert_eq!(cache.events, vec![ExecEvent::Ran, ExecEvent::Tier1Hit]);

    // Position-sensitive args do NOT commute: swapped search dirs are a
    // DIFFERENT invocation (header shadowing order changes semantics).
    let swapped = ExecPlan {
        argv: vec![
            ("-O2".into(), Role::Flag),
            ("-Wall".into(), Role::Flag),
            ("/src/lua.c".into(), Role::Input),
            ("/sys/include".into(), Role::SearchDir),
            ("/vendor/include".into(), Role::SearchDir),
            ("/out/lua.o".into(), Role::Output),
        ],
    };
    assert_ne!(a.identity_hash(), swapped.identity_hash());
}
