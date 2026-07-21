//! Cross-lane differential certificate (audit finding F1).
//!
//! A production-shaped, in-process differential over the accepted canonical rung
//! corpus (through rung 065). On a native-capable host each accepted rung is
//! compiled and lowered once, then executed through an explicitly selected
//! *native* [`weavy::exec::Executable`] and an explicitly selected *interpreter*
//! [`weavy::exec::Executable`] — the same compiled/lowered semantics driven
//! through two separately-authored op authorities — and their full
//! provenance-keyed check/failure family, value identities, and per-check
//! attribution are asserted identical across *both* the plain and chaos suites.
//!
//! Lane selection is a typed per-run request ([`LaneRequest`], threaded through
//! the lowering cache into each `Executable`), never the global `WEAVY_JIT`
//! environment toggle. That is what makes this certificate safe to run under
//! nextest's parallel process model: two lanes are materialized in one process
//! without any shared mutable process state racing sibling tests.
//!
//! ## Semantic comparison boundary
//!
//! The differential asserts equality of the semantic *outputs* of a run:
//!
//! * [`SuiteRun::check_family`] — the provenance-keyed completed check family,
//!   which carries each check's evaluated value identity, ordinary demand
//!   argument identities, pass/fail verdict, failure value, and failure-context
//!   attribution (a full [`vixen_runtime::ratchet::CheckRun`] compared by value, not a
//!   boolean);
//! * [`SuiteRun::value_family`] — the provenance-keyed published value identities
//!   and their failure payloads;
//! * the run's `all_demands_ready` / `all_tasks_terminal` completion facts;
//! * the compile-time `warnings`.
//!
//! Both the plain lane and the chaos lane are compared, so the certificate
//! preserves the existing plain/chaos guarantee while adding the cross-lane one.
//!
//! Lane-attribution telemetry is *excluded* from the semantic equality, by
//! construction: `Counters::native_task_spawns` and
//! `Counters::interpreter_task_spawns` are *defined* to differ by lane (they are
//! how each lane records which authority ran a task), and the lowering-cache
//! miss/hit counts are a non-semantic compilation artifact. Every *other* counter
//! is lane-invariant and is compared after normalizing only those two spawn
//! fields — a strengthening beyond the family equality. Those same two spawn
//! counters are additionally used, positively, to *prove* each lane genuinely
//! executed on its selected authority (lane purity), so a silent native→
//! interpreter fallback cannot manufacture a false-green interpreter-vs-
//! interpreter comparison.
//!
//! On a non-native target the native lane is unavailable, so there is no second
//! authority to differentiate against; the sweep is skipped through the same
//! [`weavy::jit::task_lane::available`] capability rule the other in-tree
//! cross-lane tests use (e.g. `tail_loop_interpreter_and_jit_agree`).

use vixen_runtime::ratchet::{
    RatchetReport, RunError, SnapshotExpectations, SuiteRun, run_source_with_snapshots_and_lane,
};
use weavy::exec::LaneRequest;

struct Rung {
    name: &'static str,
    source: &'static str,
    reject: bool,
}

const DEP_MIO_GOLDEN: &str =
    "Dep {\n    name: \"mio\",\n    req: \"^0.8\",\n    optional: false,\n}";
const GREEK_LETTERS_GOLDEN: &str = "[\n    \"alpha\",\n    \"beta\",\n    \"gamma\",\n]";

