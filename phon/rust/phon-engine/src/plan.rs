//! Compatibility planning: translate a *writer* schema with a *reader* schema
//! into a [`Plan`], then decode the writer's compact bytes into a
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

use std::collections::{BTreeMap, HashMap, HashSet};

use facet_value::{VArray, VObject, VString, Value};
use phon_schema::bytes::Reader;
use phon_schema::{
    DecodeError, Field, Primitive, SchemaId, SchemaKind, SchemaRef, Variant, VariantPayload,
    read_value,
};

use phon_ir::ir::{EnumArm, Op, Program, ValueProgram};

use crate::compact::{self, CompactError, Registry, Resolved};
use crate::compat::{self, FieldMatch, VariantMatch, incompatible};

type Result<T> = core::result::Result<T, CompactError>;

const MAX_DEPTH: usize = 128;

// ============================================================================
// Plan
// ============================================================================

/// A built translation plan from a writer schema to a reader schema. Build it
/// once with [`build_plan`], then decode many messages with [`decode_with_plan`].
pub struct Plan {
    root: Node,
    blocks: BTreeMap<SchemaId, Node>,
}

enum Node {
    /// A primitive copied through (writer and reader primitive are identical).
    Scalar(Primitive),
    /// A back-edge into a recursive reader schema, resolved through
    /// [`Plan::blocks`] at execution/lowering time.
    CallBlock(SchemaId),
    Struct(StructPlan),
    /// Writer variant index -> how to produce the reader's variant. An index
    /// absent here is a writer-only variant: a decode error if it arrives.
    Enum(HashMap<u32, VariantPlan>),
    Tuple(Vec<Node>),
    /// A `list` (`set == false`) or `set` (`set == true`). `min_wire` is the
    /// element's minimum wire size for the `r[validate.lengths]` count guard —
    /// `0` for a zero-sized element (an empty struct, `unit`, …), else `1`.
    Seq {
        set: bool,
        element: Box<Node>,
        min_wire: usize,
    },
    Map {
        key: Box<Node>,
        value: Box<Node>,
    },
    /// A fixed-shape array. `min_wire` bounds `product(dims)` exactly as in `Seq`.
    Array {
        element: Box<Node>,
        dims: Vec<u64>,
        min_wire: usize,
    },
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

struct RecCtx {
    recursive: HashSet<SchemaId>,
    blocks: BTreeMap<SchemaId, Node>,
    building: HashSet<SchemaId>,
}

// ============================================================================
// Public API
// ============================================================================

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CompatDirection {
    /// The newer schema can read bytes written by the older schema.
    Backward,
    /// The older schema can read bytes written by the newer schema.
    Forward,
    /// Both schema versions can read each other's bytes.
    Bidirectional,
    /// Neither schema version can read the other's bytes.
    Incompatible,
}

/// Build the translation plan from `writer_root` to `reader_root`.
///
/// # Errors
/// [`CompactError::Incompatible`] (or a resolution error) if the schemas cannot
/// be translated.
// r[impl compat.plan-first]
pub fn build_plan(writer_root: SchemaId, reader_root: SchemaId, reg: &Registry) -> Result<Plan> {
    let mut rec = RecCtx {
        recursive: recursive_schema_ids(reader_root, reg),
        blocks: BTreeMap::new(),
        building: HashSet::new(),
    };
    let root = plan_ref(
        &SchemaRef::concrete(writer_root),
        &SchemaRef::concrete(reader_root),
        reg,
        &mut rec,
        0,
    )?;
    Ok(Plan {
        root,
        blocks: rec.blocks,
    })
}

/// Classify compatibility between an older and newer schema by planning both
/// directions. This is tooling over [`build_plan`], not a decode path.
// r[impl compat.direction]
#[must_use]
pub fn compatibility_direction(
    older_root: SchemaId,
    newer_root: SchemaId,
    reg: &Registry,
) -> CompatDirection {
    let backward = build_plan(older_root, newer_root, reg).is_ok();
    let forward = build_plan(newer_root, older_root, reg).is_ok();
    match (backward, forward) {
        (true, true) => CompatDirection::Bidirectional,
        (true, false) => CompatDirection::Backward,
        (false, true) => CompatDirection::Forward,
        (false, false) => CompatDirection::Incompatible,
    }
}

/// Decode writer compact `bytes` into a reader-shaped value using a prebuilt plan.
///
/// # Errors
/// [`CompactError`] for malformed input, or a writer-only enum variant.
pub fn decode_with_plan(bytes: &[u8], plan: &Plan, reg: &Registry) -> Result<Value> {
    let mut r = Reader::new(bytes);
    let v = exec(&plan.root, &mut r, reg, &plan.blocks, 0)?;
    if r.remaining() != 0 {
        return Err(CompactError::Decode(DecodeError::TrailingBytes(
            r.remaining(),
        )));
    }
    Ok(v)
}

/// Build a plan and decode in one step.
///
/// # Errors
/// As [`build_plan`] and [`decode_with_plan`].
pub fn decode(
    bytes: &[u8],
    writer_root: SchemaId,
    reader_root: SchemaId,
    reg: &Registry,
) -> Result<Value> {
    let plan = build_plan(writer_root, reader_root, reg)?;
    decode_with_plan(bytes, &plan, reg)
}

/// Build a plan, lower it to the linear IR, and run the interpreter — the flat
/// counterpart to [`decode`]. The interpreter must produce the same value the
/// recursive [`decode_with_plan`] would; the compat tests assert exactly that.
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
    crate::interp::run_lowered(&program, bytes, reg)
}

