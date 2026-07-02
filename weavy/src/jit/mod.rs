//! Opt-in copy-and-patch JIT support shared by Weavy consumers.
//!
//! This module is still format- and IR-agnostic. Callers own their stencil
//! functions, state ABI, host calls, and lowering policy; Weavy only exposes the
//! neutral mechanics that multiple backends need.

pub use copypatch::{patch_branch26, patch_x86_rel32};

#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
pub use copypatch::ExecBuf;

/// Shared copy-and-patch stencil bytes extracted by Weavy's build script.
///
/// Consumers should prefer the typed helpers on [`StencilLayout`]; this module
/// is public so backend-specific compilers can still compose lower-level layouts
/// when needed.
pub mod stencils {
    include!(concat!(env!("OUT_DIR"), "/weavy_stencils.rs"));
}

/// GDB/LLDB JIT interface + in-memory ELF builder + jitdump/perf-map emission, so debuggers and
/// profilers (lldb, gdb, perf, stax) resolve JIT'd PCs to source. Salvaged from bearcove/kajit.
pub mod debug;
/// DWARF v4 emission (`.debug_line`/abbrev/info) for JIT'd code. Salvaged from bearcove/kajit.
pub mod dwarf;

/// Whether this build can allocate and run native copy-and-patch code.
pub const NATIVE_COPY_PATCH_AVAILABLE: bool = cfg!(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
));

/// One consumer-supplied intrinsic in a shared Weavy host-call chain.
#[repr(C)]
pub struct HostCallInfo {
    /// Consumer-owned immutable metadata for this intrinsic.
    pub info: *const (),
    /// Consumer-owned intrinsic body.
    ///
    /// Returning `false` stops the copied chain immediately; consumers keep their
    /// exact error in their own state.
    pub call: unsafe extern "C" fn(cx: *mut (), info: *const ()) -> bool,
}

/// Threaded context expected by the shared host-call stencil.
#[repr(C)]
pub struct HostCallCtx<C> {
    /// Current program stream cursor.
    pub prog: *const u64,
    /// Consumer-owned execution state pointer.
    pub inner: *mut C,
}

impl<C> HostCallCtx<C> {
    /// Build a typed host-call context over a mutable consumer state.
    #[must_use]
    #[inline]
    pub fn new(prog: *const u64, inner: &mut C) -> Self {
        Self { prog, inner }
    }
}

/// Safe typed host-call body for copy-and-patch host-call chains.
///
/// The raw `extern "C"` trampoline and pointer casts stay inside Weavy's JIT
/// module. Consumers provide typed metadata and a typed mutable execution
/// context.
pub trait HostCall<C> {
    /// Execute one host-call metadata item against the caller's context.
    ///
    /// Returning `false` stops the copied chain immediately.
    fn call(&self, cx: &mut C) -> bool;
}

/// Executable typed host-call chain over Weavy's shared copy-and-patch stencils.
#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
pub struct HostCallChain<I> {
    infos: Vec<I>,
    call_slots: Vec<ProgSlot>,
    calls: Vec<HostCallInfo>,
    native: NativeProgram,
}

// SAFETY: The copied code and side program streams are owned by the chain and
// remain immutable except through `&mut self` in `run`, where host-call records
// are rebuilt for the caller's current context. Moving the chain to another
// thread is sound when the consumer metadata is `Send`; sharing still requires
// external synchronization because `run` takes `&mut self`.
#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
unsafe impl<I: Send> Send for HostCallChain<I> {}

#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
impl<I> HostCallChain<I> {
    /// Build an executable chain that calls each metadata item in order.
    ///
    /// Empty chains are valid and immediately return.
    #[must_use]
    pub fn new(infos: Vec<I>) -> Self {
        let mut layout = StencilLayout::new();
        let root = layout.start_chain();
        let mut previous = None;
        let mut call_slots = Vec::with_capacity(infos.len());
        for _ in &infos {
            let slot = layout.reserve_prog_slot(root.prog_index);
            call_slots.push(slot);
            let current = layout.emit_stencil(stencils::HOSTCALL);
            if let Some(previous) = previous {
                layout.patch_hostcall_continuation(previous, current);
            }
            previous = Some(current);
        }
        let done = layout.emit_done();
        if let Some(previous) = previous {
            layout.patch_hostcall_continuation(previous, done);
        }

        Self {
            infos,
            call_slots,
            calls: Vec::new(),
            native: NativeProgram::new(layout, root),
        }
    }

