//! Multi-suspend-point proofs over a real tokio executor.

use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use weavy_async_spike::{Op, SuspendEvent, WeavyExec, available, compile};

fn boxed(f: impl Future<Output = i64> + 'static) -> Pin<Box<dyn Future<Output = i64>>> {
    Box::pin(f)
}

/// A future that yields `value` after `ms` — a stand-in for a vox stream / a
/// cross-executor part landing later.
fn later(value: i64, ms: u64) -> Pin<Box<dyn Future<Output = i64>>> {
    boxed(async move {
        tokio::time::sleep(Duration::from_millis(ms)).await;
        value
    })
}

async fn drive(mut exec: WeavyExec) -> (i64, Vec<SuspendEvent>) {
    let result =
        std::future::poll_fn(|cx| Pin::new(&mut exec).poll(cx)).await;
    (result, exec.trace.clone())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn single_await_across_a_suspend() {
    if !available() {
        return;
    }
    // push 40; await; add  ==>  40 + 2
    let program = compile(&[Op::Push(40), Op::Await, Op::Add]).unwrap();
    assert_eq!(program.await_count(), 1);
    let (result, trace) = drive(WeavyExec::new(program, vec![later(2, 40)])).await;
    assert_eq!(result, 42);
    assert_eq!(trace.len(), 1, "parked once");
    assert_eq!(trace[0].await_index, 0);
    assert_eq!(trace[0].stack_depth, 1, "40 was on the stack when it parked");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ready_input_never_suspends() {
    if !available() {
        return;
    }
    let program = compile(&[Op::Push(100), Op::Await, Op::Add]).unwrap();
    let (result, trace) =
        drive(WeavyExec::new(program, vec![boxed(std::future::ready(23))])).await;
    assert_eq!(result, 123);
    assert!(trace.is_empty(), "ready value ⇒ native fast path, no park");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_awaits_park_twice_and_resume_in_order() {
    if !available() {
        return;
    }
    // (await#0) * (await#1)  ==>  6 * 7 = 42, computed as:
    //   await X0; await X1; mul
    let program = compile(&[Op::Await, Op::Await, Op::Mul]).unwrap();
    assert_eq!(program.await_count(), 2);
    // X0 lands at 30ms, X1 at 60ms — the chain parks at #0, then at #1.
    let (result, trace) =
        drive(WeavyExec::new(program, vec![later(6, 30), later(7, 60)])).await;
    assert_eq!(result, 42);
    let indices: Vec<usize> = trace.iter().map(|e| e.await_index).collect();
    assert_eq!(indices, vec![0, 1], "parked at await #0 then await #1");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn independent_awaits_resolve_concurrently() {
    if !available() {
        return;
    }
    // The chain visits await#0 THEN await#1, but the driver polls both every
    // turn — so if #1 lands BEFORE #0, the resume past #0 sails straight
    // through #1 without parking again. Only ONE suspension despite two awaits.
    let program = compile(&[Op::Await, Op::Await, Op::Add]).unwrap();
    // #0 lands LATE (60ms); #1 lands EARLY (20ms). By the time #0 wakes us,
    // #1 is already ready.
    let (result, trace) =
        drive(WeavyExec::new(program, vec![later(40, 60), later(2, 20)])).await;
    assert_eq!(result, 42);
    assert_eq!(
        trace.len(),
        1,
        "await#1 resolved concurrently while parked on #0 — only one park: {trace:?}"
    );
    assert_eq!(trace[0].await_index, 0);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn suspension_is_inspectable_while_parked() {
    if !available() {
        return;
    }
    // push 10; push 20; add (=30); await; mul  ==>  30 * (awaited 4) = 120.
    // While parked on the await, the operand stack holds exactly [30].
    let program = compile(&[
        Op::Push(10),
        Op::Push(20),
        Op::Add,
        Op::Await,
        Op::Mul,
    ])
    .unwrap();
    let mut exec = WeavyExec::new(program, vec![later(4, 50)]);

    // Drive to completion; every time it's parked, the live suspended state
    // must show the pre-suspend operand stack ([30]).
    let mut inspected = false;
    let result = std::future::poll_fn(|cx| {
        let p = Pin::new(&mut exec).poll(cx);
        if p.is_pending()
            && let Some(s) = exec.suspension()
        {
            assert_eq!(s.await_index, 0);
            assert_eq!(s.stack, vec![30], "30 computed before the suspend point");
            inspected = true;
        }
        p
    })
    .await;

    assert!(inspected, "the chain parked at least once and was inspectable");
    assert_eq!(result, 120);
}
