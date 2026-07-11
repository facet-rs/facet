//! Static admission for [`crate::task::Program`].
//!
//! This module pairs raw task programs with an explicit contract, validates
//! every instruction and frame access, and returns an opaque [`VerifiedProgram`]
//! carrying the facts admitted execution lanes consume.
//!
//! The verifier is additive during migration: legacy task/JIT entry points still
//! accept [`Program`] directly. Until those callers move and the raw entry points
//! are removed, `machine.execution.verified-admission` is not implemented.

use core::fmt;

use crate::mem::Layout;
use crate::task::{ArgCopy, FnId, Op, Program, StructuralFieldSource};

const WORD_SIZE: usize = size_of::<i64>();
const WORD_SIZE_U32: u32 = 8;

/// Program-local index into [`ProgramContract::schemas`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SchemaRef(pub u32);

/// Program-local index into [`ProgramContract::calls`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct CallContractId(pub u32);

/// Program-local index into [`ProgramContract::value_shapes`].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ValueShapeRef(pub u32);

/// Program-local index into a [`FrameContract`]'s regions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RegionId(pub u32);

/// One machine interpretation allowed for a frame word.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WordKind {
    Scalar,
    Status,
    Handle(SchemaRef),
    Callable(CallContractId),
    Opaque,
}

/// A nonempty, sorted, deduplicated set of allowed word kinds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AllowedKinds {
    kinds: Vec<WordKind>,
}

impl AllowedKinds {
    #[must_use]
    pub fn new(kind: WordKind) -> Self {
        Self { kinds: vec![kind] }
    }

    /// Add another allowed kind while preserving canonical ordering.
    #[must_use]
    pub fn allowing(mut self, kind: WordKind) -> Self {
        if let Err(index) = self.kinds.binary_search(&kind) {
            self.kinds.insert(index, kind);
        }
        self
    }

    #[must_use]
    pub fn as_slice(&self) -> &[WordKind] {
        &self.kinds
    }

    #[must_use]
    pub fn contains(&self, kind: WordKind) -> bool {
        self.kinds.binary_search(&kind).is_ok()
    }

    pub(crate) fn is_exactly(&self, kind: WordKind) -> bool {
        self.kinds.len() == 1 && self.kinds[0] == kind
    }

    fn is_subset_of(&self, destination: &Self) -> bool {
        self.kinds.iter().all(|kind| destination.contains(*kind))
    }
}

impl From<WordKind> for AllowedKinds {
    fn from(kind: WordKind) -> Self {
        Self::new(kind)
    }
}

/// Allowed kinds for each consecutive word of a region.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RegionShape {
    pub words: Vec<AllowedKinds>,
}

impl RegionShape {
    #[must_use]
    pub fn new(words: Vec<AllowedKinds>) -> Self {
        Self { words }
    }

    #[must_use]
    pub fn word(kind: WordKind) -> Self {
        Self::new(vec![kind.into()])
    }

    #[must_use]
    pub fn checked_byte_len(&self) -> Option<usize> {
        self.words.len().checked_mul(WORD_SIZE)
    }
}

/// One nonoverlapping declared region in a function frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameRegion {
    pub offset: u32,
    pub shape: RegionShape,
    pub value_shape: Option<ValueShapeRef>,
}

impl FrameRegion {
    #[must_use]
    pub fn new(offset: u32, shape: RegionShape) -> Self {
        Self {
            offset,
            shape,
            value_shape: None,
        }
    }

    #[must_use]
    pub fn with_value_shape(mut self, value_shape: ValueShapeRef) -> Self {
        self.value_shape = Some(value_shape);
        self
    }
}

/// The sidecar declaration for one function frame.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FrameContract {
    pub layout: Layout,
    pub regions: Vec<FrameRegion>,
}

/// The concrete ABI declaration for one function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionContract {
    pub frame: FrameContract,
    pub entries: Vec<RegionId>,
    pub result: RegionId,
    pub call_contract: Option<CallContractId>,
}

/// A function-independent indirect-call ABI.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CallContract {
    pub entries: Vec<FrameRegion>,
    pub result: FrameRegion,
}

/// One field use in a structural value shape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueFieldUse {
    pub offset: u32,
    pub shape: RegionShape,
    pub value_shape: Option<ValueShapeRef>,
}

impl ValueFieldUse {
    #[must_use]
    pub fn new(offset: u32, shape: RegionShape) -> Self {
        Self {
            offset,
            shape,
            value_shape: None,
        }
    }

    #[must_use]
    pub fn with_value_shape(mut self, value_shape: ValueShapeRef) -> Self {
        self.value_shape = Some(value_shape);
        self
    }
}

/// Discriminant location for a compact enum value shape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueSelector {
    pub offset: u32,
    pub shape: RegionShape,
}

/// One selector-correlated interpretation of a compact enum payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueVariant {
    pub fields: Vec<ValueFieldUse>,
}

/// Structural interpretation of a flattened value shape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValueShapeKind {
    Product {
        fields: Vec<ValueFieldUse>,
    },
    Enum {
        selector: ValueSelector,
        variants: Vec<ValueVariant>,
    },
}

/// A program-local proof identity for a complete structural value shape.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueShapeContract {
    pub shape: RegionShape,
    pub kind: ValueShapeKind,
}

/// Dynamic payload representation for a program-local schema.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PayloadKind {
    Inline,
    OpaqueBytes { byte_comparable: bool },
    DenseArray { element: SchemaRef },
    OrderedCollection(OrderedCollectionContract),
}

/// The value arity represented by one persistent ordered-collection page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OrderedCollectionKind {
    Map,
    Set,
}

/// Closed, verifier-owned witnesses for an immutable persistent ordered
/// collection. `key` is compared structurally by the lowering admitted for
/// this exact schema; Map rows must be the complete `key`/`value` product.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct OrderedCollectionContract {
    pub kind: OrderedCollectionKind,
    pub key: SchemaRef,
    pub value: Option<SchemaRef>,
    pub row: SchemaRef,
    pub fanout: u16,
}

/// Static inline and dynamic payload facts for one program-local schema.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SchemaContract {
    pub inline: RegionShape,
    pub value_shape: Option<ValueShapeRef>,
    pub payload: PayloadKind,
}

/// Verification sidecar consumed with a raw task program.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProgramContract {
    pub functions: Vec<FunctionContract>,
    pub calls: Vec<CallContract>,
    pub schemas: Vec<SchemaContract>,
    pub value_shapes: Vec<ValueShapeContract>,
}

/// Drive-time table sizes proven necessary by the verifier.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DriveRequirements {
    pub await_inputs: usize,
    pub hosts: usize,
}

/// The dynamic check retained at an indirect-call site.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct IndirectCallObligation {
    pub function_count: usize,
    pub contract: CallContractId,
}

/// Cached call facts for one program counter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallSiteFacts {
    Direct {
        callee: FnId,
        result_size: usize,
    },
    Indirect {
        result_size: usize,
        obligation: IndirectCallObligation,
    },
}

/// Facts cached for one instruction.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PcFacts {
    reachable: bool,
    call: Option<CallSiteFacts>,
}

impl PcFacts {
    #[must_use]
    pub fn is_reachable(&self) -> bool {
        self.reachable
    }

    #[must_use]
    pub fn call(&self) -> Option<CallSiteFacts> {
        self.call
    }
}

/// Facts cached for one function.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionFacts {
    call_contract: Option<CallContractId>,
    pcs: Vec<PcFacts>,
}

impl FunctionFacts {
    #[must_use]
    pub fn call_contract(&self) -> Option<CallContractId> {
        self.call_contract
    }

    #[must_use]
    pub fn pcs(&self) -> &[PcFacts] {
        &self.pcs
    }

    #[must_use]
    pub fn pc(&self, pc: usize) -> Option<&PcFacts> {
        self.pcs.get(pc)
    }
}

/// Static facts computed once during admission.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ProgramFacts {
    functions: Vec<FunctionFacts>,
}

impl ProgramFacts {
    #[must_use]
    pub fn functions(&self) -> &[FunctionFacts] {
        &self.functions
    }

    #[must_use]
    pub fn function(&self, function: FnId) -> Option<&FunctionFacts> {
        let index = usize::try_from(function.0).ok()?;
        self.functions.get(index)
    }
}

/// A verified program and its retained proof material.
#[derive(Debug)]
pub struct VerifiedProgram {
    program: Program,
    contract: ProgramContract,
    facts: ProgramFacts,
    drive_requirements: DriveRequirements,
}

impl VerifiedProgram {
    #[must_use]
    pub fn program(&self) -> &Program {
        &self.program
    }

    #[must_use]
    pub fn contract(&self) -> &ProgramContract {
        &self.contract
    }

    #[must_use]
    pub fn facts(&self) -> &ProgramFacts {
        &self.facts
    }

    #[must_use]
    pub fn drive_requirements(&self) -> DriveRequirements {
        self.drive_requirements
    }

    #[cfg(test)]
    pub(crate) fn clear_call_facts_for_test(&mut self, function: FnId, pc: usize) {
        if let Some(function) = self.facts.functions.get_mut(function.0 as usize)
            && let Some(pc) = function.pcs.get_mut(pc)
        {
            pc.call = None;
        }
    }
}

/// Which frame access an instruction was validating.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum AccessRole {
    Destination,
    LeftOperand,
    RightOperand,
    Source,
    Condition,
    Callee,
    ArgumentSource { index: usize },
    CallResult,
    ReturnValue,
    AwaitDestination,
    CompareLeft,
    CompareRight,
    ArrayHandle,
    ArrayStatus,
    ArrayStatusSource,
    ArrayCount,
    ArrayIndex,
    ArrayElementSource,
    ArrayElementDestination,
    ArrayLengthDestination,
    OrderedCollectionHandle,
    OrderedStatus,
    OrderedCursorDestination,
    OrderedCursorSource,
    OrderedPresent,
    OrderedKeyDestination,
    OrderedValueDestination,
    OrderedChildHandle,
}

/// Structural failure for a checked frame range.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AccessDefect {
    OffsetNotWordAligned {
        offset: u32,
    },
    SizeNotWordAligned {
        size: usize,
    },
    RangeOverflow {
        offset: u32,
        size: usize,
    },
    OutOfBounds {
        offset: u32,
        size: usize,
        frame_size: usize,
    },
    UndeclaredWord {
        offset: u32,
    },
    UndeclaredRegion {
        offset: u32,
        size: usize,
    },
}

/// Machine-kind class required by an instruction operand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum KindRequirement {
    Scalar,
    Status,
    Callable,
    Handle,
    ConstantWord,
}

/// An instruction deliberately outside this admission checkpoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnsupportedOp {
    LoadIndexedI64,
    StoreIndexedI64,
    ArrayStoreWord,
    LoadArrayWord,
    HostCall,
    HostCallYield,
}

/// The owner of a word shape referenced by a contract table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ShapeOwner {
    FrameRegion(RegionId),
    CallEntry {
        contract: CallContractId,
        index: usize,
    },
    CallResult(CallContractId),
    SchemaInline(SchemaRef),
    ValueShape(ValueShapeRef),
    ValueShapeSelector(ValueShapeRef),
    ValueShapeProductField {
        value_shape: ValueShapeRef,
        field: usize,
    },
    ValueShapeVariantField {
        value_shape: ValueShapeRef,
        variant: usize,
        field: usize,
    },
}

/// Exact contract site containing an invalid program-local reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReferenceSite {
    Word { owner: ShapeOwner, word: usize },
    DenseArrayElement { schema: SchemaRef },
    OrderedCollectionKey { schema: SchemaRef },
    OrderedCollectionValue { schema: SchemaRef },
    OrderedCollectionRow { schema: SchemaRef },
    ArrayElementWitness,
}

/// Exact contract site containing an invalid structural value-shape reference.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueShapeReferenceSite {
    FrameRegion(RegionId),
    CallEntry {
        contract: CallContractId,
        index: usize,
    },
    CallResult(CallContractId),
    SchemaInline(SchemaRef),
    ProductField {
        value_shape: ValueShapeRef,
        field: usize,
    },
    VariantField {
        value_shape: ValueShapeRef,
        variant: usize,
        field: usize,
    },
}

/// Whether a structural field belongs to a product or one enum variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ValueFieldSite {
    Product { field: usize },
    Variant { variant: usize, field: usize },
}

/// Whether a bad frame-region index was used as an entry or result.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RegionReference {
    Entry { index: usize },
    Result,
}

/// A program-local table whose indices must fit its public identifier type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProgramTable {
    Functions,
    CallContracts,
    Schemas,
    ValueShapes,
    FrameRegions,
}

/// Typed cause of a verification failure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ProgramDefect {
    TableTooLarge {
        table: ProgramTable,
        len: usize,
    },
    FunctionContractCount {
        functions: usize,
        contracts: usize,
    },
    DuplicateCallContract {
        first: CallContractId,
        duplicate: CallContractId,
    },
    FrameLayoutMismatch {
        program: Layout,
        contract: Layout,
    },
    InvalidFrameAlignment {
        align: usize,
        requires_word_alignment: bool,
    },
    ShapeSizeOverflow {
        owner: ShapeOwner,
    },
    RegionOffsetNotWordAligned {
        region: RegionId,
        offset: u32,
    },
    RegionEndOverflow {
        region: RegionId,
        offset: u32,
        size: usize,
    },
    RegionOutOfBounds {
        region: RegionId,
        end: usize,
        frame_size: usize,
    },
    RegionOverlap {
        first: RegionId,
        second: RegionId,
    },
    RegionReferenceOutOfRange {
        reference: RegionReference,
        region: RegionId,
        region_count: usize,
    },
    DuplicateEntryRegion {
        first: usize,
        duplicate: usize,
        region: RegionId,
    },
    CallRegionOffsetNotWordAligned {
        owner: ShapeOwner,
        offset: u32,
    },
    CallRegionEndOverflow {
        owner: ShapeOwner,
        offset: u32,
        size: usize,
    },
    SchemaReferenceOutOfRange {
        site: ReferenceSite,
        schema: SchemaRef,
        schema_count: usize,
    },
    CallContractReferenceOutOfRange {
        site: ReferenceSite,
        contract: CallContractId,
        contract_count: usize,
    },
    ValueShapeReferenceOutOfRange {
        site: ValueShapeReferenceSite,
        value_shape: ValueShapeRef,
        value_shape_count: usize,
    },
    StructuralShapeMismatch {
        site: ValueShapeReferenceSite,
        value_shape: ValueShapeRef,
        expected: RegionShape,
        actual: RegionShape,
    },
    ValueShapeSelectorOffsetNotWordAligned {
        value_shape: ValueShapeRef,
        offset: u32,
    },
    ValueShapeSelectorInvalidShape {
        value_shape: ValueShapeRef,
        shape: RegionShape,
    },
    ValueShapeSelectorKinds {
        value_shape: ValueShapeRef,
        selector: RegionShape,
        parent: RegionShape,
    },
    ValueShapeSelectorOutOfBounds {
        value_shape: ValueShapeRef,
        offset: u32,
        size: usize,
        shape_size: usize,
    },
    ValueShapeFieldOffsetNotWordAligned {
        value_shape: ValueShapeRef,
        site: ValueFieldSite,
        offset: u32,
    },
    ValueShapeFieldEndOverflow {
        value_shape: ValueShapeRef,
        site: ValueFieldSite,
        offset: u32,
        size: usize,
    },
    ValueShapeFieldOutOfBounds {
        value_shape: ValueShapeRef,
        site: ValueFieldSite,
        end: usize,
        shape_size: usize,
    },
    ValueShapeFieldKinds {
        value_shape: ValueShapeRef,
        site: ValueFieldSite,
        field: RegionShape,
        parent: RegionShape,
    },
    ValueShapeFieldRequiresRef {
        value_shape: ValueShapeRef,
        site: ValueFieldSite,
        field: RegionShape,
    },
    ValueShapeFieldOverlap {
        value_shape: ValueShapeRef,
        first: ValueFieldSite,
        second: ValueFieldSite,
    },
    ValueShapeProductGap {
        value_shape: ValueShapeRef,
        offset: usize,
    },
    ValueShapeFieldOverlapsSelector {
        value_shape: ValueShapeRef,
        variant: usize,
        field: usize,
    },
    ValueShapeCycle {
        value_shape: ValueShapeRef,
    },
    FunctionCallContractOutOfRange {
        contract: CallContractId,
        contract_count: usize,
    },
    FunctionCallContractMismatch {
        contract: CallContractId,
    },
    Access {
        role: AccessRole,
        defect: AccessDefect,
    },
    KindMismatch {
        role: AccessRole,
        required: KindRequirement,
        allowed: AllowedKinds,
    },
    IncompatibleWordKinds {
        source: AllowedKinds,
        destination: AllowedKinds,
    },
    StructuralTransferMismatch {
        source: Option<ValueShapeRef>,
        destination: Option<ValueShapeRef>,
    },
    StructuralPartialCopy {
        source: Option<ValueShapeRef>,
        destination: Option<ValueShapeRef>,
        source_size: usize,
        destination_size: usize,
    },
    RawStructuralWordAccess {
        role: AccessRole,
        region: RegionId,
        value_shape: ValueShapeRef,
    },
    StructuralRegionOutOfRange {
        region: RegionId,
        region_count: usize,
    },
    StructuralRegionRequiresShape {
        region: RegionId,
    },
    StructuralKindMismatch {
        region: RegionId,
        value_shape: ValueShapeRef,
        expected: StructuralKind,
    },
    StructuralFieldOutOfRange {
        value_shape: ValueShapeRef,
        variant: Option<u32>,
        field: u32,
        field_count: usize,
    },
    StructuralFieldCount {
        value_shape: ValueShapeRef,
        variant: Option<u32>,
        expected: usize,
        actual: usize,
    },
    DuplicateStructuralField {
        value_shape: ValueShapeRef,
        variant: Option<u32>,
        field: u32,
    },
    StructuralFieldSourceMismatch {
        value_shape: ValueShapeRef,
        variant: Option<u32>,
        field: u32,
        source: RegionId,
        expected_shape: RegionShape,
        actual_shape: RegionShape,
        expected_value_shape: Option<ValueShapeRef>,
        actual_value_shape: Option<ValueShapeRef>,
    },
    EnumVariantOutOfRange {
        value_shape: ValueShapeRef,
        variant: u32,
        variant_count: usize,
    },
    CallArgumentCount {
        expected: usize,
        actual: usize,
    },
    CallArgumentDestination {
        index: usize,
        expected_offset: u32,
        expected_size: usize,
        actual_offset: u32,
        actual_size: u32,
    },
    CallArgumentKinds {
        index: usize,
        source: RegionShape,
        destination: RegionShape,
    },
    CallResultKinds {
        source: RegionShape,
        destination: RegionShape,
    },
    ConstantCallableTargetOutOfRange {
        value: i64,
        function_count: usize,
    },
    ConstantCallableContractMismatch {
        target: FnId,
        declared: Option<CallContractId>,
        allowed: Vec<CallContractId>,
    },
    JumpTargetOutOfRange {
        target: u32,
        code_len: usize,
    },
    DirectCalleeOutOfRange {
        callee: FnId,
        function_count: usize,
    },
    ReturnRegionMismatch {
        expected: RegionId,
        actual: RegionId,
    },
    CompareSchemaMismatch {
        left: SchemaRef,
        right: SchemaRef,
    },
    SchemaNotByteComparable {
        schema: SchemaRef,
    },
    DynamicSchemaInlineMismatch {
        schema: SchemaRef,
        inline: RegionShape,
    },
    OrderedCollectionFanout {
        schema: SchemaRef,
        fanout: u16,
    },
    OrderedCollectionValueArity {
        schema: SchemaRef,
        kind: OrderedCollectionKind,
        has_value: bool,
    },
    OrderedCollectionRowShape {
        schema: SchemaRef,
        expected: RegionShape,
        actual: RegionShape,
    },
    /// An ordered-collection op's static schema witness was not a valid schema
    /// index.
    OrderedCollectionWitnessOutOfRange {
        witness: i64,
        schema_count: usize,
    },
    /// An ordered-collection op's collection handle schema did not match the
    /// op's static schema witness.
    OrderedCollectionSchemaMismatch {
        handle: SchemaRef,
        witness: SchemaRef,
    },
    /// An ordered-collection op named a handle whose schema is not an ordered
    /// collection.
    OrderedCollectionSchemaNotCollection {
        schema: SchemaRef,
    },
    /// An ordered cursor destination was not exactly two internal opaque words.
    OrderedCursorRegionShape {
        region: RegionId,
        shape: RegionShape,
    },
    /// An opaque cursor word escaped its internal confinement: it appeared at a
    /// function/call entry or result, i.e. it could be published or aliased
    /// across a call boundary.
    OpaqueRegionEscapes {
        owner: ShapeOwner,
    },
    /// An opaque cursor word was copied. Cursors are single-use and may never be
    /// duplicated by a raw word copy.
    OpaqueWordNotCopyable {
        role: AccessRole,
    },
    /// A probe key destination's static width did not match its key schema's
    /// exact inline byte length.
    OrderedKeyWidth {
        schema: SchemaRef,
        expected: usize,
        actual: u32,
    },
    /// A probe key destination's region shape did not match its key schema's
    /// inline shape or structural value shape exactly.
    OrderedKeyShapeMismatch {
        schema: SchemaRef,
        expected: RegionShape,
        actual: RegionShape,
    },
    /// A probe value destination's static width did not match its value schema's
    /// exact inline byte length.
    OrderedValueWidth {
        schema: SchemaRef,
        expected: usize,
        actual: u32,
    },
    /// A probe value destination's region shape did not match its value schema's
    /// inline shape or structural value shape exactly.
    OrderedValueShapeMismatch {
        schema: SchemaRef,
        expected: RegionShape,
        actual: RegionShape,
    },
    /// A value projection targeted a Set collection, which has no values.
    OrderedValueOnSet {
        schema: SchemaRef,
    },
    ArraySchemaNotDense {
        schema: SchemaRef,
    },
    ArrayElementWidth {
        schema: SchemaRef,
        expected: usize,
        actual: u32,
    },
    ArrayElementShapes {
        source: RegionShape,
        destination: RegionShape,
    },
    ArrayElementSchemaMismatch {
        array: SchemaRef,
        expected: SchemaRef,
        actual: SchemaRef,
    },
    ArrayElementWitnessOutOfRange {
        witness: i64,
        schema_count: usize,
    },
    UnsupportedOp {
        op: UnsupportedOp,
    },
    AwaitInputCountOverflow {
        input: u32,
    },
    ReachableFallthrough,
}

/// Structural shape class required by a typed operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StructuralKind {
    Product,
    Enum,
}

/// Which half of an ordered-collection row a payload destination projects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OrderedPart {
    Key,
    Value,
}

/// A payload destination's schema paired with which row half it projects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct OrderedPayload {
    schema: SchemaRef,
    part: OrderedPart,
}

impl OrderedPart {
    fn role(self) -> AccessRole {
        match self {
            OrderedPart::Key => AccessRole::OrderedKeyDestination,
            OrderedPart::Value => AccessRole::OrderedValueDestination,
        }
    }

    fn width_defect(self, schema: SchemaRef, expected: usize, actual: u32) -> ProgramDefect {
        match self {
            OrderedPart::Key => ProgramDefect::OrderedKeyWidth {
                schema,
                expected,
                actual,
            },
            OrderedPart::Value => ProgramDefect::OrderedValueWidth {
                schema,
                expected,
                actual,
            },
        }
    }

    fn shape_defect(
        self,
        schema: SchemaRef,
        expected: RegionShape,
        actual: RegionShape,
    ) -> ProgramDefect {
        match self {
            OrderedPart::Key => ProgramDefect::OrderedKeyShapeMismatch {
                schema,
                expected,
                actual,
            },
            OrderedPart::Value => ProgramDefect::OrderedValueShapeMismatch {
                schema,
                expected,
                actual,
            },
        }
    }
}

/// Structured verifier error. Function and PC are absent only for global
/// contract-table defects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProgramError {
    pub function: Option<FnId>,
    pub pc: Option<usize>,
    pub defect: ProgramDefect,
}

impl fmt::Display for ProgramError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "program verification failed at {:?}:{:?}: {:?}",
            self.function, self.pc, self.defect
        )
    }
}

impl std::error::Error for ProgramError {}

#[derive(Clone, Copy)]
struct RegionBounds {
    start: usize,
    end: usize,
}

impl RegionBounds {
    fn len(self) -> usize {
        self.end - self.start
    }
}

struct ValidatedFrame {
    regions: Vec<RegionBounds>,
    entries: Vec<usize>,
    result: usize,
}

#[derive(Clone, Copy)]
struct StructuralFieldContext {
    function: FnId,
    pc: usize,
    function_index: usize,
    value_shape: ValueShapeRef,
    variant: Option<u32>,
}

struct ValidatedCall {
    entries: Vec<RegionBounds>,
    result: RegionBounds,
}

struct ValidatedContracts<'a> {
    frames: &'a [ValidatedFrame],
    calls: &'a [ValidatedCall],
}

struct CallTarget<'a> {
    entries: Vec<(&'a FrameRegion, RegionBounds)>,
    result: (&'a FrameRegion, RegionBounds),
}

struct ElementRegion {
    offset: u32,
    width: usize,
    element: SchemaRef,
    role: AccessRole,
    source: bool,
}

struct Verifier<'a> {
    program: &'a Program,
    contract: &'a ProgramContract,
}

