//! Exec input mounts — a `Tree` spliced into a command template.
//!
//! Before this, an exec request was `{capability, argv}` and a template spliced
//! `Int | String | Path` only, so there was NO WAY to put an input file in front
//! of a process. A tree projection is the file's contents, not a path, and there
//! were no mounts: rustc could be spawned but never shown `src/main.rs`. Rung
//! `073-exec-consumes-tree.vix` is that shape and is excluded from the runner.
//!
//! A `Tree` splice now contributes the tree to the request's `mounts` array and
//! renders as its deterministic workspace-relative path, so the argv and the
//! mount list are two projections of one plan. The tree's value identity is in
//! the request, which is what makes "the same plan over changed sources" a
//! different demand rather than a stale memo hit.

#![cfg(unix)]

use vixen_runtime::manifest::{CapabilityOffer, MachineManifest, host_target};
use vixen_runtime::ratchet::{RatchetReport, RunError, run_source_with_manifest};

/// The ratchet default (`Echo`/`Sh`/`ProgressiveSh`) plus a real `Rustc`, so
/// the compile tests drive the toolchain this repo is built with rather than
/// whatever a bare `rustc` on PATH resolves to.
fn manifest_with_rustc() -> MachineManifest {
    let mut manifest = MachineManifest::ratchet_default();
    manifest.capabilities.push(CapabilityOffer {
        ty: "Rustc".to_owned(),
        program: rustc_program(),
        toolchain: Some("host".to_owned()),
        targets: vec![host_target()],
    });
    manifest
}

fn rustc_program() -> String {
    std::env::var("RUSTC").unwrap_or_else(|_| "rustc".to_owned())
}

fn run(source: &str) -> Result<RatchetReport, RunError> {
    run_source_with_manifest(source, manifest_with_rustc())
}

fn expect_pass(label: &str, source: &str) -> RatchetReport {
    let report = run(source).unwrap_or_else(|error| panic!("{label}: {error:#?}"));
    assert!(report.passed(), "{label}: {:?}", report.plain.checks);
    report
}

#[test]
fn a_mounted_tree_puts_a_file_in_front_of_the_process() {
    let report = expect_pass(
        "mount",
        r#"
#[test { budget_wall: 60s, budget_rss: 2048MB }]
fn t(sh: Sh) -> Stream<Check> {
    let produced = exec sh`-c "mkdir -p nested; echo mounted > nested/note.txt"`;
    let inputs = produced.tree;
    let seen = exec sh`-c "cat {inputs}/nested/note.txt"`;
    yield expect_eq(seen.stdout.text().trim(), "mounted");
}
"#,
    );
    assert_eq!(report.plain.counters.effect_spawns, 2, "two processes ran");
}

#[test]
fn the_executable_bit_survives_capture_and_mount() {
    // The bit is part of what the output says about itself. Losing it at
    // capture (the archive hardcoded 0644) made a compiler's product a file the
    // next stage could not run — exit 126, which names a permission, not a
    // missing feature, so it is worth pinning from both ends.
    expect_pass(
        "executable",
        r#"
#[test { budget_wall: 60s, budget_rss: 2048MB }]
fn t(sh: Sh) -> Stream<Check> {
    let produced = exec sh`-c "printf '#!/bin/sh\necho ran\n' > tool; chmod +x tool"`;
    let inputs = produced.tree;
    let ran = exec sh`-c "{inputs}/tool"`;
    yield expect_eq(ran.stdout.text().trim(), "ran");
}
"#,
    );
}

#[test]
fn a_non_executable_file_stays_non_executable() {
    // The negative half: the archive must not promote everything to 0755 to
    // make the test above pass.
    expect_pass(
        "not-executable",
        r#"
#[test { budget_wall: 60s, budget_rss: 2048MB }]
fn t(sh: Sh) -> Stream<Check> {
    let produced = exec sh`-c "echo plain > note.txt"`;
    let inputs = produced.tree;
    let probed = exec sh`-c "test -x {inputs}/note.txt && echo yes || echo no"`;
    yield expect_eq(probed.stdout.text().trim(), "no");
}
"#,
    );
}

#[test]
fn vix_compiles_and_runs_a_hello_world() {
    // Rung one of the ladder. Three processes: write the source, compile it with
    // real rustc, run the binary.
    //
    // The source is the SHIPPED example, read from disk rather than copied here,
    // so `cargo-vix/hello.vix` cannot drift from the thing that is proven green.
    let report = expect_pass("hello-world", include_str!("../../cargo-vix/hello.vix"));
    assert_eq!(
        report.plain.counters.effect_spawns, 3,
        "write, compile, run — the binary really was produced and executed"
    );
}

