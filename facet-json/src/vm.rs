extern crate alloc;

use alloc::{borrow::Cow, format, string::ToString, vec, vec::Vec};
use core::{marker::PhantomData, mem};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

use facet_core::{Facet, ScalarType, Shape};
use facet_format::{DeserializeError, DeserializeErrorKind, FormatParser, ScalarValue, SpanGuard};
use facet_reflect::{
    HeapValue, Partial, ReflectError, ReflectErrorKind, Span, TypePlan, TypePlanCore,
};

use crate::{
    JsonParser,
    bytecode::{
        JsonBorrow, JsonBytes, JsonEnum, JsonEnumRepr, JsonField, JsonLowered, JsonOp, JsonProgram,
        JsonScalar, JsonString, JsonStringRole, JsonStruct, LowerError, UnknownFields,
        lower_type_plan,
    },
    parser::{JsonArrayStep, JsonObjectStep, JsonValueStart},
};

/// Runtime counters from the experimental JSON VM path.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub struct JsonVmStats {
    /// Number of VM dispatch steps executed.
    pub steps: usize,
    /// Maximum number of interpreter frames alive at once.
    pub max_vm_frames: usize,
    /// Maximum number of `Partial` frames alive at once.
    pub max_partial_frames: usize,
    /// Number of native-stack samples recorded by the VM dispatch loop.
    pub native_stack_samples: usize,
    /// Maximum observed native-stack growth in bytes while executing the VM.
    pub native_stack_bytes: usize,
    #[cfg(feature = "stacker")]
    first_native_stack_remaining: Option<usize>,
    #[cfg(feature = "stacker")]
    min_native_stack_remaining: usize,
}

impl JsonVmStats {
    fn record(&mut self, vm_frames: usize, partial_frames: usize) {
        self.steps += 1;
        self.max_vm_frames = self.max_vm_frames.max(vm_frames);
        self.max_partial_frames = self.max_partial_frames.max(partial_frames);
        self.record_native_stack();
    }

    #[cfg(feature = "stacker")]
    fn record_native_stack(&mut self) {
        let Some(remaining) = stacker::remaining_stack() else {
            return;
        };
        self.first_native_stack_remaining.get_or_insert(remaining);
        self.min_native_stack_remaining = if self.native_stack_samples == 0 {
            remaining
        } else {
            self.min_native_stack_remaining.min(remaining)
        };
        self.native_stack_samples += 1;
        if let Some(first) = self.first_native_stack_remaining {
            self.native_stack_bytes = first.saturating_sub(self.min_native_stack_remaining);
        }
    }

    #[cfg(not(feature = "stacker"))]
    fn record_native_stack(&mut self) {
        // Native stack measurement is feature-gated; frame counters are always available.
    }
}

/// Deserialize a value from a JSON string through the experimental VM path.
#[doc(hidden)]
pub fn from_str_vm<T>(input: &str) -> Result<T, DeserializeError>
where
    T: Facet<'static>,
{
    JsonVmPlan::<T>::build()?.from_str(input)
}

/// Deserialize a value from JSON bytes through the experimental VM path.
#[doc(hidden)]
pub fn from_slice_vm<T>(input: &[u8]) -> Result<T, DeserializeError>
where
    T: Facet<'static>,
{
    JsonVmPlan::<T>::build()?.from_slice(input)
}

/// Deserialize a value from a JSON string through the experimental VM path and return VM stats.
#[doc(hidden)]
pub fn from_str_vm_with_stats<T>(input: &str) -> Result<(T, JsonVmStats), DeserializeError>
where
    T: Facet<'static>,
{
    JsonVmPlan::<T>::build()?.from_str_with_stats(input)
}

/// Deserialize a value from JSON bytes through the experimental VM path and return VM stats.
#[doc(hidden)]
pub fn from_slice_vm_with_stats<T>(input: &[u8]) -> Result<(T, JsonVmStats), DeserializeError>
where
    T: Facet<'static>,
{
    JsonVmPlan::<T>::build()?.from_slice_with_stats(input)
}

/// Reusable lowered JSON VM deserialization plan.
#[doc(hidden)]
pub struct JsonVmPlan<T: ?Sized> {
    inner: Arc<JsonVmPlanInner>,
    _marker: PhantomData<fn() -> T>,
}

