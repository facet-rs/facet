//! The demand scheduler — slice 1 of the vix lowering (constitution:
//! vixen repo, docs/design/machine-lowering.md).
//!
//! The three spawn-suppression mechanisms, enforced here from the
//! first commit:
//! 1. MEMO HIT — computed before: no task, no frame, the invocation's
//!    input slot fills synchronously and the caller's Await sails
//!    through without parking.
//! 2. UNDEMANDED — never asked: nothing exists to suppress. The
//!    driver only ever materializes invocations a running body
//!    actually reached.
//! 3. PARKED — asked and waiting: the only mechanism that costs a
//!    frame, and the frame chain in the task arena is the whole cost.
//!
//! THE CALL PROTOCOL (how a vix memo boundary lowers): the body writes
//! the callee's identity and arguments into a designated frame region,
//! then executes HostCall(INVOKE) followed by Await(slot). The INVOKE
//! host is the driver itself: it reads the request from the frame,
//! consults the memo — hit fills the slot before the Await runs (the
//! sync path, no park machinery touched); miss spawns the callee task
//! and the Await parks the caller. Amos's ruled sync/async distinction
//! IS the memo hit/miss distinction, mechanically.
//!
//! Scalars (i64) only in this slice; handles into the value store
//! arrive with slice 2. Trace: driver-level events recorded directly;
//! they join the unified stream when the vix lowering emits Op::Trace
//! marks with node identities.

use std::collections::HashMap;

use weavy::task::{FnId, HostFn, Program, Task, TaskStep};

/// INVOKE request frame contract (the lowering and this driver's
/// shared knowledge): at `INVOKE_REGION` the body lays out
/// [input_slot, fn_ref, argc, arg0, arg1, ...] as i64 words before
/// HostCall(INVOKE_HOST). The region is ordinary frame space —
/// spill-rule-resident like everything else.
pub const INVOKE_HOST: u32 = 0;

/// One compiled vix function: its task program identity plus where its
/// INVOKE region and argument slots live. Cached content-addressed —
/// `hash` is the closure hash (canonical AST × referenced code+types),
/// computed by the vix side; the driver only compares it.
#[derive(Clone, Debug)]
pub struct LoweredFn {
    /// Closure hash: the memo key's function component.
    pub hash: u64,
    /// Index into the driver's task program.
    pub task_fn: FnId,
    /// Frame offsets where entry arguments land (frame-direct).
    pub arg_offsets: Vec<u32>,
    /// Byte offset of this function's INVOKE region.
    pub invoke_region: u32,
}

/// Driver-level events (join the unified trace via lowering-emitted
/// marks later; recorded directly in this slice).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriveEvent {
    /// Demand arrived for (fn hash, memo-key hash of args).
    Demanded { fn_hash: u64 },
    /// Served from memo — NO task existed.
    MemoHit { fn_hash: u64 },
    /// Spawned a task (memo miss).
    Spawned { fn_hash: u64 },
    /// A task parked awaiting another invocation's result.
    ParkedOn { fn_hash: u64 },
    /// An invocation completed and fed its awaiters.
    Completed { fn_hash: u64 },
}

/// A pending invocation request captured by the INVOKE host during a
/// task burst (applied by the driver after the burst returns).
#[derive(Clone, Debug)]
struct InvokeRequest {
    caller: usize,
    input_slot: usize,
    fn_ref: usize,
    args: Vec<i64>,
}

/// A running or parked task execution.
struct Execution {
    task: Task,
    fn_ref: usize,
    key: MemoKey,
    ready: Vec<bool>,
    awaited: Vec<i64>,
    /// input slot → the invocation key feeding it (for wiring
    /// completions).
    feeds: HashMap<usize, MemoKey>,
}

type MemoKey = (u64, Vec<i64>);

/// The demand scheduler.
pub struct Driver {
    program: Program,
    fns: Vec<LoweredFn>,
    memo: HashMap<MemoKey, i64>,
    pub trace: Vec<DriveEvent>,
}

impl Driver {
    pub fn new(program: Program, fns: Vec<LoweredFn>) -> Self {
        Driver {
            program,
            fns,
            memo: HashMap::new(),
            trace: Vec::new(),
        }
    }

    /// How many memo entries exist (tests: warm behavior).
    pub fn memo_len(&self) -> usize {
        self.memo.len()
    }