// ============================================================================
// Building the plan
// ============================================================================

fn plan_ref(
    w: &SchemaRef,
    r: &SchemaRef,
    reg: &Registry,
    rec: &mut RecCtx,
    depth: usize,
) -> Result<Node> {
    if depth > MAX_DEPTH {
        return Err(incompatible("schema nests too deep"));
    }
    if let SchemaRef::Concrete { id, .. } = r
        && rec.recursive.contains(id)
    {
        if !rec.blocks.contains_key(id) && !rec.building.contains(id) {
            rec.building.insert(*id);
            let body = plan_resolved(
                compact::resolve(reg, w)?,
                compact::resolve(reg, r)?,
                reg,
                rec,
                depth,
            )?;
            rec.building.remove(id);
            rec.blocks.insert(*id, body);
        }
        return Ok(Node::CallBlock(*id));
    }
    plan_resolved(
        compact::resolve(reg, w)?,
        compact::resolve(reg, r)?,
        reg,
        rec,
        depth,
    )
}

fn plan_resolved(
    w: Resolved,
    r: Resolved,
    reg: &Registry,
    rec: &mut RecCtx,
    depth: usize,
) -> Result<Node> {
    match (w, r) {
        (Resolved::Primitive(wp), Resolved::Primitive(rp)) => {
            if wp == rp {
                Ok(Node::Scalar(wp))
            } else {
                Err(incompatible(format!("primitive {wp:?} is not {rp:?}")))
            }
        }
        (Resolved::Composite(wk), Resolved::Composite(rk)) => plan_kind(wk, rk, reg, rec, depth),
        _ => Err(incompatible("primitive does not match composite")),
    }
}

