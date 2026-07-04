//! The vix daemon: a vox service that owns demand-driven evaluation.
//!
//! # Why a daemon
//!
//! The IDE should not embed the evaluator; it should TALK to one. The daemon
//! owns the machine (soon: with a fleet of executors), and the IDE is a
//! generated vox client. Because vox generates typed clients from
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

use std::collections::BTreeMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc as std_mpsc;

use facet::Facet;
use tokio::sync::mpsc;
use vix::exec::ExecEvent;
use vix::fetch::{FetchBackend, NoFetchBackend};
use vix::machine::driver::{
    DriveEvent, StepCommand as MachineStepCommand, StepMode as MachineStepMode,
};
use vix::machine::lower::Machine;
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

/// Counts for one completed eval generation.
#[derive(Facet, Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EvalSummary {
    pub generation: u64,
    pub hits: u64,
    pub misses: u64,
    pub created: u64,
    pub scheduled: u64,
    pub finished: u64,
}

impl EvalSummary {
    fn new(generation: u64) -> Self {
        Self {
            generation,
            ..Self::default()
        }
    }

    fn observe(&mut self, event: &DemandEvent) {
        match event {
            DemandEvent::Miss { .. } => self.misses += 1,
            DemandEvent::Hit { .. } => self.hits += 1,
            DemandEvent::Created { .. } => self.created += 1,
            DemandEvent::Scheduled { .. } => self.scheduled += 1,
            DemandEvent::Finished { .. } => self.finished += 1,
            DemandEvent::Observation { .. }
            | DemandEvent::Summary { .. }
            | DemandEvent::Done { .. }
            | DemandEvent::Failed { .. } => {}
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
        generation: u64,
        at: u64,
        func: String,
        span: Span,
        caller: Option<String>,
        args: Vec<(String, String)>,
    },
    /// A memoized call was served from the language-level cache.
    Hit {
        generation: u64,
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
        generation: u64,
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
        generation: u64,
        at: u64,
        command: String,
        run: u64,
        span: Span,
    },
    /// EXECUTION FINISHED: resolved through the two-tier exec cache;
    /// `outputs` is the produced tree — artifacts, path by path.
    Finished {
        generation: u64,
        at: u64,
        command: String,
        run: u64,
        span: Span,
        serving: Serving,
        outputs: Vec<(String, String)>,
    },
    /// A primitive observed the world (cold) or replayed its pin.
    Observation {
        generation: u64,
        at: u64,
        key: String,
        replayed: bool,
    },
    /// Evaluation completed; counts cover this generation's streamed events.
    Summary { summary: EvalSummary },
    /// Evaluation finished. `result` is the value's Debug form (v1).
    Done { generation: u64, result: String },
    /// Evaluation failed.
    Failed { generation: u64, error: String },
}

vox_schema::impl_reborrow_owned!(DemandEvent, StepCommand);

/// The daemon's RPC surface. The IDE gets a generated client for this.
#[vox::service]
pub trait Daemon {
    /// Evaluate `req.entry` of `req.source`, streaming a [`DemandEvent`] per
    /// step into `events`. In [`StepMode::Step`], each event is GATED: the
    /// daemon blocks until the client sends [`StepCommand::Step`] on `control`
    /// (or `Resume` to stop gating). The final event is `Done`/`Failed`.
    async fn eval(&self, req: EvalRequest, control: Rx<StepCommand>, events: Tx<DemandEvent>);
}

struct EvalWork {
    source: String,
    entry: String,
    mode: MachineStepMode,
    generation: u64,
    events: mpsc::UnboundedSender<DemandEvent>,
    commands: std_mpsc::Receiver<StepCommand>,
}

struct EvalJob {
    work: EvalWork,
    reply: std_mpsc::Sender<Result<String, String>>,
}

#[derive(Clone)]
struct MachineActor {
    jobs: std_mpsc::Sender<EvalJob>,
}

impl MachineActor {
    fn start(fetch_backend: Arc<dyn FetchBackend>) -> Self {
        let (jobs, rx) = std_mpsc::channel::<EvalJob>();
        std::thread::spawn(move || {
            let mut machine: Option<Machine> = None;
            for job in rx {
                let result = eval_on_actor(&mut machine, fetch_backend.clone(), job.work);
                let _ = job.reply.send(result);
            }
        });
        Self { jobs }
    }

