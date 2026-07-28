//! The machine manifest's acceptance tests
//! (`vix-core/docs/content/spec/vixen/machine.md`, "Acceptance").
//!
//! Every refusal test pins the same three facts: the failure is TYPED (a
//! `CapabilityRefusal` naming both sides — what the program requires, what
//! the machine offers), it is PRE-EFFECT (zero spawns, by counter; no check
//! ran at all), and it agrees across the plain and chaos lanes. Every passing
//! test runs a real process through a fake tool, because the point of a
//! manifest that can refuse is that the same program RUNS when the machine's
//! word covers it.

#![cfg(unix)]

use vixen_primitives::capability_package::Target;
use vixen_runtime::manifest::{
    CapabilityOffer, MachineManifest, TargetRequirement, host_target, static_requirements,
};
use vixen_runtime::ratchet::{RatchetReport, run_source_with_manifest};

/// A manifest offering `capabilities` on an `x86_64-unknown-linux-gnu`-style
/// host — the design note's "Linux-only machine", spelled with the actual
/// host triple so the fact-shaped (host-demanding) tests are runnable
/// anywhere the suite runs.
fn manifest(capabilities: Vec<CapabilityOffer>) -> MachineManifest {
    MachineManifest {
        host: host_target(),
        capabilities,
    }
}

fn offer(ty: &str, program: &str, targets: &[&str]) -> CapabilityOffer {
    CapabilityOffer {
        ty: ty.to_owned(),
        program: program.to_owned(),
        toolchain: None,
        targets: targets.iter().copied().map(Target::new).collect(),
    }
}

/// Write one executable fake tool into `dir` and return its absolute path.
/// The scripts stand in for rustc/go/gcc: the acceptance criteria are about
/// the binding check, not about real toolchains.
fn fake_tool(dir: &tempfile::TempDir, name: &str, script: &str) -> String {
    use std::os::unix::fs::PermissionsExt as _;
    let path = dir.path().join(name);
    std::fs::write(&path, format!("#!/bin/sh\n{script}\n")).expect("write fake tool");
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))
        .expect("mark fake tool executable");
    path.to_str().expect("tool path is UTF-8").to_owned()
}

/// Assert the report is a pre-effect refusal: nothing ran, both lanes agree,
/// and exactly one typed refusal exists per lane.
fn assert_refused(report: &RatchetReport) -> &vixen_runtime::manifest::CapabilityRefusal {
    assert!(!report.passed(), "a refused program cannot pass");
    assert!(
        report.agrees(),
        "plain and chaos agree on the refusal: {report:#?}"
    );
    for lane in [&report.plain, &report.chaos] {
        assert_eq!(
            lane.counters.effect_spawns, 0,
            "no process spawned before the refusal"
        );
        assert!(
            lane.checks.is_empty() && lane.values.is_empty(),
            "no island of the refused test was submitted"
        );
        assert_eq!(lane.refusals.len(), 1, "one refusal, typed: {lane:#?}");
    }
    &report.plain.refusals[0]
}

/// The exe-case program from the design note, adapted to the surface that
/// exists (`.text()` instead of `| exists`): the requirement IS the
/// invocation — nothing beside the code spells a target.
const EXE_CASE: &str = r#"
#[test]
fn build(rustc: Rustc) -> Stream<Check> {
    let out = exec rustc`--target x86_64-pc-windows-msvc main.rs`;
    yield expect_eq((out.tree / "main.exe").text(), "exe");
}
"#;

/// Acceptance 1: **missing type.** A root requiring a capability type the
/// manifest lacks fails typed, pre-effect, naming both sides.
///
/// r[verify vixen.machine.manifest]
/// r[verify vixen.machine.binding-fails-before-effects]
#[test]
fn a_capability_type_absent_from_the_manifest_refuses_before_any_effect() {
    // The harness default offers Echo/Sh/ProgressiveSh — no Rustc.
    let report = run_source_with_manifest(EXE_CASE, MachineManifest::ratchet_default())
        .expect("a binding refusal is a report verdict, never a runner error");
    let refusal = assert_refused(&report);
    assert_eq!(refusal.test, "build");
    assert_eq!(refusal.parameter, "rustc");
    assert_eq!(refusal.required_type, "Rustc");
    assert_eq!(
        refusal.offered, None,
        "the type is absent from the manifest entirely"
    );
    let diagnostic = refusal.to_string();
    assert!(
        diagnostic.contains("`build` demands Rustc") && diagnostic.contains("no effect was started"),
        "the diagnostic names the demanding side: {diagnostic}"
    );
}

