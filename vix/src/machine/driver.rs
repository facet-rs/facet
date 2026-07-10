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

use std::cell::{Cell, RefCell};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::fmt;
use std::hash::{BuildHasher, Hash, Hasher};
#[cfg(any(test, feature = "jit"))]
use std::rc::Rc;
use std::sync::{Arc, mpsc};
use std::time::Duration;

use facet::Facet;
use taxon::{Kind, Primitive, SchemaRef};
#[cfg(any(test, feature = "jit"))]
use weavy::jit::task_lane::{JitProgram, JitTask};
use weavy::mem::{Access, MapStorage, Presence, SequenceStorage, Tag};
#[cfg(any(test, feature = "jit"))]
use weavy::task::Op;
use weavy::task::{FnId, HostFn, Program, Task, TaskStep, ValueMemories, ValueMemory};

use crate::ast;
use crate::fetch::{FetchBackend, NoFetchBackend};
use crate::module::{DescriptorMap, SchemaTables, VixDescriptor};
use crate::support::{PathMissing, assign_roles, subtree, tool_for, tree_text};
use crate::value::{Payload, Value};

#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct StoreValue {
    pub handle: u64,
    pub schema: String,
    pub tier: HandleTier,
    pub bytes: Vec<u8>,
    pub content_hash: Vec<u8>,
    pub taint: Option<StructuralTaint>,
}

#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct CodeRef {
    pub module_hash: Vec<u8>,
    pub closure_hash: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct CodeBundle {
    pub module_hash: Vec<u8>,
    pub bytes: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct ValueBundle {
    pub root: u64,
    pub values: Vec<StoreValue>,
    pub code: Vec<CodeBundle>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StoreHandle(pub i64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Facet)]
#[repr(u8)]
pub enum HandleTier {
    #[default]
    Ready,
    Pending,
}

impl HandleTier {
    fn is_pending(self) -> bool {
        matches!(self, Self::Pending)
    }
}

#[derive(Clone, Debug)]
pub struct MachineExecRequest {
    pub command: String,
    pub plan: crate::exec::ExecPlan,
    pub capability: u64,
    pub mounts: Vec<crate::exec::Mount>,
    pub output: String,
    pub span: Option<(u32, u32)>,
    pub observer: Option<ValueBundle>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MachinePathDemand {
    File(String),
    FinishRequired { path: String },
    Missing { path: String },
}

pub trait MachinePendingRun: Send + Sync {
    fn demand_path(&self, path: &str) -> Result<MachinePathDemand, String>;
    fn flush(&self) -> Result<(crate::exec::Outcome, crate::exec::ExecEvent), String>;
}

pub trait MachineExecBackend: Send + Sync {
    fn spawn(&self, request: MachineExecRequest) -> Result<Arc<dyn MachinePendingRun>, String>;
}

/// INVOKE request frame contract (the lowering and this driver's
/// shared knowledge): at `INVOKE_REGION` the body lays out
/// [input_slot, fn_ref, argc, arg0, arg1, ...] as i64 words before
/// HostCall(INVOKE_HOST). The region is ordinary frame space —
/// spill-rule-resident like everything else.
pub const INVOKE_HOST: u32 = 0;
pub const STORE_ALLOC_HOST: u32 = 1;
pub const STORE_READ_HOST: u32 = 2;
pub const STORE_TAG_HOST: u32 = 3;
pub const MAP_EMPTY_HOST: u32 = 4;
pub const MAP_INSERT_HOST: u32 = 5;
pub const MAP_GET_HOST: u32 = 6;
pub const OPTION_UNWRAP_HOST: u32 = 7;
pub const ACQUIRE_HOST: u32 = 8;
pub const ARRAY_ALLOC_HOST: u32 = 9;
pub const ARRAY_MAP_PENDING_HOST: u32 = 10;
pub const ARRAY_COLLECT_HOST: u32 = 11;
pub const TREE_PROJECT_HOST: u32 = 12;
pub const EXEC_HOST: u32 = 13;
pub const PATH_WITH_EXT_HOST: u32 = 14;
pub const PENDING_ALLOC_HOST: u32 = 15;
pub const PENDING_COERCE_HOST: u32 = 16;
pub const PENDING_INVOKE_HOST: u32 = 17;
pub const FETCH_HOST: u32 = 18;
pub const ARRAY_FILTER_EXCLUDE_HOST: u32 = 19;
pub const GLOB_HOST: u32 = 20;
pub const DOC_PARSE_HOST: u32 = 21;
pub const DOC_GET_HOST: u32 = 22;
pub const DOC_COERCE_HOST: u32 = 23;
pub const ELF_DOC_HOST: u32 = 24;
pub const AST_DOC_HOST: u32 = 25;
pub const AST_FN_HOST: u32 = 26;
pub const ARRAY_LEN_HOST: u32 = 27;
pub const OCI_DOC_HOST: u32 = 28;
pub const STRING_CONCAT_HOST: u32 = 29;
pub const ARRAY_JOIN_HOST: u32 = 30;
pub const DOC_PACKAGE_HOST: u32 = 31;
pub const TARGET_HOST: u32 = 32;
pub const VERSION_PARSE_HOST: u32 = 33;
pub const VALUE_COMPARE_HOST: u32 = 34;
pub const STRING_UPPER_HOST: u32 = 35;
pub const STRING_LOWER_HOST: u32 = 36;
pub const STRING_DEFAULT_HOST: u32 = 37;
pub const SEALED_SEAL_HOST: u32 = 38;
pub const SEALED_DECLASSIFY_HOST: u32 = 39;
pub const SEALED_TO_STRING_HOST: u32 = 40;
pub const VERSION_SET_PARSE_HOST: u32 = 41;
pub const VERSION_SET_OP_HOST: u32 = 42;
pub const MOLTEN_INTERN_HOST: u32 = 43;
pub const ARRAY_PUSH_HOST: u32 = 44;
pub const ARRAY_POP_HOST: u32 = 45;
pub const ARRAY_SET_HOST: u32 = 46;
pub const ARRAY_GET_HOST: u32 = 47;
pub const MOLTEN_DUP_HOST: u32 = 48;
pub const RECORD_UPDATE_HOST: u32 = 49;
pub const CRATE_ARCHIVE_HOST: u32 = 50;
pub const VERSION_FIELD_HOST: u32 = 51;
pub const OPTION_CONSTRUCT_HOST: u32 = 52;
pub const OPTION_DESTRUCT_HOST: u32 = 53;
pub const STRING_SPLIT_HOST: u32 = 54;
pub const STRING_PARSE_INT_HOST: u32 = 55;
pub const STRING_CONTAINS_HOST: u32 = 56;
pub const STRING_IS_NUMERIC_HOST: u32 = 57;
pub const PATH_JOIN_HOST: u32 = 58;
pub const PATH_TO_STRING_HOST: u32 = 59;
pub const DOC_IS_MAP_HOST: u32 = 60;
pub const TREE_TEXT_HOST: u32 = 61;
pub const DOC_KEYS_HOST: u32 = 62;
pub const NATIVE_OPTION_UNWRAP_NONE_HOST: u32 = 63;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lane {
    #[default]
    Interp,
    #[cfg(any(test, feature = "jit"))]
    Jit,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum StepMode {
    Run,
    Step,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum StepCommand {
    Step,
    Resume,
}

pub type DriveEventSink = Box<dyn FnMut(&DriveEvent) -> StepCommand>;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct FnRef(usize);

impl FnRef {
    pub(super) fn new(index: usize) -> Self {
        Self(index)
    }

    fn from_frame_word(word: i64) -> Self {
        Self(word as usize)
    }

    pub(super) fn index(self) -> usize {
        self.0
    }
}

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
    /// Schema refs for entry args, in arg order. Scalars hash their
    /// word bytes; handles hash the target store entry's canonical
    /// content.
    pub arg_schemas: Vec<String>,
    /// Schema ref for the returned word. Float returns canonicalize at
    /// the memo boundary.
    pub return_schema: String,
    /// Semantic memo comparators, one per declared argument verifier.
    pub semantic_comparators: Vec<SemanticComparator>,
    /// Byte offset of this function's INVOKE region.
    pub invoke_region: u32,
    /// Byte offset of this function's STORE_ALLOC region:
    /// [dst_slot, type_ref, variant_index, field_count, fields...].
    pub store_alloc_region: u32,
    /// Byte offset of this function's STORE_READ region:
    /// [dst_slot, handle, field_index].
    pub store_read_region: u32,
    /// Byte offset of this function's STORE_TAG region: [dst_slot, handle].
    pub store_tag_region: u32,
    /// Byte offset of miscellaneous primitive host region.
    pub primitive_region: u32,
}

#[derive(Clone, Debug)]
pub struct SemanticComparator {
    pub arg_index: usize,
    fn_ref: FnRef,
}

impl SemanticComparator {
    pub(super) fn new(arg_index: usize, fn_ref: FnRef) -> Self {
        Self { arg_index, fn_ref }
    }

    pub(super) fn fn_ref(&self) -> FnRef {
        self.fn_ref
    }
}

/// Driver-level events (join the unified trace via lowering-emitted
/// marks later; recorded directly in this slice).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DriveEvent {
    /// Demand arrived for (fn hash, memo-key hash of args).
    Demanded { fn_hash: u64 },
    /// Served from memo — NO task existed.
    MemoHit { fn_hash: u64 },
    /// Coarse memo key missed, but all observed projections from a
    /// prior run verified against the new composite arguments.
    MemoProjectionHit { fn_hash: u64, verified: usize },
    /// Coarse memo key missed, but declared semantic comparators
    /// accepted the changed arguments.
    MemoSemanticHit { fn_hash: u64, verified: usize },
    /// Spawned a task (memo miss).
    Spawned { fn_hash: u64 },
    /// A task parked awaiting another invocation's result.
    ParkedOn { fn_hash: u64 },
    /// An invocation completed and fed its awaiters.
    Completed { fn_hash: u64 },
    /// A concrete invocation identity spawned. `key_hash` is the
    /// canonical memo-key hash, including canonicalized args.
    SpawnedInvocation { fn_hash: u64, key_hash: u64 },
    /// A value-store allocation occurred. `deduped` means the store
    /// returned an existing handle for canonical content.
    StoreAlloc { schema_ref: u64, deduped: bool },
    RunRequested {
        command: u64,
        output: u64,
        run_id: u64,
        command_name: String,
        capability_key: String,
        argv: Vec<String>,
        describe: Vec<String>,
        span: Option<(u32, u32)>,
        timestamp_us: u64,
    },
    RunStarted {
        command: u64,
        output: u64,
        run_id: u64,
        command_name: String,
        timestamp_us: u64,
    },
    RunCompleted {
        command: u64,
        output: u64,
        run_id: u64,
        command_name: String,
        serving: crate::exec::ExecEvent,
        outputs: Vec<(String, String)>,
        timestamp_us: u64,
    },
    Observation {
        key: u64,
        replayed: bool,
        key_text: String,
        timestamp_us: u64,
    },
    ArtifactProbe {
        format: String,
        projection: String,
        input: u64,
        cache_hit: bool,
        timestamp_us: u64,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MoltenStats {
    pub array_push_reused: usize,
    pub array_push_copied: usize,
}

#[derive(Clone, Debug, PartialEq, Facet)]
pub struct RenderedField {
    pub name: String,
    pub schema: String,
    pub value: RenderedValue,
}

#[derive(Clone, Debug, PartialEq, Facet)]
pub struct RenderedMapEntry {
    pub key_schema: String,
    pub key: RenderedValue,
    pub value_schema: String,
    pub realization: Option<String>,
    pub value: RenderedValue,
}

#[derive(Clone, Debug, PartialEq, Facet)]
pub struct RenderedTreeEntry {
    pub path: String,
    pub contents: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct RenderedPending {
    pub schema: String,
    pub closure_hash: String,
    pub identity_hash: String,
    pub remaining_arity: u64,
    pub arg_count: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct RenderedTreePending {
    pub kind: String,
    pub identity_hash: String,
    pub pending: Vec<RenderedPending>,
    pub run_id: Option<u64>,
}

#[derive(Clone, Debug, PartialEq, Facet)]
#[repr(u8)]
#[facet(tag = "type")]
pub enum RenderedValue {
    Int {
        value: i64,
    },
    Float {
        bits: String,
        value: String,
    },
    Bool {
        value: bool,
    },
    String {
        value: String,
    },
    Path {
        value: String,
    },
    Flag {
        value: String,
    },
    Version {
        value: String,
    },
    VersionSet {
        value: String,
    },
    Raw {
        schema: String,
        bytes_utf8: Option<String>,
    },
    Tuple {
        schema: String,
        fields: Vec<RenderedField>,
    },
    Record {
        schema: String,
        fields: Vec<RenderedField>,
    },
    Enum {
        schema: String,
        variant_index: u64,
        variant: String,
        fields: Vec<RenderedField>,
    },
    Array {
        element_schema: String,
        items: Vec<RenderedValue>,
    },
    Map {
        schema: String,
        entries: Vec<RenderedMapEntry>,
    },
    Tree {
        entries: Vec<RenderedTreeEntry>,
    },
    TreePending {
        pending: RenderedTreePending,
    },
    Pending {
        pending: RenderedPending,
    },
    Doc {
        variant: String,
        value: Option<Box<RenderedValue>>,
    },
    Sealed {
        taint: String,
        recipient: String,
        identity_hash: String,
        content_tag: Option<String>,
    },
}

#[derive(Clone, Debug, Default)]
pub struct RenderNames {
    pub structs: BTreeMap<String, Vec<String>>,
    pub enums: BTreeMap<String, Vec<RenderVariant>>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RenderVariant {
    pub name: String,
    pub fields: Vec<String>,
}

/// A pending invocation request captured by the INVOKE host during a
/// task burst (applied by the driver after the burst returns).
#[derive(Clone, Debug)]
struct InvokeRequest {
    caller: usize,
    input_slot: usize,
    fn_ref: FnRef,
    args: Vec<i64>,
}

#[derive(Clone, Debug)]
struct ProjectRequest {
    input_slot: usize,
    tree: i64,
    path: i64,
}

#[derive(Clone, Debug)]
struct TextProjectRequest {
    input_slot: usize,
    tree: i64,
    path: i64,
}

#[derive(Clone, Debug)]
enum TreeProjectRequest {
    Tree(ProjectRequest),
    Text(TextProjectRequest),
}

#[derive(Clone, Debug)]
struct ExecRequest {
    input_slot: usize,
    command: String,
    capability: i64,
    parts: Vec<CommandRequestPart>,
    span: Option<(u32, u32)>,
}

#[derive(Clone, Debug)]
enum CommandRequestPart {
    Token(i64),
    Splice(i64),
}

#[derive(Clone, Debug)]
struct FetchRequest {
    input_slot: usize,
    url: i64,
    sha256: i64,
}

#[derive(Clone, Debug)]
struct DocParseRequest {
    input_slot: usize,
    kind: DocParseKind,
    input: i64,
    target_schema: Option<String>,
}

#[derive(Clone, Debug)]
struct CrateArchiveRequest {
    input_slot: usize,
    input: i64,
}

#[derive(Clone, Copy, Debug)]
enum DocParseKind {
    Toml,
    Json,
    BuildDirectives,
    Cfg,
    RustcCfg,
}

#[derive(Clone, Debug)]
struct OptionUnwrapRequest {
    input_slot: usize,
    realization_slot: Option<usize>,
    option: i64,
}

#[derive(Clone, Debug)]
struct PendingCoerceRequest {
    input_slot: usize,
    pending: i64,
}

#[derive(Clone, Debug)]
struct PendingInvokeRequest {
    caller: usize,
    input_slot: usize,
    pending: i64,
    args: Vec<i64>,
}

/// A running or parked task execution.
struct Execution {
    task: LaneTask,
    fn_ref: FnRef,
    key: CanonMemoKey,
    args: Vec<i64>,
    molten: MoltenStore,
    read_set: ProjectionReadSet,
    ready: Vec<bool>,
    awaited: Vec<i64>,
    /// input slot → the invocation key feeding it (for wiring
    /// completions).
    feeds: HashMap<usize, CanonMemoKey>,
}

enum LaneRuntime {
    Interp,
    #[cfg(any(test, feature = "jit"))]
    Jit {
        program: Rc<JitProgram>,
    },
}

impl LaneRuntime {
    fn new(lane: Lane, _program: &Program) -> Result<Self, String> {
        match lane {
            Lane::Interp => Ok(Self::Interp),
            #[cfg(any(test, feature = "jit"))]
            Lane::Jit => {
                let Some(program) = compile_jit_program(_program) else {
                    return Err(format!(
                        "weavy JIT task lane could not compile program; ops: {}",
                        program_op_set(_program)
                    ));
                };
                Ok(Self::Jit {
                    program: Rc::new(program),
                })
            }
        }
    }

    fn spawn(
        &self,
        program: &Program,
        lowered: &LoweredFn,
        args: &[i64],
    ) -> Result<LaneTask, String> {
        match self {
            Self::Interp => {
                let mut task = Task::spawn(program, lowered.task_fn);
                for (offset, value) in lowered.arg_offsets.iter().zip(args) {
                    task.write_i64(*offset, *value);
                }
                Ok(LaneTask::Interp(task))
            }
            #[cfg(any(test, feature = "jit"))]
            Self::Jit { program: jit } => {
                let mut task = JitTask::spawn(jit.as_ref(), lowered.task_fn);
                for (offset, value) in lowered.arg_offsets.iter().zip(args) {
                    task.write_i64(*offset, *value);
                }
                Ok(LaneTask::Jit {
                    program: Rc::clone(jit),
                    task,
                })
            }
        }
    }
}

enum LaneTask {
    Interp(Task),
    #[cfg(any(test, feature = "jit"))]
    Jit {
        program: Rc<JitProgram>,
        task: JitTask,
    },
}

impl LaneTask {
    fn advance(
        &mut self,
        program: &Program,
        ready: &mut [bool],
        awaited: &[i64],
        hosts: &mut [HostFn<'_>],
        value_memories: ValueMemories<'_>,
    ) -> TaskStep {
        match self {
            Self::Interp(task) => {
                task.run_hosted_with_value_memories(program, ready, awaited, hosts, value_memories)
            }
            #[cfg(any(test, feature = "jit"))]
            Self::Jit { program, task } => task.run_hosted_with_value_memories(
                program.as_ref(),
                ready,
                awaited,
                hosts,
                value_memories,
            ),
        }
    }

    fn result_i64(&self) -> i64 {
        match self {
            Self::Interp(task) => task.result_i64(),
            #[cfg(any(test, feature = "jit"))]
            Self::Jit { task, .. } => task.result_i64(),
        }
    }
}

#[cfg(any(test, feature = "jit"))]
fn compile_jit_program(program: &Program) -> Option<JitProgram> {
    #[cfg(test)]
    JIT_PROGRAM_COMPILE_COUNT.with(|count| count.set(count.get() + 1));
    JitProgram::compile(program)
}

#[cfg(test)]
thread_local! {
    static JIT_PROGRAM_COMPILE_COUNT: std::cell::Cell<usize> = const { std::cell::Cell::new(0) };
}

#[cfg(test)]
fn reset_jit_program_compile_count() {
    JIT_PROGRAM_COMPILE_COUNT.with(|count| count.set(0));
}

#[cfg(test)]
fn jit_program_compile_count() -> usize {
    JIT_PROGRAM_COMPILE_COUNT.with(std::cell::Cell::get)
}

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ContentHash([u8; 32]);

impl ContentHash {
    fn from_slice(bytes: &[u8]) -> Result<Self, usize> {
        bytes
            .try_into()
            .map(Self)
            .map_err(|_: std::array::TryFromSliceError| bytes.len())
    }

    fn to_vec(self) -> Vec<u8> {
        self.0.to_vec()
    }
}

impl AsRef<[u8]> for ContentHash {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<[u8; 32]> for ContentHash {
    fn from(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }
}

type IdentityHashMap<K, V> = HashMap<K, V, IdentityBuildHasher>;
type IdentityHashSet<K> = HashSet<K, IdentityBuildHasher>;

#[derive(Clone, Default)]
struct IdentityBuildHasher;

#[derive(Default)]
struct IdentityHasher {
    value: u64,
}

impl BuildHasher for IdentityBuildHasher {
    type Hasher = IdentityHasher;

    fn build_hasher(&self) -> Self::Hasher {
        IdentityHasher::default()
    }
}

impl Hasher for IdentityHasher {
    fn finish(&self) -> u64 {
        self.value
    }

    fn write(&mut self, bytes: &[u8]) {
        for chunk in bytes.chunks(8) {
            let mut word = [0u8; 8];
            word[..chunk.len()].copy_from_slice(chunk);
            self.write_u64(u64::from_le_bytes(word));
        }
    }

    fn write_u8(&mut self, value: u8) {
        self.write_u64(u64::from(value));
    }

    fn write_u16(&mut self, value: u16) {
        self.write_u64(u64::from(value));
    }

    fn write_u32(&mut self, value: u32) {
        self.write_u64(u64::from(value));
    }

    fn write_u64(&mut self, value: u64) {
        self.value = self
            .value
            .rotate_left(13)
            .wrapping_mul(0x9e37_79b1_85eb_ca87)
            ^ value;
    }

    fn write_usize(&mut self, value: usize) {
        self.write_u64(value as u64);
    }
}

impl fmt::Display for ContentHash {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

fn finish_hash(hasher: blake3::Hasher) -> ContentHash {
    ContentHash(*hasher.finalize().as_bytes())
}

fn update_hash_len(hasher: &mut blake3::Hasher, len: usize) {
    hasher.update(
        &i64::try_from(len)
            .expect("hash input length fits i64")
            .to_le_bytes(),
    );
}

fn update_schema_name(hasher: &mut blake3::Hasher, schemas: &SchemaTables, name: &str) {
    update_schema_ref(hasher, &schemas.legacy_ref(name));
}

fn update_schema_ref(hasher: &mut blake3::Hasher, schema_ref: &SchemaRef) {
    match schema_ref {
        SchemaRef::Concrete { id, args } => {
            hasher.update(&id.as_u64().to_le_bytes());
            update_hash_len(hasher, args.len());
            for arg in args {
                update_schema_ref(hasher, arg);
            }
        }
        SchemaRef::Var { name } => {
            hasher.update(
                &crate::module::legacy_marker_schema_id(name)
                    .as_u64()
                    .to_le_bytes(),
            );
            update_hash_len(hasher, 0);
        }
    }
}

fn start_array_element_hasher(
    domain: &'static [u8],
    schema_tables: &SchemaTables,
    elem_schema: &str,
) -> CarriedArrayHasher {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    update_schema_name(&mut hasher, schema_tables, elem_schema);
    CarriedArrayHasher { hasher }
}

fn update_array_element_hash(hasher: &mut CarriedArrayHasher, element_hash: ContentHash) {
    hasher.hasher.update(element_hash.as_ref());
}

fn recompute_array_element_hasher(
    store: &ValueStore,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
    domain: &'static [u8],
    elem_schema: &str,
    words: &[i64],
) -> CarriedArrayHasher {
    let mut carried = start_array_element_hasher(domain, schema_tables, elem_schema);
    for word in words {
        update_array_element_hash(
            &mut carried,
            canonical_word_hash_in_store(store, schemas, elem_schema, *word),
        );
    }
    carried
}

fn finish_array_element_hash(mut carried: CarriedArrayHasher, len: usize) -> ContentHash {
    update_hash_len(&mut carried.hasher, len);
    finish_hash(carried.hasher)
}

fn short_hash_bytes(domain: &[u8], bytes: &[u8]) -> u64 {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    hasher.update(bytes);
    let hash = hasher.finalize();
    u64::from_le_bytes(hash.as_bytes()[..8].try_into().expect("blake3 prefix"))
}

fn hash_u64_debug(value: impl fmt::Debug) -> u64 {
    short_hash_bytes(b"vix-debug-u64", format!("{value:?}").as_bytes())
}

type CanonMemoKey = (u64, Vec<ContentHash>);
type ProjectionCandidateKey = (u64, Vec<ProjectionArgKey>);
#[cfg(test)]
pub type MapWordRow = (String, i64, String, i64, Option<i64>);

#[derive(Default)]
struct InFlightInvocations {
    keys: IdentityHashSet<CanonMemoKey>,
}

impl InFlightInvocations {
    fn is_running(&self, key: &CanonMemoKey) -> bool {
        self.keys.contains(key)
    }

    fn started(&mut self, key: CanonMemoKey) -> bool {
        self.keys.insert(key)
    }

    fn finished(&mut self, key: &CanonMemoKey) -> bool {
        self.keys.remove(key)
    }
}

#[derive(Clone, Debug)]
struct MemoEntry {
    value: i64,
    args: Vec<i64>,
    read_set: ProjectionReadSet,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum ProjectionArgKey {
    Exact(ContentHash),
    Projectable(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MemoHitKind {
    Exact,
    Projection,
    Semantic,
}

#[derive(Clone, Debug, Default, PartialEq, Eq)]
struct ProjectionReadSet {
    entries: Vec<ProjectionRead>,
}

impl ProjectionReadSet {
    fn record(&mut self, read: ProjectionRead) {
        if let Some(existing) = self
            .entries
            .iter_mut()
            .find(|entry| entry.arg_index == read.arg_index && entry.path == read.path)
        {
            *existing = read;
        } else {
            self.entries.push(read);
        }
    }

    fn extend(&mut self, other: &ProjectionReadSet) {
        for read in &other.entries {
            self.record(read.clone());
        }
    }

    fn len(&self) -> usize {
        self.entries.len()
    }

    fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProjectionRead {
    arg_index: usize,
    path: ProjectionPath,
    observed: ContentHash,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum ProjectionPath {
    Whole {
        schema: String,
    },
    Field {
        schema: String,
        field_index: usize,
    },
    Tag {
        schema: String,
    },
    MapGet {
        map_schema: String,
        key_schema: String,
        key_hash: ContentHash,
        value_schema: String,
    },
    TreePath {
        path: String,
    },
    DocGet {
        key: String,
    },
    Elf {
        projection: String,
    },
    Ast {
        projection: String,
        name: Option<String>,
    },
    Oci {
        projection: String,
    },
}

#[derive(Clone, Debug)]
pub struct StoreEntry {
    pub schema: String,
    pub tier: HandleTier,
    pub bytes: Vec<u8>,
    pub content_hash: ContentHash,
    pub taint: Option<StructuralTaint>,
}

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Facet)]
pub struct StructuralTaint {
    pub marker: String,
    pub recipient: String,
    pub identity_hash: Vec<u8>,
    pub content_tag: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct SealedPayload {
    ciphertext: Vec<u8>,
    taint: StructuralTaint,
}

#[derive(Debug, Default)]
pub struct ValueStore {
    entries: Vec<StoreEntry>,
    by_content: IdentityHashMap<(String, HandleTier, ContentHash), i64>,
    decoded_map_rows: IdentityHashMap<DecodedMapCacheKey, DecodedMapRows>,
    map_intern_counters: MapInternCounters,
}

impl Clone for ValueStore {
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
            by_content: self.by_content.clone(),
            decoded_map_rows: IdentityHashMap::default(),
            map_intern_counters: self.map_intern_counters.clone(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct MoltenHandle(usize);

impl MoltenHandle {
    fn index(self) -> usize {
        self.0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct StoreIx(usize);

impl StoreIx {
    fn to_word(self) -> i64 {
        Handle::Store(self).to_word()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum Handle {
    Molten(MoltenHandle),
    Store(StoreIx),
}

impl Handle {
    fn from_word(word: i64) -> Self {
        if word < 0 {
            Self::Molten(MoltenHandle(
                usize::try_from(-1 - word).expect("molten handle index"),
            ))
        } else {
            Self::Store(StoreIx(usize::try_from(word).expect("store handle index")))
        }
    }

    fn to_word(self) -> i64 {
        match self {
            Self::Molten(MoltenHandle(index)) => {
                -1 - i64::try_from(index).expect("molten handle fits i64")
            }
            Self::Store(StoreIx(index)) => i64::try_from(index).expect("store handle fits i64"),
        }
    }
}

#[derive(Clone, Debug)]
struct MapPair {
    key_schema: String,
    key_word: i64,
    value_schema: String,
    value_word: i64,
    value_realization: Option<Realization>,
}

#[derive(Clone, Debug)]
struct OrderedMapPair {
    pair: MapPair,
    key_hash: ContentHash,
    value_hash: ContentHash,
    key_taint: Option<StructuralTaint>,
    value_taint: Option<StructuralTaint>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct DecodedMapCacheKey {
    handle: i64,
    content_hash: ContentHash,
}

#[derive(Clone, Debug)]
struct DecodedMapRow {
    pair: MapPair,
    key_hash: ContentHash,
    value_hash: ContentHash,
    key_taint: Option<StructuralTaint>,
    value_taint: Option<StructuralTaint>,
}

#[derive(Clone, Debug)]
struct DecodedMapRows {
    schema: String,
    rows: Vec<DecodedMapRow>,
}

#[derive(Clone)]
struct CarriedArrayHasher {
    hasher: blake3::Hasher,
}

#[derive(Clone, Debug)]
struct CarriedMapRows {
    ordered: Vec<OrderedMapPair>,
}

impl fmt::Debug for CarriedArrayHasher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CarriedArrayHasher(..)")
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct MapInternStats {
    pub rows_canonicalized: usize,
    pub child_identity_reads: usize,
    pub sort_rows: usize,
    pub sort_comparisons: usize,
    pub hash_rows: usize,
}

#[derive(Debug, Default)]
struct MapInternCounters {
    rows_canonicalized: Cell<usize>,
    child_identity_reads: Cell<usize>,
    sort_rows: Cell<usize>,
    sort_comparisons: Cell<usize>,
    hash_rows: Cell<usize>,
}

impl Clone for MapInternCounters {
    fn clone(&self) -> Self {
        let cloned = Self::default();
        cloned.add(self.snapshot());
        cloned
    }
}

impl MapInternCounters {
    fn add(&self, stats: MapInternStats) {
        self.rows_canonicalized.set(
            self.rows_canonicalized
                .get()
                .saturating_add(stats.rows_canonicalized),
        );
        self.child_identity_reads.set(
            self.child_identity_reads
                .get()
                .saturating_add(stats.child_identity_reads),
        );
        self.sort_rows
            .set(self.sort_rows.get().saturating_add(stats.sort_rows));
        self.sort_comparisons.set(
            self.sort_comparisons
                .get()
                .saturating_add(stats.sort_comparisons),
        );
        self.hash_rows
            .set(self.hash_rows.get().saturating_add(stats.hash_rows));
    }

    fn snapshot(&self) -> MapInternStats {
        MapInternStats {
            rows_canonicalized: self.rows_canonicalized.get(),
            child_identity_reads: self.child_identity_reads.get(),
            sort_rows: self.sort_rows.get(),
            sort_comparisons: self.sort_comparisons.get(),
            hash_rows: self.hash_rows.get(),
        }
    }
}

#[derive(Clone, Debug)]
struct PendingInvocation {
    closure_hash: u64,
    primitive: Option<PendingPrimitive>,
    args: Vec<i64>,
    remaining_arity: usize,
    identity_hash: ContentHash,
}

#[derive(Clone, Debug)]
struct PendingPrimitive {
    kind: PendingPrimitiveKind,
}

#[derive(Clone, Debug)]
enum PendingPrimitiveKind {
    Elf(super::elf::Projection),
    Ast(super::ast_probe::Projection),
    Oci(super::oci::Projection),
}

impl PendingPrimitive {
    fn to_word(&self) -> i64 {
        match self.kind {
            PendingPrimitiveKind::Elf(projection) => projection.to_word(),
            PendingPrimitiveKind::Ast(projection) => projection.to_word(),
            PendingPrimitiveKind::Oci(projection) => 2_000 + projection.to_word(),
        }
    }

    fn from_word(word: i64) -> Result<Self, String> {
        let kind = if word >= 2_000 {
            PendingPrimitiveKind::Oci(super::oci::Projection::from_word(word - 2_000)?)
        } else if word >= 1_000 {
            PendingPrimitiveKind::Ast(super::ast_probe::Projection::from_word(word)?)
        } else {
            PendingPrimitiveKind::Elf(super::elf::Projection::from_word(word)?)
        };
        Ok(Self { kind })
    }
}

#[derive(Clone, Debug)]
enum ArrayEntry {
    Words {
        elem_schema: String,
        words: Vec<i64>,
    },
    Pending {
        elem_schema: String,
        pending: Vec<i64>,
    },
}

#[derive(Clone, Debug, Default)]
struct MoltenStore {
    entries: Vec<MoltenEntry>,
}

#[derive(Clone, Debug)]
struct MoltenEntry {
    refs: usize,
    value: MoltenValue,
    carried_array_hash: Option<CarriedArrayHasher>,
    carried_map_rows: Option<CarriedMapRows>,
}

#[derive(Clone, Debug)]
enum MoltenValue {
    Record {
        schema: String,
        bytes: Vec<u8>,
    },
    Map {
        schema: String,
        pairs: Vec<MapPair>,
    },
    ArrayWords {
        elem_schema: String,
        words: Vec<i64>,
    },
    Interned(i64),
    Interning,
}

impl MoltenStore {
    fn alloc(&mut self, value: MoltenValue) -> i64 {
        self.alloc_with_carried_state(value, None, None)
    }

    fn alloc_with_carried_map_rows(
        &mut self,
        value: MoltenValue,
        carried_map_rows: Option<CarriedMapRows>,
    ) -> i64 {
        self.alloc_with_carried_state(value, None, carried_map_rows)
    }

    fn alloc_with_carried_state(
        &mut self,
        value: MoltenValue,
        carried_array_hash: Option<CarriedArrayHasher>,
        carried_map_rows: Option<CarriedMapRows>,
    ) -> i64 {
        let handle = MoltenHandle(self.entries.len());
        self.entries.push(MoltenEntry {
            refs: 1,
            value,
            carried_array_hash,
            carried_map_rows,
        });
        Handle::Molten(handle).to_word()
    }

    fn entry(&self, handle: MoltenHandle) -> Option<&MoltenEntry> {
        self.entries.get(handle.index())
    }

    fn entry_mut(&mut self, handle: MoltenHandle) -> Option<&mut MoltenEntry> {
        self.entries.get_mut(handle.index())
    }

    fn array_entry(
        &self,
        store: &ValueStore,
        handle: i64,
        schema_tables: &SchemaTables,
    ) -> Result<ArrayEntry, String> {
        match Handle::from_word(handle) {
            Handle::Molten(molten_handle) => {
                match self.entry(molten_handle).map(|entry| &entry.value) {
                    Some(MoltenValue::ArrayWords { elem_schema, words }) => Ok(ArrayEntry::Words {
                        elem_schema: elem_schema.clone(),
                        words: words.clone(),
                    }),
                    Some(MoltenValue::Interned(handle)) => {
                        store.array_entry(*handle, schema_tables)
                    }
                    Some(_) => Err(format!("molten handle {handle} is not an Array")),
                    None => store.array_entry(handle, schema_tables),
                }
            }
            Handle::Store(store_ix) => store.array_entry(store_ix.to_word(), schema_tables),
        }
    }

    fn debug_counts(&self) -> (usize, usize, usize, usize, usize, usize) {
        let mut array_words = 0usize;
        let mut carried_hashes = 0usize;
        let mut array_entries = 0usize;
        let mut refs_gt_one = 0usize;
        let mut max_refs = 0usize;
        for entry in &self.entries {
            if let MoltenValue::ArrayWords { words, .. } = &entry.value {
                array_entries += 1;
                array_words += words.len();
            }
            carried_hashes += usize::from(entry.carried_array_hash.is_some());
            refs_gt_one += usize::from(entry.refs > 1);
            max_refs = max_refs.max(entry.refs);
        }
        (
            self.entries.len(),
            array_words,
            carried_hashes,
            array_entries,
            refs_gt_one,
            max_refs,
        )
    }
}

#[derive(Clone, Debug)]
enum TreeEntry {
    Concrete(crate::exec::Tree),
    Merge(Vec<i64>),
    Exec(u64),
}

struct PendingExecRun {
    command: String,
    plan: crate::exec::ExecPlan,
    capability: u64,
    mounts: Vec<ExecMount>,
    output: String,
    scheduled: bool,
    completed: Option<(crate::exec::Outcome, crate::exec::ExecEvent)>,
    completion_logged: bool,
    remote: Option<Arc<dyn MachinePendingRun>>,
    completion:
        Option<mpsc::Receiver<Result<(crate::exec::Outcome, crate::exec::ExecEvent), String>>>,
    span: Option<(u32, u32)>,
}

struct PendingFetchRun {
    key: String,
    replayed: bool,
    completion: mpsc::Receiver<Result<crate::fetch::FetchOutput, String>>,
}

struct PreparedFetchRequest {
    input_slot: usize,
    key: String,
    url: String,
    expected_sha256: Option<String>,
    replayed: bool,
}

#[derive(Default)]
struct PendingWork {
    run_waiters: BTreeMap<u64, Vec<(usize, TreeProjectRequest)>>,
    fetch_waiters: BTreeMap<u64, Vec<(usize, usize)>>,
    pending_fetches: BTreeMap<u64, PendingFetchRun>,
    in_flight_fetches: HashMap<String, u64>,
    next_fetch_run_id: u64,
}

#[derive(Default)]
struct RunnableExecutions {
    stack: Vec<usize>,
    present: BTreeSet<usize>,
}

impl RunnableExecutions {
    fn push(&mut self, execution: usize) {
        if self.present.insert(execution) {
            self.stack.push(execution);
        }
    }

    fn pop(&mut self) -> Option<usize> {
        let execution = self.stack.pop()?;
        let removed = self.present.remove(&execution);
        debug_assert!(removed, "runnable stack and membership diverged");
        Some(execution)
    }

    fn extend(&mut self, executions: impl IntoIterator<Item = usize>) {
        for execution in executions {
            self.push(execution);
        }
    }
}

#[derive(Clone, Debug)]
enum ExecMount {
    Concrete(crate::exec::Mount),
    PendingTree { at: String, tree: i64 },
}

#[derive(Clone, Copy, Debug)]
enum Realization {
    Ready,
    Pending,
}

impl Realization {
    fn from_word(word: i64) -> Result<Self, String> {
        match word {
            0 => Ok(Self::Ready),
            1 => Ok(Self::Pending),
            other => Err(format!("unknown realization bit {other}")),
        }
    }

    fn to_word(self) -> i64 {
        match self {
            Self::Ready => 0,
            Self::Pending => 1,
        }
    }

    fn bit(self) -> u64 {
        match self {
            Self::Ready => 0,
            Self::Pending => 1,
        }
    }
}

fn resolved_map_get_value(
    pair: MapPair,
    value_schema: &str,
) -> Option<(String, i64, Option<Realization>)> {
    if pair.value_schema == value_schema {
        return Some((pair.value_schema, pair.value_word, pair.value_realization));
    }
    if let Some(inner) = realized_value_schema(value_schema) {
        if let Some(realization) = pair.value_realization {
            match realization {
                Realization::Ready if pair.value_schema == inner => {
                    return Some((
                        value_schema.to_string(),
                        pair.value_word,
                        Some(Realization::Ready),
                    ));
                }
                Realization::Pending if pair.value_schema == pending_schema(inner) => {
                    return Some((
                        value_schema.to_string(),
                        pair.value_word,
                        Some(Realization::Pending),
                    ));
                }
                _ => {}
            }
        }
        if pair.value_schema == inner && pair.value_realization.is_none() {
            return Some((
                value_schema.to_string(),
                pair.value_word,
                Some(Realization::Ready),
            ));
        }
        if pair.value_schema == pending_schema(inner) && pair.value_realization.is_none() {
            return Some((
                value_schema.to_string(),
                pair.value_word,
                Some(Realization::Pending),
            ));
        }
    }
    if pair.value_schema == pending_schema(value_schema) {
        return Some((pair.value_schema, pair.value_word, pair.value_realization));
    }
    None
}

fn option_none_content_hash(value_schema: &str, schema_tables: &SchemaTables) -> ContentHash {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vix-option");
    update_schema_name(&mut hasher, schema_tables, &option_schema(value_schema));
    hasher.update(&0i64.to_le_bytes());
    finish_hash(hasher)
}

#[derive(Clone, Debug)]
enum OptionPayload {
    None,
    Some {
        word: i64,
        realization: Option<Realization>,
    },
}

impl ValueStore {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn entry(&self, handle: i64) -> Option<&StoreEntry> {
        usize::try_from(handle)
            .ok()
            .and_then(|index| self.entries.get(index))
    }

    fn entries(&self) -> Vec<StoreEntry> {
        self.entries.clone()
    }

    fn insert_at(&mut self, handle: usize, entry: StoreEntry) -> Result<(), String> {
        let key = (entry.schema.clone(), entry.tier, entry.content_hash);
        if let Some(existing) = self.entries.get(handle) {
            if existing.schema == entry.schema
                && existing.tier == entry.tier
                && existing.bytes == entry.bytes
                && existing.content_hash == entry.content_hash
                && existing.taint == entry.taint
            {
                self.by_content.insert(key, handle as i64);
                return Ok(());
            }
            return Err(format!(
                "store handle {handle} already holds `{}`, cannot import `{}`",
                existing.schema, entry.schema
            ));
        }
        if handle != self.entries.len() {
            return Err(format!(
                "cannot import sparse store handle {handle}; next handle is {}",
                self.entries.len()
            ));
        }
        self.entries.push(entry);
        self.by_content.insert(key, handle as i64);
        Ok(())
    }

    fn alloc(
        &mut self,
        schema: &str,
        bytes: Vec<u8>,
        descriptors: &DescriptorMap,
        schemas: &SchemaTables,
    ) -> (i64, bool) {
        let descriptor = descriptors
            .get(schema)
            .unwrap_or_else(|| panic!("descriptor for schema `{schema}`"));
        assert_canonical_zero_padding(schema, descriptor, &bytes);
        let taint = taint_for_value_bytes(self, descriptor, &bytes);
        let content_hash = if descriptor_supports_flat_identity(descriptor) {
            raw_value_content_hash(schema, &bytes, schemas, &taint)
        } else {
            hash_with_taint(
                hash_value_bytes(descriptors, schemas, descriptor, &bytes, self),
                &taint,
            )
        };
        let key = (schema.to_string(), HandleTier::Ready, content_hash);
        if let Some(handle) = self.by_content.get(&key).copied() {
            return (handle, true);
        }
        let handle = i64::try_from(self.entries.len()).expect("store handle fits i64");
        self.entries.push(StoreEntry {
            schema: schema.to_string(),
            tier: HandleTier::Ready,
            bytes,
            content_hash,
            taint,
        });
        self.by_content.insert(key, handle);
        (handle, false)
    }

    fn alloc_raw(&mut self, schema: &str, bytes: Vec<u8>, schemas: &SchemaTables) -> (i64, bool) {
        self.alloc_raw_tainted(schema, bytes, schemas, None)
    }

    fn alloc_raw_tainted(
        &mut self,
        schema: &str,
        bytes: Vec<u8>,
        schemas: &SchemaTables,
        taint: Option<StructuralTaint>,
    ) -> (i64, bool) {
        let content_hash = raw_value_content_hash(schema, &bytes, schemas, &taint);
        let key = (schema.to_string(), HandleTier::Ready, content_hash);
        if let Some(handle) = self.by_content.get(&key).copied() {
            return (handle, true);
        }
        let handle = i64::try_from(self.entries.len()).expect("store handle fits i64");
        self.entries.push(StoreEntry {
            schema: schema.to_string(),
            tier: HandleTier::Ready,
            bytes,
            content_hash,
            taint,
        });
        self.by_content.insert(key, handle);
        (handle, false)
    }

    fn alloc_map(
        &mut self,
        schema: &str,
        pairs: Vec<MapPair>,
        schema_tables: &SchemaTables,
        descriptors: &DescriptorMap,
        schemas: &SchemaTables,
    ) -> Result<(i64, bool), String> {
        let ordered = canonical_map_pairs(self, pairs, descriptors, schemas, schema_tables)?;
        self.alloc_map_with_ordered(schema, ordered, schema_tables)
    }

    fn alloc_map_with_carried_rows(
        &mut self,
        schema: &str,
        pairs: Vec<MapPair>,
        carried_rows: Option<CarriedMapRows>,
        schema_tables: &SchemaTables,
        descriptors: &DescriptorMap,
        schemas: &SchemaTables,
    ) -> Result<(i64, bool), String> {
        let ordered = match carried_rows {
            Some(carried) => carried.ordered,
            None => canonical_map_pairs(self, pairs, descriptors, schemas, schema_tables)?,
        };
        self.alloc_map_with_ordered(schema, ordered, schema_tables)
    }

    fn alloc_map_with_ordered(
        &mut self,
        schema: &str,
        ordered: Vec<OrderedMapPair>,
        schema_tables: &SchemaTables,
    ) -> Result<(i64, bool), String> {
        let bytes = encode_map_pairs(&ordered, schema_tables)?;
        let taint = taint_for_ordered_map_pairs(&ordered);
        self.map_intern_counters.add(MapInternStats {
            hash_rows: ordered.len(),
            ..MapInternStats::default()
        });
        let content_hash = hash_with_taint(hash_map_pairs(schema, &ordered, schema_tables), &taint);
        Ok(self.alloc_with_hash_tainted(schema, bytes, content_hash, taint))
    }

    fn map_intern_stats(&self) -> MapInternStats {
        self.map_intern_counters.snapshot()
    }

    fn map_pairs(
        &self,
        handle: i64,
        schemas: &SchemaTables,
        schema_tables: &SchemaTables,
    ) -> Result<(String, Vec<MapPair>), String> {
        let entry = self
            .entry(handle)
            .ok_or_else(|| format!("store handle {handle}"))?;
        if !schemas.is_map(&entry.schema) {
            return Err(format!("handle {handle} is `{}`, not a Map", entry.schema));
        }
        Ok((
            entry.schema.clone(),
            decode_map_pairs(&entry.bytes, schema_tables)?,
        ))
    }

    fn decoded_map_rows(
        &mut self,
        handle: i64,
        schemas: &SchemaTables,
        schema_tables: &SchemaTables,
    ) -> Result<&DecodedMapRows, String> {
        let key = {
            let entry = self
                .entry(handle)
                .ok_or_else(|| format!("store handle {handle}"))?;
            if !schemas.is_map(&entry.schema) {
                return Err(format!("handle {handle} is `{}`, not a Map", entry.schema));
            }
            DecodedMapCacheKey {
                handle,
                content_hash: entry.content_hash,
            }
        };
        if !self.decoded_map_rows.contains_key(&key) {
            let entry = self
                .entry(handle)
                .ok_or_else(|| format!("store handle {handle}"))?;
            let schema = entry.schema.clone();
            let bytes = entry.bytes.clone();
            let decoded_pairs = decode_map_pairs(&bytes, schema_tables)?;
            self.map_intern_counters.add(MapInternStats {
                child_identity_reads: decoded_pairs.len().saturating_mul(2),
                ..MapInternStats::default()
            });
            let rows = decoded_pairs
                .into_iter()
                .map(|pair| {
                    let key_hash = canonical_word_hash_in_store(
                        self,
                        schemas,
                        &pair.key_schema,
                        pair.key_word,
                    );
                    let value_hash = canonical_word_hash_in_store(
                        self,
                        schemas,
                        &pair.value_schema,
                        pair.value_word,
                    );
                    let key_taint = taint_for_word(self, pair.key_word);
                    let value_taint = taint_for_word(self, pair.value_word);
                    DecodedMapRow {
                        pair,
                        key_hash,
                        value_hash,
                        key_taint,
                        value_taint,
                    }
                })
                .collect();
            self.decoded_map_rows
                .insert(key, DecodedMapRows { schema, rows });
        }
        self.decoded_map_rows
            .get(&key)
            .ok_or_else(|| "decoded map cache miss after insert".to_string())
    }

    fn map_get(
        &mut self,
        handle: i64,
        key_schema: &str,
        key_word: i64,
        value_schema: &str,
        schemas: &SchemaTables,
        schema_tables: &SchemaTables,
    ) -> Result<(i64, bool), String> {
        let key_hash = canonical_word_hash_in_store(self, schemas, key_schema, key_word);
        self.map_get_by_key_hash(
            handle,
            key_schema,
            key_hash,
            value_schema,
            schemas,
            schema_tables,
        )
    }

    fn map_get_by_key_hash(
        &mut self,
        handle: i64,
        key_schema: &str,
        key_hash: ContentHash,
        value_schema: &str,
        schemas: &SchemaTables,
        schema_tables: &SchemaTables,
    ) -> Result<(i64, bool), String> {
        let pair = self
            .decoded_map_rows(handle, schemas, schema_tables)?
            .rows
            .iter()
            .find(|row| row.pair.key_schema == key_schema && row.key_hash == key_hash)
            .map(|row| row.pair.clone());
        let Some(pair) = pair else {
            return self.alloc_option_none(value_schema, schema_tables);
        };
        self.alloc_map_get_some(pair, value_schema, schemas, schema_tables)
    }

    fn map_get_option_hash_by_key_hash(
        &mut self,
        handle: i64,
        key_schema: &str,
        key_hash: ContentHash,
        value_schema: &str,
        schemas: &SchemaTables,
        schema_tables: &SchemaTables,
    ) -> Result<ContentHash, String> {
        let pair = self
            .decoded_map_rows(handle, schemas, schema_tables)?
            .rows
            .iter()
            .find(|row| row.pair.key_schema == key_schema && row.key_hash == key_hash)
            .map(|row| row.pair.clone());
        let Some(pair) = pair else {
            return Ok(option_none_content_hash(value_schema, schema_tables));
        };
        Ok(match resolved_map_get_value(pair, value_schema) {
            Some((schema, word, realization)) => {
                self.option_some_content_hash(&schema, word, realization, schemas, schema_tables)
            }
            None => option_none_content_hash(value_schema, schema_tables),
        })
    }

    fn alloc_map_get_some(
        &mut self,
        pair: MapPair,
        value_schema: &str,
        schemas: &SchemaTables,
        schema_tables: &SchemaTables,
    ) -> Result<(i64, bool), String> {
        if let Some((schema, word, realization)) = resolved_map_get_value(pair, value_schema) {
            return self.alloc_option_some(&schema, word, realization, schemas, schema_tables);
        }
        self.alloc_option_none(value_schema, schema_tables)
    }

    fn map_pairs_with_carried_rows_cached(
        &mut self,
        handle: i64,
        schemas: &SchemaTables,
        schema_tables: &SchemaTables,
    ) -> Result<(String, Vec<MapPair>, CarriedMapRows), String> {
        let (schema, decoded_rows) = {
            let rows = self.decoded_map_rows(handle, schemas, schema_tables)?;
            (rows.schema.clone(), rows.rows.clone())
        };
        let mut pairs = Vec::with_capacity(decoded_rows.len());
        let mut ordered = Vec::with_capacity(decoded_rows.len());
        for row in decoded_rows {
            let pair = row.pair;
            ordered.push(OrderedMapPair {
                pair: pair.clone(),
                key_hash: row.key_hash,
                value_hash: row.value_hash,
                key_taint: row.key_taint,
                value_taint: row.value_taint,
            });
            pairs.push(pair);
        }
        Ok((schema, pairs, CarriedMapRows { ordered }))
    }

    fn alloc_option_none(
        &mut self,
        value_schema: &str,
        schema_tables: &SchemaTables,
    ) -> Result<(i64, bool), String> {
        let option_schema = option_schema(value_schema);
        let value_ref = schema_ref_for(value_schema, schema_tables)?;
        let mut bytes = Vec::with_capacity(32);
        bytes.extend_from_slice(&0i64.to_le_bytes());
        bytes.extend_from_slice(&value_ref.to_le_bytes());
        bytes.extend_from_slice(&0i64.to_le_bytes());
        bytes.extend_from_slice(&(-1i64).to_le_bytes());
        let content_hash = option_none_content_hash(value_schema, schema_tables);
        Ok(self.alloc_with_hash(&option_schema, bytes, content_hash))
    }

    fn alloc_option_some(
        &mut self,
        value_schema: &str,
        value_word: i64,
        realization: Option<Realization>,
        schemas: &SchemaTables,
        schema_tables: &SchemaTables,
    ) -> Result<(i64, bool), String> {
        let option_schema = option_schema(value_schema);
        let value_ref = schema_ref_for(value_schema, schema_tables)?;
        let hash_schema = realized_value_schema(value_schema).unwrap_or(value_schema);
        let canonical_schema = match realization {
            Some(Realization::Pending) => pending_schema(hash_schema),
            _ => hash_schema.to_string(),
        };
        let value_word = canonicalize_word_for_schema(schemas, &canonical_schema, value_word);
        let mut bytes = Vec::with_capacity(32);
        bytes.extend_from_slice(&1i64.to_le_bytes());
        bytes.extend_from_slice(&value_ref.to_le_bytes());
        bytes.extend_from_slice(&value_word.to_le_bytes());
        bytes.extend_from_slice(
            &realization
                .map(Realization::to_word)
                .unwrap_or(-1)
                .to_le_bytes(),
        );
        let content_hash = self.option_some_content_hash(
            value_schema,
            value_word,
            realization,
            schemas,
            schema_tables,
        );
        Ok(self.alloc_with_hash(&option_schema, bytes, content_hash))
    }

    fn option_some_content_hash(
        &self,
        value_schema: &str,
        value_word: i64,
        realization: Option<Realization>,
        schemas: &SchemaTables,
        schema_tables: &SchemaTables,
    ) -> ContentHash {
        let hash_schema = realized_value_schema(value_schema).unwrap_or(value_schema);
        let canonical_schema = match realization {
            Some(Realization::Pending) => pending_schema(hash_schema),
            _ => hash_schema.to_string(),
        };
        let value_word = canonicalize_word_for_schema(schemas, &canonical_schema, value_word);
        let value_hash = canonical_word_hash_in_store(self, schemas, &canonical_schema, value_word);
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"vix-option");
        update_schema_name(&mut hasher, schema_tables, &option_schema(value_schema));
        hasher.update(&1i64.to_le_bytes());
        update_schema_name(&mut hasher, schema_tables, value_schema);
        if let Some(realization) = &realization {
            hasher.update(&realization.to_word().to_le_bytes());
        }
        hasher.update(value_hash.as_ref());
        finish_hash(hasher)
    }

    fn option_payload(
        &self,
        handle: i64,
        schemas: &SchemaTables,
        schema_tables: &SchemaTables,
    ) -> Result<OptionPayload, String> {
        let entry = self
            .entry(handle)
            .ok_or_else(|| format!("store handle {handle}"))?;
        if !schemas.is_option(&entry.schema) {
            return Err(format!(
                "handle {handle} is `{}`, not an Option",
                entry.schema
            ));
        }
        if entry.bytes.len() != 32 {
            return Err(format!("Option entry has {} bytes", entry.bytes.len()));
        }
        let tag = read_frame_word(&entry.bytes, 0);
        if tag == 0 {
            return Ok(OptionPayload::None);
        }
        let schema_word = read_frame_word(&entry.bytes, 8);
        let schema = schema_name_for(schema_word, schema_tables)?;
        let realization_word = read_frame_word(&entry.bytes, 24);
        let realization = if realization_word == -1 {
            None
        } else {
            Some(Realization::from_word(realization_word)?)
        };
        let word_schema = if let Some(Realization::Pending) = realization {
            pending_schema(realized_value_schema(&schema).unwrap_or(&schema))
        } else {
            realized_value_schema(&schema)
                .unwrap_or(&schema)
                .to_string()
        };
        Ok(OptionPayload::Some {
            word: canonicalize_word_for_schema(
                schemas,
                &word_schema,
                read_frame_word(&entry.bytes, 16),
            ),
            realization,
        })
    }

    fn alloc_with_hash(
        &mut self,
        schema: &str,
        bytes: Vec<u8>,
        content_hash: ContentHash,
    ) -> (i64, bool) {
        self.alloc_with_hash_tier_tainted(schema, HandleTier::Ready, bytes, content_hash, None)
    }

    fn alloc_with_hash_tainted(
        &mut self,
        schema: &str,
        bytes: Vec<u8>,
        content_hash: ContentHash,
        taint: Option<StructuralTaint>,
    ) -> (i64, bool) {
        self.alloc_with_hash_tier_tainted(schema, HandleTier::Ready, bytes, content_hash, taint)
    }

    fn alloc_with_hash_tier_tainted(
        &mut self,
        schema: &str,
        tier: HandleTier,
        bytes: Vec<u8>,
        content_hash: ContentHash,
        taint: Option<StructuralTaint>,
    ) -> (i64, bool) {
        let key = (schema.to_string(), tier, content_hash);
        if let Some(handle) = self.by_content.get(&key).copied() {
            return (handle, true);
        }
        let handle = i64::try_from(self.entries.len()).expect("store handle fits i64");
        self.entries.push(StoreEntry {
            schema: schema.to_string(),
            tier,
            bytes,
            content_hash,
            taint,
        });
        self.by_content.insert(key, handle);
        (handle, false)
    }

    fn alloc_array_words(
        &mut self,
        elem_schema: &str,
        words: Vec<i64>,
        schemas: &SchemaTables,
        schema_tables: &SchemaTables,
    ) -> Result<(i64, bool), String> {
        self.alloc_array_words_with_carried_hash(elem_schema, words, None, schemas, schema_tables)
    }

    fn alloc_array_words_with_carried_hash(
        &mut self,
        elem_schema: &str,
        words: Vec<i64>,
        carried_hash: Option<CarriedArrayHasher>,
        schemas: &SchemaTables,
        schema_tables: &SchemaTables,
    ) -> Result<(i64, bool), String> {
        let schema = array_schema(elem_schema);
        let mut bytes = Vec::with_capacity(24 + words.len() * 8);
        bytes.extend_from_slice(&0i64.to_le_bytes());
        bytes.extend_from_slice(&schema_ref_for(elem_schema, schema_tables)?.to_le_bytes());
        bytes.extend_from_slice(
            &i64::try_from(words.len())
                .expect("array length fits i64")
                .to_le_bytes(),
        );
        for word in &words {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        // Arrays hash declared element/value identity; HandleTier scheduling state never participates.
        let carried_hash = carried_hash.unwrap_or_else(|| {
            recompute_array_element_hasher(
                self,
                schemas,
                schema_tables,
                b"vix-array-words",
                elem_schema,
                &words,
            )
        });
        let taint = combine_taints(
            words
                .iter()
                .filter_map(|word| self.entry(*word).and_then(|entry| entry.taint.clone())),
        );
        let content_hash =
            hash_with_taint(finish_array_element_hash(carried_hash, words.len()), &taint);
        Ok(self.alloc_with_hash_tainted(&schema, bytes, content_hash, taint))
    }

    fn alloc_pending(&mut self, value_schema: &str, invocation: PendingInvocation) -> (i64, bool) {
        let bytes = encode_pending_invocation(&invocation);
        self.alloc_with_hash_tier_tainted(
            value_schema,
            HandleTier::Pending,
            bytes,
            invocation.identity_hash,
            None,
        )
    }

    fn pending_invocation(&self, handle: i64) -> Result<PendingInvocation, String> {
        let entry = self
            .entry(handle)
            .ok_or_else(|| format!("store handle {handle}"))?;
        if !entry.tier.is_pending() {
            return Err(format!(
                "handle {handle} is `{}`, not Pending",
                entry.schema
            ));
        }
        decode_pending_invocation(&entry.bytes)
    }

    fn alloc_array_pending(
        &mut self,
        elem_schema: &str,
        pending: Vec<i64>,
        schemas: &SchemaTables,
        schema_tables: &SchemaTables,
    ) -> Result<(i64, bool), String> {
        let schema = array_schema(elem_schema);
        let mut bytes = Vec::with_capacity(24 + pending.len() * 8);
        bytes.extend_from_slice(&1i64.to_le_bytes());
        bytes.extend_from_slice(&schema_ref_for(elem_schema, schema_tables)?.to_le_bytes());
        bytes.extend_from_slice(
            &i64::try_from(pending.len())
                .expect("array length fits i64")
                .to_le_bytes(),
        );
        for word in &pending {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        // Pending array handles contribute their declared value identity; HandleTier scheduling state stays out of the hash.
        let carried_hash = recompute_array_element_hasher(
            self,
            schemas,
            schema_tables,
            b"vix-array-pending",
            elem_schema,
            &pending,
        );
        Ok(self.alloc_with_hash(
            &schema,
            bytes,
            finish_array_element_hash(carried_hash, pending.len()),
        ))
    }

    fn array_entry(&self, handle: i64, schema_tables: &SchemaTables) -> Result<ArrayEntry, String> {
        let entry = self
            .entry(handle)
            .ok_or_else(|| format!("store handle {handle}"))?;
        if !schema_tables.is_list(&entry.schema) {
            return Err(format!("handle {handle} is `{}`, not Array", entry.schema));
        }
        let kind = read_frame_word(&entry.bytes, 0);
        match kind {
            0 | 1 => {
                let count = usize::try_from(read_frame_word(&entry.bytes, 16))
                    .map_err(|_| "array count")?;
                let expected = 24 + count * 8;
                if entry.bytes.len() != expected {
                    return Err(format!(
                        "Array words entry has {} bytes, expected {expected}",
                        entry.bytes.len()
                    ));
                }
                let elem_schema = schema_name_for(read_frame_word(&entry.bytes, 8), schema_tables)?;
                if array_element_schema(&entry.schema).is_some_and(|schema| schema != elem_schema) {
                    return Err(format!(
                        "Array payload element {elem_schema} disagrees with schema {}",
                        entry.schema
                    ));
                }
                let words = (0..count)
                    .map(|i| read_frame_word(&entry.bytes, 24 + i * 8))
                    .collect();
                if kind == 0 {
                    Ok(ArrayEntry::Words { elem_schema, words })
                } else {
                    Ok(ArrayEntry::Pending {
                        elem_schema,
                        pending: words,
                    })
                }
            }
            other => Err(format!("unknown Array kind {other}")),
        }
    }

    fn alloc_tree_concrete(&mut self, tree: crate::exec::Tree) -> (i64, bool) {
        let bytes = encode_concrete_tree(&tree);
        let content_hash = hash_concrete_tree(&tree);
        self.alloc_with_hash("Tree", bytes, content_hash)
    }

    fn alloc_tree_merge(&mut self, pending: Vec<i64>, schemas: &SchemaTables) -> (i64, bool) {
        let bytes = encode_handle_list(1, &pending);
        let content_hash = hash_handle_list(b"vix-tree-merge", &pending, self, schemas);
        self.alloc_with_hash("Tree", bytes, content_hash)
    }

    fn alloc_tree_exec(&mut self, run_id: u64, identity: ContentHash) -> (i64, bool) {
        let mut bytes = Vec::with_capacity(16);
        bytes.extend_from_slice(&2i64.to_le_bytes());
        bytes.extend_from_slice(
            &i64::try_from(run_id)
                .expect("run id fits i64")
                .to_le_bytes(),
        );
        let handle = i64::try_from(self.entries.len()).expect("store handle fits i64");
        self.entries.push(StoreEntry {
            schema: "Tree".to_string(),
            tier: HandleTier::Ready,
            bytes,
            content_hash: identity,
            taint: None,
        });
        (handle, false)
    }

    fn tree_entry(&self, handle: i64) -> Result<TreeEntry, String> {
        let entry = self
            .entry(handle)
            .ok_or_else(|| format!("store handle {handle}"))?;
        if entry.schema != "Tree" {
            return Err(format!("handle {handle} is `{}`, not Tree", entry.schema));
        }
        let kind = read_frame_word(&entry.bytes, 0);
        match kind {
            0 => Ok(TreeEntry::Concrete(decode_concrete_tree(&entry.bytes)?)),
            1 => Ok(TreeEntry::Merge(decode_handle_list(&entry.bytes)?)),
            2 => {
                if entry.bytes.len() != 16 {
                    return Err(format!(
                        "Tree exec entry has {} bytes, expected 16",
                        entry.bytes.len()
                    ));
                }
                Ok(TreeEntry::Exec(
                    u64::try_from(read_frame_word(&entry.bytes, 8)).map_err(|_| "run id")?,
                ))
            }
            other => Err(format!("unknown Tree kind {other}")),
        }
    }

    fn string_value(&self, handle: i64, schema: &str) -> Result<String, String> {
        let entry = self
            .entry(handle)
            .ok_or_else(|| format!("store handle {handle}"))?;
        if entry.schema != schema {
            return Err(format!(
                "handle {handle} is `{}`, not {schema}",
                entry.schema
            ));
        }
        String::from_utf8(entry.bytes.clone()).map_err(|err| err.to_string())
    }
}

fn intern_molten_word(
    store: &mut ValueStore,
    molten: &mut MoltenStore,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
    schema: &str,
    word: i64,
) -> Result<(i64, bool), String> {
    if schema_is_inline_word(schemas, schema) {
        return Ok((word, true));
    }
    let molten_handle = match Handle::from_word(word) {
        Handle::Molten(handle) => handle,
        Handle::Store(_) => return Ok((word, true)),
    };
    let index = molten_handle.index();
    let entry = molten
        .entries
        .get_mut(index)
        .ok_or_else(|| format!("molten handle {word}"))?;
    if let MoltenValue::Interned(handle) = entry.value {
        return Ok((handle, true));
    }
    if matches!(entry.value, MoltenValue::Interning) {
        return Err(format!("molten handle {word} is already being interned"));
    }
    let value = std::mem::replace(&mut entry.value, MoltenValue::Interning);
    let carried_array_hash = entry.carried_array_hash.take();
    let carried_map_rows = entry.carried_map_rows.take();
    let (handle, deduped) = match value {
        MoltenValue::Record {
            schema: record_schema,
            mut bytes,
        } => {
            if record_schema != schema {
                return Err(format!("molten record is `{record_schema}`, not {schema}"));
            }
            intern_value_bytes_children(
                store,
                molten,
                descriptors,
                schemas,
                schema_tables,
                &record_schema,
                &mut bytes,
            )?;
            store.alloc(&record_schema, bytes, descriptors, schemas)
        }
        MoltenValue::Map {
            schema: map_schema,
            mut pairs,
        } => {
            let mut carried_rows = carried_map_rows;
            if map_schema != schema && !map_schema_is_realized_projection(schema, &map_schema) {
                return Err(format!("molten map is `{map_schema}`, not {schema}"));
            }
            let mut changed_words = false;
            for pair in &mut pairs {
                let interned_key = intern_molten_word(
                    store,
                    molten,
                    descriptors,
                    schemas,
                    schema_tables,
                    &pair.key_schema,
                    pair.key_word,
                )?
                .0;
                changed_words |= interned_key != pair.key_word;
                pair.key_word = interned_key;
                let interned_value = intern_molten_word(
                    store,
                    molten,
                    descriptors,
                    schemas,
                    schema_tables,
                    &pair.value_schema,
                    pair.value_word,
                )?
                .0;
                changed_words |= interned_value != pair.value_word;
                pair.value_word = interned_value;
            }
            if changed_words {
                carried_rows = None;
            }
            store.alloc_map_with_carried_rows(
                &map_schema,
                pairs,
                carried_rows,
                schema_tables,
                descriptors,
                schemas,
            )?
        }
        MoltenValue::ArrayWords {
            elem_schema,
            mut words,
        } => {
            let mut carried_hash = carried_array_hash;
            if !schemas.is_list(schema) {
                return Err(format!("molten array cannot intern as {schema}"));
            }
            if array_element_schema(schema).is_some_and(|schema| schema != elem_schema) {
                return Err(format!(
                    "molten Array<{elem_schema}> cannot intern as {schema}"
                ));
            }
            let mut changed_words = false;
            for word in &mut words {
                let interned = intern_molten_word(
                    store,
                    molten,
                    descriptors,
                    schemas,
                    schema_tables,
                    &elem_schema,
                    *word,
                )?
                .0;
                changed_words |= interned != *word;
                *word = interned;
            }
            if changed_words {
                carried_hash = Some(recompute_array_element_hasher(
                    store,
                    schemas,
                    schema_tables,
                    b"vix-array-words",
                    &elem_schema,
                    &words,
                ));
            }
            store.alloc_array_words_with_carried_hash(
                &elem_schema,
                words,
                carried_hash,
                schemas,
                schema_tables,
            )?
        }
        MoltenValue::Interned(_) => unreachable!("handled above"),
        MoltenValue::Interning => unreachable!("handled above"),
    };
    molten.entries[index].value = MoltenValue::Interned(handle);
    molten.entries[index].carried_array_hash = None;
    molten.entries[index].carried_map_rows = None;
    Ok((handle, deduped))
}

fn map_schema_is_realized_projection(expected: &str, actual: &str) -> bool {
    let Some((expected_key, expected_value)) = map_schemas(expected) else {
        return false;
    };
    let Some((actual_key, actual_value)) = map_schemas(actual) else {
        return false;
    };
    expected_key == actual_key && realized_value_schema(actual_value) == Some(expected_value)
}

fn intern_value_bytes_children(
    store: &mut ValueStore,
    molten: &mut MoltenStore,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
    schema: &str,
    bytes: &mut [u8],
) -> Result<(), String> {
    let descriptor = descriptors
        .get(schema)
        .ok_or_else(|| format!("descriptor for schema `{schema}`"))?;
    match &descriptor.access {
        Access::Record(record) => {
            for field in &record.fields {
                let field_schema = descriptor_word_schema(schemas, &field.descriptor);
                let word = read_frame_word(bytes, field.offset);
                let (interned, _) = intern_molten_word(
                    store,
                    molten,
                    descriptors,
                    schemas,
                    schema_tables,
                    &field_schema,
                    word,
                )?;
                write_frame_word(bytes, field.offset, interned);
            }
        }
        Access::Enum(enum_access) => {
            let tag = usize::try_from(read_variant_tag(bytes, descriptor))
                .map_err(|_| "negative enum tag".to_string())?;
            let Some(variant) = enum_access.variants.get(tag) else {
                return Ok(());
            };
            for field in &variant.payload.fields {
                let field_schema = descriptor_word_schema(schemas, &field.descriptor);
                let word = read_frame_word(bytes, field.offset);
                let (interned, _) = intern_molten_word(
                    store,
                    molten,
                    descriptors,
                    schemas,
                    schema_tables,
                    &field_schema,
                    word,
                )?;
                write_frame_word(bytes, field.offset, interned);
            }
        }
        _ => {}
    }
    Ok(())
}

/// The demand scheduler.
pub struct Driver {
    program: Program,
    lane_kind: Lane,
    lane: LaneRuntime,
    fns: Vec<LoweredFn>,
    descriptors: DescriptorMap,
    schemas: SchemaTables,
    memo: IdentityHashMap<CanonMemoKey, MemoEntry>,
    memo_candidates: IdentityHashMap<ProjectionCandidateKey, Vec<CanonMemoKey>>,
    journal: BTreeMap<String, i64>,
    exec_cache: crate::exec::ExecCache,
    fetch_backend: Arc<dyn FetchBackend>,
    exec_backend: Option<Arc<dyn MachineExecBackend>>,
    elf_projection_memo: IdentityHashMap<(ContentHash, super::elf::Projection), i64>,
    ast_roots: HashMap<i64, i64>,
    ast_parse_memo: IdentityHashMap<ContentHash, Arc<ast::SourceFile>>,
    ast_projection_memo: IdentityHashMap<(ContentHash, super::ast_probe::Projection, String), i64>,
    crate_archive_memo: IdentityHashMap<ContentHash, i64>,
    oci_projection_memo: IdentityHashMap<(ContentHash, super::oci::Projection), i64>,
    oci_file_memo: IdentityHashMap<(ContentHash, String), Option<i64>>,
    runs: BTreeMap<u64, PendingExecRun>,
    next_run_id: u64,
    trace_clock: u64,
    pub trace: Vec<DriveEvent>,
    event_sink: Option<DriveEventSink>,
    step_mode: StepMode,
    force_molten_copy: bool,
    last_molten_debug_counts: (usize, usize, usize, usize, usize, usize),
    molten_stats: MoltenStats,
    store: RefCell<ValueStore>,
}

impl Driver {
    fn lowered(&self, fn_ref: FnRef) -> &LoweredFn {
        &self.fns[fn_ref.index()]
    }

    pub fn new(program: Program, fns: Vec<LoweredFn>) -> Self {
        Self::with_descriptors(program, fns, DescriptorMap::new())
    }

    pub(crate) fn with_descriptors(
        program: Program,
        fns: Vec<LoweredFn>,
        descriptors: DescriptorMap,
    ) -> Self {
        Self::try_with_descriptors(program, fns, descriptors, Lane::Interp)
            .expect("interp lane is always available")
    }

    pub(crate) fn try_with_descriptors(
        program: Program,
        fns: Vec<LoweredFn>,
        descriptors: DescriptorMap,
        lane: Lane,
    ) -> Result<Self, String> {
        Self::try_with_schema_tables(program, fns, descriptors, SchemaTables::empty(), lane)
    }

    pub(crate) fn try_with_schema_tables(
        program: Program,
        fns: Vec<LoweredFn>,
        descriptors: DescriptorMap,
        schemas: SchemaTables,
        lane: Lane,
    ) -> Result<Self, String> {
        let lane_runtime = LaneRuntime::new(lane, &program)?;
        Ok(Driver {
            program,
            lane_kind: lane,
            lane: lane_runtime,
            fns,
            descriptors,
            schemas,
            memo: IdentityHashMap::default(),
            memo_candidates: IdentityHashMap::default(),
            journal: BTreeMap::new(),
            exec_cache: crate::exec::ExecCache::new(),
            fetch_backend: Arc::new(NoFetchBackend),
            exec_backend: None,
            elf_projection_memo: IdentityHashMap::default(),
            ast_roots: HashMap::new(),
            ast_parse_memo: IdentityHashMap::default(),
            ast_projection_memo: IdentityHashMap::default(),
            crate_archive_memo: IdentityHashMap::default(),
            oci_projection_memo: IdentityHashMap::default(),
            oci_file_memo: IdentityHashMap::default(),
            runs: BTreeMap::new(),
            next_run_id: 0,
            trace_clock: 0,
            trace: Vec::new(),
            event_sink: None,
            step_mode: StepMode::Run,
            force_molten_copy: std::env::var_os("VIX_FORCE_MOLTEN_COPY").is_some(),
            last_molten_debug_counts: (0, 0, 0, 0, 0, 0),
            molten_stats: MoltenStats::default(),
            store: RefCell::new(ValueStore::default()),
        })
    }

    pub(crate) fn reload(
        &mut self,
        program: Program,
        fns: Vec<LoweredFn>,
        descriptors: DescriptorMap,
        schemas: SchemaTables,
    ) -> Result<(), String> {
        self.lane = LaneRuntime::new(self.lane_kind, &program)?;
        self.program = program;
        self.fns = fns;
        self.descriptors = descriptors;
        self.schemas = schemas;
        self.trace.clear();
        self.last_molten_debug_counts = (0, 0, 0, 0, 0, 0);
        self.molten_stats = MoltenStats::default();
        Ok(())
    }

    pub fn set_fetch_backend(&mut self, backend: Arc<dyn FetchBackend>) {
        self.fetch_backend = backend;
    }

    pub fn set_exec_backend(&mut self, backend: Option<Arc<dyn MachineExecBackend>>) {
        self.exec_backend = backend;
    }

    pub fn set_event_sink(&mut self, sink: Option<DriveEventSink>) {
        self.event_sink = sink;
    }

    pub fn set_step_mode(&mut self, mode: StepMode) {
        self.step_mode = mode;
    }

    pub fn set_force_molten_copy(&mut self, force: bool) {
        self.force_molten_copy = force;
    }

    pub fn molten_stats(&self) -> MoltenStats {
        self.molten_stats
    }

    pub fn map_intern_stats(&self) -> MapInternStats {
        self.store.borrow().map_intern_stats()
    }

    fn emit(&mut self, event: DriveEvent) {
        self.trace.push(event);
        let event = self.trace.last().expect("just pushed event");
        if let Some(sink) = &mut self.event_sink {
            let command = sink(event);
            if self.step_mode == StepMode::Step && command == StepCommand::Resume {
                self.step_mode = StepMode::Run;
            }
        }
    }

    fn next_timestamp(&mut self) -> u64 {
        let value = self.trace_clock;
        self.trace_clock = self.trace_clock.saturating_add(1);
        value
    }

    /// How many memo entries exist (tests: warm behavior).
    pub fn memo_len(&self) -> usize {
        self.memo.len()
    }

    pub fn store_len(&self) -> usize {
        self.store.borrow().len()
    }

    pub fn molten_debug_counts(&self) -> (usize, usize, usize, usize, usize, usize) {
        self.last_molten_debug_counts
    }

    pub fn store_entry(&self, handle: i64) -> Option<StoreEntry> {
        self.store.borrow().entry(handle).cloned()
    }

    pub fn export_value_bundle(
        &self,
        root: i64,
        code: Vec<CodeBundle>,
    ) -> Result<ValueBundle, String> {
        let values = self
            .store
            .borrow()
            .entries()
            .into_iter()
            .enumerate()
            .map(|(handle, entry)| StoreValue {
                handle: u64::try_from(handle).expect("store handle fits u64"),
                schema: entry.schema,
                tier: entry.tier,
                bytes: entry.bytes,
                content_hash: entry.content_hash.to_vec(),
                taint: entry.taint,
            })
            .collect();
        Ok(ValueBundle {
            root: u64::try_from(root).map_err(|_| format!("negative root handle {root}"))?,
            values,
            code,
        })
    }

    pub fn import_value_bundle(&self, bundle: &ValueBundle) -> Result<i64, String> {
        let mut values = bundle.values.clone();
        values.sort_by_key(|value| value.handle);
        let mut store = self.store.borrow_mut();
        for value in values {
            let content_hash = ContentHash::from_slice(&value.content_hash)
                .map_err(|len| format!("content hash has {len} bytes"))?;
            store.insert_at(
                usize::try_from(value.handle).map_err(|_| format!("handle {}", value.handle))?,
                StoreEntry {
                    schema: value.schema,
                    tier: value.tier,
                    bytes: value.bytes,
                    content_hash,
                    taint: value.taint,
                },
            )?;
        }
        i64::try_from(bundle.root).map_err(|_| format!("root handle {}", bundle.root))
    }

    #[cfg(test)]
    pub fn store_field(&self, handle: i64, field_index: usize) -> Result<i64, String> {
        let store = self.store.borrow();
        let entry = store
            .entry(handle)
            .ok_or_else(|| format!("store handle {handle}"))?;
        let descriptor = self
            .descriptors
            .get(&entry.schema)
            .ok_or_else(|| format!("descriptor for schema `{}`", entry.schema))?;
        let field = field_descriptor(descriptor, &entry.bytes, field_index);
        let offset = field_offset(descriptor, &entry.bytes, field_index);
        Ok(read_word_at(&entry.bytes, offset, field.layout.size))
    }

    #[cfg(test)]
    pub fn store_tag(&self, handle: i64) -> Result<u64, String> {
        let store = self.store.borrow();
        let entry = store
            .entry(handle)
            .ok_or_else(|| format!("store handle {handle}"))?;
        let descriptor = self
            .descriptors
            .get(&entry.schema)
            .ok_or_else(|| format!("descriptor for schema `{}`", entry.schema))?;
        Ok(read_variant_tag(&entry.bytes, descriptor))
    }

    #[cfg(test)]
    pub fn raw_string(&self, handle: i64, schema: &str) -> Result<String, String> {
        self.store.borrow().string_value(handle, schema)
    }

    #[cfg(test)]
    pub fn map_words(&self, handle: i64) -> Result<Vec<MapWordRow>, String> {
        let (_, pairs) = self
            .store
            .borrow()
            .map_pairs(handle, &self.schemas, &self.schemas)?;
        Ok(pairs
            .into_iter()
            .map(|pair| {
                (
                    pair.key_schema,
                    pair.key_word,
                    pair.value_schema,
                    pair.value_word,
                    pair.value_realization.map(Realization::to_word),
                )
            })
            .collect())
    }

    #[cfg(test)]
    pub fn array_words(&self, handle: i64) -> Result<(String, Vec<i64>), String> {
        match self.store.borrow().array_entry(handle, &self.schemas)? {
            ArrayEntry::Words { elem_schema, words } => Ok((elem_schema, words)),
            ArrayEntry::Pending { .. } => Err("array is pending".into()),
        }
    }

    #[cfg(test)]
    pub fn compare_store_words(&self, schema: &str, a: i64, b: i64) -> Result<Ordering, String> {
        compare_words(
            &self.store.borrow(),
            &self.descriptors,
            &self.schemas,
            &self.schemas,
            schema,
            a,
            b,
        )
    }

    pub fn render_word(
        &self,
        schema: &str,
        word: i64,
        names: &RenderNames,
    ) -> Result<RenderedValue, String> {
        render_word(
            &self.store.borrow(),
            &self.descriptors,
            &self.schemas,
            &self.schemas,
            names,
            schema,
            word,
        )
    }

    pub fn tree_entries(&mut self, handle: i64) -> Result<BTreeMap<String, String>, String> {
        let handle = self.force_tree_handle(handle)?;
        match self.store.borrow().tree_entry(handle)? {
            TreeEntry::Concrete(tree) => Ok(tree.entries),
            TreeEntry::Merge(_) | TreeEntry::Exec(_) => Err("tree force returned pending".into()),
        }
    }

    pub fn tree_blob_entries(&mut self, handle: i64) -> Result<BTreeMap<String, Vec<u8>>, String> {
        let handle = self.force_tree_handle(handle)?;
        match self.store.borrow().tree_entry(handle)? {
            TreeEntry::Concrete(tree) => Ok(tree.blobs),
            TreeEntry::Merge(_) | TreeEntry::Exec(_) => Err("tree force returned pending".into()),
        }
    }

    pub fn intern_tree_concrete(&self, tree: crate::exec::Tree) -> i64 {
        self.store.borrow_mut().alloc_tree_concrete(tree).0
    }

    pub fn intern_run_value(&self, ok: bool, outputs: crate::exec::Tree) -> Result<i64, String> {
        let out = self.store.borrow_mut().alloc_tree_concrete(outputs).0;
        let descriptor = self
            .descriptors
            .get("Run")
            .ok_or_else(|| "missing Run descriptor".to_string())?;
        let mut bytes = vec![0u8; descriptor.layout.size];
        let ok_offset = field_offset(descriptor, &bytes, 0);
        bytes[ok_offset..ok_offset + 8].copy_from_slice(&(ok as i64).to_le_bytes());
        let out_offset = field_offset(descriptor, &bytes, 1);
        bytes[out_offset..out_offset + 8].copy_from_slice(&out.to_le_bytes());
        Ok(self
            .store
            .borrow_mut()
            .alloc("Run", bytes, &self.descriptors, &self.schemas)
            .0)
    }

    pub(super) fn fn_hash(&self, fn_ref: FnRef) -> u64 {
        self.lowered(fn_ref).hash
    }

    #[cfg(test)]
    pub(super) fn semantic_comparator_len(&self, fn_ref: FnRef) -> usize {
        self.lowered(fn_ref).semantic_comparators.len()
    }

    pub(super) fn pending_for_fn(
        &self,
        fn_ref: FnRef,
        args: Vec<i64>,
    ) -> Result<(i64, String), String> {
        let lowered = self
            .fns
            .get(fn_ref.index())
            .ok_or_else(|| format!("function ref {}", fn_ref.index()))?;
        if args.len() > lowered.arg_schemas.len() {
            return Err(format!(
                "pending invocation got {} args, expected at most {}",
                args.len(),
                lowered.arg_schemas.len()
            ));
        }
        let invocation = pending_invocation_for(lowered, &self.store, &self.schemas, args);
        let schema = lowered.return_schema.clone();
        let handle = self.store.borrow_mut().alloc_pending(&schema, invocation).0;
        Ok((handle, schema))
    }

    pub fn invoke_pending_handle(&mut self, pending: i64, args: Vec<i64>) -> Result<i64, String> {
        let invocation = self.store.borrow().pending_invocation(pending)?;
        if invocation.primitive.is_some() {
            return Err("primitive pending values are not callable".into());
        }
        if invocation.remaining_arity != args.len() {
            return Err(format!(
                "pending invocation expected {} argument(s), got {}",
                invocation.remaining_arity,
                args.len()
            ));
        }
        let fn_ref = self.fn_ref_for_hash(invocation.closure_hash)?;
        let mut all_args = invocation.args;
        all_args.extend(args);
        self.demand(fn_ref, all_args)
    }

    #[cfg(test)]
    pub(super) fn fn_ops(&self, fn_ref: FnRef) -> &[Op] {
        &self.program.fns[self.lowered(fn_ref).task_fn.0 as usize].code
    }

    #[cfg(test)]
    pub(super) fn memo_whole_read_count(
        &self,
        fn_ref: FnRef,
        args: &[i64],
        arg_index: usize,
        schema: &str,
    ) -> usize {
        let key = self.memo_key(fn_ref, args);
        self.memo
            .get(&key)
            .map(|entry| {
                entry
                    .read_set
                    .entries
                    .iter()
                    .filter(|read| {
                        read.arg_index == arg_index
                            && matches!(&read.path, ProjectionPath::Whole { schema: read_schema } if read_schema == schema)
                    })
                    .count()
            })
            .unwrap_or(0)
    }

    pub fn intern_raw_value(&self, schema: &str, bytes: Vec<u8>) -> (i64, bool) {
        self.store
            .borrow_mut()
            .alloc_raw(schema, bytes, &self.schemas)
    }

    pub fn intern_version_value(&self, text: &str) -> Result<(i64, bool), String> {
        let version = super::version::parse(text)?;
        Ok(self.store.borrow_mut().alloc_raw(
            "Version",
            super::version::canonical_bytes(&version),
            &self.schemas,
        ))
    }

    pub fn intern_version_set_req_value(&self, text: &str) -> Result<(i64, bool), String> {
        let set = super::version_set::VersionSet::from_req(text)?;
        Ok(self
            .store
            .borrow_mut()
            .alloc_raw("VersionSet", set.canonical_bytes(), &self.schemas))
    }

    pub fn intern_linux_target(&self) -> (i64, bool) {
        self.intern_structured_target(OS_LINUX, host_arch_index())
            .unwrap_or_else(|_| {
                self.store.borrow_mut().alloc_raw(
                    "Target",
                    0x391c555cf0975f9cu64.to_le_bytes().to_vec(),
                    &self.schemas,
                )
            })
    }

    #[cfg(test)]
    pub fn intern_windows_target(&self) -> Result<(i64, bool), String> {
        self.intern_structured_target(OS_WINDOWS, host_arch_index())
    }

    pub fn intern_host_target(&self) -> Result<(i64, bool), String> {
        self.intern_structured_target(host_os_index(), host_arch_index())
    }

    #[cfg(test)]
    pub fn intern_target(&self, os_index: u64, arch_index: u64) -> Result<(i64, bool), String> {
        self.intern_structured_target(os_index, arch_index)
    }

    fn intern_structured_target(
        &self,
        os_index: u64,
        arch_index: u64,
    ) -> Result<(i64, bool), String> {
        intern_structured_target(
            &self.store,
            &self.descriptors,
            &self.schemas,
            os_index,
            arch_index,
        )
    }

    /// Demand one invocation's identity: the edge of the machine.
    /// Returns the scalar result (slice 1).
    pub(super) fn demand(&mut self, fn_ref: FnRef, args: Vec<i64>) -> Result<i64, String> {
        let key = self.memo_key(fn_ref, &args);
        self.emit(DriveEvent::Demanded { fn_hash: key.0 });
        if let Some(entry) = self.memo.get(&key).cloned() {
            self.emit(DriveEvent::MemoHit { fn_hash: key.0 });
            return Ok(entry.value);
        }
        if let Some(entry) = self.projection_memo_hit(fn_ref, &args, &key)? {
            let verified = entry.read_set.len();
            self.memo.insert(key.clone(), entry.clone());
            self.index_memo_candidate(fn_ref, &args, &key);
            self.emit(DriveEvent::MemoProjectionHit {
                fn_hash: key.0,
                verified,
            });
            return Ok(entry.value);
        }
        if let Some(entry) = self.semantic_memo_hit(fn_ref, &args, &key)? {
            let verified = self.lowered(fn_ref).semantic_comparators.len();
            self.memo.insert(key.clone(), entry.clone());
            self.index_memo_candidate(fn_ref, &args, &key);
            self.emit(DriveEvent::MemoSemanticHit {
                fn_hash: key.0,
                verified,
            });
            return Ok(entry.value);
        }

        // Waiters: invocation key → executions parked on it (by index
        // into `executions`) with the slot to fill.
        let mut executions: Vec<Option<Execution>> = Vec::new();
        let mut waiters: IdentityHashMap<CanonMemoKey, Vec<(usize, usize)>> =
            IdentityHashMap::default();
        let mut pending_work = PendingWork::default();
        let mut in_flight = InFlightInvocations::default();
        let mut runnable = RunnableExecutions::default();

        let root = self.spawn(&mut executions, fn_ref, key.clone(), &args)?;
        let inserted = in_flight.started(key.clone());
        debug_assert!(inserted, "root invocation was already in flight");
        runnable.push(root);

        loop {
            self.harvest_pending_runs(false, &mut executions, &mut runnable, &mut pending_work)?;
            let Some(ix) = runnable.pop() else {
                if pending_work.run_waiters.is_empty() && pending_work.pending_fetches.is_empty() {
                    break;
                }
                self.harvest_pending_runs(true, &mut executions, &mut runnable, &mut pending_work)?;
                continue;
            };
            let mut exec = executions[ix].take().expect("runnable execution exists");
            let requests = self.burst(&mut exec, ix);
            match requests {
                Burst::Done(value) => {
                    let done_key = exec.key.clone();
                    let removed = in_flight.finished(&done_key);
                    debug_assert!(removed, "completed invocation was not in flight");
                    let mut value = value;
                    let return_schema = self.lowered(exec.fn_ref).return_schema.clone();
                    if !schema_is_inline_word(&self.schemas, &return_schema) {
                        match Handle::from_word(value) {
                            Handle::Molten(_) => {
                                let (interned, deduped) = intern_molten_word(
                                    &mut self.store.borrow_mut(),
                                    &mut exec.molten,
                                    &self.descriptors,
                                    &self.schemas,
                                    &self.schemas,
                                    &return_schema,
                                    value,
                                )?;
                                self.emit(DriveEvent::StoreAlloc {
                                    schema_ref: hash_u64(&return_schema),
                                    deduped,
                                });
                                value = interned;
                            }
                            Handle::Store(_) => {}
                        }
                    }
                    let value = self.canonicalize_return_word(exec.fn_ref, value);
                    self.record_whole_arg_if_projectable(
                        exec.fn_ref,
                        &exec.args,
                        value,
                        &mut exec.read_set,
                    );
                    let done_entry = MemoEntry {
                        value,
                        args: exec.args.clone(),
                        read_set: exec.read_set,
                    };
                    self.last_molten_debug_counts = exec.molten.debug_counts();
                    self.memo.insert(done_key.clone(), done_entry.clone());
                    self.index_memo_candidate(exec.fn_ref, &exec.args, &done_key);
                    self.emit(DriveEvent::Completed {
                        fn_hash: done_key.0,
                    });
                    // Feed everyone parked on this invocation; they
                    // become runnable again.
                    if let Some(list) = waiters.remove(&done_key) {
                        for (waiter_ix, slot) in list {
                            let w = executions[waiter_ix]
                                .as_mut()
                                .expect("parked waiter exists");
                            w.ready[slot] = true;
                            w.awaited[slot] = value;
                            let remapped = remap_read_set_for_caller(
                                &self.lowered(w.fn_ref).arg_schemas,
                                &w.args,
                                &exec.args,
                                &done_entry.read_set,
                                &self.store.borrow(),
                                &self.descriptors,
                                &self.schemas,
                            );
                            w.read_set.extend(&remapped);
                            runnable.push(waiter_ix);
                        }
                    }
                    // Execution finished: drop it (arena and all).
                }
                Burst::Pending(pending) => {
                    let BurstPending {
                        new_requests,
                        project_requests,
                        text_project_requests,
                        exec_requests,
                        fetch_requests,
                        doc_parse_requests,
                        crate_archive_requests,
                        option_unwraps,
                        pending_coercions,
                        pending_invokes,
                        parked_input,
                    } = *pending;
                    for req in exec_requests {
                        self.record_whole_args_if_projectable(
                            exec.fn_ref,
                            &exec.args,
                            req.parts.iter().filter_map(|part| match part {
                                CommandRequestPart::Splice(word) => Some(*word),
                                CommandRequestPart::Token(_) => None,
                            }),
                            &mut exec.read_set,
                        );
                        match self.execute_request(req) {
                            Ok((input_slot, value)) => {
                                if exec.ready.len() <= input_slot {
                                    exec.ready.resize(input_slot + 1, false);
                                    exec.awaited.resize(input_slot + 1, 0);
                                }
                                exec.ready[input_slot] = true;
                                exec.awaited[input_slot] = value;
                            }
                            Err(err) => return Err(err),
                        }
                    }
                    for req in fetch_requests {
                        let prepared = self.prepare_fetch_request(req)?;
                        if let Some(&run_id) = pending_work.in_flight_fetches.get(&prepared.key) {
                            pending_work
                                .fetch_waiters
                                .entry(run_id)
                                .or_default()
                                .push((ix, prepared.input_slot));
                        } else {
                            let run_id = pending_work.next_fetch_run_id;
                            pending_work.next_fetch_run_id =
                                pending_work.next_fetch_run_id.saturating_add(1);
                            pending_work
                                .fetch_waiters
                                .entry(run_id)
                                .or_default()
                                .push((ix, prepared.input_slot));
                            pending_work
                                .in_flight_fetches
                                .insert(prepared.key.clone(), run_id);
                            let run = self.start_fetch_run(prepared);
                            pending_work.pending_fetches.insert(run_id, run);
                        }
                    }
                    for req in doc_parse_requests {
                        self.record_whole_arg_if_projectable(
                            exec.fn_ref,
                            &exec.args,
                            req.input,
                            &mut exec.read_set,
                        );
                        match self.doc_parse_request(req) {
                            Ok((input_slot, value)) => {
                                if exec.ready.len() <= input_slot {
                                    exec.ready.resize(input_slot + 1, false);
                                    exec.awaited.resize(input_slot + 1, 0);
                                }
                                exec.ready[input_slot] = true;
                                exec.awaited[input_slot] = value;
                            }
                            Err(err) => return Err(err),
                        }
                    }
                    for req in crate_archive_requests {
                        self.record_whole_arg_if_projectable(
                            exec.fn_ref,
                            &exec.args,
                            req.input,
                            &mut exec.read_set,
                        );
                        match self.crate_archive_request(req) {
                            Ok((input_slot, value)) => {
                                if exec.ready.len() <= input_slot {
                                    exec.ready.resize(input_slot + 1, false);
                                    exec.awaited.resize(input_slot + 1, 0);
                                }
                                exec.ready[input_slot] = true;
                                exec.awaited[input_slot] = value;
                            }
                            Err(err) => return Err(err),
                        }
                    }
                    for req in project_requests {
                        if let Some(run_id) = self.project_request_pending_run(req.tree)? {
                            pending_work
                                .run_waiters
                                .entry(run_id)
                                .or_default()
                                .push((ix, TreeProjectRequest::Tree(req)));
                            continue;
                        }
                        match self.project_request(req, exec.fn_ref, &exec.args, &mut exec.read_set)
                        {
                            Ok((input_slot, value)) => {
                                if exec.ready.len() <= input_slot {
                                    exec.ready.resize(input_slot + 1, false);
                                    exec.awaited.resize(input_slot + 1, 0);
                                }
                                exec.ready[input_slot] = true;
                                exec.awaited[input_slot] = value;
                            }
                            Err(err) => return Err(err),
                        }
                    }
                    for req in text_project_requests {
                        if let Some(run_id) = self.project_request_pending_run(req.tree)? {
                            pending_work
                                .run_waiters
                                .entry(run_id)
                                .or_default()
                                .push((ix, TreeProjectRequest::Text(req)));
                            continue;
                        }
                        match self.text_project_request(
                            req,
                            exec.fn_ref,
                            &exec.args,
                            &mut exec.read_set,
                        ) {
                            Ok((input_slot, value)) => {
                                if exec.ready.len() <= input_slot {
                                    exec.ready.resize(input_slot + 1, false);
                                    exec.awaited.resize(input_slot + 1, 0);
                                }
                                exec.ready[input_slot] = true;
                                exec.awaited[input_slot] = value;
                            }
                            Err(err) => return Err(err),
                        }
                    }
                    for req in option_unwraps {
                        self.record_whole_arg_if_projectable(
                            exec.fn_ref,
                            &exec.args,
                            req.option,
                            &mut exec.read_set,
                        );
                        match self.option_unwrap_request(req) {
                            Ok(fills) => {
                                for (input_slot, value) in fills {
                                    if exec.ready.len() <= input_slot {
                                        exec.ready.resize(input_slot + 1, false);
                                        exec.awaited.resize(input_slot + 1, 0);
                                    }
                                    exec.ready[input_slot] = true;
                                    exec.awaited[input_slot] = value;
                                }
                            }
                            Err(err) => return Err(err),
                        }
                    }
                    let mut new_requests = new_requests;
                    for req in pending_coercions {
                        match self.pending_coercion(
                            req,
                            ix,
                            exec.fn_ref,
                            &exec.args,
                            &mut exec.read_set,
                        ) {
                            Ok(PendingForce::Invoke(invoke)) => new_requests.push(invoke),
                            Ok(PendingForce::Ready { input_slot, value }) => {
                                if exec.ready.len() <= input_slot {
                                    exec.ready.resize(input_slot + 1, false);
                                    exec.awaited.resize(input_slot + 1, 0);
                                }
                                exec.ready[input_slot] = true;
                                exec.awaited[input_slot] = value;
                            }
                            Err(err) => return Err(err),
                        }
                    }
                    for req in pending_invokes {
                        match self.pending_invocation_call(req) {
                            Ok(invoke) => new_requests.push(invoke),
                            Err(err) => return Err(err),
                        }
                    }
                    for req in new_requests {
                        let req_key = self.memo_key(req.fn_ref, &req.args);
                        self.emit(DriveEvent::Demanded { fn_hash: req_key.0 });
                        if exec.ready.len() <= req.input_slot {
                            exec.ready.resize(req.input_slot + 1, false);
                            exec.awaited.resize(req.input_slot + 1, 0);
                        }
                        let hit = if let Some(entry) = self.memo.get(&req_key).cloned() {
                            Some((entry, MemoHitKind::Exact))
                        } else if let Some(entry) =
                            self.projection_memo_hit(req.fn_ref, &req.args, &req_key)?
                        {
                            Some((entry, MemoHitKind::Projection))
                        } else {
                            self.semantic_memo_hit(req.fn_ref, &req.args, &req_key)?
                                .map(|entry| (entry, MemoHitKind::Semantic))
                        };
                        if let Some((entry, hit_kind)) = hit {
                            // Mechanism 1: memo hit — the slot fills
                            // synchronously, no task exists.
                            match hit_kind {
                                MemoHitKind::Exact => {
                                    self.emit(DriveEvent::MemoHit { fn_hash: req_key.0 });
                                }
                                MemoHitKind::Projection => {
                                    let verified = entry.read_set.len();
                                    self.memo.insert(req_key.clone(), entry.clone());
                                    self.index_memo_candidate(req.fn_ref, &req.args, &req_key);
                                    self.emit(DriveEvent::MemoProjectionHit {
                                        fn_hash: req_key.0,
                                        verified,
                                    });
                                }
                                MemoHitKind::Semantic => {
                                    let verified =
                                        self.lowered(req.fn_ref).semantic_comparators.len();
                                    self.memo.insert(req_key.clone(), entry.clone());
                                    self.index_memo_candidate(req.fn_ref, &req.args, &req_key);
                                    self.emit(DriveEvent::MemoSemanticHit {
                                        fn_hash: req_key.0,
                                        verified,
                                    });
                                }
                            }
                            exec.ready[req.input_slot] = true;
                            exec.awaited[req.input_slot] = entry.value;
                            let remapped = remap_read_set_for_caller(
                                &self.lowered(exec.fn_ref).arg_schemas,
                                &exec.args,
                                &req.args,
                                &entry.read_set,
                                &self.store.borrow(),
                                &self.descriptors,
                                &self.schemas,
                            );
                            exec.read_set.extend(&remapped);
                        } else {
                            exec.feeds.insert(req.input_slot, req_key.clone());
                            let already_running = in_flight.is_running(&req_key);
                            debug_assert!(
                                !waiters.contains_key(&req_key) || already_running,
                                "waiter without matching in-flight invocation"
                            );
                            waiters
                                .entry(req_key.clone())
                                .or_default()
                                .push((req.caller, req.input_slot));
                            if !already_running {
                                let child = self.spawn(
                                    &mut executions,
                                    req.fn_ref,
                                    req_key.clone(),
                                    &req.args,
                                )?;
                                let inserted = in_flight.started(req_key);
                                debug_assert!(inserted, "child invocation was already in flight");
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
                        self.emit(DriveEvent::ParkedOn {
                            fn_hash: exec.key.0,
                        });
                    }
                    executions[ix] = Some(exec);
                    continue;
                }
                Burst::Error(err) => return Err(err),
            }
        }

        self.memo
            .get(&key)
            .map(|entry| entry.value)
            .ok_or_else(|| "root invocation did not complete".to_string())
    }

    fn harvest_pending_runs(
        &mut self,
        block: bool,
        executions: &mut [Option<Execution>],
        runnable: &mut RunnableExecutions,
        pending_work: &mut PendingWork,
    ) -> Result<(), String> {
        let (ready_runs, ready_fetches) = loop {
            let mut ready_runs = Vec::new();
            for &run_id in pending_work.run_waiters.keys() {
                if self.poll_run_completion(run_id)? {
                    ready_runs.push(run_id);
                }
            }

            let mut ready_fetches = Vec::new();
            for (&run_id, run) in pending_work.pending_fetches.iter() {
                match run.completion.try_recv() {
                    Ok(result) => ready_fetches.push((run_id, result)),
                    Err(mpsc::TryRecvError::Empty) => {}
                    Err(mpsc::TryRecvError::Disconnected) => {
                        return Err(format!(
                            "fetch run {run_id} completion channel disconnected"
                        ));
                    }
                }
            }

            if !block || !ready_runs.is_empty() || !ready_fetches.is_empty() {
                break (ready_runs, ready_fetches);
            }
            std::thread::sleep(Duration::from_millis(1));
        };

        let mut woken = BTreeSet::new();
        for run_id in ready_runs {
            let Some(waiters) = pending_work.run_waiters.remove(&run_id) else {
                continue;
            };
            for (waiter_ix, request) in waiters {
                let exec = executions[waiter_ix]
                    .as_mut()
                    .expect("parked run waiter exists");
                self.fill_tree_project_waiter(exec, request)?;
                woken.insert(waiter_ix);
            }
        }

        for (run_id, result) in ready_fetches {
            let run = pending_work
                .pending_fetches
                .remove(&run_id)
                .expect("ready fetch run exists");
            pending_work.in_flight_fetches.remove(&run.key);
            let value = self.finish_fetch_run(run, result?)?;
            if let Some(waiters) = pending_work.fetch_waiters.remove(&run_id) {
                for (waiter_ix, input_slot) in waiters {
                    let exec = executions[waiter_ix]
                        .as_mut()
                        .expect("parked fetch waiter exists");
                    fill_execution_input(exec, input_slot, value);
                    woken.insert(waiter_ix);
                }
            }
        }
        runnable.extend(woken);

        Ok(())
    }

    fn fill_tree_project_waiter(
        &mut self,
        exec: &mut Execution,
        request: TreeProjectRequest,
    ) -> Result<(), String> {
        let (input_slot, value) = match request {
            TreeProjectRequest::Tree(req) => {
                self.project_request(req, exec.fn_ref, &exec.args, &mut exec.read_set)?
            }
            TreeProjectRequest::Text(req) => {
                self.text_project_request(req, exec.fn_ref, &exec.args, &mut exec.read_set)?
            }
        };
        fill_execution_input(exec, input_slot, value);
        Ok(())
    }

    fn project_request_pending_run(&mut self, tree: i64) -> Result<Option<u64>, String> {
        let TreeEntry::Exec(run_id) = self.store.borrow().tree_entry(tree)? else {
            return Ok(None);
        };
        if self
            .runs
            .get(&run_id)
            .and_then(|run| run.completed.as_ref())
            .is_some()
        {
            return Ok(None);
        }
        if self.exec_backend.is_none()
            && self
                .runs
                .get(&run_id)
                .and_then(|run| run.remote.as_ref())
                .is_none()
        {
            return Ok(None);
        }
        self.ensure_run_started(run_id)?;
        if self.poll_run_completion(run_id)? {
            Ok(None)
        } else {
            Ok(Some(run_id))
        }
    }

    fn poll_run_completion(&mut self, run_id: u64) -> Result<bool, String> {
        if self
            .runs
            .get(&run_id)
            .ok_or_else(|| format!("run {run_id}"))?
            .completed
            .is_some()
        {
            return Ok(true);
        }
        let result = {
            let run = self
                .runs
                .get(&run_id)
                .ok_or_else(|| format!("run {run_id}"))?;
            let Some(completion) = run.completion.as_ref() else {
                return Ok(false);
            };
            match completion.try_recv() {
                Ok(result) => Some(result),
                Err(mpsc::TryRecvError::Empty) => None,
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err(format!("run {run_id} completion channel disconnected"));
                }
            }
        };
        if let Some(result) = result {
            let (outcome, event) = result?;
            let run = self.runs.get_mut(&run_id).expect("run checked");
            run.completed = Some((outcome, event));
            Ok(true)
        } else {
            Ok(false)
        }
    }

    fn await_run_completion(
        &mut self,
        run_id: u64,
    ) -> Result<(crate::exec::Outcome, crate::exec::ExecEvent), String> {
        if let Some(completed) = self
            .runs
            .get(&run_id)
            .ok_or_else(|| format!("run {run_id}"))?
            .completed
            .clone()
        {
            return Ok(completed);
        }
        let completion = {
            let run = self
                .runs
                .get_mut(&run_id)
                .ok_or_else(|| format!("run {run_id}"))?;
            run.completion.take()
        };
        if let Some(completion) = completion {
            completion
                .recv()
                .map_err(|_| format!("run {run_id} completion channel disconnected"))?
        } else {
            let remote = self
                .runs
                .get(&run_id)
                .and_then(|run| run.remote.clone())
                .ok_or_else(|| format!("run {run_id} has no completion source"))?;
            remote.flush()
        }
    }

    fn spawn(
        &mut self,
        executions: &mut Vec<Option<Execution>>,
        fn_ref: FnRef,
        key: CanonMemoKey,
        args: &[i64],
    ) -> Result<usize, String> {
        let fn_hash = self.lowered(fn_ref).hash;
        self.emit(DriveEvent::Spawned { fn_hash });
        self.emit(DriveEvent::SpawnedInvocation {
            fn_hash,
            key_hash: memo_key_hash(&key),
        });
        let lowered = &self.lowered(fn_ref);
        let task = self.lane.spawn(&self.program, lowered, args)?;
        executions.push(Some(Execution {
            task,
            fn_ref,
            key,
            args: args.to_vec(),
            molten: MoltenStore::default(),
            read_set: ProjectionReadSet::default(),
            ready: Vec::new(),
            awaited: Vec::new(),
            feeds: HashMap::new(),
        }));
        Ok(executions.len() - 1)
    }

    /// Run one execution until done or blocked, capturing INVOKE
    /// requests raised during the burst.
    fn burst(&mut self, exec: &mut Execution, exec_ix: usize) -> Burst {
        let lowered = &self.lowered(exec.fn_ref);
        let invoke_region = lowered.invoke_region as usize;
        let store_alloc_region = lowered.store_alloc_region as usize;
        let store_read_region = lowered.store_read_region as usize;
        let store_tag_region = lowered.store_tag_region as usize;
        let primitive_region = lowered.primitive_region as usize;
        let task_has_native_array_load = self.program.fns[lowered.task_fn.0 as usize]
            .code
            .iter()
            .any(|op| matches!(op, Op::LoadArrayWord { .. }));
        loop {
            // Size the input arrays BEFORE the burst (slots the body
            // registers this burst get sized on the next iteration —
            // the driver loop always re-enters after filling).
            let max_slot = exec.feeds.keys().copied().max().map_or(0, |m| m + 1);
            let want = max_slot.max(exec.ready.len()).max(16);
            exec.ready.resize(want, false);
            exec.awaited.resize(want, 0);

            let mut requests: Vec<InvokeRequest> = Vec::new();
            let mut project_requests: Vec<ProjectRequest> = Vec::new();
            let mut text_project_requests: Vec<TextProjectRequest> = Vec::new();
            let mut exec_requests: Vec<ExecRequest> = Vec::new();
            let mut fetch_requests: Vec<FetchRequest> = Vec::new();
            let mut doc_parse_requests: Vec<DocParseRequest> = Vec::new();
            let mut crate_archive_requests: Vec<CrateArchiveRequest> = Vec::new();
            let mut option_unwraps: Vec<OptionUnwrapRequest> = Vec::new();
            let mut pending_coercions: Vec<PendingCoerceRequest> = Vec::new();
            let mut pending_invokes: Vec<PendingInvokeRequest> = Vec::new();
            let descriptors = &self.descriptors;
            let schemas = &self.schemas;
            let schema_tables = &self.schemas;
            let store_cell = &self.store;
            let molten_cell = RefCell::new(&mut exec.molten);
            let ast_roots_cell = RefCell::new(&mut self.ast_roots);
            let oci_file_memo_cell = RefCell::new(&mut self.oci_file_memo);
            let journal_cell = RefCell::new(&mut self.journal);
            let clock_cell = RefCell::new(&mut self.trace_clock);
            let molten_stats = RefCell::new(&mut self.molten_stats);
            let lowered_fns = &self.fns;
            let store_events = RefCell::new(Vec::new());
            let projection_reads = RefCell::new(Vec::new());
            let host_error = RefCell::new(None::<String>);
            let exec_arg_schemas = lowered_fns[exec.fn_ref.index()].arg_schemas.clone();
            let exec_args = exec.args.clone();
            let force_molten_copy = self.force_molten_copy;
            let mut invoke = |frame: &mut [u8]| {
                let word = |i: usize| {
                    i64::from_le_bytes(
                        frame[invoke_region + i * 8..invoke_region + i * 8 + 8]
                            .try_into()
                            .expect("invoke region word"),
                    )
                };
                let input_slot = word(0) as usize;
                let fn_ref = FnRef::from_frame_word(word(1));
                let argc = word(2) as usize;
                let mut args = (0..argc).map(|k| word(3 + k)).collect::<Vec<_>>();
                let arg_schemas = &lowered_fns[fn_ref.index()].arg_schemas;
                for (arg, schema) in args.iter_mut().zip(arg_schemas) {
                    let was_molten = !schema_is_inline_word(schemas, schema)
                        && matches!(Handle::from_word(*arg), Handle::Molten(_));
                    let (interned, deduped) = intern_molten_word(
                        &mut store_cell.borrow_mut(),
                        &mut molten_cell.borrow_mut(),
                        descriptors,
                        schemas,
                        schema_tables,
                        schema,
                        *arg,
                    )
                    .unwrap_or_else(|err| panic!("{err}"));
                    if was_molten {
                        store_events.borrow_mut().push(DriveEvent::StoreAlloc {
                            schema_ref: hash_u64(schema),
                            deduped,
                        });
                    }
                    *arg = interned;
                }
                requests.push(InvokeRequest {
                    caller: exec_ix,
                    input_slot,
                    fn_ref,
                    args,
                });
            };

            let mut store_alloc = |frame: &mut [u8]| {
                let dst_slot = read_frame_word(frame, store_alloc_region) as usize;
                let type_ref = read_frame_word(frame, store_alloc_region + 8);
                let schema =
                    schema_name_for(type_ref, schema_tables).unwrap_or_else(|err| panic!("{err}"));
                let variant_index = read_frame_word(frame, store_alloc_region + 16);
                let field_count = read_frame_word(frame, store_alloc_region + 24) as usize;
                let descriptor = descriptors
                    .get(&schema)
                    .unwrap_or_else(|| panic!("descriptor for schema `{schema}`"));
                let mut bytes = vec![0u8; descriptor.layout.size];
                zero_inactive_enum_payload(
                    &mut bytes,
                    descriptor,
                    usize::try_from(variant_index).expect("variant index non-negative"),
                );
                write_variant_tag(&mut bytes, descriptor, variant_index as u64);
                write_alloc_fields(
                    &mut bytes,
                    schemas,
                    descriptor,
                    usize::try_from(variant_index).expect("variant index non-negative"),
                    field_count,
                    frame,
                    store_alloc_region + 32,
                );
                let fields = (0..field_count)
                    .map(|i| read_frame_word(frame, store_alloc_region + 32 + i * 8))
                    .collect::<Vec<_>>();
                record_whole_args_if_projectable_static(
                    &mut projection_reads.borrow_mut(),
                    &exec_arg_schemas,
                    &exec_args,
                    &store_cell.borrow(),
                    descriptors,
                    schemas,
                    fields,
                );
                let handle = molten_cell.borrow_mut().alloc(MoltenValue::Record {
                    schema: schema.clone(),
                    bytes,
                });
                write_frame_word(frame, dst_slot, handle);
            };

            let mut store_read = |frame: &mut [u8]| {
                if host_error.borrow().is_some() {
                    return;
                }
                let dst_slot = read_frame_word(frame, store_read_region) as usize;
                let mut handle = read_frame_word(frame, store_read_region + 8);
                let field_index = read_frame_word(frame, store_read_region + 16) as usize;
                match Handle::from_word(handle) {
                    Handle::Molten(molten_handle) => {
                        let local_read = {
                            let molten = molten_cell.borrow();
                            molten
                                .entry(molten_handle)
                                .and_then(|entry| match &entry.value {
                                    MoltenValue::Record { schema, bytes } => {
                                        let descriptor =
                                            descriptors.get(schema).unwrap_or_else(|| {
                                                panic!("descriptor for schema `{schema}`")
                                            });
                                        let field =
                                            field_descriptor(descriptor, bytes, field_index);
                                        let offset = field_offset(descriptor, bytes, field_index);
                                        Some(read_word_at(bytes, offset, field.layout.size))
                                    }
                                    _ => None,
                                })
                        };
                        if let Some(value) = local_read {
                            write_frame_word(frame, dst_slot, value);
                            return;
                        }
                        let schema = molten_cell
                            .borrow()
                            .entry(molten_handle)
                            .and_then(|entry| match &entry.value {
                                MoltenValue::Interned(handle) => store_cell
                                    .borrow()
                                    .entry(*handle)
                                    .map(|entry| entry.schema.clone()),
                                _ => None,
                            })
                            .unwrap_or_else(|| panic!("molten record handle {handle}"));
                        let (interned, deduped) = intern_molten_word(
                            &mut store_cell.borrow_mut(),
                            &mut molten_cell.borrow_mut(),
                            descriptors,
                            schemas,
                            schema_tables,
                            &schema,
                            handle,
                        )
                        .unwrap_or_else(|err| panic!("{err}"));
                        store_events.borrow_mut().push(DriveEvent::StoreAlloc {
                            schema_ref: hash_u64(&schema),
                            deduped,
                        });
                        handle = interned;
                    }
                    Handle::Store(store_ix) => {
                        handle = store_ix.to_word();
                    }
                }
                let store = store_cell.borrow();
                let entry = store
                    .entry(handle)
                    .unwrap_or_else(|| panic!("store handle {handle}"));
                let descriptor = descriptors
                    .get(&entry.schema)
                    .unwrap_or_else(|| panic!("descriptor for schema `{}`", entry.schema));
                let field = field_descriptor(descriptor, &entry.bytes, field_index);
                let offset = field_offset(descriptor, &entry.bytes, field_index);
                let value = read_word_at(&entry.bytes, offset, field.layout.size);
                let observed = canonical_word_hash_for_descriptor(&store, schemas, field, value);
                let projection_context = ProjectionRecordContext {
                    arg_schemas: &exec_arg_schemas,
                    args: &exec_args,
                    store: &store,
                    descriptors,
                    schemas,
                };
                record_projection_for_matching_args_static(
                    &mut projection_reads.borrow_mut(),
                    &projection_context,
                    handle,
                    ProjectionPath::Field {
                        schema: entry.schema.clone(),
                        field_index,
                    },
                    observed,
                );
                write_frame_word(frame, dst_slot, value);
            };

            let mut store_tag = |frame: &mut [u8]| {
                let dst_slot = read_frame_word(frame, store_tag_region) as usize;
                let mut handle = read_frame_word(frame, store_tag_region + 8);
                match Handle::from_word(handle) {
                    Handle::Molten(molten_handle) => {
                        let local_tag = {
                            let molten = molten_cell.borrow();
                            molten
                                .entry(molten_handle)
                                .and_then(|entry| match &entry.value {
                                    MoltenValue::Record { schema, bytes } => {
                                        let descriptor =
                                            descriptors.get(schema).unwrap_or_else(|| {
                                                panic!("descriptor for schema `{schema}`")
                                            });
                                        Some(read_variant_tag(bytes, descriptor))
                                    }
                                    _ => None,
                                })
                        };
                        if let Some(tag) = local_tag {
                            write_frame_word(
                                frame,
                                dst_slot,
                                i64::try_from(tag).expect("variant tag fits i64"),
                            );
                            return;
                        }
                        let schema = molten_cell
                            .borrow()
                            .entry(molten_handle)
                            .and_then(|entry| match &entry.value {
                                MoltenValue::Interned(handle) => store_cell
                                    .borrow()
                                    .entry(*handle)
                                    .map(|entry| entry.schema.clone()),
                                _ => None,
                            })
                            .unwrap_or_else(|| panic!("molten record handle {handle}"));
                        let (interned, deduped) = intern_molten_word(
                            &mut store_cell.borrow_mut(),
                            &mut molten_cell.borrow_mut(),
                            descriptors,
                            schemas,
                            schema_tables,
                            &schema,
                            handle,
                        )
                        .unwrap_or_else(|err| panic!("{err}"));
                        store_events.borrow_mut().push(DriveEvent::StoreAlloc {
                            schema_ref: hash_u64(&schema),
                            deduped,
                        });
                        handle = interned;
                    }
                    Handle::Store(store_ix) => {
                        handle = store_ix.to_word();
                    }
                }
                let store = store_cell.borrow();
                let entry = store
                    .entry(handle)
                    .unwrap_or_else(|| panic!("store handle {handle}"));
                let descriptor = descriptors
                    .get(&entry.schema)
                    .unwrap_or_else(|| panic!("descriptor for schema `{}`", entry.schema));
                let tag = read_variant_tag(&entry.bytes, descriptor);
                let observed = canonical_scalar_hash(schemas, "Int", tag as i64);
                let projection_context = ProjectionRecordContext {
                    arg_schemas: &exec_arg_schemas,
                    args: &exec_args,
                    store: &store,
                    descriptors,
                    schemas,
                };
                record_projection_for_matching_args_static(
                    &mut projection_reads.borrow_mut(),
                    &projection_context,
                    handle,
                    ProjectionPath::Tag {
                        schema: entry.schema.clone(),
                    },
                    observed,
                );
                write_frame_word(
                    frame,
                    dst_slot,
                    i64::try_from(tag).expect("variant tag fits i64"),
                );
            };
            let mut map_empty = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, store_alloc_region) as usize;
                    let schema = schema_name_for(
                        read_frame_word(frame, store_alloc_region + 8),
                        schema_tables,
                    )?;
                    let handle = molten_cell.borrow_mut().alloc_with_carried_map_rows(
                        MoltenValue::Map {
                            schema: schema.clone(),
                            pairs: Vec::new(),
                        },
                        Some(CarriedMapRows {
                            ordered: Vec::new(),
                        }),
                    );
                    write_frame_word(frame, dst_slot, handle);
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut map_insert = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, store_alloc_region) as usize;
                    let map_handle = read_frame_word(frame, store_alloc_region + 8);
                    let map_schema = schema_name_for(
                        read_frame_word(frame, store_alloc_region + 16),
                        schema_tables,
                    )?;
                    let key_schema = schema_name_for(
                        read_frame_word(frame, store_alloc_region + 24),
                        schema_tables,
                    )?;
                    let key_word = canonicalize_word_for_schema(
                        schemas,
                        &key_schema,
                        read_frame_word(frame, store_alloc_region + 32),
                    );
                    let value_schema = schema_name_for(
                        read_frame_word(frame, store_alloc_region + 40),
                        schema_tables,
                    )?;
                    let value_realization =
                        Realization::from_word(read_frame_word(frame, store_alloc_region + 56))?;
                    let logical_value_schema =
                        realized_value_schema(&value_schema).unwrap_or(&value_schema);
                    let stored_value_schema = match value_realization {
                        Realization::Ready => logical_value_schema.to_string(),
                        Realization::Pending => pending_schema(logical_value_schema),
                    };
                    let value_word = canonicalize_word_for_schema(
                        schemas,
                        &stored_value_schema,
                        read_frame_word(frame, store_alloc_region + 48),
                    );
                    let mut key_word = key_word;
                    let mut value_word = value_word;
                    key_word = intern_molten_word(
                        &mut store_cell.borrow_mut(),
                        &mut molten_cell.borrow_mut(),
                        descriptors,
                        schemas,
                        schema_tables,
                        &key_schema,
                        key_word,
                    )?
                    .0;
                    value_word = intern_molten_word(
                        &mut store_cell.borrow_mut(),
                        &mut molten_cell.borrow_mut(),
                        descriptors,
                        schemas,
                        schema_tables,
                        &stored_value_schema,
                        value_word,
                    )?
                    .0;
                    match Handle::from_word(map_handle) {
                        Handle::Molten(_) => {}
                        Handle::Store(_) => {
                            record_whole_args_if_projectable_static(
                                &mut projection_reads.borrow_mut(),
                                &exec_arg_schemas,
                                &exec_args,
                                &store_cell.borrow(),
                                descriptors,
                                schemas,
                                [map_handle, key_word, value_word],
                            );
                        }
                    }
                    let (stored_schema, mut pairs, mut carried_map_rows) =
                        match Handle::from_word(map_handle) {
                            Handle::Molten(molten_handle) => {
                                if let Some(entry) = molten_cell.borrow().entry(molten_handle) {
                                    match &entry.value {
                                        MoltenValue::Map { schema, pairs } => (
                                            schema.clone(),
                                            pairs.clone(),
                                            entry.carried_map_rows.clone(),
                                        ),
                                        MoltenValue::Interned(handle) => {
                                            let (schema, pairs, carried_rows) = store_cell
                                                .borrow_mut()
                                                .map_pairs_with_carried_rows_cached(
                                                    *handle,
                                                    schemas,
                                                    schema_tables,
                                                )?;
                                            (schema, pairs, Some(carried_rows))
                                        }
                                        _ => {
                                            return Err(format!(
                                                "molten handle {map_handle} is not a Map"
                                            ));
                                        }
                                    }
                                } else {
                                    let (schema, pairs, carried_rows) = store_cell
                                        .borrow_mut()
                                        .map_pairs_with_carried_rows_cached(
                                            map_handle,
                                            schemas,
                                            schema_tables,
                                        )?;
                                    (schema, pairs, Some(carried_rows))
                                }
                            }
                            Handle::Store(store_ix) => {
                                let (schema, pairs, carried_rows) =
                                    store_cell.borrow_mut().map_pairs_with_carried_rows_cached(
                                        store_ix.to_word(),
                                        schemas,
                                        schema_tables,
                                    )?;
                                (schema, pairs, Some(carried_rows))
                            }
                        };
                    if stored_schema != map_schema {
                        pairs = promote_map_pairs_to_realized(&stored_schema, &map_schema, pairs)?;
                        carried_map_rows = Some(CarriedMapRows {
                            ordered: canonical_map_pairs(
                                &store_cell.borrow(),
                                pairs.clone(),
                                descriptors,
                                schemas,
                                schema_tables,
                            )?,
                        });
                    } else if carried_map_rows.is_none() {
                        carried_map_rows = Some(CarriedMapRows {
                            ordered: canonical_map_pairs(
                                &store_cell.borrow(),
                                pairs.clone(),
                                descriptors,
                                schemas,
                                schema_tables,
                            )?,
                        });
                    }
                    let new_pair = MapPair {
                        key_schema,
                        key_word,
                        value_schema: stored_value_schema,
                        value_word,
                        value_realization: realized_value_schema(&value_schema)
                            .map(|_| value_realization),
                    };
                    carried_map_rows = Some(carried_map_rows_after_insert(
                        &store_cell.borrow(),
                        descriptors,
                        schemas,
                        schema_tables,
                        carried_map_rows.expect("map insert has carried rows"),
                        new_pair.clone(),
                    )?);
                    let handle = if !force_molten_copy
                        && let Handle::Molten(molten_handle) = Handle::from_word(map_handle)
                        && let Some(entry) = molten_cell.borrow_mut().entry_mut(molten_handle)
                        && entry.refs == 1
                        && let MoltenValue::Map {
                            schema,
                            pairs: entry_pairs,
                        } = &mut entry.value
                    {
                        *schema = map_schema.clone();
                        *entry_pairs = pairs;
                        entry_pairs.push(new_pair);
                        entry.carried_map_rows = carried_map_rows;
                        map_handle
                    } else {
                        pairs.push(new_pair);
                        molten_cell.borrow_mut().alloc_with_carried_map_rows(
                            MoltenValue::Map {
                                schema: map_schema.clone(),
                                pairs,
                            },
                            carried_map_rows,
                        )
                    };
                    write_frame_word(frame, dst_slot, handle);
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut map_get = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, store_alloc_region) as usize;
                    let mut map_handle = read_frame_word(frame, store_alloc_region + 8);
                    let value_schema = schema_name_for(
                        read_frame_word(frame, store_alloc_region + 16),
                        schema_tables,
                    )?;
                    let key_schema = schema_name_for(
                        read_frame_word(frame, store_alloc_region + 24),
                        schema_tables,
                    )?;
                    let key_word = canonicalize_word_for_schema(
                        schemas,
                        &key_schema,
                        read_frame_word(frame, store_alloc_region + 32),
                    );
                    let map_schema = match Handle::from_word(map_handle) {
                        Handle::Molten(molten_handle) => {
                            if let Some(entry) = molten_cell.borrow().entry(molten_handle) {
                                match &entry.value {
                                    MoltenValue::Map { schema, .. } => schema.clone(),
                                    MoltenValue::Interned(handle) => store_cell
                                        .borrow()
                                        .entry(*handle)
                                        .ok_or_else(|| format!("store handle {handle}"))?
                                        .schema
                                        .clone(),
                                    _ => {
                                        return Err(format!(
                                            "molten handle {map_handle} is not a Map"
                                        ));
                                    }
                                }
                            } else {
                                store_cell
                                    .borrow()
                                    .entry(map_handle)
                                    .ok_or_else(|| format!("store handle {map_handle}"))?
                                    .schema
                                    .clone()
                            }
                        }
                        Handle::Store(store_ix) => store_cell
                            .borrow()
                            .entry(store_ix.to_word())
                            .ok_or_else(|| format!("store handle {map_handle}"))?
                            .schema
                            .clone(),
                    };
                    match Handle::from_word(map_handle) {
                        Handle::Molten(_) => {
                            let (interned, deduped) = intern_molten_word(
                                &mut store_cell.borrow_mut(),
                                &mut molten_cell.borrow_mut(),
                                descriptors,
                                schemas,
                                schema_tables,
                                &map_schema,
                                map_handle,
                            )?;
                            store_events.borrow_mut().push(DriveEvent::StoreAlloc {
                                schema_ref: hash_u64(&map_schema),
                                deduped,
                            });
                            map_handle = interned;
                        }
                        Handle::Store(store_ix) => {
                            map_handle = store_ix.to_word();
                        }
                    }
                    let (handle, _) = store_cell.borrow_mut().map_get(
                        map_handle,
                        &key_schema,
                        key_word,
                        &value_schema,
                        schemas,
                        schema_tables,
                    )?;
                    let observed = store_cell
                        .borrow()
                        .entry(handle)
                        .expect("map_get allocated option")
                        .content_hash;
                    let store = store_cell.borrow();
                    let key_hash =
                        canonical_word_hash_in_store(&store, schemas, &key_schema, key_word);
                    let projection_context = ProjectionRecordContext {
                        arg_schemas: &exec_arg_schemas,
                        args: &exec_args,
                        store: &store,
                        descriptors,
                        schemas,
                    };
                    record_projection_for_matching_args_static(
                        &mut projection_reads.borrow_mut(),
                        &projection_context,
                        map_handle,
                        ProjectionPath::MapGet {
                            map_schema,
                            key_schema,
                            key_hash,
                            value_schema,
                        },
                        observed,
                    );
                    write_frame_word(frame, dst_slot, handle);
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut option_unwrap = |frame: &mut [u8]| {
                let input_slot = read_frame_word(frame, store_alloc_region) as usize;
                let handle = read_frame_word(frame, store_alloc_region + 8);
                let realization_slot = read_frame_word(frame, store_alloc_region + 16);
                option_unwraps.push(OptionUnwrapRequest {
                    input_slot,
                    realization_slot: usize::try_from(realization_slot).ok(),
                    option: handle,
                });
            };

            let mut native_option_unwrap_none = |_frame: &mut [u8]| {
                *host_error.borrow_mut() = Some("unwrap on None".to_string());
            };

            let mut acquire = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let kind_ref = read_frame_word(frame, primitive_region + 8);
                    let kind = schema_name_for(kind_ref, schema_tables)?;
                    let mut target = read_frame_word(frame, primitive_region + 16);
                    match Handle::from_word(target) {
                        Handle::Molten(_) => {
                            let (interned, deduped) = intern_molten_word(
                                &mut store_cell.borrow_mut(),
                                &mut molten_cell.borrow_mut(),
                                descriptors,
                                schemas,
                                schema_tables,
                                "Target",
                                target,
                            )?;
                            store_events.borrow_mut().push(DriveEvent::StoreAlloc {
                                schema_ref: hash_u64("Target"),
                                deduped,
                            });
                            target = interned;
                        }
                        Handle::Store(store_ix) => {
                            target = store_ix.to_word();
                        }
                    }
                    record_whole_args_if_projectable_static(
                        &mut projection_reads.borrow_mut(),
                        &exec_arg_schemas,
                        &exec_args,
                        &store_cell.borrow(),
                        descriptors,
                        schemas,
                        [target],
                    );
                    let target_hash = target_hash(store_cell, target)?;
                    let key = format!("acquire:{kind}:{target_hash:x}");
                    let mut journal = journal_cell.borrow_mut();
                    let (handle, replayed) = if let Some(handle) = journal.get(&key).copied() {
                        (handle, true)
                    } else {
                        let handle = store_cell
                            .borrow_mut()
                            .alloc_raw(&kind, key.clone().into_bytes(), schemas)
                            .0;
                        journal.insert(key.clone(), handle);
                        (handle, false)
                    };
                    write_frame_word(frame, dst_slot, handle);
                    let timestamp_us = {
                        let mut clock = clock_cell.borrow_mut();
                        let value = **clock;
                        **clock = value.saturating_add(1);
                        value
                    };
                    store_events.borrow_mut().push(DriveEvent::Observation {
                        key: hash_u64(&key),
                        replayed,
                        key_text: key,
                        timestamp_us,
                    });
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut array_alloc = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let elem_schema = schema_name_for(
                        read_frame_word(frame, primitive_region + 8),
                        schema_tables,
                    )?;
                    let count = usize::try_from(read_frame_word(frame, primitive_region + 16))
                        .map_err(|_| "negative array length".to_string())?;
                    let mut words = (0..count)
                        .map(|i| read_frame_word(frame, primitive_region + 24 + i * 8))
                        .collect::<Vec<_>>();
                    for word in &mut words {
                        *word = intern_molten_word(
                            &mut store_cell.borrow_mut(),
                            &mut molten_cell.borrow_mut(),
                            descriptors,
                            schemas,
                            schema_tables,
                            &elem_schema,
                            *word,
                        )?
                        .0;
                    }
                    let handle = molten_cell
                        .borrow_mut()
                        .alloc(MoltenValue::ArrayWords { elem_schema, words });
                    write_frame_word(frame, dst_slot, handle);
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut array_map_pending = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let array_handle = read_frame_word(frame, primitive_region + 8);
                    let fn_ref = FnRef::new(
                        usize::try_from(read_frame_word(frame, primitive_region + 16))
                            .map_err(|_| "negative fn ref".to_string())?,
                    );
                    let arg_count = usize::try_from(read_frame_word(frame, primitive_region + 24))
                        .map_err(|_| "negative map arg count".to_string())?;
                    let arg_specs = (0..arg_count)
                        .map(|index| {
                            let at = primitive_region + 32 + index * 16;
                            Ok((read_frame_word(frame, at), read_frame_word(frame, at + 8)))
                        })
                        .collect::<Result<Vec<_>, String>>()?;
                    let words = match molten_cell.borrow().array_entry(
                        &store_cell.borrow(),
                        array_handle,
                        schema_tables,
                    )? {
                        ArrayEntry::Words { words, .. } => words,
                        ArrayEntry::Pending { .. } => {
                            return Err("map over pending array is outside slice 4".into());
                        }
                    };
                    match Handle::from_word(array_handle) {
                        Handle::Molten(_) => {}
                        Handle::Store(_) => {
                            record_whole_args_if_projectable_static(
                                &mut projection_reads.borrow_mut(),
                                &exec_arg_schemas,
                                &exec_args,
                                &store_cell.borrow(),
                                descriptors,
                                schemas,
                                [array_handle],
                            );
                        }
                    }
                    let pending = words
                        .into_iter()
                        .map(|word| {
                            let mut args = arg_specs
                                .iter()
                                .map(|(kind, value)| match kind {
                                    0 => *value,
                                    1 => word,
                                    other => panic!("unknown map arg kind {other}"),
                                })
                                .collect::<Vec<_>>();
                            for (arg, schema) in args
                                .iter_mut()
                                .zip(&lowered_fns[fn_ref.index()].arg_schemas)
                            {
                                *arg = intern_molten_word(
                                    &mut store_cell.borrow_mut(),
                                    &mut molten_cell.borrow_mut(),
                                    descriptors,
                                    schemas,
                                    schema_tables,
                                    schema,
                                    *arg,
                                )?
                                .0;
                            }
                            let invocation = pending_invocation_for(
                                &lowered_fns[fn_ref.index()],
                                store_cell,
                                schemas,
                                args,
                            );
                            Ok::<i64, String>(
                                store_cell
                                    .borrow_mut()
                                    .alloc_pending(
                                        &lowered_fns[fn_ref.index()].return_schema,
                                        invocation,
                                    )
                                    .0,
                            )
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    let (handle, _) = store_cell.borrow_mut().alloc_array_pending(
                        &lowered_fns[fn_ref.index()].return_schema,
                        pending,
                        schemas,
                        schema_tables,
                    )?;
                    write_frame_word(frame, dst_slot, handle);
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut array_collect = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let array_handle = read_frame_word(frame, primitive_region + 8);
                    let array_entry = molten_cell.borrow().array_entry(
                        &store_cell.borrow(),
                        array_handle,
                        schema_tables,
                    )?;
                    match Handle::from_word(array_handle) {
                        Handle::Molten(_) => {}
                        Handle::Store(_) => {
                            record_whole_args_if_projectable_static(
                                &mut projection_reads.borrow_mut(),
                                &exec_arg_schemas,
                                &exec_args,
                                &store_cell.borrow(),
                                descriptors,
                                schemas,
                                [array_handle],
                            );
                        }
                    }
                    match array_entry {
                        ArrayEntry::Pending { pending, .. } => {
                            let (handle, _) =
                                store_cell.borrow_mut().alloc_tree_merge(pending, schemas);
                            write_frame_word(frame, dst_slot, handle);
                        }
                        ArrayEntry::Words {
                            elem_schema,
                            mut words,
                        } => {
                            words.sort_by(|a, b| {
                                {
                                    let store = store_cell.borrow();
                                    compare_words(
                                        &store,
                                        descriptors,
                                        schemas,
                                        schema_tables,
                                        &elem_schema,
                                        *a,
                                        *b,
                                    )
                                }
                                .expect("array collect comparison")
                            });
                            let (handle, _) = if schemas.is_external(&elem_schema, "Tree") {
                                store_cell.borrow_mut().alloc_tree_merge(words, schemas)
                            } else {
                                store_cell.borrow_mut().alloc_array_words(
                                    &elem_schema,
                                    words,
                                    schemas,
                                    schema_tables,
                                )?
                            };
                            write_frame_word(frame, dst_slot, handle);
                        }
                    }
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut tree_project = |frame: &mut [u8]| {
                let input_slot = read_frame_word(frame, primitive_region) as usize;
                let tree = read_frame_word(frame, primitive_region + 8);
                let path = read_frame_word(frame, primitive_region + 16);
                project_requests.push(ProjectRequest {
                    input_slot,
                    tree,
                    path,
                });
            };

            let mut tree_text = |frame: &mut [u8]| {
                let input_slot = read_frame_word(frame, primitive_region) as usize;
                let tree = read_frame_word(frame, primitive_region + 8);
                let path = read_frame_word(frame, primitive_region + 16);
                text_project_requests.push(TextProjectRequest {
                    input_slot,
                    tree,
                    path,
                });
            };

            let mut exec_host = |frame: &mut [u8]| {
                let input_slot = read_frame_word(frame, primitive_region) as usize;
                let command = match read_frame_word(frame, primitive_region + 8) {
                    0 => "cc",
                    1 => "ar",
                    2 => "rustc",
                    3 => "build_script",
                    other => {
                        *host_error.borrow_mut() = Some(format!("unknown command kind {other}"));
                        return;
                    }
                }
                .to_string();
                let capability = read_frame_word(frame, primitive_region + 16);
                let part_count =
                    match usize::try_from(read_frame_word(frame, primitive_region + 24)) {
                        Ok(value) => value,
                        Err(_) => {
                            *host_error.borrow_mut() = Some("negative command part count".into());
                            return;
                        }
                    };
                let span_start = read_frame_word(frame, primitive_region + 32);
                let span_end = read_frame_word(frame, primitive_region + 40);
                let span = if span_start >= 0 && span_end >= 0 {
                    Some((span_start as u32, span_end as u32))
                } else {
                    None
                };
                let mut parts = Vec::with_capacity(part_count);
                for index in 0..part_count {
                    let at = primitive_region + 48 + index * 24;
                    let kind = read_frame_word(frame, at);
                    let word = read_frame_word(frame, at + 8);
                    let part = match kind {
                        0 => CommandRequestPart::Token(word),
                        1 => {
                            let schema = match schema_name_for(
                                read_frame_word(frame, at + 16),
                                schema_tables,
                            ) {
                                Ok(schema) => schema,
                                Err(err) => {
                                    *host_error.borrow_mut() = Some(err);
                                    return;
                                }
                            };
                            let word = match intern_molten_word(
                                &mut store_cell.borrow_mut(),
                                &mut molten_cell.borrow_mut(),
                                descriptors,
                                schemas,
                                schema_tables,
                                &schema,
                                word,
                            ) {
                                Ok((word, _)) => word,
                                Err(err) => {
                                    *host_error.borrow_mut() = Some(err);
                                    return;
                                }
                            };
                            CommandRequestPart::Splice(word)
                        }
                        other => {
                            *host_error.borrow_mut() =
                                Some(format!("unknown command part kind {other}"));
                            return;
                        }
                    };
                    parts.push(part);
                }
                exec_requests.push(ExecRequest {
                    input_slot,
                    command,
                    capability,
                    parts,
                    span,
                });
            };

            let mut fetch_host = |frame: &mut [u8]| {
                let input_slot = read_frame_word(frame, primitive_region) as usize;
                let url = read_frame_word(frame, primitive_region + 8);
                let sha256 = read_frame_word(frame, primitive_region + 16);
                fetch_requests.push(FetchRequest {
                    input_slot,
                    url,
                    sha256,
                });
            };

            let mut doc_parse_host = |frame: &mut [u8]| {
                let input_slot = read_frame_word(frame, primitive_region) as usize;
                let kind = match read_frame_word(frame, primitive_region + 8) {
                    0 => DocParseKind::Toml,
                    1 => DocParseKind::Json,
                    2 => DocParseKind::BuildDirectives,
                    3 => DocParseKind::Cfg,
                    4 => DocParseKind::RustcCfg,
                    other => {
                        *host_error.borrow_mut() =
                            Some(format!("unknown document parser kind {other}"));
                        return;
                    }
                };
                let input = read_frame_word(frame, primitive_region + 16);
                let target_schema = match read_frame_word(frame, primitive_region + 24) {
                    0 => None,
                    schema_ref => match schema_name_for(schema_ref, schema_tables) {
                        Ok(schema) => Some(schema),
                        Err(err) => {
                            *host_error.borrow_mut() = Some(err);
                            return;
                        }
                    },
                };
                doc_parse_requests.push(DocParseRequest {
                    input_slot,
                    kind,
                    input,
                    target_schema,
                });
            };

            let mut crate_archive_host = |frame: &mut [u8]| {
                let input_slot = read_frame_word(frame, primitive_region) as usize;
                let input = read_frame_word(frame, primitive_region + 8);
                crate_archive_requests.push(CrateArchiveRequest { input_slot, input });
            };

            let mut doc_get_host = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let doc = read_frame_word(frame, primitive_region + 8);
                    let key = read_frame_word(frame, primitive_region + 16);
                    let key = store_cell.borrow().string_value(key, "String")?;
                    let (handle, _) = doc_get_with_virtual(
                        &VirtualDocGetCtx {
                            store: store_cell,
                            descriptors,
                            schemas,
                            schema_tables,
                            oci_file_memo: &oci_file_memo_cell,
                            store_events: &store_events,
                            clock_cell: &clock_cell,
                        },
                        doc,
                        &key,
                    )?;
                    let observed = store_cell
                        .borrow()
                        .entry(handle)
                        .expect("doc_get allocated option")
                        .content_hash;
                    let store = store_cell.borrow();
                    let projection_context = ProjectionRecordContext {
                        arg_schemas: &exec_arg_schemas,
                        args: &exec_args,
                        store: &store,
                        descriptors,
                        schemas,
                    };
                    record_projection_for_matching_args_static(
                        &mut projection_reads.borrow_mut(),
                        &projection_context,
                        doc,
                        ProjectionPath::DocGet { key },
                        observed,
                    );
                    write_frame_word(frame, dst_slot, handle);
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut doc_package_host = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let doc = read_frame_word(frame, primitive_region + 8);
                    let name = read_frame_word(frame, primitive_region + 16);
                    let name = store_cell.borrow().string_value(name, "String")?;
                    let handle =
                        doc_package(store_cell, descriptors, schemas, schema_tables, doc, &name)?;
                    write_frame_word(frame, dst_slot, handle);
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut doc_coerce_host = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let doc = read_frame_word(frame, primitive_region + 8);
                    let schema = schema_name_for(
                        read_frame_word(frame, primitive_region + 16),
                        schema_tables,
                    )?;
                    record_whole_args_if_projectable_static(
                        &mut projection_reads.borrow_mut(),
                        &exec_arg_schemas,
                        &exec_args,
                        &store_cell.borrow(),
                        descriptors,
                        schemas,
                        [doc],
                    );
                    let word = doc_coerce(
                        store_cell,
                        descriptors,
                        schemas,
                        schema_tables,
                        doc,
                        &schema,
                    )?;
                    write_frame_word(frame, dst_slot, word);
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut elf_doc_host = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let input = read_frame_word(frame, primitive_region + 8);
                    record_whole_args_if_projectable_static(
                        &mut projection_reads.borrow_mut(),
                        &exec_arg_schemas,
                        &exec_args,
                        &store_cell.borrow(),
                        descriptors,
                        schemas,
                        [input],
                    );
                    let handle =
                        alloc_elf_doc(store_cell, descriptors, schemas, schema_tables, input)?;
                    write_frame_word(frame, dst_slot, handle);
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut ast_doc_host = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let input = read_frame_word(frame, primitive_region + 8);
                    let handle =
                        alloc_ast_doc(store_cell, descriptors, schemas, schema_tables, input)?;
                    ast_roots_cell.borrow_mut().insert(handle, input);
                    write_frame_word(frame, dst_slot, handle);
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut oci_doc_host = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let input = read_frame_word(frame, primitive_region + 8);
                    let handle =
                        alloc_oci_doc(store_cell, descriptors, schemas, schema_tables, input)?;
                    write_frame_word(frame, dst_slot, handle);
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut version_parse_host = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let input = read_frame_word(frame, primitive_region + 8);
                    let text = store_cell.borrow().string_value(input, "String")?;
                    let version = super::version::parse(&text)?;
                    let (handle, _) = store_cell.borrow_mut().alloc_raw(
                        "Version",
                        super::version::canonical_bytes(&version),
                        schemas,
                    );
                    write_frame_word(frame, dst_slot, handle);
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut version_field_host = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let handle = read_frame_word(frame, primitive_region + 8);
                    let field = read_frame_word(frame, primitive_region + 16);
                    let store = store_cell.borrow();
                    let entry = store
                        .entry(handle)
                        .ok_or_else(|| format!("store handle {handle}"))?;
                    if entry.schema != "Version" {
                        return Err(format!(
                            "Version field access expected Version, got {}",
                            entry.schema
                        ));
                    }
                    let version = super::version::parse_bytes(&entry.bytes)?;
                    let value = match field {
                        0 => version.major,
                        1 => version.minor,
                        2 => version.patch,
                        other => return Err(format!("unknown Version field {other}")),
                    };
                    drop(store);
                    write_frame_word(frame, dst_slot, value as i64);
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            // Option construction: `Some(x)` / `None` build the same content-
            // addressed Option store entry that `map.get` produces, so the whole
            // language shares one Option representation.
            let mut option_construct_host = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let tag = read_frame_word(frame, primitive_region + 8);
                    let value_ref = read_frame_word(frame, primitive_region + 16);
                    let value_word = read_frame_word(frame, primitive_region + 24);
                    let value_schema = schema_name_for(value_ref, schema_tables)?;
                    let (handle, _) = match tag {
                        0 => store_cell
                            .borrow_mut()
                            .alloc_option_none(&value_schema, schema_tables)?,
                        1 => store_cell.borrow_mut().alloc_option_some(
                            &value_schema,
                            value_word,
                            None,
                            schemas,
                            schema_tables,
                        )?,
                        other => return Err(format!("unknown Option tag {other}")),
                    };
                    write_frame_word(frame, dst_slot, handle);
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            // Option matching: selector 0 reads the tag (0 = None, 1 = Some),
            // selector 1 reads the Some payload word for binding, and selector
            // 2 reads the payload realization flag.
            let mut option_destruct_host = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let handle = read_frame_word(frame, primitive_region + 8);
                    let selector = read_frame_word(frame, primitive_region + 16);
                    let payload =
                        store_cell
                            .borrow()
                            .option_payload(handle, schemas, schema_tables)?;
                    let value = match (selector, payload) {
                        (0, OptionPayload::None) => 0,
                        (0, OptionPayload::Some { .. }) => 1,
                        (1, OptionPayload::Some { word, .. }) => word,
                        (1, OptionPayload::None) => {
                            return Err("Option payload read on None".into());
                        }
                        (
                            2,
                            OptionPayload::Some {
                                realization: Some(realization),
                                ..
                            },
                        ) => realization.to_word(),
                        (
                            2,
                            OptionPayload::Some {
                                realization: None, ..
                            },
                        ) => -1,
                        (2, OptionPayload::None) => {
                            return Err("Option realization read on None".into());
                        }
                        (other, _) => {
                            return Err(format!("unknown Option destruct selector {other}"));
                        }
                    };
                    write_frame_word(frame, dst_slot, value);
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            // General string primitives (the parser building blocks). selector 0
            // = substring before the first delimiter (whole string if absent),
            // 1 = substring after it (empty if absent), 2 = strip the delimiter as
            // a prefix (whole string if it is not a prefix).
            let mut string_split_host = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let str_handle = read_frame_word(frame, primitive_region + 8);
                    let delim_handle = read_frame_word(frame, primitive_region + 16);
                    let selector = read_frame_word(frame, primitive_region + 24);
                    let store = store_cell.borrow();
                    let text = store.string_value(str_handle, "String")?;
                    let delim = store.string_value(delim_handle, "String")?;
                    drop(store);
                    let part: &str = match selector {
                        0 => text
                            .split_once(delim.as_str())
                            .map_or(text.as_str(), |(b, _)| b),
                        1 => text.split_once(delim.as_str()).map_or("", |(_, a)| a),
                        2 => text.strip_prefix(delim.as_str()).unwrap_or(text.as_str()),
                        other => return Err(format!("unknown string split selector {other}")),
                    };
                    let handle = store_cell
                        .borrow_mut()
                        .alloc_raw("String", part.as_bytes().to_vec(), schemas)
                        .0;
                    write_frame_word(frame, dst_slot, handle);
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut string_parse_int_host = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let str_handle = read_frame_word(frame, primitive_region + 8);
                    let text = store_cell.borrow().string_value(str_handle, "String")?;
                    let value: i64 = text
                        .trim()
                        .parse()
                        .map_err(|_| format!("parse_int: {text:?} is not an integer"))?;
                    write_frame_word(frame, dst_slot, value);
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut string_contains_host = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let str_handle = read_frame_word(frame, primitive_region + 8);
                    let needle_handle = read_frame_word(frame, primitive_region + 16);
                    let store = store_cell.borrow();
                    let text = store.string_value(str_handle, "String")?;
                    let needle = store.string_value(needle_handle, "String")?;
                    write_frame_word(frame, dst_slot, i64::from(text.contains(&needle)));
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            // A semver prerelease identifier is numeric iff it is a non-empty run
            // of ASCII digits (and so compares numerically, below alphanumerics).
            let mut string_is_numeric_host = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let str_handle = read_frame_word(frame, primitive_region + 8);
                    let text = store_cell.borrow().string_value(str_handle, "String")?;
                    let numeric = !text.is_empty() && text.bytes().all(|b| b.is_ascii_digit());
                    write_frame_word(frame, dst_slot, i64::from(numeric));
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut value_compare_host = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let schema = schema_name_for(
                        read_frame_word(frame, primitive_region + 8),
                        schema_tables,
                    )?;
                    let op = read_frame_word(frame, primitive_region + 16);
                    let left = read_frame_word(frame, primitive_region + 24);
                    let right = read_frame_word(frame, primitive_region + 32);
                    let store = store_cell.borrow();
                    let ordering = compare_expression_words(
                        &store,
                        descriptors,
                        schemas,
                        schema_tables,
                        &schema,
                        left,
                        right,
                    )?;
                    let value = match op {
                        0 => ordering == Ordering::Equal,
                        1 => ordering != Ordering::Equal,
                        2 => ordering == Ordering::Less,
                        3 => ordering != Ordering::Greater,
                        4 => ordering == Ordering::Greater,
                        5 => ordering != Ordering::Less,
                        other => return Err(format!("unknown compare op {other}")),
                    };
                    write_frame_word(frame, dst_slot, i64::from(value));
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut version_set_parse_host = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let input = read_frame_word(frame, primitive_region + 8);
                    let text = store_cell.borrow().string_value(input, "String")?;
                    let set = super::version_set::VersionSet::from_req(&text)?;
                    let (handle, _) = store_cell.borrow_mut().alloc_raw(
                        "VersionSet",
                        set.canonical_bytes(),
                        schemas,
                    );
                    write_frame_word(frame, dst_slot, handle);
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut version_set_op_host = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let op = read_frame_word(frame, primitive_region + 8);
                    let left = read_frame_word(frame, primitive_region + 16);
                    let right = read_frame_word(frame, primitive_region + 24);
                    let store = store_cell.borrow();
                    let left_entry = store
                        .entry(left)
                        .ok_or_else(|| format!("store handle {left}"))?;
                    if left_entry.schema != "VersionSet" {
                        return Err(format!(
                            "VersionSet op expected VersionSet, got {}",
                            left_entry.schema
                        ));
                    }
                    let left_set = super::version_set::VersionSet::parse_bytes(&left_entry.bytes)?;
                    match op {
                        0 | 1 | 3 => {
                            let right_entry = store
                                .entry(right)
                                .ok_or_else(|| format!("store handle {right}"))?;
                            if right_entry.schema != "VersionSet" {
                                return Err(format!(
                                    "VersionSet op expected VersionSet, got {}",
                                    right_entry.schema
                                ));
                            }
                            let right_set =
                                super::version_set::VersionSet::parse_bytes(&right_entry.bytes)?;
                            drop(store);
                            match op {
                                0 => {
                                    let set = left_set.union(&right_set);
                                    let handle = store_cell
                                        .borrow_mut()
                                        .alloc_raw("VersionSet", set.canonical_bytes(), schemas)
                                        .0;
                                    write_frame_word(frame, dst_slot, handle);
                                }
                                1 => {
                                    let set = left_set.intersect(&right_set);
                                    let handle = store_cell
                                        .borrow_mut()
                                        .alloc_raw("VersionSet", set.canonical_bytes(), schemas)
                                        .0;
                                    write_frame_word(frame, dst_slot, handle);
                                }
                                3 => {
                                    write_frame_word(
                                        frame,
                                        dst_slot,
                                        i64::from(left_set.is_subset_of(&right_set)),
                                    );
                                }
                                _ => unreachable!(),
                            }
                        }
                        2 => {
                            drop(store);
                            let set = left_set.complement();
                            let handle = store_cell
                                .borrow_mut()
                                .alloc_raw("VersionSet", set.canonical_bytes(), schemas)
                                .0;
                            write_frame_word(frame, dst_slot, handle);
                        }
                        4 => {
                            let right_entry = store
                                .entry(right)
                                .ok_or_else(|| format!("store handle {right}"))?;
                            if right_entry.schema != "Version" {
                                return Err(format!(
                                    "VersionSet.contains expected Version, got {}",
                                    right_entry.schema
                                ));
                            }
                            let version = super::version::parse_bytes(&right_entry.bytes)?;
                            write_frame_word(
                                frame,
                                dst_slot,
                                i64::from(left_set.contains(&version)),
                            );
                        }
                        other => return Err(format!("unknown VersionSet op {other}")),
                    }
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut ast_fn_host = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let root = read_frame_word(frame, primitive_region + 8);
                    let name = read_frame_word(frame, primitive_region + 16);
                    let input = *ast_roots_cell
                        .borrow()
                        .get(&root)
                        .ok_or_else(|| format!("handle {root} is not an ast() root"))?;
                    let input_hash = store_cell
                        .borrow()
                        .entry(input)
                        .ok_or_else(|| format!("store handle {input}"))?
                        .content_hash;
                    let pending = ast_projection_pending(
                        input,
                        input_hash,
                        super::ast_probe::Projection::Fn,
                        vec![name],
                    );
                    let pending = store_cell.borrow_mut().alloc_pending("Doc", pending).0;
                    write_frame_word(frame, dst_slot, pending);
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut array_len_host = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let array = read_frame_word(frame, primitive_region + 8);
                    let len = {
                        let store = store_cell.borrow();
                        if matches!(Handle::from_word(array), Handle::Store(_)) {
                            record_whole_args_if_projectable_static(
                                &mut projection_reads.borrow_mut(),
                                &exec_arg_schemas,
                                &exec_args,
                                &store,
                                descriptors,
                                schemas,
                                [array],
                            );
                        }
                        match molten_cell
                            .borrow()
                            .array_entry(&store, array, schema_tables)?
                        {
                            ArrayEntry::Words { words, .. } => words.len(),
                            ArrayEntry::Pending { pending, .. } => pending.len(),
                        }
                    };
                    write_frame_word(
                        frame,
                        dst_slot,
                        i64::try_from(len).expect("array length fits i64"),
                    );
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut string_concat_host = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let left = read_frame_word(frame, primitive_region + 8);
                    let right = read_frame_word(frame, primitive_region + 16);
                    let store = store_cell.borrow();
                    let taint = combine_taints([left, right].into_iter().filter_map(|word| {
                        store.entry(word).and_then(|entry| entry.taint.clone())
                    }));
                    let left = store.string_value(left, "String")?;
                    let right = store.string_value(right, "String")?;
                    drop(store);
                    let handle = store_cell
                        .borrow_mut()
                        .alloc_raw_tainted(
                            "String",
                            [left.as_str(), right.as_str()].concat().into_bytes(),
                            schemas,
                            taint,
                        )
                        .0;
                    write_frame_word(frame, dst_slot, handle);
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut array_join_host = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let array = read_frame_word(frame, primitive_region + 8);
                    let separator = read_frame_word(frame, primitive_region + 16);
                    let store = store_cell.borrow();
                    let taint = combine_taints([array, separator].into_iter().filter_map(|word| {
                        store.entry(word).and_then(|entry| entry.taint.clone())
                    }));
                    let separator = store.string_value(separator, "String")?;
                    if matches!(Handle::from_word(array), Handle::Store(_)) {
                        record_whole_args_if_projectable_static(
                            &mut projection_reads.borrow_mut(),
                            &exec_arg_schemas,
                            &exec_args,
                            &store,
                            descriptors,
                            schemas,
                            [array],
                        );
                    }
                    let array_entry =
                        molten_cell
                            .borrow()
                            .array_entry(&store, array, schema_tables)?;
                    let joined = match array_entry {
                        ArrayEntry::Words { elem_schema, words } => {
                            if elem_schema != "String" && elem_schema != "Doc" {
                                return Err(format!("join called on Array<{elem_schema}>"));
                            }
                            words
                                .into_iter()
                                .map(|word| {
                                    if schemas.is_primitive(&elem_schema, Primitive::String) {
                                        store.string_value(word, "String")
                                    } else {
                                        let DocPayload::String(handle) =
                                            doc_payload(&store, descriptors, word)?
                                        else {
                                            return Err(
                                                "join called on non-string Doc array element"
                                                    .to_string(),
                                            );
                                        };
                                        store.string_value(handle, "String")
                                    }
                                })
                                .collect::<Result<Vec<_>, _>>()?
                                .join(&separator)
                        }
                        ArrayEntry::Pending { .. } => {
                            return Err("join called on pending array".to_string());
                        }
                    };
                    drop(store);
                    let handle = store_cell
                        .borrow_mut()
                        .alloc_raw_tainted("String", joined.into_bytes(), schemas, taint)
                        .0;
                    write_frame_word(frame, dst_slot, handle);
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut array_filter_exclude = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let array_handle = read_frame_word(frame, primitive_region + 8);
                    let count = usize::try_from(read_frame_word(frame, primitive_region + 16))
                        .map_err(|_| "negative filter exclusion count")?;
                    let excluded = (0..count)
                        .map(|i| {
                            let handle = read_frame_word(frame, primitive_region + 24 + i * 8);
                            store_cell.borrow().string_value(handle, "Path")
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    let (elem_schema, words) = match molten_cell.borrow().array_entry(
                        &store_cell.borrow(),
                        array_handle,
                        schema_tables,
                    )? {
                        ArrayEntry::Words { elem_schema, words } => (elem_schema, words),
                        ArrayEntry::Pending { .. } => {
                            return Err("filter over pending array is outside B4".into());
                        }
                    };
                    match Handle::from_word(array_handle) {
                        Handle::Molten(_) => {}
                        Handle::Store(_) => {
                            record_whole_args_if_projectable_static(
                                &mut projection_reads.borrow_mut(),
                                &exec_arg_schemas,
                                &exec_args,
                                &store_cell.borrow(),
                                descriptors,
                                schemas,
                                [array_handle],
                            );
                        }
                    }
                    let kept = words
                        .into_iter()
                        .filter_map(|word| {
                            let path = store_cell.borrow().string_value(word, "Path").ok()?;
                            (!excluded.iter().any(|excluded| excluded == &path)).then_some(word)
                        })
                        .collect();
                    let (handle, _) = store_cell.borrow_mut().alloc_array_words(
                        &elem_schema,
                        kept,
                        schemas,
                        schema_tables,
                    )?;
                    write_frame_word(frame, dst_slot, handle);
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut glob_host = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let tree_handle = read_frame_word(frame, primitive_region + 8);
                    let pattern_handle = read_frame_word(frame, primitive_region + 16);
                    let pattern = store_cell.borrow().string_value(pattern_handle, "String")?;
                    let tree = match store_cell.borrow().tree_entry(tree_handle)? {
                        TreeEntry::Concrete(tree) => tree,
                        TreeEntry::Merge(_) | TreeEntry::Exec(_) => {
                            return Err("glob on pending tree is outside B5".into());
                        }
                    };
                    record_whole_args_if_projectable_static(
                        &mut projection_reads.borrow_mut(),
                        &exec_arg_schemas,
                        &exec_args,
                        &store_cell.borrow(),
                        descriptors,
                        schemas,
                        [tree_handle],
                    );
                    let mut paths: Vec<String> = tree
                        .entries
                        .keys()
                        .filter(|path| simple_glob_match(&pattern, path))
                        .cloned()
                        .collect();
                    paths.sort();
                    let words = paths
                        .into_iter()
                        .map(|path| {
                            store_cell
                                .borrow_mut()
                                .alloc_raw("Path", path.into_bytes(), schemas)
                                .0
                        })
                        .collect();
                    let (handle, _) = store_cell.borrow_mut().alloc_array_words(
                        "Path",
                        words,
                        schemas,
                        schema_tables,
                    )?;
                    write_frame_word(frame, dst_slot, handle);
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut path_join = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let root = read_frame_word(frame, primitive_region + 8);
                    let segment = read_frame_word(frame, primitive_region + 16);
                    let store = store_cell.borrow();
                    let taint = combine_taints([root, segment].into_iter().filter_map(|word| {
                        store.entry(word).and_then(|entry| entry.taint.clone())
                    }));
                    let root = store.string_value(root, "Path")?;
                    let segment = store.string_value(segment, "String")?;
                    drop(store);
                    let root = root.strip_suffix('/').unwrap_or(&root);
                    let value = match root.is_empty() {
                        true => segment,
                        false => format!("{root}/{segment}"),
                    };
                    let (handle, _) = store_cell.borrow_mut().alloc_raw_tainted(
                        "Path",
                        value.into_bytes(),
                        schemas,
                        taint,
                    );
                    write_frame_word(frame, dst_slot, handle);
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut path_to_string = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let path = read_frame_word(frame, primitive_region + 8);
                    let store = store_cell.borrow();
                    let taint = store.entry(path).and_then(|entry| entry.taint.clone());
                    let value = store.string_value(path, "Path")?;
                    drop(store);
                    let (handle, _) = store_cell.borrow_mut().alloc_raw_tainted(
                        "String",
                        value.into_bytes(),
                        schemas,
                        taint,
                    );
                    write_frame_word(frame, dst_slot, handle);
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut doc_is_map = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let doc = read_frame_word(frame, primitive_region + 8);
                    let is_map = matches!(
                        doc_payload(&store_cell.borrow(), descriptors, doc)?,
                        DocPayload::Map(_)
                    );
                    write_frame_word(frame, dst_slot, i64::from(is_map));
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut doc_keys = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let input = read_frame_word(frame, primitive_region + 8);
                    let (_, pairs) = {
                        let store = store_cell.borrow();
                        let entry = store
                            .entry(input)
                            .ok_or_else(|| format!("store handle {input}"))?;
                        if schemas.is_map(&entry.schema) {
                            store.map_pairs(input, schemas, schema_tables)?
                        } else {
                            let map_handle = match doc_payload(&store, descriptors, input)? {
                                DocPayload::Map(handle) => handle,
                                DocPayload::Virtual(_) => {
                                    return Err("Doc.keys on virtual Doc is not enumerable".into());
                                }
                                payload => {
                                    return Err(format!("keys called on Doc::{payload:?}"));
                                }
                            };
                            store.map_pairs(map_handle, schemas, schema_tables)?
                        }
                    };
                    let mut keys = pairs
                        .into_iter()
                        .map(|pair| {
                            store_cell
                                .borrow()
                                .string_value(pair.key_word, &pair.key_schema)
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    keys.sort();
                    let words = keys
                        .into_iter()
                        .map(|key| {
                            store_cell
                                .borrow_mut()
                                .alloc_raw("String", key.into_bytes(), schemas)
                                .0
                        })
                        .collect();
                    let (handle, _) = store_cell.borrow_mut().alloc_array_words(
                        "String",
                        words,
                        schemas,
                        schema_tables,
                    )?;
                    write_frame_word(frame, dst_slot, handle);
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut path_with_ext = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let path = read_frame_word(frame, primitive_region + 8);
                    let ext = read_frame_word(frame, primitive_region + 16);
                    let store = store_cell.borrow();
                    let taint = combine_taints([path, ext].into_iter().filter_map(|word| {
                        store.entry(word).and_then(|entry| entry.taint.clone())
                    }));
                    let path = store.string_value(path, "Path")?;
                    let ext = store.string_value(ext, "String")?;
                    drop(store);
                    let stem = path.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(&path);
                    let value = format!("{stem}.{ext}");
                    let (handle, _) = store_cell.borrow_mut().alloc_raw_tainted(
                        "Path",
                        value.into_bytes(),
                        schemas,
                        taint,
                    );
                    write_frame_word(frame, dst_slot, handle);
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut pending_alloc = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let value_schema = schema_name_for(
                        read_frame_word(frame, primitive_region + 8),
                        schema_tables,
                    )?;
                    let fn_ref = FnRef::new(
                        usize::try_from(read_frame_word(frame, primitive_region + 16))
                            .map_err(|_| "negative fn ref".to_string())?,
                    );
                    let argc = usize::try_from(read_frame_word(frame, primitive_region + 24))
                        .map_err(|_| "negative argc".to_string())?;
                    let mut args = (0..argc)
                        .map(|i| read_frame_word(frame, primitive_region + 32 + i * 8))
                        .collect::<Vec<_>>();
                    for (arg, schema) in args
                        .iter_mut()
                        .zip(&lowered_fns[fn_ref.index()].arg_schemas)
                    {
                        *arg = intern_molten_word(
                            &mut store_cell.borrow_mut(),
                            &mut molten_cell.borrow_mut(),
                            descriptors,
                            schemas,
                            schema_tables,
                            schema,
                            *arg,
                        )?
                        .0;
                    }
                    record_whole_args_if_projectable_static(
                        &mut projection_reads.borrow_mut(),
                        &exec_arg_schemas,
                        &exec_args,
                        &store_cell.borrow(),
                        descriptors,
                        schemas,
                        args.iter().copied(),
                    );
                    let invocation = pending_invocation_for(
                        &lowered_fns[fn_ref.index()],
                        store_cell,
                        schemas,
                        args,
                    );
                    let (handle, _) = store_cell
                        .borrow_mut()
                        .alloc_pending(&value_schema, invocation);
                    write_frame_word(frame, dst_slot, handle);
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut pending_coerce = |frame: &mut [u8]| {
                let input_slot = read_frame_word(frame, primitive_region) as usize;
                let pending = read_frame_word(frame, primitive_region + 8);
                pending_coercions.push(PendingCoerceRequest {
                    input_slot,
                    pending,
                });
            };

            let mut pending_invoke = |frame: &mut [u8]| {
                let result = (|| {
                    let input_slot = read_frame_word(frame, primitive_region) as usize;
                    let pending = read_frame_word(frame, primitive_region + 8);
                    let argc = usize::try_from(read_frame_word(frame, primitive_region + 16))
                        .map_err(|_| "negative pending invoke argc".to_string())?;
                    let mut args = (0..argc)
                        .map(|i| read_frame_word(frame, primitive_region + 24 + i * 8))
                        .collect::<Vec<_>>();
                    let invocation = store_cell.borrow().pending_invocation(pending)?;
                    let fn_ref = lowered_fns
                        .iter()
                        .position(|lowered| lowered.hash == invocation.closure_hash)
                        .map(FnRef::new)
                        .ok_or_else(|| {
                            format!(
                                "no function with closure hash {:016x}",
                                invocation.closure_hash
                            )
                        })?;
                    let start = invocation.args.len();
                    for (arg, schema) in args
                        .iter_mut()
                        .zip(lowered_fns[fn_ref.index()].arg_schemas.iter().skip(start))
                    {
                        *arg = intern_molten_word(
                            &mut store_cell.borrow_mut(),
                            &mut molten_cell.borrow_mut(),
                            descriptors,
                            schemas,
                            schema_tables,
                            schema,
                            *arg,
                        )?
                        .0;
                    }
                    pending_invokes.push(PendingInvokeRequest {
                        caller: exec_ix,
                        input_slot,
                        pending,
                        args,
                    });
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut target_host = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let (handle, _) = intern_structured_target(
                        store_cell,
                        descriptors,
                        schemas,
                        host_os_index(),
                        host_arch_index(),
                    )?;
                    write_frame_word(frame, dst_slot, handle);
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut string_upper = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let input = read_frame_word(frame, primitive_region + 8);
                    let store = store_cell.borrow();
                    let taint = store.entry(input).and_then(|entry| entry.taint.clone());
                    let value = store.string_value(input, "String")?.to_uppercase();
                    drop(store);
                    let (handle, deduped) = store_cell.borrow_mut().alloc_raw_tainted(
                        "String",
                        value.into_bytes(),
                        schemas,
                        taint,
                    );
                    write_frame_word(frame, dst_slot, handle);
                    store_events.borrow_mut().push(DriveEvent::StoreAlloc {
                        schema_ref: hash_u64("String"),
                        deduped,
                    });
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut string_lower = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let input = read_frame_word(frame, primitive_region + 8);
                    let store = store_cell.borrow();
                    let taint = store.entry(input).and_then(|entry| entry.taint.clone());
                    let value = store.string_value(input, "String")?.to_lowercase();
                    drop(store);
                    let (handle, deduped) = store_cell.borrow_mut().alloc_raw_tainted(
                        "String",
                        value.into_bytes(),
                        schemas,
                        taint,
                    );
                    write_frame_word(frame, dst_slot, handle);
                    store_events.borrow_mut().push(DriveEvent::StoreAlloc {
                        schema_ref: hash_u64("String"),
                        deduped,
                    });
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut string_default = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let input = read_frame_word(frame, primitive_region + 8);
                    let fallback = read_frame_word(frame, primitive_region + 16);
                    let value = store_cell.borrow().string_value(input, "String")?;
                    let handle = if value.is_empty() { fallback } else { input };
                    write_frame_word(frame, dst_slot, handle);
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut sealed_seal = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let ciphertext = read_frame_word(frame, primitive_region + 8);
                    let marker = read_frame_word(frame, primitive_region + 16);
                    let recipient = read_frame_word(frame, primitive_region + 24);
                    let tag = read_frame_word(frame, primitive_region + 32);
                    let store = store_cell.borrow();
                    let ciphertext = store.string_value(ciphertext, "String")?.into_bytes();
                    let marker = store.string_value(marker, "String")?;
                    let recipient = store.string_value(recipient, "String")?;
                    let tag = if tag >= 0 {
                        Some(store.string_value(tag, "String")?)
                    } else {
                        None
                    };
                    drop(store);
                    let payload = sealed_payload(ciphertext, marker, recipient, tag);
                    let taint = payload.taint.clone();
                    let (handle, deduped) = store_cell.borrow_mut().alloc_raw_tainted(
                        "Sealed",
                        encode_sealed_payload(&payload),
                        schemas,
                        Some(taint),
                    );
                    write_frame_word(frame, dst_slot, handle);
                    store_events.borrow_mut().push(DriveEvent::StoreAlloc {
                        schema_ref: hash_u64("Sealed"),
                        deduped,
                    });
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut sealed_declassify = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let sealed = read_frame_word(frame, primitive_region + 8);
                    let payload = {
                        let store = store_cell.borrow();
                        let entry = store
                            .entry(sealed)
                            .ok_or_else(|| format!("store handle {sealed}"))?;
                        if entry.schema != "Sealed" {
                            return Err(format!(
                                "declassify expected Sealed, got {}",
                                entry.schema
                            ));
                        }
                        decode_sealed_payload(&entry.bytes)?
                    };
                    if payload.taint.recipient != "test" {
                        return Err(format!(
                            "declassify has no backend for recipient `{}`",
                            payload.taint.recipient
                        ));
                    }
                    let (handle, deduped) =
                        store_cell
                            .borrow_mut()
                            .alloc_raw("String", payload.ciphertext, schemas);
                    write_frame_word(frame, dst_slot, handle);
                    store_events.borrow_mut().push(DriveEvent::StoreAlloc {
                        schema_ref: hash_u64("String"),
                        deduped,
                    });
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut sealed_to_string = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let sealed = read_frame_word(frame, primitive_region + 8);
                    let (identity, taint) = {
                        let store = store_cell.borrow();
                        let entry = store
                            .entry(sealed)
                            .ok_or_else(|| format!("store handle {sealed}"))?;
                        if entry.schema != "Sealed" {
                            return Err(format!(
                                "sealed-to-string expected Sealed, got {}",
                                entry.schema
                            ));
                        }
                        let payload = decode_sealed_payload(&entry.bytes)?;
                        (payload.taint.identity_hash, entry.taint.clone())
                    };
                    let bytes = format!("sealed:{}", hex_bytes(&identity)).into_bytes();
                    let (handle, deduped) = store_cell
                        .borrow_mut()
                        .alloc_raw_tainted("String", bytes, schemas, taint);
                    write_frame_word(frame, dst_slot, handle);
                    store_events.borrow_mut().push(DriveEvent::StoreAlloc {
                        schema_ref: hash_u64("String"),
                        deduped,
                    });
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut molten_intern = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let src = read_frame_word(frame, primitive_region + 8);
                    let schema = schema_name_for(
                        read_frame_word(frame, primitive_region + 16),
                        schema_tables,
                    )?;
                    let was_molten = !schema_is_inline_word(schemas, &schema)
                        && matches!(Handle::from_word(src), Handle::Molten(_));
                    let (handle, deduped) = intern_molten_word(
                        &mut store_cell.borrow_mut(),
                        &mut molten_cell.borrow_mut(),
                        descriptors,
                        schemas,
                        schema_tables,
                        &schema,
                        src,
                    )?;
                    write_frame_word(frame, dst_slot, handle);
                    if was_molten {
                        store_events.borrow_mut().push(DriveEvent::StoreAlloc {
                            schema_ref: hash_u64(&schema),
                            deduped,
                        });
                    }
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut array_push = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let array = read_frame_word(frame, primitive_region + 8);
                    let mut value = read_frame_word(frame, primitive_region + 16);
                    let pushed_schema = schema_name_for(
                        read_frame_word(frame, primitive_region + 24),
                        schema_tables,
                    )?;
                    let consuming_receiver = read_frame_word(frame, primitive_region + 32) != 0;
                    if !force_molten_copy
                        && let Handle::Molten(array_handle) = Handle::from_word(array)
                    {
                        let elem_schema = {
                            let molten = molten_cell.borrow();
                            molten
                                .entry(array_handle)
                                .and_then(|entry| match &entry.value {
                                    MoltenValue::ArrayWords { elem_schema, words }
                                        if entry.refs == 1 || consuming_receiver =>
                                    {
                                        Some(if words.is_empty() {
                                            pushed_schema.clone()
                                        } else {
                                            elem_schema.clone()
                                        })
                                    }
                                    _ => None,
                                })
                        };
                        if let Some(elem_schema) = elem_schema {
                            value = intern_molten_word(
                                &mut store_cell.borrow_mut(),
                                &mut molten_cell.borrow_mut(),
                                descriptors,
                                schemas,
                                schema_tables,
                                &elem_schema,
                                value,
                            )?
                            .0;
                            let mut molten = molten_cell.borrow_mut();
                            let entry = molten
                                .entry_mut(array_handle)
                                .ok_or_else(|| format!("molten handle {array}"))?;
                            if let MoltenValue::ArrayWords {
                                elem_schema: stored_schema,
                                words,
                            } = &mut entry.value
                            {
                                if words.is_empty() {
                                    *stored_schema = elem_schema.clone();
                                    entry.carried_array_hash = Some(start_array_element_hasher(
                                        b"vix-array-words",
                                        schema_tables,
                                        &elem_schema,
                                    ));
                                }
                                if entry.carried_array_hash.is_none() {
                                    entry.carried_array_hash =
                                        Some(recompute_array_element_hasher(
                                            &store_cell.borrow(),
                                            schemas,
                                            schema_tables,
                                            b"vix-array-words",
                                            stored_schema,
                                            words,
                                        ));
                                }
                                let element_hash = canonical_word_hash_in_store(
                                    &store_cell.borrow(),
                                    schemas,
                                    stored_schema,
                                    value,
                                );
                                if let Some(carried_hash) = &mut entry.carried_array_hash {
                                    update_array_element_hash(carried_hash, element_hash);
                                }
                                words.push(value);
                                molten_stats.borrow_mut().array_push_reused += 1;
                                write_frame_word(frame, dst_slot, array);
                                return Ok(());
                            }
                        }
                    }
                    let (mut elem_schema, mut words) = {
                        let store = store_cell.borrow();
                        if matches!(Handle::from_word(array), Handle::Store(_)) {
                            record_whole_args_if_projectable_static(
                                &mut projection_reads.borrow_mut(),
                                &exec_arg_schemas,
                                &exec_args,
                                &store,
                                descriptors,
                                schemas,
                                [array],
                            );
                        }
                        match molten_cell
                            .borrow()
                            .array_entry(&store, array, schema_tables)?
                        {
                            ArrayEntry::Words { elem_schema, words } => (elem_schema, words),
                            ArrayEntry::Pending { .. } => {
                                return Err("push on pending array".into());
                            }
                        }
                    };
                    if words.is_empty() {
                        elem_schema = pushed_schema;
                    }
                    value = intern_molten_word(
                        &mut store_cell.borrow_mut(),
                        &mut molten_cell.borrow_mut(),
                        descriptors,
                        schemas,
                        schema_tables,
                        &elem_schema,
                        value,
                    )?
                    .0;
                    words.push(value);
                    let handle = molten_cell
                        .borrow_mut()
                        .alloc(MoltenValue::ArrayWords { elem_schema, words });
                    molten_stats.borrow_mut().array_push_copied += 1;
                    write_frame_word(frame, dst_slot, handle);
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut array_pop = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let value_slot = read_frame_word(frame, primitive_region + 8) as usize;
                    let array = read_frame_word(frame, primitive_region + 16);
                    if !force_molten_copy
                        && let Handle::Molten(array_handle) = Handle::from_word(array)
                    {
                        let can_reuse = {
                            let molten = molten_cell.borrow();
                            molten.entry(array_handle).is_some_and(|entry| {
                                entry.refs == 1
                                    && matches!(entry.value, MoltenValue::ArrayWords { .. })
                            })
                        };
                        if can_reuse {
                            let mut molten = molten_cell.borrow_mut();
                            let entry = molten
                                .entry_mut(array_handle)
                                .ok_or_else(|| format!("molten handle {array}"))?;
                            if let MoltenValue::ArrayWords { words, .. } = &mut entry.value {
                                let value = words
                                    .pop()
                                    .ok_or_else(|| "pop on empty array".to_string())?;
                                entry.carried_array_hash = None;
                                write_frame_word(frame, value_slot, value);
                                write_frame_word(frame, dst_slot, array);
                                return Ok(());
                            }
                        }
                    }
                    let (elem_schema, mut words) = {
                        let store = store_cell.borrow();
                        if matches!(Handle::from_word(array), Handle::Store(_)) {
                            record_whole_args_if_projectable_static(
                                &mut projection_reads.borrow_mut(),
                                &exec_arg_schemas,
                                &exec_args,
                                &store,
                                descriptors,
                                schemas,
                                [array],
                            );
                        }
                        match molten_cell
                            .borrow()
                            .array_entry(&store, array, schema_tables)?
                        {
                            ArrayEntry::Words { elem_schema, words } => (elem_schema, words),
                            ArrayEntry::Pending { .. } => {
                                return Err("pop on pending array".into());
                            }
                        }
                    };
                    let value = words
                        .pop()
                        .ok_or_else(|| "pop on empty array".to_string())?;
                    write_frame_word(frame, value_slot, value);
                    let handle = molten_cell
                        .borrow_mut()
                        .alloc(MoltenValue::ArrayWords { elem_schema, words });
                    write_frame_word(frame, dst_slot, handle);
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut array_set = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let array = read_frame_word(frame, primitive_region + 8);
                    let index = usize::try_from(read_frame_word(frame, primitive_region + 16))
                        .map_err(|_| "negative array index".to_string())?;
                    let mut value = read_frame_word(frame, primitive_region + 24);
                    if !force_molten_copy
                        && let Handle::Molten(array_handle) = Handle::from_word(array)
                    {
                        let elem_schema = {
                            let molten = molten_cell.borrow();
                            molten
                                .entry(array_handle)
                                .and_then(|entry| match &entry.value {
                                    MoltenValue::ArrayWords { elem_schema, .. }
                                        if entry.refs == 1 =>
                                    {
                                        Some(elem_schema.clone())
                                    }
                                    _ => None,
                                })
                        };
                        if let Some(elem_schema) = elem_schema {
                            value = intern_molten_word(
                                &mut store_cell.borrow_mut(),
                                &mut molten_cell.borrow_mut(),
                                descriptors,
                                schemas,
                                schema_tables,
                                &elem_schema,
                                value,
                            )?
                            .0;
                            let mut molten = molten_cell.borrow_mut();
                            let entry = molten
                                .entry_mut(array_handle)
                                .ok_or_else(|| format!("molten handle {array}"))?;
                            if let MoltenValue::ArrayWords { words, .. } = &mut entry.value {
                                if index >= words.len() {
                                    return Err(format!(
                                        "array index {index} out of bounds {}",
                                        words.len()
                                    ));
                                }
                                words[index] = value;
                                entry.carried_array_hash = None;
                                write_frame_word(frame, dst_slot, array);
                                return Ok(());
                            }
                        }
                    }
                    let (elem_schema, mut words) = {
                        let store = store_cell.borrow();
                        if matches!(Handle::from_word(array), Handle::Store(_)) {
                            record_whole_args_if_projectable_static(
                                &mut projection_reads.borrow_mut(),
                                &exec_arg_schemas,
                                &exec_args,
                                &store,
                                descriptors,
                                schemas,
                                [array],
                            );
                        }
                        match molten_cell
                            .borrow()
                            .array_entry(&store, array, schema_tables)?
                        {
                            ArrayEntry::Words { elem_schema, words } => (elem_schema, words),
                            ArrayEntry::Pending { .. } => {
                                return Err("set on pending array".into());
                            }
                        }
                    };
                    value = intern_molten_word(
                        &mut store_cell.borrow_mut(),
                        &mut molten_cell.borrow_mut(),
                        descriptors,
                        schemas,
                        schema_tables,
                        &elem_schema,
                        value,
                    )?
                    .0;
                    if index >= words.len() {
                        return Err(format!("array index {index} out of bounds {}", words.len()));
                    }
                    words[index] = value;
                    let handle = molten_cell
                        .borrow_mut()
                        .alloc(MoltenValue::ArrayWords { elem_schema, words });
                    write_frame_word(frame, dst_slot, handle);
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut array_get = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let array = read_frame_word(frame, primitive_region + 8);
                    let index = usize::try_from(read_frame_word(frame, primitive_region + 16))
                        .map_err(|_| "negative array index".to_string())?;
                    let words = {
                        let store = store_cell.borrow();
                        if matches!(Handle::from_word(array), Handle::Store(_)) {
                            record_whole_args_if_projectable_static(
                                &mut projection_reads.borrow_mut(),
                                &exec_arg_schemas,
                                &exec_args,
                                &store,
                                descriptors,
                                schemas,
                                [array],
                            );
                        }
                        match molten_cell
                            .borrow()
                            .array_entry(&store, array, schema_tables)?
                        {
                            ArrayEntry::Words { words, .. }
                            | ArrayEntry::Pending { pending: words, .. } => words,
                        }
                    };
                    let value = words.get(index).copied().ok_or_else(|| {
                        format!("array index {index} out of bounds {}", words.len())
                    })?;
                    write_frame_word(frame, dst_slot, value);
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut molten_dup = |frame: &mut [u8]| {
                let dst_slot = read_frame_word(frame, primitive_region) as usize;
                let src = read_frame_word(frame, primitive_region + 8);
                match Handle::from_word(src) {
                    Handle::Molten(handle) => {
                        if let Some(entry) = molten_cell.borrow_mut().entry_mut(handle) {
                            entry.refs = entry.refs.saturating_add(1);
                        }
                    }
                    Handle::Store(_) => {}
                }
                write_frame_word(frame, dst_slot, src);
            };

            let mut record_update = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, store_alloc_region) as usize;
                    let base = read_frame_word(frame, store_alloc_region + 8);
                    let schema = schema_name_for(
                        read_frame_word(frame, store_alloc_region + 16),
                        schema_tables,
                    )?;
                    let variant_index =
                        usize::try_from(read_frame_word(frame, store_alloc_region + 24))
                            .map_err(|_| "negative record variant index".to_string())?;
                    let update_count =
                        usize::try_from(read_frame_word(frame, store_alloc_region + 32))
                            .map_err(|_| "negative record update count".to_string())?;
                    let descriptor = descriptors
                        .get(&schema)
                        .ok_or_else(|| format!("descriptor for schema `{schema}`"))?;
                    if !force_molten_copy
                        && let Handle::Molten(base_handle) = Handle::from_word(base)
                    {
                        let can_reuse = {
                            let molten = molten_cell.borrow();
                            molten.entry(base_handle).is_some_and(|entry| {
                                entry.refs == 1
                                    && matches!(
                                        &entry.value,
                                        MoltenValue::Record {
                                            schema: record_schema,
                                            ..
                                        } if record_schema == &schema
                                    )
                            })
                        };
                        if can_reuse {
                            let mut molten = molten_cell.borrow_mut();
                            let entry = molten
                                .entry_mut(base_handle)
                                .ok_or_else(|| format!("molten handle {base}"))?;
                            if let MoltenValue::Record { bytes, .. } = &mut entry.value {
                                if matches!(descriptor.access, Access::Enum(_)) {
                                    zero_inactive_enum_payload(bytes, descriptor, variant_index);
                                    if variant_index
                                        != usize::try_from(read_variant_tag(bytes, descriptor))
                                            .unwrap_or(usize::MAX)
                                    {
                                        write_variant_tag(bytes, descriptor, variant_index as u64);
                                    }
                                }
                                for update_index in 0..update_count {
                                    let field_index = usize::try_from(read_frame_word(
                                        frame,
                                        store_alloc_region + 40 + update_index * 16,
                                    ))
                                    .map_err(|_| "negative record field index".to_string())?;
                                    let field_offset = field_offset(descriptor, bytes, field_index);
                                    let field = field_descriptor(descriptor, bytes, field_index);
                                    if field.layout.size > 8 {
                                        return Err(format!(
                                            "record update field {field_index} has {} bytes",
                                            field.layout.size
                                        ));
                                    }
                                    let value = canonicalize_word_for_schema(
                                        schemas,
                                        &schemas.display_ref(&field.schema),
                                        read_frame_word(
                                            frame,
                                            store_alloc_region + 48 + update_index * 16,
                                        ),
                                    );
                                    write_canonical_word_field(
                                        bytes,
                                        field_offset,
                                        field.layout.size,
                                        value,
                                    );
                                }
                                write_frame_word(frame, dst_slot, base);
                                return Ok(());
                            }
                        }
                    }
                    let mut bytes = match Handle::from_word(base) {
                        Handle::Molten(base_handle) => {
                            if let Some(entry) = molten_cell.borrow().entry(base_handle) {
                                match &entry.value {
                                    MoltenValue::Record {
                                        schema: record_schema,
                                        bytes,
                                    } if record_schema == &schema => bytes.clone(),
                                    MoltenValue::Interned(handle) => store_cell
                                        .borrow()
                                        .entry(*handle)
                                        .ok_or_else(|| format!("store handle {handle}"))?
                                        .bytes
                                        .clone(),
                                    MoltenValue::Record {
                                        schema: record_schema,
                                        ..
                                    } => {
                                        return Err(format!(
                                            "molten record is `{record_schema}`, not {schema}"
                                        ));
                                    }
                                    _ => {
                                        return Err(format!(
                                            "molten handle {base} is not a Record"
                                        ));
                                    }
                                }
                            } else {
                                record_whole_args_if_projectable_static(
                                    &mut projection_reads.borrow_mut(),
                                    &exec_arg_schemas,
                                    &exec_args,
                                    &store_cell.borrow(),
                                    descriptors,
                                    schemas,
                                    [base],
                                );
                                store_cell
                                    .borrow()
                                    .entry(base)
                                    .ok_or_else(|| format!("store handle {base}"))?
                                    .bytes
                                    .clone()
                            }
                        }
                        Handle::Store(store_ix) => {
                            record_whole_args_if_projectable_static(
                                &mut projection_reads.borrow_mut(),
                                &exec_arg_schemas,
                                &exec_args,
                                &store_cell.borrow(),
                                descriptors,
                                schemas,
                                [store_ix.to_word()],
                            );
                            store_cell
                                .borrow()
                                .entry(store_ix.to_word())
                                .ok_or_else(|| format!("store handle {base}"))?
                                .bytes
                                .clone()
                        }
                    };
                    if matches!(descriptor.access, Access::Enum(_)) {
                        zero_inactive_enum_payload(&mut bytes, descriptor, variant_index);
                        if variant_index
                            != usize::try_from(read_variant_tag(&bytes, descriptor))
                                .unwrap_or(usize::MAX)
                        {
                            write_variant_tag(&mut bytes, descriptor, variant_index as u64);
                        }
                    }
                    for update_index in 0..update_count {
                        let field_index = usize::try_from(read_frame_word(
                            frame,
                            store_alloc_region + 40 + update_index * 16,
                        ))
                        .map_err(|_| "negative record field index".to_string())?;
                        let field_offset = field_offset(descriptor, &bytes, field_index);
                        let field = field_descriptor(descriptor, &bytes, field_index);
                        if field.layout.size > 8 {
                            return Err(format!(
                                "record update field {field_index} has {} bytes",
                                field.layout.size
                            ));
                        }
                        let value = canonicalize_word_for_schema(
                            schemas,
                            &schemas.display_ref(&field.schema),
                            read_frame_word(frame, store_alloc_region + 48 + update_index * 16),
                        );
                        write_canonical_word_field(
                            &mut bytes,
                            field_offset,
                            field.layout.size,
                            value,
                        );
                    }
                    let handle = molten_cell.borrow_mut().alloc(MoltenValue::Record {
                        schema: schema.clone(),
                        bytes,
                    });
                    write_frame_word(frame, dst_slot, handle);
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut hosts: [HostFn<'_>; 64] = [
                &mut invoke,
                &mut store_alloc,
                &mut store_read,
                &mut store_tag,
                &mut map_empty,
                &mut map_insert,
                &mut map_get,
                &mut option_unwrap,
                &mut acquire,
                &mut array_alloc,
                &mut array_map_pending,
                &mut array_collect,
                &mut tree_project,
                &mut exec_host,
                &mut path_with_ext,
                &mut pending_alloc,
                &mut pending_coerce,
                &mut pending_invoke,
                &mut fetch_host,
                &mut array_filter_exclude,
                &mut glob_host,
                &mut doc_parse_host,
                &mut doc_get_host,
                &mut doc_coerce_host,
                &mut elf_doc_host,
                &mut ast_doc_host,
                &mut ast_fn_host,
                &mut array_len_host,
                &mut oci_doc_host,
                &mut string_concat_host,
                &mut array_join_host,
                &mut doc_package_host,
                &mut target_host,
                &mut version_parse_host,
                &mut value_compare_host,
                &mut string_upper,
                &mut string_lower,
                &mut string_default,
                &mut sealed_seal,
                &mut sealed_declassify,
                &mut sealed_to_string,
                &mut version_set_parse_host,
                &mut version_set_op_host,
                &mut molten_intern,
                &mut array_push,
                &mut array_pop,
                &mut array_set,
                &mut array_get,
                &mut molten_dup,
                &mut record_update,
                &mut crate_archive_host,
                &mut version_field_host,
                &mut option_construct_host,
                &mut option_destruct_host,
                &mut string_split_host,
                &mut string_parse_int_host,
                &mut string_contains_host,
                &mut string_is_numeric_host,
                &mut path_join,
                &mut path_to_string,
                &mut doc_is_map,
                &mut tree_text,
                &mut doc_keys,
                &mut native_option_unwrap_none,
            ];
            let store_payloads = if task_has_native_array_load {
                store_cell
                    .borrow()
                    .entries
                    .iter()
                    .map(|entry| entry.bytes.clone())
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            let store_value_memories = store_payloads
                .iter()
                .map(|bytes| ValueMemory::from_slice(bytes))
                .collect::<Vec<_>>();
            let native_store_reads = native_array_store_read_handles_at_materialization(
                task_has_native_array_load,
                &exec_arg_schemas,
                &exec_args,
                &store_value_memories,
                schemas,
            );
            record_whole_args_if_projectable_static(
                &mut projection_reads.borrow_mut(),
                &exec_arg_schemas,
                &exec_args,
                &store_cell.borrow(),
                descriptors,
                schemas,
                native_store_reads,
            );
            let molten_payloads = if task_has_native_array_load {
                molten_cell
                    .borrow()
                    .entries
                    .iter()
                    .map(|entry| molten_value_payload(entry, schema_tables))
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            let molten_value_memories = molten_payloads
                .iter()
                .map(|payload| {
                    payload
                        .as_deref()
                        .map(ValueMemory::from_slice)
                        .unwrap_or_else(ValueMemory::empty)
                })
                .collect::<Vec<_>>();
            let value_memories = ValueMemories {
                store: &store_value_memories,
                molten: &molten_value_memories,
            };
            let step = exec.task.advance(
                &self.program,
                &mut exec.ready,
                &exec.awaited,
                &mut hosts,
                value_memories,
            );
            drop(hosts);
            for event in store_events.into_inner() {
                self.emit(event);
            }
            for read in projection_reads.into_inner() {
                exec.read_set.record(read);
            }
            if let Some(err) = host_error.into_inner() {
                return Burst::Error(err);
            }

            match step {
                TaskStep::Done => {
                    let value = exec.task.result_i64();
                    return Burst::Done(value);
                }
                TaskStep::Yielded => continue,
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
                    return Burst::Pending(Box::new(BurstPending {
                        new_requests: requests,
                        project_requests,
                        text_project_requests,
                        exec_requests,
                        fetch_requests,
                        doc_parse_requests,
                        crate_archive_requests,
                        option_unwraps,
                        pending_coercions,
                        pending_invokes,
                        parked_input: input,
                    }));
                }
            }
        }
    }

    fn memo_key(&self, fn_ref: FnRef, args: &[i64]) -> CanonMemoKey {
        let lowered = &self.lowered(fn_ref);
        let args = args
            .iter()
            .zip(&lowered.arg_schemas)
            .map(|(&word, schema)| self.canonical_word_hash(schema, word))
            .collect();
        (lowered.hash, args)
    }

    fn projection_candidate_key(
        &self,
        fn_ref: FnRef,
        args: &[i64],
    ) -> Option<ProjectionCandidateKey> {
        let lowered = &self.lowered(fn_ref);
        let semantic_args: BTreeSet<usize> = lowered
            .semantic_comparators
            .iter()
            .map(|comparator| comparator.arg_index)
            .collect();
        let mut saw_projectable = false;
        let args = args
            .iter()
            .zip(&lowered.arg_schemas)
            .enumerate()
            .map(|(arg_index, (&word, schema))| {
                if semantic_args.contains(&arg_index) || self.is_projectable_arg(schema, word) {
                    saw_projectable = true;
                    ProjectionArgKey::Projectable(schema.clone())
                } else {
                    ProjectionArgKey::Exact(self.canonical_word_hash(schema, word))
                }
            })
            .collect();
        saw_projectable.then_some((lowered.hash, args))
    }

    fn index_memo_candidate(&mut self, fn_ref: FnRef, args: &[i64], key: &CanonMemoKey) {
        if let Some(candidate_key) = self.projection_candidate_key(fn_ref, args) {
            let candidates = self.memo_candidates.entry(candidate_key).or_default();
            if !candidates.contains(key) {
                candidates.push(key.clone());
            }
        }
    }

    fn projection_memo_hit(
        &self,
        fn_ref: FnRef,
        args: &[i64],
        key: &CanonMemoKey,
    ) -> Result<Option<MemoEntry>, String> {
        let Some(candidate_key) = self.projection_candidate_key(fn_ref, args) else {
            return Ok(None);
        };
        let Some(candidates) = self.memo_candidates.get(&candidate_key) else {
            return Ok(None);
        };
        for candidate in candidates {
            if candidate == key {
                continue;
            }
            let Some(entry) = self.memo.get(candidate) else {
                continue;
            };
            if entry.read_set.is_empty() {
                continue;
            }
            if self.verify_projection_read_set(args, &entry.read_set)? {
                return Ok(Some(entry.clone()));
            }
        }
        Ok(None)
    }

    fn semantic_memo_hit(
        &mut self,
        fn_ref: FnRef,
        args: &[i64],
        key: &CanonMemoKey,
    ) -> Result<Option<MemoEntry>, String> {
        if self.lowered(fn_ref).semantic_comparators.is_empty() {
            return Ok(None);
        }
        let Some(candidate_key) = self.projection_candidate_key(fn_ref, args) else {
            return Ok(None);
        };
        let Some(candidates) = self.memo_candidates.get(&candidate_key).cloned() else {
            return Ok(None);
        };
        for candidate in candidates {
            if &candidate == key {
                continue;
            }
            let Some(entry) = self.memo.get(&candidate).cloned() else {
                continue;
            };
            if self.verify_semantic_comparators(fn_ref, &entry.args, args)? {
                return Ok(Some(entry));
            }
        }
        Ok(None)
    }

    fn verify_semantic_comparators(
        &mut self,
        fn_ref: FnRef,
        old_args: &[i64],
        new_args: &[i64],
    ) -> Result<bool, String> {
        let comparators = self.lowered(fn_ref).semantic_comparators.clone();
        for comparator in comparators {
            let Some(&old_value) = old_args.get(comparator.arg_index) else {
                return Ok(false);
            };
            let Some(&new_value) = new_args.get(comparator.arg_index) else {
                return Ok(false);
            };
            if old_value == new_value {
                continue;
            }
            let accepted = self.demand(comparator.fn_ref, vec![old_value, new_value])?;
            if accepted == 0 {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn verify_projection_read_set(
        &self,
        args: &[i64],
        read_set: &ProjectionReadSet,
    ) -> Result<bool, String> {
        let mut store = self.store.borrow_mut();
        for read in &read_set.entries {
            let Some(&arg) = args.get(read.arg_index) else {
                return Ok(false);
            };
            let observed = projection_observation_hash(
                &mut store,
                &self.descriptors,
                &self.schemas,
                &self.schemas,
                arg,
                &read.path,
            )?;
            if observed != read.observed {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn is_projectable_arg(&self, schema: &str, word: i64) -> bool {
        self.store
            .borrow()
            .entry(word)
            .is_some_and(|entry| entry.schema == schema && self.is_projectable_schema(schema))
    }

    fn is_projectable_schema(&self, schema: &str) -> bool {
        self.schemas.is_external(schema, "Tree")
            || self.schemas.is_list(schema)
            || self.schemas.is_named_schema(schema, "Doc")
            || self.schemas.is_primitive(schema, Primitive::Bytes)
            || self.schemas.is_map(schema)
            || pending_value_schema(schema).is_some()
            || self.descriptors.contains_key(schema)
    }

    fn record_projection_for_matching_args(
        &self,
        fn_ref: FnRef,
        args: &[i64],
        handle: i64,
        path: ProjectionPath,
        observed: ContentHash,
        read_set: &mut ProjectionReadSet,
    ) {
        let lowered = &self.lowered(fn_ref);
        for (arg_index, (&arg, schema)) in args.iter().zip(&lowered.arg_schemas).enumerate() {
            if arg == handle && self.is_projectable_arg(schema, arg) {
                read_set.record(ProjectionRead {
                    arg_index,
                    path: path.clone(),
                    observed,
                });
            }
        }
    }

    fn record_whole_arg_if_projectable(
        &self,
        fn_ref: FnRef,
        args: &[i64],
        handle: i64,
        read_set: &mut ProjectionReadSet,
    ) {
        let store = self.store.borrow();
        let lowered = &self.lowered(fn_ref);
        for (arg_index, (&arg, schema)) in args.iter().zip(&lowered.arg_schemas).enumerate() {
            if arg == handle && self.is_projectable_arg(schema, arg) {
                read_set.record(ProjectionRead {
                    arg_index,
                    path: ProjectionPath::Whole {
                        schema: schema.clone(),
                    },
                    observed: canonical_word_hash_in_store(&store, &self.schemas, schema, arg),
                });
            }
        }
    }

    fn record_whole_args_if_projectable(
        &self,
        fn_ref: FnRef,
        args: &[i64],
        handles: impl IntoIterator<Item = i64>,
        read_set: &mut ProjectionReadSet,
    ) {
        for handle in handles {
            self.record_whole_arg_if_projectable(fn_ref, args, handle, read_set);
        }
    }

    fn canonical_word_hash(&self, schema: &str, word: i64) -> ContentHash {
        let store = self.store.borrow();
        canonical_word_hash_in_store(&store, &self.schemas, schema, word)
    }

    fn canonicalize_return_word(&self, fn_ref: FnRef, word: i64) -> i64 {
        if self
            .schemas
            .is_primitive(&self.lowered(fn_ref).return_schema, Primitive::F64)
        {
            canonicalize_word_for_schema(&self.schemas, "Float", word)
        } else {
            word
        }
    }

    fn project_request(
        &mut self,
        req: ProjectRequest,
        fn_ref: FnRef,
        args: &[i64],
        read_set: &mut ProjectionReadSet,
    ) -> Result<(usize, i64), String> {
        let path = self.store.borrow().string_value(req.path, "Path")?;
        let before = self.store.borrow().tree_entry(req.tree)?;
        let value = self.project_tree_path(req.tree, &path)?;
        let observed = self
            .store
            .borrow()
            .entry(value)
            .ok_or_else(|| format!("store handle {value}"))?
            .content_hash;
        match before {
            TreeEntry::Concrete(_) => {
                self.record_projection_for_matching_args(
                    fn_ref,
                    args,
                    req.tree,
                    ProjectionPath::TreePath { path },
                    observed,
                    read_set,
                );
            }
            TreeEntry::Merge(_) | TreeEntry::Exec(_) => {
                self.record_whole_arg_if_projectable(fn_ref, args, req.tree, read_set);
            }
        }
        Ok((req.input_slot, value))
    }

    fn text_project_request(
        &mut self,
        req: TextProjectRequest,
        fn_ref: FnRef,
        args: &[i64],
        read_set: &mut ProjectionReadSet,
    ) -> Result<(usize, i64), String> {
        let path = self.store.borrow().string_value(req.path, "Path")?;
        let before = self.store.borrow().tree_entry(req.tree)?;
        let value = self.project_tree_text(req.tree, &path)?;
        let observed = self
            .store
            .borrow()
            .entry(value)
            .ok_or_else(|| format!("store handle {value}"))?
            .content_hash;
        match before {
            TreeEntry::Concrete(_) => {
                self.record_projection_for_matching_args(
                    fn_ref,
                    args,
                    req.tree,
                    ProjectionPath::TreePath { path },
                    observed,
                    read_set,
                );
            }
            TreeEntry::Merge(_) | TreeEntry::Exec(_) => {
                self.record_whole_arg_if_projectable(fn_ref, args, req.tree, read_set);
            }
        }
        Ok((req.input_slot, value))
    }

    fn project_tree_path(&mut self, tree_handle: i64, path: &str) -> Result<i64, String> {
        let tree = self.store.borrow().tree_entry(tree_handle)?;
        match tree {
            TreeEntry::Concrete(tree) => {
                let projected = subtree(&tree, path)?;
                Ok(self.store.borrow_mut().alloc_tree_concrete(projected).0)
            }
            TreeEntry::Merge(pending) => {
                for handle in pending.into_iter().rev() {
                    let value = self.merge_child_tree_value(handle)?;
                    if let Some(found) = self.project_tree_path_optional(value, path)? {
                        return Ok(found);
                    }
                }
                Err(PathMissing {
                    path: path.to_string(),
                }
                .diagnostic())
            }
            TreeEntry::Exec(run_id) => {
                self.demand_exec_path(run_id, path, false)?.ok_or_else(|| {
                    PathMissing {
                        path: path.to_string(),
                    }
                    .diagnostic()
                })
            }
        }
    }

    fn project_tree_text(&mut self, tree_handle: i64, path: &str) -> Result<i64, String> {
        let tree = self.store.borrow().tree_entry(tree_handle)?;
        match tree {
            TreeEntry::Concrete(tree) => {
                let text = tree_text(&tree, path)?;
                Ok(self
                    .store
                    .borrow_mut()
                    .alloc_raw("String", text.into_bytes(), &self.schemas)
                    .0)
            }
            TreeEntry::Merge(pending) => {
                for handle in pending.into_iter().rev() {
                    let value = self.merge_child_tree_value(handle)?;
                    if let Ok(found) = self.project_tree_text(value, path) {
                        return Ok(found);
                    }
                }
                Err(PathMissing {
                    path: path.to_string(),
                }
                .diagnostic())
            }
            TreeEntry::Exec(run_id) => {
                let projected = self.demand_exec_path(run_id, path, false)?.ok_or_else(|| {
                    PathMissing {
                        path: path.to_string(),
                    }
                    .diagnostic()
                })?;
                self.project_tree_text(projected, path.rsplit_once('/').map_or(path, |(_, b)| b))
            }
        }
    }

    fn merge_child_tree_value(&mut self, handle: i64) -> Result<i64, String> {
        let entry = self
            .store
            .borrow()
            .entry(handle)
            .ok_or_else(|| format!("store handle {handle}"))?
            .clone();
        if self.schemas.is_external(&entry.schema, "Tree") && !entry.tier.is_pending() {
            return Ok(handle);
        }
        if !entry.tier.is_pending() || !self.schemas.is_external(&entry.schema, "Tree") {
            return Err(format!(
                "merge child handle {handle} is `{}`, not Tree",
                entry.schema
            ));
        }
        let invocation = self.store.borrow().pending_invocation(handle)?;
        if invocation.primitive.is_some() {
            return Err("primitive pending value cannot produce a tree".into());
        }
        let fn_ref = self.fn_ref_for_hash(invocation.closure_hash)?;
        self.demand(fn_ref, invocation.args)
    }

    fn force_tree_handle(&mut self, tree_handle: i64) -> Result<i64, String> {
        let tree = self.store.borrow().tree_entry(tree_handle)?;
        match tree {
            TreeEntry::Concrete(_) => Ok(tree_handle),
            TreeEntry::Merge(pending) => {
                let mut merged = crate::exec::Tree::default();
                let mut values = Vec::with_capacity(pending.len());
                for handle in pending {
                    values.push(self.merge_child_tree_value(handle)?);
                }
                for &value in &values {
                    self.start_tree_runs(value)?;
                }
                for value in values {
                    let value = self.force_tree_handle(value)?;
                    let TreeEntry::Concrete(tree) = self.store.borrow().tree_entry(value)? else {
                        return Err("forced merge branch stayed pending".into());
                    };
                    for (path, contents) in tree.entries {
                        merged.entries.insert(path, contents);
                    }
                    for (path, contents) in tree.blobs {
                        merged.blobs.insert(path, contents);
                    }
                }
                Ok(self.store.borrow_mut().alloc_tree_concrete(merged).0)
            }
            TreeEntry::Exec(run_id) => {
                let tree = self.finish_run(run_id)?;
                Ok(self.store.borrow_mut().alloc_tree_concrete(tree).0)
            }
        }
    }

    fn start_tree_runs(&mut self, tree_handle: i64) -> Result<(), String> {
        let tree = self.store.borrow().tree_entry(tree_handle)?;
        match tree {
            TreeEntry::Concrete(_) => Ok(()),
            TreeEntry::Exec(run_id) => self.ensure_run_started(run_id),
            TreeEntry::Merge(pending) => {
                let mut values = Vec::with_capacity(pending.len());
                for handle in pending {
                    values.push(self.merge_child_tree_value(handle)?);
                }
                for value in values {
                    self.start_tree_runs(value)?;
                }
                Ok(())
            }
        }
    }

    fn pending_coercion(
        &mut self,
        req: PendingCoerceRequest,
        caller: usize,
        fn_ref: FnRef,
        args: &[i64],
        read_set: &mut ProjectionReadSet,
    ) -> Result<PendingForce, String> {
        let entry = self
            .store
            .borrow()
            .entry(req.pending)
            .ok_or_else(|| format!("store handle {}", req.pending))?
            .clone();
        if !entry.tier.is_pending() {
            return Err(format!(
                "pending coercion expected Pending<T>, got {}",
                entry.schema
            ));
        }
        let value_schema = entry.schema;
        let invocation = self.store.borrow().pending_invocation(req.pending)?;
        if invocation.remaining_arity != 0 {
            return Err(format!(
                "cannot coerce pending {value_schema} with {} remaining args",
                invocation.remaining_arity
            ));
        }
        if let Some(primitive) = invocation.primitive {
            let value = match primitive.kind.clone() {
                PendingPrimitiveKind::Elf(projection) => {
                    self.force_elf_projection(projection, &invocation.args)?
                }
                PendingPrimitiveKind::Ast(projection) => {
                    self.force_ast_projection(projection, &invocation.args)?
                }
                PendingPrimitiveKind::Oci(projection) => {
                    self.force_oci_projection(projection, &invocation.args)?
                }
            };
            if let Some((&input, rest)) = invocation.args.split_first() {
                let observed = self
                    .store
                    .borrow()
                    .entry(value)
                    .ok_or_else(|| format!("store handle {value}"))?
                    .content_hash;
                let path = match primitive.kind.clone() {
                    PendingPrimitiveKind::Elf(projection) => ProjectionPath::Elf {
                        projection: projection.name().to_string(),
                    },
                    PendingPrimitiveKind::Ast(projection) => {
                        let name = if matches!(
                            projection,
                            super::ast_probe::Projection::Fn
                                | super::ast_probe::Projection::FnBodyChildren
                        ) {
                            Some(self.store.borrow().string_value(
                                *rest.first().ok_or_else(|| {
                                    format!(
                                        "ast projection {} expected a name argument",
                                        projection.name()
                                    )
                                })?,
                                "String",
                            )?)
                        } else {
                            None
                        };
                        ProjectionPath::Ast {
                            projection: projection.name().to_string(),
                            name,
                        }
                    }
                    PendingPrimitiveKind::Oci(projection) => ProjectionPath::Oci {
                        projection: projection.name().to_string(),
                    },
                };
                self.record_projection_for_matching_args(
                    fn_ref, args, input, path, observed, read_set,
                );
            }
            return Ok(PendingForce::Ready {
                input_slot: req.input_slot,
                value,
            });
        }
        Ok(PendingForce::Invoke(InvokeRequest {
            caller,
            input_slot: req.input_slot,
            fn_ref: self.fn_ref_for_hash(invocation.closure_hash)?,
            args: invocation.args,
        }))
    }

    fn pending_invocation_call(&self, req: PendingInvokeRequest) -> Result<InvokeRequest, String> {
        let invocation = self.store.borrow().pending_invocation(req.pending)?;
        if invocation.primitive.is_some() {
            return Err("primitive pending values are not callable".into());
        }
        if invocation.remaining_arity != req.args.len() {
            return Err(format!(
                "pending invocation expected {} argument(s), got {}",
                invocation.remaining_arity,
                req.args.len()
            ));
        }
        let fn_ref = self.fn_ref_for_hash(invocation.closure_hash)?;
        let mut args = invocation.args;
        args.extend(req.args);
        if args.len() != self.lowered(fn_ref).arg_schemas.len() {
            return Err(format!(
                "pending invocation completed to {} argument(s), expected {}",
                args.len(),
                self.lowered(fn_ref).arg_schemas.len()
            ));
        }
        Ok(InvokeRequest {
            caller: req.caller,
            input_slot: req.input_slot,
            fn_ref,
            args,
        })
    }

    fn option_unwrap_request(&self, req: OptionUnwrapRequest) -> Result<Vec<(usize, i64)>, String> {
        match self
            .store
            .borrow()
            .option_payload(req.option, &self.schemas, &self.schemas)?
        {
            OptionPayload::None => Err("unwrap on None".into()),
            OptionPayload::Some { word, realization } => {
                let mut fills = vec![(req.input_slot, word)];
                if let Some(slot) = req.realization_slot {
                    let realization = realization.ok_or_else(|| {
                        "Option unwrap expected realization bit, got plain payload".to_string()
                    })?;
                    fills.push((slot, realization.to_word()));
                }
                Ok(fills)
            }
        }
    }

    fn fn_ref_for_hash(&self, closure_hash: u64) -> Result<FnRef, String> {
        self.fns
            .iter()
            .position(|lowered| lowered.hash == closure_hash)
            .map(FnRef::new)
            .ok_or_else(|| format!("no function with closure hash {closure_hash:016x}"))
    }

    fn project_tree_path_optional(
        &mut self,
        tree_handle: i64,
        path: &str,
    ) -> Result<Option<i64>, String> {
        let tree = self.store.borrow().tree_entry(tree_handle)?;
        let tree = match tree {
            TreeEntry::Concrete(tree) => tree,
            TreeEntry::Exec(run_id) => {
                return self.demand_exec_path(run_id, path, true);
            }
            TreeEntry::Merge(_) => {
                return Err("nested merge tree projection is outside slice 4".into());
            }
        };
        match subtree(&tree, path) {
            Ok(projected) => Ok(Some(
                self.store.borrow_mut().alloc_tree_concrete(projected).0,
            )),
            Err(_) => Ok(None),
        }
    }

    fn execute_request(&mut self, req: ExecRequest) -> Result<(usize, i64), String> {
        let cap_key = self
            .store
            .borrow()
            .string_value(req.capability, &cap_schema(&req.command))?;
        let cap_hash = capability_hash(&req.command, &cap_key);
        let mut argv = Vec::new();
        let mut mounts = Vec::new();
        for part in req.parts {
            match part {
                CommandRequestPart::Token(handle) => {
                    let token = self.store.borrow().string_value(handle, "String")?;
                    append_command_token(token, &mut argv)?;
                }
                CommandRequestPart::Splice(word) => {
                    let prefer_tree_root = argv.last().is_some_and(|arg| {
                        arg.ends_with("dependency=")
                            || arg.ends_with("OUT_DIR=")
                            || arg.ends_with("CARGO_MANIFEST_DIR=")
                    });
                    let args =
                        self.splice_word_to_command_args(word, &mut mounts, prefer_tree_root)?;
                    append_spliced_command_args(args, &mut argv)?;
                }
            }
        }
        let plan = assign_roles(&req.command, &argv)?;
        let output = plan
            .argv
            .iter()
            .find(|(_, role)| *role == crate::exec::Role::Output)
            .map(|(path, _)| path.clone())
            .unwrap_or_default();
        let identity = pending_exec_identity_hash(
            &self.store.borrow(),
            &req.command,
            &plan,
            cap_hash,
            &mounts,
        );
        let run_id = self.next_run_id;
        self.next_run_id = self.next_run_id.saturating_add(1);
        let timestamp_us = self.next_timestamp();
        self.emit(DriveEvent::RunRequested {
            command: hash_u64(&req.command),
            output: hash_u64(&output),
            run_id,
            command_name: req.command.clone(),
            capability_key: cap_key,
            argv: argv.clone(),
            describe: crate::exec::describe(&req.command, &plan),
            span: req.span,
            timestamp_us,
        });
        self.runs.insert(
            run_id,
            PendingExecRun {
                command: req.command,
                plan,
                capability: cap_hash,
                mounts,
                output,
                scheduled: false,
                completed: None,
                completion_logged: false,
                remote: None,
                completion: None,
                span: req.span,
            },
        );
        let handle = self.store.borrow_mut().alloc_tree_exec(run_id, identity).0;
        Ok((req.input_slot, handle))
    }

    fn ensure_run_started(&mut self, run_id: u64) -> Result<(), String> {
        let needs_start = {
            let run = self
                .runs
                .get(&run_id)
                .ok_or_else(|| format!("run {run_id}"))?;
            run.completed.is_none() && run.remote.is_none()
        };
        if !self
            .runs
            .get(&run_id)
            .ok_or_else(|| format!("run {run_id}"))?
            .scheduled
        {
            let (command, output) = {
                let run = self.runs.get_mut(&run_id).expect("run checked");
                run.scheduled = true;
                (run.command.clone(), run.output.clone())
            };
            let timestamp_us = self.next_timestamp();
            self.emit(DriveEvent::RunStarted {
                command: hash_u64(&command),
                output: hash_u64(&output),
                run_id,
                command_name: command,
                timestamp_us,
            });
        }
        if needs_start && let Some(backend) = self.exec_backend.clone() {
            let (command, plan, capability, mounts) = {
                let run = self.runs.get(&run_id).expect("run checked");
                (
                    run.command.clone(),
                    run.plan.clone(),
                    run.capability,
                    run.mounts.clone(),
                )
            };
            let mounts = self.resolve_exec_mounts(&mounts)?;
            let request = {
                let run = self.runs.get(&run_id).expect("run checked");
                MachineExecRequest {
                    command,
                    plan,
                    capability,
                    mounts,
                    output: run.output.clone(),
                    span: run.span,
                    observer: None,
                }
            };
            let remote = backend.spawn(request)?;
            let (completion_tx, completion_rx) = mpsc::channel();
            let completion_remote = Arc::clone(&remote);
            std::thread::spawn(move || {
                let _ = completion_tx.send(completion_remote.flush());
            });
            let run = self.runs.get_mut(&run_id).expect("run checked");
            run.remote = Some(remote);
            run.completion = Some(completion_rx);
        }
        Ok(())
    }

    fn demand_exec_path(
        &mut self,
        run_id: u64,
        path: &str,
        complete_on_file: bool,
    ) -> Result<Option<i64>, String> {
        self.ensure_run_started(run_id)?;
        if let Some(remote) = self.runs.get(&run_id).and_then(|run| run.remote.clone()) {
            match remote.demand_path(path)? {
                MachinePathDemand::File(contents) => {
                    if complete_on_file {
                        let _ = self.finish_run(run_id)?;
                    }
                    let base = path.rsplit_once('/').map(|(_, base)| base).unwrap_or(path);
                    let tree = crate::exec::Tree::of(&[(base, contents.as_str())]);
                    Ok(Some(self.store.borrow_mut().alloc_tree_concrete(tree).0))
                }
                MachinePathDemand::FinishRequired { .. } => {
                    let tree = self.finish_run(run_id)?;
                    match subtree(&tree, path) {
                        Ok(projected) => Ok(Some(
                            self.store.borrow_mut().alloc_tree_concrete(projected).0,
                        )),
                        Err(_) => Ok(None),
                    }
                }
                MachinePathDemand::Missing { .. } => {
                    let _ = self.finish_run(run_id)?;
                    Ok(None)
                }
            }
        } else {
            let outcome = self.schedule_run(run_id)?;
            match subtree(&outcome.outputs, path) {
                Ok(projected) => {
                    if complete_on_file {
                        let _ = self.finish_run(run_id)?;
                    }
                    Ok(Some(
                        self.store.borrow_mut().alloc_tree_concrete(projected).0,
                    ))
                }
                Err(_) => {
                    let _ = self.finish_run(run_id)?;
                    Ok(None)
                }
            }
        }
    }

    fn schedule_run(&mut self, run_id: u64) -> Result<crate::exec::Outcome, String> {
        self.ensure_run_started(run_id)?;
        if let Some(outcome) = self
            .runs
            .get(&run_id)
            .and_then(|run| run.completed.as_ref().map(|(outcome, _)| outcome.clone()))
        {
            return Ok(outcome);
        }
        if self
            .runs
            .get(&run_id)
            .and_then(|run| run.remote.clone())
            .is_some()
        {
            let (outcome, event) = self.await_run_completion(run_id)?;
            let run = self.runs.get_mut(&run_id).expect("run checked");
            run.completed = Some((outcome.clone(), event));
            return Ok(outcome);
        }
        let (command, plan, capability, mounts) = {
            let run = self.runs.get(&run_id).expect("run checked");
            (
                run.command.clone(),
                run.plan.clone(),
                run.capability,
                run.mounts.clone(),
            )
        };
        let mounts = self.resolve_exec_mounts(&mounts)?;
        let tool = tool_for(&command)?;
        let outcome = self.exec_cache.exec(&plan, capability, &mounts, tool)?;
        let event = self
            .exec_cache
            .events
            .last()
            .cloned()
            .expect("exec pushed an event");
        let run = self.runs.get_mut(&run_id).expect("run checked");
        run.completed = Some((outcome.clone(), event));
        Ok(outcome)
    }

    fn finish_run(&mut self, run_id: u64) -> Result<crate::exec::Tree, String> {
        let outcome = self.schedule_run(run_id)?;
        let should_log = !self
            .runs
            .get(&run_id)
            .ok_or_else(|| format!("run {run_id}"))?
            .completion_logged;
        if should_log {
            let (command, output, serving, outputs) = {
                let run = self.runs.get_mut(&run_id).expect("run checked");
                run.completion_logged = true;
                let (_, event) = run.completed.as_ref().expect("run completed");
                (
                    run.command.clone(),
                    run.output.clone(),
                    event.clone(),
                    outcome.outputs.display_entries(),
                )
            };
            let timestamp_us = self.next_timestamp();
            self.emit(DriveEvent::RunCompleted {
                command: hash_u64(&command),
                output: hash_u64(&output),
                run_id,
                command_name: command,
                serving,
                outputs,
                timestamp_us,
            });
        }
        Ok(outcome.outputs)
    }

    fn splice_word_to_command_args(
        &mut self,
        word: i64,
        mounts: &mut Vec<ExecMount>,
        prefer_tree_root: bool,
    ) -> Result<Vec<String>, String> {
        let entry = self
            .store
            .borrow()
            .entry(word)
            .cloned()
            .ok_or_else(|| format!("cannot splice scalar word {word} into a command"))?;
        match entry.schema.as_str() {
            "Path" | "String" | "Flag" => Ok(vec![
                String::from_utf8(entry.bytes).map_err(|err| err.to_string())?,
            ]),
            "Arg" => self.splice_arg_to_command_args(word, mounts),
            schema if self.schemas.is_list(schema) => {
                match { self.store.borrow().array_entry(word, &self.schemas)? } {
                    ArrayEntry::Words { words, .. } => {
                        let mut args = Vec::new();
                        for word in words {
                            let nested = self.splice_word_to_command_args(word, mounts, false)?;
                            append_spliced_command_args(nested, &mut args)?;
                        }
                        Ok(args)
                    }
                    ArrayEntry::Pending { .. } => {
                        Err("pending arrays cannot be spliced into commands".into())
                    }
                }
            }
            "Tree" => {
                let forced = self.force_tree_handle(word)?;
                let TreeEntry::Concrete(tree) = self.store.borrow().tree_entry(forced)? else {
                    return Err("forced command tree stayed pending".into());
                };
                let root = format!("/m/{}", mounts.len());
                let entry_count = tree.entries.len() + tree.blobs.len();
                let text = if entry_count == 1 && !prefer_tree_root {
                    let key = tree
                        .entries
                        .keys()
                        .next()
                        .or_else(|| tree.blobs.keys().next())
                        .expect("one entry");
                    format!("{root}/{key}")
                } else {
                    root.clone()
                };
                mounts.push(ExecMount::Concrete(crate::exec::Mount { at: root, tree }));
                Ok(vec![text])
            }
            other => Err(format!("cannot splice {other} into a command")),
        }
    }

    fn splice_arg_to_command_args(
        &mut self,
        word: i64,
        mounts: &mut Vec<ExecMount>,
    ) -> Result<Vec<String>, String> {
        let entry = self
            .store
            .borrow()
            .entry(word)
            .cloned()
            .ok_or_else(|| format!("store handle {word}"))?;
        let descriptor = self
            .descriptors
            .get("Arg")
            .ok_or_else(|| "missing Arg descriptor".to_string())?;
        let selector = read_variant_tag(&entry.bytes, descriptor);
        match selector {
            0 => {
                let value =
                    read_frame_word(&entry.bytes, field_offset(descriptor, &entry.bytes, 0));
                Ok(vec![self.store.borrow().string_value(value, "String")?])
            }
            1 => {
                let value =
                    read_frame_word(&entry.bytes, field_offset(descriptor, &entry.bytes, 0));
                Ok(vec![self.store.borrow().string_value(value, "Path")?])
            }
            2 => {
                let tree = read_frame_word(&entry.bytes, field_offset(descriptor, &entry.bytes, 0));
                let subpath =
                    read_frame_word(&entry.bytes, field_offset(descriptor, &entry.bytes, 1));
                let subpath = self.store.borrow().string_value(subpath, "Path")?;
                let root = format!("/m/{}", mounts.len());
                let text = if subpath.is_empty() {
                    root.clone()
                } else {
                    format!("{root}/{}", subpath.trim_start_matches('/'))
                };
                mounts.push(ExecMount::PendingTree { at: root, tree });
                Ok(vec![text])
            }
            other => Err(format!("unknown Arg selector {other}")),
        }
    }

    fn resolve_exec_mounts(
        &mut self,
        mounts: &[ExecMount],
    ) -> Result<Vec<crate::exec::Mount>, String> {
        mounts
            .iter()
            .map(|mount| match mount {
                ExecMount::Concrete(mount) => Ok(mount.clone()),
                ExecMount::PendingTree { at, tree } => {
                    let forced = self.force_tree_handle(*tree)?;
                    let TreeEntry::Concrete(tree) = self.store.borrow().tree_entry(forced)? else {
                        return Err("forced command tree stayed pending".into());
                    };
                    Ok(crate::exec::Mount {
                        at: at.clone(),
                        tree,
                    })
                }
            })
            .collect()
    }

    fn prepare_fetch_request(&mut self, req: FetchRequest) -> Result<PreparedFetchRequest, String> {
        let url = self.store.borrow().string_value(req.url, "String")?;
        let declared_sha256 = if req.sha256 < 0 {
            None
        } else {
            Some(self.store.borrow().string_value(req.sha256, "String")?)
        };
        let key = match &declared_sha256 {
            Some(sha) => format!("fetch:{url}:sha256:{sha}"),
            None => format!("fetch:{url}:observed"),
        };
        let (expected_sha256, replayed) = if let Some(pin) = self.journal.get(&key).copied() {
            (Some(self.store.borrow().string_value(pin, "String")?), true)
        } else {
            (declared_sha256, false)
        };
        Ok(PreparedFetchRequest {
            input_slot: req.input_slot,
            key,
            url,
            expected_sha256,
            replayed,
        })
    }

    fn start_fetch_run(&mut self, prepared: PreparedFetchRequest) -> PendingFetchRun {
        let backend = Arc::clone(&self.fetch_backend);
        let thread_url = prepared.url.clone();
        let thread_expected_sha256 = prepared.expected_sha256.clone();
        let (completion_tx, completion_rx) = mpsc::channel();
        std::thread::spawn(move || {
            let result = backend.fetch(&thread_url, thread_expected_sha256.as_deref());
            let _ = completion_tx.send(result);
        });
        PendingFetchRun {
            key: prepared.key,
            replayed: prepared.replayed,
            completion: completion_rx,
        }
    }

    fn finish_fetch_run(
        &mut self,
        run: PendingFetchRun,
        fetched: crate::fetch::FetchOutput,
    ) -> Result<i64, String> {
        if !run.replayed {
            let pin_handle = self
                .store
                .borrow_mut()
                .alloc_raw("String", fetched.actual_sha256.into_bytes(), &self.schemas)
                .0;
            self.journal.insert(run.key.clone(), pin_handle);
        }
        let timestamp_us = self.next_timestamp();
        self.emit(DriveEvent::Observation {
            key: hash_u64(&run.key),
            replayed: run.replayed,
            key_text: run.key,
            timestamp_us,
        });
        Ok(self.store.borrow_mut().alloc_tree_concrete(fetched.tree).0)
    }

    fn doc_parse_request(&mut self, req: DocParseRequest) -> Result<(usize, i64), String> {
        let input = self.document_input_value(req.input)?;
        let input_text = document_input_text_for_error(&input);
        let value = match req.kind {
            DocParseKind::Toml => crate::data::parse_toml(input)?,
            DocParseKind::Json => crate::data::parse_json(input)?,
            DocParseKind::BuildDirectives => crate::data::parse_build_directives(input)?,
            DocParseKind::Cfg => crate::data::parse_cfg(input)?,
            DocParseKind::RustcCfg => crate::data::parse_rustc_cfg(input)?,
        };
        let handle = if let Some(schema) = req.target_schema {
            alloc_typed_from_value(
                &self.store,
                &self.descriptors,
                &self.schemas,
                &self.schemas,
                &schema,
                value,
            )
            .map_err(|err| {
                format!(
                    "typed {:?} parse into {schema} failed: {err}; offending input: {input_text}",
                    req.kind
                )
            })?
        } else {
            alloc_doc_from_value(
                &self.store,
                &self.descriptors,
                &self.schemas,
                &self.schemas,
                value,
            )?
        };
        Ok((req.input_slot, handle))
    }

    fn crate_archive_request(&mut self, req: CrateArchiveRequest) -> Result<(usize, i64), String> {
        let input_hash = self
            .store
            .borrow()
            .entry(req.input)
            .ok_or_else(|| format!("store handle {}", req.input))?
            .content_hash;
        if let Some(&handle) = self.crate_archive_memo.get(&input_hash) {
            let timestamp_us = self.next_timestamp();
            self.emit(DriveEvent::ArtifactProbe {
                format: "crate_archive".to_string(),
                projection: "tree".to_string(),
                input: hash_u64(input_hash),
                cache_hit: true,
                timestamp_us,
            });
            return Ok((req.input_slot, handle));
        }

        let bytes = self.crate_archive_input_bytes(req.input)?;
        let tree = super::crate_archive::archive_to_tree(&bytes)?;
        let handle = self.store.borrow_mut().alloc_tree_concrete(tree).0;
        self.crate_archive_memo.insert(input_hash, handle);
        let timestamp_us = self.next_timestamp();
        self.emit(DriveEvent::ArtifactProbe {
            format: "crate_archive".to_string(),
            projection: "tree".to_string(),
            input: hash_u64(input_hash),
            cache_hit: false,
            timestamp_us,
        });
        Ok((req.input_slot, handle))
    }

    fn crate_archive_input_bytes(&mut self, handle: i64) -> Result<Vec<u8>, String> {
        let entry = self
            .store
            .borrow()
            .entry(handle)
            .cloned()
            .ok_or_else(|| format!("store handle {handle}"))?;
        match entry.schema.as_str() {
            "Blob" | "String" => Ok(entry.bytes),
            "Tree" => {
                let forced = self.force_tree_handle(handle)?;
                let TreeEntry::Concrete(tree) = self.store.borrow().tree_entry(forced)? else {
                    return Err("crate archive input tree stayed pending".into());
                };
                let count = tree.entries.len() + tree.blobs.len();
                if count != 1 {
                    return Err(format!(
                        "crate archive input tree must contain exactly one .crate file, got {count}"
                    ));
                }
                let (path, contents) = tree
                    .entries
                    .into_iter()
                    .map(|(path, contents)| (path, contents.into_bytes()))
                    .chain(tree.blobs)
                    .next()
                    .expect("one tree entry");
                if contents.is_empty() {
                    return Err(format!("crate archive input `{path}` is empty"));
                }
                Ok(contents)
            }
            other => Err(format!(
                "crate_archive input must be Blob, String, or single-file Tree, got {other}"
            )),
        }
    }

    fn document_input_value(&mut self, handle: i64) -> Result<Value, String> {
        let entry = self
            .store
            .borrow()
            .entry(handle)
            .cloned()
            .ok_or_else(|| format!("store handle {handle}"))?;
        match entry.schema.as_str() {
            "String" => Ok(Value::Str(
                String::from_utf8(entry.bytes).map_err(|err| err.to_string())?,
            )),
            "Tree" => {
                let forced = self.force_tree_handle(handle)?;
                let TreeEntry::Concrete(tree) = self.store.borrow().tree_entry(forced)? else {
                    return Err("document parser input tree stayed pending".into());
                };
                Ok(Value::Tree(tree))
            }
            other => Err(format!(
                "document parser input must be String or Tree, got {other}"
            )),
        }
    }

    fn force_elf_projection(
        &mut self,
        projection: super::elf::Projection,
        args: &[i64],
    ) -> Result<i64, String> {
        let [input] = args else {
            return Err(format!(
                "elf projection {} expected one input, got {}",
                projection.name(),
                args.len()
            ));
        };
        let bytes = self.elf_input_bytes(*input)?;
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"vix-elf-input");
        hasher.update(&bytes);
        let input_hash: ContentHash = finish_hash(hasher);
        if let Some(&handle) = self.elf_projection_memo.get(&(input_hash, projection)) {
            let timestamp_us = self.next_timestamp();
            self.emit(DriveEvent::ArtifactProbe {
                format: "elf".to_string(),
                projection: projection.name().to_string(),
                input: hash_u64(input_hash),
                cache_hit: true,
                timestamp_us,
            });
            return Ok(handle);
        }
        let value = super::elf::project(&bytes, projection)?;
        let handle = alloc_doc_from_value(
            &self.store,
            &self.descriptors,
            &self.schemas,
            &self.schemas,
            value,
        )?;
        self.elf_projection_memo
            .insert((input_hash, projection), handle);
        let timestamp_us = self.next_timestamp();
        self.emit(DriveEvent::ArtifactProbe {
            format: "elf".to_string(),
            projection: projection.name().to_string(),
            input: hash_u64(input_hash),
            cache_hit: false,
            timestamp_us,
        });
        Ok(handle)
    }

    fn elf_input_bytes(&mut self, handle: i64) -> Result<Vec<u8>, String> {
        let entry = self
            .store
            .borrow()
            .entry(handle)
            .cloned()
            .ok_or_else(|| format!("store handle {handle}"))?;
        match entry.schema.as_str() {
            "Blob" | "String" => Ok(entry.bytes),
            "Tree" => {
                let forced = self.force_tree_handle(handle)?;
                let TreeEntry::Concrete(tree) = self.store.borrow().tree_entry(forced)? else {
                    return Err("elf input tree stayed pending".into());
                };
                let count = tree.entries.len() + tree.blobs.len();
                if count != 1 {
                    return Err(format!(
                        "elf input tree must contain exactly one blob, got {count}"
                    ));
                }
                let (path, contents) = tree
                    .entries
                    .into_iter()
                    .map(|(path, contents)| (path, contents.into_bytes()))
                    .chain(tree.blobs)
                    .next()
                    .expect("one tree entry");
                if contents.is_empty() {
                    return Err(format!("elf input blob `{path}` is empty"));
                }
                Ok(contents)
            }
            other => Err(format!(
                "elf input must be Blob, String, or Tree, got {other}"
            )),
        }
    }

    fn force_ast_projection(
        &mut self,
        projection: super::ast_probe::Projection,
        args: &[i64],
    ) -> Result<i64, String> {
        let input = *args.first().ok_or_else(|| {
            format!(
                "ast projection {} expected an input argument",
                projection.name()
            )
        })?;
        let name = match projection {
            super::ast_probe::Projection::Items | super::ast_probe::Projection::Fns => {
                if args.len() != 1 {
                    return Err(format!(
                        "ast projection {} expected one input, got {}",
                        projection.name(),
                        args.len()
                    ));
                }
                String::new()
            }
            super::ast_probe::Projection::Fn | super::ast_probe::Projection::FnBodyChildren => {
                let [_, name] = args else {
                    return Err(format!(
                        "ast projection {} expected input and name, got {} args",
                        projection.name(),
                        args.len()
                    ));
                };
                self.store.borrow().string_value(*name, "String")?
            }
        };
        let (source, input_hash) = self.ast_input_source(input)?;
        let memo_key = (input_hash, projection, name.clone());
        if let Some(&handle) = self.ast_projection_memo.get(&memo_key) {
            let timestamp_us = self.next_timestamp();
            self.emit(DriveEvent::ArtifactProbe {
                format: "ast".to_string(),
                projection: projection.name().to_string(),
                input: hash_u64((input_hash, &name)),
                cache_hit: true,
                timestamp_us,
            });
            return Ok(handle);
        }

        let file = if let Some(file) = self.ast_parse_memo.get(&input_hash) {
            Arc::clone(file)
        } else {
            let file = Arc::new(super::ast_probe::parse(&source)?);
            self.ast_parse_memo.insert(input_hash, Arc::clone(&file));
            file
        };

        let handle = match projection {
            super::ast_probe::Projection::Items => alloc_doc_from_value(
                &self.store,
                &self.descriptors,
                &self.schemas,
                &self.schemas,
                super::ast_probe::items(&file),
            )?,
            super::ast_probe::Projection::Fns => alloc_doc_from_value(
                &self.store,
                &self.descriptors,
                &self.schemas,
                &self.schemas,
                super::ast_probe::fns(&file),
            )?,
            super::ast_probe::Projection::Fn => {
                let item = super::ast_probe::fn_item(&file, &name)?;
                alloc_ast_fn_doc(
                    &self.store,
                    &self.descriptors,
                    &self.schemas,
                    &self.schemas,
                    input,
                    input_hash,
                    item,
                )?
            }
            super::ast_probe::Projection::FnBodyChildren => {
                let item = super::ast_probe::fn_item(&file, &name)?;
                alloc_doc_from_value(
                    &self.store,
                    &self.descriptors,
                    &self.schemas,
                    &self.schemas,
                    super::ast_probe::fn_body_children(item),
                )?
            }
        };
        self.ast_projection_memo.insert(memo_key, handle);
        let timestamp_us = self.next_timestamp();
        self.emit(DriveEvent::ArtifactProbe {
            format: "ast".to_string(),
            projection: projection.name().to_string(),
            input: hash_u64((input_hash, &name)),
            cache_hit: false,
            timestamp_us,
        });
        Ok(handle)
    }

    fn force_oci_projection(
        &mut self,
        projection: super::oci::Projection,
        args: &[i64],
    ) -> Result<i64, String> {
        let [input] = args else {
            return Err(format!(
                "OCI projection {} expected one input, got {}",
                projection.name(),
                args.len()
            ));
        };
        let tree = self.oci_input_tree(*input)?;
        let input_hash = ContentHash::from(super::oci::input_hash(&tree));
        if let Some(&handle) = self.oci_projection_memo.get(&(input_hash, projection)) {
            let timestamp_us = self.next_timestamp();
            self.emit(DriveEvent::ArtifactProbe {
                format: "oci".to_string(),
                projection: projection.name().to_string(),
                input: hash_u64(input_hash),
                cache_hit: true,
                timestamp_us,
            });
            return Ok(handle);
        }
        let handle = if projection == super::oci::Projection::Files {
            alloc_oci_files_doc(
                &self.store,
                &self.descriptors,
                &self.schemas,
                input_hash,
                *input,
            )?
        } else {
            let layout = super::oci::parse_layout(tree)?;
            let value = super::oci::project(&layout, projection)?;
            alloc_doc_from_value(
                &self.store,
                &self.descriptors,
                &self.schemas,
                &self.schemas,
                value,
            )?
        };
        self.oci_projection_memo
            .insert((input_hash, projection), handle);
        let timestamp_us = self.next_timestamp();
        self.emit(DriveEvent::ArtifactProbe {
            format: "oci".to_string(),
            projection: projection.name().to_string(),
            input: hash_u64(input_hash),
            cache_hit: false,
            timestamp_us,
        });
        Ok(handle)
    }

    fn ast_input_source(&mut self, handle: i64) -> Result<(String, ContentHash), String> {
        let entry = self
            .store
            .borrow()
            .entry(handle)
            .cloned()
            .ok_or_else(|| format!("store handle {handle}"))?;
        let source = match entry.schema.as_str() {
            "String" => String::from_utf8(entry.bytes).map_err(|err| err.to_string())?,
            "Tree" => {
                let forced = self.force_tree_handle(handle)?;
                let TreeEntry::Concrete(tree) = self.store.borrow().tree_entry(forced)? else {
                    return Err("ast input tree stayed pending".into());
                };
                let len = tree.entries.len();
                let [(path, contents)] =
                    <[(String, String); 1]>::try_from(tree.entries.into_iter().collect::<Vec<_>>())
                        .map_err(|_| {
                            format!("ast input tree must contain exactly one source, got {len}")
                        })?;
                if contents.is_empty() {
                    return Err(format!("ast input source `{path}` is empty"));
                }
                contents
            }
            other => return Err(format!("ast input must be String or Tree, got {other}")),
        };
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"vix-ast-input");
        hasher.update(source.as_bytes());
        Ok((source, finish_hash(hasher)))
    }

    fn oci_input_tree(&mut self, handle: i64) -> Result<crate::exec::Tree, String> {
        let store = self.store.borrow();
        let entry = store
            .entry(handle)
            .ok_or_else(|| format!("store handle {handle}"))?;
        if self.schemas.is_external(&entry.schema, "Tree") {
            drop(store);
            {
                let forced = self.force_tree_handle(handle)?;
                let TreeEntry::Concrete(tree) = self.store.borrow().tree_entry(forced)? else {
                    return Err("OCI input tree stayed pending".into());
                };
                Ok(tree)
            }
        } else {
            oci_tree_from_entry(&self.schemas, entry)
        }
    }
}

enum Burst {
    Done(i64),
    Pending(Box<BurstPending>),
    Error(String),
}

struct BurstPending {
    new_requests: Vec<InvokeRequest>,
    project_requests: Vec<ProjectRequest>,
    text_project_requests: Vec<TextProjectRequest>,
    exec_requests: Vec<ExecRequest>,
    fetch_requests: Vec<FetchRequest>,
    doc_parse_requests: Vec<DocParseRequest>,
    crate_archive_requests: Vec<CrateArchiveRequest>,
    option_unwraps: Vec<OptionUnwrapRequest>,
    pending_coercions: Vec<PendingCoerceRequest>,
    pending_invokes: Vec<PendingInvokeRequest>,
    parked_input: usize,
}

enum PendingForce {
    Invoke(InvokeRequest),
    Ready { input_slot: usize, value: i64 },
}

fn fill_execution_input(exec: &mut Execution, input_slot: usize, value: i64) {
    if exec.ready.len() <= input_slot {
        exec.ready.resize(input_slot + 1, false);
        exec.awaited.resize(input_slot + 1, 0);
    }
    exec.ready[input_slot] = true;
    exec.awaited[input_slot] = value;
}

fn hash_u64(value: impl fmt::Debug) -> u64 {
    hash_u64_debug(value)
}

#[derive(Clone, Debug)]
enum DocPayload {
    Null,
    Bool(i64),
    Int(i64),
    Float(i64),
    String(i64),
    Blob(i64),
    Array(i64),
    Map(i64),
    Virtual(i64),
}

fn alloc_doc_from_value(
    store: &RefCell<ValueStore>,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
    value: Value,
) -> Result<i64, String> {
    match value {
        Value::Bool(value) => {
            alloc_doc_variant(store, descriptors, schemas, 1, &[i64::from(value)])
        }
        Value::Int(value) => alloc_doc_variant(store, descriptors, schemas, 2, &[value]),
        Value::Float(value) => alloc_doc_variant(
            store,
            descriptors,
            schemas,
            3,
            &[super::value::TotalF64::new(value).get().to_bits() as i64],
        ),
        Value::Str(value) => {
            let handle = store
                .borrow_mut()
                .alloc_raw("String", value.into_bytes(), schemas)
                .0;
            alloc_doc_variant(store, descriptors, schemas, 4, &[handle])
        }
        Value::Blob(value) => {
            let handle = store.borrow_mut().alloc_raw("Blob", value, schemas).0;
            alloc_doc_variant(store, descriptors, schemas, 8, &[handle])
        }
        Value::Array(values) => {
            let words = values
                .into_iter()
                .map(|value| {
                    alloc_doc_from_value(store, descriptors, schemas, schema_tables, value)
                })
                .collect::<Result<Vec<_>, _>>()?;
            let handle = store
                .borrow_mut()
                .alloc_array_words("Doc", words, schemas, schema_tables)?
                .0;
            alloc_doc_variant(store, descriptors, schemas, 5, &[handle])
        }
        Value::Map(entries) => {
            let mut pairs = Vec::new();
            for (key, value) in entries {
                let Value::Str(key) = key else {
                    return Err(format!("document object key must be a string, got {key:?}"));
                };
                let key_word = store
                    .borrow_mut()
                    .alloc_raw("String", key.into_bytes(), schemas)
                    .0;
                let value_word =
                    alloc_doc_from_value(store, descriptors, schemas, schema_tables, value)?;
                pairs.push(MapPair {
                    key_schema: "String".to_string(),
                    key_word,
                    value_schema: "Doc".to_string(),
                    value_word,
                    value_realization: None,
                });
            }
            let handle = store
                .borrow_mut()
                .alloc_map(
                    "Map<String,Doc>",
                    pairs,
                    schema_tables,
                    descriptors,
                    schemas,
                )?
                .0;
            alloc_doc_variant(store, descriptors, schemas, 6, &[handle])
        }
        Value::Variant {
            enum_name,
            name,
            index,
            ..
        } if enum_name == "Option" && name == "None" && index == 1 => {
            alloc_doc_variant(store, descriptors, schemas, 0, &[])
        }
        other => Err(format!("document value {other:?} is outside the B5 subset")),
    }
}

fn alloc_typed_from_value(
    store: &RefCell<ValueStore>,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
    schema: &str,
    value: Value,
) -> Result<i64, String> {
    if schemas.is_named_schema(schema, "Doc") {
        return alloc_doc_from_value(store, descriptors, schemas, schema_tables, value);
    }
    if schemas.is_primitive(schema, Primitive::Bool) {
        let Value::Bool(value) = value else {
            return Err(format!("expected Bool, got {}", value.short()));
        };
        return Ok(i64::from(value));
    }
    if schemas.is_primitive(schema, Primitive::I64) {
        let Value::Int(value) = value else {
            return Err(format!("expected Int, got {}", value.short()));
        };
        return Ok(value);
    }
    if schemas.is_primitive(schema, Primitive::F64) {
        return match value {
            Value::Float(value) => Ok(super::value::TotalF64::new(value).get().to_bits() as i64),
            Value::Int(value) => {
                Ok(super::value::TotalF64::new(value as f64).get().to_bits() as i64)
            }
            other => Err(format!("expected Float, got {}", other.short())),
        };
    }
    if schemas.is_primitive(schema, Primitive::String) {
        let Value::Str(value) = value else {
            return Err(format!("expected String, got {}", value.short()));
        };
        return Ok(store
            .borrow_mut()
            .alloc_raw("String", value.into_bytes(), schemas)
            .0);
    }
    if schemas.is_primitive(schema, Primitive::Bytes) {
        let Value::Blob(value) = value else {
            return Err(format!("expected Blob, got {}", value.short()));
        };
        return Ok(store.borrow_mut().alloc_raw("Blob", value, schemas).0);
    }
    if let Some(elem_schema) = array_element_schema(schema) {
        let Value::Array(values) = value else {
            return Err(format!("expected {schema}, got {}", value.short()));
        };
        let words = values
            .into_iter()
            .map(|value| {
                alloc_typed_from_value(
                    store,
                    descriptors,
                    schemas,
                    schema_tables,
                    elem_schema,
                    value,
                )
            })
            .collect::<Result<Vec<_>, _>>()?;
        return Ok(store
            .borrow_mut()
            .alloc_array_words(elem_schema, words, schemas, schema_tables)?
            .0);
    }
    if let Some((key_schema, value_schema)) = schemas.map_schema_names(schema) {
        if !schemas.is_primitive(&key_schema, Primitive::String) {
            return Err(format!(
                "typed JSON map keys must be String, got {key_schema}"
            ));
        }
        let Value::Map(entries) = value else {
            return Err(format!("expected {schema}, got {}", value.short()));
        };
        let mut pairs = Vec::with_capacity(entries.len());
        for (key, value) in entries {
            let Value::Str(key) = key else {
                return Err(format!("typed JSON object key must be string, got {key:?}"));
            };
            let key_word = store
                .borrow_mut()
                .alloc_raw("String", key.into_bytes(), schemas)
                .0;
            let value_word = alloc_typed_from_value(
                store,
                descriptors,
                schemas,
                schema_tables,
                &value_schema,
                value,
            )?;
            pairs.push(MapPair {
                key_schema: key_schema.clone(),
                key_word,
                value_schema: value_schema.clone(),
                value_word,
                value_realization: None,
            });
        }
        return Ok(store
            .borrow_mut()
            .alloc_map(schema, pairs, schema_tables, descriptors, schemas)?
            .0);
    }
    if let Some(Kind::Struct { fields, .. }) = schemas.kind_for_name(schema).cloned() {
        let Value::Map(mut entries) = value else {
            return Err(format!("expected struct {schema}, got {}", value.short()));
        };
        let descriptor = descriptors
            .get(schema)
            .ok_or_else(|| format!("descriptor for schema `{schema}`"))?;
        let mut bytes = vec![0u8; descriptor.layout.size];
        for (field_index, field) in fields.iter().enumerate() {
            let field_value = entries
                .remove(&Value::Str(field.name.clone()))
                .ok_or_else(|| format!("missing field `{}` for {schema}", field.name))?;
            let field_schema = schemas.display_ref(&field.schema);
            let word = alloc_typed_from_value(
                store,
                descriptors,
                schemas,
                schema_tables,
                &field_schema,
                field_value,
            )?;
            let field_offset = field_offset(descriptor, &bytes, field_index);
            let field_descriptor = field_descriptor(descriptor, &bytes, field_index);
            if field_descriptor.layout.size > 8 {
                return Err(format!(
                    "typed JSON field `{}` for {schema} has {} bytes",
                    field.name, field_descriptor.layout.size
                ));
            }
            let word = canonicalize_word_for_schema(schemas, &field_schema, word);
            bytes[field_offset..field_offset + field_descriptor.layout.size]
                .copy_from_slice(&word.to_le_bytes()[..field_descriptor.layout.size]);
        }
        if !entries.is_empty() {
            let keys = entries
                .keys()
                .map(|key| match key {
                    Value::Str(key) => key.clone(),
                    other => format!("{other:?}"),
                })
                .collect::<Vec<_>>();
            return Err(format!("unknown field(s) for {schema}: {}", keys.join(",")));
        }
        return Ok(store
            .borrow_mut()
            .alloc(schema, bytes, descriptors, schemas)
            .0);
    }
    Err(format!("typed JSON cannot materialize schema {schema}"))
}

fn document_input_text_for_error(value: &Value) -> String {
    match value {
        Value::Str(text) => text.clone(),
        Value::Tree(_) => "<tree document input>".to_string(),
        other => other.short(),
    }
}

fn alloc_doc_variant(
    store: &RefCell<ValueStore>,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    variant_index: u64,
    fields: &[i64],
) -> Result<i64, String> {
    let descriptor = descriptors
        .get("Doc")
        .ok_or_else(|| "missing Doc descriptor".to_string())?;
    let mut bytes = vec![0u8; descriptor.layout.size];
    zero_inactive_enum_payload(
        &mut bytes,
        descriptor,
        usize::try_from(variant_index).expect("variant index fits usize"),
    );
    write_variant_tag(&mut bytes, descriptor, variant_index);
    for (index, value) in fields.iter().enumerate() {
        let offset = field_offset(descriptor, &bytes, index);
        write_canonical_word_field(&mut bytes, offset, 8, *value);
    }
    Ok(store
        .borrow_mut()
        .alloc("Doc", bytes, descriptors, schemas)
        .0)
}

fn doc_payload(
    store: &ValueStore,
    descriptors: &DescriptorMap,
    handle: i64,
) -> Result<DocPayload, String> {
    let entry = store
        .entry(handle)
        .ok_or_else(|| format!("store handle {handle}"))?;
    if entry.schema != "Doc" {
        return Err(format!("handle {handle} is `{}`, not Doc", entry.schema));
    }
    let descriptor = descriptors
        .get("Doc")
        .ok_or_else(|| "missing Doc descriptor".to_string())?;
    Ok(match read_variant_tag(&entry.bytes, descriptor) {
        0 => DocPayload::Null,
        1 => DocPayload::Bool(read_frame_word(
            &entry.bytes,
            field_offset(descriptor, &entry.bytes, 0),
        )),
        2 => DocPayload::Int(read_frame_word(
            &entry.bytes,
            field_offset(descriptor, &entry.bytes, 0),
        )),
        3 => DocPayload::Float(read_frame_word(
            &entry.bytes,
            field_offset(descriptor, &entry.bytes, 0),
        )),
        4 => DocPayload::String(read_frame_word(
            &entry.bytes,
            field_offset(descriptor, &entry.bytes, 0),
        )),
        8 => DocPayload::Blob(read_frame_word(
            &entry.bytes,
            field_offset(descriptor, &entry.bytes, 0),
        )),
        5 => DocPayload::Array(read_frame_word(
            &entry.bytes,
            field_offset(descriptor, &entry.bytes, 0),
        )),
        6 => DocPayload::Map(read_frame_word(
            &entry.bytes,
            field_offset(descriptor, &entry.bytes, 0),
        )),
        7 => DocPayload::Virtual(read_frame_word(
            &entry.bytes,
            field_offset(descriptor, &entry.bytes, 0),
        )),
        other => return Err(format!("unknown Doc tag {other}")),
    })
}

fn doc_get(
    store: &RefCell<ValueStore>,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
    doc: i64,
    key: &str,
) -> Result<(i64, bool), String> {
    let payload = {
        let store_ref = store.borrow();
        doc_payload(&store_ref, descriptors, doc)?
    };
    let map = match payload {
        DocPayload::Map(handle) => handle,
        _ => {
            let none = store
                .borrow_mut()
                .alloc_option_none("Realized<Doc>", schema_tables)?
                .0;
            return Ok((none, false));
        }
    };
    let key_word = store
        .borrow_mut()
        .alloc_raw("String", key.as_bytes().to_vec(), schemas)
        .0;
    store.borrow_mut().map_get(
        map,
        "String",
        key_word,
        "Realized<Doc>",
        schemas,
        schema_tables,
    )
}

struct VirtualDocGetCtx<'a> {
    store: &'a RefCell<ValueStore>,
    descriptors: &'a DescriptorMap,
    schemas: &'a SchemaTables,
    schema_tables: &'a SchemaTables,
    oci_file_memo: &'a RefCell<&'a mut IdentityHashMap<(ContentHash, String), Option<i64>>>,
    store_events: &'a RefCell<Vec<DriveEvent>>,
    clock_cell: &'a RefCell<&'a mut u64>,
}

fn doc_get_with_virtual(
    ctx: &VirtualDocGetCtx<'_>,
    doc: i64,
    key: &str,
) -> Result<(i64, bool), String> {
    let virtual_input = {
        let store = ctx.store.borrow();
        oci_virtual_files_input(&store, ctx.descriptors, doc)?
    };
    if let Some(input) = virtual_input {
        return oci_virtual_file_get(ctx, input, key);
    }
    doc_get(
        ctx.store,
        ctx.descriptors,
        ctx.schemas,
        ctx.schema_tables,
        doc,
        key,
    )
}

fn doc_package(
    store: &RefCell<ValueStore>,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
    lock_doc: i64,
    wanted_name: &str,
) -> Result<i64, String> {
    let (package_option, _) = doc_get(
        store,
        descriptors,
        schemas,
        schema_tables,
        lock_doc,
        "package",
    )?;
    let packages = match store
        .borrow()
        .option_payload(package_option, schemas, schema_tables)?
    {
        OptionPayload::Some { word, .. } => {
            match doc_payload(&store.borrow(), descriptors, word)? {
                DocPayload::Array(array) => array,
                other => {
                    return Err(format!(
                        "Cargo.lock `package` expected array, got {other:?}"
                    ));
                }
            }
        }
        OptionPayload::None => {
            return Ok(store
                .borrow_mut()
                .alloc_option_none("Realized<Doc>", schema_tables)?
                .0);
        }
    };
    let package_words = match store.borrow().array_entry(packages, schema_tables)? {
        ArrayEntry::Words { elem_schema, words }
            if schemas.is_named_schema(&elem_schema, "Doc") =>
        {
            words
        }
        ArrayEntry::Words { elem_schema, .. } => {
            return Err(format!("Cargo.lock `package` array contains {elem_schema}"));
        }
        ArrayEntry::Pending { .. } => return Err("Cargo.lock `package` array is pending".into()),
    };
    for package in package_words {
        let (name_option, _) =
            doc_get(store, descriptors, schemas, schema_tables, package, "name")?;
        let name = match store
            .borrow()
            .option_payload(name_option, schemas, schema_tables)?
        {
            OptionPayload::Some { word, .. } => {
                match doc_payload(&store.borrow(), descriptors, word)? {
                    DocPayload::String(handle) => store.borrow().string_value(handle, "String")?,
                    other => {
                        return Err(format!(
                            "Cargo.lock package `name` expected string, got {other:?}"
                        ));
                    }
                }
            }
            OptionPayload::None => continue,
        };
        if name == wanted_name {
            return Ok(store
                .borrow_mut()
                .alloc_option_some(
                    "Realized<Doc>",
                    package,
                    Some(Realization::Ready),
                    schemas,
                    schema_tables,
                )?
                .0);
        }
    }
    Ok(store
        .borrow_mut()
        .alloc_option_none("Realized<Doc>", schema_tables)?
        .0)
}

fn oci_virtual_file_get(
    ctx: &VirtualDocGetCtx<'_>,
    input: i64,
    path: &str,
) -> Result<(i64, bool), String> {
    let tree = {
        let store = ctx.store.borrow();
        oci_tree_from_store(&store, ctx.schemas, input)?
    };
    let input_hash = ContentHash::from(super::oci::input_hash(&tree));
    let key = (input_hash, path.to_string());
    if let Some(cached) = ctx.oci_file_memo.borrow().get(&key).cloned() {
        emit_artifact_probe(
            ctx.store_events,
            ctx.clock_cell,
            "oci",
            &format!("files/{path}"),
            input_hash,
            true,
        );
        return match cached {
            Some(handle) => ctx.store.borrow_mut().alloc_option_some(
                "Realized<Doc>",
                handle,
                Some(Realization::Ready),
                ctx.schemas,
                ctx.schema_tables,
            ),
            None => ctx
                .store
                .borrow_mut()
                .alloc_option_none("Realized<Doc>", ctx.schema_tables),
        };
    }
    let layout = super::oci::parse_layout(tree)?;
    let projected = super::oci::project_file(&layout, path)?;
    let handle = projected
        .map(|file| {
            alloc_doc_from_value(
                ctx.store,
                ctx.descriptors,
                ctx.schemas,
                ctx.schema_tables,
                Value::Map(BTreeMap::from([
                    (Value::Str("path".to_string()), Value::Str(path.to_string())),
                    (
                        Value::Str("contents".to_string()),
                        match file.contents {
                            super::oci::FileContents::Text(contents) => Value::Str(contents),
                            super::oci::FileContents::Blob(contents) => Value::Blob(contents),
                        },
                    ),
                    (
                        Value::Str("layer_digest".to_string()),
                        Value::Str(file.layer_digest),
                    ),
                    (Value::Str("size".to_string()), Value::Int(file.size)),
                ])),
            )
        })
        .transpose()?;
    ctx.oci_file_memo.borrow_mut().insert(key, handle);
    emit_artifact_probe(
        ctx.store_events,
        ctx.clock_cell,
        "oci",
        &format!("files/{path}"),
        input_hash,
        false,
    );
    match handle {
        Some(handle) => ctx.store.borrow_mut().alloc_option_some(
            "Realized<Doc>",
            handle,
            Some(Realization::Ready),
            ctx.schemas,
            ctx.schema_tables,
        ),
        None => ctx
            .store
            .borrow_mut()
            .alloc_option_none("Realized<Doc>", ctx.schema_tables),
    }
}

fn emit_artifact_probe(
    store_events: &RefCell<Vec<DriveEvent>>,
    clock_cell: &RefCell<&mut u64>,
    format: &str,
    projection: &str,
    input_hash: ContentHash,
    cache_hit: bool,
) {
    let timestamp_us = {
        let mut clock = clock_cell.borrow_mut();
        let timestamp_us = **clock;
        **clock = timestamp_us.saturating_add(1);
        timestamp_us
    };
    store_events.borrow_mut().push(DriveEvent::ArtifactProbe {
        format: format.to_string(),
        projection: projection.to_string(),
        input: hash_u64(input_hash),
        cache_hit,
        timestamp_us,
    });
}

fn doc_coerce(
    store: &RefCell<ValueStore>,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
    doc: i64,
    schema: &str,
) -> Result<i64, String> {
    if schemas.is_named_schema(schema, "Doc") {
        return Ok(doc);
    }
    let payload = doc_payload(&store.borrow(), descriptors, doc)?;
    match payload {
        DocPayload::Bool(value) if schemas.is_primitive(schema, Primitive::Bool) => Ok(value),
        DocPayload::Int(value) if schemas.is_primitive(schema, Primitive::I64) => Ok(value),
        DocPayload::Float(value) if schemas.is_primitive(schema, Primitive::F64) => Ok(value),
        DocPayload::String(value) if schemas.is_primitive(schema, Primitive::String) => Ok(value),
        DocPayload::Blob(value) if schemas.is_primitive(schema, Primitive::Bytes) => Ok(value),
        DocPayload::Array(value) if schemas.is_list(schema) => {
            coerce_doc_array(store, descriptors, schemas, schema_tables, value, schema)
        }
        DocPayload::Map(value)
            if schemas
                .map_schema_names(schema)
                .is_some_and(|(key_schema, value_schema)| {
                    schemas.is_primitive(&key_schema, Primitive::String)
                        && schemas.is_named_schema(&value_schema, "Doc")
                }) =>
        {
            Ok(value)
        }
        DocPayload::Virtual(_) => Err(format!("cannot coerce Doc::Virtual to {schema}")),
        payload => Err(format!("cannot coerce Doc::{payload:?} to {schema}")),
    }
}

fn coerce_doc_array(
    store: &RefCell<ValueStore>,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
    array: i64,
    target_schema: &str,
) -> Result<i64, String> {
    let target_elem_schema = array_element_schema(target_schema)
        .ok_or_else(|| format!("{target_schema} is not an Array<T>"))?;
    if schemas.is_named_schema(target_elem_schema, "Doc") {
        return Ok(array);
    }
    let words = {
        let store = store.borrow();
        match store.array_entry(array, schema_tables)? {
            ArrayEntry::Words { elem_schema, words } => {
                if !schemas.is_named_schema(&elem_schema, "Doc") {
                    return Err(format!(
                        "cannot coerce Array<{elem_schema}> inside Doc::Array to {target_schema}"
                    ));
                }
                words
            }
            ArrayEntry::Pending { .. } => {
                return Err(format!(
                    "cannot coerce pending Doc::Array to {target_schema}"
                ));
            }
        }
    };
    let coerced = words
        .into_iter()
        .map(|word| {
            doc_coerce(
                store,
                descriptors,
                schemas,
                schema_tables,
                word,
                target_elem_schema,
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(store
        .borrow_mut()
        .alloc_array_words(target_elem_schema, coerced, schemas, schema_tables)?
        .0)
}

fn alloc_elf_doc(
    store: &RefCell<ValueStore>,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
    input: i64,
) -> Result<i64, String> {
    let input_hash = {
        let store = store.borrow();
        let entry = store
            .entry(input)
            .ok_or_else(|| format!("store handle {input}"))?;
        if schemas.is_primitive(&entry.schema, Primitive::Bytes)
            || schemas.is_primitive(&entry.schema, Primitive::String)
        {
            entry.content_hash
        } else if schemas.is_external(&entry.schema, "Tree") {
            let TreeEntry::Concrete(_) = store.tree_entry(input)? else {
                return Err("elf input tree must be concrete at probe creation".into());
            };
            entry.content_hash
        } else {
            return Err(format!(
                "elf input must be Blob, String, or Tree, got {}",
                entry.schema
            ));
        }
    };
    let mut pairs = Vec::new();
    for projection in super::elf::Projection::ALL {
        let key = store
            .borrow_mut()
            .alloc_raw("String", projection.name().as_bytes().to_vec(), schemas)
            .0;
        let pending = elf_projection_pending(input, input_hash, projection);
        let pending = store.borrow_mut().alloc_pending("Doc", pending).0;
        pairs.push(MapPair {
            key_schema: "String".to_string(),
            key_word: key,
            value_schema: pending_schema("Doc"),
            value_word: pending,
            value_realization: None,
        });
    }
    let map = store
        .borrow_mut()
        .alloc_map(
            "Map<String,Doc>",
            pairs,
            schema_tables,
            descriptors,
            schemas,
        )?
        .0;
    alloc_doc_variant(store, descriptors, schemas, 6, &[map])
}

fn alloc_oci_doc(
    store: &RefCell<ValueStore>,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
    input: i64,
) -> Result<i64, String> {
    let input_hash = {
        let store = store.borrow();
        let tree = oci_tree_from_store(&store, schemas, input)?;
        ContentHash::from(super::oci::input_hash(&tree))
    };
    let mut pairs = Vec::new();
    for projection in super::oci::Projection::ALL {
        let key = store
            .borrow_mut()
            .alloc_raw("String", projection.name().as_bytes().to_vec(), schemas)
            .0;
        let pending = oci_projection_pending(input, input_hash, projection);
        let pending = store.borrow_mut().alloc_pending("Doc", pending).0;
        pairs.push(MapPair {
            key_schema: "String".to_string(),
            key_word: key,
            value_schema: pending_schema("Doc"),
            value_word: pending,
            value_realization: None,
        });
    }
    let map = store
        .borrow_mut()
        .alloc_map(
            "Map<String,Doc>",
            pairs,
            schema_tables,
            descriptors,
            schemas,
        )?
        .0;
    alloc_doc_variant(store, descriptors, schemas, 6, &[map])
}

fn alloc_oci_files_doc(
    store: &RefCell<ValueStore>,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    input_hash: ContentHash,
    input: i64,
) -> Result<i64, String> {
    let marker = format!("oci-files:{}:{input}", hex_content_hash(&input_hash));
    let marker = store
        .borrow_mut()
        .alloc_raw("String", marker.into_bytes(), schemas)
        .0;
    alloc_doc_variant(store, descriptors, schemas, 7, &[marker])
}

fn elf_projection_pending(
    input: i64,
    input_hash: ContentHash,
    projection: super::elf::Projection,
) -> PendingInvocation {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vix-elf-projection");
    hasher.update(projection.name().as_bytes());
    hasher.update(input_hash.as_ref());
    let identity_hash = finish_hash(hasher);
    PendingInvocation {
        closure_hash: hash_u64(("elf", projection.name())),
        primitive: Some(PendingPrimitive {
            kind: PendingPrimitiveKind::Elf(projection),
        }),
        args: vec![input],
        remaining_arity: 0,
        identity_hash,
    }
}

fn alloc_ast_doc(
    store: &RefCell<ValueStore>,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
    input: i64,
) -> Result<i64, String> {
    let input_hash = {
        let store = store.borrow();
        let entry = store
            .entry(input)
            .ok_or_else(|| format!("store handle {input}"))?;
        match entry.schema.as_str() {
            "String" | "Tree" => entry.content_hash,
            other => return Err(format!("ast input must be String or Tree, got {other}")),
        }
    };
    let rows = [
        (
            "items",
            ast_projection_pending(
                input,
                input_hash,
                super::ast_probe::Projection::Items,
                Vec::new(),
            ),
        ),
        (
            "fns",
            ast_projection_pending(
                input,
                input_hash,
                super::ast_probe::Projection::Fns,
                Vec::new(),
            ),
        ),
    ]
    .into_iter()
    .map(|(key, pending)| {
        let pending = store.borrow_mut().alloc_pending("Doc", pending).0;
        (key.to_string(), pending_schema("Doc"), pending)
    })
    .collect::<Vec<_>>();
    alloc_doc_object(store, descriptors, schemas, schema_tables, rows)
}

fn alloc_ast_fn_doc(
    store: &RefCell<ValueStore>,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
    input: i64,
    input_hash: ContentHash,
    item: &ast::FnItem,
) -> Result<i64, String> {
    let name_handle = store
        .borrow_mut()
        .alloc_raw("String", item.name.value.as_bytes().to_vec(), schemas)
        .0;
    let children = ast_projection_pending(
        input,
        input_hash,
        super::ast_probe::Projection::FnBodyChildren,
        vec![name_handle],
    );
    let children = store.borrow_mut().alloc_pending("Doc", children).0;
    let body = alloc_doc_object(
        store,
        descriptors,
        schemas,
        schema_tables,
        vec![
            (
                "span".to_string(),
                "Doc".to_string(),
                alloc_doc_from_value(
                    store,
                    descriptors,
                    schemas,
                    schema_tables,
                    super::ast_probe::span_value(item.body.span),
                )?,
            ),
            ("children".to_string(), pending_schema("Doc"), children),
        ],
    )?;
    let mut rows = super::ast_probe::fn_fields(item)
        .into_iter()
        .map(|(key, value)| {
            let Value::Str(key) = key else {
                return Err(format!("ast fn key must be string, got {key:?}"));
            };
            let handle = alloc_doc_from_value(store, descriptors, schemas, schema_tables, value)?;
            Ok((key, "Doc".to_string(), handle))
        })
        .collect::<Result<Vec<_>, String>>()?;
    rows.push(("body".to_string(), "Doc".to_string(), body));
    alloc_doc_object(store, descriptors, schemas, schema_tables, rows)
}

fn ast_projection_pending(
    input: i64,
    input_hash: ContentHash,
    projection: super::ast_probe::Projection,
    extra_args: Vec<i64>,
) -> PendingInvocation {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vix-ast-projection");
    hasher.update(projection.name().as_bytes());
    hasher.update(input_hash.as_ref());
    for arg in &extra_args {
        hasher.update(&arg.to_le_bytes());
    }
    let mut args = Vec::with_capacity(1 + extra_args.len());
    args.push(input);
    args.extend(extra_args);
    PendingInvocation {
        closure_hash: hash_u64(("ast", projection.name())),
        primitive: Some(PendingPrimitive {
            kind: PendingPrimitiveKind::Ast(projection),
        }),
        args,
        remaining_arity: 0,
        identity_hash: finish_hash(hasher),
    }
}

fn alloc_doc_object(
    store: &RefCell<ValueStore>,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
    rows: Vec<(String, String, i64)>,
) -> Result<i64, String> {
    let mut pairs = Vec::with_capacity(rows.len());
    for (key, value_schema, value_word) in rows {
        let key_word = store
            .borrow_mut()
            .alloc_raw("String", key.into_bytes(), schemas)
            .0;
        pairs.push(MapPair {
            key_schema: "String".to_string(),
            key_word,
            value_schema,
            value_word,
            value_realization: None,
        });
    }
    let map = store
        .borrow_mut()
        .alloc_map(
            "Map<String,Doc>",
            pairs,
            schema_tables,
            descriptors,
            schemas,
        )?
        .0;
    alloc_doc_variant(store, descriptors, schemas, 6, &[map])
}

fn oci_projection_pending(
    input: i64,
    input_hash: ContentHash,
    projection: super::oci::Projection,
) -> PendingInvocation {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vix-oci-projection");
    hasher.update(projection.name().as_bytes());
    hasher.update(input_hash.as_ref());
    let identity_hash = finish_hash(hasher);
    PendingInvocation {
        closure_hash: hash_u64(("oci", projection.name())),
        primitive: Some(PendingPrimitive {
            kind: PendingPrimitiveKind::Oci(projection),
        }),
        args: vec![input],
        remaining_arity: 0,
        identity_hash,
    }
}

fn oci_virtual_files_input(
    store: &ValueStore,
    descriptors: &DescriptorMap,
    doc: i64,
) -> Result<Option<i64>, String> {
    let DocPayload::Virtual(marker) = doc_payload(store, descriptors, doc)? else {
        return Ok(None);
    };
    let marker = store.string_value(marker, "String")?;
    let Some(rest) = marker.strip_prefix("oci-files:") else {
        return Ok(None);
    };
    let Some((_, input)) = rest.rsplit_once(':') else {
        return Err(format!("bad OCI files marker `{marker}`"));
    };
    input
        .parse::<i64>()
        .map(Some)
        .map_err(|err| format!("bad OCI files marker `{marker}`: {err}"))
}

fn oci_tree_from_store(
    store: &ValueStore,
    schemas: &SchemaTables,
    input: i64,
) -> Result<crate::exec::Tree, String> {
    let entry = store
        .entry(input)
        .ok_or_else(|| format!("store handle {input}"))?;
    if schemas.is_external(&entry.schema, "Tree") {
        let TreeEntry::Concrete(tree) = store.tree_entry(input)? else {
            return Err("OCI input tree must be concrete".into());
        };
        Ok(tree)
    } else {
        oci_tree_from_entry(schemas, entry)
    }
}

fn oci_tree_from_entry(
    schemas: &SchemaTables,
    entry: &StoreEntry,
) -> Result<crate::exec::Tree, String> {
    if schemas.is_primitive(&entry.schema, Primitive::Bytes)
        || schemas.is_primitive(&entry.schema, Primitive::String)
    {
        super::oci::archive_to_tree(&entry.bytes)
    } else {
        Err(format!(
            "OCI input must be Blob, String, or Tree, got {}",
            entry.schema
        ))
    }
}

fn compare_words(
    store: &ValueStore,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
    schema: &str,
    a: i64,
    b: i64,
) -> Result<Ordering, String> {
    if a == b && !schemas.is_primitive(schema, Primitive::F64) {
        return Ok(Ordering::Equal);
    }
    if schemas.is_primitive(schema, Primitive::I64) || schemas.is_primitive(schema, Primitive::Bool)
    {
        Ok(a.cmp(&b))
    } else if schemas.is_primitive(schema, Primitive::F64) {
        let a = super::value::TotalF64::new(f64::from_bits(canonicalize_word_for_schema(
            schemas, schema, a,
        ) as u64));
        let b = super::value::TotalF64::new(f64::from_bits(canonicalize_word_for_schema(
            schemas, schema, b,
        ) as u64));
        Ok(a.cmp(&b))
    } else if schemas.is_primitive(schema, Primitive::Bytes) {
        let a = store.entry(a).ok_or_else(|| format!("store handle {a}"))?;
        let b = store.entry(b).ok_or_else(|| format!("store handle {b}"))?;
        Ok(a.bytes.cmp(&b.bytes))
    } else if schemas.is_primitive(schema, Primitive::String)
        || schemas.is_external(schema, "Path")
        || schemas.is_external(schema, "Flag")
        || schemas.is_external(schema, "Cc")
        || schemas.is_external(schema, "Ar")
        || schemas.is_external(schema, "Rustc")
    {
        let a = store.string_value(a, schema)?;
        let b = store.string_value(b, schema)?;
        Ok(a.cmp(&b))
    } else if schemas.is_external(schema, "Version") {
        let a = store.entry(a).ok_or_else(|| format!("store handle {a}"))?;
        let b = store.entry(b).ok_or_else(|| format!("store handle {b}"))?;
        if !schemas.is_external(&a.schema, "Version") || !schemas.is_external(&b.schema, "Version")
        {
            return Err(format!(
                "compare expected Version, got {} and {}",
                a.schema, b.schema
            ));
        }
        super::version::cmp_total(&a.bytes, &b.bytes)
    } else if schemas.is_external(schema, "VersionSet") {
        let a = store.entry(a).ok_or_else(|| format!("store handle {a}"))?;
        let b = store.entry(b).ok_or_else(|| format!("store handle {b}"))?;
        if !schemas.is_external(&a.schema, "VersionSet")
            || !schemas.is_external(&b.schema, "VersionSet")
        {
            return Err(format!(
                "compare expected VersionSet, got {} and {}",
                a.schema, b.schema
            ));
        }
        Ok(a.bytes.cmp(&b.bytes))
    } else if schemas.is_list(schema) {
        compare_arrays(store, descriptors, schemas, schema_tables, a, b)
    } else if schemas.is_map(schema) {
        compare_maps(store, descriptors, schemas, schema_tables, a, b)
    } else if schemas.is_named_schema(schema, "Doc") {
        compare_docs(store, descriptors, schemas, schema_tables, a, b)
    } else if schemas.is_external(schema, "Tree") {
        Ok(canonical_word_hash_in_store(store, schemas, schema, a)
            .as_ref()
            .cmp(canonical_word_hash_in_store(store, schemas, schema, b).as_ref()))
    } else {
        compare_declared_value(store, descriptors, schemas, schema_tables, schema, a, b)
    }
}

fn compare_expression_words(
    store: &ValueStore,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
    schema: &str,
    a: i64,
    b: i64,
) -> Result<Ordering, String> {
    if schemas.is_external(schema, "Version") {
        let a = store.entry(a).ok_or_else(|| format!("store handle {a}"))?;
        let b = store.entry(b).ok_or_else(|| format!("store handle {b}"))?;
        if !schemas.is_external(&a.schema, "Version") || !schemas.is_external(&b.schema, "Version")
        {
            return Err(format!(
                "compare expected Version, got {} and {}",
                a.schema, b.schema
            ));
        }
        return super::version::cmp_precedence(&a.bytes, &b.bytes);
    }
    compare_words(store, descriptors, schemas, schema_tables, schema, a, b)
}

fn compare_arrays(
    store: &ValueStore,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
    a: i64,
    b: i64,
) -> Result<Ordering, String> {
    let (a_schema, a_words) = match store.array_entry(a, schema_tables)? {
        ArrayEntry::Words { elem_schema, words } => (elem_schema, words),
        ArrayEntry::Pending { .. } => return Ok(a.cmp(&b)),
    };
    let (b_schema, b_words) = match store.array_entry(b, schema_tables)? {
        ArrayEntry::Words { elem_schema, words } => (elem_schema, words),
        ArrayEntry::Pending { .. } => return Ok(a.cmp(&b)),
    };
    let schema_order = a_schema.cmp(&b_schema);
    if schema_order != Ordering::Equal {
        return Ok(schema_order);
    }
    compare_word_slices(
        store,
        descriptors,
        schemas,
        schema_tables,
        &a_schema,
        &a_words,
        &b_words,
    )
}

fn compare_maps(
    store: &ValueStore,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
    a: i64,
    b: i64,
) -> Result<Ordering, String> {
    let (a_schema, a_pairs) = store.map_pairs(a, schemas, schema_tables)?;
    let (b_schema, b_pairs) = store.map_pairs(b, schemas, schema_tables)?;
    let schema_order = a_schema.cmp(&b_schema);
    if schema_order != Ordering::Equal {
        return Ok(schema_order);
    }
    for (a_pair, b_pair) in a_pairs.iter().zip(&b_pairs) {
        let key_schema_order = a_pair.key_schema.cmp(&b_pair.key_schema);
        if key_schema_order != Ordering::Equal {
            return Ok(key_schema_order);
        }
        let key_order = compare_words(
            store,
            descriptors,
            schemas,
            schema_tables,
            &a_pair.key_schema,
            a_pair.key_word,
            b_pair.key_word,
        )?;
        if key_order != Ordering::Equal {
            return Ok(key_order);
        }
        let value_schema_order = a_pair.value_schema.cmp(&b_pair.value_schema);
        if value_schema_order != Ordering::Equal {
            return Ok(value_schema_order);
        }
        let value_order = compare_words(
            store,
            descriptors,
            schemas,
            schema_tables,
            &a_pair.value_schema,
            a_pair.value_word,
            b_pair.value_word,
        )?;
        if value_order != Ordering::Equal {
            return Ok(value_order);
        }
    }
    Ok(a_pairs.len().cmp(&b_pairs.len()))
}

fn compare_docs(
    store: &ValueStore,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
    a: i64,
    b: i64,
) -> Result<Ordering, String> {
    let a_payload = doc_payload(store, descriptors, a)?;
    let b_payload = doc_payload(store, descriptors, b)?;
    let tag = |payload: &DocPayload| match payload {
        DocPayload::Null => 0,
        DocPayload::Bool(_) => 1,
        DocPayload::Int(_) => 2,
        DocPayload::Float(_) => 3,
        DocPayload::String(_) => 4,
        DocPayload::Array(_) => 5,
        DocPayload::Map(_) => 6,
        DocPayload::Virtual(_) => 7,
        DocPayload::Blob(_) => 8,
    };
    let tag_order = tag(&a_payload).cmp(&tag(&b_payload));
    if tag_order != Ordering::Equal {
        return Ok(tag_order);
    }
    match (a_payload, b_payload) {
        (DocPayload::Null, DocPayload::Null) => Ok(Ordering::Equal),
        (DocPayload::Bool(a), DocPayload::Bool(b)) => Ok(a.cmp(&b)),
        (DocPayload::Int(a), DocPayload::Int(b)) => Ok(a.cmp(&b)),
        (DocPayload::Float(a), DocPayload::Float(b)) => {
            compare_words(store, descriptors, schemas, schema_tables, "Float", a, b)
        }
        (DocPayload::String(a), DocPayload::String(b)) => {
            compare_words(store, descriptors, schemas, schema_tables, "String", a, b)
        }
        (DocPayload::Blob(a), DocPayload::Blob(b)) => {
            compare_words(store, descriptors, schemas, schema_tables, "Blob", a, b)
        }
        (DocPayload::Array(a), DocPayload::Array(b)) => compare_words(
            store,
            descriptors,
            schemas,
            schema_tables,
            "Array<Doc>",
            a,
            b,
        ),
        (DocPayload::Map(a), DocPayload::Map(b)) => compare_words(
            store,
            descriptors,
            schemas,
            schema_tables,
            "Map<String,Doc>",
            a,
            b,
        ),
        (DocPayload::Virtual(a), DocPayload::Virtual(b)) => {
            compare_words(store, descriptors, schemas, schema_tables, "String", a, b)
        }
        _ => Ok(Ordering::Equal),
    }
}

fn compare_declared_value(
    store: &ValueStore,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
    schema: &str,
    a: i64,
    b: i64,
) -> Result<Ordering, String> {
    let a_entry = store.entry(a).ok_or_else(|| format!("store handle {a}"))?;
    let b_entry = store.entry(b).ok_or_else(|| format!("store handle {b}"))?;
    if a_entry.schema != schema || b_entry.schema != schema {
        return Err(format!(
            "compare expected {schema}, got {} and {}",
            a_entry.schema, b_entry.schema
        ));
    }
    let descriptor = descriptors
        .get(schema)
        .ok_or_else(|| format!("descriptor for schema `{schema}`"))?;
    compare_descriptor_bytes(
        store,
        descriptors,
        schemas,
        schema_tables,
        descriptor,
        &a_entry.bytes,
        &b_entry.bytes,
    )
}

fn compare_descriptor_bytes(
    store: &ValueStore,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
    descriptor: &VixDescriptor,
    a: &[u8],
    b: &[u8],
) -> Result<Ordering, String> {
    match &descriptor.access {
        Access::Scalar
            if matches!(
                schemas.kind_for_ref(&descriptor.schema),
                Some(Kind::Primitive(Primitive::F64))
            ) =>
        {
            let schema = schemas.display_ref(&descriptor.schema);
            compare_words(
                store,
                descriptors,
                schemas,
                schema_tables,
                &schema,
                read_frame_word(a, 0),
                read_frame_word(b, 0),
            )
        }
        Access::Scalar => Ok(a.cmp(b)),
        Access::Handle { target } => compare_words(
            store,
            descriptors,
            schemas,
            schema_tables,
            &schemas.display_ref(target),
            read_frame_word(a, 0),
            read_frame_word(b, 0),
        ),
        Access::Record(record) => {
            for field in &record.fields {
                let start = field.offset;
                let end = start + field.descriptor.layout.size;
                let order = compare_descriptor_bytes(
                    store,
                    descriptors,
                    schemas,
                    schema_tables,
                    &field.descriptor,
                    &a[start..end],
                    &b[start..end],
                )?;
                if order != Ordering::Equal {
                    return Ok(order);
                }
            }
            Ok(Ordering::Equal)
        }
        Access::Enum(access) => {
            let a_tag = read_variant_tag(a, descriptor);
            let b_tag = read_variant_tag(b, descriptor);
            let tag_order = a_tag.cmp(&b_tag);
            if tag_order != Ordering::Equal {
                return Ok(tag_order);
            }
            let variant = access
                .variants
                .iter()
                .find(|variant| variant.selector == a_tag)
                .ok_or_else(|| format!("enum selector {a_tag}"))?;
            for field in &variant.payload.fields {
                let start = field.offset;
                let end = start + field.descriptor.layout.size;
                let order = compare_descriptor_bytes(
                    store,
                    descriptors,
                    schemas,
                    schema_tables,
                    &field.descriptor,
                    &a[start..end],
                    &b[start..end],
                )?;
                if order != Ordering::Equal {
                    return Ok(order);
                }
            }
            Ok(Ordering::Equal)
        }
        Access::Array {
            element,
            count,
            stride,
        } => {
            for i in 0..*count {
                let start = i * *stride;
                let end = start + element.layout.size;
                let order = compare_descriptor_bytes(
                    store,
                    descriptors,
                    schemas,
                    schema_tables,
                    element,
                    &a[start..end],
                    &b[start..end],
                )?;
                if order != Ordering::Equal {
                    return Ok(order);
                }
            }
            Ok(Ordering::Equal)
        }
        other => Err(format!("cannot compare descriptor access {other:?}")),
    }
}

fn compare_word_slices(
    store: &ValueStore,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
    schema: &str,
    a: &[i64],
    b: &[i64],
) -> Result<Ordering, String> {
    for (a, b) in a.iter().zip(b) {
        let order = compare_words(store, descriptors, schemas, schema_tables, schema, *a, *b)?;
        if order != Ordering::Equal {
            return Ok(order);
        }
    }
    Ok(a.len().cmp(&b.len()))
}

fn render_word(
    store: &ValueStore,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
    names: &RenderNames,
    schema: &str,
    word: i64,
) -> Result<RenderedValue, String> {
    if pending_value_schema(schema).is_some() {
        return Ok(RenderedValue::Pending {
            pending: render_pending(store, word)?,
        });
    }
    if schemas.is_option(schema) {
        return render_option(store, descriptors, schemas, schema_tables, names, word);
    }
    if !schemas.is_external(schema, "Sealed")
        && let Some(entry) = store.entry(word)
        && let Some(taint) = &entry.taint
    {
        return Err(format!(
            "refusing to render tainted {schema} as plaintext; declassify explicitly first (taint `{}`)",
            taint.marker
        ));
    }
    if schemas.is_primitive(schema, Primitive::I64) {
        Ok(RenderedValue::Int { value: word })
    } else if schemas.is_primitive(schema, Primitive::F64) {
        let bits = canonicalize_word_for_schema(schemas, schema, word) as u64;
        let value = f64::from_bits(bits);
        Ok(RenderedValue::Float {
            bits: format!("{bits:016x}"),
            value: if value.is_nan() {
                "NaN".to_string()
            } else {
                value.to_string()
            },
        })
    } else if schemas.is_primitive(schema, Primitive::Bool) {
        Ok(RenderedValue::Bool { value: word != 0 })
    } else if schemas.is_primitive(schema, Primitive::String) {
        Ok(RenderedValue::String {
            value: store.string_value(word, schema)?,
        })
    } else if schemas.is_external(schema, "Template") {
        Ok(RenderedValue::Raw {
            schema: "Template".to_string(),
            bytes_utf8: Some(store.string_value(word, schema)?),
        })
    } else if schemas.is_external(schema, "Path") {
        Ok(RenderedValue::Path {
            value: store.string_value(word, schema)?,
        })
    } else if schemas.is_external(schema, "Version") {
        Ok(RenderedValue::Version {
            value: store.string_value(word, schema)?,
        })
    } else if schemas.is_external(schema, "VersionSet") {
        let entry = store
            .entry(word)
            .ok_or_else(|| format!("store handle {word}"))?;
        if !schemas.is_external(&entry.schema, "VersionSet") {
            return Err(format!(
                "handle {word} is `{}`, not VersionSet",
                entry.schema
            ));
        }
        Ok(RenderedValue::VersionSet {
            value: super::version_set::VersionSet::parse_bytes(&entry.bytes)?.render(),
        })
    } else if schemas.is_external(schema, "Flag") {
        Ok(RenderedValue::Flag {
            value: store.string_value(word, schema)?,
        })
    } else if schemas.is_external(schema, "Tree") {
        render_tree(store, word)
    } else if schemas.is_external(schema, "Sealed") {
        render_sealed(store, word)
    } else if schemas.is_list(schema) {
        render_array(store, descriptors, schemas, schema_tables, names, word)
    } else if schemas.is_named_schema(schema, "Doc") {
        render_doc(store, descriptors, schemas, schema_tables, names, word)
    } else if schemas.is_map(schema) {
        render_map(
            store,
            descriptors,
            schemas,
            schema_tables,
            names,
            schema,
            word,
        )
    } else if schemas.is_primitive(schema, Primitive::Bytes) {
        let entry = store
            .entry(word)
            .ok_or_else(|| format!("store handle {word}"))?;
        Ok(RenderedValue::Raw {
            schema: schema.to_string(),
            bytes_utf8: String::from_utf8(entry.bytes.clone()).ok(),
        })
    } else if schemas.is_external(schema, "Cc")
        || schemas.is_external(schema, "Ar")
        || schemas.is_external(schema, "Rustc")
    {
        Ok(RenderedValue::Raw {
            schema: schema.to_string(),
            bytes_utf8: Some(store.string_value(word, schema)?),
        })
    } else {
        render_declared(
            store,
            descriptors,
            schemas,
            schema_tables,
            names,
            schema,
            word,
        )
    }
}

fn render_option(
    store: &ValueStore,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
    names: &RenderNames,
    handle: i64,
) -> Result<RenderedValue, String> {
    let entry = store
        .entry(handle)
        .ok_or_else(|| format!("store handle {handle}"))?;
    let value_schema = schemas
        .option_value_schema_name(&entry.schema)
        .ok_or_else(|| format!("handle {handle} is `{}`, not an Option", entry.schema))?;
    match store.option_payload(handle, schemas, schema_tables)? {
        OptionPayload::None => Ok(RenderedValue::Enum {
            schema: entry.schema.clone(),
            variant_index: 0,
            variant: "None".to_string(),
            fields: Vec::new(),
        }),
        OptionPayload::Some { word, realization } => {
            let value_schema = value_schema.as_str();
            let render_schema = match realization {
                Some(Realization::Pending) => {
                    pending_schema(realized_value_schema(value_schema).unwrap_or(value_schema))
                }
                _ => realized_value_schema(value_schema)
                    .unwrap_or(value_schema)
                    .to_string(),
            };
            Ok(RenderedValue::Enum {
                schema: entry.schema.clone(),
                variant_index: 1,
                variant: "Some".to_string(),
                fields: vec![RenderedField {
                    name: "0".to_string(),
                    schema: render_schema.clone(),
                    value: render_word(
                        store,
                        descriptors,
                        schemas,
                        schema_tables,
                        names,
                        &render_schema,
                        word,
                    )?,
                }],
            })
        }
    }
}

fn render_tree(store: &ValueStore, handle: i64) -> Result<RenderedValue, String> {
    let entry = store
        .entry(handle)
        .ok_or_else(|| format!("store handle {handle}"))?;
    match store.tree_entry(handle)? {
        TreeEntry::Concrete(tree) => Ok(RenderedValue::Tree {
            entries: tree
                .display_entries()
                .into_iter()
                .map(|(path, contents)| RenderedTreeEntry { path, contents })
                .collect(),
        }),
        TreeEntry::Merge(handles) => Ok(RenderedValue::TreePending {
            pending: RenderedTreePending {
                kind: "merge".to_string(),
                identity_hash: hex_content_hash(&entry.content_hash),
                pending: handles
                    .into_iter()
                    .map(|handle| render_pending(store, handle))
                    .collect::<Result<Vec<_>, _>>()?,
                run_id: None,
            },
        }),
        TreeEntry::Exec(run_id) => Ok(RenderedValue::TreePending {
            pending: RenderedTreePending {
                kind: "exec".to_string(),
                identity_hash: hex_content_hash(&entry.content_hash),
                pending: Vec::new(),
                run_id: Some(run_id),
            },
        }),
    }
}

fn render_sealed(store: &ValueStore, handle: i64) -> Result<RenderedValue, String> {
    let entry = store
        .entry(handle)
        .ok_or_else(|| format!("store handle {handle}"))?;
    if entry.schema != "Sealed" {
        return Err(format!(
            "render expected Sealed, got handle {handle} with schema {}",
            entry.schema
        ));
    }
    let payload = decode_sealed_payload(&entry.bytes)?;
    Ok(RenderedValue::Sealed {
        taint: payload.taint.marker,
        recipient: payload.taint.recipient,
        identity_hash: hex_bytes(&payload.taint.identity_hash),
        content_tag: payload.taint.content_tag,
    })
}

fn render_array(
    store: &ValueStore,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
    names: &RenderNames,
    handle: i64,
) -> Result<RenderedValue, String> {
    match store.array_entry(handle, schema_tables)? {
        ArrayEntry::Words { elem_schema, words } => Ok(RenderedValue::Array {
            element_schema: elem_schema.clone(),
            items: words
                .into_iter()
                .map(|word| {
                    render_word(
                        store,
                        descriptors,
                        schemas,
                        schema_tables,
                        names,
                        &elem_schema,
                        word,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?,
        }),
        ArrayEntry::Pending {
            elem_schema,
            pending,
        } => Ok(RenderedValue::Array {
            element_schema: elem_schema.clone(),
            items: pending
                .into_iter()
                .map(|handle| {
                    render_word(
                        store,
                        descriptors,
                        schemas,
                        schema_tables,
                        names,
                        &elem_schema,
                        handle,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?,
        }),
    }
}

fn render_map(
    store: &ValueStore,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
    names: &RenderNames,
    schema: &str,
    handle: i64,
) -> Result<RenderedValue, String> {
    let (_, pairs) = store.map_pairs(handle, schemas, schema_tables)?;
    Ok(RenderedValue::Map {
        schema: schema.to_string(),
        entries: pairs
            .into_iter()
            .map(|pair| {
                Ok(RenderedMapEntry {
                    key_schema: pair.key_schema.clone(),
                    key: render_word(
                        store,
                        descriptors,
                        schemas,
                        schema_tables,
                        names,
                        &pair.key_schema,
                        pair.key_word,
                    )?,
                    value_schema: pair.value_schema.clone(),
                    realization: pair.value_realization.map(|realization| match realization {
                        Realization::Ready => "Ready".to_string(),
                        Realization::Pending => "Pending".to_string(),
                    }),
                    value: render_word(
                        store,
                        descriptors,
                        schemas,
                        schema_tables,
                        names,
                        &pair.value_schema,
                        pair.value_word,
                    )?,
                })
            })
            .collect::<Result<Vec<_>, String>>()?,
    })
}

fn render_doc(
    store: &ValueStore,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
    names: &RenderNames,
    handle: i64,
) -> Result<RenderedValue, String> {
    let (variant, value) = match doc_payload(store, descriptors, handle)? {
        DocPayload::Null => ("Null".to_string(), None),
        DocPayload::Bool(word) => (
            "Bool".to_string(),
            Some(Box::new(render_word(
                store,
                descriptors,
                schemas,
                schema_tables,
                names,
                "Bool",
                word,
            )?)),
        ),
        DocPayload::Int(word) => (
            "Int".to_string(),
            Some(Box::new(render_word(
                store,
                descriptors,
                schemas,
                schema_tables,
                names,
                "Int",
                word,
            )?)),
        ),
        DocPayload::Float(word) => (
            "Float".to_string(),
            Some(Box::new(render_word(
                store,
                descriptors,
                schemas,
                schema_tables,
                names,
                "Float",
                word,
            )?)),
        ),
        DocPayload::String(word) => (
            "String".to_string(),
            Some(Box::new(render_word(
                store,
                descriptors,
                schemas,
                schema_tables,
                names,
                "String",
                word,
            )?)),
        ),
        DocPayload::Blob(word) => (
            "Blob".to_string(),
            Some(Box::new(RenderedValue::Raw {
                schema: "Blob".to_string(),
                bytes_utf8: store
                    .entry(word)
                    .and_then(|entry| String::from_utf8(entry.bytes.clone()).ok()),
            })),
        ),
        DocPayload::Array(word) => (
            "Array<Doc>".to_string(),
            Some(Box::new(render_word(
                store,
                descriptors,
                schemas,
                schema_tables,
                names,
                "Array<Doc>",
                word,
            )?)),
        ),
        DocPayload::Map(word) => (
            "Map".to_string(),
            Some(Box::new(render_word(
                store,
                descriptors,
                schemas,
                schema_tables,
                names,
                "Map<String,Doc>",
                word,
            )?)),
        ),
        DocPayload::Virtual(word) => (
            "Virtual".to_string(),
            Some(Box::new(render_word(
                store,
                descriptors,
                schemas,
                schema_tables,
                names,
                "String",
                word,
            )?)),
        ),
    };
    Ok(RenderedValue::Doc { variant, value })
}

fn render_declared(
    store: &ValueStore,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
    names: &RenderNames,
    schema: &str,
    handle: i64,
) -> Result<RenderedValue, String> {
    let entry = store
        .entry(handle)
        .ok_or_else(|| format!("store handle {handle}"))?;
    if entry.schema != schema {
        return Err(format!(
            "render expected {schema}, got handle {} with schema {}",
            handle, entry.schema
        ));
    }
    let descriptor = descriptors
        .get(schema)
        .ok_or_else(|| format!("descriptor for schema `{schema}`"))?;
    match &descriptor.access {
        Access::Record(record) => {
            let tuple_fields = tuple_schema_fields(schema);
            let field_names = names.structs.get(schema);
            let fields = record
                .fields
                .iter()
                .enumerate()
                .map(|(index, field)| {
                    let field_schema = descriptor_word_schema(schemas, &field.descriptor);
                    let word =
                        read_word_at(&entry.bytes, field.offset, field.descriptor.layout.size);
                    let name = if tuple_fields.is_some() {
                        index.to_string()
                    } else {
                        field_names
                            .and_then(|fields| fields.get(index).cloned())
                            .unwrap_or_else(|| index.to_string())
                    };
                    Ok(RenderedField {
                        name,
                        schema: field_schema.clone(),
                        value: render_word(
                            store,
                            descriptors,
                            schemas,
                            schema_tables,
                            names,
                            &field_schema,
                            word,
                        )?,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            if tuple_fields.is_some() {
                Ok(RenderedValue::Tuple {
                    schema: schema.to_string(),
                    fields,
                })
            } else {
                Ok(RenderedValue::Record {
                    schema: schema.to_string(),
                    fields,
                })
            }
        }
        Access::Enum(access) => {
            let selector = read_variant_tag(&entry.bytes, descriptor);
            let variant = access
                .variants
                .iter()
                .find(|variant| variant.selector == selector)
                .ok_or_else(|| format!("enum selector {selector}"))?;
            let variant_info = names
                .enums
                .get(schema)
                .and_then(|variants| variants.get(selector as usize));
            let fields = variant
                .payload
                .fields
                .iter()
                .enumerate()
                .map(|(index, field)| {
                    let field_schema = descriptor_word_schema(schemas, &field.descriptor);
                    let word =
                        read_word_at(&entry.bytes, field.offset, field.descriptor.layout.size);
                    Ok(RenderedField {
                        name: variant_info
                            .and_then(|info| info.fields.get(index).cloned())
                            .unwrap_or_else(|| index.to_string()),
                        schema: field_schema.clone(),
                        value: render_word(
                            store,
                            descriptors,
                            schemas,
                            schema_tables,
                            names,
                            &field_schema,
                            word,
                        )?,
                    })
                })
                .collect::<Result<Vec<_>, String>>()?;
            Ok(RenderedValue::Enum {
                schema: schema.to_string(),
                variant_index: selector,
                variant: variant_info
                    .map(|info| info.name.clone())
                    .unwrap_or_else(|| selector.to_string()),
                fields,
            })
        }
        Access::Scalar | Access::Handle { .. } | Access::Array { .. } => Ok(RenderedValue::Raw {
            schema: schema.to_string(),
            bytes_utf8: String::from_utf8(entry.bytes.clone()).ok(),
        }),
        other => Err(format!("cannot render descriptor access {other:?}")),
    }
}

fn render_pending(store: &ValueStore, handle: i64) -> Result<RenderedPending, String> {
    let entry = store
        .entry(handle)
        .ok_or_else(|| format!("store handle {handle}"))?;
    let invocation = store.pending_invocation(handle)?;
    Ok(RenderedPending {
        schema: entry.schema.clone(),
        closure_hash: format!("{:016x}", invocation.closure_hash),
        identity_hash: hex_content_hash(&invocation.identity_hash),
        remaining_arity: invocation.remaining_arity as u64,
        arg_count: invocation.args.len() as u64,
    })
}

fn descriptor_word_schema(schemas: &SchemaTables, descriptor: &VixDescriptor) -> String {
    match &descriptor.access {
        Access::Handle { target } => schemas.display_ref(target),
        Access::Scalar => match schemas.kind_for_ref(&descriptor.schema) {
            Some(Kind::Primitive(Primitive::I64)) => "Int".to_string(),
            Some(Kind::Primitive(Primitive::F64)) => "Float".to_string(),
            Some(Kind::Primitive(Primitive::Bool)) => "Bool".to_string(),
            _ => schemas.display_ref(&descriptor.schema),
        },
        _ => schemas.display_ref(&descriptor.schema),
    }
}

fn read_word_at(bytes: &[u8], offset: usize, size: usize) -> i64 {
    let mut word = [0u8; 8];
    word[..size].copy_from_slice(&bytes[offset..offset + size]);
    i64::from_le_bytes(word)
}

fn tuple_schema_fields(schema: &str) -> Option<Vec<String>> {
    let inner = schema.strip_prefix("Tuple<")?.strip_suffix('>')?;
    if inner.is_empty() {
        return Some(Vec::new());
    }
    Some(split_top_level_schemas(inner))
}

fn split_top_level_schemas(inner: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (index, ch) in inner.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                out.push(inner[start..index].to_string());
                start = index + 1;
            }
            _ => {}
        }
    }
    out.push(inner[start..].to_string());
    out
}

fn hex_content_hash(hash: &ContentHash) -> String {
    hash.to_string()
}

fn hex_bytes(hash: &[u8]) -> String {
    let mut out = String::with_capacity(hash.len() * 2);
    for byte in hash {
        use std::fmt::Write as _;
        write!(&mut out, "{byte:02x}").expect("writing to String cannot fail");
    }
    out
}

#[cfg(any(test, feature = "jit"))]
fn program_op_set(program: &Program) -> String {
    let mut names: Vec<&'static str> = program
        .fns
        .iter()
        .flat_map(|f| f.code.iter().map(op_name))
        .collect();
    names.sort_unstable();
    names.dedup();
    names.join(", ")
}

#[cfg(any(test, feature = "jit"))]
fn op_name(op: &Op) -> &'static str {
    match op {
        Op::ConstI64 { .. } => "ConstI64",
        Op::AddI64 { .. } => "AddI64",
        Op::SubI64 { .. } => "SubI64",
        Op::MulI64 { .. } => "MulI64",
        Op::DivI64 { .. } => "DivI64",
        Op::CopyI64 { .. } => "CopyI64",
        Op::EqI64 { .. } => "EqI64",
        Op::NeI64 { .. } => "NeI64",
        Op::LtI64 { .. } => "LtI64",
        Op::LeI64 { .. } => "LeI64",
        Op::GtI64 { .. } => "GtI64",
        Op::GeI64 { .. } => "GeI64",
        Op::Jump { .. } => "Jump",
        Op::JumpIfZero { .. } => "JumpIfZero",
        Op::Call { .. } => "Call",
        Op::CallIndirect { .. } => "CallIndirect",
        Op::Ret { .. } => "Ret",
        Op::Await { .. } => "Await",
        Op::LoadIndexedI64 { .. } => "LoadIndexedI64",
        Op::StoreIndexedI64 { .. } => "StoreIndexedI64",
        Op::LoadArrayWord { .. } => "LoadArrayWord",
        Op::ArrayNew { .. } => "ArrayNew",
        Op::ArrayStoreWord { .. } => "ArrayStoreWord",
        Op::ArrayStore { .. } => "ArrayStore",
        Op::LoadArray { .. } => "LoadArray",
        Op::LoadArrayLen { .. } => "LoadArrayLen",
        Op::CompareValueBytes { .. } => "CompareValueBytes",
        Op::ConstF64 { .. } => "ConstF64",
        Op::AddF64 { .. } => "AddF64",
        Op::MulF64 { .. } => "MulF64",
        Op::Trace { .. } => "Trace",
        Op::HostCall { .. } => "HostCall",
        Op::HostCallYield { .. } => "HostCallYield",
    }
}

fn memo_key_hash(key: &CanonMemoKey) -> u64 {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vix-memo-key-word");
    hasher.update(&key.0.to_le_bytes());
    for arg in &key.1 {
        hasher.update(arg.as_ref());
    }
    let hash = hasher.finalize();
    u64::from_le_bytes(hash.as_bytes()[..8].try_into().expect("blake3 prefix"))
}

fn pending_invocation_for(
    lowered: &LoweredFn,
    store: &RefCell<ValueStore>,
    schemas: &SchemaTables,
    args: Vec<i64>,
) -> PendingInvocation {
    let store = store.borrow();
    let identity_hash = pending_identity_hash(lowered, &store, schemas, &args);
    PendingInvocation {
        closure_hash: lowered.hash,
        primitive: None,
        remaining_arity: lowered.arg_schemas.len().saturating_sub(args.len()),
        args,
        identity_hash,
    }
}

fn pending_identity_hash(
    lowered: &LoweredFn,
    store: &ValueStore,
    schemas: &SchemaTables,
    args: &[i64],
) -> ContentHash {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vix-pending-invocation");
    hasher.update(&lowered.hash.to_le_bytes());
    hasher.update(
        &i64::try_from(lowered.arg_schemas.len().saturating_sub(args.len()))
            .expect("remaining arity fits i64")
            .to_le_bytes(),
    );
    for (&word, schema) in args.iter().zip(&lowered.arg_schemas) {
        update_schema_name(&mut hasher, schemas, schema);
        hasher.update(canonical_word_hash_in_store(store, schemas, schema, word).as_ref());
    }
    finish_hash(hasher)
}

fn read_frame_word(frame: &[u8], at: usize) -> i64 {
    i64::from_le_bytes(frame[at..at + 8].try_into().expect("frame word"))
}

fn write_frame_word(frame: &mut [u8], at: usize, value: i64) {
    frame[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

fn sealed_payload(
    ciphertext: Vec<u8>,
    marker: String,
    recipient: String,
    tag: Option<String>,
) -> SealedPayload {
    let identity_hash = sealed_identity_hash(&ciphertext);
    SealedPayload {
        ciphertext,
        taint: StructuralTaint {
            marker,
            recipient,
            identity_hash: identity_hash.to_vec(),
            content_tag: tag,
        },
    }
}

fn encode_sealed_payload(payload: &SealedPayload) -> Vec<u8> {
    let mut bytes = Vec::new();
    encode_bytes(&payload.ciphertext, &mut bytes);
    encode_string(&payload.taint.marker, &mut bytes);
    encode_string(&payload.taint.recipient, &mut bytes);
    match &payload.taint.content_tag {
        Some(tag) => {
            bytes.extend_from_slice(&1i64.to_le_bytes());
            encode_string(tag, &mut bytes);
        }
        None => bytes.extend_from_slice(&0i64.to_le_bytes()),
    }
    bytes
}

fn decode_sealed_payload(bytes: &[u8]) -> Result<SealedPayload, String> {
    let mut at = 0;
    let ciphertext = decode_bytes(bytes, &mut at)?;
    let marker = decode_string(bytes, &mut at)?;
    let recipient = decode_string(bytes, &mut at)?;
    let tag = match read_frame_word(bytes, at) {
        0 => {
            at += 8;
            None
        }
        1 => {
            at += 8;
            Some(decode_string(bytes, &mut at)?)
        }
        other => return Err(format!("unknown sealed content tag marker {other}")),
    };
    if at != bytes.len() {
        return Err(format!(
            "sealed payload has {} trailing byte(s)",
            bytes.len() - at
        ));
    }
    Ok(sealed_payload(ciphertext, marker, recipient, tag))
}

fn sealed_identity_hash(ciphertext: &[u8]) -> ContentHash {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vix-sealed-identity-v1");
    hasher.update(ciphertext);
    finish_hash(hasher)
}

fn hash_with_taint(base: ContentHash, taint: &Option<StructuralTaint>) -> ContentHash {
    let Some(taint) = taint else {
        return base;
    };
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vix-tainted-identity-v1");
    hasher.update(base.as_ref());
    hash_taint_into(&mut hasher, taint);
    finish_hash(hasher)
}

fn hash_taint_into(hasher: &mut blake3::Hasher, taint: &StructuralTaint) {
    update_hash_len(hasher, taint.marker.len());
    hasher.update(taint.marker.as_bytes());
    update_hash_len(hasher, taint.recipient.len());
    hasher.update(taint.recipient.as_bytes());
    update_hash_len(hasher, taint.identity_hash.len());
    hasher.update(&taint.identity_hash);
    match &taint.content_tag {
        Some(tag) => {
            hasher.update(&[1]);
            update_hash_len(hasher, tag.len());
            hasher.update(tag.as_bytes());
        }
        None => {
            hasher.update(&[0]);
        }
    }
}

fn combine_taints(taints: impl IntoIterator<Item = StructuralTaint>) -> Option<StructuralTaint> {
    let mut taints = taints.into_iter().collect::<Vec<_>>();
    taints.sort();
    taints.dedup();
    match taints.as_slice() {
        [] => None,
        [single] => Some(single.clone()),
        many => {
            let mut hasher = blake3::Hasher::new();
            hasher.update(b"vix-combined-taint-v1");
            for taint in many {
                hash_taint_into(&mut hasher, taint);
            }
            Some(StructuralTaint {
                marker: many
                    .iter()
                    .map(|taint| taint.marker.as_str())
                    .collect::<Vec<_>>()
                    .join("&"),
                // Law 4 attaches here: this field currently records all
                // observed recipients; the next rung replaces it with the
                // capability-checked intersection lattice.
                recipient: many
                    .iter()
                    .map(|taint| taint.recipient.as_str())
                    .collect::<Vec<_>>()
                    .join("&"),
                identity_hash: hasher.finalize().as_bytes().to_vec(),
                content_tag: None,
            })
        }
    }
}

fn taint_for_word(store: &ValueStore, word: i64) -> Option<StructuralTaint> {
    store.entry(word).and_then(|entry| entry.taint.clone())
}

fn taint_for_value_bytes(
    store: &ValueStore,
    descriptor: &VixDescriptor,
    bytes: &[u8],
) -> Option<StructuralTaint> {
    match &descriptor.access {
        Access::Record(record) => combine_taints(
            record
                .fields
                .iter()
                .map(|field| read_word_at(bytes, field.offset, field.descriptor.layout.size))
                .filter_map(|word| taint_for_word(store, word)),
        ),
        Access::Enum(access) => {
            let selector = read_variant_tag(bytes, descriptor);
            let variant = access
                .variants
                .iter()
                .find(|variant| variant.selector == selector)?;
            combine_taints(
                variant
                    .payload
                    .fields
                    .iter()
                    .map(|field| read_word_at(bytes, field.offset, field.descriptor.layout.size))
                    .filter_map(|word| taint_for_word(store, word)),
            )
        }
        Access::Handle { .. } => {
            taint_for_word(store, read_word_at(bytes, 0, descriptor.layout.size))
        }
        Access::Array { .. }
        | Access::Scalar
        | Access::Option(_)
        | Access::Tensor(_)
        | Access::Sequence(_)
        | Access::Set(_)
        | Access::Map(_)
        | Access::Result(_)
        | Access::Pointer(_)
        | Access::Dynamic
        | Access::Opaque(_)
        | Access::Recurse => None,
    }
}

fn taint_for_ordered_map_pairs(pairs: &[OrderedMapPair]) -> Option<StructuralTaint> {
    combine_taints(pairs.iter().flat_map(|pair| {
        [pair.key_taint.clone(), pair.value_taint.clone()]
            .into_iter()
            .flatten()
    }))
}

fn canonicalize_word_for_schema(schemas: &SchemaTables, schema: &str, word: i64) -> i64 {
    if schema_matches_primitive(schemas, schema, Primitive::F64) {
        let value = super::value::TotalF64::new(f64::from_bits(word as u64)).get();
        if value == 0.0 {
            0.0f64.to_bits() as i64
        } else {
            value.to_bits() as i64
        }
    } else {
        word
    }
}

fn canonicalize_word_for_schema_ref(
    schemas: &SchemaTables,
    schema_ref: &SchemaRef,
    word: i64,
) -> i64 {
    if matches!(
        schemas.kind_for_ref(schema_ref),
        Some(Kind::Primitive(Primitive::F64))
    ) {
        let value = super::value::TotalF64::new(f64::from_bits(word as u64)).get();
        if value == 0.0 {
            0.0f64.to_bits() as i64
        } else {
            value.to_bits() as i64
        }
    } else {
        word
    }
}

fn schema_matches_primitive(schemas: &SchemaTables, schema: &str, primitive: Primitive) -> bool {
    schemas.is_primitive(schema, primitive)
        || matches!(
            (schema, primitive),
            ("Int", Primitive::I64) | ("Bool", Primitive::Bool) | ("Float", Primitive::F64)
        )
}

fn schema_is_inline_word(schemas: &SchemaTables, schema: &str) -> bool {
    schema_matches_primitive(schemas, schema, Primitive::I64)
        || schema_matches_primitive(schemas, schema, Primitive::Bool)
        || schema_matches_primitive(schemas, schema, Primitive::F64)
}

fn identity_schema_matches(stored: &str, expected: &str) -> bool {
    stored == expected
        || pending_value_schema(expected) == Some(stored)
        || map_schema_is_realized_projection(expected, stored)
}

fn canonical_word_hash_in_store(
    store: &ValueStore,
    schemas: &SchemaTables,
    schema: &str,
    word: i64,
) -> ContentHash {
    if schema_is_inline_word(schemas, schema) {
        return canonical_scalar_hash(schemas, schema, word);
    }
    let entry = store
        .entry(word)
        .unwrap_or_else(|| panic!("non-inline `{schema}` word {word} is not interned"));
    assert!(
        identity_schema_matches(&entry.schema, schema),
        "stored identity schema mismatch: left `{}`, right `{schema}`",
        entry.schema
    );
    entry.content_hash
}

fn canonical_word_hash_for_descriptor(
    store: &ValueStore,
    schemas: &SchemaTables,
    descriptor: &VixDescriptor,
    word: i64,
) -> ContentHash {
    let schema_ref = match &descriptor.access {
        Access::Handle { target } => target,
        _ => &descriptor.schema,
    };
    if let Some(entry) = store.entry(word)
        && schemas.legacy_ref(&entry.schema) == *schema_ref
    {
        return entry.content_hash;
    }
    let word = canonicalize_word_for_schema_ref(schemas, schema_ref, word);
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vix-scalar-word");
    update_schema_ref(&mut hasher, schema_ref);
    hasher.update(&word.to_le_bytes());
    finish_hash(hasher)
}

fn canonical_scalar_hash(schemas: &SchemaTables, schema: &str, word: i64) -> ContentHash {
    let word = canonicalize_word_for_schema(schemas, schema, word);
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vix-scalar-word");
    update_schema_name(&mut hasher, schemas, schema);
    hasher.update(&word.to_le_bytes());
    finish_hash(hasher)
}

fn raw_value_content_hash(
    schema: &str,
    bytes: &[u8],
    schemas: &SchemaTables,
    taint: &Option<StructuralTaint>,
) -> ContentHash {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vix-raw-value");
    update_schema_name(&mut hasher, schemas, schema);
    hasher.update(bytes);
    hash_with_taint(finish_hash(hasher), taint)
}

fn is_projectable_schema_static(
    schema: &str,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
) -> bool {
    schemas.is_external(schema, "Tree")
        || schemas.is_list(schema)
        || schemas.is_named_schema(schema, "Doc")
        || schemas.is_primitive(schema, Primitive::Bytes)
        || schemas.is_map(schema)
        || pending_value_schema(schema).is_some()
        || descriptors.contains_key(schema)
}

fn is_projectable_arg_static(
    arg_schema: &str,
    arg: i64,
    store: &ValueStore,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
) -> bool {
    store.entry(arg).is_some_and(|entry| {
        entry.schema == arg_schema && is_projectable_schema_static(arg_schema, descriptors, schemas)
    })
}

struct ProjectionRecordContext<'a> {
    arg_schemas: &'a [String],
    args: &'a [i64],
    store: &'a ValueStore,
    descriptors: &'a DescriptorMap,
    schemas: &'a SchemaTables,
}

fn record_projection_for_matching_args_static(
    reads: &mut Vec<ProjectionRead>,
    context: &ProjectionRecordContext<'_>,
    handle: i64,
    path: ProjectionPath,
    observed: ContentHash,
) {
    for (arg_index, (&arg, schema)) in context.args.iter().zip(context.arg_schemas).enumerate() {
        if arg == handle
            && is_projectable_arg_static(
                schema,
                arg,
                context.store,
                context.descriptors,
                context.schemas,
            )
        {
            reads.push(ProjectionRead {
                arg_index,
                path: path.clone(),
                observed,
            });
        }
    }
}

fn record_whole_args_if_projectable_static(
    reads: &mut Vec<ProjectionRead>,
    arg_schemas: &[String],
    args: &[i64],
    store: &ValueStore,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    handles: impl IntoIterator<Item = i64>,
) {
    for handle in handles {
        for (arg_index, (&arg, schema)) in args.iter().zip(arg_schemas).enumerate() {
            if arg == handle && is_projectable_arg_static(schema, arg, store, descriptors, schemas)
            {
                reads.push(ProjectionRead {
                    arg_index,
                    path: ProjectionPath::Whole {
                        schema: schema.clone(),
                    },
                    observed: canonical_word_hash_in_store(store, schemas, schema, arg),
                });
            }
        }
    }
}

fn native_array_store_read_handles_at_materialization(
    task_has_native_array_load: bool,
    arg_schemas: &[String],
    args: &[i64],
    store_value_memories: &[ValueMemory],
    schemas: &SchemaTables,
) -> Vec<i64> {
    if !task_has_native_array_load {
        return Vec::new();
    }
    arg_schemas
        .iter()
        .zip(args)
        .filter_map(|(schema, &word)| {
            let elem_schema = array_element_schema(schema)?;
            schema_is_inline_word(schemas, elem_schema).then_some(())?;
            let Handle::Store(store_ix) = Handle::from_word(word) else {
                return None;
            };
            let memory = store_value_memories.get(store_ix.0)?;
            memory.is_resident().then_some(store_ix.to_word())
        })
        .collect()
}

fn remap_read_set_for_caller(
    caller_arg_schemas: &[String],
    caller_args: &[i64],
    callee_args: &[i64],
    callee_read_set: &ProjectionReadSet,
    store: &ValueStore,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
) -> ProjectionReadSet {
    let mut remapped = ProjectionReadSet::default();
    for read in &callee_read_set.entries {
        let Some(&callee_arg) = callee_args.get(read.arg_index) else {
            continue;
        };
        for (arg_index, (&caller_arg, schema)) in
            caller_args.iter().zip(caller_arg_schemas).enumerate()
        {
            if caller_arg == callee_arg
                && is_projectable_arg_static(schema, caller_arg, store, descriptors, schemas)
            {
                remapped.record(ProjectionRead {
                    arg_index,
                    path: read.path.clone(),
                    observed: read.observed,
                });
            }
        }
    }
    remapped
}

fn projection_observation_hash(
    store: &mut ValueStore,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
    handle: i64,
    path: &ProjectionPath,
) -> Result<ContentHash, String> {
    match path {
        ProjectionPath::Whole { schema } => {
            Ok(canonical_word_hash_in_store(store, schemas, schema, handle))
        }
        ProjectionPath::Field {
            schema,
            field_index,
        } => {
            let entry = store
                .entry(handle)
                .ok_or_else(|| format!("store handle {handle}"))?;
            if &entry.schema != schema {
                return Err(format!(
                    "handle {handle} is `{}`, not {schema}",
                    entry.schema
                ));
            }
            let descriptor = descriptors
                .get(schema)
                .ok_or_else(|| format!("descriptor for schema `{schema}`"))?;
            let field = field_descriptor(descriptor, &entry.bytes, *field_index);
            let offset = field_offset(descriptor, &entry.bytes, *field_index);
            let value = read_word_at(&entry.bytes, offset, field.layout.size);
            Ok(canonical_word_hash_for_descriptor(
                store, schemas, field, value,
            ))
        }
        ProjectionPath::Tag { schema } => {
            let entry = store
                .entry(handle)
                .ok_or_else(|| format!("store handle {handle}"))?;
            if &entry.schema != schema {
                return Err(format!(
                    "handle {handle} is `{}`, not {schema}",
                    entry.schema
                ));
            }
            let descriptor = descriptors
                .get(schema)
                .ok_or_else(|| format!("descriptor for schema `{schema}`"))?;
            Ok(canonical_scalar_hash(
                schemas,
                "Int",
                read_variant_tag(&entry.bytes, descriptor) as i64,
            ))
        }
        ProjectionPath::MapGet {
            map_schema,
            key_schema,
            key_hash,
            value_schema,
        } => {
            let entry = store
                .entry(handle)
                .ok_or_else(|| format!("store handle {handle}"))?;
            if &entry.schema != map_schema {
                return Err(format!(
                    "handle {handle} is `{}`, not {map_schema}",
                    entry.schema
                ));
            }
            store.map_get_option_hash_by_key_hash(
                handle,
                key_schema,
                *key_hash,
                value_schema,
                schemas,
                schema_tables,
            )
        }
        ProjectionPath::TreePath { path } => {
            let TreeEntry::Concrete(tree) = store.tree_entry(handle)? else {
                return Ok(canonical_word_hash_in_store(store, schemas, "Tree", handle));
            };
            match subtree(&tree, path) {
                Ok(projected) => Ok(hash_concrete_tree(&projected)),
                Err(_) => Ok(canonical_scalar_hash(schemas, "Missing", 0)),
            }
        }
        ProjectionPath::DocGet { key } => {
            doc_get_observation_hash(store, descriptors, schemas, schema_tables, handle, key)
        }
        ProjectionPath::Elf { projection } => {
            let projection = super::elf::Projection::ALL
                .into_iter()
                .find(|candidate| candidate.name() == projection)
                .ok_or_else(|| format!("unknown elf projection {projection}"))?;
            let bytes = match store
                .entry(handle)
                .ok_or_else(|| format!("store handle {handle}"))?
                .clone()
            {
                StoreEntry {
                    schema,
                    tier: _,
                    bytes,
                    content_hash: _,
                    taint: _,
                } if schemas.is_primitive(&schema, Primitive::Bytes)
                    || schemas.is_primitive(&schema, Primitive::String) =>
                {
                    bytes
                }
                StoreEntry { schema, .. } if schemas.is_external(&schema, "Tree") => {
                    let TreeEntry::Concrete(tree) = store.tree_entry(handle)? else {
                        return Ok(canonical_word_hash_in_store(store, schemas, "Tree", handle));
                    };
                    let count = tree.entries.len() + tree.blobs.len();
                    if count != 1 {
                        return Ok(canonical_word_hash_in_store(store, schemas, "Tree", handle));
                    }
                    tree.entries
                        .into_values()
                        .map(|contents| contents.into_bytes())
                        .chain(tree.blobs.into_values())
                        .next()
                        .expect("one tree entry")
                }
                StoreEntry { schema, .. } => {
                    return Err(format!(
                        "elf input must be Blob, String, or Tree, got {schema}"
                    ));
                }
            };
            let value = super::elf::project(&bytes, projection)?;
            let scratch = RefCell::new(store.clone());
            let handle =
                alloc_doc_from_value(&scratch, descriptors, schemas, schema_tables, value)?;
            Ok(scratch
                .borrow()
                .entry(handle)
                .expect("elf projection handle")
                .content_hash)
        }
        ProjectionPath::Ast { projection, name } => ast_projection_observation_hash(
            store,
            descriptors,
            schemas,
            schema_tables,
            handle,
            projection,
            name.as_deref(),
        ),
        ProjectionPath::Oci { projection } => {
            let projection = super::oci::Projection::ALL
                .into_iter()
                .find(|candidate| candidate.name() == projection)
                .ok_or_else(|| format!("unknown OCI projection {projection}"))?;
            let tree = match oci_tree_from_store(store, schemas, handle) {
                Ok(tree) => tree,
                Err(err)
                    if store
                        .entry(handle)
                        .is_some_and(|entry| schemas.is_external(&entry.schema, "Tree")) =>
                {
                    let _ = err;
                    return Ok(canonical_word_hash_in_store(store, schemas, "Tree", handle));
                }
                Err(err) => return Err(err),
            };
            let input_hash = ContentHash::from(super::oci::input_hash(&tree));
            let scratch = RefCell::new(store.clone());
            let projected = if projection == super::oci::Projection::Files {
                alloc_oci_files_doc(&scratch, descriptors, schemas, input_hash, handle)?
            } else {
                let layout = super::oci::parse_layout(tree)?;
                let value = super::oci::project(&layout, projection)?;
                alloc_doc_from_value(&scratch, descriptors, schemas, schema_tables, value)?
            };
            Ok(scratch
                .borrow()
                .entry(projected)
                .expect("OCI projection handle")
                .content_hash)
        }
    }
}

fn doc_get_observation_hash(
    store: &mut ValueStore,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
    handle: i64,
    key: &str,
) -> Result<ContentHash, String> {
    if let Some(input) = oci_virtual_files_input(store, descriptors, handle)? {
        return doc_get_observation_hash_via_allocation(
            store,
            descriptors,
            schemas,
            schema_tables,
            handle,
            key,
            Some(input),
        );
    }

    let payload = doc_payload(store, descriptors, handle)?;
    let DocPayload::Map(map) = payload else {
        return Ok(option_none_content_hash("Realized<Doc>", schema_tables));
    };
    let key_hash = raw_value_content_hash("String", key.as_bytes(), schemas, &None);
    store.map_get_option_hash_by_key_hash(
        map,
        "String",
        key_hash,
        "Realized<Doc>",
        schemas,
        schema_tables,
    )
}

fn doc_get_observation_hash_via_allocation(
    store: &ValueStore,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
    handle: i64,
    key: &str,
    virtual_input: Option<i64>,
) -> Result<ContentHash, String> {
    if let Some(input) = virtual_input {
        let tree = oci_tree_from_store(store, schemas, input)?;
        let layout = super::oci::parse_layout(tree)?;
        let projected = super::oci::project_file(&layout, key)?;
        let scratch = RefCell::new(store.clone());
        let option = if let Some(file) = projected {
            let doc = alloc_doc_from_value(
                &scratch,
                descriptors,
                schemas,
                schema_tables,
                Value::Map(BTreeMap::from([
                    (Value::Str("path".to_string()), Value::Str(key.to_string())),
                    (
                        Value::Str("contents".to_string()),
                        match file.contents {
                            super::oci::FileContents::Text(contents) => Value::Str(contents),
                            super::oci::FileContents::Blob(contents) => Value::Blob(contents),
                        },
                    ),
                    (
                        Value::Str("layer_digest".to_string()),
                        Value::Str(file.layer_digest),
                    ),
                    (Value::Str("size".to_string()), Value::Int(file.size)),
                ])),
            )?;
            scratch.borrow_mut().alloc_option_some(
                "Realized<Doc>",
                doc,
                Some(Realization::Ready),
                schemas,
                schema_tables,
            )?
        } else {
            scratch
                .borrow_mut()
                .alloc_option_none("Realized<Doc>", schema_tables)?
        };
        return Ok(scratch
            .borrow()
            .entry(option.0)
            .expect("virtual doc option handle")
            .content_hash);
    }

    let scratch = RefCell::new(store.clone());
    let (option, _) = doc_get(&scratch, descriptors, schemas, schema_tables, handle, key)?;
    Ok(scratch
        .borrow()
        .entry(option)
        .expect("doc option handle")
        .content_hash)
}

fn ast_projection_observation_hash(
    store: &ValueStore,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
    handle: i64,
    projection: &str,
    name: Option<&str>,
) -> Result<ContentHash, String> {
    let projection = ast_projection_by_name(projection)?;
    let Some((source, input_hash)) = ast_input_source_for_verify(store, handle)? else {
        return Ok(canonical_word_hash_in_store(store, schemas, "Tree", handle));
    };
    let file = super::ast_probe::parse(&source)?;
    let scratch = RefCell::new(store.clone());
    let projected = match projection {
        super::ast_probe::Projection::Items => alloc_doc_from_value(
            &scratch,
            descriptors,
            schemas,
            schema_tables,
            super::ast_probe::items(&file),
        )?,
        super::ast_probe::Projection::Fns => alloc_doc_from_value(
            &scratch,
            descriptors,
            schemas,
            schema_tables,
            super::ast_probe::fns(&file),
        )?,
        super::ast_probe::Projection::Fn => {
            let name = name.ok_or_else(|| "ast projection fn missing function name".to_string())?;
            let item = super::ast_probe::fn_item(&file, name)?;
            alloc_ast_fn_doc(
                &scratch,
                descriptors,
                schemas,
                schema_tables,
                handle,
                input_hash,
                item,
            )?
        }
        super::ast_probe::Projection::FnBodyChildren => {
            let name = name.ok_or_else(|| {
                "ast projection fn.body.children missing function name".to_string()
            })?;
            let item = super::ast_probe::fn_item(&file, name)?;
            alloc_doc_from_value(
                &scratch,
                descriptors,
                schemas,
                schema_tables,
                super::ast_probe::fn_body_children(item),
            )?
        }
    };
    Ok(scratch
        .borrow()
        .entry(projected)
        .expect("AST projection handle")
        .content_hash)
}

fn ast_projection_by_name(name: &str) -> Result<super::ast_probe::Projection, String> {
    Ok(match name {
        "items" => super::ast_probe::Projection::Items,
        "fns" => super::ast_probe::Projection::Fns,
        "fn" => super::ast_probe::Projection::Fn,
        "fn.body.children" => super::ast_probe::Projection::FnBodyChildren,
        other => return Err(format!("unknown AST projection {other}")),
    })
}

fn ast_input_source_for_verify(
    store: &ValueStore,
    handle: i64,
) -> Result<Option<(String, ContentHash)>, String> {
    let entry = store
        .entry(handle)
        .cloned()
        .ok_or_else(|| format!("store handle {handle}"))?;
    match entry.schema.as_str() {
        "String" => {
            let source = String::from_utf8(entry.bytes).map_err(|err| err.to_string())?;
            let mut hasher = blake3::Hasher::new();
            hasher.update(b"vix-ast-input");
            hasher.update(source.as_bytes());
            Ok(Some((source, finish_hash(hasher))))
        }
        "Tree" => {
            let TreeEntry::Concrete(tree) = store.tree_entry(handle)? else {
                return Ok(None);
            };
            if tree.entries.len() != 1 || !tree.blobs.is_empty() {
                return Ok(None);
            }
            let source = tree.entries.into_values().next().expect("one source entry");
            let mut hasher = blake3::Hasher::new();
            hasher.update(b"vix-ast-input");
            hasher.update(source.as_bytes());
            Ok(Some((source, finish_hash(hasher))))
        }
        other => Err(format!("ast input must be String or Tree, got {other}")),
    }
}

fn canonical_map_pairs(
    store: &ValueStore,
    pairs: Vec<MapPair>,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
) -> Result<Vec<OrderedMapPair>, String> {
    store.map_intern_counters.add(MapInternStats {
        rows_canonicalized: pairs.len(),
        child_identity_reads: pairs.len().saturating_mul(2),
        sort_rows: pairs.len(),
        ..MapInternStats::default()
    });
    let mut pairs: Vec<OrderedMapPair> = pairs
        .into_iter()
        .map(|pair| ordered_map_pair(store, schemas, pair))
        .collect();
    sort_and_dedup_ordered_map_pairs(store, descriptors, schemas, schema_tables, &mut pairs)?;
    Ok(pairs)
}

fn carried_map_rows_after_insert(
    store: &ValueStore,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
    carried: CarriedMapRows,
    pair: MapPair,
) -> Result<CarriedMapRows, String> {
    let mut ordered = carried.ordered;
    store.map_intern_counters.add(MapInternStats {
        rows_canonicalized: 1,
        child_identity_reads: 2,
        ..MapInternStats::default()
    });
    let row = ordered_map_pair(store, schemas, pair);
    match ordered.binary_search_by(|candidate| {
        store.map_intern_counters.add(MapInternStats {
            sort_comparisons: 1,
            ..MapInternStats::default()
        });
        ordered_map_pair_cmp(store, descriptors, schemas, schema_tables, candidate, &row)
    }) {
        Ok(index) => ordered[index] = row,
        Err(index) => ordered.insert(index, row),
    }
    Ok(CarriedMapRows { ordered })
}

fn ordered_map_pair(
    store: &ValueStore,
    schemas: &SchemaTables,
    mut pair: MapPair,
) -> OrderedMapPair {
    pair.key_word = canonicalize_word_for_schema(schemas, &pair.key_schema, pair.key_word);
    pair.value_word = canonicalize_word_for_schema(schemas, &pair.value_schema, pair.value_word);
    let key_hash = canonical_word_hash_in_store(store, schemas, &pair.key_schema, pair.key_word);
    let value_hash =
        canonical_word_hash_in_store(store, schemas, &pair.value_schema, pair.value_word);
    let key_taint = taint_for_word(store, pair.key_word);
    let value_taint = taint_for_word(store, pair.value_word);
    OrderedMapPair {
        pair,
        key_hash,
        value_hash,
        key_taint,
        value_taint,
    }
}

fn sort_and_dedup_ordered_map_pairs(
    store: &ValueStore,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
    pairs: &mut Vec<OrderedMapPair>,
) -> Result<(), String> {
    pairs.sort_by(|a, b| {
        store.map_intern_counters.add(MapInternStats {
            sort_comparisons: 1,
            ..MapInternStats::default()
        });
        ordered_map_pair_cmp(store, descriptors, schemas, schema_tables, a, b)
    });
    let mut deduped: Vec<OrderedMapPair> = Vec::new();
    for pair in std::mem::take(pairs) {
        match deduped.last_mut() {
            Some(last) if same_canonical_map_key(last, &pair) => {
                *last = pair;
                continue;
            }
            _ => {}
        }
        deduped.push(pair);
    }
    *pairs = deduped;
    Ok(())
}

fn same_canonical_map_key(a: &OrderedMapPair, b: &OrderedMapPair) -> bool {
    a.pair.key_schema == b.pair.key_schema && a.key_hash == b.key_hash
}

fn ordered_map_pair_cmp(
    store: &ValueStore,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
    a: &OrderedMapPair,
    b: &OrderedMapPair,
) -> Ordering {
    let primary = compare_ordered_map_pairs(store, descriptors, schemas, schema_tables, a, b)
        .unwrap_or_else(|_| a.key_hash.as_ref().cmp(b.key_hash.as_ref()));
    if primary == Ordering::Equal {
        a.key_hash.as_ref().cmp(b.key_hash.as_ref())
    } else {
        primary
    }
}

fn ordered_map_pairs_from_decoded(
    store: &ValueStore,
    pairs: Vec<MapPair>,
    schemas: &SchemaTables,
) -> Vec<OrderedMapPair> {
    pairs
        .into_iter()
        .map(|pair| {
            let key_hash =
                canonical_word_hash_in_store(store, schemas, &pair.key_schema, pair.key_word);
            let value_hash =
                canonical_word_hash_in_store(store, schemas, &pair.value_schema, pair.value_word);
            OrderedMapPair {
                pair,
                key_hash,
                value_hash,
                key_taint: None,
                value_taint: None,
            }
        })
        .collect()
}

fn compare_ordered_map_pairs(
    store: &ValueStore,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    schema_tables: &SchemaTables,
    a: &OrderedMapPair,
    b: &OrderedMapPair,
) -> Result<Ordering, String> {
    let schema_order = a.pair.key_schema.cmp(&b.pair.key_schema);
    if schema_order != Ordering::Equal {
        return Ok(schema_order);
    }
    compare_words(
        store,
        descriptors,
        schemas,
        schema_tables,
        &a.pair.key_schema,
        a.pair.key_word,
        b.pair.key_word,
    )
}

fn promote_map_pairs_to_realized(
    stored_schema: &str,
    map_schema: &str,
    pairs: Vec<MapPair>,
) -> Result<Vec<MapPair>, String> {
    let Some((stored_key, stored_value)) = map_schemas(stored_schema) else {
        return Err(format!("stored schema {stored_schema} is not a Map"));
    };
    let Some((map_key, map_value)) = map_schemas(map_schema) else {
        return Err(format!("target schema {map_schema} is not a Map"));
    };
    let Some(realized_value) = realized_value_schema(map_value) else {
        return Err(format!(
            "expected map schema {map_schema}, got {stored_schema}"
        ));
    };
    if stored_key != map_key || stored_value != realized_value {
        return Err(format!(
            "expected map schema {map_schema}, got {stored_schema}"
        ));
    }

    pairs
        .into_iter()
        .map(|pair| {
            if pair.value_schema != stored_value {
                return Err(format!(
                    "cannot promote pair value schema {} from {stored_schema} to {map_schema}",
                    pair.value_schema
                ));
            }
            Ok(MapPair {
                value_schema: realized_value.to_string(),
                value_word: pair.value_word,
                value_realization: Some(Realization::Ready),
                ..pair
            })
        })
        .collect()
}

fn encode_map_pairs(
    pairs: &[OrderedMapPair],
    schema_tables: &SchemaTables,
) -> Result<Vec<u8>, String> {
    let bitset_words = if pairs
        .iter()
        .any(|pair| pair.pair.value_realization.is_some())
    {
        map_realization_bitset_words(pairs.len())
    } else {
        0
    };
    let mut bytes = Vec::with_capacity(8 + pairs.len() * 32 + bitset_words * 8);
    bytes.extend_from_slice(
        &i64::try_from(pairs.len())
            .expect("map pair count fits i64")
            .to_le_bytes(),
    );
    for pair in pairs {
        bytes.extend_from_slice(
            &schema_ref_for(&pair.pair.key_schema, schema_tables)?.to_le_bytes(),
        );
        bytes.extend_from_slice(&pair.pair.key_word.to_le_bytes());
        bytes.extend_from_slice(
            &schema_ref_for(&pair.pair.value_schema, schema_tables)?.to_le_bytes(),
        );
        bytes.extend_from_slice(&pair.pair.value_word.to_le_bytes());
    }
    if bitset_words > 0 {
        let mut bitset = vec![0u64; bitset_words];
        for (index, pair) in pairs.iter().enumerate() {
            let realization = pair
                .pair
                .value_realization
                .as_ref()
                .ok_or_else(|| "realized map row missing realization bit".to_string())?;
            bitset[index / 64] |= realization.bit() << (index % 64);
        }
        for word in bitset {
            bytes.extend_from_slice(&(word as i64).to_le_bytes());
        }
    }
    Ok(bytes)
}

fn decode_map_pairs(bytes: &[u8], schema_tables: &SchemaTables) -> Result<Vec<MapPair>, String> {
    if bytes.len() < 8 {
        return Err("Map entry is shorter than its count word".into());
    }
    let count = usize::try_from(read_frame_word(bytes, 0)).map_err(|_| "negative map count")?;
    let rows_end = 8 + count * 32;
    let bitset_words = map_realization_bitset_words(count);
    let expected_plain = rows_end;
    let expected_realized = rows_end + bitset_words * 8;
    let has_realization_bitset = bytes.len() == expected_realized && bitset_words > 0;
    if bytes.len() != expected_plain && !has_realization_bitset {
        return Err(format!(
            "Map entry has {} bytes, expected {expected_plain} or {expected_realized}",
            bytes.len()
        ));
    }
    let mut pairs = Vec::with_capacity(count);
    for i in 0..count {
        let at = 8 + i * 32;
        let key_schema = schema_name_for(read_frame_word(bytes, at), schema_tables)?;
        let key_word = read_frame_word(bytes, at + 8);
        let value_schema = schema_name_for(read_frame_word(bytes, at + 16), schema_tables)?;
        let value_word = read_frame_word(bytes, at + 24);
        let value_realization = if has_realization_bitset {
            let bitset_word = read_frame_word(bytes, rows_end + (i / 64) * 8) as u64;
            Some(if ((bitset_word >> (i % 64)) & 1) == 0 {
                Realization::Ready
            } else {
                Realization::Pending
            })
        } else {
            None
        };
        pairs.push(MapPair {
            key_schema,
            key_word,
            value_schema,
            value_word,
            value_realization,
        });
    }
    Ok(pairs)
}

fn hash_map_pairs(
    schema: &str,
    pairs: &[OrderedMapPair],
    schema_tables: &SchemaTables,
) -> ContentHash {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vix-map");
    update_schema_name(&mut hasher, schema_tables, schema);
    hasher.update(
        &i64::try_from(pairs.len())
            .expect("map pair count fits i64")
            .to_le_bytes(),
    );
    for pair in pairs {
        update_schema_name(&mut hasher, schema_tables, &pair.pair.key_schema);
        hasher.update(pair.key_hash.as_ref());
        update_schema_name(&mut hasher, schema_tables, &pair.pair.value_schema);
        // Realization is a declared type wrapper and stays in content identity; HandleTier is store scheduling state and stays out.
        if let Some(realization) = &pair.pair.value_realization {
            hasher.update(&realization.to_word().to_le_bytes());
        }
        hasher.update(pair.value_hash.as_ref());
    }
    finish_hash(hasher)
}

fn map_realization_bitset_words(count: usize) -> usize {
    count.div_ceil(64)
}

fn simple_glob_match(pattern: &str, path: &str) -> bool {
    let Some((prefix, suffix)) = pattern.split_once('*') else {
        return path == pattern;
    };
    if pattern[prefix.len() + 1..].contains('*') {
        return false;
    }
    let scoped = pattern.contains('/');
    (scoped || !path.contains('/')) && path.starts_with(prefix) && path.ends_with(suffix)
}

fn schema_ref_for(schema: &str, schema_tables: &SchemaTables) -> Result<i64, String> {
    Ok(schema_tables.frame_word_for_name(schema))
}

fn schema_name_for(schema_ref: i64, schema_tables: &SchemaTables) -> Result<String, String> {
    schema_tables
        .name_for_frame_word(schema_ref)
        .map(str::to_string)
        .ok_or_else(|| format!("schema ref {schema_ref}"))
}

fn option_schema(value_schema: &str) -> String {
    format!("Option<{value_schema}>")
}

fn array_schema(elem_schema: &str) -> String {
    format!("Array<{elem_schema}>")
}

fn array_element_schema(schema: &str) -> Option<&str> {
    let (base, args) = generic_schema(schema)?;
    (base == "Array" || base == "List").then_some(())?;
    let [elem]: [&str; 1] = args.try_into().ok()?;
    Some(elem)
}

fn pending_schema(value_schema: &str) -> String {
    format!("Pending<{value_schema}>")
}

fn pending_value_schema(schema: &str) -> Option<&str> {
    schema.strip_prefix("Pending<")?.strip_suffix('>')
}

fn realized_value_schema(schema: &str) -> Option<&str> {
    schema.strip_prefix("Realized<")?.strip_suffix('>')
}

fn map_schemas(schema: &str) -> Option<(&str, &str)> {
    let (base, args) = generic_schema(schema)?;
    (base == "Map").then_some(())?;
    let [key, value]: [&str; 2] = args.try_into().ok()?;
    Some((key, value))
}

fn generic_schema(schema: &str) -> Option<(&str, Vec<&str>)> {
    let (base, rest) = schema.split_once('<')?;
    Some((base, split_top_level_schema_slices(rest.strip_suffix('>')?)))
}

fn split_top_level_schema_slices(inner: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (index, ch) in inner.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                out.push(&inner[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    out.push(&inner[start..]);
    out
}

fn encode_pending_invocation(invocation: &PendingInvocation) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&invocation.closure_hash.to_le_bytes());
    bytes.extend_from_slice(
        &i64::try_from(invocation.args.len())
            .expect("argc fits i64")
            .to_le_bytes(),
    );
    bytes.extend_from_slice(
        &i64::try_from(invocation.remaining_arity)
            .expect("remaining arity fits i64")
            .to_le_bytes(),
    );
    let primitive = invocation
        .primitive
        .as_ref()
        .map(PendingPrimitive::to_word)
        .unwrap_or(-1);
    bytes.extend_from_slice(&primitive.to_le_bytes());
    bytes.extend_from_slice(invocation.identity_hash.as_ref());
    for arg in &invocation.args {
        bytes.extend_from_slice(&arg.to_le_bytes());
    }
    bytes
}

fn decode_pending_invocation(bytes: &[u8]) -> Result<PendingInvocation, String> {
    if bytes.len() < 64 {
        return Err("pending invocation too short".into());
    }
    let closure_hash =
        u64::from_le_bytes(bytes[0..8].try_into().expect("pending closure hash word"));
    let argc = usize::try_from(read_frame_word(bytes, 8)).map_err(|_| "argc")?;
    let remaining_arity =
        usize::try_from(read_frame_word(bytes, 16)).map_err(|_| "remaining arity")?;
    let primitive = match read_frame_word(bytes, 24) {
        -1 => None,
        word => Some(PendingPrimitive::from_word(word)?),
    };
    let identity_hash =
        ContentHash::from_slice(&bytes[32..64]).expect("pending identity hash length");
    let expected = 64 + argc * 8;
    if bytes.len() != expected {
        return Err(format!(
            "pending invocation has {} bytes, expected {expected}",
            bytes.len()
        ));
    }
    let args = (0..argc)
        .map(|i| read_frame_word(bytes, 64 + i * 8))
        .collect();
    Ok(PendingInvocation {
        closure_hash,
        primitive,
        args,
        remaining_arity,
        identity_hash,
    })
}

fn encode_handle_list(kind: i64, handles: &[i64]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&kind.to_le_bytes());
    bytes.extend_from_slice(
        &i64::try_from(handles.len())
            .expect("handle length fits i64")
            .to_le_bytes(),
    );
    for handle in handles {
        bytes.extend_from_slice(&handle.to_le_bytes());
    }
    bytes
}

fn decode_handle_list(bytes: &[u8]) -> Result<Vec<i64>, String> {
    if bytes.len() < 16 {
        return Err("handle list too short".into());
    }
    let count = usize::try_from(read_frame_word(bytes, 8)).map_err(|_| "handle count")?;
    let expected = 16 + count * 8;
    if bytes.len() != expected {
        return Err(format!(
            "handle list has {} bytes, expected {expected}",
            bytes.len()
        ));
    }
    Ok((0..count)
        .map(|i| read_frame_word(bytes, 16 + i * 8))
        .collect())
}

fn encode_array_word_payload(elem_schema_ref: i64, words: &[i64]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(24 + words.len() * 8);
    bytes.extend_from_slice(&0i64.to_le_bytes());
    bytes.extend_from_slice(&elem_schema_ref.to_le_bytes());
    bytes.extend_from_slice(
        &i64::try_from(words.len())
            .expect("array length fits i64")
            .to_le_bytes(),
    );
    for word in words {
        bytes.extend_from_slice(&word.to_le_bytes());
    }
    bytes
}

fn molten_value_payload(entry: &MoltenEntry, schemas: &SchemaTables) -> Option<Vec<u8>> {
    match &entry.value {
        MoltenValue::ArrayWords { elem_schema, words } => Some(encode_array_word_payload(
            schemas.frame_word_for_name(elem_schema),
            words,
        )),
        MoltenValue::Record { .. }
        | MoltenValue::Map { .. }
        | MoltenValue::Interned(_)
        | MoltenValue::Interning => None,
    }
}

fn hash_handle_list(
    domain: &[u8],
    handles: &[i64],
    store: &ValueStore,
    schemas: &SchemaTables,
) -> ContentHash {
    let mut hasher = blake3::Hasher::new();
    hasher.update(domain);
    hasher.update(
        &i64::try_from(handles.len())
            .expect("handle length fits i64")
            .to_le_bytes(),
    );
    for handle in handles {
        let entry = store
            .entry(*handle)
            .unwrap_or_else(|| panic!("store handle {handle}"));
        update_schema_name(&mut hasher, schemas, &entry.schema);
        hasher.update(entry.content_hash.as_ref());
    }
    finish_hash(hasher)
}

fn encode_concrete_tree(tree: &crate::exec::Tree) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&0i64.to_le_bytes());
    bytes.extend_from_slice(
        &i64::try_from(tree.entries.len())
            .expect("tree entry count fits i64")
            .to_le_bytes(),
    );
    for (path, contents) in &tree.entries {
        encode_string(path, &mut bytes);
        encode_string(contents, &mut bytes);
    }
    bytes.extend_from_slice(
        &i64::try_from(tree.blobs.len())
            .expect("tree blob count fits i64")
            .to_le_bytes(),
    );
    for (path, contents) in &tree.blobs {
        encode_string(path, &mut bytes);
        encode_bytes(contents, &mut bytes);
    }
    bytes
}

fn decode_concrete_tree(bytes: &[u8]) -> Result<crate::exec::Tree, String> {
    if bytes.len() < 16 {
        return Err("tree entry too short".into());
    }
    let count = usize::try_from(read_frame_word(bytes, 8)).map_err(|_| "tree entry count")?;
    let mut at = 16;
    let mut entries = BTreeMap::new();
    for _ in 0..count {
        let path = decode_string(bytes, &mut at)?;
        let contents = decode_string(bytes, &mut at)?;
        entries.insert(path, contents);
    }
    let mut blobs = BTreeMap::new();
    if at < bytes.len() {
        let blob_count =
            usize::try_from(read_frame_word(bytes, at)).map_err(|_| "tree blob count")?;
        at += 8;
        for _ in 0..blob_count {
            let path = decode_string(bytes, &mut at)?;
            let contents = decode_bytes(bytes, &mut at)?;
            blobs.insert(path, contents);
        }
    }
    if at != bytes.len() {
        return Err(format!(
            "tree entry has {} trailing bytes",
            bytes.len() - at
        ));
    }
    Ok(crate::exec::Tree { entries, blobs })
}

fn hash_concrete_tree(tree: &crate::exec::Tree) -> ContentHash {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vix-tree-concrete");
    for (path, contents) in &tree.entries {
        hasher.update(&[0]);
        hasher.update(
            &i64::try_from(path.len())
                .expect("path length fits i64")
                .to_le_bytes(),
        );
        hasher.update(path.as_bytes());
        hasher.update(
            &i64::try_from(contents.len())
                .expect("contents length fits i64")
                .to_le_bytes(),
        );
        hasher.update(contents.as_bytes());
    }
    for (path, contents) in &tree.blobs {
        hasher.update(&[1]);
        hasher.update(
            &i64::try_from(path.len())
                .expect("path length fits i64")
                .to_le_bytes(),
        );
        hasher.update(path.as_bytes());
        hasher.update(
            &i64::try_from(contents.len())
                .expect("contents length fits i64")
                .to_le_bytes(),
        );
        hasher.update(contents);
    }
    finish_hash(hasher)
}

fn encode_string(value: &str, bytes: &mut Vec<u8>) {
    encode_bytes(value.as_bytes(), bytes);
}

fn encode_bytes(value: &[u8], bytes: &mut Vec<u8>) {
    bytes.extend_from_slice(
        &i64::try_from(value.len())
            .expect("string length fits i64")
            .to_le_bytes(),
    );
    bytes.extend_from_slice(value);
}

fn decode_string(bytes: &[u8], at: &mut usize) -> Result<String, String> {
    let value = decode_bytes(bytes, at)?;
    String::from_utf8(value).map_err(|err| err.to_string())
}

fn decode_bytes(bytes: &[u8], at: &mut usize) -> Result<Vec<u8>, String> {
    if bytes.len() < *at + 8 {
        return Err("string length truncated".into());
    }
    let len = usize::try_from(read_frame_word(bytes, *at)).map_err(|_| "string length")?;
    *at += 8;
    if bytes.len() < *at + len {
        return Err("string data truncated".into());
    }
    let value = bytes[*at..*at + len].to_vec();
    *at += len;
    Ok(value)
}

const OS_LINUX: u64 = 0;
const OS_MACOS: u64 = 1;
const OS_WINDOWS: u64 = 2;

const ARCH_X86_64: u64 = 0;
const ARCH_AARCH64: u64 = 1;
const ARCH_ARM: u64 = 2;
const ARCH_RISCV64: u64 = 3;
const ARCH_WASM32: u64 = 4;
const ARCH_UNKNOWN: u64 = 5;

fn host_os_index() -> u64 {
    if cfg!(target_os = "linux") {
        OS_LINUX
    } else if cfg!(target_os = "macos") {
        OS_MACOS
    } else if cfg!(target_os = "windows") {
        OS_WINDOWS
    } else {
        OS_LINUX
    }
}

fn host_arch_index() -> u64 {
    if cfg!(target_arch = "x86_64") {
        ARCH_X86_64
    } else if cfg!(target_arch = "aarch64") {
        ARCH_AARCH64
    } else if cfg!(target_arch = "arm") {
        ARCH_ARM
    } else if cfg!(target_arch = "riscv64") {
        ARCH_RISCV64
    } else if cfg!(target_arch = "wasm32") {
        ARCH_WASM32
    } else {
        ARCH_UNKNOWN
    }
}

fn os_variant_name(index: usize) -> &'static str {
    match index {
        0 => "Linux",
        1 => "Macos",
        2 => "Windows",
        _ => "Unknown",
    }
}

fn arch_variant_name(index: usize) -> &'static str {
    match index {
        0 => "X86_64",
        1 => "Aarch64",
        2 => "Arm",
        3 => "Riscv64",
        4 => "Wasm32",
        _ => "Unknown",
    }
}

fn intern_structured_target(
    store: &RefCell<ValueStore>,
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    os_index: u64,
    arch_index: u64,
) -> Result<(i64, bool), String> {
    let os_descriptor = descriptors
        .get("Os")
        .ok_or_else(|| "missing Os descriptor".to_string())?;
    let mut os_bytes = vec![0u8; os_descriptor.layout.size];
    zero_inactive_enum_payload(
        &mut os_bytes,
        os_descriptor,
        usize::try_from(os_index).expect("os index fits usize"),
    );
    write_variant_tag(&mut os_bytes, os_descriptor, os_index);
    let os = store
        .borrow_mut()
        .alloc("Os", os_bytes, descriptors, schemas)
        .0;

    let arch_descriptor = descriptors
        .get("Arch")
        .ok_or_else(|| "missing Arch descriptor".to_string())?;
    let mut arch_bytes = vec![0u8; arch_descriptor.layout.size];
    zero_inactive_enum_payload(
        &mut arch_bytes,
        arch_descriptor,
        usize::try_from(arch_index).expect("arch index fits usize"),
    );
    write_variant_tag(&mut arch_bytes, arch_descriptor, arch_index);
    let arch = store
        .borrow_mut()
        .alloc("Arch", arch_bytes, descriptors, schemas)
        .0;

    let target_descriptor = descriptors
        .get("Target")
        .ok_or_else(|| "missing Target descriptor".to_string())?;
    let mut target_bytes = vec![0u8; target_descriptor.layout.size];
    let os_offset = field_offset(target_descriptor, &target_bytes, 0);
    write_canonical_word_field(&mut target_bytes, os_offset, 8, os);
    let arch_offset = field_offset(target_descriptor, &target_bytes, 1);
    write_canonical_word_field(&mut target_bytes, arch_offset, 8, arch);
    Ok(store
        .borrow_mut()
        .alloc("Target", target_bytes, descriptors, schemas))
}

fn target_hash(store: &RefCell<ValueStore>, handle: i64) -> Result<u64, String> {
    let store = store.borrow();
    let entry = store
        .entry(handle)
        .ok_or_else(|| format!("store handle {handle}"))?;
    if entry.schema != "Target" {
        return Err(format!("handle {handle} is `{}`, not Target", entry.schema));
    }
    if entry.bytes.len() != 8 && entry.bytes.len() != 16 {
        return Err(format!("Target entry has {} bytes", entry.bytes.len()));
    }
    let os_word = i64::from_le_bytes(entry.bytes[..8].try_into().expect("target os bytes"));
    if let Some(os) = store.entry(os_word)
        && os.schema == "Os"
    {
        let index = usize::from(*os.bytes.first().ok_or("empty Os entry")?);
        let arch = if entry.bytes.len() >= 16 {
            let arch_word =
                i64::from_le_bytes(entry.bytes[8..16].try_into().expect("target arch bytes"));
            match store.entry(arch_word) {
                Some(arch) if arch.schema == "Arch" => {
                    usize::from(*arch.bytes.first().ok_or("empty Arch entry")?)
                }
                _ => usize::try_from(host_arch_index()).expect("host arch index fits usize"),
            }
        } else {
            usize::try_from(host_arch_index()).expect("host arch index fits usize")
        };
        let value = Value::Struct {
            name: "Target".into(),
            fields: vec![
                (
                    "os".into(),
                    Value::Variant {
                        enum_name: "Os".into(),
                        index,
                        name: os_variant_name(index).into(),
                        payload: Payload::Unit,
                    },
                ),
                (
                    "arch".into(),
                    Value::Variant {
                        enum_name: "Arch".into(),
                        index: arch,
                        name: arch_variant_name(arch).into(),
                        payload: Payload::Unit,
                    },
                ),
            ],
        };
        return Ok(value.canon_hash());
    }
    Ok(u64::from_le_bytes(
        entry.bytes[..8].try_into().expect("target hash bytes"),
    ))
}

fn cc_capability_hash(fingerprint: &str) -> u64 {
    let value = Value::Struct {
        name: "Cc".into(),
        fields: vec![("fingerprint".into(), Value::Str(fingerprint.to_string()))],
    };
    value.canon_hash()
}

fn ar_capability_hash(fingerprint: &str) -> u64 {
    let value = Value::Struct {
        name: "Ar".into(),
        fields: vec![("fingerprint".into(), Value::Str(fingerprint.to_string()))],
    };
    value.canon_hash()
}

fn cap_schema(command: &str) -> String {
    match command {
        "cc" => "Cc",
        "ar" => "Ar",
        "rustc" => "Rustc",
        "build_script" => "String",
        _ => "Cc",
    }
    .to_string()
}

fn append_command_token(token: String, argv: &mut Vec<String>) -> Result<(), String> {
    if token.starts_with(',') {
        let previous = argv
            .last_mut()
            .ok_or_else(|| format!("command affix `{token}` has no previous argument"))?;
        previous.push_str(&token);
        return Ok(());
    }
    argv.push(token);
    Ok(())
}

fn append_spliced_command_args(args: Vec<String>, argv: &mut Vec<String>) -> Result<(), String> {
    if argv.last().is_some_and(|arg| arg.ends_with('=')) {
        if args.len() != 1 {
            return Err("command affix splice must produce exactly one argument".into());
        }
        let previous = argv.last_mut().expect("checked");
        previous.push_str(&args[0]);
        return Ok(());
    }
    argv.extend(args);
    Ok(())
}

fn capability_hash(command: &str, fingerprint: &str) -> u64 {
    match command {
        "cc" => cc_capability_hash(fingerprint),
        "ar" => ar_capability_hash(fingerprint),
        _ => hash_u64(fingerprint),
    }
}

fn pending_exec_identity_hash(
    store: &ValueStore,
    command: &str,
    plan: &crate::exec::ExecPlan,
    capability: u64,
    mounts: &[ExecMount],
) -> ContentHash {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vix-pending-exec");
    hasher.update(command.as_bytes());
    hasher.update(plan.identity_hash().as_ref());
    hasher.update(&capability.to_le_bytes());
    for mount in mounts {
        match mount {
            ExecMount::Concrete(mount) => {
                hasher.update(&[0]);
                hasher.update(mount.at.as_bytes());
                hasher.update(mount.tree.fingerprint().as_ref());
            }
            ExecMount::PendingTree { at, tree } => {
                hasher.update(&[1]);
                hasher.update(at.as_bytes());
                if let Some(entry) = store.entry(*tree) {
                    hasher.update(entry.content_hash.as_ref());
                } else {
                    hasher.update(&tree.to_le_bytes());
                }
            }
        }
    }
    finish_hash(hasher)
}

fn descriptor_supports_flat_identity(descriptor: &VixDescriptor) -> bool {
    match &descriptor.access {
        Access::Scalar => true,
        Access::Record(record) => record
            .fields
            .iter()
            .all(|field| descriptor_supports_flat_identity(&field.descriptor)),
        Access::Enum(access) => access.variants.iter().all(|variant| {
            variant
                .payload
                .fields
                .iter()
                .all(|field| descriptor_supports_flat_identity(&field.descriptor))
        }),
        _ => false,
    }
}

fn hash_value_bytes(
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    descriptor: &VixDescriptor,
    bytes: &[u8],
    store: &ValueStore,
) -> ContentHash {
    let mut hasher = blake3::Hasher::new();
    hash_value_into(descriptors, schemas, &mut hasher, descriptor, bytes, store);
    finish_hash(hasher)
}

fn hash_value_into(
    descriptors: &DescriptorMap,
    schemas: &SchemaTables,
    hasher: &mut blake3::Hasher,
    descriptor: &VixDescriptor,
    bytes: &[u8],
    store: &ValueStore,
) {
    hasher.update(b"vix-value");
    let schema = schemas.display_ref(&descriptor.schema);
    update_schema_ref(hasher, &descriptor.schema);
    match &descriptor.access {
        Access::Scalar if schemas.is_primitive(&schema, Primitive::F64) => {
            let word = read_frame_word(bytes, 0);
            hasher.update(&canonicalize_word_for_schema(schemas, &schema, word).to_le_bytes());
        }
        Access::Scalar => {
            hasher.update(bytes);
        }
        Access::Handle { target } => {
            let handle = read_frame_word(bytes, 0);
            let entry = store
                .entry(handle)
                .unwrap_or_else(|| panic!("store handle {handle}"));
            let target = schemas.display_ref(target);
            assert_eq!(&entry.schema, &target, "handle target schema");
            hasher.update(entry.content_hash.as_ref());
        }
        Access::Record(record) => {
            hasher.update(b"record");
            for field in &record.fields {
                let start = field.offset;
                let end = start + field.descriptor.layout.size;
                hash_value_into(
                    descriptors,
                    schemas,
                    hasher,
                    &field.descriptor,
                    &bytes[start..end],
                    store,
                );
            }
        }
        Access::Enum(access) => {
            let selector = read_variant_tag(bytes, descriptor);
            hasher.update(b"enum");
            hasher.update(&selector.to_le_bytes());
            let variant = access
                .variants
                .iter()
                .find(|variant| variant.selector == selector)
                .unwrap_or_else(|| panic!("enum selector {selector}"));
            for field in &variant.payload.fields {
                let start = field.offset;
                let end = start + field.descriptor.layout.size;
                hash_value_into(
                    descriptors,
                    schemas,
                    hasher,
                    &field.descriptor,
                    &bytes[start..end],
                    store,
                );
            }
        }
        Access::Array {
            element,
            count,
            stride,
        } => {
            hasher.update(b"array");
            for i in 0..*count {
                let start = i * *stride;
                let end = start + element.layout.size;
                hash_value_into(
                    descriptors,
                    schemas,
                    hasher,
                    element,
                    &bytes[start..end],
                    store,
                );
            }
        }
        Access::Sequence(sequence) => {
            let SequenceStorage::Thunk { .. } = &sequence.storage else {
                panic!(
                    "sequence storage {:?} is outside vix store-byte canonicalization",
                    sequence.storage
                );
            };
            let kind = read_frame_word(bytes, 0);
            let count = usize::try_from(read_frame_word(bytes, 16))
                .unwrap_or_else(|_| panic!("sequence count for {schema}"));
            hasher.update(b"sequence");
            hasher.update(&kind.to_le_bytes());
            update_hash_len(hasher, count);
            for i in 0..count {
                let word = read_frame_word(bytes, 24 + i * 8);
                let elem_schema = descriptor_word_schema(schemas, &sequence.element);
                hasher.update(
                    canonical_word_hash_in_store(store, schemas, &elem_schema, word).as_ref(),
                );
            }
        }
        Access::Map(map) => {
            let MapStorage::Thunk { .. } = &map.storage else {
                panic!(
                    "map storage {:?} is outside vix store-byte canonicalization",
                    map.storage
                );
            };
            hasher.update(b"map");
            let pairs = decode_map_pairs(bytes, schemas).unwrap_or_else(|err| {
                panic!("map descriptor hashing failed for `{schema}`: {err}")
            });
            let ordered = ordered_map_pairs_from_decoded(store, pairs, schemas);
            let already_canonical = ordered.windows(2).all(|window| {
                compare_ordered_map_pairs(
                    store,
                    descriptors,
                    schemas,
                    schemas,
                    &window[0],
                    &window[1],
                )
                .is_ok_and(|ordering| ordering == Ordering::Less)
            });
            let ordered = if already_canonical {
                ordered
            } else {
                let pairs = ordered.into_iter().map(|pair| pair.pair).collect();
                canonical_map_pairs(store, pairs, descriptors, schemas, schemas).unwrap_or_else(
                    |err| panic!("map descriptor canonicalization failed for `{schema}`: {err}"),
                )
            };
            hasher.update(hash_map_pairs(&schema, &ordered, schemas).as_ref());
        }
        Access::Option(option) => {
            hasher.update(b"option");
            match &option.presence {
                Presence::Tag {
                    offset,
                    width,
                    none_value,
                } => {
                    let tag = read_presence_tag(bytes, *offset, *width);
                    hasher.update(&tag.to_le_bytes());
                    if tag != *none_value {
                        let payload_start = offset + width;
                        let payload_end = payload_start + option.some.layout.size;
                        hash_value_into(
                            descriptors,
                            schemas,
                            hasher,
                            &option.some,
                            &bytes[payload_start..payload_end],
                            store,
                        );
                    }
                }
                Presence::Niche {
                    offset,
                    width,
                    none_pattern,
                } => {
                    let end = offset + width;
                    let is_some = &bytes[*offset..end] != none_pattern.as_slice();
                    hasher.update(&u64::from(is_some).to_le_bytes());
                    if is_some {
                        hash_value_into(descriptors, schemas, hasher, &option.some, bytes, store);
                    }
                }
                Presence::Thunk { .. } | Presence::Vtable(_) => {
                    panic!(
                        "option presence {:?} is outside vix store-byte canonicalization",
                        option.presence
                    );
                }
            }
        }
        other => {
            panic!(
                "descriptor access {other:?} is outside vix machine value-store canonicalization"
            );
        }
    }
}

fn read_presence_tag(bytes: &[u8], offset: usize, width: usize) -> u64 {
    match width {
        1 => bytes[offset].into(),
        2 => u16::from_le_bytes(bytes[offset..offset + 2].try_into().expect("presence tag")).into(),
        4 => u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("presence tag")).into(),
        8 => u64::from_le_bytes(bytes[offset..offset + 8].try_into().expect("presence tag")),
        _ => panic!("invalid option presence tag width {width}"),
    }
}

fn assert_canonical_zero_padding(schema: &str, descriptor: &VixDescriptor, bytes: &[u8]) {
    assert_eq!(
        bytes.len(),
        descriptor.layout.size,
        "canonical-zero canary requires full `{schema}` bytes"
    );
    match &descriptor.access {
        Access::Record(record) => {
            for range in &record.byte_ownership.ranges {
                if range.owner != weavy::mem::ByteOwner::Padding {
                    continue;
                }
                let end = range
                    .offset
                    .checked_add(range.len)
                    .expect("padding range end");
                assert!(
                    bytes[range.offset..end].iter().all(|byte| *byte == 0),
                    "canonical-zero canary failed for `{schema}` padding {}..{}",
                    range.offset,
                    end
                );
            }
        }
        Access::Enum(access) => {
            let selector = read_variant_tag(bytes, descriptor);
            let variant = access
                .variants
                .iter()
                .find(|variant| variant.selector == selector)
                .unwrap_or_else(|| panic!("enum selector {selector}"));
            for (offset, byte) in bytes.iter().enumerate() {
                if enum_tag_owns_byte(access, offset)
                    || variant
                        .payload
                        .fields
                        .iter()
                        .any(|field| field_owns_byte(field, offset))
                {
                    continue;
                }
                assert_eq!(
                    *byte, 0,
                    "canonical-zero canary failed for `{schema}` inactive enum byte {offset}"
                );
            }
        }
        _ => {}
    }
}

fn enum_tag_owns_byte<SchemaRef>(
    access: &weavy::mem::EnumAccess<SchemaRef>,
    offset: usize,
) -> bool {
    match access.tag {
        Tag::Direct {
            offset: tag_offset,
            width,
        } => offset >= tag_offset && offset < tag_offset + width,
        Tag::Niche { .. } | Tag::Thunk { .. } => false,
    }
}

fn field_owns_byte<SchemaRef>(field: &weavy::mem::FieldAccess<SchemaRef>, offset: usize) -> bool {
    let end = field
        .offset
        .checked_add(field.descriptor.layout.size)
        .expect("field end");
    offset >= field.offset && offset < end
}

fn zero_inactive_enum_payload(bytes: &mut [u8], descriptor: &VixDescriptor, variant_index: usize) {
    let Access::Enum(access) = &descriptor.access else {
        return;
    };
    let variant = access
        .variants
        .get(variant_index)
        .unwrap_or_else(|| panic!("variant index {variant_index}"));
    for (offset, byte) in bytes.iter_mut().enumerate() {
        if enum_tag_owns_byte(access, offset)
            || variant
                .payload
                .fields
                .iter()
                .any(|field| field_owns_byte(field, offset))
        {
            continue;
        }
        *byte = 0;
    }
}

fn write_canonical_word_field(bytes: &mut [u8], offset: usize, field_size: usize, value: i64) {
    bytes[offset..offset + field_size].fill(0);
    bytes[offset..offset + field_size].copy_from_slice(&value.to_le_bytes()[..field_size]);
}

fn write_variant_tag(bytes: &mut [u8], descriptor: &VixDescriptor, selector: u64) {
    let Access::Enum(access) = &descriptor.access else {
        return;
    };
    let Tag::Direct { offset, width } = access.tag else {
        panic!("vix machine value store only supports direct enum tags");
    };
    match width {
        0 => {}
        1 => bytes[offset] = u8::try_from(selector).expect("selector fits u8"),
        2 => bytes[offset..offset + 2].copy_from_slice(
            &u16::try_from(selector)
                .expect("selector fits u16")
                .to_le_bytes(),
        ),
        4 => bytes[offset..offset + 4].copy_from_slice(
            &u32::try_from(selector)
                .expect("selector fits u32")
                .to_le_bytes(),
        ),
        8 => bytes[offset..offset + 8].copy_from_slice(&selector.to_le_bytes()),
        _ => panic!("invalid enum tag width {width}"),
    }
}

fn read_variant_tag(bytes: &[u8], descriptor: &VixDescriptor) -> u64 {
    let Access::Enum(access) = &descriptor.access else {
        panic!(
            "STORE_TAG used on non-enum schema `{:?}`",
            descriptor.schema
        );
    };
    let Tag::Direct { offset, width } = access.tag else {
        panic!("vix machine value store only supports direct enum tags");
    };
    match width {
        0 => 0,
        1 => bytes[offset].into(),
        2 => u16::from_le_bytes(bytes[offset..offset + 2].try_into().expect("tag")).into(),
        4 => u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("tag")).into(),
        8 => u64::from_le_bytes(bytes[offset..offset + 8].try_into().expect("tag")),
        _ => panic!("invalid enum tag width {width}"),
    }
}

fn write_alloc_fields(
    bytes: &mut [u8],
    schemas: &SchemaTables,
    descriptor: &VixDescriptor,
    variant_index: usize,
    field_count: usize,
    frame: &[u8],
    field_base: usize,
) {
    let fields = match &descriptor.access {
        Access::Record(record) => &record.fields,
        Access::Enum(access) => {
            &access
                .variants
                .get(variant_index)
                .unwrap_or_else(|| panic!("variant index {variant_index}"))
                .payload
                .fields
        }
        other => panic!("STORE_ALLOC for access {other:?}"),
    };
    assert_eq!(fields.len(), field_count, "STORE_ALLOC field count");
    for (i, field) in fields.iter().enumerate() {
        let value = canonicalize_word_for_schema(
            schemas,
            &schemas.display_ref(&field.descriptor.schema),
            read_frame_word(frame, field_base + i * 8),
        );
        let dst = field.offset;
        let field_size = field.descriptor.layout.size;
        assert!(
            field_size <= 8,
            "STORE_ALLOC field {} has {} bytes; slice-2 stores word fields only",
            i,
            field_size
        );
        write_canonical_word_field(bytes, dst, field_size, value);
    }
}

fn field_offset(descriptor: &VixDescriptor, bytes: &[u8], field_index: usize) -> usize {
    match &descriptor.access {
        Access::Record(record) => {
            record
                .fields
                .get(field_index)
                .unwrap_or_else(|| panic!("field index {field_index}"))
                .offset
        }
        Access::Enum(access) => {
            let selector = read_variant_tag(bytes, descriptor);
            let variant = access
                .variants
                .iter()
                .find(|variant| variant.selector == selector)
                .unwrap_or_else(|| panic!("enum selector {selector}"));
            variant
                .payload
                .fields
                .get(field_index)
                .unwrap_or_else(|| panic!("field index {field_index}"))
                .offset
        }
        other => panic!("STORE_READ for access {other:?}"),
    }
}

fn field_descriptor<'a>(
    descriptor: &'a VixDescriptor,
    bytes: &[u8],
    field_index: usize,
) -> &'a VixDescriptor {
    match &descriptor.access {
        Access::Record(record) => {
            &record
                .fields
                .get(field_index)
                .unwrap_or_else(|| panic!("field index {field_index}"))
                .descriptor
        }
        Access::Enum(access) => {
            let selector = read_variant_tag(bytes, descriptor);
            let variant = access
                .variants
                .iter()
                .find(|variant| variant.selector == selector)
                .unwrap_or_else(|| panic!("enum selector {selector}"));
            &variant
                .payload
                .fields
                .get(field_index)
                .unwrap_or_else(|| panic!("field index {field_index}"))
                .descriptor
        }
        other => panic!("STORE_READ for access {other:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        Mutex,
        atomic::{AtomicUsize, Ordering},
    };
    use std::time::{Duration, Instant};
    use weavy::mem::Layout;
    use weavy::mem::declared as declared_mem;
    use weavy::task::{ArgCopy, Fn as TaskFn, Op};

    fn memo_key_for_test(fn_hash: u64, arg: u8) -> CanonMemoKey {
        (fn_hash, vec![ContentHash::from([arg; 32])])
    }

    struct LatencyExecBackend {
        delay: Duration,
    }

    impl MachineExecBackend for LatencyExecBackend {
        fn spawn(&self, request: MachineExecRequest) -> Result<Arc<dyn MachinePendingRun>, String> {
            Ok(Arc::new(LatencyExecRun {
                request,
                delay: self.delay,
                completed: Mutex::new(None),
            }))
        }
    }

    struct LatencyExecRun {
        request: MachineExecRequest,
        delay: Duration,
        completed: Mutex<Option<(crate::exec::Outcome, crate::exec::ExecEvent)>>,
    }

    impl MachinePendingRun for LatencyExecRun {
        fn demand_path(&self, path: &str) -> Result<MachinePathDemand, String> {
            let completed = self
                .completed
                .lock()
                .map_err(|_| "latency run state poisoned".to_string())?;
            let Some((outcome, _)) = &*completed else {
                return Ok(MachinePathDemand::FinishRequired {
                    path: path.to_string(),
                });
            };
            if let Some(contents) = outcome.outputs.entries.get(path) {
                Ok(MachinePathDemand::File(contents.clone()))
            } else {
                Ok(MachinePathDemand::Missing {
                    path: path.to_string(),
                })
            }
        }

        fn flush(&self) -> Result<(crate::exec::Outcome, crate::exec::ExecEvent), String> {
            if let Some(completed) = self
                .completed
                .lock()
                .map_err(|_| "latency run state poisoned".to_string())?
                .clone()
            {
                return Ok(completed);
            }
            std::thread::sleep(self.delay);
            let mut tree = crate::exec::Tree::default();
            tree.entries.insert(
                self.request.output.clone(),
                format!("{};", self.request.output),
            );
            let completed = (
                crate::exec::Outcome {
                    outputs: tree,
                    read_set: crate::exec::ReadSet {
                        entries: BTreeMap::new(),
                    },
                    tree_events: Vec::new(),
                },
                crate::exec::ExecEvent::Ran,
            );
            *self
                .completed
                .lock()
                .map_err(|_| "latency run state poisoned".to_string())? = Some(completed.clone());
            Ok(completed)
        }
    }

    struct CountingFetchBackend {
        calls: Arc<AtomicUsize>,
        delay: Duration,
    }

    impl FetchBackend for CountingFetchBackend {
        fn fetch(
            &self,
            _url: &str,
            _expected_sha256: Option<&str>,
        ) -> Result<crate::fetch::FetchOutput, String> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            std::thread::sleep(self.delay);
            Ok(crate::fetch::FetchOutput {
                tree: crate::exec::Tree::of(&[("payload.txt", "fetch;")]),
                actual_sha256: "fake-fetch-sha256".to_string(),
            })
        }
    }

    fn doc_observation_test_tables() -> (DescriptorMap, SchemaTables) {
        let tables = crate::module::load_module_tables_from_modules(
            "root",
            &BTreeMap::from([("root".to_string(), "pub fn main() -> Int { 0 }".to_string())]),
        )
        .unwrap();
        let mut schemas = tables.schemas;
        schemas.register_frame_names([
            "String".to_string(),
            "Doc".to_string(),
            "Map<String,Doc>".to_string(),
            "Realized<Doc>".to_string(),
            "Option<Realized<Doc>>".to_string(),
        ]);
        let mut descriptors = tables.descriptors;
        descriptors.insert_named(
            &schemas,
            "Doc",
            declared_mem::declared_enum(
                schemas.legacy_ref("Doc"),
                vec![
                    vec![],
                    vec![declared_mem::i64_(schemas.legacy_ref("DocBool"))],
                    vec![declared_mem::i64_(schemas.legacy_ref("DocInt"))],
                    vec![declared_mem::f64_(schemas.legacy_ref("DocFloat"))],
                    vec![declared_mem::handle(
                        schemas.legacy_ref("DocString"),
                        schemas.legacy_ref("String"),
                    )],
                    vec![declared_mem::handle(
                        schemas.legacy_ref("DocArray"),
                        schemas.legacy_ref("Array<Doc>"),
                    )],
                    vec![declared_mem::handle(
                        schemas.legacy_ref("DocMap"),
                        schemas.legacy_ref("Map<String,Doc>"),
                    )],
                    vec![declared_mem::handle(
                        schemas.legacy_ref("DocVirtual"),
                        schemas.legacy_ref("String"),
                    )],
                    vec![declared_mem::handle(
                        schemas.legacy_ref("DocBlob"),
                        schemas.legacy_ref("Blob"),
                    )],
                ],
            ),
        );
        (descriptors, schemas)
    }

    #[test]
    fn doc_get_observation_direct_hash_matches_allocating_path() {
        let (descriptors, schemas) = doc_observation_test_tables();
        let store = RefCell::new(ValueStore::default());
        let doc = alloc_doc_from_value(
            &store,
            &descriptors,
            &schemas,
            &schemas,
            Value::Map(BTreeMap::from([(
                Value::Str("name".to_string()),
                Value::Str("facet".to_string()),
            )])),
        )
        .unwrap();

        for key in ["name", "missing"] {
            let allocating = doc_get_observation_hash_via_allocation(
                &store.borrow(),
                &descriptors,
                &schemas,
                &schemas,
                doc,
                key,
                None,
            )
            .unwrap();
            let direct = doc_get_observation_hash(
                &mut store.borrow_mut(),
                &descriptors,
                &schemas,
                &schemas,
                doc,
                key,
            )
            .unwrap();
            assert_eq!(direct, allocating, "{key}");
        }
    }

    #[test]
    fn in_flight_rejects_duplicate_registration_until_finish() {
        let key = memo_key_for_test(1, 7);
        let mut in_flight = InFlightInvocations::default();

        assert!(!in_flight.is_running(&key));
        assert!(in_flight.started(key.clone()));
        assert!(in_flight.is_running(&key));
        assert!(!in_flight.started(key.clone()));
        assert!(in_flight.is_running(&key));
        assert!(in_flight.finished(&key));
        assert!(!in_flight.finished(&key));
        assert!(!in_flight.is_running(&key));
    }

    #[test]
    fn in_flight_finish_reopens_key_for_later_invocation() {
        let key = memo_key_for_test(1, 7);
        let other = memo_key_for_test(1, 8);
        let mut in_flight = InFlightInvocations::default();

        assert!(in_flight.started(key.clone()));
        assert!(in_flight.started(other.clone()));
        assert!(in_flight.finished(&key));

        assert!(!in_flight.is_running(&key));
        assert!(in_flight.is_running(&other));

        assert!(in_flight.started(key.clone()));
        assert!(in_flight.is_running(&key));
    }

    fn seed_memo(driver: &mut Driver, key: CanonMemoKey, value: i64) {
        let args = key.1.iter().map(|_| 0).collect();
        driver.memo.insert(
            key,
            MemoEntry {
                value,
                args,
                read_set: ProjectionReadSet::default(),
            },
        );
    }

    fn carried_array_hash_for_test(
        store: &ValueStore,
        schemas: &SchemaTables,
        carried_hash: Option<CarriedArrayHasher>,
        words: &[i64],
    ) -> ContentHash {
        finish_array_element_hash(
            carried_hash.unwrap_or_else(|| {
                recompute_array_element_hasher(
                    store,
                    schemas,
                    schemas,
                    b"vix-array-words",
                    "Int",
                    words,
                )
            }),
            words.len(),
        )
    }

    fn assert_array_hash_matches_recomputed(
        store: &ValueStore,
        schemas: &SchemaTables,
        carried_hash: &Option<CarriedArrayHasher>,
        words: &[i64],
    ) {
        let carried = carried_array_hash_for_test(store, schemas, carried_hash.clone(), words);
        let recomputed = finish_array_element_hash(
            recompute_array_element_hasher(
                store,
                schemas,
                schemas,
                b"vix-array-words",
                "Int",
                words,
            ),
            words.len(),
        );
        assert_eq!(carried, recomputed, "carried array hash drift");

        let mut allocated_store = ValueStore::default();
        let (handle, _) = allocated_store
            .alloc_array_words("Int", words.to_vec(), schemas, schemas)
            .expect("array alloc");
        let allocated = allocated_store
            .entry(handle)
            .expect("allocated array handle")
            .content_hash;
        assert_eq!(allocated, recomputed, "allocated array hash drift");
    }

    fn map_pair_for_test(
        store: &mut ValueStore,
        schemas: &SchemaTables,
        key: &str,
        value: i64,
    ) -> MapPair {
        let key_word = store
            .alloc_raw("String", key.as_bytes().to_vec(), schemas)
            .0;
        MapPair {
            key_schema: "String".to_string(),
            key_word,
            value_schema: "Int".to_string(),
            value_word: value,
            value_realization: None,
        }
    }

    #[test]
    fn carried_map_rows_allocate_with_recomputed_hash() {
        let mut store = ValueStore::default();
        let schemas = SchemaTables::empty();
        let descriptors = DescriptorMap::new();
        let schema = "Map<String,Int>";
        let mut carried = CarriedMapRows {
            ordered: Vec::new(),
        };
        let mut pairs = Vec::new();

        for (key, value) in [("b", 2), ("a", 1), ("b", 3), ("c", 4)] {
            let pair = map_pair_for_test(&mut store, &schemas, key, value);
            carried = carried_map_rows_after_insert(
                &store,
                &descriptors,
                &schemas,
                &schemas,
                carried,
                pair.clone(),
            )
            .expect("carried map insert");
            pairs.push(pair);

            let (recomputed_handle, _) = store
                .alloc_map(schema, pairs.clone(), &schemas, &descriptors, &schemas)
                .expect("recomputed map alloc");
            let recomputed = store
                .entry(recomputed_handle)
                .expect("recomputed map entry")
                .content_hash;
            let (carried_handle, _) = store
                .alloc_map_with_carried_rows(
                    schema,
                    pairs.clone(),
                    Some(carried.clone()),
                    &schemas,
                    &descriptors,
                    &schemas,
                )
                .expect("carried map alloc");
            let carried_hash = store
                .entry(carried_handle)
                .expect("carried map entry")
                .content_hash;
            assert_eq!(carried_hash, recomputed, "{key}={value}");
        }
    }

    #[test]
    fn store_map_insert_carries_sorted_rows_to_final_intern() {
        let module_tables = crate::module::load_module_tables_from_modules(
            "root",
            &BTreeMap::from([(
                "root".to_string(),
                "pub fn main() -> Map<String, Int> { {} }".to_string(),
            )]),
        )
        .expect("module tables");
        let schemas = module_tables.schemas;
        let descriptors = module_tables.descriptors;
        let mut store = ValueStore::default();
        let schema = "Map<String,Int>";
        let base_pairs = vec![
            map_pair_for_test(&mut store, &schemas, "b", 2),
            map_pair_for_test(&mut store, &schemas, "a", 1),
        ];
        let (base_handle, _) = store
            .alloc_map(schema, base_pairs.clone(), &schemas, &descriptors, &schemas)
            .expect("base map");

        let new_pair = map_pair_for_test(&mut store, &schemas, "c", 3);
        let mut expected_pairs = base_pairs;
        expected_pairs.push(new_pair.clone());
        let (expected_handle, _) = store
            .alloc_map(
                schema,
                expected_pairs.clone(),
                &schemas,
                &descriptors,
                &schemas,
            )
            .expect("expected map");
        let expected_hash = store
            .entry(expected_handle)
            .expect("expected map entry")
            .content_hash;

        let (_, mut carried_pairs, carried) = store
            .map_pairs_with_carried_rows_cached(base_handle, &schemas, &schemas)
            .expect("cached rows");
        carried_pairs.push(new_pair.clone());
        let carried = carried_map_rows_after_insert(
            &store,
            &descriptors,
            &schemas,
            &schemas,
            carried,
            new_pair,
        )
        .expect("carried insert");
        let before_final = store.map_intern_stats();
        let (carried_handle, _) = store
            .alloc_map_with_carried_rows(
                schema,
                carried_pairs,
                Some(carried),
                &schemas,
                &descriptors,
                &schemas,
            )
            .expect("carried map");
        let after_final = store.map_intern_stats();
        let carried_hash = store
            .entry(carried_handle)
            .expect("carried map entry")
            .content_hash;

        assert_eq!(carried_hash, expected_hash);
        assert_eq!(
            after_final.rows_canonicalized, before_final.rows_canonicalized,
            "final intern must not rebuild canonical map rows"
        );
        assert_eq!(
            after_final.sort_rows, before_final.sort_rows,
            "final intern must not sort carried map rows"
        );
        assert_eq!(
            after_final.sort_comparisons, before_final.sort_comparisons,
            "final intern must not compare carried map rows"
        );
        assert_eq!(
            after_final.hash_rows,
            before_final.hash_rows + 3,
            "final intern should only hash the ordered rows"
        );
    }

    #[test]
    fn carried_map_insert_distinguishes_tainted_equal_plaintext_keys() {
        let module_tables = crate::module::load_module_tables_from_modules(
            "root",
            &BTreeMap::from([(
                "root".to_string(),
                "pub fn main() -> Map<String, Int> { {} }".to_string(),
            )]),
        )
        .expect("module tables");
        let schemas = module_tables.schemas;
        let descriptors = module_tables.descriptors;
        let mut store = ValueStore::default();
        let schema = "Map<String,Int>";
        let taint_a = StructuralTaint {
            marker: "secret.a".to_string(),
            recipient: "test".to_string(),
            identity_hash: b"a".to_vec(),
            content_tag: None,
        };
        let taint_b = StructuralTaint {
            marker: "secret.b".to_string(),
            recipient: "test".to_string(),
            identity_hash: b"b".to_vec(),
            content_tag: None,
        };
        let key_a = store
            .alloc_raw_tainted("String", b"same".to_vec(), &schemas, Some(taint_a))
            .0;
        let key_b = store
            .alloc_raw_tainted("String", b"same".to_vec(), &schemas, Some(taint_b))
            .0;
        let pair_a_one = MapPair {
            key_schema: "String".to_string(),
            key_word: key_a,
            value_schema: "Int".to_string(),
            value_word: 1,
            value_realization: None,
        };
        let pair_b = MapPair {
            key_schema: "String".to_string(),
            key_word: key_b,
            value_schema: "Int".to_string(),
            value_word: 2,
            value_realization: None,
        };
        let pair_a_two = MapPair {
            value_word: 3,
            ..pair_a_one.clone()
        };
        let pairs = vec![pair_a_one, pair_b, pair_a_two];
        let recomputed =
            canonical_map_pairs(&store, pairs.clone(), &descriptors, &schemas, &schemas)
                .expect("canonical pairs");
        let mut carried = CarriedMapRows {
            ordered: Vec::new(),
        };
        for pair in pairs.clone() {
            carried = carried_map_rows_after_insert(
                &store,
                &descriptors,
                &schemas,
                &schemas,
                carried,
                pair,
            )
            .expect("carried insert");
        }

        assert_eq!(carried.ordered.len(), 2);
        assert_eq!(
            carried
                .ordered
                .iter()
                .map(|row| (row.key_hash, row.pair.value_word))
                .collect::<Vec<_>>(),
            recomputed
                .iter()
                .map(|row| (row.key_hash, row.pair.value_word))
                .collect::<Vec<_>>()
        );
        let (recomputed_handle, _) = store
            .alloc_map(schema, pairs.clone(), &schemas, &descriptors, &schemas)
            .expect("recomputed map");
        let (carried_handle, _) = store
            .alloc_map_with_carried_rows(
                schema,
                carried.ordered.iter().map(|row| row.pair.clone()).collect(),
                Some(carried),
                &schemas,
                &descriptors,
                &schemas,
            )
            .expect("carried map");
        assert_eq!(
            store.entry(carried_handle).expect("carried").content_hash,
            store
                .entry(recomputed_handle)
                .expect("recomputed")
                .content_hash
        );
    }

    #[test]
    fn carried_array_hash_matches_recomputed_after_random_ops() {
        let store = ValueStore::default();
        let schemas = SchemaTables::empty();
        let mut words = Vec::new();
        let mut carried_hash: Option<CarriedArrayHasher> = None;
        let mut seed = 0x9e37_79b9_7f4a_7c15_u64;

        for step in 0..256 {
            seed = seed
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let op = if words.is_empty() { 0 } else { seed % 5 };
            match op {
                0 | 1 => {
                    let word = (seed.rotate_left(17) as i64) ^ i64::from(step);
                    if words.is_empty() {
                        carried_hash = Some(start_array_element_hasher(
                            b"vix-array-words",
                            &schemas,
                            "Int",
                        ));
                    } else if carried_hash.is_none() {
                        carried_hash = Some(recompute_array_element_hasher(
                            &store,
                            &schemas,
                            &schemas,
                            b"vix-array-words",
                            "Int",
                            &words,
                        ));
                    }
                    let element_hash = canonical_word_hash_in_store(&store, &schemas, "Int", word);
                    update_array_element_hash(
                        carried_hash.as_mut().expect("array hash seeded"),
                        element_hash,
                    );
                    words.push(word);
                }
                2 => {
                    words.pop().expect("non-empty array");
                    carried_hash = None;
                }
                _ => {
                    let index = usize::try_from(seed % u64::try_from(words.len()).unwrap())
                        .expect("index fits usize");
                    words[index] = (seed.rotate_right(11) as i64) ^ -i64::from(step);
                    carried_hash = None;
                }
            }

            assert_array_hash_matches_recomputed(&store, &schemas, &carried_hash, &words);
        }
    }

    #[test]
    fn inactive_enum_payload_is_zeroed_before_retag() {
        let schemas = SchemaTables::empty();
        let descriptor = declared_mem::declared_enum(
            schemas.legacy_ref("Choice"),
            vec![
                vec![declared_mem::i64_(schemas.legacy_ref("Wide"))],
                vec![declared_mem::bool_(schemas.legacy_ref("Narrow"))],
            ],
        );
        let mut bytes = vec![0u8; descriptor.layout.size];
        write_variant_tag(&mut bytes, &descriptor, 0);
        let wide_offset = field_offset(&descriptor, &bytes, 0);
        write_canonical_word_field(&mut bytes, wide_offset, 8, 0x7f7e_7d7c_7b7a_7978);

        zero_inactive_enum_payload(&mut bytes, &descriptor, 1);
        write_variant_tag(&mut bytes, &descriptor, 1);
        let narrow_offset = field_offset(&descriptor, &bytes, 0);
        write_canonical_word_field(&mut bytes, narrow_offset, 1, 1);

        assert_canonical_zero_padding("Choice", &descriptor, &bytes);
        for (offset, byte) in bytes.iter().enumerate() {
            if offset == 0 || offset == narrow_offset {
                continue;
            }
            assert_eq!(*byte, 0, "inactive payload byte {offset}");
        }
    }

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
                Op::ConstI64 { dst: 8, value: 0 }, // input_slot = 0
                Op::ConstI64 { dst: 16, value: 0 }, // fn_ref = 0 (self)
                Op::ConstI64 { dst: 24, value: 1 }, // argc = 1
                Op::ConstI64 { dst: 64, value: -1 },
                Op::AddI64 {
                    dst: 32,
                    a: 0,
                    b: 64,
                }, // arg0 = n-1
                Op::HostCall { host: INVOKE_HOST },
                // request fib(n-2) into input slot 1
                Op::ConstI64 { dst: 8, value: 1 },
                Op::ConstI64 { dst: 64, value: -2 },
                Op::AddI64 {
                    dst: 32,
                    a: 0,
                    b: 64,
                }, // arg0 = n-2
                Op::HostCall { host: INVOKE_HOST },
                // await both (joint: both requests registered before
                // the first park — batched demand, not sequential)
                Op::Await { dst: 40, input: 0 },
                Op::Await { dst: 48, input: 1 },
                Op::AddI64 {
                    dst: 56,
                    a: 40,
                    b: 48,
                },
                Op::Ret { src: 56, size: 8 },
            ],
        };
        let program = Program { fns: vec![body] };
        let fns = vec![LoweredFn {
            hash: 0xF1B,
            task_fn: FnId(0),
            arg_offsets: vec![0],
            arg_schemas: vec!["Int".into()],
            return_schema: "Int".into(),
            semantic_comparators: Vec::new(),
            invoke_region: 8,
            store_alloc_region: 0,
            store_read_region: 0,
            store_tag_region: 0,
            primitive_region: 0,
        }];
        (program, fns)
    }

    fn exec_text_overlap_program() -> (Program, Vec<LoweredFn>) {
        const REGION: u32 = 128;
        let mut code = Vec::new();
        push_exec_request_ops(&mut code, REGION, 0, 0, 8, 16);
        push_exec_request_ops(&mut code, REGION, 1, 0, 8, 24);
        code.extend([
            Op::Await { dst: 48, input: 0 },
            Op::Await { dst: 56, input: 1 },
        ]);
        push_text_project_ops(&mut code, REGION, 2, 48, 32);
        push_text_project_ops(&mut code, REGION, 3, 56, 40);
        code.extend([
            Op::Await { dst: 64, input: 2 },
            Op::Await { dst: 72, input: 3 },
            Op::ConstI64 {
                dst: REGION,
                value: 80,
            },
            Op::CopyI64 {
                dst: REGION + 8,
                src: 64,
            },
            Op::CopyI64 {
                dst: REGION + 16,
                src: 72,
            },
            Op::HostCall {
                host: STRING_CONCAT_HOST,
            },
            Op::Ret { src: 80, size: 8 },
        ]);
        let body = TaskFn {
            frame: Layout {
                size: 320,
                align: 8,
            },
            code,
        };
        let program = Program { fns: vec![body] };
        let fns = vec![LoweredFn {
            hash: 0xE0EC,
            task_fn: FnId(0),
            arg_offsets: vec![0, 8, 16, 24, 32, 40],
            arg_schemas: vec![
                "Cc".into(),
                "String".into(),
                "String".into(),
                "String".into(),
                "Path".into(),
                "Path".into(),
            ],
            return_schema: "String".into(),
            semantic_comparators: Vec::new(),
            invoke_region: 0,
            store_alloc_region: 0,
            store_read_region: 0,
            store_tag_region: 0,
            primitive_region: REGION,
        }];
        (program, fns)
    }

    fn duplicate_fetch_program() -> (Program, Vec<LoweredFn>) {
        const REGION: u32 = 128;
        let mut code = Vec::new();
        push_fetch_request_ops(&mut code, REGION, 0, 0);
        push_fetch_request_ops(&mut code, REGION, 1, 0);
        code.extend([
            Op::Await { dst: 24, input: 0 },
            Op::Await { dst: 32, input: 1 },
        ]);
        push_text_project_ops(&mut code, REGION, 2, 24, 8);
        push_text_project_ops(&mut code, REGION, 3, 32, 8);
        code.extend([
            Op::Await { dst: 40, input: 2 },
            Op::Await { dst: 48, input: 3 },
            Op::ConstI64 {
                dst: REGION,
                value: 56,
            },
            Op::CopyI64 {
                dst: REGION + 8,
                src: 40,
            },
            Op::CopyI64 {
                dst: REGION + 16,
                src: 48,
            },
            Op::HostCall {
                host: STRING_CONCAT_HOST,
            },
            Op::Ret { src: 56, size: 8 },
        ]);
        let body = TaskFn {
            frame: Layout {
                size: 256,
                align: 8,
            },
            code,
        };
        let program = Program { fns: vec![body] };
        let fns = vec![LoweredFn {
            hash: 0xFE7C,
            task_fn: FnId(0),
            arg_offsets: vec![0, 8],
            arg_schemas: vec!["String".into(), "Path".into()],
            return_schema: "String".into(),
            semantic_comparators: Vec::new(),
            invoke_region: 0,
            store_alloc_region: 0,
            store_read_region: 0,
            store_tag_region: 0,
            primitive_region: REGION,
        }];
        (program, fns)
    }

    fn push_exec_request_ops(
        code: &mut Vec<Op>,
        region: u32,
        input_slot: i64,
        capability_slot: u32,
        dash_o_slot: u32,
        output_slot: u32,
    ) {
        code.extend([
            Op::ConstI64 {
                dst: region,
                value: input_slot,
            },
            Op::ConstI64 {
                dst: region + 8,
                value: 0,
            },
            Op::CopyI64 {
                dst: region + 16,
                src: capability_slot,
            },
            Op::ConstI64 {
                dst: region + 24,
                value: 2,
            },
            Op::ConstI64 {
                dst: region + 32,
                value: -1,
            },
            Op::ConstI64 {
                dst: region + 40,
                value: -1,
            },
            Op::ConstI64 {
                dst: region + 48,
                value: 0,
            },
            Op::CopyI64 {
                dst: region + 56,
                src: dash_o_slot,
            },
            Op::ConstI64 {
                dst: region + 64,
                value: 0,
            },
            Op::ConstI64 {
                dst: region + 72,
                value: 0,
            },
            Op::CopyI64 {
                dst: region + 80,
                src: output_slot,
            },
            Op::ConstI64 {
                dst: region + 88,
                value: 0,
            },
            Op::HostCall { host: EXEC_HOST },
        ]);
    }

    fn push_text_project_ops(
        code: &mut Vec<Op>,
        region: u32,
        input_slot: i64,
        tree_slot: u32,
        path_slot: u32,
    ) {
        code.extend([
            Op::ConstI64 {
                dst: region,
                value: input_slot,
            },
            Op::CopyI64 {
                dst: region + 8,
                src: tree_slot,
            },
            Op::CopyI64 {
                dst: region + 16,
                src: path_slot,
            },
            Op::HostCall {
                host: TREE_TEXT_HOST,
            },
        ]);
    }

    fn push_fetch_request_ops(code: &mut Vec<Op>, region: u32, input_slot: i64, url_slot: u32) {
        code.extend([
            Op::ConstI64 {
                dst: region,
                value: input_slot,
            },
            Op::CopyI64 {
                dst: region + 8,
                src: url_slot,
            },
            Op::ConstI64 {
                dst: region + 16,
                value: -1,
            },
            Op::HostCall { host: FETCH_HOST },
        ]);
    }

    #[test]
    fn independent_exec_text_waits_overlap() {
        let (program, fns) = exec_text_overlap_program();
        let mut driver = Driver::new(program, fns);
        let schemas = driver.schemas.clone();
        let args = {
            let mut store = driver.store.borrow_mut();
            vec![
                store.alloc_raw("Cc", b"cc-fake".to_vec(), &schemas).0,
                store.alloc_raw("String", b"-o".to_vec(), &schemas).0,
                store.alloc_raw("String", b"a.o".to_vec(), &schemas).0,
                store.alloc_raw("String", b"b.o".to_vec(), &schemas).0,
                store.alloc_raw("Path", b"a.o".to_vec(), &schemas).0,
                store.alloc_raw("Path", b"b.o".to_vec(), &schemas).0,
            ]
        };
        driver.set_exec_backend(Some(Arc::new(LatencyExecBackend {
            delay: Duration::from_millis(150),
        })));

        let started = Instant::now();
        let value = driver.demand(FnRef::new(0), args).unwrap();
        let wall = started.elapsed();

        assert_eq!(
            driver.store.borrow().string_value(value, "String").unwrap(),
            "a.o;b.o;"
        );
        assert!(
            wall < Duration::from_millis(250),
            "independent 150ms exec waits serialized instead of overlapping: {wall:?}"
        );
        assert!(
            has_two_run_starts_before_completion(&driver.trace),
            "{:?}",
            driver.trace
        );
    }

    #[test]
    fn duplicate_fetch_demands_share_one_in_flight_backend_call() {
        let (program, fns) = duplicate_fetch_program();
        let mut driver = Driver::new(program, fns);
        let calls = Arc::new(AtomicUsize::new(0));
        driver.set_fetch_backend(Arc::new(CountingFetchBackend {
            calls: Arc::clone(&calls),
            delay: Duration::from_millis(25),
        }));
        let schemas = driver.schemas.clone();
        let args = {
            let mut store = driver.store.borrow_mut();
            vec![
                store
                    .alloc_raw(
                        "String",
                        b"https://example.invalid/archive.tgz".to_vec(),
                        &schemas,
                    )
                    .0,
                store.alloc_raw("Path", b"payload.txt".to_vec(), &schemas).0,
            ]
        };

        let value = driver.demand(FnRef::new(0), args).unwrap();

        assert_eq!(
            driver.store.borrow().string_value(value, "String").unwrap(),
            "fetch;fetch;"
        );
        assert_eq!(
            calls.load(Ordering::SeqCst),
            1,
            "duplicate same-key fetch demands must join one in-flight run"
        );
    }

    fn has_two_run_starts_before_completion(trace: &[DriveEvent]) -> bool {
        let mut starts = 0;
        for event in trace {
            match event {
                DriveEvent::RunStarted { .. } => {
                    starts += 1;
                    if starts == 2 {
                        return true;
                    }
                }
                DriveEvent::RunCompleted { .. } => return false,
                _ => {}
            }
        }
        false
    }

    #[test]
    fn memo_boundaries_kill_the_exponential_tree() {
        let (program, fns) = fib_body_program();
        let mut driver = Driver::new(program, fns);
        // Base cases enter as memo facts (vix Const nodes resolve
        // without bodies).
        let zero = driver.memo_key(FnRef::new(0), &[0]);
        let one = driver.memo_key(FnRef::new(0), &[1]);
        seed_memo(&mut driver, zero, 0);
        seed_memo(&mut driver, one, 1);

        assert_eq!(driver.demand(FnRef::new(0), vec![20]).unwrap(), 6765);

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
        let zero = driver.memo_key(FnRef::new(0), &[0]);
        let one = driver.memo_key(FnRef::new(0), &[1]);
        seed_memo(&mut driver, zero, 0);
        seed_memo(&mut driver, one, 1);
        driver.demand(FnRef::new(0), vec![15]).unwrap();
        let cold_spawns = driver
            .trace
            .iter()
            .filter(|e| matches!(e, DriveEvent::Spawned { .. }))
            .count();

        driver.trace.clear();
        assert_eq!(driver.demand(FnRef::new(0), vec![15]).unwrap(), 610);
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
    fn jit_program_is_compiled_once_per_loaded_program_not_per_spawn() {
        if !weavy::jit::task_lane::available() {
            return;
        }

        reset_jit_program_compile_count();
        let (program, fns) = fib_body_program();
        let mut driver =
            Driver::try_with_descriptors(program, fns, DescriptorMap::new(), Lane::Jit).unwrap();
        assert_eq!(jit_program_compile_count(), 1, "cold load compiles once");

        let zero = driver.memo_key(FnRef::new(0), &[0]);
        let one = driver.memo_key(FnRef::new(0), &[1]);
        seed_memo(&mut driver, zero, 0);
        seed_memo(&mut driver, one, 1);
        assert_eq!(driver.demand(FnRef::new(0), vec![20]).unwrap(), 6765);

        let spawns = driver
            .trace
            .iter()
            .filter(|e| matches!(e, DriveEvent::Spawned { .. }))
            .count();
        assert_eq!(spawns, 19, "fixture must remain spawn-heavy");
        assert_eq!(
            jit_program_compile_count(),
            1,
            "spawns reuse the lane's compiled program"
        );

        let (mut reloaded_program, reloaded_fns) = fib_body_program();
        reloaded_program.fns.push(TaskFn {
            frame: Layout { size: 16, align: 8 },
            code: vec![
                Op::ConstI64 { dst: 0, value: 42 },
                Op::Ret { src: 0, size: 8 },
            ],
        });
        driver
            .reload(
                reloaded_program,
                reloaded_fns,
                DescriptorMap::new(),
                SchemaTables::empty(),
            )
            .unwrap();
        assert_eq!(
            jit_program_compile_count(),
            2,
            "reload replaces the cached compiled program"
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
            arg_schemas: vec![],
            return_schema: "Int".into(),
            semantic_comparators: Vec::new(),
            invoke_region: 8,
            store_alloc_region: 0,
            store_read_region: 0,
            store_tag_region: 0,
            primitive_region: 0,
        });
        let mut driver = Driver::new(program, fns);
        let zero = driver.memo_key(FnRef::new(0), &[0]);
        let one = driver.memo_key(FnRef::new(0), &[1]);
        seed_memo(&mut driver, zero, 0);
        seed_memo(&mut driver, one, 1);
        driver.demand(FnRef::new(0), vec![5]).unwrap();
        assert!(
            !driver.trace.iter().any(|e| matches!(
                e,
                DriveEvent::Demanded { fn_hash: 0xDEAD } | DriveEvent::Spawned { fn_hash: 0xDEAD }
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
                Op::MulI64 {
                    dst: 16,
                    a: 0,
                    b: 8,
                },
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
                        ArgCopy {
                            src: 0,
                            dst: 0,
                            size: 8,
                        },
                        ArgCopy {
                            src: 8,
                            dst: 8,
                            size: 8,
                        },
                    ],
                    ret: 16,
                },
                Op::Ret { src: 16, size: 8 },
            ],
        };
        let program = Program {
            fns: vec![body, helper],
        };
        let fns = vec![LoweredFn {
            hash: 0xAB,
            task_fn: FnId(0),
            arg_offsets: vec![0],
            arg_schemas: vec!["Int".into()],
            return_schema: "Int".into(),
            semantic_comparators: Vec::new(),
            invoke_region: 24,
            store_alloc_region: 0,
            store_read_region: 0,
            store_tag_region: 0,
            primitive_region: 0,
        }];
        let mut driver = Driver::new(program, fns);
        assert_eq!(driver.demand(FnRef::new(0), vec![6]).unwrap(), 42);
        let spawns = driver
            .trace
            .iter()
            .filter(|e| matches!(e, DriveEvent::Spawned { .. }))
            .count();
        assert_eq!(spawns, 1, "helper call is intra-task, not a driver spawn");
    }

    fn tree_descriptor(schemas: &SchemaTables) -> VixDescriptor {
        declared_mem::declared_enum(
            schemas.legacy_ref("Tree"),
            vec![
                vec![declared_mem::i64_(schemas.legacy_ref("Int"))],
                vec![
                    declared_mem::handle(schemas.legacy_ref("TreeRef"), schemas.legacy_ref("Tree")),
                    declared_mem::handle(schemas.legacy_ref("TreeRef"), schemas.legacy_ref("Tree")),
                ],
            ],
        )
    }

    fn store_driver_for(
        mut code: Vec<Op>,
        arg_offsets: Vec<u32>,
        arg_schemas: Vec<String>,
    ) -> Driver {
        let mut schemas = SchemaTables::empty();
        schemas.register_frame_names(["Tree".to_string()]);
        let tree_word = schemas.frame_word_for_name("Tree");
        for op in &mut code {
            if let Op::ConstI64 { dst: 8, value } = op
                && *value == 0
            {
                *value = tree_word;
            }
        }
        for i in 0..code.len().saturating_sub(1) {
            let next_is_molten_intern = matches!(
                code[i + 1],
                Op::HostCall {
                    host: MOLTEN_INTERN_HOST
                }
            );
            if next_is_molten_intern
                && let Op::ConstI64 { dst: 16, value } = &mut code[i]
                && *value == 0
            {
                *value = tree_word;
            }
        }
        let mut descriptors = DescriptorMap::new();
        descriptors.insert_named(&schemas, "Tree", tree_descriptor(&schemas));
        let driver = Driver::try_with_schema_tables(
            Program {
                fns: vec![TaskFn {
                    frame: Layout {
                        size: 160,
                        align: 8,
                    },
                    code,
                }],
            },
            vec![LoweredFn {
                hash: 0x51_02,
                task_fn: FnId(0),
                arg_offsets,
                arg_schemas,
                return_schema: "Int".into(),
                semantic_comparators: Vec::new(),
                invoke_region: 128,
                store_alloc_region: 0,
                store_read_region: 48,
                store_tag_region: 80,
                primitive_region: 0,
            }],
            descriptors,
            schemas,
            Lane::Interp,
        )
        .expect("interp driver");
        assert_eq!(tree_word, driver.schemas.frame_word_for_name("Tree"));
        driver
    }

    #[test]
    fn store_alloc_read_and_tag_are_sync_hosts() {
        let code = vec![
            // alloc Tree::Leaf(21) into frame slot 96.
            Op::ConstI64 { dst: 0, value: 96 },
            Op::ConstI64 { dst: 8, value: 0 },
            Op::ConstI64 { dst: 16, value: 0 },
            Op::ConstI64 { dst: 24, value: 1 },
            Op::ConstI64 { dst: 32, value: 21 },
            Op::HostCall {
                host: STORE_ALLOC_HOST,
            },
            Op::ConstI64 { dst: 0, value: 96 },
            Op::CopyI64 { dst: 8, src: 96 },
            Op::ConstI64 { dst: 16, value: 0 },
            Op::HostCall {
                host: MOLTEN_INTERN_HOST,
            },
            // read the tag into 104 and payload field 0 into 112.
            Op::ConstI64 {
                dst: 80,
                value: 104,
            },
            Op::CopyI64 { dst: 88, src: 96 },
            Op::HostCall {
                host: STORE_TAG_HOST,
            },
            Op::ConstI64 {
                dst: 48,
                value: 112,
            },
            Op::CopyI64 { dst: 56, src: 96 },
            Op::ConstI64 { dst: 64, value: 0 },
            Op::HostCall {
                host: STORE_READ_HOST,
            },
            Op::AddI64 {
                dst: 120,
                a: 104,
                b: 112,
            },
            Op::Ret { src: 120, size: 8 },
        ];
        let mut driver = store_driver_for(code, vec![], vec![]);

        assert_eq!(driver.demand(FnRef::new(0), vec![]).unwrap(), 21);
        assert_eq!(driver.store_len(), 1);
        assert!(
            driver
                .trace
                .iter()
                .any(|e| matches!(e, DriveEvent::StoreAlloc { deduped: false, .. }))
        );
    }

    #[test]
    fn store_allocation_dedupes_structural_content() {
        let alloc_leaf = |value: i64, dst: u32| {
            vec![
                Op::ConstI64 {
                    dst: 0,
                    value: dst.into(),
                },
                Op::ConstI64 { dst: 8, value: 0 },
                Op::ConstI64 { dst: 16, value: 0 },
                Op::ConstI64 { dst: 24, value: 1 },
                Op::ConstI64 { dst: 32, value },
                Op::HostCall {
                    host: STORE_ALLOC_HOST,
                },
                Op::ConstI64 {
                    dst: 0,
                    value: dst.into(),
                },
                Op::CopyI64 { dst: 8, src: dst },
                Op::ConstI64 { dst: 16, value: 0 },
                Op::HostCall {
                    host: MOLTEN_INTERN_HOST,
                },
            ]
        };
        let mut code = Vec::new();
        code.extend(alloc_leaf(34, 96));
        code.extend(alloc_leaf(34, 104));
        code.extend([
            Op::EqI64 {
                dst: 112,
                a: 96,
                b: 104,
            },
            Op::Ret { src: 112, size: 8 },
        ]);
        let mut driver = store_driver_for(code, vec![], vec![]);

        assert_eq!(driver.demand(FnRef::new(0), vec![]).unwrap(), 1);
        assert_eq!(driver.store_len(), 1);
        assert!(
            driver
                .trace
                .iter()
                .any(|e| matches!(e, DriveEvent::StoreAlloc { deduped: true, .. }))
        );
    }

    #[test]
    fn handle_arguments_memoize_by_canonical_content() {
        let code = vec![
            Op::ConstI64 { dst: 48, value: 16 },
            Op::CopyI64 { dst: 56, src: 0 },
            Op::ConstI64 { dst: 64, value: 0 },
            Op::HostCall {
                host: STORE_READ_HOST,
            },
            Op::Ret { src: 16, size: 8 },
        ];
        let mut driver = store_driver_for(code, vec![0], vec!["Tree".into()]);
        let tree = driver
            .descriptors
            .get("Tree")
            .expect("Tree descriptor")
            .clone();
        let (h1, d1) = driver.store.borrow_mut().alloc(
            "Tree",
            {
                let mut bytes = vec![0; tree.layout.size];
                write_variant_tag(&mut bytes, &tree, 0);
                bytes[8..16].copy_from_slice(&55i64.to_le_bytes());
                bytes
            },
            &driver.descriptors,
            &driver.schemas,
        );
        let (h2, d2) = driver.store.borrow_mut().alloc(
            "Tree",
            {
                let mut bytes = vec![0; tree.layout.size];
                write_variant_tag(&mut bytes, &tree, 0);
                bytes[8..16].copy_from_slice(&55i64.to_le_bytes());
                bytes
            },
            &driver.descriptors,
            &driver.schemas,
        );
        assert!(!d1);
        assert!(d2);
        assert_eq!(h1, h2, "same value returns the same handle");

        assert_eq!(driver.demand(FnRef::new(0), vec![h1]).unwrap(), 55);
        assert_eq!(driver.demand(FnRef::new(0), vec![h2]).unwrap(), 55);
        let spawns = driver
            .trace
            .iter()
            .filter(|e| matches!(e, DriveEvent::Spawned { .. }))
            .count();
        let hits = driver
            .trace
            .iter()
            .filter(|e| matches!(e, DriveEvent::MemoHit { .. }))
            .count();
        assert_eq!(spawns, 1);
        assert_eq!(hits, 1);
    }
}
