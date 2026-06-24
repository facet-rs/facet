use alloc::{
    borrow::Cow,
    boxed::Box,
    collections::BTreeMap,
    format,
    string::{String, ToString},
    vec::Vec,
};
use core::alloc::Layout;
use core::marker::PhantomData;
use core::mem::{ManuallyDrop, MaybeUninit};
use core::str::FromStr;
#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
use core::sync::atomic::{AtomicU8, Ordering};

use facet_core::{
    Characteristic, Def, DefaultInPlaceFn, DefaultSource, EnumRepr, EnumType, Facet, Field,
    ListDef, MapDef, OptionDef, PointerDef, PtrConst, PtrMut, PtrUninit, ScalarType, SetDef, Shape,
    StructKind, Type, UserType, Variant,
};
use facet_format::{
    DeserializeError, DeserializeErrorKind, FormatParser, ParseError, ParseEventKind, ScalarValue,
};
use facet_reflect::Span;
use weavy::mem::runtime::{
    HandleGuard, InitializedLedger, RawAllocError, RawArrayBuilder, ScratchSession, ScratchSlot,
};
use weavy::{
    BlockRef, Control, DenseLowered, Lowered, Program, RunError, RunStats, Step, run_dense_program,
};

use crate::JsonParser;
use crate::parser::{
    JsonFieldKey, JsonFieldKeyInput, JsonObjectKeyStep, JsonObjectOrderedI32Step,
    JsonObjectOrderedScalarStep, JsonScalarInput, JsonScalarToken, JsonSequenceScalarStep,
};
#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
use crate::parser::{NativeArrayStep, NativeOrderedRootCursor};
use crate::scanner::{NumberHint, ParsedNumber, SpannedToken, Token as ScanToken};

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
mod native_stencils {
    include!(concat!(env!("OUT_DIR"), "/stencils.rs"));
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum JsonBlockId {
    Shape(&'static Shape),
    StructLoop(&'static Shape),
    VariantStructLoop(&'static Shape, usize),
    ListLoop(&'static Shape),
    SetLoop(&'static Shape),
    MapLoop(&'static Shape),
}

type BlockId = JsonBlockId;
type ExecBlock = BlockRef;
type SymbolicOp = JsonOp<BlockId>;
type ExecOp = JsonOp<ExecBlock>;

/// Deserialize a value from a JSON string through the opt-in Weavy runner.
pub fn from_str_weavy<T>(input: &str) -> Result<T, DeserializeError>
where
    T: Facet<'static>,
{
    JsonWeavyPlan::<T>::build()?.from_str(input)
}

/// Deserialize a value from JSON bytes through the opt-in Weavy runner.
pub fn from_slice_weavy<T>(input: &[u8]) -> Result<T, DeserializeError>
where
    T: Facet<'static>,
{
    JsonWeavyPlan::<T>::build()?.from_slice(input)
}

/// Deserialize a value from a JSON string through the opt-in Weavy runner and
/// return the generic runner counters.
pub fn from_str_weavy_with_stats<T>(input: &str) -> Result<(T, RunStats), DeserializeError>
where
    T: Facet<'static>,
{
    let plan = JsonWeavyPlan::<T>::build()?;
    plan.from_str_with_stats(input)
}

/// Deserialize a value from JSON bytes through the opt-in Weavy runner and
/// return the generic runner counters.
pub fn from_slice_weavy_with_stats<T>(input: &[u8]) -> Result<(T, RunStats), DeserializeError>
where
    T: Facet<'static>,
{
    let plan = JsonWeavyPlan::<T>::build()?;
    plan.from_slice_with_stats(input)
}

/// Deserialize a value from a JSON string through the opt-in Weavy runner with
/// JIT enabled when a JSON native backend is available.
pub fn from_str_weavy_jit<T>(input: &str) -> Result<T, DeserializeError>
where
    T: Facet<'static>,
{
    JsonWeavyPlan::<T>::build_jit()?.from_str(input)
}

/// Deserialize a value from JSON bytes through the opt-in Weavy runner with JIT
/// enabled when a JSON native backend is available.
pub fn from_slice_weavy_jit<T>(input: &[u8]) -> Result<T, DeserializeError>
where
    T: Facet<'static>,
{
    JsonWeavyPlan::<T>::build_jit()?.from_slice(input)
}

/// Requested execution policy for a reusable Weavy JSON plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsonWeavyExecutionMode {
    /// Run with the portable Weavy interpreter.
    Interpreter,
    /// Use the native JIT when available, falling back to the interpreter when
    /// the current build or lowered program cannot run natively yet.
    Jit,
}

/// Backend that will execute a reusable Weavy JSON plan.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsonWeavyActiveBackend {
    /// The portable Weavy interpreter.
    Interpreter,
    /// Native copy-and-patch code.
    NativeJit,
}

/// One diagnostic record explaining why a JSON Weavy plan did not use native
/// JIT execution after JIT was requested.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonWeavyJitFallbackRecord {
    /// Program path that fell back.
    pub path: String,
    /// Stable fallback reason.
    pub reason: &'static str,
}

/// Diagnostic report for JSON Weavy native-JIT coverage.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct JsonWeavyJitFallbackReport {
    /// Fallback records. Empty means the plan is native-clean for the selected
    /// execution mode.
    pub records: Vec<JsonWeavyJitFallbackRecord>,
}

impl JsonWeavyJitFallbackReport {
    /// Whether this report contains no fallbacks.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }
}

/// Reusable opt-in Weavy JSON deserialization plan for `T`.
///
/// The default `facet_json::from_str` path is unchanged. This type is for the
/// new VM backend and lets callers separate typed-shape lowering from repeated
/// input decoding.
pub struct JsonWeavyPlan<T> {
    lowered: DenseLowered<ExecOp>,
    execution: JsonWeavyExecutionMode,
    #[cfg(all(
        feature = "jit",
        any(
            all(target_os = "macos", target_arch = "aarch64"),
            all(target_os = "linux", target_arch = "x86_64")
        )
    ))]
    native: Option<JsonNativePlan>,
    jit_fallback_reason: Option<&'static str>,
    _marker: PhantomData<fn() -> T>,
}

impl<T> JsonWeavyPlan<T>
where
    T: Facet<'static>,
{
    /// Lower `T::SHAPE` into the JSON-specific Weavy bytecode.
    pub fn build() -> Result<Self, DeserializeError> {
        Self::build_with_execution(JsonWeavyExecutionMode::Interpreter)
    }

    /// Lower `T::SHAPE` into JSON-specific Weavy bytecode with JIT enabled when
    /// a JSON native backend is available.
    pub fn build_jit() -> Result<Self, DeserializeError> {
        Self::build_with_execution(JsonWeavyExecutionMode::Jit)
    }

    /// Lower `T::SHAPE` into JSON-specific Weavy bytecode with an explicit
    /// execution mode.
    pub fn build_with_execution(
        execution: JsonWeavyExecutionMode,
    ) -> Result<Self, DeserializeError> {
        let symbolic = Lowering::new().lower(T::SHAPE)?;
        let lowered = resolve_json_lowered(symbolic)?;
        let mut jit_fallback_reason = None;

        #[cfg(all(
            feature = "jit",
            any(
                all(target_os = "macos", target_arch = "aarch64"),
                all(target_os = "linux", target_arch = "x86_64")
            )
        ))]
        let native = if execution == JsonWeavyExecutionMode::Jit {
            match JsonNativePlan::compile(&lowered) {
                Ok(native) => Some(native),
                Err(reason) => {
                    jit_fallback_reason = Some(reason);
                    None
                }
            }
        } else {
            None
        };

        #[cfg(not(all(
            feature = "jit",
            any(
                all(target_os = "macos", target_arch = "aarch64"),
                all(target_os = "linux", target_arch = "x86_64")
            )
        )))]
        if execution == JsonWeavyExecutionMode::Jit {
            jit_fallback_reason = Some(json_weavy_jit_fallback_reason());
        }

        Ok(Self {
            lowered,
            execution,
            #[cfg(all(
                feature = "jit",
                any(
                    all(target_os = "macos", target_arch = "aarch64"),
                    all(target_os = "linux", target_arch = "x86_64")
                )
            ))]
            native,
            jit_fallback_reason,
            _marker: PhantomData,
        })
    }

    /// Requested execution mode for this plan.
    #[must_use]
    pub fn execution_mode(&self) -> JsonWeavyExecutionMode {
        self.execution
    }

    /// Backend currently selected for this plan.
    #[must_use]
    pub fn active_backend(&self) -> JsonWeavyActiveBackend {
        #[cfg(all(
            feature = "jit",
            any(
                all(target_os = "macos", target_arch = "aarch64"),
                all(target_os = "linux", target_arch = "x86_64")
            )
        ))]
        if self.native.is_some() {
            return JsonWeavyActiveBackend::NativeJit;
        }

        JsonWeavyActiveBackend::Interpreter
    }

    /// Whether this build exposes Weavy's native copy-and-patch substrate.
    #[must_use]
    pub fn native_jit_available() -> bool {
        json_weavy_native_jit_available()
    }

    /// Report why this plan is not using native JIT execution.
    #[must_use]
    pub fn jit_fallback_report(&self) -> JsonWeavyJitFallbackReport {
        if self.execution != JsonWeavyExecutionMode::Jit {
            return JsonWeavyJitFallbackReport::default();
        }

        match self.jit_fallback_reason {
            Some(reason) => JsonWeavyJitFallbackReport {
                records: vec![JsonWeavyJitFallbackRecord {
                    path: "$".to_string(),
                    reason,
                }],
            },
            None => JsonWeavyJitFallbackReport::default(),
        }
    }

    /// Deserialize from a JSON string using this pre-lowered plan.
    pub fn from_str(&self, input: &str) -> Result<T, DeserializeError> {
        let mut parser = JsonParser::<true>::new(input.as_bytes());
        self.deserialize::<true>(&mut parser)
    }

    /// Deserialize from JSON bytes using this pre-lowered plan.
    pub fn from_slice(&self, input: &[u8]) -> Result<T, DeserializeError> {
        let mut parser = JsonParser::<false>::new(input);
        self.deserialize::<false>(&mut parser)
    }

    /// Deserialize from a JSON string and return Weavy runner counters.
    pub fn from_str_with_stats(&self, input: &str) -> Result<(T, RunStats), DeserializeError> {
        let mut parser = JsonParser::<true>::new(input.as_bytes());
        self.deserialize_with_stats::<true>(&mut parser)
    }

    /// Deserialize from JSON bytes and return Weavy runner counters.
    pub fn from_slice_with_stats(&self, input: &[u8]) -> Result<(T, RunStats), DeserializeError> {
        let mut parser = JsonParser::<false>::new(input);
        self.deserialize_with_stats::<false>(&mut parser)
    }

    fn deserialize_with_stats<const TRUSTED_UTF8: bool>(
        &self,
        parser: &mut JsonParser<'_, TRUSTED_UTF8>,
    ) -> Result<(T, RunStats), DeserializeError>
    where
        for<'de> JsonParser<'de, TRUSTED_UTF8>: ScalarInputPreselected<'de, TRUSTED_UTF8>,
    {
        let mut slot = MaybeUninit::<T>::uninit();
        let root = PtrUninit::from_maybe_uninit(&mut slot);
        let mut interp = JsonInterp::new(parser, root, &self.lowered.blocks);
        let stats = match weavy::run_dense_with_stats(&self.lowered, &mut interp) {
            Ok(stats) => stats,
            Err(err) => return Err(run_error(err)),
        };
        interp.finish_success();

        Ok((unsafe { slot.assume_init() }, stats))
    }

    fn deserialize<const TRUSTED_UTF8: bool>(
        &self,
        parser: &mut JsonParser<'_, TRUSTED_UTF8>,
    ) -> Result<T, DeserializeError>
    where
        for<'de> JsonParser<'de, TRUSTED_UTF8>: ScalarInputPreselected<'de, TRUSTED_UTF8>,
    {
        let mut slot = MaybeUninit::<T>::uninit();
        let root = PtrUninit::from_maybe_uninit(&mut slot);

        #[cfg(all(
            feature = "jit",
            any(
                all(target_os = "macos", target_arch = "aarch64"),
                all(target_os = "linux", target_arch = "x86_64")
            )
        ))]
        if let Some(native) = &self.native
            && native.should_enter()
        {
            native.run(parser, root, &self.lowered)?;
            return Ok(unsafe { slot.assume_init() });
        }

        let mut interp = JsonInterp::new(parser, root, &self.lowered.blocks);
        if let Err(err) = weavy::run_dense(&self.lowered, &mut interp) {
            return Err(run_error(err));
        }
        interp.finish_success();

        Ok(unsafe { slot.assume_init() })
    }
}

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
struct JsonNativePlan {
    native: weavy::jit::NativeProgram,
    calls: Box<[weavy::jit::HostCallInfo]>,
    root: Box<JsonNativeRootInfo>,
}

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
// Safety: native plans are immutable after construction; raw pointers in their
// program stream point into `calls`/`scalar_structs`, both owned by the plan.
unsafe impl Send for JsonNativePlan {}

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
// Safety: see the `Send` impl; running a plan only mutates per-call state.
unsafe impl Sync for JsonNativePlan {}

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
enum JsonNativeRootInfo {
    ScalarStruct(JsonNativeScalarStructInfo),
    ScalarStructList(JsonNativeScalarStructListInfo),
}

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
struct JsonNativeScalarStructInfo {
    shape: &'static Shape,
    ordered_names: Box<[&'static str]>,
    fields: Box<[ScalarFieldPlan]>,
    cursor_readers: Box<[NativeCursorScalarReader]>,
    dispatch: Option<RawFieldDispatch>,
    ordered_probe_skip: AtomicU8,
}

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
type NativeCursorScalarReader =
    fn(&mut NativeOrderedRootCursor<'_>, PtrUninit) -> Result<bool, DeserializeError>;

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
struct JsonNativeScalarStructListInfo {
    list_shape: &'static Shape,
    list: ListDef,
    element_layout: Layout,
    element: JsonNativeScalarStructInfo,
}

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
const JSON_NATIVE_STATUS_OK: u64 = 0;

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
const JSON_NATIVE_STATUS_FALLBACK: u64 = 1;

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
const ORDERED_PROBE_BACKOFF: u8 = u8::MAX;

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
#[repr(C)]
struct JsonNativeState {
    input: *const u8,
    input_len: usize,
    cursor_pos: usize,
    status: u64,
    parser: *mut (),
    lowered: *const DenseLowered<ExecOp>,
    trusted_utf8: bool,
    base: PtrUninit,
    error: Option<DeserializeError>,
}

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
impl JsonNativePlan {
    fn compile(lowered: &DenseLowered<ExecOp>) -> Result<Self, &'static str> {
        let root = match lowered.program.as_slice() {
            [
                JsonOp::ReadScalarStruct {
                    shape,
                    fields,
                    dispatch,
                },
            ] => JsonNativeRootInfo::ScalarStruct(Self::compile_scalar_struct_info(
                shape, fields, dispatch,
            )?),
            [
                JsonOp::ReadList {
                    list_shape,
                    list,
                    element_layout,
                    loop_id,
                },
            ] => JsonNativeRootInfo::ScalarStructList(Self::compile_scalar_struct_list_info(
                lowered,
                list_shape,
                *list,
                *element_layout,
                *loop_id,
            )?),
            _ => {
                return Err(
                    "JSON native JIT currently supports root scalar structs or scalar struct lists only",
                );
            }
        };
        let root = Box::new(root);

        let calls = vec![weavy::jit::HostCallInfo {
            info: core::ptr::from_ref(&*root).cast(),
            call: json_native_read_root,
        }]
        .into_boxed_slice();

        let mut layout = weavy::jit::StencilLayout::new();
        let root_chain = layout.start_chain();
        let root_start = match &*root {
            JsonNativeRootInfo::ScalarStruct(_) => {
                layout.emit_stencil(native_stencils::ROOT_OBJECT_START)
            }
            JsonNativeRootInfo::ScalarStructList(_) => {
                layout.emit_stencil(native_stencils::ROOT_ARRAY_START)
            }
        };
        let hostcall = layout.emit_hostcall(root_chain, core::ptr::from_ref(&calls[0]));
        let done = layout.emit_done();
        let root_start_cont = match &*root {
            JsonNativeRootInfo::ScalarStruct(_) => native_stencils::ROOT_OBJECT_START_CONT,
            JsonNativeRootInfo::ScalarStructList(_) => native_stencils::ROOT_ARRAY_START_CONT,
        };
        for &rel in root_start_cont {
            layout.patch_continuation(root_start + rel, hostcall);
        }
        layout.patch_hostcall_continuation(hostcall, done);
        let native = weavy::jit::NativeProgram::new(layout, root_chain);

        Ok(Self {
            native,
            calls,
            root,
        })
    }

    fn compile_scalar_struct_info(
        shape: &'static Shape,
        fields: &[ScalarFieldPlan],
        dispatch: &Option<RawFieldDispatch>,
    ) -> Result<JsonNativeScalarStructInfo, &'static str> {
        let fields = fields.to_vec().into_boxed_slice();
        if fields
            .iter()
            .any(|field| !matches!(field.missing, MissingField::Required))
        {
            return Err("JSON native JIT currently supports required scalar struct fields only");
        }

        let ordered_names = fields
            .iter()
            .map(|field| field.name)
            .collect::<Vec<_>>()
            .into_boxed_slice();
        let cursor_readers = fields
            .iter()
            .map(|field| native_cursor_scalar_reader(field.scalar.scalar))
            .collect::<Vec<_>>()
            .into_boxed_slice();

        Ok(JsonNativeScalarStructInfo {
            shape,
            ordered_names,
            fields,
            cursor_readers,
            dispatch: dispatch.clone(),
            ordered_probe_skip: AtomicU8::new(0),
        })
    }

    fn compile_scalar_struct_list_info(
        lowered: &DenseLowered<ExecOp>,
        list_shape: &'static Shape,
        list: ListDef,
        element_layout: Layout,
        loop_id: ExecBlock,
    ) -> Result<JsonNativeScalarStructListInfo, &'static str> {
        if list.from_raw_parts().is_none() {
            return Err(
                "JSON native JIT currently supports raw-adoptable scalar struct lists only",
            );
        }

        let loop_program = lowered
            .blocks
            .get(loop_id.index())
            .ok_or("JSON native JIT root list loop block is missing")?;
        let [
            JsonOp::ListNext {
                element_program,
                element_scalar,
                element_option_scalar,
                ..
            },
        ] = loop_program.as_slice()
        else {
            return Err("JSON native JIT currently supports scalar struct list loops only");
        };
        if element_scalar.is_some() || element_option_scalar.is_some() {
            return Err("JSON native JIT currently supports scalar struct list elements only");
        }

        let element = Self::compile_scalar_struct_program(lowered, element_program)?;

        Ok(JsonNativeScalarStructListInfo {
            list_shape,
            list,
            element_layout,
            element,
        })
    }

    fn compile_scalar_struct_program(
        lowered: &DenseLowered<ExecOp>,
        program: &[ExecOp],
    ) -> Result<JsonNativeScalarStructInfo, &'static str> {
        let program = match program {
            [JsonOp::CallBlock(block)] => lowered
                .blocks
                .get(block.index())
                .ok_or("JSON native JIT scalar struct element block is missing")?
                .as_slice(),
            program => program,
        };

        let [
            JsonOp::ReadScalarStruct {
                shape,
                fields,
                dispatch,
            },
        ] = program
        else {
            return Err("JSON native JIT currently supports scalar struct list elements only");
        };

        Self::compile_scalar_struct_info(shape, fields, dispatch)
    }

    fn should_enter(&self) -> bool {
        self.root.should_enter_native()
    }

    fn run<const TRUSTED_UTF8: bool>(
        &self,
        parser: &mut JsonParser<'_, TRUSTED_UTF8>,
        base: PtrUninit,
        lowered: &DenseLowered<ExecOp>,
    ) -> Result<(), DeserializeError>
    where
        for<'de> JsonParser<'de, TRUSTED_UTF8>: ScalarInputPreselected<'de, TRUSTED_UTF8>,
    {
        let (input, input_len, cursor_pos) = parser.native_input_parts();
        let mut state = JsonNativeState {
            input,
            input_len,
            cursor_pos,
            status: JSON_NATIVE_STATUS_OK,
            parser: (parser as *mut JsonParser<'_, TRUSTED_UTF8>).cast(),
            lowered: lowered as *const DenseLowered<ExecOp>,
            trusted_utf8: TRUSTED_UTF8,
            base,
            error: None,
        };
        let mut cx = weavy::jit::HostCallCtx::new(self.native.entry_prog(), &mut state);
        let entry = unsafe {
            self.native
                .entry_fn::<weavy::jit::HostCallCtx<JsonNativeState>>()
        };
        unsafe {
            entry(&mut cx);
        }

        let _ = (&self.calls, &self.root);
        if let Some(error) = state.error {
            return Err(error);
        }
        match state.status {
            JSON_NATIVE_STATUS_OK => Ok(()),
            JSON_NATIVE_STATUS_FALLBACK => {
                self.root.record_ordered_probe(false);
                state.read_interpreted(parser)
            }
            _ => {
                self.root.record_ordered_probe(false);
                state.read_interpreted(parser)
            }
        }
    }
}

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
unsafe extern "C" fn json_native_read_root(state: *mut (), info: *const ()) -> bool {
    let state = unsafe { &mut *state.cast::<JsonNativeState>() };
    let info = unsafe { &*info.cast::<JsonNativeRootInfo>() };
    let result = if state.trusted_utf8 {
        unsafe { state.read_root_after_native_start::<true>(info) }
    } else {
        unsafe { state.read_root_after_native_start::<false>(info) }
    };

    match result {
        Ok(()) => true,
        Err(error) => {
            state.error = Some(error);
            false
        }
    }
}

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
impl JsonNativeState {
    unsafe fn read_root_after_native_start<const TRUSTED_UTF8: bool>(
        &mut self,
        info: &JsonNativeRootInfo,
    ) -> Result<(), DeserializeError>
    where
        for<'de> JsonParser<'de, TRUSTED_UTF8>: ScalarInputPreselected<'de, TRUSTED_UTF8>,
    {
        match info {
            JsonNativeRootInfo::ScalarStruct(info) => unsafe {
                self.read_scalar_struct_after_object_start::<TRUSTED_UTF8>(info)
            },
            JsonNativeRootInfo::ScalarStructList(info) => unsafe {
                self.read_scalar_struct_list_after_array_start::<TRUSTED_UTF8>(info)
            },
        }
    }

    unsafe fn read_scalar_struct_after_object_start<const TRUSTED_UTF8: bool>(
        &mut self,
        info: &JsonNativeScalarStructInfo,
    ) -> Result<(), DeserializeError>
    where
        for<'de> JsonParser<'de, TRUSTED_UTF8>: ScalarInputPreselected<'de, TRUSTED_UTF8>,
    {
        let parser = unsafe { &mut *self.parser.cast::<JsonParser<'_, TRUSTED_UTF8>>() };
        let can_probe_ordered = tiny_i32_struct_fields_are_fusible(&info.fields)
            || info.fields.len() <= u64::BITS as usize;
        if !can_probe_ordered {
            return self.read_scalar_struct_interpreted(parser, info);
        }

        let Some(mut cursor) = parser.native_ordered_root_cursor_from(self.cursor_pos) else {
            return self.read_scalar_struct_interpreted(parser, info);
        };
        {
            let mut guard = NativeScalarStructGuard::new(self.base, &info.fields);
            let matched = if tiny_i32_struct_fields_are_fusible(&info.fields) {
                self.read_cursor_i32_scalar_struct_object_from(
                    &mut cursor,
                    info,
                    &mut guard,
                    0,
                    None,
                )?
            } else {
                self.read_cursor_scalar_struct_object_from(
                    parser,
                    &mut cursor,
                    info,
                    &mut guard,
                    0,
                    None,
                )?
            };
            if matched {
                info.record_ordered_probe(true);
                guard.finish();
                parser.commit_native_ordered_root(cursor);
                return Ok(());
            }
        }

        info.record_ordered_probe(false);
        self.read_scalar_struct_interpreted(parser, info)
    }

    unsafe fn read_scalar_struct_list_after_array_start<const TRUSTED_UTF8: bool>(
        &mut self,
        info: &JsonNativeScalarStructListInfo,
    ) -> Result<(), DeserializeError>
    where
        for<'de> JsonParser<'de, TRUSTED_UTF8>: ScalarInputPreselected<'de, TRUSTED_UTF8>,
    {
        let parser = unsafe { &mut *self.parser.cast::<JsonParser<'_, TRUSTED_UTF8>>() };
        let Some(cursor) = parser.native_ordered_root_cursor_from(self.cursor_pos) else {
            return self.read_interpreted(parser);
        };

        if tiny_i32_struct_fields_are_fusible(&info.element.fields) {
            if self.read_cursor_i32_scalar_struct_list_after_array_start(parser, info, cursor)? {
                info.record_ordered_probe(true);
                return Ok(());
            }
            info.record_ordered_probe(false);
            return self.read_interpreted(parser);
        }

        if tiny_f64_struct_fields_are_fusible(&info.element.fields) {
            if self.read_cursor_f64_scalar_struct_list_after_array_start(parser, info, cursor)? {
                info.record_ordered_probe(true);
                return Ok(());
            }
            info.record_ordered_probe(false);
            return self.read_interpreted(parser);
        }

        if info.element.fields.len() <= u64::BITS as usize {
            if self.read_cursor_scalar_struct_list_after_array_start(parser, info, cursor)? {
                info.record_ordered_probe(true);
                return Ok(());
            }
            info.record_ordered_probe(false);
            return self.read_interpreted(parser);
        }

        self.read_interpreted(parser)
    }

    fn adopt_native_list(
        &mut self,
        info: &JsonNativeScalarStructListInfo,
        builder: &mut RawArrayBuilder,
    ) -> Result<(), DeserializeError> {
        let from_raw_parts = info
            .list
            .from_raw_parts()
            .ok_or_else(|| unsupported(info.list_shape, "list from_raw_parts"))?;
        unsafe {
            from_raw_parts(
                self.base,
                PtrMut::new(builder.ptr()),
                builder.len(),
                builder.cap(),
            );
        }
        builder.adopt();
        Ok(())
    }

    #[inline]
    fn read_cursor_i32_scalar_struct_list_after_array_start<'de, const TRUSTED_UTF8: bool>(
        &mut self,
        parser: &mut JsonParser<'de, TRUSTED_UTF8>,
        info: &JsonNativeScalarStructListInfo,
        mut cursor: NativeOrderedRootCursor<'de>,
    ) -> Result<bool, DeserializeError> {
        let mut builder = RawArrayBuilder::new(
            info.element_layout,
            info.list.t() as *const Shape as *const (),
            drop_shape_value,
        );
        let mut after_element = false;
        loop {
            match cursor.consume_array_step(after_element)? {
                Some(NativeArrayStep::End) => {
                    self.adopt_native_list(info, &mut builder)?;
                    parser.commit_native_ordered_root(cursor);
                    return Ok(true);
                }
                Some(NativeArrayStep::Element) => {}
                None => return Ok(false),
            }

            let slot = builder.next_uninit_slot().map_err(raw_alloc_error)?;
            let old_base = self.base;
            self.base = PtrUninit::new(slot);

            let mut guard = NativeScalarStructGuard::new(self.base, &info.element.fields);
            let matched = self.read_cursor_i32_scalar_struct_object(
                &mut cursor,
                &info.element,
                &mut guard,
                Some(false),
            );
            self.base = old_base;

            let matched = matched?;
            if !matched {
                return Ok(false);
            }

            guard.finish();
            unsafe {
                builder.mark_initialized();
            }
            after_element = true;
        }
    }

    #[inline]
    fn read_cursor_f64_scalar_struct_list_after_array_start<'de, const TRUSTED_UTF8: bool>(
        &mut self,
        parser: &mut JsonParser<'de, TRUSTED_UTF8>,
        info: &JsonNativeScalarStructListInfo,
        mut cursor: NativeOrderedRootCursor<'de>,
    ) -> Result<bool, DeserializeError> {
        let mut builder = RawArrayBuilder::new(
            info.element_layout,
            info.list.t() as *const Shape as *const (),
            drop_shape_value,
        );
        let mut after_element = false;
        loop {
            match cursor.consume_array_step(after_element)? {
                Some(NativeArrayStep::End) => {
                    self.adopt_native_list(info, &mut builder)?;
                    parser.commit_native_ordered_root(cursor);
                    return Ok(true);
                }
                Some(NativeArrayStep::Element) => {}
                None => return Ok(false),
            }

            let slot = builder.next_uninit_slot().map_err(raw_alloc_error)?;
            let old_base = self.base;
            self.base = PtrUninit::new(slot);

            let mut guard = NativeScalarStructGuard::new(self.base, &info.element.fields);
            let matched = self.read_cursor_f64_scalar_struct_object(
                &mut cursor,
                &info.element,
                &mut guard,
                Some(false),
            );
            self.base = old_base;

            let matched = matched?;
            if !matched {
                return Ok(false);
            }

            guard.finish();
            unsafe {
                builder.mark_initialized();
            }
            after_element = true;
        }
    }

