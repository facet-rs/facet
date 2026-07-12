use alloc::{
    borrow::Cow,
    boxed::Box,
    collections::BTreeMap,
    format,
    rc::Rc,
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};
use core::alloc::Layout;
use core::marker::PhantomData;
use core::mem::{ManuallyDrop, MaybeUninit};
use core::str::FromStr;
#[cfg(all(
    facet_json_jit_active,
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
use core::sync::atomic::{AtomicU8, Ordering};

use facet_core::{
    ArrayDef, Characteristic, Def, DefaultInPlaceFn, DefaultSource, DynamicValueDef, EnumRepr,
    EnumType, Facet, Field, KnownPointer, ListDef, MapDef, OptionDef, PointerDef, ProxyDef,
    PtrConst, PtrMut, PtrUninit, ScalarType, SetDef, Shape, StructKind, TryFromOutcome, Type,
    UserType, Variant,
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
    facet_json_jit_active,
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
use crate::parser::{NativeArrayStep, NativeOrderedRootCursor};
use crate::scanner::{NumberHint, ParsedNumber, SpannedToken, Token as ScanToken};

#[cfg(all(
    facet_json_jit_active,
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
    ArrayLoop(&'static Shape),
    DynamicLoop(&'static Shape),
    ListLoop(&'static Shape),
    PointerSliceLoop(&'static Shape),
    SetLoop(&'static Shape),
    MapLoop(&'static Shape),
}

type BlockId = JsonBlockId;
type ExecBlock = BlockRef;
type SymbolicOp = JsonOp<BlockId>;
type ExecOp = JsonOp<ExecBlock>;
const JSON_FORMAT_NAMESPACE: &str = "json";

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
        facet_json_jit_active,
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
            facet_json_jit_active,
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
            facet_json_jit_active,
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
                facet_json_jit_active,
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
            facet_json_jit_active,
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
            facet_json_jit_active,
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
    facet_json_jit_active,
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
    facet_json_jit_active,
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
// Safety: native plans are immutable after construction; raw pointers in their
// program stream point into `calls`/`scalar_structs`, both owned by the plan.
unsafe impl Send for JsonNativePlan {}

#[cfg(all(
    facet_json_jit_active,
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
// Safety: see the `Send` impl; running a plan only mutates per-call state.
unsafe impl Sync for JsonNativePlan {}

#[cfg(all(
    facet_json_jit_active,
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
    facet_json_jit_active,
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
    facet_json_jit_active,
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
type NativeCursorScalarReader =
    fn(&mut NativeOrderedRootCursor<'_>, PtrUninit) -> Result<bool, DeserializeError>;

#[cfg(all(
    facet_json_jit_active,
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
    facet_json_jit_active,
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
const JSON_NATIVE_STATUS_OK: u64 = 0;

#[cfg(all(
    facet_json_jit_active,
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
const JSON_NATIVE_STATUS_FALLBACK: u64 = 1;

#[cfg(all(
    facet_json_jit_active,
    any(
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64")
    )
))]
const ORDERED_PROBE_BACKOFF: u8 = u8::MAX;

#[cfg(all(
    facet_json_jit_active,
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
    facet_json_jit_active,
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
                    ..
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
                ..
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
    facet_json_jit_active,
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
    facet_json_jit_active,
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
        interp.read_scalar_struct(
            info.shape,
            &info.fields,
            info.dispatch.as_ref(),
            info.shape.vtable.has_invariants(),
        )?;
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
    facet_json_jit_active,
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
    facet_json_jit_active,
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
    facet_json_jit_active,
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
    facet_json_jit_active,
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
    facet_json_jit_active,
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
    facet_json_jit_active,
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
    facet_json_jit_active,
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
    facet_json_jit_active,
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
    facet_json_jit_active,
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
    facet_json_jit_active,
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
    facet_json_jit_active,
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
    facet_json_jit_active,
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
    facet_json_jit_active,
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
    facet_json_jit_active,
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

#[cfg(not(facet_json_jit_active))]
fn json_weavy_native_jit_available() -> bool {
    false
}

#[cfg(facet_json_jit_active)]
fn json_weavy_native_jit_available() -> bool {
    weavy::jit::NATIVE_COPY_PATCH_AVAILABLE
}

#[cfg(not(facet_json_jit_active))]
fn json_weavy_jit_fallback_reason() -> &'static str {
    "Weavy's native JIT is inactive for this build"
}

#[cfg(all(
    facet_json_jit_active,
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

fn single_program_op<'program>(
    blocks: &'program [Program<ExecOp>],
    program: &'program Program<ExecOp>,
) -> Option<&'program ExecOp> {
    let mut current = program;
    for _ in 0..=blocks.len() {
        match current.as_slice() {
            [JsonOp::CallBlock(block)] => {
                current = blocks.get(block.index())?;
            }
            [op] => return Some(op),
            _ => return None,
        }
    }
    None
}

#[derive(Clone, Debug)]
enum JsonOp<Block> {
    CallBlock(Block),
    ReadScalar {
        shape: &'static Shape,
        scalar: ScalarPlan,
    },
    ReadParsedScalar {
        shape: &'static Shape,
    },
    ReadBuilderShape {
        shape: &'static Shape,
        builder_shape: &'static Shape,
        builder_layout: Layout,
        builder_program: Program<JsonOp<Block>>,
    },
    ReadTransparent {
        field_offset: usize,
        field_shape: &'static Shape,
        field_program: Program<JsonOp<Block>>,
        field_scalar: Option<ScalarPlan>,
    },
    ReadProxy {
        proxy: &'static ProxyDef,
        proxy_layout: Layout,
        proxy_program: Program<JsonOp<Block>>,
    },
    ReadUnitStruct {
        shape: &'static Shape,
    },
    ReadTupleStruct {
        shape: &'static Shape,
        fields: Box<[FieldPlan<Block>]>,
        tracking: StructTracking,
    },
    ReadScalarStruct {
        shape: &'static Shape,
        fields: Box<[ScalarFieldPlan]>,
        dispatch: Option<RawFieldDispatch>,
    },
    ReadScalarStructValidate {
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
    ReadStructValidate {
        shape: &'static Shape,
        fields: Box<[FieldPlan<Block>]>,
        dispatch: Option<RawFieldDispatch>,
        loop_id: Block,
    },
    ReadFlattenStruct {
        shape: &'static Shape,
        plan: Box<FlattenStructPlan<Block>>,
    },
    ReadExternalEnum {
        shape: &'static Shape,
        enum_type: EnumType,
        variants: Box<[ExternalVariantPlan<Block>]>,
    },
    ReadNumericEnum {
        shape: &'static Shape,
        enum_type: EnumType,
        variants: Box<[ExternalVariantPlan<Block>]>,
    },
    ReadUntaggedEnum {
        shape: &'static Shape,
        enum_type: EnumType,
        variants: Box<[ExternalVariantPlan<Block>]>,
    },
    ReadCowEnum {
        shape: &'static Shape,
        enum_type: EnumType,
        owned_variant: Box<ExternalVariantPlan<Block>>,
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
    ReadArray {
        array_shape: &'static Shape,
        array: ArrayDef,
        element_layout: Layout,
        loop_id: Block,
    },
    ArrayNext {
        array: ArrayDef,
        element_program: Program<JsonOp<Block>>,
        element_scalar: Option<ScalarType>,
        element_option_scalar: Option<ListOptionScalar>,
        element_layout: Layout,
        loop_id: Block,
    },
    ReadDynamicValue {
        dynamic_shape: &'static Shape,
        dynamic: DynamicValueDef,
        loop_id: Block,
    },
    DynamicNext {
        dynamic_shape: &'static Shape,
        dynamic_layout: Layout,
        value_program: Program<JsonOp<Block>>,
        loop_id: Block,
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
        key_plan: Box<MapKeyPlan>,
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
    ReadPointerString {
        pointer_shape: &'static Shape,
        pointer: PointerDef,
    },
    ReadPointerSlice {
        pointer_shape: &'static Shape,
        pointer: PointerDef,
        pointer_layout: Layout,
        element_layout: Layout,
        loop_id: Block,
    },
    PointerSliceNext {
        pointer: PointerDef,
        element_program: Program<JsonOp<Block>>,
        element_scalar: Option<ScalarType>,
        element_option_scalar: Option<ListOptionScalar>,
        element_layout: Layout,
        loop_id: Block,
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
struct MetadataKeyField {
    name: &'static str,
    alias: Option<&'static str>,
    offset: usize,
    shape: &'static Shape,
    metadata: Option<&'static str>,
    missing: MissingField,
}

#[derive(Clone, Debug)]
enum MapKeyPlan {
    Scalar {
        shape: &'static Shape,
        scalar: ScalarType,
        layout: Layout,
    },
    MetadataContainer {
        shape: &'static Shape,
        layout: Layout,
        fields: Box<[MetadataKeyField]>,
        dispatch: Option<RawFieldDispatch>,
        value_index: usize,
        value: Box<MapKeyPlan>,
    },
}

impl MapKeyPlan {
    fn layout(&self) -> Layout {
        match self {
            Self::Scalar { layout, .. } | Self::MetadataContainer { layout, .. } => *layout,
        }
    }

    fn is_exact_string(&self) -> bool {
        matches!(
            self,
            Self::Scalar {
                shape,
                scalar: ScalarType::String,
                ..
            } if shape.is_type::<String>()
        )
    }
}

#[derive(Clone, Debug)]
struct FlattenStructPlan<Block> {
    fields: Box<[FieldPlan<Block>]>,
    direct_indices: Box<[usize]>,
    flattened: Box<[FlattenFieldPlan<Block>]>,
    tracking: StructTracking,
}

#[derive(Clone, Debug)]
struct FlattenFieldPlan<Block> {
    field_index: usize,
    kind: FlattenKind<Block>,
}

#[derive(Clone, Debug)]
enum FlattenKind<Block> {
    Struct {
        plan: Box<FlattenStructPlan<Block>>,
    },
    OptionStruct {
        option: OptionDef,
        inner_layout: Layout,
        plan: Box<FlattenStructPlan<Block>>,
    },
    ExternalEnum {
        enum_type: EnumType,
        tag_key: Option<&'static str>,
        content_key: Option<&'static str>,
        variants: Box<[ExternalVariantPlan<Block>]>,
    },
    OptionExternalEnum {
        option: OptionDef,
        inner_layout: Layout,
        enum_type: EnumType,
        tag_key: Option<&'static str>,
        content_key: Option<&'static str>,
        variants: Box<[ExternalVariantPlan<Block>]>,
    },
    Map {
        map: MapDef,
        key_plan: Box<MapKeyPlan>,
        value_program: Program<JsonOp<Block>>,
        value_scalar: Option<ScalarPlan>,
        value_layout: Layout,
    },
}

#[derive(Clone, Debug)]
struct ExternalVariantPlan<Block> {
    index: usize,
    variant: &'static Variant,
    fields: Box<[FieldPlan<Block>]>,
    dispatch: Option<RawFieldDispatch>,
    loop_id: Option<Block>,
    tracking: StructTracking,
    flatten: Option<Box<FlattenStructPlan<Block>>>,
}

struct TaggedRawField<'de> {
    name: Cow<'de, str>,
    raw: &'de str,
    span: Span,
}

type FlattenVariantSelection<'fields, 'de, 'program> = (
    usize,
    &'fields TaggedRawField<'de>,
    &'program ExternalVariantPlan<ExecBlock>,
);

struct FlattenExternalEnumRef<'program> {
    enum_type: EnumType,
    tag_key: Option<&'static str>,
    content_key: Option<&'static str>,
    variants: &'program [ExternalVariantPlan<ExecBlock>],
}

struct FlattenMapRef<'program> {
    map: MapDef,
    key_plan: &'program MapKeyPlan,
    value_program: &'program [ExecOp],
    value_scalar: Option<ScalarPlan>,
    value_layout: Layout,
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

impl StructFieldPlan for MetadataKeyField {
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
        if let Some(proxy) = shape.effective_proxy(Some(JSON_FORMAT_NAMESPACE)) {
            return self.lower_proxy(shape, proxy);
        }

        if is_empty_tuple_shape(shape)
            && let Type::User(UserType::Struct(struct_type)) = shape.ty
        {
            return self.lower_tuple_struct(shape, struct_type);
        }

        if let Some(scalar) = ScalarType::try_from_shape(shape) {
            return Ok(vec![JsonOp::ReadScalar {
                shape,
                scalar: ScalarPlan::new(scalar),
            }]);
        }

        if let Some(builder_shape) = shape.builder_shape
            && shape.vtable.has_try_from()
        {
            let builder_layout = sized_layout(builder_shape)?;
            let builder_program = self.lower_shape(builder_shape)?;
            return Ok(vec![JsonOp::ReadBuilderShape {
                shape,
                builder_shape,
                builder_layout,
                builder_program,
            }]);
        }

        if let Some(inner_shape) = shape.inner
            && shape.vtable.has_try_from()
        {
            let builder_layout = sized_layout(inner_shape)?;
            let builder_program = self.lower_shape(inner_shape)?;
            return Ok(vec![JsonOp::ReadBuilderShape {
                shape,
                builder_shape: inner_shape,
                builder_layout,
                builder_program,
            }]);
        }

        if matches!(shape.def, Def::Scalar) && shape.inner.is_none() && shape.vtable.has_parse() {
            return Ok(vec![JsonOp::ReadParsedScalar { shape }]);
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
            Def::Array(array) => {
                let element_layout = sized_layout(array.t())?;
                let element_program = self.lower_shape(array.t())?;
                let element_scalar = ScalarType::try_from_shape(array.t());
                let element_option_scalar = list_option_scalar(array.t())?;
                let loop_id = JsonBlockId::ArrayLoop(shape);
                let loop_program = vec![JsonOp::ArrayNext {
                    array,
                    element_program: element_program.clone(),
                    element_scalar,
                    element_option_scalar,
                    element_layout,
                    loop_id,
                }];
                self.lowered.blocks.insert(loop_id, loop_program);
                Ok(vec![JsonOp::ReadArray {
                    array_shape: shape,
                    array,
                    element_layout,
                    loop_id,
                }])
            }
            Def::DynamicValue(dynamic) => {
                let dynamic_layout = sized_layout(shape)?;
                let value_program = vec![JsonOp::CallBlock(JsonBlockId::Shape(shape))];
                let loop_id = JsonBlockId::DynamicLoop(shape);
                let loop_program = vec![JsonOp::DynamicNext {
                    dynamic_shape: shape,
                    dynamic_layout,
                    value_program: value_program.clone(),
                    loop_id,
                }];
                self.lowered.blocks.insert(loop_id, loop_program);
                Ok(vec![JsonOp::ReadDynamicValue {
                    dynamic_shape: shape,
                    dynamic,
                    loop_id,
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
                let key_plan = self.lower_map_key_plan(map.k())?;
                let value_layout = sized_layout(map.v())?;
                let value_program = self.lower_shape(map.v())?;
                let value_scalar = ScalarType::try_from_shape(map.v()).map(ScalarPlan::new);
                let loop_id = JsonBlockId::MapLoop(shape);
                let loop_program = vec![JsonOp::MapNext {
                    map,
                    key_plan: Box::new(key_plan),
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
                    if pointer_to_str(pointer) {
                        return Ok(vec![JsonOp::ReadPointerString {
                            pointer_shape: shape,
                            pointer,
                        }]);
                    }

                    if pointer.vtable.slice_builder_vtable.is_some()
                        && let Def::Slice(slice) = pointee.def
                    {
                        let pointer_layout = sized_layout(shape)?;
                        let element_layout = sized_layout(slice.t())?;
                        let element_program = self.lower_shape(slice.t())?;
                        let element_scalar = ScalarType::try_from_shape(slice.t());
                        let element_option_scalar = list_option_scalar(slice.t())?;
                        let loop_id = JsonBlockId::PointerSliceLoop(shape);
                        let loop_program = vec![JsonOp::PointerSliceNext {
                            pointer,
                            element_program: element_program.clone(),
                            element_scalar,
                            element_option_scalar,
                            element_layout,
                            loop_id,
                        }];
                        self.lowered.blocks.insert(loop_id, loop_program);
                        return Ok(vec![JsonOp::ReadPointerSlice {
                            pointer_shape: shape,
                            pointer,
                            pointer_layout,
                            element_layout,
                            loop_id,
                        }]);
                    }

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
                    if shape.is_transparent() {
                        return self.lower_transparent_struct(shape, struct_type);
                    }
                    match struct_type.kind {
                        StructKind::Unit => return Ok(vec![JsonOp::ReadUnitStruct { shape }]),
                        StructKind::Tuple | StructKind::TupleStruct => {
                            return self.lower_tuple_struct(shape, struct_type);
                        }
                        StructKind::Struct => {}
                    }

                    if struct_type.fields.iter().any(Field::is_flattened) {
                        return Ok(vec![JsonOp::ReadFlattenStruct {
                            shape,
                            plan: Box::new(self.lower_flatten_struct_plan(shape, struct_type)?),
                        }]);
                    }

                    let container_has_default = shape.has_default_attr();
                    let all_scalar = struct_type.fields.iter().all(|field| {
                        field.effective_proxy(Some(JSON_FORMAT_NAMESPACE)).is_none()
                            && !is_empty_tuple_shape(field.shape())
                            && ScalarType::try_from_shape(field.shape()).is_some()
                    });

                    if all_scalar {
                        let mut fields = Vec::with_capacity(struct_type.fields.len());
                        for field in struct_type.fields {
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
                        let fields = fields.into_boxed_slice();
                        if shape.vtable.has_invariants() {
                            return Ok(vec![JsonOp::ReadScalarStructValidate {
                                shape,
                                fields,
                                dispatch,
                            }]);
                        }
                        return Ok(vec![JsonOp::ReadScalarStruct {
                            shape,
                            fields,
                            dispatch,
                        }]);
                    }

                    let mut fields = Vec::with_capacity(struct_type.fields.len());
                    for field in struct_type.fields {
                        let field_shape = field.shape();
                        let (program, scalar) = self.lower_field_value(field)?;
                        fields.push(FieldPlan {
                            name: field.effective_name(),
                            alias: field.alias,
                            offset: field.offset,
                            shape: field_shape,
                            program,
                            scalar,
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
                    if shape.vtable.has_invariants() {
                        return Ok(vec![JsonOp::ReadStructValidate {
                            shape,
                            fields,
                            dispatch,
                            loop_id,
                        }]);
                    }
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

    fn lower_proxy(
        &mut self,
        target_shape: &'static Shape,
        proxy: &'static ProxyDef,
    ) -> Result<Program<SymbolicOp>, DeserializeError> {
        if core::ptr::eq(target_shape, proxy.shape) {
            return Err(unsupported(target_shape, "self-referential proxy"));
        }

        let proxy_layout = sized_layout(proxy.shape)?;
        let proxy_program = self.lower_shape(proxy.shape)?;
        Ok(vec![JsonOp::ReadProxy {
            proxy,
            proxy_layout,
            proxy_program,
        }])
    }

    fn lower_transparent_struct(
        &mut self,
        shape: &'static Shape,
        struct_type: facet_core::StructType,
    ) -> Result<Program<SymbolicOp>, DeserializeError> {
        let [field] = struct_type.fields else {
            return Err(unsupported(
                shape,
                "transparent struct without exactly one field",
            ));
        };
        if field.should_skip_deserializing() || field.is_flattened() {
            return Err(unsupported(shape, "skipped or flattened transparent field"));
        }

        let (field_program, field_scalar) = self.lower_field_value(field)?;
        Ok(vec![JsonOp::ReadTransparent {
            field_offset: field.offset,
            field_shape: field.shape(),
            field_program,
            field_scalar,
        }])
    }

    fn lower_field_value(
        &mut self,
        field: &'static Field,
    ) -> Result<(Program<SymbolicOp>, Option<ScalarPlan>), DeserializeError> {
        let field_shape = field.shape();
        if let Some(proxy) = field.effective_proxy(Some(JSON_FORMAT_NAMESPACE)) {
            return Ok((self.lower_proxy(field_shape, proxy)?, None));
        }

        Ok((
            self.lower_shape(field_shape)?,
            (!is_empty_tuple_shape(field_shape))
                .then(|| ScalarType::try_from_shape(field_shape).map(ScalarPlan::new))
                .flatten(),
        ))
    }

    fn lower_map_key_plan(
        &mut self,
        shape: &'static Shape,
    ) -> Result<MapKeyPlan, DeserializeError> {
        if let Some(scalar) = ScalarType::try_from_shape(shape) {
            if !map_key_scalar_supported(scalar) {
                return Err(unsupported(shape, "string or integer map key"));
            }

            return Ok(MapKeyPlan::Scalar {
                shape,
                scalar,
                layout: sized_layout(shape)?,
            });
        }

        if shape.is_metadata_container()
            && let Type::User(UserType::Struct(struct_type)) = shape.ty
        {
            let mut fields = Vec::with_capacity(struct_type.fields.len());
            let mut value_index = None;
            let mut value_plan = None;
            let container_has_default = shape.has_default_attr();

            for field in struct_type.fields {
                let index = fields.len();
                let field_shape = field.shape();
                if field.metadata_kind().is_none() {
                    if value_index.is_some() {
                        return Err(unsupported(
                            shape,
                            "metadata map key with multiple value fields",
                        ));
                    }
                    value_index = Some(index);
                    value_plan = Some(Box::new(self.lower_map_key_plan(field_shape)?));
                }

                fields.push(MetadataKeyField {
                    name: field.effective_name(),
                    alias: field.alias,
                    offset: field.offset,
                    shape: field_shape,
                    metadata: field.metadata_kind(),
                    missing: missing_field_action(field, container_has_default),
                });
            }

            let Some(value_index) = value_index else {
                return Err(unsupported(shape, "metadata map key without value field"));
            };
            let value = value_plan.expect("metadata map key value plan is present");
            let fields = fields.into_boxed_slice();
            let dispatch = RawFieldDispatch::for_fields(fields.as_ref());
            return Ok(MapKeyPlan::MetadataContainer {
                shape,
                layout: sized_layout(shape)?,
                fields,
                dispatch,
                value_index,
                value,
            });
        }

        Err(unsupported(shape, "scalar or metadata container map key"))
    }

    fn lower_tuple_struct(
        &mut self,
        shape: &'static Shape,
        struct_type: facet_core::StructType,
    ) -> Result<Program<SymbolicOp>, DeserializeError> {
        let mut fields = Vec::with_capacity(struct_type.fields.len());
        for field in struct_type.fields {
            if field.should_skip_deserializing() || field.is_flattened() {
                return Err(unsupported(shape, "skipped or flattened tuple fields"));
            }
            let field_shape = field.shape();
            let (program, scalar) = self.lower_field_value(field)?;
            fields.push(FieldPlan {
                name: field.effective_name(),
                alias: field.alias,
                offset: field.offset,
                shape: field_shape,
                program,
                scalar,
                missing: missing_field_action(field, false),
            });
        }

        let fields = fields.into_boxed_slice();
        let tracking = StructTracking::for_len(fields.len());
        Ok(vec![JsonOp::ReadTupleStruct {
            shape,
            fields,
            tracking,
        }])
    }

    fn lower_flatten_struct_plan(
        &mut self,
        shape: &'static Shape,
        struct_type: facet_core::StructType,
    ) -> Result<FlattenStructPlan<BlockId>, DeserializeError> {
        if shape.is_transparent() {
            return Err(unsupported(shape, "transparent flattened struct"));
        }
        if struct_type.kind != StructKind::Struct {
            return Err(unsupported(shape, "non-record flattened struct"));
        }

        self.lower_flatten_field_plan(shape, struct_type.fields, shape.has_default_attr())
    }

    fn lower_flatten_variant_plan(
        &mut self,
        shape: &'static Shape,
        fields: &'static [Field],
    ) -> Result<FlattenStructPlan<BlockId>, DeserializeError> {
        self.lower_flatten_field_plan(shape, fields, false)
    }

    fn lower_flatten_field_plan(
        &mut self,
        shape: &'static Shape,
        source_fields: &'static [Field],
        container_has_default: bool,
    ) -> Result<FlattenStructPlan<BlockId>, DeserializeError> {
        let mut fields = Vec::with_capacity(source_fields.len());
        let mut direct_indices = Vec::new();
        let mut flattened = Vec::new();

        for field in source_fields {
            let field_shape = field.shape();
            let field_index = fields.len();

            if field.is_flattened() {
                if field.should_skip_deserializing() {
                    return Err(unsupported(shape, "skipped flattened field"));
                }
                if field.effective_proxy(Some(JSON_FORMAT_NAMESPACE)).is_some() {
                    return Err(unsupported(shape, "proxy flattened field"));
                }

                fields.push(FieldPlan {
                    name: field.effective_name(),
                    alias: field.alias,
                    offset: field.offset,
                    shape: field_shape,
                    program: Vec::new(),
                    scalar: None,
                    missing: missing_field_action(field, container_has_default),
                });
                flattened.push(FlattenFieldPlan {
                    field_index,
                    kind: self.lower_flatten_kind(field_shape)?,
                });
            } else {
                let (program, scalar) = self.lower_field_value(field)?;
                fields.push(FieldPlan {
                    name: field.effective_name(),
                    alias: field.alias,
                    offset: field.offset,
                    shape: field_shape,
                    program,
                    scalar,
                    missing: missing_field_action(field, container_has_default),
                });
                direct_indices.push(field_index);
            }
        }

        let fields = fields.into_boxed_slice();
        Ok(FlattenStructPlan {
            tracking: StructTracking::for_len(fields.len()),
            fields,
            direct_indices: direct_indices.into_boxed_slice(),
            flattened: flattened.into_boxed_slice(),
        })
    }

    fn lower_flatten_kind(
        &mut self,
        shape: &'static Shape,
    ) -> Result<FlattenKind<BlockId>, DeserializeError> {
        if let Def::Option(option) = shape.def {
            let inner = option.t();
            let inner_layout = sized_layout(inner)?;
            return match inner.ty {
                Type::User(UserType::Struct(struct_type)) => Ok(FlattenKind::OptionStruct {
                    option,
                    inner_layout,
                    plan: Box::new(self.lower_flatten_struct_plan(inner, struct_type)?),
                }),
                Type::User(UserType::Enum(enum_type)) => Ok(FlattenKind::OptionExternalEnum {
                    option,
                    inner_layout,
                    enum_type,
                    tag_key: inner.get_tag_attr(),
                    content_key: inner.get_content_attr(),
                    variants: self.lower_external_enum_variants(inner, enum_type, true)?,
                }),
                _ => Err(unsupported(shape, "optional flattened non-struct/non-enum")),
            };
        }

        if let Def::Map(map) = shape.def {
            return Ok(FlattenKind::Map {
                map,
                key_plan: Box::new(self.lower_map_key_plan(map.k())?),
                value_program: self.lower_shape(map.v())?,
                value_scalar: ScalarType::try_from_shape(map.v()).map(ScalarPlan::new),
                value_layout: sized_layout(map.v())?,
            });
        }

        match shape.ty {
            Type::User(UserType::Struct(struct_type)) => Ok(FlattenKind::Struct {
                plan: Box::new(self.lower_flatten_struct_plan(shape, struct_type)?),
            }),
            Type::User(UserType::Enum(enum_type)) => Ok(FlattenKind::ExternalEnum {
                enum_type,
                tag_key: shape.get_tag_attr(),
                content_key: shape.get_content_attr(),
                variants: self.lower_external_enum_variants(shape, enum_type, true)?,
            }),
            _ => Err(unsupported(shape, "flattened non-struct/non-enum")),
        }
    }

    fn lower_external_enum_variants(
        &mut self,
        shape: &'static Shape,
        enum_type: EnumType,
        _flattened: bool,
    ) -> Result<Box<[ExternalVariantPlan<BlockId>]>, DeserializeError> {
        if enum_type.is_cow {
            return Err(unsupported(shape, "cow enum"));
        }

        let mut variants = Vec::with_capacity(enum_type.variants.len());
        for (index, variant) in enum_type.variants.iter().enumerate() {
            let flatten = if variant.data.kind == StructKind::Struct
                && variant.data.fields.iter().any(Field::is_flattened)
            {
                Some(Box::new(
                    self.lower_flatten_variant_plan(shape, variant.data.fields)?,
                ))
            } else {
                None
            };

            let fields = if let Some(flatten) = &flatten {
                flatten.fields.clone()
            } else {
                let mut fields = Vec::with_capacity(variant.data.fields.len());
                for field in variant.data.fields {
                    if variant.is_other() && (field.is_variant_tag() || field.is_variant_content())
                    {
                        return Err(unsupported(shape, "other enum variant tag/content fields"));
                    }
                    if field.should_skip_deserializing() || field.is_flattened() {
                        return Err(unsupported(
                            shape,
                            "skipped or flattened enum variant fields",
                        ));
                    }
                    let field_shape = field.shape();
                    let (program, scalar) = self.lower_field_value(field)?;
                    fields.push(FieldPlan {
                        name: field.effective_name(),
                        alias: field.alias,
                        offset: field.offset,
                        shape: field_shape,
                        program,
                        scalar,
                        missing: missing_field_action(field, false),
                    });
                }
                fields.into_boxed_slice()
            };
            let tracking = StructTracking::for_len(fields.len());
            let dispatch = RawFieldDispatch::for_fields(&fields);
            if shape.get_tag_attr().is_some()
                && shape.get_content_attr().is_none()
                && !matches!(variant.data.kind, StructKind::Unit | StructKind::Struct)
                && !(matches!(
                    variant.data.kind,
                    StructKind::Tuple | StructKind::TupleStruct
                ) && fields.len() == 1)
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
                flatten,
            });
        }

        Ok(variants.into_boxed_slice())
    }

    fn lower_cow_enum(
        &mut self,
        shape: &'static Shape,
        enum_type: EnumType,
    ) -> Result<Program<SymbolicOp>, DeserializeError> {
        let owned_variant = enum_type
            .owned_variant()
            .ok_or_else(|| unsupported(shape, "cow enum without Owned variant"))?;
        let index = enum_type
            .variants
            .iter()
            .position(|variant| variant.name == owned_variant.name)
            .ok_or_else(|| unsupported(shape, "cow enum Owned variant index"))?;

        let [field] = owned_variant.data.fields else {
            return Err(unsupported(shape, "cow enum without single Owned field"));
        };
        if field.should_skip_deserializing() || field.is_flattened() {
            return Err(unsupported(shape, "skipped or flattened cow enum field"));
        }

        let field_shape = field.shape();
        let (program, scalar) = self.lower_field_value(field)?;
        let fields = Box::new([FieldPlan {
            name: field.effective_name(),
            alias: field.alias,
            offset: field.offset,
            shape: field_shape,
            program,
            scalar,
            missing: missing_field_action(field, false),
        }]);

        Ok(vec![JsonOp::ReadCowEnum {
            shape,
            enum_type,
            owned_variant: Box::new(ExternalVariantPlan {
                index,
                variant: owned_variant,
                dispatch: RawFieldDispatch::for_fields(fields.as_ref()),
                loop_id: None,
                tracking: StructTracking::for_len(fields.len()),
                fields,
                flatten: None,
            }),
        }])
    }

    fn lower_external_enum(
        &mut self,
        shape: &'static Shape,
        enum_type: EnumType,
    ) -> Result<Program<SymbolicOp>, DeserializeError> {
        if enum_type.is_cow {
            return self.lower_cow_enum(shape, enum_type);
        }

        let tag_key = shape.get_tag_attr();
        let content_key = shape.get_content_attr();
        if tag_key.is_none() && content_key.is_some() {
            return Err(unsupported(shape, "enum content key without tag key"));
        }
        let variants = self.lower_external_enum_variants(shape, enum_type, false)?;
        if shape.is_numeric() && tag_key.is_none() {
            return Ok(vec![JsonOp::ReadNumericEnum {
                shape,
                enum_type,
                variants,
            }]);
        }
        if shape.is_untagged() {
            return Ok(vec![JsonOp::ReadUntaggedEnum {
                shape,
                enum_type,
                variants,
            }]);
        }
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
        JsonOp::ReadParsedScalar { shape } => JsonOp::ReadParsedScalar { shape },
        JsonOp::ReadBuilderShape {
            shape,
            builder_shape,
            builder_layout,
            builder_program,
        } => JsonOp::ReadBuilderShape {
            shape,
            builder_shape,
            builder_layout,
            builder_program: resolve_json_program(builder_program, refs)?,
        },
        JsonOp::ReadTransparent {
            field_offset,
            field_shape,
            field_program,
            field_scalar,
        } => JsonOp::ReadTransparent {
            field_offset,
            field_shape,
            field_program: resolve_json_program(field_program, refs)?,
            field_scalar,
        },
        JsonOp::ReadProxy {
            proxy,
            proxy_layout,
            proxy_program,
        } => JsonOp::ReadProxy {
            proxy,
            proxy_layout,
            proxy_program: resolve_json_program(proxy_program, refs)?,
        },
        JsonOp::ReadUnitStruct { shape } => JsonOp::ReadUnitStruct { shape },
        JsonOp::ReadTupleStruct {
            shape,
            fields,
            tracking,
        } => JsonOp::ReadTupleStruct {
            shape,
            fields: resolve_field_plans(fields, refs)?,
            tracking,
        },
        JsonOp::ReadScalarStruct {
            shape,
            fields,
            dispatch,
        } => JsonOp::ReadScalarStruct {
            shape,
            fields,
            dispatch,
        },
        JsonOp::ReadScalarStructValidate {
            shape,
            fields,
            dispatch,
        } => JsonOp::ReadScalarStructValidate {
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
        JsonOp::ReadStructValidate {
            shape,
            fields,
            dispatch,
            loop_id,
        } => JsonOp::ReadStructValidate {
            shape,
            fields: resolve_field_plans(fields, refs)?,
            dispatch,
            loop_id: resolve_block_ref(loop_id, refs)?,
        },
        JsonOp::ReadFlattenStruct { shape, plan } => JsonOp::ReadFlattenStruct {
            shape,
            plan: Box::new(resolve_flatten_struct_plan(*plan, refs)?),
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
        JsonOp::ReadNumericEnum {
            shape,
            enum_type,
            variants,
        } => JsonOp::ReadNumericEnum {
            shape,
            enum_type,
            variants: resolve_external_variant_plans(variants, refs)?,
        },
        JsonOp::ReadUntaggedEnum {
            shape,
            enum_type,
            variants,
        } => JsonOp::ReadUntaggedEnum {
            shape,
            enum_type,
            variants: resolve_external_variant_plans(variants, refs)?,
        },
        JsonOp::ReadCowEnum {
            shape,
            enum_type,
            owned_variant,
        } => JsonOp::ReadCowEnum {
            shape,
            enum_type,
            owned_variant: Box::new(resolve_external_variant_plan(*owned_variant, refs)?),
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
        JsonOp::ReadArray {
            array_shape,
            array,
            element_layout,
            loop_id,
        } => JsonOp::ReadArray {
            array_shape,
            array,
            element_layout,
            loop_id: resolve_block_ref(loop_id, refs)?,
        },
        JsonOp::ArrayNext {
            array,
            element_program,
            element_scalar,
            element_option_scalar,
            element_layout,
            loop_id,
        } => JsonOp::ArrayNext {
            array,
            element_program: resolve_json_program(element_program, refs)?,
            element_scalar,
            element_option_scalar,
            element_layout,
            loop_id: resolve_block_ref(loop_id, refs)?,
        },
        JsonOp::ReadDynamicValue {
            dynamic_shape,
            dynamic,
            loop_id,
        } => JsonOp::ReadDynamicValue {
            dynamic_shape,
            dynamic,
            loop_id: resolve_block_ref(loop_id, refs)?,
        },
        JsonOp::DynamicNext {
            dynamic_shape,
            dynamic_layout,
            value_program,
            loop_id,
        } => JsonOp::DynamicNext {
            dynamic_shape,
            dynamic_layout,
            value_program: resolve_json_program(value_program, refs)?,
            loop_id: resolve_block_ref(loop_id, refs)?,
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
            key_plan,
            value_program,
            value_scalar,
            value_layout,
            loop_id,
        } => JsonOp::MapNext {
            map,
            key_plan,
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
        JsonOp::ReadPointerString {
            pointer_shape,
            pointer,
        } => JsonOp::ReadPointerString {
            pointer_shape,
            pointer,
        },
        JsonOp::ReadPointerSlice {
            pointer_shape,
            pointer,
            pointer_layout,
            element_layout,
            loop_id,
        } => JsonOp::ReadPointerSlice {
            pointer_shape,
            pointer,
            pointer_layout,
            element_layout,
            loop_id: resolve_block_ref(loop_id, refs)?,
        },
        JsonOp::PointerSliceNext {
            pointer,
            element_program,
            element_scalar,
            element_option_scalar,
            element_layout,
            loop_id,
        } => JsonOp::PointerSliceNext {
            pointer,
            element_program: resolve_json_program(element_program, refs)?,
            element_scalar,
            element_option_scalar,
            element_layout,
            loop_id: resolve_block_ref(loop_id, refs)?,
        },
    })
}

fn resolve_flatten_struct_plan(
    plan: FlattenStructPlan<BlockId>,
    refs: &BTreeMap<BlockId, ExecBlock>,
) -> Result<FlattenStructPlan<ExecBlock>, DeserializeError> {
    Ok(FlattenStructPlan {
        fields: resolve_field_plans(plan.fields, refs)?,
        direct_indices: plan.direct_indices,
        flattened: resolve_flatten_field_plans(plan.flattened, refs)?,
        tracking: plan.tracking,
    })
}

fn resolve_flatten_field_plans(
    fields: Box<[FlattenFieldPlan<BlockId>]>,
    refs: &BTreeMap<BlockId, ExecBlock>,
) -> Result<Box<[FlattenFieldPlan<ExecBlock>]>, DeserializeError> {
    fields
        .into_vec()
        .into_iter()
        .map(|field| {
            Ok(FlattenFieldPlan {
                field_index: field.field_index,
                kind: resolve_flatten_kind(field.kind, refs)?,
            })
        })
        .collect()
}

fn resolve_flatten_kind(
    kind: FlattenKind<BlockId>,
    refs: &BTreeMap<BlockId, ExecBlock>,
) -> Result<FlattenKind<ExecBlock>, DeserializeError> {
    Ok(match kind {
        FlattenKind::Struct { plan } => FlattenKind::Struct {
            plan: Box::new(resolve_flatten_struct_plan(*plan, refs)?),
        },
        FlattenKind::OptionStruct {
            option,
            inner_layout,
            plan,
        } => FlattenKind::OptionStruct {
            option,
            inner_layout,
            plan: Box::new(resolve_flatten_struct_plan(*plan, refs)?),
        },
        FlattenKind::ExternalEnum {
            enum_type,
            tag_key,
            content_key,
            variants,
        } => FlattenKind::ExternalEnum {
            enum_type,
            tag_key,
            content_key,
            variants: resolve_external_variant_plans(variants, refs)?,
        },
        FlattenKind::OptionExternalEnum {
            option,
            inner_layout,
            enum_type,
            tag_key,
            content_key,
            variants,
        } => FlattenKind::OptionExternalEnum {
            option,
            inner_layout,
            enum_type,
            tag_key,
            content_key,
            variants: resolve_external_variant_plans(variants, refs)?,
        },
        FlattenKind::Map {
            map,
            key_plan,
            value_program,
            value_scalar,
            value_layout,
        } => FlattenKind::Map {
            map,
            key_plan,
            value_program: resolve_json_program(value_program, refs)?,
            value_scalar,
            value_layout,
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
        .map(|variant| resolve_external_variant_plan(variant, refs))
        .collect()
}

fn resolve_external_variant_plan(
    variant: ExternalVariantPlan<BlockId>,
    refs: &BTreeMap<BlockId, ExecBlock>,
) -> Result<ExternalVariantPlan<ExecBlock>, DeserializeError> {
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
        flatten: variant
            .flatten
            .map(|plan| resolve_flatten_struct_plan(*plan, refs).map(Box::new))
            .transpose()?,
    })
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
            _ if field.should_skip_deserializing() && field_shape.is(Characteristic::Default) => {
                MissingField::DefaultTrait { explicit: false }
            }
            _ if container_has_default && field_shape.is(Characteristic::Default) => {
                MissingField::DefaultTrait { explicit: false }
            }
            _ if field_shape.is_type::<()>() => MissingField::DefaultTrait { explicit: false },
            _ => MissingField::Required,
        },
    }
}

fn is_empty_tuple_shape(shape: &'static Shape) -> bool {
    matches!(
        shape.ty,
        Type::User(UserType::Struct(struct_type))
            if struct_type.kind == StructKind::Tuple && struct_type.fields.is_empty()
    )
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

fn find_tagged_variant<S: AsRef<str>>(
    variants: &[ExternalVariantPlan<ExecBlock>],
    name: S,
) -> Option<&ExternalVariantPlan<ExecBlock>> {
    let name = name.as_ref();
    variants.iter().find(|variant| {
        !variant.variant.is_other()
            && !variant.variant.has_builtin_attr("untagged")
            && variant.variant.effective_name() == name
    })
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

fn numeric_variant(
    variants: &[ExternalVariantPlan<ExecBlock>],
    discriminant: i64,
) -> Option<&ExternalVariantPlan<ExecBlock>> {
    variants
        .iter()
        .find(|variant| variant.variant.discriminant == Some(discriminant))
}

fn numeric_discriminant_from_token(
    token: JsonScalarToken<'_>,
    span: Span,
) -> Result<i64, DeserializeError> {
    match token {
        JsonScalarToken::I64(discriminant) => Ok(discriminant),
        JsonScalarToken::U64(discriminant) => Ok(discriminant as i64),
        JsonScalarToken::Str(discriminant) => discriminant.parse().map_err(|_| {
            vm_error(
                Some(span),
                DeserializeErrorKind::UnexpectedToken {
                    expected: "string representing an integer (i64)",
                    got: discriminant.into_owned().into(),
                },
            )
        }),
        other => Err(vm_error(
            Some(span),
            DeserializeErrorKind::Unsupported {
                message: format!("unexpected {} scalar for numeric enum", other.kind_name()).into(),
            },
        )),
    }
}

fn scalar_matches_shape(scalar: &ScalarValue<'_>, shape: &'static Shape) -> bool {
    let Some(scalar_type) = shape.scalar_type() else {
        return matches!(scalar, ScalarValue::Null) && matches!(shape.def, Def::Option(_));
    };

    match scalar {
        ScalarValue::Bool(_) => matches!(scalar_type, ScalarType::Bool),
        ScalarValue::Char(_) => matches!(scalar_type, ScalarType::Char),
        ScalarValue::I64(value) => {
            if matches!(
                scalar_type,
                ScalarType::I8
                    | ScalarType::I16
                    | ScalarType::I32
                    | ScalarType::I64
                    | ScalarType::I128
                    | ScalarType::ISize
            ) {
                return true;
            }

            if *value < 0 {
                return false;
            }

            let unsigned = *value as u64;
            match scalar_type {
                ScalarType::U8 => unsigned <= u8::MAX as u64,
                ScalarType::U16 => unsigned <= u16::MAX as u64,
                ScalarType::U32 => unsigned <= u32::MAX as u64,
                ScalarType::U64 | ScalarType::U128 | ScalarType::USize => true,
                _ => false,
            }
        }
        ScalarValue::U64(value) => {
            if matches!(
                scalar_type,
                ScalarType::U8
                    | ScalarType::U16
                    | ScalarType::U32
                    | ScalarType::U64
                    | ScalarType::U128
                    | ScalarType::USize
            ) {
                return true;
            }

            if *value > i64::MAX as u64 {
                return false;
            }

            match scalar_type {
                ScalarType::I8 => *value <= i8::MAX as u64,
                ScalarType::I16 => *value <= i16::MAX as u64,
                ScalarType::I32 => *value <= i32::MAX as u64,
                ScalarType::I64 | ScalarType::I128 | ScalarType::ISize => true,
                _ => false,
            }
        }
        ScalarValue::U128(_) => matches!(scalar_type, ScalarType::U128 | ScalarType::I128),
        ScalarValue::I128(_) => matches!(scalar_type, ScalarType::I128 | ScalarType::U128),
        ScalarValue::F64(_) => matches!(scalar_type, ScalarType::F32 | ScalarType::F64),
        ScalarValue::Str(value) => {
            if matches!(
                scalar_type,
                ScalarType::String | ScalarType::Str | ScalarType::CowStr | ScalarType::Char
            ) {
                return true;
            }

            scalar_parse_probe(shape, value.as_ref())
        }
        ScalarValue::Null | ScalarValue::Unit => matches!(scalar_type, ScalarType::Unit),
        ScalarValue::Bytes(_) => false,
        _ => false,
    }
}

fn scalar_match_quality(scalar: &ScalarValue<'_>, shape: &'static Shape) -> Option<u8> {
    if !scalar_matches_shape(scalar, shape) {
        return None;
    }

    if scalar_exactly_matches_shape(scalar, shape) {
        Some(0)
    } else {
        Some(1)
    }
}

fn scalar_exactly_matches_shape(scalar: &ScalarValue<'_>, shape: &'static Shape) -> bool {
    let scalar_type = shape.scalar_type();

    match scalar {
        ScalarValue::Bool(_) => matches!(scalar_type, Some(ScalarType::Bool)),
        ScalarValue::Char(_) => matches!(scalar_type, Some(ScalarType::Char)),
        ScalarValue::I64(_) | ScalarValue::U64(_) | ScalarValue::U128(_) | ScalarValue::I128(_) => {
            matches!(
                scalar_type,
                Some(
                    ScalarType::U8
                        | ScalarType::U16
                        | ScalarType::U32
                        | ScalarType::U64
                        | ScalarType::U128
                        | ScalarType::USize
                        | ScalarType::I8
                        | ScalarType::I16
                        | ScalarType::I32
                        | ScalarType::I64
                        | ScalarType::I128
                        | ScalarType::ISize
                )
            )
        }
        ScalarValue::F64(_) => matches!(scalar_type, Some(ScalarType::F32 | ScalarType::F64)),
        ScalarValue::Str(_) => matches!(
            scalar_type,
            Some(ScalarType::String | ScalarType::Str | ScalarType::CowStr | ScalarType::Char)
        ),
        ScalarValue::Null => {
            matches!(scalar_type, Some(ScalarType::Unit))
                || (scalar_type.is_none() && matches!(shape.def, Def::Option(_)))
        }
        ScalarValue::Unit => matches!(scalar_type, Some(ScalarType::Unit)),
        ScalarValue::Bytes(_) => false,
        _ => false,
    }
}

fn scalar_parse_probe(shape: &'static Shape, value: &str) -> bool {
    const PARSE_PROBE_SIZE: usize = 128;

    #[repr(align(64))]
    struct ParseProbeStorage([u8; PARSE_PROBE_SIZE]);

    if !shape.vtable.has_parse() {
        return false;
    }

    let Ok(layout) = shape.layout.sized_layout() else {
        return false;
    };

    if layout.size() > PARSE_PROBE_SIZE
        || layout.align() > core::mem::align_of::<ParseProbeStorage>()
    {
        return false;
    }

    let mut temp = MaybeUninit::<ParseProbeStorage>::uninit();
    let temp_bytes_ptr = unsafe { core::ptr::addr_of_mut!((*temp.as_mut_ptr()).0) };
    let temp_ptr = PtrUninit::new(temp_bytes_ptr.cast::<u8>());

    if let Some(Ok(())) = unsafe { shape.call_parse(value, temp_ptr) } {
        unsafe {
            let _ = shape.call_drop_in_place(temp_ptr.assume_init());
        }
        true
    } else {
        false
    }
}

fn single_field_variant_shape(variant: &ExternalVariantPlan<ExecBlock>) -> Option<&'static Shape> {
    match variant.variant.data.kind {
        StructKind::Tuple | StructKind::TupleStruct if variant.fields.len() == 1 => {
            Some(variant.fields[0].shape)
        }
        _ => None,
    }
}

fn untagged_scalar_variant<'a>(
    variants: &'a [ExternalVariantPlan<ExecBlock>],
    scalar: &ScalarValue<'_>,
) -> Option<&'a ExternalVariantPlan<ExecBlock>> {
    if matches!(scalar, ScalarValue::Null)
        && let Some(variant) = variants
            .iter()
            .find(|variant| variant.variant.data.kind == StructKind::Unit)
    {
        return Some(variant);
    }

    if let ScalarValue::Str(name) = scalar
        && let Some(variant) = variants.iter().find(|variant| {
            variant.variant.data.kind == StructKind::Unit
                && name.as_ref() == variant.variant.effective_name()
        })
    {
        return Some(variant);
    }

    let mut best: Option<(&ExternalVariantPlan<ExecBlock>, u8)> = None;
    for variant in variants {
        let Some(shape) = single_field_variant_shape(variant) else {
            continue;
        };
        let Some(quality) = scalar_match_quality(scalar, shape) else {
            continue;
        };
        if best.is_none_or(|(_, best_quality)| quality < best_quality) {
            best = Some((variant, quality));
        }
    }

    if let Some((variant, _)) = best {
        return Some(variant);
    }

    variants
        .iter()
        .find(|variant| single_field_variant_shape(variant).is_some())
}

fn untagged_struct_variant<'a>(
    shape: &'static Shape,
    variants: &'a [ExternalVariantPlan<ExecBlock>],
    fields: &[TaggedRawField<'_>],
) -> Result<&'a ExternalVariantPlan<ExecBlock>, DeserializeError> {
    let mut struct_variants = variants
        .iter()
        .filter(|variant| variant.variant.data.kind == StructKind::Struct);
    if let Some(variant) = struct_variants.next()
        && struct_variants.next().is_none()
        && !struct_variant_required_missing(variant, fields)
    {
        return Ok(variant);
    }

    let mut best: Option<(&ExternalVariantPlan<ExecBlock>, usize, usize)> = None;
    let mut structural_best: Option<(&ExternalVariantPlan<ExecBlock>, usize, usize)> = None;

    for variant in variants
        .iter()
        .filter(|variant| variant.variant.data.kind == StructKind::Struct)
    {
        if struct_variant_required_missing(variant, fields) {
            continue;
        }

        let mut matched = 0usize;
        let mut quality = 0usize;
        let mut viable = true;
        for raw_field in fields {
            let Some(field) = variant
                .fields
                .iter()
                .find(|field| field.matches_key_bytes(raw_field.name.as_ref().as_bytes()))
            else {
                continue;
            };
            matched += 1;

            if ScalarType::try_from_shape(field.shape).is_none() {
                continue;
            }

            let Some(scalar) = raw_scalar_value(raw_field.raw)? else {
                viable = false;
                break;
            };
            let Some(field_quality) = scalar_match_quality(&scalar, field.shape) else {
                viable = false;
                break;
            };
            quality += usize::from(field_quality);
        }

        if structural_best.is_none_or(|(_, best_matched, best_quality)| {
            matched > best_matched || (matched == best_matched && quality < best_quality)
        }) {
            structural_best = Some((variant, matched, quality));
        }

        if !viable {
            continue;
        }

        if best.is_none_or(|(_, best_matched, best_quality)| {
            matched > best_matched || (matched == best_matched && quality < best_quality)
        }) {
            best = Some((variant, matched, quality));
        }
    }

    best.or(structural_best)
        .map(|(variant, _, _)| variant)
        .ok_or_else(|| {
            vm_error(
                None,
                DeserializeErrorKind::NoMatchingVariant {
                    enum_shape: shape,
                    input_kind: "struct",
                },
            )
        })
}

fn struct_variant_required_missing(
    variant: &ExternalVariantPlan<ExecBlock>,
    fields: &[TaggedRawField<'_>],
) -> bool {
    variant.fields.iter().any(|field| {
        matches!(field.missing, MissingField::Required)
            && !fields
                .iter()
                .any(|raw_field| field.matches_key_bytes(raw_field.name.as_ref().as_bytes()))
    })
}

fn untagged_tuple_variant<'a>(
    shape: &'static Shape,
    variants: &'a [ExternalVariantPlan<ExecBlock>],
    arity: usize,
) -> Result<&'a ExternalVariantPlan<ExecBlock>, DeserializeError> {
    variants
        .iter()
        .find(|variant| {
            matches!(
                variant.variant.data.kind,
                StructKind::Tuple | StructKind::TupleStruct
            ) && (variant.fields.len() == arity
                || single_field_array_arity(variant).is_some_and(|len| len == arity))
        })
        .ok_or_else(|| {
            vm_error(
                None,
                DeserializeErrorKind::NoMatchingVariant {
                    enum_shape: shape,
                    input_kind: "sequence",
                },
            )
        })
}

fn field_plan_match_score(
    fields: &[FieldPlan<ExecBlock>],
    raw_fields: &[TaggedRawField<'_>],
) -> Result<Option<(usize, usize)>, DeserializeError> {
    let mut matched = 0usize;
    let mut quality = 0usize;

    for raw_field in raw_fields {
        let Some(field) = fields
            .iter()
            .find(|field| field.matches_key_bytes(raw_field.name.as_ref().as_bytes()))
        else {
            continue;
        };
        matched += 1;

        if ScalarType::try_from_shape(field.shape).is_none() {
            continue;
        }

        let Some(scalar) = raw_scalar_value(raw_field.raw)? else {
            return Ok(None);
        };
        let Some(field_quality) = scalar_match_quality(&scalar, field.shape) else {
            return Ok(None);
        };
        quality += usize::from(field_quality);
    }

    Ok((matched > 0).then_some((matched, quality)))
}

fn scalar_field_plan_match_score(
    fields: &[ScalarFieldPlan],
    raw_fields: &[TaggedRawField<'_>],
) -> Result<Option<(usize, usize)>, DeserializeError> {
    let mut matched = 0usize;
    let mut quality = 0usize;

    for raw_field in raw_fields {
        let Some(field) = fields
            .iter()
            .find(|field| field.matches_key_bytes(raw_field.name.as_ref().as_bytes()))
        else {
            continue;
        };
        matched += 1;

        let Some(scalar) = raw_scalar_value(raw_field.raw)? else {
            return Ok(None);
        };
        let Some(field_quality) = scalar_match_quality(&scalar, field.shape) else {
            return Ok(None);
        };
        quality += usize::from(field_quality);
    }

    Ok((matched > 0).then_some((matched, quality)))
}

fn single_field_array_arity(variant: &ExternalVariantPlan<ExecBlock>) -> Option<usize> {
    let shape = single_field_variant_shape(variant)?;
    match shape.def {
        Def::Array(array) => Some(array.n),
        _ => None,
    }
}

fn raw_scalar_value(raw: &str) -> Result<Option<ScalarValue<'_>>, DeserializeError> {
    let mut parser = JsonParser::<true>::new(raw.as_bytes());
    let Some(event) = parser.peek_event()? else {
        return Ok(None);
    };
    match event.kind {
        ParseEventKind::Scalar(scalar) => Ok(Some(scalar)),
        _ => Ok(None),
    }
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

#[cold]
#[inline(never)]
fn validate_completed_shape(
    shape: &'static Shape,
    base: PtrUninit,
) -> Result<(), DeserializeError> {
    if let Some(Err(message)) = unsafe { shape.call_invariants(base.assume_init().as_const()) } {
        return Err(vm_error(
            None,
            DeserializeErrorKind::InvalidValue {
                message: message.into(),
            },
        ));
    }
    Ok(())
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
    arrays: InlineStack<ArrayFrame>,
    dynamic_values: InlineStack<DynamicFrame>,
    lists: InlineStack<ListFrame>,
    pointer_slices: InlineStack<PointerSliceFrame>,
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
            arrays: InlineStack::new(),
            dynamic_values: InlineStack::new(),
            lists: InlineStack::new(),
            pointer_slices: InlineStack::new(),
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

    fn read_unit_struct(&mut self, shape: &'static Shape) -> Result<(), DeserializeError> {
        self.parser.consume_object_start_fast()?;
        loop {
            match self.parser.next_object_key_or_end()? {
                JsonObjectKeyStep::End => return Ok(()),
                JsonObjectKeyStep::Field { key, span } => {
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
                }
            }
        }
    }

    fn read_tuple_struct(
        &mut self,
        shape: &'static Shape,
        fields: &'program [FieldPlan<ExecBlock>],
        tracking: StructTracking,
    ) -> Result<Control<'program, ExecBlock, ExecOp, Continuation<'program>>, DeserializeError>
    {
        if fields.is_empty() && self.parser.consume_null_if_next()? {
            return Ok(Control::Continue);
        }

        let Some(event) = self.parser.peek_event()? else {
            return Err(vm_error(
                None,
                DeserializeErrorKind::UnexpectedEof { expected: "tuple" },
            ));
        };

        let mode = match event.kind {
            ParseEventKind::SequenceStart(_) => {
                self.parser.consume_array_start_fast()?;
                TupleContainerMode::Sequence
            }
            ParseEventKind::StructStart(_) => {
                self.parser.consume_object_start_fast()?;
                TupleContainerMode::Object
            }
            _ => {
                return Err(vm_error(
                    Some(event.span),
                    DeserializeErrorKind::UnexpectedToken {
                        expected: "sequence or object start for tuple",
                        got: event.kind_name().into(),
                    },
                ));
            }
        };

        self.push_struct_frame(shape, fields, None, tracking);
        self.read_tuple_struct_next(fields, tracking, 0, mode, shape.vtable.has_invariants())
    }

    fn read_tuple_struct_next(
        &mut self,
        fields: &'program [FieldPlan<ExecBlock>],
        tracking: StructTracking,
        mut next_index: usize,
        mode: TupleContainerMode,
        validate: bool,
    ) -> Result<Control<'program, ExecBlock, ExecOp, Continuation<'program>>, DeserializeError>
    {
        loop {
            let Some(field) = fields.get(next_index) else {
                match mode {
                    TupleContainerMode::Sequence => {
                        if self.parser.consume_sequence_end_if_next()? {
                            unsafe {
                                self.finish_struct_frame_with_validation(tracking, validate)?;
                            }
                            return Ok(Control::Continue);
                        }
                        let got = match self.parser.peek_event()? {
                            Some(event) => event.kind_name().into(),
                            None => "end of input".into(),
                        };
                        return Err(vm_error(
                            None,
                            DeserializeErrorKind::UnexpectedToken {
                                expected: "sequence end for tuple",
                                got,
                            },
                        ));
                    }
                    TupleContainerMode::Object => match self.parser.next_object_key_or_end()? {
                        JsonObjectKeyStep::End => {
                            unsafe {
                                self.finish_struct_frame_with_validation(tracking, validate)?;
                            }
                            return Ok(Control::Continue);
                        }
                        JsonObjectKeyStep::Field { span, .. } => {
                            return Err(vm_error(
                                Some(span),
                                DeserializeErrorKind::UnexpectedToken {
                                    expected: "object end for tuple",
                                    got: "field key".into(),
                                },
                            ));
                        }
                    },
                }
            };

            let span = match mode {
                TupleContainerMode::Sequence => {
                    if self.parser.consume_sequence_end_if_next()? {
                        return Err(vm_error(
                            None,
                            DeserializeErrorKind::UnexpectedEof {
                                expected: "tuple element",
                            },
                        ));
                    }
                    self.parser
                        .peek_event()?
                        .map_or(Span { offset: 0, len: 0 }, |event| event.span)
                }
                TupleContainerMode::Object => match self.parser.next_object_key_or_end()? {
                    JsonObjectKeyStep::End => {
                        return Err(vm_error(
                            None,
                            DeserializeErrorKind::UnexpectedEof {
                                expected: "tuple element",
                            },
                        ));
                    }
                    JsonObjectKeyStep::Field { span, .. } => span,
                },
            };

            let field_ptr = unsafe { self.base.field_uninit(field.offset) };
            if let Some(scalar) = field.scalar {
                let (value, value_span) = self.parser.read_scalar_token()?;
                unsafe {
                    scalar.write(field.shape, field_ptr, value, value_span)?;
                }
                self.mark_struct_field(tracking, next_index, span);
                next_index += 1;
                continue;
            }

            let old_base = self.base;
            self.base = field_ptr;
            return Ok(call_program_or_block_then(
                &field.program,
                Continuation::TupleFieldDone {
                    tracking,
                    fields,
                    index: next_index,
                    old_base,
                    mode,
                    validate,
                },
            ));
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

    fn finish_proxy(
        &mut self,
        proxy: &'static ProxyDef,
        target_ptr: PtrUninit,
        scratch: ScratchSlot,
    ) -> Result<(), DeserializeError> {
        let result = unsafe {
            let proxy_ptr = scratch_ptr_uninit(&scratch).assume_init().as_const();
            (proxy.convert_in)(proxy_ptr, target_ptr)
        };
        self.scratch.release(scratch);

        match result {
            Ok(ptr) if ptr.as_uninit() == target_ptr => Ok(()),
            Ok(_) => Err(vm_error(
                None,
                DeserializeErrorKind::InvalidValue {
                    message: "proxy conversion returned an unexpected pointer".into(),
                },
            )),
            Err(message) => Err(vm_error(
                None,
                DeserializeErrorKind::InvalidValue {
                    message: message.into(),
                },
            )),
        }
    }

    fn finish_builder_shape(
        &mut self,
        shape: &'static Shape,
        builder_shape: &'static Shape,
        target_ptr: PtrUninit,
        scratch: ScratchSlot,
    ) -> Result<(), DeserializeError> {
        let builder_ptr = unsafe { scratch_ptr_uninit(&scratch).assume_init().as_const() };
        let outcome = unsafe { shape.call_try_from(builder_shape, builder_ptr, target_ptr) };

        match outcome {
            Some(TryFromOutcome::Converted) => {
                self.scratch.release(scratch);
                Ok(())
            }
            Some(TryFromOutcome::Failed(message)) => {
                self.scratch.release(scratch);
                Err(vm_error(
                    None,
                    DeserializeErrorKind::InvalidValue { message },
                ))
            }
            Some(TryFromOutcome::Unsupported) | Some(_) | None => {
                unsafe {
                    drop_shape_value(builder_shape as *const Shape as *const (), scratch.ptr());
                }
                self.scratch.release(scratch);
                Err(unsupported(shape, "builder-shape conversion"))
            }
        }
    }

    fn array_slot_or_finish(&mut self) -> Result<ArraySlot, DeserializeError> {
        let frame = self
            .arrays
            .last()
            .expect("array frame is present while decoding element");
        if let Some(slot) = frame.next_slot() {
            return Ok(ArraySlot::Slot(slot));
        }

        if self.parser.consume_sequence_end_if_next()? {
            return Ok(ArraySlot::Done);
        }

        let Some(event) = self.parser.peek_event()? else {
            return Err(vm_error(
                None,
                DeserializeErrorKind::UnexpectedEof {
                    expected: "array end",
                },
            ));
        };
        Err(vm_error(
            Some(event.span),
            DeserializeErrorKind::UnexpectedToken {
                expected: "array end",
                got: event.kind_name().into(),
            },
        ))
    }

    unsafe fn mark_array_element_initialized(&mut self) {
        let frame = self
            .arrays
            .last_mut()
            .expect("array frame is present after element initialization");
        unsafe {
            frame.mark_initialized();
        }
    }

    unsafe fn finish_struct_frame(
        &mut self,
        tracking: StructTracking,
    ) -> Result<(), DeserializeError> {
        unsafe { self.finish_struct_frame_with_validation(tracking, true) }
    }

    unsafe fn finish_struct_frame_unchecked(
        &mut self,
        tracking: StructTracking,
    ) -> Result<(), DeserializeError> {
        unsafe { self.finish_struct_frame_with_validation(tracking, false) }
    }

    unsafe fn finish_struct_frame_with_validation(
        &mut self,
        tracking: StructTracking,
        validate: bool,
    ) -> Result<(), DeserializeError> {
        match tracking {
            StructTracking::Inline => {
                let frame = self
                    .inline_structs
                    .pop()
                    .expect("inline struct frame is present after struct program");
                unsafe {
                    frame.fill_missing_fields(validate)?;
                }
            }
            StructTracking::Bitset | StructTracking::Heap => {
                let frame = self
                    .large_structs_mut()
                    .pop()
                    .expect("large struct frame is present after struct program");
                unsafe {
                    frame.fill_missing_fields(validate)?;
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

    fn collect_tagged_raw_fields_from_raw(
        raw: &'de str,
        expected: &'static str,
    ) -> Result<Vec<TaggedRawField<'de>>, DeserializeError> {
        let mut parser = JsonParser::<true>::new(raw.as_bytes());
        let fields = {
            let mut interp: JsonInterp<'_, 'de, 'program, true> =
                JsonInterp::<'_, 'de, 'program, true>::new(
                    &mut parser,
                    PtrUninit::new(core::ptr::null_mut::<u8>()),
                    &[],
                );
            interp.collect_tagged_raw_fields(expected)?
        };
        Self::ensure_raw_parser_finished(&mut parser)?;
        Ok(fields)
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

    fn captured_struct_field<'fields>(
        fields: &'fields [FieldPlan<ExecBlock>],
        dispatch: Option<&RawFieldDispatch>,
        key: &[u8],
    ) -> Option<(usize, &'fields FieldPlan<ExecBlock>)> {
        if let Some(dispatch) = dispatch {
            let mut candidates = dispatch.candidates(key);
            while candidates != 0 {
                let index = candidates.trailing_zeros() as usize;
                candidates &= candidates - 1;
                let field = &fields[index];
                if field.matches_key_bytes(key) {
                    return Some((index, field));
                }
            }
            return None;
        }

        fields
            .iter()
            .enumerate()
            .find(|(_, field)| field.matches_key_bytes(key))
    }

    fn captured_scalar_field(
        fields: &[ScalarFieldPlan],
        dispatch: Option<&RawFieldDispatch>,
        key: &[u8],
    ) -> Option<(usize, ScalarFieldPlan)> {
        if let Some(dispatch) = dispatch {
            let mut candidates = dispatch.candidates(key);
            while candidates != 0 {
                let index = candidates.trailing_zeros() as usize;
                candidates &= candidates - 1;
                let field = fields[index];
                if field.matches_key_bytes(key) {
                    return Some((index, field));
                }
            }
            return None;
        }

        fields
            .iter()
            .copied()
            .enumerate()
            .find(|(_, field)| field.matches_key_bytes(key))
    }

    fn flatten_direct_field<'plan>(
        plan: &'plan FlattenStructPlan<ExecBlock>,
        key: &[u8],
    ) -> Option<(usize, &'plan FieldPlan<ExecBlock>)> {
        plan.direct_indices.iter().copied().find_map(|index| {
            let field = &plan.fields[index];
            field.matches_key_bytes(key).then_some((index, field))
        })
    }

    fn flatten_struct_matches_key(plan: &FlattenStructPlan<ExecBlock>, key: &[u8]) -> bool {
        Self::flatten_direct_field(plan, key).is_some()
            || plan
                .flattened
                .iter()
                .any(|field| Self::flatten_kind_matches_key(&field.kind, key))
    }

    fn flatten_kind_matches_key(kind: &FlattenKind<ExecBlock>, key: &[u8]) -> bool {
        match kind {
            FlattenKind::Struct { plan } | FlattenKind::OptionStruct { plan, .. } => {
                Self::flatten_struct_matches_key(plan, key)
            }
            FlattenKind::ExternalEnum {
                tag_key, variants, ..
            }
            | FlattenKind::OptionExternalEnum {
                tag_key, variants, ..
            } => {
                if let Some(tag_key) = tag_key {
                    tag_key.as_bytes() == key
                        || variants.iter().any(|variant| {
                            variant
                                .fields
                                .iter()
                                .any(|field| field.matches_key_bytes(key))
                        })
                } else {
                    variants.iter().any(|variant| {
                        !variant.variant.is_other()
                            && variant.variant.effective_name().as_bytes() == key
                    })
                }
            }
            FlattenKind::Map { .. } => true,
        }
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

    fn write_raw_map_value(
        &mut self,
        value_shape: &'static Shape,
        value_program: &'program [ExecOp],
        value_scalar: Option<ScalarPlan>,
        dst: PtrUninit,
        raw: &'de str,
    ) -> Result<(), DeserializeError> {
        let mut parser = JsonParser::<true>::new(raw.as_bytes());
        if let Some(scalar) = value_scalar {
            let (value, span) = parser.read_scalar_token()?;
            unsafe {
                scalar.write(value_shape, dst, value, span)?;
            }
            Self::ensure_raw_parser_finished(&mut parser)?;
            return Ok(());
        }

        {
            let mut interp: JsonInterp<'_, 'de, 'program, true> =
                JsonInterp::<'_, 'de, 'program, true>::new(&mut parser, dst, self.blocks);
            run_dense_program(value_program, self.blocks, &mut interp).map_err(run_error)?;
            interp.finish_success();
        }
        Self::ensure_raw_parser_finished(&mut parser)
    }

    fn read_captured_record_fields(
        &mut self,
        shape: &'static Shape,
        record_fields: &'program [FieldPlan<ExecBlock>],
        dispatch: Option<&RawFieldDispatch>,
        tracking: StructTracking,
        fields: &[TaggedRawField<'de>],
        skip_keys: &[&'static str],
    ) -> Result<(), DeserializeError> {
        for raw_field in fields {
            if skip_keys.iter().any(|key| raw_field.name.as_ref() == *key) {
                continue;
            }

            let Some((index, field)) = Self::captured_struct_field(
                record_fields,
                dispatch,
                raw_field.name.as_ref().as_bytes(),
            ) else {
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

            if let Some(first_span) = self.struct_seen_span(tracking, index) {
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
            self.mark_struct_field(tracking, index, raw_field.span);
        }

        unsafe {
            self.finish_struct_frame_with_validation(tracking, shape.vtable.has_invariants())?;
        }
        Ok(())
    }

    fn read_captured_variant_fields(
        &mut self,
        shape: &'static Shape,
        variant: &'program ExternalVariantPlan<ExecBlock>,
        fields: &[TaggedRawField<'de>],
        skip_keys: &[&'static str],
    ) -> Result<(), DeserializeError> {
        self.read_captured_record_fields(
            shape,
            &variant.fields,
            variant.dispatch.as_ref(),
            variant.tracking,
            fields,
            skip_keys,
        )
    }

    fn read_captured_scalar_struct(
        &mut self,
        shape: &'static Shape,
        fields: &'program [ScalarFieldPlan],
        dispatch: Option<&'program RawFieldDispatch>,
        raw_fields: &[TaggedRawField<'de>],
        skip_keys: &[&'static str],
    ) -> Result<(), DeserializeError> {
        match StructTracking::for_len(fields.len()) {
            StructTracking::Inline => {
                let mut frame = StructFrame::<ScalarFieldPlan, InitializedLedger<Span>>::new(
                    shape, self.base, fields, dispatch,
                );
                self.read_captured_scalar_struct_frame(shape, &mut frame, raw_fields, skip_keys)?;
                unsafe {
                    frame.fill_missing_fields(shape.vtable.has_invariants())?;
                }
            }
            StructTracking::Bitset => {
                let mut frame = StructFrame::<ScalarFieldPlan, BitsetStructSeen>::new(
                    shape, self.base, fields, dispatch,
                );
                self.read_captured_scalar_struct_frame(shape, &mut frame, raw_fields, skip_keys)?;
                unsafe {
                    frame.fill_missing_fields(shape.vtable.has_invariants())?;
                }
            }
            StructTracking::Heap => {
                let mut frame = StructFrame::<ScalarFieldPlan, HeapStructSeen>::new(
                    shape, self.base, fields, dispatch,
                );
                self.read_captured_scalar_struct_frame(shape, &mut frame, raw_fields, skip_keys)?;
                unsafe {
                    frame.fill_missing_fields(shape.vtable.has_invariants())?;
                }
            }
        }
        Ok(())
    }

    fn read_captured_scalar_struct_frame<Seen: StructSeenStore>(
        &mut self,
        shape: &'static Shape,
        frame: &mut StructFrame<'program, ScalarFieldPlan, Seen>,
        raw_fields: &[TaggedRawField<'de>],
        skip_keys: &[&'static str],
    ) -> Result<(), DeserializeError> {
        for raw_field in raw_fields {
            if skip_keys.iter().any(|key| raw_field.name.as_ref() == *key) {
                continue;
            }

            let Some((index, field)) = Self::captured_scalar_field(
                frame.fields,
                frame.dispatch,
                raw_field.name.as_ref().as_bytes(),
            ) else {
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

            if let Some(first_span) = frame.seen.get(index) {
                return Err(vm_error(
                    Some(raw_field.span),
                    DeserializeErrorKind::DuplicateField {
                        field: field.name.into(),
                        first_span: Some(first_span),
                    },
                ));
            }

            let mut parser = JsonParser::<true>::new(raw_field.raw.as_bytes());
            let (value, value_span) = parser.read_scalar_token()?;
            let field_ptr = unsafe { frame.base.field_uninit(field.offset) };
            unsafe {
                field
                    .scalar
                    .write(field.shape, field_ptr, value, value_span)?;
            }
            Self::ensure_raw_parser_finished(&mut parser)?;
            frame.mark_seen(index, raw_field.span);
        }

        Ok(())
    }

    fn read_flatten_struct(
        &mut self,
        shape: &'static Shape,
        plan: &'program FlattenStructPlan<ExecBlock>,
    ) -> Result<(), DeserializeError> {
        let fields = self.collect_tagged_raw_fields("struct with flattened fields")?;
        let mut claimed = vec![None; fields.len()];
        self.read_flatten_struct_from_fields(shape, plan, &fields, &mut claimed, true)?;
        Ok(())
    }

    fn read_flatten_struct_from_fields(
        &mut self,
        shape: &'static Shape,
        plan: &'program FlattenStructPlan<ExecBlock>,
        fields: &[TaggedRawField<'de>],
        claimed: &mut [Option<Span>],
        reject_unknowns: bool,
    ) -> Result<usize, DeserializeError> {
        self.push_struct_frame(shape, &plan.fields, None, plan.tracking);
        let matched = self.read_flatten_struct_contents(shape, plan, fields, claimed);
        if matched.is_ok() && reject_unknowns {
            self.reject_unclaimed_flatten_fields(shape, fields, claimed)?;
        }
        let matched = matched?;
        unsafe {
            self.finish_struct_frame(plan.tracking)?;
        }
        Ok(matched)
    }

    fn read_flatten_struct_contents(
        &mut self,
        shape: &'static Shape,
        plan: &'program FlattenStructPlan<ExecBlock>,
        fields: &[TaggedRawField<'de>],
        claimed: &mut [Option<Span>],
    ) -> Result<usize, DeserializeError> {
        let mut matched = 0usize;
        for (raw_index, raw_field) in fields.iter().enumerate() {
            let Some((field_index, field)) =
                Self::flatten_direct_field(plan, raw_field.name.as_ref().as_bytes())
            else {
                continue;
            };

            self.ensure_flatten_raw_unclaimed(raw_field, claimed[raw_index])?;
            if let Some(first_span) = self.struct_seen_span(plan.tracking, field_index) {
                return Err(Self::duplicate_flatten_field(raw_field, first_span));
            }

            let field_ptr = unsafe { self.base.field_uninit(field.offset) };
            self.write_raw_field_value(field, field_ptr, raw_field.raw)?;
            self.mark_struct_field(plan.tracking, field_index, raw_field.span);
            claimed[raw_index] = Some(raw_field.span);
            matched += 1;
        }

        for flattened in &plan.flattened {
            if matches!(flattened.kind, FlattenKind::Map { .. }) {
                continue;
            }
            matched += self.read_flatten_field(shape, plan, flattened, fields, claimed)?;
        }

        for flattened in &plan.flattened {
            if !matches!(flattened.kind, FlattenKind::Map { .. }) {
                continue;
            }
            matched += self.read_flatten_field(shape, plan, flattened, fields, claimed)?;
        }

        Ok(matched)
    }

    fn read_flatten_field(
        &mut self,
        _shape: &'static Shape,
        parent: &'program FlattenStructPlan<ExecBlock>,
        flattened: &'program FlattenFieldPlan<ExecBlock>,
        fields: &[TaggedRawField<'de>],
        claimed: &mut [Option<Span>],
    ) -> Result<usize, DeserializeError> {
        let field = &parent.fields[flattened.field_index];
        match &flattened.kind {
            FlattenKind::Struct { plan } => {
                let field_ptr = unsafe { self.base.field_uninit(field.offset) };
                let span =
                    Self::first_unclaimed_flatten_kind_match_span(&flattened.kind, fields, claimed);
                if span.is_none()
                    && field.shape.is(Characteristic::Default)
                    && unsafe { field.shape.call_default_in_place(field_ptr) }.is_some()
                {
                    self.mark_flatten_parent_field(
                        parent.tracking,
                        flattened.field_index,
                        field,
                        Span::default(),
                    )?;
                    return Ok(0);
                }

                let old_base = self.base;
                self.base = field_ptr;
                let result =
                    self.read_flatten_struct_from_fields(field.shape, plan, fields, claimed, false);
                self.base = old_base;
                let matched = result?;
                let span = span.unwrap_or_default();
                self.mark_flatten_parent_field(
                    parent.tracking,
                    flattened.field_index,
                    field,
                    span,
                )?;
                Ok(matched)
            }
            FlattenKind::OptionStruct {
                option,
                inner_layout,
                plan,
            } => {
                let Some(span) = Self::first_flatten_kind_match_span(&flattened.kind, fields)
                else {
                    return Ok(0);
                };

                let scratch = self.scratch.reserve(*inner_layout);
                let old_base = self.base;
                self.base = scratch_ptr_uninit(&scratch);
                let result =
                    self.read_flatten_struct_from_fields(option.t(), plan, fields, claimed, false);
                self.base = old_base;
                let matched = result?;
                unsafe {
                    (option.vtable.init_some)(
                        self.base.field_uninit(field.offset),
                        scratch_ptr_mut(&scratch),
                    );
                }
                self.scratch.release(scratch);
                self.mark_flatten_parent_field(
                    parent.tracking,
                    flattened.field_index,
                    field,
                    span,
                )?;
                Ok(matched)
            }
            FlattenKind::ExternalEnum {
                enum_type,
                tag_key,
                content_key,
                variants,
            } => self.read_flatten_external_enum_field(
                parent,
                flattened.field_index,
                field,
                FlattenExternalEnumRef {
                    enum_type: *enum_type,
                    tag_key: *tag_key,
                    content_key: *content_key,
                    variants,
                },
                fields,
                claimed,
            ),
            FlattenKind::OptionExternalEnum {
                option,
                inner_layout,
                enum_type,
                tag_key,
                content_key,
                variants,
            } => {
                let scratch = self.scratch.reserve(*inner_layout);
                let old_base = self.base;
                self.base = scratch_ptr_uninit(&scratch);
                let result = self.read_flatten_enum_value(
                    option.t(),
                    FlattenExternalEnumRef {
                        enum_type: *enum_type,
                        tag_key: *tag_key,
                        content_key: *content_key,
                        variants,
                    },
                    fields,
                    claimed,
                );
                self.base = old_base;
                let Some((matched, span)) = result? else {
                    self.scratch.release(scratch);
                    return Ok(0);
                };
                unsafe {
                    (option.vtable.init_some)(
                        self.base.field_uninit(field.offset),
                        scratch_ptr_mut(&scratch),
                    );
                }
                self.scratch.release(scratch);

                self.mark_flatten_parent_field(
                    parent.tracking,
                    flattened.field_index,
                    field,
                    span,
                )?;
                Ok(matched)
            }
            FlattenKind::Map {
                map,
                key_plan,
                value_program,
                value_scalar,
                value_layout,
            } => self.read_flatten_map_field(
                parent,
                flattened.field_index,
                field,
                FlattenMapRef {
                    map: *map,
                    key_plan,
                    value_program,
                    value_scalar: *value_scalar,
                    value_layout: *value_layout,
                },
                fields,
                claimed,
            ),
        }
    }

    fn read_flatten_map_field(
        &mut self,
        parent: &'program FlattenStructPlan<ExecBlock>,
        field_index: usize,
        field: &'program FieldPlan<ExecBlock>,
        map_ref: FlattenMapRef<'program>,
        fields: &[TaggedRawField<'de>],
        claimed: &mut [Option<Span>],
    ) -> Result<usize, DeserializeError> {
        if let Some(first_span) = self.struct_seen_span(parent.tracking, field_index) {
            return Err(vm_error(
                Some(first_span),
                DeserializeErrorKind::DuplicateField {
                    field: field.name.into(),
                    first_span: Some(first_span),
                },
            ));
        }

        let field_ptr = unsafe { self.base.field_uninit(field.offset) };
        let map_ptr = unsafe { (map_ref.map.vtable.init_in_place_with_capacity)(field_ptr, 0) };
        let span = fields
            .iter()
            .enumerate()
            .find(|(index, _)| claimed[*index].is_none())
            .map_or_else(Span::default, |(_, raw_field)| raw_field.span);
        self.mark_struct_field(parent.tracking, field_index, span);

        let mut matched = 0usize;
        for (raw_index, raw_field) in fields.iter().enumerate() {
            if claimed[raw_index].is_some() {
                continue;
            }

            let value_scratch = self.scratch.reserve(map_ref.value_layout);
            let write_result = self.write_raw_map_value(
                map_ref.map.v(),
                map_ref.value_program,
                map_ref.value_scalar,
                scratch_ptr_uninit(&value_scratch),
                raw_field.raw,
            );
            if let Err(err) = write_result {
                self.scratch.release(value_scratch);
                return Err(err);
            }
            self.insert_map_entry_into(
                map_ref.map,
                map_ptr,
                map_ref.key_plan,
                raw_field.name.to_string(),
                raw_field.span,
                value_scratch,
            )?;
            claimed[raw_index] = Some(raw_field.span);
            matched += 1;
        }

        Ok(matched)
    }

    fn read_flatten_external_enum_field(
        &mut self,
        parent: &'program FlattenStructPlan<ExecBlock>,
        field_index: usize,
        field: &'program FieldPlan<ExecBlock>,
        enum_ref: FlattenExternalEnumRef<'program>,
        fields: &[TaggedRawField<'de>],
        claimed: &mut [Option<Span>],
    ) -> Result<usize, DeserializeError> {
        let field_ptr = unsafe { self.base.field_uninit(field.offset) };
        let old_base = self.base;
        self.base = field_ptr;
        let result = self.read_flatten_enum_value(field.shape, enum_ref, fields, claimed);
        self.base = old_base;
        let Some((matched, span)) = result? else {
            return Ok(0);
        };

        self.mark_flatten_parent_field(parent.tracking, field_index, field, span)?;
        Ok(matched)
    }

    fn read_flatten_enum_value(
        &mut self,
        shape: &'static Shape,
        enum_ref: FlattenExternalEnumRef<'program>,
        fields: &[TaggedRawField<'de>],
        claimed: &mut [Option<Span>],
    ) -> Result<Option<(usize, Span)>, DeserializeError> {
        if enum_ref.content_key.is_some() {
            return Err(unsupported(shape, "adjacently tagged flattened enum"));
        }

        match enum_ref.tag_key {
            Some(tag_key) => {
                self.read_flatten_internal_enum_value(shape, enum_ref, tag_key, fields, claimed)
            }
            None => {
                let Some((raw_index, raw_field, variant)) =
                    self.select_flatten_external_variant(enum_ref.variants, fields, claimed)?
                else {
                    return Ok(None);
                };

                self.read_flatten_external_variant_payload(
                    shape,
                    enum_ref.enum_type,
                    variant,
                    raw_field,
                )?;
                claimed[raw_index] = Some(raw_field.span);
                Ok(Some((1, raw_field.span)))
            }
        }
    }

    fn read_flatten_internal_enum_value(
        &mut self,
        shape: &'static Shape,
        enum_ref: FlattenExternalEnumRef<'program>,
        tag_key: &'static str,
        fields: &[TaggedRawField<'de>],
        claimed: &mut [Option<Span>],
    ) -> Result<Option<(usize, Span)>, DeserializeError> {
        let Some((tag_index, tag_field)) =
            self.select_flatten_internal_tag_field(fields, claimed, tag_key)?
        else {
            return Ok(None);
        };

        let tag = Self::read_raw_tag_name(tag_field, tag_key)?;
        let variant = find_tagged_variant(enum_ref.variants, &tag)
            .or_else(|| external_other_variant(enum_ref.variants))
            .ok_or_else(|| {
                vm_error(
                    Some(tag_field.span),
                    DeserializeErrorKind::UnexpectedToken {
                        expected: "known enum variant",
                        got: tag.into(),
                    },
                )
            })?;

        unsafe {
            write_enum_discriminant(shape, enum_ref.enum_type, variant.variant, self.base)?;
        }
        claimed[tag_index] = Some(tag_field.span);

        let mut skip_tag_keys = vec![tag_key];
        let matched = match variant.variant.data.kind {
            StructKind::Unit => 0,
            StructKind::Struct => {
                if let Some(plan) = &variant.flatten {
                    self.read_flatten_struct_from_fields(shape, plan, fields, claimed, false)?
                } else {
                    self.push_struct_frame(
                        shape,
                        &variant.fields,
                        variant.dispatch.as_ref(),
                        variant.tracking,
                    );
                    self.read_flatten_captured_variant_fields(
                        shape,
                        variant,
                        fields,
                        claimed,
                        &skip_tag_keys,
                    )?
                }
            }
            StructKind::Tuple | StructKind::TupleStruct if variant.fields.len() == 1 => self
                .read_flatten_internal_newtype_variant_payload(
                    shape,
                    variant,
                    fields,
                    claimed,
                    &mut skip_tag_keys,
                    tag_field.span,
                )?,
            StructKind::Tuple | StructKind::TupleStruct => {
                return Err(unsupported(
                    shape,
                    "internally tagged multi-field tuple enum variant",
                ));
            }
        };

        Ok(Some((matched + 1, tag_field.span)))
    }

    fn select_flatten_internal_tag_field<'fields>(
        &self,
        fields: &'fields [TaggedRawField<'de>],
        claimed: &[Option<Span>],
        tag_key: &'static str,
    ) -> Result<Option<(usize, &'fields TaggedRawField<'de>)>, DeserializeError> {
        let mut selected: Option<(usize, &TaggedRawField<'de>)> = None;
        for (index, raw_field) in fields.iter().enumerate() {
            if raw_field.name.as_ref() != tag_key {
                continue;
            }
            self.ensure_flatten_raw_unclaimed(raw_field, claimed[index])?;
            if let Some((_, first)) = selected {
                return Err(Self::duplicate_flatten_field(raw_field, first.span));
            }
            selected = Some((index, raw_field));
        }
        Ok(selected)
    }

    fn read_flatten_captured_variant_fields(
        &mut self,
        shape: &'static Shape,
        variant: &'program ExternalVariantPlan<ExecBlock>,
        fields: &[TaggedRawField<'de>],
        claimed: &mut [Option<Span>],
        skip_keys: &[&'static str],
    ) -> Result<usize, DeserializeError> {
        self.read_flatten_captured_record_fields(
            shape,
            &variant.fields,
            variant.dispatch.as_ref(),
            fields,
            claimed,
            skip_keys,
        )
    }

    fn read_flatten_captured_record_fields(
        &mut self,
        shape: &'static Shape,
        record_fields: &'program [FieldPlan<ExecBlock>],
        dispatch: Option<&RawFieldDispatch>,
        fields: &[TaggedRawField<'de>],
        claimed: &mut [Option<Span>],
        skip_keys: &[&'static str],
    ) -> Result<usize, DeserializeError> {
        let tracking = StructTracking::for_len(record_fields.len());
        let mut matched = 0usize;
        for (raw_index, raw_field) in fields.iter().enumerate() {
            if skip_keys.iter().any(|key| raw_field.name.as_ref() == *key) {
                continue;
            }

            let Some((index, field)) = Self::captured_struct_field(
                record_fields,
                dispatch,
                raw_field.name.as_ref().as_bytes(),
            ) else {
                continue;
            };
            self.ensure_flatten_raw_unclaimed(raw_field, claimed[raw_index])?;

            if let Some(first_span) = self.struct_seen_span(tracking, index) {
                return Err(Self::duplicate_flatten_field(raw_field, first_span));
            }

            let field_ptr = unsafe { self.base.field_uninit(field.offset) };
            self.write_raw_field_value(field, field_ptr, raw_field.raw)?;
            self.mark_struct_field(tracking, index, raw_field.span);
            claimed[raw_index] = Some(raw_field.span);
            matched += 1;
        }

        unsafe {
            self.finish_struct_frame_with_validation(tracking, shape.vtable.has_invariants())?;
        }
        Ok(matched)
    }

    fn read_flatten_captured_scalar_struct(
        &mut self,
        shape: &'static Shape,
        fields: &'program [ScalarFieldPlan],
        dispatch: Option<&'program RawFieldDispatch>,
        raw_fields: &[TaggedRawField<'de>],
        claimed: &mut [Option<Span>],
        skip_keys: &[&'static str],
    ) -> Result<usize, DeserializeError> {
        match StructTracking::for_len(fields.len()) {
            StructTracking::Inline => {
                let mut frame = StructFrame::<ScalarFieldPlan, InitializedLedger<Span>>::new(
                    shape, self.base, fields, dispatch,
                );
                let matched = self.read_flatten_captured_scalar_struct_frame(
                    &mut frame, raw_fields, claimed, skip_keys,
                )?;
                unsafe {
                    frame.fill_missing_fields(shape.vtable.has_invariants())?;
                }
                Ok(matched)
            }
            StructTracking::Bitset => {
                let mut frame = StructFrame::<ScalarFieldPlan, BitsetStructSeen>::new(
                    shape, self.base, fields, dispatch,
                );
                let matched = self.read_flatten_captured_scalar_struct_frame(
                    &mut frame, raw_fields, claimed, skip_keys,
                )?;
                unsafe {
                    frame.fill_missing_fields(shape.vtable.has_invariants())?;
                }
                Ok(matched)
            }
            StructTracking::Heap => {
                let mut frame = StructFrame::<ScalarFieldPlan, HeapStructSeen>::new(
                    shape, self.base, fields, dispatch,
                );
                let matched = self.read_flatten_captured_scalar_struct_frame(
                    &mut frame, raw_fields, claimed, skip_keys,
                )?;
                unsafe {
                    frame.fill_missing_fields(shape.vtable.has_invariants())?;
                }
                Ok(matched)
            }
        }
    }

    fn read_flatten_captured_scalar_struct_frame<Seen: StructSeenStore>(
        &mut self,
        frame: &mut StructFrame<'program, ScalarFieldPlan, Seen>,
        raw_fields: &[TaggedRawField<'de>],
        claimed: &mut [Option<Span>],
        skip_keys: &[&'static str],
    ) -> Result<usize, DeserializeError> {
        let mut matched = 0usize;
        for (raw_index, raw_field) in raw_fields.iter().enumerate() {
            if skip_keys.iter().any(|key| raw_field.name.as_ref() == *key) {
                continue;
            }

            let Some((index, field)) = Self::captured_scalar_field(
                frame.fields,
                frame.dispatch,
                raw_field.name.as_ref().as_bytes(),
            ) else {
                continue;
            };
            self.ensure_flatten_raw_unclaimed(raw_field, claimed[raw_index])?;

            if let Some(first_span) = frame.seen.get(index) {
                return Err(Self::duplicate_flatten_field(raw_field, first_span));
            }

            let mut parser = JsonParser::<true>::new(raw_field.raw.as_bytes());
            let (value, value_span) = parser.read_scalar_token()?;
            let field_ptr = unsafe { frame.base.field_uninit(field.offset) };
            unsafe {
                field
                    .scalar
                    .write(field.shape, field_ptr, value, value_span)?;
            }
            Self::ensure_raw_parser_finished(&mut parser)?;
            frame.mark_seen(index, raw_field.span);
            claimed[raw_index] = Some(raw_field.span);
            matched += 1;
        }
        Ok(matched)
    }

    fn read_flatten_internal_newtype_variant_payload(
        &mut self,
        shape: &'static Shape,
        variant: &'program ExternalVariantPlan<ExecBlock>,
        fields: &[TaggedRawField<'de>],
        claimed: &mut [Option<Span>],
        skip_tag_keys: &mut Vec<&'static str>,
        span: Span,
    ) -> Result<usize, DeserializeError> {
        let [field] = variant.fields.as_ref() else {
            return Err(unsupported(
                shape,
                "non-single-field internally tagged enum payload",
            ));
        };

        self.push_struct_frame(
            shape,
            &variant.fields,
            variant.dispatch.as_ref(),
            variant.tracking,
        );
        let field_ptr = unsafe { self.base.field_uninit(field.offset) };
        let old_base = self.base;
        self.base = field_ptr;
        let result =
            self.read_flatten_internal_newtype_field(field, fields, claimed, skip_tag_keys);
        self.base = old_base;
        let matched = result?;
        self.mark_struct_field(variant.tracking, 0, span);
        unsafe {
            self.finish_struct_frame(variant.tracking)?;
        }
        Ok(matched)
    }

    fn read_flatten_internal_newtype_field(
        &mut self,
        field: &'program FieldPlan<ExecBlock>,
        fields: &[TaggedRawField<'de>],
        claimed: &mut [Option<Span>],
        skip_tag_keys: &mut Vec<&'static str>,
    ) -> Result<usize, DeserializeError> {
        match single_program_op(self.blocks, &field.program) {
            Some(JsonOp::ReadTaggedEnum {
                shape,
                enum_type,
                tag_key,
                content_key: None,
                variants,
            }) => {
                if skip_tag_keys.iter().any(|key| key == tag_key) {
                    return Err(unsupported(
                        shape,
                        "nested internally tagged enums with the same tag key",
                    ));
                }
                let enum_ref = FlattenExternalEnumRef {
                    enum_type: *enum_type,
                    tag_key: Some(*tag_key),
                    content_key: None,
                    variants,
                };
                let Some((matched, _span)) = self
                    .read_flatten_internal_enum_value(shape, enum_ref, tag_key, fields, claimed)?
                else {
                    return Err(vm_error(
                        None,
                        DeserializeErrorKind::MissingField {
                            field: tag_key,
                            container_shape: shape,
                        },
                    ));
                };
                Ok(matched)
            }
            Some(JsonOp::ReadStruct {
                shape,
                fields: record_fields,
                dispatch,
                ..
            })
            | Some(JsonOp::ReadStructValidate {
                shape,
                fields: record_fields,
                dispatch,
                ..
            }) => {
                let tracking = StructTracking::for_len(record_fields.len());
                self.push_struct_frame(shape, record_fields, dispatch.as_ref(), tracking);
                self.read_flatten_captured_record_fields(
                    shape,
                    record_fields,
                    dispatch.as_ref(),
                    fields,
                    claimed,
                    skip_tag_keys,
                )
            }
            Some(JsonOp::ReadScalarStruct {
                shape,
                fields: scalar_fields,
                dispatch,
                ..
            })
            | Some(JsonOp::ReadScalarStructValidate {
                shape,
                fields: scalar_fields,
                dispatch,
                ..
            }) => self.read_flatten_captured_scalar_struct(
                shape,
                scalar_fields,
                dispatch.as_ref(),
                fields,
                claimed,
                skip_tag_keys,
            ),
            Some(JsonOp::ReadFlattenStruct { shape, plan }) => {
                self.read_flatten_struct_from_fields(shape, plan, fields, claimed, false)
            }
            _ => Err(unsupported(
                field.shape,
                "internally tagged enum newtype payload",
            )),
        }
    }

    fn select_flatten_external_variant<'fields>(
        &self,
        variants: &'program [ExternalVariantPlan<ExecBlock>],
        fields: &'fields [TaggedRawField<'de>],
        claimed: &[Option<Span>],
    ) -> Result<Option<FlattenVariantSelection<'fields, 'de, 'program>>, DeserializeError> {
        let mut selected: Option<FlattenVariantSelection<'fields, 'de, 'program>> = None;
        for (raw_index, raw_field) in fields.iter().enumerate() {
            let Some(variant) = find_external_variant(variants, raw_field.name.as_ref()) else {
                continue;
            };
            self.ensure_flatten_raw_unclaimed(raw_field, claimed[raw_index])?;
            if let Some((_, first, _)) = selected {
                return Err(Self::duplicate_flatten_field(raw_field, first.span));
            }
            selected = Some((raw_index, raw_field, variant));
        }
        Ok(selected)
    }

    fn read_flatten_external_variant_payload(
        &mut self,
        shape: &'static Shape,
        enum_type: EnumType,
        variant: &'program ExternalVariantPlan<ExecBlock>,
        raw_field: &TaggedRawField<'de>,
    ) -> Result<(), DeserializeError> {
        unsafe {
            write_enum_discriminant(shape, enum_type, variant.variant, self.base)?;
        }

        match variant.variant.data.kind {
            StructKind::Unit => self.consume_raw_unit_variant_payload(raw_field.raw),
            StructKind::Tuple | StructKind::TupleStruct if variant.fields.len() == 1 => self
                .read_raw_single_field_variant_payload(
                    shape,
                    variant,
                    raw_field.raw,
                    raw_field.span,
                ),
            StructKind::Tuple | StructKind::TupleStruct => {
                self.read_raw_tuple_variant_payload(shape, variant, raw_field.raw)
            }
            StructKind::Struct => {
                self.read_raw_struct_variant_payload(shape, variant, raw_field.raw)
            }
        }
    }

    fn mark_flatten_parent_field(
        &mut self,
        tracking: StructTracking,
        field_index: usize,
        field: &FieldPlan<ExecBlock>,
        span: Span,
    ) -> Result<(), DeserializeError> {
        if let Some(first_span) = self.struct_seen_span(tracking, field_index) {
            return Err(vm_error(
                Some(span),
                DeserializeErrorKind::DuplicateField {
                    field: field.name.into(),
                    first_span: Some(first_span),
                },
            ));
        }
        self.mark_struct_field(tracking, field_index, span);
        Ok(())
    }

    fn reject_unclaimed_flatten_fields(
        &self,
        shape: &'static Shape,
        fields: &[TaggedRawField<'de>],
        claimed: &[Option<Span>],
    ) -> Result<(), DeserializeError> {
        if !shape.has_deny_unknown_fields_attr() {
            return Ok(());
        }

        for (raw_field, claimed) in fields.iter().zip(claimed.iter()) {
            if claimed.is_none() {
                return Err(vm_error(
                    Some(raw_field.span),
                    DeserializeErrorKind::UnknownField {
                        field: raw_field.name.to_string().into(),
                        suggestion: None,
                    },
                ));
            }
        }
        Ok(())
    }

    fn first_flatten_kind_match_span(
        kind: &FlattenKind<ExecBlock>,
        fields: &[TaggedRawField<'de>],
    ) -> Option<Span> {
        fields
            .iter()
            .find(|raw_field| {
                Self::flatten_kind_matches_key(kind, raw_field.name.as_ref().as_bytes())
            })
            .map(|raw_field| raw_field.span)
    }

    fn first_unclaimed_flatten_kind_match_span(
        kind: &FlattenKind<ExecBlock>,
        fields: &[TaggedRawField<'de>],
        claimed: &[Option<Span>],
    ) -> Option<Span> {
        fields
            .iter()
            .enumerate()
            .find(|(index, raw_field)| {
                claimed[*index].is_none()
                    && Self::flatten_kind_matches_key(kind, raw_field.name.as_ref().as_bytes())
            })
            .map(|(_, raw_field)| raw_field.span)
    }

    fn ensure_flatten_raw_unclaimed(
        &self,
        raw_field: &TaggedRawField<'de>,
        claimed: Option<Span>,
    ) -> Result<(), DeserializeError> {
        if let Some(first_span) = claimed {
            return Err(Self::duplicate_flatten_field(raw_field, first_span));
        }
        Ok(())
    }

    fn duplicate_flatten_field(
        raw_field: &TaggedRawField<'de>,
        first_span: Span,
    ) -> DeserializeError {
        vm_error(
            Some(raw_field.span),
            DeserializeErrorKind::DuplicateField {
                field: raw_field.name.to_string().into(),
                first_span: Some(first_span),
            },
        )
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
            if let Some(plan) = &variant.flatten {
                let mut claimed = vec![None; fields.len()];
                interp.read_flatten_struct_from_fields(
                    shape,
                    plan,
                    &fields,
                    &mut claimed,
                    false,
                )?;
            } else {
                interp.push_struct_frame(
                    shape,
                    &variant.fields,
                    variant.dispatch.as_ref(),
                    variant.tracking,
                );
                interp.read_captured_variant_fields(shape, variant, &fields, &[])?;
            }
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

    fn read_internal_tagged_newtype_variant_payload(
        &mut self,
        shape: &'static Shape,
        variant: &'program ExternalVariantPlan<ExecBlock>,
        fields: &[TaggedRawField<'de>],
        skip_tag_keys: &mut Vec<&'static str>,
        span: Span,
    ) -> Result<(), DeserializeError> {
        let [field] = variant.fields.as_ref() else {
            return Err(unsupported(
                shape,
                "non-single-field internally tagged enum payload",
            ));
        };

        self.push_struct_frame(
            shape,
            &variant.fields,
            variant.dispatch.as_ref(),
            variant.tracking,
        );
        let field_ptr = unsafe { self.base.field_uninit(field.offset) };
        let old_base = self.base;
        self.base = field_ptr;
        let result = self.read_internal_tagged_newtype_field(field, fields, skip_tag_keys);
        self.base = old_base;
        result?;
        self.mark_struct_field(variant.tracking, 0, span);
        unsafe {
            self.finish_struct_frame(variant.tracking)?;
        }
        Ok(())
    }

    fn read_internal_tagged_newtype_field(
        &mut self,
        field: &'program FieldPlan<ExecBlock>,
        fields: &[TaggedRawField<'de>],
        skip_tag_keys: &mut Vec<&'static str>,
    ) -> Result<(), DeserializeError> {
        match single_program_op(self.blocks, &field.program) {
            Some(JsonOp::ReadTaggedEnum {
                shape,
                enum_type,
                tag_key,
                content_key: None,
                variants,
            }) => {
                if skip_tag_keys.iter().any(|key| key == tag_key) {
                    return Err(unsupported(
                        shape,
                        "nested internally tagged enums with the same tag key",
                    ));
                }

                let tag_field = Self::require_tagged_field(fields, tag_key, shape)?;
                let tag = Self::read_raw_tag_name(tag_field, tag_key)?;
                let variant = Self::tagged_variant(variants, &tag, tag_field.span)?;

                unsafe {
                    write_enum_discriminant(shape, *enum_type, variant.variant, self.base)?;
                }

                skip_tag_keys.push(*tag_key);
                let result = match variant.variant.data.kind {
                    StructKind::Unit => Ok(()),
                    StructKind::Struct => {
                        self.push_struct_frame(
                            shape,
                            &variant.fields,
                            variant.dispatch.as_ref(),
                            variant.tracking,
                        );
                        self.read_captured_variant_fields(shape, variant, fields, skip_tag_keys)
                    }
                    StructKind::Tuple | StructKind::TupleStruct if variant.fields.len() == 1 => {
                        self.read_internal_tagged_newtype_variant_payload(
                            shape,
                            variant,
                            fields,
                            skip_tag_keys,
                            tag_field.span,
                        )
                    }
                    StructKind::Tuple | StructKind::TupleStruct => Err(unsupported(
                        shape,
                        "internally tagged multi-field tuple enum variant",
                    )),
                };
                skip_tag_keys.pop();
                result
            }
            Some(JsonOp::ReadStruct {
                shape,
                fields: record_fields,
                dispatch,
                ..
            })
            | Some(JsonOp::ReadStructValidate {
                shape,
                fields: record_fields,
                dispatch,
                ..
            }) => {
                let tracking = StructTracking::for_len(record_fields.len());
                self.push_struct_frame(shape, record_fields, dispatch.as_ref(), tracking);
                self.read_captured_record_fields(
                    shape,
                    record_fields,
                    dispatch.as_ref(),
                    tracking,
                    fields,
                    skip_tag_keys,
                )
            }
            Some(JsonOp::ReadScalarStruct {
                shape,
                fields: scalar_fields,
                dispatch,
                ..
            })
            | Some(JsonOp::ReadScalarStructValidate {
                shape,
                fields: scalar_fields,
                dispatch,
                ..
            }) => self.read_captured_scalar_struct(
                shape,
                scalar_fields,
                dispatch.as_ref(),
                fields,
                skip_tag_keys,
            ),
            _ => Err(unsupported(
                field.shape,
                "internally tagged enum newtype payload",
            )),
        }
    }

    fn untagged_fallback_variant<'variants>(
        &self,
        variants: &'variants [ExternalVariantPlan<ExecBlock>],
        fields: &[TaggedRawField<'de>],
    ) -> Result<Option<&'variants ExternalVariantPlan<ExecBlock>>, DeserializeError> {
        let mut best: Option<(&ExternalVariantPlan<ExecBlock>, usize, usize)> = None;

        for variant in variants
            .iter()
            .filter(|variant| variant.variant.has_builtin_attr("untagged"))
        {
            let score = match variant.variant.data.kind {
                StructKind::Unit => Some((0, 0)),
                StructKind::Struct => field_plan_match_score(&variant.fields, fields)?,
                StructKind::Tuple | StructKind::TupleStruct if variant.fields.len() == 1 => {
                    let [field] = variant.fields.as_ref() else {
                        unreachable!("checked single-field variant");
                    };
                    match single_program_op(self.blocks, &field.program) {
                        Some(JsonOp::ReadStruct {
                            fields: record_fields,
                            ..
                        })
                        | Some(JsonOp::ReadStructValidate {
                            fields: record_fields,
                            ..
                        }) => field_plan_match_score(record_fields, fields)?,
                        Some(JsonOp::ReadScalarStruct {
                            fields: scalar_fields,
                            ..
                        })
                        | Some(JsonOp::ReadScalarStructValidate {
                            fields: scalar_fields,
                            ..
                        }) => scalar_field_plan_match_score(scalar_fields, fields)?,
                        _ => None,
                    }
                }
                StructKind::Tuple | StructKind::TupleStruct => None,
            };

            let Some((matched, quality)) = score else {
                continue;
            };
            if best.is_none_or(|(_, best_matched, best_quality)| {
                matched > best_matched || (matched == best_matched && quality < best_quality)
            }) {
                best = Some((variant, matched, quality));
            }
        }

        Ok(best.map(|(variant, _, _)| variant))
    }

    fn can_read_internal_tagged_enum_direct(variants: &[ExternalVariantPlan<ExecBlock>]) -> bool {
        variants.iter().all(|variant| {
            !variant.variant.has_builtin_attr("untagged")
                && variant.flatten.is_none()
                && matches!(
                    variant.variant.data.kind,
                    StructKind::Unit | StructKind::Struct
                )
        })
    }

    fn read_internal_tagged_enum_direct(
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
                self.read_captured_variant_fields(shape, variant, &fields, &[tag_key])?;
                Ok(Control::Continue)
            }
            StructKind::Tuple | StructKind::TupleStruct => {
                Err(unsupported(shape, "internally tagged tuple enum variant"))
            }
        }
    }

    fn read_internal_tagged_enum(
        &mut self,
        shape: &'static Shape,
        enum_type: EnumType,
        tag_key: &'static str,
        variants: &'program [ExternalVariantPlan<ExecBlock>],
    ) -> Result<Control<'program, ExecBlock, ExecOp, Continuation<'program>>, DeserializeError>
    {
        if Self::can_read_internal_tagged_enum_direct(variants) {
            return self.read_internal_tagged_enum_direct(shape, enum_type, tag_key, variants);
        }

        let raw = self
            .parser
            .capture_raw()?
            .ok_or_else(|| unsupported_shape_message("raw JSON capture failed"))?;
        let fields =
            Self::collect_tagged_raw_fields_from_raw(raw, "struct for internally tagged enum")?;
        let tag_field = Self::unique_tagged_field(&fields, tag_key)?;
        let mut selected_untagged = false;
        let (variant, span) = if let Some(tag_field) = tag_field {
            let tag = Self::read_raw_tag_name(tag_field, tag_key)?;
            if let Some(variant) = find_tagged_variant(variants, &tag) {
                (variant, tag_field.span)
            } else if let Some(variant) = self.untagged_fallback_variant(variants, &fields)? {
                selected_untagged = true;
                (variant, tag_field.span)
            } else {
                (
                    Self::tagged_variant(variants, &tag, tag_field.span)?,
                    tag_field.span,
                )
            }
        } else if let Some(variant) = self.untagged_fallback_variant(variants, &fields)? {
            selected_untagged = true;
            (variant, Span::default())
        } else {
            return Err(vm_error(
                None,
                DeserializeErrorKind::MissingField {
                    field: tag_key,
                    container_shape: shape,
                },
            ));
        };

        unsafe {
            write_enum_discriminant(shape, enum_type, variant.variant, self.base)?;
        }

        match variant.variant.data.kind {
            StructKind::Unit => Ok(Control::Continue),
            StructKind::Struct => {
                let skip_keys: &[&'static str] = if selected_untagged { &[] } else { &[tag_key] };
                if let Some(plan) = &variant.flatten {
                    let mut claimed = vec![None; fields.len()];
                    if !selected_untagged {
                        for (index, field) in fields.iter().enumerate() {
                            if field.name.as_ref() == tag_key {
                                claimed[index] = Some(field.span);
                            }
                        }
                    }
                    self.read_flatten_struct_from_fields(
                        shape,
                        plan,
                        &fields,
                        &mut claimed,
                        false,
                    )?;
                } else {
                    self.push_struct_frame(
                        shape,
                        &variant.fields,
                        variant.dispatch.as_ref(),
                        variant.tracking,
                    );
                    self.read_captured_variant_fields(shape, variant, &fields, skip_keys)?;
                }
                Ok(Control::Continue)
            }
            StructKind::Tuple | StructKind::TupleStruct if variant.fields.len() == 1 => {
                let mut skip_tag_keys = if selected_untagged {
                    Vec::new()
                } else {
                    vec![tag_key]
                };
                self.read_internal_tagged_newtype_variant_payload(
                    shape,
                    variant,
                    &fields,
                    &mut skip_tag_keys,
                    span,
                )?;
                Ok(Control::Continue)
            }
            StructKind::Tuple | StructKind::TupleStruct => Err(unsupported(
                shape,
                "internally tagged multi-field tuple enum variant",
            )),
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

    fn read_numeric_enum(
        &mut self,
        shape: &'static Shape,
        enum_type: EnumType,
        variants: &'program [ExternalVariantPlan<ExecBlock>],
    ) -> Result<Control<'program, ExecBlock, ExecOp, Continuation<'program>>, DeserializeError>
    {
        let (token, span) = self.parser.read_scalar_token()?;
        let discriminant = numeric_discriminant_from_token(token, span)?;
        let variant = numeric_variant(variants, discriminant).ok_or_else(|| {
            vm_error(
                Some(span),
                DeserializeErrorKind::NoMatchingVariant {
                    enum_shape: shape,
                    input_kind: "numeric discriminant",
                },
            )
        })?;

        unsafe {
            write_enum_discriminant(shape, enum_type, variant.variant, self.base)?;
        }
        Ok(Control::Continue)
    }

    fn read_cow_enum(
        &mut self,
        shape: &'static Shape,
        enum_type: EnumType,
        owned_variant: &'program ExternalVariantPlan<ExecBlock>,
    ) -> Result<Control<'program, ExecBlock, ExecOp, Continuation<'program>>, DeserializeError>
    {
        let span = self
            .parser
            .peek_event()?
            .map_or(Span { offset: 0, len: 0 }, |event| event.span);
        unsafe {
            write_enum_discriminant(shape, enum_type, owned_variant.variant, self.base)?;
        }
        self.read_external_enum_single_field_payload(shape, owned_variant, span, false)
    }

    fn collect_raw_untagged_struct_fields(
        &mut self,
        raw: &'de str,
    ) -> Result<Vec<TaggedRawField<'de>>, DeserializeError> {
        let mut parser = JsonParser::<true>::new(raw.as_bytes());
        let fields = {
            let mut interp: JsonInterp<'_, 'de, 'program, true> =
                JsonInterp::<'_, 'de, 'program, true>::new(&mut parser, self.base, self.blocks);
            let fields = interp.collect_tagged_raw_fields("untagged enum struct")?;
            interp.finish_success();
            fields
        };
        Self::ensure_raw_parser_finished(&mut parser)?;
        Ok(fields)
    }

    fn raw_sequence_arity(raw: &'de str) -> Result<usize, DeserializeError> {
        let mut parser = JsonParser::<true>::new(raw.as_bytes());
        let Some(event) = parser.next_event()? else {
            return Err(vm_error(
                None,
                DeserializeErrorKind::UnexpectedEof {
                    expected: "sequence start",
                },
            ));
        };
        if !matches!(event.kind, ParseEventKind::SequenceStart(_)) {
            return Err(vm_error(
                Some(event.span),
                DeserializeErrorKind::UnexpectedToken {
                    expected: "sequence start",
                    got: event.kind_name().into(),
                },
            ));
        }

        let mut depth = 1usize;
        let mut arity = 0usize;
        while depth > 0 {
            let Some(event) = parser.next_event()? else {
                return Err(vm_error(
                    None,
                    DeserializeErrorKind::UnexpectedEof {
                        expected: "sequence item or end",
                    },
                ));
            };

            match event.kind {
                ParseEventKind::SequenceStart(_) | ParseEventKind::StructStart(_) => {
                    if depth == 1 {
                        arity += 1;
                    }
                    depth += 1;
                }
                ParseEventKind::SequenceEnd | ParseEventKind::StructEnd => {
                    depth = depth.saturating_sub(1);
                }
                ParseEventKind::Scalar(_) | ParseEventKind::VariantTag(_) if depth == 1 => {
                    arity += 1;
                }
                ParseEventKind::FieldKey(_) | ParseEventKind::OrderedField => {}
                _ => {}
            }
        }

        Self::ensure_raw_parser_finished(&mut parser)?;
        Ok(arity)
    }

    fn read_untagged_selected_variant(
        &mut self,
        shape: &'static Shape,
        enum_type: EnumType,
        variant: &'program ExternalVariantPlan<ExecBlock>,
        raw: &'de str,
        span: Span,
        captured_struct_fields: Option<&[TaggedRawField<'de>]>,
    ) -> Result<(), DeserializeError> {
        unsafe {
            write_enum_discriminant(shape, enum_type, variant.variant, self.base)?;
        }

        match variant.variant.data.kind {
            StructKind::Unit => Ok(()),
            StructKind::Tuple | StructKind::TupleStruct if variant.fields.len() == 1 => {
                self.read_raw_single_field_variant_payload(shape, variant, raw, span)
            }
            StructKind::Tuple | StructKind::TupleStruct => {
                self.read_raw_tuple_variant_payload(shape, variant, raw)
            }
            StructKind::Struct => {
                if let Some(fields) = captured_struct_fields {
                    if let Some(plan) = &variant.flatten {
                        let mut claimed = vec![None; fields.len()];
                        self.read_flatten_struct_from_fields(
                            shape,
                            plan,
                            fields,
                            &mut claimed,
                            false,
                        )
                        .map(|_| ())
                    } else {
                        self.push_struct_frame(
                            shape,
                            &variant.fields,
                            variant.dispatch.as_ref(),
                            variant.tracking,
                        );
                        self.read_captured_variant_fields(shape, variant, fields, &[])
                    }
                } else {
                    self.read_raw_struct_variant_payload(shape, variant, raw)
                }
            }
        }
    }

    fn read_untagged_enum(
        &mut self,
        shape: &'static Shape,
        enum_type: EnumType,
        variants: &'program [ExternalVariantPlan<ExecBlock>],
    ) -> Result<Control<'program, ExecBlock, ExecOp, Continuation<'program>>, DeserializeError>
    {
        enum UntaggedInput<'de> {
            Scalar(ScalarValue<'de>),
            Struct,
            Sequence,
            Other(&'static str),
        }

        let Some(event) = self.parser.peek_event()? else {
            return Err(vm_error(
                None,
                DeserializeErrorKind::UnexpectedEof {
                    expected: "untagged enum",
                },
            ));
        };

        let span = event.span;
        let input = match &event.kind {
            ParseEventKind::Scalar(scalar) => UntaggedInput::Scalar(scalar.clone()),
            ParseEventKind::StructStart(_) => UntaggedInput::Struct,
            ParseEventKind::SequenceStart(_) => UntaggedInput::Sequence,
            _ => UntaggedInput::Other(event.kind_name()),
        };
        let raw = self
            .parser
            .capture_raw()?
            .ok_or_else(|| unsupported_shape_message("raw JSON capture failed"))?;

        match input {
            UntaggedInput::Scalar(scalar) => {
                let variant = untagged_scalar_variant(variants, &scalar).ok_or_else(|| {
                    vm_error(
                        Some(span),
                        DeserializeErrorKind::UnexpectedToken {
                            expected: "matching untagged variant for scalar",
                            got: scalar.kind_name().into(),
                        },
                    )
                })?;
                self.read_untagged_selected_variant(shape, enum_type, variant, raw, span, None)?;
            }
            UntaggedInput::Struct => {
                let fields = self.collect_raw_untagged_struct_fields(raw)?;
                let variant = untagged_struct_variant(shape, variants, &fields)?;
                self.read_untagged_selected_variant(
                    shape,
                    enum_type,
                    variant,
                    raw,
                    span,
                    Some(&fields),
                )?;
            }
            UntaggedInput::Sequence => {
                let arity = Self::raw_sequence_arity(raw)?;
                let variant = untagged_tuple_variant(shape, variants, arity)?;
                self.read_untagged_selected_variant(shape, enum_type, variant, raw, span, None)?;
            }
            UntaggedInput::Other(got) => {
                return Err(vm_error(
                    Some(span),
                    DeserializeErrorKind::UnexpectedToken {
                        expected: "scalar, struct, or sequence for untagged enum",
                        got: got.into(),
                    },
                ));
            }
        }

        Ok(Control::Continue)
    }

    fn finish_external_enum_payload(
        &mut self,
        tracking: StructTracking,
        close_object: bool,
        validate: bool,
    ) -> Result<Control<'program, ExecBlock, ExecOp, Continuation<'program>>, DeserializeError>
    {
        if close_object {
            self.consume_external_enum_object_end()?;
        }
        unsafe {
            self.finish_struct_frame_with_validation(tracking, validate)?;
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
            return self.finish_external_enum_payload(
                variant.tracking,
                close_object,
                shape.vtable.has_invariants(),
            );
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
                validate: shape.vtable.has_invariants(),
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
        self.read_external_enum_tuple_next(
            &variant.fields,
            variant.tracking,
            0,
            true,
            shape.vtable.has_invariants(),
        )
    }

    fn read_external_enum_tuple_next(
        &mut self,
        fields: &'program [FieldPlan<ExecBlock>],
        tracking: StructTracking,
        mut next_index: usize,
        close_object: bool,
        validate: bool,
    ) -> Result<Control<'program, ExecBlock, ExecOp, Continuation<'program>>, DeserializeError>
    {
        loop {
            let Some(field) = fields.get(next_index) else {
                if self.parser.consume_sequence_end_if_next()? {
                    return self.finish_external_enum_payload(tracking, close_object, validate);
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
                    validate,
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
                        if variant.flatten.is_some() {
                            let raw = self.parser.capture_raw()?.ok_or_else(|| {
                                unsupported_shape_message("raw JSON capture failed")
                            })?;
                            self.read_raw_struct_variant_payload(shape, variant, raw)?;
                            self.consume_external_enum_object_end()?;
                            Ok(Control::Continue)
                        } else {
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
                                    validate: shape.vtable.has_invariants(),
                                },
                            ))
                        }
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

    fn push_pointer_slice_element(
        &mut self,
        scratch: &ScratchSlot,
    ) -> Result<(), DeserializeError> {
        let frame = self
            .pointer_slices
            .last_mut()
            .expect("pointer slice frame is present while pushing element");
        frame.push(scratch)
    }

    fn step_array_next(
        &mut self,
        array: ArrayDef,
        element_program: &'program [ExecOp],
        element_scalar: Option<ScalarType>,
        element_option_scalar: Option<ListOptionScalar>,
        loop_id: ExecBlock,
    ) -> Result<Control<'program, ExecBlock, ExecOp, Continuation<'program>>, DeserializeError>
    {
        loop {
            if let Some(scalar) = element_scalar {
                let slot = match self.array_slot_or_finish()? {
                    ArraySlot::Slot(slot) => slot,
                    ArraySlot::Done => return Ok(Control::Continue),
                };
                match self.parser.next_sequence_scalar_or_end()? {
                    JsonSequenceScalarStep::Value { value } => {
                        unsafe {
                            write_scalar_input(self.parser, array.t(), scalar, slot, value)?;
                            self.mark_array_element_initialized();
                        }
                        continue;
                    }
                    JsonSequenceScalarStep::End => {
                        return Err(fixed_array_length_error(array, self.current_array_len()));
                    }
                }
            }

            if let Some(option_scalar) = element_option_scalar {
                let slot = match self.array_slot_or_finish()? {
                    ArraySlot::Slot(slot) => slot,
                    ArraySlot::Done => return Ok(Control::Continue),
                };
                match self.parser.next_sequence_scalar_or_end()? {
                    JsonSequenceScalarStep::Value { value } => {
                        if value.is_null() {
                            unsafe {
                                (option_scalar.option.vtable.init_none)(slot);
                                self.mark_array_element_initialized();
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
                                self.mark_array_element_initialized();
                            }
                            self.scratch.release(inner);
                        }
                        continue;
                    }
                    JsonSequenceScalarStep::End => {
                        return Err(fixed_array_length_error(array, self.current_array_len()));
                    }
                }
            }

            let slot = match self.array_slot_or_finish()? {
                ArraySlot::Slot(slot) => slot,
                ArraySlot::Done => return Ok(Control::Continue),
            };
            let old_base = self.base;
            self.base = slot;
            return Ok(call_program_or_block_then(
                element_program,
                Continuation::ArrayElement { old_base, loop_id },
            ));
        }
    }

    fn current_array_len(&self) -> usize {
        self.arrays
            .last()
            .expect("array frame is present")
            .initialized
    }

    fn read_dynamic_value(
        &mut self,
        dynamic_shape: &'static Shape,
        dynamic: DynamicValueDef,
        loop_id: ExecBlock,
    ) -> Result<Control<'program, ExecBlock, ExecOp, Continuation<'program>>, DeserializeError>
    {
        let Some(event) = self.parser.peek_event()? else {
            return Err(vm_error(
                None,
                DeserializeErrorKind::UnexpectedEof {
                    expected: "dynamic value",
                },
            ));
        };

        match event.kind {
            ParseEventKind::Scalar(_) => {
                let (value, span) = self.parser.read_scalar_token()?;
                unsafe {
                    write_dynamic_scalar(dynamic, self.base, value, span)?;
                }
                Ok(Control::Continue)
            }
            ParseEventKind::SequenceStart(_) => {
                self.parser.consume_array_start_fast()?;
                unsafe {
                    (dynamic.vtable.begin_array)(self.base);
                }
                self.dynamic_values
                    .push(DynamicFrame::array(dynamic_shape, dynamic, self.base));
                Ok(Control::CallBlockThen(
                    loop_id,
                    Continuation::FinishDynamicValue,
                ))
            }
            ParseEventKind::StructStart(_) => {
                self.parser.consume_object_start_fast()?;
                unsafe {
                    (dynamic.vtable.begin_object)(self.base);
                }
                self.dynamic_values
                    .push(DynamicFrame::object(dynamic_shape, dynamic, self.base));
                Ok(Control::CallBlockThen(
                    loop_id,
                    Continuation::FinishDynamicValue,
                ))
            }
            other => Err(vm_error(
                Some(event.span),
                DeserializeErrorKind::UnexpectedToken {
                    expected: "scalar, array, or object",
                    got: other.kind_name().into(),
                },
            )),
        }
    }

    fn step_dynamic_next(
        &mut self,
        dynamic_shape: &'static Shape,
        dynamic_layout: Layout,
        value_program: &'program [ExecOp],
        loop_id: ExecBlock,
    ) -> Result<Control<'program, ExecBlock, ExecOp, Continuation<'program>>, DeserializeError>
    {
        match self.dynamic_values.last() {
            Some(DynamicFrame::Array { .. }) => {
                if self.parser.consume_sequence_end_if_next()? {
                    return Ok(Control::Continue);
                }

                let scratch = self.scratch.reserve(dynamic_layout);
                let old_base = self.base;
                self.base = scratch_ptr_uninit(&scratch);
                Ok(call_program_or_block_then(
                    value_program,
                    Continuation::DynamicArrayElement {
                        old_base,
                        scratch,
                        loop_id,
                    },
                ))
            }
            Some(DynamicFrame::Object { .. }) => {
                let (key, _span) = match self.parser.next_object_key_or_end()? {
                    JsonObjectKeyStep::End => return Ok(Control::Continue),
                    JsonObjectKeyStep::Field { key, span } => {
                        let key = self.parser.materialize_field_key(key)?.as_str().to_string();
                        (key, span)
                    }
                };

                let scratch = self.scratch.reserve(dynamic_layout);
                let old_base = self.base;
                self.base = scratch_ptr_uninit(&scratch);
                Ok(call_program_or_block_then(
                    value_program,
                    Continuation::DynamicObjectEntry {
                        key,
                        old_base,
                        scratch,
                        loop_id,
                    },
                ))
            }
            None => Err(unsupported(dynamic_shape, "dynamic value frame")),
        }
    }

    fn push_dynamic_array_element(
        &mut self,
        scratch: &ScratchSlot,
    ) -> Result<(), DeserializeError> {
        self.dynamic_values
            .last_mut()
            .expect("dynamic array frame is present while pushing element")
            .push_array_element(scratch)
    }

    fn insert_dynamic_object_entry(
        &mut self,
        key: &str,
        scratch: &ScratchSlot,
    ) -> Result<(), DeserializeError> {
        self.dynamic_values
            .last_mut()
            .expect("dynamic object frame is present while inserting entry")
            .insert_object_entry(key, scratch)
    }

    fn step_pointer_slice_next(
        &mut self,
        pointer: PointerDef,
        element_program: &'program [ExecOp],
        element_scalar: Option<ScalarType>,
        element_option_scalar: Option<ListOptionScalar>,
        element_layout: Layout,
        loop_id: ExecBlock,
    ) -> Result<Control<'program, ExecBlock, ExecOp, Continuation<'program>>, DeserializeError>
    {
        loop {
            if let Some(scalar) = element_scalar {
                let step = self.parser.next_sequence_scalar_or_end()?;
                let JsonSequenceScalarStep::Value { value } = step else {
                    return Ok(Control::Continue);
                };

                let scratch = self.scratch.reserve(element_layout);
                unsafe {
                    write_scalar_input(
                        self.parser,
                        pointer_slice_element_shape(pointer)?,
                        scalar,
                        scratch_ptr_uninit(&scratch),
                        value,
                    )?;
                }
                self.push_pointer_slice_element(&scratch)?;
                self.scratch.release(scratch);
                continue;
            }

            if let Some(option_scalar) = element_option_scalar {
                let step = self.parser.next_sequence_scalar_or_end()?;
                let JsonSequenceScalarStep::Value { value } = step else {
                    return Ok(Control::Continue);
                };

                let scratch = self.scratch.reserve(element_layout);
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
                self.push_pointer_slice_element(&scratch)?;
                self.scratch.release(scratch);
                continue;
            }

            if self.parser.consume_sequence_end_if_next()? {
                return Ok(Control::Continue);
            }

            let scratch = self.scratch.reserve(element_layout);
            let old_base = self.base;
            self.base = scratch_ptr_uninit(&scratch);
            return Ok(call_program_or_block_then(
                element_program,
                Continuation::PointerSliceElement {
                    old_base,
                    scratch,
                    loop_id,
                },
            ));
        }
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
        key_plan: &MapKeyPlan,
        key: String,
        key_span: Span,
        value_scratch: ScratchSlot,
    ) -> Result<(), DeserializeError> {
        let map_ptr = self
            .maps
            .last()
            .expect("map frame is present while inserting entry");
        let map_ptr = PtrMut::new(map_ptr.guard.ptr());
        self.insert_map_entry_into(map, map_ptr, key_plan, key, key_span, value_scratch)
    }

    fn insert_map_entry_into(
        &mut self,
        map: MapDef,
        map_ptr: PtrMut,
        key_plan: &MapKeyPlan,
        mut key: String,
        key_span: Span,
        value_scratch: ScratchSlot,
    ) -> Result<(), DeserializeError> {
        if key_plan.is_exact_string()
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

        let key_scratch = self.scratch.reserve(key_plan.layout());
        let key_result = unsafe {
            write_map_key_plan(key_plan, scratch_ptr_uninit(&key_scratch), key, key_span)
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
        key_plan: &MapKeyPlan,
        key: JsonFieldKeyInput<'de>,
        key_span: Span,
        value_scratch: ScratchSlot,
    ) -> Result<(), DeserializeError> {
        if key_plan.is_exact_string()
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
        self.insert_map_entry(map, key_plan, key, key_span, value_scratch)
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
                && plan.key_plan.is_exact_string()
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
                plan.key_plan,
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
                key_plan: plan.key_plan,
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
        validate: bool,
    ) -> Result<(), DeserializeError> {
        self.parser.consume_object_start_fast()?;
        let base = self.base;
        if fields.len() <= TINY_SCALAR_STRUCT_MAX_FIELDS {
            return self.read_tiny_scalar_struct_fields(shape, base, fields, validate);
        }

        match StructTracking::for_len(fields.len()) {
            StructTracking::Inline => {
                let mut frame = StructFrame::<ScalarFieldPlan, InitializedLedger<Span>>::new(
                    shape, base, fields, dispatch,
                );
                self.read_scalar_struct_fields(shape, &mut frame)?;
                unsafe {
                    frame.fill_missing_fields(validate)?;
                }
            }
            StructTracking::Bitset => {
                let mut frame = StructFrame::<ScalarFieldPlan, BitsetStructSeen>::new(
                    shape, base, fields, dispatch,
                );
                self.read_scalar_struct_fields(shape, &mut frame)?;
                unsafe {
                    frame.fill_missing_fields(validate)?;
                }
            }
            StructTracking::Heap => {
                let mut frame = StructFrame::<ScalarFieldPlan, HeapStructSeen>::new(
                    shape, base, fields, dispatch,
                );
                self.read_scalar_struct_fields(shape, &mut frame)?;
                unsafe {
                    frame.fill_missing_fields(validate)?;
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
        validate: bool,
    ) -> Result<(), DeserializeError> {
        let mut frame = TinyScalarStructFrame::new(shape, base, fields);
        if self.try_read_fused_tiny_i32_struct_fields(&mut frame)? {
            if validate {
                validate_completed_shape(shape, base)?;
            }
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
            if validate {
                validate_completed_shape(shape, base)?;
            }
            core::mem::forget(frame);
        } else {
            unsafe {
                frame.fill_missing_fields(validate)?;
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

enum ArraySlot {
    Slot(PtrUninit),
    Done,
}

struct ArrayFrame {
    array_shape: &'static Shape,
    element_shape: &'static Shape,
    base: PtrUninit,
    count: usize,
    stride: usize,
    initialized: usize,
}

impl ArrayFrame {
    fn new(
        array_shape: &'static Shape,
        array: ArrayDef,
        base: PtrUninit,
        element_layout: Layout,
    ) -> Self {
        Self {
            array_shape,
            element_shape: array.t(),
            base,
            count: array.n,
            stride: element_layout.size(),
            initialized: 0,
        }
    }

    fn next_slot(&self) -> Option<PtrUninit> {
        (self.initialized < self.count).then(|| {
            let ptr = unsafe {
                self.base
                    .as_mut_byte_ptr()
                    .add(self.initialized * self.stride)
            };
            PtrUninit::new(ptr)
        })
    }

    unsafe fn mark_initialized(&mut self) {
        debug_assert!(self.initialized < self.count);
        self.initialized += 1;
    }

    fn finish(self) -> Result<(), DeserializeError> {
        if self.initialized == self.count {
            core::mem::forget(self);
            return Ok(());
        }

        Err(vm_error(
            None,
            DeserializeErrorKind::InvalidValue {
                message: format!(
                    "expected array of length {}, got {} while deserializing {}",
                    self.count, self.initialized, self.array_shape
                )
                .into(),
            },
        ))
    }
}

impl Drop for ArrayFrame {
    fn drop(&mut self) {
        for index in (0..self.initialized).rev() {
            let ptr = unsafe { self.base.as_mut_byte_ptr().add(index * self.stride) };
            unsafe {
                let _ = self.element_shape.call_drop_in_place(PtrMut::new(ptr));
            }
        }
    }
}

enum DynamicFrame {
    Array {
        dynamic_shape: &'static Shape,
        dynamic: DynamicValueDef,
        ptr: PtrUninit,
        active: bool,
    },
    Object {
        dynamic_shape: &'static Shape,
        dynamic: DynamicValueDef,
        ptr: PtrUninit,
        active: bool,
    },
}

impl DynamicFrame {
    fn array(dynamic_shape: &'static Shape, dynamic: DynamicValueDef, ptr: PtrUninit) -> Self {
        Self::Array {
            dynamic_shape,
            dynamic,
            ptr,
            active: true,
        }
    }

    fn object(dynamic_shape: &'static Shape, dynamic: DynamicValueDef, ptr: PtrUninit) -> Self {
        Self::Object {
            dynamic_shape,
            dynamic,
            ptr,
            active: true,
        }
    }

    fn finish(mut self) {
        match &mut self {
            Self::Array {
                dynamic,
                ptr,
                active,
                ..
            } => {
                if let Some(end_array) = dynamic.vtable.end_array {
                    unsafe {
                        end_array((*ptr).assume_init());
                    }
                }
                *active = false;
            }
            Self::Object {
                dynamic,
                ptr,
                active,
                ..
            } => {
                if let Some(end_object) = dynamic.vtable.end_object {
                    unsafe {
                        end_object((*ptr).assume_init());
                    }
                }
                *active = false;
            }
        }
        core::mem::forget(self);
    }

    fn push_array_element(&mut self, scratch: &ScratchSlot) -> Result<(), DeserializeError> {
        let Self::Array { dynamic, ptr, .. } = self else {
            return Err(vm_error(
                None,
                DeserializeErrorKind::InvalidValue {
                    message: "dynamic object frame cannot accept an array element".into(),
                },
            ));
        };
        unsafe {
            (dynamic.vtable.push_array_element)(ptr.assume_init(), scratch_ptr_mut(scratch));
        }
        Ok(())
    }

    fn insert_object_entry(
        &mut self,
        key: &str,
        scratch: &ScratchSlot,
    ) -> Result<(), DeserializeError> {
        let Self::Object { dynamic, ptr, .. } = self else {
            return Err(vm_error(
                None,
                DeserializeErrorKind::InvalidValue {
                    message: "dynamic array frame cannot accept an object entry".into(),
                },
            ));
        };
        unsafe {
            (dynamic.vtable.insert_object_entry)(ptr.assume_init(), key, scratch_ptr_mut(scratch));
        }
        Ok(())
    }
}

impl Drop for DynamicFrame {
    fn drop(&mut self) {
        let (dynamic_shape, ptr, active) = match self {
            Self::Array {
                dynamic_shape,
                ptr,
                active,
                ..
            }
            | Self::Object {
                dynamic_shape,
                ptr,
                active,
                ..
            } => (*dynamic_shape, *ptr, *active),
        };

        if active {
            unsafe {
                let _ = dynamic_shape.call_drop_in_place(ptr.assume_init());
            }
        }
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

struct PointerSliceFrame {
    pointer_shape: &'static Shape,
    pointer: PointerDef,
    pointer_layout: Layout,
    pointer_ptr: PtrUninit,
    builder: PtrMut,
    active: bool,
}

impl PointerSliceFrame {
    fn new(
        pointer_shape: &'static Shape,
        pointer: PointerDef,
        pointer_layout: Layout,
        pointer_ptr: PtrUninit,
    ) -> Result<Self, DeserializeError> {
        let vtable = pointer
            .vtable
            .slice_builder_vtable
            .ok_or_else(|| unsupported(pointer_shape, "pointer slice builder"))?;
        Ok(Self {
            pointer_shape,
            pointer,
            pointer_layout,
            pointer_ptr,
            builder: (vtable.new_fn)(),
            active: true,
        })
    }

    fn push(&mut self, scratch: &ScratchSlot) -> Result<(), DeserializeError> {
        let vtable = self
            .pointer
            .vtable
            .slice_builder_vtable
            .ok_or_else(|| unsupported(self.pointer_shape, "pointer slice builder"))?;
        unsafe {
            (vtable.push_fn)(self.builder, scratch_ptr_mut(scratch));
        }
        Ok(())
    }

    fn finish(mut self) -> Result<(), DeserializeError> {
        let vtable = self
            .pointer
            .vtable
            .slice_builder_vtable
            .ok_or_else(|| unsupported(self.pointer_shape, "pointer slice builder"))?;
        let pointer_ptr = unsafe { (vtable.convert_fn)(self.builder) };
        self.active = false;
        unsafe {
            core::ptr::copy_nonoverlapping(
                pointer_ptr.as_byte_ptr(),
                self.pointer_ptr.as_mut_byte_ptr(),
                self.pointer_layout.size(),
            );
            if self.pointer_layout.size() != 0 {
                alloc::alloc::dealloc(pointer_ptr.as_byte_ptr() as *mut u8, self.pointer_layout);
            }
        }
        core::mem::forget(self);
        Ok(())
    }
}

impl Drop for PointerSliceFrame {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        if let Some(vtable) = self.pointer.vtable.slice_builder_vtable {
            unsafe {
                (vtable.free_fn)(self.builder);
            }
        }
    }
}

#[derive(Clone, Copy)]
struct MapStepPlan<'program> {
    map: MapDef,
    key_plan: &'program MapKeyPlan,
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

        while self.arrays.pop().is_some() {}
        while self.dynamic_values.pop().is_some() {}
        while self.maps.pop().is_some() {}
        while self.sets.pop().is_some() {}
        while self.pointer_slices.pop().is_some() {}
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
            JsonOp::ReadParsedScalar { shape } => {
                let (value, span) = self.parser.read_scalar_token()?;
                unsafe {
                    write_parsed_scalar(shape, self.base, value, span)?;
                }
                Ok(Control::Continue)
            }
            JsonOp::ReadBuilderShape {
                shape,
                builder_shape,
                builder_layout,
                builder_program,
            } => {
                let scratch = self.scratch.reserve(*builder_layout);
                let target_ptr = self.base;
                self.base = scratch_ptr_uninit(&scratch);
                Ok(call_program_or_block_then(
                    builder_program,
                    Continuation::BuilderShape {
                        shape,
                        builder_shape,
                        target_ptr,
                        old_base: target_ptr,
                        scratch,
                    },
                ))
            }
            JsonOp::ReadTransparent {
                field_offset,
                field_shape,
                field_program,
                field_scalar,
            } => {
                let field_ptr = unsafe { self.base.field_uninit(*field_offset) };
                if let Some(scalar) = field_scalar {
                    let (value, span) = self.parser.read_scalar_token()?;
                    unsafe {
                        scalar.write(field_shape, field_ptr, value, span)?;
                    }
                    return Ok(Control::Continue);
                }

                let old_base = self.base;
                self.base = field_ptr;
                Ok(call_program_or_block_then(
                    field_program,
                    Continuation::Transparent { old_base },
                ))
            }
            JsonOp::ReadProxy {
                proxy,
                proxy_layout,
                proxy_program,
            } => {
                let scratch = self.scratch.reserve(*proxy_layout);
                let target_ptr = self.base;
                self.base = scratch_ptr_uninit(&scratch);
                Ok(call_program_or_block_then(
                    proxy_program,
                    Continuation::Proxy {
                        proxy,
                        target_ptr,
                        old_base: target_ptr,
                        scratch,
                    },
                ))
            }
            JsonOp::ReadUnitStruct { shape } => {
                self.read_unit_struct(shape)?;
                Ok(Control::Continue)
            }
            JsonOp::ReadTupleStruct {
                shape,
                fields,
                tracking,
            } => self.read_tuple_struct(shape, fields, *tracking),
            JsonOp::ReadScalarStruct {
                shape,
                fields,
                dispatch,
            } => {
                self.read_scalar_struct(shape, fields, dispatch.as_ref(), false)?;
                Ok(Control::Continue)
            }
            JsonOp::ReadScalarStructValidate {
                shape,
                fields,
                dispatch,
            } => {
                self.read_scalar_struct(shape, fields, dispatch.as_ref(), true)?;
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
                    Continuation::FinishStructUnchecked { tracking },
                ))
            }
            JsonOp::ReadStructValidate {
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
            JsonOp::ReadFlattenStruct { shape, plan } => {
                self.read_flatten_struct(shape, plan)?;
                Ok(Control::Continue)
            }
            JsonOp::ReadExternalEnum {
                shape,
                enum_type,
                variants,
            } => self.read_external_enum(shape, *enum_type, variants),
            JsonOp::ReadNumericEnum {
                shape,
                enum_type,
                variants,
            } => self.read_numeric_enum(shape, *enum_type, variants),
            JsonOp::ReadUntaggedEnum {
                shape,
                enum_type,
                variants,
            } => self.read_untagged_enum(shape, *enum_type, variants),
            JsonOp::ReadCowEnum {
                shape,
                enum_type,
                owned_variant,
            } => self.read_cow_enum(shape, *enum_type, owned_variant),
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
            JsonOp::ReadArray {
                array_shape,
                array,
                element_layout,
                loop_id,
            } => {
                self.parser.consume_array_start_fast()?;
                self.arrays.push(ArrayFrame::new(
                    array_shape,
                    *array,
                    self.base,
                    *element_layout,
                ));
                Ok(Control::CallBlockThen(*loop_id, Continuation::FinishArray))
            }
            JsonOp::ArrayNext {
                array,
                element_program,
                element_scalar,
                element_option_scalar,
                element_layout: _,
                loop_id,
            } => self.step_array_next(
                *array,
                element_program,
                *element_scalar,
                *element_option_scalar,
                *loop_id,
            ),
            JsonOp::ReadDynamicValue {
                dynamic_shape,
                dynamic,
                loop_id,
            } => self.read_dynamic_value(dynamic_shape, *dynamic, *loop_id),
            JsonOp::DynamicNext {
                dynamic_shape,
                dynamic_layout,
                value_program,
                loop_id,
            } => self.step_dynamic_next(dynamic_shape, *dynamic_layout, value_program, *loop_id),
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
                key_plan,
                value_program,
                value_scalar,
                value_layout,
                loop_id,
            } => self.step_map_next(MapStepPlan {
                map: *map,
                key_plan: key_plan.as_ref(),
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
            JsonOp::ReadPointerString {
                pointer_shape,
                pointer,
            } => {
                let (value, span) = self.parser.read_scalar_token()?;
                unsafe {
                    write_pointer_string(pointer_shape, *pointer, self.base, value, span)?;
                }
                Ok(Control::Continue)
            }
            JsonOp::ReadPointerSlice {
                pointer_shape,
                pointer,
                pointer_layout,
                element_layout: _,
                loop_id,
            } => {
                self.parser.consume_array_start_fast()?;
                self.pointer_slices.push(PointerSliceFrame::new(
                    pointer_shape,
                    *pointer,
                    *pointer_layout,
                    self.base,
                )?);
                Ok(Control::CallBlockThen(
                    *loop_id,
                    Continuation::FinishPointerSlice,
                ))
            }
            JsonOp::PointerSliceNext {
                pointer,
                element_program,
                element_scalar,
                element_option_scalar,
                element_layout,
                loop_id,
            } => self.step_pointer_slice_next(
                *pointer,
                element_program,
                *element_scalar,
                *element_option_scalar,
                *element_layout,
                *loop_id,
            ),
        }
    }

    fn after_return(
        &mut self,
        continuation: Self::Continuation,
    ) -> Result<Control<'program, ExecBlock, ExecOp, Self::Continuation>, Self::Error> {
        match continuation {
            Continuation::Transparent { old_base } => {
                self.base = old_base;
                Ok(Control::Continue)
            }
            Continuation::Proxy {
                proxy,
                target_ptr,
                old_base,
                scratch,
            } => {
                let result = self.finish_proxy(proxy, target_ptr, scratch);
                self.base = old_base;
                result?;
                Ok(Control::Continue)
            }
            Continuation::BuilderShape {
                shape,
                builder_shape,
                target_ptr,
                old_base,
                scratch,
            } => {
                let result = self.finish_builder_shape(shape, builder_shape, target_ptr, scratch);
                self.base = old_base;
                result?;
                Ok(Control::Continue)
            }
            Continuation::FinishStruct { tracking } => {
                unsafe {
                    self.finish_struct_frame(tracking)?;
                }
                Ok(Control::Continue)
            }
            Continuation::FinishStructUnchecked { tracking } => {
                unsafe {
                    self.finish_struct_frame_unchecked(tracking)?;
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
            Continuation::TupleFieldDone {
                tracking,
                fields,
                index,
                old_base,
                mode,
                validate,
            } => {
                self.mark_struct_field(tracking, index, Span { offset: 0, len: 0 });
                self.base = old_base;
                self.read_tuple_struct_next(fields, tracking, index + 1, mode, validate)
            }
            Continuation::ExternalEnumSingleField {
                tracking,
                index,
                span,
                old_base,
                close_object,
                validate,
            } => {
                self.mark_struct_field(tracking, index, span);
                self.base = old_base;
                self.finish_external_enum_payload(tracking, close_object, validate)
            }
            Continuation::ExternalEnumTupleField {
                tracking,
                fields,
                index,
                span,
                old_base,
                close_object,
                validate,
            } => {
                self.mark_struct_field(tracking, index, span);
                self.base = old_base;
                self.read_external_enum_tuple_next(
                    fields,
                    tracking,
                    index + 1,
                    close_object,
                    validate,
                )
            }
            Continuation::ExternalEnumStruct { tracking, validate } => {
                self.consume_external_enum_object_end()?;
                unsafe {
                    self.finish_struct_frame_with_validation(tracking, validate)?;
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
            Continuation::FinishArray => {
                let array = self
                    .arrays
                    .pop()
                    .expect("array frame is present after array program");
                array.finish()?;
                Ok(Control::Continue)
            }
            Continuation::ArrayElement { old_base, loop_id } => {
                unsafe {
                    self.mark_array_element_initialized();
                }
                self.base = old_base;
                Ok(Control::CallBlock(loop_id))
            }
            Continuation::FinishDynamicValue => {
                let dynamic = self
                    .dynamic_values
                    .pop()
                    .expect("dynamic frame is present after dynamic program");
                dynamic.finish();
                Ok(Control::Continue)
            }
            Continuation::DynamicArrayElement {
                old_base,
                scratch,
                loop_id,
            } => {
                self.push_dynamic_array_element(&scratch)?;
                self.scratch.release(scratch);
                self.base = old_base;
                Ok(Control::CallBlock(loop_id))
            }
            Continuation::DynamicObjectEntry {
                key,
                old_base,
                scratch,
                loop_id,
            } => {
                self.insert_dynamic_object_entry(&key, &scratch)?;
                self.scratch.release(scratch);
                self.base = old_base;
                Ok(Control::CallBlock(loop_id))
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
                key_plan,
                key,
                key_span,
                value_scratch,
                old_base,
                loop_id,
            } => {
                self.insert_map_entry(map, key_plan, key, key_span, value_scratch)?;
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
            Continuation::FinishPointerSlice => {
                let pointer_slice = self
                    .pointer_slices
                    .pop()
                    .expect("pointer slice frame is present after slice program");
                pointer_slice.finish()?;
                Ok(Control::Continue)
            }
            Continuation::PointerSliceElement {
                old_base,
                scratch,
                loop_id,
            } => {
                self.push_pointer_slice_element(&scratch)?;
                self.scratch.release(scratch);
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
    Transparent {
        old_base: PtrUninit,
    },
    Proxy {
        proxy: &'static ProxyDef,
        target_ptr: PtrUninit,
        old_base: PtrUninit,
        scratch: ScratchSlot,
    },
    BuilderShape {
        shape: &'static Shape,
        builder_shape: &'static Shape,
        target_ptr: PtrUninit,
        old_base: PtrUninit,
        scratch: ScratchSlot,
    },
    FinishStruct {
        tracking: StructTracking,
    },
    FinishStructUnchecked {
        tracking: StructTracking,
    },
    FieldDone {
        tracking: StructTracking,
        index: usize,
        span: Span,
        old_base: PtrUninit,
        loop_id: ExecBlock,
    },
    TupleFieldDone {
        tracking: StructTracking,
        fields: &'program [FieldPlan<ExecBlock>],
        index: usize,
        old_base: PtrUninit,
        mode: TupleContainerMode,
        validate: bool,
    },
    ExternalEnumSingleField {
        tracking: StructTracking,
        index: usize,
        span: Span,
        old_base: PtrUninit,
        close_object: bool,
        validate: bool,
    },
    ExternalEnumTupleField {
        tracking: StructTracking,
        fields: &'program [FieldPlan<ExecBlock>],
        index: usize,
        span: Span,
        old_base: PtrUninit,
        close_object: bool,
        validate: bool,
    },
    ExternalEnumStruct {
        tracking: StructTracking,
        validate: bool,
    },
    OptionSome {
        option: OptionDef,
        option_ptr: PtrUninit,
        old_base: PtrUninit,
        scratch: ScratchSlot,
    },
    FinishArray,
    ArrayElement {
        old_base: PtrUninit,
        loop_id: ExecBlock,
    },
    FinishDynamicValue,
    DynamicArrayElement {
        old_base: PtrUninit,
        scratch: ScratchSlot,
        loop_id: ExecBlock,
    },
    DynamicObjectEntry {
        key: String,
        old_base: PtrUninit,
        scratch: ScratchSlot,
        loop_id: ExecBlock,
    },
    FinishList,
    FinishSet,
    FinishMap,
    MapValueDone {
        map: MapDef,
        key_plan: &'program MapKeyPlan,
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
    FinishPointerSlice,
    PointerSliceElement {
        old_base: PtrUninit,
        scratch: ScratchSlot,
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
    facet_json_jit_active,
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

#[derive(Clone, Copy, Debug)]
enum TupleContainerMode {
    Sequence,
    Object,
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

    unsafe fn fill_missing_fields(self, validate: bool) -> Result<(), DeserializeError> {
        match self {
            Self::Bitset(frame) => unsafe { frame.fill_missing_fields(validate) },
            Self::Heap(frame) => unsafe { frame.fill_missing_fields(validate) },
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

    unsafe fn fill_missing_fields(mut self, validate: bool) -> Result<(), DeserializeError> {
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
        if validate {
            validate_completed_shape(self.shape, self.base)?;
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

    unsafe fn fill_missing_fields(mut self, validate: bool) -> Result<(), DeserializeError> {
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
        if validate {
            validate_completed_shape(self.shape, self.base)?;
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

unsafe fn write_map_key_plan(
    plan: &MapKeyPlan,
    dst: PtrUninit,
    key: String,
    span: Span,
) -> Result<(), DeserializeError> {
    match plan {
        MapKeyPlan::Scalar { shape, scalar, .. } => unsafe {
            write_map_key(shape, *scalar, dst, key, span)
        },
        MapKeyPlan::MetadataContainer { .. } => unsafe {
            write_metadata_container_map_key(plan, dst, key, span)
        },
    }
}

unsafe fn write_metadata_container_map_key(
    plan: &MapKeyPlan,
    dst: PtrUninit,
    key: String,
    span: Span,
) -> Result<(), DeserializeError> {
    let MapKeyPlan::MetadataContainer {
        shape,
        fields,
        dispatch,
        value_index,
        value,
        ..
    } = plan
    else {
        unreachable!("metadata map key writer only accepts metadata key plans");
    };

    let mut frame = StructFrame::<MetadataKeyField, InitializedLedger<Span>>::new(
        shape,
        dst,
        fields,
        dispatch.as_ref(),
    );
    let value_field = &fields[*value_index];
    unsafe {
        write_map_key_plan(
            value.as_ref(),
            dst.field_uninit(value_field.offset),
            key,
            span,
        )?;
    }
    frame.mark_seen(*value_index, span);

    for (index, field) in fields.iter().enumerate() {
        if index == *value_index || frame.seen.is_initialized(index) {
            continue;
        }
        if field.metadata == Some("span")
            && unsafe { write_metadata_span(field.shape, dst.field_uninit(field.offset), span) }
        {
            frame.mark_seen(index, span);
        }
    }

    unsafe { frame.fill_missing_fields(shape.vtable.has_invariants()) }
}

unsafe fn write_metadata_span(shape: &'static Shape, dst: PtrUninit, span: Span) -> bool {
    if shape.is_type::<Span>() {
        unsafe {
            dst.put(span);
        }
        return true;
    }

    if let Def::Option(option) = shape.def
        && option.t().is_type::<Span>()
    {
        let mut span = span;
        unsafe {
            (option.vtable.init_some)(dst, PtrMut::new((&mut span as *mut Span).cast::<u8>()));
        }
        return true;
    }

    false
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
        _ if shape.vtable.has_parse() => {
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
        _ if shape.vtable.has_parse() => {
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

unsafe fn write_parsed_scalar(
    shape: &'static Shape,
    dst: PtrUninit,
    value: JsonScalarToken<'_>,
    span: Span,
) -> Result<(), DeserializeError> {
    let JsonScalarToken::Str(value) = value else {
        return Err(type_mismatch(span, shape, value.kind_name()));
    };
    match unsafe { shape.call_parse(value.as_ref(), dst) } {
        Some(Ok(())) => Ok(()),
        Some(Err(err)) => Err(invalid_value(span, format!("{err}"))),
        None => Err(unsupported(shape, "parsed scalar")),
    }
}

unsafe fn write_dynamic_scalar(
    dynamic: DynamicValueDef,
    dst: PtrUninit,
    value: JsonScalarToken<'_>,
    span: Span,
) -> Result<(), DeserializeError> {
    unsafe {
        match value {
            JsonScalarToken::Null => (dynamic.vtable.set_null)(dst),
            JsonScalarToken::Bool(value) => (dynamic.vtable.set_bool)(dst, value),
            JsonScalarToken::Str(value) => (dynamic.vtable.set_str)(dst, value.as_ref()),
            JsonScalarToken::U64(value) => (dynamic.vtable.set_u64)(dst, value),
            JsonScalarToken::I64(value) => (dynamic.vtable.set_i64)(dst, value),
            JsonScalarToken::U128(value) => {
                let value = value.to_string();
                (dynamic.vtable.set_str)(dst, value.as_str());
            }
            JsonScalarToken::I128(value) => {
                let value = value.to_string();
                (dynamic.vtable.set_str)(dst, value.as_str());
            }
            JsonScalarToken::F64(value) => {
                if !(dynamic.vtable.set_f64)(dst, value) {
                    return Err(vm_error(
                        Some(span),
                        DeserializeErrorKind::InvalidValue {
                            message: "f64 value is not representable in dynamic value".into(),
                        },
                    ));
                }
            }
            JsonScalarToken::Other => {
                return Err(vm_error(
                    Some(span),
                    DeserializeErrorKind::UnexpectedToken {
                        expected: "JSON scalar",
                        got: "scalar".into(),
                    },
                ));
            }
        }
    }

    Ok(())
}

unsafe fn write_pointer_string(
    pointer_shape: &'static Shape,
    pointer: PointerDef,
    dst: PtrUninit,
    value: JsonScalarToken<'_>,
    span: Span,
) -> Result<(), DeserializeError> {
    let JsonScalarToken::Str(value) = value else {
        return Err(type_mismatch(span, pointer_shape, value.kind_name()));
    };

    match pointer.known {
        Some(KnownPointer::Box) => unsafe {
            dst.put::<Box<str>>(value.into_owned().into_boxed_str());
            Ok(())
        },
        Some(KnownPointer::Rc) => unsafe {
            dst.put::<Rc<str>>(Rc::from(value.as_ref()));
            Ok(())
        },
        Some(KnownPointer::Arc) => unsafe {
            dst.put::<Arc<str>>(Arc::from(value.as_ref()));
            Ok(())
        },
        _ => Err(unsupported(pointer_shape, "string pointer construction")),
    }
}

fn pointer_to_str(pointer: PointerDef) -> bool {
    pointer.pointee().is_some_and(|shape| *shape == *str::SHAPE)
        && matches!(
            pointer.known,
            Some(KnownPointer::Box | KnownPointer::Rc | KnownPointer::Arc)
        )
}

fn pointer_slice_element_shape(pointer: PointerDef) -> Result<&'static Shape, DeserializeError> {
    let pointee = pointer
        .pointee()
        .ok_or_else(|| unsupported_shape_message("opaque pointer"))?;
    let Def::Slice(slice) = pointee.def else {
        return Err(unsupported_shape_message("pointer target is not a slice"));
    };
    Ok(slice.t())
}

fn fixed_array_length_error(array: ArrayDef, got: usize) -> DeserializeError {
    vm_error(
        None,
        DeserializeErrorKind::InvalidValue {
            message: format!("expected array of length {}, got {got}", array.n).into(),
        },
    )
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
