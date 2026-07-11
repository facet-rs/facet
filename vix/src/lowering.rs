//! VIR island lowering to architecture-neutral Weavy bytecode.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use weavy::exec::Executable;
use weavy::mem::Layout;
use weavy::task::{
    ArgCopy, Fn as WeavyFn, FnId as WeavyFnId, Op as WeavyOp, Program as WeavyProgram,
    StructuralFieldSource,
};
use weavy::{
    CallContract as WeavyCallContract, CallContractId as WeavyCallContractId,
    FrameContract as WeavyFrameContract, FrameRegion as WeavyFrameRegion,
    FunctionContract as WeavyFunctionContract, PayloadKind as WeavyPayloadKind,
    ProgramContract as WeavyProgramContract, RegionId as WeavyRegionId,
    RegionShape as WeavyRegionShape, SchemaContract as WeavySchemaContract,
    SchemaRef as WeavySchemaRef, ValueFieldUse as WeavyValueFieldUse,
    ValueSelector as WeavyValueSelector, ValueShapeContract as WeavyValueShapeContract,
    ValueShapeKind as WeavyValueShapeKind, ValueShapeRef as WeavyValueShapeRef,
    ValueVariant as WeavyValueVariant, WordKind as WeavyWordKind,
};

use crate::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticPayload, Diagnostics};
use crate::runtime::{
    DemandKey, DemandPreimage, FrameRegion, FrameSlot, FrameWords, RecipeId, SchemaId,
};
use crate::support::Span;
use crate::vir::{
    EnumType, Function, FunctionId, Island, Node, NodeId, NodeRef, ORDERING_EQUAL_VARIANT,
    ORDERING_GREATER_VARIANT, ORDERING_LESS_VARIANT, Op, Type, VariantPayload,
};

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct SourceMapEntry {
    pub trace_id: u32,
    pub function: FunctionId,
    pub node: NodeId,
    pub span: Span,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct LoweringAttribution {
    pub functions: Vec<FunctionId>,
    pub source_map: Vec<SourceMapEntry>,
}

impl LoweringAttribution {
    #[must_use]
    pub fn function_for_frame(&self, frame: u32) -> Option<FunctionId> {
        self.functions.get(frame as usize).copied()
    }

    #[must_use]
    pub fn source_for_trace(&self, trace_id: u32) -> Option<&SourceMapEntry> {
        self.source_map
            .get(trace_id as usize)
            .filter(|entry| entry.trace_id == trace_id)
    }

    #[must_use]
    pub fn source_for_node(&self, node: NodeRef) -> Option<&SourceMapEntry> {
        self.source_map
            .iter()
            .find(|entry| entry.function == node.function && entry.node == node.node)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstantBinding {
    pub function: FunctionId,
    pub entry: usize,
    pub slot: FrameSlot,
    pub schema: WeavySchemaRef,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueConstant {
    pub node: NodeRef,
    pub root: ConstantBinding,
    pub owner: ConstantBinding,
    pub store_schema: SchemaId,
    pub bytes: Vec<u8>,
}

struct PendingValueConstant {
    node: NodeRef,
    root_slot: FrameSlot,
    owner_slot: FrameSlot,
    store_schema: SchemaId,
    bytes: Vec<u8>,
    span: Span,
}

/// Cached executable bytes for one VIR recipe. Per-compilation source spans
/// and per-demand memo locations deliberately live outside this artifact.
pub struct LoweringArtifact {
    pub recipe: RecipeId,
    pub demand_key: DemandKey,
    pub demand_preimage: DemandPreimage,
    pub program: WeavyProgram,
    pub contract: WeavyProgramContract,
    pub executable: Executable,
    pub pc_nodes: Vec<Vec<NodeRef>>,
    pub constants: Vec<ValueConstant>,
}

impl LoweringArtifact {
    #[must_use]
    pub fn node_for_pc(&self, frame: u32, pc: u32) -> Option<NodeRef> {
        self.pc_nodes
            .get(frame as usize)
            .and_then(|nodes| nodes.get(pc as usize))
            .copied()
    }

    #[must_use]
    pub fn render(&self) -> String {
        let mut out = String::new();
        for constant in &self.constants {
            let _ = writeln!(
                out,
                "constant n{} root(function={}, entry={}, frame[{}], schema={:?}) owner(function={}, entry={}, frame[{}], schema={:?}) store_schema={} bytes={}",
                constant.node.node.0,
                constant.root.function.0,
                constant.root.entry,
                constant.root.slot.byte_offset(),
                constant.root.schema,
                constant.owner.function.0,
                constant.owner.entry,
                constant.owner.slot.byte_offset(),
                constant.owner.schema,
                constant.store_schema.0.hex(),
                constant.bytes.len()
            );
        }
        for (function_index, function) in self.program.fns.iter().enumerate() {
            let _ = writeln!(
                out,
                "weavy fn {function_index} frame(size={}, align={})",
                function.frame.size, function.frame.align
            );
            for (pc, op) in function.code.iter().enumerate() {
                let _ = writeln!(out, "  {pc:04} {op:?}");
            }
        }
        out
    }
}

#[derive(facet::Facet, Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LoweringCacheCounters {
    pub hits: u64,
    pub misses: u64,
}

/// Explicit non-semantic cache of lowering artifacts. This is not a value
/// memo: eviction can only cause recompilation.
///
/// r[impl machine.lowering.cache]
#[derive(Default)]
pub struct LoweringCache {
    entries: BTreeMap<RecipeId, LoweringArtifact>,
    counters: LoweringCacheCounters,
}

impl LoweringCache {
    pub fn get_or_lower(&mut self, island: &Island) -> Result<&LoweringArtifact, Diagnostics> {
        let recipe = RecipeId::from_canonical_vir(&island.canonical_recipe_bytes());
        if self.entries.contains_key(&recipe) {
            self.counters.hits += 1;
            return Ok(self.entries.get(&recipe).expect("entry was just observed"));
        }
        self.counters.misses += 1;
        let lowered = lower_island(island, recipe)?;
        self.entries.insert(recipe, lowered);
        Ok(self.entries.get(&recipe).expect("entry was just inserted"))
    }

    #[must_use]
    pub fn counters(&self) -> LoweringCacheCounters {
        self.counters
    }

    pub fn inspect(&self) -> impl Iterator<Item = (&RecipeId, &LoweringArtifact)> {
        self.entries.iter()
    }
}

/// Build the current compilation's attribution table separately from cached
/// bytecode. A span-only edit neither invalidates bytecode nor inherits a stale
/// source map.
#[must_use]
pub fn source_map_for(island: &Island) -> Vec<SourceMapEntry> {
    attribution_for(island).source_map
}

/// Build per-compilation frame and trace attribution separately from cached
/// bytecode. Closure-local Weavy ids remain stable while source spans and
/// module-local function ids may change between compilations.
#[must_use]
pub fn attribution_for(island: &Island) -> LoweringAttribution {
    let mut functions = Vec::with_capacity(1 + island.callees.len());
    functions.push(island.function);
    functions.extend(island.callees.iter().map(|function| function.id));

    let mut source_map = Vec::new();
    push_source_entries(&mut source_map, island.function, &island.nodes);
    for function in &island.callees {
        push_source_entries(&mut source_map, function.id, &function.nodes);
    }
    LoweringAttribution {
        functions,
        source_map,
    }
}

fn push_source_entries(entries: &mut Vec<SourceMapEntry>, function: FunctionId, nodes: &[Node]) {
    for node in nodes.iter().filter(|node| node.op != Op::Yield) {
        entries.push(SourceMapEntry {
            trace_id: u32::try_from(entries.len()).expect("trace count fits u32"),
            function,
            node: node.id,
            span: node.span,
        });
    }
}

fn lower_island(island: &Island, recipe: RecipeId) -> Result<LoweringArtifact, Diagnostics> {
    let output = island
        .nodes
        .iter()
        .find(|node| node.id == island.output)
        .ok_or_else(|| lowering_diagnostic(Span { start: 0, end: 0 }, "missing island output"))?;
    if output.ty != Type::Check {
        return Err(lowering_diagnostic(
            output.span,
            "island output is not a Check",
        ));
    }

    let function_ids = island.local_function_ids();
    let functions = island
        .callees
        .iter()
        .map(|function| (function.id, function))
        .collect::<BTreeMap<_, _>>();
    let attribution = attribution_for(island);
    let trace_ids = attribution
        .source_map
        .iter()
        .map(|entry| {
            (
                NodeRef {
                    function: entry.function,
                    node: entry.node,
                },
                entry.trace_id,
            )
        })
        .collect::<BTreeMap<_, _>>();
    let constant_closures = constant_closures(island);
    let mut layouts = BTreeMap::new();
    layouts.insert(
        island.function,
        FunctionLayout::build(
            island.function,
            &island.nodes,
            constant_closures.get(&island.function).ok_or_else(|| {
                lowering_diagnostic(output.span, "island function has no constant closure")
            })?,
            output.span,
        )?,
    );
    for function in &island.callees {
        layouts.insert(
            function.id,
            FunctionLayout::build(
                function.id,
                &function.nodes,
                constant_closures.get(&function.id).ok_or_else(|| {
                    lowering_diagnostic(function.span, "called function has no constant closure")
                })?,
                function.span,
            )?,
        );
    }
    let regions = RegionAssignments::build(island, &layouts, &constant_closures)?;
    let context = LoweringContext {
        root_function: island.function,
        function_ids: &function_ids,
        functions: &functions,
        trace_ids: &trace_ids,
        layouts: &layouts,
        regions: &regions,
    };

    let mut pending_constants = BTreeMap::new();
    let mut functions_out = Vec::with_capacity(1 + island.callees.len());
    let mut pc_nodes = Vec::with_capacity(1 + island.callees.len());
    let lowered_root = lower_vir_function(
        island.function,
        &island.nodes,
        &[],
        island.output,
        &context,
        &mut pending_constants,
    )?;
    functions_out.push(lowered_root.function);
    pc_nodes.push(lowered_root.pc_nodes);
    for function in &island.callees {
        let output = function.output.ok_or_else(|| {
            lowering_diagnostic(function.span, "called VIR function has no return node")
        })?;
        let lowered = lower_vir_function(
            function.id,
            &function.nodes,
            &function.parameters,
            output,
            &context,
            &mut pending_constants,
        )?;
        functions_out.push(lowered.function);
        pc_nodes.push(lowered.pc_nodes);
    }
    let program = WeavyProgram { fns: functions_out };
    let contract =
        ProgramContractBuilder::build(island, &program, &layouts, &constant_closures, &regions)?;
    let constants = bind_constants(
        pending_constants,
        &contract,
        island,
        &layouts,
        &function_ids,
    )?;
    let demand_preimage = DemandPreimage {
        closure: recipe,
        arguments: Vec::new(),
    };
    let verified = program.clone().verify(contract.clone()).map_err(|error| {
        lowering_diagnostic(
            output.span,
            &format!("Weavy verifier rejected lowered program: {error:?}"),
        )
    })?;
    Ok(LoweringArtifact {
        recipe,
        demand_key: DemandKey::from_preimage(&demand_preimage),
        demand_preimage,
        program,
        contract,
        executable: Executable::new(verified),
        pc_nodes,
        constants,
    })
}

fn bind_constants(
    pending: BTreeMap<NodeRef, PendingValueConstant>,
    contract: &WeavyProgramContract,
    island: &Island,
    layouts: &BTreeMap<FunctionId, FunctionLayout>,
    function_ids: &BTreeMap<FunctionId, u32>,
) -> Result<Vec<ValueConstant>, Diagnostics> {
    pending
        .into_values()
        .map(|pending| {
            let owner_parameters = if pending.node.function == island.function {
                0
            } else {
                island
                    .callees
                    .iter()
                    .find(|function| function.id == pending.node.function)
                    .map_or(0, |function| function.parameters.len())
            };
            let root_entry =
                constant_entry_index(island.function, pending.node, 0, layouts, pending.span)?;
            let owner_entry = constant_entry_index(
                pending.node.function,
                pending.node,
                owner_parameters,
                layouts,
                pending.span,
            )?;
            let root_schema = validate_constant_entry(
                contract,
                function_ids,
                island.function,
                root_entry,
                pending.root_slot,
                pending.span,
                "root constant publication",
            )?;
            let owner_schema = validate_constant_entry(
                contract,
                function_ids,
                pending.node.function,
                owner_entry,
                pending.owner_slot,
                pending.span,
                "owning-function constant",
            )?;
            if root_schema != owner_schema {
                return Err(lowering_diagnostic(
                    pending.span,
                    "constant root and owner contract schemas differ",
                ));
            }
            Ok(ValueConstant {
                node: pending.node,
                root: ConstantBinding {
                    function: island.function,
                    entry: root_entry,
                    slot: pending.root_slot,
                    schema: root_schema,
                },
                owner: ConstantBinding {
                    function: pending.node.function,
                    entry: owner_entry,
                    slot: pending.owner_slot,
                    schema: owner_schema,
                },
                store_schema: pending.store_schema,
                bytes: pending.bytes,
            })
        })
        .collect()
}

fn constant_entry_index(
    function: FunctionId,
    node: NodeRef,
    parameter_count: usize,
    layouts: &BTreeMap<FunctionId, FunctionLayout>,
    span: Span,
) -> Result<usize, Diagnostics> {
    let layout = layouts
        .get(&function)
        .ok_or_else(|| lowering_diagnostic(span, "constant owner has no function layout"))?;
    let ordinal = layout
        .constant_slots
        .keys()
        .position(|candidate| *candidate == node)
        .ok_or_else(|| lowering_diagnostic(span, "constant is absent from its function ABI"))?;
    parameter_count
        .checked_add(ordinal)
        .ok_or_else(|| lowering_diagnostic(span, "constant entry index overflow"))
}

fn validate_constant_entry(
    contract: &WeavyProgramContract,
    function_ids: &BTreeMap<FunctionId, u32>,
    function: FunctionId,
    entry: usize,
    slot: FrameSlot,
    span: Span,
    role: &str,
) -> Result<WeavySchemaRef, Diagnostics> {
    let function_index = function_ids
        .get(&function)
        .copied()
        .and_then(|index| usize::try_from(index).ok())
        .ok_or_else(|| lowering_diagnostic(span, "constant function is absent from the ABI"))?;
    let function_contract = contract
        .functions
        .get(function_index)
        .ok_or_else(|| lowering_diagnostic(span, "constant function contract is absent"))?;
    let region_id = *function_contract
        .entries
        .get(entry)
        .ok_or_else(|| lowering_diagnostic(span, "constant entry is absent from the ABI"))?;
    let region = function_contract
        .frame
        .regions
        .get(region_id.0 as usize)
        .ok_or_else(|| lowering_diagnostic(span, "constant entry region is absent"))?;
    if region.offset != slot.byte_offset() {
        return Err(lowering_diagnostic(
            span,
            &format!("{role} offset does not match its recorded frame slot"),
        ));
    }
    let schema = match region.shape.words.as_slice() {
        [kinds] => match kinds.as_slice() {
            [WeavyWordKind::Handle(schema)] => *schema,
            _ => {
                return Err(lowering_diagnostic(
                    span,
                    &format!("{role} is not an exact one-word Handle(schema)"),
                ));
            }
        },
        _ => {
            return Err(lowering_diagnostic(
                span,
                &format!("{role} is not an exact one-word Handle(schema)"),
            ));
        }
    };
    Ok(schema)
}

struct ProgramContractBuilder<'a> {
    layouts: &'a BTreeMap<FunctionId, FunctionLayout>,
    constant_closures: &'a BTreeMap<FunctionId, BTreeSet<NodeRef>>,
    regions: &'a RegionAssignments,
    function_order: Vec<FunctionContractSource<'a>>,
    closure_targets: BTreeSet<FunctionId>,
    calls: Vec<WeavyCallContract>,
    schemas: Vec<WeavySchemaContract>,
    schema_keys: Vec<Type>,
    value_shapes: Vec<WeavyValueShapeContract>,
    value_shape_keys: Vec<Type>,
}

fn lower_equality_node(
    node: &Node,
    dst: FrameRegion,
    values: &BTreeMap<NodeId, LoweredSlot>,
    sequence: &SequenceContext<'_, '_, '_>,
    outputs: &mut SequenceOutputs<'_, '_>,
) -> Result<ValueRepresentation, Diagnostics> {
    require_node_type(node, Type::Bool)?;
    let (a, b) = binary_values(node, values)?;
    if a.ty != b.ty || !a.ty.equality_is_structural() {
        return Err(lowering_diagnostic(
            node.span,
            "equality operands do not have one structural VIR type",
        ));
    }
    if dst.words() != FrameWords::ONE {
        return Err(lowering_diagnostic(
            node.span,
            "equality result is not one word",
        ));
    }
    let mut temps = TemporaryCursor::new(
        sequence.function.layout,
        sequence.lowering.regions,
        sequence.function.id,
        node,
    )?;
    let accumulator = temps.take(&Type::Int, node.span)?;
    let work = temps.take(&Type::Int, node.span)?;
    let equal = temps.take(&Type::Int, node.span)?;
    outputs.code.push(WeavyOp::ConstI64 {
        dst: accumulator.region.start().byte_offset(),
        value: 1,
    });
    emit_semantic_equality(
        node,
        &a.ty,
        &a,
        &b,
        accumulator.region.start(),
        work.region.start(),
        equal.region.start(),
        &mut temps,
        outputs.code,
    )?;
    if matches!(node.op, Op::Ne) {
        outputs.code.push(WeavyOp::ConstI64 {
            dst: equal.region.start().byte_offset(),
            value: 0,
        });
        outputs.code.push(WeavyOp::EqI64 {
            dst: accumulator.region.start().byte_offset(),
            a: accumulator.region.start().byte_offset(),
            b: equal.region.start().byte_offset(),
        });
    }
    outputs.code.push(WeavyOp::CopyI64 {
        dst: dst.start().byte_offset(),
        src: accumulator.region.start().byte_offset(),
    });
    temps.finish(node.span)?;
    Ok(ValueRepresentation::Word)
}

fn emit_semantic_equality(
    node: &Node,
    ty: &Type,
    a: &LoweredSlot,
    b: &LoweredSlot,
    accumulator: FrameSlot,
    work: FrameSlot,
    equal: FrameSlot,
    temps: &mut TemporaryCursor<'_>,
    code: &mut CodeBuilder,
) -> Result<(), Diagnostics> {
    match ty {
        Type::Bool | Type::Int | Type::Check => {
            code.push(WeavyOp::EqI64 {
                dst: work.byte_offset(),
                a: a.region.start().byte_offset(),
                b: b.region.start().byte_offset(),
            });
            code.push(WeavyOp::MulI64 {
                dst: accumulator.byte_offset(),
                a: accumulator.byte_offset(),
                b: work.byte_offset(),
            });
        }
        Type::String => {
            code.push(WeavyOp::CompareValueBytes {
                dst: work.byte_offset(),
                a: a.region.start().byte_offset(),
                b: b.region.start().byte_offset(),
            });
            code.push(WeavyOp::ConstI64 {
                dst: equal.byte_offset(),
                value: i64::from(ORDERING_EQUAL_VARIANT),
            });
            code.push(WeavyOp::EqI64 {
                dst: work.byte_offset(),
                a: work.byte_offset(),
                b: equal.byte_offset(),
            });
            code.push(WeavyOp::MulI64 {
                dst: accumulator.byte_offset(),
                a: accumulator.byte_offset(),
                b: work.byte_offset(),
            });
        }
        Type::Tuple(fields) => {
            for (index, field) in fields.iter().enumerate() {
                let left = temps.take(field, node.span)?;
                let right = temps.take(field, node.span)?;
                let field = u32::try_from(index)
                    .map_err(|_| lowering_diagnostic(node.span, "product field index overflow"))?;
                code.push(WeavyOp::ProductProject {
                    dst: left.region_id,
                    product: a.region_id,
                    field,
                });
                code.push(WeavyOp::ProductProject {
                    dst: right.region_id,
                    product: b.region_id,
                    field,
                });
                emit_semantic_equality(
                    node,
                    field_type(fields, index),
                    &left,
                    &right,
                    accumulator,
                    work,
                    equal,
                    temps,
                    code,
                )?;
            }
        }
        Type::Record(record) => {
            for (index, field) in record.fields.iter().enumerate() {
                let left = temps.take(&field.ty, node.span)?;
                let right = temps.take(&field.ty, node.span)?;
                let field_index = u32::try_from(index)
                    .map_err(|_| lowering_diagnostic(node.span, "record field index overflow"))?;
                code.push(WeavyOp::ProductProject {
                    dst: left.region_id,
                    product: a.region_id,
                    field: field_index,
                });
                code.push(WeavyOp::ProductProject {
                    dst: right.region_id,
                    product: b.region_id,
                    field: field_index,
                });
                emit_semantic_equality(
                    node,
                    &field.ty,
                    &left,
                    &right,
                    accumulator,
                    work,
                    equal,
                    temps,
                    code,
                )?;
            }
        }
        Type::Enum(enumeration) => {
            for (variant_index, variant) in enumeration.variants.iter().enumerate() {
                let variant_index = u32::try_from(variant_index)
                    .map_err(|_| lowering_diagnostic(node.span, "enum variant index overflow"))?;
                code.push(WeavyOp::EnumIsVariant {
                    dst: work_region_id(work, temps, node.span)?,
                    value: a.region_id,
                    variant: variant_index,
                });
                code.push(WeavyOp::EnumIsVariant {
                    dst: work_region_id(equal, temps, node.span)?,
                    value: b.region_id,
                    variant: variant_index,
                });
                code.push(WeavyOp::EqI64 {
                    dst: work.byte_offset(),
                    a: work.byte_offset(),
                    b: equal.byte_offset(),
                });
                code.push(WeavyOp::MulI64 {
                    dst: accumulator.byte_offset(),
                    a: accumulator.byte_offset(),
                    b: work.byte_offset(),
                });
                // `Eq` is also true when both operands are *not* this
                // variant. Multiplying by `b is variant` leaves one only on
                // the sole path where both checked selectors name this arm.
                code.push(WeavyOp::MulI64 {
                    dst: work.byte_offset(),
                    a: work.byte_offset(),
                    b: equal.byte_offset(),
                });
                let next = code.label();
                code.jump_if_zero(work, next);
                let fields: Vec<&Type> = match &variant.payload {
                    VariantPayload::Unit => Vec::new(),
                    VariantPayload::Tuple(fields) => fields.iter().collect(),
                    VariantPayload::Record(fields) => {
                        fields.iter().map(|field| &field.ty).collect()
                    }
                };
                for (field_index, field) in fields.into_iter().enumerate() {
                    let left = temps.take(field, node.span)?;
                    let right = temps.take(field, node.span)?;
                    let field_index = u32::try_from(field_index)
                        .map_err(|_| lowering_diagnostic(node.span, "enum field index overflow"))?;
                    code.push(WeavyOp::EnumProjectChecked {
                        dst: left.region_id,
                        value: a.region_id,
                        variant: variant_index,
                        field: field_index,
                    });
                    code.push(WeavyOp::EnumProjectChecked {
                        dst: right.region_id,
                        value: b.region_id,
                        variant: variant_index,
                        field: field_index,
                    });
                    emit_semantic_equality(
                        node,
                        field,
                        &left,
                        &right,
                        accumulator,
                        work,
                        equal,
                        temps,
                        code,
                    )?;
                }
                code.bind(next, node.span)?;
            }
        }
        Type::Array(_) | Type::Function { .. } | Type::StreamCheck => {
            return Err(lowering_diagnostic(
                node.span,
                "equality lowering is not implemented for this VIR type",
            ));
        }
    }
    Ok(())
}

fn work_region_id(
    slot: FrameSlot,
    temps: &TemporaryCursor<'_>,
    span: Span,
) -> Result<WeavyRegionId, Diagnostics> {
    temps
        .regions
        .iter()
        .zip(temps.ids)
        .find_map(|(region, id)| (region.region.start() == slot).then_some(*id))
        .ok_or_else(|| lowering_diagnostic(span, "comparison work slot has no contract region"))
}

fn field_type<'a>(fields: &'a [Type], index: usize) -> &'a Type {
    &fields[index]
}