    #[inline]
    fn read_cursor_scalar_struct_list_after_array_start<'input, const TRUSTED_UTF8: bool>(
        &mut self,
        parser: &mut JsonParser<'input, TRUSTED_UTF8>,
        info: &JsonNativeScalarStructListInfo,
        mut cursor: NativeOrderedRootCursor<'input>,
    ) -> Result<bool, DeserializeError>
    where
        for<'de> JsonParser<'de, TRUSTED_UTF8>: ScalarInputPreselected<'de, TRUSTED_UTF8>,
    {
        let mut builder = RawArrayBuilder::new(
            info.element_layout,
            info.list.t() as *const Shape as *const (),
            drop_shape_value,
        );
        let mut after_element = false;
        loop {
            match cursor.consume_array_step(after_element)? {
                Some(NativeArrayStep::End) => {
                    self.adopt_native_list(info, &mut builder)?;
                    parser.commit_native_ordered_root(cursor);
                    return Ok(true);
                }
                Some(NativeArrayStep::Element) => {}
                None => return Ok(false),
            }

            let slot = builder.next_uninit_slot().map_err(raw_alloc_error)?;
            let old_base = self.base;
            self.base = PtrUninit::new(slot);

            let mut guard = NativeScalarStructGuard::new(self.base, &info.element.fields);
            let matched = self.read_cursor_scalar_struct_object(
                parser,
                &mut cursor,
                &info.element,
                &mut guard,
                Some(false),
            );
            self.base = old_base;

            let matched = matched?;
            if !matched {
                return Ok(false);
            }

            guard.finish();
            unsafe {
                builder.mark_initialized();
            }
            after_element = true;
        }
    }

    #[inline]
    fn read_cursor_i32_scalar_struct_object(
        &mut self,
        cursor: &mut NativeOrderedRootCursor<'_>,
        info: &JsonNativeScalarStructInfo,
        guard: &mut NativeScalarStructGuard<'_>,
        array_element_object: Option<bool>,
    ) -> Result<bool, DeserializeError> {
        self.read_cursor_i32_scalar_struct_object_from(cursor, info, guard, 0, array_element_object)
    }

    #[inline]
    fn read_cursor_i32_scalar_struct_object_from(
        &mut self,
        cursor: &mut NativeOrderedRootCursor<'_>,
        info: &JsonNativeScalarStructInfo,
        guard: &mut NativeScalarStructGuard<'_>,
        start_index: usize,
        array_element_object: Option<bool>,
    ) -> Result<bool, DeserializeError> {
        if let Some(require_comma) = array_element_object
            && !cursor.consume_array_object_start(require_comma)?
        {
            return Ok(false);
        }

        for (index, expected) in info
            .ordered_names
            .iter()
            .copied()
            .enumerate()
            .skip(start_index)
        {
            let Some(_span) = cursor.consume_ordered_field_prefix(expected, index > 0)? else {
                return Ok(false);
            };
            let Some((_, value)) = cursor.consume_i32()? else {
                return Ok(false);
            };

            let field = info.fields[index];
            let field_ptr = unsafe { self.base.field_uninit(field.offset) };
            unsafe {
                field_ptr.put(value);
            }
            guard.mark(index);
        }

        if !cursor.consume_object_end()? {
            return Ok(false);
        }
        Ok(true)
    }

    #[inline]
    fn read_cursor_scalar_struct_object<const TRUSTED_UTF8: bool>(
        &mut self,
        parser: &JsonParser<'_, TRUSTED_UTF8>,
        cursor: &mut NativeOrderedRootCursor<'_>,
        info: &JsonNativeScalarStructInfo,
        guard: &mut NativeScalarStructGuard<'_>,
        array_element_object: Option<bool>,
    ) -> Result<bool, DeserializeError>
    where
        for<'de> JsonParser<'de, TRUSTED_UTF8>: ScalarInputPreselected<'de, TRUSTED_UTF8>,
    {
        self.read_cursor_scalar_struct_object_from(
            parser,
            cursor,
            info,
            guard,
            0,
            array_element_object,
        )
    }

    #[inline]
    fn read_cursor_scalar_struct_object_from<const TRUSTED_UTF8: bool>(
        &mut self,
        parser: &JsonParser<'_, TRUSTED_UTF8>,
        cursor: &mut NativeOrderedRootCursor<'_>,
        info: &JsonNativeScalarStructInfo,
        guard: &mut NativeScalarStructGuard<'_>,
        start_index: usize,
        array_element_object: Option<bool>,
    ) -> Result<bool, DeserializeError>
    where
        for<'de> JsonParser<'de, TRUSTED_UTF8>: ScalarInputPreselected<'de, TRUSTED_UTF8>,
    {
        if tiny_f64_struct_fields_are_fusible(&info.fields) {
            return self.read_cursor_f64_scalar_struct_object_from(
                cursor,
                info,
                guard,
                start_index,
                array_element_object,
            );
        }

        if let Some(require_comma) = array_element_object
            && !cursor.consume_array_object_start(require_comma)?
        {
            return Ok(false);
        }

        for (index, expected) in info
            .ordered_names
            .iter()
            .copied()
            .enumerate()
            .skip(start_index)
        {
            let Some(_span) = cursor.consume_ordered_field_prefix(expected, index > 0)? else {
                return Ok(false);
            };

            let field = &info.fields[index];
            let field_ptr = unsafe { self.base.field_uninit(field.offset) };
            if !info.cursor_readers[index](cursor, field_ptr)? {
                let token = cursor.consume_scalar_token("scalar")?;
                parser.write_scalar_input_preselected(
                    field,
                    field_ptr,
                    JsonScalarInput::Raw(token),
                )?;
            }
            guard.mark(index);
        }

        if !cursor.consume_object_end()? {
            return Ok(false);
        }
        Ok(true)
    }

    #[inline]
    fn read_cursor_f64_scalar_struct_object(
        &mut self,
        cursor: &mut NativeOrderedRootCursor<'_>,
        info: &JsonNativeScalarStructInfo,
        guard: &mut NativeScalarStructGuard<'_>,
        array_element_object: Option<bool>,
    ) -> Result<bool, DeserializeError> {
        self.read_cursor_f64_scalar_struct_object_from(cursor, info, guard, 0, array_element_object)
    }

    #[inline]
    fn read_cursor_f64_scalar_struct_object_from(
        &mut self,
        cursor: &mut NativeOrderedRootCursor<'_>,
        info: &JsonNativeScalarStructInfo,
        guard: &mut NativeScalarStructGuard<'_>,
        start_index: usize,
        array_element_object: Option<bool>,
    ) -> Result<bool, DeserializeError> {
        if let Some(require_comma) = array_element_object
            && !cursor.consume_array_object_start(require_comma)?
        {
            return Ok(false);
        }

        for (index, expected) in info
            .ordered_names
            .iter()
            .copied()
            .enumerate()
            .skip(start_index)
        {
            let Some(value) = cursor.consume_ordered_f64_field(expected, index > 0)? else {
                return Ok(false);
            };

            let field = info.fields[index];
            let field_ptr = unsafe { self.base.field_uninit(field.offset) };
            unsafe {
                field_ptr.put(value);
            }
            guard.mark(index);
        }

        if !cursor.consume_object_end()? {
            return Ok(false);
        }
        Ok(true)
    }

    fn read_scalar_struct_interpreted<const TRUSTED_UTF8: bool>(
        &mut self,
        parser: &mut JsonParser<'_, TRUSTED_UTF8>,
        info: &JsonNativeScalarStructInfo,
    ) -> Result<(), DeserializeError>
    where
        for<'de> JsonParser<'de, TRUSTED_UTF8>: ScalarInputPreselected<'de, TRUSTED_UTF8>,
    {
        let mut interp = JsonInterp::new(parser, self.base, &[]);
        interp.read_scalar_struct(info.shape, &info.fields, info.dispatch.as_ref())?;
        interp.finish_success();
        Ok(())
    }

    fn read_interpreted<const TRUSTED_UTF8: bool>(
        &mut self,
        parser: &mut JsonParser<'_, TRUSTED_UTF8>,
    ) -> Result<(), DeserializeError>
    where
        for<'de> JsonParser<'de, TRUSTED_UTF8>: ScalarInputPreselected<'de, TRUSTED_UTF8>,
    {
        let lowered = unsafe { &*self.lowered };
        let mut interp = JsonInterp::new(parser, self.base, &lowered.blocks);
        if let Err(err) = weavy::run_dense(lowered, &mut interp) {
            return Err(run_error(err));
        }
        interp.finish_success();
        Ok(())
    }
}

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
fn native_cursor_scalar_reader(scalar: ScalarType) -> NativeCursorScalarReader {
    match scalar {
        ScalarType::Unit => read_native_cursor_unit,
        ScalarType::Bool => read_native_cursor_bool,
        ScalarType::U8 => read_native_cursor_unsigned::<u8>,
        ScalarType::U16 => read_native_cursor_unsigned::<u16>,
        ScalarType::U32 => read_native_cursor_unsigned::<u32>,
        ScalarType::U64 => read_native_cursor_unsigned::<u64>,
        ScalarType::U128 => read_native_cursor_unsigned::<u128>,
        ScalarType::USize => read_native_cursor_unsigned::<usize>,
        ScalarType::I8 => read_native_cursor_signed::<i8>,
        ScalarType::I16 => read_native_cursor_signed::<i16>,
        ScalarType::I32 => read_native_cursor_signed::<i32>,
        ScalarType::I64 => read_native_cursor_signed::<i64>,
        ScalarType::I128 => read_native_cursor_signed::<i128>,
        ScalarType::ISize => read_native_cursor_signed::<isize>,
        ScalarType::F32 => read_native_cursor_f32,
        ScalarType::F64 => read_native_cursor_f64,
        _ => read_native_cursor_raw,
    }
}

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
fn read_native_cursor_raw(
    _cursor: &mut NativeOrderedRootCursor<'_>,
    _dst: PtrUninit,
) -> Result<bool, DeserializeError> {
    Ok(false)
}

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
fn read_native_cursor_unit(
    cursor: &mut NativeOrderedRootCursor<'_>,
    dst: PtrUninit,
) -> Result<bool, DeserializeError> {
    if cursor.consume_null()? {
        unsafe {
            dst.put(());
        }
        Ok(true)
    } else {
        Ok(false)
    }
}

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
fn read_native_cursor_bool(
    cursor: &mut NativeOrderedRootCursor<'_>,
    dst: PtrUninit,
) -> Result<bool, DeserializeError> {
    if let Some(value) = cursor.consume_bool()? {
        unsafe {
            dst.put(value);
        }
        Ok(true)
    } else {
        Ok(false)
    }
}

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
fn read_native_cursor_unsigned<T>(
    cursor: &mut NativeOrderedRootCursor<'_>,
    dst: PtrUninit,
) -> Result<bool, DeserializeError>
where
    T: TryFrom<u128>,
{
    if let Some(value) = cursor.consume_unsigned_integer::<T>()? {
        unsafe {
            dst.put(value);
        }
        Ok(true)
    } else {
        Ok(false)
    }
}

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
fn read_native_cursor_signed<T>(
    cursor: &mut NativeOrderedRootCursor<'_>,
    dst: PtrUninit,
) -> Result<bool, DeserializeError>
where
    T: TryFrom<i128>,
{
    if let Some(value) = cursor.consume_signed_integer::<T>()? {
        unsafe {
            dst.put(value);
        }
        Ok(true)
    } else {
        Ok(false)
    }
}

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
fn read_native_cursor_f32(
    cursor: &mut NativeOrderedRootCursor<'_>,
    dst: PtrUninit,
) -> Result<bool, DeserializeError> {
    if let Some(value) = cursor.consume_f64_number()? {
        unsafe {
            dst.put(value as f32);
        }
        Ok(true)
    } else {
        Ok(false)
    }
}

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
fn read_native_cursor_f64(
    cursor: &mut NativeOrderedRootCursor<'_>,
    dst: PtrUninit,
) -> Result<bool, DeserializeError> {
    if let Some(value) = cursor.consume_f64_number()? {
        unsafe {
            dst.put(value);
        }
        Ok(true)
    } else {
        Ok(false)
    }
}

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
impl JsonNativeRootInfo {
    fn should_enter_native(&self) -> bool {
        match self {
            Self::ScalarStruct(info) => info.should_enter_native(),
            Self::ScalarStructList(info) => info.should_enter_native(),
        }
    }

    fn record_ordered_probe(&self, matched: bool) {
        match self {
            Self::ScalarStruct(info) => info.record_ordered_probe(matched),
            Self::ScalarStructList(info) => info.record_ordered_probe(matched),
        }
    }
}

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
impl JsonNativeScalarStructInfo {
    fn should_enter_native(&self) -> bool {
        let skip = self.ordered_probe_skip.load(Ordering::Relaxed);
        if skip == 0 {
            return true;
        }

        self.ordered_probe_skip
            .store(skip.saturating_sub(1), Ordering::Relaxed);
        false
    }

    fn record_ordered_probe(&self, matched: bool) {
        self.ordered_probe_skip.store(
            if matched { 0 } else { ORDERED_PROBE_BACKOFF },
            Ordering::Relaxed,
        );
    }
}

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
impl JsonNativeScalarStructListInfo {
    fn should_enter_native(&self) -> bool {
        self.element.should_enter_native()
    }

    fn record_ordered_probe(&self, matched: bool) {
        self.element.record_ordered_probe(matched);
    }
}

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
struct NativeScalarStructGuard<'program> {
    base: PtrUninit,
    fields: &'program [ScalarFieldPlan],
    initialized: u64,
}

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
impl<'program> NativeScalarStructGuard<'program> {
    fn new(base: PtrUninit, fields: &'program [ScalarFieldPlan]) -> Self {
        debug_assert!(fields.len() <= u64::BITS as usize);
        Self {
            base,
            fields,
            initialized: 0,
        }
    }

    fn mark(&mut self, index: usize) {
        self.initialized |= struct_seen_bit(index);
    }

    fn finish(self) {
        core::mem::forget(self);
    }
}

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
impl Drop for NativeScalarStructGuard<'_> {
    fn drop(&mut self) {
        for index in (0..self.fields.len()).rev() {
            if (self.initialized & struct_seen_bit(index)) == 0 {
                continue;
            }

            let field = self.fields[index];
            let ptr = unsafe { self.base.field_init(field.offset) };
            unsafe {
                let _ = field.shape.call_drop_in_place(ptr);
            }
        }
    }
}

#[cfg(not(feature = "jit"))]
fn json_weavy_native_jit_available() -> bool {
    false
}

#[cfg(feature = "jit")]
fn json_weavy_native_jit_available() -> bool {
    weavy::jit::NATIVE_COPY_PATCH_AVAILABLE
}

#[cfg(not(feature = "jit"))]
fn json_weavy_jit_fallback_reason() -> &'static str {
    "facet-json was built without its jit feature"
}

#[cfg(all(
    feature = "jit",
    not(any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    ))
))]
fn json_weavy_jit_fallback_reason() -> &'static str {
    let _ = json_weavy_native_jit_available();
    "native JIT is not enabled for this build target"
}

fn run_error(err: RunError<ExecBlock, DeserializeError>) -> DeserializeError {
    match err {
        RunError::Step(err) => err,
        RunError::MissingBlock(block) => vm_error(
            None,
            DeserializeErrorKind::Unsupported {
                message: format!("missing Weavy block {block:?}").into(),
            },
        ),
    }
}

#[derive(Clone, Debug)]
enum JsonOp<Block> {
    CallBlock(Block),
    ReadScalar {
        shape: &'static Shape,
        scalar: ScalarPlan,
    },
    ReadScalarStruct {
        shape: &'static Shape,
        fields: Box<[ScalarFieldPlan]>,
        dispatch: Option<RawFieldDispatch>,
    },
    ReadStruct {
        shape: &'static Shape,
        fields: Box<[FieldPlan<Block>]>,
        dispatch: Option<RawFieldDispatch>,
        loop_id: Block,
    },
    ReadExternalEnum {
        shape: &'static Shape,
        enum_type: EnumType,
        variants: Box<[ExternalVariantPlan<Block>]>,
    },
    ReadTaggedEnum {
        shape: &'static Shape,
        enum_type: EnumType,
        tag_key: &'static str,
        content_key: Option<&'static str>,
        variants: Box<[ExternalVariantPlan<Block>]>,
    },
    StructNext {
        shape: &'static Shape,
        loop_id: Block,
        raw_field_dispatch: bool,
        tracking: StructTracking,
    },
    ReadOption {
        option: OptionDef,
        some_program: Program<JsonOp<Block>>,
        some_scalar: Option<ScalarPlan>,
        inner_layout: Layout,
    },
    ReadList {
        list_shape: &'static Shape,
        list: ListDef,
        element_layout: Layout,
        loop_id: Block,
    },
    ListNext {
        list: ListDef,
        element_program: Program<JsonOp<Block>>,
        element_scalar: Option<ScalarType>,
        element_option_scalar: Option<ListOptionScalar>,
        element_layout: Layout,
        loop_id: Block,
    },
    ReadSet {
        set_shape: &'static Shape,
        set: SetDef,
        loop_id: Block,
    },
    SetNext {
        set: SetDef,
        element_program: Program<JsonOp<Block>>,
        element_scalar: Option<ScalarType>,
        element_option_scalar: Option<ListOptionScalar>,
        element_layout: Layout,
        loop_id: Block,
    },
    ReadMap {
        map_shape: &'static Shape,
        map: MapDef,
        loop_id: Block,
    },
    MapNext {
        map: MapDef,
        key_scalar: ScalarType,
        key_layout: Layout,
        value_program: Program<JsonOp<Block>>,
        value_scalar: Option<ScalarPlan>,
        value_layout: Layout,
        loop_id: Block,
    },
    ReadPointer {
        pointer: PointerDef,
        pointee_program: Program<JsonOp<Block>>,
        pointee_layout: Layout,
    },
}

#[derive(Clone, Debug)]
struct FieldPlan<Block> {
    name: &'static str,
    alias: Option<&'static str>,
    offset: usize,
    shape: &'static Shape,
    program: Program<JsonOp<Block>>,
    scalar: Option<ScalarPlan>,
    missing: MissingField,
}

#[derive(Clone, Debug)]
struct ExternalVariantPlan<Block> {
    index: usize,
    variant: &'static Variant,
    fields: Box<[FieldPlan<Block>]>,
    dispatch: Option<RawFieldDispatch>,
    loop_id: Option<Block>,
    tracking: StructTracking,
}

struct TaggedRawField<'de> {
    name: Cow<'de, str>,
    raw: &'de str,
    span: Span,
}

#[derive(Clone, Copy, Debug)]
struct ScalarPlan {
    scalar: ScalarType,
    write: Option<MaterializedScalarWriter>,
}

#[derive(Clone, Copy, Debug)]
struct ScalarFieldPlan {
    name: &'static str,
    alias: Option<&'static str>,
    offset: usize,
    shape: &'static Shape,
    scalar: ScalarPlan,
    input_trusted: Option<ScalarInputWriter<true>>,
    input_checked: Option<ScalarInputWriter<false>>,
    missing: MissingField,
}

type MaterializedScalarWriter = for<'de> unsafe fn(
    &'static Shape,
    PtrUninit,
    JsonScalarToken<'de>,
    Span,
) -> Result<(), DeserializeError>;

type ScalarInputWriter<const TRUSTED_UTF8: bool> = for<'de> fn(
    &JsonParser<'de, TRUSTED_UTF8>,
    &'static Shape,
    PtrUninit,
    JsonScalarInput<'de>,
) -> Result<(), DeserializeError>;

impl ScalarPlan {
    fn new(scalar: ScalarType) -> Self {
        Self {
            scalar,
            write: materialized_scalar_writer(scalar),
        }
    }

    unsafe fn write(
        self,
        shape: &'static Shape,
        dst: PtrUninit,
        value: JsonScalarToken<'_>,
        span: Span,
    ) -> Result<(), DeserializeError> {
        if matches!(value, JsonScalarToken::Null) {
            return unsafe { write_default_from_null(shape, dst, span) };
        }

        if let Some(write) = self.write {
            unsafe {
                write(shape, dst, value, span)?;
            }
            return Ok(());
        }

        unsafe { write_scalar(shape, self.scalar, dst, value, span) }
    }

    unsafe fn write_input<'de, const TRUSTED_UTF8: bool>(
        self,
        parser: &JsonParser<'de, TRUSTED_UTF8>,
        shape: &'static Shape,
        dst: PtrUninit,
        value: JsonScalarInput<'de>,
    ) -> Result<(), DeserializeError> {
        if let Some(span) = scalar_input_null_span(&value) {
            return unsafe { write_default_from_null(shape, dst, span) };
        }

        match self.scalar {
            ScalarType::Unit => write_unit_input(parser, shape, dst, value),
            ScalarType::Bool => write_bool_input(parser, shape, dst, value),
            ScalarType::Char => write_char_input(parser, shape, dst, value),
            ScalarType::String => write_string_input(parser, shape, dst, value),
            ScalarType::CowStr => write_cow_str_input(parser, shape, dst, value),
            ScalarType::Str => write_borrowed_str_input(parser, dst, value),
            ScalarType::F32 => write_f32_input(parser, shape, dst, value),
            ScalarType::F64 => write_f64_input(parser, shape, dst, value),
            ScalarType::U8 => write_u8_input(parser, dst, value),
            ScalarType::U16 => write_u16_input(parser, dst, value),
            ScalarType::U32 => write_u32_input(parser, dst, value),
            ScalarType::U64 => write_u64_input(parser, dst, value),
            ScalarType::U128 => write_u128_input(parser, dst, value),
            ScalarType::USize => write_usize_input(parser, dst, value),
            ScalarType::I8 => write_i8_input(parser, dst, value),
            ScalarType::I16 => write_i16_input(parser, dst, value),
            ScalarType::I32 => write_i32_input(parser, dst, value),
            ScalarType::I64 => write_i64_input(parser, dst, value),
            ScalarType::I128 => write_i128_input(parser, dst, value),
            ScalarType::ISize => write_isize_input(parser, dst, value),
            _ => unsafe { write_scalar_input(parser, shape, self.scalar, dst, value) },
        }
    }
}

trait ScalarInputPreselected<'de, const TRUSTED_UTF8: bool> {
    fn write_scalar_input_preselected(
        &self,
        field: &ScalarFieldPlan,
        dst: PtrUninit,
        value: JsonScalarInput<'de>,
    ) -> Result<(), DeserializeError>;
}

impl<'de> ScalarInputPreselected<'de, true> for JsonParser<'de, true> {
    #[inline(always)]
    fn write_scalar_input_preselected(
        &self,
        field: &ScalarFieldPlan,
        dst: PtrUninit,
        value: JsonScalarInput<'de>,
    ) -> Result<(), DeserializeError> {
        if let Some(span) = scalar_input_null_span(&value) {
            return unsafe { write_default_from_null(field.shape, dst, span) };
        }

        if let Some(write) = field.input_trusted {
            return write(self, field.shape, dst, value);
        }

        unsafe { field.scalar.write_input(self, field.shape, dst, value) }
    }
}

impl<'de> ScalarInputPreselected<'de, false> for JsonParser<'de, false> {
    #[inline(always)]
    fn write_scalar_input_preselected(
        &self,
        field: &ScalarFieldPlan,
        dst: PtrUninit,
        value: JsonScalarInput<'de>,
    ) -> Result<(), DeserializeError> {
        if let Some(span) = scalar_input_null_span(&value) {
            return unsafe { write_default_from_null(field.shape, dst, span) };
        }

        if let Some(write) = field.input_checked {
            return write(self, field.shape, dst, value);
        }

        unsafe { field.scalar.write_input(self, field.shape, dst, value) }
    }
}

#[derive(Clone, Copy, Debug)]
struct ListOptionScalar {
    option: OptionDef,
    scalar: ScalarType,
    inner_layout: Layout,
}

const RAW_FIELD_DISPATCH_BUCKETS: usize = 64;

#[derive(Clone, Debug)]
struct RawFieldDispatch {
    buckets: Box<[u64; RAW_FIELD_DISPATCH_BUCKETS]>,
}

impl RawFieldDispatch {
    fn for_fields<Field: StructFieldPlan>(fields: &[Field]) -> Option<Self> {
        if fields.len() <= TINY_SCALAR_STRUCT_MAX_FIELDS || fields.len() > u64::BITS as usize {
            return None;
        }

        let mut buckets = Box::new([0; RAW_FIELD_DISPATCH_BUCKETS]);
        for (index, field) in fields.iter().enumerate() {
            Self::insert_key(&mut buckets, field.name().as_bytes(), index);
            if let Some(alias) = field.alias() {
                Self::insert_key(&mut buckets, alias.as_bytes(), index);
            }
        }
        Some(Self { buckets })
    }

    #[inline]
    fn insert_key(buckets: &mut [u64; RAW_FIELD_DISPATCH_BUCKETS], key: &[u8], index: usize) {
        buckets[raw_field_bucket(key)] |= struct_seen_bit(index);
    }

    #[inline(always)]
    fn candidates(&self, key: &[u8]) -> u64 {
        self.buckets[raw_field_bucket(key)]
    }
}

#[inline(always)]
fn raw_field_bucket(key: &[u8]) -> usize {
    (raw_field_hash(key) as usize) & (RAW_FIELD_DISPATCH_BUCKETS - 1)
}

#[inline(always)]
fn raw_field_hash(key: &[u8]) -> u64 {
    let mut hash = (key.len() as u64).wrapping_mul(0x9E37_79B1_85EB_CA87);
    for &byte in key {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x1000_0000_01B3);
    }
    hash
}

trait StructFieldPlan {
    fn name(&self) -> &'static str;
    fn alias(&self) -> Option<&'static str>;
    fn offset(&self) -> usize;
    fn shape(&self) -> &'static Shape;
    fn missing(&self) -> MissingField;

    #[inline]
    fn matches_key_bytes(&self, key: &[u8]) -> bool {
        self.name().as_bytes() == key || self.alias().is_some_and(|alias| alias.as_bytes() == key)
    }

    #[inline]
    fn matches_key_input<'de, const TRUSTED_UTF8: bool>(
        &self,
        parser: &JsonParser<'de, TRUSTED_UTF8>,
        key: &JsonFieldKeyInput<'de>,
    ) -> Result<bool, ParseError> {
        if parser.field_key_matches(key, self.name())? {
            return Ok(true);
        }

        match self.alias() {
            Some(alias) => parser.field_key_matches(key, alias),
            None => Ok(false),
        }
    }
}

impl<Block> StructFieldPlan for FieldPlan<Block> {
    #[inline(always)]
    fn name(&self) -> &'static str {
        self.name
    }

    #[inline(always)]
    fn alias(&self) -> Option<&'static str> {
        self.alias
    }

    #[inline(always)]
    fn offset(&self) -> usize {
        self.offset
    }

    #[inline(always)]
    fn shape(&self) -> &'static Shape {
        self.shape
    }

    #[inline(always)]
    fn missing(&self) -> MissingField {
        self.missing
    }
}

