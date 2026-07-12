use crate::support::Span;
use crate::vir::{FunctionId, NodeId};

use super::identity::{DemandKey, RecipeId, ValueId};
use super::store::Handle;

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum DemandState {
    Absent,
    Queued,
    Running,
    Ready,
    Failed,
    MachineFailed,
}

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum TaskState {
    Runnable,
    Running,
    Parked,
    Completed,
    Discarded,
    Failed,
}

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TaskId(pub u64);

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct DemandRecord {
    pub key: DemandKey,
    pub state: DemandState,
    pub result: Option<Handle>,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct TaskRecord {
    pub id: TaskId,
    pub demand: DemandKey,
    pub state: TaskState,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum FailureValue {
    // r[impl machine.error.failure-source-site-identity]
    IndexOutOfBounds {
        recipe: RecipeId,
        site: u32,
        index: i64,
        length: i64,
        subject: Option<ValueId>,
    },
    MissingKey {
        recipe: RecipeId,
        site: u32,
    },
    DuplicateKey {
        recipe: RecipeId,
        site: u32,
    },
    MissingDelimiter {
        recipe: RecipeId,
        site: u32,
    },
    InvalidInteger {
        recipe: RecipeId,
        site: u32,
    },
    IntegerOverflow {
        recipe: RecipeId,
        site: u32,
    },
    DivisionByZero {
        recipe: RecipeId,
        site: u32,
    },
}

/// Context rebuilt while reporting a language failure. It is deliberately not
/// resident in the store or memo identity: source spans and demand chains are
/// properties of this compilation and demand, not of the failure value.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct FailureContext {
    pub function: FunctionId,
    pub node: NodeId,
    pub span: Span,
    pub demand_chain: Vec<DemandKey>,
}

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum MemoVerdict {
    Miss,
    Exact,
    Projection,
    Semantic,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct ReadWitness {
    pub source: ValueId,
    pub projection: String,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct Receipt {
    pub demand: DemandKey,
    pub reads: Vec<ReadWitness>,
}