    /// Run the copied host-call chain against a typed context.
    pub fn run<C>(&mut self, cx: &mut C)
    where
        I: HostCall<C>,
    {
        self.calls.clear();
        self.calls
            .extend(self.infos.iter().map(|info| HostCallInfo {
                info: core::ptr::from_ref(info).cast(),
                call: typed_hostcall::<C, I>,
            }));
        for (slot, call) in self.call_slots.iter().copied().zip(&self.calls) {
            self.native
                .fill_prog_slot(slot, core::ptr::from_ref(call) as u64);
        }
        let mut host_ctx = HostCallCtx::new(self.native.entry_prog(), cx);
        let entry = unsafe { self.native.entry_fn::<HostCallCtx<C>>() };
        unsafe {
            entry(&mut host_ctx);
        }
    }

    /// Metadata items in this chain.
    #[must_use]
    pub fn infos(&self) -> &[I] {
        &self.infos
    }

    /// Number of raw host-call ABI records kept alive by this chain.
    #[must_use]
    pub fn hostcall_count(&self) -> usize {
        self.calls.len()
    }

    /// Number of copied stencils emitted by this chain.
    #[must_use]
    pub fn stencil_count(&self) -> usize {
        self.native.stencil_count()
    }
}

#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
unsafe extern "C" fn typed_hostcall<C, I>(cx: *mut (), info: *const ()) -> bool
where
    I: HostCall<C>,
{
    let cx = unsafe { &mut *cx.cast::<C>() };
    let info = unsafe { &*info.cast::<I>() };
    info.call(cx)
}

/// A copied stencil chain's entry point and associated program stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Chain {
    /// Offset into the final code buffer where this chain starts.
    pub entry: usize,
    /// Index into the program-stream table for this chain.
    pub prog_index: usize,
}

/// A reserved word in one chain's program stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProgSlot {
    /// Index into the program-stream table.
    pub prog_index: usize,
    /// Word index inside that program stream.
    pub slot: usize,
}

/// Code bytes plus side program streams for a copy-and-patch backend.
#[derive(Debug, Default)]
pub struct StencilLayout {
    code: Vec<u8>,
    progs: Vec<Vec<u64>>,
    stencil_count: usize,
}

impl StencilLayout {
    /// Create an empty stencil layout.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Start a new callable chain at the current code offset.
    pub fn start_chain(&mut self) -> Chain {
        let entry = self.code.len();
        let prog_index = self.progs.len();
        self.progs.push(Vec::new());
        Chain { entry, prog_index }
    }

    /// Append one stencil and return its starting offset.
    pub fn emit_stencil(&mut self, stencil: &[u8]) -> usize {
        let start = self.code.len();
        self.code.extend_from_slice(stencil);
        self.stencil_count += 1;
        start
    }

    /// Current code-buffer length.
    #[must_use]
    pub fn code_len(&self) -> usize {
        self.code.len()
    }

    /// Patch an AArch64 continuation relocation to another offset in this layout.
    pub fn patch_branch26(&mut self, site: usize, target: usize) {
        patch_branch26(&mut self.code, site, target);
    }

    /// Patch an x86/x86-64 `rel32` continuation relocation to another offset.
    pub fn patch_x86_rel32(&mut self, site: usize, target: usize) {
        patch_x86_rel32(&mut self.code, site, target);
    }

    /// Patch one continuation relocation to another offset in this layout.
    pub fn patch_continuation(&mut self, site: usize, target: usize) {
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            return self.patch_branch26(site, target);
        }
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            return self.patch_x86_rel32(site, target);
        }
        #[allow(unreachable_code)]
        {
            let _ = (site, target);
            panic!("native copy-and-patch is not available for this target");
        }
    }

    /// Append one word to a chain's program stream.
    pub fn push_prog_word(&mut self, prog_index: usize, value: u64) {
        self.progs[prog_index].push(value);
    }

    /// Mutably borrow a chain's program stream.
    pub fn prog_mut(&mut self, prog_index: usize) -> &mut Vec<u64> {
        &mut self.progs[prog_index]
    }

    /// Reserve one word in a chain's program stream for a later stable pointer.
    pub fn reserve_prog_slot(&mut self, prog_index: usize) -> ProgSlot {
        let slot = self.progs[prog_index].len();
        self.progs[prog_index].push(0);
        ProgSlot { prog_index, slot }
    }

    /// Fill a previously reserved program-stream word.
    pub fn fill_prog_slot(&mut self, slot: ProgSlot, value: u64) {
        self.progs[slot.prog_index][slot.slot] = value;
    }

    /// Append a shared host-call stencil to `chain`.
    ///
    /// The `info` pointer must remain valid for as long as the finalized native
    /// program can run. Callers typically point this at an element in an owned
    /// metadata vector held beside the finalized native program.
    pub fn emit_hostcall(&mut self, chain: Chain, info: *const HostCallInfo) -> usize {
        self.push_prog_word(chain.prog_index, info as u64);
        self.emit_stencil(stencils::HOSTCALL)
    }

    /// Append the shared terminal stencil.
    pub fn emit_done(&mut self) -> usize {
        self.emit_stencil(stencils::DONE)
    }

    /// Patch one shared host-call stencil's continuation to `target`.
    pub fn patch_hostcall_continuation(&mut self, hostcall_start: usize, target: usize) {
        for &rel in stencils::HOSTCALL_CONT {
            self.patch_continuation(hostcall_start + rel, target);
        }
    }

    /// Borrow a chain's program stream.
    #[must_use]
    pub fn prog(&self, prog_index: usize) -> &[u64] {
        &self.progs[prog_index]
    }

    /// Borrow the copied code bytes.
    #[must_use]
    pub fn code(&self) -> &[u8] {
        &self.code
    }

    /// Number of stencils emitted into this layout.
    #[must_use]
    pub fn stencil_count(&self) -> usize {
        self.stencil_count
    }

    /// Split the layout into executable code bytes and side program streams.
    #[must_use]
    pub fn into_parts(self) -> (Vec<u8>, Vec<Vec<u64>>, usize) {
        (self.code, self.progs, self.stencil_count)
    }
}

