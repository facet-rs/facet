use vixen_runtime::ratchet::{RunError, prepare_source};
use vix::runtime::{
    PersistentClaimRejectionReason, PersistentRuntimeJournalError, StoreJournalError,
};

const UNCHANGED_TREE_READ: &str = r#"
fn manifest_name(tree: Tree) -> String {
    let m: Manifest = toml_decode((tree / "Cargo.toml").text());
    m.package.name
}
struct Package { name: String, version: String }
struct Manifest { package: Package }

#[test]
fn unchanged_tree_read() -> Stream<Check> {
    yield expect_eq(manifest_name(fixture_tree("small-crate")), "small-crate");
}
"#;

const CHANGED_WITNESSED_VALUE: &str = r#"
fn line_count(tree: Tree) -> Int {
    (tree / "data.txt").text().lines().len()
}
fn render(n: Int) -> String { "lines: " + n.to_string() }

#[test { rerun_with: "touched-fixture" }]
fn changed_witnessed_value() -> Stream<Check> {
    yield expect_eq(render(line_count(fixture_tree("touched-fixture"))), "lines: 3");
}
"#;

const PATH_APPEARS: &str = r#"
fn normalized(n: Int) -> String {
    if n == 0 { "ok" } else { "ok" }
}

#[test { rerun_with: "path-appears" }]
fn path_appears() -> Stream<Check> {
    let sources = fixture_tree("path-appears").glob("src/*.rs").collect().values().sorted();
    yield expect_eq(normalized(sources.len()), "ok");
}
"#;

const UNRELATED_PROJECTION_CHANGE: &str = r#"
fn build_step(tree: Tree) -> String {
    (tree / "src" / "main.c").text()
}

#[test { rerun_with: "readme-changed" }]
fn unrelated_projection_change() -> Stream<Check> {
    yield expect(build_step(fixture_tree("readme-changed")).contains("int main"));
}
"#;

const NONDETERMINISTIC_EXEC: &str = r#"
#[test]
fn nondeterministic_exec(sh: Sh) -> Stream<Check> {
    let out = exec sh`-c "date +%N"`;
    yield expect_eq(out.stdout.collect().len(), 1);
}
"#;

#[test]
fn persistent_journal_unchanged_world_reuses_without_recompute() {
    let report = prepare_source(UNCHANGED_TREE_READ)
        .expect("source prepares")
        .execute_persistence_audit()
        .expect("persistence audit executes");
    assert!(report.second.checks.iter().all(|check| check.passed));
    assert!(report.journal_bytes > 0, "{report:#?}");
    assert!(report.load.store.values_loaded > 0, "{report:#?}");
    assert!(
        report.load.claims_loaded > 0,
        "at least one receipt-backed claim loads: {report:#?}",
    );
    assert_eq!(
        report.second.counters.document_parse_host_calls, 0,
        "unchanged world uses loaded decode claims instead of reparsing documents: {report:#?}",
    );
    assert!(!report.nondeterministic, "{report:#?}");
}

#[test]
fn persistent_journal_changed_witnessed_value_invalidates() {
    let report = prepare_source(CHANGED_WITNESSED_VALUE)
        .expect("source prepares")
        .execute_persistence_audit()
        .expect("persistence audit executes");
    assert!(report.second.checks.iter().all(|check| check.passed));
    assert!(
        report.load.claims_rejected > 0,
        "changed witnessed file rejects loaded claims: {report:#?}",
    );
    assert!(
        report.second.counters.primitive_invocations > 0,
        "changed witnessed file recomputes the tree-read primitive: {report:#?}",
    );
}

#[test]
fn persistent_journal_new_path_invalidates_directory_witness() {
    let report = prepare_source(PATH_APPEARS)
        .expect("source prepares")
        .execute_persistence_audit()
        .expect("persistence audit executes");
    assert!(report.second.checks.iter().all(|check| check.passed));
    assert!(
        report
            .load
            .rejected_claims
            .iter()
            .any(|claim| claim.reason == PersistentClaimRejectionReason::UnverifiableReceipt),
        "appearing path changes the directory receipt witness: {report:#?}",
    );
    assert!(
        report.second.counters.memo_misses > 0,
        "invalidated directory claim becomes a miss/recompute: {report:#?}",
    );
}

#[test]
fn persistent_journal_unrelated_projection_change_retains_reuse() {
    let report = prepare_source(UNRELATED_PROJECTION_CHANGE)
        .expect("source prepares")
        .execute_persistence_audit()
        .expect("persistence audit executes");
    assert!(report.second.checks.iter().all(|check| check.passed));
    assert!(
        report.load.claims_loaded > 0,
        "unchanged witnessed projection loads reusable claims: {report:#?}",
    );
    assert_eq!(
        report.load.claims_seen, report.load.claims_loaded,
        "receipt-backed pure and witnessed claims all load: {report:#?}",
    );
    assert!(
        report.load.rejected_claims.is_empty(),
        "unchanged claims should not be rejected after journal revalidation: {report:#?}",
    );
    assert_eq!(
        report.second.counters.primitive_invocations, 0,
        "unrelated projection change reuses the exact receipt-backed tree read: {report:#?}",
    );
    assert!(
        report.second.counters.memo_hits_exact > 0,
        "unchanged pure claim is reused through the durable journal: {report:#?}",
    );
    assert!(!report.nondeterministic, "{report:#?}");
}

#[test]
fn persistent_journal_corrupted_value_is_rejected() {
    let error = prepare_source(UNCHANGED_TREE_READ)
        .expect("source prepares")
        .execute_persistence_audit_with_journal_mutator(|mut journal| {
            let value = journal
                .store
                .values
                .first_mut()
                .expect("audit stores at least one immutable value");
            value.resident.push(0);
            journal
        })
        .expect_err("corrupt resident bytes are rejected before execution");
    match error {
        RunError::PersistentRuntime(error) => match *error {
            PersistentRuntimeJournalError::Store(store) => match *store {
                StoreJournalError::CorruptValue(_) => {}
                other => panic!("expected typed corrupt-value error, got {other:#?}"),
            },
            other => panic!("expected typed corrupt-value error, got {other:#?}"),
        },
        other => panic!("expected typed corrupt-value error, got {other:#?}"),
    }
}

#[test]
fn persistent_journal_nondeterministic_claims_do_not_load_as_hits() {
    let report = prepare_source(NONDETERMINISTIC_EXEC)
        .expect("source prepares")
        .execute_persistence_audit()
        .expect("persistence audit executes");
    assert!(report.second.checks.iter().all(|check| check.passed));
    assert!(
        report
            .load
            .rejected_claims
            .iter()
            .any(|claim| claim.reason == PersistentClaimRejectionReason::UnverifiableReceipt),
        "exec claim is not trusted after load: {report:#?}",
    );
    assert!(
        report.second.counters.effect_spawns > 0,
        "untrusted exec claim recomputes instead of hitting memo: {report:#?}",
    );
    assert!(
        report.nondeterministic,
        "recomputed nondeterministic exec is detected: {report:#?}",
    );
}
