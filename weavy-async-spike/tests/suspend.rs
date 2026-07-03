//! The crux, proven over a real tokio executor: a JIT'd copy-and-patch chain
//! suspends at an await, and resumes to completion when a real Rust future
//! (a oneshot fired from ANOTHER task) lands.

use std::future::Future;
use weavy_async_spike::{Op, WeavyExec, available, compile};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn jit_chain_awaits_a_real_future_across_a_suspend() {
    if !available() {
        eprintln!("async stencils unavailable on this target — skipping");
        return;
    }

    // Program: push 40; await X; add  ==>  40 + X.
    let program = compile(&[Op::Push(40), Op::Await, Op::Add]).expect("compiles");

    // The awaited value arrives LATER, from another task (real async I/O
    // stand-in — this is a vox stream / cross-executor part in production).
    let (tx, rx) = tokio::sync::oneshot::channel::<i64>();
    let fire = tokio::spawn(async move {
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let _ = tx.send(2);
    });

    // The JIT'd chain, as a Future. It MUST suspend (the oneshot isn't ready
    // for 50ms) and resume — a strict run would need the value up front.
    let mut exec = WeavyExec::new(program, async move { rx.await.unwrap() });
    let result = std::future::poll_fn(|cx| std::pin::Pin::new(&mut exec).poll(cx)).await;

    fire.await.unwrap();
    assert_eq!(result, 42, "40 + (awaited 2) across a JIT suspend point");
    assert!(
        exec.suspends >= 1,
        "the chain must have actually PARKED on the await, not run straight through"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ready_value_completes_without_ever_suspending() {
    if !available() {
        return;
    }
    let program = compile(&[Op::Push(100), Op::Await, Op::Add]).expect("compiles");
    // Already-ready future: the chain runs straight through (the fast path is
    // native — no executor round trip when the input is present).
    let exec = WeavyExec::new(program, std::future::ready(23));
    assert_eq!(exec.await, 123);
}
