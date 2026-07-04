use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{DefaultHasher, Hash, Hasher};

use weavy::mem::declared as declared_mem;
use weavy::mem::{Access, Descriptor, Layout};

use crate::ast::{self, EnumItem, Expr, Item, SourceFile, Span, StructItem};
use crate::binder::{self, SymbolKind};
use crate::VixParser;

#[derive(Clone)]
pub(crate) struct EnumInfo {
    pub(crate) variants: Vec<(String, VariantShape)>,
}

#[derive(Clone, Debug)]
pub(crate) enum VariantShape {
    Unit,
    Tuple(usize),
    Record(Vec<String>),
}

#[derive(Clone)]
pub(crate) struct StructInfo {
    /// Field names in declaration order, with optional default exprs.
    pub(crate) fields: Vec<(String, Option<Expr>)>,
    pub(crate) is_unit: bool,
}

pub(crate) struct ModuleTables {
    pub(crate) fns: HashMap<String, ast::FnItem>,
    pub(crate) fn_hashes: HashMap<String, u64>,
    pub(crate) enums: HashMap<String, EnumInfo>,
    pub(crate) structs: HashMap<String, StructInfo>,
    pub(crate) descriptors: HashMap<String, Descriptor<String>>,
}

pub(crate) fn load_module_tables(source: &str) -> Result<ModuleTables, String> {
    // Table construction is the expensive part (seconds in dev profile);
    // the parser itself is immutable after build — share one per process.
    static PARSER: std::sync::OnceLock<VixParser> = std::sync::OnceLock::new();
    let parser = PARSER.get_or_init(VixParser::new);
    let file: SourceFile = parser.parse(source).map_err(|e| e.message)?;
    let mut fns = HashMap::new();
    let mut bare_fn_hashes = BTreeMap::new();
    let mut enums = HashMap::new();
    let mut structs = HashMap::new();
    let mut type_hashes = BTreeMap::new();
    let mut fn_spans = BTreeMap::new();
    let mut type_spans = BTreeMap::new();
    for item in &file.items {
        match item {
            Item::Fn(f) => {
                bare_fn_hashes.insert(f.name.value.clone(), canon_fn_hash(f));
                fn_spans.insert(f.name.value.clone(), f.span);
                fns.insert(f.name.value.clone(), (**f).clone());
            }
            Item::Enum(e) => {
                type_hashes.insert(e.name.value.clone(), canon_enum_hash(e));
                type_spans.insert(e.name.value.clone(), e.span);
                let variants = e
                    .variants
                    .iter()
                    .map(|v| {
                        let shape = if let Some(t) = &v.tuple {
                            VariantShape::Tuple(t.types.len())
                        } else if let Some(fl) = &v.fields {
                            VariantShape::Record(
                                fl.fields.iter().map(|f| f.name.value.clone()).collect(),
                            )
                        } else {
                            VariantShape::Unit
                        };
                        (v.name.value.clone(), shape)
                    })
                    .collect();
                enums.insert(e.name.value.clone(), EnumInfo { variants });
            }
            Item::Struct(s) => {
                type_hashes.insert(s.name.value.clone(), canon_struct_hash(s));
                type_spans.insert(s.name.value.clone(), s.span);
                let fields = s
                    .fields
                    .iter()
                    .flat_map(|fl| &fl.fields)
                    .map(|f| (f.name.value.clone(), f.default.clone()))
                    .collect();
                structs.insert(
                    s.name.value.clone(),
                    StructInfo {
                        fields,
                        is_unit: s.fields.is_none() && s.tuple.is_none(),
                    },
                );
            }
            Item::Use(_) => {}
        }
    }
    let descriptors = declared_descriptors(&file)?;
    let fn_hashes = closure_fn_hashes(&file, &bare_fn_hashes, &type_hashes, &fn_spans, &type_spans)
        .into_iter()
        .collect();
    Ok(ModuleTables {
        fns,
        fn_hashes,
        enums,
        structs,
        descriptors,
    })
}

pub(crate) fn type_schema_name(ty: &ast::Type) -> Result<String, String> {
    match ty {
        ast::Type::Path(path) => type_path_schema_name(path),
        ast::Type::Generic(generic) => type_path_schema_name(&generic.base),
        ast::Type::Array(_) => Ok("Array".into()),
        ast::Type::Tuple(_) => Ok("Tuple".into()),
        ast::Type::Fn(_) => Ok("Fn".into()),
    }
}

pub(crate) fn type_path_schema_name(path: &ast::TypePath) -> Result<String, String> {
    if path.segments.len() == 1 {
        Ok(path.segments[0].value.clone())
    } else {
        Err(format!(
            "qualified type path {path:?} is outside the machine slice-2 subset"
        ))
    }
}

