//! `vx` — a shim that runs one `.vix` file.
//!
//! Deliberately tiny. `vx <file.vix>` reads a file and runs its `#[test]`
//! declarations through the same production path the ratchet drives. This
//! CLI reads no on-disk format of its own.
//!
//! It briefly grew a `vx r <package-dir> <command>` form that resolved
//! through a `package.styx` manifest document. That whole genre is retired: a
//! package is an entry-point module with exports, and running one means
//! demanding an export, not consulting a sidecar document. The replacement is
//! undesigned, so the subcommand is gone rather than left pointing at a
//! format that no longer exists.
//!
//! # The fixture root is the one real decision here
//!
//! A vix program reaches the filesystem through an ORIGIN ADAPTER, and the only
//! one that exists today is the harness's [`FixtureStore`] — which serves trees
//! from `<root>/trees/<name>` and a package registry from `<root>/registry`.
//! So `fixture_tree("x")` in a file run by this shim reads `<root>/trees/x`,
//! where `<root>` defaults to the directory containing the `.vix` file (for a
//! package command: the package directory) and is overridable with
//! `--fixtures`.
//!
//! That naming is inherited, not chosen: a production embedding would declare
//! its own constant surface (`workspace()`, say) over its own origin adapter
//! through the same seam — `ConstantSurfaceDecl` + `PrimitiveServices::with_origin`
//! — and `vix-core` would learn nothing new. Until such a surface exists, this
//! shim borrows the harness's rather than inventing a second one to maintain.

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;

use vix::runtime::PrimitiveServices;
use vixen_runtime::fixture::FixtureStore;
use vixen_runtime::host_exec::HostExecBackend;
use vixen_runtime::ratchet::{PreparedRun, RatchetReport, prepare_source};

fn main() -> ExitCode {
    let arguments: Vec<String> = std::env::args().skip(1).collect();
    let invocation = match Invocation::parse(&arguments) {
        Parsed::Run(invocation) => invocation,
        // Asked-for help is a success: it goes to stdout and exits 0. Only a
        // malformed invocation is an error.
        Parsed::Help => {
            println!("{USAGE}");
            return ExitCode::SUCCESS;
        }
        Parsed::Invalid => {
            eprintln!("{USAGE}");
            return ExitCode::from(2);
        }
    };

    // What to run and what to call it in the report.
    let source = &invocation.source;
    let text = match std::fs::read_to_string(source) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("vx: {}: {error}", source.display());
            return ExitCode::from(2);
        }
    };
    let (label, prepared) = match prepare_source(&text) {
        Ok(run) => (source.display().to_string(), run),
        Err(error) => {
            eprintln!("vx: {}: run failed\n{error:#?}", source.display());
            return ExitCode::from(1);
        }
    };

    // The two authorities this CLI grants, both named here on purpose. `vix-core`
    // ships neither: it declares the seams and lets the embedder decide. So the
    // decision that running vix code may spawn host processes through
    // `std::process::Command` is THIS FILE's, and swapping in a sandboxing
    // backend later is a one-line change here rather than a change to the
    // machine.
    let services = PrimitiveServices::default()
        .with_origin(
            FixtureStore::origin_decl(),
            Arc::new(FixtureStore::with_root(invocation.fixtures.clone())),
        )
        .expect("one origin adapter cannot overlap itself")
        .with_exec_backend(Arc::new(HostExecBackend));

    let report = match execute(prepared, services) {
        Ok(report) => report,
        Err(error) => {
            // The typed diagnostic set IS the error message. Rendering it as a
            // pretty report is a real feature and deliberately not attempted
            // here — a shim that half-renders diagnostics is worse than one
            // that shows them whole.
            eprintln!("vx: {label}: run failed\n{error:#?}");
            return ExitCode::from(1);
        }
    };

    print_report(&label, &report);
    if report.passed() {
        ExitCode::SUCCESS
    } else {
        ExitCode::from(1)
    }
}

fn execute(
    prepared: PreparedRun,
    services: PrimitiveServices,
) -> Result<RatchetReport, vixen_runtime::ratchet::RunError> {
    prepared.execute_with_primitive_services(services)
}

const USAGE: &str = "usage: vx [--fixtures <dir>] [--] <file.vix>";

struct Invocation {
    source: PathBuf,
    fixtures: PathBuf,
}

/// What an argument vector asked for. Help and a malformed invocation are
/// different answers and get different exit codes.
enum Parsed {
    Run(Invocation),
    Help,
    Invalid,
}

impl Invocation {
    fn parse(arguments: &[String]) -> Parsed {
        let mut source: Option<PathBuf> = None;
        let mut fixtures: Option<PathBuf> = None;
        let mut rest = arguments.iter();
        // Everything after `--` is a path, so a file whose name begins with `-`
        // is nameable.
        let mut only_paths = false;
        while let Some(argument) = rest.next() {
            if only_paths {
                if source.is_some() {
                    return Parsed::Invalid;
                }
                source = Some(PathBuf::from(argument));
                continue;
            }
            match argument.as_str() {
                "--" => only_paths = true,
                // Repeating an option is as malformed as repeating the source;
                // last-wins would silently pick one of two stated intents.
                "--fixtures" => match rest.next() {
                    Some(directory) if fixtures.is_none() => {
                        fixtures = Some(PathBuf::from(directory));
                    }
                    _ => return Parsed::Invalid,
                },
                "-h" | "--help" => return Parsed::Help,
                flag if flag.starts_with('-') => return Parsed::Invalid,
                path if source.is_none() => source = Some(PathBuf::from(path)),
                _ => return Parsed::Invalid,
            }
        }
        let Some(source) = source else {
            return Parsed::Invalid;
        };
        // The `.vix` file's own directory is the default fixture root, so a
        // program and the trees it reads travel together.
        let fixtures = fixtures.unwrap_or_else(|| {
            source
                .parent()
                .filter(|parent| !parent.as_os_str().is_empty())
                .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
        });
        Parsed::Run(Self { source, fixtures })
    }
}

/// Summary on success, whole truth on failure. A per-check line is deliberately
/// not printed on the happy path: a `ProvenanceKey` is a yield-site index, not a
/// name, so twelve `site: 0..5` lines carry less than one count does. Labelling
/// them would need the test names, which means compiling a second time — a real
/// `vx test` should thread them through the report instead.
fn print_report(label: &str, report: &RatchetReport) {
    for warning in &report.warnings.entries {
        println!("warning: {warning:?}");
    }

    let failed: Vec<_> = report
        .plain
        .checks
        .iter()
        .filter(|check| !check.passed)
        .collect();
    for check in &failed {
        println!("FAILED {:?}", check.provenance);
        if let Some(failure) = &check.failure {
            println!("  failure: {failure:#?}");
        }
        if let Some(context) = &check.failure_context {
            println!("  context: {context:#?}");
        }
        if let Some(trace) = &check.trace_failure {
            println!("  trace:   {trace:#?}");
        }
    }
    for refusal in &report.plain.refusals {
        println!("REFUSED {refusal:#?}");
    }
    // A lane disagreement is not a check failure and has no red check to print,
    // so it has to be reported on its own or a nondeterministic program looks
    // like a passing one that merely failed.
    if !report.agrees() {
        println!("FAILED: the plain and chaos lanes disagree");
    }

    let counters = &report.plain.counters;
    println!(
        "{}: {} checks ({} failed), {} effect spawns — {}",
        label,
        report.plain.checks.len(),
        failed.len(),
        counters.effect_spawns,
        if report.passed() { "PASS" } else { "FAIL" }
    );
}
