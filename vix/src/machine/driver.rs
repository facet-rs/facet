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
use std::collections::{BTreeMap, HashMap};
use std::hash::Hash;

use sha2::{Digest, Sha256};
#[cfg(any(test, feature = "jit"))]
use weavy::jit::task_lane::{JitProgram, JitTask};
use weavy::mem::{Access, Descriptor, Tag};
#[cfg(any(test, feature = "jit"))]
use weavy::task::Op;
use weavy::task::{FnId, HostFn, Program, Task, TaskStep};

use crate::oracle::{assign_roles, tool_for};

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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lane {
    #[default]
    Interp,
    #[cfg(any(test, feature = "jit"))]
    Jit,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriveEvent {
    /// Demand arrived for (fn hash, memo-key hash of args).
    Demanded {
        fn_hash: u64,
    },
    /// Served from memo — NO task existed.
    MemoHit {
        fn_hash: u64,
    },
    /// Spawned a task (memo miss).
    Spawned {
        fn_hash: u64,
    },
    /// A task parked awaiting another invocation's result.
    ParkedOn {
        fn_hash: u64,
    },
    /// An invocation completed and fed its awaiters.
    Completed {
        fn_hash: u64,
    },
    /// A concrete invocation identity spawned. `key_hash` is the
    /// canonical memo-key hash, including canonicalized args.
    SpawnedInvocation {
        fn_hash: u64,
        key_hash: u64,
    },
    /// A value-store allocation occurred. `deduped` means the store
    /// returned an existing handle for canonical content.
    StoreAlloc {
        schema_ref: u64,
        deduped: bool,
    },
    RunRequested {
        command: u64,
        output: u64,
    },
    RunStarted {
        command: u64,
        output: u64,
    },
    RunCompleted {
        command: u64,
        output: u64,
    },
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
    capability: i64,
    output: i64,
}

#[derive(Clone, Debug)]
struct ForceRequest {
    input_slot: usize,
    option: i64,
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
}

#[derive(Clone)]
struct OrderedMapPair {
    pair: MapPair,
    key_hash: ContentHash,
    value_hash: ContentHash,
}

#[derive(Clone, Debug)]
struct PendingInvocation {
    fn_ref: usize,
    args: Vec<i64>,
    remaining_arity: usize,
    identity_hash: ContentHash,
}

#[derive(Clone, Debug)]
enum ArrayEntry {
    Words(Vec<i64>),
    Pending(Vec<i64>),
}

#[derive(Clone, Debug)]
enum TreeEntry {
    Concrete(crate::exec::Tree),
    Merge(Vec<i64>),
}

#[derive(Clone, Debug)]
enum OptionPayload {
    None,
    Some { schema: String, word: i64 },
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
    ) -> Result<(i64, bool), String> {
        let ordered = canonical_map_pairs(self, pairs);
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
        schema_refs: &[String],
    ) -> Result<(i64, bool), String> {
        let (_, pairs) = self.map_pairs(handle, schema_refs)?;
        let key_hash = canonical_word_hash_in_store(self, key_schema, key_word);
        let value = pairs
            .into_iter()
            .find(|pair| {
                pair.key_schema == key_schema
                    && canonical_word_hash_in_store(self, &pair.key_schema, pair.key_word)
                        == key_hash
            })
            .and_then(|pair| {
                (pair.value_schema == value_schema
                    || pair.value_schema == pending_schema(value_schema))
                .then_some((pair.value_schema, pair.value_word))
            });
        match value {
            Some((schema, word)) => self.alloc_option_some(&schema, word, schema_refs),
            None => self.alloc_option_none(value_schema, schema_refs),
        }
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
        schema_refs: &[String],
    ) -> Result<(i64, bool), String> {
        let option_schema = option_schema(value_schema);
        let value_ref = schema_ref_for(value_schema, schema_refs)?;
        let value_word = canonicalize_word_for_schema(value_schema, value_word);
        let value_hash = canonical_word_hash_in_store(self, value_schema, value_word);
        let mut bytes = Vec::with_capacity(24);
        bytes.extend_from_slice(&1i64.to_le_bytes());
        bytes.extend_from_slice(&value_ref.to_le_bytes());
        bytes.extend_from_slice(&value_word.to_le_bytes());
        let mut hasher = Sha256::new();
        hasher.update(b"vix-option");
        hasher.update(option_schema.as_bytes());
        hasher.update(1i64.to_le_bytes());
        hasher.update(value_schema.as_bytes());
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
        if entry.bytes.len() != 24 {
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
        Ok(OptionPayload::Some {
            schema: schema.clone(),
            word: canonicalize_word_for_schema(schema, read_frame_word(&entry.bytes, 16)),
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

    fn alloc_array_words(&mut self, words: Vec<i64>) -> (i64, bool) {
        let mut bytes = Vec::with_capacity(16 + words.len() * 8);
        bytes.extend_from_slice(&0i64.to_le_bytes());
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
        hasher.update(
            i64::try_from(words.len())
                .expect("array length fits i64")
                .to_le_bytes(),
        );
        for word in &words {
            let entry = self
                .entry(*word)
                .unwrap_or_else(|| panic!("array element handle {word}"));
            hasher.update(entry.schema.as_bytes());
            hasher.update(entry.content_hash);
        }
        let content_hash = hasher.finalize().into();
        self.alloc_with_hash("Array", bytes, content_hash)
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

    fn array_entry(&self, handle: i64) -> Result<ArrayEntry, String> {
        let entry = self
            .entry(handle)
            .ok_or_else(|| format!("store handle {handle}"))?;
        if entry.schema != "Array" {
            return Err(format!("handle {handle} is `{}`, not Array", entry.schema));
        }
        let kind = read_frame_word(&entry.bytes, 0);
        match kind {
            0 => {
                let count =
                    usize::try_from(read_frame_word(&entry.bytes, 8)).map_err(|_| "array count")?;
                let expected = 16 + count * 8;
                if entry.bytes.len() != expected {
                    return Err(format!(
                        "Array words entry has {} bytes, expected {expected}",
                        entry.bytes.len()
                    ));
                }
                Ok(ArrayEntry::Words(
                    (0..count)
                        .map(|i| read_frame_word(&entry.bytes, 16 + i * 8))
                        .collect(),
                ))
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
    lane: LaneRuntime,
    fns: Vec<LoweredFn>,
    descriptors: HashMap<String, Descriptor<String>>,
    schema_refs: Vec<String>,
    memo: HashMap<CanonMemoKey, i64>,
    exec_cache: crate::exec::ExecCache,
    pub trace: Vec<DriveEvent>,
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
        let lane = LaneRuntime::new(lane, &program)?;
        Ok(Driver {
            program,
            lane,
            fns,
            descriptors,
            schema_refs: Vec::new(),
            memo: HashMap::new(),
            exec_cache: crate::exec::ExecCache::new(),
            trace: Vec::new(),
            store: RefCell::new(ValueStore::default()),
        })
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

    pub fn store_entry(&self, handle: i64) -> Option<StoreEntry> {
        self.store.borrow().entry(handle).cloned()
    }

    pub fn tree_entries(&self, handle: i64) -> Result<BTreeMap<String, String>, String> {
        match self.store.borrow().tree_entry(handle)? {
            TreeEntry::Concrete(tree) => Ok(tree.entries),
            TreeEntry::Merge(_) => Err("handle is a pending merge tree, not concrete".into()),
        }
    }

    #[cfg(test)]
    pub fn intern_tree_concrete(&self, tree: crate::exec::Tree) -> i64 {
        self.store.borrow_mut().alloc_tree_concrete(tree).0
    }

    pub fn fn_hash(&self, fn_ref: usize) -> u64 {
        self.fns[fn_ref].hash
    }

    #[cfg(test)]
    pub fn fn_ops(&self, fn_ref: usize) -> &[Op] {
        &self.program.fns[self.fns[fn_ref].task_fn.0 as usize].code
    }

    pub fn intern_raw_value(&self, schema: &str, bytes: Vec<u8>) -> (i64, bool) {
        self.store.borrow_mut().alloc_raw(schema, bytes)
    }

    pub fn intern_linux_target(&self) -> (i64, bool) {
        self.store
            .borrow_mut()
            .alloc_raw("Target", 0x391c555cf0975f9cu64.to_le_bytes().to_vec())
    }

    /// Demand one invocation's identity: the edge of the machine.
    /// Returns the scalar result (slice 1).
    pub fn demand(&mut self, fn_ref: usize, args: Vec<i64>) -> Result<i64, String> {
        let key = self.memo_key(fn_ref, &args);
        self.trace.push(DriveEvent::Demanded { fn_hash: key.0 });
        if let Some(&v) = self.memo.get(&key) {
            self.trace.push(DriveEvent::MemoHit { fn_hash: key.0 });
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
                    self.trace.push(DriveEvent::Completed {
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
                    force_requests,
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
                    let mut new_requests = new_requests;
                    for req in force_requests {
                        match self.force_request(req, ix) {
                            Ok(ForceOutcome::Ready { input_slot, value }) => {
                                if exec.ready.len() <= input_slot {
                                    exec.ready.resize(input_slot + 1, false);
                                    exec.awaited.resize(input_slot + 1, 0);
                                }
                                exec.ready[input_slot] = true;
                                exec.awaited[input_slot] = value;
                            }
                            Ok(ForceOutcome::Demand(invoke)) => new_requests.push(invoke),
                            Err(err) => return Err(err),
                        }
                    }
                    for req in new_requests {
                        let req_key = self.memo_key(req.fn_ref, &req.args);
                        self.trace.push(DriveEvent::Demanded { fn_hash: req_key.0 });
                        if exec.ready.len() <= req.input_slot {
                            exec.ready.resize(req.input_slot + 1, false);
                            exec.awaited.resize(req.input_slot + 1, 0);
                        }
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
                        self.trace.push(DriveEvent::ParkedOn {
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
        let lowered = &self.fns[fn_ref];
        self.trace.push(DriveEvent::Spawned {
            fn_hash: lowered.hash,
        });
        self.trace.push(DriveEvent::SpawnedInvocation {
            fn_hash: lowered.hash,
            key_hash: memo_key_hash(&key),
        });
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
            let mut force_requests: Vec<ForceRequest> = Vec::new();
            let descriptors = &self.descriptors;
            let schema_refs = &self.schema_refs;
            let store_cell = &self.store;
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
                    let (handle, deduped) =
                        store_cell
                            .borrow_mut()
                            .alloc_map(&schema, Vec::new(), schema_refs)?;
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
                    let value_word = canonicalize_word_for_schema(
                        &value_schema,
                        read_frame_word(frame, store_alloc_region + 48),
                    );
                    let (stored_schema, mut pairs) =
                        store_cell.borrow().map_pairs(map_handle, schema_refs)?;
                    if stored_schema != map_schema {
                        return Err(format!(
                            "expected map schema {map_schema}, got {stored_schema}"
                        ));
                    }
                    pairs.push(MapPair {
                        key_schema,
                        key_word,
                        value_schema,
                        value_word,
                    });
                    let (handle, deduped) =
                        store_cell
                            .borrow_mut()
                            .alloc_map(&map_schema, pairs, schema_refs)?;
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
                force_requests.push(ForceRequest {
                    input_slot,
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
                    let (handle, _) = store_cell.borrow_mut().alloc_raw(&kind, key.into_bytes());
                    write_frame_word(frame, dst_slot, handle);
                    Ok(())
                })();
                if let Err(err) = result {
                    *host_error.borrow_mut() = Some(err);
                }
            };

            let mut array_alloc = |frame: &mut [u8]| {
                let dst_slot = read_frame_word(frame, primitive_region) as usize;
                let count = read_frame_word(frame, primitive_region + 16) as usize;
                let words = (0..count)
                    .map(|i| read_frame_word(frame, primitive_region + 24 + i * 8))
                    .collect();
                let (handle, _) = store_cell.borrow_mut().alloc_array_words(words);
                write_frame_word(frame, dst_slot, handle);
            };

            let mut array_map_pending = |frame: &mut [u8]| {
                let result = (|| {
                    let dst_slot = read_frame_word(frame, primitive_region) as usize;
                    let array_handle = read_frame_word(frame, primitive_region + 8);
                    let fn_ref = usize::try_from(read_frame_word(frame, primitive_region + 16))
                        .map_err(|_| "negative fn ref".to_string())?;
                    let captured = read_frame_word(frame, primitive_region + 24);
                    let words = match store_cell.borrow().array_entry(array_handle)? {
                        ArrayEntry::Words(words) => words,
                        ArrayEntry::Pending(_) => {
                            return Err("map over pending array is outside slice 4".into());
                        }
                    };
                    let pending = words
                        .into_iter()
                        .map(|word| {
                            let args = vec![captured, word];
                            let invocation = pending_invocation_for(
                                &lowered_fns[fn_ref],
                                store_cell,
                                fn_ref,
                                args,
                            );
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
                    let pending = match store_cell.borrow().array_entry(array_handle)? {
                        ArrayEntry::Pending(pending) => pending,
                        ArrayEntry::Words(_) => {
                            return Err("collect over scalar array is outside slice 4".into());
                        }
                    };
                    let (handle, _) = store_cell.borrow_mut().alloc_tree_merge(pending);
                    write_frame_word(frame, dst_slot, handle);
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
                let capability = read_frame_word(frame, primitive_region + 8);
                let output = read_frame_word(frame, primitive_region + 16);
                exec_requests.push(ExecRequest {
                    input_slot,
                    capability,
                    output,
                });
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
                    let invocation =
                        pending_invocation_for(&lowered_fns[fn_ref], store_cell, fn_ref, args);
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

            let mut hosts: [HostFn<'_>; 16] = [
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
            ];
            let step = exec
                .task
                .advance(&self.program, &exec.ready, &exec.awaited, &mut hosts);
            drop(hosts);
            self.trace.extend(store_events.into_inner());
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
                        force_requests,
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
                let projected = crate::oracle::subtree(&tree, path)?;
                Ok(self.store.borrow_mut().alloc_tree_concrete(projected).0)
            }
            TreeEntry::Merge(pending) => {
                for handle in pending.into_iter().rev() {
                    let invocation = self.store.borrow().pending_invocation(handle)?;
                    let value = self.demand(invocation.fn_ref, invocation.args)?;
                    if let Some(found) = self.project_tree_path_optional(value, path)? {
                        return Ok(found);
                    }
                }
                Err(crate::oracle::PathMissing {
                    path: path.to_string(),
                }
                .diagnostic())
            }
        }
    }

    fn force_request(&mut self, req: ForceRequest, caller: usize) -> Result<ForceOutcome, String> {
        match self
            .store
            .borrow()
            .option_payload(req.option, &self.schema_refs)?
        {
            OptionPayload::None => Err("unwrap on None".into()),
            OptionPayload::Some { schema, word } => {
                if let Some(value_schema) = pending_value_schema(&schema) {
                    let invocation = self.store.borrow().pending_invocation(word)?;
                    if invocation.remaining_arity != 0 {
                        return Err(format!(
                            "cannot force pending {value_schema} with {} remaining args",
                            invocation.remaining_arity
                        ));
                    }
                    Ok(ForceOutcome::Demand(InvokeRequest {
                        caller,
                        input_slot: req.input_slot,
                        fn_ref: invocation.fn_ref,
                        args: invocation.args,
                    }))
                } else {
                    Ok(ForceOutcome::Ready {
                        input_slot: req.input_slot,
                        value: word,
                    })
                }
            }
        }
    }

    fn project_tree_path_optional(
        &mut self,
        tree_handle: i64,
        path: &str,
    ) -> Result<Option<i64>, String> {
        let tree = self.store.borrow().tree_entry(tree_handle)?;
        let TreeEntry::Concrete(tree) = tree else {
            return Err("nested merge tree projection is outside slice 4".into());
        };
        match crate::oracle::subtree(&tree, path) {
            Ok(projected) => Ok(Some(
                self.store.borrow_mut().alloc_tree_concrete(projected).0,
            )),
            Err(_) => Ok(None),
        }
    }

    fn execute_request(&mut self, req: ExecRequest) -> Result<(usize, i64), String> {
        let output = self.store.borrow().string_value(req.output, "Path")?;
        let cap_key = self.store.borrow().string_value(req.capability, "Cc")?;
        let cap_hash = cc_capability_hash(&cap_key);
        let argv = vec!["-o".to_string(), output.clone()];
        let plan = assign_roles("cc", &argv)?;
        let tool = tool_for("cc")?;
        self.trace.push(DriveEvent::RunRequested {
            command: hash_u64("cc"),
            output: hash_u64(&output),
        });
        self.trace.push(DriveEvent::RunStarted {
            command: hash_u64("cc"),
            output: hash_u64(&output),
        });
        let outcome = self.exec_cache.exec(&plan, cap_hash, &[], tool)?;
        self.trace.push(DriveEvent::RunCompleted {
            command: hash_u64("cc"),
            output: hash_u64(&output),
        });
        let handle = self
            .store
            .borrow_mut()
            .alloc_tree_concrete(outcome.outputs)
            .0;
        Ok((req.input_slot, handle))
    }
}

enum Burst {
    Done(i64),
    Pending {
        new_requests: Vec<InvokeRequest>,
        project_requests: Vec<ProjectRequest>,
        exec_requests: Vec<ExecRequest>,
        force_requests: Vec<ForceRequest>,
        parked_input: usize,
    },
    Error(String),
}

enum ForceOutcome {
    Ready { input_slot: usize, value: i64 },
    Demand(InvokeRequest),
}

fn hash_u64(value: impl Hash) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut h);
    std::hash::Hasher::finish(&h)
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
    fn_ref: usize,
    args: Vec<i64>,
) -> PendingInvocation {
    let store = store.borrow();
    let identity_hash = pending_identity_hash(lowered, &store, &args);
    PendingInvocation {
        fn_ref,
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
        super::value::TotalF64::new(f64::from_bits(word as u64))
            .get()
            .to_bits() as i64
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

fn canonical_map_pairs(store: &ValueStore, pairs: Vec<MapPair>) -> Vec<OrderedMapPair> {
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
        a.pair
            .key_schema
            .cmp(&b.pair.key_schema)
            .then_with(|| a.key_hash.cmp(&b.key_hash))
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
    deduped
}

fn encode_map_pairs(pairs: &[OrderedMapPair], schema_refs: &[String]) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::with_capacity(8 + pairs.len() * 32);
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
    Ok(bytes)
}

fn decode_map_pairs(bytes: &[u8], schema_refs: &[String]) -> Result<Vec<MapPair>, String> {
    if bytes.len() < 8 {
        return Err("Map entry is shorter than its count word".into());
    }
    let count = usize::try_from(read_frame_word(bytes, 0)).map_err(|_| "negative map count")?;
    let expected = 8 + count * 32;
    if bytes.len() != expected {
        return Err(format!(
            "Map entry has {} bytes, expected {expected}",
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
        pairs.push(MapPair {
            key_schema,
            key_word,
            value_schema,
            value_word,
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
        hasher.update(pair.value_hash);
    }
    hasher.finalize().into()
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

fn encode_pending_invocation(invocation: &PendingInvocation) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(
        &i64::try_from(invocation.fn_ref)
            .expect("fn ref fits i64")
            .to_le_bytes(),
    );
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
    bytes.extend_from_slice(&invocation.identity_hash);
    for arg in &invocation.args {
        bytes.extend_from_slice(&arg.to_le_bytes());
    }
    bytes
}

fn decode_pending_invocation(bytes: &[u8]) -> Result<PendingInvocation, String> {
    if bytes.len() < 56 {
        return Err("pending invocation too short".into());
    }
    let fn_ref = usize::try_from(read_frame_word(bytes, 0)).map_err(|_| "fn ref")?;
    let argc = usize::try_from(read_frame_word(bytes, 8)).map_err(|_| "argc")?;
    let remaining_arity =
        usize::try_from(read_frame_word(bytes, 16)).map_err(|_| "remaining arity")?;
    let identity_hash: ContentHash = bytes[24..56]
        .try_into()
        .expect("pending identity hash length");
    let expected = 56 + argc * 8;
    if bytes.len() != expected {
        return Err(format!(
            "pending invocation has {} bytes, expected {expected}",
            bytes.len()
        ));
    }
    let args = (0..argc)
        .map(|i| read_frame_word(bytes, 56 + i * 8))
        .collect();
    Ok(PendingInvocation {
        fn_ref,
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
    if at != bytes.len() {
        return Err(format!(
            "tree entry has {} trailing bytes",
            bytes.len() - at
        ));
    }
    Ok(crate::exec::Tree { entries })
}

fn hash_concrete_tree(tree: &crate::exec::Tree) -> ContentHash {
    let mut hasher = Sha256::new();
    hasher.update(b"vix-tree-concrete");
    for (path, contents) in &tree.entries {
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
    hasher.finalize().into()
}

fn encode_string(value: &str, bytes: &mut Vec<u8>) {
    bytes.extend_from_slice(
        &i64::try_from(value.len())
            .expect("string length fits i64")
            .to_le_bytes(),
    );
    bytes.extend_from_slice(value.as_bytes());
}

fn decode_string(bytes: &[u8], at: &mut usize) -> Result<String, String> {
    if bytes.len() < *at + 8 {
        return Err("string length truncated".into());
    }
    let len = usize::try_from(read_frame_word(bytes, *at)).map_err(|_| "string length")?;
    *at += 8;
    if bytes.len() < *at + len {
        return Err("string data truncated".into());
    }
    let value = String::from_utf8(bytes[*at..*at + len].to_vec()).map_err(|err| err.to_string())?;
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
    Ok(u64::from_le_bytes(
        entry.bytes[..8].try_into().expect("target hash bytes"),
    ))
}

fn cc_capability_hash(fingerprint: &str) -> u64 {
    let value = crate::oracle::Value::Struct {
        name: "Cc".into(),
        fields: vec![(
            "fingerprint".into(),
            crate::oracle::Value::Str(fingerprint.to_string()),
        )],
    };
    value.canon_hash()
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