impl StructFieldPlan for ScalarFieldPlan {
    #[inline(always)]
    fn name(&self) -> &'static str {
        self.name
    }

    #[inline(always)]
    fn alias(&self) -> Option<&'static str> {
        self.alias
    }

    #[inline(always)]
    fn offset(&self) -> usize {
        self.offset
    }

    #[inline(always)]
    fn shape(&self) -> &'static Shape {
        self.shape
    }

    #[inline(always)]
    fn missing(&self) -> MissingField {
        self.missing
    }
}

#[derive(Clone, Copy, Debug)]
enum MissingField {
    Required,
    DefaultTrait { explicit: bool },
    DefaultCustom(DefaultInPlaceFn),
    OptionNone(OptionDef),
}

struct Lowering {
    lowered: Lowered<BlockId, SymbolicOp>,
    in_progress: Vec<&'static Shape>,
}

impl Lowering {
    fn new() -> Self {
        Self {
            lowered: Lowered {
                program: Vec::new(),
                blocks: BTreeMap::new(),
            },
            in_progress: Vec::new(),
        }
    }

    fn lower(
        mut self,
        root: &'static Shape,
    ) -> Result<Lowered<BlockId, SymbolicOp>, DeserializeError> {
        let root_id = JsonBlockId::Shape(root);
        self.lower_shape(root)?;
        self.lowered.program = self
            .lowered
            .blocks
            .get(&root_id)
            .expect("root shape was lowered into a block")
            .clone();
        Ok(self.lowered)
    }

    fn lower_shape(
        &mut self,
        shape: &'static Shape,
    ) -> Result<Program<SymbolicOp>, DeserializeError> {
        let block_id = JsonBlockId::Shape(shape);
        if self.lowered.blocks.contains_key(&block_id) || self.in_progress.contains(&shape) {
            return Ok(vec![JsonOp::CallBlock(block_id)]);
        }

        self.in_progress.push(shape);
        let program = self.lower_shape_body(shape)?;
        self.in_progress.pop();
        self.lowered.blocks.insert(block_id, program);
        Ok(vec![JsonOp::CallBlock(block_id)])
    }

    fn lower_shape_body(
        &mut self,
        shape: &'static Shape,
    ) -> Result<Program<SymbolicOp>, DeserializeError> {
        if let Some(scalar) = ScalarType::try_from_shape(shape) {
            return Ok(vec![JsonOp::ReadScalar {
                shape,
                scalar: ScalarPlan::new(scalar),
            }]);
        }

        match shape.def {
            Def::Option(option) => {
                let inner_layout = sized_layout(option.t())?;
                let some_program = self.lower_shape(option.t())?;
                let some_scalar = ScalarType::try_from_shape(option.t()).map(ScalarPlan::new);
                Ok(vec![JsonOp::ReadOption {
                    option,
                    some_program,
                    some_scalar,
                    inner_layout,
                }])
            }
            Def::List(list) => {
                if list.from_raw_parts().is_none()
                    && (list.init_in_place_with_capacity().is_none() || list.push().is_none())
                {
                    return Err(unsupported(
                        shape,
                        "list from_raw_parts or initialization and push",
                    ));
                }
                let element_layout = sized_layout(list.t())?;
                let element_program = self.lower_shape(list.t())?;
                let element_scalar = ScalarType::try_from_shape(list.t());
                let element_option_scalar = list_option_scalar(list.t())?;
                let loop_id = JsonBlockId::ListLoop(shape);
                let loop_program = vec![JsonOp::ListNext {
                    list,
                    element_program: element_program.clone(),
                    element_scalar,
                    element_option_scalar,
                    element_layout,
                    loop_id,
                }];
                self.lowered.blocks.insert(loop_id, loop_program);
                Ok(vec![JsonOp::ReadList {
                    list_shape: shape,
                    list,
                    element_layout,
                    loop_id,
                }])
            }
            Def::Set(set) => {
                let element_layout = sized_layout(set.t())?;
                let element_program = self.lower_shape(set.t())?;
                let element_scalar = ScalarType::try_from_shape(set.t());
                let element_option_scalar = list_option_scalar(set.t())?;
                let loop_id = JsonBlockId::SetLoop(shape);
                let loop_program = vec![JsonOp::SetNext {
                    set,
                    element_program: element_program.clone(),
                    element_scalar,
                    element_option_scalar,
                    element_layout,
                    loop_id,
                }];
                self.lowered.blocks.insert(loop_id, loop_program);
                Ok(vec![JsonOp::ReadSet {
                    set_shape: shape,
                    set,
                    loop_id,
                }])
            }
            Def::Map(map) => {
                let Some(key_scalar) = ScalarType::try_from_shape(map.k()) else {
                    return Err(unsupported(shape, "scalar map key"));
                };
                if !map_key_scalar_supported(key_scalar) {
                    return Err(unsupported(shape, "string or integer map key"));
                }

                let key_layout = sized_layout(map.k())?;
                let value_layout = sized_layout(map.v())?;
                let value_program = self.lower_shape(map.v())?;
                let value_scalar = ScalarType::try_from_shape(map.v()).map(ScalarPlan::new);
                let loop_id = JsonBlockId::MapLoop(shape);
                let loop_program = vec![JsonOp::MapNext {
                    map,
                    key_scalar,
                    key_layout,
                    value_program: value_program.clone(),
                    value_scalar,
                    value_layout,
                    loop_id,
                }];
                self.lowered.blocks.insert(loop_id, loop_program);
                Ok(vec![JsonOp::ReadMap {
                    map_shape: shape,
                    map,
                    loop_id,
                }])
            }
            Def::Pointer(pointer) => {
                let pointee = pointer
                    .pointee()
                    .ok_or_else(|| unsupported(shape, "opaque pointer"))?;
                if pointer.vtable.new_into_fn.is_none() {
                    return Err(unsupported(shape, "pointer without new_into"));
                }
                let pointee_layout = sized_layout(pointee)?;
                let pointee_program = self.lower_shape(pointee)?;
                Ok(vec![JsonOp::ReadPointer {
                    pointer,
                    pointee_program,
                    pointee_layout,
                }])
            }
            _ => match shape.ty {
                Type::User(UserType::Enum(enum_type)) => self.lower_external_enum(shape, enum_type),
                Type::User(UserType::Struct(struct_type)) => {
                    if struct_type.kind != StructKind::Struct {
                        return Err(unsupported(shape, "non-named struct"));
                    }
                    if shape.proxy.is_some() || !shape.format_proxies.is_empty() {
                        return Err(unsupported(shape, "proxy"));
                    }

                    let container_has_default = shape.has_default_attr();
                    let all_scalar = struct_type
                        .fields
                        .iter()
                        .all(|field| ScalarType::try_from_shape(field.shape()).is_some());

                    if all_scalar {
                        let mut fields = Vec::with_capacity(struct_type.fields.len());
                        for field in struct_type.fields {
                            if field.should_skip_deserializing() || field.is_flattened() {
                                return Err(unsupported(shape, "skipped or flattened fields"));
                            }
                            let field_shape = field.shape();
                            let scalar = ScalarType::try_from_shape(field_shape)
                                .expect("scalar-only struct field has scalar type");
                            fields.push(ScalarFieldPlan {
                                name: field.effective_name(),
                                alias: field.alias,
                                offset: field.offset,
                                shape: field_shape,
                                scalar: ScalarPlan::new(scalar),
                                input_trusted: scalar_input_writer::<true>(scalar),
                                input_checked: scalar_input_writer::<false>(scalar),
                                missing: missing_field_action(field, container_has_default),
                            });
                        }
                        let dispatch = RawFieldDispatch::for_fields(&fields);
                        return Ok(vec![JsonOp::ReadScalarStruct {
                            shape,
                            fields: fields.into_boxed_slice(),
                            dispatch,
                        }]);
                    }

                    let mut fields = Vec::with_capacity(struct_type.fields.len());
                    for field in struct_type.fields {
                        if field.should_skip_deserializing() || field.is_flattened() {
                            return Err(unsupported(shape, "skipped or flattened fields"));
                        }
                        let field_shape = field.shape();
                        let program = self.lower_shape(field_shape)?;
                        fields.push(FieldPlan {
                            name: field.effective_name(),
                            alias: field.alias,
                            offset: field.offset,
                            shape: field_shape,
                            program,
                            scalar: ScalarType::try_from_shape(field_shape).map(ScalarPlan::new),
                            missing: missing_field_action(field, container_has_default),
                        });
                    }
                    let fields = fields.into_boxed_slice();
                    let dispatch = RawFieldDispatch::for_fields(&fields);
                    let loop_id = JsonBlockId::StructLoop(shape);
                    let tracking = StructTracking::for_len(fields.len());
                    let loop_program = vec![JsonOp::StructNext {
                        shape,
                        loop_id,
                        raw_field_dispatch: true,
                        tracking,
                    }];
                    self.lowered.blocks.insert(loop_id, loop_program);
                    Ok(vec![JsonOp::ReadStruct {
                        shape,
                        fields,
                        dispatch,
                        loop_id,
                    }])
                }
                _ => Err(unsupported(shape, "shape")),
            },
        }
    }

    fn lower_external_enum(
        &mut self,
        shape: &'static Shape,
        enum_type: EnumType,
    ) -> Result<Program<SymbolicOp>, DeserializeError> {
        if shape.is_numeric() {
            return Err(unsupported(shape, "numeric enum"));
        }
        if shape.is_untagged() {
            return Err(unsupported(shape, "untagged enum"));
        }
        let tag_key = shape.get_tag_attr();
        let content_key = shape.get_content_attr();
        if tag_key.is_none() && content_key.is_some() {
            return Err(unsupported(shape, "enum content key without tag key"));
        }
        if enum_type.is_cow {
            return Err(unsupported(shape, "cow enum"));
        }

        let mut variants = Vec::with_capacity(enum_type.variants.len());
        for (index, variant) in enum_type.variants.iter().enumerate() {
            let mut fields = Vec::with_capacity(variant.data.fields.len());
            for field in variant.data.fields {
                if variant.is_other() && (field.is_variant_tag() || field.is_variant_content()) {
                    return Err(unsupported(shape, "other enum variant tag/content fields"));
                }
                if field.should_skip_deserializing() || field.is_flattened() {
                    return Err(unsupported(
                        shape,
                        "skipped or flattened enum variant fields",
                    ));
                }
                let field_shape = field.shape();
                fields.push(FieldPlan {
                    name: field.effective_name(),
                    alias: field.alias,
                    offset: field.offset,
                    shape: field_shape,
                    program: self.lower_shape(field_shape)?,
                    scalar: ScalarType::try_from_shape(field_shape).map(ScalarPlan::new),
                    missing: missing_field_action(field, false),
                });
            }

            let fields = fields.into_boxed_slice();
            let tracking = StructTracking::for_len(fields.len());
            let dispatch = RawFieldDispatch::for_fields(&fields);
            if tag_key.is_some()
                && content_key.is_none()
                && !matches!(variant.data.kind, StructKind::Unit | StructKind::Struct)
            {
                return Err(unsupported(shape, "internally tagged tuple enum variant"));
            }
            let loop_id = if variant.data.kind == StructKind::Struct {
                let loop_id = JsonBlockId::VariantStructLoop(shape, index);
                let loop_program = vec![JsonOp::StructNext {
                    shape,
                    loop_id,
                    raw_field_dispatch: true,
                    tracking,
                }];
                self.lowered.blocks.insert(loop_id, loop_program);
                Some(loop_id)
            } else {
                None
            };

            variants.push(ExternalVariantPlan {
                index,
                variant,
                fields,
                dispatch,
                loop_id,
                tracking,
            });
        }

        let variants = variants.into_boxed_slice();
        match tag_key {
            Some(tag_key) => Ok(vec![JsonOp::ReadTaggedEnum {
                shape,
                enum_type,
                tag_key,
                content_key,
                variants,
            }]),
            None => Ok(vec![JsonOp::ReadExternalEnum {
                shape,
                enum_type,
                variants,
            }]),
        }
    }
}

fn resolve_json_lowered(
    symbolic: Lowered<BlockId, SymbolicOp>,
) -> Result<DenseLowered<ExecOp>, DeserializeError> {
    let refs = symbolic.block_refs();
    let program = resolve_json_program(symbolic.program, &refs)?;
    let mut blocks = Vec::with_capacity(symbolic.blocks.len());
    for (_, block) in symbolic.blocks {
        blocks.push(resolve_json_program(block, &refs)?);
    }
    Ok(DenseLowered::new(program, blocks))
}

fn resolve_json_program(
    program: Program<SymbolicOp>,
    refs: &BTreeMap<BlockId, ExecBlock>,
) -> Result<Program<ExecOp>, DeserializeError> {
    program
        .into_iter()
        .map(|op| resolve_json_op(op, refs))
        .collect()
}

fn resolve_json_op(
    op: SymbolicOp,
    refs: &BTreeMap<BlockId, ExecBlock>,
) -> Result<ExecOp, DeserializeError> {
    Ok(match op {
        JsonOp::CallBlock(block) => JsonOp::CallBlock(resolve_block_ref(block, refs)?),
        JsonOp::ReadScalar { shape, scalar } => JsonOp::ReadScalar { shape, scalar },
        JsonOp::ReadScalarStruct {
            shape,
            fields,
            dispatch,
        } => JsonOp::ReadScalarStruct {
            shape,
            fields,
            dispatch,
        },
        JsonOp::ReadStruct {
            shape,
            fields,
            dispatch,
            loop_id,
        } => JsonOp::ReadStruct {
            shape,
            fields: resolve_field_plans(fields, refs)?,
            dispatch,
            loop_id: resolve_block_ref(loop_id, refs)?,
        },
        JsonOp::ReadExternalEnum {
            shape,
            enum_type,
            variants,
        } => JsonOp::ReadExternalEnum {
            shape,
            enum_type,
            variants: resolve_external_variant_plans(variants, refs)?,
        },
        JsonOp::ReadTaggedEnum {
            shape,
            enum_type,
            tag_key,
            content_key,
            variants,
        } => JsonOp::ReadTaggedEnum {
            shape,
            enum_type,
            tag_key,
            content_key,
            variants: resolve_external_variant_plans(variants, refs)?,
        },
        JsonOp::StructNext {
            shape,
            loop_id,
            raw_field_dispatch,
            tracking,
        } => JsonOp::StructNext {
            shape,
            loop_id: resolve_block_ref(loop_id, refs)?,
            raw_field_dispatch,
            tracking,
        },
        JsonOp::ReadOption {
            option,
            some_program,
            some_scalar,
            inner_layout,
        } => JsonOp::ReadOption {
            option,
            some_program: resolve_json_program(some_program, refs)?,
            some_scalar,
            inner_layout,
        },
        JsonOp::ReadList {
            list_shape,
            list,
            element_layout,
            loop_id,
        } => JsonOp::ReadList {
            list_shape,
            list,
            element_layout,
            loop_id: resolve_block_ref(loop_id, refs)?,
        },
        JsonOp::ListNext {
            list,
            element_program,
            element_scalar,
            element_option_scalar,
            element_layout,
            loop_id,
        } => JsonOp::ListNext {
            list,
            element_program: resolve_json_program(element_program, refs)?,
            element_scalar,
            element_option_scalar,
            element_layout,
            loop_id: resolve_block_ref(loop_id, refs)?,
        },
        JsonOp::ReadSet {
            set_shape,
            set,
            loop_id,
        } => JsonOp::ReadSet {
            set_shape,
            set,
            loop_id: resolve_block_ref(loop_id, refs)?,
        },
        JsonOp::SetNext {
            set,
            element_program,
            element_scalar,
            element_option_scalar,
            element_layout,
            loop_id,
        } => JsonOp::SetNext {
            set,
            element_program: resolve_json_program(element_program, refs)?,
            element_scalar,
            element_option_scalar,
            element_layout,
            loop_id: resolve_block_ref(loop_id, refs)?,
        },
        JsonOp::ReadMap {
            map_shape,
            map,
            loop_id,
        } => JsonOp::ReadMap {
            map_shape,
            map,
            loop_id: resolve_block_ref(loop_id, refs)?,
        },
        JsonOp::MapNext {
            map,
            key_scalar,
            key_layout,
            value_program,
            value_scalar,
            value_layout,
            loop_id,
        } => JsonOp::MapNext {
            map,
            key_scalar,
            key_layout,
            value_program: resolve_json_program(value_program, refs)?,
            value_scalar,
            value_layout,
            loop_id: resolve_block_ref(loop_id, refs)?,
        },
        JsonOp::ReadPointer {
            pointer,
            pointee_program,
            pointee_layout,
        } => JsonOp::ReadPointer {
            pointer,
            pointee_program: resolve_json_program(pointee_program, refs)?,
            pointee_layout,
        },
    })
}

fn resolve_external_variant_plans(
    variants: Box<[ExternalVariantPlan<BlockId>]>,
    refs: &BTreeMap<BlockId, ExecBlock>,
) -> Result<Box<[ExternalVariantPlan<ExecBlock>]>, DeserializeError> {
    variants
        .into_vec()
        .into_iter()
        .map(|variant| {
            Ok(ExternalVariantPlan {
                index: variant.index,
                variant: variant.variant,
                fields: resolve_field_plans(variant.fields, refs)?,
                dispatch: variant.dispatch,
                loop_id: variant
                    .loop_id
                    .map(|block| resolve_block_ref(block, refs))
                    .transpose()?,
                tracking: variant.tracking,
            })
        })
        .collect()
}

fn resolve_field_plans(
    fields: Box<[FieldPlan<BlockId>]>,
    refs: &BTreeMap<BlockId, ExecBlock>,
) -> Result<Box<[FieldPlan<ExecBlock>]>, DeserializeError> {
    fields
        .into_vec()
        .into_iter()
        .map(|field| {
            Ok(FieldPlan {
                name: field.name,
                alias: field.alias,
                offset: field.offset,
                shape: field.shape,
                program: resolve_json_program(field.program, refs)?,
                scalar: field.scalar,
                missing: field.missing,
            })
        })
        .collect()
}

fn resolve_block_ref(
    block: BlockId,
    refs: &BTreeMap<BlockId, ExecBlock>,
) -> Result<ExecBlock, DeserializeError> {
    refs.get(&block).copied().ok_or_else(|| {
        vm_error(
            None,
            DeserializeErrorKind::Unsupported {
                message: format!("missing Weavy block {block:?}").into(),
            },
        )
    })
}

fn missing_field_action(field: &Field, container_has_default: bool) -> MissingField {
    let field_shape = field.shape();
    match field.default {
        Some(DefaultSource::Custom(default)) => MissingField::DefaultCustom(default),
        Some(DefaultSource::FromTrait) => MissingField::DefaultTrait { explicit: true },
        Some(_) => MissingField::DefaultTrait { explicit: true },
        None => match field_shape.def {
            Def::Option(option) => MissingField::OptionNone(option),
            _ if container_has_default && field_shape.is(Characteristic::Default) => {
                MissingField::DefaultTrait { explicit: false }
            }
            _ if field_shape.is_type::<()>() => MissingField::DefaultTrait { explicit: false },
            _ => MissingField::Required,
        },
    }
}

fn list_option_scalar(shape: &'static Shape) -> Result<Option<ListOptionScalar>, DeserializeError> {
    let Def::Option(option) = shape.def else {
        return Ok(None);
    };
    let Some(scalar) = ScalarType::try_from_shape(option.t()) else {
        return Ok(None);
    };
    Ok(Some(ListOptionScalar {
        option,
        scalar,
        inner_layout: sized_layout(option.t())?,
    }))
}

fn map_key_scalar_supported(scalar: ScalarType) -> bool {
    matches!(
        scalar,
        ScalarType::String
            | ScalarType::CowStr
            | ScalarType::I8
            | ScalarType::I16
            | ScalarType::I32
            | ScalarType::I64
            | ScalarType::I128
            | ScalarType::ISize
            | ScalarType::U8
            | ScalarType::U16
            | ScalarType::U32
            | ScalarType::U64
            | ScalarType::U128
            | ScalarType::USize
    )
}

fn find_external_variant<S: AsRef<str>>(
    variants: &[ExternalVariantPlan<ExecBlock>],
    name: S,
) -> Option<&ExternalVariantPlan<ExecBlock>> {
    let name = name.as_ref();
    variants
        .iter()
        .find(|variant| !variant.variant.is_other() && variant.variant.effective_name() == name)
}

fn find_external_variant_input<'program, 'de, const TRUSTED_UTF8: bool>(
    parser: &JsonParser<'de, TRUSTED_UTF8>,
    variants: &'program [ExternalVariantPlan<ExecBlock>],
    key: &JsonFieldKeyInput<'de>,
) -> Result<Option<&'program ExternalVariantPlan<ExecBlock>>, ParseError> {
    if let Some(key) = parser.field_key_unescaped_bytes(key) {
        return Ok(variants.iter().find(|variant| {
            !variant.variant.is_other() && variant.variant.effective_name().as_bytes() == key
        }));
    }

    variants
        .iter()
        .find_map(
            |variant| match parser.field_key_matches(key, variant.variant.effective_name()) {
                Ok(true) if !variant.variant.is_other() => Some(Ok(variant)),
                Ok(false) => None,
                Ok(true) => None,
                Err(err) => Some(Err(err)),
            },
        )
        .transpose()
}

fn external_other_variant(
    variants: &[ExternalVariantPlan<ExecBlock>],
) -> Option<&ExternalVariantPlan<ExecBlock>> {
    variants.iter().find(|variant| variant.variant.is_other())
}

unsafe fn write_enum_discriminant(
    shape: &'static Shape,
    enum_type: EnumType,
    variant: &'static Variant,
    dst: PtrUninit,
) -> Result<(), DeserializeError> {
    let Some(discriminant) = variant.discriminant else {
        return Err(unsupported(shape, "enum variant without discriminant"));
    };

    unsafe {
        match enum_type.enum_repr {
            EnumRepr::Rust => return Err(unsupported(shape, "Rust enum repr")),
            EnumRepr::RustNPO => return Err(unsupported(shape, "RustNPO enum repr")),
            EnumRepr::U8 => {
                let ptr = dst.as_mut_byte_ptr();
                *ptr = discriminant as u8;
            }
            EnumRepr::U16 => {
                let ptr = dst.as_mut_byte_ptr() as *mut u16;
                *ptr = discriminant as u16;
            }
            EnumRepr::U32 => {
                let ptr = dst.as_mut_byte_ptr() as *mut u32;
                *ptr = discriminant as u32;
            }
            EnumRepr::U64 => {
                let ptr = dst.as_mut_byte_ptr() as *mut u64;
                *ptr = discriminant as u64;
            }
            EnumRepr::USize => {
                let ptr = dst.as_mut_byte_ptr() as *mut usize;
                *ptr = discriminant as usize;
            }
            EnumRepr::I8 => {
                let ptr = dst.as_mut_byte_ptr() as *mut i8;
                *ptr = discriminant as i8;
            }
            EnumRepr::I16 => {
                let ptr = dst.as_mut_byte_ptr() as *mut i16;
                *ptr = discriminant as i16;
            }
            EnumRepr::I32 => {
                let ptr = dst.as_mut_byte_ptr() as *mut i32;
                *ptr = discriminant as i32;
            }
            EnumRepr::I64 => {
                let ptr = dst.as_mut_byte_ptr() as *mut i64;
                *ptr = discriminant;
            }
            EnumRepr::ISize => {
                let ptr = dst.as_mut_byte_ptr() as *mut isize;
                *ptr = discriminant as isize;
            }
        }
    }

    Ok(())
}

fn sized_layout(shape: &'static Shape) -> Result<Layout, DeserializeError> {
    shape
        .layout
        .sized_layout()
        .map_err(|_| unsupported(shape, "unsized shape"))
}

fn unsupported(shape: &'static Shape, what: &'static str) -> DeserializeError {
    vm_error(
        None,
        DeserializeErrorKind::Unsupported {
            message: format!("Weavy JSON deserializer does not yet support {what} for {shape}")
                .into(),
        },
    )
}

fn raw_alloc_error(error: RawAllocError) -> DeserializeError {
    match error {
        RawAllocError::InvalidLayout { .. } | RawAllocError::SizeOverflow { .. } => vm_error(
            None,
            DeserializeErrorKind::InvalidValue {
                message: "raw buffer layout overflow".into(),
            },
        ),
    }
}

struct JsonInterp<'parser, 'de, 'program, const TRUSTED_UTF8: bool> {
    parser: &'parser mut JsonParser<'de, TRUSTED_UTF8>,
    blocks: &'program [Program<ExecOp>],
    base: PtrUninit,
    inline_structs:
        InlineStack<StructFrame<'program, FieldPlan<ExecBlock>, InitializedLedger<Span>>>,
    large_structs: Option<Box<LargeStructStack<'program>>>,
    lists: InlineStack<ListFrame>,
    sets: InlineStack<SetFrame>,
    maps: InlineStack<MapFrame>,
    scratch: ScratchSession,
    success: bool,
}

impl<'parser, 'de, 'program, const TRUSTED_UTF8: bool>
    JsonInterp<'parser, 'de, 'program, TRUSTED_UTF8>
