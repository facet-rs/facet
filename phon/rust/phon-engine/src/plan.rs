//! Compatibility planning: reconcile a *writer* schema with a *reader* schema
//! into a translation [`Plan`], then decode the writer's compact bytes into a
//! reader-shaped [`Value`].
//!
//! The plan is built from the two schemas alone, before any payload is touched
//! (`r[compat.plan-first]`): if it cannot be built the schemas are incompatible
//! and decoding never begins. Struct fields are matched by name
//! (`r[compat.field-matching]`) — writer-only fields are skipped
//! (`r[compat.skip-writer-only]`), reader-only fields are defaulted or, when
//! required, fail the plan (`r[compat.reader-only-fields]`). Enum variants are
//! matched by name (`r[compat.enum]`). Types match only by the rules of
//! `r[compat.type-match]` — no implicit numeric widening.
//!
//! This is the dynamic-`Value` path: reader-only fields default to `null` (the
//! typed path will use a descriptor-supplied default instead). It builds on the
//! compact codec's registry, resolution, and decoders.
//!
//! Spec: "Compatibility".

use std::collections::{HashMap, HashSet};

use facet_value::{VArray, VObject, VString, Value};
use phon_schema::bytes::Reader;
use phon_schema::{
    DecodeError, Field, Primitive, SchemaId, SchemaKind, SchemaRef, Variant, VariantPayload,
    read_value,
};

use phon_ir::ir::{EnumArm, Op, Program};

use crate::compact::{self, CompactError, Registry, Resolved};

type Result<T> = core::result::Result<T, CompactError>;

const MAX_DEPTH: usize = 128;

// ============================================================================
// Plan
// ============================================================================

/// A built translation plan from a writer schema to a reader schema. Build it
/// once with [`build_plan`], then decode many messages with [`decode_with_plan`].
pub struct Plan(Node);

enum Node {
    /// A primitive copied through (writer and reader primitive are identical).
    Scalar(Primitive),
    Struct(StructPlan),
    /// Writer variant index -> how to produce the reader's variant. An index
    /// absent here is a writer-only variant: a decode error if it arrives.
    Enum(HashMap<u32, VariantPlan>),
    Tuple(Vec<Node>),
    /// A `list` (`set == false`) or `set` (`set == true`).
    Seq { set: bool, element: Box<Node> },
    Map { key: Box<Node>, value: Box<Node> },
    Array { element: Box<Node>, dims: Vec<u64> },
    Option(Box<Node>),
    Dynamic,
}

struct StructPlan {
    /// One step per writer field, in wire order.
    steps: Vec<Step>,
    /// Reader-only, non-required field names to fill with a default.
    defaults: Vec<String>,
}

enum Step {
    /// A writer field matched to this reader field; decode it with `node`.
    Take { reader: String, node: Node },
    /// A writer-only field: decode it by its writer schema and discard.
    Skip(SchemaRef),
}

struct VariantPlan {
    reader: String,
    payload: Payload,
}

enum Payload {
    Unit,
    Newtype(Box<Node>),
    Tuple(Vec<Node>),
    Struct(StructPlan),
}

// ============================================================================
// Public API
// ============================================================================

/// Build the translation plan from `writer_root` to `reader_root`.
///
/// # Errors
/// [`CompactError::Incompatible`] (or a resolution error) if the schemas cannot
/// be reconciled.
pub fn build_plan(writer_root: SchemaId, reader_root: SchemaId, reg: &Registry) -> Result<Plan> {
    let node = plan_ref(
        &SchemaRef::concrete(writer_root),
        &SchemaRef::concrete(reader_root),
        reg,
        0,
    )?;
    Ok(Plan(node))
}

/// Decode writer compact `bytes` into a reader-shaped value using a prebuilt plan.
///
/// # Errors
/// [`CompactError`] for malformed input, or a writer-only enum variant.
pub fn decode_with_plan(bytes: &[u8], plan: &Plan, reg: &Registry) -> Result<Value> {
    let mut r = Reader::new(bytes);
    let v = exec(&plan.0, &mut r, reg, 0)?;
    if r.remaining() != 0 {
        return Err(CompactError::Decode(DecodeError::TrailingBytes(r.remaining())));
    }
    Ok(v)
}

