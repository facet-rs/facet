//! The outer, budget-enforcing test runner.
//!
//! A `#[test]` budget (`vix::vir::Budget`) is only a gate if something outside
//! the test can *stop* an over-budget run. An in-process `Instant` check cannot
//! interrupt a stuck native loop; a leaked timeout thread cannot reclaim its
//! memory; Nextest's own timeout is coarse and does not read the typed budget.
//! So enforcement lives here: the parent reads the typed budget *before*
//! execution, launches the workload in a watched child process, and terminates
//! that child the moment it exceeds the wall-clock or execution resident-growth
//! ceiling. Compiler/JIT residency is snapshotted at the readiness boundary,
//! just as compiler/JIT time is excluded from the execution wall budget.
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

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, ChildStdout, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use vix::vir::Budget;

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

/// The parent → child protocol. Preparation is requested first; execution is
/// separately released only after the child publishes [`ChildEvent::Ready`].
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
enum ParentCommand {
    Prepare { workload: Workload },
    Execute,
}

/// The child → parent protocol. `Ready` is a real synchronization point: by
/// the time it is emitted, parsing, checking, lowering, verification, and
/// native compilation have completed, while execution is still blocked on the
/// parent's typed `Execute` release.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
enum ChildEvent {
    Ready,
    Completed { report: ChildReport },
}

/// The child → parent half of the IPC protocol: what a completed workload
/// reports. A killed workload sends nothing; its outcome is the kill itself.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum ChildReport {
    /// A `RunSource` workload completed; `passed` is its ratchet verdict.
    RanSource {
        passed: bool,
        /// Resident-set samples at typed production execution boundaries.
        /// They are captured in the child while the corresponding runtime is
        /// live, so a caller can distinguish retained work from post-return
        /// allocator accounting.
        observations: Vec<ExecutionRssObservation>,
    },
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
            ChildReport::RanSource { passed, .. } => *passed,
            ChildReport::Completed => true,
            ChildReport::Failed { .. } => false,
        }
    }
}

