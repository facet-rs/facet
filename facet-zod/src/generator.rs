//! Top-level driver that registers root types and emits a Zod schema file.

use std::collections::{HashMap, HashSet};

use facet_core::*;
use facet_core::{Facet, Shape};

use crate::config::Config;
use crate::emit::emit_schema;
use crate::mapping::{Ctx, NamedSchema, schema_name, shape_to_zod_root};

/// Accumulates root types to emit and renders them to a single Zod source string.
pub struct ZodGenerator {
    roots: Vec<&'static Shape>,
    config: Config,
}

impl ZodGenerator {
    /// Create a new generator with [`Config::default()`].
    pub fn new() -> Self {
        Self {
            roots: Vec::new(),
            config: Config::default(),
        }
    }

    /// Create a new generator with an explicit [`Config`].
    pub fn with_config(config: Config) -> Self {
        Self {
            roots: Vec::new(),
            config,
        }
    }

    /// Register a root type `T`. Nested types reachable from `T` are emitted automatically.
    pub fn add<'facet, T: Facet<'facet>>(&mut self) -> &mut Self {
        self.roots.push(T::SHAPE);
        self
    }

    /// Emit the final Zod source text for all registered roots.
    pub fn emit(&self) -> String {
        let registry = self.discover_all();
        let sorted = toposort(&registry);
        let map = &registry.map;

        let mut out = String::new();

        if let Some(header) = &self.config.header {
            out.push_str(header);
            if !header.ends_with('\n') {
                out.push('\n');
            }
            out.push('\n');
        }

        let mut emitted: HashSet<ConstTypeId> = HashSet::new();
        for shape in &sorted {
            let name = schema_name(shape);
            let ctx = Ctx {
                config: &self.config,
                registry: map,
                emitted: &emitted,
                root: shape.id,
            };
            let ty = shape_to_zod_root(shape, &ctx);

            let doc = if shape.doc.is_empty() {
                None
            } else {
                Some(shape.doc.join("\n"))
            };

            let schema = NamedSchema { name, ty, doc };
            out.push_str(&emit_schema(&schema, &self.config));
            out.push('\n');
            emitted.insert(shape.id);
        }

        out
    }

    fn discover_all(&self) -> Registry {
        let mut registry = Registry::default();
        let mut seen = HashSet::new();
        for shape in &self.roots {
            discover(shape, &mut registry, &mut seen);
        }
        registry
    }
}

/// Named schemas to emit. `map` answers "does this type get its own
/// declaration?"; `order` is deterministic (first-seen during a DFS from the
/// roots in field order) so generated output is reproducible regardless of
/// `HashMap` seeding.
#[derive(Default)]
struct Registry {
    map: HashMap<ConstTypeId, &'static Shape>,
    order: Vec<&'static Shape>,
}

impl Default for ZodGenerator {
    fn default() -> Self {
        Self::new()
    }
}

fn discover(shape: &'static Shape, registry: &mut Registry, seen: &mut HashSet<ConstTypeId>) {
    if is_primitive(shape) {
        return;
    }

    if !seen.insert(shape.id) {
        return;
    }

    if should_emit_named(shape) {
        registry.map.insert(shape.id, shape);
        registry.order.push(shape);
    }

    for_each_child_shape(shape, |child| {
        discover(child, registry, seen);
    });
}

fn for_each_child_shape(shape: &'static Shape, mut visit: impl FnMut(&'static Shape)) {
    match &shape.def {
        Def::Option(opt) => visit(opt.t),
        Def::List(list) => visit(list.t),
        Def::Set(set) => visit(set.t),
        Def::Map(map) => {
            visit(map.k);
            visit(map.v);
        }
        Def::Array(arr) => visit(arr.t),
        Def::Slice(slice) => visit(slice.t),
        Def::Result(res) => {
            visit(res.t);
            visit(res.e);
        }
        Def::Pointer(ptr) => {
            if let Some(pointee) = ptr.pointee {
                visit(pointee);
            }
        }
        _ => match &shape.ty {
            Type::User(UserType::Struct(st)) => {
                for field in st.fields {
                    if !field.should_skip_serializing_unconditional() {
                        visit(field.shape.get());
                    }
                }
            }
            Type::User(UserType::Enum(et)) => {
                for variant in et.variants {
                    for field in variant.data.fields {
                        visit(field.shape.get());
                    }
                }
            }
            Type::Sequence(SequenceType::Array(arr)) => visit(arr.t),
            Type::Sequence(SequenceType::Slice(slice)) => visit(slice.t),
            Type::Pointer(PointerType::Reference(vp) | PointerType::Raw(vp)) => visit(vp.target),
            _ => {}
        },
    }
}

fn toposort(registry: &Registry) -> Vec<&'static Shape> {
    let mut visited = HashSet::new();
    let mut result = Vec::new();

    for shape in &registry.order {
        toposort_visit(shape, &registry.map, &mut visited, &mut result);
    }

    result
}

fn toposort_visit(
    shape: &'static Shape,
    registry: &HashMap<ConstTypeId, &'static Shape>,
    visited: &mut HashSet<ConstTypeId>,
    result: &mut Vec<&'static Shape>,
) {
    if !visited.insert(shape.id) {
        return;
    }

    for_each_child_shape(shape, |child| {
        if registry.contains_key(&child.id) {
            toposort_visit(child, registry, visited, result);
        }
    });

    result.push(shape);
}

fn is_primitive(shape: &'static Shape) -> bool {
    matches!(shape.def, Def::Scalar)
        || matches!(
            shape.ty,
            Type::Primitive(_) | Type::Sequence(_) | Type::Pointer(_)
        )
}

fn should_emit_named(shape: &'static Shape) -> bool {
    let is_user_type = matches!(
        shape.ty,
        Type::User(UserType::Struct(_) | UserType::Enum(_))
    );
    let has_container_def = matches!(
        shape.def,
        Def::Option(_)
            | Def::List(_)
            | Def::Set(_)
            | Def::Map(_)
            | Def::Array(_)
            | Def::Slice(_)
            | Def::Result(_)
            | Def::Pointer(_)
    );
    is_user_type && !has_container_def
}
