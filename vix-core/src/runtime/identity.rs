use super::tree::{Blob, Tree, TreeEntry};
use crate::schema::SchemaRef;

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Digest(pub [u8; 32]);

impl Digest {
    #[must_use]
    pub fn hex(self) -> String {
        hex::encode(self.0)
    }
}

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RecipeId(pub Digest);

impl RecipeId {
    #[must_use]
    pub fn from_canonical_vir(bytes: &[u8]) -> Self {
        Self(hash_framed(b"vix.recipe.v1", &[bytes]))
    }

    /// Recipe identity for a machine-plane effect expression. The caller
    /// supplies the VIR structural fingerprint, which intentionally excludes
    /// partition-local node ids so duplicate pinned demands share one memo
    /// preimage while their input identities remain part of that preimage.
    #[must_use]
    pub fn from_effect_fingerprint(fingerprint: &str) -> Self {
        Self(hash_framed(
            b"vix.effect.recipe.v1",
            &[fingerprint.as_bytes()],
        ))
    }
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ValueId {
    pub schema: SchemaRef,
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
        let mut writer = FramedHasher::for_domain(b"vix.demand.v2");
        writer.tag(Role::Aux);
        writer.framed(&preimage.closure.0.0);
        for argument in &preimage.arguments {
            writer.tag(Role::Aux);
            writer.schema_ref(&argument.schema);
            writer.tag(Role::Aux);
            writer.framed(&argument.content.0);
        }
        Self(writer.finish())
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
    fn from_segments(segments: Vec<String>) -> Self {
        let fields = segments.iter().map(String::as_bytes).collect::<Vec<_>>();
        Self {
            id: LocationId(hash_framed(b"vix.location.v1", &fields)),
            segments,
        }
    }

    #[must_use]
    pub fn with_source_revision(self, revision: &str) -> Self {
        let mut segments = Vec::with_capacity(self.segments.len() + 2);
        segments.push("source".to_owned());
        segments.push(revision.to_owned());
        segments.extend(self.segments);
        Self::from_segments(segments)
    }

    #[must_use]
    pub fn for_test_value(test_name: &str, stable_id: &str) -> Self {
        let segments = vec![
            "test".to_owned(),
            test_name.to_owned(),
            "value".to_owned(),
            stable_id.to_owned(),
        ];
        Self::from_segments(segments)
    }

    #[must_use]
    pub fn for_test_island(test_name: &str, island: u32) -> Self {
        let segments = vec![
            "test".to_owned(),
            test_name.to_owned(),
            "check".to_owned(),
            island.to_string(),
        ];
        Self::from_segments(segments)
    }

    /// The memo-nomination location of one effect demand, keyed by the
    /// effect's node-id-independent structural fingerprint. Two structurally
    /// identical effect expressions in one test nominate the same location, so
    /// the second demand of an identical pinned fetch is an exact memo hit —
    /// never a second effect spawn.
    #[must_use]
    pub fn for_test_effect(test_name: &str, fingerprint: &str) -> Self {
        let digest = hash_framed(b"vix.effect.fingerprint.v1", &[fingerprint.as_bytes()]);
        let segments = vec![
            "test".to_owned(),
            test_name.to_owned(),
            "effect".to_owned(),
            digest.hex(),
        ];
        Self::from_segments(segments)
    }

    #[must_use]
    pub fn for_primitive(parent: &Self, site: &str) -> Self {
        let mut segments = parent.segments.clone();
        segments.push("primitive".to_owned());
        segments.push(site.to_owned());
        Self::from_segments(segments)
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
        for key in dynamic_keys {
            segments.push(format!("key:{}:{}", key.schema, key.content.hex()));
        }
        Self::from_segments(segments)
    }
}

/// Domain separator for the framed value-identity epoch.
///
/// This is an explicit NEW epoch: digests produced through [`FramedHasher`] are
/// deliberately NOT bit-compatible with the retired flat `hash_framed`/raw-ABI
/// digests. Equal semantic values still dedupe; unequal role/shape values do
/// not collide structurally.
const VALUE_EPOCH_DOMAIN: &[u8] = b"vix.identity.value.framed.v2.schema-ref";

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