impl<T: ?Sized> Clone for JsonVmPlan<T> {
    fn clone(&self) -> Self {
        Self {
            inner: Arc::clone(&self.inner),
            _marker: PhantomData,
        }
    }
}

impl<T> JsonVmPlan<T>
where
    T: Facet<'static>,
{
    /// Build or reuse the lowered VM deserialization plan for `T`.
    #[doc(hidden)]
    pub fn build() -> Result<Self, DeserializeError> {
        cached_vm_plan::<T>()
    }

    /// Deserialize a JSON string through this plan.
    #[doc(hidden)]
    pub fn from_str(&self, input: &str) -> Result<T, DeserializeError> {
        let mut parser = JsonParser::<true>::new(input.as_bytes());
        deserialize_owned_with_vm::<T, true, false>(&mut parser, &self.inner)
            .map(|(value, _)| value)
    }

    /// Deserialize JSON bytes through this plan.
    #[doc(hidden)]
    pub fn from_slice(&self, input: &[u8]) -> Result<T, DeserializeError> {
        let mut parser = JsonParser::<false>::new(input);
        deserialize_owned_with_vm::<T, false, false>(&mut parser, &self.inner)
            .map(|(value, _)| value)
    }

    /// Deserialize a JSON string through this plan and return VM stats.
    #[doc(hidden)]
    pub fn from_str_with_stats(&self, input: &str) -> Result<(T, JsonVmStats), DeserializeError> {
        let mut parser = JsonParser::<true>::new(input.as_bytes());
        deserialize_owned_with_vm::<T, true, true>(&mut parser, &self.inner)
    }

    /// Deserialize JSON bytes through this plan and return VM stats.
    #[doc(hidden)]
    pub fn from_slice_with_stats(
        &self,
        input: &[u8],
    ) -> Result<(T, JsonVmStats), DeserializeError> {
        let mut parser = JsonParser::<false>::new(input);
        deserialize_owned_with_vm::<T, false, true>(&mut parser, &self.inner)
    }
}

fn deserialize_owned_with_vm<T, const TRUSTED_UTF8: bool, const COLLECT_STATS: bool>(
    parser: &mut JsonParser<'_, TRUSTED_UTF8>,
    plan: &JsonVmPlanInner,
) -> Result<(T, JsonVmStats), DeserializeError>
where
    T: Facet<'static>,
{
    let partial = Partial::alloc_owned_with_plan(Arc::clone(&plan.type_plan))?;

    let mut vm = JsonVm::<TRUSTED_UTF8, COLLECT_STATS>::new(parser, plan.lowered.as_ref());

    // SAFETY: owned deserialization does not borrow from the JSON input.
    #[allow(unsafe_code)]
    let partial: Partial<'_, false> =
        unsafe { mem::transmute::<Partial<'static, false>, Partial<'_, false>>(partial) };

    let partial = vm.deserialize_into(partial)?;
    let stats = vm.stats;
    let last_span = vm.last_span;

    let _guard = SpanGuard::new(last_span);
    let heap_value = partial.build()?;

    // SAFETY: the heap value came from an owned Partial, so it contains no input borrows.
    #[allow(unsafe_code)]
    let heap_value: HeapValue<'static, false> =
        unsafe { mem::transmute::<HeapValue<'_, false>, HeapValue<'static, false>>(heap_value) };

    Ok((heap_value.materialize::<T>()?, stats))
}

struct JsonVmPlanInner {
    type_plan: Arc<TypePlanCore>,
    lowered: Arc<JsonLowered>,
}

fn vm_plan_cache() -> &'static Mutex<HashMap<&'static Shape, Arc<JsonVmPlanInner>>> {
    static PLAN_CACHE: OnceLock<Mutex<HashMap<&'static Shape, Arc<JsonVmPlanInner>>>> =
        OnceLock::new();
    PLAN_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

fn cached_vm_plan<T>() -> Result<JsonVmPlan<T>, DeserializeError>
where
    T: Facet<'static>,
{
    let mut guard = vm_plan_cache()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner());

    if let Some(plan) = guard.get(&T::SHAPE) {
        return Ok(JsonVmPlan {
            inner: Arc::clone(plan),
            _marker: PhantomData,
        });
    }

    let type_plan = TypePlan::<T>::build()?.core();
    let lowered = Arc::new(lower_type_plan(&type_plan).map_err(lower_error)?);
    let plan = Arc::new(JsonVmPlanInner { type_plan, lowered });
    guard.insert(T::SHAPE, Arc::clone(&plan));
    Ok(JsonVmPlan {
        inner: plan,
        _marker: PhantomData,
    })
}

