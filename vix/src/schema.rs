//! Semantic schema references used by Vix value identity.
//!
//! Taxon owns schema identity computation. This module only carries Taxon's
//! resolved, parameterized reference in a Facet-serializable Vix value and
//! provides the one batch-construction seam used by the surface compiler.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::sync::OnceLock;

use taxon::{Field, Kind, Primitive, Schema, SchemaRef as TaxonSchemaRef, Variant, VariantPayload};

/// Taxon's content-derived identifier for one schema declaration.
#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SchemaId(pub u64);

impl From<taxon::SchemaId> for SchemaId {
    fn from(value: taxon::SchemaId) -> Self {
        Self(value.as_u64())
    }
}

impl From<SchemaId> for taxon::SchemaId {
    fn from(value: SchemaId) -> Self {
        Self::from_raw(value.0)
    }
}

/// A resolved concrete Taxon schema reference.
///
/// Runtime values cannot carry an unresolved type variable. Variables exist in
/// schema declarations while Taxon computes their identity; every value-facing
/// reference has a concrete declaration id and recursively concrete arguments.
#[derive(facet::Facet, Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SchemaRef {
    pub id: SchemaId,
    pub args: Vec<SchemaRef>,
}

impl SchemaRef {
    #[must_use]
    pub fn concrete(id: impl Into<SchemaId>) -> Self {
        Self {
            id: id.into(),
            args: Vec::new(),
        }
    }

    #[must_use]
    pub fn generic(id: impl Into<SchemaId>, args: Vec<Self>) -> Self {
        Self {
            id: id.into(),
            args,
        }
    }

    /// Convert a resolved Taxon reference at the compiler/runtime boundary.
    /// An unresolved variable is a compiler defect and cannot become value
    /// identity.
    pub fn from_taxon(reference: &TaxonSchemaRef) -> Result<Self, UnresolvedSchemaVariable> {
        match reference {
            TaxonSchemaRef::Concrete { id, args } => Ok(Self {
                id: (*id).into(),
                args: args
                    .iter()
                    .map(Self::from_taxon)
                    .collect::<Result<Vec<_>, _>>()?,
            }),
            TaxonSchemaRef::Var { name } => Err(UnresolvedSchemaVariable { name: name.clone() }),
        }
    }

    #[must_use]
    pub fn to_taxon(&self) -> TaxonSchemaRef {
        TaxonSchemaRef::Concrete {
            id: self.id.into(),
            args: self.args.iter().map(Self::to_taxon).collect(),
        }
    }

    /// Canonical transport encoding for carrying the semantic reference inside
    /// demand/value framing. This does not compute schema identity; Taxon's
    /// content-derived `SchemaId` remains authoritative.
    #[must_use]
    pub fn canonical_bytes(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.canonical_len() as usize);
        self.write_canonical(&mut |bytes| out.extend_from_slice(bytes));
        out
    }

    pub fn from_canonical_bytes(bytes: &[u8]) -> Result<Self, SchemaRefDecodeError> {
        fn word(bytes: &[u8], cursor: &mut usize) -> Option<u64> {
            let end = cursor.checked_add(8)?;
            let value = u64::from_le_bytes(bytes.get(*cursor..end)?.try_into().ok()?);
            *cursor = end;
            Some(value)
        }

        fn reference(bytes: &[u8], cursor: &mut usize) -> Option<SchemaRef> {
            let id = SchemaId(word(bytes, cursor)?);
            let count = usize::try_from(word(bytes, cursor)?).ok()?;
            let mut args = Vec::with_capacity(count);
            for _ in 0..count {
                let len = usize::try_from(word(bytes, cursor)?).ok()?;
                let end = cursor.checked_add(len)?;
                let argument_bytes = bytes.get(*cursor..end)?;
                let mut argument_cursor = 0;
                let argument = reference(argument_bytes, &mut argument_cursor)?;
                if argument_cursor != argument_bytes.len() {
                    return None;
                }
                *cursor = end;
                args.push(argument);
            }
            Some(SchemaRef { id, args })
        }

        let mut cursor = 0;
        let reference = reference(bytes, &mut cursor).ok_or(SchemaRefDecodeError::Malformed)?;
        if cursor != bytes.len() {
            return Err(SchemaRefDecodeError::TrailingBytes);
        }
        Ok(reference)
    }

    pub(crate) fn canonical_len(&self) -> u64 {
        16 + self
            .args
            .iter()
            .map(|argument| 8 + argument.canonical_len())
            .sum::<u64>()
    }

    pub(crate) fn write_canonical(&self, write: &mut impl FnMut(&[u8])) {
        write(&self.id.0.to_le_bytes());
        write(&(self.args.len() as u64).to_le_bytes());
        for argument in &self.args {
            write(&argument.canonical_len().to_le_bytes());
            argument.write_canonical(write);
        }
    }

    /// Resolve one closed structural schema through Taxon's identity algorithm.
    #[must_use]
    pub fn for_kind(kind: Kind) -> Self {
        let mut referenced = BTreeSet::new();
        collect_kind_ids(&kind, &mut referenced);
        let provisional = (0..=u64::MAX)
            .rev()
            .find(|candidate| !referenced.contains(candidate))
            .expect("a closed schema has an unused provisional key");
        let resolved = taxon::resolve_ids(vec![Schema {
            id: taxon::SchemaId::from_raw(provisional),
            type_params: Vec::new(),
            kind,
        }]);
        Self::concrete(resolved[0].id)
    }

    #[must_use]
    pub fn for_facet<T: facet::Facet<'static>>() -> Self {
        let resolved = taxon::resolve_ids(facet::taxon_bridge::schemas_of(T::SHAPE));
        Self::concrete(resolved[0].id)
    }
}