    /// Append the complete resolved semantic reference. Taxon's content-derived
    /// declaration id and every concrete type argument participate; a Weavy ABI
    /// ordinal can never enter this path.
    ///
    /// r[impl machine.identity.schema-ref]
    fn schema_ref(&mut self, schema: &SchemaRef) {
        self.word(schema.canonical_len());
        schema.write_canonical(&mut |bytes| self.raw(bytes));
    }

    /// Open a value: role, its stable schema identity, and its arity.
    pub fn start(&mut self, schema: &SchemaRef, arity: u64) -> &mut Self {
        self.tag(Role::Start);
        self.schema_ref(schema);
        self.word(arity);
        self
    }

    /// A positional record/variant field header.
    pub fn field(&mut self, index: u64, schema: &SchemaRef) -> &mut Self {
        self.tag(Role::Field);
        self.word(index);
        self.schema_ref(schema);
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
    pub fn seq_element(&mut self, index: u64, schema: &SchemaRef) -> &mut Self {
        self.tag(Role::SeqElement);
        self.word(index);
        self.schema_ref(schema);
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
    pub fn child(&mut self, child: &ValueId) -> &mut Self {
        self.tag(Role::Child);
        self.schema_ref(&child.schema);
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

/// The domain separator of the semantic tree encoding.
///
/// Vix's `TreeHash` and Vixen's storage `NodeHash` are different identities over
/// different preimages and MUST NOT share one: this is `blake3("vix/tree/v1" ||
/// semantic tree encoding)`, the store's node hash is
/// `blake3("vx/cas-node/v1" || versioned phon node)`, and a side index maps
/// between them. Rechunking a file must not move a single memo entry.
///
/// r[impl machine.identity.tree-hash-is-not-node-hash]
const TREE_HASH_DOMAIN: &[u8] = b"vix/tree/v1";

/// The identity of a [`Tree`] — a Merkle map over the *semantic* encoding, never
/// over the store's chunking.
///
/// r[impl machine.identity.merkle-tree]
#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TreeHash(pub Digest);

impl TreeHash {
    #[must_use]
    pub fn hex(self) -> String {
        self.0.hex()
    }
}

/// A Blob's value identity: an ordinary opaque leaf under the `Blob` schema, in
/// the ordinary value epoch. It carries its own bytes and therefore its own
/// size, which is exactly why the tree encoding names it once and never restates
/// a length beside it.
///
/// r[impl machine.identity.tree-hash-is-not-node-hash]
#[must_use]
pub fn blob_identity(blob: &Blob) -> ValueId {
    FramedNode::leaf(
        crate::vir::Type::Extern(crate::vir::ExternKind::Blob).schema_ref(),
        blob.as_bytes().to_vec(),
    )
    .identity()
}

/// The semantic tree encoding, hashed through the closed framed writer.
///
/// Per entry it hashes the name, the kind tag, and per kind: `File` → (the
/// Blob's *value identity*, executable); `Dir` → the child `TreeHash`;
/// `Symlink` → the target text. It hashes no `blob_node`, no chunking
/// discriminant, no `total_size`, and no separately-stated `size` — those are
/// storage representation, and a size restated beside a content hash is a
/// storage field wearing a semantic coat.
///
/// Rows are in `Name` order, which is content-determined, so insertion order
/// cannot move a Tree's identity. `Dir` contributing a child hash rather than an
/// inlined subtree is what makes this a Merkle map: change one file, rehash one
/// path.
///
/// The Tree's *schema* is deliberately absent from this preimage — the schema is
/// the other half of the `(SchemaRef, ContentHash)` pair, not a component of the
/// content hash, and folding it in here would make a `TreeHash` uncomputable
/// before the embedder has registered its host types.
///
/// r[impl machine.identity.tree-hash-is-not-node-hash]
/// r[impl machine.identity.merkle-tree]
/// r[impl machine.identity.tree-canonicalization]
/// r[impl machine.identity.map-order-independence]
/// r[impl machine.identity.single-module]
#[must_use]
pub fn tree_hash(tree: &Tree) -> TreeHash {
    let mut writer = FramedHasher::for_domain(TREE_HASH_DOMAIN);
    hash_tree_into(&mut writer, tree);
    TreeHash(writer.finish())
}

fn hash_tree_into(writer: &mut FramedHasher, tree: &Tree) {
    writer.seq_len(tree.len() as u64);
    for (index, (name, entry)) in tree.iter().enumerate() {
        writer.map_pair(index as u64);
        writer.bytes(name.as_bytes());
        writer.variant(entry.kind_tag());
        match entry {
            TreeEntry::File {
                content,
                executable,
            } => {
                writer.child(&blob_identity(content));
                // Portable semantic intent, part of identity on every platform
                // — not a POSIX mode fragment.
                writer.bytes(&[u8::from(*executable)]);
            }
            TreeEntry::Dir(child) => {
                writer.bytes(&tree_hash(child).0.0);
            }
            TreeEntry::Symlink { target } => {
                writer.bytes(target.as_bytes());
            }
        }
    }
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
    Leaf { schema: SchemaRef, bytes: Vec<u8> },
    Variant {
        schema: SchemaRef,
        tag: u64,
        fields: Vec<FramedField>,
    },
    /// A compact inline scalar sequence. `canonical_bytes` packs `element_width`
    /// bytes per element contiguously; the element count is
    /// `canonical_bytes.len() / element_width`.
    SeqInline {
        schema: SchemaRef,
        element_schema: SchemaRef,
        element_width: u32,
        canonical_bytes: Vec<u8>,
    },
    /// A sequence of already-interned children, contributed by referent
    /// `ValueId` (handle-independent).
    SeqChildren {
        schema: SchemaRef,
        element_schema: SchemaRef,
        children: Vec<ValueId>,
    },
    /// Canonical key-ordered map rows. Both key and value contribute only their
    /// semantic referent identities; ordered arena topology and handles do not.
    OrderedMap {
        schema: SchemaRef,
        rows: Vec<(ValueId, ValueId)>,
    },
    /// Canonical element-ordered set members by semantic identity.
    OrderedSet {
        schema: SchemaRef,
        elements: Vec<ValueId>,
    },
}

/// A positional field of a [`FramedNode::Variant`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FramedField {
    pub schema: SchemaRef,
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
    pub fn leaf(schema: SchemaRef, bytes: Vec<u8>) -> Self {
        Self::Leaf { schema, bytes }
    }

    /// The value's stable Vix schema identity.
    #[must_use]
    pub fn schema(&self) -> &SchemaRef {
        match self {
            Self::Reference(identity) => &identity.schema,
            Self::Leaf { schema, .. }
            | Self::Variant { schema, .. }
            | Self::SeqInline { schema, .. }
            | Self::SeqChildren { schema, .. }
            | Self::OrderedMap { schema, .. }
            | Self::OrderedSet { schema, .. } => schema,
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
            return identity.clone();
        }
        let mut writer = FramedHasher::new();
        self.hash_into(&mut writer);
        ValueId {
            schema: self.schema().clone(),
            content: writer.finish(),
        }
    }

    fn hash_into(&self, writer: &mut FramedHasher) {
        match self {
            Self::Reference(identity) => {
                writer.child(identity);
            }
            Self::Leaf { schema, bytes } => {
                writer.start(schema, 1).bytes(bytes);
            }
            Self::Variant {
                schema,
                tag,
                fields,
            } => {
                writer.start(schema, fields.len() as u64);
                writer.variant(*tag);
                for (index, field) in fields.iter().enumerate() {
                    writer.field(index as u64, &field.schema);
                    match &field.value {
                        FramedValue::Bytes(payload) => {
                            writer.bytes(payload);
                        }
                        FramedValue::Optional(None) => {
                            writer.variant(0);
                        }
                        FramedValue::Optional(Some(child)) => {
                            writer.variant(1).child(child);
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
                writer.start(schema, count as u64).seq_len(count as u64);
                for index in 0..count {
                    let start = index * width;
                    writer
                        .seq_element(index as u64, element_schema)
                        .bytes(&canonical_bytes[start..start + width]);
                }
            }
            Self::SeqChildren {
                schema,
                element_schema,
                children,
            } => {
                writer
                    .start(schema, children.len() as u64)
                    .seq_len(children.len() as u64);
                for (index, child) in children.iter().enumerate() {
                    writer
                        .seq_element(index as u64, element_schema)
                        .child(child);
                }
            }
            Self::OrderedMap { schema, rows } => {
                writer
                    .start(schema, rows.len() as u64)
                    .seq_len(rows.len() as u64);
                for (index, (key, value)) in rows.iter().enumerate() {
                    writer.map_pair(index as u64).child(key).child(value);
                }
            }
            Self::OrderedSet { schema, elements } => {
                writer
                    .start(schema, elements.len() as u64)
                    .seq_len(elements.len() as u64);
                for (index, element) in elements.iter().enumerate() {
                    writer
                        .seq_element(index as u64, &element.schema)
                        .child(element);
                }
            }
        }
    }
}

// ---- pins: digests that name their own algorithm ------------------------
//
// A pin's algorithm table lives here rather than beside the wire type because
// this is the module that owns hashing vocabulary (`machine.identity.single-
// module`), and the guard in tests/framed_identity.rs enforces exactly that: no
// other runtime module may so much as name a digest family.

/// A digest algorithm the machine can compute.
///
/// The set is CLOSED and runtime-implemented — you cannot verify a digest you
/// cannot compute — which is why a pin's algorithm parses to this enum rather
/// than staying a string. What the *surface* spells self-describingly is the
/// value; the algorithm is data travelling with it, not schema
/// (`vixen.pins.self-describing`).
///
/// Adding a member is a project decision: it widens what every machine must be
/// able to compute. Removing one is an epoch.
///
/// r[impl vixen.pins.algorithm-strength]
#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum DigestAlgorithm {
    /// Vix's own identity space (`machine.identity.blake3`). A blake3 pin also
    /// names the value, so it resolves before any transfer.
    Blake3 = 0,
    /// What `Cargo.lock`, rustup's `.sha256` files, and most registries publish.
    Sha256 = 1,
    /// What npm's integrity fields publish.
    Sha512 = 2,
}

impl DigestAlgorithm {
    /// The canonical lowercase tag, as written in a pin.
    #[must_use]
    pub fn tag(self) -> &'static str {
        match self {
            Self::Blake3 => "blake3",
            Self::Sha256 => "sha256",
            Self::Sha512 => "sha512",
        }
    }

    /// How many bytes a digest of this algorithm has. A pin of the wrong length
    /// is a typo, not a digest, and is refused rather than padded or truncated.
    #[must_use]
    pub fn digest_len(self) -> usize {
        match self {
            Self::Blake3 | Self::Sha256 => 32,
            Self::Sha512 => 64,
        }
    }

    /// Tag lookup, case-insensitive because ecosystems disagree about case and
    /// nobody should have to care.
    #[must_use]
    pub fn from_tag(tag: &str) -> Option<Self> {
        match tag.to_ascii_lowercase().as_str() {
            "blake3" => Some(Self::Blake3),
            "sha256" => Some(Self::Sha256),
            "sha512" => Some(Self::Sha512),
            _ => None,
        }
    }
}

/// Algorithms that are recognized by name and deliberately refused, so the
/// diagnostic can say *why* instead of "unknown algorithm". A pin's whole value
/// is that a stranger can check it; a collision-attackable digest does not
/// deliver that, and silently accepting one would make a pin look like a
/// guarantee it is not.
const REFUSED_ALGORITHMS: &[&str] = &["md5", "sha1"];

/// A digest **of the bytes**, carrying the algorithm that produced it.
///
/// This is the "not screwed in 2064" property: `sha256: "…"` puts the algorithm
/// in the SCHEMA, so retiring sha256 means changing the language. `hash:
/// "sha256:…"` puts it in the VALUE, so retiring sha256 means writing a
/// different string — exactly how the ecosystem moved past md5 without every
/// tool that consumed a checksum needing a new field.
///
/// Wire-encoded as its canonical text (`vix::vir::facet_leaf_override` maps it
/// to `Type::String`), so widening from a bare hex digest to a tagged one costs
/// no schema change.
///
/// r[impl vixen.pins.self-describing]
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq, Hash)]
pub struct UpstreamDigest {
    pub algorithm: DigestAlgorithm,
    pub bytes: Vec<u8>,
}

/// Why a string is not a pin.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DigestParseError {
    /// No `algorithm:` prefix. An untagged digest is the shape this type exists
    /// to abolish: it is only meaningful if the reader already knows which
    /// algorithm was meant, which is the assumption that makes migration hard.
    Untagged,
    /// A recognized algorithm that is not admissible as a pin.
    Refused { tag: String },
    /// An algorithm this machine cannot compute.
    Unknown { tag: String },
    /// The digits are not hexadecimal.
    NotHex,
    /// Right algorithm, wrong number of bytes.
    Length { expected: usize, found: usize },
}

impl std::fmt::Display for DigestParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Untagged => f.write_str(
                "a pin must name its algorithm, as in `sha256:<hex>` — an untagged digest \
                 is only meaningful to a reader who already knows which one was meant",
            ),
            Self::Refused { tag } => write!(
                f,
                "`{tag}` is not admissible as a pin: a digest a stranger cannot trust is \
                 not a pin. Record it beside an admissible one if you need it for provenance"
            ),
            Self::Unknown { tag } => write!(
                f,
                "unknown digest algorithm `{tag}` — this machine can compute blake3, \
                 sha256 and sha512"
            ),
            Self::NotHex => f.write_str("a pin's digits are hexadecimal"),
            Self::Length { expected, found } => write!(
                f,
                "a digest of this algorithm is {expected} bytes, not {found}"
            ),
        }
    }
}

