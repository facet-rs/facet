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
    // Rung one of the ladder. Three processes: write the source, compile it,
    // run the binary. The braces are octal escapes because an exec template
    // reads a literal `{` as an interpolation.
    let report = expect_pass(
        "hello-world",
        r#"
#[test { budget_wall: 120s, budget_rss: 4096MB }]
fn t(sh: Sh, rustc: Rustc) -> Stream<Check> {
    let sources = exec sh`-c "mkdir -p src; printf 'fn main() \173 println!(\042hello from vix\042); \175\n' > src/main.rs"`;
    let src = sources.tree;
    let built = exec rustc`--crate-name hello --edition 2021 {src}/src/main.rs --out-dir .`;
    let out = built.tree;
    let ran = exec sh`-c "{out}/hello"`;
    yield expect_eq(ran.stdout.text().trim(), "hello from vix");
}
"#,
    );
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
