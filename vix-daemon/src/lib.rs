//! The vix daemon: a vox service that owns demand-driven evaluation.
//!
//! # Why a daemon
//!
//! The IDE should not embed the evaluator; it should TALK to one. The daemon
//! owns the oracle (soon: the real graph engine + a fleet of executors), and
//! the IDE is a generated vox client. Because vox generates typed clients from
//! the `#[service]` trait, "the browser IDE talks to a local daemon" is a
//! generated `.ts` client over a websocket — not bespoke plumbing.
//!
//! # Debugging a demand-driven language
//!
//! Source-line "stepping" doesn't fit: nothing forces locally, and the
//! "current position" is a demand FRONTIER (a set), not a point. So the debug
//! primitive is: advance the DEMAND. `eval` streams a [`DemandEvent`] for every
//! observable step (a memo hit/miss, an exec dispatch, a cache serving class,
//! an observation). In [`StepMode::Step`] the daemon GATES each event — it
//! blocks the evaluation until the client sends [`StepCommand::Step`], so the
//! IDE can walk the demand one node at a time and inspect between steps.
//!
//! This generalizes to REMOTE debugging for free: a closure/observer running on
//! an executor emits the same events over the same RPC, and the daemon mediates
//! the same step protocol — so "step through my closure running on a remote
//! executor" is default functionality, because the daemon already holds the
//! connection. (Executor-side step gating is the next slice; the protocol here
//! is the reusable part.)

use facet::Facet;
use std::sync::mpsc as std_mpsc;

use tokio::sync::{mpsc, oneshot};
use vix::exec::ExecEvent;
use vix::oracle::{Event, Oracle};
use vox::{Rx, Tx};

/// How the client wants to receive the demand stream.
#[derive(Facet, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum StepMode {
    /// Stream events as fast as evaluation produces them.
    Run,
    /// Gate each event: block evaluation until the client steps.
    Step,
}

/// What to evaluate.
#[derive(Facet, Debug, Clone)]
pub struct EvalRequest {
    /// The vix source module.
    pub source: String,
    /// The entry function to demand (must take no args, or only a `Target`).
    pub entry: String,
    pub mode: StepMode,
}

/// A client → daemon control message during a stepped evaluation.
#[derive(Facet, Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum StepCommand {
    /// Release exactly one gated demand event.
    Step,
    /// Stop gating; run to completion.
    Resume,
}

/// The exec-cache serving class, mirrored onto the wire.
#[derive(Facet, Debug, Clone, PartialEq, Eq)]
#[repr(u8)]
pub enum Serving {
    Ran,
    Tier1Hit,
    Tier2Cutoff { verified: u64 },
    Joined,
}

impl From<ExecEvent> for Serving {
    fn from(e: ExecEvent) -> Self {
        match e {
            ExecEvent::Ran => Serving::Ran,
            ExecEvent::Tier1Hit => Serving::Tier1Hit,
            ExecEvent::Tier2Cutoff { verified } => Serving::Tier2Cutoff {
                verified: verified as u64,
            },
            ExecEvent::Joined => Serving::Joined,
        }
    }
}

/// A half-open byte range into the evaluated module — the IDE's handle for
/// cross-highlighting between the graph and the editor.
#[derive(Facet, Debug, Clone, Copy, PartialEq, Eq)]
pub struct Span {
    pub start: u32,
    pub end: u32,
}

impl From<vix::support::Span> for Span {
    fn from(s: vix::support::Span) -> Self {
        Span {
            start: s.start,
            end: s.end,
        }
    }
}

/// One observable step of demand-driven evaluation, streamed to the IDE. This
/// is simultaneously the debugger's step feed and the graph-viz's edge feed:
/// `run` pairs Spawn↔Exec exactly, `caller`/`in_fn` give demand ancestry,
/// spans link every node back into the source, `at` is the eval-relative
/// timestamp in microseconds (the rails/lanes view feeds on it), and the
/// artifact payloads (args/argv/describe/outputs) say WHAT is being built.
#[derive(Facet, Debug, Clone, PartialEq)]
#[repr(u8)]
pub enum DemandEvent {
    /// A memoized call ran cold. `span` is the fn declaration; `args` render
    /// each bound argument shortly.
    Miss {
        at: u64,
        func: String,
        span: Span,
        caller: Option<String>,
        args: Vec<(String, String)>,
    },
    /// A memoized call was served from the language-level cache.
    Hit {
        at: u64,
        func: String,
        span: Span,
        caller: Option<String>,
        args: Vec<(String, String)>,
    },
    /// THUNK CREATED: the `cmd! { … }` block evaluated to a pending tree —
    /// nothing demanded yet. `describe` is the command grammar's
    /// important-first description (level 0 = verb + object; last = argv).
    Created {
        at: u64,
        command: String,
        run: u64,
        span: Span,
        in_fn: Option<String>,
        argv: Vec<String>,
        describe: Vec<String>,
    },
    /// EXECUTION SCHEDULED: the first demand touched this run (projection or
    /// identity) — paying starts here. Scheduled→Finished is the rectangle
    /// in the lanes view; Created is where a click links back to.
    Scheduled {
        at: u64,
        command: String,
        run: u64,
        span: Span,
    },
    /// EXECUTION FINISHED: resolved through the two-tier exec cache;
    /// `outputs` is the produced tree — artifacts, path by path.
    Finished {
        at: u64,
        command: String,
        run: u64,
        span: Span,
        serving: Serving,
        outputs: Vec<(String, String)>,
    },
    /// A primitive observed the world (cold) or replayed its pin.
    Observation {
        at: u64,
        key: String,
        replayed: bool,
    },
    /// Evaluation finished. `result` is the value's Debug form (v1).
    Done { result: String },
    /// Evaluation failed.
    Failed { error: String },
}