#[derive(Clone, Copy)]
struct FunctionContractSource<'a> {
    id: FunctionId,
    span: Span,
    parameters: &'a [crate::vir::Parameter],
    nodes: &'a [Node],
    output: NodeId,
}

impl<'a> ProgramContractBuilder<'a> {
    fn build(
        island: &'a Island,
        program: &WeavyProgram,
        layouts: &'a BTreeMap<FunctionId, FunctionLayout>,
        constant_closures: &'a BTreeMap<FunctionId, BTreeSet<NodeRef>>,
        regions: &'a RegionAssignments,
    ) -> Result<WeavyProgramContract, Diagnostics> {
        let root_output = island.output;
        let mut function_order = Vec::with_capacity(1 + island.callees.len());
        function_order.push(FunctionContractSource {
            id: island.function,
            span: Span { start: 0, end: 0 },
            parameters: &[],
            nodes: island.nodes.as_slice(),
            output: root_output,
        });
        for function in &island.callees {
            let output = function.output.ok_or_else(|| {
                lowering_diagnostic(function.span, "called VIR function has no return node")
            })?;
            function_order.push(FunctionContractSource {
                id: function.id,
                span: function.span,
                parameters: function.parameters.as_slice(),
                nodes: function.nodes.as_slice(),
                output,
            });
        }

        let closure_targets = function_order
            .iter()
            .flat_map(|function| function.nodes.iter())
            .filter_map(|node| match node.op {
                Op::Closure(callee) => Some(callee),
                _ => None,
            })
            .collect();

        let mut builder = Self {
            layouts,
            constant_closures,
            regions,
            function_order,
            closure_targets,
            calls: Vec::new(),
            schemas: Vec::new(),
            schema_keys: Vec::new(),
            value_shapes: Vec::new(),
            value_shape_keys: Vec::new(),
        };

        let mut functions = Vec::with_capacity(program.fns.len());
        for (function_index, function) in program.fns.iter().enumerate() {
            let source = builder.function_order[function_index];
            functions.push(builder.function_contract(source, function)?);
        }
        for (function_index, source) in builder.function_order.iter().enumerate() {
            for node in source
                .nodes
                .iter()
                .filter(|node| matches!(node.op, Op::Closure(_)))
            {
                let Op::Closure(target) = node.op else {
                    unreachable!();
                };
                let target_index = builder
                    .function_order
                    .iter()
                    .position(|candidate| candidate.id == target)
                    .ok_or_else(|| lowering_diagnostic(node.span, "closure target is absent"))?;
                let call = functions[target_index].call_contract.ok_or_else(|| {
                    lowering_diagnostic(node.span, "closure target has no exact call ABI")
                })?;
                let (callee_region, _) = builder.regions.closure(source.id, node.id, node.span)?;
                let region = functions[function_index]
                    .frame
                    .regions
                    .get_mut(callee_region.0 as usize)
                    .ok_or_else(|| {
                        lowering_diagnostic(node.span, "closure callee region is absent")
                    })?;
                region.shape = WeavyRegionShape::word(WeavyWordKind::Callable(call));
            }
        }
        Ok(WeavyProgramContract {
            functions,
            calls: builder.calls,
            schemas: builder.schemas,
            value_shapes: builder.value_shapes,
        })
    }