impl UpstreamDigest {
    /// Parse `"<algorithm>:<hex>"`.
    ///
    /// Case-insensitive in both halves. Base64 spellings (SRI's `sha512-<b64>`,
    /// Nix's `sha256-<b64>`) are NOT yet accepted — see the follow-up noted on
    /// `vixen.pins.canonical-digest-form`; they need a base64 decoder this
    /// workspace does not currently carry, and accepting them later cannot
    /// invalidate anything written today because canonical output is unchanged.
    ///
    /// r[impl vixen.pins.canonical-digest-form]
    pub fn parse(text: &str) -> Result<Self, DigestParseError> {
        let (tag, digits) = text.split_once(':').ok_or(DigestParseError::Untagged)?;
        let lowered = tag.to_ascii_lowercase();
        if REFUSED_ALGORITHMS.contains(&lowered.as_str()) {
            return Err(DigestParseError::Refused { tag: lowered });
        }
        let algorithm = DigestAlgorithm::from_tag(&lowered)
            .ok_or(DigestParseError::Unknown { tag: lowered })?;
        let bytes = hex::decode(digits).map_err(|_| DigestParseError::NotHex)?;
        if bytes.len() != algorithm.digest_len() {
            return Err(DigestParseError::Length {
                expected: algorithm.digest_len(),
                found: bytes.len(),
            });
        }
        Ok(Self { algorithm, bytes })
    }

