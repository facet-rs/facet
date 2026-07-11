//! VIR island lowering to architecture-neutral Weavy bytecode.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write;

use weavy::exec::Executable;
use weavy::mem::Layout;
use weavy::task::{
    ArgCopy, ArrayOpStatus, Fn as WeavyFn, FnId as WeavyFnId, Op as WeavyOp, OrderedOpStatus,
    Program as WeavyProgram, StructuralFieldSource,
};
use weavy::{
    CallContract as WeavyCallContract, CallContractId as WeavyCallContractId,
    FrameContract as WeavyFrameContract, FrameRegion as WeavyFrameRegion,
    FunctionContract as WeavyFunctionContract,
    OrderedCollectionContract as WeavyOrderedCollectionContract,
    OrderedCollectionKind as WeavyOrderedCollectionKind, PayloadKind as WeavyPayloadKind,
    ProgramContract as WeavyProgramContract, RegionId as WeavyRegionId,
    RegionShape as WeavyRegionShape, SchemaContract as WeavySchemaContract,
    SchemaRef as WeavySchemaRef, ValueFieldUse as WeavyValueFieldUse,
    ValueSelector as WeavyValueSelector, ValueShapeContract as WeavyValueShapeContract,
    ValueShapeKind as WeavyValueShapeKind, ValueShapeRef as WeavyValueShapeRef,
    ValueVariant as WeavyValueVariant, WordKind as WeavyWordKind,
};

use crate::diagnostic::{Diagnostic, DiagnosticCode, DiagnosticPayload, Diagnostics};
use crate::runtime::{
    DemandKey, DemandPreimage, FrameRegion, FrameSlot, FrameWords, MachineAttribution,
    MachineError, MachineOperation, RecipeId, SchemaId,
};
use crate::support::Span;
use crate::vir::{
    ArrayMapExecutionShape, ArrayMapPartition, EnumType, EnumVariant, Function, FunctionId, Island,
    Node, NodeId, NodeRef, ORDERING_EQUAL_VARIANT, ORDERING_GREATER_VARIANT, ORDERING_LESS_VARIANT,
    Op, Type, VariantPayload,
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

/// The internal result ABI carried by every function in an array-bearing
/// island. It is metadata for the verified artifact, not a source-level type.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct ArrayOutcomeAbi {
    pub ty: Type,
    pub ok_variant: u32,
    pub index_out_of_bounds_variant: u32,
    pub array_machine_variant: u32,
    pub missing_key_variant: u32,
    pub duplicate_key_variant: u32,
    pub ordered_machine_variant: u32,
}

impl ArrayOutcomeAbi {
    #[must_use]
    pub fn for_value(value: Type) -> Self {
        Self {
            ty: Type::Enum(EnumType {
                name: format!("$array_outcome<{}>", value.name()),
                variants: vec![
                    EnumVariant {
                        name: "Ok".to_owned(),
                        payload: VariantPayload::Tuple(vec![value]),
                    },
                    EnumVariant {
                        name: "IndexOutOfBounds".to_owned(),
                        payload: VariantPayload::Tuple(vec![Type::Int, Type::Int, Type::Int]),
                    },
                    EnumVariant {
                        name: "ArrayMachine".to_owned(),
                        payload: VariantPayload::Tuple(vec![Type::Int, Type::Int]),
                    },
                    EnumVariant {
                        name: "MissingKey".to_owned(),
                        payload: VariantPayload::Tuple(vec![Type::Int]),
                    },
                    EnumVariant {
                        name: "DuplicateKey".to_owned(),
                        payload: VariantPayload::Tuple(vec![Type::Int]),
                    },
                    EnumVariant {
                        name: "OrderedMachine".to_owned(),
                        payload: VariantPayload::Tuple(vec![Type::Int, Type::Int]),
                    },
                ],
            }),
            ok_variant: 0,
            index_out_of_bounds_variant: 1,
            array_machine_variant: 2,
            missing_key_variant: 3,
            duplicate_key_variant: 4,
            ordered_machine_variant: 5,
        }
    }
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
    executable: Executable,
    pub array_outcome: Option<ArrayOutcomeAbi>,
    pub pc_nodes: Vec<Vec<NodeRef>>,
    pub constants: Vec<ValueConstant>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LoweringError {
    Diagnostics(Diagnostics),
    Machine(Box<MachineError>),
}

impl From<Diagnostics> for LoweringError {
    fn from(diagnostics: Diagnostics) -> Self {
        Self::Diagnostics(diagnostics)
    }
}

impl From<MachineError> for LoweringError {
    fn from(error: MachineError) -> Self {
        Self::Machine(Box::new(error))
    }
}

impl LoweringArtifact {
    #[must_use]
    pub fn program(&self) -> &WeavyProgram {
        self.executable.program().program()
    }

    #[must_use]
    pub fn contract(&self) -> &WeavyProgramContract {
        self.executable.program().contract()
    }

    #[must_use]
    pub fn executable(&self) -> &Executable {
        &self.executable
    }

