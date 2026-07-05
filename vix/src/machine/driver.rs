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

use std::cell::RefCell;
use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};
use std::hash::Hash;
use std::sync::Arc;

use facet::Facet;
use sha2::{Digest, Sha256};
#[cfg(any(test, feature = "jit"))]
use weavy::jit::task_lane::{JitProgram, JitTask};
use weavy::mem::{Access, Descriptor, Tag};
#[cfg(any(test, feature = "jit"))]
use weavy::task::Op;
use weavy::task::{FnId, HostFn, Program, Task, TaskStep};

use crate::ast;
use crate::fetch::{FetchBackend, NoFetchBackend};
use crate::support::{PathMissing, assign_roles, subtree, tool_for};
use crate::value::{Payload, Value};

#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct StoreValue {
    pub handle: u64,
    pub schema: String,
    pub bytes: Vec<u8>,
    pub content_hash: Vec<u8>,
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
    fn flush(&self) -> Result<(crate::exec::Tree, crate::exec::ExecEvent), String>;
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

/// Driver-level events (join the unified trace via lowering-emitted
/// marks later; recorded directly in this slice).
#[derive(Clone, Debug, PartialEq, Eq)]
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
    fn_ref: usize,
    args: Vec<i64>,
}

#[derive(Clone, Debug)]
struct ProjectRequest {
    input_slot: usize,
    tree: i64,
    path: i64,
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
}

#[derive(Clone, Copy, Debug)]
enum DocParseKind {
    Toml,
    Json,
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
    fn_ref: usize,
    key: CanonMemoKey,
    ready: Vec<bool>,
    awaited: Vec<i64>,
    /// input slot → the invocation key feeding it (for wiring
    /// completions).
    feeds: HashMap<usize, CanonMemoKey>,
}

enum LaneRuntime {
    Interp,
    #[cfg(any(test, feature = "jit"))]
    Jit,
}