    /// Demand one invocation's identity: the edge of the machine.
    /// Returns the scalar result (slice 1).
    pub fn demand(&mut self, fn_ref: usize, args: Vec<i64>) -> i64 {
        let key: MemoKey = (self.fns[fn_ref].hash, args.clone());
        self.trace.push(DriveEvent::Demanded {
            fn_hash: key.0,
        });
        if let Some(&v) = self.memo.get(&key) {
            self.trace.push(DriveEvent::MemoHit { fn_hash: key.0 });
            return v;
        }

        // Waiters: invocation key → executions parked on it (by index
        // into `executions`) with the slot to fill.
        let mut executions: Vec<Option<Execution>> = Vec::new();
        let mut waiters: HashMap<MemoKey, Vec<(usize, usize)>> = HashMap::new();
        let mut runnable: Vec<usize> = Vec::new();

        let root = self.spawn(&mut executions, fn_ref, key.clone());
        runnable.push(root);

        while let Some(ix) = runnable.pop() {
            let mut exec = executions[ix].take().expect("runnable execution exists");
            let requests = self.burst(&mut exec, ix);
            match requests {
                Burst::Done(value) => {
                    let done_key = exec.key.clone();
                    self.memo.insert(done_key.clone(), value);
                    self.trace.push(DriveEvent::Completed { fn_hash: done_key.0 });
                    // Feed everyone parked on this invocation; they
                    // become runnable again.
                    if let Some(list) = waiters.remove(&done_key) {
                        for (waiter_ix, slot) in list {
                            let w = executions[waiter_ix]
                                .as_mut()
                                .expect("parked waiter exists");
                            w.ready[slot] = true;
                            w.awaited[slot] = value;
                            runnable.push(waiter_ix);
                        }
                    }
                    // Execution finished: drop it (arena and all).
                }
                Burst::Pending { new_requests, parked_input } => {
                    for req in new_requests {
                        let req_key: MemoKey =
                            (self.fns[req.fn_ref].hash, req.args.clone());
                        self.trace.push(DriveEvent::Demanded { fn_hash: req_key.0 });
                        if let Some(&v) = self.memo.get(&req_key) {
                            // Mechanism 1: memo hit — the slot fills
                            // synchronously, no task exists.
                            self.trace.push(DriveEvent::MemoHit { fn_hash: req_key.0 });
                            exec.ready[req.input_slot] = true;
                            exec.awaited[req.input_slot] = v;
                        } else {
                            exec.feeds.insert(req.input_slot, req_key.clone());
                            let already_running = waiters.contains_key(&req_key)
                                || executions.iter().flatten().any(|e| e.key == req_key);
                            waiters
                                .entry(req_key.clone())
                                .or_default()
                                .push((req.caller, req.input_slot));
                            if !already_running {
                                let child =
                                    self.spawn(&mut executions, req.fn_ref, req_key);
                                runnable.push(child);
                            }
                        }
                    }
                    // Runnable only if the slot it PARKED ON is now
                    // ready; otherwise it stays parked and the waiter
                    // wiring wakes it on completion (never re-poll a
                    // blocked task — the waker-precision rule at
                    // driver level).
                    if exec.ready.get(parked_input).copied().unwrap_or(false) {
                        runnable.push(ix);
                    } else {
                        self.trace.push(DriveEvent::ParkedOn {
                            fn_hash: exec.key.0,
                        });
                    }
                    executions[ix] = Some(exec);
                    continue;
                }
            }
        }

        *self.memo.get(&key).expect("root invocation completed")
    }

    fn spawn(
        &mut self,
        executions: &mut Vec<Option<Execution>>,
        fn_ref: usize,
        key: MemoKey,
    ) -> usize {
        let lowered = &self.fns[fn_ref];
        self.trace.push(DriveEvent::Spawned { fn_hash: lowered.hash });
        let mut task = Task::spawn(&self.program, lowered.task_fn);
        for (offset, value) in lowered.arg_offsets.iter().zip(&key.1) {
            task.write_i64(*offset, *value);
        }
        executions.push(Some(Execution {
            task,
            fn_ref,
            key,
            ready: Vec::new(),
            awaited: Vec::new(),
            feeds: HashMap::new(),
        }));
        executions.len() - 1
    }

