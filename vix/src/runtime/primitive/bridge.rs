//! taxon → vir type bridge (r[machine.primitive.registered]).
//!
//! Registration accepts only the lossless subset of the taxon type vocabulary
//! that vix values can represent structurally. Everything outside that subset is
//! rejected with a dotted field path so the author can see exactly which field or
//! element is unsupported — the bridge never silently coerces.

use std::collections::{BTreeMap, BTreeSet};

use taxon::{Kind, Primitive, Schema, SchemaId, SchemaRef, VariantPayload as TaxonVariantPayload};

use crate::vir::{EnumType, EnumVariant, RecordField, RecordType, Type, VariantPayload};

use super::descriptor::RegistrationError;

/// Convert a taxon schema batch rooted at `root` into a vir [`Type`], or reject
/// it if it uses a shape outside the lossless subset.
///
/// Identity-bearing naming rule: a taxon Struct/Enum named `Foo` with id `id`
/// becomes a vir `RecordType`/`EnumType` named `Foo@{id:016x}`. `@` cannot appear
/// in a source-authored type name, so a registered type can never collide with a
/// user type.
pub fn vir_type_for(root: SchemaId, schemas: &[Schema]) -> Result<Type, RegistrationError> {
    let by_id: BTreeMap<SchemaId, &Schema> = schemas.iter().map(|s| (s.id, s)).collect();
    let primitives: BTreeMap<SchemaId, Primitive> = Primitive::ALL
        .into_iter()
        .map(|p| (taxon::primitive_id(p), p))
        .collect();
    let mut ctx = Ctx {
        by_id,
        primitives,
        visiting: BTreeSet::new(),
        path: Vec::new(),
    };
    ctx.convert_id(root)
}

struct Ctx<'a> {
    by_id: BTreeMap<SchemaId, &'a Schema>,
    primitives: BTreeMap<SchemaId, Primitive>,
    visiting: BTreeSet<SchemaId>,
    path: Vec<String>,
}

