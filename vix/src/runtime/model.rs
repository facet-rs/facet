use super::identity::{DemandKey, ValueId};
use super::store::Handle;

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum DemandState {
    Absent,
    Queued,
    Running,
    Ready,
    Failed,
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
