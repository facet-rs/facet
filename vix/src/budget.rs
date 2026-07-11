//! The outer, budget-enforcing test runner.
//!
//! A `#[test]` budget (`crate::vir::Budget`) is only a gate if something outside
//! the test can *stop* an over-budget run. An in-process `Instant` check cannot
//! interrupt a stuck native loop; a leaked timeout thread cannot reclaim its
//! memory; Nextest's own timeout is coarse and does not read the typed budget.
//! So enforcement lives here: the parent reads the typed budget *before*
//! execution, launches the workload in a watched child process, and terminates
//! that child the moment it exceeds the wall-clock or resident-set ceiling.
//!
//! `run_source` remains the ordinary in-process production path for value/trace
//! certificates. The canonical budget proof exercises *this* path instead: a
//! real child, a real kill, and a typed red outcome.
//!
//! IPC is a tagged Facet type over facet-json in both directions; no JSON is
//! ever hand-emitted. Platform-specific resident-set observation is confined
//! behind typed `cfg` boundaries, and a platform that cannot observe a child's
//! RSS reports a typed [`BudgetOutcome::RssEnforcementUnsupported`] seam rather
//! than silently degrading to an unenforceable assertion.

use std::io::{Read, Write};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use crate::vir::Budget;

/// The watchdog poll interval. Wall and resident-set ceilings are checked at
/// this cadence; it bounds enforcement latency, not the budgets themselves.
const POLL_INTERVAL: Duration = Duration::from_millis(5);

/// A workload the outer runner executes inside a watched child process. It is
/// the parent → child half of the IPC protocol.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum Workload {
    /// Run a Vix source through the ordinary in-process production path
    /// (`run_source`) and report whether every check passed. This is how a
    /// within-budget production certificate is proven through the outer path.
    RunSource { source: String },
    /// A runaway native loop that never yields a result. It exists to prove the
    /// wall-clock watchdog can terminate stuck native code the language cannot
    /// interrupt from the inside.
    SpinForever,
    /// Allocate and fault in `target_bytes` of resident memory, then hold it and
    /// spin. A deterministic, platform-supported resident-set fixture that
    /// proves the RSS watchdog terminates an over-memory child.
    GrowResident { target_bytes: u64 },
    /// Complete immediately. A control that exercises the spawn/report path with
    /// no budget pressure.
    Immediate,
    /// Complete after a bounded delay. This proves a child that exits between
    /// watchdog polls is still rejected when its completion exceeded the wall
    /// budget; normal exit does not erase the elapsed-time verdict.
    Delay { duration_ns: u64 },
    /// Spend `prepare_ns` in the *preparation* phase and then complete instantly.
    /// It stands in for a program whose compilation/JIT baseline is large while
    /// its execution is trivial. Under a correct readiness boundary the wall
    /// clock does not start until preparation is done, so this workload passes
    /// even under a wall budget far smaller than `prepare_ns`.
    SlowPrepare { prepare_ns: u64 },
}

/// The child → parent half of the IPC protocol: what a completed workload
/// reports. A killed workload sends nothing; its outcome is the kill itself.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ChildReport {
    /// A `RunSource` workload completed; `passed` is its ratchet verdict.
    RanSource { passed: bool },
    /// An `Immediate` workload completed.
    Completed,
    /// The workload failed to run (e.g. the source did not compile).
    Failed { message: String },
}

impl ChildReport {
    /// Whether the reported workload succeeded.
    #[must_use]
    pub fn succeeded(&self) -> bool {
        match self {
            ChildReport::RanSource { passed } => *passed,
            ChildReport::Completed => true,
            ChildReport::Failed { .. } => false,
        }
    }
}

