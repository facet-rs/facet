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

    /// Machine-plane schema tag carried by array payload headers. It is an ABI
    /// witness, never an identity contribution.
    #[must_use]
    pub fn ref_word(self) -> i64 {
        i64::from_le_bytes(self.0.0[..8].try_into().expect("digest is 32 bytes"))
    }
}

/// The two planes of one realized value: what it hashes as, and what task code
/// reads it as.
///
/// `machine.identity.framed-encoding` forbids the structural hash from
/// depending on the ABI. Scalars and strings are their own framed content, so
/// [`ValueBody::flat`] states that identity. An aggregate whose payload is a
/// machine layout must state the two separately — that is what the freeze/
/// publish path will do when the first aggregate crosses an island edge.
#[derive(Clone, Copy, Debug)]
pub struct ValueBody<'a> {
    pub identity_preimage: &'a [u8],
    pub memory: &'a [u8],
}

impl<'a> ValueBody<'a> {
    /// A value whose store bytes are already its framed content.
    #[must_use]
    pub fn flat(bytes: &'a [u8]) -> Self {
        Self {
            identity_preimage: bytes,
            memory: bytes,
        }
    }

    #[must_use]
    pub fn new(identity_preimage: &'a [u8], memory: &'a [u8]) -> Self {
        Self {
            identity_preimage,
            memory,
        }
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
}

pub(crate) fn hash_framed(domain: &[u8], fields: &[&[u8]]) -> Digest {
    let mut hasher = blake3::Hasher::new();
    hasher.update(&(domain.len() as u64).to_le_bytes());
    hasher.update(domain);
    for field in fields {
        hasher.update(&(field.len() as u64).to_le_bytes());
        hasher.update(field);
    }
    Digest(*hasher.finalize().as_bytes())
}