/// The accepted canonical corpus through rung 065, with reject fixtures marked.
/// Reject rungs are rejected at compile time (before any lane is selected), so
/// they certify only that both lanes report the identical rejection; accepted
/// rungs drive the full cross-lane semantic differential.
const CORPUS: &[Rung] = &[
    Rung {
        name: "001-harness",
        source: include_str!("ratchet/001-harness.vix"),
        reject: false,
    },
    Rung {
        name: "002-arithmetic",
        source: include_str!("ratchet/002-arithmetic.vix"),
        reject: false,
    },
    Rung {
        name: "003-bindings",
        source: include_str!("ratchet/003-bindings.vix"),
        reject: false,
    },
    Rung {
        name: "004-functions",
        source: include_str!("ratchet/004-functions.vix"),
        reject: false,
    },
    Rung {
        name: "005-tuples",
        source: include_str!("ratchet/005-tuples.vix"),
        reject: false,
    },
    Rung {
        name: "006-records",
        source: include_str!("ratchet/006-records.vix"),
        reject: false,
    },
    Rung {
        name: "007-enums",
        source: include_str!("ratchet/007-enums.vix"),
        reject: false,
    },
    Rung {
        name: "008-spread",
        source: include_str!("ratchet/008-spread.vix"),
        reject: false,
    },
    Rung {
        name: "009-structural-equality",
        source: include_str!("ratchet/009-structural-equality.vix"),
        reject: false,
    },
    Rung {
        name: "010-spaceship",
        source: include_str!("ratchet/010-spaceship.vix"),
        reject: false,
    },
    Rung {
        name: "011-derived-comparisons",
        source: include_str!("ratchet/011-derived-comparisons.vix"),
        reject: false,
    },
    Rung {
        name: "012-total-order",
        source: include_str!("ratchet/012-total-order.vix"),
        reject: false,
    },
    Rung {
        name: "013-expression-statement.reject",
        source: include_str!("ratchet/013-expression-statement.reject.vix"),
        reject: true,
    },
    Rung {
        name: "014-if-else",
        source: include_str!("ratchet/014-if-else.vix"),
        reject: false,
    },
    Rung {
        name: "015-boolean-operators",
        source: include_str!("ratchet/015-boolean-operators.vix"),
        reject: false,
    },
    Rung {
        name: "016-match-expressions",
        source: include_str!("ratchet/016-match-expressions.vix"),
        reject: false,
    },
    Rung {
        name: "017-match-guards",
        source: include_str!("ratchet/017-match-guards.vix"),
        reject: false,
    },
    Rung {
        name: "018-non-exhaustive.reject",
        source: include_str!("ratchet/018-non-exhaustive.reject.vix"),
        reject: true,
    },
    Rung {
        name: "019-let-destructuring",
        source: include_str!("ratchet/019-let-destructuring.vix"),
        reject: false,
    },
    Rung {
        name: "020-match-destructuring",
        source: include_str!("ratchet/020-match-destructuring.vix"),
        reject: false,
    },
    Rung {
        name: "021-closure-destructuring",
        source: include_str!("ratchet/021-closure-destructuring.vix"),
        reject: false,
    },
    Rung {
        name: "022-record-patterns",
        source: include_str!("ratchet/022-record-patterns.vix"),
        reject: false,
    },
    Rung {
        name: "023-option",
        source: include_str!("ratchet/023-option.vix"),
        reject: false,
    },
    Rung {
        name: "024-user-result",
        source: include_str!("ratchet/024-user-result.vix"),
        reject: false,
    },
    Rung {
        name: "025-ordering-enum",
        source: include_str!("ratchet/025-ordering-enum.vix"),
        reject: false,
    },
    Rung {
        name: "026-arrays",
        source: include_str!("ratchet/026-arrays.vix"),
        reject: false,
    },
    Rung {
        name: "027-array-map",
        source: include_str!("ratchet/027-array-map.vix"),
        reject: false,
    },
    Rung {
        name: "028-array-enumerate",
        source: include_str!("ratchet/028-array-enumerate.vix"),
        reject: false,
    },
    Rung {
        name: "029-array-fold",
        source: include_str!("ratchet/029-array-fold.vix"),
        reject: false,
    },
    Rung {
        name: "030-array-predicates",
        source: include_str!("ratchet/030-array-predicates.vix"),
        reject: false,
    },
    Rung {
        name: "031-split-last",
        source: include_str!("ratchet/031-split-last.vix"),
        reject: false,
    },
    Rung {
        name: "032-pop.reject",
        source: include_str!("ratchet/032-pop.reject.vix"),
        reject: true,
    },
    Rung {
        name: "033-multiset-conversion",
        source: include_str!("ratchet/033-multiset-conversion.vix"),
        reject: false,
    },
    Rung {
        name: "034-multiset-filter",
        source: include_str!("ratchet/034-multiset-filter.vix"),
        reject: false,
    },
    Rung {
        name: "035-canonical-order",
        source: include_str!("ratchet/035-canonical-order.vix"),
        reject: false,
    },
    Rung {
        name: "036-multiset-fold",
        source: include_str!("ratchet/036-multiset-fold.vix"),
        reject: false,
    },
    Rung {
        name: "037-filter-map-flat-map",
        source: include_str!("ratchet/037-filter-map-flat-map.vix"),
        reject: false,
    },
    Rung {
        name: "038-find-split-min-max",
        source: include_str!("ratchet/038-find-split-min-max.vix"),
        reject: false,
    },
    Rung {
        name: "039-indexed-roundtrip",
        source: include_str!("ratchet/039-indexed-roundtrip.vix"),
        reject: false,
    },
    Rung {
        name: "040-sorted-by",
        source: include_str!("ratchet/040-sorted-by.vix"),
        reject: false,
    },
    Rung {
        name: "041-maps",
        source: include_str!("ratchet/041-maps.vix"),
        reject: false,
    },
    Rung {
        name: "042-map-overwrite",
        source: include_str!("ratchet/042-map-overwrite.vix"),
        reject: false,
    },
    Rung {
        name: "043-map-keys-canonical",
        source: include_str!("ratchet/043-map-keys-canonical.vix"),
        reject: false,
    },
    Rung {
        name: "044-sets",
        source: include_str!("ratchet/044-sets.vix"),
        reject: false,
    },
    Rung {
        name: "045-strings",
        source: include_str!("ratchet/045-strings.vix"),
        reject: false,
    },
    Rung {
        name: "046-paths",
        source: include_str!("ratchet/046-paths.vix"),
        reject: false,
    },
    Rung {
        name: "047-string-to-path.reject",
        source: include_str!("ratchet/047-string-to-path.reject.vix"),
        reject: true,
    },
    Rung {
        name: "048-closures-capture",
        source: include_str!("ratchet/048-closures-capture.vix"),
        reject: false,
    },
    Rung {
        name: "049-recursion",
        source: include_str!("ratchet/049-recursion.vix"),
        reject: false,
    },
    Rung {
        name: "050-deep-tail-recursion",
        source: include_str!("ratchet/050-deep-tail-recursion.vix"),
        reject: false,
    },
    Rung {
        name: "051-molten-accumulator",
        source: include_str!("ratchet/051-molten-accumulator.vix"),
        reject: false,
    },
    Rung {
        name: "052-higher-order",
        source: include_str!("ratchet/052-higher-order.vix"),
        reject: false,
    },
    Rung {
        name: "053-args-are-wires",
        source: include_str!("ratchet/053-args-are-wires.vix"),
        reject: false,
    },
    Rung {
        name: "054-partial-dependency",
        source: include_str!("ratchet/054-partial-dependency.vix"),
        reject: false,
    },
    Rung {
        name: "055-match-defers",
        source: include_str!("ratchet/055-match-defers.vix"),
        reject: false,
    },
    Rung {
        name: "056-undemanded-is-free",
        source: include_str!("ratchet/056-undemanded-is-free.vix"),
        reject: false,
    },
    Rung {
        name: "057-element-independence",
        source: include_str!("ratchet/057-element-independence.vix"),
        reject: false,
    },
    Rung {
        name: "058-memo-within-run",
        source: include_str!("ratchet/058-memo-within-run.vix"),
        reject: false,
    },
    Rung {
        name: "059-distinct-args-distinct-demands",
        source: include_str!("ratchet/059-distinct-args-distinct-demands.vix"),
        reject: false,
    },
    Rung {
        name: "060-snapshots",
        source: include_str!("ratchet/060-snapshot-record.vix"),
        reject: false,
    },
    Rung {
        name: "061-snapshot-canonical",
        source: include_str!("ratchet/061-snapshot-canonical.vix"),
        reject: false,
    },
    Rung {
        name: "062-json-decode",
        source: include_str!("ratchet/062-json-decode.vix"),
        reject: false,
    },
    Rung {
        name: "063-toml-decode",
        source: include_str!("ratchet/063-toml-decode.vix"),
        reject: false,
    },
    Rung {
        name: "064-decode-optional",
        source: include_str!("ratchet/064-decode-optional.vix"),
        reject: false,
    },
    Rung {
        name: "065-decode-enum-forms",
        source: include_str!("ratchet/065-decode-enum-forms.vix"),
        reject: false,
    },
];

