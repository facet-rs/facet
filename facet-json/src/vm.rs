extern crate alloc;

use alloc::{borrow::Cow, format, string::ToString, vec, vec::Vec};
use core::{marker::PhantomData, mem};
use std::{
    collections::HashMap,
    sync::{Arc, Mutex, OnceLock},
};

use facet_core::{Facet, ScalarType, Shape};
use facet_format::{
    ContainerKind, DeserializeError, DeserializeErrorKind, FormatParser, ParseEvent,
    ParseEventKind, ScalarValue, SpanGuard,
};
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
            last_span: Span::new(0, 0),
            stats: JsonVmStats::default(),
        }
    }

    fn deserialize_into(
        &mut self,
        mut partial: Partial<'input, false>,
    ) -> Result<Partial<'input, false>, DeserializeError> {
        let mut frames = vec![VmFrame::Program {
            program: &self.lowered.program,
            pc: 0,
        }];

        while let Some(frame) = frames.pop() {
            if COLLECT_STATS {
                self.stats.record(frames.len() + 1, partial.frame_count());
            }
            match frame {
                VmFrame::Program { program, pc } => {
                    if pc >= program.len() {
                        continue;
                    }
                    frames.push(VmFrame::Program {
                        program,
                        pc: pc + 1,
                    });
                    partial = self.execute_op(partial, &mut frames, &program[pc])?;
                }
                VmFrame::EndCurrent => {
                    partial = self.end(partial)?;
                }
                VmFrame::Struct { plan, seen } => {
                    partial = self.step_struct(partial, &mut frames, plan, seen)?;
                }
                VmFrame::List { element } => {
                    partial = self.step_list(partial, &mut frames, element)?;
                }
            }
        }

        Ok(partial)
    }

    fn execute_op(
        &mut self,
        mut partial: Partial<'input, false>,
        frames: &mut Vec<VmFrame<'program>>,
        op: &'program JsonOp,
    ) -> Result<Partial<'input, false>, DeserializeError> {
        match op {
            JsonOp::EnterShape { shape } => {
                if partial.shape() != *shape {
                    return Err(DeserializeErrorKind::TypeMismatch {
                        expected: shape,
                        got: Cow::Owned(format!("VM partial shape {}", partial.shape())),
                    }
                    .with_span(self.last_span));
                }
            }
            JsonOp::Null => {
                let event = self.next_event("null")?;
                let ParseEventKind::Scalar(ScalarValue::Null) = event.kind else {
                    return Err(self.unexpected_event(&event, "null"));
                };
                partial = self.set_default(partial)?;
            }
            JsonOp::Scalar(policy) => {
                let event = self.next_event("scalar")?;
                let ParseEventKind::Scalar(scalar) = event.kind else {
                    return Err(self.unexpected_event(&event, "scalar"));
                };
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
                let event = self.next_event("object")?;
                match event.kind {
                    ParseEventKind::StructStart(ContainerKind::Object) => {
                        frames.push(VmFrame::Struct {
                            plan,
                            seen: SeenFields::new(plan.fields.len()),
                        });
                    }
                    _ => return Err(self.unexpected_event(&event, "object")),
                }
            }
            JsonOp::Tuple { .. } => return Err(self.unsupported("VM tuple deserialization")),
            JsonOp::List {
                element,
                byte_optimized,
            } => {
                if *byte_optimized {
                    return Err(self.unsupported("VM byte-optimized list deserialization"));
                }
                let event = self.next_event("array")?;
                match event.kind {
                    ParseEventKind::SequenceStart(ContainerKind::Array) => {
                        partial = self.init_list(partial)?;
                        frames.push(VmFrame::List { element });
                    }
                    _ => return Err(self.unexpected_event(&event, "array")),
                }
            }
            JsonOp::Array { .. } => return Err(self.unsupported("VM array deserialization")),
            JsonOp::Map { .. } => return Err(self.unsupported("VM map deserialization")),
            JsonOp::Set { .. } => return Err(self.unsupported("VM set deserialization")),
            JsonOp::Option { some } => {
                let event = self.peek_event("option value")?;
                match event.kind {
                    ParseEventKind::Scalar(ScalarValue::Null) => {
                        let _ = self.next_event("null")?;
                        partial = self.set_default(partial)?;
                    }
                    _ => {
                        partial = self.begin_some(partial)?;
                        frames.push(VmFrame::EndCurrent);
                        frames.push(VmFrame::Program {
                            program: some,
                            pc: 0,
                        });
                    }
                }
            }
            JsonOp::Result { .. } => return Err(self.unsupported("VM result deserialization")),
            JsonOp::Enum(en) => return Err(self.unsupported_enum(en)),
            JsonOp::Pointer { pointee } => {
                partial = self.begin_pointer(partial, frames, pointee)?;
            }
            JsonOp::Transparent { inner } => {
                partial = self.begin_inner(partial)?;
                frames.push(VmFrame::EndCurrent);
                frames.push(VmFrame::Program {
                    program: inner,
                    pc: 0,
                });
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
                frames.push(VmFrame::Program { program, pc: 0 });
            }
            JsonOp::Return => {}
        }

        Ok(partial)
    }

    fn step_struct(
        &mut self,
        mut partial: Partial<'input, false>,
        frames: &mut Vec<VmFrame<'program>>,
        plan: &'program JsonStruct,
        mut seen: SeenFields,
    ) -> Result<Partial<'input, false>, DeserializeError> {
        let event = self.next_event("field key or object end")?;
        match event.kind {
            ParseEventKind::StructEnd => Ok(partial),
            ParseEventKind::FieldKey(ref key) => {
                let Some(name) = key.name() else {
                    return Err(self.unexpected_event(&event, "named field key"));
                };
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
                    .with_span(event.span));
                }
                partial = self.begin_nth_field(partial, idx)?;
                frames.push(VmFrame::Struct { plan, seen });
                frames.push(VmFrame::EndCurrent);
                frames.push(VmFrame::Program {
                    program: &field.value,
                    pc: 0,
                });
                Ok(partial)
            }
            _ => Err(self.unexpected_event(&event, "field key or object end")),
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
        let event = self.peek_event("array element or array end")?;
        match event.kind {
            ParseEventKind::SequenceEnd => {
                let _ = self.next_event("array end")?;
                Ok(partial)
            }
            _ => {
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
        let event = self.next_event("string")?;
        let ParseEventKind::Scalar(ScalarValue::Str(s)) = event.kind else {
            return Err(self.unexpected_event(&event, "string"));
        };
        self.set_string(partial, s)
    }

    fn set_scalar(
        &mut self,
        mut partial: Partial<'input, false>,
        scalar: ScalarValue<'input>,
        _policy: JsonScalar,
    ) -> Result<Partial<'input, false>, DeserializeError> {
        let scalar_type = partial.shape().scalar_type();
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
                _ if partial.shape().vtable.has_parse() => {
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

    fn next_event(
        &mut self,
        expected: &'static str,
    ) -> Result<ParseEvent<'input>, DeserializeError> {
        match self.parser.next_event()? {
            Some(event) => {
                self.last_span = event.span;
                Ok(event)
            }
            None => Err(DeserializeErrorKind::UnexpectedEof { expected }.with_span(self.last_span)),
        }
    }

    fn peek_event(
        &mut self,
        expected: &'static str,
    ) -> Result<ParseEvent<'input>, DeserializeError> {
        match self.parser.peek_event()? {
            Some(event) => {
                self.last_span = event.span;
                Ok(event)
            }
            None => Err(DeserializeErrorKind::UnexpectedEof { expected }.with_span(self.last_span)),
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

    fn unexpected_event(&self, event: &ParseEvent<'_>, expected: &'static str) -> DeserializeError {
        DeserializeErrorKind::UnexpectedToken {
            got: Cow::Borrowed(event.kind.kind_name()),
            expected,
        }
        .with_span(event.span)
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