vox_schema::impl_reborrow_owned!(DemandEvent, StepCommand);

impl DemandEvent {
    fn from_oracle(at: u64, event: &Event) -> DemandEvent {
        match event {
            Event::Miss {
                func,
                span,
                caller,
                args,
            } => DemandEvent::Miss {
                at,
                func: func.clone(),
                span: (*span).into(),
                caller: caller.clone(),
                args: args.clone(),
            },
            Event::Hit {
                func,
                span,
                caller,
                args,
            } => DemandEvent::Hit {
                at,
                func: func.clone(),
                span: (*span).into(),
                caller: caller.clone(),
                args: args.clone(),
            },
            Event::Created {
                command,
                run,
                span,
                in_fn,
                argv,
                describe,
            } => DemandEvent::Created {
                at,
                command: command.clone(),
                run: *run,
                span: (*span).into(),
                in_fn: in_fn.clone(),
                argv: argv.clone(),
                describe: describe.clone(),
            },
            Event::Scheduled { command, run, span } => DemandEvent::Scheduled {
                at,
                command: command.clone(),
                run: *run,
                span: (*span).into(),
            },
            Event::Finished {
                command,
                run,
                span,
                event,
                outputs,
            } => DemandEvent::Finished {
                at,
                command: command.clone(),
                run: *run,
                span: (*span).into(),
                serving: event.clone().into(),
                outputs: outputs.clone(),
            },
            Event::Observation { key, replayed } => DemandEvent::Observation {
                at,
                key: key.clone(),
                replayed: *replayed,
            },
        }
    }
}

/// The daemon's RPC surface. The IDE gets a generated client for this.
#[vox::service]
pub trait Daemon {
    /// Evaluate `req.entry` of `req.source`, streaming a [`DemandEvent`] per
    /// step into `events`. In [`StepMode::Step`], each event is GATED: the
    /// daemon blocks until the client sends [`StepCommand::Step`] on `control`
    /// (or `Resume` to stop gating). The final event is `Done`/`Failed`.
    async fn eval(&self, req: EvalRequest, control: Rx<StepCommand>, events: Tx<DemandEvent>);
}

struct EvalJob {
    source: String,
    entry: String,
    sink: vix::oracle::EventSink,
    reply: std_mpsc::Sender<Result<String, String>>,
}

#[derive(Clone)]
struct OracleActor {
    jobs: std_mpsc::Sender<EvalJob>,
}

impl OracleActor {
    fn start() -> Self {
        let (jobs, rx) = std_mpsc::channel::<EvalJob>();
        std::thread::spawn(move || {
            let mut oracle: Option<Oracle> = None;
            for job in rx {
                let result = eval_on_actor(&mut oracle, job.source, &job.entry, job.sink);
                let _ = job.reply.send(result);
            }
        });
        Self { jobs }
    }

    fn eval(
        &self,
        source: String,
        entry: String,
        sink: vix::oracle::EventSink,
    ) -> Result<String, String> {
        let (reply, rx) = std_mpsc::channel::<Result<String, String>>();
        let job = EvalJob {
            source,
            entry,
            sink,
            reply,
        };
        self.jobs
            .send(job)
            .map_err(|_| "oracle actor stopped".to_string())?;
        rx.recv()
            .map_err(|_| "oracle actor stopped before replying".to_string())?
    }
}

fn eval_on_actor(
    oracle: &mut Option<Oracle>,
    source: String,
    entry: &str,
    sink: vix::oracle::EventSink,
) -> Result<String, String> {
    if let Some(oracle) = oracle {
        oracle.reload(&source)?;
    } else {
        *oracle = Some(Oracle::load(&source)?);
    }
    let oracle = oracle.as_mut().expect("oracle loaded or reloaded");
    oracle.set_sink(Some(sink));
    let args = target_args_for(oracle, entry);
    let result = oracle.call(entry, &args).map(|v| format!("{v:?}"));
    oracle.set_sink(None);
    result
}

/// The daemon owns one warm oracle actor per service instance. Cloned service
/// handles share the same actor sender, so concurrent evals across connections
/// serialize through one dedicated Oracle thread for v1 warmth.
#[derive(Clone)]
pub struct DaemonService {
    actor: OracleActor,
}