#[test]
fn a_mount_re_keys_the_demand_when_its_sources_change() {
    // The identity property the mounts field exists for: two compiles of the
    // same plan over DIFFERENT sources are different demands. If the mount were
    // absent from the request they would collide and the second would serve the
    // first's memo — a build system's worst bug.
    expect_pass(
        "re-key",
        r#"
#[test { budget_wall: 60s, budget_rss: 2048MB }]
fn t(sh: Sh) -> Stream<Check> {
    let one = exec sh`-c "echo first > note.txt"`;
    let two = exec sh`-c "echo second > note.txt"`;
    let a = one.tree;
    let b = two.tree;
    let read_a = exec sh`-c "cat {a}/note.txt"`;
    let read_b = exec sh`-c "cat {b}/note.txt"`;
    yield expect_eq(read_a.stdout.text().trim(), "first");
    yield expect_eq(read_b.stdout.text().trim(), "second");
}
"#,
    );
}

#[test]
fn mounting_an_origin_backed_tree_refuses_loudly() {
    // NOT YET SUPPORTED, and it says so. Enumerating an origin-backed handle
    // needs a directory verb on the effect authority; falling through to
    // content enumeration would report "malformed bytes" for what is a missing
    // machine verb — the confusing lie `machine.primitive.origin-routing` bans.
    let source = r#"
#[test { budget_wall: 60s, budget_rss: 2048MB }]
fn t(sh: Sh) -> Stream<Check> {
    let sources = fixture_tree("small-crate");
    let listed = exec sh`-c "ls {sources}"`;
    yield expect_eq(listed.stdout.text().trim(), "unreachable");
}
"#;
    let error = run(source).expect_err("an origin-backed mount is refused");
    let rendered = format!("{error:?}");
    assert!(
        rendered.contains("directory verb"),
        "the refusal names the missing verb rather than blaming the bytes: {rendered}"
    );
}

#[test]
fn a_mount_does_not_come_back_in_the_output_tree() {
    // Inputs are not outputs. Without excluding the mount root from capture,
    // stage N's tree carries stage N-1's sources, mounting it into stage N+1
    // nests them again, and a build chain grows quadratically until it hits the
    // ustar path cap. Worse, two byte-identical products would get DIFFERENT
    // identities depending on what they were built from — the exact opposite of
    // what an output identity is for.
    expect_pass(
        "no-leak",
        r#"
#[test { budget_wall: 60s, budget_rss: 2048MB }]
fn t(sh: Sh) -> Stream<Check> {
    let produced = exec sh`-c "echo input > carried.txt"`;
    let inputs = produced.tree;
    let consumer = exec sh`-c "cat {inputs}/carried.txt > copied.txt"`;
    let out = consumer.tree;

    // The consumer wrote exactly one file, and that is all its tree holds.
    let listed = exec sh`-c "ls -A {out} | tr '\n' ' '"`;
    yield expect_eq(listed.stdout.text().trim(), "copied.txt");
}
"#,
    );
}

#[test]
fn a_chain_of_mounts_does_not_nest_its_inputs() {
    // The compounding half of the same property: three stages deep, the last
    // tree still holds only what the last process wrote.
    expect_pass(
        "no-nesting",
        r#"
#[test { budget_wall: 60s, budget_rss: 2048MB }]
fn t(sh: Sh) -> Stream<Check> {
    let one = exec sh`-c "echo a > a.txt"`;
    let t1 = one.tree;
    let two = exec sh`-c "cat {t1}/a.txt > b.txt"`;
    let t2 = two.tree;
    let three = exec sh`-c "cat {t2}/b.txt > c.txt"`;
    let t3 = three.tree;

    let listed = exec sh`-c "ls -A {t3} | tr '\n' ' '"`;
    yield expect_eq(listed.stdout.text().trim(), "c.txt");
}
"#,
    );
}

#[test]
fn an_empty_mounted_tree_still_creates_its_directory() {
    // The argv names the mount path unconditionally, so the directory has to
    // exist even when the tree holds no files. Creating it only as a parent of
    // written files hands the process an ENOENT where the model promises a
    // directory — the shape an exec that wrote nothing produces.
    expect_pass(
        "empty-mount",
        r#"
#[test { budget_wall: 60s, budget_rss: 2048MB }]
fn t(sh: Sh) -> Stream<Check> {
    let nothing = exec sh`-c "true"`;
    let empty = nothing.tree;
    let probed = exec sh`-c "test -d {empty} && echo dir || echo missing"`;
    yield expect_eq(probed.stdout.text().trim(), "dir");
}
"#,
    );
}

#[test]
fn one_tree_spliced_twice_mounts_once() {
    // `{t} … {t}` names one input twice, not two inputs. Both renders must
    // resolve to the SAME materialized path, or a build walk that mentions its
    // source tree once per unit pays for a fresh copy each time.
    expect_pass(
        "dedup",
        r#"
#[test { budget_wall: 60s, budget_rss: 2048MB }]
fn t(sh: Sh) -> Stream<Check> {
    let produced = exec sh`-c "echo once > note.txt"`;
    let tree = produced.tree;
    let seen = exec sh`-c "cat {tree}/note.txt {tree}/note.txt; ls -A .vix-mounts | tr '\n' ' '"`;
    yield expect_eq(seen.stdout.lines()[0], "once");
    yield expect_eq(seen.stdout.lines()[1], "once");
    // One mount directory, not two.
    yield expect_eq(seen.stdout.lines()[2].trim(), "0");
}
"#,
    );
}

