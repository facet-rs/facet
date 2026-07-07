use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::hash::{DefaultHasher, Hash, Hasher};

use taxon::{
    Field as TaxonField, Kind, Primitive, Schema, SchemaId, SchemaRef, Variant as TaxonVariant,
    VariantPayload,
};
use weavy::mem::declared as declared_mem;
use weavy::mem::{Access, Descriptor, Layout};

use crate::VixParser;
use crate::ast::{self, EnumItem, Expr, Item, SourceFile, Span, StructItem};
use crate::binder::{self, ImportKind, ModuleBindings, SymbolKind};

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

pub(crate) type VixDescriptor = Descriptor<SchemaRef>;

#[derive(Clone, Default)]
pub(crate) struct DescriptorMap {
    by_id: HashMap<SchemaId, VixDescriptor>,
    legacy_names: HashMap<String, SchemaId>,
}

impl DescriptorMap {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    pub(crate) fn insert_with_key(
        &mut self,
        name: impl Into<String>,
        id: SchemaId,
        descriptor: VixDescriptor,
    ) -> Option<VixDescriptor> {
        self.legacy_names.insert(name.into(), id);
        self.by_id.insert(id, descriptor)
    }

    pub(crate) fn insert_named(
        &mut self,
        schemas: &SchemaTables,
        name: &str,
        descriptor: VixDescriptor,
    ) -> Option<VixDescriptor> {
        self.insert_with_key(
            name.to_string(),
            schemas.descriptor_key_for_name(name),
            descriptor,
        )
    }

    pub(crate) fn insert_named_if_absent(
        &mut self,
        schemas: &SchemaTables,
        name: &str,
        descriptor: impl FnOnce() -> VixDescriptor,
    ) {
        if !self.contains_key(name) {
            self.insert_named(schemas, name, descriptor());
        }
    }

    pub(crate) fn get(&self, name: &str) -> Option<&VixDescriptor> {
        self.legacy_names
            .get(name)
            .and_then(|id| self.by_id.get(id))
    }

    pub(crate) fn contains_key(&self, name: &str) -> bool {
        self.legacy_names
            .get(name)
            .is_some_and(|id| self.by_id.contains_key(id))
    }

    pub(crate) fn keys(&self) -> impl Iterator<Item = &String> {
        self.legacy_names.keys()
    }

    pub(crate) fn values(&self) -> impl Iterator<Item = &VixDescriptor> {
        self.by_id.values()
    }
}

pub(crate) struct ModuleTables {
    pub(crate) fns: HashMap<String, ast::FnItem>,
    pub(crate) fn_modules: HashMap<String, String>,
    pub(crate) fn_hashes: HashMap<String, u64>,
    pub(crate) enums: HashMap<String, EnumInfo>,
    pub(crate) structs: HashMap<String, StructInfo>,
    pub(crate) descriptors: DescriptorMap,
    pub(crate) schemas: SchemaTables,
    modules: BTreeMap<String, ModuleInfo>,
}

impl ModuleTables {
    pub(crate) fn has_schema(&self, name: &str) -> bool {
        let Some(SchemaRef::Concrete { id, .. }) = self.schemas.ref_for_name(name) else {
            return false;
        };
        self.schemas.schema(*id).is_some() && self.schemas.display_name(*id).is_some()
    }

    pub(crate) fn resolve_fn(&self, module: &str, name: &str) -> Option<&str> {
        let info = self.modules.get(module)?;
        if let Some(local) = info.fns.get(name) {
            return Some(local.as_str());
        }
        let imported = info.imports.get(name)?;
        (imported.kind == ImportKind::Fn).then_some(imported.name.as_str())
    }

    pub(crate) fn resolve_type_module(&self, module: &str, name: &str) -> Option<&str> {
        let info = self.modules.get(module)?;
        if let Some(local_module) = info.types.get(name) {
            return Some(local_module.as_str());
        }
        let imported = info.imports.get(name)?;
        (imported.kind == ImportKind::Type).then_some(imported.module.as_str())
    }
}

#[derive(Clone)]
pub(crate) struct SchemaTables {
    by_name: HashMap<String, SchemaRef>,
    by_id: HashMap<SchemaId, Schema>,
    display_names: HashMap<SchemaId, String>,
    frame_names: HashMap<SchemaId, String>,
}

impl SchemaTables {
    pub(crate) fn empty() -> Self {
        Self {
            by_name: HashMap::new(),
            by_id: HashMap::new(),
            display_names: HashMap::new(),
            frame_names: HashMap::new(),
        }
    }

    pub(crate) fn ref_for_name(&self, name: &str) -> Option<&SchemaRef> {
        self.by_name.get(name)
    }

    pub(crate) fn schema(&self, id: SchemaId) -> Option<&Schema> {
        self.by_id.get(&id)
    }

    pub(crate) fn display_name(&self, id: SchemaId) -> Option<&str> {
        self.display_names.get(&id).map(String::as_str)
    }

