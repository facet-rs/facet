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
use facet_format::{DeserializeError, DeserializeErrorKind, FormatParser};
use facet_reflect::Span;
use weavy::mem::runtime::{
    HandleGuard, InitializedLedger, RawAllocError, RawArrayBuilder, ScratchSession, ScratchSlot,
};
use weavy::{BlockRef, Control, DenseLowered, Lowered, Program, RunError, RunStats, Step};

use crate::JsonParser;
use crate::parser::{
    JsonFieldKey, JsonObjectStep, JsonScalarInput, JsonScalarToken, JsonSequenceScalarStep,
};
use crate::scanner::{ParsedNumber, SpannedToken, Token as ScanToken};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum JsonBlockId {
    Shape(&'static Shape),
    StructLoop(&'static Shape),
    ListLoop(&'static Shape),
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

/// Reusable opt-in Weavy JSON deserialization plan for `T`.
///
/// The default `facet_json::from_str` path is unchanged. This type is for the
/// new VM backend and lets callers separate typed-shape lowering from repeated
/// input decoding.
pub struct JsonWeavyPlan<T> {
    lowered: DenseLowered<ExecOp>,
    _marker: PhantomData<fn() -> T>,
}

impl<T> JsonWeavyPlan<T>
where
    T: Facet<'static>,
{
    /// Lower `T::SHAPE` into the JSON-specific Weavy bytecode.
    pub fn build() -> Result<Self, DeserializeError> {
        let symbolic = Lowering::new().lower(T::SHAPE)?;
        Ok(Self {
            lowered: resolve_json_lowered(symbolic)?,
            _marker: PhantomData,
        })
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
    ) -> Result<(T, RunStats), DeserializeError> {
        let mut slot = MaybeUninit::<T>::uninit();
        let root = PtrUninit::from_maybe_uninit(&mut slot);
        let mut interp = JsonInterp::new(parser, root);
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
    ) -> Result<T, DeserializeError> {
        let mut slot = MaybeUninit::<T>::uninit();
        let root = PtrUninit::from_maybe_uninit(&mut slot);
        let mut interp = JsonInterp::new(parser, root);
        if let Err(err) = weavy::run_dense(&self.lowered, &mut interp) {
            return Err(run_error(err));
        }
        interp.finish_success();

        Ok(unsafe { slot.assume_init() })
    }
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
        scalar: ScalarType,
    },
    ReadStruct {
        shape: &'static Shape,
        fields: Box<[FieldPlan<Block>]>,
        loop_id: Block,
    },
    StructNext {
        shape: &'static Shape,
        loop_id: Block,
    },
    ReadOption {
        option: OptionDef,
        some_program: Program<JsonOp<Block>>,
        some_scalar: Option<ScalarType>,
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
    scalar: Option<ScalarType>,
    missing: MissingField,
}

#[derive(Clone, Copy, Debug)]
struct ListOptionScalar {
    option: OptionDef,
    scalar: ScalarType,
    inner_layout: Layout,
}

impl<Block> FieldPlan<Block> {
    fn matches_key(&self, key: &JsonFieldKey<'_>) -> bool {
        let key = key.as_str();
        self.name == key || self.alias == Some(key)
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
            return Ok(vec![JsonOp::ReadScalar { shape, scalar }]);
        }

        match shape.def {
            Def::Option(option) => {
                let inner_layout = sized_layout(option.t())?;
                let some_program = self.lower_shape(option.t())?;
                let some_scalar = ScalarType::try_from_shape(option.t());
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
                            scalar: ScalarType::try_from_shape(field_shape),
                            missing: missing_field_action(field, container_has_default),
                        });
                    }
                    let fields = fields.into_boxed_slice();
                    let loop_id = JsonBlockId::StructLoop(shape);
                    let loop_program = vec![JsonOp::StructNext { shape, loop_id }];
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
        JsonOp::ReadStruct {
            shape,
            fields,
            loop_id,
        } => JsonOp::ReadStruct {
            shape,
            fields: resolve_field_plans(fields, refs)?,
            loop_id: resolve_block_ref(loop_id, refs)?,
        },
        JsonOp::StructNext { shape, loop_id } => JsonOp::StructNext {
            shape,
            loop_id: resolve_block_ref(loop_id, refs)?,
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
                message: "raw list buffer layout overflow".into(),
            },
        ),
    }
}

struct JsonInterp<'parser, 'de, 'program, const TRUSTED_UTF8: bool> {
    parser: &'parser mut JsonParser<'de, TRUSTED_UTF8>,
    base: PtrUninit,
    structs: InlineStack<StructFrame<'program>>,
    lists: InlineStack<ListFrame>,
    scratch: ScratchSession,
    success: bool,
}