/// Build a plan and decode in one step.
///
/// # Errors
/// As [`build_plan`] and [`decode_with_plan`].
pub fn decode(bytes: &[u8], writer_root: SchemaId, reader_root: SchemaId, reg: &Registry) -> Result<Value> {
    let plan = build_plan(writer_root, reader_root, reg)?;
    decode_with_plan(bytes, &plan, reg)
}

/// Build a plan, lower it to the linear IR, and run the interpreter — the flat
/// counterpart to [`decode`]. The interpreter must produce the same value the
/// recursive [`decode_with_plan`] would; the drift tests assert exactly that.
///
/// # Errors
/// As [`build_plan`] and [`crate::interp::run`].
pub fn decode_via_ir(
    bytes: &[u8],
    writer_root: SchemaId,
    reader_root: SchemaId,
    reg: &Registry,
) -> Result<Value> {
    let plan = build_plan(writer_root, reader_root, reg)?;
    let program = lower(&plan);
    crate::interp::run(&program, bytes, reg)
}

// ============================================================================
// Building the plan
// ============================================================================

fn incompatible(why: impl Into<String>) -> CompactError {
    CompactError::Incompatible(why.into())
}

fn plan_ref(w: &SchemaRef, r: &SchemaRef, reg: &Registry, depth: usize) -> Result<Node> {
    if depth > MAX_DEPTH {
        return Err(incompatible("schema nests too deep"));
    }
    plan_resolved(compact::resolve(reg, w)?, compact::resolve(reg, r)?, reg, depth)
}

fn plan_resolved(w: Resolved, r: Resolved, reg: &Registry, depth: usize) -> Result<Node> {
    match (w, r) {
        (Resolved::Primitive(wp), Resolved::Primitive(rp)) => {
            if wp == rp {
                Ok(Node::Scalar(wp))
            } else {
                Err(incompatible(format!("primitive {wp:?} is not {rp:?}")))
            }
        }
        (Resolved::Composite(wk), Resolved::Composite(rk)) => plan_kind(wk, rk, reg, depth),
        _ => Err(incompatible("primitive does not match composite")),
    }
}

// r[impl compat.type-match]
fn plan_kind(wk: SchemaKind, rk: SchemaKind, reg: &Registry, depth: usize) -> Result<Node> {
    match (wk, rk) {
        (SchemaKind::Primitive(wp), SchemaKind::Primitive(rp)) => {
            if wp == rp {
                Ok(Node::Scalar(wp))
            } else {
                Err(incompatible(format!("primitive {wp:?} is not {rp:?}")))
            }
        }
        (SchemaKind::Struct { fields: wf, .. }, SchemaKind::Struct { fields: rf, .. }) => {
            Ok(Node::Struct(plan_struct(&wf, &rf, reg, depth)?))
        }
        (SchemaKind::Enum { variants: wv, .. }, SchemaKind::Enum { variants: rv, .. }) => {
            plan_enum(&wv, &rv, reg, depth)
        }
        (SchemaKind::Tuple { elements: we }, SchemaKind::Tuple { elements: re }) => {
            if we.len() != re.len() {
                return Err(incompatible("tuple arity differs"));
            }
            let mut nodes = Vec::with_capacity(we.len());
            for (w, r) in we.iter().zip(&re) {
                nodes.push(plan_ref(w, r, reg, depth + 1)?);
            }
            Ok(Node::Tuple(nodes))
        }
        (SchemaKind::List { element: we }, SchemaKind::List { element: re }) => Ok(Node::Seq {
            set: false,
            element: Box::new(plan_ref(&we, &re, reg, depth + 1)?),
        }),
        (SchemaKind::Set { element: we }, SchemaKind::Set { element: re }) => Ok(Node::Seq {
            set: true,
            element: Box::new(plan_ref(&we, &re, reg, depth + 1)?),
        }),
        (SchemaKind::Option { element: we }, SchemaKind::Option { element: re }) => {
            Ok(Node::Option(Box::new(plan_ref(&we, &re, reg, depth + 1)?)))
        }
        (
            SchemaKind::Map { key: wk, value: wv },
            SchemaKind::Map { key: rk, value: rv },
        ) => Ok(Node::Map {
            key: Box::new(plan_ref(&wk, &rk, reg, depth + 1)?),
            value: Box::new(plan_ref(&wv, &rv, reg, depth + 1)?),
        }),
        (
            SchemaKind::Array {
                element: we,
                dimensions: wd,
            },
            SchemaKind::Array {
                element: re,
                dimensions: rd,
            },
        ) => {
            if wd != rd {
                return Err(incompatible("array dimensions differ"));
            }
            Ok(Node::Array {
                element: Box::new(plan_ref(&we, &re, reg, depth + 1)?),
                dims: wd,
            })
        }
        (SchemaKind::Dynamic, SchemaKind::Dynamic) => Ok(Node::Dynamic),
        (SchemaKind::Tensor { .. }, SchemaKind::Tensor { .. }) => {
            Err(CompactError::Unsupported("tensor"))
        }
        (SchemaKind::Channel { .. }, SchemaKind::Channel { .. }) => {
            Err(CompactError::Unsupported("channel"))
        }
        (SchemaKind::External { .. }, SchemaKind::External { .. }) => {
            Err(CompactError::Unsupported("external"))
        }
        _ => Err(incompatible("schema kinds differ")),
    }
}

