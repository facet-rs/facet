#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Digest(pub [u8; 32]);

impl Digest {
    #[must_use]
    pub fn hex(self) -> String {
        hex::encode(self.0)
    }
}

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SchemaId(pub Digest);

impl SchemaId {
    #[must_use]
    pub fn named(name: &str) -> Self {
        Self(hash_framed(b"vix.schema.v1", &[name.as_bytes()]))
    }
}

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RecipeId(pub Digest);

impl RecipeId {
    #[must_use]
    pub fn from_canonical_vir(bytes: &[u8]) -> Self {
        Self(hash_framed(b"vix.recipe.v1", &[bytes]))
    }
}

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ValueId {
    pub schema: SchemaId,
    pub content: Digest,
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct DemandPreimage {
    pub closure: RecipeId,
    pub arguments: Vec<ValueId>,
}

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DemandKey(pub Digest);

impl DemandKey {
    /// Hash once at demand entry from identities already carried by values.
    ///
    /// r[impl machine.memo.demand-key]
    /// r[impl machine.memo.no-recompute-at-lookup]
    #[must_use]
    pub fn from_preimage(preimage: &DemandPreimage) -> Self {
        let mut fields = Vec::with_capacity(1 + preimage.arguments.len() * 2);
        fields.push(preimage.closure.0.0.as_slice());
        for argument in &preimage.arguments {
            fields.push(argument.schema.0.0.as_slice());
            fields.push(argument.content.0.as_slice());
        }
        Self(hash_framed(b"vix.demand.v1", &fields))
    }
}

/// Cost-model nomination key. Its digest never validates reuse; the memo entry
/// still compares the exact demand preimage.
#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct LocationId(pub Digest);

/// Full content-free path used to nominate prior memo entries. The digest is
/// only an index; `segments` remain the collision check and inspection value.
///
/// r[impl machine.memo.indexed-by-location]
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
pub struct Location {
    pub id: LocationId,
    pub segments: Vec<String>,
}

impl Location {
    #[must_use]
    pub fn for_test_value(test_name: &str, stable_id: &str) -> Self {
        let segments = vec![
            "test".to_owned(),
            test_name.to_owned(),
            "value".to_owned(),
            stable_id.to_owned(),
        ];
        let fields = segments.iter().map(String::as_bytes).collect::<Vec<_>>();
        Self {
            id: LocationId(hash_framed(b"vix.location.v1", &fields)),
            segments,
        }
    }

    #[must_use]
    pub fn for_test_island(test_name: &str, island: u32) -> Self {
        let segments = vec![
            "test".to_owned(),
            test_name.to_owned(),
            "check".to_owned(),
            island.to_string(),
        ];
        let fields = segments.iter().map(String::as_bytes).collect::<Vec<_>>();
        Self {
            id: LocationId(hash_framed(b"vix.location.v1", &fields)),
            segments,
        }
    }

    /// Provenance-keyed location of one evaluated check: the site's check
    /// location extended by the identities of its dynamic iteration keys. With no
    /// dynamic keys (the zero-dynamic-key base case, and every flat island) this
    /// is byte-identical to [`Location::for_test_island`]. The digest folds each
    /// key's schema and content identity — never a handle integer or ABI word —
    /// so equal values at distinct keys stay distinct provenance.
    #[must_use]
    pub fn for_test_provenance(test_name: &str, site: u32, dynamic_keys: &[ValueId]) -> Self {
        let mut segments = vec![
            "test".to_owned(),
            test_name.to_owned(),
            "check".to_owned(),
            site.to_string(),
        ];
        let id = {
            let mut fields = segments.iter().map(String::as_bytes).collect::<Vec<_>>();
            for key in dynamic_keys {
                fields.push(&key.schema.0.0);
                fields.push(&key.content.0);
            }
            LocationId(hash_framed(b"vix.location.v1", &fields))
        };
        for key in dynamic_keys {
            segments.push(format!("key:{}:{}", key.schema.0.hex(), key.content.hex()));
        }
        Self { id, segments }
    }
}

/// Domain separator for the framed value-identity epoch.
///
/// This is an explicit NEW epoch: digests produced through [`FramedHasher`] are
/// deliberately NOT bit-compatible with the retired flat `hash_framed`/raw-ABI
/// digests. Equal semantic values still dedupe; unequal role/shape values do
/// not collide structurally.
const VALUE_EPOCH_DOMAIN: &[u8] = b"vix.identity.value.framed.v1";

/// Role tags. Every framed component begins with its role byte, so the hashed
/// stream is prefix-free and unambiguous. Ordinals are load-bearing epoch
/// constants — reordering them silently invalidates every existing hash.
///
/// r[impl machine.identity.framed-encoding]
#[repr(u8)]
enum Role {
    /// Length-prefixed domain separator, written once at construction.
    Domain = 0x01,
    /// `start(schema, arity)` — opens a value under a schema.
    Start = 0x02,
    /// `field(index, schema)` — a positional record/variant field.
    Field = 0x03,
    /// `variant(tag)` — a sum-type discriminant.
    Variant = 0x04,
    /// `seq_len(len)` — an ordered-sequence length.
    SeqLen = 0x05,
    /// `seq_element(index, schema)` — one ordered-sequence element.
    SeqElement = 0x06,
    /// `map_pair(index)` — one keyed-map row (the unambiguous pair/index role).
    MapPair = 0x07,
    /// Length-prefixed variable-length bytes payload.
    Bytes = 0x08,
    /// A child contribution, by referent `ValueId` (never a handle integer).
    Child = 0x09,
    /// A generic length-prefixed field used by the auxiliary-identity path.
    Aux = 0x0a,
}

/// The single closed writer for machine content identity.
///
/// Its raw blake3 update is private; callers may only append through the
/// role-typed operations that correspond to the settled
/// `machine.identity.framed-encoding` roles. Every variable-length or
/// role-bearing component is length-prefixed or role-tagged, all words are
/// little-endian, and one ordered hasher accumulates the whole stream.
///
/// # Contract
/// - Inputs are treated as attacker-influenced: framing (not summation) is what
///   closes ambiguous-concatenation and cross-domain-reuse collisions
///   (`machine.identity.streaming-combine`).
/// - Unkeyed blake3 (`machine.identity.blake3`); the digest is true identity and
///   is never re-mixed (`machine.identity.hasher-contract`).
///
/// r[impl machine.identity.single-module]
/// r[impl machine.identity.framed-encoding]
/// r[impl machine.identity.le-encoding]
/// r[impl machine.identity.streaming-combine]
pub struct FramedHasher {
    hasher: blake3::Hasher,
}

impl FramedHasher {
    /// Open a writer for the value-identity epoch. The epoch domain is framed
    /// in immediately so no two epochs share a preimage.
    #[must_use]
    pub fn new() -> Self {
        Self::for_domain(VALUE_EPOCH_DOMAIN)
    }

    /// Open a writer for an auxiliary identity family (schema/recipe/demand/
    /// location keys). The domain is the only family separator.
    #[must_use]
    fn for_domain(domain: &[u8]) -> Self {
        let mut writer = Self {
            hasher: blake3::Hasher::new(),
        };
        writer.tag(Role::Domain);
        writer.framed(domain);
        writer
    }

    /// Private raw append — the only place blake3 bytes are written.
    fn raw(&mut self, bytes: &[u8]) {
        self.hasher.update(bytes);
    }

    fn tag(&mut self, role: Role) {
        self.raw(&[role as u8]);
    }

    fn word(&mut self, value: u64) {
        self.raw(&value.to_le_bytes());
    }

    /// A length-prefixed variable-length run.
    fn framed(&mut self, bytes: &[u8]) {
        self.word(bytes.len() as u64);
        self.raw(bytes);
    }

    /// Schemas are fixed-width 32-byte digests; no length prefix is needed, but
    /// they must never be an ABI offset or program-local ordinal.
    ///
    /// r[impl machine.identity.schema-ref]
    fn schema_id(&mut self, schema: SchemaId) {
        self.raw(&schema.0.0);
    }

    /// Open a value: role, its stable schema identity, and its arity.
    pub fn start(&mut self, schema: SchemaId, arity: u64) -> &mut Self {
        self.tag(Role::Start);
        self.schema_id(schema);
        self.word(arity);
        self
    }

    /// A positional record/variant field header.
    pub fn field(&mut self, index: u64, schema: SchemaId) -> &mut Self {
        self.tag(Role::Field);
        self.word(index);
        self.schema_id(schema);
        self
    }

    /// A sum-type discriminant.
    pub fn variant(&mut self, tag: u64) -> &mut Self {
        self.tag(Role::Variant);
        self.word(tag);
        self
    }

    /// An ordered-sequence length header.
    pub fn seq_len(&mut self, len: u64) -> &mut Self {
        self.tag(Role::SeqLen);
        self.word(len);
        self
    }

    /// One ordered-sequence element header.
    pub fn seq_element(&mut self, index: u64, schema: SchemaId) -> &mut Self {
        self.tag(Role::SeqElement);
        self.word(index);
        self.schema_id(schema);
        self
    }

    /// One keyed-map row header — the unambiguous pair/index role.
    ///
    /// r[impl machine.identity.map-order-independence]
    pub fn map_pair(&mut self, index: u64) -> &mut Self {
        self.tag(Role::MapPair);
        self.word(index);
        self
    }

    /// A length-prefixed variable-length bytes payload.
    pub fn bytes(&mut self, bytes: &[u8]) -> &mut Self {
        self.tag(Role::Bytes);
        self.framed(bytes);
        self
    }

    /// A child contribution addressed by its referent `ValueId`. Handles are
    /// process-local indirection and are never hash-visible.
    ///
    /// r[impl machine.identity.handle-by-referent]
    pub fn child(&mut self, child: ValueId) -> &mut Self {
        self.tag(Role::Child);
        self.schema_id(child.schema);
        self.raw(&child.content.0);
        self
    }

    /// Finalize the accumulated stream into a digest. Non-consuming: blake3
    /// finalization reads the state without allocating.
    #[must_use]
    pub fn finish(&self) -> Digest {
        Digest(*self.hasher.finalize().as_bytes())
    }
}

impl Default for FramedHasher {
    fn default() -> Self {
        Self::new()
    }
}

/// Auxiliary identity families (schema, recipe, demand, location) hash a domain
/// and a flat list of length-prefixed fields through the same closed writer, so
/// no runtime raw hasher update exists outside [`FramedHasher`].
pub(crate) fn hash_framed(domain: &[u8], fields: &[&[u8]]) -> Digest {
    let mut writer = FramedHasher::for_domain(domain);
    for field in fields {
        writer.tag(Role::Aux);
        writer.framed(field);
    }
    writer.finish()
}

/// An owned, pre-resolved semantic value tree. Every nested reference is already
/// resolved to a `ValueId`, so a node computes its identity without borrowing
/// the `Store` (`machine.identity.hash-at-construction`). Large scalar sequences
/// stay compact: [`FramedNode::SeqInline`] holds a single packed buffer rather
/// than one heap node per element, and each element is hashed through the closed
/// writer with no per-element allocation.
///
/// r[impl machine.identity.framed-encoding]
/// r[impl machine.identity.hash-at-construction]
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FramedNode {
    /// An already-resolved child identity used while framing a larger value.
    Reference(ValueId),
    /// A scalar/opaque leaf: canonical bytes under one stable schema.
    Leaf { schema: SchemaId, bytes: Vec<u8> },
    /// A tagged variant: discriminant then role-tagged payload fields.
    Variant {
        schema: SchemaId,
        tag: u64,
        fields: Vec<FramedField>,
    },
    /// A compact inline scalar sequence. `canonical_bytes` packs `element_width`
    /// bytes per element contiguously; the element count is
    /// `canonical_bytes.len() / element_width`.
    SeqInline {
        schema: SchemaId,
        element_schema: SchemaId,
        element_width: u32,
        canonical_bytes: Vec<u8>,
    },
    /// A sequence of already-interned children, contributed by referent
    /// `ValueId` (handle-independent).
    SeqChildren {
        schema: SchemaId,
        element_schema: SchemaId,
        children: Vec<ValueId>,
    },
    /// Canonical key-ordered map rows. Both key and value contribute only their
    /// semantic referent identities; ordered arena topology and handles do not.
    OrderedMap {
        schema: SchemaId,
        rows: Vec<(ValueId, ValueId)>,
    },
    /// Canonical element-ordered set members by semantic identity.
    OrderedSet {
        schema: SchemaId,
        elements: Vec<ValueId>,
    },
}

/// A positional field of a [`FramedNode::Variant`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FramedField {
    pub schema: SchemaId,
    pub value: FramedValue,
}

