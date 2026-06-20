//! JSON-specific lowered program vocabulary.
//!
//! `weavy` owns the op-agnostic carrier: root programs, recursive blocks, and
//! stack-safe program execution. This module keeps the JSON semantics in
//! `facet-json`: field matching, scalar/string policy, enum representation, and
//! shape-specific operations that a future interpreter and copy-and-patch backend
//! must agree on.

use alloc::{collections::BTreeMap, vec::Vec};

use facet_core::{ScalarType, Shape, StructKind};
use facet_reflect::{
    DeserStrategy, FieldPlan, FillRule, NodeId, TypePlanCore, TypePlanNode, VariantPlanMeta,
};

/// A lowered JSON deserialization program.
pub type JsonProgram = weavy::Program<JsonOp>;

/// A lowered JSON deserialization program with recursive shape blocks.
pub type JsonLowered = weavy::Lowered<JsonBlockId, JsonOp>;

/// Identifier for a recursive JSON deserialization block.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct JsonBlockId(usize);

impl JsonBlockId {
    /// Use a reflected shape as a stable block key for the current process.
    #[must_use]
    pub fn for_shape(shape: &'static Shape) -> Self {
        Self(shape as *const Shape as usize)
    }
}

/// One lowered JSON deserialization instruction.
#[derive(Clone, Debug, PartialEq, Eq)]
#[non_exhaustive]
pub enum JsonOp {
    /// Allocate or enter a value of this reflected shape.
    EnterShape { shape: &'static Shape },
    /// Read JSON `null` into unit-like or null-capable targets.
    Null,
    /// Read a scalar JSON value.
    Scalar(JsonScalar),
    /// Read a JSON string.
    String(JsonString),
    /// Read a bytes value using the active JSON bytes representation.
    Bytes(JsonBytes),
    /// Capture the source JSON value as raw JSON.
    RawJson,
    /// Deserialize a named-field object.
    Struct(JsonStruct),
    /// Deserialize a fixed tuple or tuple struct.
    Tuple {
        elements: Vec<JsonProgram>,
        single_field_transparent: bool,
    },
    /// Deserialize a variable-length sequence.
    List {
        element: JsonProgram,
        byte_optimized: bool,
    },
    /// Deserialize a fixed-length array.
    Array { len: usize, element: JsonProgram },
    /// Deserialize a map.
    Map {
        key: JsonProgram,
        value: JsonProgram,
    },
    /// Deserialize a set.
    Set { element: JsonProgram },
    /// Deserialize an option.
    Option { some: JsonProgram },
    /// Deserialize a result.
    Result { ok: JsonProgram, err: JsonProgram },
    /// Deserialize an enum using one of Facet's JSON enum representations.
    Enum(JsonEnum),
    /// Deserialize through a smart pointer.
    Pointer { pointee: JsonProgram },
    /// Deserialize through a transparent wrapper.
    Transparent { inner: JsonProgram },
    /// Deserialize through a proxy shape.
    Proxy { proxy: JsonProgram },
    /// Deserialize an opaque pointer.
    OpaquePointer,
    /// Deserialize an opaque value.
    Opaque,
    /// Deserialize a metadata container.
    MetadataContainer,
    /// Deserialize a dynamic reflected value.
    Dynamic,
    /// Call a recursive shape block.
    CallBlock(JsonBlockId),
    /// Finish the current program.
    Return,
}

/// Scalar decoding policy for a JSON value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JsonScalar {
    /// Reflected scalar target, when the shape has one.
    pub ty: Option<ScalarType>,
    /// Whether string fallback through `FromStr` is allowed for this scalar.
    pub from_str: bool,
}

/// String decoding policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct JsonString {
    /// Whether the decoded string may borrow from the input.
    pub borrow: JsonBorrow,
    /// Whether the string is the direct value or an input to scalar parsing.
    pub role: JsonStringRole,
}

/// Borrowing policy for input-backed JSON spans.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsonBorrow {
    /// Materialize an owned value.
    Owned,
    /// Borrow when JSON escaping permits it.
    Borrowed,
}

/// Why a JSON string is being read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsonStringRole {
    /// The target is string-like.
    Value,
    /// The target is a map key.
    MapKey,
    /// The target is a variant tag.
    VariantTag,
}

/// Bytes representation accepted by JSON deserialization.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsonBytes {
    /// JSON array of byte numbers.
    Array,
    /// Hex string.
    Hex,
}