#[derive(facet::Facet, Clone, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SchemaPattern {
    Concrete {
        id: SchemaId,
        args: Vec<SchemaPattern>,
    },
    Var {
        name: String,
    },
}

impl SchemaPattern {
    #[must_use]
    pub fn exact(schema: &SchemaRef) -> Self {
        Self::Concrete {
            id: schema.id,
            args: schema.args.iter().map(Self::exact).collect(),
        }
    }

    #[must_use]
    pub fn matches(&self, schema: &SchemaRef) -> bool {
        match self {
            Self::Concrete { id, args } => {
                *id == schema.id
                    && args.len() == schema.args.len()
                    && args
                        .iter()
                        .zip(&schema.args)
                        .all(|(pattern, argument)| pattern.matches(argument))
            }
            Self::Var { .. } => true,
        }
    }
}

impl fmt::Display for SchemaRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:016x}", self.id.0)?;
        if !self.args.is_empty() {
            f.write_str("<")?;
            for (index, argument) in self.args.iter().enumerate() {
                if index != 0 {
                    f.write_str(",")?;
                }
                write!(f, "{argument}")?;
            }
            f.write_str(">")?;
        }
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnresolvedSchemaVariable {
    pub name: String,
}

#[derive(facet::Facet, Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum SchemaRefDecodeError {
    Malformed,
    TrailingBytes,
}

/// Resolved declarations produced by one Taxon batch.
#[derive(Clone)]
pub(crate) struct SchemaSet {
    named: BTreeMap<String, SchemaRef>,
}

impl SchemaSet {
    pub(crate) fn named(&self, name: &str) -> Option<&SchemaRef> {
        self.named.get(name)
    }
}

struct PendingSchema {
    name: Option<String>,
    schema: Schema,
}

/// Batch orchestration for Taxon. It allocates provisional keys and delegates
/// every identity decision, including recursive SCCs, to `taxon::resolve_ids`.
pub(crate) struct SchemaBatch {
    next_key: u64,
    keys: BTreeMap<String, taxon::SchemaId>,
    defined: BTreeSet<String>,
    pending: Vec<PendingSchema>,
}