/// The payload of a framed field: inline bytes, or an optional child addressed
/// by referent identity.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FramedValue {
    /// Inline length-prefixed canonical bytes (scalars, tags, packed words).
    Bytes(Vec<u8>),
    /// An optional child contributed by referent `ValueId`.
    Optional(Option<ValueId>),
}

impl FramedNode {
    /// A scalar/opaque leaf convenience constructor.
    #[must_use]
    pub fn leaf(schema: SchemaId, bytes: Vec<u8>) -> Self {
        Self::Leaf { schema, bytes }
    }

    /// The value's stable Vix schema identity.
    #[must_use]
    pub fn schema(&self) -> SchemaId {
        match self {
            Self::Reference(identity) => identity.schema,
            Self::Leaf { schema, .. }
            | Self::Variant { schema, .. }
            | Self::SeqInline { schema, .. }
            | Self::SeqChildren { schema, .. }
            | Self::OrderedMap { schema, .. }
            | Self::OrderedSet { schema, .. } => *schema,
        }
    }

    /// Compute this value's identity through the closed writer, without
    /// borrowing the store. Hashing an inline sequence performs no per-element
    /// heap allocation.
    ///
    /// r[impl machine.identity.hash-at-construction]
    /// r[impl machine.identity.value-identity-pair]
    #[must_use]
    pub fn identity(&self) -> ValueId {
        if let Self::Reference(identity) = self {
            return *identity;
        }
        let mut writer = FramedHasher::new();
        self.hash_into(&mut writer);
        ValueId {
            schema: self.schema(),
            content: writer.finish(),
        }
    }