    fn function_contract(
        &mut self,
        function: FunctionContractSource<'_>,
        lowered: &WeavyFn,
    ) -> Result<WeavyFunctionContract, Diagnostics> {
        let layout = self.layouts.get(&function.id).ok_or_else(|| {
            lowering_diagnostic(function.span, "missing function contract layout")
        })?;
        let mut regions = Vec::new();
        let mut node_region_ids = BTreeMap::new();
        let mut constant_region_ids = BTreeMap::new();
        for node in function.nodes {
            let region = layout.region(node.id, node.span)?;
            let contract = self.frame_region(region.start(), &node.ty)?;
            let region_id = self.regions.node(function.id, node.id, node.span)?;
            if region_id.0 as usize != regions.len() {
                return Err(lowering_diagnostic(
                    node.span,
                    "contract region assignment is not in canonical node order",
                ));
            }
            node_region_ids.insert(node.id, region_id);
            regions.push(contract);
        }
        if let Some(scratch) = layout.scratch {
            regions.push(WeavyFrameRegion::new(
                scratch.byte_offset(),
                WeavyRegionShape::word(WeavyWordKind::Scalar),
            ));
        }
        if let Some(control) = layout.control_scratch {
            regions.push(WeavyFrameRegion::new(
                control.expected.byte_offset(),
                WeavyRegionShape::word(WeavyWordKind::Scalar),
            ));
            regions.push(WeavyFrameRegion::new(
                control.condition.byte_offset(),
                WeavyRegionShape::word(WeavyWordKind::Scalar),
            ));
        }
        for (&node, temps) in &layout.comparison_temps {
            let assigned = self
                .regions
                .comparison_temps(function.id, node, function.span)?;
            if assigned.len() != temps.len() {
                return Err(lowering_diagnostic(
                    function.span,
                    "comparison temporary contract assignment has the wrong length",
                ));
            }
            for (temp, region_id) in temps.iter().zip(assigned) {
                if region_id.0 as usize != regions.len() {
                    return Err(lowering_diagnostic(
                        function.span,
                        "comparison temporary contract order is not canonical",
                    ));
                }
                regions.push(self.frame_region(temp.region.start(), &temp.ty)?);
            }
        }
        for (&node, (callee, environment)) in &layout.closure_temps {
            let node_source = function
                .nodes
                .iter()
                .find(|candidate| candidate.id == node)
                .ok_or_else(|| {
                    lowering_diagnostic(function.span, "closure temporary names a missing VIR node")
                })?;
            let Type::Function { parameter, result } = &node_source.ty else {
                return Err(lowering_diagnostic(
                    node_source.span,
                    "closure temporary is not attached to a function value",
                ));
            };
            let (callee_id, environment_id) =
                self.regions.closure(function.id, node, node_source.span)?;
            if callee_id.0 as usize != regions.len() {
                return Err(lowering_diagnostic(
                    node_source.span,
                    "closure callee region is not canonical",
                ));
            }
            let call = self.call_contract_for_signature(parameter, result, node_source.span)?;
            regions.push(WeavyFrameRegion::new(
                callee.start().byte_offset(),
                WeavyRegionShape::word(WeavyWordKind::Callable(call)),
            ));
            if environment_id.0 as usize != regions.len() {
                return Err(lowering_diagnostic(
                    node_source.span,
                    "closure environment region is not canonical",
                ));
            }
            regions.push(WeavyFrameRegion::new(
                environment.start().byte_offset(),
                WeavyRegionShape::word(WeavyWordKind::Scalar),
            ));
        }
        let local_constants = self
            .constant_closures
            .get(&function.id)
            .ok_or_else(|| lowering_diagnostic(function.span, "missing closure constants"))?;
        for constant in local_constants {
            let region_id = if constant.function == function.id {
                *node_region_ids.get(&constant.node).ok_or_else(|| {
                    lowering_diagnostic(function.span, "local constant node has no frame region")
                })?
            } else {
                let slot = layout.constant_slot(*constant, function.span)?;
                let region = WeavyFrameRegion::new(
                    slot.byte_offset(),
                    WeavyRegionShape::word(WeavyWordKind::Handle(
                        self.schema_for_type(&Type::String, function.span)?,
                    )),
                );
                let region_id = WeavyRegionId(regions.len() as u32);
                regions.push(region);
                region_id
            };
            constant_region_ids.insert(*constant, region_id);
        }
        let mut entries = Vec::with_capacity(
            function
                .parameters
                .len()
                .saturating_add(layout.constant_slots.len()),
        );
        for parameter in function.parameters {
            entries.push(*node_region_ids.get(&parameter.node).ok_or_else(|| {
                lowering_diagnostic(function.span, "parameter node has no frame region")
            })?);
        }
        for constant in layout.constant_slots.keys() {
            let entry = constant_region_ids.get(constant).copied().ok_or_else(|| {
                lowering_diagnostic(function.span, "constant slot has no contract region")
            })?;
            entries.push(entry);
        }
        let result = *node_region_ids
            .get(&function.output)
            .ok_or_else(|| lowering_diagnostic(function.span, "output node has no frame region"))?;
        let call_contract = if self.closure_targets.contains(&function.id) {
            let call = self.call_contract_for_function(function, &entries, result, &regions)?;
            Some(call)
        } else {
            None
        };
        Ok(WeavyFunctionContract {
            frame: WeavyFrameContract {
                layout: lowered.frame,
                regions,
            },
            entries,
            result,
            call_contract,
        })
    }

    fn call_contract_for_function(
        &mut self,
        function: FunctionContractSource<'_>,
        entries: &[WeavyRegionId],
        result: WeavyRegionId,
        regions: &[WeavyFrameRegion],
    ) -> Result<WeavyCallContractId, Diagnostics> {
        let parameter_len = function.parameters.len();
        let call = WeavyCallContract {
            entries: entries[..parameter_len]
                .iter()
                .map(|entry| canonical_call_region(&regions[entry.0 as usize]))
                .collect(),
            result: canonical_call_region(&regions[result.0 as usize]),
        };
        Ok(self.intern_call(call))
    }

    fn frame_region(
        &mut self,
        slot: FrameSlot,
        ty: &Type,
    ) -> Result<WeavyFrameRegion, Diagnostics> {
        self.schema_for_type(ty, Span { start: 0, end: 0 })?;
        let shape = self.shape_for_type(ty, Span { start: 0, end: 0 })?;
        let mut region = WeavyFrameRegion::new(slot.byte_offset(), shape);
        if let Some(value_shape) = self.value_shape_for_type(ty, Span { start: 0, end: 0 })? {
            region = region.with_value_shape(value_shape);
        }
        Ok(region)
    }

    fn shape_for_type(&mut self, ty: &Type, span: Span) -> Result<WeavyRegionShape, Diagnostics> {
        match ty {
            Type::Bool | Type::Int | Type::Check => {
                Ok(WeavyRegionShape::word(WeavyWordKind::Scalar))
            }
            Type::String => Ok(WeavyRegionShape::word(WeavyWordKind::Handle(
                self.schema_for_type(ty, span)?,
            ))),
            Type::StreamCheck => Err(lowering_diagnostic(
                span,
                "Stream<Check> has no contract frame shape",
            )),
            Type::Function { parameter, result } => {
                let call = self.call_contract_for_signature(parameter, result, span)?;
                Ok(WeavyRegionShape::new(vec![
                    WeavyWordKind::Callable(call).into(),
                    WeavyWordKind::Scalar.into(),
                ]))
            }
            Type::Tuple(elements) => self.product_shape(elements.iter(), span),
            Type::Record(record) => {
                self.product_shape(record.fields.iter().map(|field| &field.ty), span)
            }
            Type::Enum(enumeration) => self.enum_shape(enumeration, span),
            Type::Array(_) => Ok(WeavyRegionShape::word(WeavyWordKind::Handle(
                self.schema_for_type(ty, span)?,
            ))),
        }
    }

    fn product_shape<'t>(
        &mut self,
        fields: impl IntoIterator<Item = &'t Type>,
        span: Span,
    ) -> Result<WeavyRegionShape, Diagnostics> {
        let mut words = Vec::new();
        for field in fields {
            words.extend(self.shape_for_type(field, span)?.words);
        }
        Ok(WeavyRegionShape::new(words))
    }

    fn enum_shape(
        &mut self,
        enumeration: &EnumType,
        span: Span,
    ) -> Result<WeavyRegionShape, Diagnostics> {
        let layout = EnumLayout::for_enum(enumeration, span)?;
        let mut words: Vec<weavy::AllowedKinds> =
            vec![WeavyWordKind::Scalar.into(); layout.words.as_usize()];
        for variant in &layout.variants {
            for element in &variant.elements {
                let field_shape = self.shape_for_type(element.ty, span)?;
                let start = element.offset_words.checked_add(1).ok_or_else(|| {
                    lowering_diagnostic(span, "enum payload contract offset overflow")
                })?;
                for (index, kinds) in field_shape.words.into_iter().enumerate() {
                    let target = words.get_mut(start + index).ok_or_else(|| {
                        lowering_diagnostic(span, "enum payload contract lies outside shape")
                    })?;
                    for kind in kinds.as_slice() {
                        *target = target.clone().allowing(*kind);
                    }
                }
            }
        }
        Ok(WeavyRegionShape::new(words))
    }

    fn schema_for_type(&mut self, ty: &Type, span: Span) -> Result<WeavySchemaRef, Diagnostics> {
        if let Some(index) = self
            .schema_keys
            .iter()
            .position(|candidate| candidate == ty)
        {
            return Ok(WeavySchemaRef(index as u32));
        }
        let index = self.schemas.len();
        let schema = WeavySchemaRef(index as u32);
        self.schema_keys.push(ty.clone());
        self.schemas.push(WeavySchemaContract {
            inline: WeavyRegionShape::default(),
            value_shape: None,
            payload: WeavyPayloadKind::Inline,
        });
        let inline = match ty {
            Type::String | Type::Array(_) => WeavyRegionShape::word(WeavyWordKind::Handle(schema)),
            _ => self.shape_for_type(ty, span)?,
        };
        let value_shape = self.value_shape_for_type(ty, span)?;
        let payload = match ty {
            Type::String => WeavyPayloadKind::OpaqueBytes {
                byte_comparable: true,
            },
            Type::Array(element) => WeavyPayloadKind::DenseArray {
                element: self.schema_for_type(element, span)?,
            },
            _ => WeavyPayloadKind::Inline,
        };
        self.schemas[index] = WeavySchemaContract {
            inline,
            value_shape,
            payload,
        };
        Ok(schema)
    }

    fn value_shape_for_type(
        &mut self,
        ty: &Type,
        span: Span,
    ) -> Result<Option<WeavyValueShapeRef>, Diagnostics> {
        match ty {
            Type::Function { .. } | Type::Tuple(_) | Type::Record(_) | Type::Enum(_) => {
                Ok(Some(self.intern_value_shape_for_type(ty, span)?))
            }
            Type::Bool | Type::Int | Type::Check | Type::String | Type::Array(_) => Ok(None),
            Type::StreamCheck => Err(lowering_diagnostic(
                span,
                "Stream<Check> has no contract value shape",
            )),
        }
    }

    fn intern_value_shape_for_type(
        &mut self,
        ty: &Type,
        span: Span,
    ) -> Result<WeavyValueShapeRef, Diagnostics> {
        if let Some(index) = self
            .value_shape_keys
            .iter()
            .position(|candidate| candidate == ty)
        {
            return Ok(WeavyValueShapeRef(index as u32));
        }
        let shape = self.shape_for_type(ty, span)?;
        let kind = match ty {
            Type::Function { .. } => {
                let fields = vec![
                    WeavyValueFieldUse::new(
                        0,
                        WeavyRegionShape::word(shape.words[0].as_slice()[0]),
                    ),
                    WeavyValueFieldUse::new(
                        FrameSlot::word_size(),
                        WeavyRegionShape::word(WeavyWordKind::Scalar),
                    ),
                ];
                WeavyValueShapeKind::Product { fields }
            }
            Type::Tuple(elements) => WeavyValueShapeKind::Product {
                fields: self.value_fields(elements.iter(), span)?,
            },
            Type::Record(record) => WeavyValueShapeKind::Product {
                fields: self.value_fields(record.fields.iter().map(|field| &field.ty), span)?,
            },
            Type::Enum(enumeration) => self.enum_value_shape_kind(enumeration, span)?,
            _ => {
                return Err(lowering_diagnostic(
                    span,
                    "non-structural type reached value-shape interning",
                ));
            }
        };
        let value_shape = WeavyValueShapeRef(self.value_shapes.len() as u32);
        self.value_shape_keys.push(ty.clone());
        self.value_shapes
            .push(WeavyValueShapeContract { shape, kind });
        Ok(value_shape)
    }

    fn value_fields<'t>(
        &mut self,
        fields: impl IntoIterator<Item = &'t Type>,
        span: Span,
    ) -> Result<Vec<WeavyValueFieldUse>, Diagnostics> {
        let mut offset_words = 0usize;
        let mut out = Vec::new();
        for ty in fields {
            let shape = self.shape_for_type(ty, span)?;
            let mut field = WeavyValueFieldUse::new(
                FrameSlot::for_word(offset_words)
                    .ok_or_else(|| lowering_diagnostic(span, "product field offset overflow"))?
                    .byte_offset(),
                shape.clone(),
            );
            if self.field_requires_nested_ref(&shape) {
                field = field.with_value_shape(self.intern_value_shape_for_type(ty, span)?);
            }
            offset_words = offset_words
                .checked_add(shape.words.len())
                .ok_or_else(|| lowering_diagnostic(span, "product shape offset overflow"))?;
            out.push(field);
        }
        Ok(out)
    }

    fn enum_value_shape_kind(
        &mut self,
        enumeration: &EnumType,
        span: Span,
    ) -> Result<WeavyValueShapeKind, Diagnostics> {
        let layout = EnumLayout::for_enum(enumeration, span)?;
        let mut variants = Vec::with_capacity(layout.variants.len());
        for variant in &layout.variants {
            let mut fields = Vec::with_capacity(variant.elements.len());
            for element in &variant.elements {
                let shape = self.shape_for_type(element.ty, span)?;
                let offset_words = element.offset_words.checked_add(1).ok_or_else(|| {
                    lowering_diagnostic(span, "enum value-shape field offset overflow")
                })?;
                let mut field = WeavyValueFieldUse::new(
                    FrameSlot::for_word(offset_words)
                        .ok_or_else(|| {
                            lowering_diagnostic(span, "enum value-shape field offset overflow")
                        })?
                        .byte_offset(),
                    shape.clone(),
                );
                if self.field_requires_nested_ref(&shape) {
                    field =
                        field.with_value_shape(self.intern_value_shape_for_type(element.ty, span)?);
                }
                fields.push(field);
            }
            variants.push(WeavyValueVariant { fields });
        }
        Ok(WeavyValueShapeKind::Enum {
            selector: WeavyValueSelector {
                offset: 0,
                shape: WeavyRegionShape::word(WeavyWordKind::Scalar),
            },
            variants,
        })
    }

    fn field_requires_nested_ref(&self, shape: &WeavyRegionShape) -> bool {
        !(shape.words.len() == 1 && shape.words[0].as_slice().len() == 1)
    }

    fn call_contract_for_signature(
        &mut self,
        parameter: &Type,
        result: &Type,
        span: Span,
    ) -> Result<WeavyCallContractId, Diagnostics> {
        let mut entry = WeavyFrameRegion::new(0, self.shape_for_type(parameter, span)?);
        if let Some(value_shape) = self.value_shape_for_type(parameter, span)? {
            entry = entry.with_value_shape(value_shape);
        }
        let mut result_region = WeavyFrameRegion::new(0, self.shape_for_type(result, span)?);
        if let Some(value_shape) = self.value_shape_for_type(result, span)? {
            result_region = result_region.with_value_shape(value_shape);
        }
        Ok(self.intern_call(WeavyCallContract {
            entries: vec![entry],
            result: result_region,
        }))
    }

    fn intern_call(&mut self, call: WeavyCallContract) -> WeavyCallContractId {
        if let Some(index) = self.calls.iter().position(|candidate| candidate == &call) {
            return WeavyCallContractId(index as u32);
        }
        let id = WeavyCallContractId(self.calls.len() as u32);
        self.calls.push(call);
        id
    }
}

fn canonical_call_region(region: &WeavyFrameRegion) -> WeavyFrameRegion {
    let mut canonical = WeavyFrameRegion::new(0, region.shape.clone());
    if let Some(value_shape) = region.value_shape {
        canonical = canonical.with_value_shape(value_shape);
    }
    canonical
}