// r[impl compat.field-matching]
fn plan_struct(
    w_fields: &[Field],
    r_fields: &[Field],
    reg: &Registry,
    depth: usize,
) -> Result<StructPlan> {
    let reader_by_name: HashMap<&str, &Field> =
        r_fields.iter().map(|f| (f.name.as_str(), f)).collect();

    let mut steps = Vec::with_capacity(w_fields.len());
    let mut matched: HashSet<&str> = HashSet::new();
    for wf in w_fields {
        if let Some(rf) = reader_by_name.get(wf.name.as_str()) {
            let node = plan_ref(&wf.schema, &rf.schema, reg, depth + 1)?;
            steps.push(Step::Take {
                reader: rf.name.clone(),
                node,
            });
            matched.insert(rf.name.as_str());
        } else {
            steps.push(Step::Skip(wf.schema.clone()));
        }
    }

    let mut defaults = Vec::new();
    for rf in r_fields {
        if !matched.contains(rf.name.as_str()) {
            if rf.required {
                return Err(incompatible(format!(
                    "required reader field '{}' is absent from the writer",
                    rf.name
                )));
            }
            defaults.push(rf.name.clone());
        }
    }

    Ok(StructPlan { steps, defaults })
}

// r[impl compat.enum]
fn plan_enum(
    w_variants: &[Variant],
    r_variants: &[Variant],
    reg: &Registry,
    depth: usize,
) -> Result<Node> {
    let reader_by_name: HashMap<&str, &Variant> =
        r_variants.iter().map(|v| (v.name.as_str(), v)).collect();

    let mut by_index = HashMap::new();
    for wv in w_variants {
        // A writer variant the reader lacks gets no entry: receiving it at
        // runtime is a decode error, but its absence here is fine.
        if let Some(rv) = reader_by_name.get(wv.name.as_str()) {
            let payload = plan_payload(&wv.payload, &rv.payload, reg, depth)?;
            by_index.insert(
                wv.index,
                VariantPlan {
                    reader: rv.name.clone(),
                    payload,
                },
            );
        }
    }
    Ok(Node::Enum(by_index))
}

fn plan_payload(
    w: &VariantPayload,
    r: &VariantPayload,
    reg: &Registry,
    depth: usize,
) -> Result<Payload> {
    match (w, r) {
        (VariantPayload::Unit, VariantPayload::Unit) => Ok(Payload::Unit),
        (VariantPayload::Newtype(wr), VariantPayload::Newtype(rr)) => {
            Ok(Payload::Newtype(Box::new(plan_ref(wr, rr, reg, depth + 1)?)))
        }
        (VariantPayload::Tuple(wrs), VariantPayload::Tuple(rrs)) => {
            if wrs.len() != rrs.len() {
                return Err(incompatible("variant tuple arity differs"));
            }
            let mut nodes = Vec::with_capacity(wrs.len());
            for (w, r) in wrs.iter().zip(rrs) {
                nodes.push(plan_ref(w, r, reg, depth + 1)?);
            }
            Ok(Payload::Tuple(nodes))
        }
        (VariantPayload::Struct(wfs), VariantPayload::Struct(rfs)) => {
            Ok(Payload::Struct(plan_struct(wfs, rfs, reg, depth)?))
        }
        _ => Err(incompatible("variant payload shapes differ")),
    }
}