fn snapshot_expectations(rung: &Rung) -> SnapshotExpectations {
    match rung.name {
        "060-snapshots" => {
            SnapshotExpectations::new().with("snapshot_record", "dep-mio", DEP_MIO_GOLDEN)
        }
        "061-snapshot-canonical" => SnapshotExpectations::new().with(
            "snapshot_canonical",
            "greek-letters",
            GREEK_LETTERS_GOLDEN,
        ),
        _ => SnapshotExpectations::new(),
    }
}

/// Assert the two suites publish an identical semantic family. This is the
/// [`RatchetReport::agrees`] shape (`check_family` + `value_family`), applied
/// cross-lane instead of cross-chaos, plus the run-completion facts.
fn assert_suites_agree(rung: &str, lane_pair: &str, native: &SuiteRun, interp: &SuiteRun) {
    assert_eq!(
        native.check_family(),
        interp.check_family(),
        "{rung}: {lane_pair} check/failure family diverges",
    );
    assert_eq!(
        native.value_family(),
        interp.value_family(),
        "{rung}: {lane_pair} published value identities diverge",
    );
    assert_eq!(
        native.all_demands_ready, interp.all_demands_ready,
        "{rung}: {lane_pair} all_demands_ready diverges",
    );
    assert_eq!(
        native.all_tasks_terminal, interp.all_tasks_terminal,
        "{rung}: {lane_pair} all_tasks_terminal diverges",
    );
    assert_eq!(
        native.receipt_count, interp.receipt_count,
        "{rung}: {lane_pair} receipt_count diverges",
    );

    // Strengthening: every counter *except* the two lane-attribution spawn
    // fields is lane-invariant. Normalize only those two and compare the rest.
    let mut native_counters = native.counters;
    let mut interp_counters = interp.counters;
    for counters in [&mut native_counters, &mut interp_counters] {
        counters.native_task_spawns = 0;
        counters.interpreter_task_spawns = 0;
    }
    assert_eq!(
        native_counters, interp_counters,
        "{rung}: {lane_pair} lane-invariant counters diverge",
    );
}

