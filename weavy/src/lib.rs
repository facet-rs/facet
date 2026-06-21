//! Shared lowered-program substrate for interpreters and copy-and-patch backends.
//!
//! `weavy` stays format-agnostic: callers bring schema identities, parsers, and
//! value models. The crate provides the shared carrier for lowered programs
//! (flat programs, named blocks, and a small call-stack runner), plus the generic
//! typed-memory descriptor/op vocabulary in [`mem`]. Native copy-and-patch
//! backends can use the same program/block shape.

pub mod mem;

use std::collections::BTreeMap;

/// A flat lowered program for an op vocabulary supplied by the caller.
pub type Program<Op> = Vec<Op>;

/// A root program plus named block programs.
///
/// Recursive shapes are represented by block calls instead of by infinitely
/// inlining the same op tree. `BlockId` is caller-defined: Phon uses schema ids,
/// while JSON-facing Facet code can use shape/type-plan ids.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Lowered<BlockId, Op> {
    /// Entry program.
    pub program: Program<Op>,
    /// Callable block programs.
    pub blocks: BTreeMap<BlockId, Program<Op>>,
}

impl<BlockId, Op> Lowered<BlockId, Op> {
    /// Build a lowered program with no blocks.
    #[must_use]
    pub fn new(program: Program<Op>) -> Self {
        Self {
            program,
            blocks: BTreeMap::new(),
        }
    }
}

/// Interpreter control returned by a caller's op semantics.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Control<'program, BlockId, Op, Continuation = ()> {
    /// Advance to the next op in the current program.
    Continue,
    /// Enter an inline child program.
    CallProgram(&'program [Op]),
    /// Enter an inline child program and notify the stepper when it returns.
    CallProgramThen(&'program [Op], Continuation),
    /// Enter a named block program.
    CallBlock(BlockId),
    /// Enter a named block program and notify the stepper when it returns.
    CallBlockThen(BlockId, Continuation),
    /// Return from the current program.
    Return,
}

/// Errors produced by [`run`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RunError<BlockId, StepError> {
    /// The caller's op semantics failed.
    Step(StepError),
    /// A block call referenced a block that is not present in the lowered plan.
    MissingBlock(BlockId),
}

/// Caller-provided op semantics for [`run`].
pub trait Step<'program, BlockId, Op> {
    /// The caller's error type.
    type Error;
    /// Caller-owned state resumed when a child program returns.
    type Continuation;

    /// Execute one op and report how the program counter should move.
    fn step(
        &mut self,
        op: &'program Op,
    ) -> Result<Control<'program, BlockId, Op, Self::Continuation>, Self::Error>;

    /// Resume after a child program called with a continuation returns.
    fn after_return(
        &mut self,
        _continuation: Self::Continuation,
    ) -> Result<Control<'program, BlockId, Op, Self::Continuation>, Self::Error> {
        Ok(Control::Continue)
    }
}

#[derive(Clone)]
struct Frame<'program, Op, Continuation> {
    program: &'program [Op],
    ip: usize,
    continuation: Option<Continuation>,
}

/// Opt-in execution counters for the generic lowered-program runner.
///
/// These are runtime counts, not shape counts: repeated container bodies and
/// recursive block calls are counted once per actual execution.
#[non_exhaustive]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RunStats {
    /// Number of caller ops stepped.
    pub step_count: usize,
    /// Number of inline child-program frames entered.
    pub inline_call_count: usize,
    /// Number of named block frames entered.
    pub block_call_count: usize,
    /// Number of frames returned, either by falling off the end or by
    /// [`Control::Return`].
    pub return_count: usize,
    /// Number of continuation callbacks resumed after a child frame returned.
    pub continuation_resume_count: usize,
    /// Maximum runner frame depth, including the root frame.
    pub max_frame_depth: usize,
}

impl RunStats {
    fn frame_pushed(&mut self, depth: usize) {
        self.max_frame_depth = self.max_frame_depth.max(depth);
    }

    fn step(&mut self) {
        self.step_count += 1;
    }

    fn inline_call(&mut self) {
        self.inline_call_count += 1;
    }

    fn block_call(&mut self) {
        self.block_call_count += 1;
    }

    fn returned(&mut self) {
        self.return_count += 1;
    }

    fn continuation_resumed(&mut self) {
        self.continuation_resume_count += 1;
    }
}

trait Accounting {
    fn frame_pushed(&mut self, depth: usize);
    fn step(&mut self);
    fn inline_call(&mut self);
    fn block_call(&mut self);
    fn returned(&mut self);
    fn continuation_resumed(&mut self);
}

struct NoAccounting;

impl Accounting for NoAccounting {
    #[inline(always)]
    fn frame_pushed(&mut self, _depth: usize) {}

    #[inline(always)]
    fn step(&mut self) {}

    #[inline(always)]
    fn inline_call(&mut self) {}

    #[inline(always)]
    fn block_call(&mut self) {}

    #[inline(always)]
    fn returned(&mut self) {}