// ============================================================================
// Lowering the plan to the linear IR
// ============================================================================

/// Flatten a built plan's `Node` tree into a linear [`Program`]. Every
/// type-directed decision the tree encodes is resolved here, once; what the
/// interpreter runs carries only data-directed control flow. A struct of structs
/// of scalars lowers to a single branch-free run of ops.
// r[impl ir.two-forms]
#[must_use]
pub fn lower(plan: &Plan) -> Program {
    let mut out = Vec::new();
    lower_node(&plan.0, &mut out);
    out
}

fn lower_subtree(node: &Node) -> Program {
    let mut out = Vec::new();
    lower_node(node, &mut out);
    out
}

/// Append the ops for `node`. A complete node nets one value on the stack.
fn lower_node(node: &Node, out: &mut Program) {
    match node {
        Node::Scalar(p) => out.push(Op::Scalar(*p)),
        Node::Dynamic => out.push(Op::Dynamic),
        Node::Struct(sp) => lower_struct(sp, out),
        Node::Enum(by_index) => {
            let mut arms: Vec<EnumArm> = by_index
                .iter()
                .map(|(idx, vp)| EnumArm {
                    writer_index: *idx,
                    reader_name: vp.reader.clone(),
                    payload: lower_payload(&vp.payload),
                })
                .collect();
            // Deterministic order; the interpreter dispatches by writer_index.
            arms.sort_by_key(|a| a.writer_index);
            out.push(Op::Enum { arms });
        }
        Node::Tuple(nodes) => {
            for n in nodes {
                lower_node(n, out);
            }
            out.push(Op::Array { count: nodes.len() });
        }
        Node::Seq { set, element } => out.push(Op::Seq {
            set: *set,
            body: lower_subtree(element),
        }),
        Node::Map { key, value } => out.push(Op::Map {
            key: lower_subtree(key),
            value: lower_subtree(value),
        }),
        Node::Array { element, dims } => out.push(Op::FixedArray {
            dimensions: dims.clone(),
            body: lower_subtree(element),
        }),
        Node::Option(element) => out.push(Op::Option {
            some: lower_subtree(element),
        }),
    }
}

/// Each writer field in wire order (a matched field's value, or a skip for a
/// writer-only one), then a null per reader-only default, then assemble the
/// object. The `keys` track the stack values the `Object` op will name.
///
/// A field that is itself a fixed structure splices its ops inline here (via
/// `lower_node`); only dynamic-length and branching children become sub-programs.
// r[impl ir.inlining]
fn lower_struct(sp: &StructPlan, out: &mut Program) {
    let mut keys = Vec::new();
    for step in &sp.steps {
        match step {
            Step::Take { reader, node } => {
                lower_node(node, out);
                keys.push(reader.clone());
            }
            Step::Skip(writer_ref) => out.push(Op::Skip(writer_ref.clone())),
        }
    }
    for name in &sp.defaults {
        out.push(Op::Null);
        keys.push(name.clone());
    }
    out.push(Op::Object { keys });
}

fn lower_payload(payload: &Payload) -> Program {
    let mut out = Vec::new();
    match payload {
        Payload::Unit => out.push(Op::Null),
        Payload::Newtype(node) => lower_node(node, &mut out),
        Payload::Tuple(nodes) => {
            for n in nodes {
                lower_node(n, &mut out);
            }
            out.push(Op::Array { count: nodes.len() });
        }
        Payload::Struct(sp) => lower_struct(sp, &mut out),
    }
    out
}

// ============================================================================
// Executing the plan
// ============================================================================