impl LaneRuntime {
    fn new(lane: Lane, _program: &Program) -> Result<Self, String> {
        match lane {
            Lane::Interp => Ok(Self::Interp),
            #[cfg(any(test, feature = "jit"))]
            Lane::Jit => {
                let Some(_) = JitProgram::compile(_program) else {
                    return Err(format!(
                        "weavy JIT task lane could not compile program; ops: {}",
                        program_op_set(_program)
                    ));
                };
                Ok(Self::Jit)
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
            Self::Jit => {
                let Some(jit) = JitProgram::compile(program) else {
                    return Err(format!(
                        "weavy JIT task lane could not compile program; ops: {}",
                        program_op_set(program)
                    ));
                };
                let mut task = JitTask::spawn(&jit, lowered.task_fn);
                for (offset, value) in lowered.arg_offsets.iter().zip(args) {
                    task.write_i64(*offset, *value);
                }
                Ok(LaneTask::Jit { program: jit, task })
            }
        }
    }
}

enum LaneTask {
    Interp(Task),
    #[cfg(any(test, feature = "jit"))]
    Jit {
        program: JitProgram,
        task: JitTask,
    },
}

impl LaneTask {
    fn advance(
        &mut self,
        program: &Program,
        ready: &[bool],
        awaited: &[i64],
        hosts: &mut [HostFn<'_>],
    ) -> TaskStep {
        match self {
            Self::Interp(task) => task.run_hosted(program, ready, awaited, hosts),
            #[cfg(any(test, feature = "jit"))]
            Self::Jit { program, task } => task.run_hosted(program, ready, awaited, hosts),
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

type ContentHash = [u8; 32];
type CanonMemoKey = (u64, Vec<ContentHash>);
#[cfg(test)]
pub type MapWordRow = (String, i64, String, i64, Option<i64>);

#[derive(Clone, Debug)]
pub struct StoreEntry {
    pub schema: String,
    pub bytes: Vec<u8>,
    pub content_hash: ContentHash,
}

#[derive(Clone, Debug, Default)]
pub struct ValueStore {
    entries: Vec<StoreEntry>,
    by_content: HashMap<(String, ContentHash), i64>,
}

#[derive(Clone, Debug)]
struct MapPair {
    key_schema: String,
    key_word: i64,
    value_schema: String,
    value_word: i64,
    value_realization: Option<Realization>,
}

#[derive(Clone)]
struct OrderedMapPair {
    pair: MapPair,
    key_hash: ContentHash,
    value_hash: ContentHash,
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
enum PendingPrimitive {
    Elf {
        projection: super::elf::Projection,
    },
    Ast {
        projection: super::ast_probe::Projection,
    },
}

#[derive(Clone, Debug)]
enum ArrayEntry {
    Words {
        elem_schema: String,
        words: Vec<i64>,
    },
    Pending(Vec<i64>),
}

#[derive(Clone, Debug)]
enum TreeEntry {
    Concrete(crate::exec::Tree),
    Merge(Vec<i64>),
    Exec(u64),
}

#[derive(Clone)]
struct PendingExecRun {
    command: String,
    plan: crate::exec::ExecPlan,
    capability: u64,
    mounts: Vec<crate::exec::Mount>,
    output: String,
    scheduled: bool,
    completed: Option<(crate::exec::Outcome, crate::exec::ExecEvent)>,
    completion_logged: bool,
    remote: Option<Arc<dyn MachinePendingRun>>,
    span: Option<(u32, u32)>,
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
        let key = (entry.schema.clone(), entry.content_hash);
        if let Some(existing) = self.entries.get(handle) {
            if existing.schema == entry.schema
                && existing.bytes == entry.bytes
                && existing.content_hash == entry.content_hash
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
        descriptors: &HashMap<String, Descriptor<String>>,
    ) -> (i64, bool) {
        let descriptor = descriptors
            .get(schema)
            .unwrap_or_else(|| panic!("descriptor for schema `{schema}`"));
        let content_hash = hash_value_bytes(descriptor, &bytes, self);
        let key = (schema.to_string(), content_hash);
        if let Some(handle) = self.by_content.get(&key).copied() {
            return (handle, true);
        }
        let handle = i64::try_from(self.entries.len()).expect("store handle fits i64");
        self.entries.push(StoreEntry {
            schema: schema.to_string(),
            bytes,
            content_hash,
        });
        self.by_content.insert(key, handle);
        (handle, false)
    }

    fn alloc_raw(&mut self, schema: &str, bytes: Vec<u8>) -> (i64, bool) {
        let mut hasher = Sha256::new();
        hasher.update(b"vix-raw-value");
        hasher.update(schema.as_bytes());
        hasher.update(&bytes);
        let content_hash = hasher.finalize().into();
        let key = (schema.to_string(), content_hash);
        if let Some(handle) = self.by_content.get(&key).copied() {
            return (handle, true);
        }
        let handle = i64::try_from(self.entries.len()).expect("store handle fits i64");
        self.entries.push(StoreEntry {
            schema: schema.to_string(),
            bytes,
            content_hash,
        });
        self.by_content.insert(key, handle);
        (handle, false)
    }

    fn alloc_map(
        &mut self,
        schema: &str,
        pairs: Vec<MapPair>,
        schema_refs: &[String],
        descriptors: &HashMap<String, Descriptor<String>>,
    ) -> Result<(i64, bool), String> {
        let ordered = canonical_map_pairs(self, pairs, descriptors, schema_refs)?;
        let bytes = encode_map_pairs(&ordered, schema_refs)?;
        let content_hash = hash_map_pairs(schema, &ordered);
        Ok(self.alloc_with_hash(schema, bytes, content_hash))
    }

    fn map_pairs(
        &self,
        handle: i64,
        schema_refs: &[String],
    ) -> Result<(String, Vec<MapPair>), String> {
        let entry = self
            .entry(handle)
            .ok_or_else(|| format!("store handle {handle}"))?;
        if !entry.schema.starts_with("Map") {
            return Err(format!("handle {handle} is `{}`, not a Map", entry.schema));
        }
        Ok((
            entry.schema.clone(),
            decode_map_pairs(&entry.bytes, schema_refs)?,
        ))
    }

    fn map_get(
        &mut self,
        handle: i64,
        key_schema: &str,
        key_word: i64,
        value_schema: &str,
        _descriptors: &HashMap<String, Descriptor<String>>,
        schema_refs: &[String],
    ) -> Result<(i64, bool), String> {
        let (_, pairs) = self.map_pairs(handle, schema_refs)?;
        let key_hash = canonical_word_hash_in_store(self, key_schema, key_word);
        for pair in pairs {
            if pair.key_schema != key_schema
                || canonical_word_hash_in_store(self, &pair.key_schema, pair.key_word) != key_hash
            {
                continue;
            }
            if pair.value_schema == value_schema {
                return self.alloc_option_some(
                    &pair.value_schema,
                    pair.value_word,
                    pair.value_realization,
                    schema_refs,
                );
            }
            if let Some(inner) = realized_value_schema(value_schema) {
                if let Some(realization) = pair.value_realization {
                    match realization {
                        Realization::Ready if pair.value_schema == inner => {
                            return self.alloc_option_some(
                                value_schema,
                                pair.value_word,
                                Some(Realization::Ready),
                                schema_refs,
                            );
                        }
                        Realization::Pending if pair.value_schema == pending_schema(inner) => {
                            return self.alloc_option_some(
                                value_schema,
                                pair.value_word,
                                Some(Realization::Pending),
                                schema_refs,
                            );
                        }
                        _ => {}
                    }
                }
                if pair.value_schema == inner && pair.value_realization.is_none() {
                    return self.alloc_option_some(
                        value_schema,
                        pair.value_word,
                        Some(Realization::Ready),
                        schema_refs,
                    );
                }
                if pair.value_schema == pending_schema(inner) && pair.value_realization.is_none() {
                    return self.alloc_option_some(
                        value_schema,
                        pair.value_word,
                        Some(Realization::Pending),
                        schema_refs,
                    );
                }
            }
            if pair.value_schema == pending_schema(value_schema) {
                return self.alloc_option_some(
                    &pair.value_schema,
                    pair.value_word,
                    pair.value_realization,
                    schema_refs,
                );
            }
        }
        self.alloc_option_none(value_schema, schema_refs)
    }

    fn alloc_option_none(
        &mut self,
        value_schema: &str,
        schema_refs: &[String],
    ) -> Result<(i64, bool), String> {
        let option_schema = option_schema(value_schema);
        let value_ref = schema_ref_for(value_schema, schema_refs)?;
        let mut bytes = Vec::with_capacity(24);
        bytes.extend_from_slice(&0i64.to_le_bytes());
        bytes.extend_from_slice(&value_ref.to_le_bytes());
        bytes.extend_from_slice(&0i64.to_le_bytes());
        let mut hasher = Sha256::new();
        hasher.update(b"vix-option");
        hasher.update(option_schema.as_bytes());
        hasher.update(0i64.to_le_bytes());
        let content_hash = hasher.finalize().into();
        Ok(self.alloc_with_hash(&option_schema, bytes, content_hash))
    }

    fn alloc_option_some(
        &mut self,
        value_schema: &str,
        value_word: i64,
        realization: Option<Realization>,
        schema_refs: &[String],
    ) -> Result<(i64, bool), String> {
        let option_schema = option_schema(value_schema);
        let value_ref = schema_ref_for(value_schema, schema_refs)?;
        let hash_schema = realized_value_schema(value_schema).unwrap_or(value_schema);
        let canonical_schema = match realization {
            Some(Realization::Pending) => pending_schema(hash_schema),
            _ => hash_schema.to_string(),
        };
        let value_word = canonicalize_word_for_schema(&canonical_schema, value_word);
        let value_hash = canonical_word_hash_in_store(self, &canonical_schema, value_word);
        let mut bytes = Vec::with_capacity(32);
        bytes.extend_from_slice(&1i64.to_le_bytes());
        bytes.extend_from_slice(&value_ref.to_le_bytes());
        bytes.extend_from_slice(&value_word.to_le_bytes());
        if let Some(realization) = &realization {
            bytes.extend_from_slice(&realization.to_word().to_le_bytes());
        }
        let mut hasher = Sha256::new();
        hasher.update(b"vix-option");
        hasher.update(option_schema.as_bytes());
        hasher.update(1i64.to_le_bytes());
        hasher.update(value_schema.as_bytes());
        if let Some(realization) = &realization {
            hasher.update(realization.to_word().to_le_bytes());
        }
        hasher.update(value_hash);
        let content_hash = hasher.finalize().into();
        Ok(self.alloc_with_hash(&option_schema, bytes, content_hash))
    }

    fn option_payload(&self, handle: i64, schema_refs: &[String]) -> Result<OptionPayload, String> {
        let entry = self
            .entry(handle)
            .ok_or_else(|| format!("store handle {handle}"))?;
        if !entry.schema.starts_with("Option<") {
            return Err(format!(
                "handle {handle} is `{}`, not an Option",
                entry.schema
            ));
        }
        if entry.bytes.len() != 24 && entry.bytes.len() != 32 {
            return Err(format!("Option entry has {} bytes", entry.bytes.len()));
        }
        let tag = read_frame_word(&entry.bytes, 0);
        if tag == 0 {
            return Ok(OptionPayload::None);
        }
        let schema_ref = read_frame_word(&entry.bytes, 8);
        let schema = schema_refs
            .get(usize::try_from(schema_ref).map_err(|_| format!("schema ref {schema_ref}"))?)
            .ok_or_else(|| format!("schema ref {schema_ref}"))?;
        let realization = if entry.bytes.len() == 32 {
            Some(Realization::from_word(read_frame_word(&entry.bytes, 24))?)
        } else {
            None
        };
        let word_schema = if let Some(Realization::Pending) = realization {
            pending_schema(realized_value_schema(schema).unwrap_or(schema))
        } else {
            realized_value_schema(schema).unwrap_or(schema).to_string()
        };
        Ok(OptionPayload::Some {
            word: canonicalize_word_for_schema(&word_schema, read_frame_word(&entry.bytes, 16)),
            realization,
        })
    }

    fn alloc_with_hash(
        &mut self,
        schema: &str,
        bytes: Vec<u8>,
        content_hash: ContentHash,
    ) -> (i64, bool) {
        let key = (schema.to_string(), content_hash);
        if let Some(handle) = self.by_content.get(&key).copied() {
            return (handle, true);
        }
        let handle = i64::try_from(self.entries.len()).expect("store handle fits i64");
        self.entries.push(StoreEntry {
            schema: schema.to_string(),
            bytes,
            content_hash,
        });
        self.by_content.insert(key, handle);
        (handle, false)
    }

    fn alloc_array_words(
        &mut self,
        elem_schema: &str,
        words: Vec<i64>,
        schema_refs: &[String],
    ) -> Result<(i64, bool), String> {
        let mut bytes = Vec::with_capacity(24 + words.len() * 8);
        bytes.extend_from_slice(&0i64.to_le_bytes());
        bytes.extend_from_slice(&schema_ref_for(elem_schema, schema_refs)?.to_le_bytes());
        bytes.extend_from_slice(
            &i64::try_from(words.len())
                .expect("array length fits i64")
                .to_le_bytes(),
        );
        for word in &words {
            bytes.extend_from_slice(&word.to_le_bytes());
        }
        let mut hasher = Sha256::new();
        hasher.update(b"vix-array-words");
        hasher.update(elem_schema.as_bytes());
        hasher.update(
            i64::try_from(words.len())
                .expect("array length fits i64")
                .to_le_bytes(),
        );
        for word in &words {
            hasher.update(canonical_word_hash_in_store(self, elem_schema, *word));
        }
        let content_hash = hasher.finalize().into();
        Ok(self.alloc_with_hash("Array", bytes, content_hash))
    }

    fn alloc_pending(&mut self, value_schema: &str, invocation: PendingInvocation) -> (i64, bool) {
        let bytes = encode_pending_invocation(&invocation);
        self.alloc_with_hash(
            &pending_schema(value_schema),
            bytes,
            invocation.identity_hash,
        )
    }

    fn pending_invocation(&self, handle: i64) -> Result<PendingInvocation, String> {
        let entry = self
            .entry(handle)
            .ok_or_else(|| format!("store handle {handle}"))?;
        if pending_value_schema(&entry.schema).is_none() {
            return Err(format!(
                "handle {handle} is `{}`, not Pending",
                entry.schema
            ));
        }
        decode_pending_invocation(&entry.bytes)
    }

    fn alloc_array_pending(&mut self, pending: Vec<i64>) -> (i64, bool) {
        let bytes = encode_handle_list(1, &pending);
        let content_hash = hash_handle_list(b"vix-array-pending", &pending, self);
        self.alloc_with_hash("Array", bytes, content_hash)
    }

    fn array_entry(&self, handle: i64, schema_refs: &[String]) -> Result<ArrayEntry, String> {
        let entry = self
            .entry(handle)
            .ok_or_else(|| format!("store handle {handle}"))?;
        if entry.schema != "Array" {
            return Err(format!("handle {handle} is `{}`, not Array", entry.schema));
        }
        let kind = read_frame_word(&entry.bytes, 0);
        match kind {
            0 => {
                let count = usize::try_from(read_frame_word(&entry.bytes, 16))
                    .map_err(|_| "array count")?;
                let expected = 24 + count * 8;
                if entry.bytes.len() != expected {
                    return Err(format!(
                        "Array words entry has {} bytes, expected {expected}",
                        entry.bytes.len()
                    ));
                }
                let elem_schema = schema_name_for(read_frame_word(&entry.bytes, 8), schema_refs)?;
                Ok(ArrayEntry::Words {
                    elem_schema,
                    words: (0..count)
                        .map(|i| read_frame_word(&entry.bytes, 24 + i * 8))
                        .collect(),
                })
            }
            1 => Ok(ArrayEntry::Pending(decode_handle_list(&entry.bytes)?)),
            other => Err(format!("unknown Array kind {other}")),
        }
    }

    fn alloc_tree_concrete(&mut self, tree: crate::exec::Tree) -> (i64, bool) {
        let bytes = encode_concrete_tree(&tree);
        let content_hash = hash_concrete_tree(&tree);
        self.alloc_with_hash("Tree", bytes, content_hash)
    }

    fn alloc_tree_merge(&mut self, pending: Vec<i64>) -> (i64, bool) {
        let bytes = encode_handle_list(1, &pending);
        let content_hash = hash_handle_list(b"vix-tree-merge", &pending, self);
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
            bytes,
            content_hash: identity,
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

/// The demand scheduler.
pub struct Driver {
    program: Program,
    lane_kind: Lane,
    lane: LaneRuntime,
    fns: Vec<LoweredFn>,
    descriptors: HashMap<String, Descriptor<String>>,
    schema_refs: Vec<String>,
    memo: HashMap<CanonMemoKey, i64>,
    journal: BTreeMap<String, i64>,
    exec_cache: crate::exec::ExecCache,
    fetch_backend: Arc<dyn FetchBackend>,
    exec_backend: Option<Arc<dyn MachineExecBackend>>,
    elf_projection_memo: HashMap<(ContentHash, super::elf::Projection), i64>,
    ast_roots: HashMap<i64, i64>,
    ast_parse_memo: HashMap<ContentHash, Arc<ast::SourceFile>>,
    ast_projection_memo: HashMap<(ContentHash, super::ast_probe::Projection, String), i64>,
    runs: BTreeMap<u64, PendingExecRun>,
    next_run_id: u64,
    trace_clock: u64,
    pub trace: Vec<DriveEvent>,
    event_sink: Option<DriveEventSink>,
    step_mode: StepMode,
    store: RefCell<ValueStore>,
}

impl Driver {
    pub fn new(program: Program, fns: Vec<LoweredFn>) -> Self {
        Self::with_descriptors(program, fns, HashMap::new())
    }

    pub fn with_descriptors(
        program: Program,
        fns: Vec<LoweredFn>,
        descriptors: HashMap<String, Descriptor<String>>,
    ) -> Self {
        Self::try_with_descriptors(program, fns, descriptors, Lane::Interp)
            .expect("interp lane is always available")
    }

    pub fn try_with_descriptors(
        program: Program,
        fns: Vec<LoweredFn>,
        descriptors: HashMap<String, Descriptor<String>>,
        lane: Lane,
    ) -> Result<Self, String> {
        let lane_runtime = LaneRuntime::new(lane, &program)?;
        Ok(Driver {
            program,
            lane_kind: lane,
            lane: lane_runtime,
            fns,
            descriptors,
            schema_refs: Vec::new(),
            memo: HashMap::new(),
            journal: BTreeMap::new(),
            exec_cache: crate::exec::ExecCache::new(),
            fetch_backend: Arc::new(NoFetchBackend),
            exec_backend: None,
            elf_projection_memo: HashMap::new(),
            ast_roots: HashMap::new(),
            ast_parse_memo: HashMap::new(),
            ast_projection_memo: HashMap::new(),
            runs: BTreeMap::new(),
            next_run_id: 0,
            trace_clock: 0,
            trace: Vec::new(),
            event_sink: None,
            step_mode: StepMode::Run,
            store: RefCell::new(ValueStore::default()),
        })
    }

    pub fn reload(
        &mut self,
        program: Program,
        fns: Vec<LoweredFn>,
        descriptors: HashMap<String, Descriptor<String>>,
    ) -> Result<(), String> {
        self.lane = LaneRuntime::new(self.lane_kind, &program)?;
        self.program = program;
        self.fns = fns;
        self.descriptors = descriptors;
        self.trace.clear();
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

    pub fn intern_schema_ref(&mut self, schema: impl Into<String>) -> i64 {
        let schema = schema.into();
        if let Some(index) = self.schema_refs.iter().position(|s| s == &schema) {
            return i64::try_from(index).expect("schema ref fits i64");
        }
        let index = self.schema_refs.len();
        self.schema_refs.push(schema);
        i64::try_from(index).expect("schema ref fits i64")
    }

    pub fn schema_ref_map_for(&mut self, schemas: &[String]) -> HashMap<String, i64> {
        schemas
            .iter()
            .map(|schema| (schema.clone(), self.intern_schema_ref(schema.clone())))
            .collect()
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
                bytes: entry.bytes,
                content_hash: entry.content_hash.to_vec(),
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
            let content_hash: ContentHash = value
                .content_hash
                .as_slice()
                .try_into()
                .map_err(|_| format!("content hash has {} bytes", value.content_hash.len()))?;
            store.insert_at(
                usize::try_from(value.handle).map_err(|_| format!("handle {}", value.handle))?,
                StoreEntry {
                    schema: value.schema,
                    bytes: value.bytes,
                    content_hash,
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
        let offset = field_offset(descriptor, &entry.bytes, field_index);
        Ok(read_frame_word(&entry.bytes, offset))
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
        let (_, pairs) = self.store.borrow().map_pairs(handle, &self.schema_refs)?;
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
        match self.store.borrow().array_entry(handle, &self.schema_refs)? {
            ArrayEntry::Words { elem_schema, words } => Ok((elem_schema, words)),
            ArrayEntry::Pending(_) => Err("array is pending".into()),
        }
    }

    #[cfg(test)]
    pub fn compare_store_words(&self, schema: &str, a: i64, b: i64) -> Result<Ordering, String> {
        compare_words(
            &self.store.borrow(),
            &self.descriptors,
            &self.schema_refs,
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
            &self.schema_refs,
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
            .alloc("Run", bytes, &self.descriptors)
            .0)
    }

    pub fn fn_hash(&self, fn_ref: usize) -> u64 {
        self.fns[fn_ref].hash
    }

    pub fn pending_for_fn(&self, fn_ref: usize, args: Vec<i64>) -> Result<(i64, String), String> {
        let lowered = self
            .fns
            .get(fn_ref)
            .ok_or_else(|| format!("function ref {fn_ref}"))?;
        if args.len() > lowered.arg_schemas.len() {
            return Err(format!(
                "pending invocation got {} args, expected at most {}",
                args.len(),
                lowered.arg_schemas.len()
            ));
        }
        let invocation = pending_invocation_for(lowered, &self.store, args);
        let schema = lowered.return_schema.clone();
        let handle = self.store.borrow_mut().alloc_pending(&schema, invocation).0;
        Ok((handle, pending_schema(&schema)))
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
    pub fn fn_ops(&self, fn_ref: usize) -> &[Op] {
        &self.program.fns[self.fns[fn_ref].task_fn.0 as usize].code
    }

    pub fn intern_raw_value(&self, schema: &str, bytes: Vec<u8>) -> (i64, bool) {
        self.store.borrow_mut().alloc_raw(schema, bytes)
    }

    pub fn intern_linux_target(&self) -> (i64, bool) {
        self.intern_structured_target(0).unwrap_or_else(|_| {
            self.store
                .borrow_mut()
                .alloc_raw("Target", 0x391c555cf0975f9cu64.to_le_bytes().to_vec())
        })
    }

    #[cfg(test)]
    pub fn intern_windows_target(&self) -> Result<(i64, bool), String> {
        self.intern_structured_target(2)
    }

    fn intern_structured_target(&self, os_index: u64) -> Result<(i64, bool), String> {
        let os_descriptor = self
            .descriptors
            .get("Os")
            .ok_or_else(|| "missing Os descriptor".to_string())?;
        let mut os_bytes = vec![0u8; os_descriptor.layout.size];
        write_variant_tag(&mut os_bytes, os_descriptor, os_index);
        let os = self
            .store
            .borrow_mut()
            .alloc("Os", os_bytes, &self.descriptors)
            .0;

        let target_descriptor = self
            .descriptors
            .get("Target")
            .ok_or_else(|| "missing Target descriptor".to_string())?;
        let mut target_bytes = vec![0u8; target_descriptor.layout.size];
        let offset = field_offset(target_descriptor, &target_bytes, 0);
        target_bytes[offset..offset + 8].copy_from_slice(&os.to_le_bytes());
        Ok(self
            .store
            .borrow_mut()
            .alloc("Target", target_bytes, &self.descriptors))
    }

    /// Demand one invocation's identity: the edge of the machine.
    /// Returns the scalar result (slice 1).
    pub fn demand(&mut self, fn_ref: usize, args: Vec<i64>) -> Result<i64, String> {
        let key = self.memo_key(fn_ref, &args);
        self.emit(DriveEvent::Demanded { fn_hash: key.0 });
        if let Some(&v) = self.memo.get(&key) {
            self.emit(DriveEvent::MemoHit { fn_hash: key.0 });
            return Ok(v);
        }

        // Waiters: invocation key → executions parked on it (by index
        // into `executions`) with the slot to fill.
        let mut executions: Vec<Option<Execution>> = Vec::new();
        let mut waiters: HashMap<CanonMemoKey, Vec<(usize, usize)>> = HashMap::new();
        let mut runnable: Vec<usize> = Vec::new();

        let root = self.spawn(&mut executions, fn_ref, key.clone(), &args)?;
        runnable.push(root);

        while let Some(ix) = runnable.pop() {
            let mut exec = executions[ix].take().expect("runnable execution exists");
            let requests = self.burst(&mut exec, ix);
            match requests {
                Burst::Done(value) => {
                    let done_key = exec.key.clone();
                    let value = self.canonicalize_return_word(exec.fn_ref, value);
                    self.memo.insert(done_key.clone(), value);
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
                            runnable.push(waiter_ix);
                        }
                    }
                    // Execution finished: drop it (arena and all).
                }
                Burst::Pending {
                    new_requests,
                    project_requests,
                    exec_requests,
                    fetch_requests,
                    doc_parse_requests,
                    option_unwraps,
                    pending_coercions,
                    pending_invokes,
                    parked_input,
                } => {
                    for req in exec_requests {
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
                        match self.fetch_request(req) {
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
                    for req in doc_parse_requests {
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
                    for req in project_requests {
                        match self.project_request(req) {
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
                        match self.pending_coercion(req, ix) {
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
                        if let Some(&v) = self.memo.get(&req_key) {
                            // Mechanism 1: memo hit — the slot fills
                            // synchronously, no task exists.
                            self.emit(DriveEvent::MemoHit { fn_hash: req_key.0 });
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
                                    self.spawn(&mut executions, req.fn_ref, req_key, &req.args)?;
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
            .copied()
            .ok_or_else(|| "root invocation did not complete".to_string())
    }

    fn spawn(
        &mut self,
        executions: &mut Vec<Option<Execution>>,
        fn_ref: usize,
        key: CanonMemoKey,
        args: &[i64],
    ) -> Result<usize, String> {
        let fn_hash = self.fns[fn_ref].hash;
        self.emit(DriveEvent::Spawned { fn_hash });
        self.emit(DriveEvent::SpawnedInvocation {
            fn_hash,
            key_hash: memo_key_hash(&key),
        });
        let lowered = &self.fns[fn_ref];
        let task = self.lane.spawn(&self.program, lowered, args)?;
        executions.push(Some(Execution {
            task,
            fn_ref,
            key,
            ready: Vec::new(),
            awaited: Vec::new(),
            feeds: HashMap::new(),
        }));
        Ok(executions.len() - 1)
    }

    /// Run one execution until done or blocked, capturing INVOKE
    /// requests raised during the burst.
    fn burst(&mut self, exec: &mut Execution, exec_ix: usize) -> Burst {
        let lowered = &self.fns[exec.fn_ref];
        let invoke_region = lowered.invoke_region as usize;
        let store_alloc_region = lowered.store_alloc_region as usize;
        let store_read_region = lowered.store_read_region as usize;
        let store_tag_region = lowered.store_tag_region as usize;
        let primitive_region = lowered.primitive_region as usize;
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
            let mut exec_requests: Vec<ExecRequest> = Vec::new();
            let mut fetch_requests: Vec<FetchRequest> = Vec::new();
            let mut doc_parse_requests: Vec<DocParseRequest> = Vec::new();
            let mut option_unwraps: Vec<OptionUnwrapRequest> = Vec::new();
            let mut pending_coercions: Vec<PendingCoerceRequest> = Vec::new();
            let mut pending_invokes: Vec<PendingInvokeRequest> = Vec::new();
            let descriptors = &self.descriptors;
            let schema_refs = &self.schema_refs;
            let store_cell = &self.store;
            let ast_roots_cell = RefCell::new(&mut self.ast_roots);
            let journal_cell = RefCell::new(&mut self.journal);
            let clock_cell = RefCell::new(&mut self.trace_clock);
            let lowered_fns = &self.fns;
            let store_events = RefCell::new(Vec::new());
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

            let mut store_alloc = |frame: &mut [u8]| {
                let dst_slot = read_frame_word(frame, store_alloc_region) as usize;
                let type_ref = read_frame_word(frame, store_alloc_region + 8);
                let schema =
                    schema_name_for(type_ref, schema_refs).unwrap_or_else(|err| panic!("{err}"));
                let variant_index = read_frame_word(frame, store_alloc_region + 16);
                let field_count = read_frame_word(frame, store_alloc_region + 24) as usize;
                let descriptor = descriptors
                    .get(&schema)
                    .unwrap_or_else(|| panic!("descriptor for schema `{schema}`"));
                let mut bytes = vec![0u8; descriptor.layout.size];
                write_variant_tag(&mut bytes, descriptor, variant_index as u64);
                write_alloc_fields(
                    &mut bytes,
                    descriptor,
                    usize::try_from(variant_index).expect("variant index non-negative"),
                    field_count,
                    frame,
                    store_alloc_region + 32,
                );
                let (handle, deduped) = store_cell.borrow_mut().alloc(&schema, bytes, descriptors);
                write_frame_word(frame, dst_slot, handle);
                let schema_ref = hash_u64(&schema);
                store_events.borrow_mut().push(DriveEvent::StoreAlloc {
                    schema_ref,
                    deduped,
                });
            };

            let mut store_read = |frame: &mut [u8]| {
                let dst_slot = read_frame_word(frame, store_read_region) as usize;
                let handle = read_frame_word(frame, store_read_region + 8);
                let field_index = read_frame_word(frame, store_read_region + 16) as usize;
                let store = store_cell.borrow();
                let entry = store
                    .entry(handle)
                    .unwrap_or_else(|| panic!("store handle {handle}"));
                let descriptor = descriptors
                    .get(&entry.schema)
                    .unwrap_or_else(|| panic!("descriptor for schema `{}`", entry.schema));
                let offset = field_offset(descriptor, &entry.bytes, field_index);
                let value = read_frame_word(&entry.bytes, offset);
                write_frame_word(frame, dst_slot, value);
            };

            let mut store_tag = |frame: &mut [u8]| {
                let dst_slot = read_frame_word(frame, store_tag_region) as usize;
                let handle = read_frame_word(frame, store_tag_region + 8);
                let store = store_cell.borrow();
                let entry = store
                    .entry(handle)
                    .unwrap_or_else(|| panic!("store handle {handle}"));
                let descriptor = descriptors
                    .get(&entry.schema)
                    .unwrap_or_else(|| panic!("descriptor for schema `{}`", entry.schema));
                let tag = read_variant_tag(&entry.bytes, descriptor);
                write_frame_word(
                    frame,
                    dst_slot,
                    i64::try_from(tag).expect("variant tag fits i64"),
                );
            };

            let host_error = RefCell::new(None::<String>);

            let mut map_empty = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, store_alloc_region) as usize;
                    let schema = schema_name_for(
                        read_frame_word(frame, store_alloc_region + 8),
                        schema_refs,
                    )?;
                    let (handle, deduped) = store_cell.borrow_mut().alloc_map(
                        &schema,
                        Vec::new(),
                        schema_refs,
                        descriptors,
                    )?;
                    write_frame_word(frame, dst_slot, handle);
                    store_events.borrow_mut().push(DriveEvent::StoreAlloc {
                        schema_ref: hash_u64(&schema),
                        deduped,
                    });
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
                        schema_refs,
                    )?;
                    let key_schema = schema_name_for(
                        read_frame_word(frame, store_alloc_region + 24),
                        schema_refs,
                    )?;
                    let key_word = canonicalize_word_for_schema(
                        &key_schema,
                        read_frame_word(frame, store_alloc_region + 32),
                    );
                    let value_schema = schema_name_for(
                        read_frame_word(frame, store_alloc_region + 40),
                        schema_refs,
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
                        &stored_value_schema,
                        read_frame_word(frame, store_alloc_region + 48),
                    );
                    let (stored_schema, mut pairs) =
                        store_cell.borrow().map_pairs(map_handle, schema_refs)?;
                    if stored_schema != map_schema {
                        pairs = promote_map_pairs_to_realized(&stored_schema, &map_schema, pairs)?;
                    }
                    pairs.push(MapPair {
                        key_schema,
                        key_word,
                        value_schema: stored_value_schema,
                        value_word,
                        value_realization: realized_value_schema(&value_schema)
                            .map(|_| value_realization),
                    });
                    let (handle, deduped) = store_cell.borrow_mut().alloc_map(
                        &map_schema,
                        pairs,
                        schema_refs,
                        descriptors,
                    )?;
                    write_frame_word(frame, dst_slot, handle);
                    store_events.borrow_mut().push(DriveEvent::StoreAlloc {
                        schema_ref: hash_u64(&map_schema),
                        deduped,
                    });
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut map_get = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, store_alloc_region) as usize;
                    let map_handle = read_frame_word(frame, store_alloc_region + 8);
                    let value_schema = schema_name_for(
                        read_frame_word(frame, store_alloc_region + 16),
                        schema_refs,
                    )?;
                    let key_schema = schema_name_for(
                        read_frame_word(frame, store_alloc_region + 24),
                        schema_refs,
                    )?;
                    let key_word = canonicalize_word_for_schema(
                        &key_schema,
                        read_frame_word(frame, store_alloc_region + 32),
                    );
                    let (handle, _) = store_cell.borrow_mut().map_get(
                        map_handle,
                        &key_schema,
                        key_word,
                        &value_schema,
                        descriptors,
                        schema_refs,
                    )?;
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

            let mut acquire = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let kind_ref = read_frame_word(frame, primitive_region + 8);
                    let kind = schema_name_for(kind_ref, schema_refs)?;
                    let target = read_frame_word(frame, primitive_region + 16);
                    let target_hash = target_hash(store_cell, target)?;
                    let key = format!("acquire:{kind}:{target_hash:x}");
                    let mut journal = journal_cell.borrow_mut();
                    let (handle, replayed) = if let Some(handle) = journal.get(&key).copied() {
                        (handle, true)
                    } else {
                        let handle = store_cell
                            .borrow_mut()
                            .alloc_raw(&kind, key.clone().into_bytes())
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
                    let elem_schema =
                        schema_name_for(read_frame_word(frame, primitive_region + 8), schema_refs)?;
                    let count = usize::try_from(read_frame_word(frame, primitive_region + 16))
                        .map_err(|_| "negative array length".to_string())?;
                    let words = (0..count)
                        .map(|i| read_frame_word(frame, primitive_region + 24 + i * 8))
                        .collect();
                    let (handle, _) = store_cell.borrow_mut().alloc_array_words(
                        &elem_schema,
                        words,
                        schema_refs,
                    )?;
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
                    let fn_ref = usize::try_from(read_frame_word(frame, primitive_region + 16))
                        .map_err(|_| "negative fn ref".to_string())?;
                    let arg_count = usize::try_from(read_frame_word(frame, primitive_region + 24))
                        .map_err(|_| "negative map arg count".to_string())?;
                    let arg_specs = (0..arg_count)
                        .map(|index| {
                            let at = primitive_region + 32 + index * 16;
                            Ok((read_frame_word(frame, at), read_frame_word(frame, at + 8)))
                        })
                        .collect::<Result<Vec<_>, String>>()?;
                    let words = match store_cell.borrow().array_entry(array_handle, schema_refs)? {
                        ArrayEntry::Words { words, .. } => words,
                        ArrayEntry::Pending(_) => {
                            return Err("map over pending array is outside slice 4".into());
                        }
                    };
                    let pending = words
                        .into_iter()
                        .map(|word| {
                            let args = arg_specs
                                .iter()
                                .map(|(kind, value)| match kind {
                                    0 => *value,
                                    1 => word,
                                    other => panic!("unknown map arg kind {other}"),
                                })
                                .collect();
                            let invocation =
                                pending_invocation_for(&lowered_fns[fn_ref], store_cell, args);
                            store_cell
                                .borrow_mut()
                                .alloc_pending(&lowered_fns[fn_ref].return_schema, invocation)
                                .0
                        })
                        .collect();
                    let (handle, _) = store_cell.borrow_mut().alloc_array_pending(pending);
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
                    let array_entry =
                        { store_cell.borrow().array_entry(array_handle, schema_refs)? };
                    match array_entry {
                        ArrayEntry::Pending(pending) => {
                            let (handle, _) = store_cell.borrow_mut().alloc_tree_merge(pending);
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
                                        schema_refs,
                                        &elem_schema,
                                        *a,
                                        *b,
                                    )
                                }
                                .expect("array collect comparison")
                            });
                            let (handle, _) = store_cell.borrow_mut().alloc_array_words(
                                &elem_schema,
                                words,
                                schema_refs,
                            )?;
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

            let mut exec_host = |frame: &mut [u8]| {
                let input_slot = read_frame_word(frame, primitive_region) as usize;
                let command = match read_frame_word(frame, primitive_region + 8) {
                    0 => "cc",
                    1 => "ar",
                    2 => "rustc",
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
                    let at = primitive_region + 48 + index * 16;
                    let kind = read_frame_word(frame, at);
                    let word = read_frame_word(frame, at + 8);
                    let part = match kind {
                        0 => CommandRequestPart::Token(word),
                        1 => CommandRequestPart::Splice(word),
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
                    other => {
                        *host_error.borrow_mut() =
                            Some(format!("unknown document parser kind {other}"));
                        return;
                    }
                };
                let input = read_frame_word(frame, primitive_region + 16);
                doc_parse_requests.push(DocParseRequest {
                    input_slot,
                    kind,
                    input,
                });
            };

            let mut doc_get_host = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let doc = read_frame_word(frame, primitive_region + 8);
                    let key = read_frame_word(frame, primitive_region + 16);
                    let key = store_cell.borrow().string_value(key, "String")?;
                    let (handle, _) = doc_get(store_cell, descriptors, schema_refs, doc, &key)?;
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
                        schema_refs,
                    )?;
                    let word = doc_coerce(store_cell, descriptors, doc, &schema)?;
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
                    let handle = alloc_elf_doc(store_cell, descriptors, schema_refs, input)?;
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
                    let handle = alloc_ast_doc(store_cell, descriptors, schema_refs, input)?;
                    ast_roots_cell.borrow_mut().insert(handle, input);
                    write_frame_word(frame, dst_slot, handle);
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
                    let len = match store_cell.borrow().array_entry(array, schema_refs)? {
                        ArrayEntry::Words { words, .. } => words.len(),
                        ArrayEntry::Pending(pending) => pending.len(),
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
                    let (elem_schema, words) =
                        match store_cell.borrow().array_entry(array_handle, schema_refs)? {
                            ArrayEntry::Words { elem_schema, words } => (elem_schema, words),
                            ArrayEntry::Pending(_) => {
                                return Err("filter over pending array is outside B4".into());
                            }
                        };
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
                        schema_refs,
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
                    let suffix = pattern
                        .strip_prefix('*')
                        .ok_or_else(|| "glob v0 supports `*.ext` patterns".to_string())?;
                    let tree = match store_cell.borrow().tree_entry(tree_handle)? {
                        TreeEntry::Concrete(tree) => tree,
                        TreeEntry::Merge(_) | TreeEntry::Exec(_) => {
                            return Err("glob on pending tree is outside B4".into());
                        }
                    };
                    let mut paths: Vec<String> = tree
                        .entries
                        .keys()
                        .filter(|path| !path.contains('/') && path.ends_with(suffix))
                        .cloned()
                        .collect();
                    paths.sort();
                    let words = paths
                        .into_iter()
                        .map(|path| {
                            store_cell
                                .borrow_mut()
                                .alloc_raw("Path", path.into_bytes())
                                .0
                        })
                        .collect();
                    let (handle, _) =
                        store_cell
                            .borrow_mut()
                            .alloc_array_words("Path", words, schema_refs)?;
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
                    let path = store_cell.borrow().string_value(path, "Path")?;
                    let ext = store_cell.borrow().string_value(ext, "String")?;
                    let stem = path.rsplit_once('.').map(|(stem, _)| stem).unwrap_or(&path);
                    let value = format!("{stem}.{ext}");
                    let (handle, _) = store_cell
                        .borrow_mut()
                        .alloc_raw("Path", value.into_bytes());
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
                    let value_schema =
                        schema_name_for(read_frame_word(frame, primitive_region + 8), schema_refs)?;
                    let fn_ref = usize::try_from(read_frame_word(frame, primitive_region + 16))
                        .map_err(|_| "negative fn ref".to_string())?;
                    let argc = usize::try_from(read_frame_word(frame, primitive_region + 24))
                        .map_err(|_| "negative argc".to_string())?;
                    let args = (0..argc)
                        .map(|i| read_frame_word(frame, primitive_region + 32 + i * 8))
                        .collect::<Vec<_>>();
                    let invocation = pending_invocation_for(&lowered_fns[fn_ref], store_cell, args);
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
                let input_slot = read_frame_word(frame, primitive_region) as usize;
                let pending = read_frame_word(frame, primitive_region + 8);
                let argc = usize::try_from(read_frame_word(frame, primitive_region + 16))
                    .unwrap_or_else(|_| panic!("negative pending invoke argc"));
                let args = (0..argc)
                    .map(|i| read_frame_word(frame, primitive_region + 24 + i * 8))
                    .collect();
                pending_invokes.push(PendingInvokeRequest {
                    caller: exec_ix,
                    input_slot,
                    pending,
                    args,
                });
            };

            let mut hosts: [HostFn<'_>; 28] = [
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
            ];
            let step = exec
                .task
                .advance(&self.program, &exec.ready, &exec.awaited, &mut hosts);
            drop(hosts);
            for event in store_events.into_inner() {
                self.emit(event);
            }
            if let Some(err) = host_error.into_inner() {
                return Burst::Error(err);
            }

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
                        project_requests,
                        exec_requests,
                        fetch_requests,
                        doc_parse_requests,
                        option_unwraps,
                        pending_coercions,
                        pending_invokes,
                        parked_input: input,
                    };
                }
            }
        }
    }

    fn memo_key(&self, fn_ref: usize, args: &[i64]) -> CanonMemoKey {
        let lowered = &self.fns[fn_ref];
        let args = args
            .iter()
            .zip(&lowered.arg_schemas)
            .map(|(&word, schema)| self.canonical_word_hash(schema, word))
            .collect();
        (lowered.hash, args)
    }

    fn canonical_word_hash(&self, schema: &str, word: i64) -> ContentHash {
        let store = self.store.borrow();
        canonical_word_hash_in_store(&store, schema, word)
    }

    fn canonicalize_return_word(&self, fn_ref: usize, word: i64) -> i64 {
        if self.fns[fn_ref].return_schema == "Float" {
            canonicalize_word_for_schema("Float", word)
        } else {
            word
        }
    }

    fn project_request(&mut self, req: ProjectRequest) -> Result<(usize, i64), String> {
        let path = self.store.borrow().string_value(req.path, "Path")?;
        let value = self.project_tree_path(req.tree, &path)?;
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
                    let invocation = self.store.borrow().pending_invocation(handle)?;
                    if invocation.primitive.is_some() {
                        return Err("primitive pending value cannot produce a tree".into());
                    }
                    let fn_ref = self.fn_ref_for_hash(invocation.closure_hash)?;
                    let value = self.demand(fn_ref, invocation.args)?;
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

    fn force_tree_handle(&mut self, tree_handle: i64) -> Result<i64, String> {
        let tree = self.store.borrow().tree_entry(tree_handle)?;
        match tree {
            TreeEntry::Concrete(_) => Ok(tree_handle),
            TreeEntry::Merge(pending) => {
                let mut merged = crate::exec::Tree::default();
                for handle in pending {
                    let invocation = self.store.borrow().pending_invocation(handle)?;
                    if invocation.primitive.is_some() {
                        return Err("primitive pending value cannot produce a tree".into());
                    }
                    let fn_ref = self.fn_ref_for_hash(invocation.closure_hash)?;
                    let value = self.demand(fn_ref, invocation.args)?;
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

    fn pending_coercion(
        &mut self,
        req: PendingCoerceRequest,
        caller: usize,
    ) -> Result<PendingForce, String> {
        let entry_schema = self
            .store
            .borrow()
            .entry(req.pending)
            .ok_or_else(|| format!("store handle {}", req.pending))?
            .schema
            .clone();
        let Some(value_schema) = pending_value_schema(&entry_schema) else {
            return Err(format!(
                "pending coercion expected Pending<T>, got {entry_schema}"
            ));
        };
        let invocation = self.store.borrow().pending_invocation(req.pending)?;
        if invocation.remaining_arity != 0 {
            return Err(format!(
                "cannot coerce pending {value_schema} with {} remaining args",
                invocation.remaining_arity
            ));
        }
        if let Some(primitive) = invocation.primitive {
            let value = match primitive {
                PendingPrimitive::Elf { projection } => {
                    self.force_elf_projection(projection, &invocation.args)?
                }
                PendingPrimitive::Ast { projection } => {
                    self.force_ast_projection(projection, &invocation.args)?
                }
            };
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
        if args.len() != self.fns[fn_ref].arg_schemas.len() {
            return Err(format!(
                "pending invocation completed to {} argument(s), expected {}",
                args.len(),
                self.fns[fn_ref].arg_schemas.len()
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
            .option_payload(req.option, &self.schema_refs)?
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

    fn fn_ref_for_hash(&self, closure_hash: u64) -> Result<usize, String> {
        self.fns
            .iter()
            .position(|lowered| lowered.hash == closure_hash)
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
                    argv.push(self.store.borrow().string_value(handle, "String")?);
                }
                CommandRequestPart::Splice(word) => {
                    self.splice_word_into_command(word, &mut argv, &mut mounts)?;
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
        let identity = pending_exec_identity_hash(&req.command, &plan, cap_hash, &mounts);
        let run_id = self.next_run_id;
        self.next_run_id = self.next_run_id.saturating_add(1);
        let timestamp_us = self.next_timestamp();
        self.emit(DriveEvent::RunRequested {
            command: hash_u64(&req.command),
            output: hash_u64(&output),
            run_id,
            command_name: req.command.clone(),
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
                span: req.span,
            },
        );
        let handle = self.store.borrow_mut().alloc_tree_exec(run_id, identity).0;
        Ok((req.input_slot, handle))
    }

    fn ensure_run_started(&mut self, run_id: u64) -> Result<(), String> {
        let needs_start = self
            .runs
            .get(&run_id)
            .ok_or_else(|| format!("run {run_id}"))?
            .completed
            .is_none()
            && self
                .runs
                .get(&run_id)
                .ok_or_else(|| format!("run {run_id}"))?
                .remote
                .is_none();
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
        if needs_start {
            let (command, plan, capability, mounts) = {
                let run = self.runs.get(&run_id).expect("run checked");
                (
                    run.command.clone(),
                    run.plan.clone(),
                    run.capability,
                    run.mounts.clone(),
                )
            };
            if let Some(backend) = &self.exec_backend {
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
                let run = self.runs.get_mut(&run_id).expect("run checked");
                run.remote = Some(remote);
            } else {
                let tool = tool_for(&command)?;
                let outcome = self.exec_cache.exec(&plan, capability, &mounts, tool)?;
                let event = self
                    .exec_cache
                    .events
                    .last()
                    .cloned()
                    .expect("exec pushed an event");
                let run = self.runs.get_mut(&run_id).expect("run checked");
                run.completed = Some((outcome, event));
            }
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
        if let Some(remote) = self.runs.get(&run_id).and_then(|run| run.remote.clone()) {
            let (tree, event) = remote.flush()?;
            let outcome = crate::exec::Outcome {
                outputs: tree,
                read_set: crate::exec::ReadSet::default(),
            };
            let run = self.runs.get_mut(&run_id).expect("run checked");
            run.completed = Some((outcome.clone(), event));
            return Ok(outcome);
        }
        Err(format!(
            "scheduled run {run_id} has no local or remote completion"
        ))
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

    fn splice_word_into_command(
        &mut self,
        word: i64,
        argv: &mut Vec<String>,
        mounts: &mut Vec<crate::exec::Mount>,
    ) -> Result<(), String> {
        let entry = self
            .store
            .borrow()
            .entry(word)
            .cloned()
            .ok_or_else(|| format!("cannot splice scalar word {word} into a command"))?;
        match entry.schema.as_str() {
            "Path" | "String" | "Flag" => {
                argv.push(String::from_utf8(entry.bytes).map_err(|err| err.to_string())?);
            }
            "Array" => match { self.store.borrow().array_entry(word, &self.schema_refs)? } {
                ArrayEntry::Words { words, .. } => {
                    for word in words {
                        self.splice_word_into_command(word, argv, mounts)?;
                    }
                }
                ArrayEntry::Pending(_) => {
                    return Err("pending arrays cannot be spliced into commands".into());
                }
            },
            "Tree" => {
                let forced = self.force_tree_handle(word)?;
                let TreeEntry::Concrete(tree) = self.store.borrow().tree_entry(forced)? else {
                    return Err("forced command tree stayed pending".into());
                };
                let root = format!("/m/{}", mounts.len());
                let entry_count = tree.entries.len() + tree.blobs.len();
                let text = if entry_count == 1 {
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
                mounts.push(crate::exec::Mount { at: root, tree });
                argv.push(text);
            }
            other => return Err(format!("cannot splice {other} into a command")),
        }
        Ok(())
    }

    fn fetch_request(&mut self, req: FetchRequest) -> Result<(usize, i64), String> {
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
        let (tree, replayed, pin) = if let Some(pin) = self.journal.get(&key).copied() {
            let pinned = self.store.borrow().string_value(pin, "String")?;
            let fetched = self.fetch_backend.fetch(&url, Some(&pinned))?;
            (fetched.tree, true, pinned)
        } else {
            let fetched = self.fetch_backend.fetch(&url, declared_sha256.as_deref())?;
            (fetched.tree, false, fetched.actual_sha256)
        };
        if !replayed {
            let pin_handle = self
                .store
                .borrow_mut()
                .alloc_raw("String", pin.into_bytes())
                .0;
            self.journal.insert(key.clone(), pin_handle);
        }
        let timestamp_us = self.next_timestamp();
        self.emit(DriveEvent::Observation {
            key: hash_u64(&key),
            replayed,
            key_text: key,
            timestamp_us,
        });
        let handle = self.store.borrow_mut().alloc_tree_concrete(tree).0;
        Ok((req.input_slot, handle))
    }

    fn doc_parse_request(&mut self, req: DocParseRequest) -> Result<(usize, i64), String> {
        let input = self.document_input_value(req.input)?;
        let value = match req.kind {
            DocParseKind::Toml => crate::data::parse_toml(input)?,
            DocParseKind::Json => crate::data::parse_json(input)?,
        };
        let handle =
            alloc_doc_from_value(&self.store, &self.descriptors, &self.schema_refs, value)?;
        Ok((req.input_slot, handle))
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
        let mut hasher = Sha256::new();
        hasher.update(b"vix-elf-input");
        hasher.update(&bytes);
        let input_hash: ContentHash = hasher.finalize().into();
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
        let handle =
            alloc_doc_from_value(&self.store, &self.descriptors, &self.schema_refs, value)?;
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
                &self.schema_refs,
                super::ast_probe::items(&file),
            )?,
            super::ast_probe::Projection::Fns => alloc_doc_from_value(
                &self.store,
                &self.descriptors,
                &self.schema_refs,
                super::ast_probe::fns(&file),
            )?,
            super::ast_probe::Projection::Fn => {
                let item = super::ast_probe::fn_item(&file, &name)?;
                alloc_ast_fn_doc(
                    &self.store,
                    &self.descriptors,
                    &self.schema_refs,
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
                    &self.schema_refs,
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
        let mut hasher = Sha256::new();
        hasher.update(b"vix-ast-input");
        hasher.update(source.as_bytes());
        Ok((source, hasher.finalize().into()))
    }
}

enum Burst {
    Done(i64),
    Pending {
        new_requests: Vec<InvokeRequest>,
        project_requests: Vec<ProjectRequest>,
        exec_requests: Vec<ExecRequest>,
        fetch_requests: Vec<FetchRequest>,
        doc_parse_requests: Vec<DocParseRequest>,
        option_unwraps: Vec<OptionUnwrapRequest>,
        pending_coercions: Vec<PendingCoerceRequest>,
        pending_invokes: Vec<PendingInvokeRequest>,
        parked_input: usize,
    },
    Error(String),
}

enum PendingForce {
    Invoke(InvokeRequest),
    Ready { input_slot: usize, value: i64 },
}

fn hash_u64(value: impl Hash) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut h);
    std::hash::Hasher::finish(&h)
}

#[derive(Clone, Debug)]
enum DocPayload {
    Null,
    Bool(i64),
    Int(i64),
    Float(i64),
    String(i64),
    Array(i64),
    Map(i64),
}

fn alloc_doc_from_value(
    store: &RefCell<ValueStore>,
    descriptors: &HashMap<String, Descriptor<String>>,
    schema_refs: &[String],
    value: Value,
) -> Result<i64, String> {
    match value {
        Value::Bool(value) => alloc_doc_variant(store, descriptors, 1, &[i64::from(value)]),
        Value::Int(value) => alloc_doc_variant(store, descriptors, 2, &[value]),
        Value::Float(value) => alloc_doc_variant(
            store,
            descriptors,
            3,
            &[super::value::TotalF64::new(value).get().to_bits() as i64],
        ),
        Value::Str(value) => {
            let handle = store.borrow_mut().alloc_raw("String", value.into_bytes()).0;
            alloc_doc_variant(store, descriptors, 4, &[handle])
        }
        Value::Array(values) => {
            let words = values
                .into_iter()
                .map(|value| alloc_doc_from_value(store, descriptors, schema_refs, value))
                .collect::<Result<Vec<_>, _>>()?;
            let handle = store
                .borrow_mut()
                .alloc_array_words("Doc", words, schema_refs)?
                .0;
            alloc_doc_variant(store, descriptors, 5, &[handle])
        }
        Value::Map(entries) => {
            let mut pairs = Vec::new();
            for (key, value) in entries {
                let Value::Str(key) = key else {
                    return Err(format!("document object key must be a string, got {key:?}"));
                };
                let key_word = store.borrow_mut().alloc_raw("String", key.into_bytes()).0;
                let value_word = alloc_doc_from_value(store, descriptors, schema_refs, value)?;
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
                .alloc_map("Map<String,Doc>", pairs, schema_refs, descriptors)?
                .0;
            alloc_doc_variant(store, descriptors, 6, &[handle])
        }
        Value::Variant {
            enum_name,
            name,
            index,
            ..
        } if enum_name == "Option" && name == "None" && index == 1 => {
            alloc_doc_variant(store, descriptors, 0, &[])
        }
        other => Err(format!("document value {other:?} is outside the B5 subset")),
    }
}

fn alloc_doc_variant(
    store: &RefCell<ValueStore>,
    descriptors: &HashMap<String, Descriptor<String>>,
    variant_index: u64,
    fields: &[i64],
) -> Result<i64, String> {
    let descriptor = descriptors
        .get("Doc")
        .ok_or_else(|| "missing Doc descriptor".to_string())?;
    let mut bytes = vec![0u8; descriptor.layout.size];
    write_variant_tag(&mut bytes, descriptor, variant_index);
    for (index, value) in fields.iter().enumerate() {
        let offset = field_offset(descriptor, &bytes, index);
        bytes[offset..offset + 8].copy_from_slice(&value.to_le_bytes());
    }
    Ok(store.borrow_mut().alloc("Doc", bytes, descriptors).0)
}

fn doc_payload(
    store: &ValueStore,
    descriptors: &HashMap<String, Descriptor<String>>,
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
        5 => DocPayload::Array(read_frame_word(
            &entry.bytes,
            field_offset(descriptor, &entry.bytes, 0),
        )),
        6 => DocPayload::Map(read_frame_word(
            &entry.bytes,
            field_offset(descriptor, &entry.bytes, 0),
        )),
        other => return Err(format!("unknown Doc tag {other}")),
    })
}

fn doc_get(
    store: &RefCell<ValueStore>,
    descriptors: &HashMap<String, Descriptor<String>>,
    schema_refs: &[String],
    doc: i64,
    key: &str,
) -> Result<(i64, bool), String> {
    let map = match doc_payload(&store.borrow(), descriptors, doc)? {
        DocPayload::Map(handle) => handle,
        other => return Err(format!("Doc.get expected object, got {other:?}")),
    };
    let key_word = store
        .borrow_mut()
        .alloc_raw("String", key.as_bytes().to_vec())
        .0;
    store.borrow_mut().map_get(
        map,
        "String",
        key_word,
        "Realized<Doc>",
        descriptors,
        schema_refs,
    )
}

fn doc_coerce(
    store: &RefCell<ValueStore>,
    descriptors: &HashMap<String, Descriptor<String>>,
    doc: i64,
    schema: &str,
) -> Result<i64, String> {
    if schema == "Doc" {
        return Ok(doc);
    }
    match (doc_payload(&store.borrow(), descriptors, doc)?, schema) {
        (DocPayload::Bool(value), "Bool") => Ok(value),
        (DocPayload::Int(value), "Int") => Ok(value),
        (DocPayload::Float(value), "Float") => Ok(value),
        (DocPayload::String(value), "String") => Ok(value),
        (DocPayload::Array(value), "Array") => Ok(value),
        (DocPayload::Map(value), "Map<String,Doc>") => Ok(value),
        (payload, schema) => Err(format!("cannot coerce Doc::{payload:?} to {schema}")),
    }
}

fn alloc_elf_doc(
    store: &RefCell<ValueStore>,
    descriptors: &HashMap<String, Descriptor<String>>,
    schema_refs: &[String],
    input: i64,
) -> Result<i64, String> {
    let input_hash = {
        let store = store.borrow();
        let entry = store
            .entry(input)
            .ok_or_else(|| format!("store handle {input}"))?;
        match entry.schema.as_str() {
            "Blob" | "String" => entry.content_hash,
            "Tree" => {
                let TreeEntry::Concrete(_) = store.tree_entry(input)? else {
                    return Err("elf input tree must be concrete at probe creation".into());
                };
                entry.content_hash
            }
            other => {
                return Err(format!(
                    "elf input must be Blob, String, or Tree, got {other}"
                ));
            }
        }
    };
    let mut pairs = Vec::new();
    for projection in super::elf::Projection::ALL {
        let key = store
            .borrow_mut()
            .alloc_raw("String", projection.name().as_bytes().to_vec())
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
        .alloc_map("Map<String,Doc>", pairs, schema_refs, descriptors)?
        .0;
    alloc_doc_variant(store, descriptors, 6, &[map])
}

fn elf_projection_pending(
    input: i64,
    input_hash: ContentHash,
    projection: super::elf::Projection,
) -> PendingInvocation {
    let mut hasher = Sha256::new();
    hasher.update(b"vix-elf-projection");
    hasher.update(projection.name().as_bytes());
    hasher.update(input_hash);
    let identity_hash = hasher.finalize().into();
    PendingInvocation {
        closure_hash: hash_u64(("elf", projection.name())),
        primitive: Some(PendingPrimitive::Elf { projection }),
        args: vec![input],
        remaining_arity: 0,
        identity_hash,
    }
}

fn alloc_ast_doc(
    store: &RefCell<ValueStore>,
    descriptors: &HashMap<String, Descriptor<String>>,
    schema_refs: &[String],
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
    alloc_doc_object(store, descriptors, schema_refs, rows)
}

fn alloc_ast_fn_doc(
    store: &RefCell<ValueStore>,
    descriptors: &HashMap<String, Descriptor<String>>,
    schema_refs: &[String],
    input: i64,
    input_hash: ContentHash,
    item: &ast::FnItem,
) -> Result<i64, String> {
    let name_handle = store
        .borrow_mut()
        .alloc_raw("String", item.name.value.as_bytes().to_vec())
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
        schema_refs,
        vec![
            (
                "span".to_string(),
                "Doc".to_string(),
                alloc_doc_from_value(
                    store,
                    descriptors,
                    schema_refs,
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
            let handle = alloc_doc_from_value(store, descriptors, schema_refs, value)?;
            Ok((key, "Doc".to_string(), handle))
        })
        .collect::<Result<Vec<_>, String>>()?;
    rows.push(("body".to_string(), "Doc".to_string(), body));
    alloc_doc_object(store, descriptors, schema_refs, rows)
}

fn ast_projection_pending(
    input: i64,
    input_hash: ContentHash,
    projection: super::ast_probe::Projection,
    extra_args: Vec<i64>,
) -> PendingInvocation {
    let mut hasher = Sha256::new();
    hasher.update(b"vix-ast-projection");
    hasher.update(projection.name().as_bytes());
    hasher.update(input_hash);
    for arg in &extra_args {
        hasher.update(arg.to_le_bytes());
    }
    let mut args = Vec::with_capacity(1 + extra_args.len());
    args.push(input);
    args.extend(extra_args);
    PendingInvocation {
        closure_hash: hash_u64(("ast", projection.name())),
        primitive: Some(PendingPrimitive::Ast { projection }),
        args,
        remaining_arity: 0,
        identity_hash: hasher.finalize().into(),
    }
}

fn alloc_doc_object(
    store: &RefCell<ValueStore>,
    descriptors: &HashMap<String, Descriptor<String>>,
    schema_refs: &[String],
    rows: Vec<(String, String, i64)>,
) -> Result<i64, String> {
    let mut pairs = Vec::with_capacity(rows.len());
    for (key, value_schema, value_word) in rows {
        let key_word = store.borrow_mut().alloc_raw("String", key.into_bytes()).0;
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
        .alloc_map("Map<String,Doc>", pairs, schema_refs, descriptors)?
        .0;
    alloc_doc_variant(store, descriptors, 6, &[map])
}

fn compare_words(
    store: &ValueStore,
    descriptors: &HashMap<String, Descriptor<String>>,
    schema_refs: &[String],
    schema: &str,
    a: i64,
    b: i64,
) -> Result<Ordering, String> {
    if a == b && schema != "Float" {
        return Ok(Ordering::Equal);
    }
    match schema {
        "Int" | "Bool" => Ok(a.cmp(&b)),
        "Float" => {
            let a = super::value::TotalF64::new(f64::from_bits(canonicalize_word_for_schema(
                "Float", a,
            ) as u64));
            let b = super::value::TotalF64::new(f64::from_bits(canonicalize_word_for_schema(
                "Float", b,
            ) as u64));
            Ok(a.cmp(&b))
        }
        "Blob" => {
            let a = store.entry(a).ok_or_else(|| format!("store handle {a}"))?;
            let b = store.entry(b).ok_or_else(|| format!("store handle {b}"))?;
            Ok(a.bytes.cmp(&b.bytes))
        }
        "String" | "Path" | "Flag" | "Cc" | "Ar" | "Rustc" => {
            let a = store.string_value(a, schema)?;
            let b = store.string_value(b, schema)?;
            Ok(a.cmp(&b))
        }
        "Array" => compare_arrays(store, descriptors, schema_refs, a, b),
        schema if schema.starts_with("Map<") => compare_maps(store, descriptors, schema_refs, a, b),
        "Doc" => compare_docs(store, descriptors, schema_refs, a, b),
        schema => compare_declared_value(store, descriptors, schema_refs, schema, a, b),
    }
}

fn compare_arrays(
    store: &ValueStore,
    descriptors: &HashMap<String, Descriptor<String>>,
    schema_refs: &[String],
    a: i64,
    b: i64,
) -> Result<Ordering, String> {
    let (a_schema, a_words) = match store.array_entry(a, schema_refs)? {
        ArrayEntry::Words { elem_schema, words } => (elem_schema, words),
        ArrayEntry::Pending(_) => return Ok(a.cmp(&b)),
    };
    let (b_schema, b_words) = match store.array_entry(b, schema_refs)? {
        ArrayEntry::Words { elem_schema, words } => (elem_schema, words),
        ArrayEntry::Pending(_) => return Ok(a.cmp(&b)),
    };
    let schema_order = a_schema.cmp(&b_schema);
    if schema_order != Ordering::Equal {
        return Ok(schema_order);
    }
    compare_word_slices(
        store,
        descriptors,
        schema_refs,
        &a_schema,
        &a_words,
        &b_words,
    )
}

fn compare_maps(
    store: &ValueStore,
    descriptors: &HashMap<String, Descriptor<String>>,
    schema_refs: &[String],
    a: i64,
    b: i64,
) -> Result<Ordering, String> {
    let (a_schema, a_pairs) = store.map_pairs(a, schema_refs)?;
    let (b_schema, b_pairs) = store.map_pairs(b, schema_refs)?;
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
            schema_refs,
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
            schema_refs,
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
    descriptors: &HashMap<String, Descriptor<String>>,
    schema_refs: &[String],
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
            compare_words(store, descriptors, schema_refs, "Float", a, b)
        }
        (DocPayload::String(a), DocPayload::String(b)) => {
            compare_words(store, descriptors, schema_refs, "String", a, b)
        }
        (DocPayload::Array(a), DocPayload::Array(b)) => {
            compare_words(store, descriptors, schema_refs, "Array", a, b)
        }
        (DocPayload::Map(a), DocPayload::Map(b)) => {
            compare_words(store, descriptors, schema_refs, "Map<String,Doc>", a, b)
        }
        _ => Ok(Ordering::Equal),
    }
}

fn compare_declared_value(
    store: &ValueStore,
    descriptors: &HashMap<String, Descriptor<String>>,
    schema_refs: &[String],
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
        schema_refs,
        descriptor,
        &a_entry.bytes,
        &b_entry.bytes,
    )
}

fn compare_descriptor_bytes(
    store: &ValueStore,
    descriptors: &HashMap<String, Descriptor<String>>,
    schema_refs: &[String],
    descriptor: &Descriptor<String>,
    a: &[u8],
    b: &[u8],
) -> Result<Ordering, String> {
    match &descriptor.access {
        Access::Scalar if descriptor.schema == "Float" => compare_words(
            store,
            descriptors,
            schema_refs,
            "Float",
            read_frame_word(a, 0),
            read_frame_word(b, 0),
        ),
        Access::Scalar => Ok(a.cmp(b)),
        Access::Handle { target } => compare_words(
            store,
            descriptors,
            schema_refs,
            target,
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
                    schema_refs,
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
                    schema_refs,
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
                    schema_refs,
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
    descriptors: &HashMap<String, Descriptor<String>>,
    schema_refs: &[String],
    schema: &str,
    a: &[i64],
    b: &[i64],
) -> Result<Ordering, String> {
    for (a, b) in a.iter().zip(b) {
        let order = compare_words(store, descriptors, schema_refs, schema, *a, *b)?;
        if order != Ordering::Equal {
            return Ok(order);
        }
    }
    Ok(a.len().cmp(&b.len()))
}

fn render_word(
    store: &ValueStore,
    descriptors: &HashMap<String, Descriptor<String>>,
    schema_refs: &[String],
    names: &RenderNames,
    schema: &str,
    word: i64,
) -> Result<RenderedValue, String> {
    if pending_value_schema(schema).is_some() {
        return Ok(RenderedValue::Pending {
            pending: render_pending(store, word)?,
        });
    }
    if schema.starts_with("Option<") {
        return render_option(store, descriptors, schema_refs, names, word);
    }
    match schema {
        "Int" => Ok(RenderedValue::Int { value: word }),
        "Float" => {
            let bits = canonicalize_word_for_schema("Float", word) as u64;
            let value = f64::from_bits(bits);
            Ok(RenderedValue::Float {
                bits: format!("{bits:016x}"),
                value: if value.is_nan() {
                    "NaN".to_string()
                } else {
                    value.to_string()
                },
            })
        }
        "Bool" => Ok(RenderedValue::Bool { value: word != 0 }),
        "String" => Ok(RenderedValue::String {
            value: store.string_value(word, "String")?,
        }),
        "Path" => Ok(RenderedValue::Path {
            value: store.string_value(word, "Path")?,
        }),
        "Flag" => Ok(RenderedValue::Flag {
            value: store.string_value(word, "Flag")?,
        }),
        "Tree" => render_tree(store, word),
        "Array" => render_array(store, descriptors, schema_refs, names, word),
        "Doc" => render_doc(store, descriptors, schema_refs, names, word),
        schema if schema.starts_with("Map<") => {
            render_map(store, descriptors, schema_refs, names, schema, word)
        }
        schema => {
            if schema == "Blob" {
                let entry = store
                    .entry(word)
                    .ok_or_else(|| format!("store handle {word}"))?;
                return Ok(RenderedValue::Raw {
                    schema: schema.to_string(),
                    bytes_utf8: String::from_utf8(entry.bytes.clone()).ok(),
                });
            }
            if matches!(schema, "Cc" | "Ar" | "Rustc") {
                return Ok(RenderedValue::Raw {
                    schema: schema.to_string(),
                    bytes_utf8: Some(store.string_value(word, schema)?),
                });
            }
            render_declared(store, descriptors, schema_refs, names, schema, word)
        }
    }
}

fn render_option(
    store: &ValueStore,
    descriptors: &HashMap<String, Descriptor<String>>,
    schema_refs: &[String],
    names: &RenderNames,
    handle: i64,
) -> Result<RenderedValue, String> {
    let entry = store
        .entry(handle)
        .ok_or_else(|| format!("store handle {handle}"))?;
    let value_schema = entry
        .schema
        .strip_prefix("Option<")
        .and_then(|inner| inner.strip_suffix('>'))
        .ok_or_else(|| format!("handle {handle} is `{}`, not an Option", entry.schema))?;
    match store.option_payload(handle, schema_refs)? {
        OptionPayload::None => Ok(RenderedValue::Enum {
            schema: entry.schema.clone(),
            variant_index: 0,
            variant: "None".to_string(),
            fields: Vec::new(),
        }),
        OptionPayload::Some { word, realization } => {
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
                        schema_refs,
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

fn render_array(
    store: &ValueStore,
    descriptors: &HashMap<String, Descriptor<String>>,
    schema_refs: &[String],
    names: &RenderNames,
    handle: i64,
) -> Result<RenderedValue, String> {
    match store.array_entry(handle, schema_refs)? {
        ArrayEntry::Words { elem_schema, words } => Ok(RenderedValue::Array {
            element_schema: elem_schema.clone(),
            items: words
                .into_iter()
                .map(|word| render_word(store, descriptors, schema_refs, names, &elem_schema, word))
                .collect::<Result<Vec<_>, _>>()?,
        }),
        ArrayEntry::Pending(handles) => Ok(RenderedValue::Array {
            element_schema: "Pending<Tree>".to_string(),
            items: handles
                .into_iter()
                .map(|handle| {
                    render_word(
                        store,
                        descriptors,
                        schema_refs,
                        names,
                        "Pending<Tree>",
                        handle,
                    )
                })
                .collect::<Result<Vec<_>, _>>()?,
        }),
    }
}

fn render_map(
    store: &ValueStore,
    descriptors: &HashMap<String, Descriptor<String>>,
    schema_refs: &[String],
    names: &RenderNames,
    schema: &str,
    handle: i64,
) -> Result<RenderedValue, String> {
    let (_, pairs) = store.map_pairs(handle, schema_refs)?;
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
                        schema_refs,
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
                        schema_refs,
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
    descriptors: &HashMap<String, Descriptor<String>>,
    schema_refs: &[String],
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
                schema_refs,
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
                schema_refs,
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
                schema_refs,
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
                schema_refs,
                names,
                "String",
                word,
            )?)),
        ),
        DocPayload::Array(word) => (
            "Array".to_string(),
            Some(Box::new(render_word(
                store,
                descriptors,
                schema_refs,
                names,
                "Array",
                word,
            )?)),
        ),
        DocPayload::Map(word) => (
            "Map".to_string(),
            Some(Box::new(render_word(
                store,
                descriptors,
                schema_refs,
                names,
                "Map<String,Doc>",
                word,
            )?)),
        ),
    };
    Ok(RenderedValue::Doc { variant, value })
}

fn render_declared(
    store: &ValueStore,
    descriptors: &HashMap<String, Descriptor<String>>,
    schema_refs: &[String],
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
                    let field_schema = descriptor_word_schema(&field.descriptor);
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
                            schema_refs,
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
                    let field_schema = descriptor_word_schema(&field.descriptor);
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
                            schema_refs,
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

fn descriptor_word_schema(descriptor: &Descriptor<String>) -> String {
    match &descriptor.access {
        Access::Handle { target } => target.clone(),
        Access::Scalar if descriptor.schema.starts_with("Int") => "Int".to_string(),
        Access::Scalar if descriptor.schema.starts_with("Float") => "Float".to_string(),
        Access::Scalar if descriptor.schema.starts_with("Bool") => "Bool".to_string(),
        _ => descriptor.schema.clone(),
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
        Op::Ret { .. } => "Ret",
        Op::Await { .. } => "Await",
        Op::LoadIndexedI64 { .. } => "LoadIndexedI64",
        Op::StoreIndexedI64 { .. } => "StoreIndexedI64",
        Op::ConstF64 { .. } => "ConstF64",
        Op::AddF64 { .. } => "AddF64",
        Op::MulF64 { .. } => "MulF64",
        Op::Trace { .. } => "Trace",
        Op::HostCall { .. } => "HostCall",
    }
}

fn memo_key_hash(key: &CanonMemoKey) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    key.0.hash(&mut h);
    for arg in &key.1 {
        arg.hash(&mut h);
    }
    std::hash::Hasher::finish(&h)
}

fn pending_invocation_for(
    lowered: &LoweredFn,
    store: &RefCell<ValueStore>,
    args: Vec<i64>,
) -> PendingInvocation {
    let store = store.borrow();
    let identity_hash = pending_identity_hash(lowered, &store, &args);
    PendingInvocation {
        closure_hash: lowered.hash,
        primitive: None,
        remaining_arity: lowered.arg_schemas.len().saturating_sub(args.len()),
        args,
        identity_hash,
    }
}

fn pending_identity_hash(lowered: &LoweredFn, store: &ValueStore, args: &[i64]) -> ContentHash {
    let mut hasher = Sha256::new();
    hasher.update(b"vix-pending-invocation");
    hasher.update(lowered.hash.to_le_bytes());
    hasher.update(
        i64::try_from(lowered.arg_schemas.len().saturating_sub(args.len()))
            .expect("remaining arity fits i64")
            .to_le_bytes(),
    );
    for (&word, schema) in args.iter().zip(&lowered.arg_schemas) {
        hasher.update(schema.as_bytes());
        hasher.update(canonical_word_hash_in_store(store, schema, word));
    }
    hasher.finalize().into()
}

fn read_frame_word(frame: &[u8], at: usize) -> i64 {
    i64::from_le_bytes(frame[at..at + 8].try_into().expect("frame word"))
}

fn write_frame_word(frame: &mut [u8], at: usize, value: i64) {
    frame[at..at + 8].copy_from_slice(&value.to_le_bytes());
}

fn canonicalize_word_for_schema(schema: &str, word: i64) -> i64 {
    if schema == "Float" {
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

fn canonical_word_hash_in_store(store: &ValueStore, schema: &str, word: i64) -> ContentHash {
    match store.entry(word) {
        Some(entry) if entry.schema == schema => return entry.content_hash,
        _ => {}
    }
    let word = canonicalize_word_for_schema(schema, word);
    let mut hasher = Sha256::new();
    hasher.update(b"vix-scalar-word");
    hasher.update(schema.as_bytes());
    hasher.update(word.to_le_bytes());
    hasher.finalize().into()
}

fn canonical_map_pairs(
    store: &ValueStore,
    pairs: Vec<MapPair>,
    descriptors: &HashMap<String, Descriptor<String>>,
    schema_refs: &[String],
) -> Result<Vec<OrderedMapPair>, String> {
    let mut pairs: Vec<OrderedMapPair> = pairs
        .into_iter()
        .map(|mut pair| {
            pair.key_word = canonicalize_word_for_schema(&pair.key_schema, pair.key_word);
            pair.value_word = canonicalize_word_for_schema(&pair.value_schema, pair.value_word);
            let key_hash = canonical_word_hash_in_store(store, &pair.key_schema, pair.key_word);
            let value_hash =
                canonical_word_hash_in_store(store, &pair.value_schema, pair.value_word);
            OrderedMapPair {
                pair,
                key_hash,
                value_hash,
            }
        })
        .collect();
    pairs.sort_by(|a, b| {
        let schema_order = a.pair.key_schema.cmp(&b.pair.key_schema);
        if schema_order != Ordering::Equal {
            return schema_order;
        }
        compare_words(
            store,
            descriptors,
            schema_refs,
            &a.pair.key_schema,
            a.pair.key_word,
            b.pair.key_word,
        )
        .unwrap_or_else(|_| a.key_hash.cmp(&b.key_hash))
    });
    let mut deduped: Vec<OrderedMapPair> = Vec::new();
    for pair in pairs {
        match deduped.last_mut() {
            Some(last)
                if last.pair.key_schema == pair.pair.key_schema
                    && last.key_hash == pair.key_hash =>
            {
                *last = pair;
                continue;
            }
            _ => {}
        }
        deduped.push(pair);
    }
    Ok(deduped)
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

fn encode_map_pairs(pairs: &[OrderedMapPair], schema_refs: &[String]) -> Result<Vec<u8>, String> {
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
        bytes.extend_from_slice(&schema_ref_for(&pair.pair.key_schema, schema_refs)?.to_le_bytes());
        bytes.extend_from_slice(&pair.pair.key_word.to_le_bytes());
        bytes.extend_from_slice(
            &schema_ref_for(&pair.pair.value_schema, schema_refs)?.to_le_bytes(),
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

fn decode_map_pairs(bytes: &[u8], schema_refs: &[String]) -> Result<Vec<MapPair>, String> {
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
        let key_schema = schema_name_for(read_frame_word(bytes, at), schema_refs)?;
        let key_word = read_frame_word(bytes, at + 8);
        let value_schema = schema_name_for(read_frame_word(bytes, at + 16), schema_refs)?;
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

fn hash_map_pairs(schema: &str, pairs: &[OrderedMapPair]) -> ContentHash {
    let mut hasher = Sha256::new();
    hasher.update(b"vix-map");
    hasher.update(schema.as_bytes());
    hasher.update(
        i64::try_from(pairs.len())
            .expect("map pair count fits i64")
            .to_le_bytes(),
    );
    for pair in pairs {
        hasher.update(pair.pair.key_schema.as_bytes());
        hasher.update(pair.key_hash);
        hasher.update(pair.pair.value_schema.as_bytes());
        if let Some(realization) = &pair.pair.value_realization {
            hasher.update(realization.to_word().to_le_bytes());
        }
        hasher.update(pair.value_hash);
    }
    hasher.finalize().into()
}

fn map_realization_bitset_words(count: usize) -> usize {
    count.div_ceil(64)
}

fn schema_ref_for(schema: &str, schema_refs: &[String]) -> Result<i64, String> {
    schema_refs
        .iter()
        .position(|candidate| candidate == schema)
        .map(|index| i64::try_from(index).expect("schema ref fits i64"))
        .ok_or_else(|| format!("schema `{schema}` has no schema ref"))
}

fn schema_name_for(schema_ref: i64, schema_refs: &[String]) -> Result<String, String> {
    let index = usize::try_from(schema_ref).map_err(|_| format!("schema ref {schema_ref}"))?;
    schema_refs
        .get(index)
        .cloned()
        .ok_or_else(|| format!("schema ref {schema_ref}"))
}

fn option_schema(value_schema: &str) -> String {
    format!("Option<{value_schema}>")
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
    let inner = schema.strip_prefix("Map<")?.strip_suffix('>')?;
    let (key, value) = inner.split_once(',')?;
    Some((key, value))
}

fn pending_primitive_to_word(primitive: &PendingPrimitive) -> i64 {
    match primitive {
        PendingPrimitive::Elf { projection } => projection.to_word(),
        PendingPrimitive::Ast { projection } => projection.to_word(),
    }
}

fn pending_primitive_from_word(word: i64) -> Result<PendingPrimitive, String> {
    if word >= 1000 {
        Ok(PendingPrimitive::Ast {
            projection: super::ast_probe::Projection::from_word(word)?,
        })
    } else {
        Ok(PendingPrimitive::Elf {
            projection: super::elf::Projection::from_word(word)?,
        })
    }
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
        .map(pending_primitive_to_word)
        .unwrap_or(-1);
    bytes.extend_from_slice(&primitive.to_le_bytes());
    bytes.extend_from_slice(&invocation.identity_hash);
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
        word => Some(pending_primitive_from_word(word)?),
    };
    let identity_hash: ContentHash = bytes[32..64]
        .try_into()
        .expect("pending identity hash length");
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

fn hash_handle_list(domain: &[u8], handles: &[i64], store: &ValueStore) -> ContentHash {
    let mut hasher = Sha256::new();
    hasher.update(domain);
    hasher.update(
        i64::try_from(handles.len())
            .expect("handle length fits i64")
            .to_le_bytes(),
    );
    for handle in handles {
        let entry = store
            .entry(*handle)
            .unwrap_or_else(|| panic!("store handle {handle}"));
        hasher.update(entry.schema.as_bytes());
        hasher.update(entry.content_hash);
    }
    hasher.finalize().into()
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
    let mut hasher = Sha256::new();
    hasher.update(b"vix-tree-concrete");
    for (path, contents) in &tree.entries {
        hasher.update([0]);
        hasher.update(
            i64::try_from(path.len())
                .expect("path length fits i64")
                .to_le_bytes(),
        );
        hasher.update(path.as_bytes());
        hasher.update(
            i64::try_from(contents.len())
                .expect("contents length fits i64")
                .to_le_bytes(),
        );
        hasher.update(contents.as_bytes());
    }
    for (path, contents) in &tree.blobs {
        hasher.update([1]);
        hasher.update(
            i64::try_from(path.len())
                .expect("path length fits i64")
                .to_le_bytes(),
        );
        hasher.update(path.as_bytes());
        hasher.update(
            i64::try_from(contents.len())
                .expect("contents length fits i64")
                .to_le_bytes(),
        );
        hasher.update(contents);
    }
    hasher.finalize().into()
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

fn target_hash(store: &RefCell<ValueStore>, handle: i64) -> Result<u64, String> {
    let store = store.borrow();
    let entry = store
        .entry(handle)
        .ok_or_else(|| format!("store handle {handle}"))?;
    if entry.schema != "Target" {
        return Err(format!("handle {handle} is `{}`, not Target", entry.schema));
    }
    if entry.bytes.len() != 8 {
        return Err(format!("Target entry has {} bytes", entry.bytes.len()));
    }
    let word = i64::from_le_bytes(entry.bytes[..8].try_into().expect("target hash bytes"));
    if let Some(os) = store.entry(word)
        && os.schema == "Os"
    {
        let index = usize::from(*os.bytes.first().ok_or("empty Os entry")?);
        let value = Value::Struct {
            name: "Target".into(),
            fields: vec![(
                "os".into(),
                Value::Variant {
                    enum_name: "Os".into(),
                    index,
                    name: match index {
                        0 => "Linux",
                        1 => "Macos",
                        2 => "Windows",
                        _ => "Unknown",
                    }
                    .into(),
                    payload: Payload::Unit,
                },
            )],
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
        _ => "Cc",
    }
    .to_string()
}

fn capability_hash(command: &str, fingerprint: &str) -> u64 {
    match command {
        "cc" => cc_capability_hash(fingerprint),
        "ar" => ar_capability_hash(fingerprint),
        _ => hash_u64(fingerprint),
    }
}

fn pending_exec_identity_hash(
    command: &str,
    plan: &crate::exec::ExecPlan,
    capability: u64,
    mounts: &[crate::exec::Mount],
) -> ContentHash {
    let mut hasher = Sha256::new();
    hasher.update(b"vix-pending-exec");
    hasher.update(command.as_bytes());
    hasher.update(plan.identity_hash().to_le_bytes());
    hasher.update(capability.to_le_bytes());
    for mount in mounts {
        hasher.update(mount.at.as_bytes());
        hasher.update(mount.tree.fingerprint().to_le_bytes());
    }
    hasher.finalize().into()
}

fn hash_value_bytes(
    descriptor: &Descriptor<String>,
    bytes: &[u8],
    store: &ValueStore,
) -> ContentHash {
    let mut hasher = Sha256::new();
    hash_value_into(&mut hasher, descriptor, bytes, store);
    hasher.finalize().into()
}

fn hash_value_into(
    hasher: &mut Sha256,
    descriptor: &Descriptor<String>,
    bytes: &[u8],
    store: &ValueStore,
) {
    hasher.update(b"vix-value");
    hasher.update(descriptor.schema.as_bytes());
    match &descriptor.access {
        Access::Scalar if descriptor.schema == "Float" => {
            let word = read_frame_word(bytes, 0);
            hasher.update(canonicalize_word_for_schema("Float", word).to_le_bytes());
        }
        Access::Scalar => hasher.update(bytes),
        Access::Handle { target } => {
            let handle = read_frame_word(bytes, 0);
            let entry = store
                .entry(handle)
                .unwrap_or_else(|| panic!("store handle {handle}"));
            assert_eq!(&entry.schema, target, "handle target schema");
            hasher.update(entry.content_hash);
        }
        Access::Record(record) => {
            hasher.update(b"record");
            for field in &record.fields {
                let start = field.offset;
                let end = start + field.descriptor.layout.size;
                hash_value_into(hasher, &field.descriptor, &bytes[start..end], store);
            }
        }
        Access::Enum(access) => {
            let selector = read_variant_tag(bytes, descriptor);
            hasher.update(b"enum");
            hasher.update(selector.to_le_bytes());
            let variant = access
                .variants
                .iter()
                .find(|variant| variant.selector == selector)
                .unwrap_or_else(|| panic!("enum selector {selector}"));
            for field in &variant.payload.fields {
                let start = field.offset;
                let end = start + field.descriptor.layout.size;
                hash_value_into(hasher, &field.descriptor, &bytes[start..end], store);
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
                hash_value_into(hasher, element, &bytes[start..end], store);
            }
        }
        other => {
            panic!(
                "descriptor access {other:?} is outside vix machine value-store canonicalization"
            );
        }
    }
}

fn write_variant_tag(bytes: &mut [u8], descriptor: &Descriptor<String>, selector: u64) {
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

fn read_variant_tag(bytes: &[u8], descriptor: &Descriptor<String>) -> u64 {
    let Access::Enum(access) = &descriptor.access else {
        panic!("STORE_TAG used on non-enum schema `{}`", descriptor.schema);
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
    descriptor: &Descriptor<String>,
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
            &field.descriptor.schema,
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
        bytes[dst..dst + field_size].copy_from_slice(&value.to_le_bytes()[..field_size]);
    }
}

fn field_offset(descriptor: &Descriptor<String>, bytes: &[u8], field_index: usize) -> usize {
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

#[cfg(test)]
mod tests {
    use super::*;
    use weavy::mem::Layout;
    use weavy::mem::declared as declared_mem;
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
            invoke_region: 8,
            store_alloc_region: 0,
            store_read_region: 0,
            store_tag_region: 0,
            primitive_region: 0,
        }];
        (program, fns)
    }

    #[test]
    fn memo_boundaries_kill_the_exponential_tree() {
        let (program, fns) = fib_body_program();
        let mut driver = Driver::new(program, fns);
        // Base cases enter as memo facts (vix Const nodes resolve
        // without bodies).
        let zero = driver.memo_key(0, &[0]);
        let one = driver.memo_key(0, &[1]);
        driver.memo.insert(zero, 0);
        driver.memo.insert(one, 1);

        assert_eq!(driver.demand(0, vec![20]).unwrap(), 6765);

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
        let zero = driver.memo_key(0, &[0]);
        let one = driver.memo_key(0, &[1]);
        driver.memo.insert(zero, 0);
        driver.memo.insert(one, 1);
        driver.demand(0, vec![15]).unwrap();
        let cold_spawns = driver
            .trace
            .iter()
            .filter(|e| matches!(e, DriveEvent::Spawned { .. }))
            .count();

        driver.trace.clear();
        assert_eq!(driver.demand(0, vec![15]).unwrap(), 610);
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
            arg_schemas: vec![],
            return_schema: "Int".into(),
            invoke_region: 8,
            store_alloc_region: 0,
            store_read_region: 0,
            store_tag_region: 0,
            primitive_region: 0,
        });
        let mut driver = Driver::new(program, fns);
        let zero = driver.memo_key(0, &[0]);
        let one = driver.memo_key(0, &[1]);
        driver.memo.insert(zero, 0);
        driver.memo.insert(one, 1);
        driver.demand(0, vec![5]).unwrap();
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
            invoke_region: 24,
            store_alloc_region: 0,
            store_read_region: 0,
            store_tag_region: 0,
            primitive_region: 0,
        }];
        let mut driver = Driver::new(program, fns);
        assert_eq!(driver.demand(0, vec![6]).unwrap(), 42);
        let spawns = driver
            .trace
            .iter()
            .filter(|e| matches!(e, DriveEvent::Spawned { .. }))
            .count();
        assert_eq!(spawns, 1, "helper call is intra-task, not a driver spawn");
    }

    fn tree_descriptor() -> Descriptor<String> {
        declared_mem::declared_enum(
            "Tree".to_string(),
            vec![
                vec![declared_mem::i64_("Int".to_string())],
                vec![
                    declared_mem::handle("TreeRef".to_string(), "Tree".to_string()),
                    declared_mem::handle("TreeRef".to_string(), "Tree".to_string()),
                ],
            ],
        )
    }

    fn store_driver_for(code: Vec<Op>, arg_offsets: Vec<u32>, arg_schemas: Vec<String>) -> Driver {
        let mut descriptors = HashMap::new();
        descriptors.insert("Tree".into(), tree_descriptor());
        let mut driver = Driver::with_descriptors(
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
                invoke_region: 128,
                store_alloc_region: 0,
                store_read_region: 48,
                store_tag_region: 80,
                primitive_region: 0,
            }],
            descriptors,
        );
        assert_eq!(driver.intern_schema_ref("Tree"), 0);
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

        assert_eq!(driver.demand(0, vec![]).unwrap(), 21);
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

        assert_eq!(driver.demand(0, vec![]).unwrap(), 1);
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
        let tree = driver.descriptors["Tree"].clone();
        let (h1, d1) = driver.store.borrow_mut().alloc(
            "Tree",
            {
                let mut bytes = vec![0; tree.layout.size];
                write_variant_tag(&mut bytes, &tree, 0);
                bytes[8..16].copy_from_slice(&55i64.to_le_bytes());
                bytes
            },
            &driver.descriptors,
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
        );
        assert!(!d1);
        assert!(d2);
        assert_eq!(h1, h2, "same value returns the same handle");

        assert_eq!(driver.demand(0, vec![h1]).unwrap(), 55);
        assert_eq!(driver.demand(0, vec![h2]).unwrap(), 55);
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
