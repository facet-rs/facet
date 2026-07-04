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
use std::collections::HashMap;
use std::hash::Hash;

use sha2::{Digest, Sha256};
use weavy::mem::{Access, Descriptor, Tag};
use weavy::task::{FnId, HostFn, Program, Task, TaskStep};

/// INVOKE request frame contract (the lowering and this driver's
/// shared knowledge): at `INVOKE_REGION` the body lays out
/// [input_slot, fn_ref, argc, arg0, arg1, ...] as i64 words before
/// HostCall(INVOKE_HOST). The region is ordinary frame space —
/// spill-rule-resident like everything else.
pub const INVOKE_HOST: u32 = 0;
pub const STORE_ALLOC_HOST: u32 = 1;
pub const STORE_READ_HOST: u32 = 2;
pub const STORE_TAG_HOST: u32 = 3;

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
}

/// Driver-level events (join the unified trace via lowering-emitted
/// marks later; recorded directly in this slice).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    /// A value-store allocation occurred. `deduped` means the store
    /// returned an existing handle for canonical content.
    StoreAlloc { schema_ref: u64, deduped: bool },
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

/// A running or parked task execution.
struct Execution {
    task: Task,
    fn_ref: usize,
    key: CanonMemoKey,
    ready: Vec<bool>,
    awaited: Vec<i64>,
    /// input slot → the invocation key feeding it (for wiring
    /// completions).
    feeds: HashMap<usize, CanonMemoKey>,
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
}

/// The demand scheduler.
pub struct Driver {
    program: Program,
    fns: Vec<LoweredFn>,
    descriptors: HashMap<String, Descriptor<String>>,
    schema_refs: Vec<String>,
    memo: HashMap<CanonMemoKey, i64>,
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
        Driver {
            program,
            fns,
            descriptors,
            schema_refs: Vec::new(),
            memo: HashMap::new(),
            trace: Vec::new(),
            store: RefCell::new(ValueStore::default()),
        }
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