where
    JsonParser<'de, TRUSTED_UTF8>: ScalarInputPreselected<'de, TRUSTED_UTF8>,
{
    fn new(
        parser: &'parser mut JsonParser<'de, TRUSTED_UTF8>,
        base: PtrUninit,
        blocks: &'program [Program<ExecOp>],
    ) -> Self {
        Self {
            parser,
            blocks,
            base,
            inline_structs: InlineStack::new(),
            large_structs: None,
            lists: InlineStack::new(),
            sets: InlineStack::new(),
            maps: InlineStack::new(),
            scratch: ScratchSession::new(),
            success: false,
        }
    }

    fn finish_success(&mut self) {
        self.success = true;
    }

    fn large_structs(&self) -> &LargeStructStack<'program> {
        self.large_structs
            .as_deref()
            .expect("large struct stack is present")
    }

    fn large_structs_mut(&mut self) -> &mut LargeStructStack<'program> {
        self.large_structs
            .get_or_insert_with(|| Box::new(LargeStructStack::new()))
    }

    fn step_struct_next_inline(
        &mut self,
        shape: &'static Shape,
        loop_id: ExecBlock,
        raw_field_dispatch: bool,
    ) -> Result<Control<'program, ExecBlock, ExecOp, Continuation<'program>>, DeserializeError>
    {
        loop {
            match self.parser.next_object_key_or_end()? {
                JsonObjectKeyStep::End => return Ok(Control::Continue),
                JsonObjectKeyStep::Field { mut key, span } => {
                    let key_is_raw =
                        raw_field_dispatch && matches!(key, JsonFieldKeyInput::Raw { .. });
                    if !raw_field_dispatch {
                        key = JsonFieldKeyInput::Materialized(
                            self.parser.materialize_field_key(key)?,
                        );
                    }
                    let matched = {
                        let frame = self
                            .inline_structs
                            .last()
                            .expect("inline struct frame is present while matching fields");
                        frame.match_field_input(&*self.parser, &key)?
                    };

                    let Some(matched) = matched else {
                        let key = self.parser.materialize_field_key(key)?;
                        if shape.has_deny_unknown_fields_attr() {
                            return Err(vm_error(
                                Some(span),
                                DeserializeErrorKind::UnknownField {
                                    field: key.as_str().to_owned().into(),
                                    suggestion: None,
                                },
                            ));
                        }
                        self.parser.skip_value_strict()?;
                        continue;
                    };
                    let MatchedField {
                        index,
                        field,
                        ordered,
                    } = matched;
                    let frame = self
                        .inline_structs
                        .last()
                        .expect("inline struct frame is present while decoding fields");

                    if !ordered && let Some(first_span) = StructSeenStore::get(&frame.seen, index) {
                        return Err(vm_error(
                            Some(span),
                            DeserializeErrorKind::DuplicateField {
                                field: field.name.into(),
                                first_span: Some(first_span),
                            },
                        ));
                    }

                    if let Some(scalar) = field.scalar {
                        let field_ptr = unsafe { frame.base.field_uninit(field.offset) };
                        if key_is_raw {
                            let value = self.parser.read_current_scalar_input()?;
                            unsafe {
                                scalar.write_input(&*self.parser, field.shape, field_ptr, value)?;
                            }
                        } else {
                            let (value, value_span) = self.parser.read_scalar_token()?;
                            unsafe {
                                scalar.write(field.shape, field_ptr, value, value_span)?;
                            }
                        }
                        let frame = self
                            .inline_structs
                            .last_mut()
                            .expect("inline struct frame is present while decoding scalar field");
                        frame.mark_seen(index, span);
                        continue;
                    }

                    let old_base = self.base;
                    self.base = unsafe { frame.base.field_uninit(field.offset) };
                    return Ok(call_program_or_block_then(
                        &field.program,
                        Continuation::FieldDone {
                            tracking: StructTracking::Inline,
                            index,
                            span,
                            old_base,
                            loop_id,
                        },
                    ));
                }
            }
        }
    }

    fn step_struct_next_large(
        &mut self,
        shape: &'static Shape,
        loop_id: ExecBlock,
        raw_field_dispatch: bool,
        tracking: StructTracking,
    ) -> Result<Control<'program, ExecBlock, ExecOp, Continuation<'program>>, DeserializeError>
    {
        loop {
            match self.parser.next_object_key_or_end()? {
                JsonObjectKeyStep::End => return Ok(Control::Continue),
                JsonObjectKeyStep::Field { mut key, span } => {
                    let key_is_raw =
                        raw_field_dispatch && matches!(key, JsonFieldKeyInput::Raw { .. });
                    if !raw_field_dispatch {
                        key = JsonFieldKeyInput::Materialized(
                            self.parser.materialize_field_key(key)?,
                        );
                    }
                    let matched = {
                        let frame = self
                            .large_structs()
                            .last()
                            .expect("large struct frame is present while matching fields");
                        frame.match_field_input(&*self.parser, &key)?
                    };

                    let Some(matched) = matched else {
                        let key = self.parser.materialize_field_key(key)?;
                        if shape.has_deny_unknown_fields_attr() {
                            return Err(vm_error(
                                Some(span),
                                DeserializeErrorKind::UnknownField {
                                    field: key.as_str().to_owned().into(),
                                    suggestion: None,
                                },
                            ));
                        }
                        self.parser.skip_value_strict()?;
                        continue;
                    };
                    let MatchedField {
                        index,
                        field,
                        ordered,
                    } = matched;
                    let frame = self
                        .large_structs()
                        .last()
                        .expect("large struct frame is present while decoding fields");

                    if !ordered && let Some(first_span) = frame.seen_span(index) {
                        return Err(vm_error(
                            Some(span),
                            DeserializeErrorKind::DuplicateField {
                                field: field.name.into(),
                                first_span: Some(first_span),
                            },
                        ));
                    }

                    if let Some(scalar) = field.scalar {
                        let field_ptr = unsafe { frame.field_uninit(field.offset) };
                        if key_is_raw {
                            let value = self.parser.read_current_scalar_input()?;
                            unsafe {
                                scalar.write_input(&*self.parser, field.shape, field_ptr, value)?;
                            }
                        } else {
                            let (value, value_span) = self.parser.read_scalar_token()?;
                            unsafe {
                                scalar.write(field.shape, field_ptr, value, value_span)?;
                            }
                        }
                        let frame = self
                            .large_structs_mut()
                            .last_mut()
                            .expect("large struct frame is present while decoding scalar field");
                        frame.mark(index, span);
                        continue;
                    }

                    let old_base = self.base;
                    self.base = unsafe { frame.field_uninit(field.offset) };
                    return Ok(call_program_or_block_then(
                        &field.program,
                        Continuation::FieldDone {
                            tracking,
                            index,
                            span,
                            old_base,
                            loop_id,
                        },
                    ));
                }
            }
        }
    }

    fn push_struct_frame(
        &mut self,
        shape: &'static Shape,
        fields: &'program [FieldPlan<ExecBlock>],
        dispatch: Option<&'program RawFieldDispatch>,
        tracking: StructTracking,
    ) {
        let base = self.base;
        match tracking {
            StructTracking::Inline => {
                self.inline_structs
                    .push(StructFrame::new(shape, base, fields, dispatch));
            }
            StructTracking::Bitset | StructTracking::Heap => {
                self.large_structs_mut()
                    .push(LargeStructFrameSlot::new(shape, base, fields, dispatch));
            }
        }
    }

    fn mark_struct_field(&mut self, tracking: StructTracking, index: usize, span: Span) {
        match tracking {
            StructTracking::Inline => {
                let frame = self
                    .inline_structs
                    .last_mut()
                    .expect("inline struct frame is present after field program");
                frame.mark_seen(index, span);
            }
            StructTracking::Bitset | StructTracking::Heap => {
                let frame = self
                    .large_structs_mut()
                    .last_mut()
                    .expect("large struct frame is present after field program");
                frame.mark(index, span);
            }
        }
    }

    unsafe fn finish_struct_frame(
        &mut self,
        tracking: StructTracking,
    ) -> Result<(), DeserializeError> {
        match tracking {
            StructTracking::Inline => {
                let frame = self
                    .inline_structs
                    .pop()
                    .expect("inline struct frame is present after struct program");
                unsafe {
                    frame.fill_missing_fields()?;
                }
            }
            StructTracking::Bitset | StructTracking::Heap => {
                let frame = self
                    .large_structs_mut()
                    .pop()
                    .expect("large struct frame is present after struct program");
                unsafe {
                    frame.fill_missing_fields()?;
                }
            }
        }
        Ok(())
    }

    fn consume_external_enum_object_end(&mut self) -> Result<(), DeserializeError> {
        if self.parser.consume_object_end_if_next()? {
            return Ok(());
        }

        match self.parser.next_object_key_or_end()? {
            JsonObjectKeyStep::End => Ok(()),
            JsonObjectKeyStep::Field { key, span } => {
                let key = self.parser.materialize_field_key(key)?;
                Err(vm_error(
                    Some(span),
                    DeserializeErrorKind::UnexpectedToken {
                        expected: "struct end after enum variant",
                        got: format!("field key `{}`", key.as_str()).into(),
                    },
                ))
            }
        }
    }

    fn consume_unit_variant_payload(&mut self) -> Result<(), DeserializeError> {
        let Some(event) = self.parser.peek_event()? else {
            return Err(vm_error(
                None,
                DeserializeErrorKind::UnexpectedEof {
                    expected: "unit enum variant payload",
                },
            ));
        };

        if matches!(event.kind, ParseEventKind::StructStart(_)) {
            self.parser.consume_object_start_fast()?;
            if self.parser.consume_object_end_if_next()? {
                return Ok(());
            }
            return Err(vm_error(
                Some(event.span),
                DeserializeErrorKind::UnexpectedToken {
                    expected: "empty struct for unit variant",
                    got: "non-empty struct".into(),
                },
            ));
        }

        Err(vm_error(
            Some(event.span),
            DeserializeErrorKind::UnexpectedToken {
                expected: "empty object for unit variant",
                got: event.kind_name().into(),
            },
        ))
    }

    fn collect_tagged_raw_fields(
        &mut self,
        expected: &'static str,
    ) -> Result<Vec<TaggedRawField<'de>>, DeserializeError> {
        let Some(event) = self.parser.peek_event()? else {
            return Err(vm_error(
                None,
                DeserializeErrorKind::UnexpectedEof { expected },
            ));
        };
        if !matches!(event.kind, ParseEventKind::StructStart(_)) {
            return Err(vm_error(
                Some(event.span),
                DeserializeErrorKind::UnexpectedToken {
                    expected,
                    got: event.kind_name().into(),
                },
            ));
        }

        self.parser.consume_object_start_fast()?;
        let mut fields = Vec::new();
        loop {
            match self.parser.next_object_key_or_end()? {
                JsonObjectKeyStep::End => return Ok(fields),
                JsonObjectKeyStep::Field { key, span } => {
                    let key = self.parser.materialize_field_key(key)?;
                    let name = match key {
                        JsonFieldKey::Borrowed(name) => Cow::Borrowed(name),
                        JsonFieldKey::Decoded(name) => name,
                    };
                    let raw = self
                        .parser
                        .capture_raw()?
                        .ok_or_else(|| unsupported_shape_message("raw JSON capture failed"))?;
                    fields.push(TaggedRawField { name, raw, span });
                }
            }
        }
    }

    fn unique_tagged_field<'fields>(
        fields: &'fields [TaggedRawField<'de>],
        name: &'static str,
    ) -> Result<Option<&'fields TaggedRawField<'de>>, DeserializeError> {
        let mut found: Option<&TaggedRawField<'de>> = None;
        for field in fields {
            if field.name.as_ref() != name {
                continue;
            }

            if let Some(first) = found {
                return Err(vm_error(
                    Some(field.span),
                    DeserializeErrorKind::DuplicateField {
                        field: name.into(),
                        first_span: Some(first.span),
                    },
                ));
            }
            found = Some(field);
        }
        Ok(found)
    }

    fn require_tagged_field<'fields>(
        fields: &'fields [TaggedRawField<'de>],
        name: &'static str,
        shape: &'static Shape,
    ) -> Result<&'fields TaggedRawField<'de>, DeserializeError> {
        Self::unique_tagged_field(fields, name)?.ok_or_else(|| {
            vm_error(
                None,
                DeserializeErrorKind::MissingField {
                    field: name,
                    container_shape: shape,
                },
            )
        })
    }

    fn read_raw_tag_name(
        field: &TaggedRawField<'de>,
        tag_key: &'static str,
    ) -> Result<String, DeserializeError> {
        let mut parser = JsonParser::<true>::new(field.raw.as_bytes());
        let (value, _span) = parser.read_scalar_token()?;
        let JsonScalarToken::Str(value) = value else {
            return Err(vm_error(
                Some(field.span),
                DeserializeErrorKind::UnexpectedToken {
                    expected: tag_key,
                    got: value.kind_name().into(),
                },
            ));
        };
        Self::ensure_raw_parser_finished(&mut parser)?;
        Ok(value.into_owned())
    }

    fn ensure_raw_parser_finished(
        parser: &mut JsonParser<'de, true>,
    ) -> Result<(), DeserializeError> {
        if let Some(event) = parser.peek_event()? {
            return Err(vm_error(
                Some(event.span),
                DeserializeErrorKind::UnexpectedToken {
                    expected: "end of raw JSON value",
                    got: event.kind_name().into(),
                },
            ));
        }
        Ok(())
    }

    fn tagged_variant<'variants>(
        variants: &'variants [ExternalVariantPlan<ExecBlock>],
        tag: &str,
        span: Span,
    ) -> Result<&'variants ExternalVariantPlan<ExecBlock>, DeserializeError> {
        find_external_variant(variants, tag)
            .or_else(|| external_other_variant(variants))
            .ok_or_else(|| {
                vm_error(
                    Some(span),
                    DeserializeErrorKind::UnexpectedToken {
                        expected: "known enum variant",
                        got: tag.to_owned().into(),
                    },
                )
            })
    }

    fn struct_seen_span(&self, tracking: StructTracking, index: usize) -> Option<Span> {
        match tracking {
            StructTracking::Inline => {
                let frame = self
                    .inline_structs
                    .last()
                    .expect("inline struct frame is present while checking duplicate field");
                frame.seen.get(index).copied()
            }
            StructTracking::Bitset | StructTracking::Heap => {
                let frame = self
                    .large_structs()
                    .last()
                    .expect("large struct frame is present while checking duplicate field");
                frame.seen_span(index)
            }
        }
    }

    fn captured_variant_field<'variant>(
        variant: &'variant ExternalVariantPlan<ExecBlock>,
        key: &[u8],
    ) -> Option<(usize, &'variant FieldPlan<ExecBlock>)> {
        if let Some(dispatch) = &variant.dispatch {
            let mut candidates = dispatch.candidates(key);
            while candidates != 0 {
                let index = candidates.trailing_zeros() as usize;
                candidates &= candidates - 1;
                let field = &variant.fields[index];
                if field.matches_key_bytes(key) {
                    return Some((index, field));
                }
            }
            return None;
        }

        variant
            .fields
            .iter()
            .enumerate()
            .find(|(_, field)| field.matches_key_bytes(key))
    }

    fn write_raw_field_value(
        &mut self,
        field: &'program FieldPlan<ExecBlock>,
        dst: PtrUninit,
        raw: &'de str,
    ) -> Result<(), DeserializeError> {
        let mut parser = JsonParser::<true>::new(raw.as_bytes());
        if let Some(scalar) = field.scalar {
            let (value, span) = parser.read_scalar_token()?;
            unsafe {
                scalar.write(field.shape, dst, value, span)?;
            }
            Self::ensure_raw_parser_finished(&mut parser)?;
            return Ok(());
        }

        {
            let mut interp: JsonInterp<'_, 'de, 'program, true> =
                JsonInterp::<'_, 'de, 'program, true>::new(&mut parser, dst, self.blocks);
            run_dense_program(&field.program, self.blocks, &mut interp).map_err(run_error)?;
            interp.finish_success();
        }
        Self::ensure_raw_parser_finished(&mut parser)
    }

    fn read_captured_struct_fields(
        &mut self,
        shape: &'static Shape,
        variant: &'program ExternalVariantPlan<ExecBlock>,
        fields: &[TaggedRawField<'de>],
        skip_key: Option<&'static str>,
    ) -> Result<(), DeserializeError> {
        for raw_field in fields {
            if skip_key.is_some_and(|key| raw_field.name.as_ref() == key) {
                continue;
            }

            let Some((index, field)) =
                Self::captured_variant_field(variant, raw_field.name.as_ref().as_bytes())
            else {
                if shape.has_deny_unknown_fields_attr() {
                    return Err(vm_error(
                        Some(raw_field.span),
                        DeserializeErrorKind::UnknownField {
                            field: raw_field.name.to_string().into(),
                            suggestion: None,
                        },
                    ));
                }
                continue;
            };

            if let Some(first_span) = self.struct_seen_span(variant.tracking, index) {
                return Err(vm_error(
                    Some(raw_field.span),
                    DeserializeErrorKind::DuplicateField {
                        field: field.name.into(),
                        first_span: Some(first_span),
                    },
                ));
            }

            let field_ptr = unsafe { self.base.field_uninit(field.offset) };
            self.write_raw_field_value(field, field_ptr, raw_field.raw)?;
            self.mark_struct_field(variant.tracking, index, raw_field.span);
        }

        unsafe {
            self.finish_struct_frame(variant.tracking)?;
        }
        Ok(())
    }

    fn read_raw_single_field_variant_payload(
        &mut self,
        shape: &'static Shape,
        variant: &'program ExternalVariantPlan<ExecBlock>,
        raw: &'de str,
        span: Span,
    ) -> Result<(), DeserializeError> {
        let [field] = variant.fields.as_ref() else {
            return Err(unsupported(shape, "non-single-field enum payload"));
        };

        self.push_struct_frame(
            shape,
            &variant.fields,
            variant.dispatch.as_ref(),
            variant.tracking,
        );
        let field_ptr = unsafe { self.base.field_uninit(field.offset) };
        self.write_raw_field_value(field, field_ptr, raw)?;
        self.mark_struct_field(variant.tracking, 0, span);
        unsafe {
            self.finish_struct_frame(variant.tracking)?;
        }
        Ok(())
    }

    fn read_raw_tuple_variant_payload(
        &mut self,
        shape: &'static Shape,
        variant: &'program ExternalVariantPlan<ExecBlock>,
        raw: &'de str,
    ) -> Result<(), DeserializeError> {
        let mut parser = JsonParser::<true>::new(raw.as_bytes());
        parser.consume_array_start_fast()?;
        self.push_struct_frame(
            shape,
            &variant.fields,
            variant.dispatch.as_ref(),
            variant.tracking,
        );

        for (index, field) in variant.fields.iter().enumerate() {
            if parser.consume_sequence_end_if_next()? {
                return Err(vm_error(
                    None,
                    DeserializeErrorKind::UnexpectedEof {
                        expected: "tuple variant element",
                    },
                ));
            }

            let span = parser
                .peek_event()?
                .map_or(Span { offset: 0, len: 0 }, |event| event.span);
            let raw = parser
                .capture_raw()?
                .ok_or_else(|| unsupported_shape_message("raw JSON capture failed"))?;
            let field_ptr = unsafe { self.base.field_uninit(field.offset) };
            self.write_raw_field_value(field, field_ptr, raw)?;
            self.mark_struct_field(variant.tracking, index, span);
        }

        if !parser.consume_sequence_end_if_next()? {
            let got = match parser.peek_event()? {
                Some(event) => event.kind_name().into(),
                None => "end of input".into(),
            };
            return Err(vm_error(
                None,
                DeserializeErrorKind::UnexpectedToken {
                    expected: "sequence end for tuple variant",
                    got,
                },
            ));
        }

        unsafe {
            self.finish_struct_frame(variant.tracking)?;
        }
        Self::ensure_raw_parser_finished(&mut parser)
    }

    fn read_raw_struct_variant_payload(
        &mut self,
        shape: &'static Shape,
        variant: &'program ExternalVariantPlan<ExecBlock>,
        raw: &'de str,
    ) -> Result<(), DeserializeError> {
        let mut parser = JsonParser::<true>::new(raw.as_bytes());
        {
            let mut interp: JsonInterp<'_, 'de, 'program, true> =
                JsonInterp::<'_, 'de, 'program, true>::new(&mut parser, self.base, self.blocks);
            let fields = interp.collect_tagged_raw_fields("struct variant payload")?;
            interp.push_struct_frame(
                shape,
                &variant.fields,
                variant.dispatch.as_ref(),
                variant.tracking,
            );
            interp.read_captured_struct_fields(shape, variant, &fields, None)?;
            interp.finish_success();
        }
        Self::ensure_raw_parser_finished(&mut parser)
    }

    fn consume_raw_unit_variant_payload(&mut self, raw: &'de str) -> Result<(), DeserializeError> {
        let mut parser = JsonParser::<true>::new(raw.as_bytes());
        {
            let mut interp: JsonInterp<'_, 'de, 'program, true> =
                JsonInterp::<'_, 'de, 'program, true>::new(&mut parser, self.base, self.blocks);
            interp.consume_unit_variant_payload()?;
            interp.finish_success();
        }
        Self::ensure_raw_parser_finished(&mut parser)
    }

    fn read_internal_tagged_enum(
        &mut self,
        shape: &'static Shape,
        enum_type: EnumType,
        tag_key: &'static str,
        variants: &'program [ExternalVariantPlan<ExecBlock>],
    ) -> Result<Control<'program, ExecBlock, ExecOp, Continuation<'program>>, DeserializeError>
    {
        let fields = self.collect_tagged_raw_fields("struct for internally tagged enum")?;
        let tag_field = Self::require_tagged_field(&fields, tag_key, shape)?;
        let tag = Self::read_raw_tag_name(tag_field, tag_key)?;
        let variant = Self::tagged_variant(variants, &tag, tag_field.span)?;

        unsafe {
            write_enum_discriminant(shape, enum_type, variant.variant, self.base)?;
        }

        match variant.variant.data.kind {
            StructKind::Unit => Ok(Control::Continue),
            StructKind::Struct => {
                self.push_struct_frame(
                    shape,
                    &variant.fields,
                    variant.dispatch.as_ref(),
                    variant.tracking,
                );
                self.read_captured_struct_fields(shape, variant, &fields, Some(tag_key))?;
                Ok(Control::Continue)
            }
            StructKind::Tuple | StructKind::TupleStruct => {
                Err(unsupported(shape, "internally tagged tuple enum variant"))
            }
        }
    }

    fn read_adjacent_tagged_enum(
        &mut self,
        shape: &'static Shape,
        enum_type: EnumType,
        tag_key: &'static str,
        content_key: &'static str,
        variants: &'program [ExternalVariantPlan<ExecBlock>],
    ) -> Result<Control<'program, ExecBlock, ExecOp, Continuation<'program>>, DeserializeError>
    {
        let fields = self.collect_tagged_raw_fields("struct for adjacently tagged enum")?;
        let tag_field = Self::require_tagged_field(&fields, tag_key, shape)?;
        let tag = Self::read_raw_tag_name(tag_field, tag_key)?;
        let variant = Self::tagged_variant(variants, &tag, tag_field.span)?;
        let content = Self::unique_tagged_field(&fields, content_key)?;

        unsafe {
            write_enum_discriminant(shape, enum_type, variant.variant, self.base)?;
        }

        match variant.variant.data.kind {
            StructKind::Unit => {
                if let Some(content) = content {
                    self.consume_raw_unit_variant_payload(content.raw)?;
                }
                Ok(Control::Continue)
            }
            StructKind::Tuple | StructKind::TupleStruct if variant.fields.len() == 1 => {
                let content = content.ok_or_else(|| {
                    vm_error(
                        None,
                        DeserializeErrorKind::MissingField {
                            field: content_key,
                            container_shape: shape,
                        },
                    )
                })?;
                self.read_raw_single_field_variant_payload(
                    shape,
                    variant,
                    content.raw,
                    content.span,
                )?;
                Ok(Control::Continue)
            }
            StructKind::Tuple | StructKind::TupleStruct => {
                let content = content.ok_or_else(|| {
                    vm_error(
                        None,
                        DeserializeErrorKind::MissingField {
                            field: content_key,
                            container_shape: shape,
                        },
                    )
                })?;
                self.read_raw_tuple_variant_payload(shape, variant, content.raw)?;
                Ok(Control::Continue)
            }
            StructKind::Struct => {
                let content = content.ok_or_else(|| {
                    vm_error(
                        None,
                        DeserializeErrorKind::MissingField {
                            field: content_key,
                            container_shape: shape,
                        },
                    )
                })?;
                self.read_raw_struct_variant_payload(shape, variant, content.raw)?;
                Ok(Control::Continue)
            }
        }
    }

    fn read_tagged_enum(
        &mut self,
        shape: &'static Shape,
        enum_type: EnumType,
        tag_key: &'static str,
        content_key: Option<&'static str>,
        variants: &'program [ExternalVariantPlan<ExecBlock>],
    ) -> Result<Control<'program, ExecBlock, ExecOp, Continuation<'program>>, DeserializeError>
    {
        match content_key {
            Some(content_key) => {
                self.read_adjacent_tagged_enum(shape, enum_type, tag_key, content_key, variants)
            }
            None => self.read_internal_tagged_enum(shape, enum_type, tag_key, variants),
        }
    }

    fn finish_external_enum_payload(
        &mut self,
        tracking: StructTracking,
        close_object: bool,
    ) -> Result<Control<'program, ExecBlock, ExecOp, Continuation<'program>>, DeserializeError>
    {
        if close_object {
            self.consume_external_enum_object_end()?;
        }
        unsafe {
            self.finish_struct_frame(tracking)?;
        }
        Ok(Control::Continue)
    }

    fn read_external_enum_single_field_payload(
        &mut self,
        shape: &'static Shape,
        variant: &'program ExternalVariantPlan<ExecBlock>,
        span: Span,
        close_object: bool,
    ) -> Result<Control<'program, ExecBlock, ExecOp, Continuation<'program>>, DeserializeError>
    {
        let [field] = variant.fields.as_ref() else {
            return Err(unsupported(shape, "non-single-field enum fallback payload"));
        };

        self.push_struct_frame(
            shape,
            &variant.fields,
            variant.dispatch.as_ref(),
            variant.tracking,
        );
        let field_ptr = unsafe { self.base.field_uninit(field.offset) };
        if let Some(scalar) = field.scalar {
            let (value, value_span) = self.parser.read_scalar_token()?;
            unsafe {
                scalar.write(field.shape, field_ptr, value, value_span)?;
            }
            self.mark_struct_field(variant.tracking, 0, span);
            return self.finish_external_enum_payload(variant.tracking, close_object);
        }

        let old_base = self.base;
        self.base = field_ptr;
        Ok(call_program_or_block_then(
            &field.program,
            Continuation::ExternalEnumSingleField {
                tracking: variant.tracking,
                index: 0,
                span,
                old_base,
                close_object,
            },
        ))
    }

    fn read_external_enum_tuple_payload(
        &mut self,
        shape: &'static Shape,
        variant: &'program ExternalVariantPlan<ExecBlock>,
    ) -> Result<Control<'program, ExecBlock, ExecOp, Continuation<'program>>, DeserializeError>
    {
        self.parser.consume_array_start_fast()?;
        self.push_struct_frame(
            shape,
            &variant.fields,
            variant.dispatch.as_ref(),
            variant.tracking,
        );
        self.read_external_enum_tuple_next(&variant.fields, variant.tracking, 0, true)
    }

    fn read_external_enum_tuple_next(
        &mut self,
        fields: &'program [FieldPlan<ExecBlock>],
        tracking: StructTracking,
        mut next_index: usize,
        close_object: bool,
    ) -> Result<Control<'program, ExecBlock, ExecOp, Continuation<'program>>, DeserializeError>
    {
        loop {
            let Some(field) = fields.get(next_index) else {
                if self.parser.consume_sequence_end_if_next()? {
                    return self.finish_external_enum_payload(tracking, close_object);
                }

                let got = match self.parser.peek_event()? {
                    Some(event) => event.kind_name().into(),
                    None => "end of input".into(),
                };
                return Err(vm_error(
                    None,
                    DeserializeErrorKind::UnexpectedToken {
                        expected: "sequence end for tuple variant",
                        got,
                    },
                ));
            };

            if self.parser.consume_sequence_end_if_next()? {
                return Err(vm_error(
                    None,
                    DeserializeErrorKind::UnexpectedEof {
                        expected: "tuple variant element",
                    },
                ));
            }

            let field_ptr = unsafe { self.base.field_uninit(field.offset) };
            if let Some(scalar) = field.scalar {
                let (value, span) = self.parser.read_scalar_token()?;
                unsafe {
                    scalar.write(field.shape, field_ptr, value, span)?;
                }
                self.mark_struct_field(tracking, next_index, span);
                next_index += 1;
                continue;
            }

            let span = self
                .parser
                .peek_event()?
                .map_or(Span { offset: 0, len: 0 }, |event| event.span);
            let old_base = self.base;
            self.base = field_ptr;
            return Ok(call_program_or_block_then(
                &field.program,
                Continuation::ExternalEnumTupleField {
                    tracking,
                    fields,
                    index: next_index,
                    span,
                    old_base,
                    close_object,
                },
            ));
        }
    }

    fn read_external_enum(
        &mut self,
        shape: &'static Shape,
        enum_type: EnumType,
        variants: &'program [ExternalVariantPlan<ExecBlock>],
    ) -> Result<Control<'program, ExecBlock, ExecOp, Continuation<'program>>, DeserializeError>
    {
        let Some(event) = self.parser.peek_event()? else {
            return Err(vm_error(
                None,
                DeserializeErrorKind::UnexpectedEof { expected: "enum" },
            ));
        };

        match &event.kind {
            ParseEventKind::Scalar(ScalarValue::Str(_)) => {
                let ParseEventKind::Scalar(ScalarValue::Str(variant_name)) = &event.kind else {
                    unreachable!("matched scalar string");
                };
                if let Some(variant) = find_external_variant(variants, variant_name) {
                    let (value, span) = self.parser.read_scalar_token()?;
                    let JsonScalarToken::Str(variant_name) = value else {
                        unreachable!("peeked scalar string must read back as scalar string");
                    };
                    if variant.variant.data.kind != StructKind::Unit {
                        return Err(vm_error(
                            Some(span),
                            DeserializeErrorKind::UnexpectedToken {
                                expected: "externally tagged enum object for payload variant",
                                got: variant_name.into_owned().into(),
                            },
                        ));
                    }
                    unsafe {
                        write_enum_discriminant(shape, enum_type, variant.variant, self.base)?;
                    }
                    return Ok(Control::Continue);
                }
                let Some(variant) = external_other_variant(variants) else {
                    return Err(vm_error(
                        Some(event.span),
                        DeserializeErrorKind::UnexpectedToken {
                            expected: "known enum variant",
                            got: variant_name.to_string().into(),
                        },
                    ));
                };
                unsafe {
                    write_enum_discriminant(shape, enum_type, variant.variant, self.base)?;
                }
                self.read_external_enum_single_field_payload(shape, variant, event.span, false)
            }
            ParseEventKind::Scalar(_) => {
                let Some(variant) = external_other_variant(variants) else {
                    return Err(vm_error(
                        Some(event.span),
                        DeserializeErrorKind::UnexpectedToken {
                            expected: "string or struct for enum",
                            got: event.kind_name().into(),
                        },
                    ));
                };
                unsafe {
                    write_enum_discriminant(shape, enum_type, variant.variant, self.base)?;
                }
                self.read_external_enum_single_field_payload(shape, variant, event.span, false)
            }
            ParseEventKind::StructStart(_) => {
                self.parser.consume_object_start_fast()?;
                let JsonObjectKeyStep::Field { key, span } =
                    self.parser.next_object_key_or_end()?
                else {
                    return Err(vm_error(
                        Some(event.span),
                        DeserializeErrorKind::UnexpectedToken {
                            expected: "variant name",
                            got: "empty object".into(),
                        },
                    ));
                };

                let variant = if let Some(variant) =
                    find_external_variant_input(self.parser, variants, &key)?
                {
                    variant
                } else if let Some(variant) = external_other_variant(variants) {
                    variant
                } else {
                    let key = self.parser.materialize_field_key(key)?;
                    return Err(vm_error(
                        Some(span),
                        DeserializeErrorKind::UnexpectedToken {
                            expected: "known enum variant",
                            got: key.as_str().to_string().into(),
                        },
                    ));
                };

                unsafe {
                    write_enum_discriminant(shape, enum_type, variant.variant, self.base)?;
                }

                match variant.variant.data.kind {
                    StructKind::Unit => {
                        self.consume_unit_variant_payload()?;
                        self.consume_external_enum_object_end()?;
                        Ok(Control::Continue)
                    }
                    StructKind::Tuple | StructKind::TupleStruct if variant.fields.len() == 1 => {
                        self.read_external_enum_single_field_payload(shape, variant, span, true)
                    }
                    StructKind::Struct => {
                        self.parser.consume_object_start_fast()?;
                        self.push_struct_frame(
                            shape,
                            &variant.fields,
                            variant.dispatch.as_ref(),
                            variant.tracking,
                        );
                        let loop_id = variant
                            .loop_id
                            .expect("struct enum variant has a lowered loop");
                        Ok(Control::CallBlockThen(
                            loop_id,
                            Continuation::ExternalEnumStruct {
                                tracking: variant.tracking,
                            },
                        ))
                    }
                    StructKind::Tuple | StructKind::TupleStruct => {
                        self.read_external_enum_tuple_payload(shape, variant)
                    }
                }
            }
            other => Err(vm_error(
                Some(event.span),
                DeserializeErrorKind::UnexpectedToken {
                    expected: "string or struct for enum",
                    got: other.kind_name().into(),
                },
            )),
        }
    }

    fn push_list_element(
        &mut self,
        list: ListDef,
        scratch: &ScratchSlot,
    ) -> Result<(), DeserializeError> {
        let list_ptr = match self
            .lists
            .last()
            .expect("list frame is present while pushing element")
        {
            ListFrame::Push { guard } => guard.ptr(),
            ListFrame::Adopt { .. } => {
                unreachable!("direct-adopt lists do not push through ListDef")
            }
        };
        let push = list
            .push()
            .ok_or_else(|| unsupported(list.t(), "list push"))?;
        unsafe {
            push(PtrMut::new(list_ptr), scratch_ptr_mut(scratch));
        }
        Ok(())
    }

    fn direct_list_slot(&mut self) -> Result<Option<PtrUninit>, DeserializeError> {
        let frame = self
            .lists
            .last_mut()
            .expect("list frame is present while decoding element");
        let ListFrame::Adopt { builder, .. } = frame else {
            return Ok(None);
        };
        let slot = builder.next_uninit_slot().map_err(raw_alloc_error)?;
        Ok(Some(PtrUninit::new(slot)))
    }

    unsafe fn mark_direct_list_slot_initialized(&mut self) {
        let frame = self
            .lists
            .last_mut()
            .expect("list frame is present after direct element initialization");
        let ListFrame::Adopt { builder, .. } = frame else {
            unreachable!("only direct-adopt lists mark direct slots");
        };
        unsafe {
            builder.mark_initialized();
        }
    }

    fn insert_set_element(&mut self, set: SetDef, scratch: &ScratchSlot) {
        let set_ptr = self
            .sets
            .last()
            .expect("set frame is present while inserting element");
        unsafe {
            (set.vtable.insert)(PtrMut::new(set_ptr.guard.ptr()), scratch_ptr_mut(scratch));
        }
    }

    fn step_set_next(
        &mut self,
        plan: SetStepPlan<'program>,
    ) -> Result<Control<'program, ExecBlock, ExecOp, Continuation<'program>>, DeserializeError>
    {
        loop {
            if let Some(scalar) = plan.element_scalar {
                let step = self.parser.next_sequence_scalar_or_end()?;
                let JsonSequenceScalarStep::Value { value } = step else {
                    return Ok(Control::Continue);
                };

                let scratch = self.scratch.reserve(plan.element_layout);
                unsafe {
                    write_scalar_input(
                        self.parser,
                        plan.set.t(),
                        scalar,
                        scratch_ptr_uninit(&scratch),
                        value,
                    )?;
                }
                self.insert_set_element(plan.set, &scratch);
                self.scratch.release(scratch);
                continue;
            }

            if let Some(option_scalar) = plan.element_option_scalar {
                let step = self.parser.next_sequence_scalar_or_end()?;
                let JsonSequenceScalarStep::Value { value } = step else {
                    return Ok(Control::Continue);
                };

                let scratch = self.scratch.reserve(plan.element_layout);
                if value.is_null() {
                    unsafe {
                        (option_scalar.option.vtable.init_none)(scratch_ptr_uninit(&scratch));
                    }
                } else {
                    let inner = self.scratch.reserve(option_scalar.inner_layout);
                    unsafe {
                        write_scalar_input(
                            self.parser,
                            option_scalar.option.t(),
                            option_scalar.scalar,
                            scratch_ptr_uninit(&inner),
                            value,
                        )?;
                        (option_scalar.option.vtable.init_some)(
                            scratch_ptr_uninit(&scratch),
                            scratch_ptr_mut(&inner),
                        );
                    }
                    self.scratch.release(inner);
                }
                self.insert_set_element(plan.set, &scratch);
                self.scratch.release(scratch);
                continue;
            }

            if self.parser.consume_sequence_end_if_next()? {
                return Ok(Control::Continue);
            }

            let scratch = self.scratch.reserve(plan.element_layout);
            let old_base = self.base;
            self.base = scratch_ptr_uninit(&scratch);
            return Ok(call_program_or_block_then(
                plan.element_program,
                Continuation::InsertedSetElement {
                    set: plan.set,
                    old_base,
                    scratch,
                    loop_id: plan.loop_id,
                },
            ));
        }
    }

    fn insert_map_entry(
        &mut self,
        map: MapDef,
        key_scalar: ScalarType,
        key_layout: Layout,
        mut key: String,
        key_span: Span,
        value_scratch: ScratchSlot,
    ) -> Result<(), DeserializeError> {
        let map_ptr = self
            .maps
            .last()
            .expect("map frame is present while inserting entry");
        let map_ptr = PtrMut::new(map_ptr.guard.ptr());

        if key_scalar == ScalarType::String
            && map.k().is_type::<String>()
            && let Some(insert_owned_string_key) = map.vtable.insert_owned_string_key
        {
            let mut owned_key = ManuallyDrop::new(key);
            let consumed = unsafe {
                insert_owned_string_key(
                    map_ptr,
                    PtrMut::new((&mut owned_key as *mut ManuallyDrop<String>).cast::<String>()),
                    scratch_ptr_mut(&value_scratch),
                )
            };
            if consumed {
                self.scratch.release(value_scratch);
                return Ok(());
            }
            key = ManuallyDrop::into_inner(owned_key);
        }

        let key_scratch = self.scratch.reserve(key_layout);
        let key_result = unsafe {
            write_map_key(
                map.k(),
                key_scalar,
                scratch_ptr_uninit(&key_scratch),
                key,
                key_span,
            )
        };
        if let Err(err) = key_result {
            unsafe {
                drop_shape_value(map.v() as *const Shape as *const (), value_scratch.ptr());
            }
            self.scratch.release(value_scratch);
            self.scratch.release(key_scratch);
            return Err(err);
        }

        unsafe {
            (map.vtable.insert)(
                map_ptr,
                scratch_ptr_mut(&key_scratch),
                scratch_ptr_mut(&value_scratch),
            );
        }
        self.scratch.release(value_scratch);
        self.scratch.release(key_scratch);
        Ok(())
    }

    fn insert_map_entry_input(
        &mut self,
        map: MapDef,
        key_scalar: ScalarType,
        key_layout: Layout,
        key: JsonFieldKeyInput<'de>,
        key_span: Span,
        value_scratch: ScratchSlot,
    ) -> Result<(), DeserializeError> {
        if key_scalar == ScalarType::String
            && map.k().is_type::<String>()
            && let Some(insert_borrowed_str_key) = map.vtable.insert_borrowed_str_key
        {
            match self.parser.field_key_unescaped_str(&key) {
                Ok(Some(key)) => {
                    let map_ptr = self
                        .maps
                        .last()
                        .expect("map frame is present while inserting entry");
                    let consumed = unsafe {
                        insert_borrowed_str_key(
                            PtrMut::new(map_ptr.guard.ptr()),
                            PtrConst::new(key as *const str),
                            scratch_ptr_mut(&value_scratch),
                        )
                    };
                    if consumed {
                        self.scratch.release(value_scratch);
                        return Ok(());
                    }
                }
                Ok(None) => {}
                Err(err) => {
                    unsafe {
                        drop_shape_value(map.v() as *const Shape as *const (), value_scratch.ptr());
                    }
                    self.scratch.release(value_scratch);
                    return Err(err.into());
                }
            }
        }

        let key = match self.parser.materialize_field_key(key) {
            Ok(key) => key.as_str().to_owned(),
            Err(err) => {
                unsafe {
                    drop_shape_value(map.v() as *const Shape as *const (), value_scratch.ptr());
                }
                self.scratch.release(value_scratch);
                return Err(err.into());
            }
        };
        self.insert_map_entry(map, key_scalar, key_layout, key, key_span, value_scratch)
    }

    fn try_insert_borrowed_str_map_entry(
        &mut self,
        map: MapDef,
        key: &JsonFieldKeyInput<'de>,
        value: &JsonScalarInput<'de>,
    ) -> Result<bool, DeserializeError> {
        if !map.k().is_type::<String>() || !map.v().is_type::<String>() {
            return Ok(false);
        }
        let Some(insert_borrowed_str_entry) = map.vtable.insert_borrowed_str_entry else {
            return Ok(false);
        };
        let Some(key) = self.parser.field_key_unescaped_str(key)? else {
            return Ok(false);
        };
        let Some(value) = self.parser.scalar_input_unescaped_str(value)? else {
            return Ok(false);
        };

        let map_ptr = self
            .maps
            .last()
            .expect("map frame is present while inserting entry");
        let consumed = unsafe {
            insert_borrowed_str_entry(
                PtrMut::new(map_ptr.guard.ptr()),
                PtrConst::new(key as *const str),
                PtrConst::new(value as *const str),
            )
        };
        Ok(consumed)
    }

    fn step_map_next(
        &mut self,
        plan: MapStepPlan<'program>,
    ) -> Result<Control<'program, ExecBlock, ExecOp, Continuation<'program>>, DeserializeError>
    {
        let (key_input, key_span) = match self.parser.next_object_key_or_end()? {
            JsonObjectKeyStep::End => return Ok(Control::Continue),
            JsonObjectKeyStep::Field { key, span } => (key, span),
        };

        if let Some(scalar) = plan.value_scalar {
            let value = self.parser.read_current_scalar_input()?;
            if scalar.scalar == ScalarType::String
                && self.try_insert_borrowed_str_map_entry(plan.map, &key_input, &value)?
            {
                return Ok(Control::CallBlock(plan.loop_id));
            }

            let value_scratch = self.scratch.reserve(plan.value_layout);
            unsafe {
                scalar.write_input(
                    &*self.parser,
                    plan.map.v(),
                    scratch_ptr_uninit(&value_scratch),
                    value,
                )?;
            }
            self.insert_map_entry_input(
                plan.map,
                plan.key_scalar,
                plan.key_layout,
                key_input,
                key_span,
                value_scratch,
            )?;
            return Ok(Control::CallBlock(plan.loop_id));
        }

        let key = self
            .parser
            .materialize_field_key(key_input)?
            .as_str()
            .to_owned();
        let value_scratch = self.scratch.reserve(plan.value_layout);
        let old_base = self.base;
        self.base = scratch_ptr_uninit(&value_scratch);
        Ok(call_program_or_block_then(
            plan.value_program,
            Continuation::MapValueDone {
                map: plan.map,
                key_scalar: plan.key_scalar,
                key_layout: plan.key_layout,
                key,
                key_span,
                value_scratch,
                old_base,
                loop_id: plan.loop_id,
            },
        ))
    }

    fn list_next_scalar(
        &mut self,
        list: ListDef,
        scalar: ScalarType,
        element_layout: Layout,
    ) -> Result<Control<'program, ExecBlock, ExecOp, Continuation<'program>>, DeserializeError>
    {
        match scalar {
            ScalarType::U8 => {
                self.list_next_scalar_with(list, element_layout, write_u8_input::<TRUSTED_UTF8>)
            }
            ScalarType::U16 => {
                self.list_next_scalar_with(list, element_layout, write_u16_input::<TRUSTED_UTF8>)
            }
            ScalarType::U32 => {
                self.list_next_scalar_with(list, element_layout, write_u32_input::<TRUSTED_UTF8>)
            }
            ScalarType::U64 => {
                self.list_next_scalar_with(list, element_layout, write_u64_input::<TRUSTED_UTF8>)
            }
            ScalarType::U128 => {
                self.list_next_scalar_with(list, element_layout, write_u128_input::<TRUSTED_UTF8>)
            }
            ScalarType::USize => {
                self.list_next_scalar_with(list, element_layout, write_usize_input::<TRUSTED_UTF8>)
            }
            ScalarType::I8 => {
                self.list_next_scalar_with(list, element_layout, write_i8_input::<TRUSTED_UTF8>)
            }
            ScalarType::I16 => {
                self.list_next_scalar_with(list, element_layout, write_i16_input::<TRUSTED_UTF8>)
            }
            ScalarType::I32 => {
                self.list_next_scalar_with(list, element_layout, write_i32_input::<TRUSTED_UTF8>)
            }
            ScalarType::I64 => {
                self.list_next_scalar_with(list, element_layout, write_i64_input::<TRUSTED_UTF8>)
            }
            ScalarType::I128 => {
                self.list_next_scalar_with(list, element_layout, write_i128_input::<TRUSTED_UTF8>)
            }
            ScalarType::ISize => {
                self.list_next_scalar_with(list, element_layout, write_isize_input::<TRUSTED_UTF8>)
            }
            _ => {
                let shape = list.t();
                self.list_next_scalar_with(list, element_layout, |parser, dst, value| unsafe {
                    write_scalar_input(parser, shape, scalar, dst, value)
                })
            }
        }
    }

    fn list_next_scalar_with<W>(
        &mut self,
        list: ListDef,
        element_layout: Layout,
        mut write: W,
    ) -> Result<Control<'program, ExecBlock, ExecOp, Continuation<'program>>, DeserializeError>
    where
        W: FnMut(
            &JsonParser<'de, TRUSTED_UTF8>,
            PtrUninit,
            JsonScalarInput<'de>,
        ) -> Result<(), DeserializeError>,
    {
        loop {
            let value = match self.parser.next_sequence_scalar_or_end()? {
                JsonSequenceScalarStep::End => return Ok(Control::Continue),
                JsonSequenceScalarStep::Value { value } => value,
            };

            if let Some(slot) = self.direct_list_slot()? {
                write(self.parser, slot, value)?;
                unsafe {
                    self.mark_direct_list_slot_initialized();
                }
                continue;
            }

            let scratch = self.scratch.reserve(element_layout);
            write(self.parser, scratch_ptr_uninit(&scratch), value)?;
            self.push_list_element(list, &scratch)?;
            self.scratch.release(scratch);
        }
    }

    fn read_scalar_struct(
        &mut self,
        shape: &'static Shape,
        fields: &'program [ScalarFieldPlan],
        dispatch: Option<&'program RawFieldDispatch>,
    ) -> Result<(), DeserializeError> {
        self.parser.consume_object_start_fast()?;
        let base = self.base;
        if fields.len() <= TINY_SCALAR_STRUCT_MAX_FIELDS {
            return self.read_tiny_scalar_struct_fields(shape, base, fields);
        }

        match StructTracking::for_len(fields.len()) {
            StructTracking::Inline => {
                let mut frame = StructFrame::<ScalarFieldPlan, InitializedLedger<Span>>::new(
                    shape, base, fields, dispatch,
                );
                self.read_scalar_struct_fields(shape, &mut frame)?;
                unsafe {
                    frame.fill_missing_fields()?;
                }
            }
            StructTracking::Bitset => {
                let mut frame = StructFrame::<ScalarFieldPlan, BitsetStructSeen>::new(
                    shape, base, fields, dispatch,
                );
                self.read_scalar_struct_fields(shape, &mut frame)?;
                unsafe {
                    frame.fill_missing_fields()?;
                }
            }
            StructTracking::Heap => {
                let mut frame = StructFrame::<ScalarFieldPlan, HeapStructSeen>::new(
                    shape, base, fields, dispatch,
                );
                self.read_scalar_struct_fields(shape, &mut frame)?;
                unsafe {
                    frame.fill_missing_fields()?;
                }
            }
        }
        Ok(())
    }

    fn read_tiny_scalar_struct_fields(
        &mut self,
        shape: &'static Shape,
        base: PtrUninit,
        fields: &'program [ScalarFieldPlan],
    ) -> Result<(), DeserializeError> {
        let mut frame = TinyScalarStructFrame::new(shape, base, fields);
        if self.try_read_fused_tiny_i32_struct_fields(&mut frame)? {
            core::mem::forget(frame);
            return Ok(());
        }

        loop {
            let Some(field) = frame.fields.get(frame.next_field).copied() else {
                if self.parser.consume_object_end_if_next()? {
                    break;
                }
                match self.parser.next_object_key_or_end()? {
                    JsonObjectKeyStep::End => break,
                    JsonObjectKeyStep::Field { key, span } => {
                        self.read_tiny_scalar_struct_pending_field(&mut frame, key, span)?;
                    }
                }
                continue;
            };
            let expected = field.name;

            if field.scalar.scalar == ScalarType::I32 {
                match self.parser.next_ordered_object_i32_or_key(expected)? {
                    JsonObjectOrderedI32Step::End => break,
                    JsonObjectOrderedI32Step::Matched { span, value } => {
                        let index = frame.next_field;
                        self.write_tiny_i32_struct_field(&mut frame, index, field, span, value);
                    }
                    JsonObjectOrderedI32Step::MatchedInput { span, value } => {
                        let index = frame.next_field;
                        let field = frame.fields[index];
                        self.write_tiny_scalar_struct_field(&mut frame, index, field, span, value)?;
                    }
                    JsonObjectOrderedI32Step::Field { key, span } => {
                        self.read_tiny_scalar_struct_pending_field(&mut frame, key, span)?;
                    }
                }
                continue;
            }

            match self.parser.next_ordered_object_scalar_or_key(expected)? {
                JsonObjectOrderedScalarStep::End => break,
                JsonObjectOrderedScalarStep::Matched { span, value } => {
                    let index = frame.next_field;
                    self.write_tiny_scalar_struct_field(&mut frame, index, field, span, value)?;
                }
                JsonObjectOrderedScalarStep::Field { key, span } => {
                    self.read_tiny_scalar_struct_pending_field(&mut frame, key, span)?;
                }
            }
        }

        if frame.all_initialized() {
            core::mem::forget(frame);
        } else {
            unsafe {
                frame.fill_missing_fields()?;
            }
        }
        Ok(())
    }

    fn try_read_fused_tiny_i32_struct_fields(
        &mut self,
        frame: &mut TinyScalarStructFrame<'program>,
    ) -> Result<bool, DeserializeError> {
        if !tiny_i32_struct_fields_are_fusible(frame.fields) {
            return Ok(false);
        }

        let mut names = [""; TINY_SCALAR_STRUCT_MAX_FIELDS];
        let mut spans = [Span { offset: 0, len: 0 }; TINY_SCALAR_STRUCT_MAX_FIELDS];
        let mut values = [0i32; TINY_SCALAR_STRUCT_MAX_FIELDS];
        for (index, field) in frame.fields.iter().enumerate() {
            names[index] = field.name;
        }

        if !self.parser.try_consume_ordered_i32_object_fields(
            &names[..frame.fields.len()],
            &mut spans[..frame.fields.len()],
            &mut values[..frame.fields.len()],
        )? {
            return Ok(false);
        }

        for index in 0..frame.fields.len() {
            let field = frame.fields[index];
            self.write_tiny_i32_struct_field(frame, index, field, spans[index], values[index]);
        }
        Ok(true)
    }

    fn read_tiny_scalar_struct_pending_field(
        &mut self,
        frame: &mut TinyScalarStructFrame<'program>,
        key: JsonFieldKeyInput<'_>,
        span: Span,
    ) -> Result<(), DeserializeError> {
        let key_is_raw = matches!(key, JsonFieldKeyInput::Raw { .. });
        let matched =
            if let Some((index, field)) = frame.match_next_field_input(&*self.parser, &key)? {
                Some((index, field))
            } else {
                frame.match_unordered_field_input(&*self.parser, &key)?
            };

        let Some((index, field)) = matched else {
            let key = self.parser.materialize_field_key(key)?;
            if frame.shape.has_deny_unknown_fields_attr() {
                return Err(vm_error(
                    Some(span),
                    DeserializeErrorKind::UnknownField {
                        field: key.as_str().to_owned().into(),
                        suggestion: None,
                    },
                ));
            }
            self.parser.skip_value_strict()?;
            return Ok(());
        };

        if let Some(first_span) = frame.seen_span(index) {
            return Err(vm_error(
                Some(span),
                DeserializeErrorKind::DuplicateField {
                    field: field.name.into(),
                    first_span: Some(first_span),
                },
            ));
        }

        let value = if key_is_raw {
            self.parser.read_current_scalar_input()?
        } else {
            let (value, value_span) = self.parser.read_scalar_token()?;
            JsonScalarInput::Materialized(value, value_span)
        };
        self.write_tiny_scalar_struct_field(frame, index, field, span, value)
    }

    fn write_tiny_scalar_struct_field(
        &mut self,
        frame: &mut TinyScalarStructFrame<'program>,
        index: usize,
        field: ScalarFieldPlan,
        span: Span,
        value: JsonScalarInput<'_>,
    ) -> Result<(), DeserializeError> {
        let field_ptr = unsafe { frame.base.field_uninit(field.offset) };
        unsafe {
            field
                .scalar
                .write_input(&*self.parser, field.shape, field_ptr, value)?;
        }
        frame.mark_seen(index, span);
        Ok(())
    }

    fn write_tiny_i32_struct_field(
        &mut self,
        frame: &mut TinyScalarStructFrame<'program>,
        index: usize,
        field: ScalarFieldPlan,
        span: Span,
        value: i32,
    ) {
        let field_ptr = unsafe { frame.base.field_uninit(field.offset) };
        unsafe {
            field_ptr.put(value);
        }
        frame.mark_seen(index, span);
    }

    fn read_scalar_struct_fields<Seen: StructSeenStore>(
        &mut self,
        shape: &'static Shape,
        frame: &mut StructFrame<'program, ScalarFieldPlan, Seen>,
    ) -> Result<(), DeserializeError> {
        loop {
            match self.parser.next_object_key_or_end()? {
                JsonObjectKeyStep::End => return Ok(()),
                JsonObjectKeyStep::Field { key, span } => {
                    let key_is_raw = matches!(key, JsonFieldKeyInput::Raw { .. });
                    if let Some((index, field)) =
                        frame.match_next_field_input(&*self.parser, &key)?
                    {
                        self.read_scalar_struct_field(frame, index, field, key_is_raw, span)?;
                        continue;
                    }

                    let matched = if let Some(key) = self.parser.field_key_unescaped_bytes(&key) {
                        frame.match_unordered_scalar_field_bytes(key)
                    } else {
                        frame.match_unordered_field_input(&*self.parser, &key)?
                    };

                    let Some(matched) = matched else {
                        let key = self.parser.materialize_field_key(key)?;
                        if shape.has_deny_unknown_fields_attr() {
                            return Err(vm_error(
                                Some(span),
                                DeserializeErrorKind::UnknownField {
                                    field: key.as_str().to_owned().into(),
                                    suggestion: None,
                                },
                            ));
                        }
                        self.parser.skip_value_strict()?;
                        continue;
                    };

                    let MatchedField {
                        index,
                        field,
                        ordered,
                    } = matched;
                    if let Some(first_span) = frame.seen.get(index) {
                        return Err(vm_error(
                            Some(span),
                            DeserializeErrorKind::DuplicateField {
                                field: field.name.into(),
                                first_span: Some(first_span),
                            },
                        ));
                    }

                    debug_assert!(!ordered);
                    self.read_scalar_struct_field(frame, index, field, key_is_raw, span)?;
                }
            }
        }
    }

    #[inline(always)]
    fn read_scalar_struct_field<Seen: StructSeenStore>(
        &mut self,
        frame: &mut StructFrame<'program, ScalarFieldPlan, Seen>,
        index: usize,
        field: &'program ScalarFieldPlan,
        key_is_raw: bool,
        span: Span,
    ) -> Result<(), DeserializeError> {
        let field_ptr = unsafe { frame.base.field_uninit(field.offset) };
        if key_is_raw {
            let value = self.parser.read_current_scalar_input()?;
            unsafe {
                if frame.fields.len() <= 3 {
                    field
                        .scalar
                        .write_input(&*self.parser, field.shape, field_ptr, value)?;
                } else {
                    self.parser
                        .write_scalar_input_preselected(field, field_ptr, value)?;
                }
            }
        } else {
            let (value, value_span) = self.parser.read_scalar_token()?;
            unsafe {
                field
                    .scalar
                    .write(field.shape, field_ptr, value, value_span)?;
            }
        }
        frame.mark_seen(index, span);
        Ok(())
    }
}