struct JsonVm<'parser, 'input, 'program, const TRUSTED_UTF8: bool, const COLLECT_STATS: bool> {
    parser: &'parser mut JsonParser<'input, TRUSTED_UTF8>,
    lowered: &'program JsonLowered,
    pending_value: Option<JsonValueStart<'input>>,
    last_span: Span,
    stats: JsonVmStats,
}

impl<'parser, 'input, 'program, const TRUSTED_UTF8: bool, const COLLECT_STATS: bool>
    JsonVm<'parser, 'input, 'program, TRUSTED_UTF8, COLLECT_STATS>
{
    fn new(
        parser: &'parser mut JsonParser<'input, TRUSTED_UTF8>,
        lowered: &'program JsonLowered,
    ) -> Self {
        Self {
            parser,
            lowered,
            pending_value: None,
            last_span: Span::new(0, 0),
            stats: JsonVmStats::default(),
        }
    }

    fn deserialize_into(
        &mut self,
        mut partial: Partial<'input, false>,
    ) -> Result<Partial<'input, false>, DeserializeError> {
        let mut frames = Vec::with_capacity(16);
        frames.push(VmFrame::Program {
            program: &self.lowered.program,
            pc: 0,
        });

        while let Some(frame) = frames.pop() {
            match frame {
                VmFrame::Program { program, pc } => {
                    partial = self.run_program(partial, &mut frames, program, pc)?;
                }
                VmFrame::EndCurrent => {
                    self.record_step(frames.len() + 1, partial.frame_count());
                    partial = self.end(partial)?;
                }
                VmFrame::Struct { plan, seen } => {
                    self.record_step(frames.len() + 1, partial.frame_count());
                    partial = self.step_struct(partial, &mut frames, plan, seen)?;
                }
                VmFrame::List { element } => {
                    self.record_step(frames.len() + 1, partial.frame_count());
                    partial = self.step_list(partial, &mut frames, element)?;
                }
            }
        }

        Ok(partial)
    }

    fn run_program(
        &mut self,
        mut partial: Partial<'input, false>,
        frames: &mut Vec<VmFrame<'program>>,
        program: &'program JsonProgram,
        mut pc: usize,
    ) -> Result<Partial<'input, false>, DeserializeError> {
        while let Some(op) = program.get(pc) {
            self.record_step(frames.len() + 1, partial.frame_count());
            pc += 1;
            let continuation = (pc < program.len()).then_some((program, pc));
            let (next_partial, control) = self.execute_op(partial, frames, op, continuation)?;
            partial = next_partial;
            match control {
                ProgramControl::Continue => {}
                ProgramControl::Suspend | ProgramControl::Stop => return Ok(partial),
            }
        }

        Ok(partial)
    }

    fn record_step(&mut self, vm_frames: usize, partial_frames: usize) {
        if COLLECT_STATS {
            self.stats.record(vm_frames, partial_frames);
        }
    }

    fn execute_op(
        &mut self,
        mut partial: Partial<'input, false>,
        frames: &mut Vec<VmFrame<'program>>,
        op: &'program JsonOp,
        continuation: ProgramContinuation<'program>,
    ) -> Result<(Partial<'input, false>, ProgramControl), DeserializeError> {
        match op {
            JsonOp::EnterShape { shape } => {
                self.check_enter_shape(&partial, shape)?;
            }
            JsonOp::Null => {
                let value = self.next_value_start("null")?;
                let JsonValueStart::Scalar(ScalarValue::Null, _) = value else {
                    return Err(self.unexpected_value(&value, "null"));
                };
                partial = self.set_default(partial)?;
            }
            JsonOp::Scalar(policy) => {
                let scalar = self.next_scalar("scalar")?;
                partial = self.set_scalar(partial, scalar, *policy)?;
            }
            JsonOp::String(policy) => {
                partial = self.deserialize_string(partial, *policy)?;
            }
            JsonOp::Bytes(JsonBytes::Array) => {
                return Err(self.unsupported("VM byte-array deserialization"));
            }
            JsonOp::Bytes(JsonBytes::Hex) => {
                return Err(self.unsupported("VM hex byte deserialization"));
            }
            JsonOp::RawJson => return Err(self.unsupported("VM raw JSON capture")),
            JsonOp::Struct(plan) => {
                self.expect_object_start("object")?;
                push_continuation(frames, continuation);
                frames.push(VmFrame::Struct {
                    plan,
                    seen: SeenFields::new(plan.fields.len()),
                });
                return Ok((partial, ProgramControl::Suspend));
            }
            JsonOp::Tuple { .. } => return Err(self.unsupported("VM tuple deserialization")),
            JsonOp::List {
                element,
                byte_optimized,
            } => {
                if *byte_optimized {
                    return Err(self.unsupported("VM byte-optimized list deserialization"));
                }
                self.expect_array_start("array")?;
                partial = self.init_list(partial)?;
                push_continuation(frames, continuation);
                frames.push(VmFrame::List { element });
                return Ok((partial, ProgramControl::Suspend));
            }
            JsonOp::Array { .. } => return Err(self.unsupported("VM array deserialization")),
            JsonOp::Map { .. } => return Err(self.unsupported("VM map deserialization")),
            JsonOp::Set { .. } => return Err(self.unsupported("VM set deserialization")),
            JsonOp::Option { some } => {
                let value = self.next_value_start("option value")?;
                match value {
                    JsonValueStart::Scalar(ScalarValue::Null, _) => {
                        partial = self.set_default(partial)?;
                    }
                    value => {
                        self.pending_value = Some(value);
                        partial = self.begin_some(partial)?;
                        push_continuation(frames, continuation);
                        frames.push(VmFrame::EndCurrent);
                        frames.push(VmFrame::Program {
                            program: some,
                            pc: 0,
                        });
                        return Ok((partial, ProgramControl::Suspend));
                    }
                }
            }
            JsonOp::Result { .. } => return Err(self.unsupported("VM result deserialization")),
            JsonOp::Enum(en) => return Err(self.unsupported_enum(en)),
            JsonOp::Pointer { pointee } => {
                partial = self.begin_pointer(partial, frames, pointee, continuation)?;
                return Ok((partial, ProgramControl::Suspend));
            }
            JsonOp::Transparent { inner } => {
                partial = self.begin_inner(partial)?;
                push_continuation(frames, continuation);
                frames.push(VmFrame::EndCurrent);
                frames.push(VmFrame::Program {
                    program: inner,
                    pc: 0,
                });
                return Ok((partial, ProgramControl::Suspend));
            }
            JsonOp::Proxy { .. } => return Err(self.unsupported("VM proxy deserialization")),
            JsonOp::OpaquePointer => return Err(self.unsupported("VM opaque pointer")),
            JsonOp::Opaque => return Err(self.unsupported("VM opaque value")),
            JsonOp::MetadataContainer => {
                return Err(self.unsupported("VM metadata container deserialization"));
            }
            JsonOp::Dynamic => return Err(self.unsupported("VM dynamic value deserialization")),
            JsonOp::CallBlock(block_id) => {
                let Some(program) = self.lowered.blocks.get(block_id) else {
                    return Err(
                        self.unsupported_owned(format!("VM missing recursive block {block_id:?}"))
                    );
                };
                push_continuation(frames, continuation);
                frames.push(VmFrame::Program { program, pc: 0 });
                return Ok((partial, ProgramControl::Suspend));
            }
            JsonOp::Return => return Ok((partial, ProgramControl::Stop)),
        }

        Ok((partial, ProgramControl::Continue))
    }

    fn step_struct(
        &mut self,
        mut partial: Partial<'input, false>,
        frames: &mut Vec<VmFrame<'program>>,
        plan: &'program JsonStruct,
        mut seen: SeenFields,
    ) -> Result<Partial<'input, false>, DeserializeError> {
        let step = self.parser.next_object_key_or_end()?;
        self.last_span = match &step {
            JsonObjectStep::End(span) | JsonObjectStep::Key { span, .. } => *span,
        };
        match step {
            JsonObjectStep::End(_) => Ok(partial),
            JsonObjectStep::Key { name, .. } => {
                let Some((idx, field)) = find_field(plan, name.as_ref()) else {
                    return self.handle_unknown_field(partial, frames, plan, seen, name.as_ref());
                };
                if field.skip_deserializing {
                    self.parser.skip_value()?;
                    frames.push(VmFrame::Struct { plan, seen });
                    return Ok(partial);
                }
                if field.flattened {
                    return Err(self.unsupported("VM flattened struct field"));
                }
                if seen.mark(idx) {
                    return Err(DeserializeErrorKind::DuplicateField {
                        field: Cow::Owned(name.to_string()),
                        first_span: None,
                    }
                    .with_span(self.last_span));
                }
                partial = self.begin_nth_field(partial, idx)?;
                if can_inline_field_program(&field.value) {
                    partial = self.run_program(partial, frames, &field.value, 0)?;
                    self.record_step(frames.len() + 1, partial.frame_count());
                    partial = self.end(partial)?;
                    frames.push(VmFrame::Struct { plan, seen });
                    return Ok(partial);
                }
                frames.push(VmFrame::Struct { plan, seen });
                frames.push(VmFrame::EndCurrent);
                frames.push(VmFrame::Program {
                    program: &field.value,
                    pc: 0,
                });
                Ok(partial)
            }
        }
    }

    fn handle_unknown_field(
        &mut self,
        partial: Partial<'input, false>,
        frames: &mut Vec<VmFrame<'program>>,
        plan: &'program JsonStruct,
        seen: SeenFields,
        name: &str,
    ) -> Result<Partial<'input, false>, DeserializeError> {
        match plan.unknown_fields {
            UnknownFields::Ignore => {
                self.parser.skip_value()?;
                frames.push(VmFrame::Struct { plan, seen });
                Ok(partial)
            }
            UnknownFields::Deny => Err(DeserializeErrorKind::UnknownField {
                field: Cow::Owned(name.to_string()),
                suggestion: None,
            }
            .with_span(self.last_span)),
            UnknownFields::Capture => Err(self.unsupported("VM flattened unknown-field capture")),
        }
    }

    fn step_list(
        &mut self,
        mut partial: Partial<'input, false>,
        frames: &mut Vec<VmFrame<'program>>,
        element: &'program JsonProgram,
    ) -> Result<Partial<'input, false>, DeserializeError> {
        let step = self.parser.next_array_value_or_end()?;
        match step {
            JsonArrayStep::End(span) => {
                self.last_span = span;
                Ok(partial)
            }
            JsonArrayStep::Value(value) => {
                self.last_span = value.span();
                self.pending_value = Some(value);
                partial = self.begin_list_item(partial)?;
                frames.push(VmFrame::List { element });
                frames.push(VmFrame::EndCurrent);
                frames.push(VmFrame::Program {
                    program: element,
                    pc: 0,
                });
                Ok(partial)
            }
        }
    }

    fn begin_pointer(
        &mut self,
        partial: Partial<'input, false>,
        frames: &mut Vec<VmFrame<'program>>,
        pointee: &'program JsonProgram,
        continuation: ProgramContinuation<'program>,
    ) -> Result<Partial<'input, false>, DeserializeError> {
        let (partial, action) =
            facet_dessert::begin_pointer(partial).map_err(|e| self.dessert_error(e))?;
        match action {
            facet_dessert::PointerAction::HandleAsScalar => {
                Err(self.unsupported("VM scalar pointer deserialization"))
            }
            facet_dessert::PointerAction::SliceBuilder => {
                Err(self.unsupported("VM smart-pointer slice deserialization"))
            }
            facet_dessert::PointerAction::SizedPointee => {
                push_continuation(frames, continuation);
                frames.push(VmFrame::EndCurrent);
                frames.push(VmFrame::Program {
                    program: pointee,
                    pc: 0,
                });
                Ok(partial)
            }
            _ => Err(self.unsupported("VM unknown pointer action")),
        }
    }

    fn deserialize_string(
        &mut self,
        partial: Partial<'input, false>,
        policy: JsonString,
    ) -> Result<Partial<'input, false>, DeserializeError> {
        if policy.borrow == JsonBorrow::Borrowed {
            return Err(self.unsupported("VM borrowed string deserialization"));
        }
        if policy.role != JsonStringRole::Value {
            return Err(self.unsupported("VM non-value string deserialization"));
        }
        let value = self.next_value_start("string")?;
        let JsonValueStart::Scalar(ScalarValue::Str(s), _) = value else {
            return Err(self.unexpected_value(&value, "string"));
        };
        self.set_string(partial, s)
    }

    fn set_scalar(
        &mut self,
        mut partial: Partial<'input, false>,
        scalar: ScalarValue<'input>,
        policy: JsonScalar,
    ) -> Result<Partial<'input, false>, DeserializeError> {
        let scalar_type = policy.ty;
        match scalar {
            ScalarValue::Unit | ScalarValue::Null => self.set_default(partial),
            ScalarValue::Bool(value) => self.set(partial, value),
            ScalarValue::Char(value) => self.set(partial, value),
            ScalarValue::I64(value) => match scalar_type {
                Some(ScalarType::I8) => self.set(partial, value as i8),
                Some(ScalarType::I16) => self.set(partial, value as i16),
                Some(ScalarType::I32) => self.set(partial, value as i32),
                Some(ScalarType::I64) => self.set(partial, value),
                Some(ScalarType::I128) => self.set(partial, value as i128),
                Some(ScalarType::ISize) => self.set(partial, value as isize),
                Some(ScalarType::U8) => self.set(partial, value as u8),
                Some(ScalarType::U16) => self.set(partial, value as u16),
                Some(ScalarType::U32) => self.set(partial, value as u32),
                Some(ScalarType::U64) => self.set(partial, value as u64),
                Some(ScalarType::U128) => self.set(partial, value as u128),
                Some(ScalarType::USize) => self.set(partial, value as usize),
                Some(ScalarType::F32) => self.set(partial, value as f32),
                Some(ScalarType::F64) => self.set(partial, value as f64),
                Some(ScalarType::String) => self.set_string(partial, Cow::Owned(value.to_string())),
                _ => self.set(partial, value),
            },
            ScalarValue::U64(value) => match scalar_type {
                Some(ScalarType::U8) => self.set(partial, value as u8),
                Some(ScalarType::U16) => self.set(partial, value as u16),
                Some(ScalarType::U32) => self.set(partial, value as u32),
                Some(ScalarType::U64) => self.set(partial, value),
                Some(ScalarType::U128) => self.set(partial, value as u128),
                Some(ScalarType::USize) => self.set(partial, value as usize),
                Some(ScalarType::I8) => self.set(partial, value as i8),
                Some(ScalarType::I16) => self.set(partial, value as i16),
                Some(ScalarType::I32) => self.set(partial, value as i32),
                Some(ScalarType::I64) => self.set(partial, value as i64),
                Some(ScalarType::I128) => self.set(partial, value as i128),
                Some(ScalarType::ISize) => self.set(partial, value as isize),
                Some(ScalarType::F32) => self.set(partial, value as f32),
                Some(ScalarType::F64) => self.set(partial, value as f64),
                Some(ScalarType::String) => self.set_string(partial, Cow::Owned(value.to_string())),
                _ => self.set(partial, value),
            },
            ScalarValue::U128(value) => match scalar_type {
                Some(ScalarType::U128) => self.set(partial, value),
                Some(ScalarType::I128) => self.set(partial, value as i128),
                _ => self.set(partial, value as u64),
            },
            ScalarValue::I128(value) => match scalar_type {
                Some(ScalarType::I128) => self.set(partial, value),
                Some(ScalarType::U128) => self.set(partial, value as u128),
                _ => self.set(partial, value as i64),
            },
            ScalarValue::F64(value) => match scalar_type {
                Some(ScalarType::F32) => self.set(partial, value as f32),
                Some(ScalarType::F64) => self.set(partial, value),
                _ if policy.from_str => {
                    partial = self.parse_from_str(partial, &value.to_string())?;
                    Ok(partial)
                }
                _ => self.set(partial, value),
            },
            ScalarValue::Str(value) => self.set_string(partial, value),
            ScalarValue::Bytes(value) => self.set_bytes(partial, value),
            _ => Err(self.unsupported("VM unknown scalar value")),
        }
    }

    #[inline]
    fn check_enter_shape(
        &self,
        partial: &Partial<'input, false>,
        expected: &'static Shape,
    ) -> Result<(), DeserializeError> {
        #[cfg(debug_assertions)]
        {
            if partial.shape() != expected {
                return Err(DeserializeErrorKind::TypeMismatch {
                    expected,
                    got: Cow::Owned(format!("VM partial shape {}", partial.shape())),
                }
                .with_span(self.last_span));
            }
        }
        #[cfg(not(debug_assertions))]
        {
            let _ = partial;
            let _ = expected;
        }
        Ok(())
    }

    fn next_value_start(
        &mut self,
        expected: &'static str,
    ) -> Result<JsonValueStart<'input>, DeserializeError> {
        if let Some(value) = self.pending_value.take() {
            self.last_span = value.span();
            return Ok(value);
        }

        match self.parser.next_value_start()? {
            Some(value) => {
                self.last_span = value.span();
                Ok(value)
            }
            None => Err(DeserializeErrorKind::UnexpectedEof { expected }.with_span(self.last_span)),
        }
    }

    fn next_scalar(
        &mut self,
        expected: &'static str,
    ) -> Result<ScalarValue<'input>, DeserializeError> {
        match self.next_value_start(expected)? {
            JsonValueStart::Scalar(scalar, _) => Ok(scalar),
            value => Err(self.unexpected_value(&value, expected)),
        }
    }

    fn expect_object_start(&mut self, expected: &'static str) -> Result<(), DeserializeError> {
        match self.next_value_start(expected)? {
            JsonValueStart::Object(_) => Ok(()),
            value => Err(self.unexpected_value(&value, expected)),
        }
    }

    fn expect_array_start(&mut self, expected: &'static str) -> Result<(), DeserializeError> {
        match self.next_value_start(expected)? {
            JsonValueStart::Array(_) => Ok(()),
            value => Err(self.unexpected_value(&value, expected)),
        }
    }

    fn set<T: Facet<'input>>(
        &self,
        partial: Partial<'input, false>,
        value: T,
    ) -> Result<Partial<'input, false>, DeserializeError> {
        partial.set(value).map_err(|e| self.reflect_error(e))
    }

    fn set_default(
        &self,
        partial: Partial<'input, false>,
    ) -> Result<Partial<'input, false>, DeserializeError> {
        partial.set_default().map_err(|e| self.reflect_error(e))
    }

    fn set_string(
        &self,
        partial: Partial<'input, false>,
        value: Cow<'input, str>,
    ) -> Result<Partial<'input, false>, DeserializeError> {
        facet_dessert::set_string_value(partial, value, Some(self.last_span))
            .map_err(|e| self.dessert_error(e))
    }

    fn set_bytes(
        &self,
        partial: Partial<'input, false>,
        value: Cow<'input, [u8]>,
    ) -> Result<Partial<'input, false>, DeserializeError> {
        facet_dessert::set_bytes_value(partial, value, Some(self.last_span))
            .map_err(|e| self.dessert_error(e))
    }

    fn begin_nth_field(
        &self,
        partial: Partial<'input, false>,
        idx: usize,
    ) -> Result<Partial<'input, false>, DeserializeError> {
        partial
            .begin_nth_field(idx)
            .map_err(|e| self.reflect_error(e))
    }

    fn begin_some(
        &self,
        partial: Partial<'input, false>,
    ) -> Result<Partial<'input, false>, DeserializeError> {
        partial.begin_some().map_err(|e| self.reflect_error(e))
    }

    fn begin_inner(
        &self,
        partial: Partial<'input, false>,
    ) -> Result<Partial<'input, false>, DeserializeError> {
        partial.begin_inner().map_err(|e| self.reflect_error(e))
    }

    fn init_list(
        &self,
        partial: Partial<'input, false>,
    ) -> Result<Partial<'input, false>, DeserializeError> {
        partial.init_list().map_err(|e| self.reflect_error(e))
    }

    fn begin_list_item(
        &self,
        partial: Partial<'input, false>,
    ) -> Result<Partial<'input, false>, DeserializeError> {
        partial.begin_list_item().map_err(|e| self.reflect_error(e))
    }

    fn end(
        &self,
        partial: Partial<'input, false>,
    ) -> Result<Partial<'input, false>, DeserializeError> {
        partial.end().map_err(|e| self.reflect_error(e))
    }

    fn parse_from_str(
        &self,
        partial: Partial<'input, false>,
        value: &str,
    ) -> Result<Partial<'input, false>, DeserializeError> {
        partial
            .parse_from_str(value)
            .map_err(|e| self.reflect_error(e))
    }

    fn reflect_error(&self, error: ReflectError) -> DeserializeError {
        let kind = match error.kind {
            ReflectErrorKind::UninitializedField { shape, field_name } => {
                DeserializeErrorKind::MissingField {
                    field: field_name,
                    container_shape: shape,
                }
            }
            other => DeserializeErrorKind::Reflect {
                kind: other,
                context: "json vm",
            },
        };
        DeserializeError {
            span: Some(self.last_span),
            path: Some(error.path),
            kind,
        }
    }

    fn dessert_error(&self, error: facet_dessert::DessertError) -> DeserializeError {
        match error {
            facet_dessert::DessertError::Reflect { error, span } => {
                let mut error = self.reflect_error(error);
                error.span = span.or(error.span);
                error
            }
            facet_dessert::DessertError::CannotBorrow { message } => DeserializeError {
                span: Some(self.last_span),
                path: None,
                kind: DeserializeErrorKind::CannotBorrow { reason: message },
            },
            _ => self.unsupported("VM unknown dessert error"),
        }
    }

    fn unexpected_value(
        &self,
        value: &JsonValueStart<'_>,
        expected: &'static str,
    ) -> DeserializeError {
        let got = match value {
            JsonValueStart::Object(_) => "object",
            JsonValueStart::Array(_) => "array",
            JsonValueStart::Scalar(scalar, _) => scalar.kind_name(),
        };
        DeserializeErrorKind::UnexpectedToken {
            got: Cow::Borrowed(got),
            expected,
        }
        .with_span(value.span())
    }

    fn unsupported(&self, message: &'static str) -> DeserializeError {
        self.unsupported_owned(message.to_string())
    }

    fn unsupported_owned(&self, message: alloc::string::String) -> DeserializeError {
        DeserializeErrorKind::Unsupported {
            message: Cow::Owned(message),
        }
        .with_span(self.last_span)
    }

    fn unsupported_enum(&self, en: &JsonEnum) -> DeserializeError {
        let repr = match en.repr {
            JsonEnumRepr::ExternallyTagged => "externally tagged enum",
            JsonEnumRepr::InternallyTagged { .. } => "internally tagged enum",
            JsonEnumRepr::AdjacentlyTagged { .. } => "adjacently tagged enum",
            JsonEnumRepr::Flattened => "flattened enum",
        };
        self.unsupported_owned(format!("VM {repr} deserialization"))
    }
}