impl Program {
    /// Consume a raw program and its sidecar, returning an opaque admitted
    /// representation.
    pub fn verify(self, contract: ProgramContract) -> Result<VerifiedProgram, ProgramError> {
        let (facts, drive_requirements) = Verifier {
            program: &self,
            contract: &contract,
        }
        .verify()?;
        Ok(VerifiedProgram {
            program: self,
            contract,
            facts,
            drive_requirements,
        })
    }
}

impl Verifier<'_> {
    fn verify(&self) -> Result<(ProgramFacts, DriveRequirements), ProgramError> {
        self.validate_table_len(ProgramTable::Functions, self.program.fns.len(), None)?;
        self.validate_table_len(ProgramTable::CallContracts, self.contract.calls.len(), None)?;
        self.validate_table_len(ProgramTable::Schemas, self.contract.schemas.len(), None)?;
        self.validate_table_len(
            ProgramTable::ValueShapes,
            self.contract.value_shapes.len(),
            None,
        )?;
        if self.program.fns.len() != self.contract.functions.len() {
            return Err(self.global(ProgramDefect::FunctionContractCount {
                functions: self.program.fns.len(),
                contracts: self.contract.functions.len(),
            }));
        }

        self.validate_value_shape_contracts()?;
        let calls = self.validate_call_contracts()?;
        self.validate_schema_contracts()?;
        let frames = self.validate_function_contracts()?;
        let (mut facts, drive_requirements) = self.validate_ops(&frames, &calls)?;
        self.validate_control_flow(&mut facts)?;
        Ok((facts, drive_requirements))
    }

    fn validate_call_contracts(&self) -> Result<Vec<ValidatedCall>, ProgramError> {
        for duplicate in 0..self.contract.calls.len() {
            if let Some(first) = self.contract.calls[..duplicate]
                .iter()
                .position(|contract| contract == &self.contract.calls[duplicate])
            {
                return Err(self.global(ProgramDefect::DuplicateCallContract {
                    first: CallContractId(first as u32),
                    duplicate: CallContractId(duplicate as u32),
                }));
            }
        }

        let mut validated = Vec::with_capacity(self.contract.calls.len());
        for (contract_index, contract) in self.contract.calls.iter().enumerate() {
            let contract_id = CallContractId(contract_index as u32);
            let mut entries = Vec::with_capacity(contract.entries.len());
            for (entry_index, entry) in contract.entries.iter().enumerate() {
                let owner = ShapeOwner::CallEntry {
                    contract: contract_id,
                    index: entry_index,
                };
                entries.push(self.validate_call_region(entry, owner)?);
            }
            let result =
                self.validate_call_region(&contract.result, ShapeOwner::CallResult(contract_id))?;
            validated.push(ValidatedCall { entries, result });
        }
        Ok(validated)
    }

    fn validate_value_shape_contracts(&self) -> Result<(), ProgramError> {
        for (shape_index, contract) in self.contract.value_shapes.iter().enumerate() {
            let value_shape = ValueShapeRef(shape_index as u32);
            let owner = ShapeOwner::ValueShape(value_shape);
            let shape_size = self.validate_shape(&contract.shape, owner, None)?;
            match &contract.kind {
                ValueShapeKind::Product { fields } => {
                    self.validate_product_fields(value_shape, shape_size, fields)?;
                }
                ValueShapeKind::Enum { selector, variants } => {
                    let selector =
                        self.validate_value_selector(value_shape, shape_size, selector)?;
                    for (variant_index, variant) in variants.iter().enumerate() {
                        self.validate_enum_variant_fields(
                            value_shape,
                            shape_size,
                            variant_index,
                            selector,
                            &variant.fields,
                        )?;
                    }
                }
            }
        }
        self.validate_value_shape_cycles()
    }

    fn validate_value_selector(
        &self,
        value_shape: ValueShapeRef,
        shape_size: usize,
        selector: &ValueSelector,
    ) -> Result<RegionBounds, ProgramError> {
        if !selector.offset.is_multiple_of(WORD_SIZE_U32) {
            return Err(
                self.global(ProgramDefect::ValueShapeSelectorOffsetNotWordAligned {
                    value_shape,
                    offset: selector.offset,
                }),
            );
        }
        if selector.shape.words.len() != 1 || !selector.shape.words[0].is_exactly(WordKind::Scalar)
        {
            return Err(self.global(ProgramDefect::ValueShapeSelectorInvalidShape {
                value_shape,
                shape: selector.shape.clone(),
            }));
        }
        self.validate_shape(
            &selector.shape,
            ShapeOwner::ValueShapeSelector(value_shape),
            None,
        )?;
        let start = usize::try_from(selector.offset).map_err(|_| {
            self.global(ProgramDefect::ValueShapeSelectorOutOfBounds {
                value_shape,
                offset: selector.offset,
                size: WORD_SIZE,
                shape_size,
            })
        })?;
        let end = start.checked_add(WORD_SIZE).ok_or_else(|| {
            self.global(ProgramDefect::ValueShapeSelectorOutOfBounds {
                value_shape,
                offset: selector.offset,
                size: WORD_SIZE,
                shape_size,
            })
        })?;
        if end > shape_size {
            return Err(self.global(ProgramDefect::ValueShapeSelectorOutOfBounds {
                value_shape,
                offset: selector.offset,
                size: WORD_SIZE,
                shape_size,
            }));
        }
        let parent = self.shape_slice(
            &self.contract.value_shapes[value_shape.0 as usize].shape,
            start,
            end,
        );
        if parent != selector.shape {
            return Err(self.global(ProgramDefect::ValueShapeSelectorKinds {
                value_shape,
                selector: selector.shape.clone(),
                parent,
            }));
        }
        Ok(RegionBounds { start, end })
    }

    fn validate_product_fields(
        &self,
        value_shape: ValueShapeRef,
        shape_size: usize,
        fields: &[ValueFieldUse],
    ) -> Result<(), ProgramError> {
        let mut ranges: Vec<RegionBounds> = Vec::with_capacity(fields.len());
        for (field_index, field) in fields.iter().enumerate() {
            let site = ValueFieldSite::Product { field: field_index };
            let owner = ShapeOwner::ValueShapeProductField {
                value_shape,
                field: field_index,
            };
            let nested_site = ValueShapeReferenceSite::ProductField {
                value_shape,
                field: field_index,
            };
            if !field.offset.is_multiple_of(WORD_SIZE_U32) {
                return Err(
                    self.global(ProgramDefect::ValueShapeFieldOffsetNotWordAligned {
                        value_shape,
                        site,
                        offset: field.offset,
                    }),
                );
            }
            let size = self.validate_shape(&field.shape, owner, None)?;
            let start = usize::try_from(field.offset).map_err(|_| {
                self.global(ProgramDefect::ValueShapeFieldEndOverflow {
                    value_shape,
                    site,
                    offset: field.offset,
                    size,
                })
            })?;
            let end = start.checked_add(size).ok_or_else(|| {
                self.global(ProgramDefect::ValueShapeFieldEndOverflow {
                    value_shape,
                    site,
                    offset: field.offset,
                    size,
                })
            })?;
            if end > shape_size {
                return Err(self.global(ProgramDefect::ValueShapeFieldOutOfBounds {
                    value_shape,
                    site,
                    end,
                    shape_size,
                }));
            }
            let parent = self.shape_slice(
                &self.contract.value_shapes[value_shape.0 as usize].shape,
                start,
                end,
            );
            if field.shape != parent {
                return Err(self.global(ProgramDefect::ValueShapeFieldKinds {
                    value_shape,
                    site,
                    field: field.shape.clone(),
                    parent,
                }));
            }
            self.validate_field_ref_or_leaf(value_shape, site, nested_site, field)?;
            let bounds = RegionBounds { start, end };
            for (prior_index, prior) in ranges.iter().copied().enumerate() {
                if prior.start < bounds.end && bounds.start < prior.end {
                    return Err(self.global(ProgramDefect::ValueShapeFieldOverlap {
                        value_shape,
                        first: ValueFieldSite::Product { field: prior_index },
                        second: site,
                    }));
                }
            }
            ranges.push(bounds);
        }

        ranges.sort_by_key(|range| range.start);
        let mut expected_start = 0usize;
        for range in ranges {
            if range.start != expected_start {
                return Err(self.global(ProgramDefect::ValueShapeProductGap {
                    value_shape,
                    offset: expected_start,
                }));
            }
            expected_start = range.end;
        }
        if expected_start != shape_size {
            return Err(self.global(ProgramDefect::ValueShapeProductGap {
                value_shape,
                offset: expected_start,
            }));
        }
        Ok(())
    }

    fn validate_enum_variant_fields(
        &self,
        value_shape: ValueShapeRef,
        shape_size: usize,
        variant_index: usize,
        selector: RegionBounds,
        fields: &[ValueFieldUse],
    ) -> Result<(), ProgramError> {
        let mut ranges: Vec<RegionBounds> = Vec::with_capacity(fields.len());
        for (field_index, field) in fields.iter().enumerate() {
            let site = ValueFieldSite::Variant {
                variant: variant_index,
                field: field_index,
            };
            let owner = ShapeOwner::ValueShapeVariantField {
                value_shape,
                variant: variant_index,
                field: field_index,
            };
            let nested_site = ValueShapeReferenceSite::VariantField {
                value_shape,
                variant: variant_index,
                field: field_index,
            };
            if !field.offset.is_multiple_of(WORD_SIZE_U32) {
                return Err(
                    self.global(ProgramDefect::ValueShapeFieldOffsetNotWordAligned {
                        value_shape,
                        site,
                        offset: field.offset,
                    }),
                );
            }
            let size = self.validate_shape(&field.shape, owner, None)?;
            let start = usize::try_from(field.offset).map_err(|_| {
                self.global(ProgramDefect::ValueShapeFieldEndOverflow {
                    value_shape,
                    site,
                    offset: field.offset,
                    size,
                })
            })?;
            let end = start.checked_add(size).ok_or_else(|| {
                self.global(ProgramDefect::ValueShapeFieldEndOverflow {
                    value_shape,
                    site,
                    offset: field.offset,
                    size,
                })
            })?;
            if end > shape_size {
                return Err(self.global(ProgramDefect::ValueShapeFieldOutOfBounds {
                    value_shape,
                    site,
                    end,
                    shape_size,
                }));
            }
            let parent = self.shape_slice(
                &self.contract.value_shapes[value_shape.0 as usize].shape,
                start,
                end,
            );
            if !shapes_assignable(&field.shape, &parent) {
                return Err(self.global(ProgramDefect::ValueShapeFieldKinds {
                    value_shape,
                    site,
                    field: field.shape.clone(),
                    parent,
                }));
            }
            self.validate_field_ref_or_leaf(value_shape, site, nested_site, field)?;
            let bounds = RegionBounds { start, end };
            if selector.start < bounds.end && bounds.start < selector.end {
                return Err(self.global(ProgramDefect::ValueShapeFieldOverlapsSelector {
                    value_shape,
                    variant: variant_index,
                    field: field_index,
                }));
            }
            for (prior_index, prior) in ranges.iter().copied().enumerate() {
                if prior.start < bounds.end && bounds.start < prior.end {
                    let first = ValueFieldSite::Variant {
                        variant: variant_index,
                        field: prior_index,
                    };
                    return Err(self.global(ProgramDefect::ValueShapeFieldOverlap {
                        value_shape,
                        first,
                        second: site,
                    }));
                }
            }
            ranges.push(bounds);
        }
        Ok(())
    }

    fn validate_field_ref_or_leaf(
        &self,
        value_shape: ValueShapeRef,
        site: ValueFieldSite,
        nested_site: ValueShapeReferenceSite,
        field: &ValueFieldUse,
    ) -> Result<(), ProgramError> {
        if let Some(nested) = field.value_shape {
            self.validate_value_shape_ref(nested_site, nested, &field.shape, None)?;
            return Ok(());
        }
        let is_leaf = field.shape.words.len() == 1 && field.shape.words[0].as_slice().len() == 1;
        if !is_leaf {
            return Err(self.global(ProgramDefect::ValueShapeFieldRequiresRef {
                value_shape,
                site,
                field: field.shape.clone(),
            }));
        }
        Ok(())
    }

    fn validate_value_shape_cycles(&self) -> Result<(), ProgramError> {
        let mut state = vec![0u8; self.contract.value_shapes.len()];
        for index in 0..self.contract.value_shapes.len() {
            self.visit_value_shape(ValueShapeRef(index as u32), &mut state)?;
        }
        Ok(())
    }

    fn visit_value_shape(
        &self,
        value_shape: ValueShapeRef,
        state: &mut [u8],
    ) -> Result<(), ProgramError> {
        let index = value_shape.0 as usize;
        match state[index] {
            1 => {
                return Err(self.global(ProgramDefect::ValueShapeCycle { value_shape }));
            }
            2 => return Ok(()),
            _ => {}
        }
        state[index] = 1;
        for nested in self.value_shape_nested_refs(&self.contract.value_shapes[index].kind) {
            let Some(nested_index) = usize::try_from(nested.0)
                .ok()
                .filter(|nested_index| *nested_index < self.contract.value_shapes.len())
            else {
                continue;
            };
            self.visit_value_shape(ValueShapeRef(nested_index as u32), state)?;
        }
        state[index] = 2;
        Ok(())
    }

    fn value_shape_nested_refs(&self, kind: &ValueShapeKind) -> Vec<ValueShapeRef> {
        let mut refs = Vec::new();
        match kind {
            ValueShapeKind::Product { fields } => {
                refs.extend(fields.iter().filter_map(|field| field.value_shape));
            }
            ValueShapeKind::Enum {
                selector: _,
                variants,
            } => {
                refs.extend(
                    variants
                        .iter()
                        .flat_map(|variant| &variant.fields)
                        .filter_map(|field| field.value_shape),
                );
            }
        }
        refs
    }

    fn shape_slice(&self, shape: &RegionShape, start: usize, end: usize) -> RegionShape {
        let first = start / WORD_SIZE;
        let last = end / WORD_SIZE;
        RegionShape::new(shape.words[first..last].to_vec())
    }

    fn validate_call_region(
        &self,
        region: &FrameRegion,
        owner: ShapeOwner,
    ) -> Result<RegionBounds, ProgramError> {
        if !region.offset.is_multiple_of(WORD_SIZE_U32) {
            return Err(self.global(ProgramDefect::CallRegionOffsetNotWordAligned {
                owner,
                offset: region.offset,
            }));
        }
        let size = self.validate_shape(&region.shape, owner, None)?;
        if shape_has_opaque(&region.shape) {
            return Err(self.global(ProgramDefect::OpaqueRegionEscapes { owner }));
        }
        let start = usize::try_from(region.offset).map_err(|_| {
            self.global(ProgramDefect::CallRegionEndOverflow {
                owner,
                offset: region.offset,
                size,
            })
        })?;
        let end = start.checked_add(size).ok_or_else(|| {
            self.global(ProgramDefect::CallRegionEndOverflow {
                owner,
                offset: region.offset,
                size,
            })
        })?;
        if let Some(value_shape) = region.value_shape {
            let site = self.value_shape_site_for_owner(owner);
            self.validate_value_shape_ref(site, value_shape, &region.shape, None)?;
        }
        Ok(RegionBounds { start, end })
    }

    fn value_shape_site_for_owner(&self, owner: ShapeOwner) -> ValueShapeReferenceSite {
        match owner {
            ShapeOwner::FrameRegion(region) => ValueShapeReferenceSite::FrameRegion(region),
            ShapeOwner::CallEntry { contract, index } => {
                ValueShapeReferenceSite::CallEntry { contract, index }
            }
            ShapeOwner::CallResult(contract) => ValueShapeReferenceSite::CallResult(contract),
            ShapeOwner::SchemaInline(schema) => ValueShapeReferenceSite::SchemaInline(schema),
            ShapeOwner::ValueShapeProductField { value_shape, field } => {
                ValueShapeReferenceSite::ProductField { value_shape, field }
            }
            ShapeOwner::ValueShapeVariantField {
                value_shape,
                variant,
                field,
            } => ValueShapeReferenceSite::VariantField {
                value_shape,
                variant,
                field,
            },
            ShapeOwner::ValueShape(_) | ShapeOwner::ValueShapeSelector(_) => {
                unreachable!("only regions and fields carry structural refs")
            }
        }
    }

    fn validate_value_shape_ref(
        &self,
        site: ValueShapeReferenceSite,
        value_shape: ValueShapeRef,
        actual: &RegionShape,
        function: Option<FnId>,
    ) -> Result<(), ProgramError> {
        let Some(index) = usize::try_from(value_shape.0)
            .ok()
            .filter(|index| *index < self.contract.value_shapes.len())
        else {
            return Err(ProgramError {
                function,
                pc: None,
                defect: ProgramDefect::ValueShapeReferenceOutOfRange {
                    site,
                    value_shape,
                    value_shape_count: self.contract.value_shapes.len(),
                },
            });
        };
        let expected = &self.contract.value_shapes[index].shape;
        if expected != actual {
            return Err(ProgramError {
                function,
                pc: None,
                defect: ProgramDefect::StructuralShapeMismatch {
                    site,
                    value_shape,
                    expected: expected.clone(),
                    actual: actual.clone(),
                },
            });
        }
        Ok(())
    }

    fn validate_schema_contracts(&self) -> Result<(), ProgramError> {
        for (schema_index, schema) in self.contract.schemas.iter().enumerate() {
            let schema_ref = SchemaRef(schema_index as u32);
            self.validate_shape(&schema.inline, ShapeOwner::SchemaInline(schema_ref), None)?;
            if let Some(value_shape) = schema.value_shape {
                self.validate_value_shape_ref(
                    ValueShapeReferenceSite::SchemaInline(schema_ref),
                    value_shape,
                    &schema.inline,
                    None,
                )?;
            }
            if matches!(
                schema.payload,
                PayloadKind::OpaqueBytes { .. }
                    | PayloadKind::DenseArray { .. }
                    | PayloadKind::OrderedCollection(_)
            ) && schema.inline != RegionShape::word(WordKind::Handle(schema_ref))
            {
                return Err(self.global(ProgramDefect::DynamicSchemaInlineMismatch {
                    schema: schema_ref,
                    inline: schema.inline.clone(),
                }));
            }
            if let PayloadKind::DenseArray { element } = schema.payload {
                let Ok(index) = usize::try_from(element.0) else {
                    return Err(self.global(ProgramDefect::SchemaReferenceOutOfRange {
                        site: ReferenceSite::DenseArrayElement { schema: schema_ref },
                        schema: element,
                        schema_count: self.contract.schemas.len(),
                    }));
                };
                if index >= self.contract.schemas.len() {
                    return Err(self.global(ProgramDefect::SchemaReferenceOutOfRange {
                        site: ReferenceSite::DenseArrayElement { schema: schema_ref },
                        schema: element,
                        schema_count: self.contract.schemas.len(),
                    }));
                }
            }
            if let PayloadKind::OrderedCollection(collection) = &schema.payload {
                if collection.fanout < 2 {
                    return Err(self.global(ProgramDefect::OrderedCollectionFanout {
                        schema: schema_ref,
                        fanout: collection.fanout,
                    }));
                }
                let resolve = |reference: SchemaRef, site: ReferenceSite| {
                    self.contract
                        .schemas
                        .get(reference.0 as usize)
                        .ok_or_else(|| {
                            self.global(ProgramDefect::SchemaReferenceOutOfRange {
                                site,
                                schema: reference,
                                schema_count: self.contract.schemas.len(),
                            })
                        })
                };
                let key = resolve(
                    collection.key,
                    ReferenceSite::OrderedCollectionKey { schema: schema_ref },
                )?;
                let row = resolve(
                    collection.row,
                    ReferenceSite::OrderedCollectionRow { schema: schema_ref },
                )?;
                let expected = match (collection.kind, collection.value) {
                    (OrderedCollectionKind::Map, Some(value)) => {
                        let value = resolve(
                            value,
                            ReferenceSite::OrderedCollectionValue { schema: schema_ref },
                        )?;
                        RegionShape::new(
                            [key.inline.words.clone(), value.inline.words.clone()].concat(),
                        )
                    }
                    (OrderedCollectionKind::Set, None) => key.inline.clone(),
                    (kind, value) => {
                        return Err(self.global(ProgramDefect::OrderedCollectionValueArity {
                            schema: schema_ref,
                            kind,
                            has_value: value.is_some(),
                        }));
                    }
                };
                if row.inline != expected {
                    return Err(self.global(ProgramDefect::OrderedCollectionRowShape {
                        schema: schema_ref,
                        expected,
                        actual: row.inline.clone(),
                    }));
                }
            }
        }
        Ok(())
    }

    fn validate_shape(
        &self,
        shape: &RegionShape,
        owner: ShapeOwner,
        function: Option<FnId>,
    ) -> Result<usize, ProgramError> {
        let size = shape.checked_byte_len().ok_or(ProgramError {
            function,
            pc: None,
            defect: ProgramDefect::ShapeSizeOverflow { owner },
        })?;
        for (word, kinds) in shape.words.iter().enumerate() {
            for kind in kinds.as_slice() {
                match *kind {
                    WordKind::Handle(schema) => {
                        let index = usize::try_from(schema.0).map_err(|_| ProgramError {
                            function,
                            pc: None,
                            defect: ProgramDefect::SchemaReferenceOutOfRange {
                                site: ReferenceSite::Word { owner, word },
                                schema,
                                schema_count: self.contract.schemas.len(),
                            },
                        })?;
                        if index >= self.contract.schemas.len() {
                            return Err(ProgramError {
                                function,
                                pc: None,
                                defect: ProgramDefect::SchemaReferenceOutOfRange {
                                    site: ReferenceSite::Word { owner, word },
                                    schema,
                                    schema_count: self.contract.schemas.len(),
                                },
                            });
                        }
                    }
                    WordKind::Callable(contract) => {
                        let index = usize::try_from(contract.0).map_err(|_| ProgramError {
                            function,
                            pc: None,
                            defect: ProgramDefect::CallContractReferenceOutOfRange {
                                site: ReferenceSite::Word { owner, word },
                                contract,
                                contract_count: self.contract.calls.len(),
                            },
                        })?;
                        if index >= self.contract.calls.len() {
                            return Err(ProgramError {
                                function,
                                pc: None,
                                defect: ProgramDefect::CallContractReferenceOutOfRange {
                                    site: ReferenceSite::Word { owner, word },
                                    contract,
                                    contract_count: self.contract.calls.len(),
                                },
                            });
                        }
                    }
                    WordKind::Scalar | WordKind::Status | WordKind::Opaque => {}
                }
            }
        }
        Ok(size)
    }

    fn validate_function_contracts(&self) -> Result<Vec<ValidatedFrame>, ProgramError> {
        let mut frames = Vec::with_capacity(self.program.fns.len());
        for (function_index, (function, contract)) in self
            .program
            .fns
            .iter()
            .zip(&self.contract.functions)
            .enumerate()
        {
            let function_id = FnId(u32::try_from(function_index).map_err(|_| {
                self.global(ProgramDefect::FunctionContractCount {
                    functions: self.program.fns.len(),
                    contracts: self.contract.functions.len(),
                })
            })?);
            if function.frame != contract.frame.layout {
                return Err(self.function(
                    function_id,
                    ProgramDefect::FrameLayoutMismatch {
                        program: function.frame,
                        contract: contract.frame.layout,
                    },
                ));
            }
            self.validate_table_len(
                ProgramTable::FrameRegions,
                contract.frame.regions.len(),
                Some(function_id),
            )?;
            let has_words = contract
                .frame
                .regions
                .iter()
                .any(|region| !region.shape.words.is_empty());
            let align_valid = function.frame.align.is_power_of_two()
                && function.frame.align != 0
                && (!has_words || function.frame.align >= WORD_SIZE);
            if !align_valid {
                return Err(self.function(
                    function_id,
                    ProgramDefect::InvalidFrameAlignment {
                        align: function.frame.align,
                        requires_word_alignment: has_words,
                    },
                ));
            }

            let mut regions = Vec::with_capacity(contract.frame.regions.len());
            for (region_index, region) in contract.frame.regions.iter().enumerate() {
                let region_id = RegionId(region_index as u32);
                if !region.offset.is_multiple_of(WORD_SIZE_U32) {
                    return Err(self.function(
                        function_id,
                        ProgramDefect::RegionOffsetNotWordAligned {
                            region: region_id,
                            offset: region.offset,
                        },
                    ));
                }
                let size = self.validate_shape(
                    &region.shape,
                    ShapeOwner::FrameRegion(region_id),
                    Some(function_id),
                )?;
                if let Some(value_shape) = region.value_shape {
                    self.validate_value_shape_ref(
                        ValueShapeReferenceSite::FrameRegion(region_id),
                        value_shape,
                        &region.shape,
                        Some(function_id),
                    )?;
                }
                let start = usize::try_from(region.offset).map_err(|_| {
                    self.function(
                        function_id,
                        ProgramDefect::RegionEndOverflow {
                            region: region_id,
                            offset: region.offset,
                            size,
                        },
                    )
                })?;
                let end = start.checked_add(size).ok_or_else(|| {
                    self.function(
                        function_id,
                        ProgramDefect::RegionEndOverflow {
                            region: region_id,
                            offset: region.offset,
                            size,
                        },
                    )
                })?;
                if end > function.frame.size {
                    return Err(self.function(
                        function_id,
                        ProgramDefect::RegionOutOfBounds {
                            region: region_id,
                            end,
                            frame_size: function.frame.size,
                        },
                    ));
                }
                regions.push(RegionBounds { start, end });
            }

            for second in 0..regions.len() {
                for first in 0..second {
                    if regions[first].start < regions[second].end
                        && regions[second].start < regions[first].end
                    {
                        return Err(self.function(
                            function_id,
                            ProgramDefect::RegionOverlap {
                                first: RegionId(first as u32),
                                second: RegionId(second as u32),
                            },
                        ));
                    }
                }
            }

            let mut entries = Vec::with_capacity(contract.entries.len());
            for (entry_index, region) in contract.entries.iter().copied().enumerate() {
                let Some(index) = usize::try_from(region.0)
                    .ok()
                    .filter(|index| *index < regions.len())
                else {
                    return Err(self.function(
                        function_id,
                        ProgramDefect::RegionReferenceOutOfRange {
                            reference: RegionReference::Entry { index: entry_index },
                            region,
                            region_count: regions.len(),
                        },
                    ));
                };
                if let Some(first) = entries.iter().position(|candidate| *candidate == index) {
                    return Err(self.function(
                        function_id,
                        ProgramDefect::DuplicateEntryRegion {
                            first,
                            duplicate: entry_index,
                            region,
                        },
                    ));
                }
                if shape_has_opaque(&contract.frame.regions[index].shape) {
                    return Err(self.function(
                        function_id,
                        ProgramDefect::OpaqueRegionEscapes {
                            owner: ShapeOwner::FrameRegion(region),
                        },
                    ));
                }
                entries.push(index);
            }
            let Some(result) = usize::try_from(contract.result.0)
                .ok()
                .filter(|index| *index < regions.len())
            else {
                return Err(self.function(
                    function_id,
                    ProgramDefect::RegionReferenceOutOfRange {
                        reference: RegionReference::Result,
                        region: contract.result,
                        region_count: regions.len(),
                    },
                ));
            };
            if shape_has_opaque(&contract.frame.regions[result].shape) {
                return Err(self.function(
                    function_id,
                    ProgramDefect::OpaqueRegionEscapes {
                        owner: ShapeOwner::FrameRegion(contract.result),
                    },
                ));
            }

            if let Some(call_contract) = contract.call_contract {
                let Some(call_index) = usize::try_from(call_contract.0)
                    .ok()
                    .filter(|index| *index < self.contract.calls.len())
                else {
                    return Err(self.function(
                        function_id,
                        ProgramDefect::FunctionCallContractOutOfRange {
                            contract: call_contract,
                            contract_count: self.contract.calls.len(),
                        },
                    ));
                };
                let call = &self.contract.calls[call_index];
                let entries_match = entries.len() == call.entries.len()
                    && entries
                        .iter()
                        .zip(&call.entries)
                        .all(|(region, expected)| &contract.frame.regions[*region] == expected);
                let concrete_result = &contract.frame.regions[result];
                let result_matches = concrete_result.shape == call.result.shape
                    && concrete_result.value_shape == call.result.value_shape;
                if !entries_match || !result_matches {
                    return Err(self.function(
                        function_id,
                        ProgramDefect::FunctionCallContractMismatch {
                            contract: call_contract,
                        },
                    ));
                }
            }

            frames.push(ValidatedFrame {
                regions,
                entries,
                result,
            });
        }
        Ok(frames)
    }

    fn validate_ops(
        &self,
        frames: &[ValidatedFrame],
        calls: &[ValidatedCall],
    ) -> Result<(ProgramFacts, DriveRequirements), ProgramError> {
        let mut facts = ProgramFacts {
            functions: self
                .program
                .fns
                .iter()
                .zip(&self.contract.functions)
                .map(|(function, contract)| FunctionFacts {
                    call_contract: contract.call_contract,
                    pcs: vec![PcFacts::default(); function.code.len()],
                })
                .collect(),
        };
        let mut requirements = DriveRequirements::default();
        let validated = ValidatedContracts { frames, calls };

        for (function_index, function) in self.program.fns.iter().enumerate() {
            for (pc, op) in function.code.iter().enumerate() {
                let call =
                    self.validate_op(function_index, pc, op, &validated, &mut requirements)?;
                facts.functions[function_index].pcs[pc].call = call;
            }
        }
        Ok((facts, requirements))
    }

    fn validate_op(
        &self,
        function_index: usize,
        pc: usize,
        op: &Op,
        validated: &ValidatedContracts<'_>,
        requirements: &mut DriveRequirements,
    ) -> Result<Option<CallSiteFacts>, ProgramError> {
        let function_id = FnId(function_index as u32);
        let frame = &validated.frames[function_index];
        match op {
            Op::ProductConstruct { dst, fields } => {
                let (value_shape, declared) =
                    self.product_region(function_id, pc, function_index, *dst)?;
                self.validate_structural_fields(
                    StructuralFieldContext {
                        function: function_id,
                        pc,
                        function_index,
                        value_shape,
                        variant: None,
                    },
                    declared,
                    fields,
                )?;
            }
            Op::ProductProject {
                dst,
                product,
                field,
            } => {
                let (value_shape, fields) =
                    self.product_region(function_id, pc, function_index, *product)?;
                let declared =
                    self.structural_field(function_id, pc, value_shape, None, fields, *field)?;
                self.validate_structural_source(
                    StructuralFieldContext {
                        function: function_id,
                        pc,
                        function_index,
                        value_shape,
                        variant: None,
                    },
                    *field,
                    *dst,
                    declared,
                )?;
            }
            Op::CopyValue { dst, src } => {
                let source = self.structural_region(function_id, pc, function_index, *src)?;
                let destination = self.structural_region(function_id, pc, function_index, *dst)?;
                if source.value_shape != destination.value_shape
                    || source.shape != destination.shape
                {
                    return Err(self.op(
                        function_id,
                        pc,
                        ProgramDefect::StructuralTransferMismatch {
                            source: source.value_shape,
                            destination: destination.value_shape,
                        },
                    ));
                }
            }
            Op::EnumConstruct {
                dst,
                variant,
                fields,
            } => {
                let (value_shape, variants) =
                    self.enum_region(function_id, pc, function_index, *dst)?;
                let declared =
                    self.enum_variant(function_id, pc, value_shape, variants, *variant)?;
                self.validate_structural_fields(
                    StructuralFieldContext {
                        function: function_id,
                        pc,
                        function_index,
                        value_shape,
                        variant: Some(*variant),
                    },
                    &declared.fields,
                    fields,
                )?;
            }
            Op::EnumIsVariant {
                dst,
                value,
                variant,
            } => {
                let (value_shape, variants) =
                    self.enum_region(function_id, pc, function_index, *value)?;
                self.enum_variant(function_id, pc, value_shape, variants, *variant)?;
                self.scalar_region(function_id, pc, function_index, *dst)?;
            }
            Op::EnumProjectChecked {
                dst,
                value,
                variant,
                field,
            } => {
                let (value_shape, variants) =
                    self.enum_region(function_id, pc, function_index, *value)?;
                let variant_contract =
                    self.enum_variant(function_id, pc, value_shape, variants, *variant)?;
                let declared = self.structural_field(
                    function_id,
                    pc,
                    value_shape,
                    Some(*variant),
                    &variant_contract.fields,
                    *field,
                )?;
                self.validate_structural_source(
                    StructuralFieldContext {
                        function: function_id,
                        pc,
                        function_index,
                        value_shape,
                        variant: Some(*variant),
                    },
                    *field,
                    *dst,
                    declared,
                )?;
            }
            Op::ConstI64 { dst, value } => {
                self.reject_raw_structural_word(
                    function_id,
                    pc,
                    function_index,
                    frame,
                    *dst,
                    AccessRole::Destination,
                )?;
                let kinds =
                    self.word_kinds(function_id, pc, frame, *dst, AccessRole::Destination)?;
                self.validate_const(function_id, pc, *value, kinds)?;
            }
            Op::AddI64 { dst, a, b }
            | Op::SubI64 { dst, a, b }
            | Op::MulI64 { dst, a, b }
            | Op::DivI64 { dst, a, b }
            | Op::EqI64 { dst, a, b }
            | Op::NeI64 { dst, a, b }
            | Op::LtI64 { dst, a, b }
            | Op::LeI64 { dst, a, b }
            | Op::GtI64 { dst, a, b }
            | Op::GeI64 { dst, a, b }
            | Op::AddF64 { dst, a, b }
            | Op::MulF64 { dst, a, b } => {
                self.reject_raw_structural_word(
                    function_id,
                    pc,
                    function_index,
                    frame,
                    *dst,
                    AccessRole::Destination,
                )?;
                self.reject_raw_structural_word(
                    function_id,
                    pc,
                    function_index,
                    frame,
                    *a,
                    AccessRole::LeftOperand,
                )?;
                self.reject_raw_structural_word(
                    function_id,
                    pc,
                    function_index,
                    frame,
                    *b,
                    AccessRole::RightOperand,
                )?;
                self.require_scalar_write(function_id, pc, frame, *dst, AccessRole::Destination)?;
                self.require_scalar_read(function_id, pc, frame, *a, AccessRole::LeftOperand)?;
                self.require_scalar_read(function_id, pc, frame, *b, AccessRole::RightOperand)?;
            }
            Op::CopyI64 { dst, src } => {
                self.reject_raw_structural_word(
                    function_id,
                    pc,
                    function_index,
                    frame,
                    *dst,
                    AccessRole::Destination,
                )?;
                self.reject_raw_structural_word(
                    function_id,
                    pc,
                    function_index,
                    frame,
                    *src,
                    AccessRole::Source,
                )?;
                let (destination_index, destination_word) =
                    self.word_region_index(function_id, pc, frame, *dst, AccessRole::Destination)?;
                let (source_index, source_word) =
                    self.word_region_index(function_id, pc, frame, *src, AccessRole::Source)?;
                let destination_region =
                    &self.contract.functions[function_index].frame.regions[destination_index];
                let source_region =
                    &self.contract.functions[function_index].frame.regions[source_index];
                let destination = &destination_region.shape.words[destination_word];
                let source = &source_region.shape.words[source_word];
                if destination.contains(WordKind::Opaque) {
                    return Err(self.op(
                        function_id,
                        pc,
                        ProgramDefect::OpaqueWordNotCopyable {
                            role: AccessRole::Destination,
                        },
                    ));
                }
                if source.contains(WordKind::Opaque) {
                    return Err(self.op(
                        function_id,
                        pc,
                        ProgramDefect::OpaqueWordNotCopyable {
                            role: AccessRole::Source,
                        },
                    ));
                }
                if !source.is_subset_of(destination) {
                    return Err(self.op(
                        function_id,
                        pc,
                        ProgramDefect::IncompatibleWordKinds {
                            source: source.clone(),
                            destination: destination.clone(),
                        },
                    ));
                }
                if source_region.value_shape.is_some() || destination_region.value_shape.is_some() {
                    if source_region.value_shape != destination_region.value_shape {
                        return Err(self.op(
                            function_id,
                            pc,
                            ProgramDefect::StructuralTransferMismatch {
                                source: source_region.value_shape,
                                destination: destination_region.value_shape,
                            },
                        ));
                    }
                    let source_size = frame.regions[source_index].len();
                    let destination_size = frame.regions[destination_index].len();
                    if source_size != WORD_SIZE || destination_size != WORD_SIZE {
                        return Err(self.op(
                            function_id,
                            pc,
                            ProgramDefect::StructuralPartialCopy {
                                source: source_region.value_shape,
                                destination: destination_region.value_shape,
                                source_size,
                                destination_size,
                            },
                        ));
                    }
                }
            }
            Op::Jump { target } => self.validate_jump(function_id, pc, *target)?,
            Op::JumpIfZero { value, target } => {
                self.reject_raw_structural_word(
                    function_id,
                    pc,
                    function_index,
                    frame,
                    *value,
                    AccessRole::Condition,
                )?;
                self.require_scalar_read(function_id, pc, frame, *value, AccessRole::Condition)?;
                self.validate_jump(function_id, pc, *target)?;
            }
            Op::Call { callee, args, ret } => {
                let Some(callee_index) = usize::try_from(callee.0)
                    .ok()
                    .filter(|index| *index < self.program.fns.len())
                else {
                    return Err(self.op(
                        function_id,
                        pc,
                        ProgramDefect::DirectCalleeOutOfRange {
                            callee: *callee,
                            function_count: self.program.fns.len(),
                        },
                    ));
                };
                let callee_frame = &validated.frames[callee_index];
                let callee_contract = &self.contract.functions[callee_index];
                let entries: Vec<_> = callee_frame
                    .entries
                    .iter()
                    .map(|index| {
                        (
                            &callee_contract.frame.regions[*index],
                            callee_frame.regions[*index],
                        )
                    })
                    .collect();
                let result_region = &callee_contract.frame.regions[callee_frame.result];
                let result_bounds = callee_frame.regions[callee_frame.result];
                let target = CallTarget {
                    entries,
                    result: (result_region, result_bounds),
                };
                self.validate_call_shape(function_id, pc, frame, args, *ret, &target)?;
                return Ok(Some(CallSiteFacts::Direct {
                    callee: *callee,
                    result_size: result_bounds.len(),
                }));
            }
            Op::CallIndirect { callee, args, ret } => {
                let call_contract = self.read_callable(function_id, pc, frame, *callee)?;
                let call_index = usize::try_from(call_contract.0).map_err(|_| {
                    self.op(
                        function_id,
                        pc,
                        ProgramDefect::KindMismatch {
                            role: AccessRole::Callee,
                            required: KindRequirement::Callable,
                            allowed: AllowedKinds::new(WordKind::Callable(call_contract)),
                        },
                    )
                })?;
                let call = &self.contract.calls[call_index];
                let validated_call = &validated.calls[call_index];
                let entries: Vec<_> = call
                    .entries
                    .iter()
                    .zip(validated_call.entries.iter().copied())
                    .collect();
                let target = CallTarget {
                    entries,
                    result: (&call.result, validated_call.result),
                };
                self.validate_call_shape(function_id, pc, frame, args, *ret, &target)?;
                let obligation = IndirectCallObligation {
                    function_count: self.program.fns.len(),
                    contract: call_contract,
                };
                return Ok(Some(CallSiteFacts::Indirect {
                    result_size: validated_call.result.len(),
                    obligation,
                }));
            }
            Op::Ret { src, size } => {
                let result_region =
                    &self.contract.functions[function_index].frame.regions[frame.result];
                let result_bounds = frame.regions[frame.result];
                if *src != result_region.offset
                    || usize::try_from(*size).ok() != Some(result_bounds.len())
                {
                    let actual = self.exact_region(
                        function_id,
                        pc,
                        frame,
                        *src,
                        usize::try_from(*size).map_err(|_| {
                            self.access(
                                function_id,
                                pc,
                                AccessRole::ReturnValue,
                                AccessDefect::RangeOverflow {
                                    offset: *src,
                                    size: usize::MAX,
                                },
                            )
                        })?,
                        AccessRole::ReturnValue,
                    )?;
                    return Err(self.op(
                        function_id,
                        pc,
                        ProgramDefect::ReturnRegionMismatch {
                            expected: self.contract.functions[function_index].result,
                            actual: RegionId(actual as u32),
                        },
                    ));
                }
            }
            Op::Await { dst, input } => {
                self.require_scalar_read(
                    function_id,
                    pc,
                    frame,
                    *dst,
                    AccessRole::AwaitDestination,
                )?;
                let input_count = usize::try_from(*input)
                    .ok()
                    .and_then(|input| input.checked_add(1))
                    .ok_or_else(|| {
                        self.op(
                            function_id,
                            pc,
                            ProgramDefect::AwaitInputCountOverflow { input: *input },
                        )
                    })?;
                requirements.await_inputs = requirements.await_inputs.max(input_count);
            }
            Op::CompareValueBytes { dst, a, b } => {
                self.require_scalar_write(function_id, pc, frame, *dst, AccessRole::Destination)?;
                let left_schema =
                    self.read_handle(function_id, pc, frame, *a, AccessRole::CompareLeft)?;
                let right_schema =
                    self.read_handle(function_id, pc, frame, *b, AccessRole::CompareRight)?;
                if left_schema != right_schema {
                    return Err(self.op(
                        function_id,
                        pc,
                        ProgramDefect::CompareSchemaMismatch {
                            left: left_schema,
                            right: right_schema,
                        },
                    ));
                }
                let schema_index = usize::try_from(left_schema.0).map_err(|_| {
                    self.op(
                        function_id,
                        pc,
                        ProgramDefect::SchemaNotByteComparable {
                            schema: left_schema,
                        },
                    )
                })?;
                if !matches!(
                    self.contract.schemas[schema_index].payload,
                    PayloadKind::OpaqueBytes {
                        byte_comparable: true
                    }
                ) {
                    return Err(self.op(
                        function_id,
                        pc,
                        ProgramDefect::SchemaNotByteComparable {
                            schema: left_schema,
                        },
                    ));
                }
            }
            Op::ConstF64 { dst, bits: _ } => {
                self.reject_raw_structural_word(
                    function_id,
                    pc,
                    function_index,
                    frame,
                    *dst,
                    AccessRole::Destination,
                )?;
                self.require_scalar_write(function_id, pc, frame, *dst, AccessRole::Destination)?;
            }
            Op::Trace { id: _ } => {}
            Op::LoadIndexedI64 { .. } => {
                return Err(self.unsupported(function_id, pc, UnsupportedOp::LoadIndexedI64));
            }
            Op::StoreIndexedI64 { .. } => {
                return Err(self.unsupported(function_id, pc, UnsupportedOp::StoreIndexedI64));
            }
            Op::ArrayNew {
                dst,
                status,
                count_slot,
                elem_width,
                elem_schema_ref,
            } => {
                let element = self.array_element(function_id, pc, frame, *dst, *elem_schema_ref)?;
                self.require_status_write(
                    function_id,
                    pc,
                    frame,
                    *status,
                    AccessRole::ArrayStatus,
                )?;
                self.require_scalar_read(
                    function_id,
                    pc,
                    frame,
                    *count_slot,
                    AccessRole::ArrayCount,
                )?;
                self.validate_array_width(function_id, pc, element, *elem_width)?;
            }
            Op::ArrayStore {
                status,
                array,
                index,
                src,
                elem_width,
                elem_schema_ref,
            } => {
                let element =
                    self.array_element(function_id, pc, frame, *array, *elem_schema_ref)?;
                self.require_status_write(
                    function_id,
                    pc,
                    frame,
                    *status,
                    AccessRole::ArrayStatus,
                )?;
                self.require_scalar_read(function_id, pc, frame, *index, AccessRole::ArrayIndex)?;
                let width = self.validate_array_width(function_id, pc, element, *elem_width)?;
                self.require_element_region(
                    function_id,
                    pc,
                    frame,
                    ElementRegion {
                        offset: *src,
                        width,
                        element,
                        role: AccessRole::ArrayElementSource,
                        source: true,
                    },
                )?;
            }
            Op::LoadArray {
                dst,
                status,
                array,
                index,
                elem_width,
                elem_schema_ref,
            } => {
                let element =
                    self.array_element(function_id, pc, frame, *array, *elem_schema_ref)?;
                self.require_status_write(
                    function_id,
                    pc,
                    frame,
                    *status,
                    AccessRole::ArrayStatus,
                )?;
                self.require_scalar_read(function_id, pc, frame, *index, AccessRole::ArrayIndex)?;
                let width = self.validate_array_width(function_id, pc, element, *elem_width)?;
                self.require_element_region(
                    function_id,
                    pc,
                    frame,
                    ElementRegion {
                        offset: *dst,
                        width,
                        element,
                        role: AccessRole::ArrayElementDestination,
                        source: false,
                    },
                )?;
            }
            Op::LoadArrayLen {
                dst,
                status,
                array,
                elem_schema_ref,
            } => {
                self.array_element(function_id, pc, frame, *array, *elem_schema_ref)?;
                self.require_status_write(
                    function_id,
                    pc,
                    frame,
                    *status,
                    AccessRole::ArrayStatus,
                )?;
                self.require_scalar_write(
                    function_id,
                    pc,
                    frame,
                    *dst,
                    AccessRole::ArrayLengthDestination,
                )?;
            }
            Op::ArrayStatusIs {
                dst,
                status,
                expected: _,
            } => {
                self.require_scalar_read(function_id, pc, frame, *dst, AccessRole::Destination)?;
                self.require_status_read(
                    function_id,
                    pc,
                    frame,
                    *status,
                    AccessRole::ArrayStatusSource,
                )?;
            }
            Op::ArrayStoreWord { .. } => {
                return Err(self.unsupported(function_id, pc, UnsupportedOp::ArrayStoreWord));
            }
            Op::LoadArrayWord { .. } => {
                return Err(self.unsupported(function_id, pc, UnsupportedOp::LoadArrayWord));
            }
            Op::HostCall { .. } => {
                return Err(self.unsupported(function_id, pc, UnsupportedOp::HostCall));
            }
            Op::HostCallYield { .. } => {
                return Err(self.unsupported(function_id, pc, UnsupportedOp::HostCallYield));
            }
            Op::OrderedBeginProbe {
                cursor,
                status,
                collection,
                collection_schema_ref,
            } => {
                self.ordered_collection(
                    function_id,
                    pc,
                    frame,
                    *collection,
                    *collection_schema_ref,
                )?;
                self.require_status_write(
                    function_id,
                    pc,
                    frame,
                    *status,
                    AccessRole::OrderedStatus,
                )?;
                self.require_ordered_cursor_region(
                    function_id,
                    pc,
                    function_index,
                    frame,
                    *cursor,
                    AccessRole::OrderedCursorDestination,
                )?;
            }
            Op::OrderedProbeKey {
                cursor,
                present,
                key,
                left,
                right,
                status,
                key_width,
                collection_schema_ref,
            } => {
                let (collection_schema, key_schema) =
                    self.ordered_collection_schemas(function_id, pc, *collection_schema_ref)?;
                self.require_ordered_cursor_region(
                    function_id,
                    pc,
                    function_index,
                    frame,
                    *cursor,
                    AccessRole::OrderedCursorSource,
                )?;
                self.require_scalar_write(
                    function_id,
                    pc,
                    frame,
                    *present,
                    AccessRole::OrderedPresent,
                )?;
                self.require_ordered_payload_region(
                    function_id,
                    pc,
                    frame,
                    *key,
                    *key_width,
                    OrderedPayload {
                        schema: key_schema,
                        part: OrderedPart::Key,
                    },
                )?;
                self.require_handle_write(
                    function_id,
                    pc,
                    frame,
                    *left,
                    collection_schema,
                    AccessRole::OrderedChildHandle,
                )?;
                self.require_handle_write(
                    function_id,
                    pc,
                    frame,
                    *right,
                    collection_schema,
                    AccessRole::OrderedChildHandle,
                )?;
                self.require_status_write(
                    function_id,
                    pc,
                    frame,
                    *status,
                    AccessRole::OrderedStatus,
                )?;
            }
            Op::OrderedProbeValue {
                cursor,
                present,
                value,
                status,
                value_width,
                collection_schema_ref,
            } => {
                let (_, value_schema) =
                    self.ordered_collection_value_schema(function_id, pc, *collection_schema_ref)?;
                self.require_ordered_cursor_region(
                    function_id,
                    pc,
                    function_index,
                    frame,
                    *cursor,
                    AccessRole::OrderedCursorSource,
                )?;
                self.require_scalar_write(
                    function_id,
                    pc,
                    frame,
                    *present,
                    AccessRole::OrderedPresent,
                )?;
                self.require_ordered_payload_region(
                    function_id,
                    pc,
                    frame,
                    *value,
                    *value_width,
                    OrderedPayload {
                        schema: value_schema,
                        part: OrderedPart::Value,
                    },
                )?;
                self.require_status_write(
                    function_id,
                    pc,
                    frame,
                    *status,
                    AccessRole::OrderedStatus,
                )?;
            }
        }
        Ok(None)
    }

    fn structural_region(
        &self,
        function: FnId,
        pc: usize,
        function_index: usize,
        region: RegionId,
    ) -> Result<&FrameRegion, ProgramError> {
        let regions = &self.contract.functions[function_index].frame.regions;
        let Some(region_contract) = usize::try_from(region.0)
            .ok()
            .and_then(|index| regions.get(index))
        else {
            return Err(self.op(
                function,
                pc,
                ProgramDefect::StructuralRegionOutOfRange {
                    region,
                    region_count: regions.len(),
                },
            ));
        };
        if region_contract.value_shape.is_none() {
            return Err(self.op(
                function,
                pc,
                ProgramDefect::StructuralRegionRequiresShape { region },
            ));
        }
        Ok(region_contract)
    }

    fn product_region(
        &self,
        function: FnId,
        pc: usize,
        function_index: usize,
        region: RegionId,
    ) -> Result<(ValueShapeRef, &[ValueFieldUse]), ProgramError> {
        let region_contract = self.structural_region(function, pc, function_index, region)?;
        let value_shape = region_contract.value_shape.expect("checked above");
        match &self.contract.value_shapes[value_shape.0 as usize].kind {
            ValueShapeKind::Product { fields } => Ok((value_shape, fields)),
            ValueShapeKind::Enum { .. } => Err(self.op(
                function,
                pc,
                ProgramDefect::StructuralKindMismatch {
                    region,
                    value_shape,
                    expected: StructuralKind::Product,
                },
            )),
        }
    }

    fn enum_region(
        &self,
        function: FnId,
        pc: usize,
        function_index: usize,
        region: RegionId,
    ) -> Result<(ValueShapeRef, &[ValueVariant]), ProgramError> {
        let region_contract = self.structural_region(function, pc, function_index, region)?;
        let value_shape = region_contract.value_shape.expect("checked above");
        match &self.contract.value_shapes[value_shape.0 as usize].kind {
            ValueShapeKind::Enum { variants, .. } => Ok((value_shape, variants)),
            ValueShapeKind::Product { .. } => Err(self.op(
                function,
                pc,
                ProgramDefect::StructuralKindMismatch {
                    region,
                    value_shape,
                    expected: StructuralKind::Enum,
                },
            )),
        }
    }

    fn enum_variant<'a>(
        &self,
        function: FnId,
        pc: usize,
        value_shape: ValueShapeRef,
        variants: &'a [ValueVariant],
        variant: u32,
    ) -> Result<&'a ValueVariant, ProgramError> {
        variants.get(variant as usize).ok_or_else(|| {
            self.op(
                function,
                pc,
                ProgramDefect::EnumVariantOutOfRange {
                    value_shape,
                    variant,
                    variant_count: variants.len(),
                },
            )
        })
    }

    fn structural_field<'a>(
        &self,
        function: FnId,
        pc: usize,
        value_shape: ValueShapeRef,
        variant: Option<u32>,
        fields: &'a [ValueFieldUse],
        field: u32,
    ) -> Result<&'a ValueFieldUse, ProgramError> {
        fields.get(field as usize).ok_or_else(|| {
            self.op(
                function,
                pc,
                ProgramDefect::StructuralFieldOutOfRange {
                    value_shape,
                    variant,
                    field,
                    field_count: fields.len(),
                },
            )
        })
    }

    fn validate_structural_fields(
        &self,
        context: StructuralFieldContext,
        declared: &[ValueFieldUse],
        sources: &[StructuralFieldSource],
    ) -> Result<(), ProgramError> {
        if declared.len() != sources.len() {
            return Err(self.op(
                context.function,
                context.pc,
                ProgramDefect::StructuralFieldCount {
                    value_shape: context.value_shape,
                    variant: context.variant,
                    expected: declared.len(),
                    actual: sources.len(),
                },
            ));
        }
        let mut seen = vec![false; declared.len()];
        for source in sources {
            let field = self.structural_field(
                context.function,
                context.pc,
                context.value_shape,
                context.variant,
                declared,
                source.field,
            )?;
            if core::mem::replace(&mut seen[source.field as usize], true) {
                return Err(self.op(
                    context.function,
                    context.pc,
                    ProgramDefect::DuplicateStructuralField {
                        value_shape: context.value_shape,
                        variant: context.variant,
                        field: source.field,
                    },
                ));
            }
            self.validate_structural_source(context, source.field, source.source, field)?;
        }
        Ok(())
    }

    fn validate_structural_source(
        &self,
        context: StructuralFieldContext,
        field_index: u32,
        source: RegionId,
        field: &ValueFieldUse,
    ) -> Result<(), ProgramError> {
        let regions = &self.contract.functions[context.function_index]
            .frame
            .regions;
        let Some(actual) = regions.get(source.0 as usize) else {
            return Err(self.op(
                context.function,
                context.pc,
                ProgramDefect::StructuralRegionOutOfRange {
                    region: source,
                    region_count: regions.len(),
                },
            ));
        };
        if actual.shape != field.shape || actual.value_shape != field.value_shape {
            return Err(self.op(
                context.function,
                context.pc,
                ProgramDefect::StructuralFieldSourceMismatch {
                    value_shape: context.value_shape,
                    variant: context.variant,
                    field: field_index,
                    source,
                    expected_shape: field.shape.clone(),
                    actual_shape: actual.shape.clone(),
                    expected_value_shape: field.value_shape,
                    actual_value_shape: actual.value_shape,
                },
            ));
        }
        Ok(())
    }

    fn scalar_region(
        &self,
        function: FnId,
        pc: usize,
        function_index: usize,
        region: RegionId,
    ) -> Result<(), ProgramError> {
        let regions = &self.contract.functions[function_index].frame.regions;
        let Some(actual) = regions.get(region.0 as usize) else {
            return Err(self.op(
                function,
                pc,
                ProgramDefect::StructuralRegionOutOfRange {
                    region,
                    region_count: regions.len(),
                },
            ));
        };
        if actual.value_shape.is_some() || actual.shape != RegionShape::word(WordKind::Scalar) {
            return Err(self.op(
                function,
                pc,
                ProgramDefect::KindMismatch {
                    role: AccessRole::Destination,
                    required: KindRequirement::Scalar,
                    allowed: actual
                        .shape
                        .words
                        .first()
                        .cloned()
                        .unwrap_or_else(|| AllowedKinds::new(WordKind::Opaque)),
                },
            ));
        }
        Ok(())
    }

    fn reject_raw_structural_word(
        &self,
        function: FnId,
        pc: usize,
        function_index: usize,
        frame: &ValidatedFrame,
        offset: u32,
        role: AccessRole,
    ) -> Result<(), ProgramError> {
        let (region_index, _) = self.word_region_index(function, pc, frame, offset, role)?;
        let region = &self.contract.functions[function_index].frame.regions[region_index];
        if let Some(value_shape) = region.value_shape {
            return Err(self.op(
                function,
                pc,
                ProgramDefect::RawStructuralWordAccess {
                    role,
                    region: RegionId(region_index as u32),
                    value_shape,
                },
            ));
        }
        Ok(())
    }

    fn validate_const(
        &self,
        function: FnId,
        pc: usize,
        value: i64,
        kinds: &AllowedKinds,
    ) -> Result<(), ProgramError> {
        // A scalar write does not establish that another member of a union is
        // present; later interpreted reads still require an exact kind.
        if kinds.contains(WordKind::Scalar) {
            return Ok(());
        }
        // Opaque admits a raw word write, but remains directional: it cannot
        // turn a union into a callable or handle read.
        if kinds.contains(WordKind::Opaque) {
            return Ok(());
        }
        let [WordKind::Callable(expected_contract)] = kinds.as_slice() else {
            return Err(self.op(
                function,
                pc,
                ProgramDefect::KindMismatch {
                    role: AccessRole::Destination,
                    required: KindRequirement::Status,
                    allowed: kinds.clone(),
                },
            ));
        };
        let Some(target_index) = u32::try_from(value)
            .ok()
            .and_then(|target| usize::try_from(target).ok())
            .filter(|target| *target < self.program.fns.len())
        else {
            return Err(self.op(
                function,
                pc,
                ProgramDefect::ConstantCallableTargetOutOfRange {
                    value,
                    function_count: self.program.fns.len(),
                },
            ));
        };
        let target = FnId(target_index as u32);
        let declared = self.contract.functions[target_index].call_contract;
        if declared != Some(*expected_contract) {
            return Err(self.op(
                function,
                pc,
                ProgramDefect::ConstantCallableContractMismatch {
                    target,
                    declared,
                    allowed: vec![*expected_contract],
                },
            ));
        }
        Ok(())
    }

    fn validate_call_shape(
        &self,
        function: FnId,
        pc: usize,
        caller: &ValidatedFrame,
        args: &[ArgCopy],
        ret: u32,
        target: &CallTarget<'_>,
    ) -> Result<(), ProgramError> {
        if args.len() != target.entries.len() {
            return Err(self.op(
                function,
                pc,
                ProgramDefect::CallArgumentCount {
                    expected: target.entries.len(),
                    actual: args.len(),
                },
            ));
        }
        for (index, (argument, (destination, destination_bounds))) in
            args.iter().zip(&target.entries).enumerate()
        {
            if argument.dst != destination.offset
                || usize::try_from(argument.size).ok() != Some(destination_bounds.len())
            {
                return Err(self.op(
                    function,
                    pc,
                    ProgramDefect::CallArgumentDestination {
                        index,
                        expected_offset: destination.offset,
                        expected_size: destination_bounds.len(),
                        actual_offset: argument.dst,
                        actual_size: argument.size,
                    },
                ));
            }
            let source_index = self.exact_region(
                function,
                pc,
                caller,
                argument.src,
                destination_bounds.len(),
                AccessRole::ArgumentSource { index },
            )?;
            let source = &self.contract.functions[function.0 as usize].frame.regions[source_index];
            self.validate_structural_transfer(function, pc, source, destination)?;
            if !shapes_assignable(&source.shape, &destination.shape) {
                return Err(self.op(
                    function,
                    pc,
                    ProgramDefect::CallArgumentKinds {
                        index,
                        source: source.shape.clone(),
                        destination: destination.shape.clone(),
                    },
                ));
            }
        }
        let result_index = self.exact_region(
            function,
            pc,
            caller,
            ret,
            target.result.1.len(),
            AccessRole::CallResult,
        )?;
        let destination = &self.contract.functions[function.0 as usize].frame.regions[result_index];
        self.validate_structural_transfer(function, pc, target.result.0, destination)?;
        if !shapes_assignable(&target.result.0.shape, &destination.shape) {
            return Err(self.op(
                function,
                pc,
                ProgramDefect::CallResultKinds {
                    source: target.result.0.shape.clone(),
                    destination: destination.shape.clone(),
                },
            ));
        }
        Ok(())
    }

    fn validate_structural_transfer(
        &self,
        function: FnId,
        pc: usize,
        source: &FrameRegion,
        destination: &FrameRegion,
    ) -> Result<(), ProgramError> {
        self.validate_structural_refs(function, pc, source.value_shape, destination.value_shape)
    }

    fn validate_structural_refs(
        &self,
        function: FnId,
        pc: usize,
        source: Option<ValueShapeRef>,
        destination: Option<ValueShapeRef>,
    ) -> Result<(), ProgramError> {
        if (source.is_some() || destination.is_some()) && source != destination {
            return Err(self.op(
                function,
                pc,
                ProgramDefect::StructuralTransferMismatch {
                    source,
                    destination,
                },
            ));
        }
        Ok(())
    }

    fn validate_jump(&self, function: FnId, pc: usize, target: u32) -> Result<(), ProgramError> {
        let code_len = self.program.fns[function.0 as usize].code.len();
        let in_range = usize::try_from(target).is_ok_and(|target| target < code_len);
        if !in_range {
            return Err(self.op(
                function,
                pc,
                ProgramDefect::JumpTargetOutOfRange { target, code_len },
            ));
        }
        Ok(())
    }

    fn read_callable(
        &self,
        function: FnId,
        pc: usize,
        frame: &ValidatedFrame,
        offset: u32,
    ) -> Result<CallContractId, ProgramError> {
        let kinds = self.word_kinds(function, pc, frame, offset, AccessRole::Callee)?;
        let [WordKind::Callable(contract)] = kinds.as_slice() else {
            return Err(self.op(
                function,
                pc,
                ProgramDefect::KindMismatch {
                    role: AccessRole::Callee,
                    required: KindRequirement::Callable,
                    allowed: kinds.clone(),
                },
            ));
        };
        Ok(*contract)
    }

    fn read_handle(
        &self,
        function: FnId,
        pc: usize,
        frame: &ValidatedFrame,
        offset: u32,
        role: AccessRole,
    ) -> Result<SchemaRef, ProgramError> {
        let kinds = self.word_kinds(function, pc, frame, offset, role)?;
        let [WordKind::Handle(schema)] = kinds.as_slice() else {
            return Err(self.op(
                function,
                pc,
                ProgramDefect::KindMismatch {
                    role,
                    required: KindRequirement::Handle,
                    allowed: kinds.clone(),
                },
            ));
        };
        Ok(*schema)
    }

    fn require_scalar_read(
        &self,
        function: FnId,
        pc: usize,
        frame: &ValidatedFrame,
        offset: u32,
        role: AccessRole,
    ) -> Result<(), ProgramError> {
        let kinds = self.word_kinds(function, pc, frame, offset, role)?;
        if !kinds.is_exactly(WordKind::Scalar) {
            return Err(self.op(
                function,
                pc,
                ProgramDefect::KindMismatch {
                    role,
                    required: KindRequirement::Scalar,
                    allowed: kinds.clone(),
                },
            ));
        }
        Ok(())
    }

    fn require_scalar_write(
        &self,
        function: FnId,
        pc: usize,
        frame: &ValidatedFrame,
        offset: u32,
        role: AccessRole,
    ) -> Result<(), ProgramError> {
        let kinds = self.word_kinds(function, pc, frame, offset, role)?;
        if !kinds.contains(WordKind::Scalar) {
            return Err(self.op(
                function,
                pc,
                ProgramDefect::KindMismatch {
                    role,
                    required: KindRequirement::Scalar,
                    allowed: kinds.clone(),
                },
            ));
        }
        Ok(())
    }

    fn require_status_write(
        &self,
        function: FnId,
        pc: usize,
        frame: &ValidatedFrame,
        offset: u32,
        role: AccessRole,
    ) -> Result<(), ProgramError> {
        let kinds = self.word_kinds(function, pc, frame, offset, role)?;
        if !kinds.contains(WordKind::Status) {
            return Err(self.op(
                function,
                pc,
                ProgramDefect::KindMismatch {
                    role,
                    required: KindRequirement::Status,
                    allowed: kinds.clone(),
                },
            ));
        }
        Ok(())
    }

    fn require_status_read(
        &self,
        function: FnId,
        pc: usize,
        frame: &ValidatedFrame,
        offset: u32,
        role: AccessRole,
    ) -> Result<(), ProgramError> {
        let kinds = self.word_kinds(function, pc, frame, offset, role)?;
        if !kinds.is_exactly(WordKind::Status) {
            return Err(self.op(
                function,
                pc,
                ProgramDefect::KindMismatch {
                    role,
                    required: KindRequirement::Status,
                    allowed: kinds.clone(),
                },
            ));
        }
        Ok(())
    }

    fn array_element(
        &self,
        function: FnId,
        pc: usize,
        frame: &ValidatedFrame,
        array: u32,
        witness: i64,
    ) -> Result<SchemaRef, ProgramError> {
        let array_schema = self.read_handle(function, pc, frame, array, AccessRole::ArrayHandle)?;
        let array_index = usize::try_from(array_schema.0).map_err(|_| {
            self.op(
                function,
                pc,
                ProgramDefect::SchemaReferenceOutOfRange {
                    site: ReferenceSite::ArrayElementWitness,
                    schema: array_schema,
                    schema_count: self.contract.schemas.len(),
                },
            )
        })?;
        let PayloadKind::DenseArray { element: expected } =
            self.contract.schemas[array_index].payload
        else {
            return Err(self.op(
                function,
                pc,
                ProgramDefect::ArraySchemaNotDense {
                    schema: array_schema,
                },
            ));
        };
        let witness_index = usize::try_from(witness)
            .ok()
            .filter(|index| *index < self.contract.schemas.len())
            .ok_or_else(|| {
                self.op(
                    function,
                    pc,
                    ProgramDefect::ArrayElementWitnessOutOfRange {
                        witness,
                        schema_count: self.contract.schemas.len(),
                    },
                )
            })?;
        let witness = SchemaRef(u32::try_from(witness_index).map_err(|_| {
            self.op(
                function,
                pc,
                ProgramDefect::ArrayElementWitnessOutOfRange {
                    witness,
                    schema_count: self.contract.schemas.len(),
                },
            )
        })?);
        if witness != expected {
            return Err(self.op(
                function,
                pc,
                ProgramDefect::ArrayElementSchemaMismatch {
                    array: array_schema,
                    expected,
                    actual: witness,
                },
            ));
        }
        Ok(witness)
    }

    fn ordered_collection(
        &self,
        function: FnId,
        pc: usize,
        frame: &ValidatedFrame,
        collection: u32,
        witness: i64,
    ) -> Result<SchemaRef, ProgramError> {
        let handle_schema = self.read_handle(
            function,
            pc,
            frame,
            collection,
            AccessRole::OrderedCollectionHandle,
        )?;
        let witness = usize::try_from(witness)
            .ok()
            .filter(|index| *index < self.contract.schemas.len())
            .and_then(|index| u32::try_from(index).ok().map(SchemaRef))
            .ok_or_else(|| {
                self.op(
                    function,
                    pc,
                    ProgramDefect::OrderedCollectionWitnessOutOfRange {
                        witness,
                        schema_count: self.contract.schemas.len(),
                    },
                )
            })?;
        if witness != handle_schema {
            return Err(self.op(
                function,
                pc,
                ProgramDefect::OrderedCollectionSchemaMismatch {
                    handle: handle_schema,
                    witness,
                },
            ));
        }
        if !matches!(
            self.contract.schemas[witness.0 as usize].payload,
            PayloadKind::OrderedCollection(_)
        ) {
            return Err(self.op(
                function,
                pc,
                ProgramDefect::OrderedCollectionSchemaNotCollection { schema: witness },
            ));
        }
        Ok(witness)
    }

    fn ordered_collection_schemas(
        &self,
        function: FnId,
        pc: usize,
        witness: i64,
    ) -> Result<(SchemaRef, SchemaRef), ProgramError> {
        let collection = usize::try_from(witness)
            .ok()
            .filter(|index| *index < self.contract.schemas.len())
            .and_then(|index| u32::try_from(index).ok().map(SchemaRef))
            .ok_or_else(|| {
                self.op(
                    function,
                    pc,
                    ProgramDefect::OrderedCollectionWitnessOutOfRange {
                        witness,
                        schema_count: self.contract.schemas.len(),
                    },
                )
            })?;
        let PayloadKind::OrderedCollection(contract) =
            &self.contract.schemas[collection.0 as usize].payload
        else {
            return Err(self.op(
                function,
                pc,
                ProgramDefect::OrderedCollectionSchemaNotCollection { schema: collection },
            ));
        };
        Ok((collection, contract.key))
    }

    /// The full collection schema table refs for a value projection: the
    /// collection schema and its value schema, rejecting a Set (no value).
    fn ordered_collection_value_schema(
        &self,
        function: FnId,
        pc: usize,
        witness: i64,
    ) -> Result<(SchemaRef, SchemaRef), ProgramError> {
        let collection = usize::try_from(witness)
            .ok()
            .filter(|index| *index < self.contract.schemas.len())
            .and_then(|index| u32::try_from(index).ok().map(SchemaRef))
            .ok_or_else(|| {
                self.op(
                    function,
                    pc,
                    ProgramDefect::OrderedCollectionWitnessOutOfRange {
                        witness,
                        schema_count: self.contract.schemas.len(),
                    },
                )
            })?;
        let PayloadKind::OrderedCollection(contract) =
            &self.contract.schemas[collection.0 as usize].payload
        else {
            return Err(self.op(
                function,
                pc,
                ProgramDefect::OrderedCollectionSchemaNotCollection { schema: collection },
            ));
        };
        let Some(value) = contract.value else {
            return Err(self.op(
                function,
                pc,
                ProgramDefect::OrderedValueOnSet { schema: collection },
            ));
        };
        Ok((collection, value))
    }

    /// A key or value destination must be a whole region whose shape and
    /// structural value shape are exactly the named schema, at its exact width.
    fn require_ordered_payload_region(
        &self,
        function: FnId,
        pc: usize,
        frame: &ValidatedFrame,
        offset: u32,
        width: u32,
        payload: OrderedPayload,
    ) -> Result<(), ProgramError> {
        let OrderedPayload { schema, part } = payload;
        let expected = &self.contract.schemas[schema.0 as usize];
        let expected_len = expected.inline.checked_byte_len().ok_or_else(|| {
            self.op(
                function,
                pc,
                ProgramDefect::ShapeSizeOverflow {
                    owner: ShapeOwner::SchemaInline(schema),
                },
            )
        })?;
        if expected_len == 0 || usize::try_from(width).ok() != Some(expected_len) {
            return Err(self.op(function, pc, part.width_defect(schema, expected_len, width)));
        }
        let region_index =
            self.exact_region(function, pc, frame, offset, expected_len, part.role())?;
        let region = &self.contract.functions[function.0 as usize].frame.regions[region_index];
        if region.shape != expected.inline || region.value_shape != expected.value_shape {
            return Err(self.op(
                function,
                pc,
                part.shape_defect(schema, expected.inline.clone(), region.shape.clone()),
            ));
        }
        Ok(())
    }

    /// A child collection handle destination must be exactly `Handle(schema)`.
    fn require_handle_write(
        &self,
        function: FnId,
        pc: usize,
        frame: &ValidatedFrame,
        offset: u32,
        schema: SchemaRef,
        role: AccessRole,
    ) -> Result<(), ProgramError> {
        let kinds = self.word_kinds(function, pc, frame, offset, role)?;
        if !kinds.is_exactly(WordKind::Handle(schema)) {
            return Err(self.op(
                function,
                pc,
                ProgramDefect::KindMismatch {
                    role,
                    required: KindRequirement::Handle,
                    allowed: kinds.clone(),
                },
            ));
        }
        Ok(())
    }

    /// A cursor region must name a whole two-word region that is exactly
    /// internal opaque cursor words and carries no structural value shape.
    fn require_ordered_cursor_region(
        &self,
        function: FnId,
        pc: usize,
        function_index: usize,
        frame: &ValidatedFrame,
        offset: u32,
        role: AccessRole,
    ) -> Result<(), ProgramError> {
        let region_index = self.exact_region(function, pc, frame, offset, WORD_SIZE * 2, role)?;
        let region = &self.contract.functions[function_index].frame.regions[region_index];
        let two_opaque_words = region.value_shape.is_none()
            && region.shape.words.len() == 2
            && region
                .shape
                .words
                .iter()
                .all(|word| word.is_exactly(WordKind::Opaque));
        if !two_opaque_words {
            return Err(self.op(
                function,
                pc,
                ProgramDefect::OrderedCursorRegionShape {
                    region: RegionId(region_index as u32),
                    shape: region.shape.clone(),
                },
            ));
        }
        Ok(())
    }

    fn validate_array_width(
        &self,
        function: FnId,
        pc: usize,
        element: SchemaRef,
        actual: u32,
    ) -> Result<usize, ProgramError> {
        let expected = self.contract.schemas[element.0 as usize]
            .inline
            .checked_byte_len()
            .ok_or_else(|| {
                self.op(
                    function,
                    pc,
                    ProgramDefect::ShapeSizeOverflow {
                        owner: ShapeOwner::SchemaInline(element),
                    },
                )
            })?;
        if expected == 0 || usize::try_from(actual).ok() != Some(expected) {
            return Err(self.op(
                function,
                pc,
                ProgramDefect::ArrayElementWidth {
                    schema: element,
                    expected,
                    actual,
                },
            ));
        }
        Ok(expected)
    }

    fn require_element_region(
        &self,
        function: FnId,
        pc: usize,
        frame: &ValidatedFrame,
        access: ElementRegion,
    ) -> Result<(), ProgramError> {
        let region = self.exact_region(
            function,
            pc,
            frame,
            access.offset,
            access.width,
            access.role,
        )?;
        let actual_region = &self.contract.functions[function.0 as usize].frame.regions[region];
        let expected_schema = &self.contract.schemas[access.element.0 as usize];
        let actual = &actual_region.shape;
        let expected = &expected_schema.inline;
        if access.source {
            self.validate_structural_refs(
                function,
                pc,
                actual_region.value_shape,
                expected_schema.value_shape,
            )?;
        } else {
            self.validate_structural_refs(
                function,
                pc,
                expected_schema.value_shape,
                actual_region.value_shape,
            )?;
        }
        let valid = if access.source {
            shapes_assignable(actual, expected)
        } else {
            shapes_assignable(expected, actual)
        };
        if !valid {
            return Err(self.op(
                function,
                pc,
                ProgramDefect::ArrayElementShapes {
                    source: if access.source {
                        actual.clone()
                    } else {
                        expected.clone()
                    },
                    destination: if access.source {
                        expected.clone()
                    } else {
                        actual.clone()
                    },
                },
            ));
        }
        Ok(())
    }

    fn word_kinds(
        &self,
        function: FnId,
        pc: usize,
        frame: &ValidatedFrame,
        offset: u32,
        role: AccessRole,
    ) -> Result<&AllowedKinds, ProgramError> {
        let (region_index, word) = self.word_region_index(function, pc, frame, offset, role)?;
        Ok(
            &self.contract.functions[function.0 as usize].frame.regions[region_index]
                .shape
                .words[word],
        )
    }

    fn word_region_index(
        &self,
        function: FnId,
        pc: usize,
        frame: &ValidatedFrame,
        offset: u32,
        role: AccessRole,
    ) -> Result<(usize, usize), ProgramError> {
        if !offset.is_multiple_of(WORD_SIZE_U32) {
            return Err(self.access(
                function,
                pc,
                role,
                AccessDefect::OffsetNotWordAligned { offset },
            ));
        }
        let start = usize::try_from(offset).map_err(|_| {
            self.access(
                function,
                pc,
                role,
                AccessDefect::RangeOverflow {
                    offset,
                    size: WORD_SIZE,
                },
            )
        })?;
        let end = start.checked_add(WORD_SIZE).ok_or_else(|| {
            self.access(
                function,
                pc,
                role,
                AccessDefect::RangeOverflow {
                    offset,
                    size: WORD_SIZE,
                },
            )
        })?;
        let frame_size = self.program.fns[function.0 as usize].frame.size;
        if end > frame_size {
            return Err(self.access(
                function,
                pc,
                role,
                AccessDefect::OutOfBounds {
                    offset,
                    size: WORD_SIZE,
                    frame_size,
                },
            ));
        }
        for (region_index, bounds) in frame.regions.iter().copied().enumerate() {
            if bounds.start <= start && end <= bounds.end {
                let word = (start - bounds.start) / WORD_SIZE;
                return Ok((region_index, word));
            }
        }
        Err(self.access(function, pc, role, AccessDefect::UndeclaredWord { offset }))
    }

    fn exact_region(
        &self,
        function: FnId,
        pc: usize,
        frame: &ValidatedFrame,
        offset: u32,
        size: usize,
        role: AccessRole,
    ) -> Result<usize, ProgramError> {
        if !offset.is_multiple_of(WORD_SIZE_U32) {
            return Err(self.access(
                function,
                pc,
                role,
                AccessDefect::OffsetNotWordAligned { offset },
            ));
        }
        if !size.is_multiple_of(WORD_SIZE) {
            return Err(self.access(
                function,
                pc,
                role,
                AccessDefect::SizeNotWordAligned { size },
            ));
        }
        let start = usize::try_from(offset).map_err(|_| {
            self.access(
                function,
                pc,
                role,
                AccessDefect::RangeOverflow { offset, size },
            )
        })?;
        let end = start.checked_add(size).ok_or_else(|| {
            self.access(
                function,
                pc,
                role,
                AccessDefect::RangeOverflow { offset, size },
            )
        })?;
        let frame_size = self.program.fns[function.0 as usize].frame.size;
        if end > frame_size {
            return Err(self.access(
                function,
                pc,
                role,
                AccessDefect::OutOfBounds {
                    offset,
                    size,
                    frame_size,
                },
            ));
        }
        frame
            .regions
            .iter()
            .position(|bounds| bounds.start == start && bounds.end == end)
            .ok_or_else(|| {
                self.access(
                    function,
                    pc,
                    role,
                    AccessDefect::UndeclaredRegion { offset, size },
                )
            })
    }

    fn validate_control_flow(&self, facts: &mut ProgramFacts) -> Result<(), ProgramError> {
        for (function_index, function) in self.program.fns.iter().enumerate() {
            let function_id = FnId(function_index as u32);
            let mut pending = vec![0usize];
            while let Some(pc) = pending.pop() {
                if pc == function.code.len() {
                    return Err(self.op(function_id, pc, ProgramDefect::ReachableFallthrough));
                }
                if facts.functions[function_index].pcs[pc].reachable {
                    continue;
                }
                facts.functions[function_index].pcs[pc].reachable = true;
                match &function.code[pc] {
                    Op::Ret { .. } => {}
                    Op::Jump { target } => pending.push(*target as usize),
                    Op::JumpIfZero { target, .. } => {
                        pending.push(*target as usize);
                        pending.push(pc + 1);
                    }
                    Op::ConstI64 { .. }
                    | Op::ProductConstruct { .. }
                    | Op::ProductProject { .. }
                    | Op::CopyValue { .. }
                    | Op::EnumConstruct { .. }
                    | Op::EnumIsVariant { .. }
                    | Op::EnumProjectChecked { .. }
                    | Op::AddI64 { .. }
                    | Op::SubI64 { .. }
                    | Op::MulI64 { .. }
                    | Op::DivI64 { .. }
                    | Op::CopyI64 { .. }
                    | Op::EqI64 { .. }
                    | Op::NeI64 { .. }
                    | Op::LtI64 { .. }
                    | Op::LeI64 { .. }
                    | Op::GtI64 { .. }
                    | Op::GeI64 { .. }
                    | Op::Call { .. }
                    | Op::CallIndirect { .. }
                    | Op::Await { .. }
                    | Op::CompareValueBytes { .. }
                    | Op::ConstF64 { .. }
                    | Op::AddF64 { .. }
                    | Op::MulF64 { .. }
                    | Op::ArrayNew { .. }
                    | Op::ArrayStore { .. }
                    | Op::LoadArray { .. }
                    | Op::LoadArrayLen { .. }
                    | Op::ArrayStatusIs { .. }
                    | Op::OrderedBeginProbe { .. }
                    | Op::OrderedProbeKey { .. }
                    | Op::OrderedProbeValue { .. }
                    | Op::Trace { .. } => pending.push(pc + 1),
                    Op::ArrayStoreWord { .. } => {
                        return Err(self.unsupported(
                            function_id,
                            pc,
                            UnsupportedOp::ArrayStoreWord,
                        ));
                    }
                    Op::LoadIndexedI64 { .. } => {
                        return Err(self.unsupported(
                            function_id,
                            pc,
                            UnsupportedOp::LoadIndexedI64,
                        ));
                    }
                    Op::StoreIndexedI64 { .. } => {
                        return Err(self.unsupported(
                            function_id,
                            pc,
                            UnsupportedOp::StoreIndexedI64,
                        ));
                    }
                    Op::LoadArrayWord { .. } => {
                        return Err(self.unsupported(
                            function_id,
                            pc,
                            UnsupportedOp::LoadArrayWord,
                        ));
                    }
                    Op::HostCall { .. } => {
                        return Err(self.unsupported(function_id, pc, UnsupportedOp::HostCall));
                    }
                    Op::HostCallYield { .. } => {
                        return Err(self.unsupported(
                            function_id,
                            pc,
                            UnsupportedOp::HostCallYield,
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    fn global(&self, defect: ProgramDefect) -> ProgramError {
        ProgramError {
            function: None,
            pc: None,
            defect,
        }
    }

    fn function(&self, function: FnId, defect: ProgramDefect) -> ProgramError {
        ProgramError {
            function: Some(function),
            pc: None,
            defect,
        }
    }

    fn op(&self, function: FnId, pc: usize, defect: ProgramDefect) -> ProgramError {
        ProgramError {
            function: Some(function),
            pc: Some(pc),
            defect,
        }
    }

    fn access(
        &self,
        function: FnId,
        pc: usize,
        role: AccessRole,
        defect: AccessDefect,
    ) -> ProgramError {
        self.op(function, pc, ProgramDefect::Access { role, defect })
    }

    fn unsupported(&self, function: FnId, pc: usize, op: UnsupportedOp) -> ProgramError {
        self.op(function, pc, ProgramDefect::UnsupportedOp { op })
    }

    fn validate_table_len(
        &self,
        table: ProgramTable,
        len: usize,
        function: Option<FnId>,
    ) -> Result<(), ProgramError> {
        if len != 0 && u32::try_from(len - 1).is_err() {
            return Err(ProgramError {
                function,
                pc: None,
                defect: ProgramDefect::TableTooLarge { table, len },
            });
        }
        Ok(())
    }
}

fn shapes_assignable(source: &RegionShape, destination: &RegionShape) -> bool {
    source.words.len() == destination.words.len()
        && source
            .words
            .iter()
            .zip(&destination.words)
            .all(|(source, destination)| source.is_subset_of(destination))
}

/// Whether any word of a region admits the internal opaque cursor kind. Opaque
/// words are confined to internal cursor destinations; they may never appear at
/// a function/call entry or result, where they could be published or aliased.
fn shape_has_opaque(shape: &RegionShape) -> bool {
    shape
        .words
        .iter()
        .any(|word| word.contains(WordKind::Opaque))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::task::{Fn, StructuralFieldSource};

    struct ValidCase {
        name: &'static str,
        program: Program,
        contract: ProgramContract,
        requirements: DriveRequirements,
        call: Option<(FnId, usize, CallSiteFacts)>,
    }

    struct InvalidCase {
        name: &'static str,
        program: Program,
        contract: ProgramContract,
        expected: ProgramError,
    }

    fn layout(words: usize) -> Layout {
        Layout {
            size: words * WORD_SIZE,
            align: WORD_SIZE,
        }
    }

    fn kinds(kind: WordKind) -> AllowedKinds {
        AllowedKinds::new(kind)
    }

    fn region(offset: u32, shape: RegionShape) -> FrameRegion {
        FrameRegion::new(offset, shape)
    }

    fn word_region(offset: u32, kind: WordKind) -> FrameRegion {
        region(offset, RegionShape::word(kind))
    }

    fn structural_region(
        offset: u32,
        shape: RegionShape,
        value_shape: ValueShapeRef,
    ) -> FrameRegion {
        region(offset, shape).with_value_shape(value_shape)
    }

    fn schema(inline: RegionShape, payload: PayloadKind) -> SchemaContract {
        SchemaContract {
            inline,
            value_shape: None,
            payload,
        }
    }

    fn structural_schema(
        inline: RegionShape,
        value_shape: ValueShapeRef,
        payload: PayloadKind,
    ) -> SchemaContract {
        SchemaContract {
            inline,
            value_shape: Some(value_shape),
            payload,
        }
    }

    fn field(offset: u32, shape: RegionShape) -> ValueFieldUse {
        ValueFieldUse::new(offset, shape)
    }

    fn structural_field(
        offset: u32,
        shape: RegionShape,
        value_shape: ValueShapeRef,
    ) -> ValueFieldUse {
        ValueFieldUse::new(offset, shape).with_value_shape(value_shape)
    }

    fn selector(offset: u32, shape: RegionShape) -> ValueSelector {
        ValueSelector { offset, shape }
    }

    fn variant(fields: Vec<ValueFieldUse>) -> ValueVariant {
        ValueVariant { fields }
    }

    fn function(words: usize, code: Vec<Op>) -> Fn {
        Fn {
            frame: layout(words),
            code,
        }
    }

    fn function_contract(
        words: usize,
        regions: Vec<FrameRegion>,
        entries: &[u32],
        result: u32,
        call_contract: Option<u32>,
    ) -> FunctionContract {
        FunctionContract {
            frame: FrameContract {
                layout: layout(words),
                regions,
            },
            entries: entries.iter().copied().map(RegionId).collect(),
            result: RegionId(result),
            call_contract: call_contract.map(CallContractId),
        }
    }

    fn scalar_program() -> (Program, ProgramContract) {
        let code = vec![
            Op::ConstI64 { dst: 0, value: 7 },
            Op::ConstI64 { dst: 8, value: 3 },
            Op::AddI64 {
                dst: 16,
                a: 0,
                b: 8,
            },
            Op::SubI64 {
                dst: 16,
                a: 0,
                b: 8,
            },
            Op::MulI64 {
                dst: 16,
                a: 0,
                b: 8,
            },
            Op::DivI64 {
                dst: 16,
                a: 0,
                b: 8,
            },
            Op::CopyI64 { dst: 0, src: 16 },
            Op::EqI64 {
                dst: 16,
                a: 0,
                b: 8,
            },
            Op::NeI64 {
                dst: 16,
                a: 0,
                b: 8,
            },
            Op::LtI64 {
                dst: 16,
                a: 0,
                b: 8,
            },
            Op::LeI64 {
                dst: 16,
                a: 0,
                b: 8,
            },
            Op::GtI64 {
                dst: 16,
                a: 0,
                b: 8,
            },
            Op::GeI64 {
                dst: 16,
                a: 0,
                b: 8,
            },
            Op::ConstF64 { dst: 0, bits: 1 },
            Op::ConstF64 { dst: 8, bits: 2 },
            Op::AddF64 {
                dst: 16,
                a: 0,
                b: 8,
            },
            Op::MulF64 {
                dst: 16,
                a: 0,
                b: 8,
            },
            Op::Trace { id: 9 },
            Op::Await { dst: 16, input: 4 },
            Op::Ret { src: 16, size: 8 },
        ];
        let regions = vec![
            word_region(0, WordKind::Scalar),
            word_region(8, WordKind::Scalar),
            word_region(16, WordKind::Scalar),
        ];
        (
            Program {
                fns: vec![function(3, code)],
            },
            ProgramContract {
                functions: vec![function_contract(3, regions, &[0, 1], 2, None)],
                calls: vec![],
                schemas: vec![],
                value_shapes: vec![],
            },
        )
    }

    fn direct_program(args: Vec<ArgCopy>) -> (Program, ProgramContract) {
        let caller = function(
            3,
            vec![
                Op::Call {
                    callee: FnId(1),
                    args,
                    ret: 16,
                },
                Op::Ret { src: 16, size: 8 },
            ],
        );
        let callee = function(
            3,
            vec![
                Op::AddI64 {
                    dst: 16,
                    a: 0,
                    b: 8,
                },
                Op::Ret { src: 16, size: 8 },
            ],
        );
        let regions = || {
            vec![
                word_region(0, WordKind::Scalar),
                word_region(8, WordKind::Scalar),
                word_region(16, WordKind::Scalar),
            ]
        };
        (
            Program {
                fns: vec![caller, callee],
            },
            ProgramContract {
                functions: vec![
                    function_contract(3, regions(), &[0, 1], 2, None),
                    function_contract(3, regions(), &[0, 1], 2, None),
                ],
                calls: vec![],
                schemas: vec![],
                value_shapes: vec![],
            },
        )
    }

    fn valid_direct_program() -> (Program, ProgramContract) {
        direct_program(vec![
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
        ])
    }

    fn scalar_call_contract() -> CallContract {
        CallContract {
            entries: vec![
                word_region(0, WordKind::Scalar),
                word_region(8, WordKind::Scalar),
            ],
            result: word_region(16, WordKind::Scalar),
        }
    }

    fn indirect_program() -> (Program, ProgramContract) {
        let caller_regions = vec![
            word_region(0, WordKind::Callable(CallContractId(0))),
            word_region(8, WordKind::Scalar),
            word_region(16, WordKind::Scalar),
            word_region(24, WordKind::Scalar),
        ];
        let callee_regions = vec![
            word_region(0, WordKind::Scalar),
            word_region(8, WordKind::Scalar),
            word_region(16, WordKind::Scalar),
        ];
        (
            Program {
                fns: vec![
                    function(
                        4,
                        vec![
                            Op::ConstI64 { dst: 0, value: 1 },
                            Op::CallIndirect {
                                callee: 0,
                                args: vec![
                                    ArgCopy {
                                        src: 8,
                                        dst: 0,
                                        size: 8,
                                    },
                                    ArgCopy {
                                        src: 16,
                                        dst: 8,
                                        size: 8,
                                    },
                                ],
                                ret: 24,
                            },
                            Op::Ret { src: 24, size: 8 },
                        ],
                    ),
                    function(
                        3,
                        vec![
                            Op::AddI64 {
                                dst: 16,
                                a: 0,
                                b: 8,
                            },
                            Op::Ret { src: 16, size: 8 },
                        ],
                    ),
                ],
            },
            ProgramContract {
                functions: vec![
                    function_contract(4, caller_regions, &[1, 2], 3, None),
                    function_contract(3, callee_regions, &[0, 1], 2, Some(0)),
                ],
                calls: vec![scalar_call_contract()],
                schemas: vec![],
                value_shapes: vec![],
            },
        )
    }

    fn compare_program(byte_comparable: bool) -> (Program, ProgramContract) {
        let string_schema = SchemaRef(0);
        (
            Program {
                fns: vec![function(
                    3,
                    vec![
                        Op::CompareValueBytes {
                            dst: 16,
                            a: 0,
                            b: 8,
                        },
                        Op::Ret { src: 16, size: 8 },
                    ],
                )],
            },
            ProgramContract {
                functions: vec![function_contract(
                    3,
                    vec![
                        word_region(0, WordKind::Handle(string_schema)),
                        word_region(8, WordKind::Handle(string_schema)),
                        word_region(16, WordKind::Scalar),
                    ],
                    &[0, 1],
                    2,
                    None,
                )],
                calls: vec![],
                schemas: vec![SchemaContract {
                    inline: RegionShape::word(WordKind::Handle(string_schema)),
                    value_shape: None,
                    payload: PayloadKind::OpaqueBytes { byte_comparable },
                }],
                value_shapes: vec![],
            },
        )
    }

    fn gapped_and_schema_program() -> (Program, ProgramContract) {
        (
            Program {
                fns: vec![function(
                    3,
                    vec![
                        Op::ConstI64 { dst: 0, value: 11 },
                        Op::Ret { src: 0, size: 8 },
                    ],
                )],
            },
            ProgramContract {
                functions: vec![function_contract(
                    3,
                    vec![
                        word_region(0, WordKind::Scalar),
                        word_region(16, WordKind::Scalar),
                    ],
                    &[],
                    0,
                    None,
                )],
                calls: vec![],
                schemas: vec![
                    SchemaContract {
                        inline: RegionShape::default(),
                        value_shape: None,
                        payload: PayloadKind::Inline,
                    },
                    SchemaContract {
                        inline: RegionShape::word(WordKind::Handle(SchemaRef(1))),
                        value_shape: None,
                        payload: PayloadKind::OpaqueBytes {
                            byte_comparable: true,
                        },
                    },
                    SchemaContract {
                        inline: RegionShape::word(WordKind::Handle(SchemaRef(2))),
                        value_shape: None,
                        payload: PayloadKind::DenseArray {
                            element: SchemaRef(0),
                        },
                    },
                ],
                value_shapes: vec![],
            },
        )
    }

    fn single_function(
        words: usize,
        code: Vec<Op>,
        regions: Vec<FrameRegion>,
        result: u32,
    ) -> (Program, ProgramContract) {
        (
            Program {
                fns: vec![function(words, code)],
            },
            ProgramContract {
                functions: vec![function_contract(words, regions, &[], result, None)],
                calls: vec![],
                schemas: vec![],
                value_shapes: vec![],
            },
        )
    }

    fn op_error(pc: usize, defect: ProgramDefect) -> ProgramError {
        ProgramError {
            function: Some(FnId(0)),
            pc: Some(pc),
            defect,
        }
    }

    fn function_error(defect: ProgramDefect) -> ProgramError {
        ProgramError {
            function: Some(FnId(0)),
            pc: None,
            defect,
        }
    }

    fn global_error(defect: ProgramDefect) -> ProgramError {
        ProgramError {
            function: None,
            pc: None,
            defect,
        }
    }

    #[test]
    fn valid_scalar_direct_indirect_and_compare_programs() {
        let (scalar_program, scalar_contract) = scalar_program();
        let (direct_program, direct_contract) = valid_direct_program();
        let (indirect_program, indirect_contract) = indirect_program();
        let (compare_program, compare_contract) = compare_program(true);
        let (gapped_program, gapped_contract) = gapped_and_schema_program();
        let cases = vec![
            ValidCase {
                name: "scalar",
                program: scalar_program,
                contract: scalar_contract,
                requirements: DriveRequirements {
                    await_inputs: 5,
                    hosts: 0,
                },
                call: None,
            },
            ValidCase {
                name: "direct",
                program: direct_program,
                contract: direct_contract,
                requirements: DriveRequirements::default(),
                call: Some((
                    FnId(0),
                    0,
                    CallSiteFacts::Direct {
                        callee: FnId(1),
                        result_size: 8,
                    },
                )),
            },
            ValidCase {
                name: "indirect",
                program: indirect_program,
                contract: indirect_contract,
                requirements: DriveRequirements::default(),
                call: Some((
                    FnId(0),
                    1,
                    CallSiteFacts::Indirect {
                        result_size: 8,
                        obligation: IndirectCallObligation {
                            function_count: 2,
                            contract: CallContractId(0),
                        },
                    },
                )),
            },
            ValidCase {
                name: "compare",
                program: compare_program,
                contract: compare_contract,
                requirements: DriveRequirements::default(),
                call: None,
            },
            ValidCase {
                name: "gapped schema table",
                program: gapped_program,
                contract: gapped_contract,
                requirements: DriveRequirements::default(),
                call: None,
            },
        ];

        for case in cases {
            let verified = case.program.verify(case.contract).expect(case.name);
            assert_eq!(
                verified.drive_requirements(),
                case.requirements,
                "{}",
                case.name
            );
            if let Some((function, pc, expected)) = case.call {
                let actual = verified
                    .facts()
                    .function(function)
                    .and_then(|facts| facts.pc(pc))
                    .and_then(PcFacts::call);
                assert_eq!(actual, Some(expected), "{}", case.name);
            }
            assert!(
                verified
                    .facts()
                    .functions()
                    .iter()
                    .flat_map(FunctionFacts::pcs)
                    .all(PcFacts::is_reachable),
                "{}",
                case.name
            );
        }
    }

    #[test]
    fn rejects_structural_and_instruction_defects() {
        let overlap = single_function(
            2,
            vec![Op::Ret { src: 0, size: 16 }],
            vec![
                region(
                    0,
                    RegionShape::new(vec![kinds(WordKind::Scalar), kinds(WordKind::Scalar)]),
                ),
                word_region(8, WordKind::Scalar),
            ],
            0,
        );
        let bounds = single_function(
            1,
            vec![Op::Ret { src: 8, size: 8 }],
            vec![word_region(8, WordKind::Scalar)],
            0,
        );
        let unaligned = single_function(
            2,
            vec![Op::Ret { src: 4, size: 8 }],
            vec![word_region(4, WordKind::Scalar)],
            0,
        );
        let op_bounds = single_function(
            1,
            vec![
                Op::ConstI64 { dst: 8, value: 0 },
                Op::Ret { src: 0, size: 8 },
            ],
            vec![word_region(0, WordKind::Scalar)],
            0,
        );
        let schema = SchemaRef(0);
        let kinds_program = Program {
            fns: vec![function(
                3,
                vec![
                    Op::AddI64 {
                        dst: 16,
                        a: 0,
                        b: 8,
                    },
                    Op::Ret { src: 16, size: 8 },
                ],
            )],
        };
        let kinds_contract = ProgramContract {
            functions: vec![function_contract(
                3,
                vec![
                    word_region(0, WordKind::Handle(schema)),
                    word_region(8, WordKind::Scalar),
                    word_region(16, WordKind::Scalar),
                ],
                &[],
                2,
                None,
            )],
            calls: vec![],
            schemas: vec![SchemaContract {
                inline: RegionShape::default(),
                value_shape: None,
                payload: PayloadKind::Inline,
            }],
            value_shapes: vec![],
        };
        let jump = single_function(
            1,
            vec![Op::Jump { target: 1 }],
            vec![word_region(0, WordKind::Scalar)],
            0,
        );
        let fallthrough = single_function(
            1,
            vec![Op::ConstI64 { dst: 0, value: 0 }],
            vec![word_region(0, WordKind::Scalar)],
            0,
        );
        let args = direct_program(vec![ArgCopy {
            src: 0,
            dst: 0,
            size: 8,
        }]);
        let argument_destination = direct_program(vec![
            ArgCopy {
                src: 0,
                dst: 0,
                size: 8,
            },
            ArgCopy {
                src: 8,
                dst: 0,
                size: 8,
            },
        ]);
        let ret = single_function(
            2,
            vec![Op::Ret { src: 0, size: 8 }],
            vec![
                word_region(0, WordKind::Scalar),
                word_region(8, WordKind::Scalar),
            ],
            1,
        );
        let await_kind = single_function(
            1,
            vec![Op::Await { dst: 0, input: 3 }, Op::Ret { src: 0, size: 8 }],
            vec![word_region(0, WordKind::Status)],
            0,
        );
        let (compare_program, compare_contract) = compare_program(false);

        let cases = vec![
            InvalidCase {
                name: "overlap",
                program: overlap.0,
                contract: overlap.1,
                expected: function_error(ProgramDefect::RegionOverlap {
                    first: RegionId(0),
                    second: RegionId(1),
                }),
            },
            InvalidCase {
                name: "bounds",
                program: bounds.0,
                contract: bounds.1,
                expected: function_error(ProgramDefect::RegionOutOfBounds {
                    region: RegionId(0),
                    end: 16,
                    frame_size: 8,
                }),
            },
            InvalidCase {
                name: "alignment",
                program: unaligned.0,
                contract: unaligned.1,
                expected: function_error(ProgramDefect::RegionOffsetNotWordAligned {
                    region: RegionId(0),
                    offset: 4,
                }),
            },
            InvalidCase {
                name: "op bounds",
                program: op_bounds.0,
                contract: op_bounds.1,
                expected: op_error(
                    0,
                    ProgramDefect::Access {
                        role: AccessRole::Destination,
                        defect: AccessDefect::OutOfBounds {
                            offset: 8,
                            size: 8,
                            frame_size: 8,
                        },
                    },
                ),
            },
            InvalidCase {
                name: "scalar kind",
                program: kinds_program,
                contract: kinds_contract,
                expected: op_error(
                    0,
                    ProgramDefect::KindMismatch {
                        role: AccessRole::LeftOperand,
                        required: KindRequirement::Scalar,
                        allowed: kinds(WordKind::Handle(schema)),
                    },
                ),
            },
            InvalidCase {
                name: "jump",
                program: jump.0,
                contract: jump.1,
                expected: op_error(
                    0,
                    ProgramDefect::JumpTargetOutOfRange {
                        target: 1,
                        code_len: 1,
                    },
                ),
            },
            InvalidCase {
                name: "fallthrough",
                program: fallthrough.0,
                contract: fallthrough.1,
                expected: op_error(1, ProgramDefect::ReachableFallthrough),
            },
            InvalidCase {
                name: "args",
                program: args.0,
                contract: args.1,
                expected: op_error(
                    0,
                    ProgramDefect::CallArgumentCount {
                        expected: 2,
                        actual: 1,
                    },
                ),
            },
            InvalidCase {
                name: "argument destination",
                program: argument_destination.0,
                contract: argument_destination.1,
                expected: op_error(
                    0,
                    ProgramDefect::CallArgumentDestination {
                        index: 1,
                        expected_offset: 8,
                        expected_size: 8,
                        actual_offset: 0,
                        actual_size: 8,
                    },
                ),
            },
            InvalidCase {
                name: "ret",
                program: ret.0,
                contract: ret.1,
                expected: op_error(
                    0,
                    ProgramDefect::ReturnRegionMismatch {
                        expected: RegionId(1),
                        actual: RegionId(0),
                    },
                ),
            },
            InvalidCase {
                name: "compare",
                program: compare_program,
                contract: compare_contract,
                expected: op_error(0, ProgramDefect::SchemaNotByteComparable { schema }),
            },
            InvalidCase {
                name: "await kind",
                program: await_kind.0,
                contract: await_kind.1,
                expected: op_error(
                    0,
                    ProgramDefect::KindMismatch {
                        role: AccessRole::AwaitDestination,
                        required: KindRequirement::Scalar,
                        allowed: kinds(WordKind::Status),
                    },
                ),
            },
        ];

        for case in cases {
            let error = case.program.verify(case.contract).expect_err(case.name);
            assert_eq!(error, case.expected, "{}", case.name);
        }
    }

    #[test]
    fn rejects_non_narrowed_indirect_and_mismatched_function_contracts() {
        let call_zero = CallContract {
            entries: vec![],
            result: word_region(0, WordKind::Scalar),
        };
        let callable_kinds =
            kinds(WordKind::Callable(CallContractId(0))).allowing(WordKind::Scalar);
        let non_narrowed = (
            Program {
                fns: vec![function(
                    2,
                    vec![
                        Op::ConstI64 { dst: 0, value: 1 },
                        Op::CallIndirect {
                            callee: 0,
                            args: vec![],
                            ret: 8,
                        },
                        Op::Ret { src: 8, size: 8 },
                    ],
                )],
            },
            ProgramContract {
                functions: vec![function_contract(
                    2,
                    vec![
                        region(0, RegionShape::new(vec![callable_kinds])),
                        word_region(8, WordKind::Scalar),
                    ],
                    &[],
                    1,
                    None,
                )],
                calls: vec![call_zero.clone()],
                schemas: vec![],
                value_shapes: vec![],
            },
        );
        let result_placement = (
            Program {
                fns: vec![function(1, vec![Op::Ret { src: 0, size: 8 }])],
            },
            ProgramContract {
                functions: vec![function_contract(
                    1,
                    vec![word_region(0, WordKind::Scalar)],
                    &[],
                    0,
                    Some(0),
                )],
                calls: vec![CallContract {
                    entries: vec![],
                    result: word_region(8, WordKind::Scalar),
                }],
                schemas: vec![],
                value_shapes: vec![],
            },
        );
        let result_shape_mismatch = (
            Program {
                fns: vec![function(1, vec![Op::Ret { src: 0, size: 8 }])],
            },
            ProgramContract {
                functions: vec![function_contract(
                    1,
                    vec![word_region(0, WordKind::Scalar)],
                    &[],
                    0,
                    Some(0),
                )],
                calls: vec![CallContract {
                    entries: vec![],
                    result: word_region(0, WordKind::Status),
                }],
                schemas: vec![],
                value_shapes: vec![],
            },
        );
        let multi_entry_placement = (
            Program {
                fns: vec![function(3, vec![Op::Ret { src: 0, size: 8 }])],
            },
            ProgramContract {
                functions: vec![function_contract(
                    3,
                    vec![
                        word_region(0, WordKind::Scalar),
                        word_region(8, WordKind::Scalar),
                        word_region(16, WordKind::Scalar),
                    ],
                    &[1, 2],
                    0,
                    Some(0),
                )],
                calls: vec![CallContract {
                    entries: vec![
                        word_region(8, WordKind::Scalar),
                        word_region(16, WordKind::Scalar),
                    ],
                    result: word_region(24, WordKind::Scalar),
                }],
                schemas: vec![],
                value_shapes: vec![],
            },
        );
        let entry_placement_mismatch = (
            Program {
                fns: vec![function(3, vec![Op::Ret { src: 0, size: 8 }])],
            },
            ProgramContract {
                functions: vec![function_contract(
                    3,
                    vec![
                        word_region(0, WordKind::Scalar),
                        word_region(8, WordKind::Scalar),
                        word_region(16, WordKind::Scalar),
                    ],
                    &[1, 2],
                    0,
                    Some(0),
                )],
                calls: vec![CallContract {
                    entries: vec![
                        word_region(0, WordKind::Scalar),
                        word_region(16, WordKind::Scalar),
                    ],
                    result: word_region(24, WordKind::Scalar),
                }],
                schemas: vec![],
                value_shapes: vec![],
            },
        );
        let duplicate_calls = (
            Program::default(),
            ProgramContract {
                functions: vec![],
                calls: vec![call_zero.clone(), call_zero],
                schemas: vec![],
                value_shapes: vec![],
            },
        );
        let constant_contract = (
            Program {
                fns: vec![
                    function(
                        2,
                        vec![
                            Op::ConstI64 { dst: 0, value: 1 },
                            Op::Ret { src: 8, size: 8 },
                        ],
                    ),
                    function(2, vec![Op::Ret { src: 8, size: 8 }]),
                ],
            },
            ProgramContract {
                functions: vec![
                    function_contract(
                        2,
                        vec![
                            word_region(0, WordKind::Callable(CallContractId(0))),
                            word_region(8, WordKind::Scalar),
                        ],
                        &[],
                        1,
                        None,
                    ),
                    function_contract(2, vec![word_region(8, WordKind::Scalar)], &[], 0, Some(1)),
                ],
                calls: vec![
                    CallContract {
                        entries: vec![],
                        result: word_region(0, WordKind::Scalar),
                    },
                    CallContract {
                        entries: vec![],
                        result: word_region(8, WordKind::Scalar),
                    },
                ],
                schemas: vec![],
                value_shapes: vec![],
            },
        );

        let cases = vec![
            InvalidCase {
                name: "callable and scalar union",
                program: non_narrowed.0,
                contract: non_narrowed.1,
                expected: op_error(
                    1,
                    ProgramDefect::KindMismatch {
                        role: AccessRole::Callee,
                        required: KindRequirement::Callable,
                        allowed: kinds(WordKind::Callable(CallContractId(0)))
                            .allowing(WordKind::Scalar),
                    },
                ),
            },
            InvalidCase {
                name: "function contract result shape",
                program: result_shape_mismatch.0,
                contract: result_shape_mismatch.1,
                expected: function_error(ProgramDefect::FunctionCallContractMismatch {
                    contract: CallContractId(0),
                }),
            },
            InvalidCase {
                name: "function contract entry placement",
                program: entry_placement_mismatch.0,
                contract: entry_placement_mismatch.1,
                expected: function_error(ProgramDefect::FunctionCallContractMismatch {
                    contract: CallContractId(0),
                }),
            },
            InvalidCase {
                name: "deduplicated call table",
                program: duplicate_calls.0,
                contract: duplicate_calls.1,
                expected: global_error(ProgramDefect::DuplicateCallContract {
                    first: CallContractId(0),
                    duplicate: CallContractId(1),
                }),
            },
            InvalidCase {
                name: "constant callable contract",
                program: constant_contract.0,
                contract: constant_contract.1,
                expected: op_error(
                    0,
                    ProgramDefect::ConstantCallableContractMismatch {
                        target: FnId(1),
                        declared: Some(CallContractId(1)),
                        allowed: vec![CallContractId(0)],
                    },
                ),
            },
        ];

        for case in cases {
            let error = case.program.verify(case.contract).expect_err(case.name);
            assert_eq!(error, case.expected, "{}", case.name);
        }
        result_placement
            .0
            .verify(result_placement.1)
            .expect("function call contracts ignore only concrete result placement");
        multi_entry_placement
            .0
            .verify(multi_entry_placement.1)
            .expect("function call contracts preserve exact multi-entry argument placement");
    }

    #[test]
    fn rejects_union_reads_and_directional_assignment_leaks() {
        let schema = SchemaRef(0);
        let scalar_or_handle = kinds(WordKind::Scalar).allowing(WordKind::Handle(schema));
        let scalar_or_handle_shape = RegionShape::new(vec![scalar_or_handle.clone()]);
        let schemas = || {
            vec![SchemaContract {
                inline: RegionShape::default(),
                value_shape: None,
                payload: PayloadKind::Inline,
            }]
        };

        let scalar_read = (
            Program {
                fns: vec![function(
                    3,
                    vec![
                        Op::AddI64 {
                            dst: 16,
                            a: 0,
                            b: 8,
                        },
                        Op::Ret { src: 16, size: 8 },
                    ],
                )],
            },
            ProgramContract {
                functions: vec![function_contract(
                    3,
                    vec![
                        region(0, scalar_or_handle_shape.clone()),
                        word_region(8, WordKind::Scalar),
                        word_region(16, WordKind::Scalar),
                    ],
                    &[],
                    2,
                    None,
                )],
                calls: vec![],
                schemas: schemas(),
                value_shapes: vec![],
            },
        );
        let (compare_program, mut compare_contract) = compare_program(true);
        compare_contract.functions[0].frame.regions[0].shape = scalar_or_handle_shape.clone();

        let (copy_program, mut copy_contract) = single_function(
            2,
            vec![Op::CopyI64 { dst: 8, src: 0 }, Op::Ret { src: 8, size: 8 }],
            vec![
                region(0, scalar_or_handle_shape.clone()),
                word_region(8, WordKind::Scalar),
            ],
            1,
        );
        copy_contract.schemas = schemas();

        let (argument_program, mut argument_contract) = valid_direct_program();
        argument_contract.schemas = schemas();
        argument_contract.functions[0].frame.regions[0].shape = scalar_or_handle_shape.clone();

        let (result_program, mut result_contract) = valid_direct_program();
        result_contract.schemas = schemas();
        result_contract.functions[1].frame.regions[2].shape = scalar_or_handle_shape.clone();

        let (narrow_program, mut narrow_contract) = single_function(
            2,
            vec![Op::CopyI64 { dst: 8, src: 0 }, Op::Ret { src: 8, size: 8 }],
            vec![
                word_region(0, WordKind::Scalar),
                region(8, scalar_or_handle_shape.clone()),
            ],
            1,
        );
        narrow_contract.schemas = schemas();
        narrow_program
            .verify(narrow_contract)
            .expect("narrow-to-wide copy is admitted");

        let cases = vec![
            InvalidCase {
                name: "scalar union read",
                program: scalar_read.0,
                contract: scalar_read.1,
                expected: op_error(
                    0,
                    ProgramDefect::KindMismatch {
                        role: AccessRole::LeftOperand,
                        required: KindRequirement::Scalar,
                        allowed: scalar_or_handle.clone(),
                    },
                ),
            },
            InvalidCase {
                name: "handle and scalar union",
                program: compare_program,
                contract: compare_contract,
                expected: op_error(
                    0,
                    ProgramDefect::KindMismatch {
                        role: AccessRole::CompareLeft,
                        required: KindRequirement::Handle,
                        allowed: scalar_or_handle.clone(),
                    },
                ),
            },
            InvalidCase {
                name: "overlapping copy kinds",
                program: copy_program,
                contract: copy_contract,
                expected: op_error(
                    0,
                    ProgramDefect::IncompatibleWordKinds {
                        source: scalar_or_handle.clone(),
                        destination: kinds(WordKind::Scalar),
                    },
                ),
            },
            InvalidCase {
                name: "call argument direction",
                program: argument_program,
                contract: argument_contract,
                expected: op_error(
                    0,
                    ProgramDefect::CallArgumentKinds {
                        index: 0,
                        source: scalar_or_handle_shape.clone(),
                        destination: RegionShape::word(WordKind::Scalar),
                    },
                ),
            },
            InvalidCase {
                name: "call result direction",
                program: result_program,
                contract: result_contract,
                expected: op_error(
                    0,
                    ProgramDefect::CallResultKinds {
                        source: scalar_or_handle_shape,
                        destination: RegionShape::word(WordKind::Scalar),
                    },
                ),
            },
        ];

        for case in cases {
            let error = case.program.verify(case.contract).expect_err(case.name);
            assert_eq!(error, case.expected, "{}", case.name);
        }
    }

    #[test]
    fn rejects_every_deferred_opcode_even_when_unreachable() {
        let unsupported = vec![
            (
                UnsupportedOp::LoadIndexedI64,
                Op::LoadIndexedI64 {
                    dst: 0,
                    base: 0,
                    index: 0,
                    stride: 8,
                },
            ),
            (
                UnsupportedOp::StoreIndexedI64,
                Op::StoreIndexedI64 {
                    base: 0,
                    index: 0,
                    stride: 8,
                    src: 0,
                },
            ),
            (
                UnsupportedOp::LoadArrayWord,
                Op::LoadArrayWord {
                    dst: 0,
                    present: 0,
                    array: 0,
                    index: 0,
                    elem_schema_ref: 0,
                },
            ),
            (UnsupportedOp::HostCall, Op::HostCall { host: 0 }),
            (UnsupportedOp::HostCallYield, Op::HostCallYield { host: 0 }),
        ];

        for (expected, op) in unsupported {
            let (program, contract) = single_function(
                1,
                vec![Op::Jump { target: 2 }, op, Op::Ret { src: 0, size: 8 }],
                vec![word_region(0, WordKind::Scalar)],
                0,
            );
            let error = program.verify(contract).expect_err("unsupported opcode");
            assert_eq!(
                error,
                op_error(1, ProgramDefect::UnsupportedOp { op: expected })
            );
        }
    }

    #[test]
    fn unreachable_await_is_validated_cached_and_counted() {
        let (program, contract) = single_function(
            1,
            vec![
                Op::Jump { target: 2 },
                Op::Await { dst: 0, input: 7 },
                Op::Ret { src: 0, size: 8 },
            ],
            vec![word_region(0, WordKind::Scalar)],
            0,
        );
        let verified = program.verify(contract).expect("valid program");
        assert_eq!(
            verified.drive_requirements(),
            DriveRequirements {
                await_inputs: 8,
                hosts: 0,
            }
        );
        let facts = verified.facts().function(FnId(0)).expect("function facts");
        assert!(facts.pc(0).is_some_and(PcFacts::is_reachable));
        assert!(facts.pc(1).is_some_and(|facts| !facts.is_reachable()));
        assert!(facts.pc(2).is_some_and(PcFacts::is_reachable));
    }

    fn array_fixture(element: RegionShape) -> (Program, ProgramContract) {
        let array = SchemaRef(1);
        let code = vec![
            Op::ArrayNew {
                dst: 0,
                status: 8,
                count_slot: 16,
                elem_width: 16,
                elem_schema_ref: 0,
            },
            Op::ArrayStore {
                status: 8,
                array: 0,
                index: 40,
                src: 24,
                elem_width: 16,
                elem_schema_ref: 0,
            },
            Op::LoadArray {
                dst: 48,
                status: 8,
                array: 0,
                index: 40,
                elem_width: 16,
                elem_schema_ref: 0,
            },
            Op::LoadArrayLen {
                dst: 64,
                status: 8,
                array: 0,
                elem_schema_ref: 0,
            },
            Op::Ret { src: 64, size: 8 },
        ];
        let regions = vec![
            word_region(0, WordKind::Handle(array)),
            word_region(8, WordKind::Status),
            word_region(16, WordKind::Scalar),
            region(24, element.clone()),
            word_region(40, WordKind::Scalar),
            region(48, element.clone()),
            word_region(64, WordKind::Scalar),
        ];
        (
            Program {
                fns: vec![function(9, code)],
            },
            ProgramContract {
                functions: vec![function_contract(9, regions, &[], 6, None)],
                calls: vec![],
                schemas: vec![
                    SchemaContract {
                        inline: element,
                        value_shape: None,
                        payload: PayloadKind::Inline,
                    },
                    SchemaContract {
                        inline: RegionShape::word(WordKind::Handle(array)),
                        value_shape: None,
                        payload: PayloadKind::DenseArray {
                            element: SchemaRef(0),
                        },
                    },
                    SchemaContract {
                        inline: RegionShape::word(WordKind::Scalar),
                        value_shape: None,
                        payload: PayloadKind::Inline,
                    },
                ],
                value_shapes: vec![],
            },
        )
    }

    #[test]
    fn verifies_checked_array_contract_table() {
        let narrow = RegionShape::new(vec![kinds(WordKind::Scalar), kinds(WordKind::Scalar)]);
        let wide = RegionShape::new(vec![
            kinds(WordKind::Scalar).allowing(WordKind::Opaque),
            kinds(WordKind::Scalar),
        ]);
        for (name, program, contract) in [
            {
                let (program, contract) = array_fixture(narrow.clone());
                ("multiword", program, contract)
            },
            {
                let (program, mut contract) = array_fixture(wide.clone());
                contract.functions[0].frame.regions[3] = region(24, narrow.clone());
                ("source narrow to wide", program, contract)
            },
            {
                let (program, mut contract) = array_fixture(narrow.clone());
                contract.functions[0].frame.regions[5] = region(48, wide.clone());
                ("destination narrow to wide", program, contract)
            },
        ] {
            program.verify(contract).expect(name);
        }

        let make = || array_fixture(narrow.clone());
        let mut cases = Vec::new();
        let (program, mut contract) = make();
        contract.schemas[1].payload = PayloadKind::Inline;
        cases.push((
            "non dense",
            program,
            contract,
            op_error(
                0,
                ProgramDefect::ArraySchemaNotDense {
                    schema: SchemaRef(1),
                },
            ),
        ));
        let (program, mut contract) = make();
        contract.functions[0].frame.regions[0] = region(
            0,
            RegionShape::new(vec![
                kinds(WordKind::Handle(SchemaRef(1))).allowing(WordKind::Handle(SchemaRef(0))),
            ]),
        );
        cases.push((
            "non singleton handle",
            program,
            contract,
            op_error(
                0,
                ProgramDefect::KindMismatch {
                    role: AccessRole::ArrayHandle,
                    required: KindRequirement::Handle,
                    allowed: kinds(WordKind::Handle(SchemaRef(1)))
                        .allowing(WordKind::Handle(SchemaRef(0))),
                },
            ),
        ));
        let (program, mut contract) = make();
        contract.functions[0].frame.regions[0] = word_region(0, WordKind::Scalar);
        cases.push((
            "wrong handle kind",
            program,
            contract,
            op_error(
                0,
                ProgramDefect::KindMismatch {
                    role: AccessRole::ArrayHandle,
                    required: KindRequirement::Handle,
                    allowed: kinds(WordKind::Scalar),
                },
            ),
        ));
        for (name, witness) in [("negative witness", -1), ("positive witness", 3)] {
            let (mut program, contract) = make();
            let Op::ArrayNew {
                elem_schema_ref, ..
            } = &mut program.fns[0].code[0]
            else {
                unreachable!()
            };
            *elem_schema_ref = witness;
            cases.push((
                name,
                program,
                contract,
                op_error(
                    0,
                    ProgramDefect::ArrayElementWitnessOutOfRange {
                        witness,
                        schema_count: 3,
                    },
                ),
            ));
        }
        let (mut program, contract) = make();
        let Op::ArrayNew {
            elem_schema_ref, ..
        } = &mut program.fns[0].code[0]
        else {
            unreachable!()
        };
        *elem_schema_ref = 2;
        cases.push((
            "witness mismatch",
            program,
            contract,
            op_error(
                0,
                ProgramDefect::ArrayElementSchemaMismatch {
                    array: SchemaRef(1),
                    expected: SchemaRef(0),
                    actual: SchemaRef(2),
                },
            ),
        ));
        for (name, width) in [("zero width", 0), ("wrong width", 8)] {
            let (mut program, contract) = make();
            let Op::ArrayNew { elem_width, .. } = &mut program.fns[0].code[0] else {
                unreachable!()
            };
            *elem_width = width;
            cases.push((
                name,
                program,
                contract,
                op_error(
                    0,
                    ProgramDefect::ArrayElementWidth {
                        schema: SchemaRef(0),
                        expected: 16,
                        actual: width,
                    },
                ),
            ));
        }
        let (program, mut contract) = make();
        contract.functions[0].frame.regions[1] = word_region(8, WordKind::Scalar);
        cases.push((
            "status",
            program,
            contract,
            op_error(
                0,
                ProgramDefect::KindMismatch {
                    role: AccessRole::ArrayStatus,
                    required: KindRequirement::Status,
                    allowed: kinds(WordKind::Scalar),
                },
            ),
        ));
        for (name, region_index, pc, role) in [
            ("count", 2, 0, AccessRole::ArrayCount),
            ("index", 4, 1, AccessRole::ArrayIndex),
        ] {
            let (program, mut contract) = make();
            contract.functions[0].frame.regions[region_index].shape =
                RegionShape::word(WordKind::Status);
            cases.push((
                name,
                program,
                contract,
                op_error(
                    pc,
                    ProgramDefect::KindMismatch {
                        role,
                        required: KindRequirement::Scalar,
                        allowed: kinds(WordKind::Status),
                    },
                ),
            ));
        }
        let (mut program, contract) = make();
        let Op::ArrayStore { src, .. } = &mut program.fns[0].code[1] else {
            unreachable!()
        };
        *src = 32;
        cases.push((
            "partial region",
            program,
            contract,
            op_error(
                1,
                ProgramDefect::Access {
                    role: AccessRole::ArrayElementSource,
                    defect: AccessDefect::UndeclaredRegion {
                        offset: 32,
                        size: 16,
                    },
                },
            ),
        ));
        let (program, mut contract) = make();
        contract.functions[0].frame.regions[3] = region(24, wide.clone());
        cases.push((
            "source union",
            program,
            contract,
            op_error(
                1,
                ProgramDefect::ArrayElementShapes {
                    source: wide.clone(),
                    destination: narrow.clone(),
                },
            ),
        ));
        let (program, mut contract) = array_fixture(wide.clone());
        contract.functions[0].frame.regions[5] = region(48, narrow.clone());
        cases.push((
            "destination narrow",
            program,
            contract,
            op_error(
                2,
                ProgramDefect::ArrayElementShapes {
                    source: wide,
                    destination: narrow.clone(),
                },
            ),
        ));
        for (name, inline) in [
            ("empty dynamic inline", RegionShape::default()),
            ("scalar dynamic inline", RegionShape::word(WordKind::Scalar)),
            (
                "other handle dynamic inline",
                RegionShape::word(WordKind::Handle(SchemaRef(0))),
            ),
            (
                "union dynamic inline",
                RegionShape::new(vec![
                    kinds(WordKind::Handle(SchemaRef(1))).allowing(WordKind::Opaque),
                ]),
            ),
        ] {
            let (program, mut contract) = make();
            contract.schemas[1].inline = inline.clone();
            cases.push((
                name,
                program,
                contract,
                global_error(ProgramDefect::DynamicSchemaInlineMismatch {
                    schema: SchemaRef(1),
                    inline,
                }),
            ));
        }
        let (program, mut contract) = make();
        contract.schemas[0].payload = PayloadKind::DenseArray {
            element: SchemaRef(2),
        };
        cases.push((
            "nested dense element scalar inline",
            program,
            contract,
            global_error(ProgramDefect::DynamicSchemaInlineMismatch {
                schema: SchemaRef(0),
                inline: narrow.clone(),
            }),
        ));
        for (name, program, contract, expected) in cases {
            assert_eq!(
                program.verify(contract).expect_err(name),
                expected,
                "{name}"
            );
        }
    }

    #[test]
    fn array_status_discriminator_requires_exact_scalar_and_status_regions() {
        let fixture = || {
            single_function(
                2,
                vec![
                    Op::ArrayStatusIs {
                        dst: 0,
                        status: 8,
                        expected: crate::task::ArrayOpStatus::Ok,
                    },
                    Op::Ret { src: 0, size: 8 },
                ],
                vec![
                    word_region(0, WordKind::Scalar),
                    word_region(8, WordKind::Status),
                ],
                0,
            )
        };

        let (valid_program, valid_contract) = fixture();
        valid_program
            .verify(valid_contract)
            .expect("closed array status discriminator verifies");

        let (program, mut contract) = fixture();
        contract.functions[0].frame.regions[0] = word_region(0, WordKind::Status);
        assert_eq!(
            program.verify(contract).expect_err("status destination"),
            op_error(
                0,
                ProgramDefect::KindMismatch {
                    role: AccessRole::Destination,
                    required: KindRequirement::Scalar,
                    allowed: kinds(WordKind::Status),
                },
            )
        );

        let (program, mut contract) = fixture();
        contract.functions[0].frame.regions[1] = word_region(8, WordKind::Scalar);
        assert_eq!(
            program.verify(contract).expect_err("scalar status source"),
            op_error(
                0,
                ProgramDefect::KindMismatch {
                    role: AccessRole::ArrayStatusSource,
                    required: KindRequirement::Status,
                    allowed: kinds(WordKind::Scalar),
                },
            )
        );

        let (program, mut contract) = fixture();
        contract.functions[0].frame.regions[1].shape =
            RegionShape::new(vec![kinds(WordKind::Status).allowing(WordKind::Opaque)]);
        assert_eq!(
            program.verify(contract).expect_err("status union source"),
            op_error(
                0,
                ProgramDefect::KindMismatch {
                    role: AccessRole::ArrayStatusSource,
                    required: KindRequirement::Status,
                    allowed: kinds(WordKind::Status).allowing(WordKind::Opaque),
                },
            )
        );
    }

    #[test]
    fn verifies_structural_value_shapes() {
        let string = SchemaRef(0);
        let string_schema = schema(
            RegionShape::word(WordKind::Handle(string)),
            PayloadKind::OpaqueBytes {
                byte_comparable: true,
            },
        );
        let scalar = RegionShape::word(WordKind::Scalar);
        let scalar_or_string = RegionShape::new(vec![
            kinds(WordKind::Scalar).allowing(WordKind::Handle(string)),
        ]);
        let outcome_shape = RegionShape::new(vec![
            kinds(WordKind::Scalar),
            scalar_or_string.words[0].clone(),
        ]);
        let outcome = ValueShapeContract {
            shape: outcome_shape.clone(),
            kind: ValueShapeKind::Enum {
                selector: selector(0, scalar.clone()),
                variants: vec![
                    variant(vec![field(8, scalar.clone())]),
                    variant(vec![field(8, RegionShape::word(WordKind::Handle(string)))]),
                ],
            },
        };

        let pair_shape = RegionShape::new(vec![kinds(WordKind::Scalar), kinds(WordKind::Scalar)]);
        let pair = ValueShapeContract {
            shape: pair_shape.clone(),
            kind: ValueShapeKind::Product {
                fields: vec![field(0, scalar.clone()), field(8, scalar.clone())],
            },
        };
        let nested_shape = RegionShape::new(
            [scalar.words.clone(), outcome_shape.words.clone()]
                .concat()
                .to_vec(),
        );
        let nested = ValueShapeContract {
            shape: nested_shape.clone(),
            kind: ValueShapeKind::Product {
                fields: vec![
                    field(0, scalar.clone()),
                    structural_field(8, outcome_shape.clone(), ValueShapeRef(0)),
                ],
            },
        };
        let one_word = ValueShapeContract {
            shape: scalar.clone(),
            kind: ValueShapeKind::Product {
                fields: vec![field(0, scalar.clone())],
            },
        };
        let unit = ValueShapeContract {
            shape: RegionShape::default(),
            kind: ValueShapeKind::Product { fields: vec![] },
        };
        let unit_field = ValueShapeContract {
            shape: RegionShape::default(),
            kind: ValueShapeKind::Product {
                fields: vec![structural_field(
                    0,
                    RegionShape::default(),
                    ValueShapeRef(0),
                )],
            },
        };

        let valid_outcome = (
            Program {
                fns: vec![function(2, vec![Op::Ret { src: 0, size: 16 }])],
            },
            ProgramContract {
                functions: vec![function_contract(
                    2,
                    vec![structural_region(
                        0,
                        outcome_shape.clone(),
                        ValueShapeRef(0),
                    )],
                    &[],
                    0,
                    None,
                )],
                calls: vec![],
                schemas: vec![string_schema.clone()],
                value_shapes: vec![outcome.clone()],
            },
        );

        let valid_nested = (
            Program {
                fns: vec![function(3, vec![Op::Ret { src: 0, size: 24 }])],
            },
            ProgramContract {
                functions: vec![function_contract(
                    3,
                    vec![structural_region(0, nested_shape.clone(), ValueShapeRef(1))],
                    &[],
                    0,
                    None,
                )],
                calls: vec![],
                schemas: vec![string_schema],
                value_shapes: vec![outcome, nested],
            },
        );

        let valid_array = {
            let array = SchemaRef(1);
            let code = vec![
                Op::ArrayNew {
                    dst: 0,
                    status: 8,
                    count_slot: 16,
                    elem_width: 16,
                    elem_schema_ref: 0,
                },
                Op::ArrayStore {
                    status: 8,
                    array: 0,
                    index: 40,
                    src: 24,
                    elem_width: 16,
                    elem_schema_ref: 0,
                },
                Op::LoadArray {
                    dst: 48,
                    status: 8,
                    array: 0,
                    index: 40,
                    elem_width: 16,
                    elem_schema_ref: 0,
                },
                Op::Ret { src: 48, size: 16 },
            ];
            (
                Program {
                    fns: vec![function(8, code)],
                },
                ProgramContract {
                    functions: vec![function_contract(
                        8,
                        vec![
                            word_region(0, WordKind::Handle(array)),
                            word_region(8, WordKind::Status),
                            word_region(16, WordKind::Scalar),
                            structural_region(24, pair_shape.clone(), ValueShapeRef(0)),
                            word_region(40, WordKind::Scalar),
                            structural_region(48, pair_shape.clone(), ValueShapeRef(0)),
                        ],
                        &[],
                        5,
                        None,
                    )],
                    calls: vec![],
                    schemas: vec![
                        structural_schema(
                            pair_shape.clone(),
                            ValueShapeRef(0),
                            PayloadKind::Inline,
                        ),
                        schema(
                            RegionShape::word(WordKind::Handle(array)),
                            PayloadKind::DenseArray {
                                element: SchemaRef(0),
                            },
                        ),
                    ],
                    value_shapes: vec![pair.clone()],
                },
            )
        };

        let valid_copy = (
            Program {
                fns: vec![function(
                    2,
                    vec![
                        Op::CopyValue {
                            dst: RegionId(1),
                            src: RegionId(0),
                        },
                        Op::Ret { src: 8, size: 8 },
                    ],
                )],
            },
            ProgramContract {
                functions: vec![function_contract(
                    2,
                    vec![
                        structural_region(0, scalar.clone(), ValueShapeRef(0)),
                        structural_region(8, scalar.clone(), ValueShapeRef(0)),
                    ],
                    &[],
                    1,
                    None,
                )],
                calls: vec![],
                schemas: vec![],
                value_shapes: vec![one_word],
            },
        );

        let duplicate_nominal_shapes = (
            Program::default(),
            ProgramContract {
                functions: vec![],
                calls: vec![],
                schemas: vec![],
                value_shapes: vec![pair.clone(), pair],
            },
        );
        let zero_word_product = (
            Program::default(),
            ProgramContract {
                functions: vec![],
                calls: vec![],
                schemas: vec![],
                value_shapes: vec![unit],
            },
        );
        let explicit_zero_word_field = (
            Program::default(),
            ProgramContract {
                functions: vec![],
                calls: vec![],
                schemas: vec![],
                value_shapes: vec![
                    ValueShapeContract {
                        shape: RegionShape::default(),
                        kind: ValueShapeKind::Product { fields: vec![] },
                    },
                    unit_field,
                ],
            },
        );

        for (name, program, contract) in [
            ("compact enum", valid_outcome.0, valid_outcome.1),
            ("nested product enum", valid_nested.0, valid_nested.1),
            ("dense structural array", valid_array.0, valid_array.1),
            ("one word structural copy", valid_copy.0, valid_copy.1),
            (
                "distinct nominal shapes",
                duplicate_nominal_shapes.0,
                duplicate_nominal_shapes.1,
            ),
            (
                "zero word product",
                zero_word_product.0,
                zero_word_product.1,
            ),
            (
                "explicit zero word field",
                explicit_zero_word_field.0,
                explicit_zero_word_field.1,
            ),
        ] {
            program.verify(contract).expect(name);
        }
    }

    fn structural_direct_program(
        caller_arg: Option<ValueShapeRef>,
        callee_arg: Option<ValueShapeRef>,
        callee_result: Option<ValueShapeRef>,
        caller_result: Option<ValueShapeRef>,
    ) -> (Program, ProgramContract) {
        let shape = RegionShape::new(vec![kinds(WordKind::Scalar), kinds(WordKind::Scalar)]);
        let with_ref = |offset, value_shape| {
            let region = region(offset, shape.clone());
            if let Some(value_shape) = value_shape {
                region.with_value_shape(value_shape)
            } else {
                region
            }
        };
        let shape_contract = || ValueShapeContract {
            shape: shape.clone(),
            kind: ValueShapeKind::Product {
                fields: vec![
                    field(0, RegionShape::word(WordKind::Scalar)),
                    field(8, RegionShape::word(WordKind::Scalar)),
                ],
            },
        };
        (
            Program {
                fns: vec![
                    function(
                        4,
                        vec![
                            Op::Call {
                                callee: FnId(1),
                                args: vec![ArgCopy {
                                    src: 0,
                                    dst: 0,
                                    size: 16,
                                }],
                                ret: 16,
                            },
                            Op::Ret { src: 16, size: 16 },
                        ],
                    ),
                    function(2, vec![Op::Ret { src: 0, size: 16 }]),
                ],
            },
            ProgramContract {
                functions: vec![
                    function_contract(
                        4,
                        vec![with_ref(0, caller_arg), with_ref(16, caller_result)],
                        &[0],
                        1,
                        None,
                    ),
                    function_contract(
                        2,
                        vec![with_ref(0, callee_arg.or(callee_result))],
                        &[0],
                        0,
                        None,
                    ),
                ],
                calls: vec![],
                schemas: vec![],
                value_shapes: vec![shape_contract(), shape_contract()],
            },
        )
    }

    #[test]
    fn verifies_structural_transfer_identity() {
        let same = structural_direct_program(
            Some(ValueShapeRef(0)),
            Some(ValueShapeRef(0)),
            Some(ValueShapeRef(0)),
            Some(ValueShapeRef(0)),
        );
        same.0.verify(same.1).expect("same structural ref");

        let cases = vec![
            InvalidCase {
                name: "different argument ref",
                program: structural_direct_program(
                    Some(ValueShapeRef(0)),
                    Some(ValueShapeRef(1)),
                    Some(ValueShapeRef(1)),
                    Some(ValueShapeRef(1)),
                )
                .0,
                contract: structural_direct_program(
                    Some(ValueShapeRef(0)),
                    Some(ValueShapeRef(1)),
                    Some(ValueShapeRef(1)),
                    Some(ValueShapeRef(1)),
                )
                .1,
                expected: op_error(
                    0,
                    ProgramDefect::StructuralTransferMismatch {
                        source: Some(ValueShapeRef(0)),
                        destination: Some(ValueShapeRef(1)),
                    },
                ),
            },
            InvalidCase {
                name: "structural to flat argument",
                program: structural_direct_program(Some(ValueShapeRef(0)), None, None, None).0,
                contract: structural_direct_program(Some(ValueShapeRef(0)), None, None, None).1,
                expected: op_error(
                    0,
                    ProgramDefect::StructuralTransferMismatch {
                        source: Some(ValueShapeRef(0)),
                        destination: None,
                    },
                ),
            },
            InvalidCase {
                name: "flat to structural argument",
                program: structural_direct_program(
                    None,
                    Some(ValueShapeRef(0)),
                    Some(ValueShapeRef(0)),
                    Some(ValueShapeRef(0)),
                )
                .0,
                contract: structural_direct_program(
                    None,
                    Some(ValueShapeRef(0)),
                    Some(ValueShapeRef(0)),
                    Some(ValueShapeRef(0)),
                )
                .1,
                expected: op_error(
                    0,
                    ProgramDefect::StructuralTransferMismatch {
                        source: None,
                        destination: Some(ValueShapeRef(0)),
                    },
                ),
            },
            InvalidCase {
                name: "different result ref",
                program: structural_direct_program(
                    Some(ValueShapeRef(0)),
                    Some(ValueShapeRef(0)),
                    Some(ValueShapeRef(0)),
                    Some(ValueShapeRef(1)),
                )
                .0,
                contract: structural_direct_program(
                    Some(ValueShapeRef(0)),
                    Some(ValueShapeRef(0)),
                    Some(ValueShapeRef(0)),
                    Some(ValueShapeRef(1)),
                )
                .1,
                expected: op_error(
                    0,
                    ProgramDefect::StructuralTransferMismatch {
                        source: Some(ValueShapeRef(0)),
                        destination: Some(ValueShapeRef(1)),
                    },
                ),
            },
        ];

        for case in cases {
            assert_eq!(
                case.program.verify(case.contract).expect_err(case.name),
                case.expected,
                "{}",
                case.name
            );
        }
    }

    #[test]
    fn rejects_structural_copy_and_array_transfer_mismatches() {
        let scalar = RegionShape::word(WordKind::Scalar);
        let pair_shape = RegionShape::new(vec![kinds(WordKind::Scalar), kinds(WordKind::Scalar)]);
        let one_word = || ValueShapeContract {
            shape: scalar.clone(),
            kind: ValueShapeKind::Product {
                fields: vec![field(0, scalar.clone())],
            },
        };
        let pair = || ValueShapeContract {
            shape: pair_shape.clone(),
            kind: ValueShapeKind::Product {
                fields: vec![field(0, scalar.clone()), field(8, scalar.clone())],
            },
        };

        let copy_ref_mismatch = single_function(
            2,
            vec![
                Op::CopyValue {
                    dst: RegionId(1),
                    src: RegionId(0),
                },
                Op::Ret { src: 8, size: 8 },
            ],
            vec![
                structural_region(0, scalar.clone(), ValueShapeRef(0)),
                structural_region(8, scalar.clone(), ValueShapeRef(1)),
            ],
            1,
        );
        let mut copy_ref_contract = copy_ref_mismatch.1;
        copy_ref_contract.value_shapes = vec![one_word(), one_word()];

        let partial_copy = single_function(
            4,
            vec![
                Op::CopyI64 { dst: 16, src: 0 },
                Op::Ret { src: 16, size: 16 },
            ],
            vec![
                structural_region(0, pair_shape.clone(), ValueShapeRef(0)),
                structural_region(16, pair_shape.clone(), ValueShapeRef(0)),
            ],
            1,
        );
        let mut partial_copy_contract = partial_copy.1;
        partial_copy_contract.value_shapes = vec![pair()];

        let array = SchemaRef(1);
        let array_mismatch = (
            Program {
                fns: vec![function(
                    7,
                    vec![
                        Op::ArrayNew {
                            dst: 0,
                            status: 8,
                            count_slot: 16,
                            elem_width: 16,
                            elem_schema_ref: 0,
                        },
                        Op::ArrayStore {
                            status: 8,
                            array: 0,
                            index: 40,
                            src: 24,
                            elem_width: 16,
                            elem_schema_ref: 0,
                        },
                        Op::Ret { src: 16, size: 8 },
                    ],
                )],
            },
            ProgramContract {
                functions: vec![function_contract(
                    7,
                    vec![
                        word_region(0, WordKind::Handle(array)),
                        word_region(8, WordKind::Status),
                        word_region(16, WordKind::Scalar),
                        structural_region(24, pair_shape.clone(), ValueShapeRef(1)),
                        word_region(40, WordKind::Scalar),
                    ],
                    &[],
                    2,
                    None,
                )],
                calls: vec![],
                schemas: vec![
                    structural_schema(pair_shape.clone(), ValueShapeRef(0), PayloadKind::Inline),
                    schema(
                        RegionShape::word(WordKind::Handle(array)),
                        PayloadKind::DenseArray {
                            element: SchemaRef(0),
                        },
                    ),
                ],
                value_shapes: vec![pair(), pair()],
            },
        );

        let cases = vec![
            InvalidCase {
                name: "copy different refs",
                program: copy_ref_mismatch.0,
                contract: copy_ref_contract,
                expected: op_error(
                    0,
                    ProgramDefect::StructuralTransferMismatch {
                        source: Some(ValueShapeRef(0)),
                        destination: Some(ValueShapeRef(1)),
                    },
                ),
            },
            InvalidCase {
                name: "raw multiword structural copy",
                program: partial_copy.0,
                contract: partial_copy_contract,
                expected: op_error(
                    0,
                    ProgramDefect::RawStructuralWordAccess {
                        role: AccessRole::Destination,
                        region: RegionId(1),
                        value_shape: ValueShapeRef(0),
                    },
                ),
            },
            InvalidCase {
                name: "array source different ref",
                program: array_mismatch.0,
                contract: array_mismatch.1,
                expected: op_error(
                    1,
                    ProgramDefect::StructuralTransferMismatch {
                        source: Some(ValueShapeRef(1)),
                        destination: Some(ValueShapeRef(0)),
                    },
                ),
            },
        ];

        for case in cases {
            assert_eq!(
                case.program.verify(case.contract).expect_err(case.name),
                case.expected,
                "{}",
                case.name
            );
        }
    }

    #[test]
    fn rejects_copy_value_for_non_structural_handle_regions() {
        let string_schema = SchemaRef(0);
        let error = Program {
            fns: vec![function(
                2,
                vec![
                    Op::CopyValue {
                        dst: RegionId(1),
                        src: RegionId(0),
                    },
                    Op::Ret { src: 8, size: 8 },
                ],
            )],
        }
        .verify(ProgramContract {
            functions: vec![function_contract(
                2,
                vec![
                    word_region(0, WordKind::Handle(string_schema)),
                    word_region(8, WordKind::Handle(string_schema)),
                ],
                &[],
                1,
                None,
            )],
            calls: vec![],
            schemas: vec![schema(
                RegionShape::word(WordKind::Handle(string_schema)),
                PayloadKind::OpaqueBytes {
                    byte_comparable: true,
                },
            )],
            value_shapes: vec![],
        })
        .expect_err("CopyValue is reserved for complete structural values");
        assert_eq!(
            error,
            op_error(
                0,
                ProgramDefect::StructuralRegionRequiresShape {
                    region: RegionId(0),
                }
            )
        );
    }

    #[test]
    fn rejects_structural_value_shape_table_defects() {
        let scalar = RegionShape::word(WordKind::Scalar);
        let pair_shape = RegionShape::new(vec![kinds(WordKind::Scalar), kinds(WordKind::Scalar)]);
        let string = SchemaRef(0);
        let string_schema = schema(
            RegionShape::word(WordKind::Handle(string)),
            PayloadKind::OpaqueBytes {
                byte_comparable: true,
            },
        );
        let table = |schemas, value_shapes| ProgramContract {
            functions: vec![],
            calls: vec![],
            schemas,
            value_shapes,
        };
        let product = |fields| ValueShapeContract {
            shape: pair_shape.clone(),
            kind: ValueShapeKind::Product { fields },
        };
        let union_shape = RegionShape::new(vec![
            kinds(WordKind::Scalar).allowing(WordKind::Handle(string)),
        ]);
        let enum_shape = |selector, variants| ValueShapeContract {
            shape: pair_shape.clone(),
            kind: ValueShapeKind::Enum { selector, variants },
        };
        let union_enum_shape = RegionShape::new(
            [scalar.words.clone(), union_shape.words.clone()]
                .concat()
                .to_vec(),
        );

        let frame_mismatch = (
            Program {
                fns: vec![function(1, vec![Op::Ret { src: 0, size: 8 }])],
            },
            ProgramContract {
                functions: vec![function_contract(
                    1,
                    vec![structural_region(0, scalar.clone(), ValueShapeRef(0))],
                    &[],
                    0,
                    None,
                )],
                calls: vec![],
                schemas: vec![],
                value_shapes: vec![product(vec![
                    field(0, scalar.clone()),
                    field(8, scalar.clone()),
                ])],
            },
        );

        let cases = vec![
            InvalidCase {
                name: "whole product multiword field without nested ref",
                program: Program::default(),
                contract: table(
                    vec![],
                    vec![ValueShapeContract {
                        shape: pair_shape.clone(),
                        kind: ValueShapeKind::Product {
                            fields: vec![field(0, pair_shape.clone())],
                        },
                    }],
                ),
                expected: global_error(ProgramDefect::ValueShapeFieldRequiresRef {
                    value_shape: ValueShapeRef(0),
                    site: ValueFieldSite::Product { field: 0 },
                    field: pair_shape.clone(),
                }),
            },
            InvalidCase {
                name: "exact union word field without nested ref",
                program: Program::default(),
                contract: table(
                    vec![string_schema.clone()],
                    vec![ValueShapeContract {
                        shape: union_shape.clone(),
                        kind: ValueShapeKind::Product {
                            fields: vec![field(0, union_shape.clone())],
                        },
                    }],
                ),
                expected: global_error(ProgramDefect::ValueShapeFieldRequiresRef {
                    value_shape: ValueShapeRef(0),
                    site: ValueFieldSite::Product { field: 0 },
                    field: union_shape.clone(),
                }),
            },
            InvalidCase {
                name: "enum variant union field without nested ref",
                program: Program::default(),
                contract: table(
                    vec![string_schema.clone()],
                    vec![ValueShapeContract {
                        shape: union_enum_shape.clone(),
                        kind: ValueShapeKind::Enum {
                            selector: selector(0, scalar.clone()),
                            variants: vec![variant(vec![field(8, union_shape.clone())])],
                        },
                    }],
                ),
                expected: global_error(ProgramDefect::ValueShapeFieldRequiresRef {
                    value_shape: ValueShapeRef(0),
                    site: ValueFieldSite::Variant {
                        variant: 0,
                        field: 0,
                    },
                    field: union_shape.clone(),
                }),
            },
            InvalidCase {
                name: "zero word field without nested ref",
                program: Program::default(),
                contract: table(
                    vec![],
                    vec![ValueShapeContract {
                        shape: RegionShape::default(),
                        kind: ValueShapeKind::Product {
                            fields: vec![field(0, RegionShape::default())],
                        },
                    }],
                ),
                expected: global_error(ProgramDefect::ValueShapeFieldRequiresRef {
                    value_shape: ValueShapeRef(0),
                    site: ValueFieldSite::Product { field: 0 },
                    field: RegionShape::default(),
                }),
            },
            InvalidCase {
                name: "product union narrowing",
                program: Program::default(),
                contract: ProgramContract {
                    functions: vec![],
                    calls: vec![],
                    schemas: vec![string_schema.clone()],
                    value_shapes: vec![ValueShapeContract {
                        shape: union_shape.clone(),
                        kind: ValueShapeKind::Product {
                            fields: vec![field(0, scalar.clone())],
                        },
                    }],
                },
                expected: global_error(ProgramDefect::ValueShapeFieldKinds {
                    value_shape: ValueShapeRef(0),
                    site: ValueFieldSite::Product { field: 0 },
                    field: scalar.clone(),
                    parent: union_shape,
                }),
            },
            InvalidCase {
                name: "empty nonunit product",
                program: Program::default(),
                contract: table(vec![], vec![product(vec![])]),
                expected: global_error(ProgramDefect::ValueShapeProductGap {
                    value_shape: ValueShapeRef(0),
                    offset: 0,
                }),
            },
            InvalidCase {
                name: "partial product",
                program: Program::default(),
                contract: table(vec![], vec![product(vec![field(0, scalar.clone())])]),
                expected: global_error(ProgramDefect::ValueShapeProductGap {
                    value_shape: ValueShapeRef(0),
                    offset: 8,
                }),
            },
            InvalidCase {
                name: "gapped product",
                program: Program::default(),
                contract: table(vec![], vec![product(vec![field(8, scalar.clone())])]),
                expected: global_error(ProgramDefect::ValueShapeProductGap {
                    value_shape: ValueShapeRef(0),
                    offset: 0,
                }),
            },
            InvalidCase {
                name: "bad selector kind",
                program: Program::default(),
                contract: table(
                    vec![string_schema.clone()],
                    vec![enum_shape(
                        selector(0, RegionShape::word(WordKind::Handle(string))),
                        vec![variant(vec![field(8, scalar.clone())])],
                    )],
                ),
                expected: global_error(ProgramDefect::ValueShapeSelectorInvalidShape {
                    value_shape: ValueShapeRef(0),
                    shape: RegionShape::word(WordKind::Handle(string)),
                }),
            },
            InvalidCase {
                name: "bad selector offset",
                program: Program::default(),
                contract: table(
                    vec![],
                    vec![enum_shape(
                        selector(4, scalar.clone()),
                        vec![variant(vec![field(8, scalar.clone())])],
                    )],
                ),
                expected: global_error(ProgramDefect::ValueShapeSelectorOffsetNotWordAligned {
                    value_shape: ValueShapeRef(0),
                    offset: 4,
                }),
            },
            InvalidCase {
                name: "field alignment",
                program: Program::default(),
                contract: table(vec![], vec![product(vec![field(4, scalar.clone())])]),
                expected: global_error(ProgramDefect::ValueShapeFieldOffsetNotWordAligned {
                    value_shape: ValueShapeRef(0),
                    site: ValueFieldSite::Product { field: 0 },
                    offset: 4,
                }),
            },
            InvalidCase {
                name: "field range",
                program: Program::default(),
                contract: table(vec![], vec![product(vec![field(8, pair_shape.clone())])]),
                expected: global_error(ProgramDefect::ValueShapeFieldOutOfBounds {
                    value_shape: ValueShapeRef(0),
                    site: ValueFieldSite::Product { field: 0 },
                    end: 24,
                    shape_size: 16,
                }),
            },
            InvalidCase {
                name: "field kind outside flattened shape",
                program: Program::default(),
                contract: table(
                    vec![string_schema.clone()],
                    vec![product(vec![field(
                        0,
                        RegionShape::word(WordKind::Handle(string)),
                    )])],
                ),
                expected: global_error(ProgramDefect::ValueShapeFieldKinds {
                    value_shape: ValueShapeRef(0),
                    site: ValueFieldSite::Product { field: 0 },
                    field: RegionShape::word(WordKind::Handle(string)),
                    parent: scalar.clone(),
                }),
            },
            InvalidCase {
                name: "product field overlap",
                program: Program::default(),
                contract: table(
                    vec![],
                    vec![product(vec![
                        field(0, scalar.clone()),
                        field(0, scalar.clone()),
                    ])],
                ),
                expected: global_error(ProgramDefect::ValueShapeFieldOverlap {
                    value_shape: ValueShapeRef(0),
                    first: ValueFieldSite::Product { field: 0 },
                    second: ValueFieldSite::Product { field: 1 },
                }),
            },
            InvalidCase {
                name: "variant field overlaps selector",
                program: Program::default(),
                contract: table(
                    vec![],
                    vec![enum_shape(
                        selector(0, scalar.clone()),
                        vec![variant(vec![field(0, scalar.clone())])],
                    )],
                ),
                expected: global_error(ProgramDefect::ValueShapeFieldOverlapsSelector {
                    value_shape: ValueShapeRef(0),
                    variant: 0,
                    field: 0,
                }),
            },
            InvalidCase {
                name: "variant field overlap",
                program: Program::default(),
                contract: table(
                    vec![],
                    vec![enum_shape(
                        selector(0, scalar.clone()),
                        vec![variant(vec![
                            field(8, scalar.clone()),
                            field(8, scalar.clone()),
                        ])],
                    )],
                ),
                expected: global_error(ProgramDefect::ValueShapeFieldOverlap {
                    value_shape: ValueShapeRef(0),
                    first: ValueFieldSite::Variant {
                        variant: 0,
                        field: 0,
                    },
                    second: ValueFieldSite::Variant {
                        variant: 0,
                        field: 1,
                    },
                }),
            },
            InvalidCase {
                name: "missing nested structural ref",
                program: Program::default(),
                contract: table(
                    vec![],
                    vec![product(vec![structural_field(
                        0,
                        pair_shape.clone(),
                        ValueShapeRef(1),
                    )])],
                ),
                expected: global_error(ProgramDefect::ValueShapeReferenceOutOfRange {
                    site: ValueShapeReferenceSite::ProductField {
                        value_shape: ValueShapeRef(0),
                        field: 0,
                    },
                    value_shape: ValueShapeRef(1),
                    value_shape_count: 1,
                }),
            },
            InvalidCase {
                name: "frame flattened mismatch",
                program: frame_mismatch.0,
                contract: frame_mismatch.1,
                expected: function_error(ProgramDefect::StructuralShapeMismatch {
                    site: ValueShapeReferenceSite::FrameRegion(RegionId(0)),
                    value_shape: ValueShapeRef(0),
                    expected: pair_shape.clone(),
                    actual: scalar.clone(),
                }),
            },
            InvalidCase {
                name: "inline structural cycle",
                program: Program::default(),
                contract: table(
                    vec![],
                    vec![ValueShapeContract {
                        shape: scalar.clone(),
                        kind: ValueShapeKind::Product {
                            fields: vec![structural_field(0, scalar.clone(), ValueShapeRef(0))],
                        },
                    }],
                ),
                expected: global_error(ProgramDefect::ValueShapeCycle {
                    value_shape: ValueShapeRef(0),
                }),
            },
        ];

        for case in cases {
            assert_eq!(
                case.program.verify(case.contract).expect_err(case.name),
                case.expected,
                "{}",
                case.name
            );
        }
    }

    #[test]
    fn verifies_typed_structural_ops_and_rejects_raw_structural_access() {
        let scalar = RegionShape::word(WordKind::Scalar);
        let pair_shape = RegionShape::new(vec![kinds(WordKind::Scalar), kinds(WordKind::Scalar)]);
        let value_shapes = || {
            vec![
                ValueShapeContract {
                    shape: pair_shape.clone(),
                    kind: ValueShapeKind::Product {
                        fields: vec![field(0, scalar.clone()), field(8, scalar.clone())],
                    },
                },
                ValueShapeContract {
                    shape: scalar.clone(),
                    kind: ValueShapeKind::Product {
                        fields: vec![field(0, scalar.clone())],
                    },
                },
            ]
        };
        let make = |code: Vec<Op>| {
            let (program, mut contract) = single_function(
                7,
                code,
                vec![
                    word_region(0, WordKind::Scalar),
                    word_region(8, WordKind::Scalar),
                    structural_region(16, pair_shape.clone(), ValueShapeRef(0)),
                    structural_region(32, pair_shape.clone(), ValueShapeRef(0)),
                    structural_region(48, scalar.clone(), ValueShapeRef(1)),
                ],
                2,
            );
            contract.value_shapes = value_shapes();
            (program, contract)
        };
        let construct = |fields| Op::ProductConstruct {
            dst: RegionId(2),
            fields,
        };
        let valid_fields = || {
            vec![
                StructuralFieldSource {
                    field: 0,
                    source: RegionId(0),
                },
                StructuralFieldSource {
                    field: 1,
                    source: RegionId(1),
                },
            ]
        };
        let (program, contract) = make(vec![
            construct(valid_fields()),
            Op::Ret { src: 16, size: 16 },
        ]);
        program
            .verify(contract)
            .expect("complete product construction");

        let cases = vec![
            (
                "missing field",
                construct(vec![StructuralFieldSource {
                    field: 0,
                    source: RegionId(0),
                }]),
                ProgramDefect::StructuralFieldCount {
                    value_shape: ValueShapeRef(0),
                    variant: None,
                    expected: 2,
                    actual: 1,
                },
            ),
            (
                "extra field",
                construct(vec![
                    StructuralFieldSource {
                        field: 0,
                        source: RegionId(0),
                    },
                    StructuralFieldSource {
                        field: 1,
                        source: RegionId(1),
                    },
                    StructuralFieldSource {
                        field: 2,
                        source: RegionId(0),
                    },
                ]),
                ProgramDefect::StructuralFieldCount {
                    value_shape: ValueShapeRef(0),
                    variant: None,
                    expected: 2,
                    actual: 3,
                },
            ),
            (
                "duplicate field",
                construct(vec![
                    StructuralFieldSource {
                        field: 0,
                        source: RegionId(0),
                    },
                    StructuralFieldSource {
                        field: 0,
                        source: RegionId(1),
                    },
                ]),
                ProgramDefect::DuplicateStructuralField {
                    value_shape: ValueShapeRef(0),
                    variant: None,
                    field: 0,
                },
            ),
            (
                "wrong field shape",
                construct(vec![
                    StructuralFieldSource {
                        field: 0,
                        source: RegionId(4),
                    },
                    StructuralFieldSource {
                        field: 1,
                        source: RegionId(1),
                    },
                ]),
                ProgramDefect::StructuralFieldSourceMismatch {
                    value_shape: ValueShapeRef(0),
                    variant: None,
                    field: 0,
                    source: RegionId(4),
                    expected_shape: scalar.clone(),
                    actual_shape: scalar.clone(),
                    expected_value_shape: None,
                    actual_value_shape: Some(ValueShapeRef(1)),
                },
            ),
            (
                "wrong region ref",
                Op::CopyValue {
                    dst: RegionId(99),
                    src: RegionId(2),
                },
                ProgramDefect::StructuralRegionOutOfRange {
                    region: RegionId(99),
                    region_count: 5,
                },
            ),
            (
                "raw const",
                Op::ConstI64 { dst: 16, value: 0 },
                ProgramDefect::RawStructuralWordAccess {
                    role: AccessRole::Destination,
                    region: RegionId(2),
                    value_shape: ValueShapeRef(0),
                },
            ),
            (
                "raw float const",
                Op::ConstF64 { dst: 16, bits: 0 },
                ProgramDefect::RawStructuralWordAccess {
                    role: AccessRole::Destination,
                    region: RegionId(2),
                    value_shape: ValueShapeRef(0),
                },
            ),
            (
                "raw copy",
                Op::CopyI64 { dst: 0, src: 16 },
                ProgramDefect::RawStructuralWordAccess {
                    role: AccessRole::Source,
                    region: RegionId(2),
                    value_shape: ValueShapeRef(0),
                },
            ),
            (
                "raw equality",
                Op::EqI64 {
                    dst: 0,
                    a: 16,
                    b: 8,
                },
                ProgramDefect::RawStructuralWordAccess {
                    role: AccessRole::LeftOperand,
                    region: RegionId(2),
                    value_shape: ValueShapeRef(0),
                },
            ),
            (
                "raw branch",
                Op::JumpIfZero {
                    value: 16,
                    target: 1,
                },
                ProgramDefect::RawStructuralWordAccess {
                    role: AccessRole::Condition,
                    region: RegionId(2),
                    value_shape: ValueShapeRef(0),
                },
            ),
        ];
        for (name, op, defect) in cases {
            let (program, contract) = make(vec![op, Op::Ret { src: 16, size: 16 }]);
            assert_eq!(
                program.verify(contract).expect_err(name),
                op_error(0, defect),
                "{name}"
            );
        }
    }

    #[test]
    fn zero_word_structural_ops_use_region_and_shape_identity() {
        let unit = ValueShapeContract {
            shape: RegionShape::default(),
            kind: ValueShapeKind::Product { fields: vec![] },
        };
        let make = |destination_shape| {
            let (program, mut contract) = single_function(
                0,
                vec![
                    Op::ProductConstruct {
                        dst: RegionId(0),
                        fields: vec![],
                    },
                    Op::CopyValue {
                        dst: RegionId(1),
                        src: RegionId(0),
                    },
                    Op::Ret { src: 0, size: 0 },
                ],
                vec![
                    structural_region(0, RegionShape::default(), ValueShapeRef(0)),
                    structural_region(0, RegionShape::default(), destination_shape),
                ],
                1,
            );
            contract.value_shapes = vec![unit.clone(), unit.clone()];
            (program, contract)
        };
        let (program, contract) = make(ValueShapeRef(0));
        program
            .verify(contract)
            .expect("same shape identity at shared zero offset");
        let (program, contract) = make(ValueShapeRef(1));
        assert_eq!(
            program
                .verify(contract)
                .expect_err("different zero-width identity"),
            op_error(
                1,
                ProgramDefect::StructuralTransferMismatch {
                    source: Some(ValueShapeRef(0)),
                    destination: Some(ValueShapeRef(1)),
                }
            )
        );
    }

    #[test]
    fn rejects_invalid_compact_enum_operation_references() {
        let scalar = RegionShape::word(WordKind::Scalar);
        let enum_shape = RegionShape::new(vec![kinds(WordKind::Scalar), kinds(WordKind::Scalar)]);
        let make = |op| {
            let (program, mut contract) = single_function(
                4,
                vec![op, Op::Ret { src: 24, size: 8 }],
                vec![
                    word_region(0, WordKind::Scalar),
                    structural_region(8, enum_shape.clone(), ValueShapeRef(0)),
                    word_region(24, WordKind::Scalar),
                ],
                2,
            );
            contract.value_shapes = vec![ValueShapeContract {
                shape: enum_shape.clone(),
                kind: ValueShapeKind::Enum {
                    selector: selector(0, scalar.clone()),
                    variants: vec![variant(vec![field(8, scalar.clone())])],
                },
            }];
            (program, contract)
        };
        let cases = [
            (
                "construct variant",
                Op::EnumConstruct {
                    dst: RegionId(1),
                    variant: 1,
                    fields: vec![],
                },
                ProgramDefect::EnumVariantOutOfRange {
                    value_shape: ValueShapeRef(0),
                    variant: 1,
                    variant_count: 1,
                },
            ),
            (
                "construct missing field",
                Op::EnumConstruct {
                    dst: RegionId(1),
                    variant: 0,
                    fields: vec![],
                },
                ProgramDefect::StructuralFieldCount {
                    value_shape: ValueShapeRef(0),
                    variant: Some(0),
                    expected: 1,
                    actual: 0,
                },
            ),
            (
                "projection field",
                Op::EnumProjectChecked {
                    dst: RegionId(2),
                    value: RegionId(1),
                    variant: 0,
                    field: 1,
                },
                ProgramDefect::StructuralFieldOutOfRange {
                    value_shape: ValueShapeRef(0),
                    variant: Some(0),
                    field: 1,
                    field_count: 1,
                },
            ),
            (
                "enum region",
                Op::EnumIsVariant {
                    dst: RegionId(2),
                    value: RegionId(99),
                    variant: 0,
                },
                ProgramDefect::StructuralRegionOutOfRange {
                    region: RegionId(99),
                    region_count: 3,
                },
            ),
        ];
        for (name, op, defect) in cases {
            let (program, contract) = make(op);
            assert_eq!(
                program.verify(contract).expect_err(name),
                op_error(0, defect),
                "{name}"
            );
        }
    }

    #[test]
    fn ordered_collection_contract_requires_exact_rows_and_arity() {
        let scalar = RegionShape::word(WordKind::Scalar);
        let row = RegionShape::new(vec![WordKind::Scalar.into(), WordKind::Scalar.into()]);
        let schema = |inline, payload| SchemaContract {
            inline,
            value_shape: None,
            payload,
        };
        let (program, mut contract) = scalar_program();
        contract.schemas = vec![
            schema(scalar.clone(), PayloadKind::Inline),
            schema(scalar.clone(), PayloadKind::Inline),
            schema(row.clone(), PayloadKind::Inline),
            schema(
                RegionShape::word(WordKind::Handle(SchemaRef(3))),
                PayloadKind::OrderedCollection(OrderedCollectionContract {
                    kind: OrderedCollectionKind::Map,
                    key: SchemaRef(0),
                    value: Some(SchemaRef(1)),
                    row: SchemaRef(2),
                    fanout: 8,
                }),
            ),
        ];
        program
            .clone()
            .verify(contract.clone())
            .expect("closed map row contract is admitted");

        let PayloadKind::OrderedCollection(collection) = &mut contract.schemas[3].payload else {
            unreachable!();
        };
        collection.value = None;
        assert_eq!(
            program
                .verify(contract)
                .expect_err("map needs a value schema"),
            ProgramError {
                function: None,
                pc: None,
                defect: ProgramDefect::OrderedCollectionValueArity {
                    schema: SchemaRef(3),
                    kind: OrderedCollectionKind::Map,
                    has_value: false,
                },
            }
        );
    }

    /// The map schema table shared by the ordered-begin-probe admission cases.
    fn ordered_probe_schemas() -> Vec<SchemaContract> {
        let scalar = RegionShape::word(WordKind::Scalar);
        let row = RegionShape::new(vec![WordKind::Scalar.into(), WordKind::Scalar.into()]);
        vec![
            schema(scalar.clone(), PayloadKind::Inline),
            schema(scalar.clone(), PayloadKind::Inline),
            schema(row, PayloadKind::Inline),
            schema(
                RegionShape::word(WordKind::Handle(SchemaRef(3))),
                PayloadKind::OrderedCollection(OrderedCollectionContract {
                    kind: OrderedCollectionKind::Map,
                    key: SchemaRef(0),
                    value: Some(SchemaRef(1)),
                    row: SchemaRef(2),
                    fanout: 4,
                }),
            ),
        ]
    }

    fn ordered_probe_program(
        collection_region: FrameRegion,
        cursor_region: FrameRegion,
        schema_witness: i64,
        entries: &[u32],
    ) -> (Program, ProgramContract) {
        (
            Program {
                fns: vec![function(
                    4,
                    vec![
                        Op::OrderedBeginProbe {
                            cursor: 8,
                            status: 24,
                            collection: 0,
                            collection_schema_ref: schema_witness,
                        },
                        Op::Ret { src: 24, size: 8 },
                    ],
                )],
            },
            ProgramContract {
                functions: vec![function_contract(
                    4,
                    vec![
                        collection_region,
                        cursor_region,
                        word_region(24, WordKind::Status),
                    ],
                    entries,
                    2,
                    None,
                )],
                calls: vec![],
                schemas: ordered_probe_schemas(),
                value_shapes: vec![],
            },
        )
    }

    #[test]
    fn ordered_begin_probe_admits_a_confined_cursor_and_rejects_escape() {
        let handle = || word_region(0, WordKind::Handle(SchemaRef(3)));
        let opaque_cursor = || region(8, RegionShape::new(vec![WordKind::Opaque.into(); 2]));

        // Baseline: a Handle(collection) operand, a two-word opaque cursor, and
        // a status destination, with the cursor confined (not an entry/result).
        let (program, contract) = ordered_probe_program(handle(), opaque_cursor(), 3, &[0]);
        program
            .verify(contract)
            .expect("a confined ordered begin probe is admitted");

        // The cursor destination must be exactly two internal opaque words.
        let (program, contract) = ordered_probe_program(
            handle(),
            region(8, RegionShape::new(vec![WordKind::Scalar.into(); 2])),
            3,
            &[0],
        );
        assert_eq!(
            program.verify(contract).expect_err("cursor must be opaque"),
            op_error(
                0,
                ProgramDefect::OrderedCursorRegionShape {
                    region: RegionId(1),
                    shape: RegionShape::new(vec![WordKind::Scalar.into(); 2]),
                },
            ),
        );

        // The collection handle schema must actually be an ordered collection.
        let (program, contract) = ordered_probe_program(
            word_region(0, WordKind::Handle(SchemaRef(0))),
            opaque_cursor(),
            0,
            &[0],
        );
        assert_eq!(
            program
                .verify(contract)
                .expect_err("scalar schema is not a collection"),
            op_error(
                0,
                ProgramDefect::OrderedCollectionSchemaNotCollection {
                    schema: SchemaRef(0),
                },
            ),
        );

        // The handle schema and the static witness must agree.
        let (program, contract) = ordered_probe_program(
            word_region(0, WordKind::Handle(SchemaRef(0))),
            opaque_cursor(),
            3,
            &[0],
        );
        assert_eq!(
            program
                .verify(contract)
                .expect_err("handle schema disagrees with witness"),
            op_error(
                0,
                ProgramDefect::OrderedCollectionSchemaMismatch {
                    handle: SchemaRef(0),
                    witness: SchemaRef(3),
                },
            ),
        );

        // An opaque cursor may never appear at a function entry: it would escape.
        let (program, contract) = ordered_probe_program(handle(), opaque_cursor(), 3, &[0, 1]);
        assert_eq!(
            program
                .verify(contract)
                .expect_err("opaque cursor cannot be an entry"),
            function_error(ProgramDefect::OpaqueRegionEscapes {
                owner: ShapeOwner::FrameRegion(RegionId(1)),
            }),
        );
    }

    fn ordered_probe_key_program(
        regions: Vec<FrameRegion>,
        key_width: u32,
    ) -> (Program, ProgramContract) {
        (
            Program {
                fns: vec![function(
                    7,
                    vec![
                        Op::OrderedProbeKey {
                            cursor: 0,
                            present: 16,
                            key: 24,
                            left: 32,
                            right: 40,
                            status: 48,
                            key_width,
                            collection_schema_ref: 3,
                        },
                        Op::Ret { src: 48, size: 8 },
                    ],
                )],
            },
            ProgramContract {
                functions: vec![function_contract(7, regions, &[], 5, None)],
                calls: vec![],
                schemas: ordered_probe_schemas(),
                value_shapes: vec![],
            },
        )
    }

    #[test]
    fn ordered_probe_key_types_every_handshake_operand() {
        let regions = || {
            vec![
                region(0, RegionShape::new(vec![WordKind::Opaque.into(); 2])),
                word_region(16, WordKind::Scalar),
                word_region(24, WordKind::Scalar),
                word_region(32, WordKind::Handle(SchemaRef(3))),
                word_region(40, WordKind::Handle(SchemaRef(3))),
                word_region(48, WordKind::Status),
            ]
        };

        // Baseline: opaque cursor source, scalar present, a key matching the key
        // schema, child handles typed to the collection, and a status.
        let (program, contract) = ordered_probe_key_program(regions(), 8);
        program
            .verify(contract)
            .expect("a well-typed probe key is admitted");

        // The key destination width must equal the key schema's inline width.
        let (program, contract) = ordered_probe_key_program(regions(), 16);
        assert_eq!(
            program.verify(contract).expect_err("wrong key width"),
            op_error(
                0,
                ProgramDefect::OrderedKeyWidth {
                    schema: SchemaRef(0),
                    expected: 8,
                    actual: 16,
                },
            ),
        );

        // A child handle must be exactly the collection handle schema.
        let mut wrong_child = regions();
        wrong_child[3] = word_region(32, WordKind::Handle(SchemaRef(0)));
        let (program, contract) = ordered_probe_key_program(wrong_child, 8);
        assert_eq!(
            program.verify(contract).expect_err("wrong child schema"),
            op_error(
                0,
                ProgramDefect::KindMismatch {
                    role: AccessRole::OrderedChildHandle,
                    required: KindRequirement::Handle,
                    allowed: kinds(WordKind::Handle(SchemaRef(0))),
                },
            ),
        );

        // The cursor source must be exactly two internal opaque words.
        let mut wrong_cursor = regions();
        wrong_cursor[0] = region(0, RegionShape::new(vec![WordKind::Scalar.into(); 2]));
        let (program, contract) = ordered_probe_key_program(wrong_cursor, 8);
        assert_eq!(
            program.verify(contract).expect_err("cursor must be opaque"),
            op_error(
                0,
                ProgramDefect::OrderedCursorRegionShape {
                    region: RegionId(0),
                    shape: RegionShape::new(vec![WordKind::Scalar.into(); 2]),
                },
            ),
        );
    }

    fn ordered_map_and_set_schemas() -> Vec<SchemaContract> {
        let scalar = RegionShape::word(WordKind::Scalar);
        let row = RegionShape::new(vec![WordKind::Scalar.into(), WordKind::Scalar.into()]);
        vec![
            schema(scalar.clone(), PayloadKind::Inline),
            schema(scalar.clone(), PayloadKind::Inline),
            schema(row, PayloadKind::Inline),
            schema(
                RegionShape::word(WordKind::Handle(SchemaRef(3))),
                PayloadKind::OrderedCollection(OrderedCollectionContract {
                    kind: OrderedCollectionKind::Map,
                    key: SchemaRef(0),
                    value: Some(SchemaRef(1)),
                    row: SchemaRef(2),
                    fanout: 4,
                }),
            ),
            schema(
                RegionShape::word(WordKind::Handle(SchemaRef(4))),
                PayloadKind::OrderedCollection(OrderedCollectionContract {
                    kind: OrderedCollectionKind::Set,
                    key: SchemaRef(0),
                    value: None,
                    row: SchemaRef(0),
                    fanout: 4,
                }),
            ),
        ]
    }

    fn ordered_probe_value_program(
        value_width: u32,
        collection_schema_ref: i64,
    ) -> (Program, ProgramContract) {
        (
            Program {
                fns: vec![function(
                    5,
                    vec![
                        Op::OrderedProbeValue {
                            cursor: 0,
                            present: 16,
                            value: 24,
                            status: 32,
                            value_width,
                            collection_schema_ref,
                        },
                        Op::Ret { src: 32, size: 8 },
                    ],
                )],
            },
            ProgramContract {
                functions: vec![function_contract(
                    5,
                    vec![
                        region(0, RegionShape::new(vec![WordKind::Opaque.into(); 2])),
                        word_region(16, WordKind::Scalar),
                        word_region(24, WordKind::Scalar),
                        word_region(32, WordKind::Status),
                    ],
                    &[],
                    3,
                    None,
                )],
                calls: vec![],
                schemas: ordered_map_and_set_schemas(),
                value_shapes: vec![],
            },
        )
    }

    #[test]
    fn ordered_probe_value_types_the_value_and_rejects_sets() {
        // Baseline: a Map value projection at the exact value width verifies.
        let (program, contract) = ordered_probe_value_program(8, 3);
        program
            .verify(contract)
            .expect("a well-typed value projection is admitted");

        // A Set collection has no values, so a value projection is a type error.
        let (program, contract) = ordered_probe_value_program(8, 4);
        assert_eq!(
            program.verify(contract).expect_err("value on set"),
            op_error(
                0,
                ProgramDefect::OrderedValueOnSet {
                    schema: SchemaRef(4),
                },
            ),
        );

        // The value destination width must equal the value schema inline width.
        let (program, contract) = ordered_probe_value_program(16, 3);
        assert_eq!(
            program.verify(contract).expect_err("wrong value width"),
            op_error(
                0,
                ProgramDefect::OrderedValueWidth {
                    schema: SchemaRef(1),
                    expected: 8,
                    actual: 16,
                },
            ),
        );
    }

    #[test]
    fn opaque_cursor_words_are_not_copyable() {
        // A raw word copy could duplicate a single-use cursor; both directions
        // of an opaque copy are rejected before the token can be aliased.
        let (program, contract) = single_function(
            2,
            vec![Op::CopyI64 { dst: 8, src: 0 }, Op::Ret { src: 8, size: 8 }],
            vec![
                region(0, RegionShape::word(WordKind::Opaque)),
                word_region(8, WordKind::Scalar),
            ],
            1,
        );
        assert_eq!(
            program.verify(contract).expect_err("opaque source copy"),
            op_error(
                0,
                ProgramDefect::OpaqueWordNotCopyable {
                    role: AccessRole::Source,
                },
            ),
        );
    }
}