struct InlineStack<T> {
    first: Option<T>,
    rest: Vec<T>,
}

impl<T> InlineStack<T> {
    fn new() -> Self {
        Self {
            first: None,
            rest: Vec::new(),
        }
    }

    fn push(&mut self, value: T) {
        if self.first.is_none() {
            self.first = Some(value);
        } else {
            self.rest.push(value);
        }
    }

    fn pop(&mut self) -> Option<T> {
        self.rest.pop().or_else(|| self.first.take())
    }

    fn last(&self) -> Option<&T> {
        self.rest.last().or(self.first.as_ref())
    }

    fn last_mut(&mut self) -> Option<&mut T> {
        self.rest.last_mut().or(self.first.as_mut())
    }
}

enum ListFrame {
    Push {
        guard: HandleGuard,
    },
    Adopt {
        list_shape: &'static Shape,
        list: ListDef,
        list_ptr: PtrUninit,
        builder: RawArrayBuilder,
    },
}

impl ListFrame {
    fn finish(self) -> Result<(), DeserializeError> {
        match self {
            Self::Push { mut guard } => {
                guard.disarm();
                Ok(())
            }
            Self::Adopt {
                list_shape,
                list,
                list_ptr,
                mut builder,
            } => {
                let from_raw_parts = list
                    .from_raw_parts()
                    .ok_or_else(|| unsupported(list_shape, "list from_raw_parts"))?;
                unsafe {
                    from_raw_parts(
                        list_ptr,
                        PtrMut::new(builder.ptr()),
                        builder.len(),
                        builder.cap(),
                    );
                }
                builder.adopt();
                Ok(())
            }
        }
    }
}