    /// Demand one invocation's identity: the edge of the machine.
    /// Returns the scalar result (slice 1).
    pub fn demand(&mut self, fn_ref: usize, args: Vec<i64>) -> i64 {
        let key = self.memo_key(fn_ref, &args);
        self.trace.push(DriveEvent::Demanded { fn_hash: key.0 });
        if let Some(&v) = self.memo.get(&key) {
            self.trace.push(DriveEvent::MemoHit { fn_hash: key.0 });
            return v;
        }

        // Waiters: invocation key → executions parked on it (by index
        // into `executions`) with the slot to fill.
        let mut executions: Vec<Option<Execution>> = Vec::new();
        let mut waiters: HashMap<CanonMemoKey, Vec<(usize, usize)>> = HashMap::new();
        let mut runnable: Vec<usize> = Vec::new();

        let root = self.spawn(&mut executions, fn_ref, key.clone(), &args);
        runnable.push(root);

        while let Some(ix) = runnable.pop() {
            let mut exec = executions[ix].take().expect("runnable execution exists");
            let requests = self.burst(&mut exec, ix);
            match requests {
                Burst::Done(value) => {
                    let done_key = exec.key.clone();
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
                    parked_input,
                } => {
                    for req in new_requests {
                        let req_key = self.memo_key(req.fn_ref, &req.args);
                        self.trace.push(DriveEvent::Demanded { fn_hash: req_key.0 });
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
                                    self.spawn(&mut executions, req.fn_ref, req_key, &req.args);
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
            }
        }

        *self.memo.get(&key).expect("root invocation completed")
    }

    fn spawn(
        &mut self,
        executions: &mut Vec<Option<Execution>>,
        fn_ref: usize,
        key: CanonMemoKey,
        args: &[i64],
    ) -> usize {
        let lowered = &self.fns[fn_ref];
        self.trace.push(DriveEvent::Spawned {
            fn_hash: lowered.hash,
        });
        let mut task = Task::spawn(&self.program, lowered.task_fn);
        for (offset, value) in lowered.arg_offsets.iter().zip(args) {
            task.write_i64(*offset, *value);
        }
        executions.push(Some(Execution {
            task,
            fn_ref,
            key,
            ready: Vec::new(),
            awaited: Vec::new(),
            feeds: HashMap::new(),
        }));
        executions.len() - 1
    }

    /// Run one execution until done or blocked, capturing INVOKE
    /// requests raised during the burst.
    fn burst(&mut self, exec: &mut Execution, exec_ix: usize) -> Burst {
        let lowered = &self.fns[exec.fn_ref];
        let invoke_region = lowered.invoke_region as usize;
        let store_alloc_region = lowered.store_alloc_region as usize;
        let store_read_region = lowered.store_read_region as usize;
        let store_tag_region = lowered.store_tag_region as usize;
        loop {
            // Size the input arrays BEFORE the burst (slots the body
            // registers this burst get sized on the next iteration —
            // the driver loop always re-enters after filling).
            let max_slot = exec.feeds.keys().copied().max().map_or(0, |m| m + 1);
            let want = max_slot.max(exec.ready.len()).max(16);
            exec.ready.resize(want, false);
            exec.awaited.resize(want, 0);

            let mut requests: Vec<InvokeRequest> = Vec::new();
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
                let schema = self
                    .schema_refs
                    .get(usize::try_from(type_ref).expect("schema ref non-negative"))
                    .unwrap_or_else(|| panic!("schema ref {type_ref}"))
                    .clone();
                let variant_index = read_frame_word(frame, store_alloc_region + 16);
                let field_count = read_frame_word(frame, store_alloc_region + 24) as usize;
                let descriptor = self
                    .descriptors
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
                let (handle, deduped) =
                    self.store
                        .borrow_mut()
                        .alloc(&schema, bytes, &self.descriptors);
                write_frame_word(frame, dst_slot, handle);
                let schema_ref = hash_u64(&schema);
                self.trace.push(DriveEvent::StoreAlloc {
                    schema_ref,
                    deduped,
                });
            };

            let mut store_read = |frame: &mut [u8]| {
                let dst_slot = read_frame_word(frame, store_read_region) as usize;
                let handle = read_frame_word(frame, store_read_region + 8);
                let field_index = read_frame_word(frame, store_read_region + 16) as usize;
                let store = self.store.borrow();
                let entry = store
                    .entry(handle)
                    .unwrap_or_else(|| panic!("store handle {handle}"));
                let descriptor = self
                    .descriptors
                    .get(&entry.schema)
                    .unwrap_or_else(|| panic!("descriptor for schema `{}`", entry.schema));
                let offset = field_offset(descriptor, &entry.bytes, field_index);
                let value = read_frame_word(&entry.bytes, offset);
                write_frame_word(frame, dst_slot, value);
            };

            let mut store_tag = |frame: &mut [u8]| {
                let dst_slot = read_frame_word(frame, store_tag_region) as usize;
                let handle = read_frame_word(frame, store_tag_region + 8);
                let store = self.store.borrow();
                let entry = store
                    .entry(handle)
                    .unwrap_or_else(|| panic!("store handle {handle}"));
                let descriptor = self
                    .descriptors
                    .get(&entry.schema)
                    .unwrap_or_else(|| panic!("descriptor for schema `{}`", entry.schema));
                let tag = read_variant_tag(&entry.bytes, descriptor);
                write_frame_word(
                    frame,
                    dst_slot,
                    i64::try_from(tag).expect("variant tag fits i64"),
                );
            };

            let mut hosts: [HostFn<'_>; 4] = [
                &mut invoke,
                &mut store_alloc,
                &mut store_read,
                &mut store_tag,
            ];
            let step = exec
                .task
                .run_hosted(&self.program, &exec.ready, &exec.awaited, &mut hosts);
            drop(hosts);

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
        match store.entry(word) {
            Some(entry) if entry.schema == schema => return entry.content_hash,
            _ => {}
        }
        let mut hasher = Sha256::new();
        hasher.update(b"vix-scalar-word");
        hasher.update(schema.as_bytes());
        hasher.update(word.to_le_bytes());
        hasher.finalize().into()
    }
}

enum Burst {
    Done(i64),
    Pending {
        new_requests: Vec<InvokeRequest>,
        parked_input: usize,
    },
}

fn hash_u64(value: impl Hash) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    value.hash(&mut h);
    std::hash::Hasher::finish(&h)
}

fn read_frame_word(frame: &[u8], at: usize) -> i64 {
    i64::from_le_bytes(frame[at..at + 8].try_into().expect("frame word"))
}

fn write_frame_word(frame: &mut [u8], at: usize, value: i64) {
    frame[at..at + 8].copy_from_slice(&value.to_le_bytes());
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
        let value = read_frame_word(frame, field_base + i * 8);
        let dst = field.offset;
        bytes[dst..dst + 8].copy_from_slice(&value.to_le_bytes());
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
    use weavy::mem::declared as declared_mem;
    use weavy::mem::Layout;
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
            invoke_region: 8,
            store_alloc_region: 0,
            store_read_region: 0,
            store_tag_region: 0,
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

        assert_eq!(driver.demand(0, vec![20]), 6765);

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
        driver.demand(0, vec![15]);
        let cold_spawns = driver
            .trace
            .iter()
            .filter(|e| matches!(e, DriveEvent::Spawned { .. }))
            .count();

        driver.trace.clear();
        assert_eq!(driver.demand(0, vec![15]), 610);
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
            invoke_region: 8,
            store_alloc_region: 0,
            store_read_region: 0,
            store_tag_region: 0,
        });
        let mut driver = Driver::new(program, fns);
        let zero = driver.memo_key(0, &[0]);
        let one = driver.memo_key(0, &[1]);
        driver.memo.insert(zero, 0);
        driver.memo.insert(one, 1);
        driver.demand(0, vec![5]);
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
            invoke_region: 24,
            store_alloc_region: 0,
            store_read_region: 0,
            store_tag_region: 0,
        }];
        let mut driver = Driver::new(program, fns);
        assert_eq!(driver.demand(0, vec![6]), 42);
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
                invoke_region: 128,
                store_alloc_region: 0,
                store_read_region: 48,
                store_tag_region: 80,
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

        assert_eq!(driver.demand(0, vec![]), 21);
        assert_eq!(driver.store_len(), 1);
        assert!(driver
            .trace
            .iter()
            .any(|e| matches!(e, DriveEvent::StoreAlloc { deduped: false, .. })));
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

        assert_eq!(driver.demand(0, vec![]), 1);
        assert_eq!(driver.store_len(), 1);
        assert!(driver
            .trace
            .iter()
            .any(|e| matches!(e, DriveEvent::StoreAlloc { deduped: true, .. })));
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

        assert_eq!(driver.demand(0, vec![h1]), 55);
        assert_eq!(driver.demand(0, vec![h2]), 55);
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