/// Prove the report ran purely on its selected lane. Every scheduler task that
/// actually reaches an executable spawns into exactly one authority; the chaos
/// lane may discard a task at an edge safepoint *before* it reaches the
/// executable, so the lane-classified spawns account for `task_spawns` minus
/// `task_discards`. The opposite lane's spawn counter must be zero. This is what
/// makes a silent native→interpreter fallback observable rather than a
/// false-green interpreter/interpreter comparison.
fn assert_lane_purity(rung: &str, report: &RatchetReport, lane: LaneRequest) -> u64 {
    let mut native_spawns = 0;
    for (which, suite) in [("plain", &report.plain), ("chaos", &report.chaos)] {
        let c = &suite.counters;
        let executed = c.task_spawns - c.task_discards;
        match lane {
            LaneRequest::Native => {
                assert_eq!(
                    c.interpreter_task_spawns, 0,
                    "{rung} ({which}): native lane spawned an interpreter task",
                );
                assert_eq!(
                    c.native_task_spawns, executed,
                    "{rung} ({which}): native lane left executed task spawns unaccounted",
                );
                native_spawns += c.native_task_spawns;
            }
            LaneRequest::Interpreter => {
                assert_eq!(
                    c.native_task_spawns, 0,
                    "{rung} ({which}): interpreter lane spawned a native task",
                );
                assert_eq!(
                    c.interpreter_task_spawns, executed,
                    "{rung} ({which}): interpreter lane left executed task spawns unaccounted",
                );
            }
            LaneRequest::Auto => unreachable!("differential never uses the Auto lane"),
        }
    }
    native_spawns
}

