use std::collections::HashMap;

use facet_core::{Shape, Type, UserType};
use roam_types::{
    ExtractedSchemas, FieldSchema, Schema, SchemaKind, SchemaRegistry, TypeRef, TypeSchemaId,
    VariantPayload, VariantSchema,
};

use crate::error::{PathSegment, SchemaSide, TranslationError, TranslationErrorKind};

/// A precomputed plan for deserializing postcard bytes into a local type.
///
/// When remote and local types are identical, every op is `Read` in order —
/// a no-op translation. When types differ, the plan has skips, reorders,
/// and defaults. Same code path either way.
#[derive(Debug)]
pub struct TranslationPlan {
    /// One op per remote field, in remote wire order.
    pub field_ops: Vec<FieldOp>,
    /// Nested plans for fields that are themselves structs/enums with different schemas.
    /// Keyed by local field index.
    pub nested: HashMap<usize, TranslationPlan>,
    /// Enum translation plan, if this is for an enum type.
    pub enum_plan: Option<EnumTranslationPlan>,
}

#[derive(Debug)]
pub enum FieldOp {
    /// Read this remote field into local field at `local_index`.
    Read { local_index: usize },
    /// Skip this remote field (not present in local type).
    Skip { type_ref: TypeRef },
}

#[derive(Debug)]
pub struct EnumTranslationPlan {
    /// For each remote variant index, the local variant index (or None if unknown).
    pub variant_map: Vec<Option<usize>>,
    /// Per-variant field plans, keyed by remote variant index.
    pub variant_plans: HashMap<usize, TranslationPlan>,
}

/// A schema set: the root schema (with Vars resolved) + the registry.
#[derive(Debug)]
pub struct SchemaSet {
    pub root: Schema,
    pub registry: SchemaRegistry,
}

impl SchemaSet {
    /// Build a SchemaSet from a raw list of schemas (e.g. received from the wire).
    /// The root is the last schema. Its kind is used as-is (no Var resolution).
    pub fn from_schemas(schemas: Vec<Schema>) -> Self {
        let root = schemas.last().cloned().expect("empty schema list");
        let registry = roam_types::build_registry(&schemas);
        SchemaSet { root, registry }
    }

    /// Build a SchemaSet from extracted schemas.
    /// The root TypeRef is used to resolve any Var references in the root schema.
    pub fn from_extracted(extracted: ExtractedSchemas) -> Self {
        let registry = roam_types::build_registry(&extracted.schemas);
        // Resolve the root schema's kind using the root TypeRef's args.
        let root_kind = extracted
            .root_type_ref
            .resolve_kind(&registry)
            .expect("root schema must be in registry");
        let root_id = match &extracted.root_type_ref {
            TypeRef::Concrete { type_id, .. } => *type_id,
            TypeRef::Var(_) => unreachable!("root type ref is never a Var"),
        };
        let root = Schema {
            id: root_id,
            type_params: vec![],
            kind: root_kind,
        };
        SchemaSet { root, registry }
    }
}

/// Input to `build_plan`: identifies which side is remote and which is local.
pub struct PlanInput<'a> {
    pub remote: &'a SchemaSet,
    pub local: &'a SchemaSet,
}

/// Build the trivial identity plan from a local Shape alone.
/// Every field maps 1:1, no skips, no defaults. Used when no remote schema
/// is available (same types on both sides).
pub fn build_identity_plan(shape: &'static Shape) -> TranslationPlan {
    match shape.ty {
        Type::User(UserType::Struct(struct_type)) => {
            let field_ops = (0..struct_type.fields.len())
                .map(|i| FieldOp::Read { local_index: i })
                .collect();
            TranslationPlan {
                field_ops,
                nested: HashMap::new(),
                enum_plan: None,
            }
        }
        Type::User(UserType::Enum(enum_type)) => {
            let variant_map = (0..enum_type.variants.len()).map(Some).collect();
            let mut variant_plans = HashMap::new();
            for (i, variant) in enum_type.variants.iter().enumerate() {
                let field_ops = (0..variant.data.fields.len())
                    .map(|j| FieldOp::Read { local_index: j })
                    .collect();
                variant_plans.insert(
                    i,
                    TranslationPlan {
                        field_ops,
                        nested: HashMap::new(),
                        enum_plan: None,
                    },
                );
            }
            TranslationPlan {
                field_ops: Vec::new(),
                nested: HashMap::new(),
                enum_plan: Some(EnumTranslationPlan {
                    variant_map,
                    variant_plans,
                }),
            }
        }
        _ => TranslationPlan {
            field_ops: Vec::new(),
            nested: HashMap::new(),
            enum_plan: None,
        },
    }
}

