//! Content-derived schema identity.
//!
//! A [`SchemaId`] is the first 8 bytes (little-endian `u64`) of the BLAKE3 hash
//! of a schema's *canonical structural encoding*. The encoding is byte-exact and
//! reproducible across implementations, so the same logical schema yields the
//! same id everywhere with no coordination.
//!
//! Recursive schemas are handled by partitioning the reference graph into
//! strongly-connected components, processing them dependencies-first, and — for
//! a cyclic component — hashing each member via a structural unfolding with
//! depth-indexed back-references that terminates the walk.
//!
//! Spec: "Schema identity" — `r[schema-identity.canonical-encoding]`,
//! `r[schema-identity.computation]`.

use std::collections::{HashMap, HashSet};

use crate::bytes::{Sink, write_bool, write_str, write_u32, write_u64, write_u8};
use crate::schema::{
    ChannelDirection, Field, Primitive, Schema, SchemaId, SchemaKind, SchemaRef, VariantPayload,
};

// ============================================================================
// Canonical encoding building blocks
// ============================================================================
//
// The byte primitives live in `crate::bytes`; every tag and marker token is fed
// to the sink as a *string*. (Building blocks: little-endian ints; a string is a
// u32 LE length then UTF-8; a bool is one byte.)

fn write_type_params<S: Sink>(out: &mut S, params: &[String]) {
    write_u32(out, params.len() as u32);
    for p in params {
        write_str(out, p);
    }
}

fn direction_tag(d: ChannelDirection) -> &'static str {
    match d {
        ChannelDirection::Tx => "tx",
        ChannelDirection::Rx => "rx",
    }
}

// r[impl schema-identity.content-hash]
fn finalize(hasher: &blake3::Hasher) -> SchemaId {
    let hash = hasher.finalize();
    let bytes = hash.as_bytes();
    let mut first8 = [0u8; 8];
    first8.copy_from_slice(&bytes[..8]);
    SchemaId(u64::from_le_bytes(first8))
}

/// The canonical id of a primitive schema (`Schema { kind: Primitive(p), .. }`).
/// Constant and universal — useful for referencing primitives as already-resolved
/// targets when building a batch.
///
/// Spec: `r[schema-identity.canonical-encoding]` (primitive tags).
#[must_use]
pub fn primitive_id(p: Primitive) -> SchemaId {
    let mut hasher = blake3::Hasher::new();
    write_str(&mut hasher, p.tag());
    finalize(&hasher)
}

// ============================================================================
// Graph node index
// ============================================================================

/// An index into the batch being resolved. A newtype so a batch position can't
/// be confused with any other integer.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
struct NodeIx(u32);

impl NodeIx {
    fn of(i: usize) -> Self {
        NodeIx(i as u32)
    }

    fn ix(self) -> usize {
        self.0 as usize
    }
}

// ============================================================================
// Reference graph
// ============================================================================

/// Visit every `SchemaRef::Concrete` target reachable in a kind (including those
/// nested inside type arguments), calling `f` with each referenced id.
fn visit_kind_targets(kind: &SchemaKind, f: &mut impl FnMut(SchemaId)) {
    let on_ref = |r: &SchemaRef, f: &mut dyn FnMut(SchemaId)| visit_ref_targets(r, f);
    match kind {
        SchemaKind::Primitive(_) | SchemaKind::Dynamic => {}
        SchemaKind::Struct { fields, .. } => {
            for field in fields {
                on_ref(&field.schema, f);
            }
        }
        SchemaKind::Enum { variants, .. } => {
            for v in variants {
                match &v.payload {
                    VariantPayload::Unit => {}
                    VariantPayload::Newtype(r) => on_ref(r, f),
                    VariantPayload::Tuple(refs) => {
                        for r in refs {
                            on_ref(r, f);
                        }
                    }
                    VariantPayload::Struct(fields) => {
                        for field in fields {
                            on_ref(&field.schema, f);
                        }
                    }
                }
            }
        }
        SchemaKind::Tuple { elements } => {
            for r in elements {
                on_ref(r, f);
            }
        }
        SchemaKind::List { element }
        | SchemaKind::Set { element }
        | SchemaKind::Array { element, .. }
        | SchemaKind::Tensor { element, .. }
        | SchemaKind::Option { element }
        | SchemaKind::Channel { element, .. } => on_ref(element, f),
        SchemaKind::Map { key, value } => {
            on_ref(key, f);
            on_ref(value, f);
        }
        SchemaKind::External { metadata, .. } => {
            if let Some(r) = metadata {
                on_ref(r, f);
            }
        }
    }
}