/// Object-field plan.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonStruct {
    /// Expected fields in lowered planning order.
    pub fields: Vec<JsonField>,
    /// Unknown-field behavior for this object.
    pub unknown_fields: UnknownFields,
}

/// One named JSON field.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonField {
    /// Primary serialized field name.
    pub name: &'static str,
    /// Alternate accepted field name, when `#[facet(alias = "...")]` is present.
    pub alias: Option<&'static str>,
    /// Field payload program.
    pub value: JsonProgram,
    /// How to handle this field when it is absent.
    pub missing: MissingField,
    /// Whether the field is flattened into its parent object.
    pub flattened: bool,
    /// Whether the field is skipped during deserialization.
    pub skip_deserializing: bool,
}

/// Unknown object field behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnknownFields {
    /// Skip unknown JSON fields.
    Ignore,
    /// Reject unknown JSON fields.
    Deny,
    /// Route unknown JSON fields into a flattened map field.
    Capture,
}

/// Missing-field behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MissingField {
    /// The field must be present in JSON.
    Required,
    /// Fill from the field's reflected default.
    Default,
    /// Leave unset because the reflected type can complete it.
    Omit,
}

/// Enum representation used by JSON deserialization.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonEnum {
    /// Shape-level representation.
    pub repr: JsonEnumRepr,
    /// Variant arms in declaration order.
    pub variants: Vec<JsonVariant>,
    /// Catch-all `#[facet(other)]` variant, if any.
    pub other: Option<JsonVariant>,
}

/// Facet enum representation as used by JSON.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum JsonEnumRepr {
    /// Variant name is the object key.
    ExternallyTagged,
    /// Variant tag is a field next to the variant fields.
    InternallyTagged { tag: &'static str },
    /// Tag and content are adjacent fields.
    AdjacentlyTagged {
        tag: &'static str,
        content: &'static str,
    },
    /// Untagged/flattened matching through the solver.
    Flattened,
}

impl JsonEnumRepr {
    /// Detect the JSON enum representation from a Facet shape.
    #[must_use]
    pub fn from_shape(shape: &'static Shape) -> Self {
        match (
            shape.is_untagged(),
            shape.get_tag_attr(),
            shape.get_content_attr(),
        ) {
            (true, _, _) => Self::Flattened,
            (false, Some(tag), Some(content)) => Self::AdjacentlyTagged { tag, content },
            (false, Some(tag), None) => Self::InternallyTagged { tag },
            (false, None, _) => Self::ExternallyTagged,
        }
    }
}

/// One enum variant arm.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JsonVariant {
    /// Effective serialized variant name.
    pub name: &'static str,
    /// Variant payload shape.
    pub payload: JsonVariantPayload,
}

/// Lowered enum payload.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum JsonVariantPayload {
    /// Unit variant.
    Unit,
    /// Single-field payload.
    Newtype(JsonProgram),
    /// Tuple payload.
    Tuple(Vec<JsonProgram>),
    /// Struct payload.
    Struct(JsonStruct),
}