impl DaemonService {
    pub fn new() -> Self {
        Self {
            actor: OracleActor::start(),
        }
    }
}

impl Default for DaemonService {
    fn default() -> Self {
        Self::new()
    }
}

impl Daemon for DaemonService {
    async fn eval(&self, req: EvalRequest, mut control: Rx<StepCommand>, events: Tx<DemandEvent>) {
        // The sink pushes each event onto a std channel; when stepping, it also
        // BLOCKS on a per-event permission oneshot. The oracle runs on a
        // blocking thread; this async task bridges to the vox streams.
        let (evt_tx, mut evt_rx) = mpsc::unbounded_channel::<DemandEvent>();
        let (gate_tx, gate_rx) = mpsc::unbounded_channel::<oneshot::Sender<()>>();
        let mode = req.mode;

        let gate_tx_for_oracle = gate_tx.clone();
        let evt_tx_for_oracle = evt_tx.clone();
        let actor = self.actor.clone();

        let oracle_task = tokio::task::spawn_blocking(move || {
            let sink_evt = evt_tx_for_oracle.clone();
            let sink_gate = gate_tx_for_oracle.clone();
            let sink = move |at: u64, event: &Event| {
                let de = DemandEvent::from_oracle(at, event);
                if mode == StepMode::Step {
                    // Ask the async side for permission and block until granted.
                    let (permit_tx, permit_rx) = oneshot::channel::<()>();
                    if sink_gate.send(permit_tx).is_err() {
                        return;
                    }
                    let _ = sink_evt.send(de);
                    // Block the oracle thread until the client steps.
                    let _ = permit_rx.blocking_recv();
                } else {
                    let _ = sink_evt.send(de);
                }
            };
            actor.eval(req.source, req.entry, Box::new(sink))
        });

        // Drop our own sender halves: the oracle thread holds the only clones
        // now, so evt_rx/gate_rx yield None exactly when the oracle finishes.
        // (Holding these across the loop deadlocked v1: recv() never ended.)
        drop(evt_tx);
        drop(gate_tx);

        // Bridge: forward events (gated by client Step/Resume) to the client.
        //
        // Invariants that keep this deadlock-free:
        // - The sink requests a permit for EVERY event while in Step mode (the
        //   mode is baked into the closure), so after Resume the bridge must
        //   keep draining permits and grant them immediately.
        // - At most one permit is ever outstanding (the oracle blocks on it).
        // - A Step that arrives before its permit becomes a CREDIT, not a
        //   dropped message.
        let mut gating = mode == StepMode::Step;
        let mut step_credits: u32 = 0;
        let mut pending_permit: Option<oneshot::Sender<()>> = None;
        let mut gate_rx = gate_rx;

        loop {
            tokio::select! {
                // The oracle wants permission to continue past an event.
                permit = gate_rx.recv() => {
                    if let Some(p) = permit {
                        if !gating || step_credits > 0 {
                            step_credits = step_credits.saturating_sub(1);
                            let _ = p.send(());
                        } else {
                            pending_permit = Some(p);
                        }
                    }
                    // None just means the oracle dropped its gate sender;
                    // evt_rx closing is what ends the loop.
                }
                // The oracle produced an event (emitted before it blocks).
                evt = evt_rx.recv() => {
                    match evt {
                        Some(de) => { let _ = events.send(de).await; }
                        None => break, // oracle finished
                    }
                }
                // Client control (only relevant while gating).
                cmd = control.recv(), if gating => {
                    match cmd {
                        Ok(Some(c)) => match c.get() {
                            StepCommand::Step => match pending_permit.take() {
                                Some(p) => { let _ = p.send(()); }
                                None => step_credits += 1,
                            },
                            StepCommand::Resume => {
                                gating = false;
                                if let Some(p) = pending_permit.take() { let _ = p.send(()); }
                            }
                        },
                        _ => { gating = false; if let Some(p) = pending_permit.take() { let _ = p.send(()); } }
                    }
                }
            }
        }

        // Release a straggler permit, if any.
        if let Some(p) = pending_permit.take() {
            let _ = p.send(());
        }

        let final_event = match oracle_task.await {
            Ok(Ok(result)) => DemandEvent::Done { result },
            Ok(Err(error)) => DemandEvent::Failed { error },
            Err(join) => DemandEvent::Failed {
                error: format!("daemon eval task panicked: {join}"),
            },
        };
        let _ = events.send(final_event).await;
        let _ = evt_tx;
    }
}

/// If `entry` takes a single `Target` parameter, supply a canned linux target.
fn target_args_for(oracle: &Oracle, entry: &str) -> Vec<(&'static str, vix::oracle::Value)> {
    use vix::oracle::Value;
    if oracle.fn_param_is_target(entry) {
        vec![(
            "target",
            Value::Struct {
                name: "Target".into(),
                fields: vec![("os".into(), Value::Str("linux-x86_64".into()))],
            },
        )]
    } else {
        vec![]
    }
}
