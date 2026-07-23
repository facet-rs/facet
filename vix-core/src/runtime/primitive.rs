use std::collections::BTreeMap;
use std::marker::PhantomData;
use std::sync::{Arc, Mutex, OnceLock};

use crate::schema::{SchemaPattern, SchemaRef};
use crate::vir::Type;

use super::{
    DemandKey, FramedField, FramedNode, FramedValue, ReadObservation, ReadProjection, ReadWitness,
    Receipt, ValueId,
};

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PrimitiveId {
    pub namespace: String,
    pub name: String,
    pub version: u32,
}

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PrimitiveMemoPolicy {
    Hermetic,
    Pinned,
    Observed,
    Volatile,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct PrimitiveDescriptor {
    pub id: PrimitiveId,
    pub request_schema: SchemaPattern,
    pub response_schema: SchemaPattern,
    pub failure_schema: SchemaPattern,
    pub memo_policy: PrimitiveMemoPolicy,
    pub protocol_version: u32,
    /// Minimal declared capability types. FV-E3 enriches these into semantic
    /// admissibility constraints; concrete capabilities are always request
    /// values referenced by `ValueId`.
    pub capability_schemas: Vec<SchemaPattern>,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PrimitiveMachineError {
    Unavailable { detail: String },
    Cancelled,
    Exhausted { detail: String },
    PolicyRejected { detail: String },
    CorruptCandidate { source: ValueId },
    InvalidRequest { request: ValueId },
    AuthorityViolation { detail: String },
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PrimitiveCompletion {
    Ok(ValueId),
    Failed(ValueId),
    MachineError(PrimitiveMachineError),
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct PrimitiveEvent {
    pub schema: SchemaRef,
    pub value: ValueId,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct JournalObservation {
    pub schema: SchemaRef,
    pub value: ValueId,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct ProgressivePublication {
    pub projection: ReadProjection,
    pub value: ValueId,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct ValueBodyCandidate {
    pub claimed: ValueId,
    pub bytes: Vec<u8>,
}

pub trait ValuePersistence: Send + Sync {
    fn get(&self, value: &ValueId) -> Result<Option<ValueBodyCandidate>, PrimitiveMachineError>;
    fn put(&self, value: &ValueId, bytes: &[u8]) -> Result<(), PrimitiveMachineError>;
}

pub trait OriginAdapter: Send + Sync {
    fn read(
        &self,
        capability: &ValueId,
        coordinate: &str,
    ) -> Result<Vec<u8>, PrimitiveMachineError>;
}

/// Runtime-installed services used by registered primitives. These are
/// authorities, not semantic inputs: request values still carry every
/// capability and coordinate that may affect admissibility or identity.
#[derive(Clone, Default)]
pub struct PrimitiveServices {
    value_persistence: Option<Arc<dyn ValuePersistence>>,
    origin: Option<Arc<dyn OriginAdapter>>,
    fixture_store: Option<super::FixtureStore>,
}

impl PrimitiveServices {
    #[must_use]
    pub fn with_value_persistence(mut self, persistence: Arc<dyn ValuePersistence>) -> Self {
        self.value_persistence = Some(persistence);
        self
    }

    #[must_use]
    pub fn with_origin_adapter(mut self, origin: Arc<dyn OriginAdapter>) -> Self {
        self.origin = Some(origin);
        self
    }

    #[must_use]
    pub fn with_fixture_store(mut self, fixture_store: super::FixtureStore) -> Self {
        self.fixture_store = Some(fixture_store);
        self
    }

    pub(crate) fn value_persistence(&self) -> Option<Arc<dyn ValuePersistence>> {
        self.value_persistence.clone()
    }

    pub(crate) fn origin(&self) -> Option<Arc<dyn OriginAdapter>> {
        self.origin.clone()
    }

    pub(crate) fn fixture_store(&self) -> Option<super::FixtureStore> {
        self.fixture_store.clone()
    }
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct PrimitivePublication {
    pub completion: PrimitiveCompletion,
    pub receipt: Receipt,
    pub journal: Vec<JournalObservation>,
    pub progressive: Vec<ProgressivePublication>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WitnessedValue {
    pub identity: ValueId,
    pub bytes: Vec<u8>,
    pub value: PrimitiveValue,
    pub observation: ReadObservation,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct PrimitiveValue {
    pub schema: SchemaRef,
    pub body: PrimitiveValueBody,
}

/// A type-safe handle to a Vix function retained by the runtime for a host
/// primitive. `Req` and `Resp` are part of both the Rust type and the Vix
/// `fn(Req) -> Resp` schema; the numeric token is process-local transport state
/// and is never a callable address.
#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Callback<Req, Resp> {
    token: i64,
    #[facet(skip)]
    marker: PhantomData<fn(Req) -> Resp>,
}

impl<Req, Resp> Callback<Req, Resp> {
    #[doc(hidden)]
    pub fn call_raw(&self, request: PrimitiveValue) -> Result<PrimitiveValue, CallbackError> {
        let transport = callback_transports()
            .lock()
            .expect("callback transport mutex poisoned")
            .get(&self.token)
            .cloned()
            .ok_or(CallbackError::Expired)?;
        transport(request)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallbackError {
    Expired,
    Runtime { detail: String },
    RequestCodec { detail: String },
    ResponseCodec { detail: String },
}

impl core::fmt::Display for CallbackError {
    fn fmt(&self, formatter: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Expired => formatter.write_str("the Vix callback has expired"),
            Self::Runtime { detail } => {
                write!(formatter, "Vix callback execution failed: {detail}")
            }
            Self::RequestCodec { detail } => {
                write!(formatter, "Vix callback request encoding failed: {detail}")
            }
            Self::ResponseCodec { detail } => {
                write!(formatter, "Vix callback response decoding failed: {detail}")
            }
        }
    }
}

impl std::error::Error for CallbackError {}

pub(crate) type CallbackTransport =
    Arc<dyn Fn(PrimitiveValue) -> Result<PrimitiveValue, CallbackError> + Send + Sync>;

fn callback_transports() -> &'static Mutex<BTreeMap<i64, CallbackTransport>> {
    static TRANSPORTS: OnceLock<Mutex<BTreeMap<i64, CallbackTransport>>> = OnceLock::new();
    TRANSPORTS.get_or_init(|| Mutex::new(BTreeMap::new()))
}

pub(crate) fn register_callback_transport(token: i64, transport: CallbackTransport) {
    let previous = callback_transports()
        .lock()
        .expect("callback transport mutex poisoned")
        .insert(token, transport);
    assert!(previous.is_none(), "callback tokens are process-unique");
}

pub(crate) fn unregister_callback_transport(token: i64) {
    callback_transports()
        .lock()
        .expect("callback transport mutex poisoned")
        .remove(&token);
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PrimitiveValueBody {
    Bytes(Vec<u8>),
    Product(Vec<PrimitiveField>),
    Sequence {
        element_schema: SchemaRef,
        elements: Vec<PrimitiveValue>,
    },
    Variant {
        tag: u32,
        fields: Vec<PrimitiveField>,
    },
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct PrimitiveField {
    pub schema: SchemaRef,
    pub value: PrimitiveFieldValue,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PrimitiveFieldValue {
    Inline(Vec<u8>),
    Child(Box<PrimitiveValue>),
}

impl PrimitiveValue {
    #[must_use]
    pub fn bytes(schema: SchemaRef, bytes: Vec<u8>) -> Self {
        Self {
            schema,
            body: PrimitiveValueBody::Bytes(bytes),
        }
    }

    #[must_use]
    pub fn identity(&self) -> ValueId {
        self.framed_node().identity()
    }

    #[must_use]
    pub fn resident_bytes(&self) -> &[u8] {
        match &self.body {
            PrimitiveValueBody::Bytes(bytes) => bytes,
            PrimitiveValueBody::Product(_)
            | PrimitiveValueBody::Sequence { .. }
            | PrimitiveValueBody::Variant { .. } => &[],
        }
    }

    #[must_use]
    pub fn framed_node(&self) -> FramedNode {
        match &self.body {
            PrimitiveValueBody::Bytes(bytes) => {
                FramedNode::leaf(self.schema.clone(), bytes.clone())
            }
            PrimitiveValueBody::Product(fields) => FramedNode::Variant {
                schema: self.schema.clone(),
                tag: 0,
                fields: fields.iter().map(PrimitiveField::framed).collect(),
            },
            PrimitiveValueBody::Sequence {
                element_schema,
                elements,
            } => FramedNode::SeqChildren {
                schema: self.schema.clone(),
                element_schema: element_schema.clone(),
                children: elements.iter().map(PrimitiveValue::identity).collect(),
            },
            PrimitiveValueBody::Variant { tag, fields } => FramedNode::Variant {
                schema: self.schema.clone(),
                tag: u64::from(*tag),
                fields: fields.iter().map(PrimitiveField::framed).collect(),
            },
        }
    }
}

impl PrimitiveField {
    fn framed(&self) -> FramedField {
        FramedField {
            schema: self.schema.clone(),
            value: match &self.value {
                PrimitiveFieldValue::Inline(bytes) => FramedValue::Bytes(bytes.clone()),
                PrimitiveFieldValue::Child(child) => FramedValue::Optional(Some(child.identity())),
            },
        }
    }
}

pub trait EffectAuthority: Send + Sync {
    fn read(
        &self,
        source: &ValueId,
        projection: &ReadProjection,
    ) -> Result<WitnessedValue, PrimitiveMachineError>;

    fn intern(&self, schema: &SchemaRef, bytes: &[u8]) -> Result<ValueId, PrimitiveMachineError>;

    fn intern_value(&self, value: PrimitiveValue) -> Result<ValueId, PrimitiveMachineError> {
        match &value.body {
            PrimitiveValueBody::Bytes(bytes) => self.intern(&value.schema, bytes),
            PrimitiveValueBody::Product(_)
            | PrimitiveValueBody::Sequence { .. }
            | PrimitiveValueBody::Variant { .. } => {
                Err(PrimitiveMachineError::AuthorityViolation {
                    detail: "effect authority does not admit structural values".to_owned(),
                })
            }
        }
    }

    fn emit(&self, event: PrimitiveEvent) -> Result<(), PrimitiveMachineError>;

    fn mint_mount_grant(&self, request: &ValueId) -> Result<ValueId, PrimitiveMachineError>;

    fn type_for_schema(&self, schema: &SchemaRef) -> Result<Type, PrimitiveMachineError> {
        Err(PrimitiveMachineError::AuthorityViolation {
            detail: format!("semantic schema {schema} is not present in this effect snapshot"),
        })
    }

    fn persisted_candidate(
        &self,
        _value: &ValueId,
    ) -> Result<Option<ValueBodyCandidate>, PrimitiveMachineError> {
        Ok(None)
    }

    fn persist_value(&self, _value: &ValueId, _bytes: &[u8]) -> Result<(), PrimitiveMachineError> {
        Ok(())
    }

    fn origin_candidate(
        &self,
        _capability: &ValueId,
        _coordinate: &str,
    ) -> Result<Vec<u8>, PrimitiveMachineError> {
        Err(PrimitiveMachineError::Unavailable {
            detail: "no origin adapter is installed for this effect snapshot".to_owned(),
        })
    }
}

#[derive(Default)]
pub struct StagedEffectAuthority {
    inputs: BTreeMap<ValueId, PrimitiveValue>,
    staged: Mutex<BTreeMap<ValueId, PrimitiveValue>>,
    events: Mutex<Vec<PrimitiveEvent>>,
    schema_types: BTreeMap<SchemaRef, Type>,
    persistence: Option<Arc<dyn ValuePersistence>>,
    origin: Option<Arc<dyn OriginAdapter>>,
    fixture_store: Option<super::FixtureStore>,
}

impl StagedEffectAuthority {
    #[must_use]
    pub fn new(inputs: impl IntoIterator<Item = (ValueId, PrimitiveValue)>) -> Self {
        let mut indexed = BTreeMap::new();
        for (identity, value) in inputs {
            index_primitive_value(&mut indexed, identity, value);
        }
        Self {
            inputs: indexed,
            ..Self::default()
        }
    }

    #[must_use]
    pub fn with_schema_types(mut self, types: impl IntoIterator<Item = (SchemaRef, Type)>) -> Self {
        self.schema_types = types.into_iter().collect();
        self
    }

    #[must_use]
    pub fn with_value_persistence(mut self, persistence: Arc<dyn ValuePersistence>) -> Self {
        self.persistence = Some(persistence);
        self
    }

    #[must_use]
    pub fn with_origin_adapter(mut self, origin: Arc<dyn OriginAdapter>) -> Self {
        self.origin = Some(origin);
        self
    }

    #[must_use]
    pub fn with_fixture_store(mut self, fixture_store: super::FixtureStore) -> Self {
        self.fixture_store = Some(fixture_store);
        self
    }

    #[must_use]
    pub fn staged_value(&self, identity: &ValueId) -> Option<PrimitiveValue> {
        self.staged
            .lock()
            .expect("staged authority mutex poisoned")
            .get(identity)
            .cloned()
    }

    #[must_use]
    pub fn admitted_value(&self, identity: &ValueId) -> Option<PrimitiveValue> {
        self.staged_value(identity)
            .or_else(|| self.inputs.get(identity).cloned())
    }

    #[must_use]
    pub fn events(&self) -> Vec<PrimitiveEvent> {
        self.events
            .lock()
            .expect("staged authority mutex poisoned")
            .clone()
    }
}

fn index_primitive_value(
    indexed: &mut BTreeMap<ValueId, PrimitiveValue>,
    identity: ValueId,
    value: PrimitiveValue,
) {
    match &value.body {
        PrimitiveValueBody::Bytes(_) => {}
        PrimitiveValueBody::Product(fields) | PrimitiveValueBody::Variant { fields, .. } => {
            for field in fields {
                if let PrimitiveFieldValue::Child(child) = &field.value {
                    index_primitive_value(indexed, child.identity(), child.as_ref().clone());
                }
            }
        }
        PrimitiveValueBody::Sequence { elements, .. } => {
            for element in elements {
                index_primitive_value(indexed, element.identity(), element.clone());
            }
        }
    }
    indexed.insert(identity, value);
}

impl EffectAuthority for StagedEffectAuthority {
    fn read(
        &self,
        source: &ValueId,
        projection: &ReadProjection,
    ) -> Result<WitnessedValue, PrimitiveMachineError> {
        let value = if let Some(value) = self.inputs.get(source) {
            value.clone()
        } else {
            self.staged
                .lock()
                .expect("staged authority mutex poisoned")
                .get(source)
                .cloned()
                .ok_or_else(|| PrimitiveMachineError::Unavailable {
                    detail: "staged effect input is absent".to_owned(),
                })?
        };
        if let ReadProjection::TreePath { path } = projection {
            if value.schema != Type::Extern(crate::vir::ExternKind::Host(crate::binding::TREE)).schema_ref() {
                return Err(PrimitiveMachineError::AuthorityViolation {
                    detail: "tree-path read source was not a Tree".to_owned(),
                });
            }
            let bytes = if super::fixture_tree_name(value.resident_bytes()).is_some() {
                self.fixture_store
                    .as_ref()
                    .ok_or_else(|| PrimitiveMachineError::Unavailable {
                        detail: "no fixture store is installed for this effect snapshot".to_owned(),
                    })?
                    .tree_file_bytes(path)
                    .map_err(|_| PrimitiveMachineError::Unavailable {
                        detail: format!("fixture tree path {path} is unavailable"),
                    })?
            } else {
                super::parse_ustar(value.resident_bytes())
                    .map_err(|_| PrimitiveMachineError::InvalidRequest {
                        request: source.clone(),
                    })?
                    .into_iter()
                    .find_map(|member| match member {
                        super::TarMember::File {
                            path: candidate,
                            bytes,
                            ..
                        } if candidate == *path => Some(bytes),
                        _ => None,
                    })
                    .ok_or_else(|| PrimitiveMachineError::Unavailable {
                        detail: format!("archive tree path {path} is unavailable"),
                    })?
            };
            let value = PrimitiveValue::bytes(Type::String.schema_ref(), bytes.clone());
            let identity = value.identity();
            return Ok(WitnessedValue {
                identity: identity.clone(),
                bytes,
                value,
                observation: ReadObservation::Value(identity),
            });
        }
        if !matches!(projection, ReadProjection::Whole) {
            return Err(PrimitiveMachineError::AuthorityViolation {
                detail: "staged authority does not admit this projected read".to_owned(),
            });
        }
        Ok(WitnessedValue {
            identity: source.clone(),
            bytes: value.resident_bytes().to_vec(),
            value,
            observation: ReadObservation::Value(source.clone()),
        })
    }

    fn intern(&self, schema: &SchemaRef, bytes: &[u8]) -> Result<ValueId, PrimitiveMachineError> {
        let identity = super::FramedNode::leaf(schema.clone(), bytes.to_vec()).identity();
        self.staged
            .lock()
            .expect("staged authority mutex poisoned")
            .insert(
                identity.clone(),
                PrimitiveValue::bytes(schema.clone(), bytes.to_vec()),
            );
        Ok(identity)
    }

    fn intern_value(&self, value: PrimitiveValue) -> Result<ValueId, PrimitiveMachineError> {
        let identity = value.identity();
        self.staged
            .lock()
            .expect("staged authority mutex poisoned")
            .insert(identity.clone(), value);
        Ok(identity)
    }

    fn emit(&self, event: PrimitiveEvent) -> Result<(), PrimitiveMachineError> {
        self.events
            .lock()
            .expect("staged authority mutex poisoned")
            .push(event);
        Ok(())
    }

    fn mint_mount_grant(&self, request: &ValueId) -> Result<ValueId, PrimitiveMachineError> {
        Ok(request.clone())
    }

    fn type_for_schema(&self, schema: &SchemaRef) -> Result<Type, PrimitiveMachineError> {
        self.schema_types.get(schema).cloned().ok_or_else(|| {
            PrimitiveMachineError::AuthorityViolation {
                detail: format!("semantic schema {schema} is absent from the effect snapshot"),
            }
        })
    }

    fn persisted_candidate(
        &self,
        value: &ValueId,
    ) -> Result<Option<ValueBodyCandidate>, PrimitiveMachineError> {
        self.persistence
            .as_ref()
            .map_or(Ok(None), |persistence| persistence.get(value))
    }

    fn persist_value(&self, value: &ValueId, bytes: &[u8]) -> Result<(), PrimitiveMachineError> {
        self.persistence
            .as_ref()
            .map_or(Ok(()), |persistence| persistence.put(value, bytes))
    }

    fn origin_candidate(
        &self,
        capability: &ValueId,
        coordinate: &str,
    ) -> Result<Vec<u8>, PrimitiveMachineError> {
        self.origin
            .as_ref()
            .ok_or_else(|| PrimitiveMachineError::Unavailable {
                detail: "no origin adapter is installed for this effect snapshot".to_owned(),
            })?
            .read(capability, coordinate)
    }
}

#[derive(Clone)]
pub struct EffectCtx {
    demand: DemandKey,
    authority: Arc<dyn EffectAuthority>,
    transaction: Arc<Mutex<EffectTransaction>>,
}

#[derive(Default)]
struct EffectTransaction {
    reads: Vec<ReadWitness>,
    journal: Vec<JournalObservation>,
    progressive: Vec<ProgressivePublication>,
    completed: bool,
}

impl EffectCtx {
    #[must_use]
    pub fn new(demand: DemandKey, authority: Arc<dyn EffectAuthority>) -> Self {
        Self {
            demand,
            authority,
            transaction: Arc::new(Mutex::new(EffectTransaction::default())),
        }
    }

    #[must_use]
    pub fn demand(&self) -> DemandKey {
        self.demand
    }

    pub fn read(
        &self,
        source: &ValueId,
        projection: ReadProjection,
    ) -> Result<WitnessedValue, PrimitiveMachineError> {
        let witnessed = self.authority.read(source, &projection)?;
        self.transaction
            .lock()
            .expect("effect transaction mutex poisoned")
            .reads
            .push(ReadWitness {
                source: source.clone(),
                projection,
                observation: witnessed.observation.clone(),
            });
        Ok(witnessed)
    }

    pub fn intern(
        &self,
        schema: &SchemaRef,
        bytes: &[u8],
    ) -> Result<ValueId, PrimitiveMachineError> {
        self.authority.intern(schema, bytes)
    }

    pub fn intern_value(&self, value: PrimitiveValue) -> Result<ValueId, PrimitiveMachineError> {
        self.authority.intern_value(value)
    }

    pub fn emit(&self, event: PrimitiveEvent) -> Result<(), PrimitiveMachineError> {
        self.authority.emit(event)
    }

    pub fn mint_mount_grant(&self, request: &ValueId) -> Result<ValueId, PrimitiveMachineError> {
        self.authority.mint_mount_grant(request)
    }

    pub fn type_for_schema(&self, schema: &SchemaRef) -> Result<Type, PrimitiveMachineError> {
        self.authority.type_for_schema(schema)
    }

    pub fn persisted_candidate(
        &self,
        value: &ValueId,
    ) -> Result<Option<ValueBodyCandidate>, PrimitiveMachineError> {
        self.authority.persisted_candidate(value)
    }

    pub fn persist_value(
        &self,
        value: &ValueId,
        bytes: &[u8],
    ) -> Result<(), PrimitiveMachineError> {
        self.authority.persist_value(value, bytes)
    }

    pub fn origin_candidate(
        &self,
        capability: &ValueId,
        coordinate: &str,
        expected: &ValueId,
    ) -> Result<Vec<u8>, PrimitiveMachineError> {
        let bytes = self.authority.origin_candidate(capability, coordinate)?;
        let observed = FramedNode::leaf(expected.schema.clone(), bytes.clone()).identity();
        if &observed != expected {
            return Err(PrimitiveMachineError::CorruptCandidate { source: observed });
        }
        self.transaction
            .lock()
            .expect("effect transaction mutex poisoned")
            .reads
            .push(ReadWitness {
                source: capability.clone(),
                projection: ReadProjection::Origin {
                    coordinate: coordinate.to_owned(),
                },
                observation: ReadObservation::Value(observed),
            });
        Ok(bytes)
    }

    pub fn observe(&self, observation: JournalObservation) {
        self.transaction
            .lock()
            .expect("effect transaction mutex poisoned")
            .journal
            .push(observation);
    }

    pub fn publish_progress(&self, publication: ProgressivePublication) {
        self.transaction
            .lock()
            .expect("effect transaction mutex poisoned")
            .progressive
            .push(publication);
    }

    pub fn finish(
        &self,
        completion: PrimitiveCompletion,
    ) -> Result<PrimitivePublication, PrimitiveMachineError> {
        let mut transaction = self
            .transaction
            .lock()
            .expect("effect transaction mutex poisoned");
        if transaction.completed {
            return Err(PrimitiveMachineError::AuthorityViolation {
                detail: "primitive attempted more than one completion transaction".to_owned(),
            });
        }
        transaction.completed = true;
        Ok(PrimitivePublication {
            completion,
            receipt: Receipt {
                demand: self.demand,
                reads: std::mem::take(&mut transaction.reads),
            },
            journal: std::mem::take(&mut transaction.journal),
            progressive: std::mem::take(&mut transaction.progressive),
        })
    }

    pub fn ticket(
        &self,
        cancel: impl FnOnce() + Send + 'static,
    ) -> (RawEffectTicket, RawEffectCompleter) {
        RawEffectTicket::pair(self.demand, cancel)
    }
}

pub trait FromRef<Ctx> {
    fn from_ref(ctx: &Ctx) -> Self;
}

/// A primitive that needs nothing from the embedder declares `type Deps = ()`
/// and stays agnostic over `Ctx`: the empty slice is projectable out of *any*
/// context, so a bare `Runtime<S, ()>` and a richly-provisioned one both admit
/// `fetch`/`observe`. This is deliberately a generic `impl … for ()` rather than
/// the reflexive `impl<T: Clone> FromRef<T> for T` (whole-context-as-its-own-dep):
/// the two overlap at `Ctx = ()`, and it is `()`-deps agnosticism the built-in
/// primitives actually rely on. Concrete slices (`PgPool`, a fixture store, …)
/// name their own `impl FromRef<Ctx>` per embedder context, the way the
/// `from_ref_tests` `FakePool` does.
impl<Ctx> FromRef<Ctx> for () {
    fn from_ref(_ctx: &Ctx) {}
}

/// The structural role a surface argument plays in a primitive's request record.
/// The request record has one field per argument, in this order.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ArgRole {
    /// Lowered as an ordinary value and required to have the given type
    /// (`fetch`'s `PinnedBlobRef`).
    Value { expected: Type },
}

/// How a registered primitive builds its request from its surface arguments — the
/// data a single generic lowering step consumes in place of a bespoke Rust arm per
/// primitive. Arity is `args.len()`; the compiler builds a `request_ty` record with
/// one field per argument (in order), invokes `primitive`, and yields `result`.
///
/// Only the primitives whose construction is *fully uniform* declare one
/// ([`RawPrimitive::request_shape`]) today (`fetch`, `observe`). `decode`/
/// `try_decode` (compile-time constant folding, expected-type-derived targets)
/// are not yet expressible here and stay on the `None` default.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RequestShape {
    pub args: Vec<ArgRole>,
    pub request_ty: Type,
    pub result: Type,
    pub primitive: PrimitiveId,
}

pub trait RawPrimitive<Ctx>: Send + Sync {
    fn descriptor(&self) -> &PrimitiveDescriptor;
    /// `app` is the whole shared embedder context; the impl projects the
    /// slice it needs out of it via [`FromRef`].
    fn begin(&self, request: ValueId, ctx: EffectCtx, app: &Ctx) -> RawEffectTicket;

    /// The primitive's surface name in the vix prelude, or `None` if it
    /// projects no surface binding at all (e.g. `TreeReadPrimitive`, reached
    /// only through the `.text()` method surface, never by a free-function
    /// call). A primitive with `Some` name here is exactly the primitives
    /// `binding::builtin_bindings` harvests one prelude binding from.
    fn surface_name(&self) -> Option<&'static str> {
        None
    }

    /// The [`RequestShape`] this primitive's surface call lowers through, or
    /// `None` when request construction is not yet fully data (selector reads
    /// and expected-type-derived targets that don't reduce to a plain record
    /// shape). Returning `Some` is the contract that the compiler can build
    /// this primitive's request generically, with no bespoke Rust arm.
    fn request_shape(&self) -> Option<RequestShape> {
        None
    }
}

type TicketWaiter = Box<dyn FnOnce(PrimitivePublication) + Send + 'static>;

struct TicketState {
    outcome: Option<PrimitivePublication>,
    waiters: BTreeMap<u64, TicketWaiter>,
    next_waiter: u64,
    lease_generation: u64,
    cancelled: bool,
    cancel: Option<Box<dyn FnOnce() + Send + 'static>>,
}

struct TicketShared {
    demand: DemandKey,
    state: Mutex<TicketState>,
}

#[derive(Clone)]
pub struct RawEffectTicket {
    shared: Arc<TicketShared>,
}

pub struct RawEffectCompleter {
    shared: Arc<TicketShared>,
}

pub struct TicketSubscription {
    shared: Arc<TicketShared>,
    waiter: Option<u64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TicketCompletionError {
    AlreadyCompleted,
    Cancelled,
}

impl RawEffectTicket {
    fn pair(
        demand: DemandKey,
        cancel: impl FnOnce() + Send + 'static,
    ) -> (Self, RawEffectCompleter) {
        let shared = Arc::new(TicketShared {
            demand,
            state: Mutex::new(TicketState {
                outcome: None,
                waiters: BTreeMap::new(),
                next_waiter: 0,
                lease_generation: 0,
                cancelled: false,
                cancel: Some(Box::new(cancel)),
            }),
        });
        (
            Self {
                shared: shared.clone(),
            },
            RawEffectCompleter { shared },
        )
    }

    #[must_use]
    pub fn demand(&self) -> DemandKey {
        self.shared.demand
    }

    pub fn renew_lease(&self) -> u64 {
        let mut state = self.shared.state.lock().expect("ticket mutex poisoned");
        state.lease_generation = state.lease_generation.wrapping_add(1);
        state.lease_generation
    }

    pub fn join(
        &self,
        waiter: impl FnOnce(PrimitivePublication) + Send + 'static,
    ) -> TicketSubscription {
        let mut waiter = Some(Box::new(waiter) as TicketWaiter);
        let mut state = self.shared.state.lock().expect("ticket mutex poisoned");
        if let Some(outcome) = state.outcome.clone() {
            drop(state);
            waiter.take().expect("waiter exists")(outcome);
            return TicketSubscription {
                shared: self.shared.clone(),
                waiter: None,
            };
        }
        if state.cancelled {
            return TicketSubscription {
                shared: self.shared.clone(),
                waiter: None,
            };
        }
        let id = state.next_waiter;
        state.next_waiter = state.next_waiter.wrapping_add(1);
        state
            .waiters
            .insert(id, waiter.take().expect("waiter exists"));
        TicketSubscription {
            shared: self.shared.clone(),
            waiter: Some(id),
        }
    }

    #[must_use]
    pub fn outcome(&self) -> Option<PrimitivePublication> {
        self.shared
            .state
            .lock()
            .expect("ticket mutex poisoned")
            .outcome
            .clone()
    }

    pub fn cancel_demand(&self) -> bool {
        let cancel = {
            let mut state = self.shared.state.lock().expect("ticket mutex poisoned");
            if state.cancelled || state.outcome.is_some() {
                return false;
            }
            state.cancelled = true;
            state.waiters.clear();
            state.cancel.take()
        };
        if let Some(cancel) = cancel {
            cancel();
        }
        true
    }
}

impl RawEffectCompleter {
    pub fn complete(self, outcome: PrimitivePublication) -> Result<(), TicketCompletionError> {
        let waiters = {
            let mut state = self.shared.state.lock().expect("ticket mutex poisoned");
            if state.cancelled {
                return Err(TicketCompletionError::Cancelled);
            }
            if state.outcome.is_some() {
                return Err(TicketCompletionError::AlreadyCompleted);
            }
            state.outcome = Some(outcome.clone());
            std::mem::take(&mut state.waiters)
        };
        for (_, waiter) in waiters {
            waiter(outcome.clone());
        }
        Ok(())
    }
}

impl Drop for TicketSubscription {
    fn drop(&mut self) {
        if let Some(waiter) = self.waiter.take() {
            self.shared
                .state
                .lock()
                .expect("ticket mutex poisoned")
                .waiters
                .remove(&waiter);
        }
    }
}

pub struct PrimitiveRegistry<Ctx> {
    primitives: BTreeMap<PrimitiveId, Arc<dyn RawPrimitive<Ctx>>>,
}

impl<Ctx> Default for PrimitiveRegistry<Ctx> {
    fn default() -> Self {
        Self {
            primitives: BTreeMap::new(),
        }
    }
}

pub struct PrimitiveDispatcher<Ctx> {
    registry: Arc<PrimitiveRegistry<Ctx>>,
    in_flight: Mutex<BTreeMap<DemandKey, RawEffectTicket>>,
}

impl<Ctx> PrimitiveDispatcher<Ctx> {
    #[must_use]
    pub fn new(registry: Arc<PrimitiveRegistry<Ctx>>) -> Self {
        Self {
            registry,
            in_flight: Mutex::new(BTreeMap::new()),
        }
    }

    /// A dispatcher with no primitives registered — the bare language performs no
    /// effects. `vix-core` constructs every runtime with this; the `vixen`
    /// runtime replaces it (via [`crate::runtime::Runtime::set_primitive_dispatcher`])
    /// with one carrying the builtin primitives.
    #[must_use]
    pub fn empty() -> Self {
        Self::new(Arc::new(PrimitiveRegistry::default()))
    }

    pub fn begin_or_join(
        &self,
        id: &PrimitiveId,
        request: ValueId,
        ctx: EffectCtx,
        app: &Ctx,
    ) -> Result<RawEffectTicket, Box<PrimitiveDispatchError>> {
        let demand = ctx.demand();
        let mut in_flight = self.in_flight.lock().expect("dispatcher mutex poisoned");
        if let Some(ticket) = in_flight.get(&demand) {
            return Ok(ticket.clone());
        }
        let ticket = self.registry.begin(id, request, ctx, app)?;
        in_flight.insert(demand, ticket.clone());
        Ok(ticket)
    }

    #[must_use]
    pub fn descriptor(&self, id: &PrimitiveId) -> Option<&PrimitiveDescriptor> {
        self.registry.descriptor(id)
    }

    pub fn retire(&self, demand: DemandKey) -> Option<RawEffectTicket> {
        self.in_flight
            .lock()
            .expect("dispatcher mutex poisoned")
            .remove(&demand)
    }

    #[must_use]
    pub fn in_flight(&self) -> usize {
        self.in_flight
            .lock()
            .expect("dispatcher mutex poisoned")
            .len()
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrimitiveRegistrationError {
    Duplicate(PrimitiveId),
}

impl<Ctx> PrimitiveRegistry<Ctx> {
    pub fn register(
        &mut self,
        primitive: Arc<dyn RawPrimitive<Ctx>>,
    ) -> Result<(), PrimitiveRegistrationError> {
        let id = primitive.descriptor().id.clone();
        if self.primitives.insert(id.clone(), primitive).is_some() {
            return Err(PrimitiveRegistrationError::Duplicate(id));
        }
        Ok(())
    }

    #[must_use]
    pub fn descriptor(&self, id: &PrimitiveId) -> Option<&PrimitiveDescriptor> {
        self.primitives
            .get(id)
            .map(|primitive| primitive.descriptor())
    }

    pub fn begin(
        &self,
        id: &PrimitiveId,
        request: ValueId,
        ctx: EffectCtx,
        app: &Ctx,
    ) -> Result<RawEffectTicket, Box<PrimitiveDispatchError>> {
        let primitive = self
            .primitives
            .get(id)
            .ok_or_else(|| Box::new(PrimitiveDispatchError::Unregistered(id.clone())))?;
        if !primitive
            .descriptor()
            .request_schema
            .matches(&request.schema)
        {
            return Err(Box::new(PrimitiveDispatchError::RequestSchema {
                primitive: id.clone(),
                expected: primitive.descriptor().request_schema.clone(),
                found: request.schema,
            }));
        }
        Ok(primitive.begin(request, ctx, app))
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PrimitiveDispatchError {
    Unregistered(PrimitiveId),
    RequestSchema {
        primitive: PrimitiveId,
        expected: SchemaPattern,
        found: SchemaRef,
    },
}

#[cfg(test)]
mod from_ref_tests {
    use super::*;
    use crate::runtime::{DemandPreimage, RecipeId};

    /// A stand-in for a shared authority an embedder installs once — a DB
    /// pool, say — and reuses across every primitive invocation.
    #[derive(Clone)]
    struct FakePool {
        label: &'static str,
    }

    /// The embedder's application context: an ordinary struct assembling
    /// whatever shared authorities it wants primitives to reach.
    #[derive(Clone)]
    struct AppCtx {
        pool: FakePool,
    }

    impl FromRef<AppCtx> for FakePool {
        fn from_ref(ctx: &AppCtx) -> FakePool {
            ctx.pool.clone()
        }
    }

    /// A primitive that names exactly the slice it needs; a missing `FakePool`
    /// on `Ctx` would be a compile error here, not a runtime downcast.
    struct PoolLabelPrimitive {
        descriptor: PrimitiveDescriptor,
        seen: Arc<Mutex<Option<&'static str>>>,
    }

    impl<Ctx> RawPrimitive<Ctx> for PoolLabelPrimitive
    where
        FakePool: FromRef<Ctx>,
    {
        fn descriptor(&self) -> &PrimitiveDescriptor {
            &self.descriptor
        }

        fn begin(&self, request: ValueId, ctx: EffectCtx, app: &Ctx) -> RawEffectTicket {
            let pool = FakePool::from_ref(app);
            *self.seen.lock().expect("seen mutex poisoned") = Some(pool.label);
            let (ticket, completer) = ctx.ticket(|| {});
            let publication = ctx
                .finish(PrimitiveCompletion::Ok(request))
                .expect("single completion transaction");
            completer
                .complete(publication)
                .expect("fresh ticket accepts one completion");
            ticket
        }
    }

    fn descriptor() -> PrimitiveDescriptor {
        PrimitiveDescriptor {
            id: PrimitiveId {
                namespace: "vix.test".to_owned(),
                name: "pool-label".to_owned(),
                version: 1,
            },
            request_schema: SchemaPattern::exact(&Type::String.schema_ref()),
            response_schema: SchemaPattern::exact(&Type::String.schema_ref()),
            failure_schema: SchemaPattern::exact(&Type::String.schema_ref()),
            memo_policy: PrimitiveMemoPolicy::Hermetic,
            protocol_version: 1,
            capability_schemas: Vec::new(),
        }
    }

    #[test]
    fn primitive_projects_its_dependency_out_of_the_shared_context_via_from_ref() {
        let request = FramedNode::leaf(Type::String.schema_ref(), b"ignored".to_vec()).identity();
        let demand = DemandKey::from_preimage(&DemandPreimage {
            closure: RecipeId::from_canonical_vir(b"from-ref-test"),
            arguments: vec![request.clone()],
        });
        let authority = Arc::new(StagedEffectAuthority::new(std::iter::empty()));
        let ctx = EffectCtx::new(demand, authority);

        let primitive = Arc::new(PoolLabelPrimitive {
            descriptor: descriptor(),
            seen: Arc::new(Mutex::new(None)),
        });
        let mut registry = PrimitiveRegistry::default();
        registry
            .register(primitive.clone())
            .expect("primitive registers once");
        let dispatcher = PrimitiveDispatcher::new(Arc::new(registry));

        let app = AppCtx {
            pool: FakePool { label: "prod-pool" },
        };
        let ticket = dispatcher
            .begin_or_join(&primitive.descriptor.id, request, ctx, &app)
            .expect("registered primitive dispatches");
        ticket.outcome().expect("immediate primitive completed");

        assert_eq!(
            *primitive.seen.lock().expect("seen mutex poisoned"),
            Some("prod-pool")
        );
    }
}