    #[inline(always)]
    fn continuation_resumed(&mut self) {}
}

impl Accounting for RunStats {
    #[inline(always)]
    fn frame_pushed(&mut self, depth: usize) {
        RunStats::frame_pushed(self, depth);
    }

    #[inline(always)]
    fn step(&mut self) {
        RunStats::step(self);
    }

    #[inline(always)]
    fn inline_call(&mut self) {
        RunStats::inline_call(self);
    }

    #[inline(always)]
    fn block_call(&mut self) {
        RunStats::block_call(self);
    }

    #[inline(always)]
    fn returned(&mut self) {
        RunStats::returned(self);
    }

    #[inline(always)]
    fn continuation_resumed(&mut self) {
        RunStats::continuation_resumed(self);
    }
}

/// Run a lowered program through caller-supplied op semantics.
///
/// The runner maintains its own program stack. Calling a block or inline program
/// does not recurse through Rust functions, which is the property Facet JSON
/// deserialization needs for recursive values.
pub fn run<'program, BlockId, Op, S>(
    lowered: &'program Lowered<BlockId, Op>,
    stepper: &mut S,
) -> Result<(), RunError<BlockId, S::Error>>
where
    BlockId: Clone + Ord,
    S: Step<'program, BlockId, Op>,
{
    run_program(&lowered.program, &lowered.blocks, stepper)
}

/// Run a lowered program and return opt-in execution counters.
///
/// The normal [`run`] path uses the same runner with a zero-sized accounting
/// implementation, so callers only pay for these counters when they request
/// them.
pub fn run_with_stats<'program, BlockId, Op, S>(
    lowered: &'program Lowered<BlockId, Op>,
    stepper: &mut S,
) -> Result<RunStats, RunError<BlockId, S::Error>>
where
    BlockId: Clone + Ord,
    S: Step<'program, BlockId, Op>,
{
    run_program_with_stats(&lowered.program, &lowered.blocks, stepper)
}

/// Run a program with an explicit block table.
pub fn run_program<'program, BlockId, Op, S>(
    program: &'program [Op],
    blocks: &'program BTreeMap<BlockId, Program<Op>>,
    stepper: &mut S,
) -> Result<(), RunError<BlockId, S::Error>>
where
    BlockId: Clone + Ord,
    S: Step<'program, BlockId, Op>,
{
    let mut accounting = NoAccounting;
    run_program_accounted(program, blocks, stepper, &mut accounting)
}

/// Run a program with an explicit block table and return execution counters.
pub fn run_program_with_stats<'program, BlockId, Op, S>(
    program: &'program [Op],
    blocks: &'program BTreeMap<BlockId, Program<Op>>,
    stepper: &mut S,
) -> Result<RunStats, RunError<BlockId, S::Error>>
where
    BlockId: Clone + Ord,
    S: Step<'program, BlockId, Op>,
{
    let mut stats = RunStats::default();
    run_program_accounted(program, blocks, stepper, &mut stats)?;
    Ok(stats)
}

fn run_program_accounted<'program, BlockId, Op, S, A>(
    program: &'program [Op],
    blocks: &'program BTreeMap<BlockId, Program<Op>>,
    stepper: &mut S,
    accounting: &mut A,
) -> Result<(), RunError<BlockId, S::Error>>
where
    BlockId: Clone + Ord,
    S: Step<'program, BlockId, Op>,
    A: Accounting,
{
    let mut frames = vec![Frame {
        program,
        ip: 0,
        continuation: None,
    }];
    accounting.frame_pushed(frames.len());

    while let Some(frame) = frames.last_mut() {
        if let Some(op) = frame.program.get(frame.ip) {
            frame.ip += 1;
            accounting.step();
            let control = stepper.step(op).map_err(RunError::Step)?;
            apply_control(control, &mut frames, blocks, stepper, accounting)?;
        } else {
            finish_frame(&mut frames, blocks, stepper, accounting)?;
        }
    }

    Ok(())
}

fn finish_frame<'program, BlockId, Op, S, A>(
    frames: &mut Vec<Frame<'program, Op, S::Continuation>>,
    blocks: &'program BTreeMap<BlockId, Program<Op>>,
    stepper: &mut S,
    accounting: &mut A,
) -> Result<(), RunError<BlockId, S::Error>>
where
    BlockId: Clone + Ord,
    S: Step<'program, BlockId, Op>,
    A: Accounting,
{
    let continuation = frames.pop().and_then(|frame| frame.continuation);
    accounting.returned();
    if let Some(continuation) = continuation {
        accounting.continuation_resumed();
        let control = stepper.after_return(continuation).map_err(RunError::Step)?;
        apply_control(control, frames, blocks, stepper, accounting)?;
    }
    Ok(())
}