/// Acceptance 2, refusal half: **the exe case** on a Linux-only manifest.
/// The type is present; the extracted `--target` capture is not among the
/// offered targets. Typed refusal before any process exists, and the
/// diagnostic names both sides in `Target` vocabulary.
///
/// r[verify vixen.machine.binding-fails-before-effects]
/// r[verify vixen.machine.requirements-from-use]
#[test]
fn the_exe_case_refuses_pre_effect_on_a_linux_only_manifest() {
    let linux_only = manifest(vec![offer(
        "Rustc",
        "rustc-must-never-spawn",
        &["x86_64-unknown-linux-gnu"],
    )]);
    let report = run_source_with_manifest(EXE_CASE, linux_only)
        .expect("a binding refusal is a report verdict, never a runner error");
    let refusal = assert_refused(&report);
    assert_eq!(refusal.required_type, "Rustc");
    assert_eq!(
        refusal.required_target.as_deref(),
        Some("x86_64-pc-windows-msvc"),
        "the requirement was extracted from the invocation, normalized to a Target"
    );
    let offered = refusal.offered.as_deref().expect("the offer side is named");
    assert!(
        offered.contains("x86_64-unknown-linux-gnu"),
        "the diagnostic names what the machine offers: {offered}"
    );
}

/// Acceptance 2, passing half: the same program, a manifest whose `Rustc`
/// offers the target — it runs, through a fake rustc that produces the
/// artifact the program checks.
///
/// r[verify vixen.machine.manifest]
/// r[verify vixen.machine.requirements-from-use]
#[test]
fn the_exe_case_runs_when_the_manifest_offers_the_target() {
    let tools = tempfile::tempdir().expect("tool dir");
    let rustc = fake_tool(&tools, "rustc", "printf exe > main.exe");
    let offering = manifest(vec![offer(
        "Rustc",
        &rustc,
        &["x86_64-unknown-linux-gnu", "x86_64-pc-windows-msvc"],
    )]);
    let report = run_source_with_manifest(EXE_CASE, offering).expect("the exe case runs");
    assert!(report.passed(), "the offered target runs: {report:#?}");
    for lane in [&report.plain, &report.chaos] {
        assert_eq!(lane.counters.effect_spawns, 1, "one process, really spawned");
        assert!(lane.refusals.is_empty());
    }
}

/// The go-shaped program: the target rides declared environment roles, not a
/// flag. Same extraction, same vocabulary, same refusal — proving the
/// mechanism is not flag-shaped.
const ENV_CASE_CROSS: &str = r#"
#[test]
fn build(go: Go) -> Stream<Check> {
    let out = exec go`GOOS=windows GOARCH=amd64 build`;
    yield expect_eq((out.tree / "target.txt").text(), "windows/amd64");
}
"#;

/// Acceptance 3, refusal half: **the env-role case.** `GOOS=windows
/// GOARCH=amd64` normalizes to `x86_64-pc-windows-gnu` through the package's
/// grammar; a Linux-only `Go` offer refuses pre-effect.
///
/// r[verify vixen.machine.requirements-from-use]
/// r[verify vixen.machine.binding-fails-before-effects]
#[test]
fn an_env_role_target_refuses_pre_effect_on_a_linux_only_manifest() {
    let linux_only = manifest(vec![offer(
        "Go",
        "go-must-never-spawn",
        &["x86_64-unknown-linux-gnu"],
    )]);
    let report = run_source_with_manifest(ENV_CASE_CROSS, linux_only)
        .expect("a binding refusal is a report verdict, never a runner error");
    let refusal = assert_refused(&report);
    assert_eq!(refusal.required_type, "Go");
    assert_eq!(
        refusal.required_target.as_deref(),
        Some("x86_64-pc-windows-gnu"),
        "the env roles normalized into Target vocabulary — never GOOS/GOARCH strings"
    );
}