    /// The canonical spelling: lowercase tag, colon, lowercase hex. Everything
    /// that enters a demand key or a receipt goes through here, so two spellings
    /// of one digest are one pin and cannot fork the cache.
    ///
    /// r[impl vixen.pins.canonical-digest-form]
    #[must_use]
    pub fn render(&self) -> String {
        format!("{}:{}", self.algorithm.tag(), hex::encode(&self.bytes))
    }
}

#[cfg(test)]
mod pin_tests {
    use super::{DigestAlgorithm, DigestParseError, UpstreamDigest};

    const SHA256: &str = "sha256:9f3c1d8e0a7b6c5d4e3f2a1b0c9d8e7f6a5b4c3d2e1f0a9b8c7d6e5f4a3b2c1d";

    /// r[verify vixen.pins.self-describing]
    #[test]
    fn a_pin_states_its_algorithm() {
        let pin = UpstreamDigest::parse(SHA256).expect("a tagged sha256 pin parses");
        assert_eq!(pin.algorithm, DigestAlgorithm::Sha256);
        assert_eq!(pin.bytes.len(), 32);
        assert_eq!(pin.render(), SHA256);
    }

    /// The property that keeps two spellings of one digest from forking the
    /// cache: everything reaching a demand key or a receipt is the canonical
    /// render, so case is not a second identity.
    ///
    /// r[verify vixen.pins.canonical-digest-form]
    #[test]
    fn case_is_not_a_second_identity() {
        let shouted = SHA256.to_ascii_uppercase();
        let quiet = UpstreamDigest::parse(SHA256).expect("lowercase parses");
        let loud = UpstreamDigest::parse(&shouted).expect("uppercase parses");
        assert_eq!(quiet, loud);
        assert_eq!(quiet.render(), loud.render());
    }