impl SchemaBatch {
    #[must_use]
    pub(crate) fn vix_builtins() -> Self {
        let mut batch = Self {
            next_key: 1,
            keys: BTreeMap::new(),
            defined: BTreeSet::new(),
            pending: Vec::new(),
        };
        for name in [
            "Bool",
            "Int",
            "String",
            "Blob",
            "Check",
            "Path",
            "Tree",
            "TreeEntry",
            "Registry",
            "PinnedUrl",
            "Schema",
            "StreamCheck",
            "Ordering",
            "DecodeError",
            "Echo",
            "Sh",
            "Fn",
            "Array",
            "Map",
            "Set",
            "Option",
            "Result",
            "Stream",
            "Order",
        ] {
            batch.reserve_named(name);
        }
        batch.add_named("Bool", Vec::new(), Kind::Primitive(Primitive::Bool));
        batch.add_named("Int", Vec::new(), Kind::Primitive(Primitive::I64));
        batch.add_named("String", Vec::new(), Kind::Primitive(Primitive::String));
        batch.add_named("Blob", Vec::new(), Kind::Primitive(Primitive::Bytes));
        for name in [
            "Check",
            "Path",
            "Tree",
            "TreeEntry",
            "Registry",
            "PinnedUrl",
            "Schema",
            "StreamCheck",
        ] {
            batch.add_named(
                name,
                Vec::new(),
                Kind::External {
                    kind: format!("vix.{name}"),
                    metadata: None,
                },
            );
        }
        batch.add_named(
            "Ordering",
            Vec::new(),
            Kind::Enum {
                name: "Ordering".into(),
                variants: ["Less", "Equal", "Greater"]
                    .into_iter()
                    .enumerate()
                    .map(|(index, name)| Variant {
                        name: name.into(),
                        index: u32::try_from(index).expect("ordering variant index fits u32"),
                        payload: VariantPayload::Unit,
                    })
                    .collect(),
            },
        );
        let string_ref = batch
            .named_ref("String")
            .expect("String builtin was reserved");
        let int_ref = batch.named_ref("Int").expect("Int builtin was reserved");
        batch.add_named(
            "DecodeError",
            Vec::new(),
            Kind::Struct {
                name: "DecodeError".into(),
                fields: [
                    ("kind", string_ref.clone()),
                    ("path", string_ref.clone()),
                    ("document_offset", int_ref.clone()),
                    ("document_len", int_ref),
                ]
                .into_iter()
                .map(|(name, schema)| Field {
                    name: name.into(),
                    schema,
                    required: true,
                })
                .collect(),
            },
        );
        for name in ["Echo", "Sh"] {
            batch.add_named(
                name,
                Vec::new(),
                Kind::Struct {
                    name: name.into(),
                    fields: vec![Field {
                        name: "$program".into(),
                        schema: string_ref.clone(),
                        required: true,
                    }],
                },
            );
        }
        batch.add_named(
            "Fn",
            vec!["P".into(), "R".into()],
            Kind::External {
                kind: "vix.Fn".into(),
                metadata: None,
            },
        );
        batch.add_named(
            "Array",
            vec!["T".into()],
            Kind::List {
                element: TaxonSchemaRef::var("T"),
            },
        );
        batch.add_named(
            "Map",
            vec!["K".into(), "V".into()],
            Kind::Map {
                key: TaxonSchemaRef::var("K"),
                value: TaxonSchemaRef::var("V"),
            },
        );
        batch.add_named(
            "Set",
            vec!["T".into()],
            Kind::Set {
                element: TaxonSchemaRef::var("T"),
            },
        );
        batch.add_named(
            "Option",
            vec!["T".into()],
            Kind::Option {
                element: TaxonSchemaRef::var("T"),
            },
        );
        batch.add_named(
            "Result",
            vec!["T".into(), "E".into()],
            Kind::Enum {
                name: "Result".into(),
                variants: vec![
                    Variant {
                        name: "Ok".into(),
                        index: 0,
                        payload: VariantPayload::Newtype(TaxonSchemaRef::var("T")),
                    },
                    Variant {
                        name: "Err".into(),
                        index: 1,
                        payload: VariantPayload::Newtype(TaxonSchemaRef::var("E")),
                    },
                ],
            },
        );
        for name in ["Stream", "Order"] {
            let params = if name == "Stream" {
                vec!["K".into(), "V".into()]
            } else {
                vec!["T".into()]
            };
            batch.add_named(
                name,
                params,
                Kind::External {
                    kind: format!("vix.{name}"),
                    metadata: None,
                },
            );
        }
        batch
    }

    pub(crate) fn reserve_named(&mut self, name: &str) -> taxon::SchemaId {
        if let Some(id) = self.keys.get(name) {
            return *id;
        }
        let id = taxon::SchemaId::from_raw(self.next_key);
        self.next_key += 1;
        self.keys.insert(name.to_owned(), id);
        id
    }