/// Build a translation plan by comparing remote and local schemas.
///
/// Both sides are represented as schemas — the same extraction logic
/// (channel unwrapping, transparent wrappers, etc.) has already run on
/// both. This avoids mismatches between schema representation and raw
/// Shape inspection.
// r[impl schema.translation.field-matching]
// r[impl schema.translation.skip-unknown]
// r[impl schema.translation.fill-defaults]
// r[impl schema.translation.reorder]
// r[impl schema.errors.early-detection]
pub fn build_plan(input: &PlanInput) -> Result<TranslationPlan, TranslationError> {
    let remote = &input.remote.root;
    let local = &input.local.root;

    // Validate type names match for nominal types (struct/enum).
    if let (Some(remote_name), Some(local_name)) = (remote.name(), local.name())
        && remote_name != local_name
    {
        return Err(TranslationError::new(TranslationErrorKind::NameMismatch {
            remote: remote.clone(),
            local: local.clone(),
        }));
    }

    match (&remote.kind, &local.kind) {
        (
            SchemaKind::Struct {
                fields: remote_fields,
                ..
            },
            SchemaKind::Struct {
                fields: local_fields,
                ..
            },
        ) => build_struct_plan(remote_fields, local_fields, remote, local, input),
        (
            SchemaKind::Enum {
                variants: remote_variants,
                ..
            },
            SchemaKind::Enum {
                variants: local_variants,
                ..
            },
        ) => build_enum_plan(remote_variants, local_variants, remote, local, input),
        (
            SchemaKind::Tuple {
                elements: remote_elements,
            },
            SchemaKind::Tuple {
                elements: local_elements,
            },
        ) => build_tuple_plan(remote_elements, local_elements, remote, local, input),
        // Same kind, no field-level translation needed
        (SchemaKind::Primitive { .. }, SchemaKind::Primitive { .. })
        | (SchemaKind::List { .. }, SchemaKind::List { .. })
        | (SchemaKind::Map { .. }, SchemaKind::Map { .. })
        | (SchemaKind::Array { .. }, SchemaKind::Array { .. })
        | (SchemaKind::Option { .. }, SchemaKind::Option { .. }) => Ok(TranslationPlan {
            field_ops: Vec::new(),
            nested: HashMap::new(),
            enum_plan: None,
        }),
        // Kind mismatch
        _ => Err(TranslationError::new(TranslationErrorKind::KindMismatch {
            remote: remote.clone(),
            local: local.clone(),
        })),
    }
}

/// Build a nested plan for two TypeRefs looked up in their respective registries.
/// Handles generic types by resolving Var references using the TypeRef's args.
fn nested_plan(
    remote_type_ref: &TypeRef,
    local_type_ref: &TypeRef,
    input: &PlanInput,
) -> Result<Option<TranslationPlan>, TranslationError> {
    let resolve_schema = |type_ref: &TypeRef, registry: &SchemaRegistry, side: SchemaSide| {
        let kind = type_ref.resolve_kind(registry).ok_or_else(|| {
            TranslationError::new(TranslationErrorKind::SchemaNotFound {
                type_id: match type_ref {
                    TypeRef::Concrete { type_id, .. } => *type_id,
                    TypeRef::Var(_) => TypeSchemaId(0),
                },
                side,
            })
        })?;
        // Get the base schema for metadata (id, name, type_params)
        let base = match type_ref {
            TypeRef::Concrete { type_id, .. } => registry.get(type_id).cloned(),
            _ => None,
        };
        let schema = match base {
            Some(s) => Schema {
                id: s.id,
                type_params: vec![], // resolved — no more params
                kind,
            },
            None => {
                return Err(TranslationError::new(
                    TranslationErrorKind::SchemaNotFound {
                        type_id: TypeSchemaId(0),
                        side,
                    },
                ));
            }
        };
        Ok(schema)
    };

    let remote_schema =
        resolve_schema(remote_type_ref, &input.remote.registry, SchemaSide::Remote)?;
    let local_schema = resolve_schema(local_type_ref, &input.local.registry, SchemaSide::Local)?;

    let sub_input = PlanInput {
        remote: &SchemaSet {
            root: remote_schema,
            registry: input.remote.registry.clone(),
        },
        local: &SchemaSet {
            root: local_schema,
            registry: input.local.registry.clone(),
        },
    };
    build_plan(&sub_input).map(Some)
}