fn constant_closures(island: &Island) -> BTreeMap<FunctionId, BTreeSet<NodeRef>> {
    let mut closures = BTreeMap::new();
    let mut calls = BTreeMap::new();
    let mut register = |function: FunctionId, nodes: &[Node]| {
        closures.insert(
            function,
            nodes
                .iter()
                .filter(|node| matches!(node.op, Op::String(_)))
                .map(|node| NodeRef {
                    function,
                    node: node.id,
                })
                .collect::<BTreeSet<_>>(),
        );
        calls.insert(
            function,
            nodes
                .iter()
                .filter_map(|node| match &node.op {
                    Op::Call(callee) | Op::Closure(callee) => Some(*callee),
                    _ => None,
                })
                .collect::<BTreeSet<_>>(),
        );
    };
    register(island.function, &island.nodes);
    for function in &island.callees {
        register(function.id, &function.nodes);
    }

    loop {
        let previous = closures.clone();
        let mut changed = false;
        for (&function, callees) in &calls {
            let inherited = callees
                .iter()
                .filter_map(|callee| previous.get(callee))
                .flat_map(|constants| constants.iter().copied())
                .collect::<Vec<_>>();
            let constants = closures
                .get_mut(&function)
                .expect("every call-graph function has a closure");
            let before = constants.len();
            constants.extend(inherited);
            changed |= constants.len() != before;
        }
        if !changed {
            break;
        }
    }
    closures
}

struct LoweringContext<'a> {
    root_function: FunctionId,
    function_ids: &'a BTreeMap<FunctionId, u32>,
    functions: &'a BTreeMap<FunctionId, &'a Function>,
    trace_ids: &'a BTreeMap<NodeRef, u32>,
    layouts: &'a BTreeMap<FunctionId, FunctionLayout>,
    regions: &'a RegionAssignments,
}

/// The contract and the instruction stream share this canonical region order.
/// Nodes are retained even for zero-width values, so later regions cannot shift
/// when a product has no fields.
struct RegionAssignments {
    nodes: BTreeMap<FunctionId, BTreeMap<NodeId, WeavyRegionId>>,
    control: BTreeMap<FunctionId, AssignedControlRegions>,
    temps: BTreeMap<FunctionId, BTreeMap<NodeId, Vec<WeavyRegionId>>>,
    closures: BTreeMap<FunctionId, BTreeMap<NodeId, (WeavyRegionId, WeavyRegionId)>>,
}

#[derive(Clone, Copy)]
struct AssignedControlRegions {
    condition: WeavyRegionId,
}

impl RegionAssignments {
    fn build(
        island: &Island,
        layouts: &BTreeMap<FunctionId, FunctionLayout>,
        constant_closures: &BTreeMap<FunctionId, BTreeSet<NodeRef>>,
    ) -> Result<Self, Diagnostics> {
        let mut nodes = BTreeMap::new();
        let mut control = BTreeMap::new();
        let mut temps = BTreeMap::new();
        let mut closures = BTreeMap::new();
        let mut insert = |function: FunctionId, body: &[Node], span: Span| {
            let layout = layouts.get(&function).ok_or_else(|| {
                lowering_diagnostic(span, "missing function layout for region assignment")
            })?;
            let mut assigned = BTreeMap::new();
            for (index, node) in body.iter().enumerate() {
                layout.region(node.id, node.span)?;
                let id = WeavyRegionId(u32::try_from(index).map_err(|_| {
                    lowering_diagnostic(node.span, "region assignment exceeds u32")
                })?);
                if assigned.insert(node.id, id).is_some() {
                    return Err(lowering_diagnostic(
                        node.span,
                        "duplicate region assignment",
                    ));
                }
            }
            let mut next = body.len();
            if layout.scratch.is_some() {
                next = next.checked_add(1).ok_or_else(|| {
                    lowering_diagnostic(span, "scratch region assignment overflow")
                })?;
            }
            if layout.control_scratch.is_some() {
                let condition = next.checked_add(1).ok_or_else(|| {
                    lowering_diagnostic(span, "control region assignment overflow")
                })?;
                control.insert(
                    function,
                    AssignedControlRegions {
                        condition: WeavyRegionId(u32::try_from(condition).map_err(|_| {
                            lowering_diagnostic(span, "control region assignment exceeds u32")
                        })?),
                    },
                );
                next = condition.checked_add(1).ok_or_else(|| {
                    lowering_diagnostic(span, "control region assignment overflow")
                })?;
            }
            let mut assigned_temps = BTreeMap::new();
            for (&node, regions) in &layout.comparison_temps {
                let mut ids = Vec::with_capacity(regions.len());
                for _ in regions {
                    ids.push(WeavyRegionId(u32::try_from(next).map_err(|_| {
                        lowering_diagnostic(span, "temporary region assignment exceeds u32")
                    })?));
                    next = next.checked_add(1).ok_or_else(|| {
                        lowering_diagnostic(span, "temporary region assignment overflow")
                    })?;
                }
                assigned_temps.insert(node, ids);
            }
            let mut assigned_closures = BTreeMap::new();
            for &node in layout.closure_temps.keys() {
                let callee = WeavyRegionId(u32::try_from(next).map_err(|_| {
                    lowering_diagnostic(span, "closure temporary assignment exceeds u32")
                })?);
                next = next.checked_add(1).ok_or_else(|| {
                    lowering_diagnostic(span, "closure temporary assignment overflow")
                })?;
                let environment = WeavyRegionId(u32::try_from(next).map_err(|_| {
                    lowering_diagnostic(span, "closure temporary assignment exceeds u32")
                })?);
                next = next.checked_add(1).ok_or_else(|| {
                    lowering_diagnostic(span, "closure temporary assignment overflow")
                })?;
                assigned_closures.insert(node, (callee, environment));
            }
            let constants = constant_closures.get(&function).ok_or_else(|| {
                lowering_diagnostic(span, "missing constants for region assignment")
            })?;
            for constant in constants {
                if constant.function != function {
                    next = next.checked_add(1).ok_or_else(|| {
                        lowering_diagnostic(span, "constant region assignment overflow")
                    })?;
                }
            }
            nodes.insert(function, assigned);
            temps.insert(function, assigned_temps);
            closures.insert(function, assigned_closures);
            Ok(())
        };
        insert(island.function, &island.nodes, Span { start: 0, end: 0 })?;
        for function in &island.callees {
            insert(function.id, &function.nodes, function.span)?;
        }
        Ok(Self {
            nodes,
            control,
            temps,
            closures,
        })
    }

    fn node(
        &self,
        function: FunctionId,
        node: NodeId,
        span: Span,
    ) -> Result<WeavyRegionId, Diagnostics> {
        self.nodes
            .get(&function)
            .and_then(|nodes| nodes.get(&node))
            .copied()
            .ok_or_else(|| lowering_diagnostic(span, "VIR node has no assigned contract region"))
    }

    fn control(
        &self,
        function: FunctionId,
        span: Span,
    ) -> Result<AssignedControlRegions, Diagnostics> {
        self.control
            .get(&function)
            .copied()
            .ok_or_else(|| lowering_diagnostic(span, "function has no assigned control region"))
    }

    fn comparison_temps(
        &self,
        function: FunctionId,
        node: NodeId,
        span: Span,
    ) -> Result<&[WeavyRegionId], Diagnostics> {
        self.temps
            .get(&function)
            .and_then(|nodes| nodes.get(&node))
            .map(Vec::as_slice)
            .ok_or_else(|| {
                lowering_diagnostic(span, "comparison node has no assigned temporary regions")
            })
    }

    fn closure(
        &self,
        function: FunctionId,
        node: NodeId,
        span: Span,
    ) -> Result<(WeavyRegionId, WeavyRegionId), Diagnostics> {
        self.closures
            .get(&function)
            .and_then(|nodes| nodes.get(&node))
            .copied()
            .ok_or_else(|| lowering_diagnostic(span, "closure node has no assigned typed regions"))
    }
}

struct FunctionLayout {
    regions: BTreeMap<NodeId, FrameRegion>,
    comparison_temps: BTreeMap<NodeId, Vec<TemporaryRegion>>,
    closure_temps: BTreeMap<NodeId, (FrameRegion, FrameRegion)>,
    constant_slots: BTreeMap<NodeRef, FrameSlot>,
    scratch: Option<FrameSlot>,
    control_scratch: Option<ControlScratch>,
    frame_size: usize,
}

#[derive(Clone)]
struct TemporaryRegion {
    region: FrameRegion,
    ty: Type,
}

#[derive(Clone, Copy)]
struct ControlScratch {
    expected: FrameSlot,
    condition: FrameSlot,
}

impl FunctionLayout {
    fn build(
        function: FunctionId,
        nodes: &[Node],
        constants: &BTreeSet<NodeRef>,
        span: Span,
    ) -> Result<Self, Diagnostics> {
        let mut regions = BTreeMap::new();
        let mut next_word = 0usize;
        for node in nodes {
            let width = node.ty.word_width().ok_or_else(|| {
                lowering_diagnostic(
                    node.span,
                    &format!("{} has no island-interior representation", node.ty.name()),
                )
            })?;
            let words = FrameWords::from_usize(width)
                .ok_or_else(|| lowering_diagnostic(node.span, "VIR value width overflow"))?;
            let region = FrameRegion::for_words(next_word, words)
                .ok_or_else(|| lowering_diagnostic(node.span, "Weavy frame region overflow"))?;
            if regions.insert(node.id, region).is_some() {
                return Err(lowering_diagnostic(node.span, "duplicate VIR node id"));
            }
            next_word = next_word
                .checked_add(width)
                .ok_or_else(|| lowering_diagnostic(node.span, "function frame size overflow"))?;
        }

        let needs_scratch = nodes.iter().any(|node| {
            matches!(node.op, Op::Compare)
                || (matches!(node.op, Op::Eq | Op::Ne)
                    && node
                        .inputs
                        .first()
                        .and_then(|input| regions.get(input))
                        .is_some_and(|region| region.words().as_usize() > 1))
        });
        let scratch = if needs_scratch {
            let slot = FrameSlot::for_word(next_word)
                .ok_or_else(|| lowering_diagnostic(span, "Weavy scratch offset overflow"))?;
            next_word = next_word
                .checked_add(FrameWords::ONE.as_usize())
                .ok_or_else(|| lowering_diagnostic(span, "function frame size overflow"))?;
            Some(slot)
        } else {
            None
        };
        let control_scratch = if nodes.iter().any(|node| matches!(node.op, Op::Match { .. })) {
            let expected = FrameSlot::for_word(next_word)
                .ok_or_else(|| lowering_diagnostic(span, "Weavy control offset overflow"))?;
            next_word = next_word
                .checked_add(FrameWords::ONE.as_usize())
                .ok_or_else(|| lowering_diagnostic(span, "function frame size overflow"))?;
            let condition = FrameSlot::for_word(next_word)
                .ok_or_else(|| lowering_diagnostic(span, "Weavy control offset overflow"))?;
            next_word = next_word
                .checked_add(FrameWords::ONE.as_usize())
                .ok_or_else(|| lowering_diagnostic(span, "function frame size overflow"))?;
            Some(ControlScratch {
                expected,
                condition,
            })
        } else {
            None
        };
        let mut comparison_temps = BTreeMap::new();
        for node in nodes
            .iter()
            .filter(|node| matches!(node.op, Op::Eq | Op::Ne | Op::Compare))
        {
            let mut types = if matches!(node.op, Op::Eq | Op::Ne) {
                vec![Type::Int, Type::Int, Type::Int]
            } else if matches!(node.op, Op::Compare) {
                vec![Type::Int, Type::Int]
            } else {
                Vec::new()
            };
            let operand = node
                .inputs
                .first()
                .and_then(|input| nodes.iter().find(|candidate| candidate.id == *input))
                .ok_or_else(|| {
                    lowering_diagnostic(node.span, "comparison node has no first operand")
                })?;
            comparison_temporary_types(&operand.ty, &mut types);
            let mut temps = Vec::with_capacity(types.len());
            for ty in types {
                let words = type_words(&ty, node.span)?;
                let region = FrameRegion::for_words(next_word, words).ok_or_else(|| {
                    lowering_diagnostic(node.span, "comparison temporary region overflow")
                })?;
                next_word = next_word.checked_add(words.as_usize()).ok_or_else(|| {
                    lowering_diagnostic(node.span, "function frame size overflow")
                })?;
                temps.push(TemporaryRegion { region, ty });
            }
            comparison_temps.insert(node.id, temps);
        }

        let mut constant_slots = BTreeMap::new();
        let mut closure_temps = BTreeMap::new();
        for node in nodes
            .iter()
            .filter(|node| matches!(node.op, Op::Closure(_)))
        {
            let callee = FrameRegion::for_words(next_word, FrameWords::ONE).ok_or_else(|| {
                lowering_diagnostic(node.span, "closure callee temporary region overflow")
            })?;
            next_word = next_word
                .checked_add(1)
                .ok_or_else(|| lowering_diagnostic(node.span, "function frame size overflow"))?;
            let environment =
                FrameRegion::for_words(next_word, FrameWords::ONE).ok_or_else(|| {
                    lowering_diagnostic(node.span, "closure environment temporary region overflow")
                })?;
            next_word = next_word
                .checked_add(1)
                .ok_or_else(|| lowering_diagnostic(node.span, "function frame size overflow"))?;
            closure_temps.insert(node.id, (callee, environment));
        }
        for &constant in constants {
            let slot = if constant.function == function {
                let node = nodes
                    .iter()
                    .find(|node| node.id == constant.node)
                    .ok_or_else(|| {
                        lowering_diagnostic(span, "closure names a missing local constant")
                    })?;
                if !matches!(node.op, Op::String(_)) {
                    return Err(lowering_diagnostic(
                        node.span,
                        "closure constant is not a String node",
                    ));
                }
                regions
                    .get(&constant.node)
                    .copied()
                    .ok_or_else(|| {
                        lowering_diagnostic(node.span, "String constant has no frame region")
                    })?
                    .start()
            } else {
                let slot = FrameSlot::for_word(next_word)
                    .ok_or_else(|| lowering_diagnostic(span, "closure constant offset overflow"))?;
                next_word = next_word
                    .checked_add(1)
                    .ok_or_else(|| lowering_diagnostic(span, "function frame size overflow"))?;
                slot
            };
            constant_slots.insert(constant, slot);
        }
        let frame_size = FrameSlot::frame_size(next_word)
            .ok_or_else(|| lowering_diagnostic(span, "function frame size overflow"))?;
        Ok(Self {
            regions,
            comparison_temps,
            closure_temps,
            constant_slots,
            scratch,
            control_scratch,
            frame_size,
        })
    }

    fn region(&self, node: NodeId, span: Span) -> Result<FrameRegion, Diagnostics> {
        self.regions
            .get(&node)
            .copied()
            .ok_or_else(|| lowering_diagnostic(span, "VIR node has no frame region"))
    }

    fn constant_slot(&self, constant: NodeRef, span: Span) -> Result<FrameSlot, Diagnostics> {
        self.constant_slots
            .get(&constant)
            .copied()
            .ok_or_else(|| lowering_diagnostic(span, "function closure is missing a constant"))
    }