    fn hash_into(&self, writer: &mut FramedHasher) {
        match self {
            Self::Reference(identity) => {
                writer.child(*identity);
            }
            Self::Leaf { schema, bytes } => {
                writer.start(*schema, 1).bytes(bytes);
            }
            Self::Variant {
                schema,
                tag,
                fields,
            } => {
                writer.start(*schema, fields.len() as u64);
                writer.variant(*tag);
                for (index, field) in fields.iter().enumerate() {
                    writer.field(index as u64, field.schema);
                    match &field.value {
                        FramedValue::Bytes(payload) => {
                            writer.bytes(payload);
                        }
                        FramedValue::Optional(None) => {
                            writer.variant(0);
                        }
                        FramedValue::Optional(Some(child)) => {
                            writer.variant(1).child(*child);
                        }
                    }
                }
            }
            Self::SeqInline {
                schema,
                element_schema,
                element_width,
                canonical_bytes,
            } => {
                let width = *element_width as usize;
                let count = canonical_bytes.len().checked_div(width).unwrap_or(0);
                writer.start(*schema, count as u64).seq_len(count as u64);
                for index in 0..count {
                    let start = index * width;
                    writer
                        .seq_element(index as u64, *element_schema)
                        .bytes(&canonical_bytes[start..start + width]);
                }
            }
            Self::SeqChildren {
                schema,
                element_schema,
                children,
            } => {
                writer
                    .start(*schema, children.len() as u64)
                    .seq_len(children.len() as u64);
                for (index, child) in children.iter().enumerate() {
                    writer
                        .seq_element(index as u64, *element_schema)
                        .child(*child);
                }
            }
            Self::OrderedMap { schema, rows } => {
                writer
                    .start(*schema, rows.len() as u64)
                    .seq_len(rows.len() as u64);
                for (index, (key, value)) in rows.iter().enumerate() {
                    writer.map_pair(index as u64).child(*key).child(*value);
                }
            }
            Self::OrderedSet { schema, elements } => {
                writer
                    .start(*schema, elements.len() as u64)
                    .seq_len(elements.len() as u64);
                for (index, element) in elements.iter().enumerate() {
                    writer
                        .seq_element(index as u64, element.schema)
                        .child(*element);
                }
            }
        }
    }
}