fn visit_ref_targets(r: &SchemaRef, f: &mut dyn FnMut(SchemaId)) {
    if let SchemaRef::Concrete { id, args } = r {
        f(*id);
        for a in args {
            visit_ref_targets(a, f);
        }
    }
}

// ============================================================================
// Tarjan SCC (yields components dependencies-first)
// ============================================================================

struct Tarjan<'a> {
    adj: &'a [Vec<NodeIx>],
    next_order: usize,
    order: Vec<Option<usize>>,
    lowlink: Vec<usize>,
    on_stack: Vec<bool>,
    stack: Vec<NodeIx>,
    sccs: Vec<Vec<NodeIx>>,
}

impl<'a> Tarjan<'a> {
    fn run(adj: &'a [Vec<NodeIx>]) -> Vec<Vec<NodeIx>> {
        let n = adj.len();
        let mut t = Tarjan {
            adj,
            next_order: 0,
            order: vec![None; n],
            lowlink: vec![0; n],
            on_stack: vec![false; n],
            stack: Vec::new(),
            sccs: Vec::new(),
        };
        for v in 0..n {
            if t.order[v].is_none() {
                t.strongconnect(NodeIx::of(v));
            }
        }
        // Components are popped when their root finishes, so dependencies (which
        // finish first) appear before dependents: dependencies-first order.
        t.sccs
    }

    fn strongconnect(&mut self, v: NodeIx) {
        self.order[v.ix()] = Some(self.next_order);
        self.lowlink[v.ix()] = self.next_order;
        self.next_order += 1;
        self.stack.push(v);
        self.on_stack[v.ix()] = true;

        for &w in &self.adj[v.ix()] {
            if self.order[w.ix()].is_none() {
                self.strongconnect(w);
                self.lowlink[v.ix()] = self.lowlink[v.ix()].min(self.lowlink[w.ix()]);
            } else if self.on_stack[w.ix()] {
                self.lowlink[v.ix()] = self.lowlink[v.ix()].min(self.order[w.ix()].unwrap());
            }
        }

        if self.lowlink[v.ix()] == self.order[v.ix()].unwrap() {
            let mut scc = Vec::new();
            loop {
                let w = self.stack.pop().unwrap();
                self.on_stack[w.ix()] = false;
                scc.push(w);
                if w == v {
                    break;
                }
            }
            self.sccs.push(scc);
        }
    }
}

// ============================================================================
// The walk
// ============================================================================

/// Context shared across a member's structural walk.
struct Walk<'a> {
    batch: &'a [Schema],
    key_to_index: &'a HashMap<u64, NodeIx>,
    component: &'a HashSet<NodeIx>,
    assigned: &'a HashMap<NodeIx, SchemaId>,
}