    pub(crate) fn frame_id_for_name(&self, name: &str) -> SchemaId {
        if self.by_name.contains_key(name) {
            return self.descriptor_key_for_name(name);
        }
        legacy_marker_schema_id(name)
    }

    pub(crate) fn frame_word_for_name(&self, name: &str) -> i64 {
        i64::from_ne_bytes(self.frame_id_for_name(name).as_u64().to_ne_bytes())
    }

    pub(crate) fn name_for_frame_word(&self, word: i64) -> Option<&str> {
        let id = SchemaId::from_raw(u64::from_ne_bytes(word.to_ne_bytes()));
        self.frame_names
            .get(&id)
            .or_else(|| self.display_names.get(&id))
            .map(String::as_str)
    }

    pub(crate) fn register_frame_names(&mut self, names: impl IntoIterator<Item = String>) {
        for name in names {
            self.frame_names
                .entry(self.frame_id_for_name(&name))
                .or_insert(name);
        }
        for (id, name) in &self.display_names {
            self.frame_names.entry(*id).or_insert_with(|| name.clone());
        }
    }

    pub(crate) fn legacy_ref(&self, name: &str) -> SchemaRef {
        if let Some(schema_ref) = self.by_name.get(name) {
            return schema_ref.clone();
        }
        if let Some((base, args)) = legacy_generic_schema(name)
            && matches!(base, "Array" | "List" | "Map" | "Option")
            && let Some(SchemaRef::Concrete { id, .. }) = self.by_name.get(base)
        {
            let args = args.iter().map(|arg| self.legacy_ref(arg)).collect();
            return SchemaRef::generic(*id, args);
        }
        SchemaRef::var(name)
    }

    pub(crate) fn descriptor_key_for_name(&self, name: &str) -> SchemaId {
        match self.legacy_ref(name) {
            SchemaRef::Concrete { id, .. } => id,
            SchemaRef::Var { .. } => legacy_marker_schema_id(name),
        }
    }

    pub(crate) fn display_ref(&self, schema_ref: &SchemaRef) -> String {
        match schema_ref {
            SchemaRef::Var { name } => name.clone(),
            SchemaRef::Concrete { id, args } => {
                let base = self
                    .display_name(*id)
                    .map(str::to_string)
                    .unwrap_or_else(|| id.to_string());
                if args.is_empty() {
                    base
                } else {
                    let args = args
                        .iter()
                        .map(|arg| self.display_ref(arg))
                        .collect::<Vec<_>>();
                    format!("{base}<{}>", args.join(","))
                }
            }
        }
    }

    pub(crate) fn kind_for_ref(&self, schema_ref: &SchemaRef) -> Option<&Kind> {
        let SchemaRef::Concrete { id, .. } = schema_ref else {
            return None;
        };
        self.schema(*id).map(|schema| &schema.kind)
    }

    pub(crate) fn kind_for_name(&self, name: &str) -> Option<&Kind> {
        self.kind_for_ref(&self.legacy_ref(name))
    }

    pub(crate) fn is_primitive(&self, name: &str, primitive: Primitive) -> bool {
        matches!(
            self.kind_for_name(name),
            Some(Kind::Primitive(candidate)) if *candidate == primitive
        )
    }

    pub(crate) fn is_list(&self, name: &str) -> bool {
        matches!(self.kind_for_name(name), Some(Kind::List { .. }))
    }

    pub(crate) fn is_map(&self, name: &str) -> bool {
        matches!(self.kind_for_name(name), Some(Kind::Map { .. }))
    }

    pub(crate) fn is_option(&self, name: &str) -> bool {
        matches!(self.kind_for_name(name), Some(Kind::Option { .. }))
    }

    pub(crate) fn is_struct_or_enum(&self, name: &str) -> bool {
        matches!(
            self.kind_for_name(name),
            Some(Kind::Struct { .. } | Kind::Enum { .. })
        )
    }

    pub(crate) fn is_external(&self, name: &str, external_kind: &str) -> bool {
        let namespaced = format!("vix.{external_kind}");
        matches!(
            self.kind_for_name(name),
            Some(Kind::External { kind, .. }) if kind == external_kind || kind == &namespaced
        )
    }

    pub(crate) fn is_named_schema(&self, name: &str, expected: &str) -> bool {
        match self.kind_for_name(name) {
            Some(Kind::Struct { name, .. } | Kind::Enum { name, .. }) => name == expected,
            Some(Kind::External { kind, .. }) => {
                let namespaced = format!("vix.{expected}");
                kind == expected || kind == &namespaced
            }
            _ => self
                .ref_for_name(expected)
                .is_some_and(|expected_ref| *expected_ref == self.legacy_ref(name)),
        }
    }

