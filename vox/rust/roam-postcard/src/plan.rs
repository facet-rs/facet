use std::collections::HashMap;

use facet_core::{Shape, StructKind, Type, UserType};
use roam_schema::{Schema, SchemaKind, SchemaRegistry, TypeId};

use crate::error::TranslationError;

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
    Skip { type_id: TypeId },
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
        _ => {
            // Scalars, containers, etc. — no field ops needed
            TranslationPlan {
                field_ops: Vec::new(),
                nested: HashMap::new(),
                enum_plan: None,
            }
        }
    }
}

/// Build a translation plan from a remote schema and local Shape.
// r[impl schema.translation.field-matching]
// r[impl schema.translation.skip-unknown]
// r[impl schema.translation.fill-defaults]
// r[impl schema.translation.reorder]
pub fn build_plan(
    remote_schema: &Schema,
    local_shape: &'static Shape,
    registry: &SchemaRegistry,
) -> Result<TranslationPlan, TranslationError> {
    match &remote_schema.kind {
        SchemaKind::Struct {
            fields: remote_fields,
        } => build_struct_plan(remote_fields, local_shape, registry),
        SchemaKind::Enum {
            variants: remote_variants,
        } => build_enum_plan(remote_variants, local_shape, registry),
        _ => {
            // Primitives, containers — no field-level translation needed.
            // Compatibility was checked when the plan was requested.
            Ok(TranslationPlan {
                field_ops: Vec::new(),
                nested: HashMap::new(),
                enum_plan: None,
            })
        }
    }
}

fn build_struct_plan(
    remote_fields: &[roam_schema::FieldSchema],
    local_shape: &'static Shape,
    registry: &SchemaRegistry,
) -> Result<TranslationPlan, TranslationError> {
    let local_struct = match local_shape.ty {
        Type::User(UserType::Struct(s)) => s,
        _ => {
            return Err(TranslationError::TypeMismatch {
                field: String::new(),
                remote: "struct".into(),
                local: format!("{}", local_shape),
            });
        }
    };

    let mut field_ops = Vec::with_capacity(remote_fields.len());
    let mut nested = HashMap::new();
    let mut matched_local = vec![false; local_struct.fields.len()];

    for remote_field in remote_fields {
        // Look up by name in local struct
        if let Some((local_idx, local_field)) = local_struct
            .fields
            .iter()
            .enumerate()
            .find(|(_, f)| f.name == remote_field.name)
        {
            matched_local[local_idx] = true;
            field_ops.push(FieldOp::Read {
                local_index: local_idx,
            });

            // Check if nested plan is needed (remote field is a struct/enum that may differ)
            if let Some(remote_field_schema) = registry.get(&remote_field.type_id) {
                let local_field_shape = local_field.shape();
                let local_field_id = roam_schema::type_id_of(local_field_shape);
                if remote_field.type_id != local_field_id {
                    // Types differ — build nested plan
                    let nested_plan = build_plan(remote_field_schema, local_field_shape, registry)?;
                    nested.insert(local_idx, nested_plan);
                }
            }
        } else {
            // Remote field not in local type — skip it
            field_ops.push(FieldOp::Skip {
                type_id: remote_field.type_id,
            });
        }
    }

    // Check that all required local fields are covered
    for (i, matched) in matched_local.iter().enumerate() {
        if !matched {
            let field = &local_struct.fields[i];
            if field.default.is_none() {
                return Err(TranslationError::MissingRequiredField {
                    name: field.name.to_string(),
                });
            }
        }
    }

    Ok(TranslationPlan {
        field_ops,
        nested,
        enum_plan: None,
    })
}

// r[impl schema.translation.enum]
fn build_enum_plan(
    remote_variants: &[roam_schema::VariantSchema],
    local_shape: &'static Shape,
    _registry: &SchemaRegistry,
) -> Result<TranslationPlan, TranslationError> {
    let local_enum = match local_shape.ty {
        Type::User(UserType::Enum(e)) => e,
        _ => {
            return Err(TranslationError::TypeMismatch {
                field: String::new(),
                remote: "enum".into(),
                local: format!("{}", local_shape),
            });
        }
    };

    let mut variant_map = Vec::with_capacity(remote_variants.len());
    let mut variant_plans = HashMap::new();

    for (remote_idx, remote_variant) in remote_variants.iter().enumerate() {
        // Match by name
        if let Some((local_idx, local_variant)) = local_enum
            .variants
            .iter()
            .enumerate()
            .find(|(_, v)| v.name == remote_variant.name)
        {
            variant_map.push(Some(local_idx));

            // Build per-variant field plan if it's a struct variant
            if let roam_schema::VariantPayload::Struct {
                fields: remote_fields,
            } = &remote_variant.payload
            {
                if local_variant.data.kind == StructKind::Struct
                    || local_variant.data.kind == StructKind::TupleStruct
                {
                    // Build a mini struct plan for this variant's fields
                    let variant_field_ops: Vec<FieldOp> = remote_fields
                        .iter()
                        .enumerate()
                        .map(|(_, rf)| {
                            if let Some((local_field_idx, _)) = local_variant
                                .data
                                .fields
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
            }
        } else {
            // r[impl schema.translation.enum.unknown-variant]
            // Unknown variant — will cause runtime error if received
            variant_map.push(None);
        }
    }

    Ok(TranslationPlan {
        field_ops: Vec::new(),
        nested: HashMap::new(),
        enum_plan: Some(EnumTranslationPlan {
            variant_map,
            variant_plans,
        }),
    })
}