fn apply_control<'program, BlockId, Op, S, A>(
    control: Control<'program, BlockId, Op, S::Continuation>,
    frames: &mut Vec<Frame<'program, Op, S::Continuation>>,
    blocks: &'program BTreeMap<BlockId, Program<Op>>,
    stepper: &mut S,
    accounting: &mut A,
) -> Result<(), RunError<BlockId, S::Error>>
where
    BlockId: Clone + Ord,
    S: Step<'program, BlockId, Op>,
    A: Accounting,
{
    let mut control = control;
    loop {
        match control {
            Control::Continue => {}
            Control::CallProgram(program) => {
                accounting.inline_call();
                frames.push(Frame {
                    program,
                    ip: 0,
                    continuation: None,
                });
                accounting.frame_pushed(frames.len());
            }
            Control::CallProgramThen(program, continuation) => {
                accounting.inline_call();
                frames.push(Frame {
                    program,
                    ip: 0,
                    continuation: Some(continuation),
                });
                accounting.frame_pushed(frames.len());
            }
            Control::CallBlock(block) => {
                let program = blocks
                    .get(&block)
                    .ok_or_else(|| RunError::MissingBlock(block.clone()))?;
                accounting.block_call();
                frames.push(Frame {
                    program,
                    ip: 0,
                    continuation: None,
                });
                accounting.frame_pushed(frames.len());
            }
            Control::CallBlockThen(block, continuation) => {
                let program = blocks
                    .get(&block)
                    .ok_or_else(|| RunError::MissingBlock(block.clone()))?;
                accounting.block_call();
                frames.push(Frame {
                    program,
                    ip: 0,
                    continuation: Some(continuation),
                });
                accounting.frame_pushed(frames.len());
            }
            Control::Return => {
                let continuation = frames.pop().and_then(|frame| frame.continuation);
                accounting.returned();
                if let Some(continuation) = continuation {
                    accounting.continuation_resumed();
                    control = stepper.after_return(continuation).map_err(RunError::Step)?;
                    continue;
                }
            }
        }
        return Ok(());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Op {
        Push(u32),
        Call(u32),
        CallThen(u32, u32),
        Nested(Vec<Op>),
        NestedThen(Vec<Op>, u32),
        Stop,
    }

    struct Eval {
        seen: Vec<u32>,
    }

    impl<'program> Step<'program, u32, Op> for Eval {
        type Error = ();
        type Continuation = u32;

        fn step(
            &mut self,
            op: &'program Op,
        ) -> Result<Control<'program, u32, Op, Self::Continuation>, Self::Error> {
            Ok(match op {
                Op::Push(n) => {
                    self.seen.push(*n);
                    Control::Continue
                }
                Op::Call(block) => Control::CallBlock(*block),
                Op::CallThen(block, tag) => Control::CallBlockThen(*block, *tag),
                Op::Nested(program) => Control::CallProgram(program),
                Op::NestedThen(program, tag) => Control::CallProgramThen(program, *tag),
                Op::Stop => Control::Return,
            })
        }

        fn after_return(
            &mut self,
            tag: Self::Continuation,
        ) -> Result<Control<'program, u32, Op, Self::Continuation>, Self::Error> {
            self.seen.push(tag);
            Ok(Control::Continue)
        }
    }

    #[test]
    fn run_uses_explicit_program_stack() {
        let lowered = Lowered {
            program: vec![
                Op::Push(1),
                Op::Call(7),
                Op::Nested(vec![Op::Push(3), Op::Stop, Op::Push(99)]),
                Op::Push(4),
            ],
            blocks: BTreeMap::from([(7, vec![Op::Push(2)])]),
        };
        let mut eval = Eval { seen: Vec::new() };

        run(&lowered, &mut eval).unwrap();

        assert_eq!(eval.seen, vec![1, 2, 3, 4]);
    }

    #[test]
    fn run_with_stats_reports_runner_activity() {
        let lowered = Lowered {
            program: vec![
                Op::Push(1),
                Op::Call(7),
                Op::NestedThen(vec![Op::Push(3)], 30),
                Op::Push(4),
            ],
            blocks: BTreeMap::from([(7, vec![Op::Push(2)])]),
        };
        let mut eval = Eval { seen: Vec::new() };

        let stats = run_with_stats(&lowered, &mut eval).unwrap();

        assert_eq!(eval.seen, vec![1, 2, 3, 30, 4]);
        assert_eq!(
            stats,
            RunStats {
                step_count: 6,
                inline_call_count: 1,
                block_call_count: 1,
                return_count: 3,
                continuation_resume_count: 1,
                max_frame_depth: 2,
            }
        );
    }

    #[test]
    fn run_resumes_continuations_after_child_programs() {
        let lowered = Lowered {
            program: vec![
                Op::Push(1),
                Op::NestedThen(vec![Op::Push(2)], 20),
                Op::CallThen(7, 70),
                Op::Push(4),
            ],
            blocks: BTreeMap::from([(7, vec![Op::Push(3)])]),
        };
        let mut eval = Eval { seen: Vec::new() };

        run(&lowered, &mut eval).unwrap();

        assert_eq!(eval.seen, vec![1, 2, 20, 3, 70, 4]);
    }
}
