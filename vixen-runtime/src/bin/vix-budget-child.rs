//! Child process for the outer budget-enforcing test runner.
//!
//! It reads a facet-json `Workload` from stdin, executes it, and writes a
//! facet-json `ChildReport` to stdout. Runaway fixtures never return; the parent
//! watchdog in `vix::budget` terminates them over wall or resident-set budget.
//! All process orchestration lives in the library so this binary is a thin,
//! auditable entry point.

fn main() {
    vixen_runtime::budget::run_child_from_stdio();
}