    pub(crate) fn generic_arg_names(&self, name: &str) -> Option<Vec<String>> {
        match self.legacy_ref(name) {
            SchemaRef::Concrete { args, .. } => Some(
                args.iter()
                    .map(|arg| self.display_ref(arg))
                    .collect::<Vec<_>>(),
            ),
            SchemaRef::Var { .. } => None,
        }
    }

    pub(crate) fn map_schema_names(&self, name: &str) -> Option<(String, String)> {
        if !self.is_map(name) {
            return None;
        }
        let args = self.generic_arg_names(name)?;
        let [key, value]: [String; 2] = args.try_into().ok()?;
        Some((key, value))
    }

    pub(crate) fn option_value_schema_name(&self, name: &str) -> Option<String> {
        if !self.is_option(name) {
            return None;
        }
        let args = self.generic_arg_names(name)?;
        let [value]: [String; 1] = args.try_into().ok()?;
        Some(value)
    }
}

struct PendingSchema {
    name: Option<String>,
    schema: Schema,
}

struct SchemaBuilder {
    next_key: u64,
    keys: HashMap<String, SchemaId>,
    defined: BTreeSet<String>,
    batch: Vec<PendingSchema>,
}

impl SchemaBuilder {
    fn new() -> Self {
        Self {
            next_key: 1,
            keys: HashMap::new(),
            defined: BTreeSet::new(),
            batch: Vec::new(),
        }
    }

    fn reserve_named(&mut self, name: &str) -> SchemaId {
        if let Some(id) = self.keys.get(name) {
            return *id;
        }
        let id = SchemaId::from_raw(self.next_key);
        self.next_key += 1;
        self.keys.insert(name.to_string(), id);
        id
    }

    fn named_ref(&self, name: &str) -> Result<SchemaRef, String> {
        self.keys
            .get(name)
            .copied()
            .map(SchemaRef::concrete)
            .ok_or_else(|| format!("unknown type `{name}`"))
    }

    fn generic_ref(&self, name: &str, args: Vec<SchemaRef>) -> Result<SchemaRef, String> {
        let id = self
            .keys
            .get(name)
            .copied()
            .ok_or_else(|| format!("unknown generic type `{name}`"))?;
        Ok(SchemaRef::generic(id, args))
    }

    fn add_named(
        &mut self,
        name: &str,
        type_params: Vec<String>,
        kind: Kind,
    ) -> Result<(), String> {
        let id = self.reserve_named(name);
        if !self.defined.insert(name.to_string()) {
            return Err(format!("duplicate schema model entry `{name}`"));
        }
        self.batch.push(PendingSchema {
            name: Some(name.to_string()),
            schema: Schema {
                id,
                type_params,
                kind,
            },
        });
        Ok(())
    }

    fn add_builtin_if_absent(
        &mut self,
        name: &str,
        type_params: Vec<String>,
        kind: impl FnOnce(&mut Self) -> Result<Kind, String>,
    ) -> Result<(), String> {
        if self.keys.contains_key(name) {
            return Ok(());
        }
        self.reserve_named(name);
        let kind = kind(self)?;
        self.add_named(name, type_params, kind)
    }

    fn add_tuple(&mut self, elements: Vec<SchemaRef>) -> SchemaRef {
        let id = SchemaId::from_raw(self.next_key);
        self.next_key += 1;
        self.batch.push(PendingSchema {
            name: None,
            schema: Schema {
                id,
                type_params: Vec::new(),
                kind: Kind::Tuple { elements },
            },
        });
        SchemaRef::concrete(id)
    }

    fn type_ref(
        &mut self,
        ty: &ast::Type,
        type_params: &BTreeSet<String>,
    ) -> Result<SchemaRef, String> {
        match ty {
            ast::Type::Path(path) => {
                let name = type_path_schema_name(path)?;
                if type_params.contains(&name) {
                    return Ok(SchemaRef::var(name));
                }
                self.named_ref(&name)
            }
            ast::Type::Generic(generic) => {
                let base = type_path_schema_name(&generic.base)?;
                if type_params.contains(&base) {
                    return Err(format!(
                        "generic type parameter `{base}` cannot take arguments"
                    ));
                }
                let args = generic
                    .args
                    .iter()
                    .map(|arg| self.type_ref(arg, type_params))
                    .collect::<Result<Vec<_>, _>>()?;
                match base.as_str() {
                    "Map" => {
                        if args.len() != 2 {
                            return Err("Map expects two type arguments".into());
                        }
                        self.generic_ref("Map", args)
                    }
                    "Option" => {
                        if args.len() != 1 {
                            return Err("Option expects one type argument".into());
                        }
                        self.generic_ref("Option", args)
                    }
                    "Array" | "List" => {
                        if args.len() != 1 {
                            return Err(format!("{base} expects one type argument"));
                        }
                        self.generic_ref("Array", args)
                    }
                    "Tuple" => Ok(self.add_tuple(args)),
                    "Pending" | "Realized" => {
                        let [inner]: [SchemaRef; 1] = args
                            .try_into()
                            .map_err(|_| format!("{base} expects one type argument"))?;
                        Ok(inner)
                    }
                    _ => self.generic_ref(&base, args),
                }
            }
            ast::Type::Array(array) => {
                let element = self.type_ref(&array.elem, type_params)?;
                self.generic_ref("Array", vec![element])
            }
            ast::Type::Tuple(tuple) => {
                let elements = tuple
                    .elems
                    .iter()
                    .map(|elem| self.type_ref(elem, type_params))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(self.add_tuple(elements))
            }
            ast::Type::Fn(_) => self.named_ref("Fn"),
        }
    }

