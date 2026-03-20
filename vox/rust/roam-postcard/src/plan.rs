use std::collections::HashMap;

use facet_core::{Shape, Type, UserType};
use roam_types::{
    ExtractedSchemas, FieldSchema, Schema, SchemaHash, SchemaKind, SchemaRegistry, TypeRef,
    VariantPayload, VariantSchema,
};

use crate::error::{PathSegment, SchemaSide, TranslationError, TranslationErrorKind};

/// A precomputed plan for deserializing postcard bytes into a local type.
///
/// This is a recursive enum that mirrors the shape of data. Each variant
/// carries sub-plans for its children, so translation works through
/// arbitrarily nested containers (Vec, Option, Map, etc.).
#[derive(Debug)]
pub enum TranslationPlan {
    /// Identity — no translation needed. Used for leaves (scalars, etc.)
    /// and for container elements when remote and local types match.
    Identity,

    /// Struct or tuple-struct: field reordering, skipping unknown, filling defaults.
    Struct {
        /// One op per remote field, in remote wire order.
        field_ops: Vec<FieldOp>,
        /// Nested plans for fields that need translation, keyed by local field index.
        nested: HashMap<usize, TranslationPlan>,
    },

    /// Enum: variant remapping + per-variant field plans.
    Enum {
        /// For each remote variant index, the local variant index (or None if unknown).
        variant_map: Vec<Option<usize>>,
        /// Per-variant field plans, keyed by remote variant index.
        variant_plans: HashMap<usize, TranslationPlan>,
        /// Nested plans for newtype/tuple variant inner types, keyed by local variant index.
        nested: HashMap<usize, TranslationPlan>,
    },

    /// Tuple: positional elements with nested plans.
    Tuple {
        field_ops: Vec<FieldOp>,
        nested: HashMap<usize, TranslationPlan>,
    },

    /// List/Vec/Slice: element type needs translation.
    List { element: Box<TranslationPlan> },

    /// Option: inner type needs translation.
    Option { inner: Box<TranslationPlan> },

    /// Map: key and/or value types need translation.
    Map {
        key: Box<TranslationPlan>,
        value: Box<TranslationPlan>,
    },

    /// Fixed-size array: element type needs translation.
    Array { element: Box<TranslationPlan> },

    /// Pointer (Box, Arc, etc.): pointee needs translation.
    Pointer { pointee: Box<TranslationPlan> },
}

#[derive(Debug)]
pub enum FieldOp {
    /// Read this remote field into local field at `local_index`.
    Read { local_index: usize },
    /// Skip this remote field (not present in local type).
    Skip { type_ref: TypeRef },
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
            TypeRef::Var { .. } => unreachable!("root type ref is never a Var"),
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
            TranslationPlan::Struct {
                field_ops,
                nested: HashMap::new(),
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
                    TranslationPlan::Struct {
                        field_ops,
                        nested: HashMap::new(),
                    },
                );
            }
            TranslationPlan::Enum {
                variant_map,
                variant_plans,
                nested: HashMap::new(),
            }
        }
        _ => TranslationPlan::Identity,
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
        // Container types — recurse into element/value types
        (
            SchemaKind::List {
                element: remote_elem,
            },
            SchemaKind::List {
                element: local_elem,
            },
        ) => {
            let element_plan = nested_plan(remote_elem, local_elem, input)?;
            Ok(TranslationPlan::List {
                element: Box::new(element_plan.unwrap_or(TranslationPlan::Identity)),
            })
        }
        (
            SchemaKind::Option {
                element: remote_elem,
            },
            SchemaKind::Option {
                element: local_elem,
            },
        ) => {
            let inner_plan = nested_plan(remote_elem, local_elem, input)?;
            Ok(TranslationPlan::Option {
                inner: Box::new(inner_plan.unwrap_or(TranslationPlan::Identity)),
            })
        }
        (
            SchemaKind::Map {
                key: remote_key,
                value: remote_val,
            },
            SchemaKind::Map {
                key: local_key,
                value: local_val,
            },
        ) => {
            let key_plan = nested_plan(remote_key, local_key, input)?;
            let val_plan = nested_plan(remote_val, local_val, input)?;
            Ok(TranslationPlan::Map {
                key: Box::new(key_plan.unwrap_or(TranslationPlan::Identity)),
                value: Box::new(val_plan.unwrap_or(TranslationPlan::Identity)),
            })
        }
        (
            SchemaKind::Array {
                element: remote_elem,
                ..
            },
            SchemaKind::Array {
                element: local_elem,
                ..
            },
        ) => {
            let element_plan = nested_plan(remote_elem, local_elem, input)?;
            Ok(TranslationPlan::Array {
                element: Box::new(element_plan.unwrap_or(TranslationPlan::Identity)),
            })
        }
        // Primitives — no translation needed
        (SchemaKind::Primitive { .. }, SchemaKind::Primitive { .. }) => {
            Ok(TranslationPlan::Identity)
        }
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
        let type_id = match type_ref {
            TypeRef::Concrete { type_id, .. } => *type_id,
            TypeRef::Var { name } => {
                return Err(TranslationError::new(TranslationErrorKind::UnresolvedVar {
                    name: format!("{name:?}"),
                    side,
                }));
            }
        };
        let kind = type_ref.resolve_kind(registry).ok_or_else(|| {
            TranslationError::new(TranslationErrorKind::SchemaNotFound { type_id, side })
        })?;
        let base = registry.get(&type_id).ok_or_else(|| {
            TranslationError::new(TranslationErrorKind::SchemaNotFound { type_id, side })
        })?;
        Ok(Schema {
            id: base.id,
            type_params: vec![],
            kind,
        })
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

    Ok(TranslationPlan::Struct { field_ops, nested })
}

fn build_tuple_plan(
    remote_elements: &[TypeRef<SchemaHash>],
    local_elements: &[TypeRef<SchemaHash>],
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

    Ok(TranslationPlan::Tuple { field_ops, nested })
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
                        TranslationPlan::Struct {
                            field_ops: variant_field_ops,
                            nested: HashMap::new(),
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
                    let tuple_plan = build_tuple_plan(
                        remote_types,
                        local_types,
                        _remote_schema,
                        _local_schema,
                        input,
                    )
                    .map_err(|e| {
                        e.with_path_prefix(PathSegment::Variant(remote_variant.name.clone()))
                    })?;
                    variant_plans.insert(remote_idx, tuple_plan);
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

    Ok(TranslationPlan::Enum {
        variant_map,
        variant_plans,
        nested,
    })
}