// r[impl compat.type-match]
fn plan_kind(
    wk: SchemaKind,
    rk: SchemaKind,
    reg: &Registry,
    rec: &mut RecCtx,
    depth: usize,
) -> Result<Node> {
    match (wk, rk) {
        (SchemaKind::Primitive(wp), SchemaKind::Primitive(rp)) => {
            if wp == rp {
                Ok(Node::Scalar(wp))
            } else {
                Err(incompatible(format!("primitive {wp:?} is not {rp:?}")))
            }
        }
        (SchemaKind::Struct { fields: wf, .. }, SchemaKind::Struct { fields: rf, .. }) => {
            Ok(Node::Struct(plan_struct(&wf, &rf, reg, rec, depth)?))
        }
        (SchemaKind::Enum { variants: wv, .. }, SchemaKind::Enum { variants: rv, .. }) => {
            plan_enum(&wv, &rv, reg, rec, depth)
        }
        (SchemaKind::Tuple { elements: we }, SchemaKind::Tuple { elements: re }) => {
            if we.len() != re.len() {
                return Err(incompatible("tuple arity differs"));
            }
            let mut nodes = Vec::with_capacity(we.len());
            for (w, r) in we.iter().zip(&re) {
                nodes.push(plan_ref(w, r, reg, rec, depth + 1)?);
            }
            Ok(Node::Tuple(nodes))
        }
        (SchemaKind::List { element: we }, SchemaKind::List { element: re }) => Ok(Node::Seq {
            set: false,
            min_wire: compact::min_wire_size_ref(reg, &we),
            element: Box::new(plan_ref(&we, &re, reg, rec, depth + 1)?),
        }),
        (SchemaKind::Set { element: we }, SchemaKind::Set { element: re }) => Ok(Node::Seq {
            set: true,
            min_wire: compact::min_wire_size_ref(reg, &we),
            element: Box::new(plan_ref(&we, &re, reg, rec, depth + 1)?),
        }),
        (SchemaKind::Option { element: we }, SchemaKind::Option { element: re }) => Ok(
            Node::Option(Box::new(plan_ref(&we, &re, reg, rec, depth + 1)?)),
        ),
        (SchemaKind::Map { key: wk, value: wv }, SchemaKind::Map { key: rk, value: rv }) => {
            Ok(Node::Map {
                key: Box::new(plan_ref(&wk, &rk, reg, rec, depth + 1)?),
                value: Box::new(plan_ref(&wv, &rv, reg, rec, depth + 1)?),
            })
        }
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
                min_wire: compact::min_wire_size_ref(reg, &we),
                element: Box::new(plan_ref(&we, &re, reg, rec, depth + 1)?),
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
// r[impl compat.skip-writer-only]
// r[impl compat.reader-only-fields]
// r[impl compat.defaults-are-reader-side]
fn plan_struct(
    w_fields: &[Field],
    r_fields: &[Field],
    reg: &Registry,
    rec: &mut RecCtx,
    depth: usize,
) -> Result<StructPlan> {
    let mut steps = Vec::with_capacity(w_fields.len());
    let mut defaults = Vec::new();
    for step in compat::match_fields(
        w_fields,
        r_fields,
        |_, rf| !rf.required,
        |rf| {
            incompatible(format!(
                "required reader field '{}' is absent from the writer",
                rf.name
            ))
        },
    )? {
        match step {
            FieldMatch::Take {
                writer,
                reader_index,
            } => {
                let rf = &r_fields[reader_index];
                let node = plan_ref(&writer.schema, &rf.schema, reg, rec, depth + 1)?;
                steps.push(Step::Take {
                    reader: rf.name.clone(),
                    node,
                });
            }
            FieldMatch::Skip { writer } => steps.push(Step::Skip(writer.schema.clone())),
            FieldMatch::Default { reader_index } => {
                defaults.push(r_fields[reader_index].name.clone());
            }
        }
    }

    Ok(StructPlan { steps, defaults })
}

// r[impl compat.enum]
fn plan_enum(
    w_variants: &[Variant],
    r_variants: &[Variant],
    reg: &Registry,
    rec: &mut RecCtx,
    depth: usize,
) -> Result<Node> {
    let mut by_index = HashMap::new();
    for step in compat::match_variants(w_variants, r_variants) {
        let VariantMatch::Take {
            writer,
            reader_index,
        } = step
        else {
            continue;
        };
        let rv = &r_variants[reader_index];
        let payload = plan_payload(&writer.payload, &rv.payload, reg, rec, depth)?;
        by_index.insert(
            writer.index,
            VariantPlan {
                reader: rv.name.clone(),
                payload,
            },
        );
    }
    Ok(Node::Enum(by_index))
}

fn plan_payload(
    w: &VariantPayload,
    r: &VariantPayload,
    reg: &Registry,
    rec: &mut RecCtx,
    depth: usize,
) -> Result<Payload> {
    match (w, r) {
        (VariantPayload::Unit, VariantPayload::Unit) => Ok(Payload::Unit),
        (VariantPayload::Newtype(wr), VariantPayload::Newtype(rr)) => Ok(Payload::Newtype(
            Box::new(plan_ref(wr, rr, reg, rec, depth + 1)?),
        )),
        (VariantPayload::Tuple(wrs), VariantPayload::Tuple(rrs)) => {
            if wrs.len() != rrs.len() {
                return Err(incompatible("variant tuple arity differs"));
            }
            let mut nodes = Vec::with_capacity(wrs.len());
            for (w, r) in wrs.iter().zip(rrs) {
                nodes.push(plan_ref(w, r, reg, rec, depth + 1)?);
            }
            Ok(Payload::Tuple(nodes))
        }
        (VariantPayload::Struct(wfs), VariantPayload::Struct(rfs)) => {
            Ok(Payload::Struct(plan_struct(wfs, rfs, reg, rec, depth)?))
        }
        _ => Err(incompatible("variant payload shapes differ")),
    }
}

fn recursive_schema_ids(root: SchemaId, reg: &Registry) -> HashSet<SchemaId> {
    let mut recursive = HashSet::new();
    let mut visited = HashSet::new();
    let mut on_stack = HashSet::new();
    let mut stack = Vec::new();
    visit_schema(
        root,
        reg,
        &mut recursive,
        &mut visited,
        &mut on_stack,
        &mut stack,
    );
    recursive
}

fn visit_schema(
    id: SchemaId,
    reg: &Registry,
    recursive: &mut HashSet<SchemaId>,
    visited: &mut HashSet<SchemaId>,
    on_stack: &mut HashSet<SchemaId>,
    stack: &mut Vec<SchemaId>,
) {
    visited.insert(id);
    on_stack.insert(id);
    stack.push(id);

    if let Ok(Resolved::Composite(kind)) = compact::resolve(reg, &SchemaRef::concrete(id)) {
        for target in ref_targets(&kind) {
            if on_stack.contains(&target) {
                for &candidate in stack.iter().rev() {
                    recursive.insert(candidate);
                    if candidate == target {
                        break;
                    }
                }
            } else if !visited.contains(&target) {
                visit_schema(target, reg, recursive, visited, on_stack, stack);
            }
        }
    }

    stack.pop();
    on_stack.remove(&id);
}

fn ref_targets(kind: &SchemaKind) -> Vec<SchemaId> {
    let mut out = Vec::new();
    match kind {
        SchemaKind::Struct { fields, .. } => {
            for f in fields {
                add_ref_targets(&f.schema, &mut out);
            }
        }
        SchemaKind::Enum { variants, .. } => {
            for v in variants {
                match &v.payload {
                    VariantPayload::Unit => {}
                    VariantPayload::Newtype(r) => add_ref_targets(r, &mut out),
                    VariantPayload::Tuple(refs) => {
                        for r in refs {
                            add_ref_targets(r, &mut out);
                        }
                    }
                    VariantPayload::Struct(fields) => {
                        for f in fields {
                            add_ref_targets(&f.schema, &mut out);
                        }
                    }
                }
            }
        }
        SchemaKind::Tuple { elements } => {
            for r in elements {
                add_ref_targets(r, &mut out);
            }
        }
        SchemaKind::List { element }
        | SchemaKind::Set { element }
        | SchemaKind::Array { element, .. }
        | SchemaKind::Tensor { element, .. }
        | SchemaKind::Option { element }
        | SchemaKind::Channel { element, .. } => add_ref_targets(element, &mut out),
        SchemaKind::Map { key, value } => {
            add_ref_targets(key, &mut out);
            add_ref_targets(value, &mut out);
        }
        SchemaKind::External { metadata, .. } => {
            if let Some(r) = metadata {
                add_ref_targets(r, &mut out);
            }
        }
        SchemaKind::Primitive(_) | SchemaKind::Dynamic => {}
    }
    out
}

fn add_ref_targets(r: &SchemaRef, out: &mut Vec<SchemaId>) {
    if let SchemaRef::Concrete { id, args } = r {
        out.push(*id);
        for arg in args {
            add_ref_targets(arg, out);
        }
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
pub fn lower(plan: &Plan) -> ValueProgram {
    let mut out = Vec::new();
    lower_node(&plan.root, &mut out);
    let blocks = plan
        .blocks
        .iter()
        .map(|(id, node)| (*id, lower_subtree(node)))
        .collect();
    ValueProgram {
        program: out,
        blocks,
    }
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
        Node::CallBlock(schema) => out.push(Op::CallBlock { schema: *schema }),
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
        Node::Seq {
            set,
            element,
            min_wire,
        } => out.push(Op::Seq {
            set: *set,
            min_wire: *min_wire,
            body: lower_subtree(element),
        }),
        Node::Map { key, value } => out.push(Op::Map {
            key: lower_subtree(key),
            value: lower_subtree(value),
        }),
        Node::Array {
            element,
            dims,
            min_wire,
        } => out.push(Op::FixedArray {
            dimensions: dims.clone(),
            min_wire: *min_wire,
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

fn exec(
    node: &Node,
    r: &mut Reader,
    reg: &Registry,
    blocks: &BTreeMap<SchemaId, Node>,
    depth: usize,
) -> Result<Value> {
    if depth > MAX_DEPTH {
        return Err(CompactError::Decode(DecodeError::DepthExceeded));
    }
    match node {
        Node::Scalar(p) => compact::decode_primitive(r, *p),
        Node::CallBlock(schema) => {
            let block = blocks
                .get(schema)
                .ok_or(CompactError::Decode(DecodeError::Malformed(
                    "missing recursion block",
                )))?;
            exec(block, r, reg, blocks, depth + 1)
        }
        Node::Struct(sp) => exec_struct(sp, r, reg, blocks, depth),
        Node::Enum(by_index) => {
            let idx = r.read_u32()?;
            let v = by_index
                .get(&idx)
                .ok_or(CompactError::WriterOnlyVariant(idx))?;
            let payload = exec_payload(&v.payload, r, reg, blocks, depth)?;
            let mut obj = VObject::new();
            obj.insert(VString::new(&v.reader), payload);
            Ok(obj.into())
        }
        Node::Tuple(nodes) => {
            let mut a = VArray::new();
            for n in nodes {
                a.push(exec(n, r, reg, blocks, depth + 1)?);
            }
            Ok(a.into())
        }
        Node::Seq {
            set,
            element,
            min_wire,
        } => {
            let n = r.read_len(*min_wire)?;
            let mut a = VArray::new();
            let mut seen = if *set { Some(HashSet::new()) } else { None };
            for _ in 0..n {
                let v = exec(element, r, reg, blocks, depth + 1)?;
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
                let k = exec(key, r, reg, blocks, depth + 1)?;
                let v = exec(value, r, reg, blocks, depth + 1)?;
                let ks = k
                    .as_string()
                    .ok_or(CompactError::Unsupported("map with non-string keys"))?;
                if obj.insert(VString::new(ks.as_str()), v).is_some() {
                    return Err(CompactError::Decode(DecodeError::DuplicateKey));
                }
            }
            Ok(obj.into())
        }
        Node::Array {
            element,
            dims,
            min_wire,
        } => {
            let count = compact::product(dims)?;
            compact::check_fixed_count(count, *min_wire, r.remaining())?;
            let mut a = VArray::new();
            for _ in 0..count {
                a.push(exec(element, r, reg, blocks, depth + 1)?);
            }
            Ok(a.into())
        }
        Node::Option(element) => match r.read_u8()? {
            0 => Ok(Value::NULL),
            1 => exec(element, r, reg, blocks, depth + 1),
            b => Err(CompactError::Decode(DecodeError::InvalidBool(b))),
        },
        Node::Dynamic => Ok(read_value(r)?),
    }
}

fn exec_struct(
    sp: &StructPlan,
    r: &mut Reader,
    reg: &Registry,
    blocks: &BTreeMap<SchemaId, Node>,
    depth: usize,
) -> Result<Value> {
    let mut obj = VObject::new();
    for step in &sp.steps {
        match step {
            Step::Take { reader, node } => {
                let v = exec(node, r, reg, blocks, depth + 1)?;
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

fn exec_payload(
    p: &Payload,
    r: &mut Reader,
    reg: &Registry,
    blocks: &BTreeMap<SchemaId, Node>,
    depth: usize,
) -> Result<Value> {
    match p {
        Payload::Unit => Ok(Value::NULL),
        Payload::Newtype(n) => exec(n, r, reg, blocks, depth + 1),
        Payload::Tuple(ns) => {
            let mut a = VArray::new();
            for n in ns {
                a.push(exec(n, r, reg, blocks, depth + 1)?);
            }
            Ok(a.into())
        }
        Payload::Struct(sp) => exec_struct(sp, r, reg, blocks, depth),
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
        assert_eq!(
            recursive, flat,
            "IR interpreter disagreed with recursive exec"
        );
        recursive
    }

    // r[verify compat.field-matching]
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
        assert_eq!(
            got,
            obj(&[("x", Value::from(7u32)), ("y", Value::from(9u32))])
        );
    }

    // r[verify compat.skip-writer-only]
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

    // r[verify compat.plan-first]
    // r[verify compat.reader-only-fields]
    // r[verify compat.defaults-are-reader-side]
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
        let bytes =
            compact::to_bytes(&obj(&[("x", Value::from(7u32))]), SchemaId(1), &reg).unwrap();

        let got = decode_both(&bytes, SchemaId(1), SchemaId(2), &reg);
        assert_eq!(
            got,
            obj(&[("x", Value::from(7u32)), ("extra", Value::NULL)])
        );

        assert!(matches!(
            build_plan(SchemaId(1), SchemaId(3), &reg),
            Err(CompactError::Incompatible(_))
        ));
        assert!(matches!(
            decode_via_ir(&bytes, SchemaId(1), SchemaId(3), &reg),
            Err(CompactError::Incompatible(_))
        ));
    }

    // r[verify compat.type-match]
    #[test]
    fn numeric_widening_is_not_implicit() {
        let writer = schema(
            1,
            SchemaKind::List {
                element: prim(Primitive::U32),
            },
        );
        let reader = schema(
            2,
            SchemaKind::List {
                element: prim(Primitive::U64),
            },
        );
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

    // r[verify compat.enum]
    #[test]
    fn enum_variant_added_and_removed() {
        // writer enum { A, B(u32) }; reader enum { A, B(u32), C } (C added).
        let writer = schema(
            1,
            SchemaKind::Enum {
                name: "E".to_string(),
                variants: vec![
                    Variant {
                        name: "A".to_string(),
                        index: 0,
                        payload: VariantPayload::Unit,
                    },
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
                    Variant {
                        name: "A".to_string(),
                        index: 0,
                        payload: VariantPayload::Unit,
                    },
                    Variant {
                        name: "B".to_string(),
                        index: 1,
                        payload: VariantPayload::Newtype(prim(Primitive::U32)),
                    },
                    Variant {
                        name: "C".to_string(),
                        index: 2,
                        payload: VariantPayload::Unit,
                    },
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
    fn nested_struct_compat() {
        // Inner differs (field added); Outer holds an Inner.
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
            obj(&[(
                "inner",
                obj(&[("a", Value::from(5u32)), ("b", Value::NULL)])
            )])
        );
    }

    // r[verify compat.direction]
    #[test]
    fn compatibility_direction_reports_both_ways() {
        let older = point_struct(1, vec![field("x", prim(Primitive::U32), true)]);
        let newer_optional = point_struct(
            2,
            vec![
                field("x", prim(Primitive::U32), true),
                field("y", prim(Primitive::U32), false),
            ],
        );
        let newer_required = point_struct(
            3,
            vec![
                field("x", prim(Primitive::U32), true),
                field("y", prim(Primitive::U32), true),
            ],
        );
        let different = point_struct(4, vec![field("x", prim(Primitive::U64), true)]);
        let reg = Registry::new([older, newer_optional, newer_required, different]);

        assert_eq!(
            compatibility_direction(SchemaId(1), SchemaId(2), &reg),
            CompatDirection::Bidirectional
        );
        assert_eq!(
            compatibility_direction(SchemaId(1), SchemaId(3), &reg),
            CompatDirection::Forward
        );
        assert_eq!(
            compatibility_direction(SchemaId(1), SchemaId(4), &reg),
            CompatDirection::Incompatible
        );
    }

    /// Regression (found by `tests/compat_fuzz.rs`): a `list` of *zero-sized*
    /// elements (an empty struct) encodes to nothing but its count, so after the
    /// count is read the buffer is empty. The `r[validate.lengths]` guard wrongly
    /// rejected this — it assumed every element costs at least one wire byte —
    /// even at writer==reader. The element's true minimum wire size (0 here) now
    /// flows into the guard, which falls back to a fixed count cap for zero-sized
    /// elements. Both decode paths must accept `[{}, {}, {}]`.
    #[test]
    fn list_of_zero_sized_structs_decodes() {
        // Inner = empty struct; List<Inner>.
        let inner = point_struct(1, vec![]);
        let list = schema(
            2,
            SchemaKind::List {
                element: SchemaRef::concrete(SchemaId(1)),
            },
        );
        let reg = Registry::new([inner, list]);

        // [ {}, {}, {} ] — three empty structs, zero payload bytes each.
        let mut arr = VArray::new();
        for _ in 0..3 {
            arr.push(obj(&[]));
        }
        let value = Value::from(arr);
        let bytes = compact::to_bytes(&value, SchemaId(2), &reg).unwrap();
        // The whole message is just the u32 count: 4 bytes.
        assert_eq!(bytes.len(), 4);

        let got = decode_both(&bytes, SchemaId(2), SchemaId(2), &reg);
        assert_eq!(got, value);
    }

    /// Regression: a fixed `array` of zero-sized elements has element count from
    /// the schema (here `[3]`) but zero wire bytes, so the in-decoder count bound
    /// (`count > remaining`) wrongly rejected it. The fixed-count check now uses
    /// the same zero-sized fallback as the wire-driven path.
    #[test]
    fn array_of_zero_sized_units_decodes() {
        // Array<unit, 3> — three units, zero payload bytes total.
        let arr_schema = schema(
            1,
            SchemaKind::Array {
                element: prim(Primitive::Unit),
                dimensions: vec![3],
            },
        );
        let reg = Registry::new([arr_schema]);
        let mut arr = VArray::new();
        for _ in 0..3 {
            arr.push(Value::NULL);
        }
        let value = Value::from(arr);
        let bytes = compact::to_bytes(&value, SchemaId(1), &reg).unwrap();
        assert_eq!(bytes.len(), 0);

        let got = decode_both(&bytes, SchemaId(1), SchemaId(1), &reg);
        assert_eq!(got, value);
    }
}
