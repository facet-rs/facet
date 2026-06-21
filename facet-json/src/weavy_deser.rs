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
use core::mem::MaybeUninit;

use facet_core::{
    Def, DefaultInPlaceFn, DefaultSource, Facet, Field, ListDef, OptionDef, PointerDef, PtrMut,
    PtrUninit, ScalarType, Shape, StructKind, Type, UserType,
};
use facet_format::{
    ContainerKind, DeserializeError, DeserializeErrorKind, FieldKey, FormatParser, ParseEvent,
    ParseEventKind, ScalarValue,
};
use facet_reflect::Span;
use weavy::{Control, Lowered, Program, RunError, RunStats, Step};

use crate::JsonParser;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum JsonBlockId {
    Shape(&'static Shape),
    StructLoop(&'static Shape),
    ListLoop(&'static Shape),
}

type BlockId = JsonBlockId;

/// Deserialize a value from a JSON string through the opt-in Weavy runner.
pub fn from_str_weavy<T>(input: &str) -> Result<T, DeserializeError>
where
    T: Facet<'static>,
{
    let (value, _) = from_str_weavy_with_stats(input)?;
    Ok(value)
}

/// Deserialize a value from JSON bytes through the opt-in Weavy runner.
pub fn from_slice_weavy<T>(input: &[u8]) -> Result<T, DeserializeError>
where
    T: Facet<'static>,
{
    let (value, _) = from_slice_weavy_with_stats(input)?;
    Ok(value)
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

/// Reusable opt-in Weavy JSON deserialization plan for `T`.
///
/// The default `facet_json::from_str` path is unchanged. This type is for the
/// new VM backend and lets callers separate typed-shape lowering from repeated
/// input decoding.
pub struct JsonWeavyPlan<T> {
    lowered: Lowered<BlockId, JsonOp>,
    _marker: PhantomData<fn() -> T>,
}

impl<T> JsonWeavyPlan<T>
where
    T: Facet<'static>,
{
    /// Lower `T::SHAPE` into the JSON-specific Weavy bytecode.
    pub fn build() -> Result<Self, DeserializeError> {
        Ok(Self {
            lowered: Lowering::new().lower(T::SHAPE)?,
            _marker: PhantomData,
        })
    }

    /// Deserialize from a JSON string using this pre-lowered plan.
    pub fn from_str(&self, input: &str) -> Result<T, DeserializeError> {
        let (value, _) = self.from_str_with_stats(input)?;
        Ok(value)
    }

    /// Deserialize from JSON bytes using this pre-lowered plan.
    pub fn from_slice(&self, input: &[u8]) -> Result<T, DeserializeError> {
        let (value, _) = self.from_slice_with_stats(input)?;
        Ok(value)
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
    ) -> Result<(T, RunStats), DeserializeError> {
        let mut slot = MaybeUninit::<T>::uninit();
        let root = PtrUninit::from_maybe_uninit(&mut slot);
        let mut interp = JsonInterp::new(parser, root);
        let stats = match weavy::run_with_stats(&self.lowered, &mut interp) {
            Ok(stats) => stats,
            Err(err) => return Err(run_error(err)),
        };
        interp.finish_success();

        Ok((unsafe { slot.assume_init() }, stats))
    }
}

fn run_error(err: RunError<BlockId, DeserializeError>) -> DeserializeError {
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
enum JsonOp {
    CallBlock(BlockId),
    ReadScalar {
        shape: &'static Shape,
        scalar: ScalarType,
    },
    ReadStruct {
        shape: &'static Shape,
        fields: Box<[FieldPlan]>,
        loop_id: BlockId,
    },
    StructNext {
        shape: &'static Shape,
        fields: Box<[FieldPlan]>,
        loop_id: BlockId,
    },
    ReadOption {
        option: OptionDef,
        some_program: Program<JsonOp>,
        inner_layout: Layout,
    },
    ReadList {
        list_shape: &'static Shape,
        list: ListDef,
        loop_id: BlockId,
    },
    ListNext {
        list: ListDef,
        element_program: Program<JsonOp>,
        element_layout: Layout,
        loop_id: BlockId,
    },
    ReadPointer {
        pointer: PointerDef,
        pointee_program: Program<JsonOp>,
        pointee_layout: Layout,
    },
}

#[derive(Clone, Debug)]
struct FieldPlan {
    name: &'static str,
    alias: Option<&'static str>,
    offset: usize,
    shape: &'static Shape,
    program: Program<JsonOp>,
    missing: MissingField,
}

#[derive(Clone, Copy, Debug)]
enum MissingField {
    Required,
    DefaultTrait { explicit: bool },
    DefaultCustom(DefaultInPlaceFn),
    OptionNone(OptionDef),
}

struct Lowering {
    lowered: Lowered<BlockId, JsonOp>,
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

    fn lower(mut self, root: &'static Shape) -> Result<Lowered<BlockId, JsonOp>, DeserializeError> {
        self.lower_shape(root)?;
        self.lowered.program = vec![JsonOp::CallBlock(JsonBlockId::Shape(root))];
        Ok(self.lowered)
    }

    fn lower_shape(&mut self, shape: &'static Shape) -> Result<Program<JsonOp>, DeserializeError> {
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
    ) -> Result<Program<JsonOp>, DeserializeError> {
        if let Some(scalar) = ScalarType::try_from_shape(shape) {
            return Ok(vec![JsonOp::ReadScalar { shape, scalar }]);
        }

        match shape.def {
            Def::Option(option) => {
                let inner_layout = sized_layout(option.t())?;
                let some_program = self.lower_shape(option.t())?;
                Ok(vec![JsonOp::ReadOption {
                    option,
                    some_program,
                    inner_layout,
                }])
            }
            Def::List(list) => {
                if list.init_in_place_with_capacity().is_none() || list.push().is_none() {
                    return Err(unsupported(shape, "list initialization and push"));
                }
                let element_layout = sized_layout(list.t())?;
                let element_program = self.lower_shape(list.t())?;
                let loop_id = JsonBlockId::ListLoop(shape);
                let loop_program = vec![JsonOp::ListNext {
                    list,
                    element_program: element_program.clone(),
                    element_layout,
                    loop_id,
                }];
                self.lowered.blocks.insert(loop_id, loop_program);
                Ok(vec![JsonOp::ReadList {
                    list_shape: shape,
                    list,
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
                Type::User(UserType::Struct(struct_type)) => {
                    if struct_type.kind != StructKind::Struct {
                        return Err(unsupported(shape, "non-named struct"));
                    }
                    if shape.proxy.is_some() || !shape.format_proxies.is_empty() {
                        return Err(unsupported(shape, "proxy"));
                    }

                    let container_has_default = shape.has_default_attr();
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
                            missing: missing_field_action(field, container_has_default),
                        });
                    }
                    let fields = fields.into_boxed_slice();
                    let loop_id = JsonBlockId::StructLoop(shape);
                    let loop_program = vec![JsonOp::StructNext {
                        shape,
                        fields: fields.clone(),
                        loop_id,
                    }];
                    self.lowered.blocks.insert(loop_id, loop_program);
                    Ok(vec![JsonOp::ReadStruct {
                        shape,
                        fields,
                        loop_id,
                    }])
                }
                _ => Err(unsupported(shape, "shape")),
            },
        }
    }
}

fn missing_field_action(field: &Field, container_has_default: bool) -> MissingField {
    match field.default {
        Some(DefaultSource::Custom(default)) => MissingField::DefaultCustom(default),
        Some(DefaultSource::FromTrait) => MissingField::DefaultTrait { explicit: true },
        Some(_) => MissingField::DefaultTrait { explicit: true },
        None if container_has_default => MissingField::DefaultTrait { explicit: false },
        None => match field.shape().def {
            Def::Option(option) => MissingField::OptionNone(option),
            Def::List(_) | Def::Map(_) | Def::Set(_) => {
                MissingField::DefaultTrait { explicit: false }
            }
            _ if field.shape().is_type::<()>() => MissingField::DefaultTrait { explicit: false },
            _ => MissingField::Required,
        },
    }
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

struct JsonInterp<'parser, 'de, 'program, const TRUSTED_UTF8: bool> {
    parser: &'parser mut JsonParser<'de, TRUSTED_UTF8>,
    base: PtrUninit,
    structs: Vec<StructFrame<'program>>,
    lists: Vec<ListFrame>,
    success: bool,
}

impl<'parser, 'de, 'program, const TRUSTED_UTF8: bool>
    JsonInterp<'parser, 'de, 'program, TRUSTED_UTF8>
{
    fn new(parser: &'parser mut JsonParser<'de, TRUSTED_UTF8>, base: PtrUninit) -> Self {
        Self {
            parser,
            base,
            structs: Vec::new(),
            lists: Vec::new(),
            success: false,
        }
    }

    fn finish_success(&mut self) {
        self.success = true;
    }

    fn next_event(&mut self, expected: &'static str) -> Result<ParseEvent<'de>, DeserializeError> {
        self.parser
            .next_event()?
            .ok_or_else(|| vm_error(None, DeserializeErrorKind::UnexpectedEof { expected }))
    }

    fn peek_event(&mut self) -> Result<Option<ParseEvent<'de>>, DeserializeError> {
        Ok(self.parser.peek_event()?)
    }
}

impl<const TRUSTED_UTF8: bool> Drop for JsonInterp<'_, '_, '_, TRUSTED_UTF8> {
    fn drop(&mut self) {
        if self.success {
            return;
        }

        while let Some(list) = self.lists.pop() {
            unsafe {
                let _ = list.shape.call_drop_in_place(list.ptr);
            }
        }

        while self.structs.pop().is_some() {}
    }
}

impl<'program, 'parser, 'de, const TRUSTED_UTF8: bool> Step<'program, BlockId, JsonOp>
    for JsonInterp<'parser, 'de, 'program, TRUSTED_UTF8>
{
    type Error = DeserializeError;
    type Continuation = Continuation;

    fn step(
        &mut self,
        op: &'program JsonOp,
    ) -> Result<Control<'program, BlockId, JsonOp, Self::Continuation>, Self::Error> {
        match op {
            JsonOp::CallBlock(shape) => Ok(Control::CallBlock(*shape)),
            JsonOp::ReadScalar { shape, scalar } => {
                let event = self.next_event("scalar")?;
                match event.kind {
                    ParseEventKind::Scalar(value) => unsafe {
                        write_scalar(shape, *scalar, self.base, value, event.span)?;
                    },
                    other => {
                        return Err(vm_error(
                            Some(event.span),
                            DeserializeErrorKind::UnexpectedToken {
                                got: other.kind_name().into(),
                                expected: "scalar",
                            },
                        ));
                    }
                }
                Ok(Control::Continue)
            }
            JsonOp::ReadStruct {
                shape,
                fields,
                loop_id,
            } => {
                let event = self.next_event("object")?;
                if !matches!(
                    event.kind,
                    ParseEventKind::StructStart(ContainerKind::Object)
                ) {
                    return Err(vm_error(
                        Some(event.span),
                        DeserializeErrorKind::UnexpectedToken {
                            got: event.kind.kind_name().into(),
                            expected: "object",
                        },
                    ));
                }

                self.structs
                    .push(StructFrame::new(shape, self.base, fields));
                Ok(Control::CallBlockThen(*loop_id, Continuation::FinishStruct))
            }
            JsonOp::StructNext {
                shape,
                fields,
                loop_id,
            } => {
                let event = self.next_event("field key or object end")?;
                match event.kind {
                    ParseEventKind::StructEnd => Ok(Control::Continue),
                    ParseEventKind::FieldKey(key) => {
                        let Some(field_name) = field_key_name(&key) else {
                            self.parser.skip_value()?;
                            return Ok(Control::CallBlock(*loop_id));
                        };

                        let Some((index, field)) = fields.iter().enumerate().find(|(_, field)| {
                            field.name == field_name || field.alias == Some(field_name)
                        }) else {
                            if shape.has_deny_unknown_fields_attr() {
                                return Err(vm_error(
                                    Some(event.span),
                                    DeserializeErrorKind::UnknownField {
                                        field: field_name.to_owned().into(),
                                        suggestion: None,
                                    },
                                ));
                            }
                            self.parser.skip_value()?;
                            return Ok(Control::CallBlock(*loop_id));
                        };

                        let frame = self
                            .structs
                            .last()
                            .expect("struct frame is present while decoding fields");
                        if let Some(first_span) = frame.seen[index] {
                            return Err(vm_error(
                                Some(event.span),
                                DeserializeErrorKind::DuplicateField {
                                    field: field.name.into(),
                                    first_span: Some(first_span),
                                },
                            ));
                        }

                        let old_base = self.base;
                        self.base = unsafe { frame.base.field_uninit(field.offset) };
                        Ok(Control::CallProgramThen(
                            &field.program,
                            Continuation::FieldDone {
                                index,
                                span: event.span,
                                old_base,
                                loop_id: *loop_id,
                            },
                        ))
                    }
                    other => Err(vm_error(
                        Some(event.span),
                        DeserializeErrorKind::UnexpectedToken {
                            got: other.kind_name().into(),
                            expected: "field key or object end",
                        },
                    )),
                }
            }
            JsonOp::ReadOption {
                option,
                some_program,
                inner_layout,
            } => {
                match self.peek_event()? {
                    Some(event)
                        if matches!(event.kind, ParseEventKind::Scalar(ScalarValue::Null)) =>
                    {
                        let _ = self.next_event("null")?;
                        unsafe {
                            (option.vtable.init_none)(self.base);
                        }
                        return Ok(Control::Continue);
                    }
                    _ => {}
                }

                let scratch = Scratch::alloc(option.t(), *inner_layout)?;
                let old_base = self.base;
                self.base = scratch.ptr;
                Ok(Control::CallProgramThen(
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
                loop_id,
            } => {
                let event = self.next_event("array")?;
                if !matches!(
                    event.kind,
                    ParseEventKind::SequenceStart(ContainerKind::Array)
                ) {
                    return Err(vm_error(
                        Some(event.span),
                        DeserializeErrorKind::UnexpectedToken {
                            got: event.kind.kind_name().into(),
                            expected: "array",
                        },
                    ));
                }
                let init = list
                    .init_in_place_with_capacity()
                    .ok_or_else(|| unsupported(list_shape, "list initialization"))?;
                let list_ptr = unsafe { init(self.base, 0) };
                self.lists.push(ListFrame {
                    shape: list_shape,
                    ptr: list_ptr,
                });
                Ok(Control::CallBlockThen(*loop_id, Continuation::FinishList))
            }
            JsonOp::ListNext {
                list,
                element_program,
                element_layout,
                loop_id,
            } => {
                match self.peek_event()? {
                    Some(event) if matches!(event.kind, ParseEventKind::SequenceEnd) => {
                        let _ = self.next_event("array end")?;
                        return Ok(Control::Continue);
                    }
                    _ => {}
                }

                let scratch = Scratch::alloc(list.t(), *element_layout)?;
                let old_base = self.base;
                self.base = scratch.ptr;
                Ok(Control::CallProgramThen(
                    element_program,
                    Continuation::ListElement {
                        list: *list,
                        old_base,
                        scratch,
                        loop_id: *loop_id,
                    },
                ))
            }
            JsonOp::ReadPointer {
                pointer,
                pointee_program,
                pointee_layout,
            } => {
                let pointee = pointer
                    .pointee()
                    .ok_or_else(|| unsupported_shape_message("opaque pointer"))?;
                let scratch = Scratch::alloc(pointee, *pointee_layout)?;
                let old_base = self.base;
                self.base = scratch.ptr;
                Ok(Control::CallProgramThen(
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
    ) -> Result<Control<'program, BlockId, JsonOp, Self::Continuation>, Self::Error> {
        match continuation {
            Continuation::FinishStruct => {
                let frame = self
                    .structs
                    .pop()
                    .expect("struct frame is present after struct program");
                unsafe {
                    frame.fill_missing_fields()?;
                }
                Ok(Control::Continue)
            }
            Continuation::FieldDone {
                index,
                span,
                old_base,
                loop_id,
            } => {
                let frame = self
                    .structs
                    .last_mut()
                    .expect("struct frame is present after field program");
                frame.seen[index] = Some(span);
                self.base = old_base;
                Ok(Control::CallBlock(loop_id))
            }
            Continuation::OptionSome {
                option,
                option_ptr,
                old_base,
                mut scratch,
            } => {
                unsafe {
                    (option.vtable.init_some)(option_ptr, scratch.ptr_mut());
                }
                scratch.dealloc_uninit();
                self.base = old_base;
                Ok(Control::Continue)
            }
            Continuation::FinishList => {
                self.lists.pop();
                Ok(Control::Continue)
            }
            Continuation::ListElement {
                list,
                old_base,
                mut scratch,
                loop_id,
            } => {
                let list_ptr = self
                    .lists
                    .last()
                    .expect("list frame is present after element program")
                    .ptr;
                let push = list
                    .push()
                    .ok_or_else(|| unsupported(list.t(), "list push"))?;
                unsafe {
                    push(list_ptr, scratch.ptr_mut());
                }
                scratch.dealloc_uninit();
                self.base = old_base;
                Ok(Control::CallBlock(loop_id))
            }
            Continuation::Pointer {
                pointer,
                pointer_ptr,
                old_base,
                mut scratch,
            } => {
                let new_into = pointer
                    .vtable
                    .new_into_fn
                    .ok_or_else(|| unsupported_shape_message("pointer without new_into"))?;
                unsafe {
                    new_into(pointer_ptr, scratch.ptr_mut());
                }
                scratch.dealloc_uninit();
                self.base = old_base;
                Ok(Control::Continue)
            }
        }
    }
}

enum Continuation {
    FinishStruct,
    FieldDone {
        index: usize,
        span: Span,
        old_base: PtrUninit,
        loop_id: BlockId,
    },
    OptionSome {
        option: OptionDef,
        option_ptr: PtrUninit,
        old_base: PtrUninit,
        scratch: Scratch,
    },
    FinishList,
    ListElement {
        list: ListDef,
        old_base: PtrUninit,
        scratch: Scratch,
        loop_id: BlockId,
    },
    Pointer {
        pointer: PointerDef,
        pointer_ptr: PtrUninit,
        old_base: PtrUninit,
        scratch: Scratch,
    },
}

struct StructFrame<'program> {
    shape: &'static Shape,
    base: PtrUninit,
    fields: &'program [FieldPlan],
    seen: Vec<Option<Span>>,
}

impl<'program> StructFrame<'program> {
    fn new(shape: &'static Shape, base: PtrUninit, fields: &'program [FieldPlan]) -> Self {
        Self {
            shape,
            base,
            fields,
            seen: vec![None; fields.len()],
        }
    }

    unsafe fn fill_missing_fields(mut self) -> Result<(), DeserializeError> {
        for (index, field) in self.fields.iter().enumerate() {
            if self.seen[index].is_some() {
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
                    self.seen[index] = Some(Span { offset: 0, len: 0 });
                }
                MissingField::DefaultTrait { explicit } => {
                    if unsafe { field.shape.call_default_in_place(field_ptr) }.is_some() {
                        self.seen[index] = Some(Span { offset: 0, len: 0 });
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
                    self.seen[index] = Some(Span { offset: 0, len: 0 });
                }
            }
        }
        core::mem::forget(self);
        Ok(())
    }

    unsafe fn drop_initialized_fields(&self) {
        for (index, field) in self.fields.iter().enumerate().rev() {
            if self.seen[index].is_none() {
                continue;
            }
            let ptr = unsafe { self.base.field_init(field.offset) };
            unsafe {
                let _ = field.shape.call_drop_in_place(ptr);
            }
        }
    }
}

impl Drop for StructFrame<'_> {
    fn drop(&mut self) {
        unsafe {
            self.drop_initialized_fields();
        }
    }
}

#[derive(Clone, Copy)]
struct ListFrame {
    shape: &'static Shape,
    ptr: PtrMut,
}

struct Scratch {
    shape: &'static Shape,
    ptr: PtrUninit,
    active: bool,
}

impl Scratch {
    fn alloc(shape: &'static Shape, layout: Layout) -> Result<Self, DeserializeError> {
        let ptr = if layout.size() == 0 {
            core::ptr::null_mut::<u8>().wrapping_add(layout.align())
        } else {
            let ptr = unsafe { alloc::alloc::alloc(layout) };
            if ptr.is_null() {
                alloc::alloc::handle_alloc_error(layout);
            }
            ptr
        };
        Ok(Self {
            shape,
            ptr: PtrUninit::new(ptr),
            active: true,
        })
    }

    unsafe fn ptr_mut(&self) -> PtrMut {
        unsafe { self.ptr.assume_init() }
    }

    fn dealloc_uninit(&mut self) {
        if !self.active {
            return;
        }
        unsafe {
            let _ = self.shape.deallocate_uninit(self.ptr);
        }
        self.active = false;
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        if self.active {
            unsafe {
                let _ = self.shape.deallocate_uninit(self.ptr);
            }
        }
    }
}

unsafe fn write_scalar(
    shape: &'static Shape,
    scalar: ScalarType,
    dst: PtrUninit,
    value: ScalarValue<'_>,
    span: Span,
) -> Result<(), DeserializeError> {
    match scalar {
        ScalarType::Unit => match value {
            ScalarValue::Null | ScalarValue::Unit => {
                unsafe { dst.put(()) };
            }
            other => return Err(type_mismatch(span, shape, other.kind_name())),
        },
        ScalarType::Bool => match value {
            ScalarValue::Bool(value) => {
                unsafe { dst.put(value) };
            }
            other => return Err(type_mismatch(span, shape, other.kind_name())),
        },
        ScalarType::Char => match value {
            ScalarValue::Char(value) => {
                unsafe { dst.put(value) };
            }
            ScalarValue::Str(value) => {
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
                ScalarValue::Str(value) => value.into_owned(),
                other => return Err(type_mismatch(span, shape, other.kind_name())),
            };
            unsafe { dst.put(string) };
        }
        ScalarType::CowStr => {
            let string = match value {
                ScalarValue::Str(value) => value.into_owned(),
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
            let ScalarValue::Str(value) = value else {
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

fn scalar_to_f64(
    value: ScalarValue<'_>,
    span: Span,
    shape: &'static Shape,
) -> Result<f64, DeserializeError> {
    match value {
        ScalarValue::F64(value) => Ok(value),
        ScalarValue::I64(value) => Ok(value as f64),
        ScalarValue::U64(value) => Ok(value as f64),
        ScalarValue::I128(value) => Ok(value as f64),
        ScalarValue::U128(value) => Ok(value as f64),
        ScalarValue::Str(value) => value
            .parse::<f64>()
            .map_err(|_| type_mismatch(span, shape, "string")),
        other => Err(type_mismatch(span, shape, other.kind_name())),
    }
}

fn into_unsigned<T>(
    value: ScalarValue<'_>,
    span: Span,
    target: &'static str,
) -> Result<T, DeserializeError>
where
    T: TryFrom<u128>,
{
    let value = match value {
        ScalarValue::U64(value) => value as u128,
        ScalarValue::U128(value) => value,
        ScalarValue::I64(value) if value >= 0 => value as u128,
        ScalarValue::I128(value) if value >= 0 => value as u128,
        ScalarValue::Str(value) => value
            .parse::<u128>()
            .map_err(|_| number_out_of_range(span, value.into_owned(), target))?,
        other => return Err(type_mismatch_name(span, target, other.kind_name())),
    };
    T::try_from(value).map_err(|_| number_out_of_range(span, value.to_string(), target))
}

fn into_signed<T>(
    value: ScalarValue<'_>,
    span: Span,
    target: &'static str,
) -> Result<T, DeserializeError>
where
    T: TryFrom<i128>,
{
    let value = match value {
        ScalarValue::I64(value) => value as i128,
        ScalarValue::I128(value) => value,
        ScalarValue::U64(value) => value as i128,
        ScalarValue::U128(value) if value <= i128::MAX as u128 => value as i128,
        ScalarValue::Str(value) => value
            .parse::<i128>()
            .map_err(|_| number_out_of_range(span, value.into_owned(), target))?,
        other => return Err(type_mismatch_name(span, target, other.kind_name())),
    };
    T::try_from(value).map_err(|_| number_out_of_range(span, value.to_string(), target))
}

fn field_key_name<'a>(key: &'a FieldKey<'_>) -> Option<&'a str> {
    key.name().map(|name| name.as_ref())
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