    pub(crate) fn named_ref(&self, name: &str) -> Option<TaxonSchemaRef> {
        self.keys.get(name).copied().map(TaxonSchemaRef::concrete)
    }

    pub(crate) fn generic_ref(
        &self,
        name: &str,
        args: Vec<TaxonSchemaRef>,
    ) -> Option<TaxonSchemaRef> {
        self.keys
            .get(name)
            .copied()
            .map(|id| TaxonSchemaRef::generic(id, args))
    }

    pub(crate) fn add_anonymous(&mut self, kind: Kind) -> TaxonSchemaRef {
        let id = taxon::SchemaId::from_raw(self.next_key);
        self.next_key += 1;
        self.pending.push(PendingSchema {
            name: None,
            schema: Schema {
                id,
                type_params: Vec::new(),
                kind,
            },
        });
        TaxonSchemaRef::concrete(id)
    }

    pub(crate) fn add_named(&mut self, name: &str, type_params: Vec<String>, kind: Kind) {
        let id = self.reserve_named(name);
        assert!(
            self.defined.insert(name.to_owned()),
            "schema `{name}` is defined once"
        );
        self.pending.push(PendingSchema {
            name: Some(name.to_owned()),
            schema: Schema {
                id,
                type_params,
                kind,
            },
        });
    }

    #[must_use]
    pub(crate) fn finish(self) -> SchemaSet {
        let names = self
            .pending
            .iter()
            .map(|pending| pending.name.clone())
            .collect::<Vec<_>>();
        let resolved = taxon::resolve_ids(
            self.pending
                .into_iter()
                .map(|pending| pending.schema)
                .collect(),
        );
        let mut named = BTreeMap::new();
        for (name, schema) in names.into_iter().zip(resolved) {
            if let Some(name) = name {
                named.insert(name, SchemaRef::concrete(schema.id));
            }
        }
        SchemaSet { named }
    }
}

#[must_use]
pub(crate) fn builtin_schema(name: &str) -> SchemaRef {
    static BUILTINS: OnceLock<SchemaSet> = OnceLock::new();
    BUILTINS
        .get_or_init(|| SchemaBatch::vix_builtins().finish())
        .named(name)
        .unwrap_or_else(|| panic!("unknown Vix builtin schema `{name}`"))
        .clone()
}

#[must_use]
pub(crate) fn generic_builtin_schema(name: &str, args: Vec<SchemaRef>) -> SchemaRef {
    let base = builtin_schema(name);
    SchemaRef::generic(base.id, args)
}

fn collect_ref_ids(reference: &TaxonSchemaRef, out: &mut BTreeSet<u64>) {
    if let TaxonSchemaRef::Concrete { id, args } = reference {
        out.insert(id.as_u64());
        for argument in args {
            collect_ref_ids(argument, out);
        }
    }
}

fn collect_kind_ids(kind: &Kind, out: &mut BTreeSet<u64>) {
    let mut collect = |reference: &TaxonSchemaRef| collect_ref_ids(reference, out);
    match kind {
        Kind::Primitive(_) | Kind::Dynamic => {}
        Kind::Struct { fields, .. } => fields.iter().for_each(|field| collect(&field.schema)),
        Kind::Enum { variants, .. } => {
            for variant in variants {
                match &variant.payload {
                    VariantPayload::Unit => {}
                    VariantPayload::Newtype(reference) => collect(reference),
                    VariantPayload::Tuple(references) => references.iter().for_each(&mut collect),
                    VariantPayload::Struct(fields) => {
                        fields.iter().for_each(|field| collect(&field.schema));
                    }
                }
            }
        }
        Kind::Tuple { elements } => elements.iter().for_each(&mut collect),
        Kind::List { element }
        | Kind::Set { element }
        | Kind::Option { element }
        | Kind::Array { element, .. }
        | Kind::Tensor { element, .. }
        | Kind::Channel { element, .. } => collect(element),
        Kind::Map { key, value } => {
            collect(key);
            collect(value);
        }
        Kind::External { metadata, .. } => {
            if let Some(metadata) = metadata {
                collect(metadata);
            }
        }
    }
}

#[must_use]
pub(crate) fn taxon_field(name: impl Into<String>, schema: &SchemaRef) -> Field {
    Field {
        name: name.into(),
        schema: schema.to_taxon(),
        required: true,
    }
}