/// Executable copied code plus stable side program streams.
///
/// The ABI is still owned by the caller: this only binds a finalized
/// [`StencilLayout`] into native memory and keeps each chain's program stream in
/// stable heap storage for stencils to read.
#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
pub struct NativeProgram {
    buf: ExecBuf,
    progs: Vec<Vec<u64>>,
    entry: Chain,
    stencil_count: usize,
}

#[cfg(any(
    all(target_os = "macos", target_arch = "aarch64"),
    all(target_os = "linux", target_arch = "x86_64")
))]
impl NativeProgram {
    /// Make a finalized stencil layout executable.
    #[must_use]
    #[inline]
    pub fn new(layout: StencilLayout, entry: Chain) -> Self {
        let (code, progs, stencil_count) = layout.into_parts();
        Self {
            buf: ExecBuf::new(&code),
            progs,
            entry,
            stencil_count,
        }
    }

    /// Return the executable code buffer's base pointer.
    #[must_use]
    #[inline]
    pub fn code_ptr(&self) -> *const u8 {
        self.buf.as_ptr()
    }

    /// Return this program's root chain as a function pointer.
    ///
    /// # Safety
    /// The copied root code must use the caller's `extern "C" fn(*mut C)` ABI.
    #[must_use]
    #[inline]
    pub unsafe fn entry_fn<C>(&self) -> unsafe extern "C" fn(*mut C) {
        unsafe { self.chain_fn(self.entry.entry) }
    }

    /// Return an arbitrary chain entry as a function pointer.
    ///
    /// # Safety
    /// `entry` must be a chain offset in this program, and that chain must use
    /// the caller's `extern "C" fn(*mut C)` ABI.
    #[must_use]
    #[inline]
    pub unsafe fn chain_fn<C>(&self, entry: usize) -> unsafe extern "C" fn(*mut C) {
        unsafe {
            core::mem::transmute::<*const u8, unsafe extern "C" fn(*mut C)>(
                self.code_ptr().add(entry),
            )
        }
    }

    /// Return the root chain's program-stream index.
    #[must_use]
    #[inline]
    pub fn entry_prog_index(&self) -> usize {
        self.entry.prog_index
    }

    /// Return the root chain's program stream.
    #[must_use]
    #[inline]
    pub fn entry_prog(&self) -> *const u64 {
        self.prog_ptr(self.entry_prog_index())
    }

    /// Return one chain's program stream.
    #[must_use]
    #[inline]
    pub fn prog_ptr(&self, prog_index: usize) -> *const u64 {
        self.progs[prog_index].as_ptr()
    }

    /// Fill a previously reserved word in a program stream.
    #[inline]
    pub fn fill_prog_word(&mut self, prog_index: usize, slot: usize, value: u64) {
        self.progs[prog_index][slot] = value;
    }

    /// Fill a previously reserved program-stream slot.
    #[inline]
    pub fn fill_prog_slot(&mut self, slot: ProgSlot, value: u64) {
        self.fill_prog_word(slot.prog_index, slot.slot, value);
    }

    /// Number of callable chains.
    #[must_use]
    #[inline]
    pub fn chain_count(&self) -> usize {
        self.progs.len()
    }

    /// Number of copied stencils.
    #[must_use]
    #[inline]
    pub fn stencil_count(&self) -> usize {
        self.stencil_count
    }

    /// Total number of program-stream words.
    #[must_use]
    #[inline]
    pub fn prog_slot_count(&self) -> usize {
        self.progs.iter().map(Vec::len).sum()
    }
}

#[cfg(test)]
mod tests {
    use super::StencilLayout;