    fn eval(
        &self,
        source: String,
        entry: String,
        mode: MachineStepMode,
        generation: u64,
        events: mpsc::UnboundedSender<DemandEvent>,
        commands: std_mpsc::Receiver<StepCommand>,
    ) -> Result<String, String> {
        let (reply, rx) = std_mpsc::channel::<Result<String, String>>();
        let job = EvalJob {
            work: EvalWork {
                source,
                entry,
                mode,
                generation,
                events,
                commands,
            },
            reply,
        };
        self.jobs
            .send(job)
            .map_err(|_| "machine actor stopped".to_string())?;
        rx.recv()
            .map_err(|_| "machine actor stopped before replying".to_string())?
    }
}

fn eval_on_actor(
    machine: &mut Option<Machine>,
    fetch_backend: Arc<dyn FetchBackend>,
    work: EvalWork,
) -> Result<String, String> {
    let EvalWork {
        source,
        entry,
        mode,
        generation,
        events,
        commands,
    } = work;

    if let Some(machine) = machine {
        machine.reload(&source)?;
    } else {
        *machine = Some(Machine::load(&source)?.with_fetch_backend_arc(fetch_backend));
    }
    let machine = machine.as_mut().expect("machine loaded or reloaded");
    machine.clear_trace();
    machine.set_step_mode(mode);

    let mut adapter = MachineEventAdapter::new(
        generation,
        &source,
        machine,
        events,
        commands,
        mode == MachineStepMode::Step,
    )?;
    machine.set_event_sink(Some(Box::new(move |event| adapter.handle(event))));

    let result = target_args_for(machine, &entry)
        .and_then(|args| machine.demand_i64(&entry, args))
        .and_then(|word| machine.render_result(&entry, word))
        .map(|value| format!("{value:?}"));

    machine.set_event_sink(None);
    machine.set_step_mode(MachineStepMode::Run);
    result
}

struct MachineEventAdapter {
    generation: u64,
    events: mpsc::UnboundedSender<DemandEvent>,
    commands: std_mpsc::Receiver<StepCommand>,
    hash_names: BTreeMap<u64, String>,
    fn_spans: BTreeMap<String, Span>,
    run_spans: BTreeMap<u64, Span>,
    synthetic_at: u64,
    gating: bool,
}

impl MachineEventAdapter {
    fn new(
        generation: u64,
        source: &str,
        machine: &Machine,
        events: mpsc::UnboundedSender<DemandEvent>,
        commands: std_mpsc::Receiver<StepCommand>,
        gating: bool,
    ) -> Result<Self, String> {
        Ok(Self {
            generation,
            events,
            commands,
            hash_names: machine
                .fn_hashes()
                .into_iter()
                .map(|(name, hash)| (hash, name))
                .collect(),
            fn_spans: function_spans(source)?,
            run_spans: BTreeMap::new(),
            synthetic_at: 0,
            gating,
        })
    }

    fn handle(&mut self, event: &DriveEvent) -> MachineStepCommand {
        if let Some(event) = self.convert(event) {
            if self.events.send(event).is_err() {
                self.gating = false;
                MachineStepCommand::Resume
            } else {
                self.wait_if_gated()
            }
        } else {
            MachineStepCommand::Step
        }
    }

    fn convert(&mut self, event: &DriveEvent) -> Option<DemandEvent> {
        match event {
            DriveEvent::MemoHit { fn_hash } => {
                let func = self.func_name(*fn_hash);
                Some(DemandEvent::Hit {
                    generation: self.generation,
                    at: self.next_synthetic_at(),
                    span: self.func_span(&func),
                    caller: None,
                    args: Vec::new(),
                    func,
                })
            }
            DriveEvent::Spawned { fn_hash } => {
                let func = self.func_name(*fn_hash);
                Some(DemandEvent::Miss {
                    generation: self.generation,
                    at: self.next_synthetic_at(),
                    span: self.func_span(&func),
                    caller: None,
                    args: Vec::new(),
                    func,
                })
            }
            DriveEvent::RunRequested {
                run_id,
                command_name,
                argv,
                describe,
                span,
                timestamp_us,
                ..
            } => {
                let span = span_from_option(*span);
                self.run_spans.insert(*run_id, span);
                Some(DemandEvent::Created {
                    generation: self.generation,
                    at: *timestamp_us,
                    command: command_name.clone(),
                    run: *run_id,
                    span,
                    in_fn: None,
                    argv: argv.clone(),
                    describe: describe.clone(),
                })
            }
            DriveEvent::RunStarted {
                run_id,
                command_name,
                timestamp_us,
                ..
            } => Some(DemandEvent::Scheduled {
                generation: self.generation,
                at: *timestamp_us,
                command: command_name.clone(),
                run: *run_id,
                span: self.run_span(*run_id),
            }),
            DriveEvent::RunCompleted {
                run_id,
                command_name,
                serving,
                outputs,
                timestamp_us,
                ..
            } => Some(DemandEvent::Finished {
                generation: self.generation,
                at: *timestamp_us,
                command: command_name.clone(),
                run: *run_id,
                span: self.run_span(*run_id),
                serving: serving.clone().into(),
                outputs: outputs.clone(),
            }),
            DriveEvent::Observation {
                replayed,
                key_text,
                timestamp_us,
                ..
            } => Some(DemandEvent::Observation {
                generation: self.generation,
                at: *timestamp_us,
                key: key_text.clone(),
                replayed: *replayed,
            }),
            DriveEvent::Demanded { .. }
            | DriveEvent::ParkedOn { .. }
            | DriveEvent::Completed { .. }
            | DriveEvent::SpawnedInvocation { .. }
            | DriveEvent::StoreAlloc { .. } => None,
        }
    }