fn build_struct_plan(
    remote_fields: &[FieldSchema],
    local_fields: &[FieldSchema],
    remote_schema: &Schema,
    _local_schema: &Schema,
    input: &PlanInput,
) -> Result<TranslationPlan, TranslationError> {
    let mut field_ops = Vec::with_capacity(remote_fields.len());
    let mut nested = HashMap::new();
    let mut matched_local = vec![false; local_fields.len()];

    for remote_field in remote_fields {
        if let Some((local_idx, local_field)) = local_fields
            .iter()
            .enumerate()
            .find(|(_, f)| f.name == remote_field.name)
        {
            matched_local[local_idx] = true;
            field_ops.push(FieldOp::Read {
                local_index: local_idx,
            });

            // r[impl schema.translation.type-compat]
            let nested_plan = nested_plan(&remote_field.type_ref, &local_field.type_ref, input)
                .map_err(|e| e.with_path_prefix(PathSegment::Field(remote_field.name.clone())))?;
            if let Some(plan) = nested_plan {
                nested.insert(local_idx, plan);
            }
        } else {
            field_ops.push(FieldOp::Skip {
                type_ref: remote_field.type_ref.clone(),
            });
        }
    }

    // r[impl schema.errors.missing-required]
    for (i, matched) in matched_local.iter().enumerate() {
        if !matched && local_fields[i].required {
            return Err(TranslationError::new(
                TranslationErrorKind::MissingRequiredField {
                    field: local_fields[i].clone(),
                    remote_struct: remote_schema.clone(),
                },
            ));
        }
    }

    Ok(TranslationPlan {
        field_ops,
        nested,
        enum_plan: None,
    })
}

fn build_tuple_plan(
    remote_elements: &[TypeRef<TypeSchemaId>],
    local_elements: &[TypeRef<TypeSchemaId>],
    remote_schema: &Schema,
    local_schema: &Schema,
    input: &PlanInput,
) -> Result<TranslationPlan, TranslationError> {
    if remote_elements.len() != local_elements.len() {
        return Err(TranslationError::new(
            TranslationErrorKind::TupleLengthMismatch {
                remote: remote_schema.clone(),
                local: local_schema.clone(),
                remote_len: remote_elements.len(),
                local_len: local_elements.len(),
            },
        ));
    }

    let mut field_ops = Vec::with_capacity(remote_elements.len());
    let mut nested = HashMap::new();

    for (i, (remote_elem, local_elem)) in remote_elements
        .iter()
        .zip(local_elements.iter())
        .enumerate()
    {
        field_ops.push(FieldOp::Read { local_index: i });

        let nested_plan = nested_plan(remote_elem, local_elem, input)
            .map_err(|e| e.with_path_prefix(PathSegment::Index(i)))?;
        if let Some(plan) = nested_plan {
            nested.insert(i, plan);
        }
    }

    Ok(TranslationPlan {
        field_ops,
        nested,
        enum_plan: None,
    })
}