    /// An untagged digest is the shape this type abolishes: it only means
    /// something to a reader who already knows which algorithm was intended.
    ///
    /// r[verify vixen.pins.self-describing]
    #[test]
    fn an_untagged_digest_is_not_a_pin() {
        let bare = SHA256.trim_start_matches("sha256:");
        assert_eq!(UpstreamDigest::parse(bare), Err(DigestParseError::Untagged));
    }

    /// Refused is distinct from Unknown on purpose. "unknown algorithm md5"
    /// reads like a missing feature; the truth is that we can compute it and
    /// decline to trust it, and the diagnostic should say so.
    ///
    /// r[verify vixen.pins.algorithm-strength]
    #[test]
    fn weak_algorithms_are_refused_by_name_not_merely_unknown() {
        for weak in [
            "md5:d41d8cd98f00b204e9800998ecf8427e",
            "sha1:da39a3ee5e6b4b0d",
        ] {
            let tag = weak.split_once(':').expect("test input is tagged").0;
            assert_eq!(
                UpstreamDigest::parse(weak),
                Err(DigestParseError::Refused {
                    tag: tag.to_owned()
                }),
                "{tag} must be refused as a pin, not reported as unknown"
            );
        }
        assert!(matches!(
            UpstreamDigest::parse("shalala:00"),
            Err(DigestParseError::Unknown { .. })
        ));
    }