    #[test]
    fn layout_tracks_chains_stencils_and_program_slots() {
        let mut layout = StencilLayout::new();
        let root = layout.start_chain();
        let first = layout.emit_stencil(&[1, 2, 3, 4]);
        layout.push_prog_word(root.prog_index, 7);
        let slot = layout.reserve_prog_slot(root.prog_index);
        layout.fill_prog_slot(slot, 11);
        let child = layout.start_chain();
        let second = layout.emit_stencil(&[5, 6]);

        assert_eq!(root.entry, 0);
        assert_eq!(root.prog_index, 0);
        assert_eq!(first, 0);
        assert_eq!(child.entry, 4);
        assert_eq!(child.prog_index, 1);
        assert_eq!(second, 4);
        assert_eq!(layout.code(), &[1, 2, 3, 4, 5, 6]);
        assert_eq!(layout.prog(root.prog_index), &[7, 11]);
        assert_eq!(layout.stencil_count(), 2);
    }

    #[cfg(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    ))]
    fn ret_stencil() -> &'static [u8] {
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        {
            &[0xc0, 0x03, 0x5f, 0xd6]
        }
        #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
        {
            &[0xc3]
        }
    }

    #[cfg(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    ))]
    #[test]
    fn native_program_owns_executable_code_and_program_slots() {
        use super::NativeProgram;

        let mut layout = StencilLayout::new();
        let root = layout.start_chain();
        layout.emit_stencil(ret_stencil());
        layout.push_prog_word(root.prog_index, 7);
        let slot = layout.reserve_prog_slot(root.prog_index);

        let mut native = NativeProgram::new(layout, root);
        native.fill_prog_slot(slot, 11);

        assert_eq!(native.chain_count(), 1);
        assert_eq!(native.stencil_count(), 1);
        assert_eq!(native.prog_slot_count(), 2);
        assert!(!native.entry_prog().is_null());

        let entry = unsafe { native.entry_fn::<u8>() };
        let mut ctx = 0u8;
        unsafe { entry(&mut ctx) };
    }

    #[cfg(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    ))]
    #[test]
    fn shared_hostcall_stencil_runs_consumer_intrinsic() {
        use super::{HostCallCtx, HostCallInfo, NativeProgram};

        struct State {
            value: u64,
        }

        struct Info {
            add: u64,
        }

        unsafe extern "C" fn add(cx: *mut (), info: *const ()) -> bool {
            let state = unsafe { &mut *cx.cast::<State>() };
            let info = unsafe { &*info.cast::<Info>() };
            state.value += info.add;
            true
        }

        let infos = [Info { add: 41 }];
        let calls = [HostCallInfo {
            info: core::ptr::from_ref(&infos[0]).cast(),
            call: add,
        }];
        let mut layout = StencilLayout::new();
        let root = layout.start_chain();
        let hostcall = layout.emit_hostcall(root, core::ptr::from_ref(&calls[0]));
        let done = layout.emit_done();
        layout.patch_hostcall_continuation(hostcall, done);

        let native = NativeProgram::new(layout, root);
        let mut state = State { value: 1 };
        let mut cx = HostCallCtx::new(native.entry_prog(), &mut state);
        let entry = unsafe { native.entry_fn::<HostCallCtx<State>>() };
        unsafe {
            entry(&mut cx);
        }

        assert_eq!(state.value, 42);
    }

    #[cfg(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    ))]
    #[test]
    fn typed_hostcall_chain_runs_consumer_intrinsics_without_raw_abi() {
        use super::{HostCall, HostCallChain};

        struct State {
            value: u64,
        }

        struct Add(u64);

        impl HostCall<State> for Add {
            fn call(&self, cx: &mut State) -> bool {
                cx.value += self.0;
                true
            }
        }

        let mut chain = HostCallChain::new(vec![Add(20), Add(21)]);
        let mut state = State { value: 1 };
        chain.run(&mut state);

        assert_eq!(state.value, 42);
        assert_eq!(chain.infos().len(), 2);
        assert_eq!(chain.hostcall_count(), 2);
        assert_eq!(chain.stencil_count(), 3);
    }

    #[cfg(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    ))]
    #[test]
    fn typed_hostcall_chain_stops_when_consumer_returns_false() {
        use super::{HostCall, HostCallChain};

        struct State {
            value: u64,
        }

        struct Step {
            add: u64,
            keep_running: bool,
        }

        impl HostCall<State> for Step {
            fn call(&self, cx: &mut State) -> bool {
                cx.value += self.add;
                self.keep_running
            }
        }

        let mut chain = HostCallChain::new(vec![
            Step {
                add: 1,
                keep_running: true,
            },
            Step {
                add: 10,
                keep_running: false,
            },
            Step {
                add: 100,
                keep_running: true,
            },
        ]);
        let mut state = State { value: 0 };
        chain.run(&mut state);

        assert_eq!(state.value, 11);
        assert_eq!(chain.hostcall_count(), 3);
        assert_eq!(chain.stencil_count(), 4);
    }
}