/// The full corpus-wide cross-lane semantic differential (F1).
#[test]
fn accepted_corpus_agrees_across_native_and_interpreter_lanes() {
    // On a non-native target there is only one authority; skip through the same
    // capability rule the other in-tree cross-lane tests use.
    if !weavy::jit::task_lane::available() {
        return;
    }

    let mut total_native_spawns = 0u64;
    let mut differentiated = 0usize;
    let mut rejected = 0usize;

    for rung in CORPUS {
        let expectations = snapshot_expectations(rung);
        let native =
            run_source_with_snapshots_and_lane(rung.source, &expectations, LaneRequest::Native);
        let interp = run_source_with_snapshots_and_lane(
            rung.source,
            &expectations,
            LaneRequest::Interpreter,
        );

        if rung.reject {
            // Rejection is a compile-time verdict, decided before any lane is
            // selected: both requests must reject identically.
            let native_err = native.expect_err(&format!("{}: native lane rejects", rung.name));
            let interp_err = interp.expect_err(&format!("{}: interpreter lane rejects", rung.name));
            assert_eq!(
                format!("{native_err:?}"),
                format!("{interp_err:?}"),
                "{}: reject diagnostics diverge across lanes",
                rung.name,
            );
            rejected += 1;
            continue;
        }

        let native: RatchetReport =
            native.unwrap_or_else(|err: RunError| panic!("{} native run: {err:?}", rung.name));
        let interp: RatchetReport =
            interp.unwrap_or_else(|err: RunError| panic!("{} interp run: {err:?}", rung.name));

        // Both lanes really ran on their selected authority.
        total_native_spawns += assert_lane_purity(rung.name, &native, LaneRequest::Native);
        assert_lane_purity(rung.name, &interp, LaneRequest::Interpreter);

        // The two lanes agree, plain and chaos, on the full semantic family.
        assert_suites_agree(rung.name, "plain", &native.plain, &interp.plain);
        assert_suites_agree(rung.name, "chaos", &native.chaos, &interp.chaos);

        // Compile-time warnings are lane-independent but pinned here so a lane
        // that suppressed a warning would be caught too.
        assert_eq!(
            native.warnings, interp.warnings,
            "{}: warnings diverge across lanes",
            rung.name,
        );

        // The chaos discipline is preserved within each lane.
        assert!(
            native.agrees(),
            "{}: native plain/chaos disagree",
            rung.name
        );
        assert!(
            interp.agrees(),
            "{}: interp plain/chaos disagree",
            rung.name
        );

        differentiated += 1;
    }

    assert!(
        differentiated > 0,
        "the native-capable differential must exercise at least one accepted rung",
    );
    assert!(
        rejected > 0,
        "the corpus must certify reject semantics across lanes",
    );
    // The native lane genuinely executed real work; the differential is not a
    // vacuous interpreter-vs-interpreter comparison.
    assert!(
        total_native_spawns > 0,
        "no task ever spawned on the native lane; the native authority was never exercised",
    );
}