#[derive(Clone, Copy)]
struct MapStepPlan<'program> {
    map: MapDef,
    key_scalar: ScalarType,
    key_layout: Layout,
    value_program: &'program [ExecOp],
    value_scalar: Option<ScalarPlan>,
    value_layout: Layout,
    loop_id: ExecBlock,
}

#[derive(Clone, Copy)]
struct SetStepPlan<'program> {
    set: SetDef,
    element_program: &'program [ExecOp],
    element_scalar: Option<ScalarType>,
    element_option_scalar: Option<ListOptionScalar>,
    element_layout: Layout,
    loop_id: ExecBlock,
}

struct SetFrame {
    guard: HandleGuard,
}

impl SetFrame {
    fn finish(mut self) -> Result<(), DeserializeError> {
        self.guard.disarm();
        Ok(())
    }
}

struct MapFrame {
    guard: HandleGuard,
}

impl MapFrame {
    fn finish(mut self) -> Result<(), DeserializeError> {
        self.guard.disarm();
        Ok(())
    }
}

impl<const TRUSTED_UTF8: bool> Drop for JsonInterp<'_, '_, '_, TRUSTED_UTF8> {
    fn drop(&mut self) {
        if self.success {
            return;
        }

        while self.inline_structs.pop().is_some() {}
        if let Some(mut large_structs) = self.large_structs.take() {
            while large_structs.pop().is_some() {}
        }

        while self.maps.pop().is_some() {}
        while self.sets.pop().is_some() {}
        while self.lists.pop().is_some() {}
    }
}

impl<'program, 'parser, 'de, const TRUSTED_UTF8: bool> Step<'program, ExecBlock, ExecOp>
    for JsonInterp<'parser, 'de, 'program, TRUSTED_UTF8>
where
    JsonParser<'de, TRUSTED_UTF8>: ScalarInputPreselected<'de, TRUSTED_UTF8>,
{
    type Error = DeserializeError;
    type Continuation = Continuation<'program>;

    fn step(
        &mut self,
        op: &'program ExecOp,
    ) -> Result<Control<'program, ExecBlock, ExecOp, Self::Continuation>, Self::Error> {
        match op {
            JsonOp::CallBlock(shape) => Ok(Control::CallBlock(*shape)),
            JsonOp::ReadScalar { shape, scalar } => {
                let (value, span) = self.parser.read_scalar_token()?;
                unsafe {
                    scalar.write(shape, self.base, value, span)?;
                }
                Ok(Control::Continue)
            }
            JsonOp::ReadScalarStruct {
                shape,
                fields,
                dispatch,
            } => {
                self.read_scalar_struct(shape, fields, dispatch.as_ref())?;
                Ok(Control::Continue)
            }
            JsonOp::ReadStruct {
                shape,
                fields,
                dispatch,
                loop_id,
            } => {
                self.parser.consume_object_start_fast()?;
                let tracking = StructTracking::for_len(fields.len());
                self.push_struct_frame(shape, fields, dispatch.as_ref(), tracking);
                Ok(Control::CallBlockThen(
                    *loop_id,
                    Continuation::FinishStruct { tracking },
                ))
            }
            JsonOp::ReadExternalEnum {
                shape,
                enum_type,
                variants,
            } => self.read_external_enum(shape, *enum_type, variants),
            JsonOp::ReadTaggedEnum {
                shape,
                enum_type,
                tag_key,
                content_key,
                variants,
            } => self.read_tagged_enum(shape, *enum_type, tag_key, *content_key, variants),
            JsonOp::StructNext {
                shape,
                loop_id,
                raw_field_dispatch,
                tracking,
            } => match tracking {
                StructTracking::Inline => {
                    self.step_struct_next_inline(shape, *loop_id, *raw_field_dispatch)
                }
                StructTracking::Bitset | StructTracking::Heap => {
                    self.step_struct_next_large(shape, *loop_id, *raw_field_dispatch, *tracking)
                }
            },
            JsonOp::ReadOption {
                option,
                some_program,
                some_scalar,
                inner_layout,
            } => {
                if self.parser.consume_null_if_next()? {
                    unsafe {
                        (option.vtable.init_none)(self.base);
                    }
                    return Ok(Control::Continue);
                }

                let scratch = self.scratch.reserve(*inner_layout);
                if let Some(scalar) = some_scalar {
                    let (value, span) = self.parser.read_scalar_token()?;
                    unsafe {
                        scalar.write(option.t(), scratch_ptr_uninit(&scratch), value, span)?;
                        (option.vtable.init_some)(self.base, scratch_ptr_mut(&scratch));
                    }
                    self.scratch.release(scratch);
                    return Ok(Control::Continue);
                }

                let old_base = self.base;
                self.base = scratch_ptr_uninit(&scratch);
                Ok(call_program_or_block_then(
                    some_program,
                    Continuation::OptionSome {
                        option: *option,
                        option_ptr: old_base,
                        old_base,
                        scratch,
                    },
                ))
            }
            JsonOp::ReadList {
                list_shape,
                list,
                element_layout,
                loop_id,
            } => {
                self.parser.consume_array_start_fast()?;
                if list.from_raw_parts().is_some() {
                    let builder = RawArrayBuilder::new(
                        *element_layout,
                        list.t() as *const Shape as *const (),
                        drop_shape_value,
                    );
                    self.lists.push(ListFrame::Adopt {
                        list_shape,
                        list: *list,
                        list_ptr: self.base,
                        builder,
                    });
                } else {
                    let init = list
                        .init_in_place_with_capacity()
                        .ok_or_else(|| unsupported(list_shape, "list initialization"))?;
                    let list_ptr = unsafe { init(self.base, 0) };
                    self.lists.push(ListFrame::Push {
                        guard: HandleGuard::new(
                            list_ptr.as_mut_byte_ptr(),
                            *list_shape as *const Shape as *const (),
                            drop_shape_value,
                        ),
                    });
                }
                Ok(Control::CallBlockThen(*loop_id, Continuation::FinishList))
            }
            JsonOp::ListNext {
                list,
                element_program,
                element_scalar,
                element_option_scalar,
                element_layout,
                loop_id,
            } => loop {
                if let Some(scalar) = element_scalar {
                    return self.list_next_scalar(*list, *scalar, *element_layout);
                }

                if let Some(option_scalar) = element_option_scalar {
                    let step = self.parser.next_sequence_scalar_or_end()?;
                    let JsonSequenceScalarStep::Value { value } = step else {
                        return Ok(Control::Continue);
                    };

                    if let Some(slot) = self.direct_list_slot()? {
                        if value.is_null() {
                            unsafe {
                                (option_scalar.option.vtable.init_none)(slot);
                                self.mark_direct_list_slot_initialized();
                            }
                        } else {
                            let inner = self.scratch.reserve(option_scalar.inner_layout);
                            unsafe {
                                write_scalar_input(
                                    self.parser,
                                    option_scalar.option.t(),
                                    option_scalar.scalar,
                                    scratch_ptr_uninit(&inner),
                                    value,
                                )?;
                                (option_scalar.option.vtable.init_some)(
                                    slot,
                                    scratch_ptr_mut(&inner),
                                );
                                self.mark_direct_list_slot_initialized();
                            }
                            self.scratch.release(inner);
                        }
                        continue;
                    }

                    let scratch = self.scratch.reserve(*element_layout);
                    if value.is_null() {
                        unsafe {
                            (option_scalar.option.vtable.init_none)(scratch_ptr_uninit(&scratch));
                        }
                    } else {
                        let inner = self.scratch.reserve(option_scalar.inner_layout);
                        unsafe {
                            write_scalar_input(
                                self.parser,
                                option_scalar.option.t(),
                                option_scalar.scalar,
                                scratch_ptr_uninit(&inner),
                                value,
                            )?;
                            (option_scalar.option.vtable.init_some)(
                                scratch_ptr_uninit(&scratch),
                                scratch_ptr_mut(&inner),
                            );
                        }
                        self.scratch.release(inner);
                    }
                    self.push_list_element(*list, &scratch)?;
                    self.scratch.release(scratch);
                    continue;
                }

                if self.parser.consume_sequence_end_if_next()? {
                    return Ok(Control::Continue);
                }

                if let Some(slot) = self.direct_list_slot()? {
                    let old_base = self.base;
                    self.base = slot;
                    return Ok(call_program_or_block_then(
                        element_program,
                        Continuation::DirectListElement {
                            old_base,
                            loop_id: *loop_id,
                        },
                    ));
                }

                let scratch = self.scratch.reserve(*element_layout);
                let old_base = self.base;
                self.base = scratch_ptr_uninit(&scratch);
                return Ok(call_program_or_block_then(
                    element_program,
                    Continuation::PushedListElement {
                        list: *list,
                        old_base,
                        scratch,
                        loop_id: *loop_id,
                    },
                ));
            },
            JsonOp::ReadSet {
                set_shape,
                set,
                loop_id,
            } => {
                self.parser.consume_array_start_fast()?;
                let set_ptr = unsafe { (set.vtable.init_in_place_with_capacity)(self.base, 0) };
                self.sets.push(SetFrame {
                    guard: HandleGuard::new(
                        set_ptr.as_mut_byte_ptr(),
                        *set_shape as *const Shape as *const (),
                        drop_shape_value,
                    ),
                });
                Ok(Control::CallBlockThen(*loop_id, Continuation::FinishSet))
            }
            JsonOp::SetNext {
                set,
                element_program,
                element_scalar,
                element_option_scalar,
                element_layout,
                loop_id,
            } => self.step_set_next(SetStepPlan {
                set: *set,
                element_program,
                element_scalar: *element_scalar,
                element_option_scalar: *element_option_scalar,
                element_layout: *element_layout,
                loop_id: *loop_id,
            }),
            JsonOp::ReadMap {
                map_shape,
                map,
                loop_id,
            } => {
                if self.parser.consume_empty_array_if_next()? {
                    unsafe {
                        (map.vtable.init_in_place_with_capacity)(self.base, 0);
                    }
                    return Ok(Control::Continue);
                }

                self.parser.consume_object_start_fast()?;
                let map_ptr = unsafe { (map.vtable.init_in_place_with_capacity)(self.base, 0) };
                self.maps.push(MapFrame {
                    guard: HandleGuard::new(
                        map_ptr.as_mut_byte_ptr(),
                        *map_shape as *const Shape as *const (),
                        drop_shape_value,
                    ),
                });
                Ok(Control::CallBlockThen(*loop_id, Continuation::FinishMap))
            }
            JsonOp::MapNext {
                map,
                key_scalar,
                key_layout,
                value_program,
                value_scalar,
                value_layout,
                loop_id,
            } => self.step_map_next(MapStepPlan {
                map: *map,
                key_scalar: *key_scalar,
                key_layout: *key_layout,
                value_program,
                value_scalar: *value_scalar,
                value_layout: *value_layout,
                loop_id: *loop_id,
            }),
            JsonOp::ReadPointer {
                pointer,
                pointee_program,
                pointee_layout,
            } => {
                pointer
                    .pointee()
                    .ok_or_else(|| unsupported_shape_message("opaque pointer"))?;
                let scratch = self.scratch.reserve(*pointee_layout);
                let old_base = self.base;
                self.base = scratch_ptr_uninit(&scratch);
                Ok(call_program_or_block_then(
                    pointee_program,
                    Continuation::Pointer {
                        pointer: *pointer,
                        pointer_ptr: old_base,
                        old_base,
                        scratch,
                    },
                ))
            }
        }
    }

    fn after_return(
        &mut self,
        continuation: Self::Continuation,
    ) -> Result<Control<'program, ExecBlock, ExecOp, Self::Continuation>, Self::Error> {
        match continuation {
            Continuation::FinishStruct { tracking } => {
                unsafe {
                    self.finish_struct_frame(tracking)?;
                }
                Ok(Control::Continue)
            }
            Continuation::FieldDone {
                tracking,
                index,
                span,
                old_base,
                loop_id,
            } => {
                self.mark_struct_field(tracking, index, span);
                self.base = old_base;
                Ok(Control::CallBlock(loop_id))
            }
            Continuation::ExternalEnumSingleField {
                tracking,
                index,
                span,
                old_base,
                close_object,
            } => {
                self.mark_struct_field(tracking, index, span);
                self.base = old_base;
                self.finish_external_enum_payload(tracking, close_object)
            }
            Continuation::ExternalEnumTupleField {
                tracking,
                fields,
                index,
                span,
                old_base,
                close_object,
            } => {
                self.mark_struct_field(tracking, index, span);
                self.base = old_base;
                self.read_external_enum_tuple_next(fields, tracking, index + 1, close_object)
            }
            Continuation::ExternalEnumStruct { tracking } => {
                self.consume_external_enum_object_end()?;
                unsafe {
                    self.finish_struct_frame(tracking)?;
                }
                Ok(Control::Continue)
            }
            Continuation::OptionSome {
                option,
                option_ptr,
                old_base,
                scratch,
            } => {
                unsafe {
                    (option.vtable.init_some)(option_ptr, scratch_ptr_mut(&scratch));
                }
                self.scratch.release(scratch);
                self.base = old_base;
                Ok(Control::Continue)
            }
            Continuation::FinishList => {
                let list = self
                    .lists
                    .pop()
                    .expect("list frame is present after list program");
                list.finish()?;
                Ok(Control::Continue)
            }
            Continuation::FinishSet => {
                let set = self
                    .sets
                    .pop()
                    .expect("set frame is present after set program");
                set.finish()?;
                Ok(Control::Continue)
            }
            Continuation::FinishMap => {
                let map = self
                    .maps
                    .pop()
                    .expect("map frame is present after map program");
                map.finish()?;
                Ok(Control::Continue)
            }
            Continuation::MapValueDone {
                map,
                key_scalar,
                key_layout,
                key,
                key_span,
                value_scratch,
                old_base,
                loop_id,
            } => {
                self.insert_map_entry(map, key_scalar, key_layout, key, key_span, value_scratch)?;
                self.base = old_base;
                Ok(Control::CallBlock(loop_id))
            }
            Continuation::PushedListElement {
                list,
                old_base,
                scratch,
                loop_id,
            } => {
                self.push_list_element(list, &scratch)?;
                self.scratch.release(scratch);
                self.base = old_base;
                Ok(Control::CallBlock(loop_id))
            }
            Continuation::InsertedSetElement {
                set,
                old_base,
                scratch,
                loop_id,
            } => {
                self.insert_set_element(set, &scratch);
                self.scratch.release(scratch);
                self.base = old_base;
                Ok(Control::CallBlock(loop_id))
            }
            Continuation::DirectListElement { old_base, loop_id } => {
                unsafe {
                    self.mark_direct_list_slot_initialized();
                }
                self.base = old_base;
                Ok(Control::CallBlock(loop_id))
            }
            Continuation::Pointer {
                pointer,
                pointer_ptr,
                old_base,
                scratch,
            } => {
                let new_into = pointer
                    .vtable
                    .new_into_fn
                    .ok_or_else(|| unsupported_shape_message("pointer without new_into"))?;
                unsafe {
                    new_into(pointer_ptr, scratch_ptr_mut(&scratch));
                }
                self.scratch.release(scratch);
                self.base = old_base;
                Ok(Control::Continue)
            }
        }
    }
}

fn call_program_or_block_then<'program>(
    program: &'program [ExecOp],
    continuation: Continuation<'program>,
) -> Control<'program, ExecBlock, ExecOp, Continuation<'program>> {
    match program {
        [JsonOp::CallBlock(block)] => Control::CallBlockThen(*block, continuation),
        _ => Control::CallProgramThen(program, continuation),
    }
}

enum Continuation<'program> {
    FinishStruct {
        tracking: StructTracking,
    },
    FieldDone {
        tracking: StructTracking,
        index: usize,
        span: Span,
        old_base: PtrUninit,
        loop_id: ExecBlock,
    },
    ExternalEnumSingleField {
        tracking: StructTracking,
        index: usize,
        span: Span,
        old_base: PtrUninit,
        close_object: bool,
    },
    ExternalEnumTupleField {
        tracking: StructTracking,
        fields: &'program [FieldPlan<ExecBlock>],
        index: usize,
        span: Span,
        old_base: PtrUninit,
        close_object: bool,
    },
    ExternalEnumStruct {
        tracking: StructTracking,
    },
    OptionSome {
        option: OptionDef,
        option_ptr: PtrUninit,
        old_base: PtrUninit,
        scratch: ScratchSlot,
    },
    FinishList,
    FinishSet,
    FinishMap,
    MapValueDone {
        map: MapDef,
        key_scalar: ScalarType,
        key_layout: Layout,
        key: String,
        key_span: Span,
        value_scratch: ScratchSlot,
        old_base: PtrUninit,
        loop_id: ExecBlock,
    },
    PushedListElement {
        list: ListDef,
        old_base: PtrUninit,
        scratch: ScratchSlot,
        loop_id: ExecBlock,
    },
    InsertedSetElement {
        set: SetDef,
        old_base: PtrUninit,
        scratch: ScratchSlot,
        loop_id: ExecBlock,
    },
    DirectListElement {
        old_base: PtrUninit,
        loop_id: ExecBlock,
    },
    Pointer {
        pointer: PointerDef,
        pointer_ptr: PtrUninit,
        old_base: PtrUninit,
        scratch: ScratchSlot,
    },
}

struct MatchedField<'program, Field> {
    index: usize,
    field: &'program Field,
    ordered: bool,
}

const TINY_SCALAR_STRUCT_MAX_FIELDS: usize = 3;

fn tiny_i32_struct_fields_are_fusible(fields: &[ScalarFieldPlan]) -> bool {
    !fields.is_empty()
        && fields.len() <= TINY_SCALAR_STRUCT_MAX_FIELDS
        && fields.iter().all(|field| {
            field.alias.is_none()
                && field.scalar.scalar == ScalarType::I32
                && matches!(field.missing, MissingField::Required)
        })
}

#[cfg(all(
    feature = "jit",
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
fn tiny_f64_struct_fields_are_fusible(fields: &[ScalarFieldPlan]) -> bool {
    !fields.is_empty()
        && fields.len() <= TINY_SCALAR_STRUCT_MAX_FIELDS
        && fields.iter().all(|field| {
            field.alias.is_none()
                && field.scalar.scalar == ScalarType::F64
                && matches!(field.name.as_bytes(), [_])
                && matches!(field.missing, MissingField::Required)
        })
}

struct TinyScalarStructFrame<'program> {
    shape: &'static Shape,
    base: PtrUninit,
    fields: &'program [ScalarFieldPlan],
    initialized: u8,
    spans: [Span; TINY_SCALAR_STRUCT_MAX_FIELDS],
    next_field: usize,
}

struct StructFrame<'program, Field: StructFieldPlan, Seen: StructSeenStore> {
    shape: &'static Shape,
    base: PtrUninit,
    fields: &'program [Field],
    dispatch: Option<&'program RawFieldDispatch>,
    seen: Seen,
    next_field: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum StructTracking {
    Inline,
    Bitset,
    Heap,
}

impl StructTracking {
    fn for_len(len: usize) -> Self {
        if len <= 8 {
            Self::Inline
        } else if len <= u64::BITS as usize {
            Self::Bitset
        } else {
            Self::Heap
        }
    }
}

enum LargeStructFrameSlot<'program> {
    Bitset(StructFrame<'program, FieldPlan<ExecBlock>, BitsetStructSeen>),
    Heap(StructFrame<'program, FieldPlan<ExecBlock>, HeapStructSeen>),
}

struct LargeStructStack<'program> {
    frames: Vec<LargeStructFrameSlot<'program>>,
}

impl<'program> LargeStructStack<'program> {
    fn new() -> Self {
        Self { frames: Vec::new() }
    }

    fn push(&mut self, frame: LargeStructFrameSlot<'program>) {
        self.frames.push(frame);
    }

    fn pop(&mut self) -> Option<LargeStructFrameSlot<'program>> {
        self.frames.pop()
    }

    fn last(&self) -> Option<&LargeStructFrameSlot<'program>> {
        self.frames.last()
    }

    fn last_mut(&mut self) -> Option<&mut LargeStructFrameSlot<'program>> {
        self.frames.last_mut()
    }
}

struct BitsetStructSeen {
    initialized: u64,
    spans: Vec<Span>,
}

struct HeapStructSeen(Vec<Option<Span>>);

trait StructSeenStore {
    fn new(len: usize) -> Self;
    fn is_initialized(&self, index: usize) -> bool;
    fn get(&self, index: usize) -> Option<Span>;
    fn mark(&mut self, index: usize, span: Span);
}

impl StructSeenStore for InitializedLedger<Span> {
    #[inline]
    fn new(len: usize) -> Self {
        InitializedLedger::new(len)
    }

    #[inline(always)]
    fn is_initialized(&self, index: usize) -> bool {
        InitializedLedger::is_initialized(self, index)
    }

    #[inline(always)]
    fn get(&self, index: usize) -> Option<Span> {
        InitializedLedger::get(self, index).copied()
    }

    #[inline(always)]
    fn mark(&mut self, index: usize, span: Span) {
        InitializedLedger::mark(self, index, span);
    }
}

impl StructSeenStore for BitsetStructSeen {
    #[inline]
    fn new(len: usize) -> Self {
        Self {
            initialized: 0,
            spans: (0..len).map(|_| Span::default()).collect(),
        }
    }

    #[inline(always)]
    fn is_initialized(&self, index: usize) -> bool {
        assert!(index < self.spans.len(), "struct field index out of bounds");
        (self.initialized & struct_seen_bit(index)) != 0
    }

    #[inline(always)]
    fn get(&self, index: usize) -> Option<Span> {
        assert!(index < self.spans.len(), "struct field index out of bounds");
        ((self.initialized & struct_seen_bit(index)) != 0).then_some(self.spans[index])
    }

    #[inline(always)]
    fn mark(&mut self, index: usize, span: Span) {
        assert!(index < self.spans.len(), "struct field index out of bounds");
        self.spans[index] = span;
        self.initialized |= struct_seen_bit(index);
    }
}

impl StructSeenStore for HeapStructSeen {
    #[inline]
    fn new(len: usize) -> Self {
        Self((0..len).map(|_| None).collect())
    }

    #[inline(always)]
    fn is_initialized(&self, index: usize) -> bool {
        self.0[index].is_some()
    }

    #[inline(always)]
    fn get(&self, index: usize) -> Option<Span> {
        self.0[index]
    }

    #[inline(always)]
    fn mark(&mut self, index: usize, span: Span) {
        self.0[index] = Some(span);
    }
}

impl<'program> LargeStructFrameSlot<'program> {
    fn new(
        shape: &'static Shape,
        base: PtrUninit,
        fields: &'program [FieldPlan<ExecBlock>],
        dispatch: Option<&'program RawFieldDispatch>,
    ) -> Self {
        if fields.len() <= u64::BITS as usize {
            Self::Bitset(StructFrame::new(shape, base, fields, dispatch))
        } else {
            Self::Heap(StructFrame::new(shape, base, fields, dispatch))
        }
    }

    #[inline(always)]
    fn match_field_input<'de, const TRUSTED_UTF8: bool>(
        &self,
        parser: &JsonParser<'de, TRUSTED_UTF8>,
        key: &JsonFieldKeyInput<'de>,
    ) -> Result<Option<MatchedField<'program, FieldPlan<ExecBlock>>>, ParseError> {
        match self {
            Self::Bitset(frame) => frame.match_field_input(parser, key),
            Self::Heap(frame) => frame.match_field_input(parser, key),
        }
    }

    #[inline(always)]
    fn seen_span(&self, index: usize) -> Option<Span> {
        match self {
            Self::Bitset(frame) => frame.seen.get(index),
            Self::Heap(frame) => frame.seen.get(index),
        }
    }

    #[inline(always)]
    unsafe fn field_uninit(&self, offset: usize) -> PtrUninit {
        match self {
            Self::Bitset(frame) => unsafe { frame.base.field_uninit(offset) },
            Self::Heap(frame) => unsafe { frame.base.field_uninit(offset) },
        }
    }

    #[inline(always)]
    fn mark(&mut self, index: usize, span: Span) {
        match self {
            Self::Bitset(frame) => frame.mark_seen(index, span),
            Self::Heap(frame) => frame.mark_seen(index, span),
        }
    }

    unsafe fn fill_missing_fields(self) -> Result<(), DeserializeError> {
        match self {
            Self::Bitset(frame) => unsafe { frame.fill_missing_fields() },
            Self::Heap(frame) => unsafe { frame.fill_missing_fields() },
        }
    }
}

fn struct_seen_bit(index: usize) -> u64 {
    1u64 << index
}

impl<'program> TinyScalarStructFrame<'program> {
    fn new(shape: &'static Shape, base: PtrUninit, fields: &'program [ScalarFieldPlan]) -> Self {
        debug_assert!(fields.len() <= TINY_SCALAR_STRUCT_MAX_FIELDS);
        Self {
            shape,
            base,
            fields,
            initialized: 0,
            spans: [Span { offset: 0, len: 0 }; TINY_SCALAR_STRUCT_MAX_FIELDS],
            next_field: 0,
        }
    }

    #[inline(always)]
    fn is_initialized(&self, index: usize) -> bool {
        debug_assert!(index < TINY_SCALAR_STRUCT_MAX_FIELDS);
        (self.initialized & tiny_struct_seen_bit(index)) != 0
    }

    #[inline(always)]
    fn seen_span(&self, index: usize) -> Option<Span> {
        self.is_initialized(index).then_some(self.spans[index])
    }

    #[inline(always)]
    fn all_initialized(&self) -> bool {
        self.initialized == self.complete_mask()
    }

    #[inline(always)]
    fn complete_mask(&self) -> u8 {
        (1u8 << self.fields.len()) - 1
    }

    #[inline(always)]
    fn mark_seen(&mut self, index: usize, span: Span) {
        debug_assert!(index < self.fields.len());
        self.spans[index] = span;
        self.initialized |= tiny_struct_seen_bit(index);
        if index == self.next_field {
            self.advance_next_field();
        }
    }

    #[inline(always)]
    fn advance_next_field(&mut self) {
        while self
            .fields
            .get(self.next_field)
            .is_some_and(|_| self.is_initialized(self.next_field))
        {
            self.next_field += 1;
        }
    }

    #[inline(always)]
    fn match_next_field_input<'de, const TRUSTED_UTF8: bool>(
        &self,
        parser: &JsonParser<'de, TRUSTED_UTF8>,
        key: &JsonFieldKeyInput<'de>,
    ) -> Result<Option<(usize, ScalarFieldPlan)>, ParseError> {
        let Some(field) = self.fields.get(self.next_field).copied() else {
            return Ok(None);
        };
        if let Some(key) = parser.field_key_unescaped_bytes(key) {
            return Ok(field
                .matches_key_bytes(key)
                .then_some((self.next_field, field)));
        }
        if field.matches_key_input(parser, key)? {
            Ok(Some((self.next_field, field)))
        } else {
            Ok(None)
        }
    }

    #[inline]
    fn match_unordered_field_input<'de, const TRUSTED_UTF8: bool>(
        &self,
        parser: &JsonParser<'de, TRUSTED_UTF8>,
        key: &JsonFieldKeyInput<'de>,
    ) -> Result<Option<(usize, ScalarFieldPlan)>, ParseError> {
        if let Some(key) = parser.field_key_unescaped_bytes(key) {
            return Ok(self.match_unordered_field_bytes(key));
        }

        self.fields
            .iter()
            .copied()
            .enumerate()
            .filter(|(index, _)| *index != self.next_field)
            .find_map(
                |(index, field)| match field.matches_key_input(parser, key) {
                    Ok(true) => Some(Ok((index, field))),
                    Ok(false) => None,
                    Err(err) => Some(Err(err)),
                },
            )
            .transpose()
    }

    #[inline]
    fn match_unordered_field_bytes(&self, key: &[u8]) -> Option<(usize, ScalarFieldPlan)> {
        self.fields
            .iter()
            .copied()
            .enumerate()
            .filter(|(index, _)| *index != self.next_field)
            .find(|(_, field)| field.matches_key_bytes(key))
    }

    unsafe fn fill_missing_fields(mut self) -> Result<(), DeserializeError> {
        for (index, field) in self.fields.iter().copied().enumerate() {
            if self.is_initialized(index) {
                continue;
            }

            let field_ptr = unsafe { self.base.field_uninit(field.offset) };
            match field.missing {
                MissingField::Required => {
                    return Err(vm_error(
                        None,
                        DeserializeErrorKind::MissingField {
                            field: field.name,
                            container_shape: self.shape,
                        },
                    ));
                }
                MissingField::DefaultCustom(default) => {
                    unsafe {
                        default(field_ptr);
                    }
                    self.mark_seen(index, Span { offset: 0, len: 0 });
                }
                MissingField::DefaultTrait { explicit } => {
                    if unsafe { field.shape.call_default_in_place(field_ptr) }.is_some() {
                        self.mark_seen(index, Span { offset: 0, len: 0 });
                    } else if explicit {
                        return Err(vm_error(
                            None,
                            DeserializeErrorKind::Unsupported {
                                message: format!(
                                    "field `{}` on {} has #[facet(default)] but no default_in_place",
                                    field.name, self.shape
                                )
                                .into(),
                            },
                        ));
                    } else {
                        return Err(vm_error(
                            None,
                            DeserializeErrorKind::MissingField {
                                field: field.name,
                                container_shape: self.shape,
                            },
                        ));
                    }
                }
                MissingField::OptionNone(option) => {
                    unsafe {
                        (option.vtable.init_none)(field_ptr);
                    }
                    self.mark_seen(index, Span { offset: 0, len: 0 });
                }
            }
        }
        core::mem::forget(self);
        Ok(())
    }

    unsafe fn drop_initialized_fields(&self) {
        for index in (0..self.fields.len()).rev() {
            if self.is_initialized(index) {
                let field = self.fields[index];
                let ptr = unsafe { self.base.field_init(field.offset) };
                unsafe {
                    let _ = field.shape.call_drop_in_place(ptr);
                }
            }
        }
    }
}