#[derive(Debug)]
enum VmFrame<'program> {
    Program {
        program: &'program JsonProgram,
        pc: usize,
    },
    EndCurrent,
    Struct {
        plan: &'program JsonStruct,
        seen: SeenFields,
    },
    List {
        element: &'program JsonProgram,
    },
}

type ProgramContinuation<'program> = Option<(&'program JsonProgram, usize)>;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ProgramControl {
    Continue,
    Suspend,
    Stop,
}

fn push_continuation<'program>(
    frames: &mut Vec<VmFrame<'program>>,
    continuation: ProgramContinuation<'program>,
) {
    if let Some((program, pc)) = continuation {
        frames.push(VmFrame::Program { program, pc });
    }
}

fn can_inline_field_program(program: &JsonProgram) -> bool {
    program.iter().all(|op| {
        matches!(
            op,
            JsonOp::EnterShape { .. } | JsonOp::Null | JsonOp::Scalar(_) | JsonOp::String(_)
        )
    })
}

fn find_field<'program>(
    plan: &'program JsonStruct,
    name: &str,
) -> Option<(usize, &'program JsonField)> {
    let idx = plan.lookup.find(name)?;
    let field = &plan.fields[idx];
    Some((idx, field))
}

#[derive(Debug)]
enum SeenFields {
    Bits(u64),
    Many(Vec<bool>),
}

impl SeenFields {
    fn new(field_count: usize) -> Self {
        if field_count <= u64::BITS as usize {
            Self::Bits(0)
        } else {
            Self::Many(vec![false; field_count])
        }
    }

    fn mark(&mut self, idx: usize) -> bool {
        match self {
            Self::Bits(bits) => {
                let bit = 1_u64 << idx;
                let was_seen = *bits & bit != 0;
                *bits |= bit;
                was_seen
            }
            Self::Many(seen) => {
                let was_seen = seen[idx];
                seen[idx] = true;
                was_seen
            }
        }
    }
}

fn lower_error(error: LowerError) -> DeserializeError {
    DeserializeError {
        span: None,
        path: None,
        kind: DeserializeErrorKind::Unsupported {
            message: Cow::Owned(format!("json VM lowering failed: {error:?}")),
        },
    }
}