/// The typed outcome of running a workload under an enforced budget. Anything
/// other than an in-budget successful completion is red.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum BudgetOutcome {
    /// The child completed within budget; carries its report.
    Within { report: ChildReport },
    /// The child exceeded the wall-clock ceiling and was killed.
    OverWall { budget_ns: u64, elapsed_ns: u64 },
    /// The child exceeded the resident-set ceiling and was killed.
    OverRss {
        budget_bytes: u64,
        observed_bytes: u64,
    },
    /// The child could not be spawned, exited abnormally, or its report could
    /// not be decoded.
    ChildError { detail: String },
    /// A resident-set budget was requested on a platform whose child RSS this
    /// runner cannot soundly observe. Reported as a typed seam rather than a
    /// silently unenforced budget.
    RssEnforcementUnsupported { platform: String },
    /// The source was rejected before a child was spawned, so no declared
    /// budget could be trusted as the enforcement authority.
    SourceRejected { detail: String },
    /// The outer source runner requires one test because `run_source` executes
    /// a whole module while each test owns its own budget.
    BudgetTestCardinality { count: u64 },
    /// The one test has no wall or RSS ceiling, so calling this an enforced run
    /// would be false.
    BudgetNotDeclared { test: String },
}

impl BudgetOutcome {
    /// Whether the run passed: an in-budget successful completion. Every budget
    /// breach, child error, and unsupported-platform seam is red.
    #[must_use]
    pub fn passed(&self) -> bool {
        matches!(self, BudgetOutcome::Within { report } if report.succeeded())
    }
}

/// Whether this platform can soundly observe a child process's resident-set
/// size. Decided at compile time so an unsupported platform is a typed seam,
/// never a transient runtime `None`.
#[cfg(any(target_os = "macos", target_os = "linux"))]
const RSS_ENFORCEABLE: bool = true;
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
const RSS_ENFORCEABLE: bool = false;

/// Compile `source` in the parent and enforce the one test's declared budget.
/// The source metadata is the authority: callers cannot substitute a looser
/// [`Budget`] than the `#[test { ... }]` declaration before spawning the child.
#[must_use]
pub fn run_source_under_declared_budget(child_exe: &Path, source: &str) -> BudgetOutcome {
    let compilation = match crate::compiler::Compiler::new().compile(source) {
        Ok(compilation) => compilation,
        Err(diagnostics) => {
            return BudgetOutcome::SourceRejected {
                detail: format!("{diagnostics:?}"),
            };
        }
    };
    let [test] = compilation.module.tests.as_slice() else {
        return BudgetOutcome::BudgetTestCardinality {
            count: u64::try_from(compilation.module.tests.len()).unwrap_or(u64::MAX),
        };
    };
    if !test.metadata.budget.is_present() {
        return BudgetOutcome::BudgetNotDeclared {
            test: test.name.clone(),
        };
    }
    run_under_budget(
        child_exe,
        &test.metadata.budget,
        &Workload::RunSource {
            source: source.to_owned(),
        },
    )
}

