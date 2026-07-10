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

impl LocationId {
    #[must_use]
    pub fn for_test_island(test_name: &str, island: u32) -> Self {
        Self(hash_framed(
            b"vix.location.v1",
            &[test_name.as_bytes(), &island.to_le_bytes()],
        ))
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
