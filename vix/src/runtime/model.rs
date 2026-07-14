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
    /// A tree projection named a segment the tree does not contain.
    MissingTreeEntry {
        recipe: RecipeId,
        site: u32,
    },
    /// `.text()` (or a deeper projection) demanded a File where the entry is a
    /// directory or symlink — or projected through a non-directory.
    TreeEntryNotAFile {
        recipe: RecipeId,
        site: u32,
    },
    /// A file's bytes were demanded as text but are not valid UTF-8.
    InvalidText {
        recipe: RecipeId,
        site: u32,
    },
    /// The registry manifest has no row for the requested artifact name.
    MissingRegistryArtifact {
        recipe: RecipeId,
        site: u32,
    },
    /// Fetched bytes do not hash to the pinned vix ContentHash.
    ///
    /// r[impl machine.primitive.fetch-integrity-vs-identity]
    FetchIntegrity {
        recipe: RecipeId,
        site: u32,
    },
    /// An archive Blob could not be extracted as a well-formed ustar tree.
    MalformedArchive {
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

/// A structural snapshot captured by an `expect_snapshot` check.
///
/// The rendering is produced by a type-directed walk of the value's structural
/// result — never a `Debug` impl — so it is a stable harness artifact keyed by
/// `name`, byte-identical across the plain and chaos lanes and across the native
/// and interpreter execution lanes. The `outcome` is the check's verdict against
/// the harness snapshot oracle: the check passes only when it is [`Matched`].
///
/// [`Matched`]: SnapshotOutcome::Matched
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct SnapshotCapture {
    pub name: String,
    /// The actual structural rendering. Empty exactly when the render faulted.
    pub rendered: String,
    pub outcome: SnapshotOutcome,
}

impl SnapshotCapture {
    /// A snapshot check passes only when the rendering matched its oracle golden.
    #[must_use]
    pub fn passed(&self) -> bool {
        matches!(self.outcome, SnapshotOutcome::Matched)
    }
}

/// The verdict of one snapshot check against the harness snapshot oracle. Every
/// non-`Matched` outcome is a red check carrying the typed context needed to
/// report it — the actual rendering lives on [`SnapshotCapture::rendered`].
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SnapshotOutcome {
    /// The rendering equals the oracle's expected golden for this name.
    Matched,
    /// The oracle had a golden for this name, but it differs from the rendering.
    Mismatch { expected: String },
    /// No golden exists for this test + name. A snapshot with no expectation is a
    /// red check, never a silent pass (the future disk loader records it instead).
    MissingExpected,
    /// A second snapshot reused a name already emitted in this test run.
    DuplicateName,
    /// The value published but could not be rendered structurally: a machine
    /// invariant, attributed to this site rather than aborting the whole run.
    RenderFault { detail: String },
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