impl Drop for TinyScalarStructFrame<'_> {
    fn drop(&mut self) {
        unsafe {
            self.drop_initialized_fields();
        }
    }
}

fn tiny_struct_seen_bit(index: usize) -> u8 {
    1u8 << index
}

impl<'program, Field, Seen> StructFrame<'program, Field, Seen>
where
    Field: StructFieldPlan,
    Seen: StructSeenStore,
{
    fn new(
        shape: &'static Shape,
        base: PtrUninit,
        fields: &'program [Field],
        dispatch: Option<&'program RawFieldDispatch>,
    ) -> Self {
        Self {
            shape,
            base,
            fields,
            dispatch,
            seen: Seen::new(fields.len()),
            next_field: 0,
        }
    }

    fn match_field_input<'de, const TRUSTED_UTF8: bool>(
        &self,
        parser: &JsonParser<'de, TRUSTED_UTF8>,
        key: &JsonFieldKeyInput<'de>,
    ) -> Result<Option<MatchedField<'program, Field>>, ParseError> {
        if let Some(key) = parser.field_key_unescaped_bytes(key) {
            return Ok(self.match_field_bytes(key));
        }

        if let Some(field) = self.fields.get(self.next_field)
            && field.matches_key_input(parser, key)?
        {
            return Ok(Some(MatchedField {
                index: self.next_field,
                field,
                ordered: true,
            }));
        }

        let matched = self
            .fields
            .iter()
            .enumerate()
            .find_map(
                |(index, field)| match field.matches_key_input(parser, key) {
                    Ok(true) => Some(Ok(MatchedField {
                        index,
                        field,
                        ordered: false,
                    })),
                    Ok(false) => None,
                    Err(err) => Some(Err(err)),
                },
            )
            .transpose()?;

        Ok(matched)
    }

    #[inline(always)]
    fn match_next_field_input<'de, const TRUSTED_UTF8: bool>(
        &self,
        parser: &JsonParser<'de, TRUSTED_UTF8>,
        key: &JsonFieldKeyInput<'de>,
    ) -> Result<Option<(usize, &'program Field)>, ParseError> {
        let Some(field) = self.fields.get(self.next_field) else {
            return Ok(None);
        };
        if let Some(key) = parser.field_key_unescaped_bytes(key) {
            return Ok(field
                .matches_key_bytes(key)
                .then_some((self.next_field, field)));
        }
        if field.matches_key_input(parser, key)? {
            Ok(Some((self.next_field, field)))
        } else {
            Ok(None)
        }
    }

    #[inline]
    fn match_unordered_field_input<'de, const TRUSTED_UTF8: bool>(
        &self,
        parser: &JsonParser<'de, TRUSTED_UTF8>,
        key: &JsonFieldKeyInput<'de>,
    ) -> Result<Option<MatchedField<'program, Field>>, ParseError> {
        if let Some(key) = parser.field_key_unescaped_bytes(key) {
            return Ok(self.match_unordered_field_bytes(key));
        }

        let matched = self
            .fields
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != self.next_field)
            .find_map(
                |(index, field)| match field.matches_key_input(parser, key) {
                    Ok(true) => Some(Ok(MatchedField {
                        index,
                        field,
                        ordered: false,
                    })),
                    Ok(false) => None,
                    Err(err) => Some(Err(err)),
                },
            )
            .transpose()?;

        Ok(matched)
    }

    fn match_field_bytes(&self, key: &[u8]) -> Option<MatchedField<'program, Field>> {
        if let Some(field) = self.fields.get(self.next_field)
            && field.matches_key_bytes(key)
        {
            return Some(MatchedField {
                index: self.next_field,
                field,
                ordered: true,
            });
        }

        self.match_unordered_field_bytes(key)
    }

    #[inline]
    fn match_unordered_field_bytes(&self, key: &[u8]) -> Option<MatchedField<'program, Field>> {
        if let Some(dispatch) = self.dispatch {
            let mut candidates = dispatch.candidates(key);
            while candidates != 0 {
                let index = candidates.trailing_zeros() as usize;
                candidates &= candidates - 1;
                if index == self.next_field {
                    continue;
                }
                let field = &self.fields[index];
                if field.matches_key_bytes(key) {
                    return Some(MatchedField {
                        index,
                        field,
                        ordered: false,
                    });
                }
            }
            return None;
        }

        self.fields
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != self.next_field)
            .find(|(_, field)| field.matches_key_bytes(key))
            .map(|(index, field)| MatchedField {
                index,
                field,
                ordered: false,
            })
    }

    #[inline(always)]
    fn mark_seen(&mut self, index: usize, span: Span) {
        self.seen.mark(index, span);
        if index == self.next_field {
            self.advance_next_field();
        }
    }

    #[inline(always)]
    fn advance_next_field(&mut self) {
        while self
            .fields
            .get(self.next_field)
            .is_some_and(|_| self.seen.is_initialized(self.next_field))
        {
            self.next_field += 1;
        }
    }

    unsafe fn fill_missing_fields(mut self) -> Result<(), DeserializeError> {
        for (index, field) in self.fields.iter().enumerate() {
            if self.seen.is_initialized(index) {
                continue;
            }

            let field_ptr = unsafe { self.base.field_uninit(field.offset()) };
            match field.missing() {
                MissingField::Required => {
                    return Err(vm_error(
                        None,
                        DeserializeErrorKind::MissingField {
                            field: field.name(),
                            container_shape: self.shape,
                        },
                    ));
                }
                MissingField::DefaultCustom(default) => {
                    unsafe {
                        default(field_ptr);
                    }
                    self.seen.mark(index, Span { offset: 0, len: 0 });
                }
                MissingField::DefaultTrait { explicit } => {
                    if unsafe { field.shape().call_default_in_place(field_ptr) }.is_some() {
                        self.seen.mark(index, Span { offset: 0, len: 0 });
                    } else if explicit {
                        return Err(vm_error(
                            None,
                            DeserializeErrorKind::Unsupported {
                                message: format!(
                                    "field `{}` on {} has #[facet(default)] but no default_in_place",
                                    field.name(), self.shape
                                )
                                .into(),
                            },
                        ));
                    } else {
                        return Err(vm_error(
                            None,
                            DeserializeErrorKind::MissingField {
                                field: field.name(),
                                container_shape: self.shape,
                            },
                        ));
                    }
                }
                MissingField::OptionNone(option) => {
                    unsafe {
                        (option.vtable.init_none)(field_ptr);
                    }
                    self.seen.mark(index, Span { offset: 0, len: 0 });
                }
            }
        }
        core::mem::forget(self);
        Ok(())
    }

    unsafe fn drop_initialized_fields(&self) {
        for index in (0..self.fields.len()).rev() {
            if self.seen.is_initialized(index) {
                let field = &self.fields[index];
                let ptr = unsafe { self.base.field_init(field.offset()) };
                unsafe {
                    let _ = field.shape().call_drop_in_place(ptr);
                }
            }
        }
    }
}

impl<'program, Seen> StructFrame<'program, ScalarFieldPlan, Seen>
where
    Seen: StructSeenStore,
{
    #[inline]
    fn match_unordered_scalar_field_bytes(
        &self,
        key: &[u8],
    ) -> Option<MatchedField<'program, ScalarFieldPlan>> {
        if self.dispatch.is_some() {
            return self.match_unordered_field_bytes(key);
        }

        if key.len() != 1 {
            return self.match_unordered_field_bytes(key);
        }

        let key = key[0];
        self.fields
            .iter()
            .enumerate()
            .filter(|(index, _)| *index != self.next_field)
            .find(|(_, field)| scalar_field_matches_single_byte_key(field, key))
            .map(|(index, field)| MatchedField {
                index,
                field,
                ordered: false,
            })
    }
}

#[inline(always)]
fn scalar_field_matches_single_byte_key(field: &ScalarFieldPlan, key: u8) -> bool {
    let name = field.name.as_bytes();
    (name.len() == 1 && name[0] == key)
        || field.alias.is_some_and(|alias| {
            let alias = alias.as_bytes();
            alias.len() == 1 && alias[0] == key
        })
}

impl<Field, Seen> Drop for StructFrame<'_, Field, Seen>
where
    Field: StructFieldPlan,
    Seen: StructSeenStore,
{
    fn drop(&mut self) {
        unsafe {
            self.drop_initialized_fields();
        }
    }
}

unsafe fn drop_shape_value(ctx: *const (), ptr: *mut u8) {
    let shape = unsafe { &*(ctx as *const Shape) };
    unsafe {
        let _ = shape.call_drop_in_place(PtrMut::new(ptr));
    }
}

fn scratch_ptr_uninit(scratch: &ScratchSlot) -> PtrUninit {
    PtrUninit::new(scratch.ptr())
}

unsafe fn scratch_ptr_mut(scratch: &ScratchSlot) -> PtrMut {
    unsafe { scratch_ptr_uninit(scratch).assume_init() }
}

unsafe fn write_map_key(
    shape: &'static Shape,
    scalar: ScalarType,
    dst: PtrUninit,
    key: String,
    span: Span,
) -> Result<(), DeserializeError> {
    macro_rules! write_parsed_key {
        ($ty:ty, $expected:literal) => {{
            let value = parse_map_key::<$ty>(&key, span, $expected)?;
            unsafe {
                dst.put(value);
            }
            Ok(())
        }};
    }

    match scalar {
        ScalarType::String => {
            unsafe {
                dst.put(key);
            }
            Ok(())
        }
        ScalarType::CowStr => {
            unsafe {
                dst.put::<Cow<'static, str>>(Cow::Owned(key));
            }
            Ok(())
        }
        ScalarType::I8 => write_parsed_key!(i8, "valid integer for map key"),
        ScalarType::I16 => write_parsed_key!(i16, "valid integer for map key"),
        ScalarType::I32 => write_parsed_key!(i32, "valid integer for map key"),
        ScalarType::I64 => write_parsed_key!(i64, "valid integer for map key"),
        ScalarType::I128 => write_parsed_key!(i128, "valid integer for map key"),
        ScalarType::ISize => write_parsed_key!(isize, "valid integer for map key"),
        ScalarType::U8 => write_parsed_key!(u8, "valid unsigned integer for map key"),
        ScalarType::U16 => write_parsed_key!(u16, "valid unsigned integer for map key"),
        ScalarType::U32 => write_parsed_key!(u32, "valid unsigned integer for map key"),
        ScalarType::U64 => write_parsed_key!(u64, "valid unsigned integer for map key"),
        ScalarType::U128 => write_parsed_key!(u128, "valid unsigned integer for map key"),
        ScalarType::USize => write_parsed_key!(usize, "valid unsigned integer for map key"),
        _ => Err(unsupported(shape, "string or integer map key")),
    }
}

fn parse_map_key<T>(key: &str, span: Span, expected: &'static str) -> Result<T, DeserializeError>
where
    T: FromStr,
{
    key.parse().map_err(|_| {
        vm_error(
            Some(span),
            DeserializeErrorKind::UnexpectedToken {
                expected,
                got: format!("string '{}'", key).into(),
            },
        )
    })
}

fn write_unit_input<'de, const TRUSTED_UTF8: bool>(
    _parser: &JsonParser<'de, TRUSTED_UTF8>,
    shape: &'static Shape,
    dst: PtrUninit,
    value: JsonScalarInput<'de>,
) -> Result<(), DeserializeError> {
    match value {
        JsonScalarInput::Raw(token) => match token.token {
            ScanToken::Null => unsafe {
                dst.put(());
                Ok(())
            },
            other => Err(type_mismatch(
                token.span,
                shape,
                raw_token_kind_name(&other),
            )),
        },
        JsonScalarInput::Materialized(value, span) => unsafe {
            write_unit_token(shape, dst, value, span)
        },
    }
}

fn write_bool_input<'de, const TRUSTED_UTF8: bool>(
    _parser: &JsonParser<'de, TRUSTED_UTF8>,
    shape: &'static Shape,
    dst: PtrUninit,
    value: JsonScalarInput<'de>,
) -> Result<(), DeserializeError> {
    match value {
        JsonScalarInput::Raw(token) => match token.token {
            ScanToken::True => unsafe {
                dst.put(true);
                Ok(())
            },
            ScanToken::False => unsafe {
                dst.put(false);
                Ok(())
            },
            other => Err(type_mismatch(
                token.span,
                shape,
                raw_token_kind_name(&other),
            )),
        },
        JsonScalarInput::Materialized(value, span) => unsafe {
            write_bool_token(shape, dst, value, span)
        },
    }
}

fn write_char_input<'de, const TRUSTED_UTF8: bool>(
    parser: &JsonParser<'de, TRUSTED_UTF8>,
    shape: &'static Shape,
    dst: PtrUninit,
    value: JsonScalarInput<'de>,
) -> Result<(), DeserializeError> {
    match value {
        JsonScalarInput::Raw(token) => {
            let span = token.span;
            let value = raw_string(parser, token, shape)?;
            let mut chars = value.chars();
            let Some(ch) = chars.next() else {
                return Err(invalid_value(span, "empty string is not a char"));
            };
            if chars.next().is_some() {
                return Err(invalid_value(span, "string has more than one char"));
            }
            unsafe {
                dst.put(ch);
            }
            Ok(())
        }
        JsonScalarInput::Materialized(value, span) => unsafe {
            write_char_token(shape, dst, value, span)
        },
    }
}

fn write_string_input<'de, const TRUSTED_UTF8: bool>(
    parser: &JsonParser<'de, TRUSTED_UTF8>,
    shape: &'static Shape,
    dst: PtrUninit,
    value: JsonScalarInput<'de>,
) -> Result<(), DeserializeError> {
    match value {
        JsonScalarInput::Raw(token) => {
            let value = raw_string_or_integer_string(parser, token, shape)?;
            unsafe {
                dst.put(value);
            }
            Ok(())
        }
        JsonScalarInput::Materialized(value, span) => unsafe {
            write_string_token(shape, dst, value, span)
        },
    }
}

fn write_cow_str_input<'de, const TRUSTED_UTF8: bool>(
    parser: &JsonParser<'de, TRUSTED_UTF8>,
    shape: &'static Shape,
    dst: PtrUninit,
    value: JsonScalarInput<'de>,
) -> Result<(), DeserializeError> {
    match value {
        JsonScalarInput::Raw(token) => {
            let value = raw_string(parser, token, shape)?;
            unsafe {
                dst.put::<Cow<'static, str>>(Cow::Owned(value.into_owned()));
            }
            Ok(())
        }
        JsonScalarInput::Materialized(value, span) => unsafe {
            write_cow_str_token(shape, dst, value, span)
        },
    }
}

fn write_borrowed_str_input<'de, const TRUSTED_UTF8: bool>(
    _parser: &JsonParser<'de, TRUSTED_UTF8>,
    _dst: PtrUninit,
    value: JsonScalarInput<'de>,
) -> Result<(), DeserializeError> {
    let span = match value {
        JsonScalarInput::Raw(token) => token.span,
        JsonScalarInput::Materialized(_, span) => span,
    };
    Err(vm_error(
        Some(span),
        DeserializeErrorKind::CannotBorrow {
            reason: "Weavy JSON owned deserializer does not support borrowed str yet".into(),
        },
    ))
}

fn write_f32_input<'de, const TRUSTED_UTF8: bool>(
    parser: &JsonParser<'de, TRUSTED_UTF8>,
    shape: &'static Shape,
    dst: PtrUninit,
    value: JsonScalarInput<'de>,
) -> Result<(), DeserializeError> {
    match value {
        JsonScalarInput::Raw(token) => {
            let span = token.span;
            unsafe {
                dst.put(raw_to_f64(parser, token, span, shape)? as f32);
            }
            Ok(())
        }
        JsonScalarInput::Materialized(value, span) => unsafe {
            write_f32_token(shape, dst, value, span)
        },
    }
}

fn write_f64_input<'de, const TRUSTED_UTF8: bool>(
    parser: &JsonParser<'de, TRUSTED_UTF8>,
    shape: &'static Shape,
    dst: PtrUninit,
    value: JsonScalarInput<'de>,
) -> Result<(), DeserializeError> {
    match value {
        JsonScalarInput::Raw(token) => {
            let span = token.span;
            unsafe {
                dst.put(raw_to_f64(parser, token, span, shape)?);
            }
            Ok(())
        }
        JsonScalarInput::Materialized(value, span) => unsafe {
            write_f64_token(shape, dst, value, span)
        },
    }
}

macro_rules! write_unsigned_input {
    ($name:ident, $ty:ty, $target:literal) => {
        fn $name<'de, const TRUSTED_UTF8: bool>(
            parser: &JsonParser<'de, TRUSTED_UTF8>,
            dst: PtrUninit,
            value: JsonScalarInput<'de>,
        ) -> Result<(), DeserializeError> {
            let value = match value {
                JsonScalarInput::Raw(token) => {
                    let span = token.span;
                    raw_into_unsigned::<$ty, TRUSTED_UTF8>(parser, token, span, $target)?
                }
                JsonScalarInput::Materialized(value, span) => {
                    into_unsigned::<$ty>(value, span, $target)?
                }
            };
            unsafe {
                dst.put(value);
            }
            Ok(())
        }
    };
}

macro_rules! write_signed_input {
    ($name:ident, $ty:ty, $target:literal) => {
        fn $name<'de, const TRUSTED_UTF8: bool>(
            parser: &JsonParser<'de, TRUSTED_UTF8>,
            dst: PtrUninit,
            value: JsonScalarInput<'de>,
        ) -> Result<(), DeserializeError> {
            let value = match value {
                JsonScalarInput::Raw(token) => {
                    let span = token.span;
                    raw_into_signed::<$ty, TRUSTED_UTF8>(parser, token, span, $target)?
                }
                JsonScalarInput::Materialized(value, span) => {
                    into_signed::<$ty>(value, span, $target)?
                }
            };
            unsafe {
                dst.put(value);
            }
            Ok(())
        }
    };
}

write_unsigned_input!(write_u8_input, u8, "u8");
write_unsigned_input!(write_u16_input, u16, "u16");
write_unsigned_input!(write_u32_input, u32, "u32");
write_unsigned_input!(write_u64_input, u64, "u64");
write_unsigned_input!(write_u128_input, u128, "u128");
write_unsigned_input!(write_usize_input, usize, "usize");

write_signed_input!(write_i8_input, i8, "i8");
write_signed_input!(write_i16_input, i16, "i16");
write_signed_input!(write_i64_input, i64, "i64");
write_signed_input!(write_i128_input, i128, "i128");
write_signed_input!(write_isize_input, isize, "isize");

fn write_i32_input<'de, const TRUSTED_UTF8: bool>(
    parser: &JsonParser<'de, TRUSTED_UTF8>,
    dst: PtrUninit,
    value: JsonScalarInput<'de>,
) -> Result<(), DeserializeError> {
    let value = match value {
        JsonScalarInput::Raw(token) => {
            let span = token.span;
            raw_into_i32::<TRUSTED_UTF8>(parser, token, span)?
        }
        JsonScalarInput::Materialized(value, span) => into_signed::<i32>(value, span, "i32")?,
    };
    unsafe {
        dst.put(value);
    }
    Ok(())
}

fn write_borrowed_str_input_shaped<'de, const TRUSTED_UTF8: bool>(
    parser: &JsonParser<'de, TRUSTED_UTF8>,
    _shape: &'static Shape,
    dst: PtrUninit,
    value: JsonScalarInput<'de>,
) -> Result<(), DeserializeError> {
    write_borrowed_str_input(parser, dst, value)
}

macro_rules! shaped_input_writer {
    ($name:ident, $write:ident) => {
        fn $name<'de, const TRUSTED_UTF8: bool>(
            parser: &JsonParser<'de, TRUSTED_UTF8>,
            _shape: &'static Shape,
            dst: PtrUninit,
            value: JsonScalarInput<'de>,
        ) -> Result<(), DeserializeError> {
            $write(parser, dst, value)
        }
    };
}

shaped_input_writer!(write_u8_input_shaped, write_u8_input);
shaped_input_writer!(write_u16_input_shaped, write_u16_input);
shaped_input_writer!(write_u32_input_shaped, write_u32_input);
shaped_input_writer!(write_u64_input_shaped, write_u64_input);
shaped_input_writer!(write_u128_input_shaped, write_u128_input);
shaped_input_writer!(write_usize_input_shaped, write_usize_input);

shaped_input_writer!(write_i8_input_shaped, write_i8_input);
shaped_input_writer!(write_i16_input_shaped, write_i16_input);
shaped_input_writer!(write_i32_input_shaped, write_i32_input);
shaped_input_writer!(write_i64_input_shaped, write_i64_input);
shaped_input_writer!(write_i128_input_shaped, write_i128_input);
shaped_input_writer!(write_isize_input_shaped, write_isize_input);

fn scalar_input_writer<const TRUSTED_UTF8: bool>(
    scalar: ScalarType,
) -> Option<ScalarInputWriter<TRUSTED_UTF8>> {
    match scalar {
        ScalarType::Unit => Some(write_unit_input::<TRUSTED_UTF8>),
        ScalarType::Bool => Some(write_bool_input::<TRUSTED_UTF8>),
        ScalarType::Char => Some(write_char_input::<TRUSTED_UTF8>),
        ScalarType::String => Some(write_string_input::<TRUSTED_UTF8>),
        ScalarType::CowStr => Some(write_cow_str_input::<TRUSTED_UTF8>),
        ScalarType::Str => Some(write_borrowed_str_input_shaped::<TRUSTED_UTF8>),
        ScalarType::F32 => Some(write_f32_input::<TRUSTED_UTF8>),
        ScalarType::F64 => Some(write_f64_input::<TRUSTED_UTF8>),
        ScalarType::U8 => Some(write_u8_input_shaped::<TRUSTED_UTF8>),
        ScalarType::U16 => Some(write_u16_input_shaped::<TRUSTED_UTF8>),
        ScalarType::U32 => Some(write_u32_input_shaped::<TRUSTED_UTF8>),
        ScalarType::U64 => Some(write_u64_input_shaped::<TRUSTED_UTF8>),
        ScalarType::U128 => Some(write_u128_input_shaped::<TRUSTED_UTF8>),
        ScalarType::USize => Some(write_usize_input_shaped::<TRUSTED_UTF8>),
        ScalarType::I8 => Some(write_i8_input_shaped::<TRUSTED_UTF8>),
        ScalarType::I16 => Some(write_i16_input_shaped::<TRUSTED_UTF8>),
        ScalarType::I32 => Some(write_i32_input_shaped::<TRUSTED_UTF8>),
        ScalarType::I64 => Some(write_i64_input_shaped::<TRUSTED_UTF8>),
        ScalarType::I128 => Some(write_i128_input_shaped::<TRUSTED_UTF8>),
        ScalarType::ISize => Some(write_isize_input_shaped::<TRUSTED_UTF8>),
        _ => None,
    }
}

fn materialized_scalar_writer(scalar: ScalarType) -> Option<MaterializedScalarWriter> {
    match scalar {
        ScalarType::Unit => Some(write_unit_token),
        ScalarType::Bool => Some(write_bool_token),
        ScalarType::Char => Some(write_char_token),
        ScalarType::String => Some(write_string_token),
        ScalarType::CowStr => Some(write_cow_str_token),
        ScalarType::Str => Some(write_borrowed_str_token),
        ScalarType::F32 => Some(write_f32_token),
        ScalarType::F64 => Some(write_f64_token),
        ScalarType::U8 => Some(write_u8_token),
        ScalarType::U16 => Some(write_u16_token),
        ScalarType::U32 => Some(write_u32_token),
        ScalarType::U64 => Some(write_u64_token),
        ScalarType::U128 => Some(write_u128_token),
        ScalarType::USize => Some(write_usize_token),
        ScalarType::I8 => Some(write_i8_token),
        ScalarType::I16 => Some(write_i16_token),
        ScalarType::I32 => Some(write_i32_token),
        ScalarType::I64 => Some(write_i64_token),
        ScalarType::I128 => Some(write_i128_token),
        ScalarType::ISize => Some(write_isize_token),
        _ => None,
    }
}

unsafe fn write_unit_token(
    shape: &'static Shape,
    dst: PtrUninit,
    value: JsonScalarToken<'_>,
    span: Span,
) -> Result<(), DeserializeError> {
    match value {
        JsonScalarToken::Null => unsafe {
            dst.put(());
            Ok(())
        },
        other => Err(type_mismatch(span, shape, other.kind_name())),
    }
}

unsafe fn write_bool_token(
    shape: &'static Shape,
    dst: PtrUninit,
    value: JsonScalarToken<'_>,
    span: Span,
) -> Result<(), DeserializeError> {
    match value {
        JsonScalarToken::Bool(value) => unsafe {
            dst.put(value);
            Ok(())
        },
        other => Err(type_mismatch(span, shape, other.kind_name())),
    }
}

unsafe fn write_char_token(
    shape: &'static Shape,
    dst: PtrUninit,
    value: JsonScalarToken<'_>,
    span: Span,
) -> Result<(), DeserializeError> {
    let JsonScalarToken::Str(value) = value else {
        return Err(type_mismatch(span, shape, value.kind_name()));
    };
    let mut chars = value.chars();
    let Some(ch) = chars.next() else {
        return Err(invalid_value(span, "empty string is not a char"));
    };
    if chars.next().is_some() {
        return Err(invalid_value(span, "string has more than one char"));
    }
    unsafe {
        dst.put(ch);
    }
    Ok(())
}

unsafe fn write_string_token(
    shape: &'static Shape,
    dst: PtrUninit,
    value: JsonScalarToken<'_>,
    span: Span,
) -> Result<(), DeserializeError> {
    let value = string_token_or_integer_string(value, span, shape)?;
    unsafe {
        dst.put(value);
    }
    Ok(())
}

unsafe fn write_cow_str_token(
    shape: &'static Shape,
    dst: PtrUninit,
    value: JsonScalarToken<'_>,
    span: Span,
) -> Result<(), DeserializeError> {
    let JsonScalarToken::Str(value) = value else {
        return Err(type_mismatch(span, shape, value.kind_name()));
    };
    unsafe {
        dst.put::<Cow<'static, str>>(Cow::Owned(value.into_owned()));
    }
    Ok(())
}

unsafe fn write_borrowed_str_token(
    _shape: &'static Shape,
    _dst: PtrUninit,
    _value: JsonScalarToken<'_>,
    span: Span,
) -> Result<(), DeserializeError> {
    Err(vm_error(
        Some(span),
        DeserializeErrorKind::CannotBorrow {
            reason: "Weavy JSON owned deserializer does not support borrowed str yet".into(),
        },
    ))
}