    fn comparison_temps(
        &self,
        node: NodeId,
        span: Span,
    ) -> Result<&[TemporaryRegion], Diagnostics> {
        self.comparison_temps
            .get(&node)
            .map(Vec::as_slice)
            .ok_or_else(|| lowering_diagnostic(span, "comparison node has no temporary regions"))
    }

    fn closure_temps(
        &self,
        node: NodeId,
        span: Span,
    ) -> Result<(FrameRegion, FrameRegion), Diagnostics> {
        self.closure_temps
            .get(&node)
            .copied()
            .ok_or_else(|| lowering_diagnostic(span, "closure node has no typed temporary regions"))
    }
}

fn comparison_temporary_types(ty: &Type, out: &mut Vec<Type>) {
    match ty {
        Type::Tuple(fields) => {
            for field in fields {
                out.push(field.clone());
                out.push(field.clone());
                comparison_temporary_types(field, out);
            }
        }
        Type::Record(record) => {
            for field in &record.fields {
                out.push(field.ty.clone());
                out.push(field.ty.clone());
                comparison_temporary_types(&field.ty, out);
            }
        }
        Type::Enum(enumeration) => {
            for variant in &enumeration.variants {
                let fields: Vec<&Type> = match &variant.payload {
                    VariantPayload::Unit => Vec::new(),
                    VariantPayload::Tuple(fields) => fields.iter().collect(),
                    VariantPayload::Record(fields) => {
                        fields.iter().map(|field| &field.ty).collect()
                    }
                };
                for field in fields {
                    out.push(field.clone());
                    out.push(field.clone());
                    comparison_temporary_types(field, out);
                }
            }
        }
        Type::Bool
        | Type::Int
        | Type::Check
        | Type::String
        | Type::Array(_)
        | Type::Function { .. }
        | Type::StreamCheck => {}
    }
}

#[derive(Clone, Copy)]
struct CodeLabel(usize);

enum PendingOp {
    Concrete(WeavyOp),
    Jump(CodeLabel),
    JumpIfZero { value: u32, target: CodeLabel },
}

struct PendingInstruction {
    op: PendingOp,
    source: Option<NodeRef>,
}

struct CodeBuilder {
    ops: Vec<PendingInstruction>,
    labels: Vec<Option<usize>>,
    current_source: Option<NodeRef>,
}

impl CodeBuilder {
    fn with_capacity(capacity: usize) -> Self {
        Self {
            ops: Vec::with_capacity(capacity),
            labels: Vec::new(),
            current_source: None,
        }
    }

    fn swap_source(&mut self, source: Option<NodeRef>) -> Option<NodeRef> {
        std::mem::replace(&mut self.current_source, source)
    }

    fn push(&mut self, op: WeavyOp) {
        self.ops.push(PendingInstruction {
            op: PendingOp::Concrete(op),
            source: self.current_source,
        });
    }

    fn extend(&mut self, ops: impl IntoIterator<Item = WeavyOp>) {
        self.ops
            .extend(ops.into_iter().map(|op| PendingInstruction {
                op: PendingOp::Concrete(op),
                source: self.current_source,
            }));
    }

    fn label(&mut self) -> CodeLabel {
        let label = CodeLabel(self.labels.len());
        self.labels.push(None);
        label
    }

    fn bind(&mut self, label: CodeLabel, span: Span) -> Result<(), Diagnostics> {
        let instruction = self.ops.len();
        let target = self
            .labels
            .get_mut(label.0)
            .ok_or_else(|| lowering_diagnostic(span, "unknown Weavy code label"))?;
        if target.replace(instruction).is_some() {
            return Err(lowering_diagnostic(span, "duplicate Weavy code label"));
        }
        Ok(())
    }

    fn jump(&mut self, target: CodeLabel) {
        self.ops.push(PendingInstruction {
            op: PendingOp::Jump(target),
            source: self.current_source,
        });
    }

    fn jump_if_zero(&mut self, value: FrameSlot, target: CodeLabel) {
        self.ops.push(PendingInstruction {
            op: PendingOp::JumpIfZero {
                value: value.byte_offset(),
                target,
            },
            source: self.current_source,
        });
    }

    fn finish(self, span: Span) -> Result<(Vec<WeavyOp>, Vec<NodeRef>), Diagnostics> {
        let labels = self.labels;
        let mut code = Vec::with_capacity(self.ops.len());
        let mut pc_nodes = Vec::with_capacity(self.ops.len());
        for instruction in self.ops {
            let source = instruction.source.ok_or_else(|| {
                lowering_diagnostic(span, "Weavy instruction has no owning VIR node")
            })?;
            let op = match instruction.op {
                PendingOp::Concrete(op) => op,
                PendingOp::Jump(label) => WeavyOp::Jump {
                    target: Self::resolve(&labels, label, span)?,
                },
                PendingOp::JumpIfZero { value, target } => WeavyOp::JumpIfZero {
                    value,
                    target: Self::resolve(&labels, target, span)?,
                },
            };
            code.push(op);
            pc_nodes.push(source);
        }
        Ok((code, pc_nodes))
    }

    fn resolve(labels: &[Option<usize>], label: CodeLabel, span: Span) -> Result<u32, Diagnostics> {
        let target = labels
            .get(label.0)
            .and_then(|target| *target)
            .ok_or_else(|| lowering_diagnostic(span, "unbound Weavy code label"))?;
        u32::try_from(target).map_err(|_| lowering_diagnostic(span, "Weavy jump target overflow"))
    }
}

struct LoweredWeavyFunction {
    function: WeavyFn,
    pc_nodes: Vec<NodeRef>,
}

fn lower_vir_function(
    function: FunctionId,
    nodes: &[Node],
    parameters: &[crate::vir::Parameter],
    output: NodeId,
    context: &LoweringContext<'_>,
    constants: &mut BTreeMap<NodeRef, PendingValueConstant>,
) -> Result<LoweredWeavyFunction, Diagnostics> {
    let layout = context
        .layouts
        .get(&function)
        .ok_or_else(|| lowering_diagnostic(Span { start: 0, end: 0 }, "missing function layout"))?;
    let function_context = FunctionLoweringContext {
        id: function,
        parameters,
        layout,
    };
    let output_node = nodes
        .iter()
        .find(|node| node.id == output)
        .ok_or_else(|| lowering_diagnostic(Span { start: 0, end: 0 }, "missing function output"))?;
    let nodes_by_id = nodes
        .iter()
        .map(|node| (node.id, node))
        .collect::<BTreeMap<_, _>>();
    let node_ids = nodes.iter().map(|node| node.id).collect::<Vec<_>>();
    let mut values = BTreeMap::new();
    let mut code = CodeBuilder::with_capacity(nodes.len().saturating_mul(2) + 1);
    {
        let sequence = SequenceContext {
            nodes: &nodes_by_id,
            function: &function_context,
            lowering: context,
        };
        let mut outputs = SequenceOutputs {
            constants,
            code: &mut code,
        };
        lower_node_sequence(&node_ids, &mut values, &sequence, &mut outputs, None)?;
    }
    let output_region = values
        .get(&output)
        .map(|value| value.region)
        .ok_or_else(|| {
            lowering_diagnostic(output_node.span, "function output has no frame region")
        })?;
    let previous_source = code.swap_source(Some(NodeRef {
        function,
        node: output,
    }));
    code.push(WeavyOp::Ret {
        src: output_region.start().byte_offset(),
        size: output_region
            .byte_size()
            .ok_or_else(|| lowering_diagnostic(output_node.span, "return size overflow"))?,
    });
    code.swap_source(previous_source);
    let (code, pc_nodes) = code.finish(output_node.span)?;
    Ok(LoweredWeavyFunction {
        function: WeavyFn {
            frame: Layout {
                size: layout.frame_size,
                align: FrameSlot::word_align(),
            },
            code,
        },
        pc_nodes,
    })
}

struct SequenceContext<'nodes, 'function, 'lowering> {
    nodes: &'nodes BTreeMap<NodeId, &'nodes Node>,
    function: &'function FunctionLoweringContext<'function>,
    lowering: &'lowering LoweringContext<'lowering>,
}

struct SequenceOutputs<'constants, 'code> {
    constants: &'constants mut BTreeMap<NodeRef, PendingValueConstant>,
    code: &'code mut CodeBuilder,
}

fn lower_node_sequence(
    node_ids: &[NodeId],
    values: &mut BTreeMap<NodeId, LoweredSlot>,
    sequence: &SequenceContext<'_, '_, '_>,
    outputs: &mut SequenceOutputs<'_, '_>,
    active_variant: Option<u32>,
) -> Result<(), Diagnostics> {
    let mut controlled = BTreeSet::new();
    for node_id in node_ids {
        let node = sequence.nodes.get(node_id).copied().ok_or_else(|| {
            lowering_diagnostic(
                Span { start: 0, end: 0 },
                "control region names a missing node",
            )
        })?;
        match &node.op {
            Op::Match { arms } => {
                for arm in arms {
                    controlled.extend(arm.nodes.iter().copied());
                }
            }
            Op::If {
                consequent,
                alternative,
            } => {
                controlled.extend(consequent.nodes.iter().copied());
                controlled.extend(alternative.nodes.iter().copied());
            }
            Op::OrderedMatch { arms, fallback } => {
                for arm in arms {
                    controlled.extend(arm.condition.nodes.iter().copied());
                    controlled.extend(arm.body.nodes.iter().copied());
                }
                controlled.extend(fallback.nodes.iter().copied());
            }
            _ => {}
        }
    }

    for node_id in node_ids {
        if controlled.contains(node_id) {
            continue;
        }
        let node =
            sequence.nodes.get(node_id).copied().ok_or_else(|| {
                lowering_diagnostic(Span { start: 0, end: 0 }, "missing VIR node")
            })?;
        if values.contains_key(&node.id) {
            return Err(lowering_diagnostic(node.span, "duplicate VIR node id"));
        }
        let dst = sequence.function.layout.region(node.id, node.span)?;
        let dst_region_id =
            sequence
                .lowering
                .regions
                .node(sequence.function.id, node.id, node.span)?;
        let node_ref = NodeRef {
            function: sequence.function.id,
            node: node.id,
        };
        let trace_id = sequence
            .lowering
            .trace_ids
            .get(&node_ref)
            .copied()
            .ok_or_else(|| lowering_diagnostic(node.span, "VIR node has no trace attribution"))?;
        let previous_source = outputs.code.swap_source(Some(node_ref));
        outputs.code.push(WeavyOp::Trace { id: trace_id });
        let representation = match &node.op {
            Op::Match { .. } => {
                lower_match_node(node, dst, dst_region_id, values, sequence, outputs)?
            }
            Op::If { .. } => lower_if_node(
                node,
                dst,
                dst_region_id,
                values,
                sequence,
                outputs,
                active_variant,
            )?,
            Op::OrderedMatch { .. } => lower_ordered_match_node(
                node,
                dst,
                dst_region_id,
                values,
                sequence,
                outputs,
                active_variant,
            )?,
            Op::Compare => lower_compare_node(node, dst, dst_region_id, values, sequence, outputs)?,
            Op::Eq | Op::Ne => lower_equality_node(node, dst, values, sequence, outputs)?,
            _ => {
                let lowered = lower_node(
                    node,
                    dst,
                    dst_region_id,
                    values,
                    sequence.function,
                    sequence.lowering,
                    outputs.constants,
                    active_variant,
                )?;
                outputs.code.extend(lowered.ops);
                lowered.representation
            }
        };
        outputs.code.swap_source(previous_source);
        values.insert(
            node.id,
            LoweredSlot {
                region: dst,
                region_id: dst_region_id,
                ty: node.ty.clone(),
                representation,
            },
        );
    }
    Ok(())
}

fn lower_ordered_match_node(
    node: &Node,
    dst: FrameRegion,
    dst_region: WeavyRegionId,
    values: &BTreeMap<NodeId, LoweredSlot>,
    sequence: &SequenceContext<'_, '_, '_>,
    outputs: &mut SequenceOutputs<'_, '_>,
    active_variant: Option<u32>,
) -> Result<ValueRepresentation, Diagnostics> {
    let Op::OrderedMatch { arms, fallback } = &node.op else {
        return Err(lowering_diagnostic(
            node.span,
            "non-OrderedMatch node reached structured ordered-match lowering",
        ));
    };
    require_input_count(node, 1)?;
    let _ = input_value(node, values, 0)?;
    let result_representation = representation_for_type(&node.ty, node.span)?;
    let end = outputs.code.label();

    for arm in arms {
        let mut condition_values = values.clone();
        lower_node_sequence(
            &arm.condition.nodes,
            &mut condition_values,
            sequence,
            outputs,
            active_variant,
        )?;
        let condition = condition_values.get(&arm.condition.output).ok_or_else(|| {
            lowering_diagnostic(node.span, "ordered-match condition has no lowered value")
        })?;
        require_value(node, condition, &Type::Bool, ValueRepresentation::Word)?;
        let next = outputs.code.label();
        outputs.code.jump_if_zero(condition.region.start(), next);

        let mut body_values = condition_values;
        lower_node_sequence(
            &arm.body.nodes,
            &mut body_values,
            sequence,
            outputs,
            active_variant,
        )?;
        let body = body_values.get(&arm.body.output).ok_or_else(|| {
            lowering_diagnostic(node.span, "ordered-match body has no lowered value")
        })?;
        require_value(node, body, &node.ty, result_representation)?;
        outputs
            .code
            .extend(copy_lowered_value(node, body, dst, dst_region)?);
        outputs.code.jump(end);
        outputs.code.bind(next, node.span)?;
    }

    let mut fallback_values = values.clone();
    lower_node_sequence(
        &fallback.nodes,
        &mut fallback_values,
        sequence,
        outputs,
        active_variant,
    )?;
    let fallback = fallback_values.get(&fallback.output).ok_or_else(|| {
        lowering_diagnostic(node.span, "ordered-match fallback has no lowered value")
    })?;
    require_value(node, fallback, &node.ty, result_representation)?;
    outputs
        .code
        .extend(copy_lowered_value(node, fallback, dst, dst_region)?);
    outputs.code.bind(end, node.span)?;

    Ok(result_representation)
}