#[test]
fn writing_the_reserved_mount_name_without_mounting_is_refused() {
    // The reservation cuts both ways. Capture skips `.vix-mounts` so a mount
    // never comes back as output — but an invocation that mounted NOTHING and
    // wrote a top-level `.vix-mounts` is naming the reservation itself, and the
    // skip cannot tell that output from an input area. Silently dropping it
    // would give two different runs the same output identity. Refuse by name.
    let source = r#"
#[test { budget_wall: 60s, budget_rss: 2048MB }]
fn t(sh: Sh) -> Stream<Check> {
    let out = exec sh`-c "mkdir -p .vix-mounts; echo mine > .vix-mounts/note.txt"`;
    yield expect_eq(out.stdout.text(), "unreachable");
}
"#;
    let error = run(source).expect_err("the reserved name is refused");
    let rendered = format!("{error:?}");
    assert!(
        rendered.contains("reserved for exec input mounts"),
        "the refusal names the reservation: {rendered}"
    );
}

#[test]
fn a_progressive_product_may_not_be_announced_from_the_mount_area() {
    // The mount area is INPUT. Announcing a product from it would replay an
    // input as an output, and capture excludes that area — so the two halves of
    // the protocol would disagree about the same bytes.
    let source = r#"
#[test { budget_wall: 60s, budget_rss: 2048MB }]
fn t(sh: ProgressiveSh) -> Stream<Check> {
    let produced = exec sh`-c "echo seed > seed.txt"`;
    let inputs = produced.tree;
    let sneaky = exec sh`-c "printf 'vix-ready\t.vix-mounts/0/seed.txt\n'; echo done"`;
    let out = sneaky.tree;
    yield expect_eq((out / "seed.txt").text(), "unreachable");
}
"#;
    let error = run(source).expect_err("announcing from the mount area is refused");
    let rendered = format!("{error:?}");
    assert!(
        rendered.contains("reserved input-mount area"),
        "the refusal names the area: {rendered}"
    );
}

#[test]
fn a_mount_materializes_every_tree_entry_kind() {
    // Directories and symlinks participate in tree identity: an EMPTY directory
    // is explicitly representable (it is how one process hands structure to a
    // later one) and a symlink's target is preserved verbatim. A mount that
    // flattened to files alone would materialize something that is not the
    // value the request named.
    //
    // Driven through the backend directly rather than through a vix program,
    // because the CAPTURE half cannot yet produce such a tree: `archive_directory`
    // writes only file entries (so an empty directory is lost) and refuses
    // symlinks outright. Mounting is correct for all three kinds now; capture is
    // the remaining half, and testing through an exec-produced tree would test
    // that limitation instead of this fix.
    use std::sync::Arc;
    use vix::runtime::{
        ExecBackend, ExecEvent, ExecInvocation, ExecMount, ExecMountEntry, ExecOutputProtocol,
    };
    use vixen_runtime::host_exec::HostExecBackend;

    let invocation = ExecInvocation {
        program: "sh".to_owned(),
        argv: vec!["-c".to_owned(), "true".to_owned()],
        env_remove: Vec::new(),
        env: Vec::new(),
        protocol: ExecOutputProtocol::ExitOnly,
        mounts: vec![ExecMount {
            path: "m".to_owned(),
            entries: vec![
                ExecMountEntry::Dir {
                    path: "empty".to_owned(),
                },
                ExecMountEntry::File {
                    path: "full/note.txt".to_owned(),
                    bytes: b"hi\n".to_vec(),
                    executable: false,
                },
                ExecMountEntry::Symlink {
                    path: "link".to_owned(),
                    target: "full/note.txt".to_owned(),
                },
            ],
        }],
    };

    let events: vix::runtime::ExecEventSender = Arc::new(|_event: ExecEvent| {});
    let workspace = HostExecBackend
        .begin(invocation, events)
        .expect("the backend materializes and spawns");
    let mount = workspace.path().join("m");

    assert!(
        mount.join("empty").is_dir(),
        "the empty directory is created, not inferred from files under it"
    );
    assert_eq!(
        std::fs::read_to_string(mount.join("full/note.txt")).expect("the file is written"),
        "hi\n"
    );
    let link = mount.join("link");
    assert!(
        std::fs::symlink_metadata(&link)
            .expect("the symlink exists")
            .file_type()
            .is_symlink(),
        "the symlink is a symlink, not a copy of its target"
    );
    assert_eq!(
        std::fs::read_link(&link).expect("the target is preserved"),
        std::path::Path::new("full/note.txt")
    );
}