/// A resident-set observation made at one typed [`crate::ratchet::ExecutionPhase`].
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct ExecutionRssObservation {
    pub phase: crate::ratchet::ExecutionPhase,
    pub resident_bytes: Option<u64>,
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
    /// The child exceeded its post-readiness resident-growth ceiling and was
    /// killed. `observed_bytes` is growth above the prepared baseline.
    OverRss {
        budget_bytes: u64,
        /// Absolute child RSS captured while it was waiting at the readiness
        /// boundary. Compiler/JIT/runtime baseline is deliberately not charged
        /// to the program's O(1)-space execution.
        ready_baseline_bytes: u64,
        /// Absolute RSS observed at the execution peak that breached the
        /// declared execution-owned delta budget.
        observed_bytes: u64,
        /// `observed_bytes - ready_baseline_bytes`, the charged execution peak.
        execution_peak_bytes: u64,
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
#[cfg(any(target_os = "macos", target_os = "linux", windows))]
const RSS_ENFORCEABLE: bool = true;
#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
const RSS_ENFORCEABLE: bool = false;

/// Compile `source` in the parent and enforce the one test's declared budget.
/// The source metadata is the authority: callers cannot substitute a looser
/// [`Budget`] than the `#[test { ... }]` declaration before spawning the child.
#[must_use]
pub fn run_source_under_declared_budget(child_exe: &Path, source: &str) -> BudgetOutcome {
    let compilation =
        match vix::compiler::Compiler::with_config(crate::default_config()).compile(source) {
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
/// the instant it exceeds a declared wall or post-readiness resident-growth
/// ceiling.
#[must_use]
pub fn run_under_budget(child_exe: &Path, budget: &Budget, workload: &Workload) -> BudgetOutcome {
    if budget.rss_bytes.is_some() && !RSS_ENFORCEABLE {
        return BudgetOutcome::RssEnforcementUnsupported {
            platform: std::env::consts::OS.to_owned(),
        };
    }

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

    let pid = child.id();
    let Some(mut stdin) = child.stdin.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return BudgetOutcome::ChildError {
            detail: "spawned child did not provide stdin".to_owned(),
        };
    };
    let Some(stdout) = child.stdout.take() else {
        let _ = child.kill();
        let _ = child.wait();
        return BudgetOutcome::ChildError {
            detail: "spawned child did not provide stdout".to_owned(),
        };
    };
    let mut stdout = BufReader::new(stdout);

    if let Err(detail) = write_parent_command(
        &mut stdin,
        &ParentCommand::Prepare {
            workload: workload.clone(),
        },
    ) {
        return kill_with_child_error(&mut child, detail);
    }
    match read_child_event(&mut stdout) {
        Ok(ChildEvent::Ready) => {}
        Ok(ChildEvent::Completed { .. }) => {
            return kill_with_child_error(
                &mut child,
                "child completed before publishing readiness".to_owned(),
            );
        }
        Err(detail) => return kill_with_child_error(&mut child, detail),
    }

    // The child cannot execute until it reads `Execute`, so this sample is the
    // documented readiness baseline rather than an arbitrary spawn-time RSS.
    let ready_baseline = if budget.rss_bytes.is_some() {
        match resident_bytes(pid) {
            Some(bytes) => bytes,
            None => {
                return kill_with_child_error(
                    &mut child,
                    "observing child RSS at readiness boundary".to_owned(),
                );
            }
        }
    } else {
        0
    };

    // The wall clock begins with the typed execution release, after every
    // parse/check/lower/verify/JIT action has reached the ready boundary.
    let start = Instant::now();
    if let Err(detail) = write_parent_command(&mut stdin, &ParentCommand::Execute) {
        return kill_with_child_error(&mut child, detail);
    }
    drop(stdin);
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
                return finished(&mut child, &mut stdout, status);
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
                && observed.saturating_sub(ready_baseline) > limit
            {
                let _ = child.kill();
                let _ = child.wait();
                return BudgetOutcome::OverRss {
                    budget_bytes: limit,
                    ready_baseline_bytes: ready_baseline,
                    observed_bytes: observed,
                    execution_peak_bytes: observed.saturating_sub(ready_baseline),
                };
            }
        }

        std::thread::sleep(POLL_INTERVAL);
    }
}

/// Collect a normally-exited child's report from stdout.
fn finished(
    _child: &mut Child,
    stdout: &mut BufReader<ChildStdout>,
    status: ExitStatus,
) -> BudgetOutcome {
    if !status.success() {
        return BudgetOutcome::ChildError {
            detail: format!("child exited with {status}"),
        };
    }
    match read_child_event(stdout) {
        Ok(ChildEvent::Completed { report }) => BudgetOutcome::Within { report },
        Ok(ChildEvent::Ready) => BudgetOutcome::ChildError {
            detail: "child exited after a duplicate readiness event".to_owned(),
        },
        Err(detail) => BudgetOutcome::ChildError { detail },
    }
}

fn saturating_nanos(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

/// The stack reserved for the child's protocol thread. Preparing a `RunSource`
/// workload natively compiles it, and lowering recurses over the island graph
/// deeply enough to overflow the platform default main-thread stack (1 MiB on
/// Windows/MSVC). A stack overflow aborts the process without unwinding, so the
/// child would die before publishing readiness and the parent would see only an
/// opaque "child exited before child event". Give preparation a generous stack.
const CHILD_STACK_BYTES: usize = 64 * 1024 * 1024;

/// The child entry point. Runs the protocol on a large-stack worker thread (see
/// `CHILD_STACK_BYTES`) so native compilation cannot silently overflow the
/// default main-thread stack, then exits with the thread's success. Runaway
/// workloads never publish `Completed`; the parent watchdog terminates them.
pub fn run_child_from_stdio() -> ! {
    let worker = std::thread::Builder::new()
        .stack_size(CHILD_STACK_BYTES)
        .spawn(run_child_protocol)
        .expect("spawn budget-child protocol thread");
    // A panic in the worker has already printed to the inherited stderr; a
    // nonzero exit lets the parent observe the child as a failed run rather
    // than a clean completion.
    match worker.join() {
        Ok(()) => std::process::exit(0),
        Err(_) => std::process::exit(101),
    }
}

/// The child protocol body: prepare a typed workload, publish `Ready`, wait for
/// the parent's typed execution release, run it, and report. Runs on the
/// large-stack worker thread spawned by [`run_child_from_stdio`].
fn run_child_protocol() {
    let stdin = std::io::stdin();
    let mut input = BufReader::new(stdin.lock());
    let command = read_parent_command(&mut input).expect("decode workload preparation command");
    let ParentCommand::Prepare { workload } = command else {
        panic!("first parent command must prepare workload");
    };
    let prepared = prepare_workload(&workload);
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    write_child_event(&mut output, &ChildEvent::Ready).expect("write readiness event");
    let command = read_parent_command(&mut input).expect("decode workload execution command");
    if command != ParentCommand::Execute {
        panic!("second parent command must release execution");
    }
    let report = execute_prepared(prepared);
    write_child_event(&mut output, &ChildEvent::Completed { report })
        .expect("write child completion event");
}

fn write_parent_command(writer: &mut ChildStdin, command: &ParentCommand) -> Result<(), String> {
    write_frame(writer, command).map_err(|error| format!("writing parent command: {error}"))
}

fn write_child_event(writer: &mut impl Write, event: &ChildEvent) -> Result<(), String> {
    write_frame(writer, event).map_err(|error| format!("writing child event: {error}"))
}

fn write_frame<T: for<'a> facet::Facet<'a>>(
    writer: &mut impl Write,
    value: &T,
) -> Result<(), String> {
    let encoded = facet_json::to_string(value).map_err(|error| error.to_string())?;
    writer
        .write_all(encoded.as_bytes())
        .map_err(|error| error.to_string())?;
    writer.write_all(b"\n").map_err(|error| error.to_string())?;
    writer.flush().map_err(|error| error.to_string())
}

fn read_parent_command(reader: &mut impl BufRead) -> Result<ParentCommand, String> {
    read_frame(reader, "parent command")
}

fn read_child_event(reader: &mut impl BufRead) -> Result<ChildEvent, String> {
    read_frame(reader, "child event")
}

fn read_frame<T: for<'a> facet::Facet<'a>>(
    reader: &mut impl BufRead,
    kind: &str,
) -> Result<T, String> {
    let mut line = String::new();
    let bytes = reader
        .read_line(&mut line)
        .map_err(|error| format!("reading {kind}: {error}"))?;
    if bytes == 0 {
        return Err(format!("child exited before {kind}"));
    }
    facet_json::from_str(&line).map_err(|error| format!("decoding {kind}: {error}"))
}

fn kill_with_child_error(child: &mut Child, detail: String) -> BudgetOutcome {
    let _ = child.kill();
    let _ = child.wait();
    BudgetOutcome::ChildError { detail }
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
        Prepared::Source(Ok(run)) => {
            let mut observations = Vec::new();
            let outcome = run.execute_with_observer(|phase| {
                observations.push(ExecutionRssObservation {
                    phase,
                    resident_bytes: resident_bytes(std::process::id()),
                });
            });
            match outcome {
                Ok(report) => ChildReport::RanSource {
                    passed: report.passed(),
                    observations,
                },
                Err(error) => ChildReport::Failed {
                    message: format!("{error:?}"),
                },
            }
        }
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
    // Physical footprint is macOS's pressure-accounted resident measure. Raw
    // `pti_resident_size` includes shared/file-backed pages and can exceed the
    // process's attributable footprint by hundreds of MiB, making the same
    // typed budget change meaning as binaries and the dyld cache evolve.
    let mut info: libc::rusage_info_v2 = unsafe { std::mem::zeroed() };
    let result = unsafe {
        libc::proc_pid_rusage(
            pid as libc::c_int,
            libc::RUSAGE_INFO_V2,
            (&raw mut info).cast::<libc::rusage_info_t>(),
        )
    };
    (result == 0).then_some(info.ri_phys_footprint)
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

/// The resident-set size of `pid` in bytes from the process working set
/// (`GetProcessMemoryInfo`'s `WorkingSetSize`), Windows's resident-memory
/// measure. `None` on a transient race (the child just exited, so the handle
/// can no longer be opened or queried); `try_wait` in the watchdog owns exit.
#[cfg(windows)]
fn resident_bytes(pid: u32) -> Option<u64> {
    use windows_sys::Win32::Foundation::CloseHandle;
    use windows_sys::Win32::System::ProcessStatus::{
        GetProcessMemoryInfo, PROCESS_MEMORY_COUNTERS,
    };
    use windows_sys::Win32::System::Threading::{
        OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_VM_READ,
    };

    // SAFETY: `OpenProcess` is a plain FFI call; it returns a null handle on
    // failure (e.g. the child has exited), which we treat as an unobservable
    // sample. The `0` is the `bInheritHandle` BOOL.
    let handle = unsafe {
        OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_VM_READ, 0, pid)
    };
    if handle.is_null() {
        return None;
    }

    let mut counters = PROCESS_MEMORY_COUNTERS::default();
    let size = u32::try_from(std::mem::size_of::<PROCESS_MEMORY_COUNTERS>()).ok()?;
    // SAFETY: `handle` is a live process handle from `OpenProcess`; `counters`
    // is a valid, correctly sized out-parameter for the working-set query.
    let ok = unsafe { GetProcessMemoryInfo(handle, &raw mut counters, size) };
    // SAFETY: `handle` came from `OpenProcess` and is not used after this close.
    unsafe { CloseHandle(handle) };
    if ok == 0 {
        return None;
    }
    u64::try_from(counters.WorkingSetSize).ok()
}

#[cfg(not(any(target_os = "macos", target_os = "linux", windows)))]
fn resident_bytes(_pid: u32) -> Option<u64> {
    None
}