fn lower_if_node(
    node: &Node,
    dst: FrameRegion,
    dst_region: WeavyRegionId,
    values: &BTreeMap<NodeId, LoweredSlot>,
    sequence: &SequenceContext<'_, '_, '_>,
    outputs: &mut SequenceOutputs<'_, '_>,
    active_variant: Option<u32>,
) -> Result<ValueRepresentation, Diagnostics> {
    let Op::If {
        consequent,
        alternative,
    } = &node.op
    else {
        return Err(lowering_diagnostic(
            node.span,
            "non-If node reached structured conditional lowering",
        ));
    };
    require_input_count(node, 1)?;
    let condition = input_value(node, values, 0)?;
    require_value(node, &condition, &Type::Bool, ValueRepresentation::Word)?;
    let result_representation = representation_for_type(&node.ty, node.span)?;

    let alternative_label = outputs.code.label();
    let end = outputs.code.label();
    outputs
        .code
        .jump_if_zero(condition.region.start(), alternative_label);

    let mut consequent_values = values.clone();
    lower_node_sequence(
        &consequent.nodes,
        &mut consequent_values,
        sequence,
        outputs,
        active_variant,
    )?;
    let consequent_output = consequent_values.get(&consequent.output).ok_or_else(|| {
        lowering_diagnostic(node.span, "if consequent output has no lowered value")
    })?;
    require_value(node, consequent_output, &node.ty, result_representation)?;
    outputs.code.extend(copy_lowered_value(
        node,
        consequent_output,
        dst,
        dst_region,
    )?);
    outputs.code.jump(end);

    outputs.code.bind(alternative_label, node.span)?;
    let mut alternative_values = values.clone();
    lower_node_sequence(
        &alternative.nodes,
        &mut alternative_values,
        sequence,
        outputs,
        active_variant,
    )?;
    let alternative_output = alternative_values.get(&alternative.output).ok_or_else(|| {
        lowering_diagnostic(node.span, "if alternative output has no lowered value")
    })?;
    require_value(node, alternative_output, &node.ty, result_representation)?;
    outputs.code.extend(copy_lowered_value(
        node,
        alternative_output,
        dst,
        dst_region,
    )?);
    outputs.code.bind(end, node.span)?;

    Ok(result_representation)
}

fn lower_match_node(
    node: &Node,
    dst: FrameRegion,
    dst_region: WeavyRegionId,
    values: &BTreeMap<NodeId, LoweredSlot>,
    sequence: &SequenceContext<'_, '_, '_>,
    outputs: &mut SequenceOutputs<'_, '_>,
) -> Result<ValueRepresentation, Diagnostics> {
    let Op::Match { arms } = &node.op else {
        return Err(lowering_diagnostic(
            node.span,
            "non-Match node reached structured match lowering",
        ));
    };
    require_input_count(node, 1)?;
    let scrutinee = input_value(node, values, 0)?;
    require_value(
        node,
        &scrutinee,
        &scrutinee.ty,
        ValueRepresentation::InlineComposite,
    )?;
    let enum_layout = EnumLayout::for_type(&scrutinee.ty, node.span)?;
    if enum_layout.words != scrutinee.region.words() {
        return Err(lowering_diagnostic(
            node.span,
            "match scrutinee has the wrong enum frame width",
        ));
    }
    if arms.is_empty() {
        return Err(lowering_diagnostic(node.span, "Match has no arms"));
    }
    let mut seen = BTreeSet::new();
    for arm in arms {
        enum_layout.variant(arm.variant, node.span)?;
        if !seen.insert(arm.variant) {
            return Err(lowering_diagnostic(
                node.span,
                "Match repeats an enum variant",
            ));
        }
    }
    if seen.len() != enum_layout.enumeration.variants.len() {
        return Err(lowering_diagnostic(
            node.span,
            "Match is not exhaustive over its enum type",
        ));
    }

    let result_representation = representation_for_type(&node.ty, node.span)?;
    let control = sequence
        .lowering
        .regions
        .control(sequence.function.id, node.span)?;
    let scratch = sequence
        .function
        .layout
        .control_scratch
        .ok_or_else(|| lowering_diagnostic(node.span, "Match has no control scratch region"))?;
    let end = outputs.code.label();
    for arm in arms {
        let next = outputs.code.label();
        outputs.code.push(WeavyOp::EnumIsVariant {
            dst: control.condition,
            value: scrutinee.region_id,
            variant: arm.variant,
        });
        outputs.code.jump_if_zero(scratch.condition, next);

        let mut arm_values = values.clone();
        lower_node_sequence(
            &arm.nodes,
            &mut arm_values,
            sequence,
            outputs,
            Some(arm.variant),
        )?;
        let output = arm_values.get(&arm.output).ok_or_else(|| {
            lowering_diagnostic(node.span, "match arm output has no lowered value")
        })?;
        require_value(node, output, &node.ty, result_representation)?;
        outputs
            .code
            .extend(copy_lowered_value(node, output, dst, dst_region)?);
        outputs.code.jump(end);
        outputs.code.bind(next, node.span)?;
    }
    outputs.code.bind(end, node.span)?;
    Ok(result_representation)
}

#[derive(Clone, Copy)]
enum CompareLeafKind {
    SignedWord,
    ValueBytes,
}

#[derive(Clone, Copy)]
struct CompareLeaf {
    kind: CompareLeafKind,
    a: FrameSlot,
    b: FrameSlot,
}

// r[related machine.value.structural-order]
fn lower_compare_node(
    node: &Node,
    dst: FrameRegion,
    dst_region: WeavyRegionId,
    values: &BTreeMap<NodeId, LoweredSlot>,
    sequence: &SequenceContext<'_, '_, '_>,
    outputs: &mut SequenceOutputs<'_, '_>,
) -> Result<ValueRepresentation, Diagnostics> {
    require_node_type(node, Type::ordering())?;
    let (a, b) = binary_values(node, values)?;
    if a.ty != b.ty {
        return Err(lowering_diagnostic(
            node.span,
            "comparison operands have different VIR types",
        ));
    }
    if !a.ty.structural_order_is_defined() {
        return Err(lowering_diagnostic(
            node.span,
            "structural order is not defined for this VIR type",
        ));
    }
    let representation = representation_for_type(&a.ty, node.span)?;
    require_value(node, &a, &a.ty, representation)?;
    require_value(node, &b, &a.ty, representation)?;
    if dst.words() != FrameWords::ONE {
        return Err(lowering_diagnostic(
            node.span,
            "Ordering result does not occupy one frame word",
        ));
    }

    let mut temps = TemporaryCursor::new(
        sequence.function.layout,
        sequence.lowering.regions,
        sequence.function.id,
        node,
    )?;
    let ordering = temps.take(&Type::Int, node.span)?;
    let test = temps.take(&Type::Int, node.span)?;
    let mut leaves = Vec::new();
    collect_typed_compare_leaves(node, &a.ty, &a, &b, &mut temps, outputs.code, &mut leaves)?;
    let dst = ordering.region.start();
    let end = outputs.code.label();
    if leaves.is_empty() {
        outputs.code.push(WeavyOp::ConstI64 {
            dst: dst.byte_offset(),
            value: i64::from(ORDERING_EQUAL_VARIANT),
        });
    }
    for (index, leaf) in leaves.iter().enumerate() {
        let is_last = index + 1 == leaves.len();
        match leaf.kind {
            CompareLeafKind::SignedWord => {
                let not_less = outputs.code.label();
                outputs.code.push(WeavyOp::LtI64 {
                    dst: dst.byte_offset(),
                    a: leaf.a.byte_offset(),
                    b: leaf.b.byte_offset(),
                });
                outputs.code.jump_if_zero(dst, not_less);
                outputs.code.push(WeavyOp::ConstI64 {
                    dst: dst.byte_offset(),
                    value: i64::from(ORDERING_LESS_VARIANT),
                });
                outputs.code.jump(end);
                outputs.code.bind(not_less, node.span)?;

                let equal = outputs.code.label();
                outputs.code.push(WeavyOp::GtI64 {
                    dst: dst.byte_offset(),
                    a: leaf.a.byte_offset(),
                    b: leaf.b.byte_offset(),
                });
                outputs.code.jump_if_zero(dst, equal);
                outputs.code.push(WeavyOp::ConstI64 {
                    dst: dst.byte_offset(),
                    value: i64::from(ORDERING_GREATER_VARIANT),
                });
                outputs.code.jump(end);
                outputs.code.bind(equal, node.span)?;
                if is_last {
                    outputs.code.push(WeavyOp::ConstI64 {
                        dst: dst.byte_offset(),
                        value: i64::from(ORDERING_EQUAL_VARIANT),
                    });
                }
            }
            CompareLeafKind::ValueBytes => {
                outputs.code.push(WeavyOp::CompareValueBytes {
                    dst: dst.byte_offset(),
                    a: leaf.a.byte_offset(),
                    b: leaf.b.byte_offset(),
                });
                if !is_last {
                    let scratch = sequence.function.layout.scratch.ok_or_else(|| {
                        lowering_diagnostic(node.span, "comparison has no scratch word")
                    })?;
                    outputs.code.push(WeavyOp::ConstI64 {
                        dst: scratch.byte_offset(),
                        value: i64::from(ORDERING_EQUAL_VARIANT),
                    });
                    outputs.code.push(WeavyOp::EqI64 {
                        dst: scratch.byte_offset(),
                        a: dst.byte_offset(),
                        b: scratch.byte_offset(),
                    });
                    outputs.code.jump_if_zero(scratch, end);
                }
            }
        }
    }
    outputs.code.bind(end, node.span)?;
    let not_less = outputs.code.label();
    let not_equal = outputs.code.label();
    let done = outputs.code.label();
    outputs.code.push(WeavyOp::ConstI64 {
        dst: test.region.start().byte_offset(),
        value: i64::from(ORDERING_LESS_VARIANT),
    });
    outputs.code.push(WeavyOp::EqI64 {
        dst: test.region.start().byte_offset(),
        a: dst.byte_offset(),
        b: test.region.start().byte_offset(),
    });
    outputs.code.jump_if_zero(test.region.start(), not_less);
    outputs.code.push(WeavyOp::EnumConstruct {
        dst: dst_region,
        variant: ORDERING_LESS_VARIANT as u32,
        fields: Vec::new(),
    });
    outputs.code.jump(done);
    outputs.code.bind(not_less, node.span)?;
    outputs.code.push(WeavyOp::ConstI64 {
        dst: test.region.start().byte_offset(),
        value: i64::from(ORDERING_EQUAL_VARIANT),
    });
    outputs.code.push(WeavyOp::EqI64 {
        dst: test.region.start().byte_offset(),
        a: dst.byte_offset(),
        b: test.region.start().byte_offset(),
    });
    outputs.code.jump_if_zero(test.region.start(), not_equal);
    outputs.code.push(WeavyOp::EnumConstruct {
        dst: dst_region,
        variant: ORDERING_EQUAL_VARIANT as u32,
        fields: Vec::new(),
    });
    outputs.code.jump(done);
    outputs.code.bind(not_equal, node.span)?;
    outputs.code.push(WeavyOp::EnumConstruct {
        dst: dst_region,
        variant: ORDERING_GREATER_VARIANT as u32,
        fields: Vec::new(),
    });
    outputs.code.bind(done, node.span)?;
    temps.finish(node.span)?;
    Ok(ValueRepresentation::InlineComposite)
}

fn collect_typed_compare_leaves(
    node: &Node,
    ty: &Type,
    a: &LoweredSlot,
    b: &LoweredSlot,
    temps: &mut TemporaryCursor<'_>,
    code: &mut CodeBuilder,
    leaves: &mut Vec<CompareLeaf>,
) -> Result<(), Diagnostics> {
    match ty {
        Type::Bool | Type::Int => leaves.push(CompareLeaf {
            kind: CompareLeafKind::SignedWord,
            a: a.region.start(),
            b: b.region.start(),
        }),
        Type::String => leaves.push(CompareLeaf {
            kind: CompareLeafKind::ValueBytes,
            a: a.region.start(),
            b: b.region.start(),
        }),
        Type::Tuple(fields) => {
            for (index, field) in fields.iter().enumerate() {
                let left = temps.take(field, node.span)?;
                let right = temps.take(field, node.span)?;
                let field_index = u32::try_from(index)
                    .map_err(|_| lowering_diagnostic(node.span, "tuple field index overflow"))?;
                code.push(WeavyOp::ProductProject {
                    dst: left.region_id,
                    product: a.region_id,
                    field: field_index,
                });
                code.push(WeavyOp::ProductProject {
                    dst: right.region_id,
                    product: b.region_id,
                    field: field_index,
                });
                collect_typed_compare_leaves(node, field, &left, &right, temps, code, leaves)?;
            }
        }
        Type::Record(record) => {
            for (index, field) in record.fields.iter().enumerate() {
                let left = temps.take(&field.ty, node.span)?;
                let right = temps.take(&field.ty, node.span)?;
                let field_index = u32::try_from(index)
                    .map_err(|_| lowering_diagnostic(node.span, "record field index overflow"))?;
                code.push(WeavyOp::ProductProject {
                    dst: left.region_id,
                    product: a.region_id,
                    field: field_index,
                });
                code.push(WeavyOp::ProductProject {
                    dst: right.region_id,
                    product: b.region_id,
                    field: field_index,
                });
                collect_typed_compare_leaves(node, &field.ty, &left, &right, temps, code, leaves)?;
            }
        }
        Type::Enum(_) => {
            return Err(lowering_diagnostic(
                node.span,
                "enum order needs variant-directed typed lowering",
            ));
        }
        Type::Array(_) => {
            return Err(lowering_diagnostic(
                node.span,
                "array comparison lowering is not implemented",
            ));
        }
        Type::Function { .. } => {
            return Err(lowering_diagnostic(
                node.span,
                "function comparison requires stable closure identity",
            ));
        }
        Type::Check | Type::StreamCheck => {
            return Err(lowering_diagnostic(
                node.span,
                "comparison reached a non-orderable VIR type",
            ));
        }
    }
    Ok(())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ValueRepresentation {
    Word,
    RealizedHandle,
    InlineComposite,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct LoweredSlot {
    region: FrameRegion,
    region_id: WeavyRegionId,
    ty: Type,
    representation: ValueRepresentation,
}

struct LoweredNode {
    ops: Vec<WeavyOp>,
    representation: ValueRepresentation,
}

struct TemporaryCursor<'a> {
    regions: &'a [TemporaryRegion],
    ids: &'a [WeavyRegionId],
    next: usize,
}

impl<'a> TemporaryCursor<'a> {
    fn new(
        layout: &'a FunctionLayout,
        assignments: &'a RegionAssignments,
        function: FunctionId,
        node: &Node,
    ) -> Result<Self, Diagnostics> {
        let regions = layout.comparison_temps(node.id, node.span)?;
        let ids = assignments.comparison_temps(function, node.id, node.span)?;
        if regions.len() != ids.len() {
            return Err(lowering_diagnostic(
                node.span,
                "comparison temporary regions and contracts differ",
            ));
        }
        Ok(Self {
            regions,
            ids,
            next: 0,
        })
    }

    fn take(&mut self, ty: &Type, span: Span) -> Result<LoweredSlot, Diagnostics> {
        let region = self.regions.get(self.next).ok_or_else(|| {
            lowering_diagnostic(span, "comparison lowering exhausted its temporary regions")
        })?;
        let region_id = *self.ids.get(self.next).ok_or_else(|| {
            lowering_diagnostic(span, "comparison temporary region has no contract id")
        })?;
        self.next += 1;
        if &region.ty != ty {
            return Err(lowering_diagnostic(
                span,
                "comparison temporary type does not match projected field",
            ));
        }
        Ok(LoweredSlot {
            region: region.region,
            region_id,
            ty: ty.clone(),
            representation: representation_for_type(ty, span)?,
        })
    }

    fn finish(self, span: Span) -> Result<(), Diagnostics> {
        if self.next != self.regions.len() {
            return Err(lowering_diagnostic(
                span,
                "comparison lowering did not consume its permanent temporaries",
            ));
        }
        Ok(())
    }
}

struct FunctionLoweringContext<'a> {
    id: FunctionId,
    parameters: &'a [crate::vir::Parameter],
    layout: &'a FunctionLayout,
}

