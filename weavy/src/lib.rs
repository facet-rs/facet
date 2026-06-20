//! Shared lowered-program substrate for interpreters and copy-and-patch backends.
//!
//! `weavy` deliberately knows nothing about schemas, parsers, memory layouts, or
//! value models. A caller brings its own op enum and semantics; `weavy` provides
//! the common carrier: flat programs, named blocks for recursive plans, and a
//! small call-stack runner that keeps program execution out of the Rust call
//! stack. Native copy-and-patch backends can use the same program/block shape.

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
pub enum Control<'program, BlockId, Op> {
    /// Advance to the next op in the current program.
    Continue,
    /// Enter an inline child program.
    CallProgram(&'program [Op]),
    /// Enter a named block program.
    CallBlock(BlockId),
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

    /// Execute one op and report how the program counter should move.
    fn step(&mut self, op: &'program Op) -> Result<Control<'program, BlockId, Op>, Self::Error>;
}

#[derive(Clone, Copy)]
struct Frame<'program, Op> {
    program: &'program [Op],
    ip: usize,
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
    let mut frames = vec![Frame { program, ip: 0 }];

    while let Some(frame) = frames.last_mut() {
        let Some(op) = frame.program.get(frame.ip) else {
            frames.pop();
            continue;
        };
        frame.ip += 1;

        match stepper.step(op).map_err(RunError::Step)? {
            Control::Continue => {}
            Control::CallProgram(program) => frames.push(Frame { program, ip: 0 }),
            Control::CallBlock(block) => {
                let program = blocks
                    .get(&block)
                    .ok_or_else(|| RunError::MissingBlock(block.clone()))?;
                frames.push(Frame { program, ip: 0 });
            }
            Control::Return => {
                frames.pop();
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Debug, PartialEq, Eq)]
    enum Op {
        Push(u32),
        Call(u32),
        Nested(Vec<Op>),
        Stop,
    }

    struct Eval {
        seen: Vec<u32>,
    }

    impl<'program> Step<'program, u32, Op> for Eval {
        type Error = ();

        fn step(&mut self, op: &'program Op) -> Result<Control<'program, u32, Op>, Self::Error> {
            Ok(match op {
                Op::Push(n) => {
                    self.seen.push(*n);
                    Control::Continue
                }
                Op::Call(block) => Control::CallBlock(*block),
                Op::Nested(program) => Control::CallProgram(program),
                Op::Stop => Control::Return,
            })
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
}