    fn finish(self) -> SchemaTables {
        let pending = self.batch;
        let names = pending
            .iter()
            .map(|schema| schema.name.clone())
            .collect::<Vec<_>>();
        let resolved = taxon::resolve_ids(
            pending
                .into_iter()
                .map(|pending| pending.schema)
                .collect::<Vec<_>>(),
        );
        let mut by_name = HashMap::new();
        let mut by_id = HashMap::new();
        let mut display_names = HashMap::new();
        for (name, schema) in names.into_iter().zip(resolved) {
            if let Some(name) = name {
                by_name.insert(name.clone(), SchemaRef::concrete(schema.id));
                display_names.entry(schema.id).or_insert(name);
            }
            by_id.insert(schema.id, schema);
        }
        SchemaTables {
            by_name,
            by_id,
            display_names,
            frame_names: HashMap::new(),
        }
    }
}

#[derive(Clone)]
struct ModuleInfo {
    fns: BTreeMap<String, String>,
    types: BTreeMap<String, String>,
    imports: BTreeMap<String, ResolvedModuleItem>,
}

#[derive(Clone)]
struct ResolvedModuleItem {
    module: String,
    name: String,
    kind: ImportKind,
}

pub(crate) fn load_module_tables_from_modules(
    root: &str,
    modules: &BTreeMap<String, String>,
) -> Result<ModuleTables, String> {
    // Table construction is the expensive part (seconds in dev profile);
    // the parser itself is immutable after build — share one per process.
    static PARSER: std::sync::OnceLock<VixParser> = std::sync::OnceLock::new();
    let parser = PARSER.get_or_init(VixParser::new);
    let files: BTreeMap<String, SourceFile> = modules
        .iter()
        .map(|(path, source)| {
            parser
                .parse(source)
                .map(|file| (path.clone(), file))
                .map_err(|e| format!("parsing module `{path}`: {}", e.message))
        })
        .collect::<Result<_, _>>()?;
    let bindings = binder::bind_module_set(root, &files)?;

    let mut fns = HashMap::new();
    let mut fn_modules = HashMap::new();
    let mut bare_fn_hashes = BTreeMap::new();
    let mut enums = HashMap::new();
    let mut structs = HashMap::new();
    let mut type_hashes = BTreeMap::new();
    let mut fn_spans_by_module = BTreeMap::new();
    let mut type_spans_by_module = BTreeMap::new();
    let mut module_infos: BTreeMap<String, ModuleInfo> = files
        .keys()
        .map(|path| {
            (
                path.clone(),
                ModuleInfo {
                    fns: BTreeMap::new(),
                    types: BTreeMap::new(),
                    imports: BTreeMap::new(),
                },
            )
        })
        .collect();

    for (module, file) in &files {
        let mut fn_spans = BTreeMap::new();
        let mut type_spans = BTreeMap::new();
        for item in &file.items {
            match item {
                Item::Fn(f) => {
                    let canonical = canonical_fn_name(root, module, &f.name.value);
                    bare_fn_hashes.insert(canonical.clone(), canon_fn_hash(f));
                    fn_spans.insert(canonical.clone(), f.span);
                    module_infos
                        .get_mut(module)
                        .expect("module info exists")
                        .fns
                        .insert(f.name.value.clone(), canonical.clone());
                    fn_modules.insert(canonical.clone(), module.clone());
                    fns.insert(canonical, (**f).clone());
                }
                Item::Enum(e) => {
                    insert_unique_type_hash(&mut type_hashes, &e.name.value, canon_enum_hash(e))?;
                    insert_unique_span(&mut type_spans, &e.name.value, e.span)?;
                    module_infos
                        .get_mut(module)
                        .expect("module info exists")
                        .types
                        .insert(e.name.value.clone(), module.clone());
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
                    insert_unique(&mut enums, &e.name.value, EnumInfo { variants })?;
                }
                Item::Struct(s) => {
                    insert_unique_type_hash(&mut type_hashes, &s.name.value, canon_struct_hash(s))?;
                    insert_unique_span(&mut type_spans, &s.name.value, s.span)?;
                    module_infos
                        .get_mut(module)
                        .expect("module info exists")
                        .types
                        .insert(s.name.value.clone(), module.clone());
                    let fields = s
                        .fields
                        .iter()
                        .flat_map(|fl| &fl.fields)
                        .map(|f| (f.name.value.clone(), f.default.clone()))
                        .collect();
                    insert_unique(
                        &mut structs,
                        &s.name.value,
                        StructInfo {
                            fields,
                            is_unit: s.fields.is_none() && s.tuple.is_none(),
                        },
                    )?;
                }
                Item::Use(_) => {}
            }
        }
        fn_spans_by_module.insert(module.clone(), fn_spans);
        type_spans_by_module.insert(module.clone(), type_spans);
    }

    for ((module, imported_name), import) in bindings.imports() {
        let resolved = match import.kind {
            ImportKind::Fn => ResolvedModuleItem {
                module: import.module.clone(),
                name: canonical_fn_name(root, &import.module, &import.name),
                kind: import.kind,
            },
            ImportKind::Type => ResolvedModuleItem {
                module: import.module.clone(),
                name: import.name.clone(),
                kind: import.kind,
            },
        };
        module_infos
            .get_mut(module)
            .expect("module info exists")
            .imports
            .insert(imported_name.clone(), resolved);
    }

    let schemas = schema_tables(&files)?;
    let descriptors = declared_descriptors(&files, &schemas)?;
    let fn_hashes = closure_fn_hashes(
        &bindings,
        &bare_fn_hashes,
        &type_hashes,
        &fn_spans_by_module,
        &type_spans_by_module,
        &module_infos,
    )
    .into_iter()
    .collect();
    Ok(ModuleTables {
        fns,
        fn_modules,
        fn_hashes,
        enums,
        structs,
        descriptors,
        schemas,
        modules: module_infos,
    })
}