    /// A digest of the wrong length is a typo, and padding or truncating one
    /// would turn a typo into a pin that verifies something else.
    #[test]
    fn a_short_digest_is_refused_rather_than_padded() {
        assert_eq!(
            UpstreamDigest::parse("sha256:9f3c"),
            Err(DigestParseError::Length {
                expected: 32,
                found: 2
            })
        );
        assert_eq!(
            UpstreamDigest::parse("sha256:zz"),
            Err(DigestParseError::NotHex)
        );
    }

    /// The three admissible algorithms round-trip at their own widths — the
    /// point of the exercise being that adding the next one in 2064 is a new
    /// string, not a schema change.
    ///
    /// r[verify vixen.pins.algorithm-strength]
    #[test]
    fn every_admissible_algorithm_round_trips() {
        for algorithm in [
            DigestAlgorithm::Blake3,
            DigestAlgorithm::Sha256,
            DigestAlgorithm::Sha512,
        ] {
            let digits = "ab".repeat(algorithm.digest_len());
            let text = format!("{}:{digits}", algorithm.tag());
            let pin = UpstreamDigest::parse(&text).expect("admissible pin parses");
            assert_eq!(pin.algorithm, algorithm);
            assert_eq!(pin.bytes.len(), algorithm.digest_len());
            assert_eq!(pin.render(), text);
        }
    }
}
