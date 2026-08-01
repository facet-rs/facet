//! The process boundary is the EMBEDDER's decision, not the machine's.
//!
//! `vix-core` declares the [`ExecBackend`] seam and ships no implementation of
//! it — the same doctrine as primitives, host types, and the prelude. That was
//! already the design, and `EffectCtx::exec_backend` already failed loudly on a
//! snapshot without one; but `PrimitiveServices::exec_backend()` quietly
//! substituted the host backend for anyone who installed nothing, so in
//! practice the MACHINE decided that vix spawns host processes, invisibly, for
//! every caller. These tests pin both halves of removing that default: no
//! backend is a typed refusal, and an installed one is what makes exec work.

#![cfg(unix)]

use std::sync::{Arc, Mutex};

use vix::runtime::{
    ExecBackend, ExecEventSender, ExecInvocation, ExecWorkspace, PrimitiveServices,
};
use vixen_runtime::fixture::FixtureStore;
use vixen_runtime::host_exec::HostExecBackend;
use vixen_runtime::ratchet::prepare_source;

const SPAWNS_A_PROCESS: &str = r#"
#[test { budget_wall: 60s, budget_rss: 2048MB }]
fn t(sh: Sh) -> Stream<Check> {
    let out = exec sh`-c "echo ran"`;
    yield expect_eq(out.stdout.text().trim(), "ran");
}
"#;

fn origin_only() -> PrimitiveServices {
    PrimitiveServices::default()
        .with_origin(
            FixtureStore::origin_decl(),
            Arc::new(FixtureStore::default()),
        )
        .expect("one origin adapter cannot overlap itself")
}

#[test]
fn without_an_installed_backend_an_exec_refuses_loudly() {
    let report = prepare_source(SPAWNS_A_PROCESS)
        .expect("the program compiles")
        .execute_with_primitive_services(origin_only());

    match report {
        Ok(report) => panic!(
            "an exec with no installed backend must not succeed: {:?}",
            report.plain.checks
        ),
        Err(error) => {
            let rendered = format!("{error:?}");
            assert!(
                rendered.contains("no exec backend is installed"),
                "the refusal names the missing authority: {rendered}"
            );
        }
    }
}

#[test]
fn installing_the_host_backend_is_what_grants_the_process_boundary() {
    let services = origin_only().with_exec_backend(Arc::new(HostExecBackend));
    let report = prepare_source(SPAWNS_A_PROCESS)
        .expect("the program compiles")
        .execute_with_primitive_services(services)
        .expect("with a backend installed, the exec runs");
    assert!(report.passed(), "{:?}", report.plain.checks);
    assert_eq!(report.plain.counters.effect_spawns, 1);
}

/// A backend that spawns nothing and reports a boundary failure. Standing in
/// for the confining backend a sandbox would install: the point is that the
/// machine accepts it on equal footing with the host one, because nothing keys
/// on the backend.
struct RefusingBackend {
    asked: Mutex<Vec<ExecInvocation>>,
}

impl ExecBackend for RefusingBackend {
    fn begin(
        &self,
        invocation: ExecInvocation,
        _events: ExecEventSender,
    ) -> Result<ExecWorkspace, String> {
        self.asked
            .lock()
            .expect("invocation recorder mutex poisoned")
            .push(invocation);
        Err("this backend refuses to spawn".to_owned())
    }
}

#[test]
fn a_substituted_backend_receives_the_invocation_instead() {
    let backend = Arc::new(RefusingBackend {
        asked: Mutex::new(Vec::new()),
    });
    let services = origin_only().with_exec_backend(backend.clone());
    let outcome = prepare_source(SPAWNS_A_PROCESS)
        .expect("the program compiles")
        .execute_with_primitive_services(services);

    assert!(
        outcome.is_err(),
        "the substituted backend's refusal reaches the caller"
    );
    let asked = backend.asked.lock().expect("invocation recorder mutex");
    assert_eq!(asked.len(), 1, "the invocation went to the installed backend");
    assert_eq!(asked[0].program, "sh", "…with the plan the program spelled");
    assert_eq!(asked[0].argv, vec!["-c".to_owned(), "echo ran".to_owned()]);
}