impl Ctx<'_> {
    fn unsupported(&self, kind: impl Into<String>) -> RegistrationError {
        RegistrationError::UnsupportedShape {
            path: self.path.join("."),
            kind: kind.into(),
        }
    }

    fn convert_ref(&mut self, sref: &SchemaRef) -> Result<Type, RegistrationError> {
        match sref {
            SchemaRef::Concrete { id, args } => {
                if !args.is_empty() {
                    return Err(self.unsupported("generic"));
                }
                self.convert_id(*id)
            }
            SchemaRef::Var { .. } => Err(self.unsupported("generic")),
        }
    }

    fn convert_id(&mut self, id: SchemaId) -> Result<Type, RegistrationError> {
        if let Some(primitive) = self.primitives.get(&id).copied() {
            return self.primitive_type(primitive);
        }
        let Some(schema) = self.by_id.get(&id).copied() else {
            return Err(self.unsupported("unresolved"));
        };
        if self.visiting.contains(&id) {
            return Err(self.unsupported("recursive"));
        }
        self.visiting.insert(id);
        let kind = schema.kind.clone();
        let ty = self.convert_kind(id, &kind)?;
        self.visiting.remove(&id);
        Ok(ty)
    }

    fn primitive_type(&self, primitive: Primitive) -> Result<Type, RegistrationError> {
        match primitive {
            Primitive::Bool => Ok(Type::Bool),
            Primitive::I64 => Ok(Type::Int),
            Primitive::String => Ok(Type::String),
            Primitive::Unit => Ok(Type::Tuple(Vec::new())),
            other => Err(self.unsupported(format!("{other:?}"))),
        }
    }

    fn convert_kind(&mut self, id: SchemaId, kind: &Kind) -> Result<Type, RegistrationError> {
        match kind {
            Kind::Primitive(primitive) => self.primitive_type(*primitive),
            Kind::Struct { name, fields } => {
                let pushed = self.path.is_empty();
                if pushed {
                    self.path.push(name.clone());
                }
                let mut record_fields = Vec::with_capacity(fields.len());
                for field in fields {
                    self.path.push(field.name.clone());
                    if !field.required {
                        return Err(self.unsupported("non-required field"));
                    }
                    let ty = self.convert_ref(&field.schema)?;
                    self.path.pop();
                    record_fields.push(RecordField {
                        name: field.name.clone(),
                        ty,
                    });
                }
                if pushed {
                    self.path.pop();
                }
                Ok(Type::Record(RecordType {
                    name: registered_name(name, id),
                    fields: record_fields,
                }))
            }
            Kind::Enum { name, variants } => {
                let pushed = self.path.is_empty();
                if pushed {
                    self.path.push(name.clone());
                }
                let mut vir_variants = Vec::with_capacity(variants.len());
                for (index, variant) in variants.iter().enumerate() {
                    self.path.push(variant.name.clone());
                    if variant.index != index as u32 {
                        return Err(self.unsupported("sparse variant indices"));
                    }
                    let payload = self.convert_payload(&variant.payload)?;
                    self.path.pop();
                    vir_variants.push(EnumVariant {
                        name: variant.name.clone(),
                        payload,
                    });
                }
                if pushed {
                    self.path.pop();
                }
                Ok(Type::Enum(EnumType {
                    name: registered_name(name, id),
                    variants: vir_variants,
                }))
            }
            Kind::Tuple { elements } => Ok(Type::Tuple(self.convert_elements(elements)?)),
            Kind::List { element } => Ok(Type::array(self.convert_ref(element)?)),
            Kind::Set { element } => Ok(Type::set(self.convert_ref(element)?)),
            Kind::Map { key, value } => {
                self.path.push("<key>".to_owned());
                let key_ty = self.convert_ref(key)?;
                self.path.pop();
                self.path.push("<value>".to_owned());
                let value_ty = self.convert_ref(value)?;
                self.path.pop();
                Ok(Type::map(key_ty, value_ty))
            }
            Kind::Option { element } => Ok(Type::option(self.convert_ref(element)?)),
            Kind::Array { .. } => Err(self.unsupported("fixed-size array")),
            Kind::Tensor { .. } => Err(self.unsupported("tensor")),
            Kind::Channel { .. } => Err(self.unsupported("channel")),
            Kind::Dynamic => Err(self.unsupported("dynamic")),
            Kind::External { .. } => Err(self.unsupported("external")),
        }
    }

    fn convert_payload(
        &mut self,
        payload: &TaxonVariantPayload,
    ) -> Result<VariantPayload, RegistrationError> {
        match payload {
            TaxonVariantPayload::Unit => Ok(VariantPayload::Unit),
            TaxonVariantPayload::Newtype(element) => {
                Ok(VariantPayload::Tuple(vec![self.convert_ref(element)?]))
            }
            TaxonVariantPayload::Tuple(elements) => {
                Ok(VariantPayload::Tuple(self.convert_elements(elements)?))
            }
            TaxonVariantPayload::Struct(fields) => {
                let mut record_fields = Vec::with_capacity(fields.len());
                for field in fields {
                    self.path.push(field.name.clone());
                    if !field.required {
                        return Err(self.unsupported("non-required field"));
                    }
                    let ty = self.convert_ref(&field.schema)?;
                    self.path.pop();
                    record_fields.push(RecordField {
                        name: field.name.clone(),
                        ty,
                    });
                }
                Ok(VariantPayload::Record(record_fields))
            }
        }
    }

    fn convert_elements(&mut self, elements: &[SchemaRef]) -> Result<Vec<Type>, RegistrationError> {
        let mut out = Vec::with_capacity(elements.len());
        for (index, element) in elements.iter().enumerate() {
            self.path.push(index.to_string());
            out.push(self.convert_ref(element)?);
            self.path.pop();
        }
        Ok(out)
    }
}

fn registered_name(name: &str, id: SchemaId) -> String {
    format!("{name}@{:016x}", id.as_u64())
}

#[cfg(test)]
mod tests {
    use super::*;
    use taxon::{Field, Primitive as P, SchemaId, Variant};

    fn concrete(p: P) -> SchemaRef {
        SchemaRef::concrete(taxon::primitive_id(p))
    }

    fn schema(id: u64, kind: Kind) -> Schema {
        Schema {
            id: SchemaId::from_raw(id),
            type_params: Vec::new(),
            kind,
        }
    }

    fn field(name: &str, schema: SchemaRef) -> Field {
        Field {
            name: name.to_owned(),
            schema,
            required: true,
        }
    }