fn schema_tables(files: &BTreeMap<String, SourceFile>) -> Result<SchemaTables, String> {
    let mut builder = SchemaBuilder::new();
    for file in files.values() {
        for item in &file.items {
            match item {
                Item::Struct(s) => {
                    builder.reserve_named(&s.name.value);
                }
                Item::Enum(e) => {
                    builder.reserve_named(&e.name.value);
                }
                Item::Fn(_) | Item::Use(_) => {}
            }
        }
    }

    add_builtin_schemas(&mut builder)?;
    for file in files.values() {
        for item in &file.items {
            match item {
                Item::Struct(s) => add_struct_schema(&mut builder, s)?,
                Item::Enum(e) => add_enum_schema(&mut builder, e)?,
                Item::Fn(_) | Item::Use(_) => {}
            }
        }
    }
    Ok(builder.finish())
}

fn add_builtin_schemas(builder: &mut SchemaBuilder) -> Result<(), String> {
    builder.add_builtin_if_absent("Int", Vec::new(), |_| Ok(Kind::Primitive(Primitive::I64)))?;
    builder.add_builtin_if_absent("Float", Vec::new(), |_| Ok(Kind::Primitive(Primitive::F64)))?;
    builder.add_builtin_if_absent("Bool", Vec::new(), |_| Ok(Kind::Primitive(Primitive::Bool)))?;
    builder.add_builtin_if_absent("String", Vec::new(), |_| {
        Ok(Kind::Primitive(Primitive::String))
    })?;
    builder.add_builtin_if_absent("Blob", Vec::new(), |_| {
        Ok(Kind::Primitive(Primitive::Bytes))
    })?;
    for name in ["Path", "Flag", "Template", "Sealed", "Tree", "Fn"] {
        builder.add_builtin_if_absent(name, Vec::new(), |builder| {
            Ok(external_schema_kind(builder, name))
        })?;
    }

    builder.add_builtin_if_absent("Array", vec!["T".into()], |_| {
        Ok(Kind::List {
            element: SchemaRef::var("T"),
        })
    })?;
    builder.add_builtin_if_absent("Map", vec!["K".into(), "V".into()], |_| {
        Ok(Kind::Map {
            key: SchemaRef::var("K"),
            value: SchemaRef::var("V"),
        })
    })?;
    builder.add_builtin_if_absent("Option", vec!["T".into()], |_| {
        Ok(Kind::Option {
            element: SchemaRef::var("T"),
        })
    })?;

    builder.add_builtin_if_absent("Os", Vec::new(), |_| {
        Ok(Kind::Enum {
            name: "Os".into(),
            variants: unit_variants(["Linux", "Macos", "Windows"]),
        })
    })?;
    builder.add_builtin_if_absent("Arch", Vec::new(), |_| {
        Ok(Kind::Enum {
            name: "Arch".into(),
            variants: unit_variants(["X86_64", "Aarch64", "Arm", "Riscv64", "Wasm32", "Unknown"]),
        })
    })?;
    builder.add_builtin_if_absent("Target", Vec::new(), |builder| {
        Ok(Kind::Struct {
            name: "Target".into(),
            fields: vec![
                taxon_field("os", builder.named_ref("Os")?, true),
                taxon_field("arch", builder.named_ref("Arch")?, true),
            ],
        })
    })?;
    builder.add_builtin_if_absent("Run", Vec::new(), |builder| {
        Ok(Kind::Struct {
            name: "Run".into(),
            fields: vec![
                taxon_field("ok", builder.named_ref("Int")?, true),
                taxon_field("out", builder.named_ref("Tree")?, true),
            ],
        })
    })?;
    builder.add_builtin_if_absent("Doc", Vec::new(), |builder| {
        let doc = builder.named_ref("Doc")?;
        let string = builder.named_ref("String")?;
        let array_doc = builder.generic_ref("Array", vec![doc.clone()])?;
        let map_string_doc = builder.generic_ref("Map", vec![string.clone(), doc.clone()])?;
        Ok(Kind::Enum {
            name: "Doc".into(),
            variants: vec![
                taxon_variant("Null", 0, VariantPayload::Unit),
                taxon_variant(
                    "Bool",
                    1,
                    VariantPayload::Newtype(builder.named_ref("Bool")?),
                ),
                taxon_variant("Int", 2, VariantPayload::Newtype(builder.named_ref("Int")?)),
                taxon_variant(
                    "Float",
                    3,
                    VariantPayload::Newtype(builder.named_ref("Float")?),
                ),
                taxon_variant("String", 4, VariantPayload::Newtype(string.clone())),
                taxon_variant("Array", 5, VariantPayload::Newtype(array_doc)),
                taxon_variant("Map", 6, VariantPayload::Newtype(map_string_doc)),
                taxon_variant("Virtual", 7, VariantPayload::Newtype(string)),
                taxon_variant(
                    "Blob",
                    8,
                    VariantPayload::Newtype(builder.named_ref("Blob")?),
                ),
            ],
        })
    })?;
    for name in ["Cc", "Ar", "Rustc", "Version", "VersionSet", "Ordering"] {
        builder.add_builtin_if_absent(name, Vec::new(), |builder| {
            Ok(external_schema_kind(builder, name))
        })?;
    }
    Ok(())
}