    #[cfg(test)]
    pub(crate) fn with_test_verified_executable(&self, executable: Executable) -> Self {
        Self {
            recipe: self.recipe,
            demand_key: self.demand_key,
            demand_preimage: self.demand_preimage.clone(),
            executable,
            array_outcome: self.array_outcome.clone(),
            pc_nodes: self.pc_nodes.clone(),
            constants: self.constants.clone(),
        }
    }

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
        for (function_index, function) in self.program().fns.iter().enumerate() {
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
    pub fn get_or_lower(&mut self, island: &Island) -> Result<&LoweringArtifact, LoweringError> {
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

fn lower_island(island: &Island, recipe: RecipeId) -> Result<LoweringArtifact, LoweringError> {
    let output = island
        .nodes
        .iter()
        .find(|node| node.id == island.output)
        .ok_or_else(|| lowering_diagnostic(Span { start: 0, end: 0 }, "missing island output"))?;
    if output.ty != Type::Check {
        return Err(lowering_diagnostic(output.span, "island output is not a Check").into());
    }
    let array_outcome = island_contains_checked_collection_ops(island)
        .then(|| ArrayOutcomeAbi::for_value(output.ty.clone()));
    let schemas = SchemaAssignments::build(island, array_outcome.is_some())?;

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
            array_outcome.as_ref().map(|_| output.ty.clone()),
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
                array_outcome.as_ref().map(|_| function.return_type.clone()),
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
        schemas: &schemas,
        array_map_partitions: &island.array_map_partitions,
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
    let contract = ProgramContractBuilder::build(
        island,
        &program,
        &layouts,
        &constant_closures,
        &regions,
        &schemas,
    )?;
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
    let demand_key = DemandKey::from_preimage(&demand_preimage);
    let verified = program.verify(contract).map_err(|error| {
        let source = program_error_attribution(&error, &pc_nodes, &attribution);
        MachineError::program(
            MachineOperation::LoweringVerification,
            error,
            source,
            demand_key,
        )
    })?;
    Ok(LoweringArtifact {
        recipe,
        demand_key,
        demand_preimage,
        executable: Executable::new(verified),
        array_outcome,
        pc_nodes,
        constants,
    })
}

fn island_contains_checked_collection_ops(island: &Island) -> bool {
    nodes_contain_checked_collection_ops(&island.nodes)
        || island
            .callees
            .iter()
            .any(|function| nodes_contain_checked_collection_ops(&function.nodes))
}

fn nodes_contain_checked_collection_ops(nodes: &[Node]) -> bool {
    nodes.iter().any(|node| {
        matches!(
            node.op,
            Op::Array
                | Op::ArrayIndex
                | Op::ArrayLen
                | Op::ArrayMap { .. }
                | Op::ArrayFold
                | Op::ArrayAppend
                | Op::ArrayConcat
                | Op::Map
                | Op::MapAdd
                | Op::MapConcat
                | Op::MapWith
                | Op::MapGet
                | Op::MapHas
                | Op::MapLen
                | Op::MapKeys
                | Op::Set
                | Op::SetAdd
                | Op::SetConcat
                | Op::SetHas
                | Op::SetLen
                | Op::SetValues
                | Op::StreamCollect
        ) || matches!(node.op, Op::Eq | Op::Ne)
            && node.inputs.iter().any(|input| {
                nodes
                    .iter()
                    .find(|candidate| candidate.id == *input)
                    .is_some_and(|candidate| type_contains_array(&candidate.ty))
            })
    })
}

fn type_contains_array(ty: &Type) -> bool {
    match ty {
        Type::Array(_) => true,
        Type::Tuple(fields) => fields.iter().any(type_contains_array),
        Type::Record(record) => record
            .fields
            .iter()
            .any(|field| type_contains_array(&field.ty)),
        Type::Enum(enumeration) => {
            enumeration
                .variants
                .iter()
                .any(|variant| match &variant.payload {
                    VariantPayload::Unit => false,
                    VariantPayload::Tuple(fields) => fields.iter().any(type_contains_array),
                    VariantPayload::Record(fields) => {
                        fields.iter().any(|field| type_contains_array(&field.ty))
                    }
                })
        }
        Type::Function { parameter, result } => {
            type_contains_array(parameter) || type_contains_array(result)
        }
        Type::Map { key, value } => type_contains_array(key) || type_contains_array(value),
        Type::Set(element) => type_contains_array(element),
        Type::Stream { key, value } => type_contains_array(key) || type_contains_array(value),
        Type::Bool | Type::Int | Type::Check | Type::StreamCheck | Type::String => false,
    }
}

fn program_error_attribution(
    error: &weavy::ProgramError,
    pc_nodes: &[Vec<NodeRef>],
    attribution: &LoweringAttribution,
) -> Option<MachineAttribution> {
    let (function, pc) = (error.function?, error.pc?);
    let node = pc_nodes
        .get(function.0 as usize)
        .and_then(|nodes| nodes.get(pc))
        .copied()?;
    let source = attribution.source_for_node(node)?;
    Some(MachineAttribution {
        function: source.function,
        node: source.node,
        span: source.span,
        weavy_function: Some(function),
        weavy_pc: Some(pc),
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
    schemas_preassigned: &'a SchemaAssignments,
    function_order: Vec<FunctionContractSource<'a>>,
    closure_targets: BTreeSet<FunctionId>,
    callable_outcomes: bool,
    calls: Vec<WeavyCallContract>,
    schemas: Vec<WeavySchemaContract>,
    schema_ready: Vec<bool>,
    value_shapes: Vec<WeavyValueShapeContract>,
    value_shape_keys: Vec<Type>,
}

fn empty_schema() -> WeavySchemaContract {
    WeavySchemaContract {
        inline: WeavyRegionShape::default(),
        value_shape: None,
        payload: WeavyPayloadKind::Inline,
    }
}

/// Closed program-local schema order. It is sorted by Vix's canonical semantic
/// type encoding, so source spans and lowering traversal cannot affect a
/// Weavy `SchemaRef` witness.
struct SchemaAssignments {
    types: Vec<Type>,
}

impl SchemaAssignments {
    fn build(island: &Island, array_outcomes: bool) -> Result<Self, Diagnostics> {
        // Cross-frame captured constants are represented by a contract entry
        // even when the owning String node is not in this function body.
        let mut types = vec![Type::Bool, Type::Int, Type::Check, Type::String];
        let mut add = |ty: &Type| collect_schema_types(ty, &mut types);
        for node in &island.nodes {
            add(&node.ty);
        }
        for function in &island.callees {
            add(&function.return_type);
            for parameter in &function.parameters {
                add(&parameter.ty);
            }
            for node in &function.nodes {
                add(&node.ty);
            }
        }
        if array_outcomes {
            let output = island
                .nodes
                .iter()
                .find(|node| node.id == island.output)
                .ok_or_else(|| {
                    lowering_diagnostic(Span { start: 0, end: 0 }, "missing island output")
                })?;
            add(&ArrayOutcomeAbi::for_value(output.ty.clone()).ty);
            for function in &island.callees {
                add(&ArrayOutcomeAbi::for_value(function.return_type.clone()).ty);
            }
        }
        types.sort_by_key(crate::vir::canonical_type);
        types.dedup();
        if types.iter().any(|ty| matches!(ty, Type::StreamCheck)) {
            return Err(lowering_diagnostic(
                Span { start: 0, end: 0 },
                "Stream<Check> cannot be a dynamic program payload schema",
            ));
        }
        Ok(Self { types })
    }

    fn schema_for(&self, ty: &Type, span: Span) -> Result<WeavySchemaRef, Diagnostics> {
        self.types
            .iter()
            .position(|candidate| candidate == ty)
            .and_then(|index| u32::try_from(index).ok())
            .map(WeavySchemaRef)
            .ok_or_else(|| {
                lowering_diagnostic(
                    span,
                    &format!("{} is absent from closed schema assignment", ty.name()),
                )
            })
    }
}

fn collect_schema_types(ty: &Type, out: &mut Vec<Type>) {
    if let Type::Stream { key, value } = ty {
        collect_schema_types(key, out);
        collect_schema_types(value, out);
        return;
    }
    out.push(ty.clone());
    match ty {
        Type::Function { parameter, result } => {
            collect_schema_types(parameter, out);
            collect_schema_types(result, out);
        }
        Type::Tuple(fields) => fields
            .iter()
            .for_each(|field| collect_schema_types(field, out)),
        Type::Record(record) => record
            .fields
            .iter()
            .for_each(|field| collect_schema_types(&field.ty, out)),
        Type::Enum(enumeration) => {
            for variant in &enumeration.variants {
                match &variant.payload {
                    VariantPayload::Unit => {}
                    VariantPayload::Tuple(fields) => fields
                        .iter()
                        .for_each(|field| collect_schema_types(field, out)),
                    VariantPayload::Record(fields) => fields
                        .iter()
                        .for_each(|field| collect_schema_types(&field.ty, out)),
                }
            }
        }
        Type::Array(element) => collect_schema_types(element, out),
        Type::Map { key, value } => {
            collect_schema_types(
                &Type::Tuple(vec![key.as_ref().clone(), value.as_ref().clone()]),
                out,
            );
        }
        Type::Set(element) => collect_schema_types(element, out),
        Type::Stream { .. } => unreachable!("stream schemas return before insertion"),
        Type::Bool | Type::Int | Type::Check | Type::StreamCheck | Type::String => {}
    }
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
    let array = if type_contains_array(&a.ty) {
        let scratch = sequence.function.layout.outcome_scratch.ok_or_else(|| {
            lowering_diagnostic(node.span, "array equality has no checked outcome scratch")
        })?;
        let assigned = sequence
            .lowering
            .regions
            .outcome_scratch(sequence.function.id, node.span)?;
        let outcome = sequence
            .lowering
            .regions
            .array_outcome(sequence.function.id, node.span)?;
        let return_label = sequence.array_return.ok_or_else(|| {
            lowering_diagnostic(node.span, "array equality has no checked outcome return")
        })?;
        let site = sequence
            .lowering
            .trace_ids
            .get(&NodeRef {
                function: sequence.function.id,
                node: node.id,
            })
            .copied()
            .ok_or_else(|| {
                lowering_diagnostic(node.span, "array equality has no stable trace site")
            })?;
        Some(ArrayEqualityContext {
            schemas: sequence.lowering.schemas,
            scratch,
            assigned,
            outcome,
            return_label,
            site,
        })
    } else {
        None
    };
    outputs.code.push(WeavyOp::ConstI64 {
        dst: accumulator.region.start().byte_offset(),
        value: 1,
    });
    SemanticEqualityEmitter {
        node,
        accumulator: accumulator.region.start(),
        work: work.region.start(),
        equal: equal.region.start(),
        temps: &mut temps,
        code: outputs.code,
        array,
    }
    .emit(&a.ty, &a, &b)?;
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

struct SemanticEqualityEmitter<'node, 'temps, 'code> {
    node: &'node Node,
    accumulator: FrameSlot,
    work: FrameSlot,
    equal: FrameSlot,
    temps: &'node mut TemporaryCursor<'temps>,
    code: &'code mut CodeBuilder,
    array: Option<ArrayEqualityContext<'node>>,
}

#[derive(Clone, Copy)]
struct ArrayEqualityContext<'a> {
    schemas: &'a SchemaAssignments,
    scratch: OutcomeScratch,
    assigned: AssignedOutcomeScratch,
    outcome: WeavyRegionId,
    return_label: CodeLabel,
    site: u32,
}

impl SemanticEqualityEmitter<'_, '_, '_> {
    fn emit(&mut self, ty: &Type, a: &LoweredSlot, b: &LoweredSlot) -> Result<(), Diagnostics> {
        let node = self.node;
        let accumulator = self.accumulator;
        let work = self.work;
        let equal = self.equal;
        match ty {
            Type::Bool | Type::Int | Type::Check => {
                self.code.push(WeavyOp::EqI64 {
                    dst: work.byte_offset(),
                    a: a.region.start().byte_offset(),
                    b: b.region.start().byte_offset(),
                });
                self.code.push(WeavyOp::MulI64 {
                    dst: accumulator.byte_offset(),
                    a: accumulator.byte_offset(),
                    b: work.byte_offset(),
                });
            }
            Type::String => {
                self.code.push(WeavyOp::CompareValueBytes {
                    dst: work.byte_offset(),
                    a: a.region.start().byte_offset(),
                    b: b.region.start().byte_offset(),
                });
                self.code.push(WeavyOp::ConstI64 {
                    dst: equal.byte_offset(),
                    value: i64::from(ORDERING_EQUAL_VARIANT),
                });
                self.code.push(WeavyOp::EqI64 {
                    dst: work.byte_offset(),
                    a: work.byte_offset(),
                    b: equal.byte_offset(),
                });
                self.code.push(WeavyOp::MulI64 {
                    dst: accumulator.byte_offset(),
                    a: accumulator.byte_offset(),
                    b: work.byte_offset(),
                });
            }
            Type::Tuple(fields) => {
                for (index, field) in fields.iter().enumerate() {
                    let left = self.temps.take(field, node.span)?;
                    let right = self.temps.take(field, node.span)?;
                    let field = u32::try_from(index).map_err(|_| {
                        lowering_diagnostic(node.span, "product field index overflow")
                    })?;
                    self.code.push(WeavyOp::ProductProject {
                        dst: left.region_id,
                        product: a.region_id,
                        field,
                    });
                    self.code.push(WeavyOp::ProductProject {
                        dst: right.region_id,
                        product: b.region_id,
                        field,
                    });
                    self.emit(field_type(fields, index), &left, &right)?;
                }
            }
            Type::Record(record) => {
                for (index, field) in record.fields.iter().enumerate() {
                    let left = self.temps.take(&field.ty, node.span)?;
                    let right = self.temps.take(&field.ty, node.span)?;
                    let field_index = u32::try_from(index).map_err(|_| {
                        lowering_diagnostic(node.span, "record field index overflow")
                    })?;
                    self.code.push(WeavyOp::ProductProject {
                        dst: left.region_id,
                        product: a.region_id,
                        field: field_index,
                    });
                    self.code.push(WeavyOp::ProductProject {
                        dst: right.region_id,
                        product: b.region_id,
                        field: field_index,
                    });
                    self.emit(&field.ty, &left, &right)?;
                }
            }
            Type::Enum(enumeration) => {
                for (variant_index, variant) in enumeration.variants.iter().enumerate() {
                    let variant_index = u32::try_from(variant_index).map_err(|_| {
                        lowering_diagnostic(node.span, "enum variant index overflow")
                    })?;
                    self.code.push(WeavyOp::EnumIsVariant {
                        dst: work_region_id(work, self.temps, node.span)?,
                        value: a.region_id,
                        variant: variant_index,
                    });
                    self.code.push(WeavyOp::EnumIsVariant {
                        dst: work_region_id(equal, self.temps, node.span)?,
                        value: b.region_id,
                        variant: variant_index,
                    });
                    self.code.push(WeavyOp::EqI64 {
                        dst: work.byte_offset(),
                        a: work.byte_offset(),
                        b: equal.byte_offset(),
                    });
                    self.code.push(WeavyOp::MulI64 {
                        dst: accumulator.byte_offset(),
                        a: accumulator.byte_offset(),
                        b: work.byte_offset(),
                    });
                    // `Eq` is also true when both operands are *not* this
                    // variant. Multiplying by `b is variant` leaves one only on
                    // the sole path where both checked selectors name this arm.
                    self.code.push(WeavyOp::MulI64 {
                        dst: work.byte_offset(),
                        a: work.byte_offset(),
                        b: equal.byte_offset(),
                    });
                    let next = self.code.label();
                    self.code.jump_if_zero(work, next);
                    let fields: Vec<&Type> = match &variant.payload {
                        VariantPayload::Unit => Vec::new(),
                        VariantPayload::Tuple(fields) => fields.iter().collect(),
                        VariantPayload::Record(fields) => {
                            fields.iter().map(|field| &field.ty).collect()
                        }
                    };
                    for (field_index, field) in fields.into_iter().enumerate() {
                        let left = self.temps.take(field, node.span)?;
                        let right = self.temps.take(field, node.span)?;
                        let field_index = u32::try_from(field_index).map_err(|_| {
                            lowering_diagnostic(node.span, "enum field index overflow")
                        })?;
                        self.code.push(WeavyOp::EnumProjectChecked {
                            dst: left.region_id,
                            value: a.region_id,
                            variant: variant_index,
                            field: field_index,
                        });
                        self.code.push(WeavyOp::EnumProjectChecked {
                            dst: right.region_id,
                            value: b.region_id,
                            variant: variant_index,
                            field: field_index,
                        });
                        self.emit(field, &left, &right)?;
                    }
                    self.code.bind(next, node.span)?;
                }
            }
            Type::Array(element) => {
                let array = self.array.ok_or_else(|| {
                    lowering_diagnostic(node.span, "array equality has no checked access context")
                })?;
                let left_length = self.temps.take(&Type::Int, node.span)?;
                let right_length = self.temps.take(&Type::Int, node.span)?;
                let index = self.temps.take(&Type::Int, node.span)?;
                let left = self.temps.take(element, node.span)?;
                let right = self.temps.take(element, node.span)?;
                let element_schema = array.schemas.schema_for(element, node.span)?;
                let element_width = element_byte_width(element, node.span)?;

                self.code.push(WeavyOp::LoadArrayLen {
                    dst: left_length.region.start().byte_offset(),
                    status: array.scratch.status.byte_offset(),
                    array: a.region.start().byte_offset(),
                    elem_schema_ref: i64::from(element_schema.0),
                });
                emit_array_status_machine_checks(
                    node,
                    array.site,
                    array.scratch,
                    array.assigned,
                    array.outcome,
                    array.return_label,
                    self.code,
                )?;
                self.code.push(WeavyOp::LoadArrayLen {
                    dst: right_length.region.start().byte_offset(),
                    status: array.scratch.status.byte_offset(),
                    array: b.region.start().byte_offset(),
                    elem_schema_ref: i64::from(element_schema.0),
                });
                emit_array_status_machine_checks(
                    node,
                    array.site,
                    array.scratch,
                    array.assigned,
                    array.outcome,
                    array.return_label,
                    self.code,
                )?;
                self.code.push(WeavyOp::EqI64 {
                    dst: work.byte_offset(),
                    a: left_length.region.start().byte_offset(),
                    b: right_length.region.start().byte_offset(),
                });
                self.code.push(WeavyOp::MulI64 {
                    dst: accumulator.byte_offset(),
                    a: accumulator.byte_offset(),
                    b: work.byte_offset(),
                });
                let done = self.code.label();
                self.code.jump_if_zero(work, done);
                self.code.push(WeavyOp::ConstI64 {
                    dst: index.region.start().byte_offset(),
                    value: 0,
                });
                let next = self.code.label();
                self.code.bind(next, node.span)?;
                self.code.push(WeavyOp::LtI64 {
                    dst: work.byte_offset(),
                    a: index.region.start().byte_offset(),
                    b: left_length.region.start().byte_offset(),
                });
                self.code.jump_if_zero(work, done);
                self.code.push(WeavyOp::LoadArray {
                    dst: left.region.start().byte_offset(),
                    status: array.scratch.status.byte_offset(),
                    array: a.region.start().byte_offset(),
                    index: index.region.start().byte_offset(),
                    elem_width: element_width,
                    elem_schema_ref: i64::from(element_schema.0),
                });
                emit_array_status_machine_checks(
                    node,
                    array.site,
                    array.scratch,
                    array.assigned,
                    array.outcome,
                    array.return_label,
                    self.code,
                )?;
                self.code.push(WeavyOp::LoadArray {
                    dst: right.region.start().byte_offset(),
                    status: array.scratch.status.byte_offset(),
                    array: b.region.start().byte_offset(),
                    index: index.region.start().byte_offset(),
                    elem_width: element_width,
                    elem_schema_ref: i64::from(element_schema.0),
                });
                emit_array_status_machine_checks(
                    node,
                    array.site,
                    array.scratch,
                    array.assigned,
                    array.outcome,
                    array.return_label,
                    self.code,
                )?;
                self.emit(element, &left, &right)?;
                self.code.push(WeavyOp::ConstI64 {
                    dst: equal.byte_offset(),
                    value: 1,
                });
                self.code.push(WeavyOp::AddI64 {
                    dst: index.region.start().byte_offset(),
                    a: index.region.start().byte_offset(),
                    b: equal.byte_offset(),
                });
                self.code.jump(next);
                self.code.bind(done, node.span)?;
            }
            Type::Map { .. }
            | Type::Set(_)
            | Type::Function { .. }
            | Type::StreamCheck
            | Type::Stream { .. } => {
                return Err(lowering_diagnostic(
                    node.span,
                    "equality lowering is not implemented for this VIR type",
                ));
            }
        }
        Ok(())
    }
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

fn field_type(fields: &[Type], index: usize) -> &Type {
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
        schemas_preassigned: &'a SchemaAssignments,
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
            schemas_preassigned,
            function_order,
            closure_targets,
            callable_outcomes: layouts
                .values()
                .any(|layout| layout.array_outcome.is_some()),
            calls: Vec::new(),
            schemas: vec![empty_schema(); schemas_preassigned.types.len()],
            schema_ready: vec![false; schemas_preassigned.types.len()],
            value_shapes: Vec::new(),
            value_shape_keys: Vec::new(),
        };
        for ty in builder.schemas_preassigned.types.clone() {
            builder.schema_for_type(&ty, Span { start: 0, end: 0 })?;
        }

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
                let callee = functions[function_index]
                    .frame
                    .regions
                    .get_mut(callee_region.0 as usize)
                    .ok_or_else(|| {
                        lowering_diagnostic(node.span, "closure callee region is absent")
                    })?;
                callee.shape = WeavyRegionShape::word(WeavyWordKind::Callable(call));

                let shape = WeavyRegionShape::new(vec![
                    WeavyWordKind::Callable(call).into(),
                    WeavyWordKind::Scalar.into(),
                ]);
                let value_shape = WeavyValueShapeRef(builder.value_shapes.len() as u32);
                builder.value_shapes.push(WeavyValueShapeContract {
                    shape: shape.clone(),
                    kind: WeavyValueShapeKind::Product {
                        fields: vec![
                            WeavyValueFieldUse::new(
                                0,
                                WeavyRegionShape::word(WeavyWordKind::Callable(call)),
                            ),
                            WeavyValueFieldUse::new(
                                FrameSlot::word_size(),
                                WeavyRegionShape::word(WeavyWordKind::Scalar),
                            ),
                        ],
                    },
                });
                let closure_region = builder.regions.node(source.id, node.id, node.span)?;
                let closure = functions[function_index]
                    .frame
                    .regions
                    .get_mut(closure_region.0 as usize)
                    .ok_or_else(|| {
                        lowering_diagnostic(node.span, "closure value region is absent")
                    })?;
                closure.shape = shape;
                closure.value_shape = Some(value_shape);
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
        for (&node, temps) in &layout.typed_temps {
            let assigned = self.regions.typed_temps(function.id, node, function.span)?;
            if assigned.len() != temps.len() {
                return Err(lowering_diagnostic(
                    function.span,
                    "typed temporary contract assignment has the wrong length",
                ));
            }
            for (temp, region_id) in temps.iter().zip(assigned) {
                if region_id.0 as usize != regions.len() {
                    return Err(lowering_diagnostic(
                        function.span,
                        "typed temporary contract order is not canonical",
                    ));
                }
                regions.push(self.frame_region(temp.region.start(), &temp.ty)?);
            }
        }
        for (&node, cursors) in &layout.ordered_cursors {
            let assigned = self
                .regions
                .ordered_cursors(function.id, node, function.span)?;
            if assigned.len() != cursors.len() {
                return Err(lowering_diagnostic(
                    function.span,
                    "ordered cursor contract assignment has the wrong length",
                ));
            }
            for (cursor, region_id) in cursors.iter().zip(assigned) {
                if region_id.0 as usize != regions.len() {
                    return Err(lowering_diagnostic(
                        function.span,
                        "ordered cursor contract order is not canonical",
                    ));
                }
                regions.push(WeavyFrameRegion::new(
                    cursor.start().byte_offset(),
                    WeavyRegionShape::new(vec![WeavyWordKind::Opaque.into(); 2]),
                ));
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
        for (&node, (input, output)) in &layout.array_map_temps {
            let (input_id, output_id) =
                self.regions
                    .array_map_temps(function.id, node, function.span)?;
            if input_id.0 as usize != regions.len() {
                return Err(lowering_diagnostic(
                    function.span,
                    "array map input temporary contract order is not canonical",
                ));
            }
            regions.push(self.frame_region(input.region.start(), &input.ty)?);
            if output_id.0 as usize != regions.len() {
                return Err(lowering_diagnostic(
                    function.span,
                    "array map output temporary contract order is not canonical",
                ));
            }
            regions.push(self.frame_region(output.region.start(), &output.ty)?);
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
        let array_outcome = if let Some(outcome) = &layout.array_outcome {
            let region_id = self.regions.array_outcome(function.id, function.span)?;
            if region_id.0 as usize != regions.len() {
                return Err(lowering_diagnostic(
                    function.span,
                    "array outcome contract order is not canonical",
                ));
            }
            regions.push(self.frame_region(outcome.region.start(), &outcome.ty)?);
            Some(region_id)
        } else {
            None
        };
        if let Some(scratch) = layout.outcome_scratch {
            let assigned = self.regions.outcome_scratch(function.id, function.span)?;
            if assigned.status.0 as usize != regions.len() {
                return Err(lowering_diagnostic(
                    function.span,
                    "array status scratch contract order is not canonical",
                ));
            }
            regions.push(WeavyFrameRegion::new(
                scratch.status.byte_offset(),
                WeavyRegionShape::word(WeavyWordKind::Status),
            ));
            for (slot, region_id) in std::iter::once((scratch.condition, assigned.condition))
                .chain(scratch.fields.into_iter().zip(assigned.fields))
            {
                if region_id.0 as usize != regions.len() {
                    return Err(lowering_diagnostic(
                        function.span,
                        "array outcome scratch contract order is not canonical",
                    ));
                }
                regions.push(WeavyFrameRegion::new(
                    slot.byte_offset(),
                    WeavyRegionShape::word(WeavyWordKind::Scalar),
                ));
            }
        }
        for (&node, outcome) in &layout.call_outcomes {
            let region_id = self
                .regions
                .call_outcome(function.id, node, function.span)?;
            if region_id.0 as usize != regions.len() {
                return Err(lowering_diagnostic(
                    function.span,
                    "array call outcome contract order is not canonical",
                ));
            }
            regions.push(self.frame_region(outcome.region.start(), &outcome.ty)?);
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
        let result = match array_outcome {
            Some(outcome) => outcome,
            None => *node_region_ids.get(&function.output).ok_or_else(|| {
                lowering_diagnostic(function.span, "output node has no frame region")
            })?,
        };
        let call_contract = if self.closure_targets.contains(&function.id) {
            let call = self.call_contract_for_function(&entries, result, &regions)?;
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
        entries: &[WeavyRegionId],
        result: WeavyRegionId,
        regions: &[WeavyFrameRegion],
    ) -> Result<WeavyCallContractId, Diagnostics> {
        let call = WeavyCallContract {
            entries: entries
                .iter()
                .map(|entry| regions[entry.0 as usize].clone())
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
            Type::Array(_) | Type::Map { .. } | Type::Set(_) => Ok(WeavyRegionShape::word(
                WeavyWordKind::Handle(self.schema_for_type(ty, span)?),
            )),
            Type::Stream { .. } => Ok(WeavyRegionShape::default()),
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
        let schema = self.schemas_preassigned.schema_for(ty, span)?;
        let index = schema.0 as usize;
        if self.schema_ready[index] {
            return Ok(schema);
        }
        self.schema_ready[index] = true;
        let inline = match ty {
            Type::String | Type::Array(_) | Type::Map { .. } | Type::Set(_) => {
                WeavyRegionShape::word(WeavyWordKind::Handle(schema))
            }
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
            Type::Map { key, value } => {
                let row = Type::Tuple(vec![key.as_ref().clone(), value.as_ref().clone()]);
                WeavyPayloadKind::OrderedCollection(WeavyOrderedCollectionContract {
                    kind: WeavyOrderedCollectionKind::Map,
                    key: self.schema_for_type(key, span)?,
                    value: Some(self.schema_for_type(value, span)?),
                    row: self.schema_for_type(&row, span)?,
                    fanout: 2,
                })
            }
            Type::Set(element) => {
                WeavyPayloadKind::OrderedCollection(WeavyOrderedCollectionContract {
                    kind: WeavyOrderedCollectionKind::Set,
                    key: self.schema_for_type(element, span)?,
                    value: None,
                    row: self.schema_for_type(element, span)?,
                    fanout: 2,
                })
            }
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
            Type::Function { .. }
            | Type::Tuple(_)
            | Type::Record(_)
            | Type::Enum(_)
            | Type::Stream { .. } => Ok(Some(self.intern_value_shape_for_type(ty, span)?)),
            Type::Bool
            | Type::Int
            | Type::Check
            | Type::String
            | Type::Array(_)
            | Type::Map { .. }
            | Type::Set(_) => Ok(None),
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
            Type::Stream { .. } => WeavyValueShapeKind::Product { fields: Vec::new() },
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
        let result = self
            .callable_outcomes
            .then(|| ArrayOutcomeAbi::for_value(result.clone()).ty)
            .unwrap_or_else(|| result.clone());
        let mut result_region = WeavyFrameRegion::new(0, self.shape_for_type(&result, span)?);
        if let Some(value_shape) = self.value_shape_for_type(&result, span)? {
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
    schemas: &'a SchemaAssignments,
    array_map_partitions: &'a [ArrayMapPartition],
}

impl LoweringContext<'_> {
    fn array_map_shape(
        &self,
        function: FunctionId,
        node: NodeId,
        span: Span,
    ) -> Result<ArrayMapExecutionShape, Diagnostics> {
        self.array_map_partitions
            .iter()
            .find(|partition| partition.node == NodeRef { function, node })
            .map(|partition| partition.shape)
            .ok_or_else(|| lowering_diagnostic(span, "array map has no partition decision"))
    }
}

/// The contract and the instruction stream share this canonical region order.
/// Nodes are retained even for zero-width values, so later regions cannot shift
/// when a product has no fields.
struct RegionAssignments {
    nodes: BTreeMap<FunctionId, BTreeMap<NodeId, WeavyRegionId>>,
    control: BTreeMap<FunctionId, AssignedControlRegions>,
    temps: BTreeMap<FunctionId, BTreeMap<NodeId, Vec<WeavyRegionId>>>,
    ordered_cursors: BTreeMap<FunctionId, BTreeMap<NodeId, Vec<WeavyRegionId>>>,
    closures: BTreeMap<FunctionId, BTreeMap<NodeId, (WeavyRegionId, WeavyRegionId)>>,
    array_map_temps: BTreeMap<FunctionId, BTreeMap<NodeId, (WeavyRegionId, WeavyRegionId)>>,
    outcomes: BTreeMap<FunctionId, WeavyRegionId>,
    outcome_scratch: BTreeMap<FunctionId, AssignedOutcomeScratch>,
    call_outcomes: BTreeMap<FunctionId, BTreeMap<NodeId, WeavyRegionId>>,
}

#[derive(Clone, Copy)]
struct AssignedControlRegions {
    condition: WeavyRegionId,
}

#[derive(Clone, Copy)]
struct AssignedOutcomeScratch {
    status: WeavyRegionId,
    condition: WeavyRegionId,
    fields: [WeavyRegionId; 3],
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
        let mut ordered_cursors = BTreeMap::new();
        let mut closures = BTreeMap::new();
        let mut array_map_temps = BTreeMap::new();
        let mut outcomes = BTreeMap::new();
        let mut outcome_scratch = BTreeMap::new();
        let mut call_outcomes = BTreeMap::new();
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
            for (&node, regions) in &layout.typed_temps {
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
            let mut assigned_ordered_cursors = BTreeMap::new();
            for (&node, cursors) in &layout.ordered_cursors {
                let mut ids = Vec::with_capacity(cursors.len());
                for _ in cursors {
                    ids.push(WeavyRegionId(u32::try_from(next).map_err(|_| {
                        lowering_diagnostic(span, "ordered cursor assignment exceeds u32")
                    })?));
                    next = next.checked_add(1).ok_or_else(|| {
                        lowering_diagnostic(span, "ordered cursor assignment overflow")
                    })?;
                }
                assigned_ordered_cursors.insert(node, ids);
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
            let mut assigned_array_map_temps = BTreeMap::new();
            for &node in layout.array_map_temps.keys() {
                let input = WeavyRegionId(u32::try_from(next).map_err(|_| {
                    lowering_diagnostic(span, "array map temporary assignment exceeds u32")
                })?);
                next = next.checked_add(1).ok_or_else(|| {
                    lowering_diagnostic(span, "array map temporary assignment overflow")
                })?;
                let output = WeavyRegionId(u32::try_from(next).map_err(|_| {
                    lowering_diagnostic(span, "array map temporary assignment exceeds u32")
                })?);
                next = next.checked_add(1).ok_or_else(|| {
                    lowering_diagnostic(span, "array map temporary assignment overflow")
                })?;
                assigned_array_map_temps.insert(node, (input, output));
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
            if layout.array_outcome.is_some() {
                let outcome = WeavyRegionId(u32::try_from(next).map_err(|_| {
                    lowering_diagnostic(span, "array outcome region assignment exceeds u32")
                })?);
                outcomes.insert(function, outcome);
                next += 1;
                let status = WeavyRegionId(u32::try_from(next).map_err(|_| {
                    lowering_diagnostic(span, "array outcome scratch assignment exceeds u32")
                })?);
                next += 1;
                let condition = WeavyRegionId(u32::try_from(next).map_err(|_| {
                    lowering_diagnostic(span, "array outcome scratch assignment exceeds u32")
                })?);
                next += 1;
                let mut fields = [WeavyRegionId(0); 3];
                for field in &mut fields {
                    *field = WeavyRegionId(u32::try_from(next).map_err(|_| {
                        lowering_diagnostic(span, "array outcome scratch assignment exceeds u32")
                    })?);
                    next += 1;
                }
                outcome_scratch.insert(
                    function,
                    AssignedOutcomeScratch {
                        status,
                        condition,
                        fields,
                    },
                );
                let mut assigned_calls = BTreeMap::new();
                for node in layout.call_outcomes.keys() {
                    assigned_calls.insert(
                        *node,
                        WeavyRegionId(u32::try_from(next).map_err(|_| {
                            lowering_diagnostic(span, "array call outcome assignment exceeds u32")
                        })?),
                    );
                    next += 1;
                }
                call_outcomes.insert(function, assigned_calls);
            }
            nodes.insert(function, assigned);
            temps.insert(function, assigned_temps);
            ordered_cursors.insert(function, assigned_ordered_cursors);
            closures.insert(function, assigned_closures);
            array_map_temps.insert(function, assigned_array_map_temps);
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
            ordered_cursors,
            closures,
            array_map_temps,
            outcomes,
            outcome_scratch,
            call_outcomes,
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

    fn typed_temps(
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
                lowering_diagnostic(span, "VIR node has no assigned typed temporary regions")
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

    fn ordered_cursors(
        &self,
        function: FunctionId,
        node: NodeId,
        span: Span,
    ) -> Result<&[WeavyRegionId], Diagnostics> {
        self.ordered_cursors
            .get(&function)
            .and_then(|nodes| nodes.get(&node))
            .map(Vec::as_slice)
            .ok_or_else(|| lowering_diagnostic(span, "collection node has no ordered cursors"))
    }

    fn array_map_temps(
        &self,
        function: FunctionId,
        node: NodeId,
        span: Span,
    ) -> Result<(WeavyRegionId, WeavyRegionId), Diagnostics> {
        self.array_map_temps
            .get(&function)
            .and_then(|nodes| nodes.get(&node))
            .copied()
            .ok_or_else(|| lowering_diagnostic(span, "array map node has no typed temporaries"))
    }

    fn array_outcome(
        &self,
        function: FunctionId,
        span: Span,
    ) -> Result<WeavyRegionId, Diagnostics> {
        self.outcomes.get(&function).copied().ok_or_else(|| {
            lowering_diagnostic(
                span,
                "array-bearing function has no assigned outcome region",
            )
        })
    }

    fn outcome_scratch(
        &self,
        function: FunctionId,
        span: Span,
    ) -> Result<AssignedOutcomeScratch, Diagnostics> {
        self.outcome_scratch.get(&function).copied().ok_or_else(|| {
            lowering_diagnostic(span, "array-bearing function has no outcome scratch")
        })
    }

    fn call_outcome(
        &self,
        function: FunctionId,
        node: NodeId,
        span: Span,
    ) -> Result<WeavyRegionId, Diagnostics> {
        self.call_outcomes
            .get(&function)
            .and_then(|nodes| nodes.get(&node))
            .copied()
            .ok_or_else(|| lowering_diagnostic(span, "direct call has no assigned outcome region"))
    }
}

struct FunctionLayout {
    regions: BTreeMap<NodeId, FrameRegion>,
    typed_temps: BTreeMap<NodeId, Vec<TemporaryRegion>>,
    ordered_cursors: BTreeMap<NodeId, Vec<FrameRegion>>,
    closure_temps: BTreeMap<NodeId, (FrameRegion, FrameRegion)>,
    array_map_temps: BTreeMap<NodeId, (TemporaryRegion, TemporaryRegion)>,
    constant_slots: BTreeMap<NodeRef, FrameSlot>,
    scratch: Option<FrameSlot>,
    control_scratch: Option<ControlScratch>,
    array_outcome: Option<TemporaryRegion>,
    outcome_scratch: Option<OutcomeScratch>,
    call_outcomes: BTreeMap<NodeId, TemporaryRegion>,
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

#[derive(Clone, Copy)]
struct OutcomeScratch {
    status: FrameSlot,
    condition: FrameSlot,
    fields: [FrameSlot; 3],
}

impl FunctionLayout {
    fn build(
        function: FunctionId,
        nodes: &[Node],
        constants: &BTreeSet<NodeRef>,
        outcome_value: Option<Type>,
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
        let mut typed_temps = BTreeMap::new();
        for node in nodes.iter().filter(|node| {
            matches!(
                node.op,
                Op::Eq
                    | Op::Ne
                    | Op::Compare
                    | Op::ArrayAppend
                    | Op::ArrayConcat
                    | Op::ArrayFold
                    | Op::Map
                    | Op::MapAdd
                    | Op::MapConcat
                    | Op::MapWith
                    | Op::MapGet
                    | Op::MapHas
                    | Op::MapKeys
                    | Op::Set
                    | Op::SetAdd
                    | Op::SetConcat
                    | Op::SetHas
                    | Op::SetValues
                    | Op::StreamCollect
            )
        }) {
            let mut types = if matches!(node.op, Op::Eq | Op::Ne) {
                vec![Type::Int, Type::Int, Type::Int]
            } else if matches!(node.op, Op::Compare) {
                vec![Type::Int, Type::Int]
            } else if matches!(node.op, Op::ArrayFold) {
                array_fold_temporary_types(node, nodes)?
            } else if let Some(types) = collection_temporary_types(node, nodes)? {
                types
            } else if let Type::Array(element) = &node.ty {
                vec![element.as_ref().clone()]
            } else {
                return Err(lowering_diagnostic(
                    node.span,
                    "collection operation has no typed temporary layout",
                ));
            };
            if matches!(node.op, Op::Eq | Op::Ne | Op::Compare) {
                let operand = node
                    .inputs
                    .first()
                    .and_then(|input| nodes.iter().find(|candidate| candidate.id == *input))
                    .ok_or_else(|| {
                        lowering_diagnostic(node.span, "comparison node has no first operand")
                    })?;
                comparison_temporary_types(&operand.ty, &mut types);
            }
            let mut temps = Vec::with_capacity(types.len());
            for ty in types {
                let words = type_words(&ty, node.span)?;
                let region = FrameRegion::for_words(next_word, words).ok_or_else(|| {
                    lowering_diagnostic(node.span, "typed temporary region overflow")
                })?;
                next_word = next_word.checked_add(words.as_usize()).ok_or_else(|| {
                    lowering_diagnostic(node.span, "function frame size overflow")
                })?;
                temps.push(TemporaryRegion { region, ty });
            }
            typed_temps.insert(node.id, temps);
        }

        let mut ordered_cursors = BTreeMap::new();
        for node in nodes.iter().filter(|node| {
            matches!(
                node.op,
                Op::Map
                    | Op::MapAdd
                    | Op::MapConcat
                    | Op::MapWith
                    | Op::MapGet
                    | Op::MapHas
                    | Op::MapKeys
                    | Op::Set
                    | Op::SetAdd
                    | Op::SetConcat
                    | Op::SetHas
                    | Op::SetValues
                    | Op::StreamCollect
            )
        }) {
            let count = usize::from(matches!(node.op, Op::MapConcat | Op::SetConcat)) + 1;
            let mut cursors = Vec::with_capacity(count);
            for _ in 0..count {
                let region = FrameRegion::for_words(
                    next_word,
                    FrameWords::from_usize(2).expect("two cursor words"),
                )
                .ok_or_else(|| lowering_diagnostic(node.span, "ordered cursor region overflow"))?;
                next_word = next_word.checked_add(2).ok_or_else(|| {
                    lowering_diagnostic(node.span, "function frame size overflow")
                })?;
                cursors.push(region);
            }
            ordered_cursors.insert(node.id, cursors);
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
        let mut array_map_temps = BTreeMap::new();
        for node in nodes
            .iter()
            .filter(|node| matches!(node.op, Op::ArrayMap { .. }))
        {
            let source = node
                .inputs
                .first()
                .and_then(|input| nodes.iter().find(|candidate| candidate.id == *input))
                .ok_or_else(|| lowering_diagnostic(node.span, "array map source is missing"))?;
            let Type::Array(input_ty) = &source.ty else {
                return Err(lowering_diagnostic(
                    node.span,
                    "array map source does not have array type",
                ));
            };
            let Type::Array(output_ty) = &node.ty else {
                return Err(lowering_diagnostic(
                    node.span,
                    "array map result does not have array type",
                ));
            };
            let input_words = type_words(input_ty, node.span)?;
            let input_region = FrameRegion::for_words(next_word, input_words).ok_or_else(|| {
                lowering_diagnostic(node.span, "array map input temporary overflow")
            })?;
            next_word = next_word
                .checked_add(input_words.as_usize())
                .ok_or_else(|| lowering_diagnostic(node.span, "function frame size overflow"))?;
            let output_words = type_words(output_ty, node.span)?;
            let output_region =
                FrameRegion::for_words(next_word, output_words).ok_or_else(|| {
                    lowering_diagnostic(node.span, "array map output temporary overflow")
                })?;
            next_word = next_word
                .checked_add(output_words.as_usize())
                .ok_or_else(|| lowering_diagnostic(node.span, "function frame size overflow"))?;
            array_map_temps.insert(
                node.id,
                (
                    TemporaryRegion {
                        region: input_region,
                        ty: input_ty.as_ref().clone(),
                    },
                    TemporaryRegion {
                        region: output_region,
                        ty: output_ty.as_ref().clone(),
                    },
                ),
            );
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
        let array_outcome = if let Some(value) = outcome_value {
            let ty = ArrayOutcomeAbi::for_value(value).ty;
            let words = type_words(&ty, span)?;
            let region = FrameRegion::for_words(next_word, words)
                .ok_or_else(|| lowering_diagnostic(span, "array outcome frame region overflow"))?;
            next_word = next_word
                .checked_add(words.as_usize())
                .ok_or_else(|| lowering_diagnostic(span, "function frame size overflow"))?;
            Some(TemporaryRegion { region, ty })
        } else {
            None
        };
        let outcome_scratch = if array_outcome.is_some() {
            let status = FrameSlot::for_word(next_word)
                .ok_or_else(|| lowering_diagnostic(span, "array outcome scratch overflow"))?;
            next_word += 1;
            let condition = FrameSlot::for_word(next_word)
                .ok_or_else(|| lowering_diagnostic(span, "array outcome scratch overflow"))?;
            next_word += 1;
            let mut fields = [FrameSlot::for_word(0).expect("zero slot"); 3];
            for field in &mut fields {
                *field = FrameSlot::for_word(next_word)
                    .ok_or_else(|| lowering_diagnostic(span, "array outcome scratch overflow"))?;
                next_word += 1;
            }
            Some(OutcomeScratch {
                status,
                condition,
                fields,
            })
        } else {
            None
        };
        let mut call_outcomes = BTreeMap::new();
        if array_outcome.is_some() {
            for node in nodes.iter().filter(|node| {
                matches!(
                    node.op,
                    Op::Call(_) | Op::CallValue | Op::ArrayMap { .. } | Op::ArrayFold
                )
            }) {
                let value_ty = match &node.op {
                    Op::ArrayMap { .. } => node
                        .ty
                        .array_element()
                        .ok_or_else(|| {
                            lowering_diagnostic(node.span, "array map result is not an array")
                        })?
                        .clone(),
                    _ => node.ty.clone(),
                };
                let ty = ArrayOutcomeAbi::for_value(value_ty).ty;
                let words = type_words(&ty, node.span)?;
                let region = FrameRegion::for_words(next_word, words).ok_or_else(|| {
                    lowering_diagnostic(node.span, "array call outcome frame region overflow")
                })?;
                next_word += words.as_usize();
                call_outcomes.insert(node.id, TemporaryRegion { region, ty });
            }
        }
        let frame_size = FrameSlot::frame_size(next_word)
            .ok_or_else(|| lowering_diagnostic(span, "function frame size overflow"))?;
        Ok(Self {
            regions,
            typed_temps,
            ordered_cursors,
            closure_temps,
            array_map_temps,
            constant_slots,
            scratch,
            control_scratch,
            array_outcome,
            outcome_scratch,
            call_outcomes,
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

    fn typed_temps(&self, node: NodeId, span: Span) -> Result<&[TemporaryRegion], Diagnostics> {
        self.typed_temps
            .get(&node)
            .map(Vec::as_slice)
            .ok_or_else(|| lowering_diagnostic(span, "VIR node has no typed temporary regions"))
    }

    fn ordered_cursors(&self, node: NodeId, span: Span) -> Result<&[FrameRegion], Diagnostics> {
        self.ordered_cursors
            .get(&node)
            .map(Vec::as_slice)
            .ok_or_else(|| lowering_diagnostic(span, "VIR node has no ordered cursor regions"))
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

    fn array_map_temps(
        &self,
        node: NodeId,
        span: Span,
    ) -> Result<&(TemporaryRegion, TemporaryRegion), Diagnostics> {
        self.array_map_temps
            .get(&node)
            .ok_or_else(|| lowering_diagnostic(span, "array map node has no temporary layout"))
    }
}

fn array_fold_temporary_types(node: &Node, nodes: &[Node]) -> Result<Vec<Type>, Diagnostics> {
    if node.inputs.len() != 3 {
        return Err(lowering_diagnostic(
            node.span,
            "array fold does not have source, initial value, and callable inputs",
        ));
    }
    let source = nodes
        .iter()
        .find(|candidate| candidate.id == node.inputs[0])
        .ok_or_else(|| lowering_diagnostic(node.span, "array fold source is unavailable"))?;
    let Type::Array(element) = &source.ty else {
        return Err(lowering_diagnostic(
            node.span,
            "array fold source is not an array",
        ));
    };
    let initial = nodes
        .iter()
        .find(|candidate| candidate.id == node.inputs[1])
        .ok_or_else(|| lowering_diagnostic(node.span, "array fold initial value is unavailable"))?;
    if initial.ty != node.ty {
        return Err(lowering_diagnostic(
            node.span,
            "array fold result does not match its initial value",
        ));
    }
    Ok(vec![
        element.as_ref().clone(),
        Type::Tuple(vec![node.ty.clone(), element.as_ref().clone()]),
        node.ty.clone(),
    ])
}

fn collection_temporary_types(
    node: &Node,
    nodes: &[Node],
) -> Result<Option<Vec<Type>>, Diagnostics> {
    let collection = match node.op {
        Op::Map | Op::StreamCollect => node.ty.clone(),
        Op::MapAdd | Op::MapConcat | Op::MapWith | Op::MapGet | Op::MapHas | Op::MapKeys => node
            .inputs
            .first()
            .and_then(|input| nodes.iter().find(|candidate| candidate.id == *input))
            .map(|input| input.ty.clone())
            .unwrap_or_else(|| node.ty.clone()),
        Op::Set => node.ty.clone(),
        Op::SetAdd | Op::SetConcat | Op::SetHas | Op::SetValues => node
            .inputs
            .first()
            .and_then(|input| nodes.iter().find(|candidate| candidate.id == *input))
            .map(|input| input.ty.clone())
            .unwrap_or_else(|| node.ty.clone()),
        _ => return Ok(None),
    };
    let mut types = vec![
        Type::Int,
        Type::Int,
        Type::Int,
        collection.clone(),
        collection.clone(),
        collection.clone(),
    ];
    match collection {
        Type::Map { key, value } => {
            let row = Type::Tuple(vec![key.as_ref().clone(), value.as_ref().clone()]);
            types.extend([
                row,
                key.as_ref().clone(),
                key.as_ref().clone(),
                value.as_ref().clone(),
            ]);
            comparison_temporary_types(&key, &mut types);
        }
        Type::Set(element) => {
            types.extend([element.as_ref().clone(), element.as_ref().clone()]);
            comparison_temporary_types(&element, &mut types);
        }
        _ => {
            return Err(lowering_diagnostic(
                node.span,
                "collection operation receiver is not Map or Set",
            ));
        }
    }
    Ok(Some(types))
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
        Type::Array(element) => {
            out.extend([Type::Int, Type::Int, Type::Int]);
            out.push(element.as_ref().clone());
            out.push(element.as_ref().clone());
            comparison_temporary_types(element, out);
        }
        Type::Bool
        | Type::Int
        | Type::Check
        | Type::String
        | Type::Map { .. }
        | Type::Set(_)
        | Type::Function { .. }
        | Type::StreamCheck
        | Type::Stream { .. } => {}
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
    let array_return = layout.array_outcome.as_ref().map(|_| code.label());
    {
        let sequence = SequenceContext {
            nodes: &nodes_by_id,
            function: &function_context,
            lowering: context,
            array_return,
        };
        let mut outputs = SequenceOutputs {
            constants,
            code: &mut code,
        };
        lower_node_sequence(&node_ids, &mut values, &sequence, &mut outputs, None)?;
    }
    let output_value = values.get(&output).cloned().ok_or_else(|| {
        lowering_diagnostic(output_node.span, "function output has no frame region")
    })?;
    let previous_source = code.swap_source(Some(NodeRef {
        function,
        node: output,
    }));
    if let Some(outcome) = &layout.array_outcome {
        let outcome_region = context.regions.array_outcome(function, output_node.span)?;
        code.push(WeavyOp::EnumConstruct {
            dst: outcome_region,
            variant: 0,
            fields: vec![StructuralFieldSource {
                field: 0,
                source: output_value.region_id,
            }],
        });
        code.bind(
            array_return.expect("array outcome has return label"),
            output_node.span,
        )?;
        code.push(WeavyOp::Ret {
            src: outcome.region.start().byte_offset(),
            size: outcome.region.byte_size().ok_or_else(|| {
                lowering_diagnostic(output_node.span, "array outcome return size overflow")
            })?,
        });
    } else {
        code.push(WeavyOp::Ret {
            src: output_value.region.start().byte_offset(),
            size: output_value
                .region
                .byte_size()
                .ok_or_else(|| lowering_diagnostic(output_node.span, "return size overflow"))?,
        });
    }
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
    array_return: Option<CodeLabel>,
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
            Op::Call(callee) if sequence.function.layout.array_outcome.is_some() => {
                lower_array_call_node(node, dst_region_id, values, *callee, sequence, outputs)?
            }
            Op::CallValue if sequence.function.layout.array_outcome.is_some() => {
                lower_checked_call_value_node(node, dst, dst_region_id, values, sequence, outputs)?
            }
            Op::Array | Op::ArrayIndex | Op::ArrayLen | Op::ArrayMap { .. } => {
                lower_partitioned_array_node(node, dst, dst_region_id, values, sequence, outputs)?
            }
            Op::ArrayAppend | Op::ArrayConcat => {
                lower_checked_array_node(node, dst, values, sequence, outputs)?
            }
            Op::ArrayFold => {
                lower_checked_array_fold_node(node, dst, dst_region_id, values, sequence, outputs)?
            }
            Op::Map
            | Op::MapAdd
            | Op::MapConcat
            | Op::MapWith
            | Op::MapGet
            | Op::MapHas
            | Op::MapLen
            | Op::MapKeys
            | Op::Set
            | Op::SetAdd
            | Op::SetConcat
            | Op::SetHas
            | Op::SetLen
            | Op::SetValues
            | Op::StreamCollect => {
                lower_checked_collection_node(node, dst, dst_region_id, values, sequence, outputs)?
            }
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
                let mut lowering = NodeLoweringContext {
                    function: sequence.function,
                    context: sequence.lowering,
                    constants: outputs.constants,
                };
                let lowered = lower_node(node, dst, dst_region_id, values, &mut lowering)?;
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
        variant: ORDERING_LESS_VARIANT,
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
        variant: ORDERING_EQUAL_VARIANT,
        fields: Vec::new(),
    });
    outputs.code.jump(done);
    outputs.code.bind(not_equal, node.span)?;
    outputs.code.push(WeavyOp::EnumConstruct {
        dst: dst_region,
        variant: ORDERING_GREATER_VARIANT,
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
        Type::Map { .. } | Type::Set(_) => {
            return Err(lowering_diagnostic(
                node.span,
                "map/set comparison lowering is not implemented",
            ));
        }
        Type::Function { .. } => {
            return Err(lowering_diagnostic(
                node.span,
                "function comparison requires stable closure identity",
            ));
        }
        Type::Check | Type::StreamCheck | Type::Stream { .. } => {
            return Err(lowering_diagnostic(
                node.span,
                "comparison reached a non-orderable VIR type",
            ));
        }
    }
    Ok(())
}

fn emit_structural_order(
    node: &Node,
    ty: &Type,
    a: &LoweredSlot,
    b: &LoweredSlot,
    dst: FrameSlot,
    condition: FrameSlot,
    temps: &mut TemporaryCursor<'_>,
    code: &mut CodeBuilder,
) -> Result<(), Diagnostics> {
    let mut leaves = Vec::new();
    collect_typed_compare_leaves(node, ty, a, b, temps, code, &mut leaves)?;
    let done = code.label();
    if leaves.is_empty() {
        code.push(WeavyOp::ConstI64 {
            dst: dst.byte_offset(),
            value: i64::from(ORDERING_EQUAL_VARIANT),
        });
    }
    for (index, leaf) in leaves.iter().enumerate() {
        let last = index + 1 == leaves.len();
        match leaf.kind {
            CompareLeafKind::SignedWord => {
                let not_less = code.label();
                code.push(WeavyOp::LtI64 {
                    dst: condition.byte_offset(),
                    a: leaf.a.byte_offset(),
                    b: leaf.b.byte_offset(),
                });
                code.jump_if_zero(condition, not_less);
                code.push(WeavyOp::ConstI64 {
                    dst: dst.byte_offset(),
                    value: i64::from(ORDERING_LESS_VARIANT),
                });
                code.jump(done);
                code.bind(not_less, node.span)?;
                let equal = code.label();
                code.push(WeavyOp::GtI64 {
                    dst: condition.byte_offset(),
                    a: leaf.a.byte_offset(),
                    b: leaf.b.byte_offset(),
                });
                code.jump_if_zero(condition, equal);
                code.push(WeavyOp::ConstI64 {
                    dst: dst.byte_offset(),
                    value: i64::from(ORDERING_GREATER_VARIANT),
                });
                code.jump(done);
                code.bind(equal, node.span)?;
                if last {
                    code.push(WeavyOp::ConstI64 {
                        dst: dst.byte_offset(),
                        value: i64::from(ORDERING_EQUAL_VARIANT),
                    });
                }
            }
            CompareLeafKind::ValueBytes => {
                code.push(WeavyOp::CompareValueBytes {
                    dst: dst.byte_offset(),
                    a: leaf.a.byte_offset(),
                    b: leaf.b.byte_offset(),
                });
                if !last {
                    code.push(WeavyOp::ConstI64 {
                        dst: condition.byte_offset(),
                        value: i64::from(ORDERING_EQUAL_VARIANT),
                    });
                    code.push(WeavyOp::EqI64 {
                        dst: condition.byte_offset(),
                        a: dst.byte_offset(),
                        b: condition.byte_offset(),
                    });
                    code.jump_if_zero(condition, done);
                }
            }
        }
    }
    code.bind(done, node.span)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ValueRepresentation {
    Word,
    RealizedHandle,
    InlineComposite,
    CodataRecipe,
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
        let regions = layout.typed_temps(node.id, node.span)?;
        let ids = assignments.typed_temps(function, node.id, node.span)?;
        if regions.len() != ids.len() {
            return Err(lowering_diagnostic(
                node.span,
                "typed temporary regions and contracts differ",
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
            lowering_diagnostic(span, "typed temporary region has no contract id")
        })?;
        self.next += 1;
        if &region.ty != ty {
            return Err(lowering_diagnostic(
                span,
                "typed temporary does not match the requested value type",
            ));
        }
        Ok(LoweredSlot {
            region: region.region,
            region_id,
            ty: ty.clone(),
            representation: representation_for_type(ty, span)?,
        })
    }

    fn checkpoint(&self) -> usize {
        self.next
    }

    fn rewind(&mut self, checkpoint: usize, span: Span) -> Result<(), Diagnostics> {
        if checkpoint > self.regions.len() {
            return Err(lowering_diagnostic(
                span,
                "typed temporary checkpoint is out of range",
            ));
        }
        self.next = checkpoint;
        Ok(())
    }

    fn drain(&mut self, span: Span) -> Result<(), Diagnostics> {
        while self.next < self.regions.len() {
            let ty = self.regions[self.next].ty.clone();
            self.take(&ty, span)?;
        }
        Ok(())
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

struct NodeLoweringContext<'function, 'lowering, 'constants> {
    function: &'function FunctionLoweringContext<'function>,
    context: &'lowering LoweringContext<'lowering>,
    constants: &'constants mut BTreeMap<NodeRef, PendingValueConstant>,
}

fn lower_node(
    node: &Node,
    dst: FrameRegion,
    dst_region_id: WeavyRegionId,
    values: &BTreeMap<NodeId, LoweredSlot>,
    lowering: &mut NodeLoweringContext<'_, '_, '_>,
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
                function: lowering.function.id,
                node: node.id,
            };
            if lowering
                .function
                .layout
                .constant_slot(constant, node.span)?
                != dst_slot
            {
                return Err(lowering_diagnostic(
                    node.span,
                    "String node does not occupy its local closure slot",
                ));
            }
            let root_layout = lowering
                .context
                .layouts
                .get(&lowering.context.root_function)
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
            if let Some(previous) = lowering.constants.insert(constant, pending)
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
            let parameter = lowering
                .function
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
            let op = lower_call_node(
                node,
                dst_region,
                values,
                *callee,
                lowering.function.layout,
                lowering.context,
            )?;
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
            let target = lowering
                .context
                .functions
                .get(callee)
                .copied()
                .ok_or_else(|| {
                    lowering_diagnostic(node.span, "closure function is absent from the island")
                })?;
            let target_layout = lowering.context.layouts.get(callee).ok_or_else(|| {
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
            let callee = lowering
                .context
                .function_ids
                .get(callee)
                .copied()
                .ok_or_else(|| {
                    lowering_diagnostic(node.span, "closure function has no local ABI id")
                })?;
            let (callee_region, environment_region) =
                lowering
                    .context
                    .regions
                    .closure(lowering.function.id, node.id, node.span)?;
            let (callee_temp, environment_temp) =
                lowering.function.layout.closure_temps(node.id, node.span)?;
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
        Op::ArrayMap { .. } => {
            return Err(lowering_diagnostic(
                node.span,
                "array map lowering is not implemented",
            ));
        }
        Op::ArrayFold => {
            return Err(lowering_diagnostic(
                node.span,
                "array fold did not reach checked lowering",
            ));
        }
        Op::ArrayStream => {
            require_input_count(node, 1)?;
            let source = input_value(node, values, 0)?;
            let Type::Array(element) = &source.ty else {
                return Err(lowering_diagnostic(
                    node.span,
                    "array stream source is not an array",
                ));
            };
            require_value(
                node,
                &source,
                &Type::array(element.as_ref().clone()),
                ValueRepresentation::RealizedHandle,
            )?;
            require_node_type(node, Type::stream(Type::Int, element.as_ref().clone()))?;
            (Vec::new(), ValueRepresentation::CodataRecipe)
        }
        Op::StreamCollect => {
            return Err(lowering_diagnostic(
                node.span,
                "stream collect did not reach checked lowering",
            ));
        }
        Op::ArrayAppend | Op::ArrayConcat => {
            return Err(lowering_diagnostic(
                node.span,
                "collection addition lowering is not implemented",
            ));
        }
        Op::Map
        | Op::Set
        | Op::MapAdd
        | Op::MapConcat
        | Op::MapWith
        | Op::MapGet
        | Op::MapHas
        | Op::MapLen
        | Op::MapKeys
        | Op::SetAdd
        | Op::SetConcat
        | Op::SetHas
        | Op::SetLen
        | Op::SetValues => {
            return Err(lowering_diagnostic(
                node.span,
                "map/set lowering is not implemented",
            ));
        }
        Op::StringConcat => {
            return Err(lowering_diagnostic(
                node.span,
                "string concatenation lowering is not implemented",
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

fn lower_array_call_node(
    node: &Node,
    dst_region: WeavyRegionId,
    values: &BTreeMap<NodeId, LoweredSlot>,
    callee: FunctionId,
    sequence: &SequenceContext<'_, '_, '_>,
    outputs: &mut SequenceOutputs<'_, '_>,
) -> Result<ValueRepresentation, Diagnostics> {
    let target = sequence
        .lowering
        .functions
        .get(&callee)
        .copied()
        .ok_or_else(|| {
            lowering_diagnostic(
                node.span,
                "called function is absent from the island closure",
            )
        })?;
    if target.return_type != node.ty {
        return Err(lowering_diagnostic(
            node.span,
            "call result does not match the called function",
        ));
    }
    require_input_count(node, target.parameters.len())?;
    let target_layout = sequence
        .lowering
        .layouts
        .get(&callee)
        .ok_or_else(|| lowering_diagnostic(node.span, "called function has no frame layout"))?;
    let call_outcome = sequence
        .function
        .layout
        .call_outcomes
        .get(&node.id)
        .ok_or_else(|| lowering_diagnostic(node.span, "direct call has no typed outcome layout"))?;
    let call_outcome_id =
        sequence
            .lowering
            .regions
            .call_outcome(sequence.function.id, node.id, node.span)?;
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
        let source_slot = sequence
            .function
            .layout
            .constant_slot(constant, node.span)?;
        args.push(ArgCopy {
            src: source_slot.byte_offset(),
            dst: target_slot.byte_offset(),
            size: FrameSlot::word_size(),
        });
    }
    let callee = *sequence
        .lowering
        .function_ids
        .get(&callee)
        .ok_or_else(|| lowering_diagnostic(node.span, "called function has no local ABI id"))?;
    let scratch = sequence.function.layout.outcome_scratch.ok_or_else(|| {
        lowering_diagnostic(node.span, "array-bearing function has no outcome scratch")
    })?;
    let assigned = sequence
        .lowering
        .regions
        .outcome_scratch(sequence.function.id, node.span)?;
    let own_outcome = sequence
        .lowering
        .regions
        .array_outcome(sequence.function.id, node.span)?;
    let return_label = sequence.array_return.ok_or_else(|| {
        lowering_diagnostic(
            node.span,
            "array-bearing function has no outcome return label",
        )
    })?;
    let failures = outputs.code.label();
    let done = outputs.code.label();
    outputs.code.push(WeavyOp::Call {
        callee: WeavyFnId(callee),
        args,
        ret: call_outcome.region.start().byte_offset(),
    });
    outputs.code.push(WeavyOp::EnumIsVariant {
        dst: assigned.condition,
        value: call_outcome_id,
        variant: 0,
    });
    outputs.code.jump_if_zero(scratch.condition, failures);
    outputs.code.push(WeavyOp::EnumProjectChecked {
        dst: dst_region,
        value: call_outcome_id,
        variant: 0,
        field: 0,
    });
    outputs.code.jump(done);
    outputs.code.bind(failures, node.span)?;
    propagate_checked_call_failure(
        node,
        call_outcome_id,
        own_outcome,
        scratch,
        assigned,
        return_label,
        outputs.code,
    )?;
    outputs.code.bind(done, node.span)?;
    representation_for_type(&node.ty, node.span)
}

fn lower_partitioned_array_node(
    node: &Node,
    dst: FrameRegion,
    dst_region_id: WeavyRegionId,
    values: &BTreeMap<NodeId, LoweredSlot>,
    sequence: &SequenceContext<'_, '_, '_>,
    outputs: &mut SequenceOutputs<'_, '_>,
) -> Result<ValueRepresentation, Diagnostics> {
    match node.op {
        Op::ArrayMap { .. } => {
            match sequence
                .lowering
                .array_map_shape(sequence.function.id, node.id, node.span)?
            {
                ArrayMapExecutionShape::FusedProjection => {
                    validate_array_map(node, values)?;
                    Ok(ValueRepresentation::RealizedHandle)
                }
                ArrayMapExecutionShape::MaterializedLoop => {
                    lower_materialized_array_map(node, dst, values, sequence, outputs)
                }
            }
        }
        Op::ArrayIndex | Op::ArrayLen => {
            let map = node
                .inputs
                .first()
                .and_then(|input| sequence.nodes.get(input).copied())
                .filter(|input| matches!(input.op, Op::ArrayMap { .. }));
            if let Some(map) = map
                && sequence
                    .lowering
                    .array_map_shape(sequence.function.id, map.id, map.span)?
                    == ArrayMapExecutionShape::FusedProjection
            {
                return lower_fused_array_map_projection(
                    node,
                    map,
                    dst,
                    dst_region_id,
                    values,
                    sequence,
                    outputs,
                );
            }
            lower_checked_array_node(node, dst, values, sequence, outputs)
        }
        Op::Array => lower_checked_array_node(node, dst, values, sequence, outputs),
        _ => unreachable!("partitioned array lowering receives only array operations"),
    }
}

fn validate_array_map(
    map: &Node,
    values: &BTreeMap<NodeId, LoweredSlot>,
) -> Result<(), Diagnostics> {
    require_input_count(map, 2)?;
    let source = input_value(map, values, 0)?;
    let mapper = input_value(map, values, 1)?;
    let Type::Array(input) = &source.ty else {
        return Err(lowering_diagnostic(
            map.span,
            "array map source is not an array",
        ));
    };
    let Type::Array(output) = &map.ty else {
        return Err(lowering_diagnostic(
            map.span,
            "array map result is not an array",
        ));
    };
    let Type::Function { parameter, result } = &mapper.ty else {
        return Err(lowering_diagnostic(
            map.span,
            "array map mapper is not callable",
        ));
    };
    if parameter.as_ref() != input.as_ref() || result.as_ref() != output.as_ref() {
        return Err(lowering_diagnostic(
            map.span,
            "array map callable signature does not match its array types",
        ));
    }
    require_value(
        map,
        &source,
        &source.ty,
        ValueRepresentation::RealizedHandle,
    )?;
    require_value(
        map,
        &mapper,
        &mapper.ty,
        ValueRepresentation::InlineComposite,
    )
}

fn array_map_temporary_slots(
    map: &Node,
    sequence: &SequenceContext<'_, '_, '_>,
) -> Result<(LoweredSlot, LoweredSlot), Diagnostics> {
    let (input, output) = sequence.function.layout.array_map_temps(map.id, map.span)?;
    let (input_id, output_id) =
        sequence
            .lowering
            .regions
            .array_map_temps(sequence.function.id, map.id, map.span)?;
    Ok((
        LoweredSlot {
            region: input.region,
            region_id: input_id,
            ty: input.ty.clone(),
            representation: representation_for_type(&input.ty, map.span)?,
        },
        LoweredSlot {
            region: output.region,
            region_id: output_id,
            ty: output.ty.clone(),
            representation: representation_for_type(&output.ty, map.span)?,
        },
    ))
}

fn emit_checked_call_indirect(
    node: &Node,
    mapper: &LoweredSlot,
    input: &LoweredSlot,
    output: &LoweredSlot,
    sequence: &SequenceContext<'_, '_, '_>,
    outputs: &mut SequenceOutputs<'_, '_>,
) -> Result<(), Diagnostics> {
    let Type::Function { parameter, result } = &mapper.ty else {
        return Err(lowering_diagnostic(
            node.span,
            "indirect callee is not callable",
        ));
    };
    require_value(
        node,
        input,
        parameter,
        representation_for_type(parameter, node.span)?,
    )?;
    require_value(
        node,
        output,
        result,
        representation_for_type(result, node.span)?,
    )?;
    let call_outcome = sequence
        .function
        .layout
        .call_outcomes
        .get(&node.id)
        .ok_or_else(|| lowering_diagnostic(node.span, "indirect call has no outcome layout"))?;
    let call_outcome_id =
        sequence
            .lowering
            .regions
            .call_outcome(sequence.function.id, node.id, node.span)?;
    let scratch = sequence.function.layout.outcome_scratch.ok_or_else(|| {
        lowering_diagnostic(node.span, "indirect call has no checked outcome scratch")
    })?;
    let assigned = sequence
        .lowering
        .regions
        .outcome_scratch(sequence.function.id, node.span)?;
    let own_outcome = sequence
        .lowering
        .regions
        .array_outcome(sequence.function.id, node.span)?;
    let return_label = sequence.array_return.ok_or_else(|| {
        lowering_diagnostic(node.span, "indirect call has no checked outcome return")
    })?;
    let mut args = vec![ArgCopy {
        src: input.region.start().byte_offset(),
        dst: 0,
        size: input
            .region
            .byte_size()
            .ok_or_else(|| lowering_diagnostic(node.span, "indirect argument size overflow"))?,
    }];
    if let Some(target) = static_closure_target(mapper, sequence) {
        let target_layout =
            sequence.lowering.layouts.get(&target).ok_or_else(|| {
                lowering_diagnostic(node.span, "closure target has no frame layout")
            })?;
        for (&constant, &destination) in &target_layout.constant_slots {
            let source = sequence
                .function
                .layout
                .constant_slot(constant, node.span)?;
            args.push(ArgCopy {
                src: source.byte_offset(),
                dst: destination.byte_offset(),
                size: FrameSlot::word_size(),
            });
        }
    }
    outputs.code.push(WeavyOp::CallIndirect {
        callee: mapper.region.start().byte_offset(),
        args,
        ret: call_outcome.region.start().byte_offset(),
    });
    let failures = outputs.code.label();
    let done = outputs.code.label();
    outputs.code.push(WeavyOp::EnumIsVariant {
        dst: assigned.condition,
        value: call_outcome_id,
        variant: 0,
    });
    outputs.code.jump_if_zero(scratch.condition, failures);
    outputs.code.push(WeavyOp::EnumProjectChecked {
        dst: output.region_id,
        value: call_outcome_id,
        variant: 0,
        field: 0,
    });
    outputs.code.jump(done);
    outputs.code.bind(failures, node.span)?;
    propagate_checked_call_failure(
        node,
        call_outcome_id,
        own_outcome,
        scratch,
        assigned,
        return_label,
        outputs.code,
    )?;
    outputs.code.bind(done, node.span)
}

fn static_closure_target(
    mapper: &LoweredSlot,
    sequence: &SequenceContext<'_, '_, '_>,
) -> Option<FunctionId> {
    sequence.nodes.values().copied().find_map(|candidate| {
        let Op::Closure(target) = candidate.op else {
            return None;
        };
        (candidate.ty == mapper.ty
            && sequence
                .function
                .layout
                .region(candidate.id, candidate.span)
                .ok()
                == Some(mapper.region))
        .then_some(target)
    })
}

fn propagate_checked_call_failure(
    node: &Node,
    call_outcome: WeavyRegionId,
    own_outcome: WeavyRegionId,
    scratch: OutcomeScratch,
    assigned: AssignedOutcomeScratch,
    return_label: CodeLabel,
    code: &mut CodeBuilder,
) -> Result<(), Diagnostics> {
    for (variant, field_count) in [
        (1u32, 3usize),
        (2u32, 2usize),
        (3u32, 1usize),
        (4u32, 1usize),
        (5u32, 2usize),
    ] {
        let next = code.label();
        code.push(WeavyOp::EnumIsVariant {
            dst: assigned.condition,
            value: call_outcome,
            variant,
        });
        code.jump_if_zero(scratch.condition, next);
        for field in 0..field_count {
            code.push(WeavyOp::EnumProjectChecked {
                dst: assigned.fields[field],
                value: call_outcome,
                variant,
                field: field as u32,
            });
        }
        code.push(WeavyOp::EnumConstruct {
            dst: own_outcome,
            variant,
            fields: (0..field_count)
                .map(|field| StructuralFieldSource {
                    field: field as u32,
                    source: assigned.fields[field],
                })
                .collect(),
        });
        code.jump(return_label);
        code.bind(next, node.span)?;
    }
    // Every selector probe validates the closed enum. Reaching this operation
    // is impossible for a valid selector and remains a typed dynamic fault.
    code.push(WeavyOp::EnumProjectChecked {
        dst: assigned.fields[0],
        value: call_outcome,
        variant: 5,
        field: 0,
    });
    Ok(())
}

fn lower_checked_call_value_node(
    node: &Node,
    dst: FrameRegion,
    dst_region_id: WeavyRegionId,
    values: &BTreeMap<NodeId, LoweredSlot>,
    sequence: &SequenceContext<'_, '_, '_>,
    outputs: &mut SequenceOutputs<'_, '_>,
) -> Result<ValueRepresentation, Diagnostics> {
    require_input_count(node, 2)?;
    let callee = input_value(node, values, 0)?;
    let argument = input_value(node, values, 1)?;
    let output = LoweredSlot {
        region: dst,
        region_id: dst_region_id,
        ty: node.ty.clone(),
        representation: representation_for_type(&node.ty, node.span)?,
    };
    emit_checked_call_indirect(node, &callee, &argument, &output, sequence, outputs)?;
    Ok(output.representation)
}

fn array_map_array_facts(
    map: &Node,
    values: &BTreeMap<NodeId, LoweredSlot>,
    sequence: &SequenceContext<'_, '_, '_>,
) -> Result<(LoweredSlot, LoweredSlot, Type, Type, i64, i64, u32, u32), Diagnostics> {
    validate_array_map(map, values)?;
    let source = input_value(map, values, 0)?;
    let mapper = input_value(map, values, 1)?;
    let Type::Array(input_ty) = &source.ty else {
        unreachable!("validated array map source")
    };
    let Type::Array(output_ty) = &map.ty else {
        unreachable!("validated array map result")
    };
    let input_ty = input_ty.as_ref().clone();
    let output_ty = output_ty.as_ref().clone();
    let input_schema = sequence.lowering.schemas.schema_for(&input_ty, map.span)?;
    let output_schema = sequence.lowering.schemas.schema_for(&output_ty, map.span)?;
    let word_bytes = usize::try_from(FrameSlot::word_size()).expect("word size fits usize");
    let input_width = u32::try_from(type_words(&input_ty, map.span)?.as_usize() * word_bytes)
        .map_err(|_| lowering_diagnostic(map.span, "array map input width overflow"))?;
    let output_width = u32::try_from(type_words(&output_ty, map.span)?.as_usize() * word_bytes)
        .map_err(|_| lowering_diagnostic(map.span, "array map output width overflow"))?;
    Ok((
        source,
        mapper,
        input_ty,
        output_ty,
        i64::from(input_schema.0),
        i64::from(output_schema.0),
        input_width,
        output_width,
    ))
}

fn lower_materialized_array_map(
    map: &Node,
    dst: FrameRegion,
    values: &BTreeMap<NodeId, LoweredSlot>,
    sequence: &SequenceContext<'_, '_, '_>,
    outputs: &mut SequenceOutputs<'_, '_>,
) -> Result<ValueRepresentation, Diagnostics> {
    // r[impl lang.collection.array-map]
    let (source, mapper, _, _, input_schema, output_schema, input_width, output_width) =
        array_map_array_facts(map, values, sequence)?;
    let (input, output) = array_map_temporary_slots(map, sequence)?;
    let scratch =
        sequence.function.layout.outcome_scratch.ok_or_else(|| {
            lowering_diagnostic(map.span, "array map has no checked outcome scratch")
        })?;
    let assigned = sequence
        .lowering
        .regions
        .outcome_scratch(sequence.function.id, map.span)?;
    let outcome = sequence
        .lowering
        .regions
        .array_outcome(sequence.function.id, map.span)?;
    let return_label = sequence
        .array_return
        .ok_or_else(|| lowering_diagnostic(map.span, "array map has no checked outcome return"))?;
    let site = sequence
        .lowering
        .trace_ids
        .get(&NodeRef {
            function: sequence.function.id,
            node: map.id,
        })
        .copied()
        .ok_or_else(|| lowering_diagnostic(map.span, "array map has no stable trace site"))?;

    outputs.code.push(WeavyOp::LoadArrayLen {
        dst: scratch.fields[2].byte_offset(),
        status: scratch.status.byte_offset(),
        array: source.region.start().byte_offset(),
        elem_schema_ref: input_schema,
    });
    emit_array_status_machine_checks(
        map,
        site,
        scratch,
        assigned,
        outcome,
        return_label,
        outputs.code,
    )?;
    outputs.code.push(WeavyOp::ArrayNew {
        dst: dst.start().byte_offset(),
        status: scratch.status.byte_offset(),
        count_slot: scratch.fields[2].byte_offset(),
        elem_width: output_width,
        elem_schema_ref: output_schema,
    });
    emit_array_status_machine_checks(
        map,
        site,
        scratch,
        assigned,
        outcome,
        return_label,
        outputs.code,
    )?;
    outputs.code.push(WeavyOp::ConstI64 {
        dst: scratch.fields[0].byte_offset(),
        value: 0,
    });
    outputs.code.push(WeavyOp::ConstI64 {
        dst: scratch.fields[1].byte_offset(),
        value: 1,
    });
    let loop_start = outputs.code.label();
    let done = outputs.code.label();
    outputs.code.bind(loop_start, map.span)?;
    outputs.code.push(WeavyOp::LtI64 {
        dst: scratch.condition.byte_offset(),
        a: scratch.fields[0].byte_offset(),
        b: scratch.fields[2].byte_offset(),
    });
    outputs.code.jump_if_zero(scratch.condition, done);
    outputs.code.push(WeavyOp::LoadArray {
        dst: input.region.start().byte_offset(),
        status: scratch.status.byte_offset(),
        array: source.region.start().byte_offset(),
        index: scratch.fields[0].byte_offset(),
        elem_width: input_width,
        elem_schema_ref: input_schema,
    });
    emit_array_load_status_checks(ArrayLoadStatusContext {
        node: map,
        site,
        index: assigned.fields[0],
        length: assigned.fields[2],
        scratch,
        assigned,
        outcome,
        return_label,
        code: outputs.code,
    })?;
    emit_checked_call_indirect(map, &mapper, &input, &output, sequence, outputs)?;
    outputs.code.push(WeavyOp::ArrayStore {
        status: scratch.status.byte_offset(),
        array: dst.start().byte_offset(),
        index: scratch.fields[0].byte_offset(),
        src: output.region.start().byte_offset(),
        elem_width: output_width,
        elem_schema_ref: output_schema,
    });
    emit_array_status_machine_checks(
        map,
        site,
        scratch,
        assigned,
        outcome,
        return_label,
        outputs.code,
    )?;
    outputs.code.push(WeavyOp::AddI64 {
        dst: scratch.fields[0].byte_offset(),
        a: scratch.fields[0].byte_offset(),
        b: scratch.fields[1].byte_offset(),
    });
    outputs.code.jump(loop_start);
    outputs.code.bind(done, map.span)?;
    Ok(ValueRepresentation::RealizedHandle)
}

fn lower_fused_array_map_projection(
    node: &Node,
    map: &Node,
    dst: FrameRegion,
    dst_region_id: WeavyRegionId,
    values: &BTreeMap<NodeId, LoweredSlot>,
    sequence: &SequenceContext<'_, '_, '_>,
    outputs: &mut SequenceOutputs<'_, '_>,
) -> Result<ValueRepresentation, Diagnostics> {
    let (source, mapper, _, output_ty, input_schema, _, input_width, _) =
        array_map_array_facts(map, values, sequence)?;
    let (input, output) = array_map_temporary_slots(map, sequence)?;
    let scratch = sequence.function.layout.outcome_scratch.ok_or_else(|| {
        lowering_diagnostic(node.span, "fused array map has no checked outcome scratch")
    })?;
    let assigned = sequence
        .lowering
        .regions
        .outcome_scratch(sequence.function.id, node.span)?;
    let outcome = sequence
        .lowering
        .regions
        .array_outcome(sequence.function.id, node.span)?;
    let return_label = sequence.array_return.ok_or_else(|| {
        lowering_diagnostic(node.span, "fused array map has no checked outcome return")
    })?;
    let site = sequence
        .lowering
        .trace_ids
        .get(&NodeRef {
            function: sequence.function.id,
            node: node.id,
        })
        .copied()
        .ok_or_else(|| lowering_diagnostic(node.span, "projection has no stable trace site"))?;
    match node.op {
        Op::ArrayLen => {
            require_input_count(node, 1)?;
            outputs.code.push(WeavyOp::LoadArrayLen {
                dst: dst.start().byte_offset(),
                status: scratch.status.byte_offset(),
                array: source.region.start().byte_offset(),
                elem_schema_ref: input_schema,
            });
            emit_array_status_machine_checks(
                node,
                site,
                scratch,
                assigned,
                outcome,
                return_label,
                outputs.code,
            )?;
            Ok(ValueRepresentation::Word)
        }
        Op::ArrayIndex => {
            require_input_count(node, 2)?;
            let index = input_value(node, values, 1)?;
            require_value(node, &index, &Type::Int, ValueRepresentation::Word)?;
            outputs.code.push(WeavyOp::LoadArrayLen {
                dst: scratch.fields[2].byte_offset(),
                status: scratch.status.byte_offset(),
                array: source.region.start().byte_offset(),
                elem_schema_ref: input_schema,
            });
            emit_array_status_machine_checks(
                node,
                site,
                scratch,
                assigned,
                outcome,
                return_label,
                outputs.code,
            )?;
            outputs.code.push(WeavyOp::LoadArray {
                dst: input.region.start().byte_offset(),
                status: scratch.status.byte_offset(),
                array: source.region.start().byte_offset(),
                index: index.region.start().byte_offset(),
                elem_width: input_width,
                elem_schema_ref: input_schema,
            });
            emit_array_load_status_checks(ArrayLoadStatusContext {
                node,
                site,
                index: index.region_id,
                length: assigned.fields[2],
                scratch,
                assigned,
                outcome,
                return_label,
                code: outputs.code,
            })?;
            emit_checked_call_indirect(map, &mapper, &input, &output, sequence, outputs)?;
            if node.ty != output_ty {
                return Err(lowering_diagnostic(
                    node.span,
                    "fused array map projection type does not match mapper result",
                ));
            }
            outputs
                .code
                .extend(copy_lowered_value(node, &output, dst, dst_region_id)?);
            representation_for_type(&output_ty, node.span)
        }
        _ => unreachable!("fused map projection is index or length"),
    }
}

fn lower_checked_array_fold_node(
    node: &Node,
    dst: FrameRegion,
    dst_region_id: WeavyRegionId,
    values: &BTreeMap<NodeId, LoweredSlot>,
    sequence: &SequenceContext<'_, '_, '_>,
    outputs: &mut SequenceOutputs<'_, '_>,
) -> Result<ValueRepresentation, Diagnostics> {
    require_input_count(node, 3)?;
    let source = input_value(node, values, 0)?;
    let initial = input_value(node, values, 1)?;
    let folder = input_value(node, values, 2)?;
    let Type::Array(element) = &source.ty else {
        return Err(lowering_diagnostic(
            node.span,
            "array fold source is not an array",
        ));
    };
    require_value(
        node,
        &source,
        &Type::array(element.as_ref().clone()),
        ValueRepresentation::RealizedHandle,
    )?;
    require_value(
        node,
        &initial,
        &node.ty,
        representation_for_type(&node.ty, node.span)?,
    )?;
    let parameter_ty = Type::Tuple(vec![node.ty.clone(), element.as_ref().clone()]);
    require_value(
        node,
        &folder,
        &Type::Function {
            parameter: Box::new(parameter_ty.clone()),
            result: Box::new(node.ty.clone()),
        },
        ValueRepresentation::InlineComposite,
    )?;
    let scratch = sequence.function.layout.outcome_scratch.ok_or_else(|| {
        lowering_diagnostic(node.span, "array fold has no checked outcome scratch")
    })?;
    let assigned = sequence
        .lowering
        .regions
        .outcome_scratch(sequence.function.id, node.span)?;
    let outcome = sequence
        .lowering
        .regions
        .array_outcome(sequence.function.id, node.span)?;
    let return_label = sequence.array_return.ok_or_else(|| {
        lowering_diagnostic(node.span, "array fold has no checked outcome return")
    })?;
    let site = sequence
        .lowering
        .trace_ids
        .get(&NodeRef {
            function: sequence.function.id,
            node: node.id,
        })
        .copied()
        .ok_or_else(|| lowering_diagnostic(node.span, "array fold has no stable trace site"))?;
    let element_schema = sequence.lowering.schemas.schema_for(element, node.span)?;
    let element_width = element_byte_width(element, node.span)?;
    let mut temps = TemporaryCursor::new(
        sequence.function.layout,
        sequence.lowering.regions,
        sequence.function.id,
        node,
    )?;
    let element_slot = temps.take(element, node.span)?;
    let parameter = temps.take(&parameter_ty, node.span)?;
    let result = temps.take(&node.ty, node.span)?;
    let accumulator = LoweredSlot {
        region: dst,
        region_id: dst_region_id,
        ty: node.ty.clone(),
        representation: representation_for_type(&node.ty, node.span)?,
    };
    outputs
        .code
        .extend(copy_lowered_value(node, &initial, dst, dst_region_id)?);
    outputs.code.push(WeavyOp::LoadArrayLen {
        dst: scratch.fields[1].byte_offset(),
        status: scratch.status.byte_offset(),
        array: source.region.start().byte_offset(),
        elem_schema_ref: i64::from(element_schema.0),
    });
    emit_array_status_machine_checks(
        node,
        site,
        scratch,
        assigned,
        outcome,
        return_label,
        outputs.code,
    )?;
    outputs.code.push(WeavyOp::ConstI64 {
        dst: scratch.fields[0].byte_offset(),
        value: 0,
    });
    let next = outputs.code.label();
    let done = outputs.code.label();
    outputs.code.bind(next, node.span)?;
    outputs.code.push(WeavyOp::LtI64 {
        dst: scratch.condition.byte_offset(),
        a: scratch.fields[0].byte_offset(),
        b: scratch.fields[1].byte_offset(),
    });
    outputs.code.jump_if_zero(scratch.condition, done);
    outputs.code.push(WeavyOp::LoadArray {
        dst: element_slot.region.start().byte_offset(),
        status: scratch.status.byte_offset(),
        array: source.region.start().byte_offset(),
        index: scratch.fields[0].byte_offset(),
        elem_width: element_width,
        elem_schema_ref: i64::from(element_schema.0),
    });
    emit_array_status_machine_checks(
        node,
        site,
        scratch,
        assigned,
        outcome,
        return_label,
        outputs.code,
    )?;
    outputs.code.push(WeavyOp::ProductConstruct {
        dst: parameter.region_id,
        fields: vec![
            StructuralFieldSource {
                field: 0,
                source: accumulator.region_id,
            },
            StructuralFieldSource {
                field: 1,
                source: element_slot.region_id,
            },
        ],
    });
    emit_checked_call_indirect(node, &folder, &parameter, &result, sequence, outputs)?;
    outputs
        .code
        .extend(copy_lowered_value(node, &result, dst, dst_region_id)?);
    outputs.code.push(WeavyOp::ConstI64 {
        dst: scratch.fields[2].byte_offset(),
        value: 1,
    });
    outputs.code.push(WeavyOp::AddI64 {
        dst: scratch.fields[0].byte_offset(),
        a: scratch.fields[0].byte_offset(),
        b: scratch.fields[2].byte_offset(),
    });
    outputs.code.jump(next);
    outputs.code.bind(done, node.span)?;
    temps.finish(node.span)?;
    Ok(accumulator.representation)
}

fn lower_checked_array_node(
    node: &Node,
    dst: FrameRegion,
    values: &BTreeMap<NodeId, LoweredSlot>,
    sequence: &SequenceContext<'_, '_, '_>,
    outputs: &mut SequenceOutputs<'_, '_>,
) -> Result<ValueRepresentation, Diagnostics> {
    let scratch = sequence.function.layout.outcome_scratch.ok_or_else(|| {
        lowering_diagnostic(node.span, "array operation has no typed outcome scratch")
    })?;
    let assigned = sequence
        .lowering
        .regions
        .outcome_scratch(sequence.function.id, node.span)?;
    let outcome = sequence
        .lowering
        .regions
        .array_outcome(sequence.function.id, node.span)?;
    let return_label = sequence.array_return.ok_or_else(|| {
        lowering_diagnostic(node.span, "array operation has no typed outcome return")
    })?;
    let site = sequence
        .lowering
        .trace_ids
        .get(&NodeRef {
            function: sequence.function.id,
            node: node.id,
        })
        .copied()
        .ok_or_else(|| {
            lowering_diagnostic(node.span, "array operation has no stable trace site")
        })?;

    match node.op {
        Op::Array => {
            let Type::Array(element) = &node.ty else {
                return Err(lowering_diagnostic(
                    node.span,
                    "array literal has non-array type",
                ));
            };
            let element_schema = sequence.lowering.schemas.schema_for(element, node.span)?;
            let width = type_words(element, node.span)?.as_usize()
                * usize::try_from(FrameSlot::word_size()).expect("word size fits usize");
            outputs.code.push(WeavyOp::ConstI64 {
                dst: scratch.fields[2].byte_offset(),
                value: i64::try_from(node.inputs.len())
                    .map_err(|_| lowering_diagnostic(node.span, "array literal count overflow"))?,
            });
            outputs.code.push(WeavyOp::ArrayNew {
                dst: dst.start().byte_offset(),
                status: scratch.status.byte_offset(),
                count_slot: scratch.fields[2].byte_offset(),
                elem_width: u32::try_from(width)
                    .map_err(|_| lowering_diagnostic(node.span, "array element width overflow"))?,
                elem_schema_ref: i64::from(element_schema.0),
            });
            emit_array_status_machine_checks(
                node,
                site,
                scratch,
                assigned,
                outcome,
                return_label,
                outputs.code,
            )?;
            for (index, input) in node.inputs.iter().enumerate() {
                let value = values.get(input).ok_or_else(|| {
                    lowering_diagnostic(node.span, "array element is not topologically prior")
                })?;
                require_value(
                    node,
                    value,
                    element,
                    representation_for_type(element, node.span)?,
                )?;
                outputs.code.push(WeavyOp::ConstI64 {
                    dst: scratch.fields[2].byte_offset(),
                    value: i64::try_from(index)
                        .map_err(|_| lowering_diagnostic(node.span, "array index overflow"))?,
                });
                outputs.code.push(WeavyOp::ArrayStore {
                    status: scratch.status.byte_offset(),
                    array: dst.start().byte_offset(),
                    index: scratch.fields[2].byte_offset(),
                    src: value.region.start().byte_offset(),
                    elem_width: u32::try_from(width).map_err(|_| {
                        lowering_diagnostic(node.span, "array element width overflow")
                    })?,
                    elem_schema_ref: i64::from(element_schema.0),
                });
                emit_array_status_machine_checks(
                    node,
                    site,
                    scratch,
                    assigned,
                    outcome,
                    return_label,
                    outputs.code,
                )?;
            }
            Ok(ValueRepresentation::RealizedHandle)
        }
        Op::ArrayLen => {
            require_input_count(node, 1)?;
            let array = input_value(node, values, 0)?;
            let Type::Array(element) = &array.ty else {
                return Err(lowering_diagnostic(
                    node.span,
                    "array length input is not an array",
                ));
            };
            let element_schema = sequence.lowering.schemas.schema_for(element, node.span)?;
            outputs.code.push(WeavyOp::LoadArrayLen {
                dst: dst.start().byte_offset(),
                status: scratch.status.byte_offset(),
                array: array.region.start().byte_offset(),
                elem_schema_ref: i64::from(element_schema.0),
            });
            emit_array_status_machine_checks(
                node,
                site,
                scratch,
                assigned,
                outcome,
                return_label,
                outputs.code,
            )?;
            Ok(ValueRepresentation::Word)
        }
        // r[impl lang.collection.array-index]
        Op::ArrayIndex => {
            require_input_count(node, 2)?;
            let array = input_value(node, values, 0)?;
            let index = input_value(node, values, 1)?;
            let Type::Array(element) = &array.ty else {
                return Err(lowering_diagnostic(
                    node.span,
                    "array index input is not an array",
                ));
            };
            require_value(node, &index, &Type::Int, ValueRepresentation::Word)?;
            let element_schema = sequence.lowering.schemas.schema_for(element, node.span)?;
            let width = type_words(element, node.span)?.as_usize()
                * usize::try_from(FrameSlot::word_size()).expect("word size fits usize");
            outputs.code.push(WeavyOp::LoadArrayLen {
                dst: scratch.fields[2].byte_offset(),
                status: scratch.status.byte_offset(),
                array: array.region.start().byte_offset(),
                elem_schema_ref: i64::from(element_schema.0),
            });
            emit_array_status_machine_checks(
                node,
                site,
                scratch,
                assigned,
                outcome,
                return_label,
                outputs.code,
            )?;
            outputs.code.push(WeavyOp::LoadArray {
                dst: dst.start().byte_offset(),
                status: scratch.status.byte_offset(),
                array: array.region.start().byte_offset(),
                index: index.region.start().byte_offset(),
                elem_width: u32::try_from(width)
                    .map_err(|_| lowering_diagnostic(node.span, "array element width overflow"))?,
                elem_schema_ref: i64::from(element_schema.0),
            });
            emit_array_load_status_checks(ArrayLoadStatusContext {
                node,
                site,
                index: index.region_id,
                length: assigned.fields[2],
                scratch,
                assigned,
                outcome,
                return_label,
                code: outputs.code,
            })?;
            representation_for_type(element, node.span)
        }
        Op::ArrayAppend => {
            require_input_count(node, 2)?;
            let array = input_value(node, values, 0)?;
            let appended = input_value(node, values, 1)?;
            let Type::Array(element) = &node.ty else {
                return Err(lowering_diagnostic(
                    node.span,
                    "array append result is not an array",
                ));
            };
            require_value(node, &array, &node.ty, ValueRepresentation::RealizedHandle)?;
            require_value(
                node,
                &appended,
                element,
                representation_for_type(element, node.span)?,
            )?;
            let element_schema = sequence.lowering.schemas.schema_for(element, node.span)?;
            let width = element_byte_width(element, node.span)?;
            let mut temps = TemporaryCursor::new(
                sequence.function.layout,
                sequence.lowering.regions,
                sequence.function.id,
                node,
            )?;
            let temp = temps.take(element, node.span)?;

            outputs.code.push(WeavyOp::LoadArrayLen {
                dst: scratch.fields[2].byte_offset(),
                status: scratch.status.byte_offset(),
                array: array.region.start().byte_offset(),
                elem_schema_ref: i64::from(element_schema.0),
            });
            emit_array_status_machine_checks(
                node,
                site,
                scratch,
                assigned,
                outcome,
                return_label,
                outputs.code,
            )?;
            outputs.code.push(WeavyOp::ConstI64 {
                dst: scratch.fields[0].byte_offset(),
                value: 1,
            });
            outputs.code.push(WeavyOp::AddI64 {
                dst: scratch.fields[1].byte_offset(),
                a: scratch.fields[2].byte_offset(),
                b: scratch.fields[0].byte_offset(),
            });
            outputs.code.push(WeavyOp::ArrayNew {
                dst: dst.start().byte_offset(),
                status: scratch.status.byte_offset(),
                count_slot: scratch.fields[1].byte_offset(),
                elem_width: width,
                elem_schema_ref: i64::from(element_schema.0),
            });
            emit_array_status_machine_checks(
                node,
                site,
                scratch,
                assigned,
                outcome,
                return_label,
                outputs.code,
            )?;
            outputs.code.push(WeavyOp::ConstI64 {
                dst: scratch.fields[0].byte_offset(),
                value: 0,
            });
            emit_array_copy_loop(ArrayCopyLoopContext {
                node,
                site,
                source: &array,
                destination: dst,
                source_index: scratch.fields[0],
                source_index_region: assigned.fields[0],
                destination_index: scratch.fields[0],
                length: scratch.fields[2],
                length_region: assigned.fields[2],
                temp: &temp,
                elem_width: width,
                elem_schema_ref: element_schema,
                scratch,
                assigned,
                outcome,
                return_label,
                code: outputs.code,
            })?;
            outputs.code.push(WeavyOp::ArrayStore {
                status: scratch.status.byte_offset(),
                array: dst.start().byte_offset(),
                index: scratch.fields[0].byte_offset(),
                src: appended.region.start().byte_offset(),
                elem_width: width,
                elem_schema_ref: i64::from(element_schema.0),
            });
            emit_array_status_machine_checks(
                node,
                site,
                scratch,
                assigned,
                outcome,
                return_label,
                outputs.code,
            )?;
            temps.finish(node.span)?;
            Ok(ValueRepresentation::RealizedHandle)
        }
        Op::ArrayConcat => {
            require_input_count(node, 2)?;
            let left = input_value(node, values, 0)?;
            let right = input_value(node, values, 1)?;
            let Type::Array(element) = &node.ty else {
                return Err(lowering_diagnostic(
                    node.span,
                    "array concatenation result is not an array",
                ));
            };
            for array in [&left, &right] {
                require_value(node, array, &node.ty, ValueRepresentation::RealizedHandle)?;
            }
            let element_schema = sequence.lowering.schemas.schema_for(element, node.span)?;
            let width = element_byte_width(element, node.span)?;
            let mut temps = TemporaryCursor::new(
                sequence.function.layout,
                sequence.lowering.regions,
                sequence.function.id,
                node,
            )?;
            let temp = temps.take(element, node.span)?;

            for (array, length) in [(&left, scratch.fields[0]), (&right, scratch.fields[1])] {
                outputs.code.push(WeavyOp::LoadArrayLen {
                    dst: length.byte_offset(),
                    status: scratch.status.byte_offset(),
                    array: array.region.start().byte_offset(),
                    elem_schema_ref: i64::from(element_schema.0),
                });
                emit_array_status_machine_checks(
                    node,
                    site,
                    scratch,
                    assigned,
                    outcome,
                    return_label,
                    outputs.code,
                )?;
            }
            outputs.code.push(WeavyOp::AddI64 {
                dst: scratch.fields[2].byte_offset(),
                a: scratch.fields[0].byte_offset(),
                b: scratch.fields[1].byte_offset(),
            });
            outputs.code.push(WeavyOp::ArrayNew {
                dst: dst.start().byte_offset(),
                status: scratch.status.byte_offset(),
                count_slot: scratch.fields[2].byte_offset(),
                elem_width: width,
                elem_schema_ref: i64::from(element_schema.0),
            });
            emit_array_status_machine_checks(
                node,
                site,
                scratch,
                assigned,
                outcome,
                return_label,
                outputs.code,
            )?;
            outputs.code.push(WeavyOp::ConstI64 {
                dst: scratch.fields[2].byte_offset(),
                value: 0,
            });
            emit_array_copy_loop(ArrayCopyLoopContext {
                node,
                site,
                source: &left,
                destination: dst,
                source_index: scratch.fields[2],
                source_index_region: assigned.fields[2],
                destination_index: scratch.fields[2],
                length: scratch.fields[0],
                length_region: assigned.fields[0],
                temp: &temp,
                elem_width: width,
                elem_schema_ref: element_schema,
                scratch,
                assigned,
                outcome,
                return_label,
                code: outputs.code,
            })?;
            outputs.code.push(WeavyOp::LoadArrayLen {
                dst: scratch.fields[0].byte_offset(),
                status: scratch.status.byte_offset(),
                array: right.region.start().byte_offset(),
                elem_schema_ref: i64::from(element_schema.0),
            });
            emit_array_status_machine_checks(
                node,
                site,
                scratch,
                assigned,
                outcome,
                return_label,
                outputs.code,
            )?;
            outputs.code.push(WeavyOp::ConstI64 {
                dst: scratch.fields[1].byte_offset(),
                value: 0,
            });
            emit_array_copy_loop(ArrayCopyLoopContext {
                node,
                site,
                source: &right,
                destination: dst,
                source_index: scratch.fields[1],
                source_index_region: assigned.fields[1],
                destination_index: scratch.fields[2],
                length: scratch.fields[0],
                length_region: assigned.fields[0],
                temp: &temp,
                elem_width: width,
                elem_schema_ref: element_schema,
                scratch,
                assigned,
                outcome,
                return_label,
                code: outputs.code,
            })?;
            temps.finish(node.span)?;
            Ok(ValueRepresentation::RealizedHandle)
        }
        _ => unreachable!("array lowering dispatched only array operations"),
    }
}

fn lower_array_stream_collect_node(
    node: &Node,
    dst: FrameRegion,
    values: &BTreeMap<NodeId, LoweredSlot>,
    sequence: &SequenceContext<'_, '_, '_>,
    outputs: &mut SequenceOutputs<'_, '_>,
) -> Result<ValueRepresentation, Diagnostics> {
    require_input_count(node, 1)?;
    let stream_id = node.inputs[0];
    let stream = values.get(&stream_id).ok_or_else(|| {
        lowering_diagnostic(
            node.span,
            "stream collect recipe is not topologically prior",
        )
    })?;
    let recipe =
        sequence.nodes.get(&stream_id).copied().ok_or_else(|| {
            lowering_diagnostic(node.span, "stream collect input has no VIR recipe")
        })?;
    if !matches!(recipe.op, Op::ArrayStream) {
        return Err(lowering_diagnostic(
            node.span,
            "stream collect recipe source is not implemented",
        ));
    }
    require_input_count(recipe, 1)?;
    let source = values.get(&recipe.inputs[0]).cloned().ok_or_else(|| {
        lowering_diagnostic(node.span, "array stream source is not topologically prior")
    })?;
    let Type::Array(element) = &source.ty else {
        return Err(lowering_diagnostic(
            node.span,
            "array stream source is not a dense array",
        ));
    };
    let stream_ty = Type::stream(Type::Int, element.as_ref().clone());
    require_value(node, stream, &stream_ty, ValueRepresentation::CodataRecipe)?;
    require_value(
        node,
        &source,
        &Type::array(element.as_ref().clone()),
        ValueRepresentation::RealizedHandle,
    )?;
    let collection_ty = Type::map(Type::Int, element.as_ref().clone());
    require_node_type(node, collection_ty.clone())?;

    let scratch = sequence.function.layout.outcome_scratch.ok_or_else(|| {
        lowering_diagnostic(node.span, "stream collect has no typed outcome scratch")
    })?;
    let assigned = sequence
        .lowering
        .regions
        .outcome_scratch(sequence.function.id, node.span)?;
    let outcome = sequence
        .lowering
        .regions
        .array_outcome(sequence.function.id, node.span)?;
    let return_label = sequence.array_return.ok_or_else(|| {
        lowering_diagnostic(node.span, "stream collect has no typed outcome return")
    })?;
    let site = sequence
        .lowering
        .trace_ids
        .get(&NodeRef {
            function: sequence.function.id,
            node: node.id,
        })
        .copied()
        .ok_or_else(|| lowering_diagnostic(node.span, "stream collect has no trace site"))?;
    let collection_schema = sequence
        .lowering
        .schemas
        .schema_for(&collection_ty, node.span)?;
    let element_schema = sequence.lowering.schemas.schema_for(element, node.span)?;
    let element_width = element_byte_width(element, node.span)?;
    let mut temps = ordered_collection_temps(node, &collection_ty, sequence)?;
    let comparison = temps.cursor.checkpoint();
    let cursor = ordered_cursor(node, sequence, 0)?;

    outputs.code.push(WeavyOp::OrderedEmpty {
        dst: dst.start().byte_offset(),
        collection_schema_ref: i64::from(collection_schema.0),
    });
    outputs.code.push(WeavyOp::LoadArrayLen {
        dst: scratch.fields[1].byte_offset(),
        status: scratch.status.byte_offset(),
        array: source.region.start().byte_offset(),
        elem_schema_ref: i64::from(element_schema.0),
    });
    emit_array_status_machine_checks(
        node,
        site,
        scratch,
        assigned,
        outcome,
        return_label,
        outputs.code,
    )?;
    outputs.code.push(WeavyOp::ConstI64 {
        dst: scratch.fields[0].byte_offset(),
        value: 0,
    });
    let next = outputs.code.label();
    let done = outputs.code.label();
    outputs.code.bind(next, node.span)?;
    outputs.code.push(WeavyOp::LtI64 {
        dst: scratch.condition.byte_offset(),
        a: scratch.fields[0].byte_offset(),
        b: scratch.fields[1].byte_offset(),
    });
    outputs.code.jump_if_zero(scratch.condition, done);
    outputs.code.push(WeavyOp::LoadArray {
        dst: temps.projected_value.region.start().byte_offset(),
        status: scratch.status.byte_offset(),
        array: source.region.start().byte_offset(),
        index: scratch.fields[0].byte_offset(),
        elem_width: element_width,
        elem_schema_ref: i64::from(element_schema.0),
    });
    emit_array_status_machine_checks(
        node,
        site,
        scratch,
        assigned,
        outcome,
        return_label,
        outputs.code,
    )?;
    let key = LoweredSlot {
        region: FrameRegion::for_words(scratch.fields[0].word_index(), FrameWords::ONE)
            .expect("scratch scalar is one frame word"),
        region_id: assigned.fields[0],
        ty: Type::Int,
        representation: ValueRepresentation::Word,
    };
    temps.cursor.rewind(comparison, node.span)?;
    emit_ordered_insert(OrderedInsertLowering {
        node,
        site,
        collection: dst.start(),
        destination: dst.start(),
        key: &key,
        value: Some(&temps.projected_value),
        key_ty: &Type::Int,
        collection_schema,
        cursor,
        candidate: &temps.candidate,
        ordering: &temps.ordering,
        ready: &temps.ready,
        present: &temps.present,
        duplicate: DuplicateDisposition::LanguageFailure,
        comparison_checkpoint: comparison,
        temps: &mut temps.cursor,
        scratch,
        assigned,
        outcome,
        return_label,
        code: outputs.code,
    })?;
    outputs.code.push(WeavyOp::ConstI64 {
        dst: scratch.fields[2].byte_offset(),
        value: 1,
    });
    outputs.code.push(WeavyOp::AddI64 {
        dst: scratch.fields[0].byte_offset(),
        a: scratch.fields[0].byte_offset(),
        b: scratch.fields[2].byte_offset(),
    });
    outputs.code.jump(next);
    outputs.code.bind(done, node.span)?;
    temps.cursor.drain(node.span)?;
    temps.cursor.finish(node.span)?;
    Ok(ValueRepresentation::RealizedHandle)
}

fn lower_checked_collection_node(
    node: &Node,
    dst: FrameRegion,
    _dst_region: WeavyRegionId,
    values: &BTreeMap<NodeId, LoweredSlot>,
    sequence: &SequenceContext<'_, '_, '_>,
    outputs: &mut SequenceOutputs<'_, '_>,
) -> Result<ValueRepresentation, Diagnostics> {
    if matches!(node.op, Op::StreamCollect) {
        return lower_array_stream_collect_node(node, dst, values, sequence, outputs);
    }
    let scratch = sequence.function.layout.outcome_scratch.ok_or_else(|| {
        lowering_diagnostic(
            node.span,
            "collection operation has no typed outcome scratch",
        )
    })?;
    let assigned = sequence
        .lowering
        .regions
        .outcome_scratch(sequence.function.id, node.span)?;
    let outcome = sequence
        .lowering
        .regions
        .array_outcome(sequence.function.id, node.span)?;
    let return_label = sequence.array_return.ok_or_else(|| {
        lowering_diagnostic(
            node.span,
            "collection operation has no typed outcome return",
        )
    })?;
    let site = sequence
        .lowering
        .trace_ids
        .get(&NodeRef {
            function: sequence.function.id,
            node: node.id,
        })
        .copied()
        .ok_or_else(|| lowering_diagnostic(node.span, "collection operation has no trace site"))?;

    let collection_ty = if matches!(node.op, Op::Map | Op::Set) {
        node.ty.clone()
    } else {
        node.inputs
            .first()
            .and_then(|input| values.get(input))
            .map(|value| value.ty.clone())
            .ok_or_else(|| lowering_diagnostic(node.span, "collection receiver is unavailable"))?
    };
    let collection_schema = sequence
        .lowering
        .schemas
        .schema_for(&collection_ty, node.span)?;

    match node.op {
        Op::Map | Op::Set => {
            outputs.code.push(WeavyOp::OrderedEmpty {
                dst: dst.start().byte_offset(),
                collection_schema_ref: i64::from(collection_schema.0),
            });
            let mut temps = ordered_collection_temps(node, &collection_ty, sequence)?;
            let cursor = ordered_cursor(node, sequence, 0)?;
            let comparison = temps.cursor.checkpoint();
            match &collection_ty {
                Type::Map { key, value } => {
                    if node.inputs.len() % 2 != 0 {
                        return Err(lowering_diagnostic(
                            node.span,
                            "map literal does not contain key/value pairs",
                        ));
                    }
                    for pair in node.inputs.chunks_exact(2) {
                        let key_value = values.get(&pair[0]).ok_or_else(|| {
                            lowering_diagnostic(node.span, "map literal key is unavailable")
                        })?;
                        let value_value = values.get(&pair[1]).ok_or_else(|| {
                            lowering_diagnostic(node.span, "map literal value is unavailable")
                        })?;
                        require_value(
                            node,
                            key_value,
                            key,
                            representation_for_type(key, node.span)?,
                        )?;
                        require_value(
                            node,
                            value_value,
                            value,
                            representation_for_type(value, node.span)?,
                        )?;
                        temps.cursor.rewind(comparison, node.span)?;
                        emit_ordered_insert(OrderedInsertLowering {
                            node,
                            site,
                            collection: dst.start(),
                            destination: dst.start(),
                            key: key_value,
                            value: Some(value_value),
                            key_ty: key,
                            collection_schema,
                            cursor,
                            candidate: &temps.candidate,
                            ordering: &temps.ordering,
                            ready: &temps.ready,
                            present: &temps.present,
                            duplicate: DuplicateDisposition::LanguageFailure,
                            comparison_checkpoint: comparison,
                            temps: &mut temps.cursor,
                            scratch,
                            assigned,
                            outcome,
                            return_label,
                            code: outputs.code,
                        })?;
                    }
                }
                Type::Set(element) => {
                    for input in &node.inputs {
                        let element_value = values.get(input).ok_or_else(|| {
                            lowering_diagnostic(node.span, "set literal element is unavailable")
                        })?;
                        require_value(
                            node,
                            element_value,
                            element,
                            representation_for_type(element, node.span)?,
                        )?;
                        temps.cursor.rewind(comparison, node.span)?;
                        emit_ordered_insert(OrderedInsertLowering {
                            node,
                            site,
                            collection: dst.start(),
                            destination: dst.start(),
                            key: element_value,
                            value: None,
                            key_ty: element,
                            collection_schema,
                            cursor,
                            candidate: &temps.candidate,
                            ordering: &temps.ordering,
                            ready: &temps.ready,
                            present: &temps.present,
                            duplicate: DuplicateDisposition::Success,
                            comparison_checkpoint: comparison,
                            temps: &mut temps.cursor,
                            scratch,
                            assigned,
                            outcome,
                            return_label,
                            code: outputs.code,
                        })?;
                    }
                }
                _ => unreachable!(),
            }
            temps.cursor.drain(node.span)?;
            temps.cursor.finish(node.span)?;
            Ok(ValueRepresentation::RealizedHandle)
        }
        Op::MapAdd | Op::MapWith | Op::SetAdd => {
            let expected = if matches!(node.op, Op::MapWith) { 3 } else { 2 };
            require_input_count(node, expected)?;
            let collection = input_value(node, values, 0)?;
            require_value(
                node,
                &collection,
                &collection_ty,
                ValueRepresentation::RealizedHandle,
            )?;
            let mut temps = ordered_collection_temps(node, &collection_ty, sequence)?;
            let (key_ty, value, duplicate) = match &collection_ty {
                Type::Map { key: key_ty, value } => {
                    if matches!(node.op, Op::MapAdd) {
                        let row = input_value(node, values, 1)?;
                        let row_ty =
                            Type::Tuple(vec![key_ty.as_ref().clone(), value.as_ref().clone()]);
                        require_value(node, &row, &row_ty, ValueRepresentation::InlineComposite)?;
                        outputs.code.push(WeavyOp::ProductProject {
                            dst: temps.projected_key.region_id,
                            product: row.region_id,
                            field: 0,
                        });
                        outputs.code.push(WeavyOp::ProductProject {
                            dst: temps.projected_value.region_id,
                            product: row.region_id,
                            field: 1,
                        });
                        (
                            key_ty.as_ref(),
                            Some(temps.projected_value.clone()),
                            DuplicateDisposition::LanguageFailure,
                        )
                    } else {
                        let value_slot = input_value(node, values, 2)?;
                        require_value(
                            node,
                            &value_slot,
                            value,
                            representation_for_type(value, node.span)?,
                        )?;
                        (
                            key_ty.as_ref(),
                            Some(value_slot),
                            DuplicateDisposition::Machine,
                        )
                    }
                }
                Type::Set(element) => (element.as_ref(), None, DuplicateDisposition::Success),
                _ => unreachable!(),
            };
            let key = if matches!(node.op, Op::MapAdd) {
                temps.projected_key.clone()
            } else {
                input_value(node, values, 1)?
            };
            require_value(
                node,
                &key,
                key_ty,
                representation_for_type(key_ty, node.span)?,
            )?;
            let comparison = temps.cursor.checkpoint();
            emit_ordered_insert(OrderedInsertLowering {
                node,
                site,
                collection: collection.region.start(),
                destination: dst.start(),
                key: &key,
                value: value.as_ref(),
                key_ty,
                collection_schema,
                cursor: ordered_cursor(node, sequence, 0)?,
                candidate: &temps.candidate,
                ordering: &temps.ordering,
                ready: &temps.ready,
                present: &temps.present,
                duplicate,
                comparison_checkpoint: comparison,
                temps: &mut temps.cursor,
                scratch,
                assigned,
                outcome,
                return_label,
                code: outputs.code,
            })?;
            temps.cursor.drain(node.span)?;
            temps.cursor.finish(node.span)?;
            Ok(ValueRepresentation::RealizedHandle)
        }
        Op::MapConcat | Op::SetConcat => {
            require_input_count(node, 2)?;
            let left = input_value(node, values, 0)?;
            let right = input_value(node, values, 1)?;
            require_value(
                node,
                &left,
                &collection_ty,
                ValueRepresentation::RealizedHandle,
            )?;
            require_value(
                node,
                &right,
                &collection_ty,
                ValueRepresentation::RealizedHandle,
            )?;
            outputs.code.push(WeavyOp::CopyI64 {
                dst: dst.start().byte_offset(),
                src: left.region.start().byte_offset(),
            });
            let mut temps = ordered_collection_temps(node, &collection_ty, sequence)?;
            let comparison = temps.cursor.checkpoint();
            let iterate_cursor = ordered_cursor(node, sequence, 1)?;
            outputs.code.push(WeavyOp::OrderedBeginIterate {
                cursor: iterate_cursor.start().byte_offset(),
                status: scratch.status.byte_offset(),
                collection: right.region.start().byte_offset(),
                collection_schema_ref: i64::from(collection_schema.0),
            });
            emit_ordered_status_checks(OrderedStatusLowering {
                node,
                site,
                duplicate: DuplicateDisposition::Machine,
                scratch,
                assigned,
                outcome,
                return_label,
                code: outputs.code,
            })?;
            let next = outputs.code.label();
            let done = outputs.code.label();
            outputs.code.bind(next, node.span)?;
            outputs.code.push(WeavyOp::OrderedIterateRow {
                cursor: iterate_cursor.start().byte_offset(),
                present: temps.present.region.start().byte_offset(),
                row: temps.row.region.start().byte_offset(),
                status: scratch.status.byte_offset(),
                row_width: element_byte_width(
                    &collection_element_type(&collection_ty, node.span)?,
                    node.span,
                )?,
                collection_schema_ref: i64::from(collection_schema.0),
            });
            emit_ordered_status_checks(OrderedStatusLowering {
                node,
                site,
                duplicate: DuplicateDisposition::Machine,
                scratch,
                assigned,
                outcome,
                return_label,
                code: outputs.code,
            })?;
            outputs
                .code
                .jump_if_zero(temps.present.region.start(), done);
            let (key, value, key_ty, duplicate) = match &collection_ty {
                Type::Map { key, .. } => {
                    outputs.code.push(WeavyOp::ProductProject {
                        dst: temps.projected_key.region_id,
                        product: temps.row.region_id,
                        field: 0,
                    });
                    outputs.code.push(WeavyOp::ProductProject {
                        dst: temps.projected_value.region_id,
                        product: temps.row.region_id,
                        field: 1,
                    });
                    (
                        &temps.projected_key,
                        Some(&temps.projected_value),
                        key.as_ref(),
                        DuplicateDisposition::LanguageFailure,
                    )
                }
                Type::Set(element) => (
                    &temps.row,
                    None,
                    element.as_ref(),
                    DuplicateDisposition::Success,
                ),
                _ => unreachable!(),
            };
            temps.cursor.rewind(comparison, node.span)?;
            emit_ordered_insert(OrderedInsertLowering {
                node,
                site,
                collection: dst.start(),
                destination: dst.start(),
                key,
                value,
                key_ty,
                collection_schema,
                cursor: ordered_cursor(node, sequence, 0)?,
                candidate: &temps.candidate,
                ordering: &temps.ordering,
                ready: &temps.ready,
                present: &temps.present,
                duplicate,
                comparison_checkpoint: comparison,
                temps: &mut temps.cursor,
                scratch,
                assigned,
                outcome,
                return_label,
                code: outputs.code,
            })?;
            outputs.code.jump(next);
            outputs.code.bind(done, node.span)?;
            temps.cursor.drain(node.span)?;
            temps.cursor.finish(node.span)?;
            Ok(ValueRepresentation::RealizedHandle)
        }
        Op::MapGet | Op::MapHas | Op::SetHas => {
            require_input_count(node, 2)?;
            let collection = input_value(node, values, 0)?;
            let sought = input_value(node, values, 1)?;
            require_value(
                node,
                &collection,
                &collection_ty,
                ValueRepresentation::RealizedHandle,
            )?;
            let key_ty = match &collection_ty {
                Type::Map { key, .. } => key.as_ref(),
                Type::Set(element) => element.as_ref(),
                _ => unreachable!(),
            };
            require_value(
                node,
                &sought,
                key_ty,
                representation_for_type(key_ty, node.span)?,
            )?;
            let mut temps = ordered_collection_temps(node, &collection_ty, sequence)?;
            let comparison = temps.cursor.checkpoint();
            outputs.code.push(WeavyOp::CopyI64 {
                dst: temps.current.region.start().byte_offset(),
                src: collection.region.start().byte_offset(),
            });
            if !matches!(node.op, Op::MapGet) {
                outputs.code.push(WeavyOp::ConstI64 {
                    dst: dst.start().byte_offset(),
                    value: 0,
                });
            }
            let scan = outputs.code.label();
            let choose_right = outputs.code.label();
            let absent = outputs.code.label();
            let found = outputs.code.label();
            let done = outputs.code.label();
            outputs.code.bind(scan, node.span)?;
            let cursor = ordered_cursor(node, sequence, 0)?;
            outputs.code.push(WeavyOp::OrderedBeginProbe {
                cursor: cursor.start().byte_offset(),
                status: scratch.status.byte_offset(),
                collection: temps.current.region.start().byte_offset(),
                collection_schema_ref: i64::from(collection_schema.0),
            });
            emit_ordered_status_checks(OrderedStatusLowering {
                node,
                site,
                duplicate: DuplicateDisposition::Machine,
                scratch,
                assigned,
                outcome,
                return_label,
                code: outputs.code,
            })?;
            outputs.code.push(WeavyOp::OrderedProbeKey {
                cursor: cursor.start().byte_offset(),
                present: temps.present.region.start().byte_offset(),
                key: temps.candidate.region.start().byte_offset(),
                left: temps.left.region.start().byte_offset(),
                right: temps.right.region.start().byte_offset(),
                status: scratch.status.byte_offset(),
                key_width: element_byte_width(key_ty, node.span)?,
                collection_schema_ref: i64::from(collection_schema.0),
            });
            emit_ordered_status_checks(OrderedStatusLowering {
                node,
                site,
                duplicate: DuplicateDisposition::Machine,
                scratch,
                assigned,
                outcome,
                return_label,
                code: outputs.code,
            })?;
            outputs
                .code
                .jump_if_zero(temps.present.region.start(), absent);
            temps.cursor.rewind(comparison, node.span)?;
            emit_structural_order(
                node,
                key_ty,
                &sought,
                &temps.candidate,
                temps.ordering.region.start(),
                scratch.condition,
                &mut temps.cursor,
                outputs.code,
            )?;
            outputs.code.push(WeavyOp::ConstI64 {
                dst: scratch.fields[0].byte_offset(),
                value: i64::from(ORDERING_EQUAL_VARIANT),
            });
            outputs.code.push(WeavyOp::EqI64 {
                dst: scratch.condition.byte_offset(),
                a: temps.ordering.region.start().byte_offset(),
                b: scratch.fields[0].byte_offset(),
            });
            outputs.code.jump_if_zero(scratch.condition, choose_right);
            outputs.code.jump(found);
            outputs.code.bind(choose_right, node.span)?;
            outputs.code.push(WeavyOp::ConstI64 {
                dst: scratch.fields[0].byte_offset(),
                value: i64::from(ORDERING_LESS_VARIANT),
            });
            outputs.code.push(WeavyOp::EqI64 {
                dst: scratch.condition.byte_offset(),
                a: temps.ordering.region.start().byte_offset(),
                b: scratch.fields[0].byte_offset(),
            });
            let descend_right = outputs.code.label();
            outputs.code.jump_if_zero(scratch.condition, descend_right);
            outputs.code.push(WeavyOp::CopyI64 {
                dst: temps.current.region.start().byte_offset(),
                src: temps.left.region.start().byte_offset(),
            });
            outputs.code.jump(scan);
            outputs.code.bind(descend_right, node.span)?;
            outputs.code.push(WeavyOp::CopyI64 {
                dst: temps.current.region.start().byte_offset(),
                src: temps.right.region.start().byte_offset(),
            });
            outputs.code.jump(scan);
            outputs.code.bind(found, node.span)?;
            if matches!(node.op, Op::MapGet) {
                outputs.code.push(WeavyOp::OrderedBeginProbe {
                    cursor: cursor.start().byte_offset(),
                    status: scratch.status.byte_offset(),
                    collection: temps.current.region.start().byte_offset(),
                    collection_schema_ref: i64::from(collection_schema.0),
                });
                emit_ordered_status_checks(OrderedStatusLowering {
                    node,
                    site,
                    duplicate: DuplicateDisposition::Machine,
                    scratch,
                    assigned,
                    outcome,
                    return_label,
                    code: outputs.code,
                })?;
                outputs.code.push(WeavyOp::OrderedProbeValue {
                    cursor: cursor.start().byte_offset(),
                    present: temps.present.region.start().byte_offset(),
                    value: dst.start().byte_offset(),
                    status: scratch.status.byte_offset(),
                    value_width: element_byte_width(&node.ty, node.span)?,
                    collection_schema_ref: i64::from(collection_schema.0),
                });
                emit_ordered_status_checks(OrderedStatusLowering {
                    node,
                    site,
                    duplicate: DuplicateDisposition::Machine,
                    scratch,
                    assigned,
                    outcome,
                    return_label,
                    code: outputs.code,
                })?;
                outputs
                    .code
                    .jump_if_zero(temps.present.region.start(), absent);
            } else {
                outputs.code.push(WeavyOp::ConstI64 {
                    dst: dst.start().byte_offset(),
                    value: 1,
                });
            }
            outputs.code.jump(done);
            outputs.code.bind(absent, node.span)?;
            if matches!(node.op, Op::MapGet) {
                outputs.code.push(WeavyOp::ConstI64 {
                    dst: scratch.fields[0].byte_offset(),
                    value: i64::from(site),
                });
                outputs.code.push(WeavyOp::EnumConstruct {
                    dst: outcome,
                    variant: ArrayOutcomeAbi::for_value(node.ty.clone()).missing_key_variant,
                    fields: vec![StructuralFieldSource {
                        field: 0,
                        source: assigned.fields[0],
                    }],
                });
                outputs.code.jump(return_label);
            }
            outputs.code.bind(done, node.span)?;
            temps.cursor.drain(node.span)?;
            temps.cursor.finish(node.span)?;
            representation_for_type(&node.ty, node.span)
        }
        Op::MapLen | Op::SetLen => {
            require_input_count(node, 1)?;
            let collection = input_value(node, values, 0)?;
            require_value(
                node,
                &collection,
                &collection_ty,
                ValueRepresentation::RealizedHandle,
            )?;
            outputs.code.push(WeavyOp::OrderedLen {
                dst: dst.start().byte_offset(),
                status: scratch.status.byte_offset(),
                collection: collection.region.start().byte_offset(),
                collection_schema_ref: i64::from(collection_schema.0),
            });
            emit_ordered_status_checks(OrderedStatusLowering {
                node,
                site,
                duplicate: DuplicateDisposition::Machine,
                scratch,
                assigned,
                outcome,
                return_label,
                code: outputs.code,
            })?;
            Ok(ValueRepresentation::Word)
        }
        Op::MapKeys | Op::SetValues => {
            require_input_count(node, 1)?;
            let collection = input_value(node, values, 0)?;
            require_value(
                node,
                &collection,
                &collection_ty,
                ValueRepresentation::RealizedHandle,
            )?;
            let Type::Array(output_element) = &node.ty else {
                return Err(lowering_diagnostic(
                    node.span,
                    "collection projection result is not an array",
                ));
            };
            let output_schema = sequence
                .lowering
                .schemas
                .schema_for(output_element, node.span)?;
            let output_width = element_byte_width(output_element, node.span)?;
            let row_ty = collection_element_type(&collection_ty, node.span)?;
            let row_width = element_byte_width(&row_ty, node.span)?;
            let mut temps = ordered_collection_temps(node, &collection_ty, sequence)?;
            outputs.code.push(WeavyOp::OrderedLen {
                dst: scratch.fields[0].byte_offset(),
                status: scratch.status.byte_offset(),
                collection: collection.region.start().byte_offset(),
                collection_schema_ref: i64::from(collection_schema.0),
            });
            emit_ordered_status_checks(OrderedStatusLowering {
                node,
                site,
                duplicate: DuplicateDisposition::Machine,
                scratch,
                assigned,
                outcome,
                return_label,
                code: outputs.code,
            })?;
            outputs.code.push(WeavyOp::ArrayNew {
                dst: dst.start().byte_offset(),
                status: scratch.status.byte_offset(),
                count_slot: scratch.fields[0].byte_offset(),
                elem_width: output_width,
                elem_schema_ref: i64::from(output_schema.0),
            });
            emit_array_status_machine_checks(
                node,
                site,
                scratch,
                assigned,
                outcome,
                return_label,
                outputs.code,
            )?;
            let cursor = ordered_cursor(node, sequence, 0)?;
            outputs.code.push(WeavyOp::OrderedBeginIterate {
                cursor: cursor.start().byte_offset(),
                status: scratch.status.byte_offset(),
                collection: collection.region.start().byte_offset(),
                collection_schema_ref: i64::from(collection_schema.0),
            });
            emit_ordered_status_checks(OrderedStatusLowering {
                node,
                site,
                duplicate: DuplicateDisposition::Machine,
                scratch,
                assigned,
                outcome,
                return_label,
                code: outputs.code,
            })?;
            outputs.code.push(WeavyOp::ConstI64 {
                dst: scratch.fields[1].byte_offset(),
                value: 0,
            });
            let next = outputs.code.label();
            let done = outputs.code.label();
            outputs.code.bind(next, node.span)?;
            outputs.code.push(WeavyOp::OrderedIterateRow {
                cursor: cursor.start().byte_offset(),
                present: temps.present.region.start().byte_offset(),
                row: temps.row.region.start().byte_offset(),
                status: scratch.status.byte_offset(),
                row_width,
                collection_schema_ref: i64::from(collection_schema.0),
            });
            emit_ordered_status_checks(OrderedStatusLowering {
                node,
                site,
                duplicate: DuplicateDisposition::Machine,
                scratch,
                assigned,
                outcome,
                return_label,
                code: outputs.code,
            })?;
            outputs
                .code
                .jump_if_zero(temps.present.region.start(), done);
            let source = if matches!(node.op, Op::MapKeys) {
                outputs.code.push(WeavyOp::ProductProject {
                    dst: temps.projected_key.region_id,
                    product: temps.row.region_id,
                    field: 0,
                });
                &temps.projected_key
            } else {
                &temps.row
            };
            outputs.code.push(WeavyOp::ArrayStore {
                status: scratch.status.byte_offset(),
                array: dst.start().byte_offset(),
                index: scratch.fields[1].byte_offset(),
                src: source.region.start().byte_offset(),
                elem_width: output_width,
                elem_schema_ref: i64::from(output_schema.0),
            });
            emit_array_status_machine_checks(
                node,
                site,
                scratch,
                assigned,
                outcome,
                return_label,
                outputs.code,
            )?;
            outputs.code.push(WeavyOp::ConstI64 {
                dst: scratch.fields[2].byte_offset(),
                value: 1,
            });
            outputs.code.push(WeavyOp::AddI64 {
                dst: scratch.fields[1].byte_offset(),
                a: scratch.fields[1].byte_offset(),
                b: scratch.fields[2].byte_offset(),
            });
            outputs.code.jump(next);
            outputs.code.bind(done, node.span)?;
            temps.cursor.drain(node.span)?;
            temps.cursor.finish(node.span)?;
            Ok(ValueRepresentation::RealizedHandle)
        }
        _ => unreachable!("collection lowering dispatched only Map/Set operations"),
    }
}

#[derive(Clone, Copy)]
enum DuplicateDisposition {
    LanguageFailure,
    Success,
    Machine,
}

struct OrderedCollectionTemps<'a> {
    cursor: TemporaryCursor<'a>,
    ordering: LoweredSlot,
    ready: LoweredSlot,
    present: LoweredSlot,
    current: LoweredSlot,
    left: LoweredSlot,
    right: LoweredSlot,
    row: LoweredSlot,
    candidate: LoweredSlot,
    projected_key: LoweredSlot,
    projected_value: LoweredSlot,
}

fn ordered_collection_temps<'a>(
    node: &Node,
    collection: &Type,
    sequence: &'a SequenceContext<'_, '_, '_>,
) -> Result<OrderedCollectionTemps<'a>, Diagnostics> {
    let mut cursor = TemporaryCursor::new(
        sequence.function.layout,
        sequence.lowering.regions,
        sequence.function.id,
        node,
    )?;
    let ordering = cursor.take(&Type::Int, node.span)?;
    let ready = cursor.take(&Type::Int, node.span)?;
    let present = cursor.take(&Type::Int, node.span)?;
    let current = cursor.take(collection, node.span)?;
    let left = cursor.take(collection, node.span)?;
    let right = cursor.take(collection, node.span)?;
    match collection {
        Type::Map { key, value } => {
            let row_ty = Type::Tuple(vec![key.as_ref().clone(), value.as_ref().clone()]);
            let row = cursor.take(&row_ty, node.span)?;
            let candidate = cursor.take(key, node.span)?;
            let projected_key = cursor.take(key, node.span)?;
            let projected_value = cursor.take(value, node.span)?;
            Ok(OrderedCollectionTemps {
                cursor,
                ordering,
                ready,
                present,
                current,
                left,
                right,
                row,
                candidate,
                projected_key,
                projected_value,
            })
        }
        Type::Set(element) => {
            let row = cursor.take(element, node.span)?;
            let candidate = cursor.take(element, node.span)?;
            Ok(OrderedCollectionTemps {
                cursor,
                ordering,
                ready,
                present,
                current,
                left,
                right,
                projected_key: row.clone(),
                projected_value: row.clone(),
                row,
                candidate,
            })
        }
        _ => Err(lowering_diagnostic(
            node.span,
            "ordered temporary receiver is not Map or Set",
        )),
    }
}

fn ordered_cursor(
    node: &Node,
    sequence: &SequenceContext<'_, '_, '_>,
    index: usize,
) -> Result<FrameRegion, Diagnostics> {
    sequence
        .function
        .layout
        .ordered_cursors(node.id, node.span)?
        .get(index)
        .copied()
        .ok_or_else(|| lowering_diagnostic(node.span, "ordered cursor index is unavailable"))
}

struct OrderedInsertLowering<'a, 'temps, 'code> {
    node: &'a Node,
    site: u32,
    collection: FrameSlot,
    destination: FrameSlot,
    key: &'a LoweredSlot,
    value: Option<&'a LoweredSlot>,
    key_ty: &'a Type,
    collection_schema: WeavySchemaRef,
    cursor: FrameRegion,
    candidate: &'a LoweredSlot,
    ordering: &'a LoweredSlot,
    ready: &'a LoweredSlot,
    present: &'a LoweredSlot,
    duplicate: DuplicateDisposition,
    comparison_checkpoint: usize,
    temps: &'a mut TemporaryCursor<'temps>,
    scratch: OutcomeScratch,
    assigned: AssignedOutcomeScratch,
    outcome: WeavyRegionId,
    return_label: CodeLabel,
    code: &'code mut CodeBuilder,
}

fn emit_ordered_insert(context: OrderedInsertLowering<'_, '_, '_>) -> Result<(), Diagnostics> {
    let OrderedInsertLowering {
        node,
        site,
        collection,
        destination,
        key,
        value,
        key_ty,
        collection_schema,
        cursor,
        candidate,
        ordering,
        ready,
        present,
        duplicate,
        comparison_checkpoint,
        temps,
        scratch,
        assigned,
        outcome,
        return_label,
        code,
    } = context;
    code.push(WeavyOp::OrderedBeginInsert {
        cursor: cursor.start().byte_offset(),
        status: scratch.status.byte_offset(),
        collection: collection.byte_offset(),
        collection_schema_ref: i64::from(collection_schema.0),
    });
    emit_ordered_status_checks(OrderedStatusLowering {
        node,
        site,
        duplicate: DuplicateDisposition::Machine,
        scratch,
        assigned,
        outcome,
        return_label,
        code,
    })?;
    let inspect = code.label();
    let commit = code.label();
    code.bind(inspect, node.span)?;
    code.push(WeavyOp::OrderedInsertInspect {
        cursor: cursor.start().byte_offset(),
        present: present.region.start().byte_offset(),
        key: candidate.region.start().byte_offset(),
        status: scratch.status.byte_offset(),
        key_width: element_byte_width(key_ty, node.span)?,
        collection_schema_ref: i64::from(collection_schema.0),
    });
    emit_ordered_status_checks(OrderedStatusLowering {
        node,
        site,
        duplicate: DuplicateDisposition::Machine,
        scratch,
        assigned,
        outcome,
        return_label,
        code,
    })?;
    code.jump_if_zero(present.region.start(), commit);
    temps.rewind(comparison_checkpoint, node.span)?;
    emit_structural_order(
        node,
        key_ty,
        key,
        candidate,
        ordering.region.start(),
        scratch.condition,
        temps,
        code,
    )?;
    code.push(WeavyOp::OrderedInsertAdvance {
        cursor: cursor.start().byte_offset(),
        ordering: ordering.region.start().byte_offset(),
        ready: ready.region.start().byte_offset(),
        status: scratch.status.byte_offset(),
        collection_schema_ref: i64::from(collection_schema.0),
    });
    emit_ordered_status_checks(OrderedStatusLowering {
        node,
        site,
        duplicate: DuplicateDisposition::Machine,
        scratch,
        assigned,
        outcome,
        return_label,
        code,
    })?;
    code.jump_if_zero(ready.region.start(), inspect);
    code.bind(commit, node.span)?;
    code.push(WeavyOp::OrderedInsertCommit {
        dst: destination.byte_offset(),
        cursor: cursor.start().byte_offset(),
        key: key.region.start().byte_offset(),
        value: value.map(|value| value.region.start().byte_offset()),
        status: scratch.status.byte_offset(),
        key_width: element_byte_width(key_ty, node.span)?,
        value_width: value
            .map(|value| element_byte_width(&value.ty, node.span))
            .transpose()?
            .unwrap_or(0),
        collection_schema_ref: i64::from(collection_schema.0),
        replace: matches!(
            duplicate,
            DuplicateDisposition::Machine | DuplicateDisposition::Success
        ),
    });
    emit_ordered_status_checks(OrderedStatusLowering {
        node,
        site,
        duplicate,
        scratch,
        assigned,
        outcome,
        return_label,
        code,
    })
}

struct OrderedStatusLowering<'a, 'code> {
    node: &'a Node,
    site: u32,
    duplicate: DuplicateDisposition,
    scratch: OutcomeScratch,
    assigned: AssignedOutcomeScratch,
    outcome: WeavyRegionId,
    return_label: CodeLabel,
    code: &'code mut CodeBuilder,
}

fn emit_ordered_status_checks(context: OrderedStatusLowering<'_, '_>) -> Result<(), Diagnostics> {
    let OrderedStatusLowering {
        node,
        site,
        duplicate,
        scratch,
        assigned,
        outcome,
        return_label,
        code,
    } = context;
    let success = code.label();
    for expected in [
        OrderedOpStatus::Ok,
        OrderedOpStatus::InvalidHandle,
        OrderedOpStatus::SchemaMismatch,
        OrderedOpStatus::OperationMismatch,
        OrderedOpStatus::Stale,
        OrderedOpStatus::AllocationFailed,
        OrderedOpStatus::DuplicateKey,
        OrderedOpStatus::InvalidOrdering,
    ] {
        let next = code.label();
        code.push(WeavyOp::OrderedStatusIs {
            dst: scratch.condition.byte_offset(),
            status: scratch.status.byte_offset(),
            expected,
        });
        code.jump_if_zero(scratch.condition, next);
        if expected == OrderedOpStatus::Ok
            || (expected == OrderedOpStatus::DuplicateKey
                && matches!(duplicate, DuplicateDisposition::Success))
        {
            code.jump(success);
        } else if expected == OrderedOpStatus::DuplicateKey
            && matches!(duplicate, DuplicateDisposition::LanguageFailure)
        {
            code.push(WeavyOp::ConstI64 {
                dst: scratch.fields[0].byte_offset(),
                value: i64::from(site),
            });
            code.push(WeavyOp::EnumConstruct {
                dst: outcome,
                variant: ArrayOutcomeAbi::for_value(node.ty.clone()).duplicate_key_variant,
                fields: vec![StructuralFieldSource {
                    field: 0,
                    source: assigned.fields[0],
                }],
            });
            code.jump(return_label);
        } else {
            code.push(WeavyOp::ConstI64 {
                dst: scratch.fields[0].byte_offset(),
                value: i64::from(site),
            });
            code.push(WeavyOp::ConstI64 {
                dst: scratch.fields[1].byte_offset(),
                value: expected as i64,
            });
            code.push(WeavyOp::EnumConstruct {
                dst: outcome,
                variant: ArrayOutcomeAbi::for_value(node.ty.clone()).ordered_machine_variant,
                fields: vec![
                    StructuralFieldSource {
                        field: 0,
                        source: assigned.fields[0],
                    },
                    StructuralFieldSource {
                        field: 1,
                        source: assigned.fields[1],
                    },
                ],
            });
            code.jump(return_label);
        }
        code.bind(next, node.span)?;
    }
    code.bind(success, node.span)
}

fn collection_element_type(collection: &Type, span: Span) -> Result<Type, Diagnostics> {
    match collection {
        Type::Map { key, value } => Ok(Type::Tuple(vec![
            key.as_ref().clone(),
            value.as_ref().clone(),
        ])),
        Type::Set(element) => Ok(element.as_ref().clone()),
        _ => Err(lowering_diagnostic(
            span,
            "collection operation input is not Map or Set",
        )),
    }
}

fn element_byte_width(element: &Type, span: Span) -> Result<u32, Diagnostics> {
    let bytes = type_words(element, span)?
        .as_usize()
        .checked_mul(usize::try_from(FrameSlot::word_size()).expect("word size fits usize"))
        .ok_or_else(|| lowering_diagnostic(span, "array element width overflow"))?;
    u32::try_from(bytes).map_err(|_| lowering_diagnostic(span, "array element width overflow"))
}

struct ArrayCopyLoopContext<'a> {
    node: &'a Node,
    site: u32,
    source: &'a LoweredSlot,
    destination: FrameRegion,
    source_index: FrameSlot,
    source_index_region: WeavyRegionId,
    destination_index: FrameSlot,
    length: FrameSlot,
    length_region: WeavyRegionId,
    temp: &'a LoweredSlot,
    elem_width: u32,
    elem_schema_ref: WeavySchemaRef,
    scratch: OutcomeScratch,
    assigned: AssignedOutcomeScratch,
    outcome: WeavyRegionId,
    return_label: CodeLabel,
    code: &'a mut CodeBuilder,
}

fn emit_array_copy_loop(context: ArrayCopyLoopContext<'_>) -> Result<(), Diagnostics> {
    let ArrayCopyLoopContext {
        node,
        site,
        source,
        destination,
        source_index,
        source_index_region,
        destination_index,
        length,
        length_region,
        temp,
        elem_width,
        elem_schema_ref,
        scratch,
        assigned,
        outcome,
        return_label,
        code,
    } = context;
    let loop_start = code.label();
    let done = code.label();
    code.bind(loop_start, node.span)?;
    code.push(WeavyOp::LtI64 {
        dst: scratch.condition.byte_offset(),
        a: source_index.byte_offset(),
        b: length.byte_offset(),
    });
    code.jump_if_zero(scratch.condition, done);
    code.push(WeavyOp::LoadArray {
        dst: temp.region.start().byte_offset(),
        status: scratch.status.byte_offset(),
        array: source.region.start().byte_offset(),
        index: source_index.byte_offset(),
        elem_width,
        elem_schema_ref: i64::from(elem_schema_ref.0),
    });
    emit_array_load_status_checks(ArrayLoadStatusContext {
        node,
        site,
        index: source_index_region,
        length: length_region,
        scratch,
        assigned,
        outcome,
        return_label,
        code,
    })?;
    code.push(WeavyOp::ArrayStore {
        status: scratch.status.byte_offset(),
        array: destination.start().byte_offset(),
        index: destination_index.byte_offset(),
        src: temp.region.start().byte_offset(),
        elem_width,
        elem_schema_ref: i64::from(elem_schema_ref.0),
    });
    emit_array_status_machine_checks(node, site, scratch, assigned, outcome, return_label, code)?;
    code.push(WeavyOp::ConstI64 {
        dst: scratch.condition.byte_offset(),
        value: 1,
    });
    code.push(WeavyOp::AddI64 {
        dst: source_index.byte_offset(),
        a: source_index.byte_offset(),
        b: scratch.condition.byte_offset(),
    });
    if source_index != destination_index {
        code.push(WeavyOp::AddI64 {
            dst: destination_index.byte_offset(),
            a: destination_index.byte_offset(),
            b: scratch.condition.byte_offset(),
        });
    }
    code.jump(loop_start);
    code.bind(done, node.span)
}

fn emit_array_status_machine_checks(
    node: &Node,
    site: u32,
    scratch: OutcomeScratch,
    assigned: AssignedOutcomeScratch,
    outcome: WeavyRegionId,
    return_label: CodeLabel,
    code: &mut CodeBuilder,
) -> Result<(), Diagnostics> {
    let success = code.label();
    for expected in [
        ArrayOpStatus::Ok,
        ArrayOpStatus::InvalidHandle,
        ArrayOpStatus::MalformedPayload,
        ArrayOpStatus::WidthMismatch,
        ArrayOpStatus::SchemaMismatch,
        ArrayOpStatus::OutOfRange,
        ArrayOpStatus::Overflow,
        ArrayOpStatus::AllocationFailed,
        ArrayOpStatus::Uninitialized,
    ] {
        let next = code.label();
        code.push(WeavyOp::ArrayStatusIs {
            dst: scratch.condition.byte_offset(),
            status: scratch.status.byte_offset(),
            expected,
        });
        code.jump_if_zero(scratch.condition, next);
        if expected == ArrayOpStatus::Ok {
            code.jump(success);
        } else {
            code.push(WeavyOp::ConstI64 {
                dst: scratch.fields[0].byte_offset(),
                value: i64::from(site),
            });
            code.push(WeavyOp::ConstI64 {
                dst: scratch.fields[1].byte_offset(),
                value: expected as i64,
            });
            code.push(WeavyOp::EnumConstruct {
                dst: outcome,
                variant: 2,
                fields: vec![
                    StructuralFieldSource {
                        field: 0,
                        source: assigned.fields[0],
                    },
                    StructuralFieldSource {
                        field: 1,
                        source: assigned.fields[1],
                    },
                ],
            });
            code.jump(return_label);
        }
        code.bind(next, node.span)?;
    }
    code.bind(success, node.span)
}

struct ArrayLoadStatusContext<'a> {
    node: &'a Node,
    site: u32,
    index: WeavyRegionId,
    length: WeavyRegionId,
    scratch: OutcomeScratch,
    assigned: AssignedOutcomeScratch,
    outcome: WeavyRegionId,
    return_label: CodeLabel,
    code: &'a mut CodeBuilder,
}

fn emit_array_load_status_checks(context: ArrayLoadStatusContext<'_>) -> Result<(), Diagnostics> {
    let ArrayLoadStatusContext {
        node,
        site,
        index,
        length,
        scratch,
        assigned,
        outcome,
        return_label,
        code,
    } = context;
    let success = code.label();
    for expected in [ArrayOpStatus::Ok, ArrayOpStatus::OutOfRange] {
        let next = code.label();
        code.push(WeavyOp::ArrayStatusIs {
            dst: scratch.condition.byte_offset(),
            status: scratch.status.byte_offset(),
            expected,
        });
        code.jump_if_zero(scratch.condition, next);
        if expected == ArrayOpStatus::Ok {
            code.jump(success);
        } else {
            code.push(WeavyOp::ConstI64 {
                dst: scratch.fields[0].byte_offset(),
                value: i64::from(site),
            });
            code.push(WeavyOp::EnumConstruct {
                dst: outcome,
                variant: 1,
                fields: vec![
                    StructuralFieldSource {
                        field: 0,
                        source: assigned.fields[0],
                    },
                    StructuralFieldSource {
                        field: 1,
                        source: index,
                    },
                    StructuralFieldSource {
                        field: 2,
                        source: length,
                    },
                ],
            });
            code.jump(return_label);
        }
        code.bind(next, node.span)?;
    }
    emit_array_status_machine_checks(node, site, scratch, assigned, outcome, return_label, code)?;
    code.bind(success, node.span)
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
        ValueRepresentation::RealizedHandle => copy_region(node, source.region, destination),
        ValueRepresentation::InlineComposite => Ok(vec![WeavyOp::CopyValue {
            dst: destination_region,
            src: source.region_id,
        }]),
        ValueRepresentation::CodataRecipe => Err(lowering_diagnostic(
            node.span,
            "codata recipes cannot be copied as frame values",
        )),
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
        Type::Array(_) | Type::Map { .. } | Type::Set(_) => Ok(ValueRepresentation::RealizedHandle),
        Type::Stream { .. } => Ok(ValueRepresentation::CodataRecipe),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::Compiler;
    use weavy::ProgramDefect;
    use weavy::task::FnId;

    const SOURCE: &str = r#"
#[test]
fn attribution() -> Stream<Check> {
    yield expect_eq(1 + 2, 3);
}
"#;

    const ARRAY_SOURCE: &str = r#"
#[test]
fn schema_order() -> Stream<Check> {
    let xs = [1, 2];
    yield expect_eq(xs.len(), 2);
}
"#;

    fn array_schema_contracts(source: &str) -> (SchemaAssignments, Vec<WeavySchemaContract>) {
        let module = Compiler::new()
            .compile(source)
            .expect("array source compiles");
        let partitioned = module.partition_test(&module.tests[0]);
        let island = &partitioned.islands[0];
        let assignments = SchemaAssignments::build(island, true).expect("closed schema assignment");
        let layouts = BTreeMap::new();
        let constants = BTreeMap::new();
        let regions = RegionAssignments {
            nodes: BTreeMap::new(),
            control: BTreeMap::new(),
            temps: BTreeMap::new(),
            ordered_cursors: BTreeMap::new(),
            closures: BTreeMap::new(),
            array_map_temps: BTreeMap::new(),
            outcomes: BTreeMap::new(),
            outcome_scratch: BTreeMap::new(),
            call_outcomes: BTreeMap::new(),
        };
        let mut builder = ProgramContractBuilder {
            layouts: &layouts,
            constant_closures: &constants,
            regions: &regions,
            schemas_preassigned: &assignments,
            function_order: Vec::new(),
            closure_targets: BTreeSet::new(),
            callable_outcomes: false,
            calls: Vec::new(),
            schemas: vec![empty_schema(); assignments.types.len()],
            schema_ready: vec![false; assignments.types.len()],
            value_shapes: Vec::new(),
            value_shape_keys: Vec::new(),
        };
        for ty in assignments.types.clone() {
            builder
                .schema_for_type(&ty, Span { start: 0, end: 0 })
                .expect("predeclared schema materializes");
        }
        let schemas = std::mem::take(&mut builder.schemas);
        drop(builder);
        (assignments, schemas)
    }

    #[test]
    fn closed_schema_assignment_is_span_independent_and_dense_arrays_are_exact() {
        let (cold, cold_contracts) = array_schema_contracts(ARRAY_SOURCE);
        let (shifted, shifted_contracts) = array_schema_contracts(&format!("\n\n{ARRAY_SOURCE}"));
        assert_eq!(
            cold.types
                .iter()
                .map(crate::vir::canonical_type)
                .collect::<Vec<_>>(),
            shifted
                .types
                .iter()
                .map(crate::vir::canonical_type)
                .collect::<Vec<_>>()
        );
        assert_eq!(cold_contracts, shifted_contracts);

        let array = Type::array(Type::Int);
        let array_ref = cold
            .schema_for(&array, Span { start: 0, end: 0 })
            .expect("array has preassigned witness");
        let element = cold
            .schema_for(&Type::Int, Span { start: 0, end: 0 })
            .expect("element has preassigned witness");
        let array_contract = &cold_contracts[array_ref.0 as usize];
        assert_eq!(
            array_contract.payload,
            WeavyPayloadKind::DenseArray { element }
        );
        assert_eq!(
            cold_contracts[element.0 as usize].inline,
            WeavyRegionShape::word(WeavyWordKind::Scalar)
        );
        assert_eq!(cold_contracts[element.0 as usize].value_shape, None);
    }

    #[test]
    fn program_error_keeps_its_typed_cause_and_current_pc_span() {
        let module = Compiler::new().compile(SOURCE).expect("source compiles");
        let partitioned = module.partition_test(&module.tests[0]);
        let island = &partitioned.islands[0];
        let attribution = attribution_for(island);
        let mut cache = LoweringCache::default();
        let lowered = cache.get_or_lower(island).expect("source lowers");
        let pc = 0;
        let node = lowered.node_for_pc(0, pc).expect("first pc has a node");
        let source = attribution
            .source_for_node(node)
            .expect("node has current source attribution");
        let error = weavy::ProgramError {
            function: Some(FnId(0)),
            pc: Some(pc as usize),
            defect: ProgramDefect::ReachableFallthrough,
        };
        let machine = MachineError::program(
            MachineOperation::LoweringVerification,
            error.clone(),
            program_error_attribution(&error, &lowered.pc_nodes, &attribution),
            lowered.demand_key,
        );
        assert_eq!(
            machine.cause,
            crate::runtime::MachineCause::Program(Box::new(error.clone()))
        );
        let mapped = machine.attribution.expect("pc maps to a Vix source node");
        assert_eq!(mapped.span, source.span);
        assert_eq!(mapped.weavy_function, Some(FnId(0)));
        assert_eq!(mapped.weavy_pc, Some(pc as usize));
        assert_eq!(machine.demand_chain, [lowered.demand_key]);

        let shifted = format!("\n\n{SOURCE}");
        let shifted_module = Compiler::new()
            .compile(&shifted)
            .expect("shifted source compiles");
        let shifted_partitioned = shifted_module.partition_test(&shifted_module.tests[0]);
        let shifted_attribution = attribution_for(&shifted_partitioned.islands[0]);
        let shifted_mapped =
            program_error_attribution(&error, &lowered.pc_nodes, &shifted_attribution)
                .expect("cached pc maps against fresh compilation attribution");
        assert_ne!(mapped.span, shifted_mapped.span);
    }
}