fn exec(node: &Node, r: &mut Reader, reg: &Registry, depth: usize) -> Result<Value> {
    if depth > MAX_DEPTH {
        return Err(CompactError::Decode(DecodeError::DepthExceeded));
    }
    match node {
        Node::Scalar(p) => compact::decode_primitive(r, *p),
        Node::Struct(sp) => exec_struct(sp, r, reg, depth),
        Node::Enum(by_index) => {
            let idx = r.read_u32()?;
            let v = by_index
                .get(&idx)
                .ok_or(CompactError::WriterOnlyVariant(idx))?;
            let payload = exec_payload(&v.payload, r, reg, depth)?;
            let mut obj = VObject::new();
            obj.insert(VString::new(&v.reader), payload);
            Ok(obj.into())
        }
        Node::Tuple(nodes) => {
            let mut a = VArray::new();
            for n in nodes {
                a.push(exec(n, r, reg, depth + 1)?);
            }
            Ok(a.into())
        }
        Node::Seq { set, element } => {
            let n = r.read_len(1)?;
            let mut a = VArray::new();
            let mut seen = if *set { Some(HashSet::new()) } else { None };
            for _ in 0..n {
                let v = exec(element, r, reg, depth + 1)?;
                if let Some(s) = &mut seen
                    && !s.insert(v.clone())
                {
                    return Err(CompactError::Decode(DecodeError::DuplicateElement));
                }
                a.push(v);
            }
            Ok(a.into())
        }
        Node::Map { key, value } => {
            let n = r.read_len(1)?;
            let mut obj = VObject::new();
            for _ in 0..n {
                let k = exec(key, r, reg, depth + 1)?;
                let v = exec(value, r, reg, depth + 1)?;
                let ks = k
                    .as_string()
                    .ok_or(CompactError::Unsupported("map with non-string keys"))?;
                if obj.insert(VString::new(ks.as_str()), v).is_some() {
                    return Err(CompactError::Decode(DecodeError::DuplicateKey));
                }
            }
            Ok(obj.into())
        }
        Node::Array { element, dims } => {
            let count = compact::product(dims)?;
            if count > r.remaining() as u64 {
                return Err(CompactError::Decode(DecodeError::LengthTooLarge {
                    count,
                    remaining: r.remaining(),
                }));
            }
            let mut a = VArray::new();
            for _ in 0..count {
                a.push(exec(element, r, reg, depth + 1)?);
            }
            Ok(a.into())
        }
        Node::Option(element) => match r.read_u8()? {
            0 => Ok(Value::NULL),
            1 => exec(element, r, reg, depth + 1),
            b => Err(CompactError::Decode(DecodeError::InvalidBool(b))),
        },
        Node::Dynamic => Ok(read_value(r)?),
    }
}

fn exec_struct(sp: &StructPlan, r: &mut Reader, reg: &Registry, depth: usize) -> Result<Value> {
    let mut obj = VObject::new();
    for step in &sp.steps {
        match step {
            Step::Take { reader, node } => {
                let v = exec(node, r, reg, depth + 1)?;
                obj.insert(VString::new(reader), v);
            }
            Step::Skip(writer_ref) => {
                // Walk the writer field by its own schema and discard it.
                compact::decode_ref(r, writer_ref, reg, depth + 1)?;
            }
        }
    }
    for name in &sp.defaults {
        obj.insert(VString::new(name), Value::NULL);
    }
    Ok(obj.into())
}