// r[impl schema.translation.enum]
// r[impl schema.translation.enum.missing-variant]
// r[impl schema.translation.enum.payload-compat]
fn build_enum_plan(
    remote_variants: &[VariantSchema],
    local_variants: &[VariantSchema],
    _remote_schema: &Schema,
    _local_schema: &Schema,
    input: &PlanInput,
) -> Result<TranslationPlan, TranslationError> {
    let mut variant_map = Vec::with_capacity(remote_variants.len());
    let mut variant_plans = HashMap::new();
    let mut nested = HashMap::new();

    for (remote_idx, remote_variant) in remote_variants.iter().enumerate() {
        if let Some((local_idx, local_variant)) = local_variants
            .iter()
            .enumerate()
            .find(|(_, v)| v.name == remote_variant.name)
        {
            variant_map.push(Some(local_idx));

            match (&remote_variant.payload, &local_variant.payload) {
                // Both struct variants — build a per-variant field plan
                (
                    VariantPayload::Struct {
                        fields: remote_fields,
                    },
                    VariantPayload::Struct {
                        fields: local_fields,
                    },
                ) => {
                    let variant_field_ops: Vec<FieldOp> = remote_fields
                        .iter()
                        .map(|rf| {
                            if let Some((local_field_idx, _)) = local_fields
                                .iter()
                                .enumerate()
                                .find(|(_, f)| f.name == rf.name)
                            {
                                FieldOp::Read {
                                    local_index: local_field_idx,
                                }
                            } else {
                                FieldOp::Skip {
                                    type_ref: rf.type_ref.clone(),
                                }
                            }
                        })
                        .collect();
                    variant_plans.insert(
                        remote_idx,
                        TranslationPlan {
                            field_ops: variant_field_ops,
                            nested: HashMap::new(),
                            enum_plan: None,
                        },
                    );
                }
                // Both newtype — build a nested plan for the inner type
                (
                    VariantPayload::Newtype {
                        type_ref: remote_inner_ref,
                    },
                    VariantPayload::Newtype {
                        type_ref: local_inner_ref,
                    },
                ) => {
                    let inner_plan = nested_plan(remote_inner_ref, local_inner_ref, input)
                        .map_err(|e| {
                            e.with_path_prefix(PathSegment::Variant(remote_variant.name.clone()))
                        })?;
                    if let Some(plan) = inner_plan {
                        nested.insert(local_idx, plan);
                    }
                }
                // Both tuple — check arity matches and build nested plans
                (
                    VariantPayload::Tuple {
                        types: remote_types,
                    },
                    VariantPayload::Tuple { types: local_types },
                ) => {
                    if remote_types.len() != local_types.len() {
                        return Err(TranslationError::new(
                            TranslationErrorKind::IncompatibleVariantPayload {
                                remote_variant: remote_variant.clone(),
                                local_variant: local_variant.clone(),
                            },
                        )
                        .with_path_prefix(PathSegment::Variant(remote_variant.name.clone())));
                    }
                    for (i, (remote_elem, local_elem)) in
                        remote_types.iter().zip(local_types.iter()).enumerate()
                    {
                        let inner_plan =
                            nested_plan(remote_elem, local_elem, input).map_err(|e| {
                                e.with_path_prefix(PathSegment::Variant(
                                    remote_variant.name.clone(),
                                ))
                            })?;
                        if let Some(plan) = inner_plan {
                            // Use a synthetic index for tuple element plans
                            nested.insert(local_idx * 1000 + i, plan);
                        }
                    }
                }
                (VariantPayload::Unit, VariantPayload::Unit) => {}
                // Payload kind mismatch within a variant
                _ => {
                    return Err(TranslationError::new(
                        TranslationErrorKind::IncompatibleVariantPayload {
                            remote_variant: remote_variant.clone(),
                            local_variant: local_variant.clone(),
                        },
                    )
                    .with_path_prefix(PathSegment::Variant(remote_variant.name.clone())));
                }
            }
        } else {
            // r[impl schema.translation.enum.unknown-variant]
            variant_map.push(None);
        }
    }

    Ok(TranslationPlan {
        field_ops: Vec::new(),
        nested,
        enum_plan: Some(EnumTranslationPlan {
            variant_map,
            variant_plans,
        }),
    })
}