unsafe fn write_f32_token(
    shape: &'static Shape,
    dst: PtrUninit,
    value: JsonScalarToken<'_>,
    span: Span,
) -> Result<(), DeserializeError> {
    let value = scalar_to_f64(value, span, shape)?;
    unsafe {
        dst.put(value as f32);
    }
    Ok(())
}

unsafe fn write_f64_token(
    shape: &'static Shape,
    dst: PtrUninit,
    value: JsonScalarToken<'_>,
    span: Span,
) -> Result<(), DeserializeError> {
    let value = scalar_to_f64(value, span, shape)?;
    unsafe {
        dst.put(value);
    }
    Ok(())
}

macro_rules! write_unsigned_token {
    ($name:ident, $ty:ty, $target:literal) => {
        unsafe fn $name(
            _shape: &'static Shape,
            dst: PtrUninit,
            value: JsonScalarToken<'_>,
            span: Span,
        ) -> Result<(), DeserializeError> {
            let value = into_unsigned::<$ty>(value, span, $target)?;
            unsafe {
                dst.put(value);
            }
            Ok(())
        }
    };
}

macro_rules! write_signed_token {
    ($name:ident, $ty:ty, $target:literal) => {
        unsafe fn $name(
            _shape: &'static Shape,
            dst: PtrUninit,
            value: JsonScalarToken<'_>,
            span: Span,
        ) -> Result<(), DeserializeError> {
            let value = into_signed::<$ty>(value, span, $target)?;
            unsafe {
                dst.put(value);
            }
            Ok(())
        }
    };
}

write_unsigned_token!(write_u8_token, u8, "u8");
write_unsigned_token!(write_u16_token, u16, "u16");
write_unsigned_token!(write_u32_token, u32, "u32");
write_unsigned_token!(write_u64_token, u64, "u64");
write_unsigned_token!(write_u128_token, u128, "u128");
write_unsigned_token!(write_usize_token, usize, "usize");

write_signed_token!(write_i8_token, i8, "i8");
write_signed_token!(write_i16_token, i16, "i16");
write_signed_token!(write_i32_token, i32, "i32");
write_signed_token!(write_i64_token, i64, "i64");
write_signed_token!(write_i128_token, i128, "i128");
write_signed_token!(write_isize_token, isize, "isize");

unsafe fn write_scalar_input<const TRUSTED_UTF8: bool>(
    parser: &JsonParser<'_, TRUSTED_UTF8>,
    shape: &'static Shape,
    scalar: ScalarType,
    dst: PtrUninit,
    value: JsonScalarInput<'_>,
) -> Result<(), DeserializeError> {
    match value {
        JsonScalarInput::Raw(token) => unsafe {
            write_scalar_raw(parser, shape, scalar, dst, token)
        },
        JsonScalarInput::Materialized(value, span) => unsafe {
            write_scalar(shape, scalar, dst, value, span)
        },
    }
}

unsafe fn write_scalar_raw<const TRUSTED_UTF8: bool>(
    parser: &JsonParser<'_, TRUSTED_UTF8>,
    shape: &'static Shape,
    scalar: ScalarType,
    dst: PtrUninit,
    token: SpannedToken,
) -> Result<(), DeserializeError> {
    let span = token.span;
    match scalar {
        ScalarType::Unit => match token.token {
            ScanToken::Null => unsafe {
                dst.put(());
            },
            other => return Err(type_mismatch(span, shape, raw_token_kind_name(&other))),
        },
        ScalarType::Bool => match token.token {
            ScanToken::True => unsafe {
                dst.put(true);
            },
            ScanToken::False => unsafe {
                dst.put(false);
            },
            other => return Err(type_mismatch(span, shape, raw_token_kind_name(&other))),
        },
        ScalarType::Char => {
            let value = raw_string(parser, token, shape)?;
            let mut chars = value.chars();
            let Some(ch) = chars.next() else {
                return Err(invalid_value(span, "empty string is not a char"));
            };
            if chars.next().is_some() {
                return Err(invalid_value(span, "string has more than one char"));
            }
            unsafe {
                dst.put(ch);
            }
        }
        ScalarType::String => {
            let value = raw_string_or_integer_string(parser, token, shape)?;
            unsafe {
                dst.put(value);
            }
        }
        ScalarType::CowStr => {
            let value = raw_string(parser, token, shape)?;
            unsafe {
                dst.put::<Cow<'static, str>>(Cow::Owned(value.into_owned()));
            }
        }
        ScalarType::Str => {
            return Err(vm_error(
                Some(span),
                DeserializeErrorKind::CannotBorrow {
                    reason: "Weavy JSON owned deserializer does not support borrowed str yet"
                        .into(),
                },
            ));
        }
        ScalarType::F32 => unsafe {
            dst.put(raw_to_f64(parser, token, span, shape)? as f32);
        },
        ScalarType::F64 => unsafe {
            dst.put(raw_to_f64(parser, token, span, shape)?);
        },
        ScalarType::U8 => unsafe {
            dst.put(raw_into_unsigned::<u8, TRUSTED_UTF8>(
                parser, token, span, "u8",
            )?);
        },
        ScalarType::U16 => unsafe {
            dst.put(raw_into_unsigned::<u16, TRUSTED_UTF8>(
                parser, token, span, "u16",
            )?);
        },
        ScalarType::U32 => unsafe {
            dst.put(raw_into_unsigned::<u32, TRUSTED_UTF8>(
                parser, token, span, "u32",
            )?);
        },
        ScalarType::U64 => unsafe {
            dst.put(raw_into_unsigned::<u64, TRUSTED_UTF8>(
                parser, token, span, "u64",
            )?);
        },
        ScalarType::U128 => unsafe {
            dst.put(raw_into_unsigned::<u128, TRUSTED_UTF8>(
                parser, token, span, "u128",
            )?);
        },
        ScalarType::USize => unsafe {
            dst.put(raw_into_unsigned::<usize, TRUSTED_UTF8>(
                parser, token, span, "usize",
            )?);
        },
        ScalarType::I8 => unsafe {
            dst.put(raw_into_signed::<i8, TRUSTED_UTF8>(
                parser, token, span, "i8",
            )?);
        },
        ScalarType::I16 => unsafe {
            dst.put(raw_into_signed::<i16, TRUSTED_UTF8>(
                parser, token, span, "i16",
            )?);
        },
        ScalarType::I32 => unsafe {
            dst.put(raw_into_signed::<i32, TRUSTED_UTF8>(
                parser, token, span, "i32",
            )?);
        },
        ScalarType::I64 => unsafe {
            dst.put(raw_into_signed::<i64, TRUSTED_UTF8>(
                parser, token, span, "i64",
            )?);
        },
        ScalarType::I128 => unsafe {
            dst.put(raw_into_signed::<i128, TRUSTED_UTF8>(
                parser, token, span, "i128",
            )?);
        },
        ScalarType::ISize => unsafe {
            dst.put(raw_into_signed::<isize, TRUSTED_UTF8>(
                parser, token, span, "isize",
            )?);
        },
        ScalarType::ConstTypeId => {
            return Err(vm_error(
                Some(span),
                DeserializeErrorKind::Unsupported {
                    message: "Weavy JSON deserializer does not support ConstTypeId yet".into(),
                },
            ));
        }
        #[cfg(feature = "net")]
        ScalarType::SocketAddr
        | ScalarType::IpAddr
        | ScalarType::Ipv4Addr
        | ScalarType::Ipv6Addr => {
            let value = raw_string(parser, token, shape)?;
            match unsafe { shape.call_parse(value.as_ref(), dst) } {
                Some(Ok(())) => {}
                Some(Err(err)) => return Err(invalid_value(span, format!("{err}"))),
                None => return Err(unsupported(shape, "parsed scalar")),
            }
        }
        _ => {
            return Err(vm_error(
                Some(span),
                DeserializeErrorKind::Unsupported {
                    message: format!(
                        "Weavy JSON deserializer does not yet support scalar {scalar:?}"
                    )
                    .into(),
                },
            ));
        }
    }
    Ok(())
}

unsafe fn write_scalar(
    shape: &'static Shape,
    scalar: ScalarType,
    dst: PtrUninit,
    value: JsonScalarToken<'_>,
    span: Span,
) -> Result<(), DeserializeError> {
    match scalar {
        ScalarType::Unit => match value {
            JsonScalarToken::Null => {
                unsafe { dst.put(()) };
            }
            other => return Err(type_mismatch(span, shape, other.kind_name())),
        },
        ScalarType::Bool => match value {
            JsonScalarToken::Bool(value) => {
                unsafe { dst.put(value) };
            }
            other => return Err(type_mismatch(span, shape, other.kind_name())),
        },
        ScalarType::Char => match value {
            JsonScalarToken::Str(value) => {
                let mut chars = value.chars();
                let Some(ch) = chars.next() else {
                    return Err(invalid_value(span, "empty string is not a char"));
                };
                if chars.next().is_some() {
                    return Err(invalid_value(span, "string has more than one char"));
                }
                unsafe { dst.put(ch) };
            }
            other => return Err(type_mismatch(span, shape, other.kind_name())),
        },
        ScalarType::String => {
            let string = match value {
                JsonScalarToken::Str(value) => value.into_owned(),
                JsonScalarToken::I64(value) => value.to_string(),
                JsonScalarToken::U64(value) => value.to_string(),
                JsonScalarToken::I128(value) => value.to_string(),
                JsonScalarToken::U128(value) => value.to_string(),
                JsonScalarToken::F64(value) => value.to_string(),
                other => return Err(type_mismatch(span, shape, other.kind_name())),
            };
            unsafe { dst.put(string) };
        }
        ScalarType::CowStr => {
            let string = match value {
                JsonScalarToken::Str(value) => value.into_owned(),
                other => return Err(type_mismatch(span, shape, other.kind_name())),
            };
            unsafe { dst.put::<Cow<'static, str>>(Cow::Owned(string)) };
        }
        ScalarType::Str => {
            return Err(vm_error(
                Some(span),
                DeserializeErrorKind::CannotBorrow {
                    reason: "Weavy JSON owned deserializer does not support borrowed str yet"
                        .into(),
                },
            ));
        }
        ScalarType::F32 => {
            let value = scalar_to_f64(value, span, shape)?;
            unsafe { dst.put(value as f32) };
        }
        ScalarType::F64 => {
            let value = scalar_to_f64(value, span, shape)?;
            unsafe { dst.put(value) };
        }
        ScalarType::U8 => unsafe {
            dst.put(into_unsigned::<u8>(value, span, "u8")?);
        },
        ScalarType::U16 => unsafe {
            dst.put(into_unsigned::<u16>(value, span, "u16")?);
        },
        ScalarType::U32 => unsafe {
            dst.put(into_unsigned::<u32>(value, span, "u32")?);
        },
        ScalarType::U64 => unsafe {
            dst.put(into_unsigned::<u64>(value, span, "u64")?);
        },
        ScalarType::U128 => unsafe {
            dst.put(into_unsigned::<u128>(value, span, "u128")?);
        },
        ScalarType::USize => unsafe {
            dst.put(into_unsigned::<usize>(value, span, "usize")?);
        },
        ScalarType::I8 => unsafe {
            dst.put(into_signed::<i8>(value, span, "i8")?);
        },
        ScalarType::I16 => unsafe {
            dst.put(into_signed::<i16>(value, span, "i16")?);
        },
        ScalarType::I32 => unsafe {
            dst.put(into_signed::<i32>(value, span, "i32")?);
        },
        ScalarType::I64 => unsafe {
            dst.put(into_signed::<i64>(value, span, "i64")?);
        },
        ScalarType::I128 => unsafe {
            dst.put(into_signed::<i128>(value, span, "i128")?);
        },
        ScalarType::ISize => unsafe {
            dst.put(into_signed::<isize>(value, span, "isize")?);
        },
        ScalarType::ConstTypeId => {
            return Err(vm_error(
                Some(span),
                DeserializeErrorKind::Unsupported {
                    message: "Weavy JSON deserializer does not support ConstTypeId yet".into(),
                },
            ));
        }
        #[cfg(feature = "net")]
        ScalarType::SocketAddr
        | ScalarType::IpAddr
        | ScalarType::Ipv4Addr
        | ScalarType::Ipv6Addr => {
            let JsonScalarToken::Str(value) = value else {
                return Err(type_mismatch(span, shape, value.kind_name()));
            };
            match unsafe { shape.call_parse(value.as_ref(), dst) } {
                Some(Ok(())) => {}
                Some(Err(err)) => return Err(invalid_value(span, format!("{err}"))),
                None => return Err(unsupported(shape, "parsed scalar")),
            }
        }
        _ => {
            return Err(vm_error(
                Some(span),
                DeserializeErrorKind::Unsupported {
                    message: format!(
                        "Weavy JSON deserializer does not yet support scalar {scalar:?}"
                    )
                    .into(),
                },
            ));
        }
    }
    Ok(())
}

fn raw_token_kind_name(token: &ScanToken) -> &'static str {
    match token {
        ScanToken::Null => "null",
        ScanToken::True | ScanToken::False => "bool",
        ScanToken::String { .. } => "string",
        ScanToken::Number { hint, .. } => match hint {
            crate::scanner::NumberHint::Unsigned => "u64",
            crate::scanner::NumberHint::Signed => "i64",
            crate::scanner::NumberHint::Float => "f64",
        },
        ScanToken::ObjectStart => "object start",
        ScanToken::ObjectEnd => "object end",
        ScanToken::ArrayStart => "array start",
        ScanToken::ArrayEnd => "array end",
        ScanToken::Colon => "colon",
        ScanToken::Comma => "comma",
        ScanToken::Eof => "eof",
    }
}

fn parsed_number_kind_name(value: &ParsedNumber) -> &'static str {
    match value {
        ParsedNumber::U64(_) => "u64",
        ParsedNumber::I64(_) => "i64",
        ParsedNumber::U128(_) => "u128",
        ParsedNumber::I128(_) => "i128",
        ParsedNumber::F64(_) => "f64",
    }
}

fn raw_string<'de, const TRUSTED_UTF8: bool>(
    parser: &JsonParser<'de, TRUSTED_UTF8>,
    token: SpannedToken,
    shape: &'static Shape,
) -> Result<Cow<'de, str>, DeserializeError> {
    let span = token.span;
    match token.token {
        ScanToken::String {
            start,
            end,
            has_escapes,
        } => Ok(parser.decode_string(start, end, has_escapes, span)?),
        other => Err(type_mismatch(span, shape, raw_token_kind_name(&other))),
    }
}

fn raw_string_or_integer_string<const TRUSTED_UTF8: bool>(
    parser: &JsonParser<'_, TRUSTED_UTF8>,
    token: SpannedToken,
    shape: &'static Shape,
) -> Result<String, DeserializeError> {
    let span = token.span;
    match token.token {
        ScanToken::String {
            start,
            end,
            has_escapes,
        } => Ok(parser
            .decode_string(start, end, has_escapes, span)?
            .into_owned()),
        ScanToken::Number { start, end, hint } => {
            let number = parser.parse_number(start, end, hint)?;
            match number {
                ParsedNumber::I64(value) => Ok(value.to_string()),
                ParsedNumber::U64(value) => Ok(value.to_string()),
                ParsedNumber::I128(value) => Ok(value.to_string()),
                ParsedNumber::U128(value) => Ok(value.to_string()),
                ParsedNumber::F64(value) => Ok(value.to_string()),
            }
        }
        other => Err(type_mismatch(span, shape, raw_token_kind_name(&other))),
    }
}

fn string_token_or_integer_string(
    value: JsonScalarToken<'_>,
    span: Span,
    shape: &'static Shape,
) -> Result<String, DeserializeError> {
    match value {
        JsonScalarToken::Str(value) => Ok(value.into_owned()),
        JsonScalarToken::I64(value) => Ok(value.to_string()),
        JsonScalarToken::U64(value) => Ok(value.to_string()),
        JsonScalarToken::I128(value) => Ok(value.to_string()),
        JsonScalarToken::U128(value) => Ok(value.to_string()),
        JsonScalarToken::F64(value) => Ok(value.to_string()),
        other => Err(type_mismatch(span, shape, other.kind_name())),
    }
}

fn scalar_input_null_span(value: &JsonScalarInput<'_>) -> Option<Span> {
    match value {
        JsonScalarInput::Raw(token) if matches!(token.token, ScanToken::Null) => Some(token.span),
        JsonScalarInput::Materialized(JsonScalarToken::Null, span) => Some(*span),
        _ => None,
    }
}

unsafe fn write_default_from_null(
    shape: &'static Shape,
    dst: PtrUninit,
    span: Span,
) -> Result<(), DeserializeError> {
    if unsafe { shape.call_default_in_place(dst) }.is_some() {
        Ok(())
    } else {
        Err(type_mismatch(span, shape, "null"))
    }
}

fn raw_to_f64<const TRUSTED_UTF8: bool>(
    parser: &JsonParser<'_, TRUSTED_UTF8>,
    token: SpannedToken,
    span: Span,
    shape: &'static Shape,
) -> Result<f64, DeserializeError> {
    match token.token {
        ScanToken::Number { start, end, hint } => {
            let number = parser.parse_number(start, end, hint)?;
            match number {
                ParsedNumber::F64(value) => Ok(value),
                ParsedNumber::I64(value) => Ok(value as f64),
                ParsedNumber::U64(value) => Ok(value as f64),
                ParsedNumber::I128(value) => Ok(value as f64),
                ParsedNumber::U128(value) => Ok(value as f64),
            }
        }
        ScanToken::String {
            start,
            end,
            has_escapes,
        } => {
            let value = parser.decode_string(start, end, has_escapes, span)?;
            value
                .parse::<f64>()
                .map_err(|_| type_mismatch(span, shape, "string"))
        }
        other => Err(type_mismatch(span, shape, raw_token_kind_name(&other))),
    }
}

fn raw_into_unsigned<T, const TRUSTED_UTF8: bool>(
    parser: &JsonParser<'_, TRUSTED_UTF8>,
    token: SpannedToken,
    span: Span,
    target: &'static str,
) -> Result<T, DeserializeError>
where
    T: FromStr,
    T: TryFrom<u128>,
{
    let value = match token.token {
        ScanToken::Number { start, end, hint } => match hint {
            NumberHint::Unsigned => {
                let text = parser.number_text(start, end, span)?;
                if let Ok(value) = text.parse::<T>() {
                    return Ok(value);
                }
                parsed_unsigned_number(parser, start, end, hint, span, target)?
            }
            NumberHint::Signed | NumberHint::Float => {
                parsed_unsigned_number(parser, start, end, hint, span, target)?
            }
        },
        ScanToken::String {
            start,
            end,
            has_escapes,
        } => {
            let value = parser.decode_string(start, end, has_escapes, span)?;
            value
                .parse::<u128>()
                .map_err(|_| number_out_of_range(span, value.into_owned(), target))?
        }
        other => {
            return Err(type_mismatch_name(
                span,
                target,
                raw_token_kind_name(&other),
            ));
        }
    };
    T::try_from(value).map_err(|_| number_out_of_range(span, value.to_string(), target))
}

fn raw_into_signed<T, const TRUSTED_UTF8: bool>(
    parser: &JsonParser<'_, TRUSTED_UTF8>,
    token: SpannedToken,
    span: Span,
    target: &'static str,
) -> Result<T, DeserializeError>
where
    T: FromStr,
    T: TryFrom<i128>,
{
    let value = match token.token {
        ScanToken::Number { start, end, hint } => match hint {
            NumberHint::Signed | NumberHint::Unsigned => {
                let text = parser.number_text(start, end, span)?;
                if let Ok(value) = text.parse::<T>() {
                    return Ok(value);
                }
                parsed_signed_number(parser, start, end, hint, span, target)?
            }
            NumberHint::Float => parsed_signed_number(parser, start, end, hint, span, target)?,
        },
        ScanToken::String {
            start,
            end,
            has_escapes,
        } => {
            let value = parser.decode_string(start, end, has_escapes, span)?;
            value
                .parse::<i128>()
                .map_err(|_| number_out_of_range(span, value.into_owned(), target))?
        }
        other => {
            return Err(type_mismatch_name(
                span,
                target,
                raw_token_kind_name(&other),
            ));
        }
    };
    T::try_from(value).map_err(|_| number_out_of_range(span, value.to_string(), target))
}

fn raw_into_i32<const TRUSTED_UTF8: bool>(
    parser: &JsonParser<'_, TRUSTED_UTF8>,
    token: SpannedToken,
    span: Span,
) -> Result<i32, DeserializeError> {
    match token.token {
        ScanToken::Number { start, end, hint } => match hint {
            NumberHint::Signed | NumberHint::Unsigned => {
                let text = parser.number_text(start, end, span)?;
                if let Some(value) = parse_i32_bytes(text.as_bytes()) {
                    return Ok(value);
                }
                let value = parsed_signed_number(parser, start, end, hint, span, "i32")?;
                i32::try_from(value)
                    .map_err(|_| number_out_of_range(span, value.to_string(), "i32"))
            }
            NumberHint::Float => {
                let value = parsed_signed_number(parser, start, end, hint, span, "i32")?;
                i32::try_from(value)
                    .map_err(|_| number_out_of_range(span, value.to_string(), "i32"))
            }
        },
        ScanToken::String {
            start,
            end,
            has_escapes,
        } => {
            let value = parser.decode_string(start, end, has_escapes, span)?;
            value
                .parse::<i32>()
                .map_err(|_| number_out_of_range(span, value.into_owned(), "i32"))
        }
        other => Err(type_mismatch_name(span, "i32", raw_token_kind_name(&other))),
    }
}

fn parse_i32_bytes(bytes: &[u8]) -> Option<i32> {
    let (&first, rest) = bytes.split_first()?;
    let (negative, digits) = if first == b'-' {
        (true, rest)
    } else {
        (false, bytes)
    };
    if digits.is_empty() {
        return None;
    }

    let mut value = 0i64;
    for &byte in digits {
        if !byte.is_ascii_digit() {
            return None;
        }
        value = value.checked_mul(10)?.checked_add((byte - b'0') as i64)?;
    }

    if negative {
        let value = -value;
        i32::try_from(value).ok()
    } else {
        i32::try_from(value).ok()
    }
}

fn parsed_unsigned_number<const TRUSTED_UTF8: bool>(
    parser: &JsonParser<'_, TRUSTED_UTF8>,
    start: usize,
    end: usize,
    hint: NumberHint,
    span: Span,
    target: &'static str,
) -> Result<u128, DeserializeError> {
    let number = parser.parse_number(start, end, hint)?;
    match number {
        ParsedNumber::U64(value) => Ok(value as u128),
        ParsedNumber::U128(value) => Ok(value),
        ParsedNumber::I64(value) if value >= 0 => Ok(value as u128),
        ParsedNumber::I128(value) if value >= 0 => Ok(value as u128),
        ParsedNumber::F64(value) => f64_to_u128(value, span, target),
        other => Err(type_mismatch_name(
            span,
            target,
            parsed_number_kind_name(&other),
        )),
    }
}

fn parsed_signed_number<const TRUSTED_UTF8: bool>(
    parser: &JsonParser<'_, TRUSTED_UTF8>,
    start: usize,
    end: usize,
    hint: NumberHint,
    span: Span,
    target: &'static str,
) -> Result<i128, DeserializeError> {
    let number = parser.parse_number(start, end, hint)?;
    match number {
        ParsedNumber::I64(value) => Ok(value as i128),
        ParsedNumber::I128(value) => Ok(value),
        ParsedNumber::U64(value) => Ok(value as i128),
        ParsedNumber::U128(value) if value <= i128::MAX as u128 => Ok(value as i128),
        ParsedNumber::F64(value) => f64_to_i128(value, span, target),
        other => Err(type_mismatch_name(
            span,
            target,
            parsed_number_kind_name(&other),
        )),
    }
}

fn f64_to_u128(value: f64, span: Span, target: &'static str) -> Result<u128, DeserializeError> {
    let text = value.to_string();
    text.parse::<u128>()
        .map_err(|_| type_mismatch_name(span, target, "f64"))
}

fn f64_to_i128(value: f64, span: Span, target: &'static str) -> Result<i128, DeserializeError> {
    let text = value.to_string();
    text.parse::<i128>()
        .map_err(|_| type_mismatch_name(span, target, "f64"))
}

fn scalar_to_f64(
    value: JsonScalarToken<'_>,
    span: Span,
    shape: &'static Shape,
) -> Result<f64, DeserializeError> {
    match value {
        JsonScalarToken::F64(value) => Ok(value),
        JsonScalarToken::I64(value) => Ok(value as f64),
        JsonScalarToken::U64(value) => Ok(value as f64),
        JsonScalarToken::I128(value) => Ok(value as f64),
        JsonScalarToken::U128(value) => Ok(value as f64),
        JsonScalarToken::Str(value) => value
            .parse::<f64>()
            .map_err(|_| type_mismatch(span, shape, "string")),
        other => Err(type_mismatch(span, shape, other.kind_name())),
    }
}

fn into_unsigned<T>(
    value: JsonScalarToken<'_>,
    span: Span,
    target: &'static str,
) -> Result<T, DeserializeError>
where
    T: TryFrom<u128>,
{
    let value = match value {
        JsonScalarToken::U64(value) => value as u128,
        JsonScalarToken::U128(value) => value,
        JsonScalarToken::I64(value) if value >= 0 => value as u128,
        JsonScalarToken::I128(value) if value >= 0 => value as u128,
        JsonScalarToken::F64(value) => f64_to_u128(value, span, target)?,
        JsonScalarToken::Str(value) => value
            .parse::<u128>()
            .map_err(|_| number_out_of_range(span, value.into_owned(), target))?,
        other => return Err(type_mismatch_name(span, target, other.kind_name())),
    };
    T::try_from(value).map_err(|_| number_out_of_range(span, value.to_string(), target))
}

fn into_signed<T>(
    value: JsonScalarToken<'_>,
    span: Span,
    target: &'static str,
) -> Result<T, DeserializeError>
where
    T: TryFrom<i128>,
{
    let value = match value {
        JsonScalarToken::I64(value) => value as i128,
        JsonScalarToken::I128(value) => value,
        JsonScalarToken::U64(value) => value as i128,
        JsonScalarToken::U128(value) if value <= i128::MAX as u128 => value as i128,
        JsonScalarToken::F64(value) => f64_to_i128(value, span, target)?,
        JsonScalarToken::Str(value) => value
            .parse::<i128>()
            .map_err(|_| number_out_of_range(span, value.into_owned(), target))?,
        other => return Err(type_mismatch_name(span, target, other.kind_name())),
    };
    T::try_from(value).map_err(|_| number_out_of_range(span, value.to_string(), target))
}

fn vm_error(span: Option<Span>, kind: DeserializeErrorKind) -> DeserializeError {
    DeserializeError {
        span,
        path: None,
        kind,
    }
}

fn type_mismatch(span: Span, expected: &'static Shape, got: &'static str) -> DeserializeError {
    vm_error(
        Some(span),
        DeserializeErrorKind::TypeMismatch {
            expected,
            got: got.into(),
        },
    )
}

fn type_mismatch_name(span: Span, expected: &'static str, got: &'static str) -> DeserializeError {
    vm_error(
        Some(span),
        DeserializeErrorKind::UnexpectedToken {
            got: got.into(),
            expected,
        },
    )
}

fn number_out_of_range(span: Span, value: String, target_type: &'static str) -> DeserializeError {
    vm_error(
        Some(span),
        DeserializeErrorKind::NumberOutOfRange {
            value: value.into(),
            target_type,
        },
    )
}

fn invalid_value(span: Span, message: impl Into<Cow<'static, str>>) -> DeserializeError {
    vm_error(
        Some(span),
        DeserializeErrorKind::InvalidValue {
            message: message.into(),
        },
    )
}

fn unsupported_shape_message(message: &'static str) -> DeserializeError {
    vm_error(
        None,
        DeserializeErrorKind::Unsupported {
            message: message.into(),
        },
    )
}