/// Acceptance 3, passing half: the same shape with host-satisfiable roles
/// runs — and the capture is REAL: the fake tool reads `$GOOS`/`$GOARCH`
/// from its actual process environment and writes them into the tree the
/// program checks, so the declared env roles demonstrably reached the
/// spawned process.
///
/// r[verify vixen.machine.requirements-from-use]
#[test]
fn an_env_role_target_runs_and_the_roles_reach_the_process_environment() {
    const ENV_CASE_HOST: &str = r#"
#[test]
fn build(go: Go) -> Stream<Check> {
    let out = exec go`GOOS=linux GOARCH=amd64 build`;
    yield expect_eq((out.tree / "target.txt").text(), "linux/amd64");
}
"#;
    let tools = tempfile::tempdir().expect("tool dir");
    let go = fake_tool(&tools, "go", r#"printf '%s/%s' "$GOOS" "$GOARCH" > target.txt"#);
    let offering = manifest(vec![offer("Go", &go, &["x86_64-unknown-linux-gnu"])]);
    let report = run_source_with_manifest(ENV_CASE_HOST, offering).expect("the env case runs");
    assert!(
        report.passed(),
        "the process saw GOOS/GOARCH in its environment: {report:#?}"
    );
    for lane in [&report.plain, &report.chaos] {
        assert_eq!(lane.counters.effect_spawns, 1);
    }
}

/// Acceptance 4: **neutral tools run anywhere.** The argv contains a literal
/// `--target` element and a triple — but `Sh`'s grammar declares no target
/// role, so no requirement is imposed and the program runs on a manifest
/// offering no targets at all. Roles come from the grammar, never from
/// string-sniffing the plan (`machine.capability.no-argv-dialect`).
///
/// r[verify vixen.machine.requirements-from-use]
#[test]
fn a_target_neutral_invocation_imposes_no_target_requirement() {
    const NEUTRAL: &str = r#"
#[test]
fn neutral(sh: Sh) -> Stream<Check> {
    let out = exec sh`-c "printf done" ignored --target x86_64-pc-windows-msvc`;
    yield expect_eq(out.stdout.collect().values(), ["done"]);
}
"#;
    let report = run_source_with_manifest(NEUTRAL, MachineManifest::ratchet_default())
        .expect("a neutral invocation runs anywhere its tool exists");
    assert!(report.passed(), "no target requirement was invented: {report:#?}");
    for lane in [&report.plain, &report.chaos] {
        assert_eq!(lane.counters.effect_spawns, 1);
        assert!(lane.refusals.is_empty());
    }
}

/// Acceptance 5: **the static report.** The requirement set — capability
/// parameter types plus literal role captures — is readable off the compiled
/// module without executing anything, in exactly the "needs `Rustc`
/// producing `x86_64-pc-windows-msvc`" shape.
///
/// r[verify vixen.machine.requirements-are-static]
#[test]
fn the_requirement_set_is_reported_without_executing() {
    let module = vixen_runtime::default_compiler()
        .compile(EXE_CASE)
        .expect("the exe case compiles")
        .module;
    let requirements = static_requirements(&module).expect("the report needs no execution");
    let [test] = requirements.as_slice() else {
        panic!("one test, one requirement set: {requirements:#?}");
    };
    assert_eq!(test.test, "build");
    let [rustc] = test.capabilities.as_slice() else {
        panic!("one capability requirement: {test:#?}");
    };
    assert_eq!(rustc.parameter, "rustc");
    assert_eq!(rustc.ty, "Rustc");
    assert_eq!(
        rustc.targets,
        vec![TargetRequirement::Literal(Target::new("x86_64-pc-windows-msvc"))],
        "needs Rustc producing x86_64-pc-windows-msvc, statically"
    );
}

/// The static report's honest degradation: a target spliced from a computed
/// value is reported as "decided at run time", never silently dropped and
/// never guessed.
///
/// r[verify vixen.machine.requirements-are-static]
#[test]
fn a_computed_capture_degrades_honestly_in_the_static_report() {
    const COMPUTED: &str = r#"
fn os(windows: Bool) -> String {
    if windows { "pc-windows-msvc" } else { "unknown-linux-gnu" }
}
#[test]
fn build(rustc: Rustc) -> Stream<Check> {
    let os = os(1 == 1);
    let out = exec rustc`--target x86_64-{os} main.rs`;
    yield expect_eq((out.tree / "main.exe").text(), "exe");
}
"#;
    let module = vixen_runtime::default_compiler()
        .compile(COMPUTED)
        .expect("the computed-capture case compiles")
        .module;
    let requirements = static_requirements(&module).expect("the report needs no execution");
    let rustc = requirements[0]
        .capabilities
        .iter()
        .find(|capability| capability.ty == "Rustc")
        .expect("the Rustc requirement row exists");
    assert_eq!(
        rustc.targets,
        vec![TargetRequirement::Computed],
        "a computed capture is 'target decided at run time': {requirements:#?}"
    );
}

/// The fact-shaped end of the generality table: `MingwGcc` spells no target
/// anywhere in an invocation — the capability's own target facts are the
/// claim, checked against the plan's implicit demand for the host. Both
/// ways: an offer whose facts include the host runs; a cross-target offer
/// refuses pre-effect.
///
/// r[verify vixen.machine.requirements-from-use]
/// r[verify vixen.machine.facts-are-fields]
#[test]
fn a_fact_shaped_capability_is_checked_against_its_own_target_facts() {
    const FACT_CASE: &str = r#"
#[test]
fn compile(gcc: MingwGcc) -> Stream<Check> {
    let out = exec gcc`main.c`;
    yield expect_eq((out.tree / "a.out").text(), "obj");
}
"#;
    // A per-target gcc whose facts claim only a foreign target: the plan
    // implicitly requires the host, so binding refuses before any effect.
    let cross = manifest(vec![offer(
        "MingwGcc",
        "gcc-must-never-spawn",
        &["x86_64-pc-windows-gnu"],
    )]);
    let report = run_source_with_manifest(FACT_CASE, cross)
        .expect("a binding refusal is a report verdict, never a runner error");
    let refusal = assert_refused(&report);
    assert_eq!(refusal.required_type, "MingwGcc");
    assert_eq!(
        refusal.required_target.as_deref(),
        Some(host_target().as_str()),
        "no capture exists; the requirement is the machine's host"
    );

    // The same program under an offer whose facts include the host: runs.
    let tools = tempfile::tempdir().expect("tool dir");
    let gcc = fake_tool(&tools, "gcc", "printf obj > a.out");
    let native = manifest(vec![offer("MingwGcc", &gcc, &[host_target().as_str()])]);
    let report = run_source_with_manifest(FACT_CASE, native).expect("the native gcc runs");
    assert!(report.passed(), "host-fact facts admit the host: {report:#?}");
    for lane in [&report.plain, &report.chaos] {
        assert_eq!(lane.counters.effect_spawns, 1);
    }
}

/// The embedder-loads-config half of `vixen.machine.manifest`: the TOML
/// spelling round-trips into the same typed value the tests construct
/// directly.
///
/// r[verify vixen.machine.manifest]
#[test]
fn the_manifest_loads_from_its_toml_config_spelling() {
    let manifest = MachineManifest::from_toml(
        r#"
host = "x86_64-unknown-linux-gnu"

[[capability]]
ty = "Rustc"
program = "/toolchains/1.89.0/bin/rustc"
toolchain = "1.89.0"
targets = ["x86_64-unknown-linux-gnu"]

[[capability]]
ty = "Sh"
program = "/bin/sh"
"#,
    )
    .expect("the config spelling loads");
    assert_eq!(manifest.host, Target::new("x86_64-unknown-linux-gnu"));
    let rustc = manifest.offer("Rustc").expect("Rustc is offered");
    assert_eq!(rustc.program, "/toolchains/1.89.0/bin/rustc");
    assert_eq!(rustc.toolchain.as_deref(), Some("1.89.0"));
    assert_eq!(rustc.targets, vec![Target::new("x86_64-unknown-linux-gnu")]);
    let sh = manifest.offer("Sh").expect("Sh is offered");
    assert!(sh.targets.is_empty() && sh.toolchain.is_none());
}