fn lower_node(
    node: &Node,
    dst: FrameRegion,
    dst_region_id: WeavyRegionId,
    values: &BTreeMap<NodeId, LoweredSlot>,
    function: &FunctionLoweringContext<'_>,
    context: &LoweringContext<'_>,
    constants: &mut BTreeMap<NodeRef, PendingValueConstant>,
    _active_variant: Option<u32>,
) -> Result<LoweredNode, Diagnostics> {
    let dst_region = dst;
    let dst_slot = dst.start();
    let dst = dst_slot.byte_offset();
    let (ops, representation) = match &node.op {
        Op::Bool(value) => {
            require_input_count(node, 0)?;
            require_node_type(node, Type::Bool)?;
            (
                vec![WeavyOp::ConstI64 {
                    dst,
                    value: i64::from(*value),
                }],
                ValueRepresentation::Word,
            )
        }
        Op::Int(value) => {
            require_input_count(node, 0)?;
            require_node_type(node, Type::Int)?;
            (
                vec![WeavyOp::ConstI64 { dst, value: *value }],
                ValueRepresentation::Word,
            )
        }
        Op::String(value) => {
            require_input_count(node, 0)?;
            require_node_type(node, Type::String)?;
            let constant = NodeRef {
                function: function.id,
                node: node.id,
            };
            if function.layout.constant_slot(constant, node.span)? != dst_slot {
                return Err(lowering_diagnostic(
                    node.span,
                    "String node does not occupy its local closure slot",
                ));
            }
            let root_layout = context
                .layouts
                .get(&context.root_function)
                .ok_or_else(|| lowering_diagnostic(node.span, "missing island root layout"))?;
            let root_slot = root_layout.constant_slot(constant, node.span)?;
            let pending = PendingValueConstant {
                node: constant,
                root_slot,
                owner_slot: dst_slot,
                store_schema: SchemaId::named("vix.String.v1"),
                bytes: value.as_bytes().to_vec(),
                span: node.span,
            };
            if let Some(previous) = constants.insert(constant, pending)
                && (previous.root_slot != root_slot
                    || previous.owner_slot != dst_slot
                    || previous.store_schema != SchemaId::named("vix.String.v1")
                    || previous.bytes != value.as_bytes())
            {
                return Err(lowering_diagnostic(
                    node.span,
                    "constant NodeRef was lowered with conflicting metadata",
                ));
            }
            (Vec::new(), ValueRepresentation::RealizedHandle)
        }
        Op::Parameter(parameter_id) => {
            require_input_count(node, 0)?;
            let parameter = function
                .parameters
                .iter()
                .find(|parameter| parameter.id == *parameter_id)
                .ok_or_else(|| {
                    lowering_diagnostic(node.span, "VIR parameter node has no declaration")
                })?;
            if parameter.node != node.id || parameter.ty != node.ty {
                return Err(lowering_diagnostic(
                    node.span,
                    "VIR parameter declaration does not match its node",
                ));
            }
            (Vec::new(), representation_for_type(&node.ty, node.span)?)
        }
        Op::Call(callee) => {
            let op = lower_call_node(node, dst_region, values, *callee, function.layout, context)?;
            (vec![op], representation_for_type(&node.ty, node.span)?)
        }
        Op::Closure(callee) => {
            require_input_count(node, 0)?;
            if !matches!(node.ty, Type::Function { .. }) || dst_region.words().as_usize() != 2 {
                return Err(lowering_diagnostic(
                    node.span,
                    "closure value does not occupy its two-word ABI",
                ));
            }
            let target = context.functions.get(callee).copied().ok_or_else(|| {
                lowering_diagnostic(node.span, "closure function is absent from the island")
            })?;
            let target_layout = context.layouts.get(callee).ok_or_else(|| {
                lowering_diagnostic(node.span, "closure function has no frame layout")
            })?;
            let Type::Function { parameter, result } = &node.ty else {
                unreachable!("closure type was checked above")
            };
            let [target_parameter] = target.parameters.as_slice() else {
                return Err(lowering_diagnostic(
                    node.span,
                    "closure function does not have exactly one parameter",
                ));
            };
            if &target_parameter.ty != parameter.as_ref()
                || &target.return_type != result.as_ref()
                || target_layout
                    .region(target_parameter.node, node.span)?
                    .start()
                    != FrameSlot::for_word(0).expect("word zero is a frame slot")
            {
                return Err(lowering_diagnostic(
                    node.span,
                    "closure function does not satisfy the callable ABI",
                ));
            }
            if !target_layout.constant_slots.is_empty() {
                return Err(lowering_diagnostic(
                    node.span,
                    "indirect closure constants require an environment",
                ));
            }
            let callee = context.function_ids.get(callee).copied().ok_or_else(|| {
                lowering_diagnostic(node.span, "closure function has no local ABI id")
            })?;
            let (callee_region, environment_region) =
                context.regions.closure(function.id, node.id, node.span)?;
            let (callee_temp, environment_temp) =
                function.layout.closure_temps(node.id, node.span)?;
            (
                vec![
                    WeavyOp::ConstI64 {
                        dst: callee_temp.start().byte_offset(),
                        value: i64::from(callee),
                    },
                    WeavyOp::ConstI64 {
                        dst: environment_temp.start().byte_offset(),
                        value: 0,
                    },
                    WeavyOp::ProductConstruct {
                        dst: dst_region_id,
                        fields: vec![
                            StructuralFieldSource {
                                field: 0,
                                source: callee_region,
                            },
                            StructuralFieldSource {
                                field: 1,
                                source: environment_region,
                            },
                        ],
                    },
                ],
                ValueRepresentation::InlineComposite,
            )
        }
        Op::CallValue => {
            let op = lower_call_value_node(node, dst_region, values)?;
            (vec![op], representation_for_type(&node.ty, node.span)?)
        }
        Op::Add | Op::Sub | Op::Mul | Op::Div => {
            require_node_type(node, Type::Int)?;
            let (a, b) = binary_values(node, values)?;
            require_value(node, &a, &Type::Int, ValueRepresentation::Word)?;
            require_value(node, &b, &Type::Int, ValueRepresentation::Word)?;
            let op = match &node.op {
                Op::Add => WeavyOp::AddI64 {
                    dst,
                    a: a.region.start().byte_offset(),
                    b: b.region.start().byte_offset(),
                },
                Op::Sub => WeavyOp::SubI64 {
                    dst,
                    a: a.region.start().byte_offset(),
                    b: b.region.start().byte_offset(),
                },
                Op::Mul => WeavyOp::MulI64 {
                    dst,
                    a: a.region.start().byte_offset(),
                    b: b.region.start().byte_offset(),
                },
                Op::Div => WeavyOp::DivI64 {
                    dst,
                    a: a.region.start().byte_offset(),
                    b: b.region.start().byte_offset(),
                },
                _ => unreachable!("matched arithmetic VIR op"),
            };
            (vec![op], ValueRepresentation::Word)
        }
        Op::Tuple => lower_aggregate_node(
            node,
            dst_region,
            dst_region_id,
            values,
            AggregateKind::Tuple,
        )?,
        Op::Record => lower_aggregate_node(
            node,
            dst_region,
            dst_region_id,
            values,
            AggregateKind::Record,
        )?,
        Op::Project { index } => {
            lower_project_node(node, dst_region, dst_region_id, values, *index)?
        }
        Op::Array => {
            return Err(lowering_diagnostic(
                node.span,
                "array literal lowering is not implemented",
            ));
        }
        Op::ArrayIndex => {
            return Err(lowering_diagnostic(
                node.span,
                "array indexing lowering is not implemented",
            ));
        }
        Op::ArrayLen => {
            return Err(lowering_diagnostic(
                node.span,
                "array length lowering is not implemented",
            ));
        }
        Op::Variant { variant } => {
            lower_variant_node(node, dst_region, dst_region_id, values, *variant)?
        }
        Op::VariantProject { variant, field } => {
            lower_variant_project_node(node, dst_region, dst_region_id, values, *variant, *field)?
        }
        Op::IsVariant { variant } => {
            require_node_type(node, Type::Bool)?;
            require_input_count(node, 1)?;
            let value = input_value(node, values, 0)?;
            let Type::Enum(enumeration) = &value.ty else {
                return Err(lowering_diagnostic(
                    node.span,
                    "variant predicate input is not an enum",
                ));
            };
            if usize::try_from(*variant)
                .ok()
                .is_none_or(|variant| variant >= enumeration.variants.len())
            {
                return Err(lowering_diagnostic(
                    node.span,
                    "variant predicate index is out of range",
                ));
            }
            require_value(
                node,
                &value,
                &value.ty,
                ValueRepresentation::InlineComposite,
            )?;
            (
                vec![WeavyOp::EnumIsVariant {
                    dst: dst_region_id,
                    value: value.region_id,
                    variant: *variant,
                }],
                ValueRepresentation::Word,
            )
        }
        Op::Match { .. } => {
            return Err(lowering_diagnostic(
                node.span,
                "structured Match reached scalar node lowering",
            ));
        }
        Op::If { .. } => {
            return Err(lowering_diagnostic(
                node.span,
                "structured If reached scalar node lowering",
            ));
        }
        Op::OrderedMatch { .. } => {
            return Err(lowering_diagnostic(
                node.span,
                "structured OrderedMatch reached scalar node lowering",
            ));
        }
        Op::Compare => {
            return Err(lowering_diagnostic(
                node.span,
                "structured Compare reached scalar node lowering",
            ));
        }
        Op::Eq | Op::Ne => {
            return Err(lowering_diagnostic(
                node.span,
                "structured equality reached scalar node lowering",
            ));
        }
        Op::Expect => {
            require_node_type(node, Type::Check)?;
            require_input_count(node, 1)?;
            let condition = input_value(node, values, 0)?;
            require_value(node, &condition, &Type::Bool, ValueRepresentation::Word)?;
            (
                vec![WeavyOp::CopyI64 {
                    dst,
                    src: condition.region.start().byte_offset(),
                }],
                ValueRepresentation::Word,
            )
        }
        Op::Yield => {
            return Err(lowering_diagnostic(
                node.span,
                "codata Yield appeared inside an island",
            ));
        }
    };
    Ok(LoweredNode {
        ops,
        representation,
    })
}

fn lower_call_node(
    node: &Node,
    dst: FrameRegion,
    values: &BTreeMap<NodeId, LoweredSlot>,
    callee: FunctionId,
    caller_layout: &FunctionLayout,
    context: &LoweringContext<'_>,
) -> Result<WeavyOp, Diagnostics> {
    let target = context.functions.get(&callee).copied().ok_or_else(|| {
        lowering_diagnostic(
            node.span,
            "called function is absent from the island closure",
        )
    })?;
    let target_output = target
        .output
        .ok_or_else(|| lowering_diagnostic(node.span, "called function has no return value"))?;
    if target.return_type != node.ty {
        return Err(lowering_diagnostic(
            node.span,
            "call result does not match the called function",
        ));
    }
    require_input_count(node, target.parameters.len())?;
    let target_layout = context
        .layouts
        .get(&callee)
        .ok_or_else(|| lowering_diagnostic(node.span, "called function has no frame layout"))?;
    if target_layout.region(target_output, node.span)?.words() != dst.words() {
        return Err(lowering_diagnostic(
            node.span,
            "call result region does not match the called function",
        ));
    }

    let mut args = Vec::with_capacity(
        target
            .parameters
            .len()
            .saturating_add(target_layout.constant_slots.len()),
    );
    for (index, parameter) in target.parameters.iter().enumerate() {
        let source = input_value(node, values, index)?;
        require_value(
            node,
            &source,
            &parameter.ty,
            representation_for_type(&parameter.ty, node.span)?,
        )?;
        let parameter_region = target_layout.region(parameter.node, node.span)?;
        if source.region.words() != parameter_region.words() {
            return Err(lowering_diagnostic(
                node.span,
                "call argument region does not match its parameter",
            ));
        }
        args.push(ArgCopy {
            src: source.region.start().byte_offset(),
            dst: parameter_region.start().byte_offset(),
            size: source
                .region
                .byte_size()
                .ok_or_else(|| lowering_diagnostic(node.span, "argument size overflow"))?,
        });
    }
    for (&constant, &target_slot) in &target_layout.constant_slots {
        let source_slot = caller_layout.constant_slot(constant, node.span)?;
        args.push(ArgCopy {
            src: source_slot.byte_offset(),
            dst: target_slot.byte_offset(),
            size: FrameSlot::word_size(),
        });
    }

    let callee = context
        .function_ids
        .get(&callee)
        .copied()
        .ok_or_else(|| lowering_diagnostic(node.span, "called function has no local ABI id"))?;
    Ok(WeavyOp::Call {
        callee: WeavyFnId(callee),
        args,
        ret: dst.start().byte_offset(),
    })
}

fn lower_call_value_node(
    node: &Node,
    dst: FrameRegion,
    values: &BTreeMap<NodeId, LoweredSlot>,
) -> Result<WeavyOp, Diagnostics> {
    require_input_count(node, 2)?;
    let callee = input_value(node, values, 0)?;
    let Type::Function { parameter, result } = &callee.ty else {
        return Err(lowering_diagnostic(
            node.span,
            "indirect call input is not a function value",
        ));
    };
    require_value(
        node,
        &callee,
        &callee.ty,
        ValueRepresentation::InlineComposite,
    )?;
    if callee.region.words().as_usize() != 2 || result.as_ref() != &node.ty {
        return Err(lowering_diagnostic(
            node.span,
            "indirect call does not match the closure ABI",
        ));
    }
    let argument = input_value(node, values, 1)?;
    require_value(
        node,
        &argument,
        parameter,
        representation_for_type(parameter, node.span)?,
    )?;
    Ok(WeavyOp::CallIndirect {
        callee: callee.region.start().byte_offset(),
        args: vec![ArgCopy {
            src: argument.region.start().byte_offset(),
            dst: 0,
            size: argument
                .region
                .byte_size()
                .ok_or_else(|| lowering_diagnostic(node.span, "argument size overflow"))?,
        }],
        ret: dst.start().byte_offset(),
    })
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AggregateKind {
    Tuple,
    Record,
}

struct AggregateElementLayout<'a> {
    ty: &'a Type,
    offset_words: usize,
    words: FrameWords,
}