impl<'parser, 'de, 'program, const TRUSTED_UTF8: bool>
    JsonInterp<'parser, 'de, 'program, TRUSTED_UTF8>
{
    fn new(parser: &'parser mut JsonParser<'de, TRUSTED_UTF8>, base: PtrUninit) -> Self {
        Self {
            parser,
            base,
            structs: InlineStack::new(),
            lists: InlineStack::new(),
            scratch: ScratchSession::new(),
            success: false,
        }
    }

    fn finish_success(&mut self) {
        self.success = true;
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

impl<const TRUSTED_UTF8: bool> Drop for JsonInterp<'_, '_, '_, TRUSTED_UTF8> {
    fn drop(&mut self) {
        if self.success {
            return;
        }

        while self.structs.pop().is_some() {}

        while self.lists.pop().is_some() {}
    }
}

impl<'program, 'parser, 'de, const TRUSTED_UTF8: bool> Step<'program, ExecBlock, ExecOp>
    for JsonInterp<'parser, 'de, 'program, TRUSTED_UTF8>
{
    type Error = DeserializeError;
    type Continuation = Continuation;

    fn step(
        &mut self,
        op: &'program ExecOp,
    ) -> Result<Control<'program, ExecBlock, ExecOp, Self::Continuation>, Self::Error> {
        match op {
            JsonOp::CallBlock(shape) => Ok(Control::CallBlock(*shape)),
            JsonOp::ReadScalar { shape, scalar } => {
                let value = self.parser.read_scalar_input()?;
                unsafe {
                    write_scalar_input(self.parser, shape, *scalar, self.base, value)?;
                }
                Ok(Control::Continue)
            }
            JsonOp::ReadStruct {
                shape,
                fields,
                loop_id,
            } => {
                self.parser.consume_object_start()?;
                self.structs
                    .push(StructFrame::new(shape, self.base, fields));
                Ok(Control::CallBlockThen(*loop_id, Continuation::FinishStruct))
            }
            JsonOp::StructNext { shape, loop_id } => loop {
                match self.parser.next_object_field_or_end()? {
                    JsonObjectStep::End => return Ok(Control::Continue),
                    JsonObjectStep::Field { key, span } => {
                        let frame = self
                            .structs
                            .last()
                            .expect("struct frame is present while matching fields");
                        let matched = frame.match_field(&key);

                        let Some(matched) = matched else {
                            if shape.has_deny_unknown_fields_attr() {
                                return Err(vm_error(
                                    Some(span),
                                    DeserializeErrorKind::UnknownField {
                                        field: key.as_str().to_owned().into(),
                                        suggestion: None,
                                    },
                                ));
                            }
                            self.parser.skip_value()?;
                            continue;
                        };
                        let MatchedField {
                            index,
                            field,
                            ordered,
                        } = matched;

                        if !ordered && let Some(first_span) = frame.seen.get(index).copied() {
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
                            let value = self.parser.read_scalar_input()?;
                            unsafe {
                                write_scalar_input(
                                    self.parser,
                                    field.shape,
                                    scalar,
                                    field_ptr,
                                    value,
                                )?;
                            }
                            let frame = self
                                .structs
                                .last_mut()
                                .expect("struct frame is present while decoding scalar field");
                            frame.mark_seen(index, span);
                            continue;
                        }

                        let old_base = self.base;
                        self.base = unsafe { frame.base.field_uninit(field.offset) };
                        return Ok(call_program_or_block_then(
                            &field.program,
                            Continuation::FieldDone {
                                index,
                                span,
                                old_base,
                                loop_id: *loop_id,
                            },
                        ));
                    }
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
                    let value = self.parser.read_scalar_input()?;
                    unsafe {
                        write_scalar_input(
                            self.parser,
                            option.t(),
                            *scalar,
                            scratch_ptr_uninit(&scratch),
                            value,
                        )?;
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
                self.parser.consume_array_start()?;
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
                    let value = match self.parser.next_sequence_scalar_or_end()? {
                        JsonSequenceScalarStep::End => return Ok(Control::Continue),
                        JsonSequenceScalarStep::Value { value } => value,
                    };

                    if let Some(slot) = self.direct_list_slot()? {
                        unsafe {
                            write_scalar_input(self.parser, list.t(), *scalar, slot, value)?;
                            self.mark_direct_list_slot_initialized();
                        }
                        continue;
                    }

                    let scratch = self.scratch.reserve(*element_layout);
                    unsafe {
                        write_scalar_input(
                            self.parser,
                            list.t(),
                            *scalar,
                            scratch_ptr_uninit(&scratch),
                            value,
                        )?;
                    }
                    self.push_list_element(*list, &scratch)?;
                    self.scratch.release(scratch);
                    continue;
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
                frame.mark_seen(index, span);
                self.base = old_base;
                Ok(Control::CallBlock(loop_id))
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
    continuation: Continuation,
) -> Control<'program, ExecBlock, ExecOp, Continuation> {
    match program {
        [JsonOp::CallBlock(block)] => Control::CallBlockThen(*block, continuation),
        _ => Control::CallProgramThen(program, continuation),
    }
}

enum Continuation {
    FinishStruct,
    FieldDone {
        index: usize,
        span: Span,
        old_base: PtrUninit,
        loop_id: ExecBlock,
    },
    OptionSome {
        option: OptionDef,
        option_ptr: PtrUninit,
        old_base: PtrUninit,
        scratch: ScratchSlot,
    },
    FinishList,
    PushedListElement {
        list: ListDef,
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

struct MatchedField<'program> {
    index: usize,
    field: &'program FieldPlan<ExecBlock>,
    ordered: bool,
}

struct StructFrame<'program> {
    shape: &'static Shape,
    base: PtrUninit,
    fields: &'program [FieldPlan<ExecBlock>],
    seen: InitializedLedger<Span>,
    next_field: usize,
}

impl<'program> StructFrame<'program> {
    fn new(
        shape: &'static Shape,
        base: PtrUninit,
        fields: &'program [FieldPlan<ExecBlock>],
    ) -> Self {
        Self {
            shape,
            base,
            fields,
            seen: InitializedLedger::new(fields.len()),
            next_field: 0,
        }
    }

    fn match_field(&self, key: &JsonFieldKey<'_>) -> Option<MatchedField<'program>> {
        if let Some(field) = self.fields.get(self.next_field)
            && field.matches_key(key)
        {
            return Some(MatchedField {
                index: self.next_field,
                field,
                ordered: true,
            });
        }

        self.fields
            .iter()
            .enumerate()
            .find(|(_, field)| field.matches_key(key))
            .map(|(index, field)| MatchedField {
                index,
                field,
                ordered: false,
            })
    }

    fn mark_seen(&mut self, index: usize, span: Span) {
        self.seen.mark(index, span);
        if index == self.next_field {
            self.advance_next_field();
        }
    }

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
                    self.seen.mark(index, Span { offset: 0, len: 0 });
                }
                MissingField::DefaultTrait { explicit } => {
                    if unsafe { field.shape.call_default_in_place(field_ptr) }.is_some() {
                        self.seen.mark(index, Span { offset: 0, len: 0 });
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
                    self.seen.mark(index, Span { offset: 0, len: 0 });
                }
            }
        }
        core::mem::forget(self);
        Ok(())
    }

    unsafe fn drop_initialized_fields(&self) {
        for (index, _) in self.seen.iter_initialized_rev() {
            let field = &self.fields[index];
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
            let value = raw_string(parser, token, shape)?;
            unsafe {
                dst.put(value.into_owned());
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
        ScanToken::NeedMore { .. } => "incomplete token",
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
    T: TryFrom<u128>,
{
    let value = match token.token {
        ScanToken::Number { start, end, hint } => {
            let number = parser.parse_number(start, end, hint)?;
            match number {
                ParsedNumber::U64(value) => value as u128,
                ParsedNumber::U128(value) => value,
                ParsedNumber::I64(value) if value >= 0 => value as u128,
                ParsedNumber::I128(value) if value >= 0 => value as u128,
                other => {
                    return Err(type_mismatch_name(
                        span,
                        target,
                        parsed_number_kind_name(&other),
                    ));
                }
            }
        }
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
    T: TryFrom<i128>,
{
    let value = match token.token {
        ScanToken::Number { start, end, hint } => {
            let number = parser.parse_number(start, end, hint)?;
            match number {
                ParsedNumber::I64(value) => value as i128,
                ParsedNumber::I128(value) => value,
                ParsedNumber::U64(value) => value as i128,
                ParsedNumber::U128(value) if value <= i128::MAX as u128 => value as i128,
                other => {
                    return Err(type_mismatch_name(
                        span,
                        target,
                        parsed_number_kind_name(&other),
                    ));
                }
            }
        }
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