    /// Run one execution until done or blocked, capturing INVOKE
    /// requests raised during the burst.
    fn burst(&mut self, exec: &mut Execution, exec_ix: usize) -> Burst {
        let invoke_region = self.fns[exec.fn_ref].invoke_region as usize;
        loop {
            // Size the input arrays BEFORE the burst (slots the body
            // registers this burst get sized on the next iteration —
            // the driver loop always re-enters after filling).
            let max_slot = exec.feeds.keys().copied().max().map_or(0, |m| m + 1);
            let want = max_slot.max(exec.ready.len()).max(16);
            exec.ready.resize(want, false);
            exec.awaited.resize(want, 0);

            let mut requests: Vec<InvokeRequest> = Vec::new();
            let mut invoke = |frame: &mut [u8]| {
                let word = |i: usize| {
                    i64::from_le_bytes(
                        frame[invoke_region + i * 8..invoke_region + i * 8 + 8]
                            .try_into()
                            .expect("invoke region word"),
                    )
                };
                let input_slot = word(0) as usize;
                let fn_ref = word(1) as usize;
                let argc = word(2) as usize;
                let args = (0..argc).map(|k| word(3 + k)).collect();
                requests.push(InvokeRequest {
                    caller: exec_ix,
                    input_slot,
                    fn_ref,
                    args,
                });
            };
            let mut hosts: [HostFn<'_>; 1] = [&mut invoke];
            let step =
                exec.task
                    .run_hosted(&self.program, &exec.ready, &exec.awaited, &mut hosts);
            drop(hosts);

            match step {
                TaskStep::Done => {
                    let value = exec.task.result_i64();
                    return Burst::Done(value);
                }
                TaskStep::Parked { input } => {
                    let input = input as usize;
                    if exec.ready.len() <= input {
                        exec.ready.resize(input + 1, false);
                        exec.awaited.resize(input + 1, 0);
                    }
                    if requests.is_empty() && exec.ready[input] {
                        // Slot filled between bursts: loop and re-enter.
                        continue;
                    }
                    return Burst::Pending {
                        new_requests: requests,
                        parked_input: input,
                    };
                }
            }
        }
    }
}

enum Burst {
    Done(i64),
    Pending {
        new_requests: Vec<InvokeRequest>,
        parked_input: usize,
    },
}

#[cfg(test)]
mod tests {
    use super::*;
    use weavy::mem::Layout;
    use weavy::task::{ArgCopy, Fn as TaskFn, Op};

    /// Build the classic: fib(n) = n < 2 ? n : fib(n-1) + fib(n-2),
    /// expressed as vix WOULD lower it — every fib(k) is a MEMO
    /// BOUNDARY invocation through the INVOKE protocol, so the driver
    /// computes fib(20) with exactly 21 spawns (memo kills the
    /// exponential tree), parks on misses, sails through hits.
    ///
    /// Slice-1 note: the task lane has no branches yet (control flow
    /// is the vix graph's job — cond arms are separate nodes). So the
    /// fixture splits fib into base/recursive FUNCTIONS and the test
    /// demands them per the graph shape vix will generate: base cases
    /// seeded via memo (as Const nodes resolve), recursive body as one
    /// lowered fn.
    fn fib_body_program() -> (Program, Vec<LoweredFn>) {
        // frame: [n @0, invoke region @8.. (slot,fn,argc,arg) = 8..40,
        //         r1 @40, r2 @48, out @56, tmp @64]
        let body = TaskFn {
            frame: Layout { size: 96, align: 8 },
            code: vec![
                // request fib(n-1) into input slot 0
                Op::ConstI64 { dst: 8, value: 0 },  // input_slot = 0
                Op::ConstI64 { dst: 16, value: 0 }, // fn_ref = 0 (self)
                Op::ConstI64 { dst: 24, value: 1 }, // argc = 1
                Op::ConstI64 { dst: 64, value: -1 },
                Op::AddI64 { dst: 32, a: 0, b: 64 }, // arg0 = n-1
                Op::HostCall { host: INVOKE_HOST },
                // request fib(n-2) into input slot 1
                Op::ConstI64 { dst: 8, value: 1 },
                Op::ConstI64 { dst: 64, value: -2 },
                Op::AddI64 { dst: 32, a: 0, b: 64 }, // arg0 = n-2
                Op::HostCall { host: INVOKE_HOST },
                // await both (joint: both requests registered before
                // the first park — batched demand, not sequential)
                Op::Await { dst: 40, input: 0 },
                Op::Await { dst: 48, input: 1 },
                Op::AddI64 { dst: 56, a: 40, b: 48 },
                Op::Ret { src: 56, size: 8 },
            ],
        };
        let program = Program { fns: vec![body] };
        let fns = vec![LoweredFn {
            hash: 0xF1B,
            task_fn: FnId(0),
            arg_offsets: vec![0],
            invoke_region: 8,
        }];
        (program, fns)
    }