struct AggregateLayout<'a> {
    kind: AggregateKind,
    elements: Vec<AggregateElementLayout<'a>>,
    words: FrameWords,
}

impl<'a> AggregateLayout<'a> {
    fn for_type(ty: &'a Type, span: Span) -> Result<Self, Diagnostics> {
        let (kind, element_types): (AggregateKind, Vec<&Type>) = match ty {
            Type::Tuple(elements) => (AggregateKind::Tuple, elements.iter().collect()),
            Type::Record(record) => (
                AggregateKind::Record,
                record.fields.iter().map(|field| &field.ty).collect(),
            ),
            _ => {
                return Err(lowering_diagnostic(
                    span,
                    &format!("{} is not an aggregate type", ty.name()),
                ));
            }
        };

        let mut offset_words = 0usize;
        let mut elements = Vec::with_capacity(element_types.len());
        for element_type in element_types {
            let words = type_words(element_type, span)?;
            elements.push(AggregateElementLayout {
                ty: element_type,
                offset_words,
                words,
            });
            offset_words = offset_words
                .checked_add(words.as_usize())
                .ok_or_else(|| lowering_diagnostic(span, "aggregate layout width overflow"))?;
        }
        let words = FrameWords::from_usize(offset_words)
            .ok_or_else(|| lowering_diagnostic(span, "aggregate layout width overflow"))?;
        Ok(Self {
            kind,
            elements,
            words,
        })
    }
}

struct EnumVariantLayout<'a> {
    elements: Vec<AggregateElementLayout<'a>>,
    words: FrameWords,
}

struct EnumLayout<'a> {
    enumeration: &'a EnumType,
    variants: Vec<EnumVariantLayout<'a>>,
    words: FrameWords,
}

impl<'a> EnumLayout<'a> {
    fn for_type(ty: &'a Type, span: Span) -> Result<Self, Diagnostics> {
        let Type::Enum(enumeration) = ty else {
            return Err(lowering_diagnostic(
                span,
                &format!("{} is not an enum type", ty.name()),
            ));
        };
        Self::for_enum(enumeration, span)
    }

    fn for_enum(enumeration: &'a EnumType, span: Span) -> Result<Self, Diagnostics> {
        let mut widest_payload = 0usize;
        let mut variants = Vec::with_capacity(enumeration.variants.len());
        for variant in &enumeration.variants {
            let element_types = match &variant.payload {
                VariantPayload::Unit => Vec::new(),
                VariantPayload::Tuple(elements) => elements.iter().collect(),
                VariantPayload::Record(fields) => fields.iter().map(|field| &field.ty).collect(),
            };
            let mut offset_words = 0usize;
            let mut elements = Vec::with_capacity(element_types.len());
            for element_type in element_types {
                let words = type_words(element_type, span)?;
                elements.push(AggregateElementLayout {
                    ty: element_type,
                    offset_words,
                    words,
                });
                offset_words = offset_words.checked_add(words.as_usize()).ok_or_else(|| {
                    lowering_diagnostic(span, "enum payload layout width overflow")
                })?;
            }
            widest_payload = widest_payload.max(offset_words);
            variants.push(EnumVariantLayout {
                elements,
                words: FrameWords::from_usize(offset_words).ok_or_else(|| {
                    lowering_diagnostic(span, "enum payload layout width overflow")
                })?,
            });
        }
        let total_words = widest_payload
            .checked_add(1)
            .ok_or_else(|| lowering_diagnostic(span, "enum layout width overflow"))?;
        Ok(Self {
            enumeration,
            variants,
            words: FrameWords::from_usize(total_words)
                .ok_or_else(|| lowering_diagnostic(span, "enum layout width overflow"))?,
        })
    }

    fn variant(&self, index: u32, span: Span) -> Result<&EnumVariantLayout<'a>, Diagnostics> {
        let index = usize::try_from(index)
            .map_err(|_| lowering_diagnostic(span, "enum variant index overflow"))?;
        self.variants
            .get(index)
            .ok_or_else(|| lowering_diagnostic(span, "enum variant index is out of bounds"))
    }
}

fn lower_variant_node(
    node: &Node,
    dst: FrameRegion,
    dst_region: WeavyRegionId,
    values: &BTreeMap<NodeId, LoweredSlot>,
    variant: u32,
) -> Result<(Vec<WeavyOp>, ValueRepresentation), Diagnostics> {
    let layout = EnumLayout::for_type(&node.ty, node.span)?;
    let variant_layout = layout.variant(variant, node.span)?;
    require_input_count(node, variant_layout.elements.len())?;
    if layout.words != dst.words() {
        return Err(lowering_diagnostic(
            node.span,
            "enum construction destination has the wrong frame width",
        ));
    }
    if variant_layout
        .words
        .as_usize()
        .checked_add(1)
        .is_none_or(|occupied| occupied > layout.words.as_usize())
    {
        return Err(lowering_diagnostic(
            node.span,
            "enum variant payload exceeds its frame layout",
        ));
    }

    let mut fields = Vec::new();
    for (index, element) in variant_layout.elements.iter().enumerate() {
        let value = input_value(node, values, index)?;
        require_value(
            node,
            &value,
            element.ty,
            representation_for_type(element.ty, node.span)?,
        )?;
        fields.push(StructuralFieldSource {
            field: u32::try_from(index)
                .map_err(|_| lowering_diagnostic(node.span, "enum field index overflow"))?,
            source: value.region_id,
        });
    }
    Ok((
        vec![WeavyOp::EnumConstruct {
            dst: dst_region,
            variant,
            fields,
        }],
        ValueRepresentation::InlineComposite,
    ))
}

fn lower_variant_project_node(
    node: &Node,
    dst: FrameRegion,
    dst_region: WeavyRegionId,
    values: &BTreeMap<NodeId, LoweredSlot>,
    variant: u32,
    field: u32,
) -> Result<(Vec<WeavyOp>, ValueRepresentation), Diagnostics> {
    require_input_count(node, 1)?;
    let receiver = input_value(node, values, 0)?;
    require_value(
        node,
        &receiver,
        &receiver.ty,
        ValueRepresentation::InlineComposite,
    )?;
    let layout = EnumLayout::for_type(&receiver.ty, node.span)?;
    let variant_layout = layout.variant(variant, node.span)?;
    let field = usize::try_from(field)
        .map_err(|_| lowering_diagnostic(node.span, "variant field index overflow"))?;
    let element = variant_layout
        .elements
        .get(field)
        .ok_or_else(|| lowering_diagnostic(node.span, "variant field index is out of bounds"))?;
    if &node.ty != element.ty {
        return Err(lowering_diagnostic(
            node.span,
            "variant projection result has the wrong VIR type",
        ));
    }
    if element.words != dst.words() {
        return Err(lowering_diagnostic(
            node.span,
            "variant projection destination has the wrong frame width",
        ));
    }
    Ok((
        vec![WeavyOp::EnumProjectChecked {
            dst: dst_region,
            value: receiver.region_id,
            variant,
            field: u32::try_from(field)
                .map_err(|_| lowering_diagnostic(node.span, "enum field index overflow"))?,
        }],
        representation_for_type(element.ty, node.span)?,
    ))
}

fn lower_aggregate_node(
    node: &Node,
    dst: FrameRegion,
    dst_region: WeavyRegionId,
    values: &BTreeMap<NodeId, LoweredSlot>,
    expected_kind: AggregateKind,
) -> Result<(Vec<WeavyOp>, ValueRepresentation), Diagnostics> {
    let layout = AggregateLayout::for_type(&node.ty, node.span)?;
    if layout.kind != expected_kind {
        return Err(lowering_diagnostic(
            node.span,
            "aggregate construction op does not match its VIR type",
        ));
    }
    require_input_count(node, layout.elements.len())?;
    let mut fields = Vec::new();
    for (index, element) in layout.elements.iter().enumerate() {
        let value = input_value(node, values, index)?;
        require_value(
            node,
            &value,
            element.ty,
            representation_for_type(element.ty, node.span)?,
        )?;
        fields.push(StructuralFieldSource {
            field: u32::try_from(index)
                .map_err(|_| lowering_diagnostic(node.span, "product field index overflow"))?,
            source: value.region_id,
        });
    }
    if layout.words != dst.words() {
        return Err(lowering_diagnostic(
            node.span,
            "aggregate fields do not fill their frame region",
        ));
    }
    Ok((
        vec![WeavyOp::ProductConstruct {
            dst: dst_region,
            fields,
        }],
        ValueRepresentation::InlineComposite,
    ))
}

fn lower_project_node(
    node: &Node,
    dst: FrameRegion,
    dst_region: WeavyRegionId,
    values: &BTreeMap<NodeId, LoweredSlot>,
    index: u32,
) -> Result<(Vec<WeavyOp>, ValueRepresentation), Diagnostics> {
    require_input_count(node, 1)?;
    let receiver = input_value(node, values, 0)?;
    let layout = AggregateLayout::for_type(&receiver.ty, node.span)?;
    require_value(
        node,
        &receiver,
        &receiver.ty,
        ValueRepresentation::InlineComposite,
    )?;
    let index = usize::try_from(index)
        .map_err(|_| lowering_diagnostic(node.span, "aggregate projection index overflow"))?;
    let element = layout
        .elements
        .get(index)
        .ok_or_else(|| lowering_diagnostic(node.span, "aggregate projection is out of bounds"))?;
    if &node.ty != element.ty {
        return Err(lowering_diagnostic(
            node.span,
            "aggregate projection result has the wrong VIR type",
        ));
    }
    if element.words != dst.words() {
        return Err(lowering_diagnostic(
            node.span,
            "aggregate projection destination has the wrong frame width",
        ));
    }
    Ok((
        vec![WeavyOp::ProductProject {
            dst: dst_region,
            product: receiver.region_id,
            field: u32::try_from(index)
                .map_err(|_| lowering_diagnostic(node.span, "product field index overflow"))?,
        }],
        representation_for_type(element.ty, node.span)?,
    ))
}

fn copy_region(
    node: &Node,
    source: FrameRegion,
    target: FrameRegion,
) -> Result<Vec<WeavyOp>, Diagnostics> {
    if source.words() != target.words() {
        return Err(lowering_diagnostic(
            node.span,
            "value copy has incompatible frame regions",
        ));
    }
    (0..source.words().as_usize())
        .map(|index| {
            let src = source
                .word(index)
                .ok_or_else(|| lowering_diagnostic(node.span, "source word offset overflow"))?;
            let dst = target
                .word(index)
                .ok_or_else(|| lowering_diagnostic(node.span, "target word offset overflow"))?;
            Ok(WeavyOp::CopyI64 {
                dst: dst.byte_offset(),
                src: src.byte_offset(),
            })
        })
        .collect()
}

fn copy_lowered_value(
    node: &Node,
    source: &LoweredSlot,
    destination: FrameRegion,
    destination_region: WeavyRegionId,
) -> Result<Vec<WeavyOp>, Diagnostics> {
    if source.region.words() != destination.words() {
        return Err(lowering_diagnostic(
            node.span,
            "value merge has incompatible frame regions",
        ));
    }
    match source.representation {
        ValueRepresentation::Word => copy_region(node, source.region, destination),
        ValueRepresentation::RealizedHandle | ValueRepresentation::InlineComposite => {
            Ok(vec![WeavyOp::CopyValue {
                dst: destination_region,
                src: source.region_id,
            }])
        }
    }
}

fn type_words(ty: &Type, span: Span) -> Result<FrameWords, Diagnostics> {
    ty.word_width()
        .and_then(FrameWords::from_usize)
        .ok_or_else(|| lowering_diagnostic(span, "type has no finite frame width"))
}

fn representation_for_type(ty: &Type, span: Span) -> Result<ValueRepresentation, Diagnostics> {
    match ty {
        Type::Bool | Type::Int | Type::Check => Ok(ValueRepresentation::Word),
        Type::String => Ok(ValueRepresentation::RealizedHandle),
        Type::Function { .. } | Type::Tuple(_) | Type::Record(_) | Type::Enum(_) => {
            Ok(ValueRepresentation::InlineComposite)
        }
        Type::Array(_) => Err(lowering_diagnostic(
            span,
            "array values require array lowering",
        )),
        Type::StreamCheck => Err(lowering_diagnostic(
            span,
            "Stream<Check> has no island-interior word representation",
        )),
    }
}

fn binary_values(
    node: &Node,
    values: &BTreeMap<NodeId, LoweredSlot>,
) -> Result<(LoweredSlot, LoweredSlot), Diagnostics> {
    require_input_count(node, 2)?;
    Ok((input_value(node, values, 0)?, input_value(node, values, 1)?))
}

fn input_value(
    node: &Node,
    values: &BTreeMap<NodeId, LoweredSlot>,
    index: usize,
) -> Result<LoweredSlot, Diagnostics> {
    let input = node
        .inputs
        .get(index)
        .ok_or_else(|| lowering_diagnostic(node.span, "missing VIR input"))?;
    values
        .get(input)
        .cloned()
        .ok_or_else(|| lowering_diagnostic(node.span, "VIR input is not topologically prior"))
}

fn require_node_type(node: &Node, expected: Type) -> Result<(), Diagnostics> {
    if node.ty != expected {
        return Err(lowering_diagnostic(
            node.span,
            &format!(
                "VIR op produces {}, expected {}",
                node.ty.name(),
                expected.name()
            ),
        ));
    }
    Ok(())
}

fn require_value(
    node: &Node,
    value: &LoweredSlot,
    expected_type: &Type,
    expected_representation: ValueRepresentation,
) -> Result<(), Diagnostics> {
    if &value.ty != expected_type || value.representation != expected_representation {
        return Err(lowering_diagnostic(
            node.span,
            &format!(
                "VIR input has {} {:?}, expected {} {:?}",
                value.ty.name(),
                value.representation,
                expected_type.name(),
                expected_representation
            ),
        ));
    }
    Ok(())
}

fn require_input_count(node: &Node, expected: usize) -> Result<(), Diagnostics> {
    if node.inputs.len() != expected {
        return Err(lowering_diagnostic(
            node.span,
            &format!(
                "VIR op has {} inputs, expected {expected}",
                node.inputs.len()
            ),
        ));
    }
    Ok(())
}

fn lowering_diagnostic(span: Span, construct: &str) -> Diagnostics {
    Diagnostics::one(Diagnostic {
        code: DiagnosticCode::LoweringUnsupported,
        primary: span,
        labels: Vec::new(),
        payload: DiagnosticPayload::Unsupported {
            construct: construct.to_owned(),
        },
    })
}
