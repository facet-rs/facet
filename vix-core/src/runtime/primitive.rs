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
    /// The origin taxonomy's miss: a coordinate or tree projection found
    /// nothing. Distinct from [`Self::Unavailable`] because a miss is an
    /// *observation* — a multi-origin fetch may fall through it, and
    /// [`EffectCtx`] witnesses it as `ReadObservation::Missing`.
    ///
    /// r[impl machine.primitive.origin-verbs]
    ProjectionMissing { detail: String },
    /// A tree projection found the entry with a kind contradicting the
    /// request (a file read that found a directory). Not a miss: the entry
    /// exists, and the audit must be able to tell "appeared" from "changed
    /// kind", so [`EffectCtx`] witnesses the found kind.
    ///
    /// r[impl machine.primitive.origin-verbs]
    ProjectionWrongKind {
        found: super::TreeEntryKind,
        detail: String,
    },
    /// The origin taxonomy's refusal: no installed declaration serves the
    /// request. Loud, typed, and it names what was asked and what is
    /// installed — the anti-conjuring answer. Never witnessed: nobody looked.
    ///
    /// r[impl machine.primitive.origin-routing]
    OriginUnroutable { detail: String },
    /// The origin taxonomy's corruption: a backend served something it knows
    /// is wrong. Stops the demand; never falls through.
    ///
    /// r[impl machine.primitive.origin-verbs]
    OriginCorrupt { detail: String },
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum PrimitiveCompletion {
    Ok(ValueId),
    Failed(ValueId),
    /// A process termination the effect's termination grammar mapped to a typed
    /// language failure rather than an answer. The grammar itself is capability
    /// data enacted by the primitive; only the already-mapped raw termination
    /// crosses this boundary, and the scheduler interns it as the core
    /// `FailureValue::ProcessFailure` vocabulary — no naked status integer ever
    /// becomes a value (`machine.primitive.exit-status-is-not-a-value`).
    /// `diagnostic` is the failing process's captured stderr, retained as the
    /// failure's diagnostic payload exactly as the machine-op path retained it.
    ProcessFailed {
        termination: super::ProcessTermination,
        diagnostic: Vec<u8>,
    },
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

/// Runtime-installed services used by registered primitives. These are
/// authorities, not semantic inputs: request values still carry every
/// capability and coordinate that may affect admissibility or identity.
#[derive(Clone, Default)]
pub struct PrimitiveServices {
    value_persistence: Option<Arc<dyn ValuePersistence>>,
    origins: super::OriginAdapterSet,
    exec_backend: Option<Arc<dyn super::ExecBackend>>,
}

impl PrimitiveServices {
    #[must_use]
    pub fn with_value_persistence(mut self, persistence: Arc<dyn ValuePersistence>) -> Self {
        self.value_persistence = Some(persistence);
        self
    }

    /// Install one origin adapter under its declaration
    /// (`machine.primitive.origin-routing`). Overlapping declarations — a
    /// scheme claimed twice, prefix-related tree namespaces — are rejected
    /// here, at install time, so routing stays a function of the declared set.
    ///
    /// r[impl machine.primitive.origin-routing]
    pub fn with_origin(
        mut self,
        decl: super::OriginAdapterDecl,
        adapter: Arc<dyn super::OriginAdapter>,
    ) -> Result<Self, super::OriginInstallError> {
        self.origins.install(decl, adapter)?;
        Ok(self)
    }

    /// Install the exec process-boundary service
    /// (`machine.primitive.effect-backend-service`). Nothing keys on the
    /// backend: swapping it never re-keys a demand, it changes what a witness
    /// is worth.
    #[must_use]
    pub fn with_exec_backend(mut self, backend: Arc<dyn super::ExecBackend>) -> Self {
        self.exec_backend = Some(backend);
        self
    }

    pub(crate) fn value_persistence(&self) -> Option<Arc<dyn ValuePersistence>> {
        self.value_persistence.clone()
    }

    pub(crate) fn origins(&self) -> super::OriginAdapterSet {
        self.origins.clone()
    }

    /// The installed exec backend, or the host-trusting default — the current
    /// behavior verbatim, whose receipts carry `Unverifiable` capability
    /// witnesses (`machine.primitive.memo-policy`).
    pub(crate) fn exec_backend(&self) -> Arc<dyn super::ExecBackend> {
        self.exec_backend
            .clone()
            .unwrap_or_else(|| Arc::new(super::HostExecBackend))
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
    /// Canonical key-ordered map rows, mirroring [`FramedNode::OrderedMap`]:
    /// both halves of every row contribute their semantic identities, so a
    /// primitive can stage a map-bearing response (an exec outcome's line map)
    /// without the store learning any new identity rule.
    OrderedMap(Vec<(PrimitiveValue, PrimitiveValue)>),
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
            | PrimitiveValueBody::Variant { .. }
            | PrimitiveValueBody::OrderedMap(_) => &[],
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
            PrimitiveValueBody::OrderedMap(rows) => FramedNode::OrderedMap {
                schema: self.schema.clone(),
                rows: rows
                    .iter()
                    .map(|(key, value)| (key.identity(), value.identity()))
                    .collect(),
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
            | PrimitiveValueBody::Variant { .. }
            | PrimitiveValueBody::OrderedMap(_) => {
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

    /// The refusal is loud and names the coordinate: the machine holds no
    /// default origin backend, so an unconfigured origin never falls back to
    /// anything — it says what was asked and that nothing is installed.
    /// r[impl machine.primitive.origin-routing]
    fn origin_candidate(
        &self,
        _capability: &ValueId,
        coordinate: &str,
    ) -> Result<Vec<u8>, PrimitiveMachineError> {
        Err(PrimitiveMachineError::OriginUnroutable {
            detail: format!(
                "origin read {coordinate} refused: no origin adapter is installed \
                 for this effect snapshot"
            ),
        })
    }

    /// The process-boundary service installed for this effect snapshot
    /// (`machine.primitive.effect-backend-service`). The backend is an
    /// authority, never a semantic input: nothing keys on it, and an authority
    /// without one installed simply cannot cross the process boundary.
    fn exec_backend(&self) -> Option<Arc<dyn super::ExecBackend>> {
        None
    }
}

#[derive(Default)]
pub struct StagedEffectAuthority {
    inputs: BTreeMap<ValueId, PrimitiveValue>,
    staged: Mutex<BTreeMap<ValueId, PrimitiveValue>>,
    events: Mutex<Vec<PrimitiveEvent>>,
    schema_types: BTreeMap<SchemaRef, Type>,
    persistence: Option<Arc<dyn ValuePersistence>>,
    origins: super::OriginAdapterSet,
    exec_backend: Option<Arc<dyn super::ExecBackend>>,
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

    /// Install the declared origin adapter set this snapshot routes through
    /// (`machine.primitive.origin-routing`).
    #[must_use]
    pub fn with_origins(mut self, origins: super::OriginAdapterSet) -> Self {
        self.origins = origins;
        self
    }

    /// Install the process-boundary service for this snapshot
    /// (`machine.primitive.effect-backend-service`).
    #[must_use]
    pub fn with_exec_backend(mut self, backend: Arc<dyn super::ExecBackend>) -> Self {
        self.exec_backend = Some(backend);
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
        PrimitiveValueBody::OrderedMap(rows) => {
            for (key, entry) in rows {
                index_primitive_value(indexed, key.identity(), key.clone());
                index_primitive_value(indexed, entry.identity(), entry.clone());
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
            if value.schema
                != Type::Extern(crate::vir::ExternKind::Host(crate::binding::TREE)).schema_ref()
            {
                return Err(PrimitiveMachineError::AuthorityViolation {
                    detail: "tree-path read source was not a Tree".to_owned(),
                });
            }
            // Route by the source's resident bytes: a content-identified tree
            // carries its own members; a lazily-backed handle routes to the
            // adapter whose declared namespace owns it; anything else is the
            // loud refusal. r[impl machine.primitive.origin-routing]
            let bytes = match self.origins.route_tree(value.resident_bytes()) {
                super::TreeRouting::ContentIdentified => {
                    // Through the semantic Tree rather than the archive
                    // reader: a Tree's resident bytes may be an archive, a
                    // carrier, or the canonical form, and which one it is is
                    // a storage concern this read has no business knowing.
                    let tree = super::tree_from_resident(value.resident_bytes()).map_err(|_| {
                        PrimitiveMachineError::InvalidRequest {
                            request: source.clone(),
                        }
                    })?;
                    match tree.project(path) {
                        Some(super::TreeEntry::File { content, .. }) => {
                            content.as_bytes().to_vec()
                        }
                        Some(entry) => {
                            let found = match entry {
                                super::TreeEntry::File { .. } => super::TreeEntryKind::File,
                                super::TreeEntry::Dir(_) => super::TreeEntryKind::Dir,
                                super::TreeEntry::Symlink { .. } => super::TreeEntryKind::Symlink,
                            };
                            return Err(super::origin_tree_machine_error(
                                super::OriginTreeError::WrongKind { found },
                                path,
                            ));
                        }
                        None => {
                            return Err(super::origin_tree_machine_error(
                                super::OriginTreeError::Missing,
                                path,
                            ));
                        }
                    }
                }
                super::TreeRouting::Origin(installation) => installation
                    .adapter
                    .tree_bytes(value.resident_bytes(), path)
                    .map_err(|error| super::origin_tree_machine_error(error, path))?,
                super::TreeRouting::Unclaimed(refusal) => return Err(refusal),
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
        if let ReadProjection::RegistryManifest = projection {
            if value.schema != Type::Extern(crate::vir::ExternKind::Registry).schema_ref() {
                return Err(PrimitiveMachineError::AuthorityViolation {
                    detail: "registry-manifest read source was not a Registry".to_owned(),
                });
            }
            // The manifest is an ordinary coordinate read through the declared
            // set (the projection variant retires in stage 3 of the origin
            // rail); the Registry source is its own capability.
            let coordinate = super::REGISTRY_MANIFEST_COORDINATE;
            let bytes = self
                .origins
                .route_coordinate(source, coordinate)?
                .adapter
                .read(source, coordinate)
                .map_err(|error| super::origin_read_machine_error(error, coordinate))?;
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

    // r[impl machine.primitive.origin-routing] — selection is a lookup over
    // the installed declarations: an unclaimed scheme, an inadmissible
    // capability, or an empty set is the loud typed refusal, never a fallback.
    fn origin_candidate(
        &self,
        capability: &ValueId,
        coordinate: &str,
    ) -> Result<Vec<u8>, PrimitiveMachineError> {
        self.origins
            .route_coordinate(capability, coordinate)?
            .adapter
            .read(capability, coordinate)
            .map_err(|error| super::origin_read_machine_error(error, coordinate))
    }

    fn exec_backend(&self) -> Option<Arc<dyn super::ExecBackend>> {
        self.exec_backend.clone()
    }
}

/// The scheduler-installed live delivery authority for in-flight progressive
/// publications (`machine.primitive.progressive-response`): a `Send + Sync`
/// sender the primitive may call from any worker thread; the scheduler alone
/// consumes what it forwards.
pub type ProgressSender = Arc<dyn Fn(ProgressivePublication) + Send + Sync>;

#[derive(Clone)]
pub struct EffectCtx {
    demand: DemandKey,
    authority: Arc<dyn EffectAuthority>,
    transaction: Arc<Mutex<EffectTransaction>>,
    progress: Option<ProgressSender>,
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
            progress: None,
        }
    }

    /// Install the live progressive-publication route. Without one,
    /// [`Self::publish_progress`] still records the publication for the
    /// completion's witness — it just cannot be served while in flight.
    #[must_use]
    pub fn with_progress(mut self, progress: ProgressSender) -> Self {
        self.progress = Some(progress);
        self
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
        match self.authority.read(source, &projection) {
            Ok(witnessed) => {
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
            Err(error) => {
                // Misses are witnessed: a projection that found nothing (or
                // found the wrong kind) is an OBSERVATION in the receipt, not
                // a silent absence — the rerun audit re-verifies "it was not
                // there" like any other claim. Refusals and machine faults
                // are not observations: nobody looked.
                //
                // r[impl machine.primitive.witness-reverification]
                let observation = match &error {
                    PrimitiveMachineError::ProjectionMissing { .. } => {
                        Some(ReadObservation::Missing)
                    }
                    PrimitiveMachineError::ProjectionWrongKind { found, .. } => {
                        Some(ReadObservation::Kind(*found))
                    }
                    _ => None,
                };
                if let Some(observation) = observation {
                    self.transaction
                        .lock()
                        .expect("effect transaction mutex poisoned")
                        .reads
                        .push(ReadWitness {
                            source: source.clone(),
                            projection,
                            observation,
                        });
                }
                Err(error)
            }
        }
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
        let bytes = match self.authority.origin_candidate(capability, coordinate) {
            Ok(bytes) => bytes,
            Err(error) => {
                // A failed origin candidate is witnessed as one Missing
                // observation for its tried coordinate — a multi-origin fetch
                // that falls through has every attempt in the receipt, not
                // forgotten. Refusals are not witnessed: nobody looked.
                //
                // r[impl machine.primitive.witness-reverification]
                if matches!(error, PrimitiveMachineError::ProjectionMissing { .. }) {
                    self.transaction
                        .lock()
                        .expect("effect transaction mutex poisoned")
                        .reads
                        .push(ReadWitness {
                            source: capability.clone(),
                            projection: ReadProjection::Origin {
                                coordinate: coordinate.to_owned(),
                            },
                            observation: ReadObservation::Missing,
                        });
                }
                return Err(error);
            }
        };
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

    /// The process-boundary service this effect may cross through
    /// (`machine.primitive.effect-backend-service`): the installed backend, or
    /// a typed machine error when the snapshot carries none — a primitive that
    /// needs a process boundary its embedder never granted fails loudly, it
    /// never reaches for `std::process` itself.
    pub fn exec_backend(&self) -> Result<Arc<dyn super::ExecBackend>, PrimitiveMachineError> {
        self.authority
            .exec_backend()
            .ok_or_else(|| PrimitiveMachineError::Unavailable {
                detail: "no exec backend is installed for this effect snapshot".to_owned(),
            })
    }

    pub fn observe(&self, observation: JournalObservation) {
        self.transaction
            .lock()
            .expect("effect transaction mutex poisoned")
            .journal
            .push(observation);
    }

    /// Publish one progressive projection of the in-flight response: an
    /// immutable product's readiness or a byte-stream extension. The
    /// publication is recorded in the transaction FIRST — the completion's
    /// witness lists everything published, which is what makes a replayed
    /// stream indistinguishable from a live one — and then forwarded live so
    /// the scheduler can serve a waiting projection demand before completion.
    ///
    /// r[impl machine.primitive.progressive-response]
    pub fn publish_progress(&self, publication: ProgressivePublication) {
        self.transaction
            .lock()
            .expect("effect transaction mutex poisoned")
            .progressive
            .push(publication.clone());
        if let Some(progress) = &self.progress {
            (progress)(publication);
        }
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
    /// Lowered exactly like `Value` — the request record still carries the
    /// capability as a semantic input — but declared so the scheduler derives
    /// the effect demand preimage from it: the capability's identity enters the
    /// preimage arguments while every other request field enters the normalized
    /// request recipe. Its value is redeemed only host-side by the effect's
    /// backend service, never by demand keying.
    ///
    /// r[impl machine.primitive.capability-role]
    Capability { expected: Type },
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

    /// The registered primitive's declared [`RequestShape`], if it lowers its
    /// surface call as data. The scheduler consults this to derive the effect
    /// demand preimage from capability-role declarations
    /// (`machine.primitive.capability-role`).
    #[must_use]
    pub fn request_shape(&self, id: &PrimitiveId) -> Option<RequestShape> {
        self.registry.request_shape(id)
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

    #[must_use]
    pub fn request_shape(&self, id: &PrimitiveId) -> Option<RequestShape> {
        self.primitives
            .get(id)
            .and_then(|primitive| primitive.request_shape())
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

// ---- codata primitives ----------------------------------------------------
//
// A [`RawPrimitive`] completes an asynchronous ticket with exactly one interned
// value (`PrimitiveCompletion::Ok(ValueId)`). A stream is *codata*: per
// `r[machine.identity.streams-cross-island-edges]` a stream "has recipe identity
// and no value identity of its own", so it can never be a single `ValueId` and a
// `RawPrimitive` can never hand one back. `glob` — the build language's "find
// files" op — is exactly this shape: `Tree.glob(pattern) -> Stream<Path, Path>`,
// whose elements are realized only when a later `.collect()` drains it.
//
// A [`CodataPrimitive`] is the codata analogue of `RawPrimitive`: where a raw
// primitive begins an async effect and completes with one value, a codata
// primitive *synchronously realizes a stream recipe* into its ordered elements
// when the scheduler drains it. It runs inside the effect-island interpreter (a
// straight-line `&mut self` evaluator), so it is synchronous by construction —
// `glob` over an already-materialized tree performs only local directory reads.
// The scheduler assembles the drained elements into the stream's canonical
// collected value (an `OrderedMap`); the primitive owns only the *domain* logic
// (pattern matching, directory/archive enumeration).

/// The context a [`CodataPrimitive`] drains through: it exposes the stream's
/// source value and enumerates a lazily-backed tree's directories, recording
/// each listing as a witnessed read. It is the codata analogue of
/// [`EffectCtx`], scoped to the synchronous effect-island interpreter. The
/// scheduler owns the concrete implementation (it holds the installed origin
/// backends and the read log); the primitive sees only this trait, and the
/// trait names no backend (`machine.primitive.origin-verbs` retires the
/// fixture-named method this one replaces).
pub trait CodataDrainCtx {
    /// The resident bytes of the stream's source value (e.g. the `Tree` a glob
    /// matches against). For a lazily-backed tree these are its opaque handle
    /// bytes; for a content-identified tree they are the archive/carrier/
    /// canonical bytes the drain enumerates directly.
    fn source_bytes(&self) -> &[u8];

    /// List a directory of a lazily-backed source, recording the listing as a
    /// witnessed `Directory` read against the source. Listings are
    /// `(name, TreeEntryKind)` rows — kinds, never contents.
    ///
    /// r[impl machine.primitive.origin-verbs]
    fn directory(
        &mut self,
        projection: &str,
    ) -> Result<Vec<(String, super::TreeEntryKind)>, PrimitiveMachineError>;
}

/// A registered producer of effect codata. Unlike [`RawPrimitive`], a codata
/// primitive needs nothing from the embedder context: draining a stream is a
/// local, deterministic function of the source value plus witnessed directory
/// reads, so the trait is not `Ctx`-generic.
pub trait CodataPrimitive: Send + Sync {
    fn descriptor(&self) -> &PrimitiveDescriptor;

    /// Realize the stream recipe into its ordered elements. `pattern` is the
    /// recipe's sole non-source operand (the glob pattern); each returned string
    /// is one element key/value the scheduler interns as a `Path` and collects
    /// into the canonical stream value. The returned order is authoritative —
    /// the scheduler sorts keys when building the `OrderedMap`, but the drain is
    /// expected to yield a deterministic, path-ordered set.
    fn drain(
        &self,
        pattern: &str,
        ctx: &mut dyn CodataDrainCtx,
    ) -> Result<Vec<String>, PrimitiveMachineError>;
}

/// The registered codata primitives, keyed by id. `vix-core` constructs every
/// runtime with an empty registry (the bare language realizes no streams); the
/// `vixen` runtime installs one carrying the builtin codata primitives (`glob`)
/// via [`crate::runtime::Runtime::set_codata_registry`].
#[derive(Default)]
pub struct CodataRegistry {
    primitives: BTreeMap<PrimitiveId, Arc<dyn CodataPrimitive>>,
}

impl CodataRegistry {
    pub fn register(
        &mut self,
        primitive: Arc<dyn CodataPrimitive>,
    ) -> Result<(), PrimitiveRegistrationError> {
        let id = primitive.descriptor().id.clone();
        if self.primitives.insert(id.clone(), primitive).is_some() {
            return Err(PrimitiveRegistrationError::Duplicate(id));
        }
        Ok(())
    }

    #[must_use]
    pub fn get(&self, id: &PrimitiveId) -> Option<Arc<dyn CodataPrimitive>> {
        self.primitives.get(id).cloned()
    }
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