    #[test]
    fn memo_boundaries_kill_the_exponential_tree() {
        let (program, fns) = fib_body_program();
        let mut driver = Driver::new(program, fns);
        // Base cases enter as memo facts (vix Const nodes resolve
        // without bodies).
        driver.memo.insert((0xF1B, vec![0]), 0);
        driver.memo.insert((0xF1B, vec![1]), 1);

        assert_eq!(driver.demand(0, vec![20]), 6765);

        let spawns = driver
            .trace
            .iter()
            .filter(|e| matches!(e, DriveEvent::Spawned { .. }))
            .count();
        // fib(2)..fib(20): exactly 19 bodies ever ran. Naive recursion
        // would run 13,528. Mechanism 1 (memo) + shared-waiter joining
        // did the rest.
        assert_eq!(spawns, 19, "one spawn per distinct argument, ever");

        let hits = driver
            .trace
            .iter()
            .filter(|e| matches!(e, DriveEvent::MemoHit { .. }))
            .count();
        assert!(hits > 0, "sync path exercised");
    }

    #[test]
    fn warm_demand_spawns_nothing() {
        let (program, fns) = fib_body_program();
        let mut driver = Driver::new(program, fns);
        driver.memo.insert((0xF1B, vec![0]), 0);
        driver.memo.insert((0xF1B, vec![1]), 1);
        driver.demand(0, vec![15]);
        let cold_spawns = driver
            .trace
            .iter()
            .filter(|e| matches!(e, DriveEvent::Spawned { .. }))
            .count();

        driver.trace.clear();
        assert_eq!(driver.demand(0, vec![15]), 610);
        let warm_spawns = driver
            .trace
            .iter()
            .filter(|e| matches!(e, DriveEvent::Spawned { .. }))
            .count();
        assert_eq!(warm_spawns, 0, "mechanism 1: warm demand costs NO task");
        assert!(cold_spawns > 0);
        assert_eq!(
            driver.trace,
            vec![
                DriveEvent::Demanded { fn_hash: 0xF1B },
                DriveEvent::MemoHit { fn_hash: 0xF1B },
            ],
            "the whole warm trace is one demand and one hit"
        );
    }

    #[test]
    fn undemanded_functions_never_appear_in_the_trace() {
        // Two lowered fns; only one is demanded. The other's hash must
        // be ABSENT from the trace entirely — mechanism 2 as a trace-
        // absence assertion, the ruled testing style.
        let (program, mut fns) = fib_body_program();
        let mut program = program;
        program.fns.push(TaskFn {
            frame: Layout { size: 16, align: 8 },
            code: vec![
                Op::ConstI64 { dst: 8, value: 999 },
                Op::Ret { src: 8, size: 8 },
            ],
        });
        fns.push(LoweredFn {
            hash: 0xDEAD,
            task_fn: FnId(1),
            arg_offsets: vec![],
            invoke_region: 8,
        });
        let mut driver = Driver::new(program, fns);
        driver.memo.insert((0xF1B, vec![0]), 0);
        driver.memo.insert((0xF1B, vec![1]), 1);
        driver.demand(0, vec![5]);
        assert!(
            !driver.trace.iter().any(|e| matches!(
                e,
                DriveEvent::Demanded { fn_hash: 0xDEAD }
                    | DriveEvent::Spawned { fn_hash: 0xDEAD }
            )),
            "never asked, never anything"
        );
    }

    #[test]
    fn plain_task_calls_still_work_below_memo_boundaries() {
        // Sub-memo helper calls stay ordinary task-level Calls (no
        // driver involvement): aggregation unit = memo unit.
        let helper = TaskFn {
            frame: Layout { size: 24, align: 8 },
            code: vec![
                Op::MulI64 { dst: 16, a: 0, b: 8 },
                Op::Ret { src: 16, size: 8 },
            ],
        };
        let body = TaskFn {
            frame: Layout { size: 32, align: 8 },
            code: vec![
                Op::ConstI64 { dst: 8, value: 7 },
                Op::Call {
                    callee: FnId(1),
                    args: vec![
                        ArgCopy { src: 0, dst: 0, size: 8 },
                        ArgCopy { src: 8, dst: 8, size: 8 },
                    ],
                    ret: 16,
                },
                Op::Ret { src: 16, size: 8 },
            ],
        };
        let program = Program { fns: vec![body, helper] };
        let fns = vec![LoweredFn {
            hash: 0xAB,
            task_fn: FnId(0),
            arg_offsets: vec![0],
            invoke_region: 24,
        }];
        let mut driver = Driver::new(program, fns);
        assert_eq!(driver.demand(0, vec![6]), 42);
        let spawns = driver
            .trace
            .iter()
            .filter(|e| matches!(e, DriveEvent::Spawned { .. }))
            .count();
        assert_eq!(spawns, 1, "helper call is intra-task, not a driver spawn");
    }
}