    fn wait_if_gated(&mut self) -> MachineStepCommand {
        if !self.gating {
            return MachineStepCommand::Resume;
        }
        match self.commands.recv() {
            Ok(StepCommand::Step) => MachineStepCommand::Step,
            Ok(StepCommand::Resume) | Err(_) => {
                self.gating = false;
                MachineStepCommand::Resume
            }
        }
    }

    fn func_name(&self, hash: u64) -> String {
        self.hash_names
            .get(&hash)
            .cloned()
            .unwrap_or_else(|| format!("fn#{hash:016x}"))
    }

    fn func_span(&self, func: &str) -> Span {
        self.fn_spans.get(func).copied().unwrap_or_else(empty_span)
    }

    fn run_span(&self, run_id: u64) -> Span {
        self.run_spans
            .get(&run_id)
            .copied()
            .unwrap_or_else(empty_span)
    }

    fn next_synthetic_at(&mut self) -> u64 {
        let at = self.synthetic_at;
        self.synthetic_at = self.synthetic_at.saturating_add(1);
        at
    }
}

fn function_spans(source: &str) -> Result<BTreeMap<String, Span>, String> {
    let parser = vix::VixParser::new();
    let file: vix::ast::SourceFile = parser.parse(source).map_err(|error| error.message)?;
    Ok(file
        .items
        .iter()
        .filter_map(|item| match item {
            vix::ast::Item::Fn(function) => {
                Some((function.name.value.clone(), function.span.into()))
            }
            vix::ast::Item::Enum(_) | vix::ast::Item::Struct(_) | vix::ast::Item::Use(_) => None,
        })
        .collect())
}

fn span_from_option(span: Option<(u32, u32)>) -> Span {
    match span {
        Some((start, end)) => Span { start, end },
        None => empty_span(),
    }
}

fn empty_span() -> Span {
    Span { start: 0, end: 0 }
}

/// The daemon owns one warm machine actor per service instance. Cloned service
/// handles share the same actor sender, so concurrent evals across connections
/// serialize through one dedicated machine thread for v1 warmth.
#[derive(Clone)]
pub struct DaemonService {
    actor: MachineActor,
    next_generation: Arc<AtomicU64>,
}

impl DaemonService {
    pub fn new() -> Self {
        Self::with_fetch_backend(NoFetchBackend)
    }

    pub fn with_fetch_backend(backend: impl FetchBackend + 'static) -> Self {
        Self {
            actor: MachineActor::start(Arc::new(backend)),
            next_generation: Arc::new(AtomicU64::new(1)),
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
        let generation = self.next_generation.fetch_add(1, Ordering::Relaxed);
        let (evt_tx, mut evt_rx) = mpsc::unbounded_channel::<DemandEvent>();
        let mode = req.mode;
        let machine_mode = match mode {
            StepMode::Run => MachineStepMode::Run,
            StepMode::Step => MachineStepMode::Step,
        };
        let (command_tx, command_rx) = std_mpsc::channel::<StepCommand>();
        let disconnect_command_tx = command_tx.clone();
        let actor = self.actor.clone();

        let control_task = tokio::spawn(async move {
            if mode == StepMode::Step {
                while let Ok(Some(command)) = control.recv().await {
                    let command = *command.get();
                    let _ = command_tx.send(command);
                    if command == StepCommand::Resume {
                        return;
                    }
                }
            }
            let _ = command_tx.send(StepCommand::Resume);
        });

        let machine_task = tokio::task::spawn_blocking(move || {
            actor.eval(
                req.source,
                req.entry,
                machine_mode,
                generation,
                evt_tx,
                command_rx,
            )
        });

        let mut summary = EvalSummary::new(generation);

        while let Some(de) = evt_rx.recv().await {
            summary.observe(&de);
            if events.send(de).await.is_err() {
                let _ = disconnect_command_tx.send(StepCommand::Resume);
                break;
            }
        }

        let _ = events.send(DemandEvent::Summary { summary }).await;
        let final_event = match machine_task.await {
            Ok(Ok(result)) => DemandEvent::Done { generation, result },
            Ok(Err(error)) => DemandEvent::Failed { generation, error },
            Err(join) => DemandEvent::Failed {
                generation,
                error: format!("daemon eval task panicked: {join}"),
            },
        };
        let _ = events.send(final_event).await;
        control_task.abort();
    }
}

/// If `entry` takes a single `Target` parameter, supply a canned linux target.
fn target_args_for(machine: &Machine, entry: &str) -> Result<Vec<i64>, String> {
    match machine.entry_param_schemas(entry) {
        Some([]) => Ok(Vec::new()),
        Some([schema]) if schema == "Target" => Ok(vec![machine.linux_target_handle()]),
        Some(params) => Err(format!(
            "entry `{entry}` has unsupported machine daemon params {params:?}"
        )),
        None => Err(format!("no function named {entry}")),
    }
}