/// Run `workload` in a child process launched from `child_exe`, enforcing
/// `budget`. The typed budget is read *before* execution; the child is killed
/// the instant it exceeds a declared wall or resident-set ceiling.
#[must_use]
pub fn run_under_budget(child_exe: &Path, budget: &Budget, workload: &Workload) -> BudgetOutcome {
    if budget.rss_bytes.is_some() && !RSS_ENFORCEABLE {
        return BudgetOutcome::RssEnforcementUnsupported {
            platform: std::env::consts::OS.to_owned(),
        };
    }

    let request = match facet_json::to_string(workload) {
        Ok(request) => request,
        Err(error) => {
            return BudgetOutcome::ChildError {
                detail: format!("encoding workload request: {error}"),
            };
        }
    };

    let mut child = match Command::new(child_exe)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(error) => {
            return BudgetOutcome::ChildError {
                detail: format!("spawning {}: {error}", child_exe.display()),
            };
        }
    };

    // Hand the child its request and close stdin so the child reaches EOF and
    // begins execution.
    if let Some(mut stdin) = child.stdin.take()
        && let Err(error) = stdin.write_all(request.as_bytes())
    {
        let _ = child.kill();
        let _ = child.wait();
        return BudgetOutcome::ChildError {
            detail: format!("writing workload request: {error}"),
        };
    }

    let pid = child.id();
    let start = Instant::now();
    loop {
        let elapsed = start.elapsed();
        match child.try_wait() {
            Ok(Some(status)) => {
                if let Some(wall) = budget.wall()
                    && elapsed > wall
                {
                    return BudgetOutcome::OverWall {
                        budget_ns: saturating_nanos(wall),
                        elapsed_ns: saturating_nanos(elapsed),
                    };
                }
                return finished(&mut child, status);
            }
            Ok(None) => {}
            Err(error) => {
                let _ = child.kill();
                let _ = child.wait();
                return BudgetOutcome::ChildError {
                    detail: format!("waiting on child: {error}"),
                };
            }
        }

        if let Some(wall) = budget.wall()
            && elapsed > wall
        {
            let _ = child.kill();
            let _ = child.wait();
            return BudgetOutcome::OverWall {
                budget_ns: saturating_nanos(wall),
                elapsed_ns: saturating_nanos(elapsed),
            };
        }

        if let Some(limit) = budget.rss_bytes {
            // A transient `None` (the child is between fork and exec, or has
            // just exited) simply skips this poll; `try_wait` above owns exit.
            if let Some(observed) = resident_bytes(pid)
                && observed > limit
            {
                let _ = child.kill();
                let _ = child.wait();
                return BudgetOutcome::OverRss {
                    budget_bytes: limit,
                    observed_bytes: observed,
                };
            }
        }

        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Collect a normally-exited child's report from stdout.
fn finished(child: &mut Child, status: ExitStatus) -> BudgetOutcome {
    let mut output = String::new();
    if let Some(mut stdout) = child.stdout.take() {
        let _ = stdout.read_to_string(&mut output);
    }
    if !status.success() {
        return BudgetOutcome::ChildError {
            detail: format!("child exited with {status}; stdout: {output:?}"),
        };
    }
    match facet_json::from_str::<ChildReport>(&output) {
        Ok(report) => BudgetOutcome::Within { report },
        Err(error) => BudgetOutcome::ChildError {
            detail: format!("decoding child report: {error}; stdout: {output:?}"),
        },
    }
}

fn saturating_nanos(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

/// The child entry point: read a workload request from stdin, prepare it,
/// execute it, and write the report to stdout as facet-json. Runaway workloads
/// never reach the write; the parent watchdog terminates them. Never returns.
pub fn run_child_from_stdio() -> ! {
    let mut input = String::new();
    std::io::stdin()
        .read_to_string(&mut input)
        .expect("read workload request from stdin");
    let workload: Workload =
        facet_json::from_str(&input).expect("decode workload request from stdin");
    let prepared = prepare_workload(&workload);
    let report = execute_prepared(prepared);
    let encoded = facet_json::to_string(&report).expect("encode child report");
    let mut stdout = std::io::stdout();
    stdout
        .write_all(encoded.as_bytes())
        .expect("write child report to stdout");
    stdout.flush().expect("flush child report");
    std::process::exit(0);
}

/// A workload after its preparation phase: all parsing, checking, lowering,
/// verification, and native compilation is done, so [`execute_prepared`] does
/// only the work a budget is meant to gate. A runaway fixture carries no
/// prepared state; its divergence is the execution.
enum Prepared {
    /// A source prepared through [`crate::ratchet::prepare_source`]. `Err` holds
    /// a preparation failure (e.g. the source did not compile) surfaced at
    /// execution as a failed report.
    Source(Result<crate::ratchet::PreparedRun, String>),
    /// An immediate completion.
    Immediate,
    /// A completion after the given delay, exercised in the execution phase.
    Delay(Duration),
    /// A runaway native loop.
    SpinForever,
    /// A resident-set fixture: allocate and hold `target_bytes` in execution.
    GrowResident(usize),
    /// A workload whose preparation was slow; execution is instant.
    SlowPrepare,
}

/// Run a workload's preparation phase. All fixed compiler/JIT cost happens here,
/// before the readiness boundary; nothing here is the tested program's work.
fn prepare_workload(workload: &Workload) -> Prepared {
    match workload {
        Workload::RunSource { source } => Prepared::Source(
            crate::ratchet::prepare_source(source).map_err(|error| format!("{error:?}")),
        ),
        Workload::Immediate => Prepared::Immediate,
        Workload::Delay { duration_ns } => Prepared::Delay(Duration::from_nanos(*duration_ns)),
        Workload::SpinForever => Prepared::SpinForever,
        Workload::GrowResident { target_bytes } => {
            Prepared::GrowResident(usize::try_from(*target_bytes).unwrap_or(usize::MAX))
        }
        Workload::SlowPrepare { prepare_ns } => {
            std::thread::sleep(Duration::from_nanos(*prepare_ns));
            Prepared::SlowPrepare
        }
    }
}

/// Execute a prepared workload. Completing workloads return a report; runaway
/// fixtures diverge and are terminated by the parent watchdog.
#[must_use]
fn execute_prepared(prepared: Prepared) -> ChildReport {
    match prepared {
        Prepared::Source(Ok(run)) => match run.execute() {
            Ok(report) => ChildReport::RanSource {
                passed: report.passed(),
            },
            Err(error) => ChildReport::Failed {
                message: format!("{error:?}"),
            },
        },
        Prepared::Source(Err(message)) => ChildReport::Failed { message },
        Prepared::Immediate | Prepared::SlowPrepare => ChildReport::Completed,
        Prepared::Delay(duration) => {
            std::thread::sleep(duration);
            ChildReport::Completed
        }
        Prepared::SpinForever => loop {
            std::hint::spin_loop();
        },
        Prepared::GrowResident(target_bytes) => {
            // `vec![_; n]` writes every byte, faulting the pages resident. Hold
            // the buffer and spin so the parent observes the elevated RSS.
            let mut held = vec![0xAB_u8; target_bytes];
            if let Some(first) = held.first_mut() {
                *first = 1;
            }
            let held = std::hint::black_box(held);
            loop {
                std::hint::spin_loop();
                let _ = std::hint::black_box(&held);
            }
        }
    }
}

/// The resident-set size of `pid` in bytes, or `None` when it cannot be observed
/// (a transient race, or an unsupported platform).
#[cfg(target_os = "macos")]
fn resident_bytes(pid: u32) -> Option<u64> {
    // proc_pidinfo(PROC_PIDTASKINFO) reports a child's resident_size without
    // privileged task-port access. A short write count means the call failed.
    let mut info: libc::proc_taskinfo = unsafe { std::mem::zeroed() };
    let size = std::mem::size_of::<libc::proc_taskinfo>() as libc::c_int;
    let written = unsafe {
        libc::proc_pidinfo(
            pid as libc::c_int,
            libc::PROC_PIDTASKINFO,
            0,
            (&mut info as *mut libc::proc_taskinfo).cast::<libc::c_void>(),
            size,
        )
    };
    (written == size).then_some(info.pti_resident_size)
}

/// The resident-set size of `pid` in bytes from `/proc/<pid>/statm` (field 2 is
/// resident pages).
#[cfg(target_os = "linux")]
fn resident_bytes(pid: u32) -> Option<u64> {
    let statm = std::fs::read_to_string(format!("/proc/{pid}/statm")).ok()?;
    let resident_pages: u64 = statm.split_whitespace().nth(1)?.parse().ok()?;
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    let page_size = u64::try_from(page_size).ok()?;
    resident_pages.checked_mul(page_size)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
fn resident_bytes(_pid: u32) -> Option<u64> {
    None
}