/// Error while lowering a Facet type plan into JSON bytecode.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LowerError {
    /// A proxy strategy did not have a matching lowered proxy node.
    MissingProxyNode { shape: &'static Shape },
    /// The node strategy required struct metadata, but none was present.
    MissingStructPlan { shape: &'static Shape },
    /// The node strategy required enum metadata, but none was present.
    MissingEnumPlan { shape: &'static Shape },
    /// The node strategy referenced a child node that was absent.
    MissingChild {
        shape: &'static Shape,
        relation: &'static str,
    },
    /// The enum plan named an out-of-range `#[facet(other)]` variant.
    MissingOtherVariant { shape: &'static Shape, index: usize },
    /// The reflected plan contains a strategy this lowerer does not know yet.
    UnsupportedStrategy { shape: &'static Shape },
}

type LowerResult<T> = core::result::Result<T, LowerError>;

/// Lower a reflected deserialization plan into JSON bytecode.
///
/// This pass is deliberately format-owned: `weavy` supplies the generic
/// program/block carrier, while `facet-json` decides which JSON-specific op each
/// `TypePlan` strategy becomes.
pub fn lower_type_plan(core: &TypePlanCore) -> LowerResult<JsonLowered> {
    let mut lowerer = Lowerer {
        core,
        blocks: BTreeMap::new(),
        building: Vec::new(),
    };
    let program = lowerer.lower_node_id(core.root_id())?;
    Ok(JsonLowered {
        program,
        blocks: lowerer.blocks,
    })
}

struct Lowerer<'plan> {
    core: &'plan TypePlanCore,
    blocks: BTreeMap<JsonBlockId, JsonProgram>,
    building: Vec<JsonBlockId>,
}

impl Lowerer<'_> {
    fn lower_node_id(&mut self, node_id: NodeId) -> LowerResult<JsonProgram> {
        self.lower_node(self.core.node(node_id))
    }

    fn lower_node(&mut self, node: &TypePlanNode) -> LowerResult<JsonProgram> {
        if let DeserStrategy::BackRef { .. } = node.strategy {
            let target = self.core.resolve_backref(node);
            return self.lower_block_call(target);
        }

        self.lower_resolved_node(node)
    }

    fn lower_block_call(&mut self, node: &TypePlanNode) -> LowerResult<JsonProgram> {
        let block_id = JsonBlockId::for_shape(node.shape);
        if !self.blocks.contains_key(&block_id) && !self.building.contains(&block_id) {
            self.building.push(block_id);
            let body = self.lower_resolved_node(node)?;
            self.building.pop();
            self.blocks.insert(block_id, body);
        }
        Ok(vec![JsonOp::CallBlock(block_id)])
    }

    fn lower_resolved_node(&mut self, node: &TypePlanNode) -> LowerResult<JsonProgram> {
        let mut program = vec![JsonOp::EnterShape { shape: node.shape }];

        match &node.strategy {
            DeserStrategy::ContainerProxy => {
                let proxy = self.proxy_program(node)?;
                program.push(JsonOp::Proxy { proxy });
            }
            DeserStrategy::FieldProxy => {
                let proxy = self.proxy_program(node)?;
                program.push(JsonOp::Proxy { proxy });
            }
            DeserStrategy::Pointer { pointee_node } => {
                program.push(JsonOp::Pointer {
                    pointee: self.lower_node_id(*pointee_node)?,
                });
            }
            DeserStrategy::OpaquePointer => program.push(JsonOp::OpaquePointer),
            DeserStrategy::Opaque => program.push(JsonOp::Opaque),
            DeserStrategy::TransparentConvert { inner_node } => {
                program.push(JsonOp::Transparent {
                    inner: self.lower_node_id(*inner_node)?,
                });
            }
            DeserStrategy::Scalar {
                scalar_type,
                is_from_str,
            } => {
                program.push(JsonOp::Scalar(JsonScalar {
                    ty: *scalar_type,
                    from_str: *is_from_str,
                }));
            }
            DeserStrategy::Struct => {
                let struct_plan = self
                    .core
                    .as_struct_plan(node)
                    .ok_or(LowerError::MissingStructPlan { shape: node.shape })?;
                match struct_plan.struct_def.kind {
                    StructKind::Struct | StructKind::Unit => {
                        program.push(JsonOp::Struct(self.lower_struct(
                            self.core.fields(struct_plan.fields),
                            struct_plan.has_flatten,
                            struct_plan.deny_unknown_fields,
                        )?));
                    }
                    StructKind::Tuple | StructKind::TupleStruct => {
                        let elements =
                            self.lower_fields_as_programs(self.core.fields(struct_plan.fields))?;
                        program.push(JsonOp::Tuple {
                            elements,
                            single_field_transparent: false,
                        });
                    }
                }
            }
            DeserStrategy::Tuple {
                is_single_field_transparent,
                ..
            } => {
                let struct_plan = self
                    .core
                    .as_struct_plan(node)
                    .ok_or(LowerError::MissingStructPlan { shape: node.shape })?;
                let elements =
                    self.lower_fields_as_programs(self.core.fields(struct_plan.fields))?;
                program.push(JsonOp::Tuple {
                    elements,
                    single_field_transparent: *is_single_field_transparent,
                });
            }
            DeserStrategy::Enum => {
                let enum_plan = self
                    .core
                    .as_enum_plan(node)
                    .ok_or(LowerError::MissingEnumPlan { shape: node.shape })?;
                let variants = self.core.variants(enum_plan.variants);
                let lowered_variants = self.lower_variants(variants)?;
                let other = enum_plan
                    .other_variant_idx
                    .map(|idx| {
                        lowered_variants
                            .get(idx)
                            .cloned()
                            .ok_or(LowerError::MissingOtherVariant {
                                shape: node.shape,
                                index: idx,
                            })
                    })
                    .transpose()?;
                program.push(JsonOp::Enum(JsonEnum {
                    repr: JsonEnumRepr::from_shape(node.shape),
                    variants: lowered_variants,
                    other,
                }));
            }
            DeserStrategy::Option { some_node } => {
                program.push(JsonOp::Option {
                    some: self.lower_node_id(*some_node)?,
                });
            }
            DeserStrategy::Result { ok_node, err_node } => {
                program.push(JsonOp::Result {
                    ok: self.lower_node_id(*ok_node)?,
                    err: self.lower_node_id(*err_node)?,
                });
            }
            DeserStrategy::List {
                item_node,
                is_byte_vec,
            } => {
                program.push(JsonOp::List {
                    element: self.lower_node_id(*item_node)?,
                    byte_optimized: *is_byte_vec,
                });
            }
            DeserStrategy::Map {
                key_node,
                value_node,
            } => {
                program.push(JsonOp::Map {
                    key: self.lower_node_id(*key_node)?,
                    value: self.lower_node_id(*value_node)?,
                });
            }
            DeserStrategy::Set { item_node } => {
                program.push(JsonOp::Set {
                    element: self.lower_node_id(*item_node)?,
                });
            }
            DeserStrategy::Array { len, item_node } => {
                program.push(JsonOp::Array {
                    len: *len,
                    element: self.lower_node_id(*item_node)?,
                });
            }
            DeserStrategy::DynamicValue => program.push(JsonOp::Dynamic),
            DeserStrategy::MetadataContainer => program.push(JsonOp::MetadataContainer),
            DeserStrategy::BackRef { .. } => unreachable!("BackRef handled before lowering node"),
            _ => {
                return Err(LowerError::UnsupportedStrategy { shape: node.shape });
            }
        }

        Ok(program)
    }

    fn proxy_program(&mut self, node: &TypePlanNode) -> LowerResult<JsonProgram> {
        let proxy_node = node
            .proxies
            .node_for(Some("json"))
            .ok_or(LowerError::MissingProxyNode { shape: node.shape })?;
        self.lower_node_id(proxy_node)
    }

    fn lower_struct(
        &mut self,
        fields: &[FieldPlan],
        has_flatten: bool,
        deny_unknown_fields: bool,
    ) -> LowerResult<JsonStruct> {
        let unknown_fields = if has_flatten {
            UnknownFields::Capture
        } else if deny_unknown_fields {
            UnknownFields::Deny
        } else {
            UnknownFields::Ignore
        };
        Ok(JsonStruct {
            fields: fields
                .iter()
                .map(|field| self.lower_field(field))
                .collect::<LowerResult<_>>()?,
            unknown_fields,
        })
    }

    fn lower_field(&mut self, field: &FieldPlan) -> LowerResult<JsonField> {
        Ok(JsonField {
            name: field.effective_name,
            alias: field.alias,
            value: self.lower_field_value(field)?,
            missing: missing_field(field),
            flattened: field.is_flattened,
            skip_deserializing: field.field.should_skip_deserializing(),
        })
    }

    fn lower_field_value(&mut self, field: &FieldPlan) -> LowerResult<JsonProgram> {
        let child = self.core.node(field.type_node);
        if let DeserStrategy::FieldProxy = child.strategy {
            let proxy_node =
                child
                    .proxies
                    .node_for(Some("json"))
                    .ok_or(LowerError::MissingProxyNode {
                        shape: field.field_shape,
                    })?;
            self.lower_node_id(proxy_node)
        } else {
            self.lower_node(child)
        }
    }

    fn lower_fields_as_programs(&mut self, fields: &[FieldPlan]) -> LowerResult<Vec<JsonProgram>> {
        fields
            .iter()
            .map(|field| self.lower_field_value(field))
            .collect()
    }

    fn lower_variants(&mut self, variants: &[VariantPlanMeta]) -> LowerResult<Vec<JsonVariant>> {
        variants
            .iter()
            .map(|variant| self.lower_variant(variant))
            .collect()
    }

    fn lower_variant(&mut self, variant: &VariantPlanMeta) -> LowerResult<JsonVariant> {
        Ok(JsonVariant {
            name: variant.variant.effective_name(),
            payload: self.lower_variant_payload(variant)?,
        })
    }

    fn lower_variant_payload(
        &mut self,
        variant: &VariantPlanMeta,
    ) -> LowerResult<JsonVariantPayload> {
        let fields = self.core.fields(variant.fields);
        Ok(match variant.variant.data.kind {
            StructKind::Unit => JsonVariantPayload::Unit,
            StructKind::Struct => {
                JsonVariantPayload::Struct(self.lower_struct(fields, variant.has_flatten, false)?)
            }
            StructKind::Tuple | StructKind::TupleStruct if fields.len() == 1 => {
                JsonVariantPayload::Newtype(self.lower_field_value(&fields[0])?)
            }
            StructKind::Tuple | StructKind::TupleStruct => {
                JsonVariantPayload::Tuple(self.lower_fields_as_programs(fields)?)
            }
        })
    }
}

fn missing_field(field: &FieldPlan) -> MissingField {
    if field.field.should_skip_deserializing() {
        return MissingField::Omit;
    }

    match field.fill_rule {
        FillRule::Required => MissingField::Required,
        FillRule::Defaultable(_) => MissingField::Default,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(facet::Facet)]
    struct Person {
        name: String,
        age: u32,
    }

    #[derive(facet::Facet)]
    struct Node {
        value: u32,
        next: Option<Box<Node>>,
    }

    #[test]
    fn lowers_struct_fields_from_type_plan() {
        let core = facet_reflect::TypePlan::<Person>::build().unwrap().core();
        let lowered = lower_type_plan(&core).unwrap();

        let [JsonOp::EnterShape { .. }, JsonOp::Struct(plan)] = lowered.program.as_slice() else {
            panic!("expected enter-shape plus struct op");
        };

        assert_eq!(plan.unknown_fields, UnknownFields::Ignore);
        assert_eq!(plan.fields.len(), 2);
        assert_eq!(plan.fields[0].name, "name");
        assert_eq!(plan.fields[0].missing, MissingField::Required);
        assert_eq!(plan.fields[1].name, "age");
        assert!(lowered.blocks.is_empty());
    }

    #[test]
    fn recursive_shapes_lower_to_blocks() {
        let core = facet_reflect::TypePlan::<Node>::build().unwrap().core();
        let lowered = lower_type_plan(&core).unwrap();

        assert!(!lowered.blocks.is_empty());
        assert!(program_contains_call_block(&lowered.program));
    }

    fn program_contains_call_block(program: &JsonProgram) -> bool {
        program.iter().any(op_contains_call_block)
    }

    fn op_contains_call_block(op: &JsonOp) -> bool {
        match op {
            JsonOp::CallBlock(_) => true,
            JsonOp::Tuple { elements, .. } => elements.iter().any(program_contains_call_block),
            JsonOp::List { element, .. }
            | JsonOp::Array { element, .. }
            | JsonOp::Set { element }
            | JsonOp::Option { some: element }
            | JsonOp::Pointer { pointee: element }
            | JsonOp::Transparent { inner: element }
            | JsonOp::Proxy { proxy: element } => program_contains_call_block(element),
            JsonOp::Map { key, value } => {
                program_contains_call_block(key) || program_contains_call_block(value)
            }
            JsonOp::Result { ok, err } => {
                program_contains_call_block(ok) || program_contains_call_block(err)
            }
            JsonOp::Struct(plan) => plan
                .fields
                .iter()
                .any(|field| program_contains_call_block(&field.value)),
            JsonOp::Enum(en) => {
                en.variants
                    .iter()
                    .any(|variant| variant_payload_contains_call_block(&variant.payload))
                    || en.other.as_ref().is_some_and(|variant| {
                        variant_payload_contains_call_block(&variant.payload)
                    })
            }
            JsonOp::EnterShape { .. }
            | JsonOp::Null
            | JsonOp::Scalar(_)
            | JsonOp::String(_)
            | JsonOp::Bytes(_)
            | JsonOp::RawJson
            | JsonOp::Dynamic
            | JsonOp::OpaquePointer
            | JsonOp::Opaque
            | JsonOp::MetadataContainer
            | JsonOp::Return => false,
        }
    }

    fn variant_payload_contains_call_block(payload: &JsonVariantPayload) -> bool {
        match payload {
            JsonVariantPayload::Unit => false,
            JsonVariantPayload::Newtype(program) => program_contains_call_block(program),
            JsonVariantPayload::Tuple(programs) => programs.iter().any(program_contains_call_block),
            JsonVariantPayload::Struct(plan) => plan
                .fields
                .iter()
                .any(|field| program_contains_call_block(&field.value)),
        }
    }
}