    fn struct_fixture() -> (SchemaId, Vec<Schema>) {
        let list = schema(101, Kind::List { element: concrete(P::String) });
        let option = schema(102, Kind::Option { element: concrete(P::String) });
        let root = schema(
            100,
            Kind::Struct {
                name: "ProbeRequest".to_owned(),
                fields: vec![
                    field("text", concrete(P::String)),
                    field("deep", concrete(P::Bool)),
                    field("count", concrete(P::I64)),
                    field("tags", SchemaRef::concrete(SchemaId::from_raw(101))),
                    field("extra", SchemaRef::concrete(SchemaId::from_raw(102))),
                ],
            },
        );
        (SchemaId::from_raw(100), vec![root, list, option])
    }

    fn f64_fixture() -> (SchemaId, Vec<Schema>) {
        let root = schema(
            200,
            Kind::Struct {
                name: "Bad".to_owned(),
                fields: vec![field("weight", concrete(P::F64))],
            },
        );
        (SchemaId::from_raw(200), vec![root])
    }

    fn primitive_field_fixture(p: P) -> (SchemaId, Vec<Schema>) {
        let root = schema(
            300,
            Kind::Struct {
                name: "Holder".to_owned(),
                fields: vec![field("value", concrete(p))],
            },
        );
        (SchemaId::from_raw(300), vec![root])
    }

    #[test]
    fn maps_the_supported_subset() {
        let (root, schemas) = struct_fixture();
        let ty = vir_type_for(root, &schemas).unwrap();
        let Type::Record(record) = ty else {
            panic!("expected record")
        };
        assert!(record.name.starts_with("ProbeRequest@"));
        assert_eq!(record.fields.len(), 5);
        assert_eq!(record.fields[0].ty, Type::String);
        assert_eq!(record.fields[1].ty, Type::Bool);
        assert_eq!(record.fields[2].ty, Type::Int);
        assert_eq!(record.fields[3].ty, Type::array(Type::String));
        assert_eq!(record.fields[4].ty, Type::option(Type::String));
    }

    #[test]
    fn rejects_with_field_path() {
        let (root, schemas) = f64_fixture();
        let err = vir_type_for(root, &schemas).unwrap_err();
        let RegistrationError::UnsupportedShape { path, kind } = err else {
            panic!()
        };
        assert_eq!(path, "Bad.weight");
        assert_eq!(kind, "F64");
    }

    #[test]
    fn rejects_every_unsupported_primitive() {
        for p in [
            P::U8,
            P::U16,
            P::U32,
            P::U64,
            P::U128,
            P::I8,
            P::I16,
            P::I32,
            P::I128,
            P::F32,
            P::F64,
            P::Char,
            P::Bytes,
            P::DateTime,
            P::Uuid,
            P::QName,
            P::Never,
        ] {
            let (root, schemas) = primitive_field_fixture(p);
            assert!(
                matches!(
                    vir_type_for(root, &schemas),
                    Err(RegistrationError::UnsupportedShape { .. })
                ),
                "{p:?} must be rejected"
            );
        }
    }

    #[test]
    fn rejects_sparse_variant_indices() {
        let root = schema(
            400,
            Kind::Enum {
                name: "Sparse".to_owned(),
                variants: vec![Variant {
                    name: "Late".to_owned(),
                    index: 3,
                    payload: TaxonVariantPayload::Unit,
                }],
            },
        );
        let err = vir_type_for(SchemaId::from_raw(400), &[root]).unwrap_err();
        assert!(matches!(
            err,
            RegistrationError::UnsupportedShape { kind, .. } if kind == "sparse variant indices"
        ));
    }

    #[test]
    fn maps_enum_payloads() {
        let root = schema(
            500,
            Kind::Enum {
                name: "Verdict".to_owned(),
                variants: vec![
                    Variant {
                        name: "Pass".to_owned(),
                        index: 0,
                        payload: TaxonVariantPayload::Unit,
                    },
                    Variant {
                        name: "Fail".to_owned(),
                        index: 1,
                        payload: TaxonVariantPayload::Struct(vec![field("reason", concrete(P::String))]),
                    },
                ],
            },
        );
        let ty = vir_type_for(SchemaId::from_raw(500), &[root]).unwrap();
        let Type::Enum(enumeration) = ty else {
            panic!("expected enum")
        };
        assert!(enumeration.name.starts_with("Verdict@"));
        assert_eq!(enumeration.variants[0].payload, VariantPayload::Unit);
        assert_eq!(
            enumeration.variants[1].payload,
            VariantPayload::Record(vec![RecordField {
                name: "reason".to_owned(),
                ty: Type::String,
            }])
        );
    }
}
