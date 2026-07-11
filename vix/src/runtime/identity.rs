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