impl Walk<'_> {
    /// Walk schema `idx`'s kind, with `path` holding the component members from
    /// the root of this walk down to (and including) `idx`.
    // r[impl schema-identity.canonical-encoding]
    fn schema<S: Sink>(&self, idx: NodeIx, path: &[NodeIx], out: &mut S) {
        let schema = &self.batch[idx.ix()];
        match &schema.kind {
            SchemaKind::Primitive(p) => write_str(out, p.tag()),
            SchemaKind::Struct { name, fields } => {
                write_str(out, "struct");
                write_str(out, name);
                write_type_params(out, &schema.type_params);
                write_u32(out, fields.len() as u32);
                for field in fields {
                    self.field(field, path, out);
                }
            }
            SchemaKind::Enum { name, variants } => {
                write_str(out, "enum");
                write_str(out, name);
                write_type_params(out, &schema.type_params);
                write_u32(out, variants.len() as u32);
                for v in variants {
                    write_str(out, &v.name);
                    write_u32(out, v.index);
                    match &v.payload {
                        VariantPayload::Unit => write_str(out, "unit"),
                        VariantPayload::Newtype(r) => {
                            write_str(out, "newtype");
                            self.reference(r, path, out);
                        }
                        VariantPayload::Tuple(refs) => {
                            write_str(out, "tuple");
                            write_u32(out, refs.len() as u32);
                            for r in refs {
                                self.reference(r, path, out);
                            }
                        }
                        VariantPayload::Struct(fields) => {
                            write_str(out, "struct");
                            write_u32(out, fields.len() as u32);
                            for field in fields {
                                self.field(field, path, out);
                            }
                        }
                    }
                }
            }
            SchemaKind::Tuple { elements } => {
                write_str(out, "tuple");
                write_u32(out, elements.len() as u32);
                for r in elements {
                    self.reference(r, path, out);
                }
            }
            SchemaKind::List { element } => {
                write_str(out, "list");
                self.reference(element, path, out);
            }
            SchemaKind::Set { element } => {
                write_str(out, "set");
                self.reference(element, path, out);
            }
            SchemaKind::Option { element } => {
                write_str(out, "option");
                self.reference(element, path, out);
            }
            SchemaKind::Map { key, value } => {
                write_str(out, "map");
                self.reference(key, path, out);
                self.reference(value, path, out);
            }
            SchemaKind::Array {
                element,
                dimensions,
            } => {
                write_str(out, "array");
                self.reference(element, path, out);
                write_u32(out, dimensions.len() as u32);
                for d in dimensions {
                    write_u64(out, *d);
                }
            }
            SchemaKind::Tensor { element, rank } => {
                write_str(out, "tensor");
                self.reference(element, path, out);
                match rank {
                    None => write_u8(out, 0),
                    Some(r) => {
                        write_u8(out, 1);
                        write_u32(out, *r);
                    }
                }
            }
            SchemaKind::Channel { direction, element } => {
                write_str(out, "channel");
                write_str(out, direction_tag(*direction));
                self.reference(element, path, out);
            }
            SchemaKind::Dynamic => write_str(out, "dynamic"),
            SchemaKind::External { kind, metadata } => {
                write_str(out, "external");
                write_str(out, kind);
                match metadata {
                    None => write_u8(out, 0),
                    Some(r) => {
                        write_u8(out, 1);
                        self.reference(r, path, out);
                    }
                }
            }
        }
    }

    fn field<S: Sink>(&self, field: &Field, path: &[NodeIx], out: &mut S) {
        write_str(out, &field.name);
        write_bool(out, field.required);
        self.reference(&field.schema, path, out);
    }

    fn reference<S: Sink>(&self, r: &SchemaRef, path: &[NodeIx], out: &mut S) {
        match r {
            SchemaRef::Var { name } => {
                write_str(out, "var");
                write_str(out, name);
            }
            SchemaRef::Concrete { id, args } => {
                match self.key_to_index.get(&id.0) {
                    Some(&target) if self.component.contains(&target) => {
                        if let Some(depth) = path.iter().position(|&p| p == target) {
                            // Target is an ancestor on the current walk path: the
                            // back-reference that terminates the walk.
                            write_str(out, "backref");
                            write_u32(out, depth as u32);
                        } else {
                            // Target is another component member, off-path: inline
                            // its structure with the path extended by it.
                            write_str(out, "inline");
                            let mut next = path.to_vec();
                            next.push(target);
                            self.schema(target, &next, out);
                        }
                    }
                    Some(&target) => {
                        // A different, already-processed component: feed its id.
                        let rid = self.assigned.get(&target).copied().expect(
                            "dependency component must be assigned before its dependents",
                        );
                        write_str(out, "concrete");
                        write_u64(out, rid.0);
                    }
                    None => {
                        // External: the reference already carries a real id.
                        write_str(out, "concrete");
                        write_u64(out, id.0);
                    }
                }
                write_u32(out, args.len() as u32);
                for a in args {
                    self.reference(a, path, out);
                }
            }
        }
    }
}

// ============================================================================
// Substitution of provisional keys with computed ids in the output
// ============================================================================

fn remap_ref(r: &SchemaRef, map: &HashMap<u64, SchemaId>) -> SchemaRef {
    match r {
        SchemaRef::Var { name } => SchemaRef::Var { name: name.clone() },
        SchemaRef::Concrete { id, args } => SchemaRef::Concrete {
            id: map.get(&id.0).copied().unwrap_or(*id),
            args: args.iter().map(|a| remap_ref(a, map)).collect(),
        },
    }
}