fn add_struct_schema(builder: &mut SchemaBuilder, item: &StructItem) -> Result<(), String> {
    let type_params = generic_param_names(&item.generics);
    let type_param_scope = type_params.iter().cloned().collect::<BTreeSet<_>>();
    let fields = if let Some(fields) = &item.fields {
        fields
            .fields
            .iter()
            .map(|field| {
                Ok(taxon_field(
                    &field.name.value,
                    builder.type_ref(&field.ty, &type_param_scope)?,
                    field.default.is_none(),
                ))
            })
            .collect::<Result<Vec<_>, String>>()?
    } else if let Some(tuple) = &item.tuple {
        tuple
            .types
            .iter()
            .enumerate()
            .map(|(index, ty)| {
                Ok(taxon_field(
                    index.to_string(),
                    builder.type_ref(ty, &type_param_scope)?,
                    true,
                ))
            })
            .collect::<Result<Vec<_>, String>>()?
    } else {
        Vec::new()
    };
    builder.add_named(
        &item.name.value,
        type_params,
        Kind::Struct {
            name: item.name.value.clone(),
            fields,
        },
    )
}

fn add_enum_schema(builder: &mut SchemaBuilder, item: &EnumItem) -> Result<(), String> {
    let type_params = generic_param_names(&item.generics);
    let type_param_scope = type_params.iter().cloned().collect::<BTreeSet<_>>();
    let variants = item
        .variants
        .iter()
        .enumerate()
        .map(|(index, variant)| {
            let payload = if let Some(tuple) = &variant.tuple {
                VariantPayload::Tuple(
                    tuple
                        .types
                        .iter()
                        .map(|ty| builder.type_ref(ty, &type_param_scope))
                        .collect::<Result<Vec<_>, _>>()?,
                )
            } else if let Some(fields) = &variant.fields {
                VariantPayload::Struct(
                    fields
                        .fields
                        .iter()
                        .map(|field| {
                            Ok(taxon_field(
                                &field.name.value,
                                builder.type_ref(&field.ty, &type_param_scope)?,
                                field.default.is_none(),
                            ))
                        })
                        .collect::<Result<Vec<_>, String>>()?,
                )
            } else {
                VariantPayload::Unit
            };
            Ok(taxon_variant(
                &variant.name.value,
                u32::try_from(index).expect("variant count fits u32"),
                payload,
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    builder.add_named(
        &item.name.value,
        type_params,
        Kind::Enum {
            name: item.name.value.clone(),
            variants,
        },
    )
}

fn external_schema_kind(_builder: &SchemaBuilder, name: &str) -> Kind {
    Kind::External {
        kind: format!("vix.{name}"),
        metadata: None,
    }
}

fn taxon_field(name: impl Into<String>, schema: SchemaRef, required: bool) -> TaxonField {
    TaxonField {
        name: name.into(),
        schema,
        required,
    }
}

fn taxon_variant(name: impl Into<String>, index: u32, payload: VariantPayload) -> TaxonVariant {
    TaxonVariant {
        name: name.into(),
        index,
        payload,
    }
}

fn unit_variants<const N: usize>(names: [&str; N]) -> Vec<TaxonVariant> {
    names
        .into_iter()
        .enumerate()
        .map(|(index, name)| {
            taxon_variant(
                name,
                u32::try_from(index).expect("variant count fits u32"),
                VariantPayload::Unit,
            )
        })
        .collect()
}

fn generic_param_names(generics: &Option<ast::GenericParams>) -> Vec<String> {
    generics
        .as_ref()
        .map(|generics| {
            generics
                .params
                .iter()
                .map(|param| param.value.clone())
                .collect()
        })
        .unwrap_or_default()
}

fn legacy_generic_schema(schema: &str) -> Option<(&str, Vec<String>)> {
    let (base, rest) = schema.split_once('<')?;
    let args = rest.strip_suffix('>')?;
    let mut parts = Vec::new();
    let mut start = 0;
    let mut depth = 0usize;
    for (index, byte) in args.bytes().enumerate() {
        match byte {
            b'<' => depth += 1,
            b'>' => depth = depth.saturating_sub(1),
            b',' if depth == 0 => {
                parts.push(args[start..index].to_string());
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(args[start..].to_string());
    Some((base, parts))
}

fn legacy_marker_schema_id(name: &str) -> SchemaId {
    let mut hasher = DefaultHasher::new();
    "vix-legacy-schema-marker".hash(&mut hasher);
    name.hash(&mut hasher);
    SchemaId::from_raw(hasher.finish())
}

pub(crate) fn type_schema_name(ty: &ast::Type) -> Result<String, String> {
    match ty {
        ast::Type::Path(path) => type_path_schema_name(path),
        ast::Type::Generic(generic) => {
            let base = type_path_schema_name(&generic.base)?;
            let args = generic
                .args
                .iter()
                .map(type_schema_name)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(format!("{base}<{}>", args.join(",")))
        }
        ast::Type::Array(_) => Ok("Array".into()),
        ast::Type::Tuple(tuple) => {
            let elems = tuple
                .elems
                .iter()
                .map(type_schema_name)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(format!("Tuple<{}>", elems.join(",")))
        }
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

fn canonical_fn_name(root: &str, module: &str, name: &str) -> String {
    if module == root {
        name.to_string()
    } else {
        format!("{module}::{name}")
    }
}

fn insert_unique<T>(map: &mut HashMap<String, T>, name: &str, value: T) -> Result<(), String> {
    if map.insert(name.to_string(), value).is_some() {
        return Err(format!(
            "duplicate type name `{name}` across module set is outside vix v1"
        ));
    }
    Ok(())
}

fn insert_unique_type_hash(
    map: &mut BTreeMap<String, u64>,
    name: &str,
    value: u64,
) -> Result<(), String> {
    if map.insert(name.to_string(), value).is_some() {
        return Err(format!(
            "duplicate type name `{name}` across module set is outside vix v1"
        ));
    }
    Ok(())
}

fn insert_unique_span(
    map: &mut BTreeMap<String, Span>,
    name: &str,
    value: Span,
) -> Result<(), String> {
    if map.insert(name.to_string(), value).is_some() {
        return Err(format!(
            "duplicate type name `{name}` across module set is outside vix v1"
        ));
    }
    Ok(())
}

fn declared_descriptors(
    files: &BTreeMap<String, SourceFile>,
    schemas: &SchemaTables,
) -> Result<DescriptorMap, String> {
    let mut descriptors = DescriptorMap::new();
    insert_descriptor(
        schemas,
        &mut descriptors,
        "Int",
        declared_mem::i64_(schemas.legacy_ref("Int")),
    );
    insert_descriptor(
        schemas,
        &mut descriptors,
        "Float",
        declared_mem::f64_(schemas.legacy_ref("Float")),
    );
    insert_descriptor(
        schemas,
        &mut descriptors,
        "Bool",
        declared_mem::i64_(schemas.legacy_ref("Bool")),
    );

    for file in files.values() {
        for item in &file.items {
            match item {
                Item::Struct(s) => {
                    let fields = if let Some(fields) = &s.fields {
                        fields
                            .fields
                            .iter()
                            .map(|field| descriptor_for_type(schemas, &field.ty))
                            .collect::<Result<Vec<_>, _>>()?
                    } else if let Some(tuple) = &s.tuple {
                        tuple
                            .types
                            .iter()
                            .map(|ty| descriptor_for_type(schemas, ty))
                            .collect::<Result<Vec<_>, _>>()?
                    } else {
                        Vec::new()
                    };
                    insert_descriptor(
                        schemas,
                        &mut descriptors,
                        &s.name.value,
                        declared_mem::declared_struct(schemas.legacy_ref(&s.name.value), fields),
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
                                    .map(|ty| descriptor_for_type(schemas, ty))
                                    .collect::<Result<Vec<_>, _>>()
                            } else if let Some(fields) = &variant.fields {
                                fields
                                    .fields
                                    .iter()
                                    .map(|field| descriptor_for_type(schemas, &field.ty))
                                    .collect::<Result<Vec<_>, _>>()
                            } else {
                                Ok(Vec::new())
                            }
                        })
                        .collect::<Result<Vec<_>, _>>()?;
                    insert_descriptor(
                        schemas,
                        &mut descriptors,
                        &e.name.value,
                        declared_mem::declared_enum(schemas.legacy_ref(&e.name.value), variants),
                    );
                }
                Item::Fn(_) | Item::Use(_) => {}
            }
        }
    }
    Ok(descriptors)
}

fn insert_descriptor(
    schemas: &SchemaTables,
    descriptors: &mut DescriptorMap,
    name: &str,
    descriptor: VixDescriptor,
) {
    let key = schemas.descriptor_key_for_name(name);
    descriptors.insert_with_key(name.to_string(), key, descriptor);
}

fn descriptor_for_type(schemas: &SchemaTables, ty: &ast::Type) -> Result<VixDescriptor, String> {
    let schema = type_schema_name(ty)?;
    Ok(match schema.as_str() {
        "Int" => declared_mem::i64_(schemas.legacy_ref("Int")),
        "Float" => declared_mem::f64_(schemas.legacy_ref("Float")),
        "Bool" => declared_mem::i64_(schemas.legacy_ref("Bool")),
        "String" => handle_i64(schemas, "StringRef", "String"),
        other => handle_i64(schemas, format!("{other}Ref"), other),
    })
}

fn handle_i64(
    schemas: &SchemaTables,
    schema: impl AsRef<str>,
    target: impl AsRef<str>,
) -> VixDescriptor {
    Descriptor {
        schema: schemas.legacy_ref(schema.as_ref()),
        layout: Layout { size: 8, align: 8 },
        access: Access::Handle {
            target: schemas.legacy_ref(target.as_ref()),
        },
    }
}

fn closure_fn_hashes(
    bindings: &ModuleBindings,
    bare_fn_hashes: &BTreeMap<String, u64>,
    type_hashes: &BTreeMap<String, u64>,
    fn_spans_by_module: &BTreeMap<String, BTreeMap<String, Span>>,
    type_spans_by_module: &BTreeMap<String, BTreeMap<String, Span>>,
    modules: &BTreeMap<String, ModuleInfo>,
) -> BTreeMap<String, u64> {
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

    for (module, module_bindings) in bindings.modules() {
        let fn_spans = &fn_spans_by_module[module];
        let type_spans = &type_spans_by_module[module];
        for (span, id) in module_bindings.refs() {
            let symbol = module_bindings.symbol(id);
            let resolved = resolve_symbol_ref(modules, module, symbol.kind, &symbol.name);
            match resolved {
                Some(ResolvedSymbolRef::Fn(target)) => {
                    if let Some(owner) = owner_for(span, fn_spans) {
                        fn_edges
                            .entry(owner.to_string())
                            .or_default()
                            .insert(target);
                    }
                }
                Some(ResolvedSymbolRef::Type(target)) => {
                    if let Some(owner) = owner_for(span, fn_spans) {
                        fn_type_refs
                            .entry(owner.to_string())
                            .or_default()
                            .insert(target);
                    } else if type_hashes.contains_key(&target)
                        && let Some(owner) = owner_for(span, type_spans)
                    {
                        type_edges
                            .entry(owner.to_string())
                            .or_default()
                            .insert(target);
                    }
                }
                None => {}
            }
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

enum ResolvedSymbolRef {
    Fn(String),
    Type(String),
}

fn resolve_symbol_ref(
    modules: &BTreeMap<String, ModuleInfo>,
    module: &str,
    kind: SymbolKind,
    name: &str,
) -> Option<ResolvedSymbolRef> {
    match kind {
        SymbolKind::Fn => modules[module]
            .fns
            .get(name)
            .cloned()
            .map(ResolvedSymbolRef::Fn),
        SymbolKind::Type => Some(ResolvedSymbolRef::Type(name.to_string())),
        SymbolKind::Import => {
            let item = modules[module].imports.get(name)?;
            match item.kind {
                ImportKind::Fn => Some(ResolvedSymbolRef::Fn(item.name.clone())),
                ImportKind::Type => Some(ResolvedSymbolRef::Type(item.name.clone())),
            }
        }
        SymbolKind::Param
        | SymbolKind::Let
        | SymbolKind::ClosureParam
        | SymbolKind::TypeParam
        | SymbolKind::Binding => None,
    }
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
