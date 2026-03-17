use std::collections::HashMap;

use facet_core::{Shape, Type, UserType};
use roam_types::{
    FieldSchema, Schema, SchemaKind, SchemaRegistry, TypeSchemaId, VariantPayload, VariantSchema,
};

use crate::error::{TranslationError, TranslationErrorKind};

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
    Skip { type_id: TypeSchemaId },
}

#[derive(Debug)]
pub struct EnumTranslationPlan {
    /// Maps remote variant index → local variant index.
    /// `None` = unknown variant (runtime error if received).
    pub variant_map: Vec<Option<usize>>,
    /// Per-variant field translation (for struct variants that may have evolved).
    /// Keyed by remote variant index.
    pub variant_plans: HashMap<usize, TranslationPlan>,
}

/// A complete schema set: root schema + registry for resolving type references.
#[derive(Debug)]
pub struct SchemaSet {
    pub root: Schema,
    pub registry: SchemaRegistry,
}

impl SchemaSet {
    /// Build a SchemaSet from extracted schemas. The root is the last schema
    /// (extraction produces dependencies before dependents).
    pub fn from_extracted(schemas: Vec<Schema>) -> Self {
        let root = schemas.last().cloned().expect("empty schema list");
        let registry = roam_types::build_registry(&schemas);
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
#[allow(clippy::result_large_err)]
pub fn build_plan(input: &PlanInput) -> Result<TranslationPlan, TranslationError> {
    let remote_type_id = input.remote.root.type_id;
    let local_type_name = if input.local.root.name.is_empty() {
        schema_kind_label(&input.local.root.kind)
    } else {
        input.local.root.name.clone()
    };

    let err_ctx = |kind: TranslationErrorKind| TranslationError {
        path: Vec::new(),
        remote_type_id,
        local_type_name: local_type_name.clone(),
        kind,
    };

    match (&input.remote.root.kind, &input.local.root.kind) {
        (
            SchemaKind::Struct {
                fields: remote_fields,
            },
            SchemaKind::Struct {
                fields: local_fields,
            },
        ) => build_struct_plan(
            remote_fields,
            local_fields,
            remote_type_id,
            &local_type_name,
            input,
        ),
        (
            SchemaKind::Enum {
                variants: remote_variants,
            },
            SchemaKind::Enum {
                variants: local_variants,
            },
        ) => build_enum_plan(
            remote_variants,
            local_variants,
            remote_type_id,
            &local_type_name,
            input,
        ),
        (
            SchemaKind::Tuple {
                elements: remote_elements,
            },
            SchemaKind::Tuple {
                elements: local_elements,
            },
        ) => build_tuple_plan(
            remote_elements,
            local_elements,
            remote_type_id,
            &local_type_name,
            input,
        ),
        // Same kind, no field-level translation needed
        (SchemaKind::Primitive { .. }, SchemaKind::Primitive { .. })
        | (SchemaKind::List { .. }, SchemaKind::List { .. })
        | (SchemaKind::Map { .. }, SchemaKind::Map { .. })
        | (SchemaKind::Set { .. }, SchemaKind::Set { .. })
        | (SchemaKind::Array { .. }, SchemaKind::Array { .. })
        | (SchemaKind::Option { .. }, SchemaKind::Option { .. }) => Ok(TranslationPlan {
            field_ops: Vec::new(),
            nested: HashMap::new(),
            enum_plan: None,
        }),
        // Kind mismatch
        _ => Err(err_ctx(TranslationErrorKind::KindMismatch {
            remote_kind: if input.remote.root.name.is_empty() {
                schema_kind_label(&input.remote.root.kind)
            } else {
                input.remote.root.name.clone()
            },
            local_kind: local_type_name.clone(),
        })),
    }
}

/// Build a nested plan for two type IDs looked up in their respective registries.
#[allow(clippy::result_large_err)]
fn nested_plan(
    remote_type_id: &TypeSchemaId,
    local_type_id: &TypeSchemaId,
    input: &PlanInput,
) -> Option<Result<TranslationPlan, TranslationError>> {
    let remote_schema = input.remote.registry.get(remote_type_id)?;
    let local_schema = input.local.registry.get(local_type_id)?;
    let sub_input = PlanInput {
        remote: &SchemaSet {
            root: remote_schema.clone(),
            registry: input.remote.registry.clone(),
        },
        local: &SchemaSet {
            root: local_schema.clone(),
            registry: input.local.registry.clone(),
        },
    };
    Some(build_plan(&sub_input))
}

#[allow(clippy::result_large_err)]
fn build_struct_plan(
    remote_fields: &[FieldSchema],
    local_fields: &[FieldSchema],
    remote_type_id: TypeSchemaId,
    local_type_name: &str,
    input: &PlanInput,
) -> Result<TranslationPlan, TranslationError> {
    let err = |kind: TranslationErrorKind| TranslationError {
        path: Vec::new(),
        remote_type_id,
        local_type_name: local_type_name.to_string(),
        kind,
    };

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
            if let Some(result) = nested_plan(&remote_field.type_id, &local_field.type_id, input) {
                let nested_plan =
                    result.map_err(|e| e.with_path_prefix(remote_field.name.as_str()))?;
                nested.insert(local_idx, nested_plan);
            }
        } else {
            field_ops.push(FieldOp::Skip {
                type_id: remote_field.type_id,
            });
        }
    }