fn declared_descriptors(file: &SourceFile) -> Result<HashMap<String, Descriptor<String>>, String> {
    let mut descriptors = HashMap::new();
    descriptors.insert("Int".into(), declared_mem::i64_("Int".into()));

    for item in &file.items {
        match item {
            Item::Struct(s) => {
                let fields = if let Some(fields) = &s.fields {
                    fields
                        .fields
                        .iter()
                        .map(|field| descriptor_for_type(&field.ty))
                        .collect::<Result<Vec<_>, _>>()?
                } else if let Some(tuple) = &s.tuple {
                    tuple
                        .types
                        .iter()
                        .map(descriptor_for_type)
                        .collect::<Result<Vec<_>, _>>()?
                } else {
                    Vec::new()
                };
                descriptors.insert(
                    s.name.value.clone(),
                    declared_mem::declared_struct(s.name.value.clone(), fields),
                );
            }
            Item::Enum(e) => {
                let variants = e
                    .variants
                    .iter()
                    .map(|variant| {
                        if let Some(tuple) = &variant.tuple {
                            tuple
                                .types
                                .iter()
                                .map(descriptor_for_type)
                                .collect::<Result<Vec<_>, _>>()
                        } else if let Some(fields) = &variant.fields {
                            fields
                                .fields
                                .iter()
                                .map(|field| descriptor_for_type(&field.ty))
                                .collect::<Result<Vec<_>, _>>()
                        } else {
                            Ok(Vec::new())
                        }
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                descriptors.insert(
                    e.name.value.clone(),
                    declared_mem::declared_enum(e.name.value.clone(), variants),
                );
            }
            Item::Fn(_) | Item::Use(_) => {}
        }
    }
    Ok(descriptors)
}

fn descriptor_for_type(ty: &ast::Type) -> Result<Descriptor<String>, String> {
    let schema = type_schema_name(ty)?;
    Ok(match schema.as_str() {
        "Int" => declared_mem::i64_("Int".into()),
        "String" => handle_i64("StringRef", "String"),
        other => handle_i64(format!("{other}Ref"), other.to_string()),
    })
}

fn handle_i64(schema: impl Into<String>, target: impl Into<String>) -> Descriptor<String> {
    Descriptor {
        schema: schema.into(),
        layout: Layout { size: 8, align: 8 },
        access: Access::Handle {
            target: target.into(),
        },
    }
}

fn closure_fn_hashes(
    file: &SourceFile,
    bare_fn_hashes: &BTreeMap<String, u64>,
    type_hashes: &BTreeMap<String, u64>,
    fn_spans: &BTreeMap<String, Span>,
    type_spans: &BTreeMap<String, Span>,
) -> BTreeMap<String, u64> {
    let bindings = binder::bind(file);
    let mut fn_edges: BTreeMap<String, BTreeSet<String>> = bare_fn_hashes
        .keys()
        .map(|name| (name.clone(), BTreeSet::new()))
        .collect();
    let mut fn_type_refs: BTreeMap<String, BTreeSet<String>> = bare_fn_hashes
        .keys()
        .map(|name| (name.clone(), BTreeSet::new()))
        .collect();
    let mut type_edges: BTreeMap<String, BTreeSet<String>> = type_hashes
        .keys()
        .map(|name| (name.clone(), BTreeSet::new()))
        .collect();

    for (span, id) in bindings.refs() {
        let symbol = bindings.symbol(id);
        match symbol.kind {
            SymbolKind::Fn => {
                if let Some(owner) = owner_for(span, fn_spans) {
                    fn_edges
                        .entry(owner.to_string())
                        .or_default()
                        .insert(symbol.name.clone());
                }
            }
            SymbolKind::Type => {
                if let Some(owner) = owner_for(span, fn_spans) {
                    fn_type_refs
                        .entry(owner.to_string())
                        .or_default()
                        .insert(symbol.name.clone());
                } else if let Some(owner) = owner_for(span, type_spans) {
                    type_edges
                        .entry(owner.to_string())
                        .or_default()
                        .insert(symbol.name.clone());
                }
            }
            SymbolKind::Param
            | SymbolKind::Let
            | SymbolKind::ClosureParam
            | SymbolKind::Import
            | SymbolKind::TypeParam
            | SymbolKind::Binding => {}
        }
    }

    let type_closure_hashes = graph_closure_hashes(type_hashes, &type_edges, &BTreeMap::new());
    let fn_type_hashes = fn_type_refs
        .into_iter()
        .map(|(func, refs)| {
            let hashes = refs
                .into_iter()
                .filter_map(|name| type_closure_hashes.get(&name).copied())
                .collect();
            (func, hashes)
        })
        .collect();
    graph_closure_hashes(bare_fn_hashes, &fn_edges, &fn_type_hashes)
}

fn owner_for(span: Span, owners: &BTreeMap<String, Span>) -> Option<&str> {
    owners
        .iter()
        .find(|(_, owner_span)| owner_span.contains(span.start))
        .map(|(name, _)| name.as_str())
}

fn graph_closure_hashes(
    own_hashes: &BTreeMap<String, u64>,
    edges: &BTreeMap<String, BTreeSet<String>>,
    extras: &BTreeMap<String, BTreeSet<u64>>,
) -> BTreeMap<String, u64> {
    let components = strongly_connected_components(own_hashes.keys().cloned().collect(), edges);
    let mut component_of = BTreeMap::new();
    for (index, component) in components.iter().enumerate() {
        for name in component {
            component_of.insert(name.clone(), index);
        }
    }

    let mut memo = BTreeMap::new();
    for name in own_hashes.keys() {
        let hash = graph_node_closure_hash(
            name,
            own_hashes,
            edges,
            extras,
            &components,
            &component_of,
            &mut memo,
        );
        memo.insert(name.clone(), hash);
    }
    memo
}

fn graph_node_closure_hash(
    name: &str,
    own_hashes: &BTreeMap<String, u64>,
    edges: &BTreeMap<String, BTreeSet<String>>,
    extras: &BTreeMap<String, BTreeSet<u64>>,
    components: &[Vec<String>],
    component_of: &BTreeMap<String, usize>,
    memo: &mut BTreeMap<String, u64>,
) -> u64 {
    if let Some(hash) = memo.get(name) {
        return *hash;
    }

    let component_index = component_of[name];
    let component = &components[component_index];
    let mut component_hashes: Vec<u64> =
        component.iter().map(|member| own_hashes[member]).collect();
    component_hashes.sort_unstable();
    let scc_hash = hash_list("scc", &component_hashes);

    let mut out_hashes = BTreeSet::new();
    let mut extra_hashes = BTreeSet::new();
    for member in component {
        if let Some(member_extras) = extras.get(member) {
            extra_hashes.extend(member_extras.iter().copied());
        }
        for target in edges.get(member).into_iter().flatten() {
            if component_of.get(target).copied() == Some(component_index) {
                continue;
            }
            let target_hash = graph_node_closure_hash(
                target,
                own_hashes,
                edges,
                extras,
                components,
                component_of,
                memo,
            );
            out_hashes.insert(target_hash);
        }
    }

    let mut h = DefaultHasher::new();
    "closure".hash(&mut h);
    own_hashes[name].hash(&mut h);
    scc_hash.hash(&mut h);
    out_hashes.len().hash(&mut h);
    for hash in out_hashes {
        hash.hash(&mut h);
    }
    extra_hashes.len().hash(&mut h);
    for hash in extra_hashes {
        hash.hash(&mut h);
    }
    h.finish()
}

fn strongly_connected_components(
    names: Vec<String>,
    edges: &BTreeMap<String, BTreeSet<String>>,
) -> Vec<Vec<String>> {
    struct Tarjan<'a> {
        edges: &'a BTreeMap<String, BTreeSet<String>>,
        index: usize,
        stack: Vec<String>,
        on_stack: BTreeSet<String>,
        indices: BTreeMap<String, usize>,
        lowlinks: BTreeMap<String, usize>,
        components: Vec<Vec<String>>,
    }

    impl Tarjan<'_> {
        fn connect(&mut self, name: String) {
            self.indices.insert(name.clone(), self.index);
            self.lowlinks.insert(name.clone(), self.index);
            self.index += 1;
            self.stack.push(name.clone());
            self.on_stack.insert(name.clone());

            for target in self.edges.get(&name).into_iter().flatten() {
                if !self.indices.contains_key(target) {
                    self.connect(target.clone());
                    let low = self.lowlinks[&name].min(self.lowlinks[target]);
                    self.lowlinks.insert(name.clone(), low);
                } else if self.on_stack.contains(target) {
                    let low = self.lowlinks[&name].min(self.indices[target]);
                    self.lowlinks.insert(name.clone(), low);
                }
            }

            if self.lowlinks[&name] == self.indices[&name] {
                let mut component = Vec::new();
                loop {
                    let member = self.stack.pop().expect("component member");
                    self.on_stack.remove(&member);
                    component.push(member.clone());
                    if member == name {
                        break;
                    }
                }
                component.sort();
                self.components.push(component);
            }
        }
    }

    let mut tarjan = Tarjan {
        edges,
        index: 0,
        stack: Vec::new(),
        on_stack: BTreeSet::new(),
        indices: BTreeMap::new(),
        lowlinks: BTreeMap::new(),
        components: Vec::new(),
    };
    for name in names {
        if !tarjan.indices.contains_key(&name) {
            tarjan.connect(name);
        }
    }
    tarjan.components
}

fn hash_list(domain: &str, hashes: &[u64]) -> u64 {
    let mut h = DefaultHasher::new();
    domain.hash(&mut h);
    hashes.len().hash(&mut h);
    for hash in hashes {
        hash.hash(&mut h);
    }
    h.finish()
}

fn canon_fn_hash(item: &ast::FnItem) -> u64 {
    let mut canonical = item.clone();
    canonical.strip_spans();
    let bytes = phon::api::encode(&canonical).expect("AST serializes");
    let mut h = DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

fn canon_enum_hash(item: &EnumItem) -> u64 {
    let mut canonical = item.clone();
    canonical.strip_spans();
    let bytes = phon::api::encode(&canonical).expect("AST serializes");
    let mut h = DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}

fn canon_struct_hash(item: &StructItem) -> u64 {
    let mut canonical = item.clone();
    canonical.strip_spans();
    let bytes = phon::api::encode(&canonical).expect("AST serializes");
    let mut h = DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}