fn remap_field(field: &Field, map: &HashMap<u64, SchemaId>) -> Field {
    Field {
        name: field.name.clone(),
        schema: remap_ref(&field.schema, map),
        required: field.required,
    }
}

fn remap_kind(kind: &SchemaKind, map: &HashMap<u64, SchemaId>) -> SchemaKind {
    match kind {
        SchemaKind::Primitive(p) => SchemaKind::Primitive(*p),
        SchemaKind::Dynamic => SchemaKind::Dynamic,
        SchemaKind::Struct { name, fields } => SchemaKind::Struct {
            name: name.clone(),
            fields: fields.iter().map(|f| remap_field(f, map)).collect(),
        },
        SchemaKind::Enum { name, variants } => SchemaKind::Enum {
            name: name.clone(),
            variants: variants
                .iter()
                .map(|v| crate::schema::Variant {
                    name: v.name.clone(),
                    index: v.index,
                    payload: match &v.payload {
                        VariantPayload::Unit => VariantPayload::Unit,
                        VariantPayload::Newtype(r) => VariantPayload::Newtype(remap_ref(r, map)),
                        VariantPayload::Tuple(refs) => {
                            VariantPayload::Tuple(refs.iter().map(|r| remap_ref(r, map)).collect())
                        }
                        VariantPayload::Struct(fields) => VariantPayload::Struct(
                            fields.iter().map(|f| remap_field(f, map)).collect(),
                        ),
                    },
                })
                .collect(),
        },
        SchemaKind::Tuple { elements } => SchemaKind::Tuple {
            elements: elements.iter().map(|r| remap_ref(r, map)).collect(),
        },
        SchemaKind::List { element } => SchemaKind::List {
            element: remap_ref(element, map),
        },
        SchemaKind::Set { element } => SchemaKind::Set {
            element: remap_ref(element, map),
        },
        SchemaKind::Option { element } => SchemaKind::Option {
            element: remap_ref(element, map),
        },
        SchemaKind::Map { key, value } => SchemaKind::Map {
            key: remap_ref(key, map),
            value: remap_ref(value, map),
        },
        SchemaKind::Array {
            element,
            dimensions,
        } => SchemaKind::Array {
            element: remap_ref(element, map),
            dimensions: dimensions.clone(),
        },
        SchemaKind::Tensor { element, rank } => SchemaKind::Tensor {
            element: remap_ref(element, map),
            rank: *rank,
        },
        SchemaKind::Channel { direction, element } => SchemaKind::Channel {
            direction: *direction,
            element: remap_ref(element, map),
        },
        SchemaKind::External { kind, metadata } => SchemaKind::External {
            kind: kind.clone(),
            metadata: metadata.as_ref().map(|r| remap_ref(r, map)),
        },
    }
}

// ============================================================================
// Entry point
// ============================================================================

/// Compute content-derived [`SchemaId`]s for a batch of mutually-referential
/// schemas.
///
/// On input, each schema's `id` and every in-batch `SchemaRef::Concrete.id` is a
/// caller-assigned *provisional key* (any unique `u64` — dense indices work
/// well). A reference whose id is not a provisional key in the batch is treated
/// as already resolved (its id is a real, external [`SchemaId`]). The returned
/// schemas have real ids substituted everywhere — on each schema and on every
/// in-batch reference.
///
/// Caller contract: provisional keys must be unique within the batch and must
/// not collide with the real id of any external schema the batch references.
///
/// Spec: `r[schema-identity.computation]`.
// r[impl schema-identity.computation]
#[must_use]
pub fn resolve_ids(batch: Vec<Schema>) -> Vec<Schema> {
    let n = batch.len();

    // Provisional key -> node index.
    let mut key_to_index: HashMap<u64, NodeIx> = HashMap::with_capacity(n);
    for (i, s) in batch.iter().enumerate() {
        key_to_index.insert(s.id.0, NodeIx::of(i));
    }

    // Reference graph: edge i -> j when schema i references in-batch schema j.
    let mut adj: Vec<Vec<NodeIx>> = vec![Vec::new(); n];
    for (i, s) in batch.iter().enumerate() {
        let mut seen = HashSet::new();
        visit_kind_targets(&s.kind, &mut |id| {
            if let Some(&j) = key_to_index.get(&id.0)
                && seen.insert(j)
            {
                adj[i].push(j);
            }
        });
    }

    let sccs = Tarjan::run(&adj);

    // Assign ids component-by-component, dependencies first.
    let mut assigned: HashMap<NodeIx, SchemaId> = HashMap::with_capacity(n);
    for scc in &sccs {
        let component: HashSet<NodeIx> = scc.iter().copied().collect();
        let walk = Walk {
            batch: &batch,
            key_to_index: &key_to_index,
            component: &component,
            assigned: &assigned,
        };
        // Within a component every member's id is independent (same-component
        // references use inline/backref, never an assigned id), so order here
        // does not matter.
        let mut local = Vec::with_capacity(scc.len());
        for &i in scc {
            let mut hasher = blake3::Hasher::new();
            walk.schema(i, &[i], &mut hasher);
            local.push((i, finalize(&hasher)));
        }
        for (i, id) in local {
            assigned.insert(i, id);
        }
    }

    // Provisional key -> real id, for rewriting references.
    let mut key_to_real: HashMap<u64, SchemaId> = HashMap::with_capacity(n);
    for (i, s) in batch.iter().enumerate() {
        key_to_real.insert(s.id.0, assigned[&NodeIx::of(i)]);
    }

    batch
        .iter()
        .enumerate()
        .map(|(i, s)| Schema {
            id: assigned[&NodeIx::of(i)],
            type_params: s.type_params.clone(),
            kind: remap_kind(&s.kind, &key_to_real),
        })
        .collect()
}