    // r[impl schema.errors.missing-required]
    for (i, matched) in matched_local.iter().enumerate() {
        if !matched && local_fields[i].required {
            return Err(err(TranslationErrorKind::MissingRequiredField {
                field_name: local_fields[i].name.to_string(),
                field_type: format!("{:?}", local_fields[i].type_id),
            }));
        }
    }

    Ok(TranslationPlan {
        field_ops,
        nested,
        enum_plan: None,
    })
}

#[allow(clippy::result_large_err)]
fn build_tuple_plan(
    remote_elements: &[TypeSchemaId],
    local_elements: &[TypeSchemaId],
    remote_type_id: TypeSchemaId,
    local_type_name: &str,
    input: &PlanInput,
) -> Result<TranslationPlan, TranslationError> {
    let err = |kind: TranslationErrorKind| TranslationError {
        path: Vec::new(),
        remote_type_id,
        local_type_name: local_type_name.to_string(),
        kind,
    };

    if remote_elements.len() != local_elements.len() {
        return Err(err(TranslationErrorKind::KindMismatch {
            remote_kind: format!("tuple({} elements)", remote_elements.len()),
            local_kind: format!("tuple({} elements)", local_elements.len()),
        }));
    }

    let mut field_ops = Vec::with_capacity(remote_elements.len());
    let mut nested = HashMap::new();

    for (i, (remote_elem_id, local_elem_id)) in remote_elements
        .iter()
        .zip(local_elements.iter())
        .enumerate()
    {
        field_ops.push(FieldOp::Read { local_index: i });

        if let Some(result) = nested_plan(remote_elem_id, local_elem_id, input) {
            let plan = result.map_err(|e| e.with_path_prefix(&i.to_string()))?;
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
#[allow(clippy::result_large_err)]
fn build_enum_plan(
    remote_variants: &[VariantSchema],
    local_variants: &[VariantSchema],
    remote_type_id: TypeSchemaId,
    local_type_name: &str,
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
                                    type_id: rf.type_id,
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
                        type_id: remote_inner_id,
                    },
                    VariantPayload::Newtype {
                        type_id: local_inner_id,
                    },
                ) => {
                    if let Some(result) = nested_plan(remote_inner_id, local_inner_id, input) {
                        let inner_plan =
                            result.map_err(|e| e.with_path_prefix(&remote_variant.name))?;
                        nested.insert(local_idx, inner_plan);
                    }
                }
                (VariantPayload::Unit, VariantPayload::Unit) => {}
                _ => {}
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

fn schema_kind_label(kind: &SchemaKind) -> String {
    match kind {
        SchemaKind::Struct { .. } => "Struct".into(),
        SchemaKind::Enum { .. } => "Enum".into(),
        SchemaKind::Tuple { .. } => "Tuple".into(),
        SchemaKind::List { .. } => "List".into(),
        SchemaKind::Map { .. } => "Map".into(),
        SchemaKind::Set { .. } => "Set".into(),
        SchemaKind::Array { .. } => "Array".into(),
        SchemaKind::Option { .. } => "Option".into(),
        SchemaKind::Primitive { primitive_type } => format!("Primitive({primitive_type:?})"),
    }
}