fn exec_payload(p: &Payload, r: &mut Reader, reg: &Registry, depth: usize) -> Result<Value> {
    match p {
        Payload::Unit => Ok(Value::NULL),
        Payload::Newtype(n) => exec(n, r, reg, depth + 1),
        Payload::Tuple(ns) => {
            let mut a = VArray::new();
            for n in ns {
                a.push(exec(n, r, reg, depth + 1)?);
            }
            Ok(a.into())
        }
        Payload::Struct(sp) => exec_struct(sp, r, reg, depth),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compact;
    use phon_schema::{Schema, primitive_id};

    fn prim(p: Primitive) -> SchemaRef {
        SchemaRef::concrete(primitive_id(p))
    }

    fn schema(id: u64, kind: SchemaKind) -> Schema {
        Schema {
            id: SchemaId(id),
            type_params: Vec::new(),
            kind,
        }
    }

    fn field(name: &str, schema: SchemaRef, required: bool) -> Field {
        Field {
            name: name.to_string(),
            schema,
            required,
        }
    }

    fn point_struct(id: u64, fields: Vec<Field>) -> Schema {
        schema(
            id,
            SchemaKind::Struct {
                name: "P".to_string(),
                fields,
            },
        )
    }

    fn obj(entries: &[(&str, Value)]) -> Value {
        let mut o = VObject::new();
        for (k, v) in entries {
            o.insert(VString::new(k), v.clone());
        }
        o.into()
    }

    /// Decode through both the recursive `exec` and the lowered IR, assert they
    /// agree, and return the value. `exec` is the oracle for the interpreter.
    fn decode_both(bytes: &[u8], w: SchemaId, r: SchemaId, reg: &Registry) -> Value {
        let recursive = decode(bytes, w, r, reg).unwrap();
        let flat = decode_via_ir(bytes, w, r, reg).unwrap();
        assert_eq!(recursive, flat, "IR interpreter disagreed with recursive exec");
        recursive
    }

    #[test]
    fn field_reorder_is_transparent() {
        // writer: { x: u32, y: u32 }; reader: { y: u32, x: u32 }
        let writer = point_struct(
            1,
            vec![
                field("x", prim(Primitive::U32), true),
                field("y", prim(Primitive::U32), true),
            ],
        );
        let reader = point_struct(
            2,
            vec![
                field("y", prim(Primitive::U32), true),
                field("x", prim(Primitive::U32), true),
            ],
        );
        let reg = Registry::new([writer, reader]);
        let bytes = compact::to_bytes(
            &obj(&[("x", Value::from(7u32)), ("y", Value::from(9u32))]),
            SchemaId(1),
            &reg,
        )
        .unwrap();
        let got = decode_both(&bytes, SchemaId(1), SchemaId(2), &reg);
        assert_eq!(got, obj(&[("x", Value::from(7u32)), ("y", Value::from(9u32))]));
    }

    #[test]
    fn writer_only_field_is_skipped() {
        // writer: { x: u32, gone: string }; reader: { x: u32 }
        let writer = point_struct(
            1,
            vec![
                field("x", prim(Primitive::U32), true),
                field("gone", prim(Primitive::String), true),
            ],
        );
        let reader = point_struct(2, vec![field("x", prim(Primitive::U32), true)]);
        let reg = Registry::new([writer, reader]);
        let bytes = compact::to_bytes(
            &obj(&[("x", Value::from(7u32)), ("gone", Value::from("bye"))]),
            SchemaId(1),
            &reg,
        )
        .unwrap();
        let got = decode_both(&bytes, SchemaId(1), SchemaId(2), &reg);
        assert_eq!(got, obj(&[("x", Value::from(7u32))]));
    }

    #[test]
    fn reader_only_field_defaults_or_fails() {
        // writer: { x: u32 }; reader: { x: u32, extra: u32 }
        let writer = point_struct(1, vec![field("x", prim(Primitive::U32), true)]);
        let optional = point_struct(
            2,
            vec![
                field("x", prim(Primitive::U32), true),
                field("extra", prim(Primitive::U32), false), // non-required -> default
            ],
        );
        let required = point_struct(
            3,
            vec![
                field("x", prim(Primitive::U32), true),
                field("extra", prim(Primitive::U32), true), // required -> plan fails
            ],
        );
        let reg = Registry::new([writer, optional, required]);
        let bytes = compact::to_bytes(&obj(&[("x", Value::from(7u32))]), SchemaId(1), &reg).unwrap();

        let got = decode_both(&bytes, SchemaId(1), SchemaId(2), &reg);
        assert_eq!(got, obj(&[("x", Value::from(7u32)), ("extra", Value::NULL)]));

        assert!(matches!(
            build_plan(SchemaId(1), SchemaId(3), &reg),
            Err(CompactError::Incompatible(_))
        ));
        assert!(matches!(
            decode_via_ir(&bytes, SchemaId(1), SchemaId(3), &reg),
            Err(CompactError::Incompatible(_))
        ));
    }

    #[test]
    fn numeric_widening_is_not_implicit() {
        let writer = schema(1, SchemaKind::List { element: prim(Primitive::U32) });
        let reader = schema(2, SchemaKind::List { element: prim(Primitive::U64) });
        let reg = Registry::new([writer, reader]);
        assert!(matches!(
            build_plan(SchemaId(1), SchemaId(2), &reg),
            Err(CompactError::Incompatible(_))
        ));
        assert!(matches!(
            decode_via_ir(&[], SchemaId(1), SchemaId(2), &reg),
            Err(CompactError::Incompatible(_))
        ));
    }

    #[test]
    fn enum_variant_added_and_removed() {
        // writer enum { A, B(u32) }; reader enum { A, B(u32), C } (C added).
        let writer = schema(
            1,
            SchemaKind::Enum {
                name: "E".to_string(),
                variants: vec![
                    Variant { name: "A".to_string(), index: 0, payload: VariantPayload::Unit },
                    Variant {
                        name: "B".to_string(),
                        index: 1,
                        payload: VariantPayload::Newtype(prim(Primitive::U32)),
                    },
                ],
            },
        );
        let reader_more = schema(
            2,
            SchemaKind::Enum {
                name: "E".to_string(),
                variants: vec![
                    Variant { name: "A".to_string(), index: 0, payload: VariantPayload::Unit },
                    Variant {
                        name: "B".to_string(),
                        index: 1,
                        payload: VariantPayload::Newtype(prim(Primitive::U32)),
                    },
                    Variant { name: "C".to_string(), index: 2, payload: VariantPayload::Unit },
                ],
            },
        );
        // reader that lacks B: receiving B at runtime is a decode error.
        let reader_fewer = schema(
            3,
            SchemaKind::Enum {
                name: "E".to_string(),
                variants: vec![Variant {
                    name: "A".to_string(),
                    index: 0,
                    payload: VariantPayload::Unit,
                }],
            },
        );
        let reg = Registry::new([writer, reader_more, reader_fewer]);

        let b = obj(&[("B", Value::from(42u32))]);
        let bytes = compact::to_bytes(&b, SchemaId(1), &reg).unwrap();

        // reader_more can read B fine (C just goes unused).
        assert_eq!(decode_both(&bytes, SchemaId(1), SchemaId(2), &reg), b);

        // reader_fewer plans (A matches), but receiving B is a decode error.
        assert!(matches!(
            decode(&bytes, SchemaId(1), SchemaId(3), &reg),
            Err(CompactError::WriterOnlyVariant(1))
        ));
        assert!(matches!(
            decode_via_ir(&bytes, SchemaId(1), SchemaId(3), &reg),
            Err(CompactError::WriterOnlyVariant(1))
        ));
        // an A value still decodes against reader_fewer.
        let a = obj(&[("A", Value::NULL)]);
        let a_bytes = compact::to_bytes(&a, SchemaId(1), &reg).unwrap();
        assert_eq!(decode_both(&a_bytes, SchemaId(1), SchemaId(3), &reg), a);
    }

    #[test]
    fn nested_struct_drift() {
        // Inner drifts (field added); Outer holds an Inner.
        let w_inner = point_struct(10, vec![field("a", prim(Primitive::U32), true)]);
        let r_inner = point_struct(
            20,
            vec![
                field("a", prim(Primitive::U32), true),
                field("b", prim(Primitive::Bool), false),
            ],
        );
        let w_outer = schema(
            1,
            SchemaKind::Struct {
                name: "Outer".to_string(),
                fields: vec![field("inner", SchemaRef::concrete(SchemaId(10)), true)],
            },
        );
        let r_outer = schema(
            2,
            SchemaKind::Struct {
                name: "Outer".to_string(),
                fields: vec![field("inner", SchemaRef::concrete(SchemaId(20)), true)],
            },
        );
        let reg = Registry::new([w_inner, r_inner, w_outer, r_outer]);
        let bytes = compact::to_bytes(
            &obj(&[("inner", obj(&[("a", Value::from(5u32))]))]),
            SchemaId(1),
            &reg,
        )
        .unwrap();
        let got = decode_both(&bytes, SchemaId(1), SchemaId(2), &reg);
        assert_eq!(
            got,
            obj(&[("inner", obj(&[("a", Value::from(5u32)), ("b", Value::NULL)]))])
        );
    }
}