/// The ids of schemas that are part of a reference cycle — a self-reference or a
/// mutual-recursion group. These are the schemas an engine must lower to a callable
/// block (rather than inline) so a recursive type's descriptor/program stays finite;
/// every non-cyclic schema can still be inlined. Computed by the same SCC pass that
/// resolves cyclic ids: any SCC of size > 1 is recursive, and a singleton SCC is
/// recursive iff the node references itself.
#[must_use]
pub fn recursive_schema_ids(schemas: &[Schema]) -> std::collections::BTreeSet<SchemaId> {
    let n = schemas.len();
    let mut id_to_index: HashMap<u64, NodeIx> = HashMap::with_capacity(n);
    for (i, s) in schemas.iter().enumerate() {
        id_to_index.insert(s.id.0, NodeIx::of(i));
    }
    let mut adj: Vec<Vec<NodeIx>> = vec![Vec::new(); n];
    let mut self_edge = vec![false; n];
    for (i, s) in schemas.iter().enumerate() {
        let mut seen = HashSet::new();
        visit_kind_targets(&s.kind, &mut |id| {
            if let Some(&j) = id_to_index.get(&id.0) {
                if j.ix() == i {
                    self_edge[i] = true;
                }
                if seen.insert(j) {
                    adj[i].push(j);
                }
            }
        });
    }

    let mut out = std::collections::BTreeSet::new();
    for scc in Tarjan::run(&adj) {
        let recursive = scc.len() > 1 || (scc.len() == 1 && self_edge[scc[0].ix()]);
        if recursive {
            for ix in scc {
                out.insert(schemas[ix.ix()].id);
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema::{Field, Schema, SchemaKind, SchemaRef, Variant};

    /// A schema with a provisional key and the given kind, no type params.
    fn proto(key: u64, kind: SchemaKind) -> Schema {
        Schema {
            id: SchemaId(key),
            type_params: Vec::new(),
            kind,
        }
    }

    fn field(name: &str, r: SchemaRef) -> Field {
        Field {
            name: name.to_string(),
            schema: r,
            required: true,
        }
    }

    fn point(name: &str, x: &str, y: &str) -> Schema {
        proto(
            1,
            SchemaKind::Struct {
                name: name.to_string(),
                fields: vec![
                    field(x, SchemaRef::concrete(primitive_id(Primitive::U32))),
                    field(y, SchemaRef::concrete(primitive_id(Primitive::F64))),
                ],
            },
        )
    }

    // --- Sink ---------------------------------------------------------------

    #[test]
    fn vec_sink_captures_canonical_bytes() {
        let mut buf: Vec<u8> = Vec::new();
        write_str(&mut buf, Primitive::U32.tag());
        // u32 LE length (3) then the UTF-8 bytes of "u32".
        assert_eq!(buf, vec![3, 0, 0, 0, b'u', b'3', b'2']);
    }

    // --- SCC ----------------------------------------------------------------

    /// Run Tarjan on a hand-built edge list, returning components as sorted
    /// `usize` vectors (preserving Tarjan's component order).
    fn run_scc(n: usize, edges: &[(usize, usize)]) -> Vec<Vec<usize>> {
        let mut adj: Vec<Vec<NodeIx>> = vec![Vec::new(); n];
        for &(a, b) in edges {
            adj[a].push(NodeIx::of(b));
        }
        Tarjan::run(&adj)
            .into_iter()
            .map(|c| {
                let mut v: Vec<usize> = c.into_iter().map(NodeIx::ix).collect();
                v.sort_unstable();
                v
            })
            .collect()
    }

    fn as_set(components: &[Vec<usize>]) -> HashSet<Vec<usize>> {
        components.iter().cloned().collect()
    }

    /// Position of the component containing `node` within the result order.
    fn order_of(components: &[Vec<usize>], node: usize) -> usize {
        components
            .iter()
            .position(|c| c.contains(&node))
            .expect("node must be in some component")
    }

    #[test]
    fn scc_self_loop_is_its_own_component() {
        // 0 -> 0
        let comps = run_scc(1, &[(0, 0)]);
        assert_eq!(as_set(&comps), as_set(&[vec![0]]));
    }

    #[test]
    fn scc_partitions_independent_cycles_chains_and_isolates() {
        // {0,1} cycle; 2 -> 0 (depends on the cycle); {3} self-loop;
        // {4,5} independent cycle; {6} isolated.
        let comps = run_scc(
            7,
            &[(0, 1), (1, 0), (2, 0), (3, 3), (4, 5), (5, 4)],
        );
        assert_eq!(
            as_set(&comps),
            as_set(&[vec![0, 1], vec![2], vec![3], vec![4, 5], vec![6]])
        );
    }

    #[test]
    fn scc_yields_dependencies_first() {
        // 0 -> 1 -> 2 -> 1: component {1,2} is a dependency of {0}.
        let comps = run_scc(3, &[(0, 1), (1, 2), (2, 1)]);
        assert_eq!(as_set(&comps), as_set(&[vec![1, 2], vec![0]]));
        // The cycle {1,2} must be assigned before its dependent {0}.
        assert!(order_of(&comps, 1) < order_of(&comps, 0));

        // Chain feeding the earlier independent-cycles graph: the cycle {0,1}
        // is emitted before its dependent singleton {2}.
        let comps = run_scc(3, &[(0, 1), (1, 0), (2, 0)]);
        assert!(order_of(&comps, 0) < order_of(&comps, 2));
    }

    // --- identity hashing ---------------------------------------------------

    #[test]
    fn primitive_ids_are_distinct_and_stable() {
        assert_eq!(primitive_id(Primitive::U32), primitive_id(Primitive::U32));
        assert_ne!(primitive_id(Primitive::U32), primitive_id(Primitive::U64));
        assert_ne!(primitive_id(Primitive::I32), primitive_id(Primitive::U32));
        assert_ne!(
            primitive_id(Primitive::String),
            primitive_id(Primitive::Bytes)
        );
    }

    #[test]
    fn struct_id_is_deterministic() {
        let a = resolve_ids(vec![point("Point", "x", "y")])[0].id;
        let b = resolve_ids(vec![point("Point", "x", "y")])[0].id;
        assert_eq!(a, b);
    }

    #[test]
    fn name_and_field_order_matter() {
        let base = resolve_ids(vec![point("Point", "x", "y")])[0].id;
        let renamed = resolve_ids(vec![point("Vec2", "x", "y")])[0].id;
        let reordered = resolve_ids(vec![point("Point", "y", "x")])[0].id;
        let renamed_field = resolve_ids(vec![point("Point", "a", "y")])[0].id;
        assert_ne!(base, renamed);
        assert_ne!(base, reordered);
        assert_ne!(base, renamed_field);
    }

    #[test]
    fn required_flag_is_part_of_identity() {
        let required = point("Point", "x", "y");
        let mut optional = point("Point", "x", "y");
        if let SchemaKind::Struct { fields, .. } = &mut optional.kind {
            fields[0].required = false;
        }
        assert_ne!(
            resolve_ids(vec![required])[0].id,
            resolve_ids(vec![optional])[0].id
        );
    }

    /// Build the linked-list cycle `Node { value: u32, next: Option<Node> }`,
    /// modelled as two schemas: `Node` (key 10) and `Option<Node>` (key 20),
    /// referencing each other. Returns them in the given order.
    fn linked_list(node_first: bool) -> Vec<Schema> {
        let node = proto(
            10,
            SchemaKind::Struct {
                name: "Node".to_string(),
                fields: vec![
                    field("value", SchemaRef::concrete(primitive_id(Primitive::U32))),
                    field("next", SchemaRef::concrete(SchemaId(20))),
                ],
            },
        );
        let opt = proto(
            20,
            SchemaKind::Option {
                element: SchemaRef::concrete(SchemaId(10)),
            },
        );
        if node_first {
            vec![node, opt]
        } else {
            vec![opt, node]
        }
    }

    #[test]
    fn recursive_schema_ids_flags_the_cycle() {
        // Every schema in the `Node`/`Option<Node>` cycle is recursive.
        let resolved = resolve_ids(linked_list(true));
        let rec = recursive_schema_ids(&resolved);
        assert_eq!(rec.len(), 2);
        for s in &resolved {
            assert!(rec.contains(&s.id), "{:?} should be flagged recursive", s.id);
        }

        // A flat, non-recursive struct flags nothing.
        let flat = resolve_ids(vec![proto(
            10,
            SchemaKind::Struct {
                name: "Flat".to_string(),
                fields: vec![field("a", SchemaRef::concrete(primitive_id(Primitive::U32)))],
            },
        )]);
        assert!(recursive_schema_ids(&flat).is_empty());
    }

    #[test]
    fn recursive_schema_terminates_and_is_order_independent() {
        let forward = resolve_ids(linked_list(true));
        let reversed = resolve_ids(linked_list(false));

        let id_of = |schemas: &[Schema], want_struct: bool| -> SchemaId {
            schemas
                .iter()
                .find(|s| matches!(&s.kind, SchemaKind::Struct { .. }) == want_struct)
                .unwrap()
                .id
        };

        // Same logical schema gets the same id regardless of input order.
        assert_eq!(id_of(&forward, true), id_of(&reversed, true));
        assert_eq!(id_of(&forward, false), id_of(&reversed, false));

        // References were rewritten to real ids (no provisional keys left).
        let node = forward
            .iter()
            .find(|s| matches!(&s.kind, SchemaKind::Struct { .. }))
            .unwrap();
        if let SchemaKind::Struct { fields, .. } = &node.kind {
            if let SchemaRef::Concrete { id, .. } = &fields[1].schema {
                assert_eq!(*id, id_of(&forward, false));
            } else {
                panic!("next field should be concrete");
            }
        }
    }

    #[test]
    fn distinct_recursive_types_differ() {
        let list = resolve_ids(linked_list(true));
        let node_id = list
            .iter()
            .find(|s| matches!(&s.kind, SchemaKind::Struct { .. }))
            .unwrap()
            .id;

        let node2 = proto(
            10,
            SchemaKind::Struct {
                name: "Cell".to_string(),
                fields: vec![
                    field("value", SchemaRef::concrete(primitive_id(Primitive::U32))),
                    field("next", SchemaRef::concrete(SchemaId(20))),
                ],
            },
        );
        let opt2 = proto(
            20,
            SchemaKind::Option {
                element: SchemaRef::concrete(SchemaId(10)),
            },
        );
        let cell_id = resolve_ids(vec![node2, opt2])
            .iter()
            .find(|s| matches!(&s.kind, SchemaKind::Struct { .. }))
            .unwrap()
            .id;

        assert_ne!(node_id, cell_id);
    }

    #[test]
    fn enum_variants_contribute_to_identity() {
        let make = |variant_name: &str| {
            proto(
                1,
                SchemaKind::Enum {
                    name: "E".to_string(),
                    variants: vec![
                        Variant {
                            name: variant_name.to_string(),
                            index: 0,
                            payload: VariantPayload::Unit,
                        },
                        Variant {
                            name: "B".to_string(),
                            index: 1,
                            payload: VariantPayload::Newtype(SchemaRef::concrete(primitive_id(
                                Primitive::U32,
                            ))),
                        },
                    ],
                },
            )
        };
        assert_ne!(
            resolve_ids(vec![make("A")])[0].id,
            resolve_ids(vec![make("Z")])[0].id
        );
    }
}
