//! The AST lowering — vix functions become task programs (lowering
//! constitution: vixen repo, docs/design/machine-lowering.md).
//!
//! Slice-1 subset, honestly bounded: scalar Int functions, parameters,
//! lets, `+ - *`, parens, and CALLS — every user-function call is a
//! MEMO BOUNDARY lowered to the INVOKE protocol (write [slot, fn,
//! argc, args...] into the frame's invoke region, HostCall(INVOKE),
//! Await(slot)). Anything outside the subset is a loud error, never a
//! silent approximation. Floats, conds, composites arrive with their
//! slices.
//!
//! Memo identity is right from day one: the memo key's function
//! component is the CLOSURE HASH from the module tables (canonical
//! AST of the fn plus everything it references, code and types,
//! transitively) — trivia edits preserve it, semantic edits change
//! exactly the affected closures.

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::rc::Rc;
use std::sync::Arc;

use taxon::Primitive;
use weavy::mem::Layout;
use weavy::mem::declared as declared_mem;
use weavy::task::{Fn as TaskFn, FnId, Op, Program};

use super::TotalF64;
use super::driver::{
    ACQUIRE_HOST, ARRAY_ALLOC_HOST, ARRAY_COLLECT_HOST, ARRAY_FILTER_EXCLUDE_HOST, ARRAY_JOIN_HOST,
    ARRAY_LEN_HOST, ARRAY_MAP_PENDING_HOST, ARRAY_POP_HOST, ARRAY_PUSH_HOST, ARRAY_SET_HOST,
    AST_DOC_HOST, AST_FN_HOST, CRATE_ARCHIVE_HOST, CodeBundle, DOC_COERCE_HOST, DOC_GET_HOST,
    DOC_IS_MAP_HOST, DOC_KEYS_HOST, DOC_PACKAGE_HOST, DOC_PARSE_HOST, DriveEvent, DriveEventSink,
    Driver, ELF_DOC_HOST, EXEC_HOST, FETCH_HOST, FnRef, GLOB_HOST, INVOKE_HOST, Lane, LoweredFn,
    MAP_EMPTY_HOST, MAP_GET_HOST, MAP_INSERT_HOST, MOLTEN_DUP_HOST, MachineExecBackend,
    MoltenStats, OCI_DOC_HOST, OPTION_CONSTRUCT_HOST, OPTION_DESTRUCT_HOST, OPTION_UNWRAP_HOST,
    PATH_JOIN_HOST, PATH_TO_STRING_HOST, PATH_WITH_EXT_HOST, PENDING_ALLOC_HOST,
    PENDING_COERCE_HOST, PENDING_INVOKE_HOST, RECORD_UPDATE_HOST, RenderNames, RenderVariant,
    RenderedValue, SEALED_DECLASSIFY_HOST, SEALED_SEAL_HOST, SEALED_TO_STRING_HOST,
    STORE_ALLOC_HOST, STORE_READ_HOST, STORE_TAG_HOST, STRING_CONCAT_HOST, STRING_CONTAINS_HOST,
    STRING_DEFAULT_HOST, STRING_IS_NUMERIC_HOST, STRING_LOWER_HOST, STRING_PARSE_INT_HOST,
    STRING_SPLIT_HOST, STRING_UPPER_HOST, SemanticComparator, StepMode, StoreHandle, TARGET_HOST,
    TREE_PROJECT_HOST, TREE_TEXT_HOST, VALUE_COMPARE_HOST, VERSION_PARSE_HOST, VERSION_SET_OP_HOST,
    VERSION_SET_PARSE_HOST, ValueBundle,
};
use crate::ast;
use crate::fetch::FetchBackend;
use crate::module::{
    DescriptorMap, ModuleTables, SchemaTables, VariantShape, VixDescriptor,
    load_module_tables_from_modules, type_schema_name,
};

/// The machine facade for this slice: load source, demand a function's
/// value at the edge.
pub struct Machine {
    driver: Driver,
    fn_refs: HashMap<String, usize>,
    fn_params: BTreeMap<String, Vec<String>>,
    fn_param_names: BTreeMap<String, Vec<String>>,
    fn_returns: BTreeMap<String, String>,
    render_names: RenderNames,
    root: String,
    modules: BTreeMap<String, String>,
    source: String,
    module_hash: Vec<u8>,
    lower_options: LowerOptions,
    diagnostics: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReloadDiff {
    pub changed: BTreeSet<String>,
}

#[derive(Clone, Debug)]
pub enum MachineArg {
    Word(i64),
    Handle(StoreHandle),
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Path(String),
    Flag(String),
    Tree(crate::exec::Tree),
    LinuxTarget,
}

#[derive(Clone, Debug)]
pub struct NamedArg {
    pub name: String,
    pub value: MachineArg,
}

impl Machine {
    pub fn load(source: &str) -> Result<Machine, String> {
        Self::load_with_lane(source, Lane::Interp)
    }

    pub fn load_with_lane(source: &str, lane: Lane) -> Result<Machine, String> {
        let mut modules = BTreeMap::new();
        modules.insert("root".to_string(), source.to_string());
        Self::load_modules_with_lane("root", modules, lane)
    }

    pub fn load_modules(root: &str, modules: BTreeMap<String, String>) -> Result<Machine, String> {
        Self::load_modules_with_lane(root, modules, Lane::Interp)
    }

    pub fn load_modules_with_lane(
        root: &str,
        modules: BTreeMap<String, String>,
        lane: Lane,
    ) -> Result<Machine, String> {
        let mut modules = modules;
        inject_std_modules(&mut modules);
        let lower_options = LowerOptions::from_env();
        let c = compile_module_set(root, &modules, RefSource::Fresh, lower_options)?;

        let mut driver =
            Driver::try_with_schema_tables(c.program, c.lowered, c.descriptors, c.schemas, lane)?;
        assert_literal_handles(&mut driver, "String", &c.literal_handles.strings);
        assert_literal_handles(&mut driver, "Path", &c.literal_handles.paths);
        assert_literal_handles(&mut driver, "Flag", &c.literal_handles.flags);
        assert_literal_handles(&mut driver, "Template", &c.literal_handles.templates);

        let source = modules.get(root).cloned().unwrap_or_default();
        Ok(Machine {
            driver,
            fn_refs: c.fn_refs,
            fn_params: c.fn_params.into_iter().collect(),
            fn_param_names: c.fn_param_names.into_iter().collect(),
            fn_returns: c.fn_returns.into_iter().collect(),
            render_names: c.render_names,
            root: root.to_string(),
            modules,
            source,
            module_hash: c.module_hash,
            lower_options,
            diagnostics: c.diagnostics,
        })
    }

    pub fn with_fetch_backend(mut self, backend: impl FetchBackend + 'static) -> Self {
        self.driver.set_fetch_backend(Arc::new(backend));
        self
    }

    pub fn with_fetch_backend_arc(mut self, backend: Arc<dyn FetchBackend>) -> Self {
        self.driver.set_fetch_backend(backend);
        self
    }

    pub fn with_exec_backend(mut self, backend: Arc<dyn MachineExecBackend>) -> Self {
        self.driver.set_exec_backend(Some(backend));
        self
    }

    pub fn reload(&mut self, source: &str) -> Result<ReloadDiff, String> {
        let mut modules = BTreeMap::new();
        modules.insert("root".to_string(), source.to_string());
        self.reload_modules("root", modules)
    }

    pub fn reload_modules(
        &mut self,
        root: &str,
        modules: BTreeMap<String, String>,
    ) -> Result<ReloadDiff, String> {
        let mut modules = modules;
        inject_std_modules(&mut modules);
        let before = self.fn_hashes();
        let c = compile_module_set(
            root,
            &modules,
            RefSource::Existing(&mut self.driver),
            self.lower_options,
        )?;

        self.driver
            .reload(c.program, c.lowered, c.descriptors, c.schemas)?;
        self.fn_refs = c.fn_refs;
        self.fn_params = c.fn_params.into_iter().collect();
        self.fn_param_names = c.fn_param_names.into_iter().collect();
        self.fn_returns = c.fn_returns.into_iter().collect();
        self.render_names = c.render_names;
        self.root = root.to_string();
        self.source = modules.get(root).cloned().unwrap_or_default();
        self.modules = modules;
        self.module_hash = c.module_hash;
        self.diagnostics = c.diagnostics;

        let after = self.fn_hashes();
        let changed = before
            .keys()
            .chain(after.keys())
            .filter(|name| before.get(*name) != after.get(*name))
            .cloned()
            .collect();
        Ok(ReloadDiff { changed })
    }

    /// Demand a function's value at the edge (scalars, this slice).
    pub fn demand_i64(&mut self, name: &str, args: Vec<i64>) -> Result<i64, String> {
        let fn_ref = *self
            .fn_refs
            .get(name)
            .ok_or_else(|| format!("no function named {name}"))?;
        self.driver.demand(FnRef::new(fn_ref), args)
    }

    pub fn demand_f64(&mut self, name: &str, args: Vec<i64>) -> Result<f64, String> {
        let bits = self.demand_i64(name, args)? as u64;
        Ok(f64::from_bits(bits))
    }

    pub fn call(&mut self, name: &str, args: &[NamedArg]) -> Result<StoreHandle, String> {
        let params = self
            .fn_params
            .get(name)
            .ok_or_else(|| format!("no function named {name}"))?
            .clone();
        let names = self
            .fn_param_names
            .get(name)
            .ok_or_else(|| format!("no parameter names for {name}"))?
            .clone();
        let mut words = Vec::with_capacity(params.len());
        for (param_name, schema) in names.iter().zip(&params) {
            let arg = args
                .iter()
                .find(|arg| arg.name == *param_name)
                .ok_or_else(|| format!("missing argument `{param_name}`"))?;
            words.push(self.intern_arg(schema, arg.value.clone())?.0);
        }
        self.demand_i64(name, words).map(StoreHandle)
    }

    pub fn intern_arg(&self, schema: &str, arg: MachineArg) -> Result<StoreHandle, String> {
        match (schema, arg) {
            (_, MachineArg::Word(word)) => Ok(StoreHandle(word)),
            (expected, MachineArg::Handle(handle)) => {
                if matches!(expected, "Int" | "Float" | "Bool") {
                    return Ok(handle);
                }
                let actual = self
                    .driver
                    .store_entry(handle.0)
                    .ok_or_else(|| format!("store handle {}", handle.0))?
                    .schema;
                if actual != expected {
                    return Err(format!(
                        "store handle {} is `{actual}`, expected `{expected}`",
                        handle.0
                    ));
                }
                Ok(handle)
            }
            ("Int", MachineArg::Int(value)) => Ok(StoreHandle(value)),
            ("Float", MachineArg::Float(value)) => Ok(StoreHandle(value.to_bits() as i64)),
            ("Bool", MachineArg::Bool(value)) => Ok(StoreHandle(value as i64)),
            ("Version", MachineArg::String(value)) => {
                Ok(StoreHandle(self.driver.intern_version_value(&value)?.0))
            }
            ("VersionSet", MachineArg::String(value)) => Ok(StoreHandle(
                self.driver.intern_version_set_req_value(&value)?.0,
            )),
            ("String", MachineArg::String(value)) => Ok(StoreHandle(
                self.driver.intern_raw_value("String", value.into_bytes()).0,
            )),
            ("Path", MachineArg::Path(value)) => Ok(StoreHandle(
                self.driver.intern_raw_value("Path", value.into_bytes()).0,
            )),
            ("Flag", MachineArg::Flag(value)) => Ok(StoreHandle(
                self.driver.intern_raw_value("Flag", value.into_bytes()).0,
            )),
            ("Tree", MachineArg::Tree(tree)) => {
                Ok(StoreHandle(self.driver.intern_tree_concrete(tree)))
            }
            ("Target", MachineArg::LinuxTarget) => Ok(StoreHandle(self.linux_target_handle())),
            (expected, other) => Err(format!("cannot intern {other:?} as {expected}")),
        }
    }

    pub fn export_value(&self, root: StoreHandle) -> Result<ValueBundle, String> {
        self.driver
            .export_value_bundle(root.0, vec![self.code_bundle()])
    }

    pub fn import_value(&self, bundle: &ValueBundle) -> Result<StoreHandle, String> {
        self.driver.import_value_bundle(bundle).map(StoreHandle)
    }

    pub fn pending_function(
        &self,
        name: &str,
        args: &[NamedArg],
    ) -> Result<(StoreHandle, ValueBundle), String> {
        let fn_ref = *self
            .fn_refs
            .get(name)
            .ok_or_else(|| format!("no function named {name}"))?;
        let params = self
            .fn_params
            .get(name)
            .ok_or_else(|| format!("no function named {name}"))?
            .clone();
        let names = self
            .fn_param_names
            .get(name)
            .ok_or_else(|| format!("no parameter names for {name}"))?
            .clone();
        let mut words = Vec::new();
        for arg in args {
            let Some(index) = names.iter().position(|name| name == &arg.name) else {
                return Err(format!("unknown argument `{}` for {name}", arg.name));
            };
            words.push((index, self.intern_arg(&params[index], arg.value.clone())?.0));
        }
        words.sort_by_key(|(index, _)| *index);
        if words
            .iter()
            .enumerate()
            .any(|(expected, (actual, _))| expected != *actual)
        {
            return Err(format!("pending args for {name} must be a prefix"));
        }
        let words = words.into_iter().map(|(_, word)| word).collect();
        let (handle, _) = self.driver.pending_for_fn(FnRef::new(fn_ref), words)?;
        let handle = StoreHandle(handle);
        let bundle = self.export_value(handle)?;
        Ok((handle, bundle))
    }

    pub fn invoke_pending(
        &mut self,
        pending: StoreHandle,
        args: &[StoreHandle],
    ) -> Result<StoreHandle, String> {
        self.driver
            .invoke_pending_handle(pending.0, args.iter().map(|arg| arg.0).collect())
            .map(StoreHandle)
    }

    pub fn intern_run_value(
        &self,
        ok: bool,
        outputs: crate::exec::Tree,
    ) -> Result<StoreHandle, String> {
        self.driver.intern_run_value(ok, outputs).map(StoreHandle)
    }

    pub fn intern_tree(&self, tree: crate::exec::Tree) -> StoreHandle {
        StoreHandle(self.driver.intern_tree_concrete(tree))
    }

    pub fn code_bundle(&self) -> CodeBundle {
        CodeBundle {
            module_hash: self.module_hash.clone(),
            bytes: self.source.as_bytes().to_vec(),
        }
    }

    pub fn load_code_bundle(bundle: &CodeBundle) -> Result<Machine, String> {
        let source = String::from_utf8(bundle.bytes.clone()).map_err(|err| err.to_string())?;
        let actual = module_hash(&source);
        if actual != bundle.module_hash {
            return Err("code bundle hash mismatch".into());
        }
        Machine::load(&source)
    }

    pub fn linux_target_handle(&self) -> i64 {
        self.driver.intern_linux_target().0
    }

    pub fn trace(&self) -> &[DriveEvent] {
        &self.driver.trace
    }

    pub fn clear_trace(&mut self) {
        self.driver.trace.clear();
    }

    pub fn set_event_sink(&mut self, sink: Option<DriveEventSink>) {
        self.driver.set_event_sink(sink);
    }

    pub fn set_step_mode(&mut self, mode: StepMode) {
        self.driver.set_step_mode(mode);
    }

    pub fn set_force_molten_copy(&mut self, force: bool) {
        self.driver.set_force_molten_copy(force);
    }

    pub fn set_force_tail_invoke(&mut self, force: bool) -> Result<ReloadDiff, String> {
        self.lower_options.force_tail_invoke = force;
        let root = self.root.clone();
        let modules = self.modules.clone();
        self.reload_modules(&root, modules)
    }

    pub fn diagnostics(&self) -> &[String] {
        &self.diagnostics
    }

    pub fn entry_param_schemas(&self, name: &str) -> Option<&[String]> {
        self.fn_params.get(name).map(Vec::as_slice)
    }

    pub fn entry_return_schema(&self, name: &str) -> Option<&str> {
        self.fn_returns.get(name).map(String::as_str)
    }

    pub fn render_value(&self, schema: &str, word: i64) -> Result<RenderedValue, String> {
        self.driver.render_word(schema, word, &self.render_names)
    }

    pub fn render_result(&self, name: &str, word: i64) -> Result<RenderedValue, String> {
        let schema = self
            .entry_return_schema(name)
            .ok_or_else(|| format!("no function named {name}"))?;
        self.render_value(schema, word)
    }

    pub fn store_len(&self) -> usize {
        self.driver.store_len()
    }

    pub fn molten_debug_counts(&self) -> (usize, usize, usize, usize, usize, usize) {
        self.driver.molten_debug_counts()
    }

    pub fn molten_stats(&self) -> MoltenStats {
        self.driver.molten_stats()
    }

    pub fn tree_entries(
        &mut self,
        handle: i64,
    ) -> Result<std::collections::BTreeMap<String, String>, String> {
        self.driver.tree_entries(handle)
    }

    pub fn tree_blob_entries(
        &mut self,
        handle: i64,
    ) -> Result<std::collections::BTreeMap<String, Vec<u8>>, String> {
        self.driver.tree_blob_entries(handle)
    }

    #[cfg(test)]
    fn intern_tree_concrete(&self, tree: crate::exec::Tree) -> i64 {
        self.driver.intern_tree_concrete(tree)
    }

    pub fn fn_hash(&self, name: &str) -> Option<u64> {
        self.fn_refs
            .get(name)
            .map(|&fn_ref| self.driver.fn_hash(FnRef::new(fn_ref)))
    }

    pub fn fn_hashes(&self) -> BTreeMap<String, u64> {
        self.fn_refs
            .keys()
            .filter_map(|name| self.fn_hash(name).map(|hash| (name.clone(), hash)))
            .collect()
    }

    #[cfg(test)]
    fn fn_ops(&self, name: &str) -> Option<&[Op]> {
        self.fn_refs
            .get(name)
            .map(|&fn_ref| self.driver.fn_ops(FnRef::new(fn_ref)))
    }

    #[cfg(test)]
    fn semantic_comparator_len(&self, name: &str) -> Option<usize> {
        self.fn_refs
            .get(name)
            .map(|&fn_ref| self.driver.semantic_comparator_len(FnRef::new(fn_ref)))
    }
}

fn module_hash(source: &str) -> Vec<u8> {
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vix-machine-module");
    hasher.update(source.as_bytes());
    hasher.finalize().as_bytes().to_vec()
}

/// The interned handle tables for a compiled module set. Fresh (0-based) on a
/// cold load; preserved from the live driver on reload so warm memo/store stay
/// valid.
struct LiteralHandleMaps {
    strings: HashMap<String, i64>,
    paths: HashMap<String, i64>,
    flags: HashMap<String, i64>,
    templates: HashMap<String, i64>,
}

#[derive(Clone, Copy)]
struct LowerOptions {
    force_tail_invoke: bool,
}

impl LowerOptions {
    fn from_env() -> Self {
        Self {
            force_tail_invoke: std::env::var_os("VIX_FORCE_TAIL_INVOKE").is_some(),
        }
    }
}

/// Where schema-refs and literal handles come from — the one axis on which cold
/// load and warm reload differ. Fresh assigns 0..N; Existing preserves the live
/// driver's numbering (and extends it), which is what keeps warm state valid.
enum RefSource<'a> {
    Fresh,
    Existing(&'a mut Driver),
}

impl RefSource<'_> {
    fn literal_handles(&mut self, tables: &ModuleTables) -> LiteralHandleMaps {
        match self {
            RefSource::Fresh => {
                let strings = string_handles(tables);
                let paths = path_handles(tables, strings.len());
                let flags = flag_handles(tables, strings.len() + paths.len());
                let templates = template_handles(tables, strings.len() + paths.len() + flags.len());
                LiteralHandleMaps {
                    strings,
                    paths,
                    flags,
                    templates,
                }
            }
            RefSource::Existing(driver) => {
                let driver: &Driver = driver;
                LiteralHandleMaps {
                    strings: live_string_handles(driver, tables),
                    paths: live_path_handles(driver, tables),
                    flags: live_flag_handles(driver, tables),
                    templates: live_template_handles(driver, tables),
                }
            }
        }
    }
}

/// Everything a module set lowers to, before it is handed to a driver — the
/// single shared pipeline behind both cold load and warm reload.
struct Compiled {
    program: Program,
    lowered: Vec<LoweredFn>,
    descriptors: DescriptorMap,
    schemas: SchemaTables,
    fn_refs: HashMap<String, usize>,
    fn_returns: HashMap<String, String>,
    fn_params: HashMap<String, Vec<String>>,
    fn_param_names: HashMap<String, Vec<String>>,
    render_names: RenderNames,
    literal_handles: LiteralHandleMaps,
    module_hash: Vec<u8>,
    diagnostics: Vec<String>,
}

/// Compile a module set into a lowered program: the whole of loading a vix
/// program *except* interning into a driver. Cold load and warm reload share it
/// verbatim, differing only in `ref_source` (fresh vs. driver-backed) and in
/// what they do with the `Compiled` result.
fn compile_module_set(
    root: &str,
    modules: &BTreeMap<String, String>,
    mut ref_source: RefSource,
    lower_options: LowerOptions,
) -> Result<Compiled, String> {
    let module_hash = module_set_hash(root, modules);
    let tables = load_module_tables_from_modules(root, modules)?;
    debug_assert!(tables.has_schema("Int"));
    debug_assert!(tables.has_schema("Map"));

    // Deterministic fn_ref assignment: sorted names.
    let mut names: Vec<&String> = tables.fns.keys().collect();
    names.sort();
    let fn_refs: HashMap<String, usize> = names
        .iter()
        .enumerate()
        .map(|(ix, name)| ((*name).clone(), ix))
        .collect();
    let fn_returns: HashMap<String, String> = names
        .iter()
        .map(|name| {
            let item = &tables.fns[*name];
            let schema = item
                .return_type
                .as_ref()
                .map(type_schema_name)
                .transpose()?
                .unwrap_or_else(|| "Int".into());
            Ok(((*name).clone(), schema))
        })
        .collect::<Result<_, String>>()?;
    let fn_params: HashMap<String, Vec<String>> = names
        .iter()
        .map(|name| {
            let item = &tables.fns[*name];
            Ok((
                (*name).clone(),
                item.params
                    .params
                    .iter()
                    .map(|param| type_schema_name(&param.ty))
                    .collect::<Result<Vec<_>, _>>()?,
            ))
        })
        .collect::<Result<_, String>>()?;
    let fn_param_names: HashMap<String, Vec<String>> = names
        .iter()
        .map(|name| {
            let item = &tables.fns[*name];
            (
                (*name).clone(),
                item.params
                    .params
                    .iter()
                    .map(|param| param.name.value.clone())
                    .collect(),
            )
        })
        .collect();
    let mut schema_names = schema_names_for(&tables, &fn_returns, &fn_params)?;
    schema_names.sort();
    schema_names.dedup();
    let mut schemas = tables.schemas.clone();
    schemas.register_frame_names(schema_names.clone());
    let render_names = render_names_for(&tables);
    let schema_words = schema_names
        .iter()
        .map(|name| (name.clone(), schemas.frame_word_for_name(name)))
        .collect::<HashMap<_, _>>();
    let handles = ref_source.literal_handles(&tables);
    let literal_handles = LiteralHandles {
        strings: &handles.strings,
        paths: &handles.paths,
        flags: &handles.flags,
        templates: &handles.templates,
    };
    let signatures = FnSignatures {
        returns: &fn_returns,
        params: &fn_params,
        param_names: &fn_param_names,
    };

    let mut task_fns = Vec::with_capacity(names.len());
    let mut lowered = Vec::with_capacity(names.len());
    let mut diagnostics = Vec::new();
    for (ix, name) in names.iter().enumerate() {
        let item = &tables.fns[*name];
        let hash = tables.fn_hashes[*name];
        diagnostics.extend(tail_self_call_diagnostics(
            item,
            &tables,
            &tables.fn_modules[*name],
            name,
        ));
        let (task_fn, info) = if parked_generic_or_fn_typed(item) {
            parked_stub(item)?
        } else {
            FnLowerer::lower(
                item,
                LowerEnv {
                    tables: &tables,
                    current_module: &tables.fn_modules[*name],
                    current_fn_name: name,
                    current_fn_ref: ix,
                    fn_refs: &fn_refs,
                    signatures,
                    schema_words: &schema_words,
                    literal_handles,
                    lower_options,
                },
            )
            .map_err(|e| format!("lowering {name}: {e}"))?
        };
        task_fns.push(task_fn);
        let arg_schemas = item
            .params
            .params
            .iter()
            .map(|param| type_schema_name(&param.ty))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| format!("lowering {name}: {e}"))?;
        let param_names = item
            .params
            .params
            .iter()
            .map(|param| param.name.value.clone())
            .collect::<Vec<_>>();
        let semantic_comparators = semantic_comparators_for(
            name,
            &arg_schemas,
            &param_names,
            &fn_refs,
            &fn_params,
            &fn_returns,
        )?;
        let hash = hash_with_semantic_comparators(hash, &semantic_comparators, &names, &tables);
        lowered.push(LoweredFn {
            hash,
            task_fn: FnId(u32::try_from(ix).expect("fn count fits u32")),
            arg_offsets: info.arg_offsets,
            arg_schemas,
            return_schema: fn_returns[*name].clone(),
            semantic_comparators,
            invoke_region: info.invoke_region,
            store_alloc_region: info.store_alloc_region,
            store_read_region: info.store_read_region,
            store_tag_region: info.store_tag_region,
            primitive_region: info.primitive_region,
        });
    }

    let mut descriptors = tables.descriptors;
    add_builtin_descriptors(&mut descriptors, &schemas);
    for name in &schema_names {
        if let Some(descriptor) = derived_descriptor(&schemas, name) {
            descriptors.insert_named_if_absent(&schemas, name, || descriptor);
        }
    }

    Ok(Compiled {
        program: Program { fns: task_fns },
        lowered,
        descriptors,
        schemas,
        fn_refs,
        fn_returns,
        fn_params,
        fn_param_names,
        render_names,
        literal_handles: handles,
        module_hash,
        diagnostics,
    })
}

/// On a cold load the driver's interning must reproduce the fresh handle
/// assignment exactly — assert it, in handle order.
fn assert_literal_handles(driver: &mut Driver, schema: &str, handles: &HashMap<String, i64>) {
    let mut sorted: Vec<(&String, &i64)> = handles.iter().collect();
    sorted.sort_by_key(|(_, handle)| **handle);
    for (value, expected) in sorted {
        let (actual, _) = driver.intern_raw_value(schema, value.as_bytes().to_vec());
        assert_eq!(
            actual, *expected,
            "{schema} handle assignment is deterministic"
        );
    }
}

/// Pull the bundled vix standard library into a program's module set — but only
/// when the program actually references it, so programs that never touch
/// `Version` don't pay for its source (interned literals, lowered fns). Applied
/// on load and reload alike, keyed on the same source, so the module set is
/// identical across warm reloads. `use vix::X` then resolves to std via the
/// binder's real-module lookup, with host primitives as the fallback.
///
/// The reference test is a heuristic word scan pending proper import-driven
/// inclusion; a false positive merely includes std unnecessarily.
fn inject_std_modules(modules: &mut BTreeMap<String, String>) {
    if modules.contains_key("vix") || !references_vix_std(modules) {
        return;
    }
    modules.insert(
        "vix".to_string(),
        include_str!("../../std/version.vix").to_string(),
    );
}

fn references_vix_std(modules: &BTreeMap<String, String>) -> bool {
    const STD_NAMES: &[&str] = &["Version", "parse_version", "version_lte", "Ordering"];
    modules
        .values()
        .any(|source| STD_NAMES.iter().any(|name| contains_word(source, name)))
}

/// Whole-word substring test — so `Version` does not match inside `VersionSet`.
fn contains_word(haystack: &str, word: &str) -> bool {
    let is_boundary = |ch: Option<char>| ch.is_none_or(|c| !c.is_alphanumeric() && c != '_');
    haystack.match_indices(word).any(|(start, _)| {
        is_boundary(haystack[..start].chars().next_back())
            && is_boundary(haystack[start + word.len()..].chars().next())
    })
}

fn module_set_hash(root: &str, modules: &BTreeMap<String, String>) -> Vec<u8> {
    if modules.len() == 1
        && root == "root"
        && let Some(source) = modules.get(root)
    {
        return module_hash(source);
    }
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vix-machine-module-set");
    hasher.update(root.as_bytes());
    for (path, source) in modules {
        hasher.update(path.as_bytes());
        hasher.update(&(source.len() as u64).to_le_bytes());
        hasher.update(source.as_bytes());
    }
    hasher.finalize().as_bytes().to_vec()
}

fn schema_names_for(
    tables: &ModuleTables,
    fn_returns: &HashMap<String, String>,
    fn_params: &HashMap<String, Vec<String>>,
) -> Result<Vec<String>, String> {
    let mut schema_names: Vec<String> = tables.descriptors.keys().cloned().collect();
    schema_names.extend([
        "String".to_string(),
        "Path".to_string(),
        "Blob".to_string(),
        "Bool".to_string(),
        "Target".to_string(),
        "Arch".to_string(),
        "Os".to_string(),
        "Cc".to_string(),
        "Ar".to_string(),
        "Rustc".to_string(),
        "Run".to_string(),
        "Flag".to_string(),
        "Arg".to_string(),
        "Template".to_string(),
        "Sealed".to_string(),
        "Tree".to_string(),
        "Array".to_string(),
        "Array<Arg>".to_string(),
        "Array<Doc>".to_string(),
        "Array<Path>".to_string(),
        "Map".to_string(),
        "Doc".to_string(),
        "Version".to_string(),
        "VersionSet".to_string(),
        "Option<Doc>".to_string(),
        "Pending<Doc>".to_string(),
        "Realized<Doc>".to_string(),
        "Option<Realized<Doc>>".to_string(),
        "Map<String,Doc>".to_string(),
        "Map<String,Realized<Doc>>".to_string(),
        "Tuple<Int,Array>".to_string(),
    ]);
    for schema in fn_returns.values() {
        push_schema_closure(schema, &mut schema_names);
    }
    for schemas in fn_params.values() {
        for schema in schemas {
            push_schema_closure(schema, &mut schema_names);
        }
    }
    for descriptor in tables.descriptors.values() {
        collect_descriptor_schemas(&tables.schemas, descriptor, &mut schema_names);
    }
    for item in tables.fns.values() {
        collect_block_type_schemas(&item.body, &mut schema_names)?;
        collect_expr_schemas_in_block(&item.body, &mut schema_names, tables, fn_returns)?;
    }
    Ok(schema_names)
}

fn render_names_for(tables: &ModuleTables) -> RenderNames {
    let mut structs: BTreeMap<String, Vec<String>> = tables
        .structs
        .iter()
        .map(|(name, info)| {
            (
                name.clone(),
                info.fields
                    .iter()
                    .map(|(field, _)| field.clone())
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    structs
        .entry("Target".into())
        .or_insert_with(|| vec!["os".into(), "arch".into()]);
    let mut enums: BTreeMap<String, Vec<RenderVariant>> = tables
        .enums
        .iter()
        .map(|(name, info)| {
            (
                name.clone(),
                info.variants
                    .iter()
                    .map(|(variant, shape)| RenderVariant {
                        name: variant.clone(),
                        fields: match shape {
                            VariantShape::Unit => Vec::new(),
                            VariantShape::Tuple(count) => {
                                (0..*count).map(|index| index.to_string()).collect()
                            }
                            VariantShape::Record(fields) => fields.clone(),
                        },
                    })
                    .collect::<Vec<_>>(),
            )
        })
        .collect();
    enums.entry("Os".into()).or_insert_with(|| {
        ["Linux", "Macos", "Windows"]
            .into_iter()
            .map(|name| RenderVariant {
                name: name.into(),
                fields: Vec::new(),
            })
            .collect()
    });
    enums.entry("Arch".into()).or_insert_with(|| {
        ["X86_64", "Aarch64", "Arm", "Riscv64", "Wasm32", "Unknown"]
            .into_iter()
            .map(|name| RenderVariant {
                name: name.into(),
                fields: Vec::new(),
            })
            .collect()
    });
    RenderNames { structs, enums }
}

fn push_schema_closure(schema: &str, schema_names: &mut Vec<String>) {
    schema_names.push(schema.to_string());
    schema_names.push(pending_schema(schema));
    if let Some(value_schema) = map_value_schema(schema) {
        let pending = pending_schema(value_schema);
        let realized = realized_schema(value_schema);
        schema_names.push(value_schema.to_string());
        schema_names.push(pending);
        schema_names.push(option_schema(value_schema));
        schema_names.push(realized.clone());
        schema_names.push(option_schema(&realized));
        if let Some((key_schema, _)) = map_schemas(schema) {
            schema_names.push(map_schema(key_schema, &realized));
        }
    }
}

fn collect_descriptor_schemas(
    schemas: &SchemaTables,
    descriptor: &VixDescriptor,
    out: &mut Vec<String>,
) {
    push_schema_closure(&schemas.display_ref(&descriptor.schema), out);
    match &descriptor.access {
        weavy::mem::Access::Handle { target } => {
            push_schema_closure(&schemas.display_ref(target), out)
        }
        weavy::mem::Access::Array { element, .. } => {
            collect_descriptor_schemas(schemas, element, out)
        }
        weavy::mem::Access::Record(record) => {
            for field in &record.fields {
                collect_descriptor_schemas(schemas, &field.descriptor, out);
            }
        }
        weavy::mem::Access::Enum(access) => {
            for variant in &access.variants {
                for field in &variant.payload.fields {
                    collect_descriptor_schemas(schemas, &field.descriptor, out);
                }
            }
        }
        _ => {}
    }
}

fn string_handles(tables: &ModuleTables) -> HashMap<String, i64> {
    string_literals(tables)
        .into_iter()
        .enumerate()
        .map(|(ix, value)| (value, i64::try_from(ix).expect("string handle fits i64")))
        .collect()
}

fn live_string_handles(driver: &Driver, tables: &ModuleTables) -> HashMap<String, i64> {
    string_literals(tables)
        .into_iter()
        .map(|value| {
            let (handle, _) = driver.intern_raw_value("String", value.as_bytes().to_vec());
            (value, handle)
        })
        .collect()
}

fn string_literals(tables: &ModuleTables) -> BTreeSet<String> {
    let mut strings = BTreeSet::new();
    for item in tables.fns.values() {
        collect_block_strings(&item.body, &mut strings);
    }
    strings
}

fn path_handles(tables: &ModuleTables, offset: usize) -> HashMap<String, i64> {
    path_literals(tables)
        .into_iter()
        .enumerate()
        .map(|(ix, value)| {
            (
                value,
                i64::try_from(offset + ix).expect("path handle fits i64"),
            )
        })
        .collect()
}

fn live_path_handles(driver: &Driver, tables: &ModuleTables) -> HashMap<String, i64> {
    path_literals(tables)
        .into_iter()
        .map(|value| {
            let (handle, _) = driver.intern_raw_value("Path", value.as_bytes().to_vec());
            (value, handle)
        })
        .collect()
}

fn path_literals(tables: &ModuleTables) -> BTreeSet<String> {
    let mut paths = BTreeSet::new();
    for item in tables.fns.values() {
        collect_block_paths(&item.body, &mut paths);
    }
    paths
}

fn flag_handles(tables: &ModuleTables, offset: usize) -> HashMap<String, i64> {
    flag_literals(tables)
        .into_iter()
        .enumerate()
        .map(|(ix, value)| {
            (
                value,
                i64::try_from(offset + ix).expect("flag handle fits i64"),
            )
        })
        .collect()
}

fn live_flag_handles(driver: &Driver, tables: &ModuleTables) -> HashMap<String, i64> {
    flag_literals(tables)
        .into_iter()
        .map(|value| {
            let (handle, _) = driver.intern_raw_value("Flag", value.as_bytes().to_vec());
            (value, handle)
        })
        .collect()
}

fn flag_literals(tables: &ModuleTables) -> BTreeSet<String> {
    let mut flags = BTreeSet::new();
    for item in tables.fns.values() {
        collect_block_flags(&item.body, &mut flags);
    }
    flags
}

fn template_handles(tables: &ModuleTables, offset: usize) -> HashMap<String, i64> {
    template_literals(tables)
        .into_iter()
        .enumerate()
        .map(|(ix, value)| {
            (
                value,
                i64::try_from(offset + ix).expect("template handle fits i64"),
            )
        })
        .collect()
}

fn live_template_handles(driver: &Driver, tables: &ModuleTables) -> HashMap<String, i64> {
    template_literals(tables)
        .into_iter()
        .map(|value| {
            let (handle, _) = driver.intern_raw_value("Template", value.as_bytes().to_vec());
            (value, handle)
        })
        .collect()
}

fn template_literals(tables: &ModuleTables) -> BTreeSet<String> {
    let mut templates = BTreeSet::new();
    for item in tables.fns.values() {
        collect_block_templates(&item.body, &mut templates);
    }
    templates
}

fn collect_block_strings(block: &ast::Block, out: &mut BTreeSet<String>) {
    for stmt in &block.stmts {
        match stmt {
            ast::Stmt::Let(l) => collect_expr_strings(&l.value, out),
            ast::Stmt::Expr(e) => collect_expr_strings(&e.expr, out),
        }
    }
    if let Some(tail) = &block.tail {
        collect_expr_strings(tail, out);
    }
}

fn collect_block_templates(block: &ast::Block, out: &mut BTreeSet<String>) {
    for stmt in &block.stmts {
        match stmt {
            ast::Stmt::Let(l) => collect_expr_templates(&l.value, out),
            ast::Stmt::Expr(e) => collect_expr_templates(&e.expr, out),
        }
    }
    if let Some(tail) = &block.tail {
        collect_expr_templates(tail, out);
    }
}

fn collect_block_paths(block: &ast::Block, out: &mut BTreeSet<String>) {
    for stmt in &block.stmts {
        match stmt {
            ast::Stmt::Let(l) => collect_expr_paths(&l.value, out),
            ast::Stmt::Expr(e) => collect_expr_paths(&e.expr, out),
        }
    }
    if let Some(tail) = &block.tail {
        collect_expr_paths(tail, out);
    }
}

fn collect_block_flags(block: &ast::Block, out: &mut BTreeSet<String>) {
    for stmt in &block.stmts {
        match stmt {
            ast::Stmt::Let(l) => collect_expr_flags(&l.value, out),
            ast::Stmt::Expr(e) => collect_expr_flags(&e.expr, out),
        }
    }
    if let Some(tail) = &block.tail {
        collect_expr_flags(tail, out);
    }
}

fn collect_expr_paths(expr: &ast::Expr, out: &mut BTreeSet<String>) {
    match expr {
        ast::Expr::Path(p) => {
            out.insert(p.value.clone());
        }
        ast::Expr::Binary(b) => {
            collect_expr_paths(&b.left, out);
            collect_expr_paths(&b.right, out);
        }
        ast::Expr::Unary(u) => collect_expr_paths(&u.operand, out),
        ast::Expr::Call(c) => {
            for arg in &c.args.args {
                match arg {
                    ast::Arg::Expr(e) => collect_expr_paths(e, out),
                    ast::Arg::Kwarg(k) => collect_expr_paths(&k.value, out),
                    ast::Arg::Partial(_) => {}
                }
            }
        }
        ast::Expr::MethodCall(m) => {
            collect_expr_paths(&m.receiver, out);
            for arg in &m.args.args {
                match arg {
                    ast::Arg::Expr(e) => collect_expr_paths(e, out),
                    ast::Arg::Kwarg(k) => collect_expr_paths(&k.value, out),
                    ast::Arg::Partial(_) => {}
                }
            }
        }
        ast::Expr::Match(m) => {
            collect_expr_paths(&m.scrutinee, out);
            for arm in &m.arms {
                if let Some(guard) = &arm.guard {
                    collect_expr_paths(guard, out);
                }
                collect_expr_paths(&arm.value, out);
            }
        }
        ast::Expr::StructLit(lit) => {
            for field in &lit.fields {
                collect_expr_paths(&field.value, out);
            }
        }
        ast::Expr::Paren(p) => collect_expr_paths(&p.inner, out),
        ast::Expr::Field(f) => collect_expr_paths(&f.receiver, out),
        ast::Expr::Tuple(t) => {
            for elem in &t.elems {
                collect_expr_paths(elem, out);
            }
        }
        ast::Expr::Array(a) => {
            for elem in &a.elems {
                if let ast::ArrayElem::Expr(e) = elem {
                    collect_expr_paths(e, out);
                }
            }
        }
        ast::Expr::Map(m) => {
            for entry in &m.entries {
                collect_expr_paths(&entry.key, out);
                collect_expr_paths(&entry.value, out);
            }
        }
        ast::Expr::Closure(c) => collect_expr_paths(&c.body, out),
        ast::Expr::Command(c) => {
            for part in &c.parts {
                if let ast::CommandPart::Splice(s) = part {
                    collect_expr_paths(&s.expr, out);
                }
            }
        }
        ast::Expr::Scoped(_)
        | ast::Expr::Identifier(_)
        | ast::Expr::Template(_)
        | ast::Expr::Str(_)
        | ast::Expr::Number(_)
        | ast::Expr::Bool(_) => {}
    }
}

fn collect_expr_strings(expr: &ast::Expr, out: &mut BTreeSet<String>) {
    match expr {
        ast::Expr::Str(s) => {
            out.insert(s.value.clone());
        }
        ast::Expr::Template(t) => {
            if let Ok(source) = decode_template_literal(&t.value)
                && let Ok(parts) = parse_template(&source)
            {
                out.insert(String::new());
                for part in parts {
                    match part {
                        TemplatePart::Text(text) => {
                            out.insert(text);
                        }
                        TemplatePart::Hole(hole) => {
                            out.insert(hole.name);
                            for filter in hole.filters {
                                if let TemplateFilter::Default(value) = filter {
                                    out.insert(value);
                                }
                            }
                        }
                    }
                }
            }
        }
        ast::Expr::Binary(b) => {
            collect_expr_strings(&b.left, out);
            collect_expr_strings(&b.right, out);
        }
        ast::Expr::Unary(u) => collect_expr_strings(&u.operand, out),
        ast::Expr::Call(c) => {
            for arg in &c.args.args {
                match arg {
                    ast::Arg::Expr(e) => collect_expr_strings(e, out),
                    ast::Arg::Kwarg(k) => collect_expr_strings(&k.value, out),
                    ast::Arg::Partial(_) => {}
                }
            }
        }
        ast::Expr::Match(m) => {
            collect_expr_strings(&m.scrutinee, out);
            for arm in &m.arms {
                collect_pattern_strings(&arm.pattern, out);
                if let Some(guard) = &arm.guard {
                    collect_expr_strings(guard, out);
                }
                collect_expr_strings(&arm.value, out);
            }
        }
        ast::Expr::StructLit(lit) => {
            for field in &lit.fields {
                collect_expr_strings(&field.value, out);
            }
            for spread in &lit.spreads {
                if let Some(base) = &spread.base {
                    collect_expr_strings(base, out);
                }
            }
        }
        ast::Expr::Paren(p) => collect_expr_strings(&p.inner, out),
        ast::Expr::Field(f) => {
            collect_expr_strings(&f.receiver, out);
            if let ast::Member::Identifier(name) = &f.name {
                out.insert(name.value.clone());
            }
        }
        ast::Expr::Tuple(t) => {
            for elem in &t.elems {
                collect_expr_strings(elem, out);
            }
        }
        ast::Expr::Array(a) => {
            for elem in &a.elems {
                match elem {
                    ast::ArrayElem::Expr(e) => collect_expr_strings(e, out),
                    ast::ArrayElem::Flag(_) => {}
                }
            }
        }
        ast::Expr::Map(m) => {
            for entry in &m.entries {
                collect_expr_strings(&entry.key, out);
                collect_expr_strings(&entry.value, out);
            }
        }
        ast::Expr::MethodCall(m) => {
            collect_expr_strings(&m.receiver, out);
            for arg in &m.args.args {
                match arg {
                    ast::Arg::Expr(e) => collect_expr_strings(e, out),
                    ast::Arg::Kwarg(k) => collect_expr_strings(&k.value, out),
                    ast::Arg::Partial(_) => {}
                }
            }
        }
        ast::Expr::Closure(c) => collect_expr_strings(&c.body, out),
        ast::Expr::Command(c) => {
            for part in &c.parts {
                match part {
                    ast::CommandPart::Token(token) => {
                        out.insert(token.value.clone());
                    }
                    ast::CommandPart::Splice(s) => collect_expr_strings(&s.expr, out),
                }
            }
        }
        ast::Expr::Scoped(_)
        | ast::Expr::Identifier(_)
        | ast::Expr::Path(_)
        | ast::Expr::Number(_)
        | ast::Expr::Bool(_) => {}
    }
}

fn collect_expr_templates(expr: &ast::Expr, out: &mut BTreeSet<String>) {
    match expr {
        ast::Expr::Template(t) => {
            out.insert(t.value.clone());
        }
        ast::Expr::Binary(b) => {
            collect_expr_templates(&b.left, out);
            collect_expr_templates(&b.right, out);
        }
        ast::Expr::Unary(u) => collect_expr_templates(&u.operand, out),
        ast::Expr::Call(c) => {
            for arg in &c.args.args {
                match arg {
                    ast::Arg::Expr(e) => collect_expr_templates(e, out),
                    ast::Arg::Kwarg(k) => collect_expr_templates(&k.value, out),
                    ast::Arg::Partial(_) => {}
                }
            }
        }
        ast::Expr::MethodCall(m) => {
            collect_expr_templates(&m.receiver, out);
            for arg in &m.args.args {
                match arg {
                    ast::Arg::Expr(e) => collect_expr_templates(e, out),
                    ast::Arg::Kwarg(k) => collect_expr_templates(&k.value, out),
                    ast::Arg::Partial(_) => {}
                }
            }
        }
        ast::Expr::Match(m) => {
            collect_expr_templates(&m.scrutinee, out);
            for arm in &m.arms {
                if let Some(guard) = &arm.guard {
                    collect_expr_templates(guard, out);
                }
                collect_expr_templates(&arm.value, out);
            }
        }
        ast::Expr::StructLit(lit) => {
            for field in &lit.fields {
                collect_expr_templates(&field.value, out);
            }
            for spread in &lit.spreads {
                if let Some(base) = &spread.base {
                    collect_expr_templates(base, out);
                }
            }
        }
        ast::Expr::Paren(p) => collect_expr_templates(&p.inner, out),
        ast::Expr::Field(f) => collect_expr_templates(&f.receiver, out),
        ast::Expr::Tuple(t) => {
            for elem in &t.elems {
                collect_expr_templates(elem, out);
            }
        }
        ast::Expr::Array(a) => {
            for elem in &a.elems {
                if let ast::ArrayElem::Expr(e) = elem {
                    collect_expr_templates(e, out);
                }
            }
        }
        ast::Expr::Map(m) => {
            for entry in &m.entries {
                collect_expr_templates(&entry.key, out);
                collect_expr_templates(&entry.value, out);
            }
        }
        ast::Expr::Closure(c) => collect_expr_templates(&c.body, out),
        ast::Expr::Command(c) => {
            for part in &c.parts {
                if let ast::CommandPart::Splice(s) = part {
                    collect_expr_templates(&s.expr, out);
                }
            }
        }
        ast::Expr::Scoped(_)
        | ast::Expr::Identifier(_)
        | ast::Expr::Str(_)
        | ast::Expr::Path(_)
        | ast::Expr::Number(_)
        | ast::Expr::Bool(_) => {}
    }
}

fn collect_expr_flags(expr: &ast::Expr, out: &mut BTreeSet<String>) {
    match expr {
        ast::Expr::Array(a) => {
            for elem in &a.elems {
                match elem {
                    ast::ArrayElem::Flag(flag) => {
                        out.insert(flag.value.clone());
                    }
                    ast::ArrayElem::Expr(expr) => collect_expr_flags(expr, out),
                }
            }
        }
        ast::Expr::Binary(b) => {
            collect_expr_flags(&b.left, out);
            collect_expr_flags(&b.right, out);
        }
        ast::Expr::Unary(u) => collect_expr_flags(&u.operand, out),
        ast::Expr::Call(c) => {
            for arg in &c.args.args {
                match arg {
                    ast::Arg::Expr(e) => collect_expr_flags(e, out),
                    ast::Arg::Kwarg(k) => collect_expr_flags(&k.value, out),
                    ast::Arg::Partial(_) => {}
                }
            }
        }
        ast::Expr::MethodCall(m) => {
            collect_expr_flags(&m.receiver, out);
            for arg in &m.args.args {
                match arg {
                    ast::Arg::Expr(e) => collect_expr_flags(e, out),
                    ast::Arg::Kwarg(k) => collect_expr_flags(&k.value, out),
                    ast::Arg::Partial(_) => {}
                }
            }
        }
        ast::Expr::Match(m) => {
            collect_expr_flags(&m.scrutinee, out);
            for arm in &m.arms {
                if let Some(guard) = &arm.guard {
                    collect_expr_flags(guard, out);
                }
                collect_expr_flags(&arm.value, out);
            }
        }
        ast::Expr::StructLit(lit) => {
            for field in &lit.fields {
                collect_expr_flags(&field.value, out);
            }
            for spread in &lit.spreads {
                if let Some(base) = &spread.base {
                    collect_expr_flags(base, out);
                }
            }
        }
        ast::Expr::Paren(p) => collect_expr_flags(&p.inner, out),
        ast::Expr::Field(f) => collect_expr_flags(&f.receiver, out),
        ast::Expr::Tuple(t) => {
            for elem in &t.elems {
                collect_expr_flags(elem, out);
            }
        }
        ast::Expr::Map(m) => {
            for entry in &m.entries {
                collect_expr_flags(&entry.key, out);
                collect_expr_flags(&entry.value, out);
            }
        }
        ast::Expr::Closure(c) => collect_expr_flags(&c.body, out),
        ast::Expr::Command(c) => {
            for part in &c.parts {
                if let ast::CommandPart::Splice(s) = part {
                    collect_expr_flags(&s.expr, out);
                }
            }
        }
        ast::Expr::Scoped(_)
        | ast::Expr::Identifier(_)
        | ast::Expr::Template(_)
        | ast::Expr::Str(_)
        | ast::Expr::Path(_)
        | ast::Expr::Number(_)
        | ast::Expr::Bool(_) => {}
    }
}

fn collect_pattern_strings(pattern: &ast::Pattern, out: &mut BTreeSet<String>) {
    match pattern {
        ast::Pattern::Str(s) => {
            out.insert(s.value.clone());
        }
        ast::Pattern::Variant(v) => {
            for arg in &v.args {
                collect_pattern_strings(arg, out);
            }
        }
        ast::Pattern::Struct(s) => {
            for field in &s.fields {
                if let Some(pattern) = &field.pattern {
                    collect_pattern_strings(pattern, out);
                }
            }
        }
        ast::Pattern::Tuple(t) => {
            for elem in &t.elems {
                collect_pattern_strings(elem, out);
            }
        }
        ast::Pattern::Wildcard(_)
        | ast::Pattern::Scoped(_)
        | ast::Pattern::Identifier(_)
        | ast::Pattern::Number(_)
        | ast::Pattern::Bool(_) => {}
    }
}

fn collect_expr_identifiers(expr: &ast::Expr, out: &mut BTreeSet<String>) {
    match expr {
        ast::Expr::Identifier(name) => {
            out.insert(name.value.clone());
        }
        ast::Expr::Binary(b) => {
            collect_expr_identifiers(&b.left, out);
            collect_expr_identifiers(&b.right, out);
        }
        ast::Expr::Unary(u) => collect_expr_identifiers(&u.operand, out),
        ast::Expr::Call(c) => {
            for arg in &c.args.args {
                match arg {
                    ast::Arg::Expr(e) => collect_expr_identifiers(e, out),
                    ast::Arg::Kwarg(k) => collect_expr_identifiers(&k.value, out),
                    ast::Arg::Partial(_) => {}
                }
            }
        }
        ast::Expr::MethodCall(m) => {
            collect_expr_identifiers(&m.receiver, out);
            for arg in &m.args.args {
                match arg {
                    ast::Arg::Expr(e) => collect_expr_identifiers(e, out),
                    ast::Arg::Kwarg(k) => collect_expr_identifiers(&k.value, out),
                    ast::Arg::Partial(_) => {}
                }
            }
        }
        ast::Expr::Field(f) => collect_expr_identifiers(&f.receiver, out),
        ast::Expr::Match(m) => {
            collect_expr_identifiers(&m.scrutinee, out);
            for arm in &m.arms {
                let mut local = BTreeSet::new();
                collect_expr_identifiers(&arm.value, &mut local);
                if let Some(guard) = &arm.guard {
                    collect_expr_identifiers(guard, &mut local);
                }
                let mut bound = BTreeSet::new();
                collect_pattern_bindings(&arm.pattern, &mut bound);
                for name in local {
                    if !bound.contains(&name) {
                        out.insert(name);
                    }
                }
            }
        }
        ast::Expr::StructLit(lit) => {
            for field in &lit.fields {
                collect_expr_identifiers(&field.value, out);
            }
            for base in &lit.spreads {
                if let Some(base) = &base.base {
                    collect_expr_identifiers(base, out);
                }
            }
        }
        ast::Expr::Map(m) => {
            for entry in &m.entries {
                collect_expr_identifiers(&entry.key, out);
                collect_expr_identifiers(&entry.value, out);
            }
        }
        ast::Expr::Tuple(t) => {
            for elem in &t.elems {
                collect_expr_identifiers(elem, out);
            }
        }
        ast::Expr::Array(a) => {
            for elem in &a.elems {
                if let ast::ArrayElem::Expr(expr) = elem {
                    collect_expr_identifiers(expr, out);
                }
            }
        }
        ast::Expr::Paren(p) => collect_expr_identifiers(&p.inner, out),
        ast::Expr::Closure(c) => collect_expr_identifiers(&c.body, out),
        ast::Expr::Command(c) => {
            for part in &c.parts {
                if let ast::CommandPart::Splice(s) = part {
                    collect_expr_identifiers(&s.expr, out);
                }
            }
        }
        ast::Expr::Scoped(_)
        | ast::Expr::Template(_)
        | ast::Expr::Str(_)
        | ast::Expr::Path(_)
        | ast::Expr::Number(_)
        | ast::Expr::Bool(_) => {}
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum TailPosition {
    Tail,
    NonTail,
}

fn tail_self_call_diagnostics(
    item: &ast::FnItem,
    tables: &ModuleTables,
    current_module: &str,
    fn_name: &str,
) -> Vec<String> {
    let mut out = Vec::new();
    collect_tail_self_calls_in_block(&item.body, tables, current_module, fn_name, &mut out);
    out
}

fn collect_tail_self_calls_in_block(
    block: &ast::Block,
    tables: &ModuleTables,
    current_module: &str,
    fn_name: &str,
    out: &mut Vec<String>,
) {
    for stmt in &block.stmts {
        match stmt {
            ast::Stmt::Let(stmt) => collect_tail_self_calls_expr(
                &stmt.value,
                TailPosition::NonTail,
                tables,
                current_module,
                fn_name,
                out,
            ),
            ast::Stmt::Expr(stmt) => collect_tail_self_calls_expr(
                &stmt.expr,
                TailPosition::NonTail,
                tables,
                current_module,
                fn_name,
                out,
            ),
        }
    }
    if let Some(tail) = &block.tail {
        collect_tail_self_calls_expr(
            tail,
            TailPosition::Tail,
            tables,
            current_module,
            fn_name,
            out,
        );
    }
}

fn collect_tail_self_calls_expr(
    expr: &ast::Expr,
    position: TailPosition,
    tables: &ModuleTables,
    current_module: &str,
    fn_name: &str,
    out: &mut Vec<String>,
) {
    match expr {
        ast::Expr::Call(call) => {
            if position == TailPosition::NonTail
                && call_resolves_to(call, tables, current_module, fn_name)
            {
                let span = expr_span(expr);
                out.push(format!(
                    "self-call at {}..{} is a demand boundary (not tail position) - not looped",
                    span.start, span.end
                ));
            }
            collect_tail_self_calls_args(&call.args, tables, current_module, fn_name, out);
        }
        ast::Expr::Match(m) => {
            collect_tail_self_calls_expr(
                &m.scrutinee,
                TailPosition::NonTail,
                tables,
                current_module,
                fn_name,
                out,
            );
            for arm in &m.arms {
                if let Some(guard) = &arm.guard {
                    collect_tail_self_calls_expr(
                        guard,
                        TailPosition::NonTail,
                        tables,
                        current_module,
                        fn_name,
                        out,
                    );
                }
                collect_tail_self_calls_expr(
                    &arm.value,
                    position,
                    tables,
                    current_module,
                    fn_name,
                    out,
                );
            }
        }
        ast::Expr::Paren(paren) => collect_tail_self_calls_expr(
            &paren.inner,
            position,
            tables,
            current_module,
            fn_name,
            out,
        ),
        ast::Expr::Binary(binary) => {
            collect_tail_self_calls_expr(
                &binary.left,
                TailPosition::NonTail,
                tables,
                current_module,
                fn_name,
                out,
            );
            collect_tail_self_calls_expr(
                &binary.right,
                TailPosition::NonTail,
                tables,
                current_module,
                fn_name,
                out,
            );
        }
        ast::Expr::Unary(unary) => collect_tail_self_calls_expr(
            &unary.operand,
            TailPosition::NonTail,
            tables,
            current_module,
            fn_name,
            out,
        ),
        ast::Expr::MethodCall(call) => {
            collect_tail_self_calls_expr(
                &call.receiver,
                TailPosition::NonTail,
                tables,
                current_module,
                fn_name,
                out,
            );
            collect_tail_self_calls_args(&call.args, tables, current_module, fn_name, out);
        }
        ast::Expr::Field(field) => collect_tail_self_calls_expr(
            &field.receiver,
            TailPosition::NonTail,
            tables,
            current_module,
            fn_name,
            out,
        ),
        ast::Expr::StructLit(lit) => {
            for field in &lit.fields {
                collect_tail_self_calls_expr(
                    &field.value,
                    TailPosition::NonTail,
                    tables,
                    current_module,
                    fn_name,
                    out,
                );
            }
            for spread in &lit.spreads {
                if let Some(base) = &spread.base {
                    collect_tail_self_calls_expr(
                        base,
                        TailPosition::NonTail,
                        tables,
                        current_module,
                        fn_name,
                        out,
                    );
                }
            }
        }
        ast::Expr::Map(map) => {
            for entry in &map.entries {
                collect_tail_self_calls_expr(
                    &entry.key,
                    TailPosition::NonTail,
                    tables,
                    current_module,
                    fn_name,
                    out,
                );
                collect_tail_self_calls_expr(
                    &entry.value,
                    TailPosition::NonTail,
                    tables,
                    current_module,
                    fn_name,
                    out,
                );
            }
        }
        ast::Expr::Tuple(tuple) => {
            for elem in &tuple.elems {
                collect_tail_self_calls_expr(
                    elem,
                    TailPosition::NonTail,
                    tables,
                    current_module,
                    fn_name,
                    out,
                );
            }
        }
        ast::Expr::Array(array) => {
            for elem in &array.elems {
                if let ast::ArrayElem::Expr(expr) = elem {
                    collect_tail_self_calls_expr(
                        expr,
                        TailPosition::NonTail,
                        tables,
                        current_module,
                        fn_name,
                        out,
                    );
                }
            }
        }
        ast::Expr::Closure(closure) => collect_tail_self_calls_expr(
            &closure.body,
            TailPosition::NonTail,
            tables,
            current_module,
            fn_name,
            out,
        ),
        ast::Expr::Command(command) => {
            for part in &command.parts {
                if let ast::CommandPart::Splice(splice) = part {
                    collect_tail_self_calls_expr(
                        &splice.expr,
                        TailPosition::NonTail,
                        tables,
                        current_module,
                        fn_name,
                        out,
                    );
                }
            }
        }
        ast::Expr::Scoped(_)
        | ast::Expr::Identifier(_)
        | ast::Expr::Template(_)
        | ast::Expr::Str(_)
        | ast::Expr::Path(_)
        | ast::Expr::Number(_)
        | ast::Expr::Bool(_) => {}
    }
}

fn collect_tail_self_calls_args(
    args: &ast::ArgList,
    tables: &ModuleTables,
    current_module: &str,
    fn_name: &str,
    out: &mut Vec<String>,
) {
    for arg in &args.args {
        match arg {
            ast::Arg::Expr(expr) => collect_tail_self_calls_expr(
                expr,
                TailPosition::NonTail,
                tables,
                current_module,
                fn_name,
                out,
            ),
            ast::Arg::Kwarg(kwarg) => collect_tail_self_calls_expr(
                &kwarg.value,
                TailPosition::NonTail,
                tables,
                current_module,
                fn_name,
                out,
            ),
            ast::Arg::Partial(_) => {}
        }
    }
}

fn call_resolves_to(
    call: &ast::Call,
    tables: &ModuleTables,
    current_module: &str,
    fn_name: &str,
) -> bool {
    let ast::PathRef::Identifier(name) = &call.callee else {
        return false;
    };
    tables.resolve_fn(current_module, &name.value) == Some(fn_name)
}

fn expr_span(expr: &ast::Expr) -> ast::Span {
    match expr {
        ast::Expr::Binary(expr) => expr.span,
        ast::Expr::Unary(expr) => expr.span,
        ast::Expr::Call(expr) => expr.span,
        ast::Expr::MethodCall(expr) => expr.span,
        ast::Expr::Field(expr) => expr.span,
        ast::Expr::Match(expr) => expr.span,
        ast::Expr::Closure(expr) => expr.span,
        ast::Expr::Command(expr) => expr.span,
        ast::Expr::StructLit(expr) => expr.span,
        ast::Expr::Map(expr) => expr.span,
        ast::Expr::Tuple(expr) => expr.span,
        ast::Expr::Array(expr) => expr.span,
        ast::Expr::Paren(expr) => expr.span,
        ast::Expr::Scoped(expr) => expr.span,
        ast::Expr::Identifier(expr)
        | ast::Expr::Template(expr)
        | ast::Expr::Str(expr)
        | ast::Expr::Path(expr)
        | ast::Expr::Number(expr) => expr.span,
        ast::Expr::Bool(expr) => expr.span,
    }
}

fn consuming_rebind_receiver(binding_name: &str, value: &ast::Expr) -> Option<String> {
    consuming_update_receiver(value)
        .filter(|receiver| *receiver == binding_name)
        .map(str::to_string)
}

fn consuming_update_receiver(value: &ast::Expr) -> Option<&str> {
    match unparen_expr(value) {
        ast::Expr::MethodCall(call) if aggregate_update_method(call.name.value.as_str()) => {
            plain_identifier_expr(&call.receiver)
        }
        ast::Expr::Field(field) if matches!(&field.name, ast::Member::Index(index) if index.value == "1") =>
        {
            let receiver = unparen_expr(&field.receiver);
            if let ast::Expr::MethodCall(call) = receiver
                && call.name.value == "pop"
            {
                return plain_identifier_expr(&call.receiver);
            }
            None
        }
        _ => None,
    }
}

fn aggregate_update_method(name: &str) -> bool {
    matches!(name, "push" | "pop" | "set" | "insert")
}

fn plain_identifier_expr(expr: &ast::Expr) -> Option<&str> {
    match unparen_expr(expr) {
        ast::Expr::Identifier(name) => Some(name.value.as_str()),
        _ => None,
    }
}

fn identifier_uses_in_arg_list(args: &ast::ArgList) -> HashMap<String, usize> {
    let mut uses = HashMap::new();
    count_identifier_uses_in_args(args, &mut uses);
    uses
}

fn count_identifier_uses_in_args(args: &ast::ArgList, uses: &mut HashMap<String, usize>) {
    for arg in &args.args {
        match arg {
            ast::Arg::Expr(expr) => count_identifier_uses_in_expr(expr, uses),
            ast::Arg::Kwarg(kwarg) => count_identifier_uses_in_expr(&kwarg.value, uses),
            ast::Arg::Partial(_) => {}
        }
    }
}

fn count_identifier_uses_in_expr(expr: &ast::Expr, uses: &mut HashMap<String, usize>) {
    match expr {
        ast::Expr::Identifier(name) => {
            *uses.entry(name.value.clone()).or_default() += 1;
        }
        ast::Expr::Paren(paren) => count_identifier_uses_in_expr(&paren.inner, uses),
        ast::Expr::Binary(binary) => {
            count_identifier_uses_in_expr(&binary.left, uses);
            count_identifier_uses_in_expr(&binary.right, uses);
        }
        ast::Expr::Unary(unary) => count_identifier_uses_in_expr(&unary.operand, uses),
        ast::Expr::Call(call) => count_identifier_uses_in_args(&call.args, uses),
        ast::Expr::MethodCall(call) => {
            count_identifier_uses_in_expr(&call.receiver, uses);
            count_identifier_uses_in_args(&call.args, uses);
        }
        ast::Expr::Field(field) => count_identifier_uses_in_expr(&field.receiver, uses),
        ast::Expr::Match(m) => {
            count_identifier_uses_in_expr(&m.scrutinee, uses);
            for arm in &m.arms {
                if let Some(guard) = &arm.guard {
                    count_identifier_uses_in_expr(guard, uses);
                }
                count_identifier_uses_in_expr(&arm.value, uses);
            }
        }
        ast::Expr::StructLit(lit) => {
            for field in &lit.fields {
                count_identifier_uses_in_expr(&field.value, uses);
            }
            for spread in &lit.spreads {
                if let Some(base) = &spread.base {
                    count_identifier_uses_in_expr(base, uses);
                }
            }
        }
        ast::Expr::Map(map) => {
            for entry in &map.entries {
                count_identifier_uses_in_expr(&entry.key, uses);
                count_identifier_uses_in_expr(&entry.value, uses);
            }
        }
        ast::Expr::Tuple(tuple) => {
            for elem in &tuple.elems {
                count_identifier_uses_in_expr(elem, uses);
            }
        }
        ast::Expr::Array(array) => {
            for elem in &array.elems {
                if let ast::ArrayElem::Expr(expr) = elem {
                    count_identifier_uses_in_expr(expr, uses);
                }
            }
        }
        ast::Expr::Closure(closure) => count_identifier_uses_in_expr(&closure.body, uses),
        ast::Expr::Command(command) => {
            for part in &command.parts {
                if let ast::CommandPart::Splice(splice) = part {
                    count_identifier_uses_in_expr(&splice.expr, uses);
                }
            }
        }
        ast::Expr::Scoped(_)
        | ast::Expr::Template(_)
        | ast::Expr::Str(_)
        | ast::Expr::Path(_)
        | ast::Expr::Number(_)
        | ast::Expr::Bool(_) => {}
    }
}

fn collect_consuming_update_receivers_in_expr(expr: &ast::Expr, out: &mut BTreeSet<String>) {
    if let Some(receiver) = consuming_update_receiver(expr) {
        out.insert(receiver.to_string());
    }
    match expr {
        ast::Expr::Identifier(_)
        | ast::Expr::Scoped(_)
        | ast::Expr::Template(_)
        | ast::Expr::Str(_)
        | ast::Expr::Path(_)
        | ast::Expr::Number(_)
        | ast::Expr::Bool(_) => {}
        ast::Expr::Paren(paren) => collect_consuming_update_receivers_in_expr(&paren.inner, out),
        ast::Expr::Binary(binary) => {
            collect_consuming_update_receivers_in_expr(&binary.left, out);
            collect_consuming_update_receivers_in_expr(&binary.right, out);
        }
        ast::Expr::Unary(unary) => collect_consuming_update_receivers_in_expr(&unary.operand, out),
        ast::Expr::Call(call) => {
            for arg in &call.args.args {
                match arg {
                    ast::Arg::Expr(expr) => collect_consuming_update_receivers_in_expr(expr, out),
                    ast::Arg::Kwarg(kwarg) => {
                        collect_consuming_update_receivers_in_expr(&kwarg.value, out)
                    }
                    ast::Arg::Partial(_) => {}
                }
            }
        }
        ast::Expr::MethodCall(call) => {
            collect_consuming_update_receivers_in_expr(&call.receiver, out);
            for arg in &call.args.args {
                match arg {
                    ast::Arg::Expr(expr) => collect_consuming_update_receivers_in_expr(expr, out),
                    ast::Arg::Kwarg(kwarg) => {
                        collect_consuming_update_receivers_in_expr(&kwarg.value, out)
                    }
                    ast::Arg::Partial(_) => {}
                }
            }
        }
        ast::Expr::Field(field) => collect_consuming_update_receivers_in_expr(&field.receiver, out),
        ast::Expr::Match(m) => {
            collect_consuming_update_receivers_in_expr(&m.scrutinee, out);
            for arm in &m.arms {
                if let Some(guard) = &arm.guard {
                    collect_consuming_update_receivers_in_expr(guard, out);
                }
                collect_consuming_update_receivers_in_expr(&arm.value, out);
            }
        }
        ast::Expr::StructLit(lit) => {
            for field in &lit.fields {
                collect_consuming_update_receivers_in_expr(&field.value, out);
            }
            for spread in &lit.spreads {
                if let Some(base) = &spread.base {
                    collect_consuming_update_receivers_in_expr(base, out);
                }
            }
        }
        ast::Expr::Map(map) => {
            for entry in &map.entries {
                collect_consuming_update_receivers_in_expr(&entry.key, out);
                collect_consuming_update_receivers_in_expr(&entry.value, out);
            }
        }
        ast::Expr::Tuple(tuple) => {
            for elem in &tuple.elems {
                collect_consuming_update_receivers_in_expr(elem, out);
            }
        }
        ast::Expr::Array(array) => {
            for elem in &array.elems {
                if let ast::ArrayElem::Expr(expr) = elem {
                    collect_consuming_update_receivers_in_expr(expr, out);
                }
            }
        }
        ast::Expr::Closure(closure) => {
            collect_consuming_update_receivers_in_expr(&closure.body, out)
        }
        ast::Expr::Command(command) => {
            for part in &command.parts {
                if let ast::CommandPart::Splice(splice) = part {
                    collect_consuming_update_receivers_in_expr(&splice.expr, out);
                }
            }
        }
    }
}

fn arg_list_has_partial(args: &ast::ArgList) -> bool {
    args.args
        .iter()
        .any(|arg| matches!(arg, ast::Arg::Partial(_)))
}

fn unparen_expr(mut expr: &ast::Expr) -> &ast::Expr {
    while let ast::Expr::Paren(paren) = expr {
        expr = &paren.inner;
    }
    expr
}

fn collect_pattern_bindings(pattern: &ast::Pattern, out: &mut BTreeSet<String>) {
    match pattern {
        ast::Pattern::Identifier(name) => {
            out.insert(name.value.clone());
        }
        ast::Pattern::Variant(v) => {
            for arg in &v.args {
                collect_pattern_bindings(arg, out);
            }
        }
        ast::Pattern::Struct(s) => {
            for field in &s.fields {
                if let Some(pattern) = &field.pattern {
                    collect_pattern_bindings(pattern, out);
                } else {
                    out.insert(field.name.value.clone());
                }
            }
        }
        ast::Pattern::Tuple(t) => {
            for elem in &t.elems {
                collect_pattern_bindings(elem, out);
            }
        }
        ast::Pattern::Wildcard(_)
        | ast::Pattern::Scoped(_)
        | ast::Pattern::Str(_)
        | ast::Pattern::Number(_)
        | ast::Pattern::Bool(_) => {}
    }
}

fn collect_block_type_schemas(block: &ast::Block, out: &mut Vec<String>) -> Result<(), String> {
    for stmt in &block.stmts {
        match stmt {
            ast::Stmt::Let(l) => {
                if let Some(ty) = &l.ty {
                    collect_type_schema(ty, out)?;
                }
            }
            ast::Stmt::Expr(_) => {}
        }
    }
    Ok(())
}

fn collect_expr_schemas_in_block(
    block: &ast::Block,
    out: &mut Vec<String>,
    tables: &ModuleTables,
    fn_returns: &HashMap<String, String>,
) -> Result<(), String> {
    let mut env = HashMap::new();
    for stmt in &block.stmts {
        if let ast::Stmt::Let(l) = stmt {
            let annotated = l.ty.as_ref().map(type_schema_name).transpose()?;
            let inferred = collect_expr_schemas(&l.value, out, tables, fn_returns, &env)?;
            if let Some(schema) = annotated.or(inferred) {
                env.insert(l.name.value.clone(), schema);
            }
        }
    }
    if let Some(tail) = &block.tail {
        collect_expr_schemas(tail, out, tables, fn_returns, &env)?;
    }
    Ok(())
}

fn collect_expr_schemas(
    expr: &ast::Expr,
    out: &mut Vec<String>,
    tables: &ModuleTables,
    fn_returns: &HashMap<String, String>,
    env: &HashMap<String, String>,
) -> Result<Option<String>, String> {
    match expr {
        ast::Expr::Number(n) => Ok(Some(if n.value.contains('.') {
            "Float".into()
        } else {
            "Int".into()
        })),
        ast::Expr::Bool(_) => Ok(Some("Bool".into())),
        ast::Expr::Str(_) => Ok(Some("String".into())),
        ast::Expr::Template(_) => Ok(Some("Template".into())),
        ast::Expr::Path(_) => Ok(Some("Path".into())),
        ast::Expr::Tuple(tuple) => {
            let mut fields = Vec::new();
            for elem in &tuple.elems {
                if let Some(schema) = collect_expr_schemas(elem, out, tables, fn_returns, env)? {
                    fields.push(schema);
                }
            }
            let schema = tuple_schema(&fields);
            out.push(schema.clone());
            Ok(Some(schema))
        }
        ast::Expr::Binary(binary) => {
            let left = collect_expr_schemas(&binary.left, out, tables, fn_returns, env)?;
            let right = collect_expr_schemas(&binary.right, out, tables, fn_returns, env)?;
            if binary.op == "+"
                && left.as_deref() == Some("String")
                && right.as_deref() == Some("String")
            {
                Ok(Some("String".into()))
            } else {
                Ok(None)
            }
        }
        ast::Expr::Unary(unary) => {
            collect_expr_schemas(&unary.operand, out, tables, fn_returns, env)
        }
        ast::Expr::Call(call) => {
            for arg in &call.args.args {
                if let ast::Arg::Expr(expr) = arg {
                    let _ = collect_expr_schemas(expr, out, tables, fn_returns, env)?;
                } else if let ast::Arg::Kwarg(kwarg) = arg {
                    let _ = collect_expr_schemas(&kwarg.value, out, tables, fn_returns, env)?;
                }
            }
            if let ast::PathRef::Identifier(name) = &call.callee
                && let Some(schema) = fn_returns.get(&name.value)
            {
                return Ok(Some(schema.clone()));
            }
            if let ast::PathRef::Identifier(name) = &call.callee
                && name.value == "version"
            {
                out.push("Version".to_string());
                return Ok(Some("Version".to_string()));
            }
            if let ast::PathRef::Scoped(path) = &call.callee {
                let segments: Vec<&str> = path.segments.iter().map(|s| s.value.as_str()).collect();
                if segments.as_slice() == ["VersionSet", "from_req"] {
                    out.push("VersionSet".to_string());
                    return Ok(Some("VersionSet".to_string()));
                }
            }
            if let ast::PathRef::Identifier(name) = &call.callee
                && name.value == "render"
            {
                return Ok(Some("String".into()));
            }
            if let Ok((enum_name, _, _)) = resolve_path_variant_for_collect(tables, &call.callee) {
                return Ok(Some(enum_name));
            }
            Ok(None)
        }
        ast::Expr::MethodCall(call) => {
            let receiver_schema =
                collect_expr_schemas(&call.receiver, out, tables, fn_returns, env)?;
            for arg in &call.args.args {
                if let ast::Arg::Expr(expr) = arg {
                    let _ = collect_expr_schemas(expr, out, tables, fn_returns, env)?;
                } else if let ast::Arg::Kwarg(kwarg) = arg {
                    let _ = collect_expr_schemas(&kwarg.value, out, tables, fn_returns, env)?;
                }
            }
            match (call.name.value.as_str(), receiver_schema.as_deref()) {
                ("with_ext", Some("Path")) => Ok(Some("Path".into())),
                ("glob", Some("Tree")) => Ok(Some(array_schema("Path"))),
                ("text", Some("Tree")) => Ok(Some("String".into())),
                ("insert", Some(schema)) if map_schemas(schema).is_some() => {
                    Ok(Some(schema.to_string()))
                }
                ("len", Some(schema)) if tables.schemas.is_list(schema) => Ok(Some("Int".into())),
                ("push" | "set" | "filter", Some(schema)) if tables.schemas.is_list(schema) => {
                    Ok(Some(schema.to_string()))
                }
                ("pop", Some(schema)) if tables.schemas.is_list(schema) => {
                    let elem_schema = array_element_schema(schema)
                        .ok_or_else(|| format!("{schema} is not an Array<T>"))?
                        .to_string();
                    let tuple = tuple_schema(&[elem_schema, schema.to_string()]);
                    push_schema_closure(&tuple, out);
                    Ok(Some(tuple))
                }
                ("get", Some("Doc")) | ("get", Some("Realized<Doc>")) => {
                    Ok(Some(option_schema("Realized<Doc>")))
                }
                ("keys", Some("Doc")) | ("keys", Some("Realized<Doc>")) => {
                    Ok(Some(array_schema("String")))
                }
                ("package", Some("Doc")) | ("package", Some("Realized<Doc>")) => {
                    Ok(Some(option_schema("Realized<Doc>")))
                }
                ("get", Some(schema)) => Ok(map_value_schema(schema).map(option_schema)),
                ("unwrap", Some(schema)) => Ok(option_value_schema(schema).map(str::to_string)),
                ("join", Some(schema))
                    if tables.schemas.is_list(schema)
                        || matches!(schema, "Doc" | "Realized<Doc>") =>
                {
                    Ok(Some("String".into()))
                }
                ("union" | "intersect" | "complement", Some("VersionSet")) => {
                    Ok(Some("VersionSet".into()))
                }
                ("subset" | "contains", Some("VersionSet")) => Ok(Some("Bool".into())),
                ("before" | "after" | "strip_prefix", Some("String")) => Ok(Some("String".into())),
                ("parse_int", Some("String")) => Ok(Some("Int".into())),
                ("contains" | "is_numeric", Some("String")) => Ok(Some("Bool".into())),
                _ => Ok(None),
            }
        }
        ast::Expr::Field(field) => {
            let receiver = collect_expr_schemas(&field.receiver, out, tables, fn_returns, env)?;
            if let Some(schema) = &receiver
                && let ast::Member::Index(index) = &field.name
                && let Some(fields) = tuple_schema_fields(schema)
                && let Ok(field_index) = index.value.parse::<usize>()
            {
                return Ok(fields.get(field_index).cloned());
            }
            Ok(None)
        }
        ast::Expr::Match(m) => {
            let _ = collect_expr_schemas(&m.scrutinee, out, tables, fn_returns, env)?;
            let mut result_schema: Option<String> = None;
            for arm in &m.arms {
                if let Some(guard) = &arm.guard {
                    let _ = collect_expr_schemas(guard, out, tables, fn_returns, env)?;
                }
                let arm_schema = collect_expr_schemas(&arm.value, out, tables, fn_returns, env)?;
                match (&result_schema, arm_schema) {
                    (None, Some(schema)) => result_schema = Some(schema),
                    (Some(expected), Some(schema)) if expected == &schema => {}
                    _ => result_schema = None,
                }
            }
            Ok(result_schema)
        }
        ast::Expr::StructLit(lit) => {
            for field in &lit.fields {
                let _ = collect_expr_schemas(&field.value, out, tables, fn_returns, env)?;
            }
            for spread in &lit.spreads {
                if let Some(base) = &spread.base {
                    let _ = collect_expr_schemas(base, out, tables, fn_returns, env)?;
                }
            }
            let segments = path_ref_segments(&lit.path)?;
            if segments.len() == 1 {
                return Ok(Some(segments[0].clone()));
            }
            if let Ok((enum_name, _, _)) = resolve_variant_segments(tables, &segments) {
                return Ok(Some(enum_name));
            }
            Ok(None)
        }
        ast::Expr::Map(map) => {
            let mut key_schema = None;
            let mut value_schema = None;
            for entry in &map.entries {
                key_schema = key_schema.or(collect_expr_schemas(
                    &entry.key, out, tables, fn_returns, env,
                )?);
                value_schema = value_schema.or(collect_expr_schemas(
                    &entry.value,
                    out,
                    tables,
                    fn_returns,
                    env,
                )?);
            }
            if let (Some(key), Some(value)) = (key_schema, value_schema) {
                let schema = map_schema(&key, &value);
                push_schema_closure(&schema, out);
                Ok(Some(schema))
            } else {
                Ok(None)
            }
        }
        ast::Expr::Array(array) => {
            let mut elem_schema = None::<String>;
            for elem in &array.elems {
                if let ast::ArrayElem::Expr(expr) = elem {
                    elem_schema =
                        elem_schema.or(collect_expr_schemas(expr, out, tables, fn_returns, env)?);
                }
            }
            if let Some(elem_schema) = elem_schema {
                let schema = array_schema(&elem_schema);
                push_schema_closure(&schema, out);
                Ok(Some(schema))
            } else {
                Ok(None)
            }
        }
        ast::Expr::Paren(paren) => collect_expr_schemas(&paren.inner, out, tables, fn_returns, env),
        ast::Expr::Closure(closure) => {
            collect_expr_schemas(&closure.body, out, tables, fn_returns, env)
        }
        ast::Expr::Command(command) => {
            for part in &command.parts {
                if let ast::CommandPart::Splice(splice) = part {
                    let _ = collect_expr_schemas(&splice.expr, out, tables, fn_returns, env)?;
                }
            }
            Ok(None)
        }
        ast::Expr::Identifier(name) => Ok(env.get(&name.value).cloned().or_else(|| {
            tables
                .structs
                .get(&name.value)
                .and_then(|info| info.is_unit.then(|| name.value.clone()))
        })),
        ast::Expr::Scoped(path) => {
            let (enum_name, _, _) = resolve_variant_segments(
                tables,
                &path
                    .segments
                    .iter()
                    .map(|segment| segment.value.clone())
                    .collect::<Vec<_>>(),
            )?;
            Ok(Some(enum_name))
        }
    }
}

fn resolve_path_variant_for_collect(
    tables: &ModuleTables,
    path: &ast::PathRef,
) -> Result<(String, usize, VariantShape), String> {
    let segments = path_ref_segments(path)?;
    resolve_variant_segments(tables, &segments)
}

fn collect_type_schema(ty: &ast::Type, out: &mut Vec<String>) -> Result<(), String> {
    let schema = type_schema_name(ty)?;
    out.push(schema.clone());
    out.push(pending_schema(&schema));
    if let Some(value_schema) = map_value_schema(&schema) {
        let pending = pending_schema(value_schema);
        let realized = realized_schema(value_schema);
        out.push(value_schema.to_string());
        out.push(pending.clone());
        out.push(option_schema(value_schema));
        out.push(realized.clone());
        out.push(option_schema(&realized));
        if let Some((key_schema, _)) = map_schemas(&schema) {
            out.push(map_schema(key_schema, &realized));
        }
    }
    if let ast::Type::Generic(generic) = ty {
        for arg in &generic.args {
            collect_type_schema(arg, out)?;
        }
    }
    Ok(())
}

fn add_builtin_descriptors(descriptors: &mut DescriptorMap, schemas: &SchemaTables) {
    descriptors.insert_named_if_absent(schemas, "Bool", || {
        declared_mem::i64_(schemas.legacy_ref("Bool"))
    });
    descriptors.insert_named_if_absent(schemas, "Sealed", || {
        declared_mem::handle(
            schemas.legacy_ref("SealedRef"),
            schemas.legacy_ref("Sealed"),
        )
    });
    descriptors.insert_named_if_absent(schemas, "Target", || {
        declared_mem::declared_struct(
            schemas.legacy_ref("Target"),
            vec![
                declared_mem::handle(schemas.legacy_ref("OsRef"), schemas.legacy_ref("Os")),
                declared_mem::handle(schemas.legacy_ref("ArchRef"), schemas.legacy_ref("Arch")),
            ],
        )
    });
    descriptors.insert_named_if_absent(schemas, "Os", || {
        declared_mem::declared_enum(schemas.legacy_ref("Os"), vec![vec![], vec![], vec![]])
    });
    descriptors.insert_named_if_absent(schemas, "Arch", || {
        declared_mem::declared_enum(
            schemas.legacy_ref("Arch"),
            vec![vec![], vec![], vec![], vec![], vec![], vec![]],
        )
    });
    descriptors.insert_named_if_absent(schemas, "Run", || {
        declared_mem::declared_struct(
            schemas.legacy_ref("Run"),
            vec![
                declared_mem::i64_(schemas.legacy_ref("RunOk")),
                declared_mem::handle(schemas.legacy_ref("RunOut"), schemas.legacy_ref("Tree")),
            ],
        )
    });
    descriptors.insert_named_if_absent(schemas, "Arg", || {
        declared_mem::declared_enum(
            schemas.legacy_ref("Arg"),
            vec![
                vec![declared_mem::handle(
                    schemas.legacy_ref("ArgStr"),
                    schemas.legacy_ref("String"),
                )],
                vec![declared_mem::handle(
                    schemas.legacy_ref("ArgPath"),
                    schemas.legacy_ref("Path"),
                )],
                vec![
                    declared_mem::handle(
                        schemas.legacy_ref("ArgInterpolationTree"),
                        schemas.legacy_ref("Tree"),
                    ),
                    declared_mem::handle(
                        schemas.legacy_ref("ArgInterpolationSubpath"),
                        schemas.legacy_ref("Path"),
                    ),
                ],
            ],
        )
    });
    descriptors.insert_named_if_absent(schemas, "Doc", || {
        declared_mem::declared_enum(
            schemas.legacy_ref("Doc"),
            vec![
                vec![],
                vec![declared_mem::i64_(schemas.legacy_ref("DocBool"))],
                vec![declared_mem::i64_(schemas.legacy_ref("DocInt"))],
                vec![declared_mem::f64_(schemas.legacy_ref("DocFloat"))],
                vec![declared_mem::handle(
                    schemas.legacy_ref("DocString"),
                    schemas.legacy_ref("String"),
                )],
                vec![declared_mem::handle(
                    schemas.legacy_ref("DocArray"),
                    schemas.legacy_ref("Array<Doc>"),
                )],
                vec![declared_mem::handle(
                    schemas.legacy_ref("DocMap"),
                    schemas.legacy_ref("Map<String,Doc>"),
                )],
                vec![declared_mem::handle(
                    schemas.legacy_ref("DocVirtual"),
                    schemas.legacy_ref("String"),
                )],
                vec![declared_mem::handle(
                    schemas.legacy_ref("DocBlob"),
                    schemas.legacy_ref("Blob"),
                )],
            ],
        )
    });
}

fn derived_descriptor(schemas: &SchemaTables, schema: &str) -> Option<VixDescriptor> {
    if schemas.is_list(schema)
        && let Some(elem_schema) = array_element_schema(schema)
    {
        return Some(declared_mem::sequence(
            schemas.legacy_ref(schema),
            word_descriptor_for_schema(schemas, elem_schema),
        ));
    }
    if schemas.is_map(schema)
        && let Some((key_schema, value_schema)) = schemas.map_schema_names(schema)
    {
        return Some(declared_mem::map(
            schemas.legacy_ref(schema),
            word_descriptor_for_schema(schemas, &key_schema),
            word_descriptor_for_schema(schemas, &value_schema),
        ));
    }
    if schemas.is_option(schema)
        && let Some(value_schema) = schemas.option_value_schema_name(schema)
    {
        return Some(declared_mem::option(
            schemas.legacy_ref(schema),
            declared_mem::declared_struct(
                schemas.legacy_ref(&format!("{schema}::Some")),
                vec![
                    declared_mem::i64_(schemas.legacy_ref(&format!("{schema}::value_schema"))),
                    word_descriptor_for_schema(schemas, &value_schema),
                    declared_mem::i64_(schemas.legacy_ref(&format!("{schema}::realization"))),
                ],
            ),
        ));
    }
    if let Some(value_schema) = realized_value_schema(schema) {
        return Some(declared_mem::declared_struct(
            schemas.legacy_ref(schema),
            vec![
                word_descriptor_for_schema(schemas, value_schema),
                declared_mem::i64_(schemas.legacy_ref(&format!("{schema}::realization_bitset"))),
            ],
        ));
    }
    if let Some(fields) = tuple_schema_fields(schema) {
        return Some(declared_mem::declared_struct(
            schemas.legacy_ref(schema),
            fields
                .into_iter()
                .enumerate()
                .map(|(index, field)| word_descriptor_for_schema_with_name(schemas, &field, index))
                .collect(),
        ));
    }
    None
}

fn word_descriptor_for_schema(schemas: &SchemaTables, schema: &str) -> VixDescriptor {
    word_descriptor_for_schema_with_name(schemas, schema, 0)
}

fn word_descriptor_for_schema_with_name(
    schemas: &SchemaTables,
    schema: &str,
    index: usize,
) -> VixDescriptor {
    if schemas.is_primitive(schema, Primitive::I64) {
        declared_mem::i64_(schemas.legacy_ref(schema))
    } else if schemas.is_primitive(schema, Primitive::F64) {
        declared_mem::f64_(schemas.legacy_ref(schema))
    } else if schemas.is_primitive(schema, Primitive::Bool) {
        declared_mem::i64_(schemas.legacy_ref(schema))
    } else {
        declared_mem::handle(
            schemas.legacy_ref(&format!("{schema}Ref{index}")),
            schemas.legacy_ref(schema),
        )
    }
}

fn descriptor_field_schema(
    schemas: &SchemaTables,
    descriptor: &VixDescriptor,
    field_index: usize,
) -> Result<String, String> {
    let field = match &descriptor.access {
        weavy::mem::Access::Record(record) => record.fields.get(field_index),
        other => {
            return Err(format!(
                "descriptor `{}` has access {other:?}, not fields",
                schemas.display_ref(&descriptor.schema)
            ));
        }
    }
    .ok_or_else(|| {
        format!(
            "missing field {field_index} on `{}`",
            schemas.display_ref(&descriptor.schema)
        )
    })?;
    match &field.descriptor.access {
        weavy::mem::Access::Handle { target } => Ok(schemas.display_ref(target)),
        _ => Ok(schemas.display_ref(&field.descriptor.schema)),
    }
}

fn parked_generic_or_fn_typed(item: &ast::FnItem) -> bool {
    item.generics.is_some()
        || item
            .params
            .params
            .iter()
            .any(|param| type_contains_fn(&param.ty))
        || item.return_type.as_ref().is_some_and(type_contains_fn)
}

fn type_contains_fn(ty: &ast::Type) -> bool {
    match ty {
        ast::Type::Fn(_) => true,
        ast::Type::Array(array) => type_contains_fn(&array.elem),
        ast::Type::Generic(generic) => generic.args.iter().any(type_contains_fn),
        ast::Type::Tuple(tuple) => tuple.elems.iter().any(type_contains_fn),
        ast::Type::Path(_) => false,
    }
}

fn parked_stub(item: &ast::FnItem) -> Result<(TaskFn, LoweredInfo), String> {
    let mut arg_offsets = Vec::new();
    let mut next = 0u32;
    for _ in &item.params.params {
        arg_offsets.push(next);
        next += 8;
    }
    let result = next;
    next += 8;
    let code = vec![
        Op::ConstI64 {
            dst: result,
            value: 0,
        },
        Op::Ret {
            src: result,
            size: 8,
        },
    ];
    Ok((
        TaskFn {
            frame: Layout {
                size: next as usize,
                align: 8,
            },
            code,
        },
        LoweredInfo {
            arg_offsets,
            invoke_region: result,
            store_alloc_region: result,
            store_read_region: result,
            store_tag_region: result,
            primitive_region: result,
        },
    ))
}

fn semantic_comparators_for(
    fn_name: &str,
    arg_schemas: &[String],
    param_names: &[String],
    fn_refs: &HashMap<String, usize>,
    fn_params: &HashMap<String, Vec<String>>,
    fn_returns: &HashMap<String, String>,
) -> Result<Vec<SemanticComparator>, String> {
    let mut comparators = Vec::new();
    for (arg_index, (arg_name, arg_schema)) in param_names.iter().zip(arg_schemas).enumerate() {
        let comparator_name = format!("{fn_name}__memo_verify_{arg_name}");
        let Some(&fn_ref) = fn_refs.get(&comparator_name) else {
            continue;
        };
        let params = fn_params
            .get(&comparator_name)
            .ok_or_else(|| format!("missing comparator params for {comparator_name}"))?;
        if params != &[arg_schema.clone(), arg_schema.clone()] {
            return Err(format!(
                "semantic comparator `{comparator_name}` must take ({arg_schema}, {arg_schema}), got {params:?}"
            ));
        }
        let return_schema = fn_returns
            .get(&comparator_name)
            .ok_or_else(|| format!("missing comparator return for {comparator_name}"))?;
        if return_schema != "Bool" {
            return Err(format!(
                "semantic comparator `{comparator_name}` must return Bool, got {return_schema}"
            ));
        }
        comparators.push(SemanticComparator::new(arg_index, FnRef::new(fn_ref)));
    }
    Ok(comparators)
}

fn hash_with_semantic_comparators(
    base: u64,
    comparators: &[SemanticComparator],
    fn_names: &[&String],
    tables: &ModuleTables,
) -> u64 {
    if comparators.is_empty() {
        return base;
    }
    let mut hasher = blake3::Hasher::new();
    hasher.update(b"vix-semantic-comparator-hash");
    hasher.update(&base.to_le_bytes());
    for comparator in comparators {
        hasher.update(
            &u64::try_from(comparator.arg_index)
                .expect("comparator arg index fits u64")
                .to_le_bytes(),
        );
        if let Some(name) = fn_names.get(comparator.fn_ref().index()) {
            hasher.update(&tables.fn_hashes[*name].to_le_bytes());
        }
    }
    let hash = hasher.finalize();
    u64::from_le_bytes(hash.as_bytes()[..8].try_into().expect("blake3 prefix"))
}

struct LoweredInfo {
    arg_offsets: Vec<u32>,
    invoke_region: u32,
    store_alloc_region: u32,
    store_read_region: u32,
    store_tag_region: u32,
    primitive_region: u32,
}

#[derive(Clone, Copy)]
struct LiteralHandles<'a> {
    strings: &'a HashMap<String, i64>,
    paths: &'a HashMap<String, i64>,
    flags: &'a HashMap<String, i64>,
    templates: &'a HashMap<String, i64>,
}

#[derive(Clone)]
struct ValueSlot {
    slot: u32,
    schema: String,
    realization: Option<u32>,
    pending: Option<PendingSlot>,
}

#[derive(Clone, Copy)]
struct PendingSlot {
    fn_ref: usize,
    given: usize,
}

struct BoundCall {
    args: Vec<ValueSlot>,
    partial: bool,
    given: usize,
}

struct BindCallSpec<'a> {
    fn_name: &'a str,
    param_names: &'a [String],
    param_schemas: &'a [String],
    args: &'a ast::ArgList,
    start: usize,
    allow_partial: bool,
    tail_identifier_uses: Option<&'a HashMap<String, usize>>,
}

enum TailOutcome {
    Value(ValueSlot),
    Jumped,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TemplatePart {
    Text(String),
    Hole(TemplateHole),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TemplateHole {
    name: String,
    filters: Vec<TemplateFilter>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TemplateFilter {
    Upper,
    Lower,
    Default(String),
}

fn parse_template(source: &str) -> Result<Vec<TemplatePart>, String> {
    let mut parts = Vec::new();
    let mut rest = source;
    loop {
        let Some(start) = rest.find("{{") else {
            if !rest.is_empty() || parts.is_empty() {
                parts.push(TemplatePart::Text(rest.to_string()));
            }
            return Ok(parts);
        };
        if start > 0 {
            parts.push(TemplatePart::Text(rest[..start].to_string()));
        }
        let after_start = &rest[start + 2..];
        let Some(end) = after_start.find("}}") else {
            return Err("template hole opened with `{{` but never closed".into());
        };
        let expr = after_start[..end].trim();
        parts.push(TemplatePart::Hole(parse_template_hole(expr)?));
        rest = &after_start[end + 2..];
    }
}

fn decode_template_literal(raw: &str) -> Result<String, String> {
    let quoted = raw
        .strip_prefix("tmpl")
        .ok_or_else(|| format!("template literal {raw:?} is missing `tmpl` prefix"))?;
    parse_template_string_arg(quoted)
}

fn parse_template_hole(expr: &str) -> Result<TemplateHole, String> {
    let mut pieces = expr.split('|').map(str::trim);
    let name = pieces
        .next()
        .filter(|name| is_template_identifier(name))
        .ok_or_else(|| format!("template hole `{expr}` does not start with a binding name"))?
        .to_string();
    let mut filters = Vec::new();
    for filter in pieces {
        filters.push(parse_template_filter(filter)?);
    }
    Ok(TemplateHole { name, filters })
}

fn parse_template_filter(filter: &str) -> Result<TemplateFilter, String> {
    match filter {
        "upper" => Ok(TemplateFilter::Upper),
        "lower" => Ok(TemplateFilter::Lower),
        _ => {
            let Some(arg) = filter
                .strip_prefix("default(")
                .and_then(|rest| rest.strip_suffix(')'))
            else {
                return Err(format!("unknown template filter `{filter}`"));
            };
            Ok(TemplateFilter::Default(parse_template_string_arg(arg)?))
        }
    }
}

fn parse_template_string_arg(arg: &str) -> Result<String, String> {
    let arg = arg.trim();
    if !arg.starts_with('"') || !arg.ends_with('"') {
        return Err(format!(
            "template default filter expects a string literal, got `{arg}`"
        ));
    }
    let mut out = String::with_capacity(arg.len().saturating_sub(2));
    let mut chars = arg[1..arg.len() - 1].chars();
    while let Some(ch) = chars.next() {
        if ch == '\\' {
            match chars.next() {
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some('r') => out.push('\r'),
                Some(other) => out.push(other),
                None => return Err("template default string ends with a backslash".into()),
            }
        } else {
            out.push(ch);
        }
    }
    Ok(out)
}

fn is_template_identifier(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

#[derive(Clone, Copy)]
struct FnSignatures<'a> {
    returns: &'a HashMap<String, String>,
    params: &'a HashMap<String, Vec<String>>,
    param_names: &'a HashMap<String, Vec<String>>,
}

type Bindings = HashMap<String, BindingCell>;

struct LowerEnv<'a> {
    tables: &'a ModuleTables,
    current_module: &'a str,
    current_fn_name: &'a str,
    current_fn_ref: usize,
    fn_refs: &'a HashMap<String, usize>,
    signatures: FnSignatures<'a>,
    schema_words: &'a HashMap<String, i64>,
    literal_handles: LiteralHandles<'a>,
    lower_options: LowerOptions,
}

#[derive(Clone)]
struct BindingCell(Rc<RefCell<BindingState>>);

#[derive(Clone)]
enum BindingState {
    Value(ValueSlot),
    Lazy {
        value: ast::Expr,
        expected: Option<String>,
        env: Bindings,
        consume_receiver: Option<String>,
    },
}

impl BindingCell {
    fn value(slot: ValueSlot) -> Self {
        Self(Rc::new(RefCell::new(BindingState::Value(slot))))
    }

    fn lazy(
        value: ast::Expr,
        expected: Option<String>,
        env: Bindings,
        consume_receiver: Option<String>,
    ) -> Self {
        Self(Rc::new(RefCell::new(BindingState::Lazy {
            value,
            expected,
            env,
            consume_receiver,
        })))
    }
}

fn fork_bindings(bindings: &Bindings) -> Bindings {
    bindings
        .iter()
        .map(|(name, cell)| (name.clone(), fork_binding_cell(cell)))
        .collect()
}

fn fork_binding_cell(cell: &BindingCell) -> BindingCell {
    match cell.0.borrow().clone() {
        BindingState::Value(slot) => BindingCell::value(slot),
        BindingState::Lazy {
            value,
            expected,
            env,
            consume_receiver,
        } => BindingCell::lazy(value, expected, fork_bindings(&env), consume_receiver),
    }
}

struct FnLowerer<'a> {
    tables: &'a ModuleTables,
    current_module: &'a str,
    current_fn_name: &'a str,
    current_fn_ref: usize,
    fn_refs: &'a HashMap<String, usize>,
    signatures: FnSignatures<'a>,
    schema_words: &'a HashMap<String, i64>,
    literal_handles: LiteralHandles<'a>,
    slots: Bindings,
    param_slots: Vec<ValueSlot>,
    next: u32,
    code: Vec<Op>,
    loop_header: u32,
    invoke_region: u32,
    store_alloc_region: u32,
    store_read_region: u32,
    store_tag_region: u32,
    primitive_region: u32,
    next_input_slot: i64,
    consume_receiver: Option<String>,
    force_tail_invoke: bool,
}

struct FnLowererSnapshot {
    slots: Bindings,
    next: u32,
    code: Vec<Op>,
    next_input_slot: i64,
    consume_receiver: Option<String>,
}

impl<'a> FnLowerer<'a> {
    fn lower(item: &ast::FnItem, env: LowerEnv<'a>) -> Result<(TaskFn, LoweredInfo), String> {
        let mut this = FnLowerer {
            tables: env.tables,
            current_module: env.current_module,
            current_fn_name: env.current_fn_name,
            current_fn_ref: env.current_fn_ref,
            fn_refs: env.fn_refs,
            signatures: env.signatures,
            schema_words: env.schema_words,
            literal_handles: env.literal_handles,
            slots: HashMap::new(),
            param_slots: Vec::new(),
            next: 0,
            code: Vec::new(),
            loop_header: 0,
            invoke_region: 0,
            store_alloc_region: 0,
            store_read_region: 0,
            store_tag_region: 0,
            primitive_region: 0,
            next_input_slot: 0,
            consume_receiver: None,
            force_tail_invoke: env.lower_options.force_tail_invoke,
        };

        let mut arg_offsets = Vec::new();
        for param in &item.params.params {
            let slot = this.alloc();
            let schema = type_schema_name(&param.ty)?;
            let value = ValueSlot {
                slot,
                schema,
                realization: None,
                pending: None,
            };
            this.slots
                .insert(param.name.value.clone(), BindingCell::value(value.clone()));
            this.param_slots.push(value);
            arg_offsets.push(slot);
        }

        // Reserve the invoke region: [slot, fn_ref, argc, args...] —
        // sized for the widest call in the body.
        let max_argc = max_call_argc(&item.body);
        this.invoke_region = this.next;
        this.next += 8 * (3 + u32::try_from(max_argc).expect("argc fits u32"));
        let max_store_fields = max_store_field_count(&item.body);
        this.store_alloc_region = this.next;
        this.next += 8 * (4 + u32::try_from(max_store_fields).expect("field count fits u32"));
        this.store_read_region = this.next;
        this.next += 8 * 3;
        this.store_tag_region = this.next;
        this.next += 8 * 2;
        this.primitive_region = this.next;
        let primitive_words = max_store_fields
            .max(max_argc)
            .max(max_command_part_words(&item.body));
        this.next += 8 * (128 + u32::try_from(primitive_words).expect("primitive word count"));
        this.loop_header = u32::try_from(this.code.len()).expect("code len fits u32");

        let return_schema = item
            .return_type
            .as_ref()
            .map(type_schema_name)
            .transpose()?
            .unwrap_or_else(|| "Int".into());
        match this.tail_block(&item.body, Some(&return_schema))? {
            TailOutcome::Value(result) => {
                let result = this.coerce_to_schema(result, &return_schema)?;
                this.code.push(Op::Ret {
                    src: result.slot,
                    size: 8,
                });
            }
            TailOutcome::Jumped => {}
        }

        let frame = Layout {
            size: this.next as usize,
            align: 8,
        };
        Ok((
            TaskFn {
                frame,
                code: this.code,
            },
            LoweredInfo {
                arg_offsets,
                invoke_region: this.invoke_region,
                store_alloc_region: this.store_alloc_region,
                store_read_region: this.store_read_region,
                store_tag_region: this.store_tag_region,
                primitive_region: this.primitive_region,
            },
        ))
    }

    fn alloc(&mut self) -> u32 {
        let slot = self.next;
        self.next += 8;
        slot
    }

    fn snapshot(&self) -> FnLowererSnapshot {
        FnLowererSnapshot {
            slots: fork_bindings(&self.slots),
            next: self.next,
            code: self.code.clone(),
            next_input_slot: self.next_input_slot,
            consume_receiver: self.consume_receiver.clone(),
        }
    }

    fn restore(&mut self, snapshot: FnLowererSnapshot) {
        self.slots = snapshot.slots;
        self.next = snapshot.next;
        self.code = snapshot.code;
        self.next_input_slot = snapshot.next_input_slot;
        self.consume_receiver = snapshot.consume_receiver;
    }

    fn tail_block(
        &mut self,
        block: &ast::Block,
        tail_expected: Option<&str>,
    ) -> Result<TailOutcome, String> {
        for stmt in &block.stmts {
            match stmt {
                ast::Stmt::Let(l) => {
                    let expected = l.ty.as_ref().map(type_schema_name).transpose()?;
                    let consume_receiver = consuming_rebind_receiver(&l.name.value, &l.value);
                    self.slots.insert(
                        l.name.value.clone(),
                        BindingCell::lazy(
                            l.value.clone(),
                            expected,
                            self.slots.clone(),
                            consume_receiver,
                        ),
                    );
                }
                ast::Stmt::Expr(_) => {
                    return Err("expression statements are outside the slice-1 subset".into());
                }
            }
        }
        let tail = block
            .tail
            .as_ref()
            .ok_or("slice-1 functions must end in a tail expression")?;
        self.tail_expr_expected(tail, tail_expected)
    }

    /// Compile an expression; returns the frame slot holding its value.
    fn expr(&mut self, e: &ast::Expr) -> Result<ValueSlot, String> {
        self.expr_expected(e, None)
    }

    fn tail_expr_expected(
        &mut self,
        e: &ast::Expr,
        expected: Option<&str>,
    ) -> Result<TailOutcome, String> {
        match e {
            ast::Expr::Paren(paren) => self.tail_expr_expected(&paren.inner, expected),
            ast::Expr::Match(m) => self.tail_match_expr(m, expected),
            ast::Expr::Call(call) => {
                if let Some(outcome) = self.self_tail_call(call)? {
                    Ok(outcome)
                } else {
                    self.expr_expected(e, expected).map(TailOutcome::Value)
                }
            }
            _ => self.expr_expected(e, expected).map(TailOutcome::Value),
        }
    }

    fn expr_expected(
        &mut self,
        e: &ast::Expr,
        expected: Option<&str>,
    ) -> Result<ValueSlot, String> {
        match e {
            ast::Expr::Number(n) => {
                if n.value.contains('.')
                    || expected.is_some_and(|schema| {
                        self.tables.schemas.is_primitive(schema, Primitive::F64)
                    })
                {
                    let value: f64 = n
                        .value
                        .parse()
                        .map_err(|_| format!("float literal {} does not parse", n.value))?;
                    let dst = self.alloc();
                    self.code.push(Op::ConstF64 {
                        dst,
                        bits: TotalF64::new(value).get().to_bits(),
                    });
                    return Ok(ValueSlot {
                        slot: dst,
                        schema: "Float".into(),
                        realization: None,
                        pending: None,
                    });
                }
                let value: i64 = n
                    .value
                    .parse()
                    .map_err(|_| format!("integer literal {} does not parse", n.value))?;
                let dst = self.alloc();
                self.code.push(Op::ConstI64 { dst, value });
                Ok(ValueSlot {
                    slot: dst,
                    schema: "Int".into(),
                    realization: None,
                    pending: None,
                })
            }
            ast::Expr::Str(s) => {
                let value = *self
                    .literal_handles
                    .strings
                    .get(&s.value)
                    .ok_or_else(|| format!("string literal {:?} was not interned", s.value))?;
                let dst = self.alloc();
                self.code.push(Op::ConstI64 { dst, value });
                Ok(ValueSlot {
                    slot: dst,
                    schema: "String".into(),
                    realization: None,
                    pending: None,
                })
            }
            ast::Expr::Template(t) => {
                let value = *self
                    .literal_handles
                    .templates
                    .get(&t.value)
                    .ok_or_else(|| format!("template literal {:?} was not interned", t.value))?;
                let dst = self.alloc();
                self.code.push(Op::ConstI64 { dst, value });
                Ok(ValueSlot {
                    slot: dst,
                    schema: "Template".into(),
                    realization: None,
                    pending: None,
                })
            }
            ast::Expr::Path(p) => {
                let value = *self
                    .literal_handles
                    .paths
                    .get(&p.value)
                    .ok_or_else(|| format!("path literal {:?} was not interned", p.value))?;
                let dst = self.alloc();
                self.code.push(Op::ConstI64 { dst, value });
                Ok(ValueSlot {
                    slot: dst,
                    schema: "Path".into(),
                    realization: None,
                    pending: None,
                })
            }
            ast::Expr::Bool(b) => {
                let dst = self.alloc();
                self.code.push(Op::ConstI64 {
                    dst,
                    value: i64::from(b.value),
                });
                Ok(ValueSlot {
                    slot: dst,
                    schema: "Bool".into(),
                    realization: None,
                    pending: None,
                })
            }
            ast::Expr::Identifier(name) => {
                if name.value == "None" && !self.slots.contains_key("None") {
                    return self.option_none(expected);
                }
                if let Some(info) = self.tables.structs.get(&name.value)
                    && info.is_unit
                    && !self.slots.contains_key(&name.value)
                {
                    return self.store_alloc(&name.value, 0, &[]);
                }
                self.resolve_binding(&name.value, expected)
            }
            ast::Expr::Paren(p) => self.expr(&p.inner),
            ast::Expr::Scoped(path) => self.scoped_value(path),
            ast::Expr::StructLit(lit) => self.struct_literal(lit),
            ast::Expr::Tuple(tuple) => self.tuple_literal(tuple, expected),
            ast::Expr::Field(field) => self.field_access(field, expected),
            ast::Expr::Binary(b) if b.op.as_str() == "/" => {
                let left = self.expr(&b.left)?;
                let right = self.expr(&b.right)?;
                match (left.schema.as_str(), right.schema.as_str()) {
                    ("Tree", "Path") => self.tree_project(&left, &right),
                    ("Path", "Path") => {
                        Err("Path / Path is outside the machine slice-4 subset".into())
                    }
                    _ => Err(format!(
                        "`/` on {} and {} is outside the machine slice-4 subset",
                        left.schema, right.schema
                    )),
                }
            }
            ast::Expr::Binary(b) => {
                let mut a = self.expr(&b.left)?;
                let mut r = self.expr(&b.right)?;
                // Comparison operators derive from a user-defined spaceship
                // `fn <=>(self: T, other) -> Ordering`: `a < b` ≡ `(a <=> b)` is
                // Less, etc. Only ordering is overridable this way; `==`/`!=` stay
                // structural (identity). The `<=>` name is a normal function name
                // (the grammar allows operator symbols).
                if matches!(b.op.as_str(), "<" | "<=" | ">" | ">=") {
                    let dispatch = self
                        .resolve_function_name("<=>")
                        .map(str::to_string)
                        .and_then(|resolved| {
                            let fn_ref = *self.fn_refs.get(&resolved)?;
                            let params = self.signatures.params.get(&resolved)?;
                            (params.first() == Some(&a.schema))
                                .then(|| (fn_ref, params.get(1).cloned()))
                        });
                    if let Some((fn_ref, p1)) = dispatch {
                        let r = match &p1 {
                            Some(p1) => self.coerce_to_schema(r, p1)?,
                            None => r,
                        };
                        let ord = self.invoke_fn(fn_ref, vec![a, r], "Ordering".into())?;
                        return Ok(self.ordering_to_bool(&ord, b.op.as_str()));
                    }
                }
                if b.op == "+"
                    && self.schema_is_stringish(&a.schema)
                    && self.schema_is_stringish(&r.schema)
                {
                    a = self.coerce_to_schema(a, "String")?;
                    r = self.coerce_to_schema(r, "String")?;
                    return self.string_concat(&a, &r);
                }
                if let Some(schema) =
                    strict_binary_operand_schema(&self.tables.schemas, &b.op, &a.schema, &r.schema)
                        .map(str::to_string)
                {
                    a = self.coerce_to_schema(a, &schema)?;
                    r = self.coerce_to_schema(r, &schema)?;
                }
                if b.op == "+"
                    && self
                        .tables
                        .schemas
                        .is_primitive(&a.schema, Primitive::String)
                    && self
                        .tables
                        .schemas
                        .is_primitive(&r.schema, Primitive::String)
                {
                    return self.string_concat(&a, &r);
                }
                if a.schema == r.schema
                    && (self
                        .tables
                        .schemas
                        .is_primitive(&a.schema, Primitive::String)
                        || self.tables.schemas.is_external(&a.schema, "Version"))
                    && matches!(b.op.as_str(), "==" | "!=" | "<" | "<=" | ">" | ">=")
                {
                    let schema = a.schema.clone();
                    return self.compare_value(b.op.as_str(), &a, &r, &schema);
                }
                let dst = self.alloc();
                let a_int = self.tables.schemas.is_primitive(&a.schema, Primitive::I64);
                let r_int = self.tables.schemas.is_primitive(&r.schema, Primitive::I64);
                let a_float = self.tables.schemas.is_primitive(&a.schema, Primitive::F64);
                let r_float = self.tables.schemas.is_primitive(&r.schema, Primitive::F64);
                let a_bool = self.tables.schemas.is_primitive(&a.schema, Primitive::Bool);
                let r_bool = self.tables.schemas.is_primitive(&r.schema, Primitive::Bool);
                let (op, schema) = match b.op.as_str() {
                    "+" if a_int && r_int => (
                        Op::AddI64 {
                            dst,
                            a: a.slot,
                            b: r.slot,
                        },
                        "Int",
                    ),
                    "-" if a_int && r_int => (
                        Op::SubI64 {
                            dst,
                            a: a.slot,
                            b: r.slot,
                        },
                        "Int",
                    ),
                    "*" if a_int && r_int => (
                        Op::MulI64 {
                            dst,
                            a: a.slot,
                            b: r.slot,
                        },
                        "Int",
                    ),
                    "+" if a_float && r_float => (
                        Op::AddF64 {
                            dst,
                            a: a.slot,
                            b: r.slot,
                        },
                        "Float",
                    ),
                    "*" if a_float && r_float => (
                        Op::MulF64 {
                            dst,
                            a: a.slot,
                            b: r.slot,
                        },
                        "Float",
                    ),
                    "==" => {
                        if a.schema != r.schema {
                            return Err(format!(
                                "cannot compare {} to {} in the machine slice-2 subset",
                                a.schema, r.schema
                            ));
                        }
                        (
                            Op::EqI64 {
                                dst,
                                a: a.slot,
                                b: r.slot,
                            },
                            "Bool",
                        )
                    }
                    "!=" => {
                        if a.schema != r.schema {
                            return Err(format!(
                                "cannot compare {} to {} in the machine B4 subset",
                                a.schema, r.schema
                            ));
                        }
                        (
                            Op::NeI64 {
                                dst,
                                a: a.slot,
                                b: r.slot,
                            },
                            "Bool",
                        )
                    }
                    "<" if a_int && r_int => (
                        Op::LtI64 {
                            dst,
                            a: a.slot,
                            b: r.slot,
                        },
                        "Bool",
                    ),
                    "<=" if a_int && r_int => (
                        Op::LeI64 {
                            dst,
                            a: a.slot,
                            b: r.slot,
                        },
                        "Bool",
                    ),
                    ">" if a_int && r_int => (
                        Op::GtI64 {
                            dst,
                            a: a.slot,
                            b: r.slot,
                        },
                        "Bool",
                    ),
                    ">=" if a_int && r_int => (
                        Op::GeI64 {
                            dst,
                            a: a.slot,
                            b: r.slot,
                        },
                        "Bool",
                    ),
                    "&&" if a_bool && r_bool => (
                        Op::MulI64 {
                            dst,
                            a: a.slot,
                            b: r.slot,
                        },
                        "Bool",
                    ),
                    other => {
                        return Err(format!(
                            "operator {other:?} on {} and {} is outside the machine slice-3 subset",
                            a.schema, r.schema
                        ));
                    }
                };
                self.code.push(op);
                Ok(ValueSlot {
                    slot: dst,
                    schema: schema.into(),
                    realization: None,
                    pending: None,
                })
            }
            ast::Expr::Call(call)
                if matches!(
                    &call.callee,
                    ast::PathRef::Identifier(name) if name.value == "json_typed"
                ) =>
            {
                self.typed_doc_parse_call(call, 1, expected)
            }
            ast::Expr::Call(call) => self.call(call),
            ast::Expr::MethodCall(call) => self.method_call(call, expected),
            ast::Expr::Map(map) => self.map_literal(map, expected),
            ast::Expr::Array(array) => self.array_literal(array, expected),
            ast::Expr::Command(command) => self.command_block(command),
            ast::Expr::Match(m) => self.match_expr(m, expected),
            other => Err(format!(
                "expression {other:?} is outside the slice-1 subset"
            )),
        }
    }

    fn resolve_binding(
        &mut self,
        name: &str,
        contextual_expected: Option<&str>,
    ) -> Result<ValueSlot, String> {
        let cell = self
            .slots
            .get(name)
            .cloned()
            .ok_or_else(|| format!("unbound name {name}"))?;
        let state = cell.0.borrow().clone();
        match state {
            BindingState::Value(slot) => {
                if self.schema_can_be_molten(&slot.schema) {
                    self.molten_dup(&slot)
                } else {
                    Ok(slot)
                }
            }
            BindingState::Lazy {
                value,
                expected,
                env,
                consume_receiver,
            } => {
                let saved = std::mem::replace(&mut self.slots, env);
                let saved_consume_receiver =
                    std::mem::replace(&mut self.consume_receiver, consume_receiver);
                let slot =
                    self.expr_expected(&value, contextual_expected.or(expected.as_deref()))?;
                *cell.0.borrow_mut() = BindingState::Value(slot.clone());
                self.consume_receiver = saved_consume_receiver;
                self.slots = saved;
                if self.schema_can_be_molten(&slot.schema) {
                    self.molten_dup(&slot)
                } else {
                    Ok(slot)
                }
            }
        }
    }

    fn resolve_binding_consuming_move(
        &mut self,
        name: &str,
        contextual_expected: Option<&str>,
    ) -> Result<ValueSlot, String> {
        let cell = self
            .slots
            .get(name)
            .cloned()
            .ok_or_else(|| format!("unbound name {name}"))?;
        let state = cell.0.borrow().clone();
        let slot = match state {
            BindingState::Value(slot) => slot,
            BindingState::Lazy {
                value,
                expected,
                env,
                consume_receiver,
            } => {
                let saved = std::mem::replace(&mut self.slots, env);
                let saved_consume_receiver =
                    std::mem::replace(&mut self.consume_receiver, consume_receiver);
                let slot =
                    self.expr_expected(&value, contextual_expected.or(expected.as_deref()))?;
                *cell.0.borrow_mut() = BindingState::Value(slot.clone());
                self.consume_receiver = saved_consume_receiver;
                self.slots = saved;
                slot
            }
        };
        Ok(self.copy_value(&slot))
    }

    fn copy_value(&mut self, value: &ValueSlot) -> ValueSlot {
        let dst = self.alloc();
        self.code.push(Op::CopyI64 {
            dst,
            src: value.slot,
        });
        ValueSlot {
            slot: dst,
            schema: value.schema.clone(),
            realization: value.realization,
            pending: value.pending,
        }
    }

    fn pending_lazy_alias_reads(&self, name: &str) -> bool {
        self.slots.iter().any(|(binding_name, cell)| {
            if binding_name == name {
                return false;
            }
            let BindingState::Lazy { value, env, .. } = &*cell.0.borrow() else {
                return false;
            };
            if !env.contains_key(name) {
                return false;
            }
            let mut identifiers = BTreeSet::new();
            collect_expr_identifiers(value, &mut identifiers);
            identifiers.contains(name)
        })
    }

    fn schema_can_be_molten(&self, schema: &str) -> bool {
        self.tables.schemas.is_list(schema)
            || self.tables.schemas.is_map(schema)
            || self.tables.schemas.is_struct_or_enum(schema)
    }

    fn schema_is_stringish(&self, schema: &str) -> bool {
        self.tables.schemas.is_primitive(schema, Primitive::String)
            || self.tables.schemas.is_external(schema, "Sealed")
    }

    fn value_schema_is_realized_named(&self, schema: &str, name: &str) -> bool {
        realized_value_schema(schema)
            .is_some_and(|inner| self.tables.schemas.is_named_schema(inner, name))
    }

    /// Match on scalars: literal arms compile to EqI64 + JumpIfZero
    /// chains; the final arm must be irrefutable (wildcard or a
    /// binding) until the checker owns exhaustiveness. THE LAZINESS
    /// INVARIANT AT MACHINE LEVEL: an untaken arm's code never
    /// executes, so an INVOKE it contains never fires — unused arms
    /// never spawn, provable by trace absence.
    fn match_expr(
        &mut self,
        m: &ast::MatchExpr,
        expected: Option<&str>,
    ) -> Result<ValueSlot, String> {
        match self.match_expr_outcome(m, expected, false)? {
            TailOutcome::Value(value) => Ok(value),
            TailOutcome::Jumped => Err("non-tail match arm unexpectedly jumped".into()),
        }
    }

    fn tail_match_expr(
        &mut self,
        m: &ast::MatchExpr,
        expected: Option<&str>,
    ) -> Result<TailOutcome, String> {
        self.match_expr_outcome(m, expected, true)
    }

    fn match_expr_outcome(
        &mut self,
        m: &ast::MatchExpr,
        expected: Option<&str>,
        tail: bool,
    ) -> Result<TailOutcome, String> {
        let scrut = self.expr(&m.scrutinee)?;
        let scrut = self.coerce_inner(scrut)?;
        self.hoist_bindings_used_by_multiple_arms(m)?;
        let mut result = None;
        let mut result_schema: Option<String> = None;
        let mut jump_to_end: Vec<usize> = Vec::new();
        let mut bool_covered = BTreeSet::new();

        let last = m.arms.len().saturating_sub(1);
        for (i, arm) in m.arms.iter().enumerate() {
            let saved_slots = self.slots.clone();
            self.slots = fork_bindings(&saved_slots);
            let mut skip_patches: Vec<usize> = Vec::new();
            let mut bool_pattern = None;
            match &arm.pattern {
                ast::Pattern::Number(n) => {
                    let value: i64 = n
                        .value
                        .parse()
                        .map_err(|_| format!("pattern {} does not parse", n.value))?;
                    let lit = self.alloc();
                    self.code.push(Op::ConstI64 { dst: lit, value });
                    let test = self.alloc();
                    self.code.push(Op::EqI64 {
                        dst: test,
                        a: self.expect_schema(&scrut, "Int")?,
                        b: lit,
                    });
                    skip_patches.push(self.code.len());
                    self.code.push(Op::JumpIfZero {
                        value: test,
                        target: 0,
                    });
                }
                ast::Pattern::Bool(b) => {
                    let lit = self.alloc();
                    self.code.push(Op::ConstI64 {
                        dst: lit,
                        value: i64::from(b.value),
                    });
                    let test = self.alloc();
                    self.code.push(Op::EqI64 {
                        dst: test,
                        a: self.expect_schema(&scrut, "Bool")?,
                        b: lit,
                    });
                    skip_patches.push(self.code.len());
                    self.code.push(Op::JumpIfZero {
                        value: test,
                        target: 0,
                    });
                    bool_pattern = Some(b.value);
                }
                ast::Pattern::Str(s) => {
                    let value =
                        *self.literal_handles.strings.get(&s.value).ok_or_else(|| {
                            format!("string pattern {:?} was not interned", s.value)
                        })?;
                    let lit = self.alloc();
                    self.code.push(Op::ConstI64 { dst: lit, value });
                    let test = self.alloc();
                    self.code.push(Op::EqI64 {
                        dst: test,
                        a: self.expect_schema(&scrut, "String")?,
                        b: lit,
                    });
                    skip_patches.push(self.code.len());
                    self.code.push(Op::JumpIfZero {
                        value: test,
                        target: 0,
                    });
                }
                ast::Pattern::Scoped(path) => {
                    let (enum_name, variant_index, _) = self.resolve_scoped_variant(path)?;
                    self.variant_match_test(&scrut, &enum_name, variant_index, &mut skip_patches)?;
                }
                ast::Pattern::Variant(p)
                    if self
                        .tables
                        .schemas
                        .option_value_schema_name(&scrut.schema)
                        .is_some() =>
                {
                    let segments = path_ref_segments(&p.path)?;
                    let variant = segments.last().map(String::as_str).unwrap_or_default();
                    if variant != "Some" {
                        return Err(format!("Option pattern `{variant}` is not Some or None"));
                    }
                    let value_schema = self
                        .tables
                        .schemas
                        .option_value_schema_name(&scrut.schema)
                        .ok_or_else(|| format!("scrutinee {} is not an Option", scrut.schema))?
                        .to_string();
                    self.option_tag_test(&scrut, 1, &mut skip_patches);
                    let [pattern] = p.args.as_slice() else {
                        return Err("Some pattern takes one binding".into());
                    };
                    let payload = self.option_destruct(&scrut, 1, &value_schema);
                    self.bind_option_payload(pattern, payload)?;
                }
                ast::Pattern::Variant(p) => {
                    let (enum_name, variant_index, shape) = self.resolve_path_variant(&p.path)?;
                    self.variant_match_test(&scrut, &enum_name, variant_index, &mut skip_patches)?;
                    let VariantShape::Tuple(expected) = shape else {
                        return Err(format!(
                            "tuple variant pattern used on non-tuple variant {enum_name}"
                        ));
                    };
                    if p.args.len() != expected {
                        return Err(format!(
                            "variant pattern expected {expected} fields, got {}",
                            p.args.len()
                        ));
                    }
                    for (field_index, pattern) in p.args.iter().enumerate() {
                        self.bind_payload_pattern(&scrut, variant_index, field_index, pattern)?;
                    }
                }
                ast::Pattern::Struct(p) => {
                    let (enum_name, variant_index, shape) = self.resolve_path_variant(&p.path)?;
                    self.variant_match_test(&scrut, &enum_name, variant_index, &mut skip_patches)?;
                    let VariantShape::Record(field_names) = shape else {
                        return Err(format!(
                            "record pattern used on non-record variant {enum_name}"
                        ));
                    };
                    for field in &p.fields {
                        let field_index = field_names
                            .iter()
                            .position(|name| name == &field.name.value)
                            .ok_or_else(|| format!("unknown field {}", field.name.value))?;
                        if let Some(pattern) = &field.pattern {
                            self.bind_payload_pattern(&scrut, variant_index, field_index, pattern)?;
                        } else {
                            let value = self.store_read(
                                &scrut,
                                field_index,
                                self.variant_field_schema(&enum_name, variant_index, field_index)?,
                            );
                            self.slots
                                .insert(field.name.value.clone(), BindingCell::value(value));
                        }
                    }
                }
                ast::Pattern::Wildcard(_) => {
                    if i != last {
                        return Err("wildcard arm must be last".into());
                    }
                }
                ast::Pattern::Identifier(name)
                    if self
                        .tables
                        .schemas
                        .option_value_schema_name(&scrut.schema)
                        .is_some()
                        && name.value == "None" =>
                {
                    self.option_tag_test(&scrut, 0, &mut skip_patches);
                }
                ast::Pattern::Identifier(name) => {
                    if let Some(variant_index) =
                        self.shorthand_unit_variant(&scrut.schema, &name.value)
                    {
                        self.variant_match_test(
                            &scrut,
                            &scrut.schema,
                            variant_index,
                            &mut skip_patches,
                        )?;
                    } else if i != last {
                        return Err("binding arm must be last".into());
                    } else {
                        self.slots
                            .insert(name.value.clone(), BindingCell::value(scrut.clone()));
                    }
                }
                other => {
                    return Err(format!("pattern {other:?} is outside the slice-2 subset"));
                }
            }
            if let Some(guard) = &arm.guard {
                let guard = self.expr_expected(guard, Some("Bool"))?;
                let guard = self.coerce_to_schema(guard, "Bool")?;
                skip_patches.push(self.code.len());
                self.code.push(Op::JumpIfZero {
                    value: self.expect_schema(&guard, "Bool")?,
                    target: 0,
                });
            } else if self
                .tables
                .schemas
                .is_primitive(&scrut.schema, Primitive::Bool)
                && let Some(value) = bool_pattern
            {
                bool_covered.insert(value);
            }
            if skip_patches.is_empty() && i != last {
                return Err("irrefutable arm before the last arm".into());
            }
            let outcome = if tail {
                self.tail_expr_expected(&arm.value, expected)?
            } else {
                let v = self.expr_expected(&arm.value, expected)?;
                let v = if let Some(expected) = expected {
                    self.coerce_to_schema(v, expected)?
                } else {
                    v
                };
                TailOutcome::Value(v)
            };
            let outcome = match (outcome, expected) {
                (TailOutcome::Value(v), Some(expected)) => {
                    TailOutcome::Value(self.coerce_to_schema(v, expected)?)
                }
                (outcome, _) => outcome,
            };
            match outcome {
                TailOutcome::Value(v) => {
                    match &result_schema {
                        Some(schema) if schema != &v.schema => {
                            return Err(format!(
                                "match arm returned {}, previous arm returned {schema}",
                                v.schema
                            ));
                        }
                        None => result_schema = Some(v.schema.clone()),
                        _ => {}
                    }
                    let result = *result.get_or_insert_with(|| self.alloc());
                    self.code.push(Op::CopyI64 {
                        dst: result,
                        src: v.slot,
                    });
                    if i != last {
                        jump_to_end.push(self.code.len());
                        self.code.push(Op::Jump { target: 0 });
                    }
                }
                TailOutcome::Jumped => {}
            }
            for at in skip_patches {
                let next = u32::try_from(self.code.len()).expect("code len fits u32");
                let Op::JumpIfZero { value, .. } = self.code[at] else {
                    unreachable!("skip patch site is a JumpIfZero");
                };
                self.code[at] = Op::JumpIfZero {
                    value,
                    target: next,
                };
            }
            if i == last {
                break;
            }
            self.slots = saved_slots;
        }
        if (self
            .tables
            .schemas
            .is_primitive(&scrut.schema, Primitive::I64)
            || self
                .tables
                .schemas
                .is_primitive(&scrut.schema, Primitive::String))
            && !matches!(
                m.arms.last().map(|a| &a.pattern),
                Some(ast::Pattern::Wildcard(_) | ast::Pattern::Identifier(_))
            )
        {
            return Err(
                "scalar/string match must end with an irrefutable arm (exhaustiveness \
                 checking arrives with the checker)"
                    .into(),
            );
        }
        if self
            .tables
            .schemas
            .is_primitive(&scrut.schema, Primitive::Bool)
            && !matches!(
                m.arms.last().map(|a| &a.pattern),
                Some(ast::Pattern::Wildcard(_) | ast::Pattern::Identifier(_))
            )
            && !(bool_covered.contains(&true) && bool_covered.contains(&false))
        {
            let missing = match (bool_covered.contains(&true), bool_covered.contains(&false)) {
                (false, false) => "true,false",
                (false, true) => "true",
                (true, false) => "false",
                (true, true) => unreachable!("checked above"),
            };
            return Err(format!(
                "bool match must cover true and false; missing {missing}"
            ));
        }
        let end = u32::try_from(self.code.len()).expect("code len fits u32");
        for at in jump_to_end {
            self.code[at] = Op::Jump { target: end };
        }
        if let Some(result) = result {
            Ok(TailOutcome::Value(ValueSlot {
                slot: result,
                schema: result_schema.unwrap_or_else(|| "Int".into()),
                realization: None,
                pending: None,
            }))
        } else if tail {
            Ok(TailOutcome::Jumped)
        } else {
            Err("non-tail match produced no value".into())
        }
    }

    fn hoist_bindings_used_by_multiple_arms(&mut self, m: &ast::MatchExpr) -> Result<(), String> {
        let mut counts: HashMap<String, usize> = HashMap::new();
        let mut consumed_receivers = BTreeSet::new();
        for arm in &m.arms {
            collect_consuming_update_receivers_in_expr(&arm.value, &mut consumed_receivers);
            let mut names = BTreeSet::new();
            collect_expr_identifiers(&arm.value, &mut names);
            if let Some(guard) = &arm.guard {
                collect_expr_identifiers(guard, &mut names);
            }
            let mut bound = BTreeSet::new();
            collect_pattern_bindings(&arm.pattern, &mut bound);
            for name in names {
                if !bound.contains(&name) {
                    *counts.entry(name).or_default() += 1;
                }
            }
        }
        let hoist: Vec<String> = counts
            .into_iter()
            .filter_map(|(name, count)| {
                (count > 1 && self.slots.contains_key(&name) && !consumed_receivers.contains(&name))
                    .then_some(name)
            })
            .collect();
        for name in hoist {
            self.resolve_binding(&name, None)?;
        }
        Ok(())
    }

    /// A user-function call: a MEMO BOUNDARY through the INVOKE
    /// protocol. Argument values are computed into slots first, then
    /// copied into the invoke region (frame-direct at the driver
    /// boundary), then HostCall + Await.
    fn call(&mut self, call: &ast::Call) -> Result<ValueSlot, String> {
        if let Some(value) = self.variant_constructor_call(call)? {
            return Ok(value);
        }
        if let Some(value) = self.builtin_scoped_call(call)? {
            return Ok(value);
        }
        let name = match &call.callee {
            ast::PathRef::Identifier(name) => name.value.as_str(),
            other => {
                return Err(format!("callee {other:?} is outside the slice-1 subset"));
            }
        };
        match name {
            "fetch" => return self.fetch_call(call),
            "extract" => return self.extract_call(call),
            "toml" => return self.doc_parse_call(call, 0),
            "json" => return self.doc_parse_call(call, 1),
            "build_directives" => return self.doc_parse_call(call, 2),
            "cfg" => return self.doc_parse_call(call, 3),
            "rustc_cfg" => return self.doc_parse_call(call, 4),
            "crate_archive" => return self.crate_archive_call(call),
            "version" => return self.version_call(call),
            "Some" => return self.option_some_call(call),
            "render" => return self.render_call(call),
            "elf" => return self.elf_call(call),
            "ast" => return self.ast_call(call),
            "oci" => return self.oci_call(call),
            _ => {}
        }
        if let Some(resolved_name) = self.resolve_function_name(name).map(str::to_string)
            && let Some(&fn_ref) = self.fn_refs.get(&resolved_name)
        {
            let param_names = self
                .signatures
                .param_names
                .get(&resolved_name)
                .ok_or_else(|| format!("missing param names for {resolved_name}"))?
                .clone();
            let param_schemas = self
                .signatures
                .params
                .get(&resolved_name)
                .ok_or_else(|| format!("missing param schemas for {resolved_name}"))?
                .clone();
            let bound = self.bind_call_args(BindCallSpec {
                fn_name: &resolved_name,
                param_names: &param_names,
                param_schemas: &param_schemas,
                args: &call.args,
                start: 0,
                allow_partial: true,
                tail_identifier_uses: None,
            })?;
            let return_schema = self.signatures.returns[&resolved_name].clone();
            if bound.partial {
                return self.pending_alloc_for_fn(
                    fn_ref,
                    &return_schema,
                    bound.args,
                    Some(PendingSlot {
                        fn_ref,
                        given: bound.given,
                    }),
                );
            }
            return self.invoke_fn(fn_ref, bound.args, return_schema);
        }

        let callee = self.resolve_binding(name, None)?;
        let Some(pending) = callee.pending else {
            return Err(format!("unknown function {name}"));
        };
        let fn_name = self.fn_name_for_ref(pending.fn_ref)?.to_string();
        let param_names = self
            .signatures
            .param_names
            .get(&fn_name)
            .ok_or_else(|| format!("missing param names for {fn_name}"))?
            .clone();
        let param_schemas = self
            .signatures
            .params
            .get(&fn_name)
            .ok_or_else(|| format!("missing param schemas for {fn_name}"))?
            .clone();
        let bound = self.bind_call_args(BindCallSpec {
            fn_name: &fn_name,
            param_names: &param_names,
            param_schemas: &param_schemas,
            args: &call.args,
            start: pending.given,
            allow_partial: false,
            tail_identifier_uses: None,
        })?;
        let value_schema = pending_value_schema(&callee.schema)
            .ok_or_else(|| format!("callee {name} is `{}`, not pending", callee.schema))?
            .to_string();
        self.pending_invoke(callee, bound.args, &value_schema)
    }

    fn resolve_function_name(&self, name: &str) -> Option<&str> {
        self.tables.resolve_fn(self.current_module, name)
    }

    fn invoke_fn(
        &mut self,
        fn_ref: usize,
        arg_slots: Vec<ValueSlot>,
        return_schema: String,
    ) -> Result<ValueSlot, String> {
        let input_slot = self.next_input_slot;
        self.next_input_slot += 1;
        let region = self.invoke_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: input_slot,
        });
        self.code.push(Op::ConstI64 {
            dst: region + 8,
            value: i64::try_from(fn_ref).expect("fn_ref fits i64"),
        });
        self.code.push(Op::ConstI64 {
            dst: region + 16,
            value: i64::try_from(arg_slots.len()).expect("argc fits i64"),
        });
        for (i, slot) in arg_slots.iter().enumerate() {
            self.code.push(Op::CopyI64 {
                dst: region + 24 + 8 * u32::try_from(i).expect("arg index"),
                src: slot.slot,
            });
        }
        self.code.push(Op::HostCall { host: INVOKE_HOST });
        let dst = self.alloc();
        self.code.push(Op::Await {
            dst,
            input: u32::try_from(input_slot).expect("input slot fits u32"),
        });
        Ok(ValueSlot {
            slot: dst,
            schema: return_schema,
            realization: None,
            pending: None,
        })
    }

    fn self_tail_call(&mut self, call: &ast::Call) -> Result<Option<TailOutcome>, String> {
        if self.force_tail_invoke {
            return Ok(None);
        }
        let name = match &call.callee {
            ast::PathRef::Identifier(name) => name.value.as_str(),
            _ => return Ok(None),
        };
        let Some(resolved_name) = self.resolve_function_name(name).map(str::to_string) else {
            return Ok(None);
        };
        if resolved_name != self.current_fn_name {
            return Ok(None);
        }
        let Some(&fn_ref) = self.fn_refs.get(&resolved_name) else {
            return Ok(None);
        };
        if fn_ref != self.current_fn_ref {
            return Ok(None);
        }
        let param_names = self
            .signatures
            .param_names
            .get(&resolved_name)
            .ok_or_else(|| format!("missing param names for {resolved_name}"))?
            .clone();
        let param_schemas = self
            .signatures
            .params
            .get(&resolved_name)
            .ok_or_else(|| format!("missing param schemas for {resolved_name}"))?
            .clone();
        if arg_list_has_partial(&call.args) {
            return Ok(None);
        }
        let snapshot = self.snapshot();
        let tail_identifier_uses = identifier_uses_in_arg_list(&call.args);
        let bound = self.bind_call_args(BindCallSpec {
            fn_name: &resolved_name,
            param_names: &param_names,
            param_schemas: &param_schemas,
            args: &call.args,
            start: 0,
            allow_partial: false,
            tail_identifier_uses: Some(&tail_identifier_uses),
        })?;
        if self.emit_self_tail_jump(bound.args, &param_schemas)? {
            Ok(Some(TailOutcome::Jumped))
        } else {
            self.restore(snapshot);
            Ok(None)
        }
    }

    fn emit_self_tail_jump(
        &mut self,
        args: Vec<ValueSlot>,
        param_schemas: &[String],
    ) -> Result<bool, String> {
        if args.len() != self.param_slots.len() || args.len() != param_schemas.len() {
            return Err(format!(
                "self-tail-call argument count {} did not match parameter count {}",
                args.len(),
                self.param_slots.len()
            ));
        }
        let mut temps = Vec::with_capacity(args.len());
        for (arg, schema) in args.into_iter().zip(param_schemas) {
            let arg = self.coerce_to_schema(arg, schema)?;
            if arg.schema != *schema {
                return Ok(false);
            }
            temps.push(self.copy_value(&arg));
        }
        for (param, temp) in self.param_slots.clone().into_iter().zip(&temps) {
            self.code.push(Op::CopyI64 {
                dst: param.slot,
                src: temp.slot,
            });
        }
        self.code.push(Op::Jump {
            target: self.loop_header,
        });
        Ok(true)
    }

    fn fn_name_for_ref(&self, fn_ref: usize) -> Result<&str, String> {
        self.fn_refs
            .iter()
            .find_map(|(name, candidate)| (*candidate == fn_ref).then_some(name.as_str()))
            .ok_or_else(|| format!("unknown fn_ref {fn_ref}"))
    }

    fn bind_call_args(&mut self, spec: BindCallSpec<'_>) -> Result<BoundCall, String> {
        let BindCallSpec {
            fn_name,
            param_names,
            param_schemas,
            args,
            start,
            allow_partial,
            tail_identifier_uses,
        } = spec;
        if start > param_names.len() || param_names.len() != param_schemas.len() {
            return Err(format!("bad parameter table for `{fn_name}`"));
        }
        let mut values: Vec<Option<ValueSlot>> = vec![None; param_names.len() - start];
        let mut positional = 0usize;
        let mut partial = false;
        for arg in &args.args {
            match arg {
                ast::Arg::Partial(_) => {
                    if !allow_partial {
                        return Err(format!(
                            "partial call marker is not accepted here for `{fn_name}`"
                        ));
                    }
                    if partial {
                        return Err(format!("duplicate partial marker for `{fn_name}`"));
                    }
                    partial = true;
                }
                ast::Arg::Expr(expr) => {
                    while values.get(positional).is_some_and(Option::is_some) {
                        positional += 1;
                    }
                    let Some(expected) = param_schemas.get(start + positional) else {
                        return Err(format!("too many arguments for `{fn_name}`"));
                    };
                    let value =
                        self.expr_expected_call_arg(expr, Some(expected), tail_identifier_uses)?;
                    values[positional] = Some(self.coerce_to_schema(value, expected)?);
                    positional += 1;
                }
                ast::Arg::Kwarg(kwarg) => {
                    let offset = param_names[start..]
                        .iter()
                        .position(|name| name == &kwarg.name.value)
                        .ok_or_else(|| {
                            format!("`{fn_name}` has no argument `{}`", kwarg.name.value)
                        })?;
                    if values[offset].is_some() {
                        return Err(format!("duplicate argument `{}`", kwarg.name.value));
                    }
                    let expected = &param_schemas[start + offset];
                    let value = self.expr_expected_call_arg(
                        &kwarg.value,
                        Some(expected),
                        tail_identifier_uses,
                    )?;
                    values[offset] = Some(self.coerce_to_schema(value, expected)?);
                }
            }
        }

        let missing: Vec<String> = values
            .iter()
            .enumerate()
            .filter(|(_, value)| value.is_none())
            .map(|(offset, _)| param_names[start + offset].clone())
            .collect();
        if !partial && !missing.is_empty() {
            return Err(format!("`{fn_name}` missing argument(s): {missing:?}"));
        }
        if partial {
            let prefix_len = values.iter().take_while(|value| value.is_some()).count();
            if values[prefix_len..].iter().any(Option::is_some) {
                return Err(format!(
                    "partial call to `{fn_name}` must bind a contiguous argument prefix"
                ));
            }
            let args = values.into_iter().take(prefix_len).flatten().collect();
            return Ok(BoundCall {
                args,
                partial: true,
                given: start + prefix_len,
            });
        }
        let args = values
            .into_iter()
            .map(|value| value.expect("missing arguments checked"))
            .collect::<Vec<_>>();
        Ok(BoundCall {
            args,
            partial: false,
            given: param_names.len(),
        })
    }

    fn expr_expected_call_arg(
        &mut self,
        expr: &ast::Expr,
        expected: Option<&str>,
        tail_identifier_uses: Option<&HashMap<String, usize>>,
    ) -> Result<ValueSlot, String> {
        let Some(identifier_uses) = tail_identifier_uses else {
            return self.expr_expected(expr, expected);
        };
        let Some(receiver) = consuming_update_receiver(expr) else {
            return self.expr_expected(expr, expected);
        };
        if identifier_uses.get(receiver).copied() != Some(1) {
            return self.expr_expected(expr, expected);
        }
        let saved_consume_receiver = self.consume_receiver.replace(receiver.to_string());
        let result = self.expr_expected(expr, expected);
        self.consume_receiver = saved_consume_receiver;
        result
    }

    fn builtin_scoped_call(&mut self, call: &ast::Call) -> Result<Option<ValueSlot>, String> {
        let ast::PathRef::Scoped(path) = &call.callee else {
            return Ok(None);
        };
        let segments: Vec<&str> = path.segments.iter().map(|s| s.value.as_str()).collect();
        match segments.as_slice() {
            [kind @ ("Cc" | "Ar" | "Rustc"), "acquire"] => {
                if self.tables.resolve_type_module(self.current_module, kind) != Some("caps") {
                    return Ok(None);
                }
                let [ast::Arg::Expr(target)] = call.args.args.as_slice() else {
                    return Err(format!("{kind}::acquire takes one target"));
                };
                let target = self.expr_expected(target, Some("Target"))?;
                Ok(Some(self.acquire(kind, &target)?))
            }
            ["Target", "host"] => {
                if self
                    .tables
                    .resolve_type_module(self.current_module, "Target")
                    != Some("vix")
                {
                    return Ok(None);
                }
                if !call.args.args.is_empty() {
                    return Err("Target::host takes no arguments".into());
                }
                Ok(Some(self.target_host()))
            }
            ["VersionSet", "from_req"] => {
                if self
                    .tables
                    .resolve_type_module(self.current_module, "VersionSet")
                    != Some("vix")
                {
                    return Ok(None);
                }
                let [arg] = call.args.args.as_slice() else {
                    return Err("VersionSet::from_req takes one String".into());
                };
                let input = self.method_arg(arg, Some("String"))?;
                Ok(Some(self.version_set_from_req(&input)?))
            }
            ["Sealed", "seal"] => {
                if self
                    .tables
                    .resolve_type_module(self.current_module, "Sealed")
                    != Some("vix")
                {
                    return Ok(None);
                }
                let args = call
                    .args
                    .args
                    .iter()
                    .map(|arg| self.method_arg(arg, Some("String")))
                    .collect::<Result<Vec<_>, _>>()?;
                match args.as_slice() {
                    [ciphertext, marker, recipient] => {
                        Ok(Some(self.sealed_seal(ciphertext, marker, recipient, None)?))
                    }
                    [ciphertext, marker, recipient, tag] => Ok(Some(self.sealed_seal(
                        ciphertext,
                        marker,
                        recipient,
                        Some(tag),
                    )?)),
                    _ => Err(
                        "Sealed::seal takes ciphertext, taint, recipient, and optional tag".into(),
                    ),
                }
            }
            ["Sealed", "declassify"] => {
                if self
                    .tables
                    .resolve_type_module(self.current_module, "Sealed")
                    != Some("vix")
                {
                    return Ok(None);
                }
                let [arg] = call.args.args.as_slice() else {
                    return Err("Sealed::declassify takes one sealed value".into());
                };
                let sealed = self.method_arg(arg, Some("Sealed"))?;
                Ok(Some(self.sealed_declassify(&sealed)?))
            }
            _ => Ok(None),
        }
    }

    fn method_call(
        &mut self,
        call: &ast::MethodCall,
        expected: Option<&str>,
    ) -> Result<ValueSlot, String> {
        let (mut receiver, consuming_receiver) = self.method_receiver(call)?;
        if self.value_schema_is_realized_named(&receiver.schema, "Doc") {
            receiver = self.coerce_to_schema(receiver, "Doc")?;
        }
        match call.name.value.as_str() {
            "with_ext" => {
                if !self.tables.schemas.is_external(&receiver.schema, "Path") {
                    return Err(format!("with_ext called on {}", receiver.schema));
                }
                let [arg] = call.args.args.as_slice() else {
                    return Err("Path.with_ext takes one extension".into());
                };
                let ext = self.method_arg(arg, Some("String"))?;
                self.path_with_ext(&receiver, &ext)
            }
            "to_string" => {
                if !call.args.args.is_empty() {
                    return Err("Path.to_string takes no arguments".into());
                }
                self.expect_schema(&receiver, "Path")?;
                self.raw_string_convert(&receiver, "String", PATH_TO_STRING_HOST)
            }
            "is_map" => {
                if !call.args.args.is_empty() {
                    return Err("Doc.is_map takes no arguments".into());
                }
                self.expect_schema(&receiver, "Doc")?;
                self.doc_is_map(&receiver)
            }
            "keys" => {
                if !call.args.args.is_empty() {
                    return Err("keys takes no arguments".into());
                }
                if !self.tables.schemas.is_named_schema(&receiver.schema, "Doc") {
                    let Some((key_schema, _)) =
                        self.tables.schemas.map_schema_names(&receiver.schema)
                    else {
                        return Err(format!("keys called on {}", receiver.schema));
                    };
                    if !self
                        .tables
                        .schemas
                        .is_primitive(&key_schema, Primitive::String)
                    {
                        return Err(format!("keys requires String map keys, got {key_schema}"));
                    }
                }
                self.doc_keys(&receiver)
            }
            "len" => {
                if !call.args.args.is_empty() {
                    return Err("len takes no arguments".into());
                }
                let receiver = if self.tables.schemas.is_list(&receiver.schema) {
                    receiver
                } else if self.tables.schemas.is_named_schema(&receiver.schema, "Doc") {
                    self.coerce_doc_to_schema(receiver, "Array<Doc>")?
                } else if self.value_schema_is_realized_named(&receiver.schema, "Doc") {
                    let doc = self.coerce_to_schema(receiver, "Doc")?;
                    self.coerce_doc_to_schema(doc, "Array<Doc>")?
                } else {
                    return Err(format!("len called on {}", receiver.schema));
                };
                self.array_len(&receiver)
            }
            "glob" => self.tree_glob(&receiver, call),
            "text" => {
                self.expect_schema(&receiver, "Tree")?;
                let [arg] = call.args.args.as_slice() else {
                    return Err("Tree.text takes one Path".into());
                };
                let path = self.method_arg(arg, Some("Path"))?;
                self.tree_text(&receiver, &path)
            }
            "filter" => self.array_filter_exclude(&receiver, call),
            "map" => self.array_map_pending(&receiver, call),
            "collect" => {
                if !call.args.args.is_empty() {
                    return Err("collect takes no arguments".into());
                }
                self.array_collect(&receiver, expected)
            }
            "join" => {
                if self.tables.schemas.is_external(&receiver.schema, "Path") {
                    let [arg] = call.args.args.as_slice() else {
                        return Err("Path.join takes one segment".into());
                    };
                    let segment = self.method_arg(arg, Some("String"))?;
                    return self.path_join(&receiver, &segment);
                }
                let receiver = if self.tables.schemas.is_list(&receiver.schema) {
                    receiver
                } else if self.tables.schemas.is_named_schema(&receiver.schema, "Doc") {
                    self.coerce_doc_to_schema(receiver, "Array<Doc>")?
                } else if self.value_schema_is_realized_named(&receiver.schema, "Doc") {
                    let doc = self.coerce_to_schema(receiver, "Doc")?;
                    self.coerce_doc_to_schema(doc, "Array<Doc>")?
                } else {
                    return Err(format!("join called on {}", receiver.schema));
                };
                let [arg] = call.args.args.as_slice() else {
                    return Err("Array.join takes one separator".into());
                };
                let separator = self.method_arg(arg, Some("String"))?;
                self.array_join(&receiver, &separator)
            }
            "push" => {
                if !self.tables.schemas.is_list(&receiver.schema) {
                    return Err(format!("push called on {}", receiver.schema));
                }
                let [arg] = call.args.args.as_slice() else {
                    return Err("Array.push takes one value".into());
                };
                let elem_schema = array_element_schema(&receiver.schema)
                    .ok_or_else(|| format!("{} is not an Array<T>", receiver.schema))?
                    .to_string();
                let value = self.method_arg(arg, Some(&elem_schema))?;
                self.array_push(&receiver, &value, consuming_receiver)
            }
            "pop" => {
                if !self.tables.schemas.is_list(&receiver.schema) {
                    return Err(format!("pop called on {}", receiver.schema));
                }
                if !call.args.args.is_empty() {
                    return Err("Array.pop takes no arguments".into());
                }
                self.array_pop(&receiver)
            }
            "set" => {
                if !self.tables.schemas.is_list(&receiver.schema) {
                    return Err(format!("set called on {}", receiver.schema));
                }
                let [index_arg, value_arg] = call.args.args.as_slice() else {
                    return Err("Array.set takes index and value".into());
                };
                let index = self.method_arg(index_arg, Some("Int"))?;
                let elem_schema = array_element_schema(&receiver.schema)
                    .ok_or_else(|| format!("{} is not an Array<T>", receiver.schema))?
                    .to_string();
                let value = self.method_arg(value_arg, Some(&elem_schema))?;
                self.array_set(&receiver, &index, &value)
            }
            "insert" => {
                let Some((key_schema, value_schema)) =
                    self.tables.schemas.map_schema_names(&receiver.schema)
                else {
                    return Err(format!("insert called on {}", receiver.schema));
                };
                let logical_value_schema =
                    realized_value_schema(&value_schema).unwrap_or(&value_schema);
                if call.args.args.len() != 2 {
                    return Err("Map.insert takes key and value".into());
                }
                let key = self.method_arg(&call.args.args[0], Some(&key_schema))?;
                let value = self.map_insert_value(&call.args.args[1], logical_value_schema)?;
                self.map_insert(&receiver, key, value, &key_schema, logical_value_schema)
            }
            "get" => {
                if self.tables.schemas.is_named_schema(&receiver.schema, "Doc") {
                    if call.args.args.len() != 1 {
                        return Err("Doc.get takes one key".into());
                    }
                    let key = self.method_arg(&call.args.args[0], Some("String"))?;
                    return self.doc_get(&receiver, &key);
                }
                let Some((key_schema, value_schema)) =
                    self.tables.schemas.map_schema_names(&receiver.schema)
                else {
                    return Err(format!("get called on {}", receiver.schema));
                };
                let logical_value_schema =
                    realized_value_schema(&value_schema).unwrap_or(&value_schema);
                let result_value_schema = realized_schema(logical_value_schema);
                if call.args.args.len() != 1 {
                    return Err("Map.get takes one key".into());
                }
                let key = self.method_arg(&call.args.args[0], Some(&key_schema))?;
                self.map_get(&receiver, key, &key_schema, &result_value_schema)
            }
            "package" => {
                if call.args.args.len() != 1 {
                    return Err("Doc.package takes one package name".into());
                }
                let receiver = self.coerce_to_schema(receiver, "Doc")?;
                let name = self.method_arg(&call.args.args[0], Some("String"))?;
                self.doc_package(&receiver, &name)
            }
            "fn" => {
                if call.args.args.len() != 1 {
                    return Err("Doc.fn takes one name".into());
                }
                let receiver = self.coerce_to_schema(receiver, "Doc")?;
                let name = self.method_arg(&call.args.args[0], Some("String"))?;
                self.ast_fn(&receiver, &name)
            }
            "union" | "intersect" => {
                if !self
                    .tables
                    .schemas
                    .is_external(&receiver.schema, "VersionSet")
                {
                    return Err(format!("{} called on {}", call.name.value, receiver.schema));
                }
                let [arg] = call.args.args.as_slice() else {
                    return Err(format!(
                        "VersionSet.{} takes one VersionSet",
                        call.name.value
                    ));
                };
                let right = self.method_arg(arg, Some("VersionSet"))?;
                let op = if call.name.value == "union" { 0 } else { 1 };
                self.version_set_op(op, &receiver, Some(&right), "VersionSet")
            }
            "complement" => {
                if !self
                    .tables
                    .schemas
                    .is_external(&receiver.schema, "VersionSet")
                {
                    return Err(format!("complement called on {}", receiver.schema));
                }
                if !call.args.args.is_empty() {
                    return Err("VersionSet.complement takes no arguments".into());
                }
                self.version_set_op(2, &receiver, None, "VersionSet")
            }
            "subset" => {
                if !self
                    .tables
                    .schemas
                    .is_external(&receiver.schema, "VersionSet")
                {
                    return Err(format!("subset called on {}", receiver.schema));
                }
                let [arg] = call.args.args.as_slice() else {
                    return Err("VersionSet.subset takes one VersionSet".into());
                };
                let right = self.method_arg(arg, Some("VersionSet"))?;
                self.version_set_op(3, &receiver, Some(&right), "Bool")
            }
            "contains"
                if self
                    .tables
                    .schemas
                    .is_primitive(&receiver.schema, Primitive::String) =>
            {
                let [arg] = call.args.args.as_slice() else {
                    return Err("String.contains takes one needle".into());
                };
                let needle = self.method_arg(arg, Some("String"))?;
                Ok(self.string_query(&receiver, Some(&needle), STRING_CONTAINS_HOST))
            }
            "contains" => {
                if !self
                    .tables
                    .schemas
                    .is_external(&receiver.schema, "VersionSet")
                {
                    return Err(format!("contains called on {}", receiver.schema));
                }
                let [arg] = call.args.args.as_slice() else {
                    return Err("VersionSet.contains takes one Version".into());
                };
                let right = self.method_arg(arg, Some("Version"))?;
                self.version_set_op(4, &receiver, Some(&right), "Bool")
            }
            "unwrap" => {
                if !call.args.args.is_empty() {
                    return Err("Option.unwrap takes no arguments".into());
                }
                let Some(value_schema) = self
                    .tables
                    .schemas
                    .option_value_schema_name(&receiver.schema)
                else {
                    return Err(format!("unwrap called on {}", receiver.schema));
                };
                Ok(self.option_unwrap(&receiver, &value_schema))
            }
            "before" | "after" | "strip_prefix" => {
                if !self
                    .tables
                    .schemas
                    .is_primitive(&receiver.schema, Primitive::String)
                {
                    return Err(format!("{} called on {}", call.name.value, receiver.schema));
                }
                let [arg] = call.args.args.as_slice() else {
                    return Err(format!("String.{} takes one argument", call.name.value));
                };
                let delim = self.method_arg(arg, Some("String"))?;
                let selector = match call.name.value.as_str() {
                    "before" => 0,
                    "after" => 1,
                    _ => 2,
                };
                Ok(self.string_split(&receiver, &delim, selector))
            }
            "parse_int" => {
                if !self
                    .tables
                    .schemas
                    .is_primitive(&receiver.schema, Primitive::String)
                {
                    return Err(format!("parse_int called on {}", receiver.schema));
                }
                if !call.args.args.is_empty() {
                    return Err("String.parse_int takes no arguments".into());
                }
                Ok(self.string_parse_int(&receiver))
            }
            "is_numeric" => {
                if !self
                    .tables
                    .schemas
                    .is_primitive(&receiver.schema, Primitive::String)
                {
                    return Err(format!("is_numeric called on {}", receiver.schema));
                }
                if !call.args.args.is_empty() {
                    return Err("String.is_numeric takes no arguments".into());
                }
                Ok(self.string_query(&receiver, None, STRING_IS_NUMERIC_HOST))
            }
            other => Err(format!(
                "method {other} is outside the machine slice-3 subset"
            )),
        }
    }

    fn method_receiver(&mut self, call: &ast::MethodCall) -> Result<(ValueSlot, bool), String> {
        if aggregate_update_method(call.name.value.as_str())
            && let Some(name) = plain_identifier_expr(&call.receiver)
            && self.consume_receiver.as_deref() == Some(name)
            && !self.pending_lazy_alias_reads(name)
        {
            return Ok((self.resolve_binding_consuming_move(name, None)?, true));
        }
        Ok((self.expr(&call.receiver)?, false))
    }

    fn method_arg(&mut self, arg: &ast::Arg, expected: Option<&str>) -> Result<ValueSlot, String> {
        match arg {
            ast::Arg::Expr(expr) => {
                let value = self.expr_expected(expr, expected)?;
                if let Some(expected) = expected {
                    self.coerce_to_schema(value, expected)
                } else {
                    Ok(value)
                }
            }
            other => Err(format!(
                "method argument {other:?} is outside the machine slice-3 subset"
            )),
        }
    }

    fn map_insert_value(
        &mut self,
        arg: &ast::Arg,
        value_schema: &str,
    ) -> Result<ValueSlot, String> {
        match arg {
            ast::Arg::Expr(ast::Expr::Call(call)) => self.pending_call_value(call, value_schema),
            ast::Arg::Expr(expr) => {
                let value = self.expr_expected(expr, Some(value_schema))?;
                if value.schema == realized_schema(value_schema)
                    || value.schema == pending_schema(value_schema)
                {
                    Ok(value)
                } else {
                    self.coerce_to_schema(value, value_schema)
                }
            }
            other => Err(format!(
                "method argument {other:?} is outside the machine slice-3 subset"
            )),
        }
    }

    fn map_literal(
        &mut self,
        map: &ast::MapLiteral,
        expected: Option<&str>,
    ) -> Result<ValueSlot, String> {
        let schema = expected
            .filter(|schema| self.tables.schemas.is_map(schema))
            .unwrap_or("Map");
        let mut value = self.map_empty(schema)?;
        let (key_schema, value_schema) = self
            .tables
            .schemas
            .map_schema_names(schema)
            .map(|(key, value)| (Some(key), Some(value)))
            .unwrap_or((None, None));
        for entry in &map.entries {
            let key = self.expr_expected(&entry.key, key_schema.as_deref())?;
            let item = self.expr_expected(&entry.value, value_schema.as_deref())?;
            let key_schema = key_schema.clone().unwrap_or_else(|| key.schema.clone());
            let value_schema = value_schema.clone().unwrap_or_else(|| item.schema.clone());
            value = self.map_insert(&value, key, item, &key_schema, &value_schema)?;
        }
        Ok(value)
    }

    fn scoped_value(&mut self, path: &ast::ScopedIdentifier) -> Result<ValueSlot, String> {
        let (enum_name, variant_index, shape) = self.resolve_scoped_variant(path)?;
        match shape {
            VariantShape::Unit => self.store_alloc(&enum_name, variant_index, &[]),
            other => Err(format!(
                "scoped value {enum_name}::{variant_index} has payload shape {other:?}; use call or struct literal"
            )),
        }
    }

    fn tuple_literal(
        &mut self,
        tuple: &ast::TupleExpr,
        expected: Option<&str>,
    ) -> Result<ValueSlot, String> {
        let expected_fields = expected.and_then(tuple_schema_fields);
        let mut fields = Vec::new();
        let mut schemas = Vec::new();
        for (index, elem) in tuple.elems.iter().enumerate() {
            let expected = expected_fields
                .as_ref()
                .and_then(|fields| fields.get(index).map(String::as_str));
            let value = self.expr_expected(elem, expected)?;
            let value = if let Some(expected) = expected {
                self.coerce_to_schema(value, expected)?
            } else {
                value
            };
            schemas.push(value.schema.clone());
            fields.push(value);
        }
        self.store_alloc(&tuple_schema(&schemas), 0, &fields)
    }

    fn field_access(
        &mut self,
        field: &ast::FieldAccess,
        expected: Option<&str>,
    ) -> Result<ValueSlot, String> {
        let receiver = self.expr(&field.receiver)?;
        match &field.name {
            ast::Member::Index(index) => {
                let field_index = index
                    .value
                    .parse::<usize>()
                    .map_err(|_| format!("tuple index {} does not parse", index.value))?;
                let fields = tuple_schema_fields(&receiver.schema)
                    .ok_or_else(|| format!("tuple indexing on {}", receiver.schema))?;
                let schema = fields
                    .get(field_index)
                    .ok_or_else(|| format!("tuple {} has no field {field_index}", receiver.schema))?
                    .clone();
                Ok(self.store_read(&receiver, field_index, schema))
            }
            ast::Member::Identifier(name) => {
                if self.value_schema_is_realized_named(&receiver.schema, "Doc") {
                    let receiver = self.coerce_to_schema(receiver, "Doc")?;
                    return self.doc_field_access(receiver, name.value.as_str(), expected);
                }
                if self.tables.schemas.is_named_schema(&receiver.schema, "Doc") {
                    return self.doc_field_access(receiver, name.value.as_str(), expected);
                }
                if self
                    .tables
                    .schemas
                    .is_named_schema(&receiver.schema, "Target")
                    && name.value == "os"
                {
                    return Ok(self.store_read(&receiver, 0, "Os".into()));
                }
                if self
                    .tables
                    .schemas
                    .is_named_schema(&receiver.schema, "Target")
                    && name.value == "arch"
                {
                    return Ok(self.store_read(&receiver, 1, "Arch".into()));
                }
                let info = self
                    .tables
                    .structs
                    .get(&receiver.schema)
                    .ok_or_else(|| format!("field access on {}", receiver.schema))?;
                let field_index = info
                    .fields
                    .iter()
                    .position(|(field_name, _)| field_name == &name.value)
                    .ok_or_else(|| {
                        format!("unknown field {} on {}", name.value, receiver.schema)
                    })?;
                let schema = self.struct_field_schema(&receiver.schema, field_index)?;
                Ok(self.store_read(&receiver, field_index, schema))
            }
        }
    }

    fn doc_field_access(
        &mut self,
        receiver: ValueSlot,
        name: &str,
        expected: Option<&str>,
    ) -> Result<ValueSlot, String> {
        let key = *self
            .literal_handles
            .strings
            .get(name)
            .ok_or_else(|| format!("doc field key {name:?} was not interned"))?;
        let key_slot = self.alloc();
        self.code.push(Op::ConstI64 {
            dst: key_slot,
            value: key,
        });
        let key = ValueSlot {
            slot: key_slot,
            schema: "String".into(),
            realization: None,
            pending: None,
        };
        let option = self.doc_get(&receiver, &key)?;
        let value = self.option_unwrap(&option, "Realized<Doc>");
        let Some(expected) = expected else {
            return Ok(value);
        };
        if realized_value_schema(expected)
            .is_some_and(|schema| self.tables.schemas.is_named_schema(schema, "Doc"))
        {
            return Ok(value);
        }
        let doc = self.coerce_to_schema(value, "Doc")?;
        self.coerce_to_schema(doc, expected)
    }

    fn struct_literal(&mut self, lit: &ast::StructLiteral) -> Result<ValueSlot, String> {
        let path = path_ref_segments(&lit.path)?;
        if path.len() == 1 {
            let name = &path[0];
            if name == "Target" {
                return self.target_literal(lit);
            }
            let info = self
                .tables
                .structs
                .get(name)
                .ok_or_else(|| format!("unknown struct {name}"))?
                .clone();
            if info.is_unit {
                return self.store_alloc(name, 0, &[]);
            }
            let mut explicit = HashMap::new();
            for field in &lit.fields {
                if explicit
                    .insert(field.name.value.clone(), &field.value)
                    .is_some()
                {
                    return Err(format!(
                        "duplicate field `{}` in struct literal `{name}`",
                        field.name.value
                    ));
                }
            }
            let base = match lit.spreads.as_slice() {
                [] => None,
                [spread] => {
                    let Some(base) = &spread.base else {
                        return Err("record update spread requires a base expression".into());
                    };
                    Some(self.expr_expected(base, Some(name))?)
                }
                _ => return Err("multiple record update spreads are outside the B3 subset".into()),
            };
            if let Some(base) = base {
                let mut updates = Vec::new();
                for (field_index, (field_name, _)) in info.fields.iter().enumerate() {
                    let Some(init) = explicit.get(field_name) else {
                        continue;
                    };
                    let field_schema = self.struct_field_schema(name, field_index)?;
                    let value = self.expr_expected(init, Some(&field_schema))?;
                    updates.push((field_index, value));
                }
                return self.record_update(name, 0, &base, &updates);
            }
            let mut fields = Vec::new();
            for (field_index, (field_name, default)) in info.fields.iter().enumerate() {
                let field_schema = self.struct_field_schema(name, field_index)?;
                let value = if let Some(init) = explicit.get(field_name) {
                    self.expr_expected(init, Some(&field_schema))?
                } else if let Some(default) = default {
                    self.expr_expected(default, Some(&field_schema))?
                } else {
                    return Err(format!("missing field {field_name} for struct {name}"));
                };
                fields.push(value);
            }
            return self.store_alloc(name, 0, &fields);
        }

        let (enum_name, variant_index, shape) = self.resolve_path_variant(&lit.path)?;
        let VariantShape::Record(field_names) = shape else {
            return Err(format!(
                "struct literal syntax used for non-record variant {enum_name}"
            ));
        };
        let mut fields = Vec::new();
        for field_name in &field_names {
            let init = lit
                .fields
                .iter()
                .find(|field| &field.name.value == field_name)
                .ok_or_else(|| format!("missing field {field_name} for variant {enum_name}"))?;
            fields.push(self.expr(&init.value)?);
        }
        self.store_alloc(&enum_name, variant_index, &fields)
    }

    fn target_literal(&mut self, lit: &ast::StructLiteral) -> Result<ValueSlot, String> {
        if lit.spreads.len() > 1 {
            return Err("multiple Target record update spreads are outside the B3 subset".into());
        }
        let mut os = None;
        let mut arch = None;
        for field in &lit.fields {
            match field.name.value.as_str() {
                "os" => {
                    if os
                        .replace(self.expr_expected(&field.value, Some("Os"))?)
                        .is_some()
                    {
                        return Err("duplicate field `os` in struct literal `Target`".into());
                    }
                }
                "arch" => {
                    if arch
                        .replace(self.expr_expected(&field.value, Some("Arch"))?)
                        .is_some()
                    {
                        return Err("duplicate field `arch` in struct literal `Target`".into());
                    }
                }
                other => return Err(format!("unknown field {other} on Target")),
            }
        }
        let base = match lit.spreads.as_slice() {
            [] => None,
            [spread] => {
                let Some(base) = &spread.base else {
                    return Err("Target record update spread requires a base expression".into());
                };
                Some(self.expr_expected(base, Some("Target"))?)
            }
            _ => unreachable!("length checked above"),
        };
        let os = if let Some(os) = os {
            os
        } else if let Some(base) = &base {
            self.store_read(base, 0, "Os".into())
        } else {
            return Err("missing field os for struct Target".into());
        };
        let arch = if let Some(arch) = arch {
            arch
        } else if let Some(base) = &base {
            self.store_read(base, 1, "Arch".into())
        } else {
            let host = self.target_host();
            self.store_read(&host, 1, "Arch".into())
        };
        self.store_alloc("Target", 0, &[os, arch])
    }

    fn struct_field_schema(&self, struct_name: &str, field_index: usize) -> Result<String, String> {
        let descriptor = self
            .tables
            .descriptors
            .get(struct_name)
            .ok_or_else(|| format!("missing descriptor for {struct_name}"))?;
        descriptor_field_schema(&self.tables.schemas, descriptor, field_index)
    }

    fn variant_constructor_call(&mut self, call: &ast::Call) -> Result<Option<ValueSlot>, String> {
        let Ok((enum_name, variant_index, shape)) = self.resolve_path_variant(&call.callee) else {
            return Ok(None);
        };
        let VariantShape::Tuple(expected) = shape else {
            return Err(format!(
                "call syntax used for non-tuple variant {enum_name}"
            ));
        };
        if call.args.args.len() != expected {
            return Err(format!(
                "variant constructor expected {expected} args, got {}",
                call.args.args.len()
            ));
        }
        let mut fields = Vec::new();
        for arg in &call.args.args {
            let ast::Arg::Expr(expr) = arg else {
                return Err(
                    "variant constructor kwargs are outside the machine slice-2 subset".into(),
                );
            };
            fields.push(self.expr(expr)?);
        }
        self.store_alloc(&enum_name, variant_index, &fields)
            .map(Some)
    }

    fn resolve_scoped_variant(
        &self,
        path: &ast::ScopedIdentifier,
    ) -> Result<(String, usize, VariantShape), String> {
        let segments: Vec<String> = path.segments.iter().map(|s| s.value.clone()).collect();
        resolve_variant_segments(self.tables, &segments)
    }

    fn resolve_path_variant(
        &self,
        path: &ast::PathRef,
    ) -> Result<(String, usize, VariantShape), String> {
        let segments = path_ref_segments(path)?;
        resolve_variant_segments(self.tables, &segments)
    }

    fn variant_match_test(
        &mut self,
        scrut: &ValueSlot,
        enum_name: &str,
        variant_index: usize,
        skip_patches: &mut Vec<usize>,
    ) -> Result<(), String> {
        self.expect_schema(scrut, enum_name)?;
        let tag = self.store_tag(scrut);
        let lit = self.alloc();
        self.code.push(Op::ConstI64 {
            dst: lit,
            value: i64::try_from(variant_index).expect("variant index fits i64"),
        });
        let test = self.alloc();
        self.code.push(Op::EqI64 {
            dst: test,
            a: tag.slot,
            b: lit,
        });
        skip_patches.push(self.code.len());
        self.code.push(Op::JumpIfZero {
            value: test,
            target: 0,
        });
        Ok(())
    }

    fn shorthand_unit_variant(&self, enum_name: &str, variant_name: &str) -> Option<usize> {
        if let Some(info) = self.tables.enums.get(enum_name) {
            return info.variants.iter().position(|(name, shape)| {
                name == variant_name && matches!(shape, VariantShape::Unit)
            });
        }
        builtin_unit_variant_index(enum_name, variant_name)
    }

    fn bind_payload_pattern(
        &mut self,
        scrut: &ValueSlot,
        variant_index: usize,
        field_index: usize,
        pattern: &ast::Pattern,
    ) -> Result<(), String> {
        let schema = self.variant_field_schema(&scrut.schema, variant_index, field_index)?;
        match pattern {
            ast::Pattern::Identifier(name) => {
                let value = self.store_read(scrut, field_index, schema);
                self.slots
                    .insert(name.value.clone(), BindingCell::value(value));
                Ok(())
            }
            ast::Pattern::Wildcard(_) => Ok(()),
            other => Err(format!(
                "nested pattern {other:?} is outside the machine slice-2 subset"
            )),
        }
    }

    fn variant_field_schema(
        &self,
        enum_name: &str,
        variant_index: usize,
        field_index: usize,
    ) -> Result<String, String> {
        let descriptor = self
            .tables
            .descriptors
            .get(enum_name)
            .ok_or_else(|| format!("missing descriptor for {enum_name}"))?;
        let weavy::mem::Access::Enum(access) = &descriptor.access else {
            return Err(format!("{enum_name} is not an enum descriptor"));
        };
        let field = access
            .variants
            .get(variant_index)
            .and_then(|variant| variant.payload.fields.get(field_index))
            .ok_or_else(|| format!("missing payload field {field_index} for {enum_name}"))?;
        match &field.descriptor.access {
            weavy::mem::Access::Handle { target } => Ok(self.tables.schemas.display_ref(target)),
            _ => Ok(self.tables.schemas.display_ref(&field.descriptor.schema)),
        }
    }

    fn coerce_inner(&mut self, value: ValueSlot) -> Result<ValueSlot, String> {
        let Some(inner) = value_schema_inside_barrier(&value.schema).map(str::to_string) else {
            return Ok(value);
        };
        self.coerce_to_schema(value, &inner)
    }

    fn coerce_to_schema(&mut self, value: ValueSlot, expected: &str) -> Result<ValueSlot, String> {
        if value.schema == expected {
            return Ok(value);
        }
        if pending_value_schema(&value.schema) == Some(expected) {
            return self.coerce_pending_to_schema(value, expected);
        }
        if realized_value_schema(&value.schema) == Some(expected) {
            return self.coerce_realized_to_schema(value, expected);
        }
        if self.value_schema_is_realized_named(&value.schema, "Sealed")
            && self
                .tables
                .schemas
                .is_primitive(expected, Primitive::String)
        {
            let sealed = self.coerce_realized_to_schema(value, "Sealed")?;
            return self.sealed_to_string(&sealed);
        }
        if self.value_schema_is_realized_named(&value.schema, "Doc")
            && !realized_value_schema(expected)
                .is_some_and(|schema| self.tables.schemas.is_named_schema(schema, "Doc"))
        {
            let doc = self.coerce_realized_to_schema(value, "Doc")?;
            return self.coerce_to_schema(doc, expected);
        }
        if self.tables.schemas.is_external(&value.schema, "Sealed")
            && self
                .tables
                .schemas
                .is_primitive(expected, Primitive::String)
        {
            return self.sealed_to_string(&value);
        }
        if self.tables.schemas.is_named_schema(&value.schema, "Doc")
            && self.schema_accepts_doc_coercion(expected)
        {
            return self.coerce_doc_to_schema(value, expected);
        }
        Ok(value)
    }

    fn schema_accepts_doc_coercion(&self, schema: &str) -> bool {
        self.tables.schemas.is_primitive(schema, Primitive::String)
            || self.tables.schemas.is_primitive(schema, Primitive::I64)
            || self.tables.schemas.is_primitive(schema, Primitive::Bool)
            || self.tables.schemas.is_primitive(schema, Primitive::F64)
            || self.tables.schemas.is_primitive(schema, Primitive::Bytes)
            || self.tables.schemas.is_list(schema)
            || self.tables.schemas.map_schema_names(schema).is_some_and(
                |(key_schema, value_schema)| {
                    self.tables
                        .schemas
                        .is_primitive(&key_schema, Primitive::String)
                        && self.tables.schemas.is_named_schema(&value_schema, "Doc")
                },
            )
    }

    fn coerce_doc_to_schema(
        &mut self,
        value: ValueSlot,
        expected: &str,
    ) -> Result<ValueSlot, String> {
        self.expect_schema(&value, "Doc")?;
        let schema_ref = *self
            .schema_words
            .get(expected)
            .ok_or_else(|| format!("no schema ref for {expected}"))?;
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: value.slot,
        });
        self.code.push(Op::ConstI64 {
            dst: region + 16,
            value: schema_ref,
        });
        self.code.push(Op::HostCall {
            host: DOC_COERCE_HOST,
        });
        Ok(ValueSlot {
            slot: dst,
            schema: expected.to_string(),
            realization: None,
            pending: None,
        })
    }

    fn coerce_pending_to_schema(
        &mut self,
        value: ValueSlot,
        expected: &str,
    ) -> Result<ValueSlot, String> {
        if value.schema != pending_schema(expected) {
            return Ok(value);
        }
        let input_slot = self.next_input_slot;
        self.next_input_slot += 1;
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: input_slot,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: value.slot,
        });
        self.code.push(Op::HostCall {
            host: PENDING_COERCE_HOST,
        });
        let dst = self.alloc();
        self.code.push(Op::Await {
            dst,
            input: u32::try_from(input_slot).expect("input slot fits u32"),
        });
        Ok(ValueSlot {
            slot: dst,
            schema: expected.to_string(),
            realization: None,
            pending: None,
        })
    }

    fn coerce_realized_to_schema(
        &mut self,
        value: ValueSlot,
        expected: &str,
    ) -> Result<ValueSlot, String> {
        self.expect_schema(&value, &realized_schema(expected))?;
        let Some(flag) = value.realization else {
            return Err(format!(
                "{} arrived without a frame realization flag",
                value.schema
            ));
        };
        let pending_jump = self.code.len();
        self.code.push(Op::JumpIfZero {
            value: flag,
            target: 0,
        });

        let pending = ValueSlot {
            slot: value.slot,
            schema: pending_schema(expected),
            realization: None,
            pending: None,
        };
        let forced = self.coerce_pending_to_schema(pending, expected)?;
        self.code.push(Op::CopyI64 {
            dst: value.slot,
            src: forced.slot,
        });
        self.code.push(Op::ConstI64 {
            dst: flag,
            value: 0,
        });
        let end_jump = self.code.len();
        self.code.push(Op::Jump { target: 0 });

        let ready_target = u32::try_from(self.code.len()).expect("code len fits u32");
        self.code[pending_jump] = Op::JumpIfZero {
            value: flag,
            target: ready_target,
        };

        let end = u32::try_from(self.code.len()).expect("code len fits u32");
        self.code[end_jump] = Op::Jump { target: end };
        Ok(ValueSlot {
            slot: value.slot,
            schema: expected.to_string(),
            realization: None,
            pending: None,
        })
    }

    fn render_call(&mut self, call: &ast::Call) -> Result<ValueSlot, String> {
        let [template_arg, bindings_arg] = call.args.args.as_slice() else {
            return Err("render takes a template and bindings".into());
        };
        let ast::Arg::Expr(ast::Expr::Template(template)) = template_arg else {
            return Err(
                "render's first argument must be a tmpl\"...\" literal in this rung".into(),
            );
        };
        let bindings = self.method_arg(bindings_arg, None)?;
        let source = decode_template_literal(&template.value)?;
        let parts = parse_template(&source)?;
        let mut out = self.string_literal("")?;
        for part in parts {
            let fragment = match part {
                TemplatePart::Text(text) => self.string_literal(&text)?,
                TemplatePart::Hole(hole) => self.template_hole(&bindings, &hole)?,
            };
            out = self.string_concat(&out, &fragment)?;
        }
        Ok(out)
    }

    fn template_hole(
        &mut self,
        bindings: &ValueSlot,
        hole: &TemplateHole,
    ) -> Result<ValueSlot, String> {
        let mut value = self.template_binding(bindings, &hole.name)?;
        value = self.coerce_to_schema(value, "String")?;
        self.expect_schema(&value, "String")?;
        for filter in &hole.filters {
            value = match filter {
                TemplateFilter::Upper => self.string_upper(&value)?,
                TemplateFilter::Lower => self.string_lower(&value)?,
                TemplateFilter::Default(default) => {
                    let default = self.string_literal(default)?;
                    self.string_default(&value, &default)?
                }
            };
        }
        Ok(value)
    }

    fn template_binding(&mut self, bindings: &ValueSlot, name: &str) -> Result<ValueSlot, String> {
        if self.value_schema_is_realized_named(&bindings.schema, "Doc") {
            let doc = self.coerce_to_schema(bindings.clone(), "Doc")?;
            return self.doc_field_access(doc, name, Some("String"));
        }
        if self.tables.schemas.is_named_schema(&bindings.schema, "Doc") {
            return self.doc_field_access(bindings.clone(), name, Some("String"));
        }
        if let Some((key_schema, value_schema)) =
            self.tables.schemas.map_schema_names(&bindings.schema)
        {
            if !self
                .tables
                .schemas
                .is_primitive(&key_schema, Primitive::String)
            {
                return Err(format!(
                    "template bindings map keys must be String, got {key_schema}"
                ));
            }
            let key = self.string_literal(name)?;
            let logical_value_schema =
                realized_value_schema(&value_schema).unwrap_or(&value_schema);
            let result_value_schema = realized_schema(logical_value_schema);
            let option = self.map_get(bindings, key, "String", &result_value_schema)?;
            return Ok(self.option_unwrap(&option, &result_value_schema));
        }
        if let Some(info) = self.tables.structs.get(&bindings.schema) {
            let field_index = info
                .fields
                .iter()
                .position(|(field_name, _)| field_name == name)
                .ok_or_else(|| {
                    format!("template binding `{name}` missing on {}", bindings.schema)
                })?;
            let schema = self.struct_field_schema(&bindings.schema, field_index)?;
            return Ok(self.store_read(bindings, field_index, schema));
        }
        Err(format!(
            "template bindings must be a Map, Doc, or record struct, got {}",
            bindings.schema
        ))
    }

    fn string_literal(&mut self, value: &str) -> Result<ValueSlot, String> {
        let value = *self
            .literal_handles
            .strings
            .get(value)
            .ok_or_else(|| format!("string literal {value:?} was not interned"))?;
        let dst = self.alloc();
        self.code.push(Op::ConstI64 { dst, value });
        Ok(ValueSlot {
            slot: dst,
            schema: "String".into(),
            realization: None,
            pending: None,
        })
    }

    fn string_default(
        &mut self,
        value: &ValueSlot,
        default: &ValueSlot,
    ) -> Result<ValueSlot, String> {
        self.string_host2(value, default, STRING_DEFAULT_HOST)
    }

    fn string_host2(
        &mut self,
        left: &ValueSlot,
        right: &ValueSlot,
        host: u32,
    ) -> Result<ValueSlot, String> {
        self.expect_schema(left, "String")?;
        self.expect_schema(right, "String")?;
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: left.slot,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 16,
            src: right.slot,
        });
        self.code.push(Op::HostCall { host });
        Ok(ValueSlot {
            slot: dst,
            schema: "String".into(),
            realization: None,
            pending: None,
        })
    }

    fn string_upper(&mut self, value: &ValueSlot) -> Result<ValueSlot, String> {
        self.string_host1(value, STRING_UPPER_HOST)
    }

    fn string_lower(&mut self, value: &ValueSlot) -> Result<ValueSlot, String> {
        self.string_host1(value, STRING_LOWER_HOST)
    }

    fn string_host1(&mut self, value: &ValueSlot, host: u32) -> Result<ValueSlot, String> {
        self.expect_schema(value, "String")?;
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: value.slot,
        });
        self.code.push(Op::HostCall { host });
        Ok(ValueSlot {
            slot: dst,
            schema: "String".into(),
            realization: None,
            pending: None,
        })
    }

    fn expect_schema(&self, value: &ValueSlot, expected: &str) -> Result<u32, String> {
        if value.schema == expected
            || (expected == "Array" && self.tables.schemas.is_list(&value.schema))
        {
            Ok(value.slot)
        } else {
            Err(format!(
                "expected {expected}, got {} in the machine slice-2 subset",
                value.schema
            ))
        }
    }

    fn store_alloc(
        &mut self,
        schema: &str,
        variant_index: usize,
        fields: &[ValueSlot],
    ) -> Result<ValueSlot, String> {
        let dst = self.alloc();
        let region = self.store_alloc_region;
        let type_ref = *self
            .schema_words
            .get(schema)
            .ok_or_else(|| format!("no schema ref for {schema}"))?;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::ConstI64 {
            dst: region + 8,
            value: type_ref,
        });
        self.code.push(Op::ConstI64 {
            dst: region + 16,
            value: i64::try_from(variant_index).expect("variant index fits i64"),
        });
        self.code.push(Op::ConstI64 {
            dst: region + 24,
            value: i64::try_from(fields.len()).expect("field count fits i64"),
        });
        for (i, field) in fields.iter().enumerate() {
            self.code.push(Op::CopyI64 {
                dst: region + 32 + 8 * u32::try_from(i).expect("field index fits u32"),
                src: field.slot,
            });
        }
        self.code.push(Op::HostCall {
            host: STORE_ALLOC_HOST,
        });
        Ok(ValueSlot {
            slot: dst,
            schema: schema.to_string(),
            realization: None,
            pending: None,
        })
    }

    fn record_update(
        &mut self,
        schema: &str,
        variant_index: usize,
        base: &ValueSlot,
        updates: &[(usize, ValueSlot)],
    ) -> Result<ValueSlot, String> {
        self.expect_schema(base, schema)?;
        let dst = self.alloc();
        let region = self.store_alloc_region;
        let schema_ref = *self
            .schema_words
            .get(schema)
            .ok_or_else(|| format!("no schema ref for {schema}"))?;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: base.slot,
        });
        self.code.push(Op::ConstI64 {
            dst: region + 16,
            value: schema_ref,
        });
        self.code.push(Op::ConstI64 {
            dst: region + 24,
            value: i64::try_from(variant_index).expect("variant index fits i64"),
        });
        self.code.push(Op::ConstI64 {
            dst: region + 32,
            value: i64::try_from(updates.len()).expect("update count fits i64"),
        });
        for (update_index, (field_index, value)) in updates.iter().enumerate() {
            let update_offset = 16 * u32::try_from(update_index).expect("update index fits u32");
            self.code.push(Op::ConstI64 {
                dst: region + 40 + update_offset,
                value: i64::try_from(*field_index).expect("field index fits i64"),
            });
            self.code.push(Op::CopyI64 {
                dst: region + 48 + update_offset,
                src: value.slot,
            });
        }
        self.code.push(Op::HostCall {
            host: RECORD_UPDATE_HOST,
        });
        Ok(ValueSlot {
            slot: dst,
            schema: schema.to_string(),
            realization: None,
            pending: None,
        })
    }

    fn store_read(&mut self, handle: &ValueSlot, field_index: usize, schema: String) -> ValueSlot {
        let dst = self.alloc();
        let region = self.store_read_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: handle.slot,
        });
        self.code.push(Op::ConstI64 {
            dst: region + 16,
            value: i64::try_from(field_index).expect("field index fits i64"),
        });
        self.code.push(Op::HostCall {
            host: STORE_READ_HOST,
        });
        ValueSlot {
            slot: dst,
            schema,
            realization: None,
            pending: None,
        }
    }

    fn store_tag(&mut self, handle: &ValueSlot) -> ValueSlot {
        let dst = self.alloc();
        let region = self.store_tag_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: handle.slot,
        });
        self.code.push(Op::HostCall {
            host: STORE_TAG_HOST,
        });
        ValueSlot {
            slot: dst,
            schema: "Int".into(),
            realization: None,
            pending: None,
        }
    }

    fn molten_dup(&mut self, value: &ValueSlot) -> Result<ValueSlot, String> {
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: value.slot,
        });
        self.code.push(Op::HostCall {
            host: MOLTEN_DUP_HOST,
        });
        Ok(ValueSlot {
            slot: dst,
            schema: value.schema.clone(),
            realization: value.realization,
            pending: value.pending,
        })
    }

    fn map_empty(&mut self, schema: &str) -> Result<ValueSlot, String> {
        let dst = self.alloc();
        let region = self.store_alloc_region;
        let schema_ref = *self
            .schema_words
            .get(schema)
            .ok_or_else(|| format!("no schema ref for {schema}"))?;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::ConstI64 {
            dst: region + 8,
            value: schema_ref,
        });
        self.code.push(Op::HostCall {
            host: MAP_EMPTY_HOST,
        });
        Ok(ValueSlot {
            slot: dst,
            schema: schema.to_string(),
            realization: None,
            pending: None,
        })
    }

    fn map_insert(
        &mut self,
        map: &ValueSlot,
        key: ValueSlot,
        value: ValueSlot,
        key_schema: &str,
        value_schema: &str,
    ) -> Result<ValueSlot, String> {
        self.expect_schema(&key, key_schema)?;
        let pending_value_schema = pending_schema(value_schema);
        let realized_value_schema = realized_schema(value_schema);
        if value.schema == pending_value_schema || value.schema == realized_value_schema {
            self.expect_schema(&value, &value.schema)?;
        } else {
            self.expect_schema(&value, value_schema)?;
        }
        let output_schema = if value.schema == pending_value_schema
            || map_value_schema(&map.schema).is_some_and(|schema| schema == realized_value_schema)
        {
            map_schema(key_schema, &realized_value_schema)
        } else {
            map.schema.clone()
        };
        let stored_value = if output_schema == map_schema(key_schema, &realized_value_schema) {
            self.realize_value(value, value_schema)
        } else {
            value
        };
        let dst = self.alloc();
        let region = self.store_alloc_region;
        let map_schema_ref = *self
            .schema_words
            .get(&output_schema)
            .ok_or_else(|| format!("no schema ref for {output_schema}"))?;
        let key_schema_ref = *self
            .schema_words
            .get(key_schema)
            .ok_or_else(|| format!("no schema ref for {key_schema}"))?;
        let value_schema_ref = *self
            .schema_words
            .get(&stored_value.schema)
            .ok_or_else(|| format!("no schema ref for {}", stored_value.schema))?;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: map.slot,
        });
        self.code.push(Op::ConstI64 {
            dst: region + 16,
            value: map_schema_ref,
        });
        self.code.push(Op::ConstI64 {
            dst: region + 24,
            value: key_schema_ref,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 32,
            src: key.slot,
        });
        self.code.push(Op::ConstI64 {
            dst: region + 40,
            value: value_schema_ref,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 48,
            src: stored_value.slot,
        });
        if let Some(flag) = stored_value.realization {
            self.code.push(Op::CopyI64 {
                dst: region + 56,
                src: flag,
            });
        } else {
            self.code.push(Op::ConstI64 {
                dst: region + 56,
                value: 0,
            });
        }
        self.code.push(Op::HostCall {
            host: MAP_INSERT_HOST,
        });
        Ok(ValueSlot {
            slot: dst,
            schema: output_schema,
            realization: None,
            pending: None,
        })
    }

    fn realize_value(&mut self, value: ValueSlot, value_schema: &str) -> ValueSlot {
        let realized = realized_schema(value_schema);
        if value.schema == realized {
            return value;
        }
        if value.schema == pending_schema(value_schema) {
            let flag = self.alloc();
            self.code.push(Op::ConstI64 {
                dst: flag,
                value: 1,
            });
            return ValueSlot {
                slot: value.slot,
                schema: realized,
                realization: Some(flag),
                pending: None,
            };
        }
        let flag = self.alloc();
        self.code.push(Op::ConstI64 {
            dst: flag,
            value: 0,
        });
        ValueSlot {
            slot: value.slot,
            schema: realized,
            realization: Some(flag),
            pending: None,
        }
    }

    fn map_get(
        &mut self,
        map: &ValueSlot,
        key: ValueSlot,
        key_schema: &str,
        value_schema: &str,
    ) -> Result<ValueSlot, String> {
        self.expect_schema(&key, key_schema)?;
        let dst = self.alloc();
        let region = self.store_alloc_region;
        let key_schema_ref = *self
            .schema_words
            .get(key_schema)
            .ok_or_else(|| format!("no schema ref for {key_schema}"))?;
        let value_schema_ref = *self
            .schema_words
            .get(value_schema)
            .ok_or_else(|| format!("no schema ref for {value_schema}"))?;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: map.slot,
        });
        self.code.push(Op::ConstI64 {
            dst: region + 16,
            value: value_schema_ref,
        });
        self.code.push(Op::ConstI64 {
            dst: region + 24,
            value: key_schema_ref,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 32,
            src: key.slot,
        });
        self.code.push(Op::HostCall { host: MAP_GET_HOST });
        Ok(ValueSlot {
            slot: dst,
            schema: option_schema(value_schema),
            realization: None,
            pending: None,
        })
    }

    fn option_unwrap(&mut self, option: &ValueSlot, value_schema: &str) -> ValueSlot {
        let input_slot = self.next_input_slot;
        self.next_input_slot += 1;
        let realization_slot = realized_value_schema(value_schema).map(|_| {
            let slot = self.next_input_slot;
            self.next_input_slot += 1;
            slot
        });
        let region = self.store_alloc_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: input_slot,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: option.slot,
        });
        self.code.push(Op::ConstI64 {
            dst: region + 16,
            value: -1,
        });
        if let Some(realization_slot) = realization_slot {
            self.code.push(Op::ConstI64 {
                dst: region + 16,
                value: realization_slot,
            });
        }
        self.code.push(Op::HostCall {
            host: OPTION_UNWRAP_HOST,
        });
        let dst = self.alloc();
        self.code.push(Op::Await {
            dst,
            input: u32::try_from(input_slot).expect("input slot fits u32"),
        });
        let realization = realization_slot.map(|input| {
            let flag = self.alloc();
            self.code.push(Op::Await {
                dst: flag,
                input: u32::try_from(input).expect("input slot fits u32"),
            });
            flag
        });
        ValueSlot {
            slot: dst,
            schema: value_schema.to_string(),
            realization,
            pending: None,
        }
    }

    fn pending_call_value(
        &mut self,
        call: &ast::Call,
        value_schema: &str,
    ) -> Result<ValueSlot, String> {
        let name = match &call.callee {
            ast::PathRef::Identifier(name) => &name.value,
            _ => return self.call(call),
        };
        let fn_ref = *self
            .fn_refs
            .get(name)
            .ok_or_else(|| format!("unknown function {name}"))?;
        let return_schema = self
            .signatures
            .returns
            .get(name)
            .ok_or_else(|| format!("unknown function {name}"))?
            .clone();
        if return_schema != value_schema {
            return Err(format!(
                "pending call {name} returns {return_schema}, expected {value_schema}"
            ));
        }
        let param_names = self
            .signatures
            .param_names
            .get(name)
            .ok_or_else(|| format!("missing param names for {name}"))?
            .clone();
        let param_schemas = self
            .signatures
            .params
            .get(name)
            .ok_or_else(|| format!("missing param schemas for {name}"))?
            .clone();
        let bound = self.bind_call_args(BindCallSpec {
            fn_name: name,
            param_names: &param_names,
            param_schemas: &param_schemas,
            args: &call.args,
            start: 0,
            allow_partial: false,
            tail_identifier_uses: None,
        })?;
        self.pending_alloc_for_fn(fn_ref, value_schema, bound.args, None)
    }

    fn pending_alloc_for_fn(
        &mut self,
        fn_ref: usize,
        value_schema: &str,
        arg_slots: Vec<ValueSlot>,
        pending: Option<PendingSlot>,
    ) -> Result<ValueSlot, String> {
        let dst = self.alloc();
        let region = self.primitive_region;
        let value_schema_ref = *self
            .schema_words
            .get(value_schema)
            .ok_or_else(|| format!("no schema ref for {value_schema}"))?;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::ConstI64 {
            dst: region + 8,
            value: value_schema_ref,
        });
        self.code.push(Op::ConstI64 {
            dst: region + 16,
            value: i64::try_from(fn_ref).expect("fn ref fits i64"),
        });
        self.code.push(Op::ConstI64 {
            dst: region + 24,
            value: i64::try_from(arg_slots.len()).expect("argc fits i64"),
        });
        for (index, slot) in arg_slots.iter().enumerate() {
            self.code.push(Op::CopyI64 {
                dst: region + 32 + 8 * u32::try_from(index).expect("arg index fits u32"),
                src: slot.slot,
            });
        }
        self.code.push(Op::HostCall {
            host: PENDING_ALLOC_HOST,
        });
        Ok(ValueSlot {
            slot: dst,
            schema: pending_schema(value_schema),
            realization: None,
            pending,
        })
    }

    fn pending_invoke(
        &mut self,
        pending: ValueSlot,
        arg_slots: Vec<ValueSlot>,
        value_schema: &str,
    ) -> Result<ValueSlot, String> {
        let input_slot = self.next_input_slot;
        self.next_input_slot += 1;
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: input_slot,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: pending.slot,
        });
        self.code.push(Op::ConstI64 {
            dst: region + 16,
            value: i64::try_from(arg_slots.len()).expect("argc fits i64"),
        });
        for (index, slot) in arg_slots.iter().enumerate() {
            self.code.push(Op::CopyI64 {
                dst: region + 24 + 8 * u32::try_from(index).expect("arg index fits u32"),
                src: slot.slot,
            });
        }
        self.code.push(Op::HostCall {
            host: PENDING_INVOKE_HOST,
        });
        let dst = self.alloc();
        self.code.push(Op::Await {
            dst,
            input: u32::try_from(input_slot).expect("input slot fits u32"),
        });
        Ok(ValueSlot {
            slot: dst,
            schema: value_schema.to_string(),
            realization: None,
            pending: None,
        })
    }

    fn acquire(&mut self, kind: &str, target: &ValueSlot) -> Result<ValueSlot, String> {
        self.expect_schema(target, "Target")?;
        let dst = self.alloc();
        let region = self.primitive_region;
        let kind_ref = *self
            .schema_words
            .get(kind)
            .ok_or_else(|| format!("no schema ref for {kind}"))?;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::ConstI64 {
            dst: region + 8,
            value: kind_ref,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 16,
            src: target.slot,
        });
        self.code.push(Op::HostCall { host: ACQUIRE_HOST });
        Ok(ValueSlot {
            slot: dst,
            schema: kind.to_string(),
            realization: None,
            pending: None,
        })
    }

    fn target_host(&mut self) -> ValueSlot {
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::HostCall { host: TARGET_HOST });
        ValueSlot {
            slot: dst,
            schema: "Target".to_string(),
            realization: None,
            pending: None,
        }
    }

    fn sealed_seal(
        &mut self,
        ciphertext: &ValueSlot,
        marker: &ValueSlot,
        recipient: &ValueSlot,
        tag: Option<&ValueSlot>,
    ) -> Result<ValueSlot, String> {
        self.expect_schema(ciphertext, "String")?;
        self.expect_schema(marker, "String")?;
        self.expect_schema(recipient, "String")?;
        if let Some(tag) = tag {
            self.expect_schema(tag, "String")?;
        }
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: ciphertext.slot,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 16,
            src: marker.slot,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 24,
            src: recipient.slot,
        });
        if let Some(tag) = tag {
            self.code.push(Op::CopyI64 {
                dst: region + 32,
                src: tag.slot,
            });
        } else {
            self.code.push(Op::ConstI64 {
                dst: region + 32,
                value: -1,
            });
        }
        self.code.push(Op::HostCall {
            host: SEALED_SEAL_HOST,
        });
        Ok(ValueSlot {
            slot: dst,
            schema: "Sealed".into(),
            realization: None,
            pending: None,
        })
    }

    fn sealed_declassify(&mut self, sealed: &ValueSlot) -> Result<ValueSlot, String> {
        self.expect_schema(sealed, "Sealed")?;
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: sealed.slot,
        });
        self.code.push(Op::HostCall {
            host: SEALED_DECLASSIFY_HOST,
        });
        Ok(ValueSlot {
            slot: dst,
            schema: "String".into(),
            realization: None,
            pending: None,
        })
    }

    fn sealed_to_string(&mut self, sealed: &ValueSlot) -> Result<ValueSlot, String> {
        self.expect_schema(sealed, "Sealed")?;
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: sealed.slot,
        });
        self.code.push(Op::HostCall {
            host: SEALED_TO_STRING_HOST,
        });
        Ok(ValueSlot {
            slot: dst,
            schema: "String".into(),
            realization: None,
            pending: None,
        })
    }

    fn array_literal(
        &mut self,
        array: &ast::Array,
        expected: Option<&str>,
    ) -> Result<ValueSlot, String> {
        let mut elems = Vec::new();
        for elem in &array.elems {
            match elem {
                ast::ArrayElem::Expr(expr) => elems.push(self.expr(expr)?),
                ast::ArrayElem::Flag(flag) => {
                    let handle =
                        *self.literal_handles.flags.get(&flag.value).ok_or_else(|| {
                            format!("flag literal {:?} was not interned", flag.value)
                        })?;
                    let slot = self.alloc();
                    self.code.push(Op::ConstI64 {
                        dst: slot,
                        value: handle,
                    });
                    elems.push(ValueSlot {
                        slot,
                        schema: "Flag".into(),
                        realization: None,
                        pending: None,
                    });
                }
            };
        }
        let elem_schema = if let Some(first) = elems.first() {
            for elem in &elems[1..] {
                if elem.schema != first.schema {
                    return Err(format!(
                        "array literal mixes {} and {} in the machine slice-2 subset",
                        first.schema, elem.schema
                    ));
                }
            }
            first.schema.clone()
        } else {
            expected
                .and_then(array_element_schema)
                .map(str::to_string)
                .ok_or_else(|| {
                    "empty array literal needs an expected Array<T> schema".to_string()
                })?
        };
        let schema = array_schema(&elem_schema);
        let dst = self.alloc();
        let region = self.primitive_region;
        let elem_schema_ref = *self
            .schema_words
            .get(&elem_schema)
            .ok_or_else(|| format!("no schema ref for {elem_schema}"))?;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::ConstI64 {
            dst: region + 8,
            value: elem_schema_ref,
        });
        self.code.push(Op::ConstI64 {
            dst: region + 16,
            value: i64::try_from(elems.len()).expect("array length fits i64"),
        });
        for (index, elem) in elems.iter().enumerate() {
            self.code.push(Op::CopyI64 {
                dst: region + 24 + 8 * u32::try_from(index).expect("array index fits u32"),
                src: elem.slot,
            });
        }
        self.code.push(Op::HostCall {
            host: ARRAY_ALLOC_HOST,
        });
        Ok(ValueSlot {
            slot: dst,
            schema,
            realization: None,
            pending: None,
        })
    }

    fn tree_glob(
        &mut self,
        receiver: &ValueSlot,
        call: &ast::MethodCall,
    ) -> Result<ValueSlot, String> {
        self.expect_schema(receiver, "Tree")?;
        let [arg] = call.args.args.as_slice() else {
            return Err("glob takes one pattern".into());
        };
        let pattern = self.method_arg(arg, Some("String"))?;
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: receiver.slot,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 16,
            src: pattern.slot,
        });
        self.code.push(Op::HostCall { host: GLOB_HOST });
        Ok(ValueSlot {
            slot: dst,
            schema: array_schema("Path"),
            realization: None,
            pending: None,
        })
    }

    fn array_filter_exclude(
        &mut self,
        receiver: &ValueSlot,
        call: &ast::MethodCall,
    ) -> Result<ValueSlot, String> {
        self.expect_schema(receiver, "Array")?;
        let [ast::Arg::Expr(ast::Expr::Closure(closure))] = call.args.args.as_slice() else {
            return Err("slice B4 array filter requires a single closure argument".into());
        };
        let [param] = closure.params.as_slice() else {
            return Err("slice B4 array filter closure must have one parameter".into());
        };
        let excluded = filter_excluded_paths(&closure.body, &param.value)?;
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: receiver.slot,
        });
        self.code.push(Op::ConstI64 {
            dst: region + 16,
            value: i64::try_from(excluded.len()).expect("excluded path count fits i64"),
        });
        for (index, path) in excluded.iter().enumerate() {
            let handle = *self
                .literal_handles
                .paths
                .get(path)
                .ok_or_else(|| format!("path literal {path:?} was not interned"))?;
            self.code.push(Op::ConstI64 {
                dst: region + 24 + 8 * u32::try_from(index).expect("excluded path index"),
                value: handle,
            });
        }
        self.code.push(Op::HostCall {
            host: ARRAY_FILTER_EXCLUDE_HOST,
        });
        Ok(ValueSlot {
            slot: dst,
            schema: receiver.schema.clone(),
            realization: None,
            pending: None,
        })
    }

    fn array_map_pending(
        &mut self,
        receiver: &ValueSlot,
        call: &ast::MethodCall,
    ) -> Result<ValueSlot, String> {
        self.expect_schema(receiver, "Array")?;
        let [ast::Arg::Expr(ast::Expr::Closure(closure))] = call.args.args.as_slice() else {
            return Err("slice-4 array map requires a single closure argument".into());
        };
        let (fn_name, args) = partial_named_fn_closure(closure)?;
        let fn_ref = *self
            .fn_refs
            .get(fn_name)
            .ok_or_else(|| format!("unknown function {fn_name}"))?;
        let pending_elem_schema = self.signatures.returns[fn_name].clone();
        let mut lowered_args = Vec::new();
        for arg in &args {
            match arg {
                ClosureMapArg::Param => lowered_args.push(None),
                ClosureMapArg::Capture(name) => {
                    let captured = self
                        .resolve_binding(name, None)
                        .map_err(|_| format!("unbound capture {name}"))?;
                    lowered_args.push(Some(captured));
                }
            }
        }
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: receiver.slot,
        });
        self.code.push(Op::ConstI64 {
            dst: region + 16,
            value: i64::try_from(fn_ref).expect("fn ref fits i64"),
        });
        self.code.push(Op::ConstI64 {
            dst: region + 24,
            value: i64::try_from(lowered_args.len()).expect("map arg count fits i64"),
        });
        for (index, arg) in lowered_args.iter().enumerate() {
            let at = region + 32 + 16 * u32::try_from(index).expect("map arg index");
            match arg {
                Some(slot) => {
                    self.code.push(Op::ConstI64 { dst: at, value: 0 });
                    self.code.push(Op::CopyI64 {
                        dst: at + 8,
                        src: slot.slot,
                    });
                }
                None => {
                    self.code.push(Op::ConstI64 { dst: at, value: 1 });
                    self.code.push(Op::ConstI64 {
                        dst: at + 8,
                        value: 0,
                    });
                }
            }
        }
        self.code.push(Op::HostCall {
            host: ARRAY_MAP_PENDING_HOST,
        });
        Ok(ValueSlot {
            slot: dst,
            schema: array_schema(&pending_elem_schema),
            realization: None,
            pending: None,
        })
    }

    fn array_collect(
        &mut self,
        receiver: &ValueSlot,
        expected: Option<&str>,
    ) -> Result<ValueSlot, String> {
        self.expect_schema(receiver, "Array")?;
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: receiver.slot,
        });
        self.code.push(Op::HostCall {
            host: ARRAY_COLLECT_HOST,
        });
        Ok(ValueSlot {
            slot: dst,
            schema: expected
                .filter(|schema| self.tables.schemas.is_list(schema))
                .unwrap_or("Tree")
                .into(),
            realization: None,
            pending: None,
        })
    }

    fn array_len(&mut self, receiver: &ValueSlot) -> Result<ValueSlot, String> {
        self.expect_schema(receiver, "Array")?;
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: receiver.slot,
        });
        self.code.push(Op::HostCall {
            host: ARRAY_LEN_HOST,
        });
        Ok(ValueSlot {
            slot: dst,
            schema: "Int".into(),
            realization: None,
            pending: None,
        })
    }

    fn array_push(
        &mut self,
        receiver: &ValueSlot,
        value: &ValueSlot,
        consuming_receiver: bool,
    ) -> Result<ValueSlot, String> {
        self.expect_schema(receiver, "Array")?;
        let elem_schema = array_element_schema(&receiver.schema)
            .ok_or_else(|| format!("{} is not an Array<T>", receiver.schema))?;
        if value.schema != elem_schema {
            return Err(format!(
                "array push expected {elem_schema}, got {} in the machine slice-2 subset",
                value.schema
            ));
        }
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: receiver.slot,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 16,
            src: value.slot,
        });
        self.code.push(Op::ConstI64 {
            dst: region + 24,
            value: *self
                .schema_words
                .get(&value.schema)
                .ok_or_else(|| format!("no schema ref for {}", value.schema))?,
        });
        self.code.push(Op::ConstI64 {
            dst: region + 32,
            value: i64::from(consuming_receiver),
        });
        self.code.push(Op::HostCall {
            host: ARRAY_PUSH_HOST,
        });
        Ok(ValueSlot {
            slot: dst,
            schema: receiver.schema.clone(),
            realization: None,
            pending: None,
        })
    }

    fn array_pop(&mut self, receiver: &ValueSlot) -> Result<ValueSlot, String> {
        self.expect_schema(receiver, "Array")?;
        let array_dst = self.alloc();
        let value_dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: array_dst.into(),
        });
        self.code.push(Op::ConstI64 {
            dst: region + 8,
            value: value_dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 16,
            src: receiver.slot,
        });
        self.code.push(Op::HostCall {
            host: ARRAY_POP_HOST,
        });
        let elem_schema = array_element_schema(&receiver.schema)
            .ok_or_else(|| format!("{} is not an Array<T>", receiver.schema))?
            .to_string();
        self.store_alloc(
            &tuple_schema(&[elem_schema.clone(), receiver.schema.clone()]),
            0,
            &[
                ValueSlot {
                    slot: value_dst,
                    schema: elem_schema,
                    realization: None,
                    pending: None,
                },
                ValueSlot {
                    slot: array_dst,
                    schema: receiver.schema.clone(),
                    realization: None,
                    pending: None,
                },
            ],
        )
    }

    fn array_set(
        &mut self,
        receiver: &ValueSlot,
        index: &ValueSlot,
        value: &ValueSlot,
    ) -> Result<ValueSlot, String> {
        self.expect_schema(receiver, "Array")?;
        self.expect_schema(index, "Int")?;
        let elem_schema = array_element_schema(&receiver.schema)
            .ok_or_else(|| format!("{} is not an Array<T>", receiver.schema))?;
        if value.schema != elem_schema {
            return Err(format!(
                "array set expected {elem_schema}, got {} in the machine slice-2 subset",
                value.schema
            ));
        }
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: receiver.slot,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 16,
            src: index.slot,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 24,
            src: value.slot,
        });
        self.code.push(Op::HostCall {
            host: ARRAY_SET_HOST,
        });
        Ok(ValueSlot {
            slot: dst,
            schema: receiver.schema.clone(),
            realization: None,
            pending: None,
        })
    }

    fn array_join(
        &mut self,
        receiver: &ValueSlot,
        separator: &ValueSlot,
    ) -> Result<ValueSlot, String> {
        self.expect_schema(receiver, "Array")?;
        self.expect_schema(separator, "String")?;
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: receiver.slot,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 16,
            src: separator.slot,
        });
        self.code.push(Op::HostCall {
            host: ARRAY_JOIN_HOST,
        });
        Ok(ValueSlot {
            slot: dst,
            schema: "String".into(),
            realization: None,
            pending: None,
        })
    }

    fn string_concat(&mut self, left: &ValueSlot, right: &ValueSlot) -> Result<ValueSlot, String> {
        self.expect_schema(left, "String")?;
        self.expect_schema(right, "String")?;
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: left.slot,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 16,
            src: right.slot,
        });
        self.code.push(Op::HostCall {
            host: STRING_CONCAT_HOST,
        });
        Ok(ValueSlot {
            slot: dst,
            schema: "String".into(),
            realization: None,
            pending: None,
        })
    }

    fn doc_get(&mut self, doc: &ValueSlot, key: &ValueSlot) -> Result<ValueSlot, String> {
        self.expect_schema(doc, "Doc")?;
        self.expect_schema(key, "String")?;
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: doc.slot,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 16,
            src: key.slot,
        });
        self.code.push(Op::HostCall { host: DOC_GET_HOST });
        Ok(ValueSlot {
            slot: dst,
            schema: option_schema("Realized<Doc>"),
            realization: None,
            pending: None,
        })
    }

    fn doc_package(&mut self, doc: &ValueSlot, name: &ValueSlot) -> Result<ValueSlot, String> {
        self.expect_schema(doc, "Doc")?;
        self.expect_schema(name, "String")?;
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: doc.slot,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 16,
            src: name.slot,
        });
        self.code.push(Op::HostCall {
            host: DOC_PACKAGE_HOST,
        });
        Ok(ValueSlot {
            slot: dst,
            schema: option_schema("Realized<Doc>"),
            realization: None,
            pending: None,
        })
    }

    fn tree_project(&mut self, tree: &ValueSlot, path: &ValueSlot) -> Result<ValueSlot, String> {
        self.expect_schema(tree, "Tree")?;
        self.expect_schema(path, "Path")?;
        let input_slot = self.next_input_slot;
        self.next_input_slot += 1;
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: input_slot,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: tree.slot,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 16,
            src: path.slot,
        });
        self.code.push(Op::HostCall {
            host: TREE_PROJECT_HOST,
        });
        let dst = self.alloc();
        self.code.push(Op::Await {
            dst,
            input: u32::try_from(input_slot).expect("input slot fits u32"),
        });
        Ok(ValueSlot {
            slot: dst,
            schema: "Tree".into(),
            realization: None,
            pending: None,
        })
    }

    fn tree_text(&mut self, tree: &ValueSlot, path: &ValueSlot) -> Result<ValueSlot, String> {
        self.expect_schema(tree, "Tree")?;
        self.expect_schema(path, "Path")?;
        let input_slot = self.next_input_slot;
        self.next_input_slot += 1;
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: input_slot,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: tree.slot,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 16,
            src: path.slot,
        });
        self.code.push(Op::HostCall {
            host: TREE_TEXT_HOST,
        });
        let dst = self.alloc();
        self.code.push(Op::Await {
            dst,
            input: u32::try_from(input_slot).expect("input slot fits u32"),
        });
        Ok(ValueSlot {
            slot: dst,
            schema: "String".into(),
            realization: None,
            pending: None,
        })
    }

    fn command_block(&mut self, command: &ast::CommandBlock) -> Result<ValueSlot, String> {
        if !matches!(
            command.command.value.as_str(),
            "cc" | "ar" | "rustc" | "build_script"
        ) {
            return Err(format!(
                "command {} is outside the machine exec subset",
                command.command.value
            ));
        }
        let capability = self
            .resolve_binding(&command.command.value, None)
            .map_err(|_| format!("no capability `{}` in scope", command.command.value))?;
        enum LoweredCommandPart {
            Token(i64),
            Splice(ValueSlot),
        }
        let mut parts = Vec::with_capacity(command.parts.len());
        for part in &command.parts {
            match part {
                ast::CommandPart::Token(token) => {
                    let handle =
                        *self
                            .literal_handles
                            .strings
                            .get(&token.value)
                            .ok_or_else(|| {
                                format!("command token {:?} was not interned", token.value)
                            })?;
                    parts.push(LoweredCommandPart::Token(handle));
                }
                ast::CommandPart::Splice(splice) => {
                    parts.push(LoweredCommandPart::Splice(self.expr(&splice.expr)?));
                }
            }
        }
        let input_slot = self.next_input_slot;
        self.next_input_slot += 1;
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: input_slot,
        });
        self.code.push(Op::ConstI64 {
            dst: region + 8,
            value: command_kind(&command.command.value)?,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 16,
            src: capability.slot,
        });
        self.code.push(Op::ConstI64 {
            dst: region + 24,
            value: i64::try_from(parts.len()).expect("command part count fits i64"),
        });
        self.code.push(Op::ConstI64 {
            dst: region + 32,
            value: command.span.start.into(),
        });
        self.code.push(Op::ConstI64 {
            dst: region + 40,
            value: command.span.end.into(),
        });
        for (index, part) in parts.iter().enumerate() {
            let at = region + 48 + 24 * u32::try_from(index).expect("command part index");
            match part {
                LoweredCommandPart::Token(handle) => {
                    self.code.push(Op::ConstI64 { dst: at, value: 0 });
                    self.code.push(Op::ConstI64 {
                        dst: at + 8,
                        value: *handle,
                    });
                    self.code.push(Op::ConstI64 {
                        dst: at + 16,
                        value: 0,
                    });
                }
                LoweredCommandPart::Splice(value) => {
                    let schema_ref = *self
                        .schema_words
                        .get(&value.schema)
                        .ok_or_else(|| format!("no schema ref for {}", value.schema))?;
                    self.code.push(Op::ConstI64 { dst: at, value: 1 });
                    self.code.push(Op::CopyI64 {
                        dst: at + 8,
                        src: value.slot,
                    });
                    self.code.push(Op::ConstI64 {
                        dst: at + 16,
                        value: schema_ref,
                    });
                }
            }
        }
        self.code.push(Op::HostCall { host: EXEC_HOST });
        let dst = self.alloc();
        self.code.push(Op::Await {
            dst,
            input: u32::try_from(input_slot).expect("input slot fits u32"),
        });
        Ok(ValueSlot {
            slot: dst,
            schema: "Tree".into(),
            realization: None,
            pending: None,
        })
    }

    fn fetch_call(&mut self, call: &ast::Call) -> Result<ValueSlot, String> {
        let mut url = None;
        let mut sha256 = None;
        for arg in &call.args.args {
            let ast::Arg::Kwarg(kwarg) = arg else {
                return Err("fetch arguments must be named".into());
            };
            match kwarg.name.value.as_str() {
                "url" => url = Some(self.expr_expected(&kwarg.value, Some("String"))?),
                "sha256" => sha256 = Some(self.expr_expected(&kwarg.value, Some("String"))?),
                other => return Err(format!("fetch got unknown argument `{other}`")),
            }
        }
        let url = url.ok_or_else(|| "fetch requires a url".to_string())?;
        let input_slot = self.next_input_slot;
        self.next_input_slot += 1;
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: input_slot,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: url.slot,
        });
        match sha256 {
            Some(sha256) => self.code.push(Op::CopyI64 {
                dst: region + 16,
                src: sha256.slot,
            }),
            None => self.code.push(Op::ConstI64 {
                dst: region + 16,
                value: -1,
            }),
        }
        self.code.push(Op::HostCall { host: FETCH_HOST });
        let dst = self.alloc();
        self.code.push(Op::Await {
            dst,
            input: u32::try_from(input_slot).expect("input slot fits u32"),
        });
        Ok(ValueSlot {
            slot: dst,
            schema: "Tree".into(),
            realization: None,
            pending: None,
        })
    }

    fn doc_parse_call(&mut self, call: &ast::Call, kind: i64) -> Result<ValueSlot, String> {
        self.document_parser_call(call, kind, None)
    }

    fn typed_doc_parse_call(
        &mut self,
        call: &ast::Call,
        kind: i64,
        expected: Option<&str>,
    ) -> Result<ValueSlot, String> {
        let Some(expected) = expected else {
            return Err("json_typed requires a contextual result type".into());
        };
        self.document_parser_call(call, kind, Some(expected))
    }

    fn document_parser_call(
        &mut self,
        call: &ast::Call,
        kind: i64,
        target_schema: Option<&str>,
    ) -> Result<ValueSlot, String> {
        let [ast::Arg::Expr(arg)] = call.args.args.as_slice() else {
            return Err("document parser takes one argument".into());
        };
        let input = self.expr(arg)?;
        if input.schema != "String" && input.schema != "Tree" {
            return Err(format!("document parser called on {}", input.schema));
        }
        let input_slot = self.next_input_slot;
        self.next_input_slot += 1;
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: input_slot,
        });
        self.code.push(Op::ConstI64 {
            dst: region + 8,
            value: kind,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 16,
            src: input.slot,
        });
        let target_schema_ref = match target_schema {
            Some(schema) => *self
                .schema_words
                .get(schema)
                .ok_or_else(|| format!("no schema ref for {schema}"))?,
            None => 0,
        };
        self.code.push(Op::ConstI64 {
            dst: region + 24,
            value: target_schema_ref,
        });
        self.code.push(Op::HostCall {
            host: DOC_PARSE_HOST,
        });
        let dst = self.alloc();
        self.code.push(Op::Await {
            dst,
            input: u32::try_from(input_slot).expect("input slot fits u32"),
        });
        Ok(ValueSlot {
            slot: dst,
            schema: target_schema.unwrap_or("Doc").into(),
            realization: None,
            pending: None,
        })
    }

    fn crate_archive_call(&mut self, call: &ast::Call) -> Result<ValueSlot, String> {
        let [ast::Arg::Expr(arg)] = call.args.args.as_slice() else {
            return Err("crate_archive takes one Blob, String, or single-file Tree".into());
        };
        let input = self.expr(arg)?;
        if !(self
            .tables
            .schemas
            .is_primitive(&input.schema, Primitive::Bytes)
            || self
                .tables
                .schemas
                .is_primitive(&input.schema, Primitive::String)
            || self.tables.schemas.is_external(&input.schema, "Tree"))
        {
            return Err(format!("crate_archive called on {}", input.schema));
        }
        let input_slot = self.next_input_slot;
        self.next_input_slot += 1;
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: input_slot,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: input.slot,
        });
        self.code.push(Op::HostCall {
            host: CRATE_ARCHIVE_HOST,
        });
        let dst = self.alloc();
        self.code.push(Op::Await {
            dst,
            input: u32::try_from(input_slot).expect("input slot fits u32"),
        });
        Ok(ValueSlot {
            slot: dst,
            schema: "Tree".into(),
            realization: None,
            pending: None,
        })
    }

    fn extract_call(&mut self, call: &ast::Call) -> Result<ValueSlot, String> {
        let [ast::Arg::Expr(arg)] = call.args.args.as_slice() else {
            return Err("extract takes a tree".into());
        };
        self.expr_expected(arg, Some("Tree"))
    }

    fn elf_call(&mut self, call: &ast::Call) -> Result<ValueSlot, String> {
        let [ast::Arg::Expr(arg)] = call.args.args.as_slice() else {
            return Err("elf takes one Blob, String, or single-blob Tree".into());
        };
        let mut input = self.expr(arg)?;
        if self.value_schema_is_realized_named(&input.schema, "Doc") {
            input = self.coerce_to_schema(input, "Doc")?;
        }
        if self.tables.schemas.is_named_schema(&input.schema, "Doc") {
            input = self.coerce_to_schema(input, "Blob")?;
        }
        if !(self
            .tables
            .schemas
            .is_primitive(&input.schema, Primitive::Bytes)
            || self
                .tables
                .schemas
                .is_primitive(&input.schema, Primitive::String)
            || self.tables.schemas.is_external(&input.schema, "Tree"))
        {
            return Err(format!("elf called on {}", input.schema));
        }
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: input.slot,
        });
        self.code.push(Op::HostCall { host: ELF_DOC_HOST });
        Ok(ValueSlot {
            slot: dst,
            schema: "Doc".into(),
            realization: None,
            pending: None,
        })
    }

    fn ast_call(&mut self, call: &ast::Call) -> Result<ValueSlot, String> {
        let [ast::Arg::Expr(arg)] = call.args.args.as_slice() else {
            return Err("ast takes one source String or single-source Tree".into());
        };
        let input = self.expr(arg)?;
        if !matches!(input.schema.as_str(), "String" | "Tree") {
            return Err(format!("ast called on {}", input.schema));
        }
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: input.slot,
        });
        self.code.push(Op::HostCall { host: AST_DOC_HOST });
        Ok(ValueSlot {
            slot: dst,
            schema: "Doc".into(),
            realization: None,
            pending: None,
        })
    }

    fn oci_call(&mut self, call: &ast::Call) -> Result<ValueSlot, String> {
        let [ast::Arg::Expr(arg)] = call.args.args.as_slice() else {
            return Err("oci takes one OCI layout Blob, String, or Tree".into());
        };
        let input = self.expr(arg)?;
        if !matches!(input.schema.as_str(), "Blob" | "String" | "Tree") {
            return Err(format!("oci called on {}", input.schema));
        }
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: input.slot,
        });
        self.code.push(Op::HostCall { host: OCI_DOC_HOST });
        Ok(ValueSlot {
            slot: dst,
            schema: "Doc".into(),
            realization: None,
            pending: None,
        })
    }

    fn version_call(&mut self, call: &ast::Call) -> Result<ValueSlot, String> {
        let [ast::Arg::Expr(arg)] = call.args.args.as_slice() else {
            return Err("version takes one String".into());
        };
        let input = self.expr_expected(arg, Some("String"))?;
        if input.schema != "String" {
            return Err(format!("version called on {}", input.schema));
        }
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: input.slot,
        });
        self.code.push(Op::HostCall {
            host: VERSION_PARSE_HOST,
        });
        Ok(ValueSlot {
            slot: dst,
            schema: "Version".into(),
            realization: None,
            pending: None,
        })
    }

    fn version_set_from_req(&mut self, input: &ValueSlot) -> Result<ValueSlot, String> {
        self.expect_schema(input, "String")?;
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: input.slot,
        });
        self.code.push(Op::HostCall {
            host: VERSION_SET_PARSE_HOST,
        });
        Ok(ValueSlot {
            slot: dst,
            schema: "VersionSet".into(),
            realization: None,
            pending: None,
        })
    }

    fn version_set_op(
        &mut self,
        op: i64,
        left: &ValueSlot,
        right: Option<&ValueSlot>,
        result_schema: &str,
    ) -> Result<ValueSlot, String> {
        self.expect_schema(left, "VersionSet")?;
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::ConstI64 {
            dst: region + 8,
            value: op,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 16,
            src: left.slot,
        });
        if let Some(right) = right {
            self.code.push(Op::CopyI64 {
                dst: region + 24,
                src: right.slot,
            });
        } else {
            self.code.push(Op::ConstI64 {
                dst: region + 24,
                value: 0,
            });
        }
        self.code.push(Op::HostCall {
            host: VERSION_SET_OP_HOST,
        });
        Ok(ValueSlot {
            slot: dst,
            schema: result_schema.into(),
            realization: None,
            pending: None,
        })
    }

    fn string_split(
        &mut self,
        receiver: &ValueSlot,
        delim: &ValueSlot,
        selector: i64,
    ) -> ValueSlot {
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: receiver.slot,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 16,
            src: delim.slot,
        });
        self.code.push(Op::ConstI64 {
            dst: region + 24,
            value: selector,
        });
        self.code.push(Op::HostCall {
            host: STRING_SPLIT_HOST,
        });
        ValueSlot {
            slot: dst,
            schema: "String".into(),
            realization: None,
            pending: None,
        }
    }

    /// Derive a comparison operator's Bool from an `Ordering` value (variants
    /// Less=0, Equal=1, Greater=2): `<` is Less, `>` is Greater, `<=` is
    /// not-Greater, `>=` is not-Less.
    fn ordering_to_bool(&mut self, ord: &ValueSlot, op: &str) -> ValueSlot {
        let tag = self.store_tag(ord);
        let (target, equal) = match op {
            "<" => (0, true),
            ">" => (2, true),
            "<=" => (2, false),
            _ => (0, false),
        };
        let lit = self.alloc();
        self.code.push(Op::ConstI64 {
            dst: lit,
            value: target,
        });
        let dst = self.alloc();
        self.code.push(if equal {
            Op::EqI64 {
                dst,
                a: tag.slot,
                b: lit,
            }
        } else {
            Op::NeI64 {
                dst,
                a: tag.slot,
                b: lit,
            }
        });
        ValueSlot {
            slot: dst,
            schema: "Bool".into(),
            realization: None,
            pending: None,
        }
    }

    /// A String -> Bool query (contains needs an argument, is_numeric does not).
    fn string_query(
        &mut self,
        receiver: &ValueSlot,
        arg: Option<&ValueSlot>,
        host: u32,
    ) -> ValueSlot {
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: receiver.slot,
        });
        if let Some(arg) = arg {
            self.code.push(Op::CopyI64 {
                dst: region + 16,
                src: arg.slot,
            });
        }
        self.code.push(Op::HostCall { host });
        ValueSlot {
            slot: dst,
            schema: "Bool".into(),
            realization: None,
            pending: None,
        }
    }

    fn string_parse_int(&mut self, receiver: &ValueSlot) -> ValueSlot {
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: receiver.slot,
        });
        self.code.push(Op::HostCall {
            host: STRING_PARSE_INT_HOST,
        });
        ValueSlot {
            slot: dst,
            schema: "Int".into(),
            realization: None,
            pending: None,
        }
    }

    fn option_some_call(&mut self, call: &ast::Call) -> Result<ValueSlot, String> {
        let [ast::Arg::Expr(arg)] = call.args.args.as_slice() else {
            return Err("Some takes one value".into());
        };
        let value = self.expr(arg)?;
        let value_schema = value.schema.clone();
        self.option_construct(1, &value_schema, Some(&value))
    }

    fn option_none(&mut self, expected: Option<&str>) -> Result<ValueSlot, String> {
        let Some(option_schema) = expected else {
            return Err("None requires a known Option type from context".into());
        };
        let Some(value_schema) = option_value_schema(option_schema) else {
            return Err(format!(
                "None used where non-Option type {option_schema} is expected"
            ));
        };
        let value_schema = value_schema.to_string();
        self.option_construct(0, &value_schema, None)
    }

    fn option_construct(
        &mut self,
        tag: i64,
        value_schema: &str,
        value: Option<&ValueSlot>,
    ) -> Result<ValueSlot, String> {
        let value_ref = *self
            .schema_words
            .get(value_schema)
            .ok_or_else(|| format!("no schema ref for {value_schema}"))?;
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::ConstI64 {
            dst: region + 8,
            value: tag,
        });
        self.code.push(Op::ConstI64 {
            dst: region + 16,
            value: value_ref,
        });
        if let Some(value) = value {
            self.code.push(Op::CopyI64 {
                dst: region + 24,
                src: value.slot,
            });
        } else {
            self.code.push(Op::ConstI64 {
                dst: region + 24,
                value: 0,
            });
        }
        self.code.push(Op::HostCall {
            host: OPTION_CONSTRUCT_HOST,
        });
        Ok(ValueSlot {
            slot: dst,
            schema: option_schema(value_schema),
            realization: None,
            pending: None,
        })
    }

    fn option_destruct(&mut self, scrut: &ValueSlot, selector: i64, schema: &str) -> ValueSlot {
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: scrut.slot,
        });
        self.code.push(Op::ConstI64 {
            dst: region + 16,
            value: selector,
        });
        self.code.push(Op::HostCall {
            host: OPTION_DESTRUCT_HOST,
        });
        let realization = realized_value_schema(schema).map(|_| {
            let slot = self.alloc();
            self.code.push(Op::ConstI64 {
                dst: region,
                value: slot.into(),
            });
            self.code.push(Op::CopyI64 {
                dst: region + 8,
                src: scrut.slot,
            });
            self.code.push(Op::ConstI64 {
                dst: region + 16,
                value: 2,
            });
            self.code.push(Op::HostCall {
                host: OPTION_DESTRUCT_HOST,
            });
            slot
        });
        ValueSlot {
            slot: dst,
            schema: schema.into(),
            realization,
            pending: None,
        }
    }

    /// Emit the tag test for an Option match arm: skip to the next arm unless
    /// the scrutinee's tag equals `want_tag` (0 = None, 1 = Some).
    fn option_tag_test(&mut self, scrut: &ValueSlot, want_tag: i64, skip_patches: &mut Vec<usize>) {
        let tag = self.option_destruct(scrut, 0, "Int");
        let lit = self.alloc();
        self.code.push(Op::ConstI64 {
            dst: lit,
            value: want_tag,
        });
        let test = self.alloc();
        self.code.push(Op::EqI64 {
            dst: test,
            a: tag.slot,
            b: lit,
        });
        skip_patches.push(self.code.len());
        self.code.push(Op::JumpIfZero {
            value: test,
            target: 0,
        });
    }

    fn bind_option_payload(
        &mut self,
        pattern: &ast::Pattern,
        payload: ValueSlot,
    ) -> Result<(), String> {
        match pattern {
            ast::Pattern::Identifier(name) => {
                self.slots
                    .insert(name.value.clone(), BindingCell::value(payload));
                Ok(())
            }
            ast::Pattern::Wildcard(_) => Ok(()),
            other => Err(format!(
                "Some pattern binding {other:?} is outside the machine slice-2 subset"
            )),
        }
    }

    fn compare_value(
        &mut self,
        op: &str,
        left: &ValueSlot,
        right: &ValueSlot,
        schema: &str,
    ) -> Result<ValueSlot, String> {
        let dst = self.alloc();
        let schema_ref = *self
            .schema_words
            .get(schema)
            .ok_or_else(|| format!("no schema ref for {schema}"))?;
        let op_code = match op {
            "==" => 0,
            "!=" => 1,
            "<" => 2,
            "<=" => 3,
            ">" => 4,
            ">=" => 5,
            other => return Err(format!("unknown comparison operator {other:?}")),
        };
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::ConstI64 {
            dst: region + 8,
            value: schema_ref,
        });
        self.code.push(Op::ConstI64 {
            dst: region + 16,
            value: op_code,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 24,
            src: left.slot,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 32,
            src: right.slot,
        });
        self.code.push(Op::HostCall {
            host: VALUE_COMPARE_HOST,
        });
        Ok(ValueSlot {
            slot: dst,
            schema: "Bool".into(),
            realization: None,
            pending: None,
        })
    }

    fn ast_fn(&mut self, receiver: &ValueSlot, name: &ValueSlot) -> Result<ValueSlot, String> {
        self.expect_schema(receiver, "Doc")?;
        self.expect_schema(name, "String")?;
        let pending_slot = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: pending_slot.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: receiver.slot,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 16,
            src: name.slot,
        });
        self.code.push(Op::HostCall { host: AST_FN_HOST });
        let pending = ValueSlot {
            slot: pending_slot,
            schema: pending_schema("Doc"),
            realization: None,
            pending: None,
        };
        self.coerce_pending_to_schema(pending, "Doc")
    }

    fn path_with_ext(&mut self, path: &ValueSlot, ext: &ValueSlot) -> Result<ValueSlot, String> {
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: path.slot,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 16,
            src: ext.slot,
        });
        self.code.push(Op::HostCall {
            host: PATH_WITH_EXT_HOST,
        });
        Ok(ValueSlot {
            slot: dst,
            schema: "Path".into(),
            realization: None,
            pending: None,
        })
    }

    fn path_join(&mut self, path: &ValueSlot, segment: &ValueSlot) -> Result<ValueSlot, String> {
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: path.slot,
        });
        self.code.push(Op::CopyI64 {
            dst: region + 16,
            src: segment.slot,
        });
        self.code.push(Op::HostCall {
            host: PATH_JOIN_HOST,
        });
        Ok(ValueSlot {
            slot: dst,
            schema: "Path".into(),
            realization: None,
            pending: None,
        })
    }

    fn raw_string_convert(
        &mut self,
        value: &ValueSlot,
        schema: &str,
        host: u32,
    ) -> Result<ValueSlot, String> {
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: value.slot,
        });
        self.code.push(Op::HostCall { host });
        Ok(ValueSlot {
            slot: dst,
            schema: schema.into(),
            realization: None,
            pending: None,
        })
    }

    fn doc_is_map(&mut self, doc: &ValueSlot) -> Result<ValueSlot, String> {
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: doc.slot,
        });
        self.code.push(Op::HostCall {
            host: DOC_IS_MAP_HOST,
        });
        Ok(ValueSlot {
            slot: dst,
            schema: "Bool".into(),
            realization: None,
            pending: None,
        })
    }

    fn doc_keys(&mut self, doc: &ValueSlot) -> Result<ValueSlot, String> {
        let dst = self.alloc();
        let region = self.primitive_region;
        self.code.push(Op::ConstI64 {
            dst: region,
            value: dst.into(),
        });
        self.code.push(Op::CopyI64 {
            dst: region + 8,
            src: doc.slot,
        });
        self.code.push(Op::HostCall {
            host: DOC_KEYS_HOST,
        });
        Ok(ValueSlot {
            slot: dst,
            schema: array_schema("String"),
            realization: None,
            pending: None,
        })
    }
}

fn path_ref_segments(path: &ast::PathRef) -> Result<Vec<String>, String> {
    match path {
        ast::PathRef::Identifier(name) => Ok(vec![name.value.clone()]),
        ast::PathRef::Scoped(path) => Ok(path.segments.iter().map(|s| s.value.clone()).collect()),
    }
}

fn map_schemas(schema: &str) -> Option<(&str, &str)> {
    let (base, args) = legacy_generic_schema(schema)?;
    (base == "Map").then_some(())?;
    let [key, value]: [&str; 2] = args.try_into().ok()?;
    Some((key, value))
}

fn map_schema(key_schema: &str, value_schema: &str) -> String {
    format!("Map<{key_schema},{value_schema}>")
}

fn map_value_schema(schema: &str) -> Option<&str> {
    map_schemas(schema).map(|(_, value)| value)
}

fn array_schema(elem_schema: &str) -> String {
    format!("Array<{elem_schema}>")
}

fn array_element_schema(schema: &str) -> Option<&str> {
    let (base, args) = legacy_generic_schema(schema)?;
    (base == "Array" || base == "List").then_some(())?;
    let [elem]: [&str; 1] = args.try_into().ok()?;
    Some(elem)
}

fn legacy_generic_schema(schema: &str) -> Option<(&str, Vec<&str>)> {
    let (base, rest) = schema.split_once('<')?;
    let inner = rest.strip_suffix('>')?;
    Some((base, split_top_level_schema_slices(inner)))
}

fn tuple_schema(fields: &[String]) -> String {
    format!("Tuple<{}>", fields.join(","))
}

fn tuple_schema_fields(schema: &str) -> Option<Vec<String>> {
    let inner = schema.strip_prefix("Tuple<")?.strip_suffix('>')?;
    if inner.is_empty() {
        return Some(Vec::new());
    }
    Some(split_top_level_schemas(inner))
}

fn split_top_level_schemas(inner: &str) -> Vec<String> {
    split_top_level_schema_slices(inner)
        .into_iter()
        .map(str::to_string)
        .collect()
}

fn split_top_level_schema_slices(inner: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (index, ch) in inner.char_indices() {
        match ch {
            '<' => depth += 1,
            '>' => depth = depth.saturating_sub(1),
            ',' if depth == 0 => {
                out.push(&inner[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    out.push(&inner[start..]);
    out
}

fn option_schema(value_schema: &str) -> String {
    format!("Option<{value_schema}>")
}

fn pending_schema(value_schema: &str) -> String {
    format!("Pending<{value_schema}>")
}

fn pending_value_schema(schema: &str) -> Option<&str> {
    schema.strip_prefix("Pending<")?.strip_suffix('>')
}

fn realized_schema(value_schema: &str) -> String {
    format!("Realized<{value_schema}>")
}

fn realized_value_schema(schema: &str) -> Option<&str> {
    schema.strip_prefix("Realized<")?.strip_suffix('>')
}

fn value_schema_inside_barrier(schema: &str) -> Option<&str> {
    pending_value_schema(schema).or_else(|| realized_value_schema(schema))
}

fn strict_binary_operand_schema<'a>(
    schemas: &SchemaTables,
    op: &str,
    left: &'a str,
    right: &'a str,
) -> Option<&'a str> {
    let left = value_schema_inside_barrier(left).unwrap_or(left);
    let right = value_schema_inside_barrier(right).unwrap_or(right);
    if left != right {
        return None;
    }
    let is_int = schemas.is_primitive(left, Primitive::I64);
    let is_float = schemas.is_primitive(left, Primitive::F64);
    let is_string = schemas.is_primitive(left, Primitive::String);
    let is_bool = schemas.is_primitive(left, Primitive::Bool);
    let is_path = schemas.is_external(left, "Path");
    let is_version = schemas.is_external(left, "Version");
    let is_version_set = schemas.is_external(left, "VersionSet");
    match op {
        "+" | "-" | "*" if is_int => Some(left),
        "+" | "*" if is_float => Some(left),
        "+" if is_string => Some(left),
        "==" | "!="
            if is_int
                || is_float
                || is_string
                || is_path
                || is_bool
                || is_version
                || is_version_set =>
        {
            Some(left)
        }
        "<" | "<=" | ">" | ">=" if is_int || is_string || is_version => Some(left),
        "&&" if is_bool => Some(left),
        _ => None,
    }
}

fn option_value_schema(schema: &str) -> Option<&str> {
    schema.strip_prefix("Option<")?.strip_suffix('>')
}

enum ClosureMapArg<'a> {
    Capture(&'a str),
    Param,
}

fn partial_named_fn_closure(
    closure: &ast::Closure,
) -> Result<(&str, Vec<ClosureMapArg<'_>>), String> {
    let [param] = closure.params.as_slice() else {
        return Err("slice-4 map closure must have one parameter".into());
    };
    let ast::Expr::Call(call) = &closure.body else {
        return Err("slice-4 map closure body must be a named function call".into());
    };
    let ast::PathRef::Identifier(fn_name) = &call.callee else {
        return Err("slice-4 map closure callee must be a named function".into());
    };
    let mut args = Vec::new();
    let mut saw_param = false;
    for arg in &call.args.args {
        let ast::Arg::Expr(ast::Expr::Identifier(name)) = arg else {
            return Err("slice-4 map closure arguments must be identifiers".into());
        };
        if name.value == param.value {
            saw_param = true;
            args.push(ClosureMapArg::Param);
        } else {
            args.push(ClosureMapArg::Capture(name.value.as_str()));
        }
    }
    if !saw_param {
        return Err(format!(
            "slice-4 map closure must pass parameter {}",
            param.value
        ));
    }
    Ok((fn_name.value.as_str(), args))
}

fn filter_excluded_paths(expr: &ast::Expr, param: &str) -> Result<Vec<String>, String> {
    let mut out = Vec::new();
    collect_filter_excluded_paths(expr, param, &mut out)?;
    Ok(out)
}

fn collect_filter_excluded_paths(
    expr: &ast::Expr,
    param: &str,
    out: &mut Vec<String>,
) -> Result<(), String> {
    match expr {
        ast::Expr::Binary(binary) if binary.op == "&&" => {
            collect_filter_excluded_paths(&binary.left, param, out)?;
            collect_filter_excluded_paths(&binary.right, param, out)
        }
        ast::Expr::Binary(binary) if binary.op == "!=" => match (&binary.left, &binary.right) {
            (ast::Expr::Identifier(name), ast::Expr::Path(path)) if name.value == param => {
                out.push(path.value.clone());
                Ok(())
            }
            _ => Err("slice B4 filter supports `param != p\"...\"` clauses".into()),
        },
        _ => Err("slice B4 filter supports `&&` of path exclusions".into()),
    }
}

fn command_kind(command: &str) -> Result<i64, String> {
    match command {
        "cc" => Ok(0),
        "ar" => Ok(1),
        "rustc" => Ok(2),
        "build_script" => Ok(3),
        other => Err(format!(
            "command {other} is outside the machine exec subset"
        )),
    }
}

fn resolve_variant_segments(
    tables: &ModuleTables,
    segments: &[String],
) -> Result<(String, usize, VariantShape), String> {
    let [enum_name, variant_name] = segments else {
        return Err(format!("path {segments:?} is not a declared variant path"));
    };
    let Some(info) = tables.enums.get(enum_name) else {
        let Some((index, shape)) = builtin_variant_shape(enum_name, variant_name) else {
            return Err(format!("unknown enum {enum_name}"));
        };
        return Ok((enum_name.clone(), index, shape));
    };
    let (index, (_, shape)) = info
        .variants
        .iter()
        .enumerate()
        .find(|(_, (name, _))| name == variant_name)
        .ok_or_else(|| format!("unknown variant {enum_name}::{variant_name}"))?;
    Ok((enum_name.clone(), index, shape.clone()))
}

fn builtin_unit_variant_index(enum_name: &str, variant_name: &str) -> Option<usize> {
    builtin_variant_shape(enum_name, variant_name)
        .and_then(|(index, shape)| matches!(shape, VariantShape::Unit).then_some(index))
}

fn builtin_variant_shape(enum_name: &str, variant_name: &str) -> Option<(usize, VariantShape)> {
    match (enum_name, variant_name) {
        ("Arg", "Str") => Some((0, VariantShape::Tuple(1))),
        ("Arg", "Path") => Some((1, VariantShape::Tuple(1))),
        ("Arg", "Interpolation") => Some((
            2,
            VariantShape::Record(vec!["tree".to_string(), "subpath".to_string()]),
        )),
        ("Os", "Linux") => Some((0, VariantShape::Unit)),
        ("Os", "Macos") => Some((1, VariantShape::Unit)),
        ("Os", "Windows") => Some((2, VariantShape::Unit)),
        ("Arch", "X86_64") => Some((0, VariantShape::Unit)),
        ("Arch", "Aarch64") => Some((1, VariantShape::Unit)),
        ("Arch", "Arm") => Some((2, VariantShape::Unit)),
        ("Arch", "Riscv64") => Some((3, VariantShape::Unit)),
        ("Arch", "Wasm32") => Some((4, VariantShape::Unit)),
        ("Arch", "Unknown") => Some((5, VariantShape::Unit)),
        _ => None,
    }
}

fn max_call_argc(block: &ast::Block) -> usize {
    fn in_expr(e: &ast::Expr, max: &mut usize) {
        match e {
            ast::Expr::Call(c) => {
                *max = (*max).max(c.args.args.len());
                for arg in &c.args.args {
                    if let ast::Arg::Expr(e) = arg {
                        in_expr(e, max);
                    }
                }
            }
            ast::Expr::MethodCall(c) => {
                *max = (*max).max(c.args.args.len());
                in_expr(&c.receiver, max);
                for arg in &c.args.args {
                    if let ast::Arg::Expr(e) = arg {
                        in_expr(e, max);
                    }
                }
            }
            ast::Expr::Map(m) => {
                for entry in &m.entries {
                    in_expr(&entry.key, max);
                    in_expr(&entry.value, max);
                }
            }
            ast::Expr::StructLit(lit) => {
                for field in &lit.fields {
                    in_expr(&field.value, max);
                }
            }
            ast::Expr::Binary(b) => {
                in_expr(&b.left, max);
                in_expr(&b.right, max);
            }
            ast::Expr::Paren(p) => in_expr(&p.inner, max),
            ast::Expr::Match(m) => {
                in_expr(&m.scrutinee, max);
                for arm in &m.arms {
                    in_expr(&arm.value, max);
                }
            }
            _ => {}
        }
    }
    let mut max = 0;
    for stmt in &block.stmts {
        if let ast::Stmt::Let(l) = stmt {
            in_expr(&l.value, &mut max);
        }
    }
    if let Some(tail) = &block.tail {
        in_expr(tail, &mut max);
    }
    max.max(2)
}

fn max_store_field_count(block: &ast::Block) -> usize {
    fn in_expr(e: &ast::Expr, max: &mut usize) {
        match e {
            ast::Expr::Call(c) => {
                *max = (*max).max(c.args.args.len());
                for arg in &c.args.args {
                    if let ast::Arg::Expr(e) = arg {
                        in_expr(e, max);
                    }
                }
            }
            ast::Expr::MethodCall(c) => {
                in_expr(&c.receiver, max);
                for arg in &c.args.args {
                    if let ast::Arg::Expr(e) = arg {
                        in_expr(e, max);
                    }
                }
            }
            ast::Expr::Map(m) => {
                for entry in &m.entries {
                    in_expr(&entry.key, max);
                    in_expr(&entry.value, max);
                }
            }
            ast::Expr::StructLit(lit) => {
                *max = (*max).max(lit.fields.len());
                for field in &lit.fields {
                    in_expr(&field.value, max);
                }
            }
            ast::Expr::Binary(b) => {
                in_expr(&b.left, max);
                in_expr(&b.right, max);
            }
            ast::Expr::Paren(p) => in_expr(&p.inner, max),
            ast::Expr::Match(m) => {
                in_expr(&m.scrutinee, max);
                for arm in &m.arms {
                    in_expr(&arm.value, max);
                }
            }
            _ => {}
        }
    }
    let mut max = 0;
    for stmt in &block.stmts {
        if let ast::Stmt::Let(l) = stmt {
            in_expr(&l.value, &mut max);
        }
    }
    if let Some(tail) = &block.tail {
        in_expr(tail, &mut max);
    }
    max.max(4)
}

fn max_command_part_words(block: &ast::Block) -> usize {
    fn in_expr(e: &ast::Expr, max: &mut usize) {
        match e {
            ast::Expr::Call(c) => {
                for arg in &c.args.args {
                    if let ast::Arg::Expr(e) = arg {
                        in_expr(e, max);
                    }
                }
            }
            ast::Expr::MethodCall(c) => {
                in_expr(&c.receiver, max);
                for arg in &c.args.args {
                    if let ast::Arg::Expr(e) = arg {
                        in_expr(e, max);
                    }
                }
            }
            ast::Expr::Map(m) => {
                for entry in &m.entries {
                    in_expr(&entry.key, max);
                    in_expr(&entry.value, max);
                }
            }
            ast::Expr::StructLit(lit) => {
                for field in &lit.fields {
                    in_expr(&field.value, max);
                }
            }
            ast::Expr::Binary(b) => {
                in_expr(&b.left, max);
                in_expr(&b.right, max);
            }
            ast::Expr::Paren(p) => in_expr(&p.inner, max),
            ast::Expr::Match(m) => {
                in_expr(&m.scrutinee, max);
                for arm in &m.arms {
                    in_expr(&arm.value, max);
                }
            }
            ast::Expr::Command(command) => {
                *max = (*max).max(6 + command.parts.len() * 3);
                for part in &command.parts {
                    if let ast::CommandPart::Splice(splice) = part {
                        in_expr(&splice.expr, max);
                    }
                }
            }
            _ => {}
        }
    }
    let mut max = 0;
    for stmt in &block.stmts {
        if let ast::Stmt::Let(l) = stmt {
            in_expr(&l.value, &mut max);
        }
    }
    if let Some(tail) = &block.tail {
        in_expr(tail, &mut max);
    }
    max
}

#[cfg(test)]
pub(crate) mod tests {
    use super::super::driver::{
        MachineExecBackend, MachineExecRequest, MachinePathDemand, MachinePendingRun, StepCommand,
    };
    use super::*;
    use crate::exec::{ExecEvent, Outcome, ReadSet, Tree};
    use crate::fetch::{FakeFetchBackend, sha256_hex};
    use sha2::{Digest, Sha256};
    use std::cell::RefCell;
    use std::collections::{BTreeMap, BTreeSet};
    use std::rc::Rc;
    use std::sync::Arc;

    const CORPUS: &str = r#"
fn square(x: Int) -> Int { x * x }

fn twice_sq(n: Int) -> Int { square(n) + square(n) }

pub fn poly(n: Int) -> Int {
    let t = twice_sq(n + 1);
    t - n
}
"#;

    fn lanes() -> Vec<Lane> {
        let mut lanes = vec![Lane::Interp];
        #[cfg(any(test, feature = "jit"))]
        lanes.push(Lane::Jit);
        lanes
    }

    fn load_with_lane(source: &str, lane: Lane) -> Machine {
        Machine::load_with_lane(source, lane).unwrap_or_else(|err| {
            panic!("loads on {lane:?}: {err}");
        })
    }

    fn crate_sample_source() -> String {
        format!(
            "{}\n\n{}",
            include_str!("../../../rodin/rodin.vix"),
            include_str!("../../../playgrounds/snark/src/bundled/vix/samples/crate.vix")
        )
    }

    #[derive(Default)]
    struct DeferredExecBackend;

    impl MachineExecBackend for DeferredExecBackend {
        fn spawn(&self, request: MachineExecRequest) -> Result<Arc<dyn MachinePendingRun>, String> {
            Ok(Arc::new(DeferredRun { request }))
        }
    }

    struct DeferredRun {
        request: MachineExecRequest,
    }

    impl MachinePendingRun for DeferredRun {
        fn demand_path(&self, path: &str) -> Result<MachinePathDemand, String> {
            Ok(MachinePathDemand::FinishRequired {
                path: path.to_string(),
            })
        }

        fn flush(&self) -> Result<(Outcome, ExecEvent), String> {
            let mut tree = Tree::default();
            tree.entries.insert(
                self.request.output.clone(),
                format!("deferred({})", self.request.output),
            );
            Ok((
                Outcome {
                    outputs: tree,
                    read_set: ReadSet {
                        entries: BTreeMap::new(),
                    },
                    tree_events: Vec::new(),
                },
                ExecEvent::Ran,
            ))
        }
    }

    fn load_modules_with_lane(
        root: &str,
        modules: BTreeMap<String, String>,
        lane: Lane,
    ) -> Machine {
        Machine::load_modules_with_lane(root, modules, lane).unwrap_or_else(|err| {
            panic!("module set loads on {lane:?}: {err}");
        })
    }

    #[test]
    fn sealed_values_render_metadata_but_not_plaintext() {
        let source = r#"
use vix::Sealed;

pub fn secret() -> Sealed {
    Sealed::seal("ciphertext", "secret.db", "alice", "db-v1")
}
"#;
        for lane in lanes() {
            let mut machine = load_with_lane(source, lane);
            let secret = machine.demand_i64("secret", Vec::new()).unwrap();
            let RenderedValue::Sealed {
                taint,
                recipient,
                identity_hash,
                content_tag,
            } = machine.render_result("secret", secret).unwrap()
            else {
                panic!("secret did not render as sealed metadata on {lane:?}");
            };
            assert_eq!(taint, "secret.db");
            assert_eq!(recipient, "alice");
            assert_eq!(content_tag.as_deref(), Some("db-v1"));
            assert_eq!(identity_hash.len(), 64);
            assert!(!identity_hash.contains("ciphertext"));
        }
    }

    #[test]
    fn sealed_taint_propagates_through_concat_and_blocks_plaintext_render() {
        let source = r#"
use vix::Sealed;

pub fn derived() -> String {
    let secret: Sealed = Sealed::seal("ciphertext", "secret.db", "alice");
    "prefix-" + secret
}
"#;
        for lane in lanes() {
            let mut machine = load_with_lane(source, lane);
            let derived = machine.demand_i64("derived", Vec::new()).unwrap();
            let err = machine.render_result("derived", derived).unwrap_err();
            assert!(
                err.contains("refusing to render tainted String as plaintext"),
                "{lane:?}: {err}"
            );
            assert!(err.contains("secret.db"), "{lane:?}: {err}");
        }
    }

    #[test]
    fn sealed_taint_propagates_through_template_holes_and_maps() {
        let source = r#"
use vix::{Map, Sealed};

pub fn templated() -> String {
    let secret: Sealed = Sealed::seal("ciphertext", "secret.template", "alice");
    let bindings: Map<String, Sealed> = {};
    let bindings = bindings.insert("secret", secret);
    render(tmpl"token={{ secret }}", bindings)
}
"#;
        for lane in lanes() {
            let mut machine = load_with_lane(source, lane);
            let templated = machine.demand_i64("templated", Vec::new()).unwrap();
            let err = machine.render_result("templated", templated).unwrap_err();
            assert!(
                err.contains("refusing to render tainted String as plaintext"),
                "{lane:?}: {err}"
            );
            assert!(err.contains("secret.template"), "{lane:?}: {err}");
        }
    }

    #[test]
    fn declassify_is_the_explicit_taint_removal_seam() {
        let source = r#"
use vix::Sealed;

pub fn opened() -> String {
    let secret: Sealed = Sealed::seal("ciphertext", "secret.db", "test");
    Sealed::declassify(secret)
}
"#;
        for lane in lanes() {
            let mut machine = load_with_lane(source, lane);
            let opened = machine.demand_i64("opened", Vec::new()).unwrap();
            let RenderedValue::String { value } = machine.render_result("opened", opened).unwrap()
            else {
                panic!("opened did not render as String on {lane:?}");
            };
            assert_eq!(value, "ciphertext");
        }
    }

    fn modules(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
        entries
            .iter()
            .map(|(path, source)| ((*path).to_string(), (*source).to_string()))
            .collect()
    }

    fn lua_fetch_backend() -> FakeFetchBackend {
        FakeFetchBackend::new().with_archive(
            "https://www.lua.org/ftp/lua-5.4.8.tar.gz",
            b"lua-5.4.8 fixture archive",
            Tree::of(&[
                ("lua-5.4.8/src/lua.h", "// lua.h api"),
                (
                    "lua-5.4.8/src/lua.c",
                    "#include \"lua.h\"\n// interpreter main",
                ),
                ("lua-5.4.8/src/lapi.c", "#include \"lua.h\"\n// api impl"),
                ("lua-5.4.8/src/lauxlib.c", "#include \"lua.h\"\n// aux lib"),
                (
                    "lua-5.4.8/src/luac.c",
                    "#include \"lua.h\"\n// compiler main",
                ),
            ]),
        )
    }

    fn mini_vendored_tree(readme: &str) -> Tree {
        Tree::of(&[
            (
                "Cargo.toml",
                include_str!(
                    "../../../playgrounds/snark/src/bundled/vix/samples/fixtures/mini_vendored/Cargo.toml"
                ),
            ),
            (
                "src/lib.rs",
                include_str!(
                    "../../../playgrounds/snark/src/bundled/vix/samples/fixtures/mini_vendored/src/lib.rs"
                ),
            ),
            ("README.md", readme),
        ])
    }

    fn tiny_oci_layout() -> (Tree, String) {
        let config =
            r#"{"config":{"Env":["A=base","B=top"],"Entrypoint":["/bin/app"],"Cmd":["--serve"]}}"#;
        let config_digest = digest(config.as_bytes());
        let config_path = blob_path(&config_digest);

        let base = tar(&[
            ("etc/message", b"base\n".as_slice()),
            ("etc/remove", b"remove me\n".as_slice()),
            ("bin/app", b"not-an-elf\n".as_slice()),
        ]);
        let overlay = tar(&[("etc/message", b"overlay\n".as_slice())]);
        let whiteout = tar(&[("etc/.wh.remove", b"".as_slice())]);
        let base_digest = digest(base.as_bytes());
        let overlay_digest = digest(overlay.as_bytes());
        let whiteout_digest = digest(whiteout.as_bytes());

        let manifest = format!(
            r#"{{"schemaVersion":2,"config":{{"mediaType":"application/vnd.oci.image.config.v1+json","digest":"{config_digest}","size":{config_size}}},"layers":[{{"mediaType":"application/vnd.oci.image.layer.v1.tar","digest":"{base_digest}","size":{base_size}}},{{"mediaType":"application/vnd.oci.image.layer.v1.tar","digest":"{overlay_digest}","size":{overlay_size}}},{{"mediaType":"application/vnd.oci.image.layer.v1.tar","digest":"{whiteout_digest}","size":{whiteout_size}}}]}}"#,
            config_size = config.len(),
            base_size = base.len(),
            overlay_size = overlay.len(),
            whiteout_size = whiteout.len(),
        );
        let manifest_digest = digest(manifest.as_bytes());
        let manifest_path = blob_path(&manifest_digest);
        let index = format!(
            r#"{{"schemaVersion":2,"manifests":[{{"mediaType":"application/vnd.oci.image.manifest.v1+json","digest":"{manifest_digest}","size":{manifest_size}}}]}}"#,
            manifest_size = manifest.len(),
        );
        (
            Tree::of(&[
                ("oci-layout", r#"{"imageLayoutVersion":"1.0.0"}"#),
                ("index.json", index.as_str()),
                (manifest_path.as_str(), manifest.as_str()),
                (config_path.as_str(), config),
                (blob_path(&base_digest).as_str(), base.as_str()),
                (blob_path(&overlay_digest).as_str(), overlay.as_str()),
                (blob_path(&whiteout_digest).as_str(), whiteout.as_str()),
            ]),
            overlay_digest,
        )
    }

    fn oci_layout_with_libc(libc: &[u8]) -> Tree {
        let config = r#"{"config":{"Env":[],"Entrypoint":[],"Cmd":[]}}"#;
        let config_digest = digest(config.as_bytes());
        let config_path = blob_path(&config_digest);
        let layer = tar_blob(&[("usr/lib/libc.so.6", libc)]);
        let layer_digest = digest(&layer);
        let manifest = format!(
            r#"{{"schemaVersion":2,"config":{{"mediaType":"application/vnd.oci.image.config.v1+json","digest":"{config_digest}","size":{config_size}}},"layers":[{{"mediaType":"application/vnd.oci.image.layer.v1.tar","digest":"{layer_digest}","size":{layer_size}}}]}}"#,
            config_size = config.len(),
            layer_size = layer.len(),
        );
        let manifest_digest = digest(manifest.as_bytes());
        let manifest_path = blob_path(&manifest_digest);
        let index = format!(
            r#"{{"schemaVersion":2,"manifests":[{{"mediaType":"application/vnd.oci.image.manifest.v1+json","digest":"{manifest_digest}","size":{manifest_size}}}]}}"#,
            manifest_size = manifest.len(),
        );
        Tree {
            entries: BTreeMap::from([
                (
                    "oci-layout".to_string(),
                    r#"{"imageLayoutVersion":"1.0.0"}"#.to_string(),
                ),
                ("index.json".to_string(), index),
                (manifest_path, manifest),
                (config_path, config.to_string()),
            ]),
            blobs: BTreeMap::from([(blob_path(&layer_digest), layer)]),
        }
    }

    fn digest(bytes: &[u8]) -> String {
        let mut hasher = Sha256::new();
        hasher.update(bytes);
        format!("sha256:{}", hex::encode(hasher.finalize()))
    }

    fn blob_path(digest: &str) -> String {
        let (algorithm, hex) = digest.split_once(':').expect("digest algorithm");
        format!("blobs/{algorithm}/{hex}")
    }

    fn tar(entries: &[(&str, &[u8])]) -> String {
        String::from_utf8(tar_blob(entries)).expect("test tar is valid UTF-8")
    }

    fn tar_blob(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut out = Vec::new();
        for (path, contents) in entries {
            let mut header = [0u8; 512];
            assert!(path.len() <= 100, "test tar path too long: {path}");
            header[..path.len()].copy_from_slice(path.as_bytes());
            header[100..108].copy_from_slice(b"0000644\0");
            header[108..116].copy_from_slice(b"0000000\0");
            header[116..124].copy_from_slice(b"0000000\0");
            write_octal(&mut header[124..136], contents.len() as u64);
            header[136..148].copy_from_slice(b"00000000000\0");
            header[148..156].fill(b' ');
            header[156] = b'0';
            header[257..263].copy_from_slice(b"ustar\0");
            header[263..265].copy_from_slice(b"00");
            let checksum: u32 = header.iter().map(|byte| u32::from(*byte)).sum();
            write_checksum(&mut header[148..156], checksum);
            out.extend_from_slice(&header);
            out.extend_from_slice(contents);
            out.resize(out.len().div_ceil(512) * 512, 0);
        }
        out.resize(out.len() + 1024, 0);
        out
    }

    fn write_octal(dst: &mut [u8], value: u64) {
        let text = format!("{value:011o}\0");
        dst.copy_from_slice(text.as_bytes());
    }

    fn write_checksum(dst: &mut [u8], value: u32) {
        let text = format!("{value:06o}\0 ");
        dst.copy_from_slice(text.as_bytes());
    }

    fn tree_archive(tree: &Tree) -> String {
        let entries = tree
            .entries
            .iter()
            .map(|(path, contents)| (path.as_str(), contents.as_bytes()))
            .collect::<Vec<_>>();
        tar(&entries)
    }

    fn two_crate_graph_tree() -> Tree {
        Tree::of(&[
            (
                "app/Cargo.toml",
                include_str!(
                    "../../../playgrounds/snark/src/bundled/vix/samples/fixtures/two_crate_graph/app/Cargo.toml"
                ),
            ),
            (
                "app/src/main.rs",
                include_str!(
                    "../../../playgrounds/snark/src/bundled/vix/samples/fixtures/two_crate_graph/app/src/main.rs"
                ),
            ),
            (
                "crates/helper/Cargo.toml",
                include_str!(
                    "../../../playgrounds/snark/src/bundled/vix/samples/fixtures/two_crate_graph/crates/helper/Cargo.toml"
                ),
            ),
            (
                "crates/helper/src/lib.rs",
                include_str!(
                    "../../../playgrounds/snark/src/bundled/vix/samples/fixtures/two_crate_graph/crates/helper/src/lib.rs"
                ),
            ),
        ])
    }

    fn proc_macro_graph_tree() -> Tree {
        Tree::of(&[
            (
                "app/Cargo.toml",
                include_str!(
                    "../../../playgrounds/snark/src/bundled/vix/samples/fixtures/proc_macro_graph/app/Cargo.toml"
                ),
            ),
            (
                "app/src/main.rs",
                include_str!(
                    "../../../playgrounds/snark/src/bundled/vix/samples/fixtures/proc_macro_graph/app/src/main.rs"
                ),
            ),
            (
                "crates/emit_answer_macro/Cargo.toml",
                include_str!(
                    "../../../playgrounds/snark/src/bundled/vix/samples/fixtures/proc_macro_graph/crates/emit_answer_macro/Cargo.toml"
                ),
            ),
            (
                "crates/emit_answer_macro/src/lib.rs",
                include_str!(
                    "../../../playgrounds/snark/src/bundled/vix/samples/fixtures/proc_macro_graph/crates/emit_answer_macro/src/lib.rs"
                ),
            ),
        ])
    }

    #[test]
    fn the_scalar_corpus_runs_on_the_machine() {
        for lane in lanes() {
            let mut m = load_with_lane(CORPUS, lane);
            // poly(3): square(4)=16 twice -> 32; 32 - 3 = 29.
            assert_eq!(m.demand_i64("poly", vec![3]).unwrap(), 29, "{lane:?}");
        }
    }

    #[test]
    fn negative_int_words_are_not_molten_handles() {
        let src = r#"
fn id(n: Int) -> Int { n }

fn negative() -> Int { 0 - 1 }

fn pass_negative() -> Int { id(negative()) }
"#;
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            assert_eq!(
                machine.demand_i64("negative", vec![]).unwrap(),
                -1,
                "{lane:?}"
            );
            assert_eq!(
                machine.demand_i64("pass_negative", vec![]).unwrap(),
                -1,
                "{lane:?}"
            );
        }
    }

    #[test]
    fn fib_runs_linear_on_the_machine() {
        let src = r#"
fn fib(n: Int) -> Int {
    match n {
        0 => 0,
        1 => 1,
        _ => fib(n - 1) + fib(n - 2),
    }
}
"#;
        let mut traces = Vec::new();
        for lane in lanes() {
            let mut m = load_with_lane(src, lane);
            assert_eq!(
                host_call_count(&m, "fib", PENDING_COERCE_HOST),
                0,
                "scalar fib lane has no read barriers on {lane:?}"
            );
            assert_eq!(m.demand_i64("fib", vec![20]).unwrap(), 6765, "{lane:?}");
            let spawns = m
                .trace()
                .iter()
                .filter(|e| matches!(e, DriveEvent::Spawned { .. }))
                .count();
            // fib(0)..fib(20): 21 distinct invocations, 21 spawns — LINEAR.
            // Naive recursion runs 13,529 more bodies than this.
            assert_eq!(spawns, 21, "{lane:?}");
            traces.push((lane, m.trace().to_vec()));
        }
        assert_lane_traces_equal(&traces);
    }

    #[test]
    fn binding_arms_bind_the_scrutinee() {
        let src = r#"
fn f(n: Int) -> Int {
    match n {
        0 => 7,
        m => m * 2,
    }
}
"#;
        for lane in lanes() {
            let mut m = load_with_lane(src, lane);
            assert_eq!(m.demand_i64("f", vec![0]).unwrap(), 7, "{lane:?}");
            assert_eq!(m.demand_i64("f", vec![21]).unwrap(), 42, "{lane:?}");
        }
    }

    #[test]
    fn refutable_matches_without_irrefutable_tail_are_rejected() {
        let src = "fn f(n: Int) -> Int { match n { 0 => 1, 1 => 2 } }";
        for lane in lanes() {
            let err = Machine::load_with_lane(src, lane)
                .and_then(|mut m| m.demand_i64("f", vec![0]))
                .unwrap_err();
            assert!(err.contains("irrefutable"), "{lane:?}: {err}");
        }
    }

    #[test]
    fn bool_literal_patterns_are_exhaustive_as_true_plus_false() {
        let src = "fn f(b: Bool) -> Int { match b { true => 1, false => 0 } }";
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            assert_eq!(
                machine
                    .call(
                        "f",
                        &[NamedArg {
                            name: "b".to_string(),
                            value: MachineArg::Bool(true),
                        }],
                    )
                    .unwrap()
                    .0,
                1,
                "{lane:?}"
            );
            assert_eq!(
                machine
                    .call(
                        "f",
                        &[NamedArg {
                            name: "b".to_string(),
                            value: MachineArg::Bool(false),
                        }],
                    )
                    .unwrap()
                    .0,
                0,
                "{lane:?}"
            );
        }
    }

    #[test]
    fn bool_literal_patterns_missing_false_are_rejected() {
        let src = "fn f(b: Bool) -> Int { match b { true => 1 } }";
        for lane in lanes() {
            let err = Machine::load_with_lane(src, lane)
                .and_then(|mut m| {
                    m.call(
                        "f",
                        &[NamedArg {
                            name: "b".to_string(),
                            value: MachineArg::Bool(true),
                        }],
                    )
                    .map(|_| ())
                })
                .unwrap_err();
            assert!(err.contains("missing false"), "{lane:?}: {err}");
        }
    }

    #[test]
    fn float_literals_lower_as_canonical_bits() {
        for lane in lanes() {
            let mut m = load_with_lane("fn f() -> Float { 1.5 }", lane);
            assert_eq!(
                (m.demand_i64("f", vec![]).unwrap() as u64),
                1.5f64.to_bits(),
                "{lane:?}"
            );
        }
    }

    #[test]
    fn molten_reuse_is_unobservable_for_aggregate_updates() {
        let cases = [
            (
                "array",
                r#"
pub fn main() -> Int {
    ([0].push(1).push(2).pop()).0
}
"#,
                2,
            ),
            (
                "map",
                r#"
pub fn main() -> Int {
    let m: Map<String, Int> = {};
    m.insert("a", 1).insert("b", 2).get("b").unwrap() + 1
}
"#,
                3,
            ),
            (
                "record",
                r#"
struct Pair { left: Int, right: Int }

pub fn main() -> Int {
    let p = Pair { left: 1, right: 2 };
    let q = Pair { right: 3, ..p };
    q.left + q.right
}
"#,
                4,
            ),
        ];
        for lane in lanes() {
            for (name, src, expected) in cases {
                let mut reuse = Machine::load_with_lane(src, lane).unwrap();
                reuse.driver.set_force_molten_copy(false);
                let reuse_result = reuse.demand_i64("main", vec![]).unwrap();
                let reuse_trace = reuse.driver.trace.clone();
                let reuse_bundle = reuse
                    .driver
                    .export_value_bundle(reuse_result, Vec::new())
                    .unwrap();

                let mut copy = Machine::load_with_lane(src, lane).unwrap();
                copy.driver.set_force_molten_copy(true);
                let copy_result = copy.demand_i64("main", vec![]).unwrap();
                let copy_trace = copy.driver.trace.clone();
                let copy_bundle = copy
                    .driver
                    .export_value_bundle(copy_result, Vec::new())
                    .unwrap();

                assert_eq!(reuse_result, expected, "{lane:?} {name}");
                assert_eq!(reuse_result, copy_result, "{lane:?} {name}");
                assert_eq!(reuse_trace, copy_trace, "{lane:?} {name}");
                assert_eq!(reuse_bundle.values, copy_bundle.values, "{lane:?} {name}");
            }
        }
    }

    #[test]
    fn map_get_cache_observes_insert_after_get_after_insert() {
        let src = r#"
pub fn main() -> Int {
    let m: Map<String, Int> = {};
    let m = m.insert("a", 1);
    let first = m.get("a").unwrap();
    let m = m.insert("b", 2);
    let second = m.get("b").unwrap();
    let still_first = m.get("a").unwrap();
    first * 100 + second * 10 + still_first
}
"#;
        for lane in lanes() {
            let mut reuse = Machine::load_with_lane(src, lane).unwrap();
            reuse.driver.set_force_molten_copy(false);
            let reuse_result = reuse.demand_i64("main", vec![]).unwrap();
            let reuse_trace = reuse.driver.trace.clone();
            let reuse_bundle = reuse
                .driver
                .export_value_bundle(reuse_result, Vec::new())
                .unwrap();

            let mut copy = Machine::load_with_lane(src, lane).unwrap();
            copy.driver.set_force_molten_copy(true);
            let copy_result = copy.demand_i64("main", vec![]).unwrap();
            let copy_trace = copy.driver.trace.clone();
            let copy_bundle = copy
                .driver
                .export_value_bundle(copy_result, Vec::new())
                .unwrap();

            assert_eq!(reuse_result, 121, "{lane:?}");
            assert_eq!(reuse_result, copy_result, "{lane:?}");
            assert_eq!(reuse_trace, copy_trace, "{lane:?}");
            assert_eq!(reuse_bundle.values, copy_bundle.values, "{lane:?}");
        }
    }

    #[test]
    fn typed_array_pop_preserves_remaining_array_schema() {
        let src = r#"
pub fn main() -> Int {
    let stack: Array<Int> = [0].push(1).push(2);
    (stack.pop().1).len()
}
"#;
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            assert_eq!(machine.demand_i64("main", vec![]).unwrap(), 2, "{lane:?}");
        }
    }

    #[test]
    fn empty_arrays_keep_their_declared_element_identity() {
        let src = r#"
pub fn ints() -> [Int] { [] }
pub fn strings() -> [String] { [] }
"#;
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            let ints = machine.demand_i64("ints", vec![]).unwrap();
            let strings = machine.demand_i64("strings", vec![]).unwrap();
            assert_ne!(ints, strings, "{lane:?}");

            let int_entry = machine.driver.store_entry(ints).expect("int array");
            let string_entry = machine.driver.store_entry(strings).expect("string array");
            assert_eq!(int_entry.schema, "Array<Int>", "{lane:?}");
            assert_eq!(string_entry.schema, "Array<String>", "{lane:?}");
            assert_ne!(
                int_entry.content_hash, string_entry.content_hash,
                "{lane:?}"
            );

            let (int_elem, int_words) = machine.driver.array_words(ints).unwrap();
            let (string_elem, string_words) = machine.driver.array_words(strings).unwrap();
            assert_eq!(int_elem, "Int", "{lane:?}");
            assert_eq!(string_elem, "String", "{lane:?}");
            assert!(int_words.is_empty(), "{lane:?}");
            assert!(string_words.is_empty(), "{lane:?}");
        }
    }

    #[test]
    fn derived_container_descriptors_are_keyed_by_full_schema_ref() {
        let modules = BTreeMap::from([(
            "root".to_string(),
            r#"
pub fn ints() -> [Int] { [] }
pub fn strings() -> [String] { [] }
"#
            .to_string(),
        )]);
        let compiled = compile_module_set(
            "root",
            &modules,
            RefSource::Fresh,
            LowerOptions {
                force_tail_invoke: false,
            },
        )
        .unwrap();

        let int_descriptor = compiled
            .descriptors
            .get("Array<Int>")
            .expect("Array<Int> descriptor");
        let string_descriptor = compiled
            .descriptors
            .get("Array<String>")
            .expect("Array<String> descriptor");

        assert_eq!(
            compiled.schemas.display_ref(&int_descriptor.schema),
            "Array<Int>"
        );
        assert_eq!(
            compiled.schemas.display_ref(&string_descriptor.schema),
            "Array<String>"
        );
        assert_ne!(int_descriptor.schema, string_descriptor.schema);
    }

    #[test]
    fn molten_consuming_rebind_preserves_pending_aliases() {
        let src = r#"
pub fn main() -> Int {
    let a = [1];
    let b = a;
    let a = a.push(2);
    a.len() * 10 + b.len()
}
"#;
        for lane in lanes() {
            let mut reuse = Machine::load_with_lane(src, lane).unwrap();
            reuse.driver.set_force_molten_copy(false);
            let reuse_result = reuse.demand_i64("main", vec![]).unwrap();
            let reuse_stats = reuse.driver.molten_stats();

            let mut copy = Machine::load_with_lane(src, lane).unwrap();
            copy.driver.set_force_molten_copy(true);
            let copy_result = copy.demand_i64("main", vec![]).unwrap();

            assert_eq!(reuse_result, 21, "{lane:?}");
            assert_eq!(reuse_result, copy_result, "{lane:?}");
            assert_eq!(reuse_stats.array_push_reused, 0, "{lane:?}");
            assert_eq!(reuse_stats.array_push_copied, 1, "{lane:?}");
        }
    }

    #[test]
    fn molten_consuming_rebind_reuses_array_push_receiver() {
        let src = r#"
pub fn main() -> Int {
    let x = [0];
    let x = x.push(1);
    let x = x.push(2);
    let x = x.push(3);
    x.len()
}
"#;
        for lane in lanes() {
            let mut reuse = Machine::load_with_lane(src, lane).unwrap();
            reuse.driver.set_force_molten_copy(false);
            let reuse_result = reuse.demand_i64("main", vec![]).unwrap();
            let reuse_stats = reuse.driver.molten_stats();

            let mut copy = Machine::load_with_lane(src, lane).unwrap();
            copy.driver.set_force_molten_copy(true);
            let copy_result = copy.demand_i64("main", vec![]).unwrap();
            let copy_stats = copy.driver.molten_stats();

            assert_eq!(reuse_result, 4, "{lane:?}");
            assert_eq!(reuse_result, copy_result, "{lane:?}");
            assert_eq!(reuse_stats.array_push_reused, 3, "{lane:?}");
            assert_eq!(reuse_stats.array_push_copied, 0, "{lane:?}");
            assert_eq!(copy_stats.array_push_reused, 0, "{lane:?}");
            assert_eq!(copy_stats.array_push_copied, 3, "{lane:?}");
        }
    }

    #[test]
    fn molten_array_carried_hash_matches_from_scratch_after_many_updates() {
        let mut expr = "[0, 1, 2, 3]".to_string();
        let mut len = 4usize;
        let mut seed = 0x1234_5678_u64;
        for step in 0..32 {
            seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1);
            match seed % 3 {
                0 => {
                    expr = format!("({expr}).push({})", step + 10);
                    len += 1;
                }
                1 if len > 0 => {
                    let index = (seed as usize >> 8) % len;
                    expr = format!("({expr}).set({index}, {})", step + 20);
                }
                _ if len > 1 => {
                    expr = format!("(({expr}).pop()).1");
                    len -= 1;
                }
                _ => {
                    expr = format!("({expr}).push({})", step + 30);
                    len += 1;
                }
            }
        }
        let src = format!("pub fn main() -> [Int] {{\n    {expr}\n}}\n");
        for lane in lanes() {
            let mut reuse = Machine::load_with_lane(&src, lane).unwrap();
            reuse.driver.set_force_molten_copy(false);
            let reuse_result = reuse.demand_i64("main", vec![]).unwrap();
            let reuse_bundle = reuse
                .driver
                .export_value_bundle(reuse_result, Vec::new())
                .unwrap();

            let mut copy = Machine::load_with_lane(&src, lane).unwrap();
            copy.driver.set_force_molten_copy(true);
            let copy_result = copy.demand_i64("main", vec![]).unwrap();
            let copy_bundle = copy
                .driver
                .export_value_bundle(copy_result, Vec::new())
                .unwrap();

            assert_eq!(reuse_bundle.values, copy_bundle.values, "{lane:?}");
        }
    }

    #[test]
    fn molten_tail_loop_array_accumulator_reuses_push_receiver() {
        let src = r#"
pub fn seed() -> [Int] {
    [0]
}

pub fn grow(n: Int, acc: [Int]) -> [Int] {
    match n {
        0 => acc,
        _ => grow(n - 1, acc.push(n)),
    }
}
"#;
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            let grow = FnRef::new(*machine.fn_refs.get("grow").expect("grow fn ref"));
            let ops = machine.driver.fn_ops(grow).to_vec();
            let dup_hosts = ops
                .iter()
                .filter(|op| {
                    matches!(
                        op,
                        Op::HostCall {
                            host: MOLTEN_DUP_HOST
                        }
                    )
                })
                .count();
            let push_hosts = ops
                .iter()
                .filter(|op| {
                    matches!(
                        op,
                        Op::HostCall {
                            host: ARRAY_PUSH_HOST
                        }
                    )
                })
                .count();
            let seed = machine.demand_i64("seed", vec![]).unwrap();
            let result = machine.demand_i64("grow", vec![256, seed]).unwrap();
            let (_elem, words) = machine.driver.array_words(result).unwrap();
            let stats = machine.driver.molten_stats();

            assert_eq!(push_hosts, 1, "{lane:?}");
            assert_eq!(dup_hosts, 1, "{lane:?}: {ops:#?}");
            assert_eq!(words.len(), 257, "{lane:?}");
            assert_eq!(stats.array_push_reused, 255, "{lane:?}");
            assert_eq!(stats.array_push_copied, 1, "{lane:?}");
        }
    }

    #[test]
    fn recursive_enum_tree_evaluates_on_the_machine() {
        let src = r#"
enum Expr {
    Num(Int),
    Add(Expr, Expr),
    Mul(Expr, Expr),
}

fn eval(e: Expr) -> Int {
    match e {
        Expr::Num(n) => n,
        Expr::Add(a, b) => eval(a) + eval(b),
        Expr::Mul(a, b) => eval(a) * eval(b),
    }
}

fn main() -> Int {
    eval(Expr::Add(Expr::Num(2), Expr::Mul(Expr::Num(3), Expr::Num(4))))
}
"#;
        for lane in lanes() {
            let mut m = load_with_lane(src, lane);
            assert_eq!(m.demand_i64("main", vec![]).unwrap(), 14, "{lane:?}");
        }
    }

    #[test]
    fn strings_are_interned_and_match_by_handle() {
        let src = r#"
fn classify() -> Int {
    let a = "same";
    let b = "same";
    match a {
        "same" => 42,
        _ => 0,
    }
}
"#;
        for lane in lanes() {
            let mut m = load_with_lane(src, lane);
            assert_eq!(
                m.store_len(),
                1,
                "two identical literals intern once on {lane:?}"
            );
            assert_eq!(m.demand_i64("classify", vec![]).unwrap(), 42, "{lane:?}");
            assert_eq!(
                m.store_len(),
                1,
                "string matching does not allocate on {lane:?}"
            );
        }
    }

    #[test]
    fn maps_are_canonical_regardless_of_insertion_order() {
        let src = r#"
fn ab() -> Map<String, Float> {
    let m: Map<String, Float> = {};
    m.insert("a", 1.0).insert("b", 2.0)
}

fn ba() -> Map<String, Float> {
    let m: Map<String, Float> = {};
    m.insert("b", 2.0).insert("a", 1.0)
}
"#;
        for lane in lanes() {
            let mut m = load_with_lane(src, lane);
            let ab = m.demand_i64("ab", vec![]).unwrap();
            let ba = m.demand_i64("ba", vec![]).unwrap();
            let ab_entry = m.driver.store_entry(ab).expect("ab entry");
            let ba_entry = m.driver.store_entry(ba).expect("ba entry");
            assert_eq!(ab_entry.content_hash, ba_entry.content_hash, "{lane:?}");
            assert_eq!(
                ab, ba,
                "dedupe returns the same canonical handle on {lane:?}"
            );
        }
    }

    #[test]
    fn option_unwrap_none_is_a_machine_error() {
        let src = r#"
fn missing() -> Float {
    let m: Map<String, Float> = {};
    m.get("missing").unwrap()
}
"#;
        for lane in lanes() {
            let err = Machine::load_with_lane(src, lane)
                .and_then(|mut machine| machine.demand_i64("missing", vec![]))
                .unwrap_err();
            assert!(err.contains("unwrap on None"), "{lane:?}: {err}");
        }
    }

    #[test]
    fn concrete_tree_missing_path_errors_without_runs() {
        let src = r#"
use vix::Tree;

pub fn main(input: Tree) -> Tree {
    input / p"missing.txt"
}
"#;
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            let input =
                machine.intern_tree_concrete(crate::exec::Tree::of(&[("present.txt", "ok")]));
            let err = machine
                .demand_i64("main", vec![input])
                .expect_err("missing path errors");
            assert!(err.contains("missing.txt"), "{lane:?}: {err}");
            assert_eq!(run_requested_count(&machine), 0, "{lane:?}");
            assert_eq!(completed_outputs(&machine), Vec::<u64>::new(), "{lane:?}");
        }
    }

    #[test]
    fn pending_tree_projection_serves_file_through_one_run() {
        let src = r#"
use vix::Target;
use caps::Cc;

pub fn main(target: Target) -> Tree {
    let cc = Cc::acquire(target);
    cc! { -o {p"artifact.o"} } / p"artifact.o"
}
"#;
        let mut traces = Vec::new();
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            let target = machine.linux_target_handle();
            let handle = machine.demand_i64("main", vec![target]).unwrap();
            assert_eq!(
                machine
                    .tree_entries(handle)
                    .unwrap()
                    .keys()
                    .collect::<Vec<_>>(),
                vec![&"artifact.o".to_string()],
                "{lane:?}"
            );
            assert_eq!(run_requested_count(&machine), 1, "{lane:?}");
            assert_eq!(completed_outputs(&machine), Vec::<u64>::new(), "{lane:?}");
            traces.push((lane, machine.trace().to_vec()));
        }
        assert_lane_traces_equal(&traces);
    }

    #[test]
    fn pending_tree_missing_path_errors_after_one_run() {
        let src = r#"
use vix::Target;
use caps::Cc;

pub fn main(target: Target) -> Tree {
    let cc = Cc::acquire(target);
    cc! { -o {p"artifact.o"} } / p"never-written.o"
}
"#;
        let mut traces = Vec::new();
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            let target = machine.linux_target_handle();
            let err = machine
                .demand_i64("main", vec![target])
                .expect_err("missing produced path errors");
            assert!(err.contains("never-written.o"), "{lane:?}: {err}");
            assert_eq!(run_requested_count(&machine), 1, "{lane:?}");
            assert_eq!(
                completed_outputs(&machine),
                output_set(&["artifact.o"]),
                "{lane:?}"
            );
            traces.push((lane, machine.trace().to_vec()));
        }
        assert_lane_traces_equal(&traces);
    }

    #[test]
    fn machine_exec_mount_ceiling_errors_outside_declared_mounts() {
        let src = r#"
use vix::Target;
use caps::Cc;

pub fn bad(target: Target) -> Tree {
    let cc = Cc::acquire(target);
    cc! { -c {p"/m/unmounted/ghost.c"} -o {p"ghost.o"} }
}
"#;
        let mut traces = Vec::new();
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            let target = machine.linux_target_handle();
            let handle = machine.demand_i64("bad", vec![target]).unwrap();
            let err = machine
                .tree_entries(handle)
                .expect_err("undeclared input path is outside the mount ceiling");
            assert!(err.contains("outside the mounts"), "{lane:?}: {err}");
            assert_eq!(run_requested_count(&machine), 1, "{lane:?}");
            assert_eq!(
                started_outputs(&machine),
                output_set(&["ghost.o"]),
                "{lane:?}"
            );
            assert_eq!(completed_outputs(&machine), Vec::<u64>::new(), "{lane:?}");
            traces.push((lane, machine.trace().to_vec()));
        }
        assert_lane_traces_equal(&traces);
    }

    #[test]
    fn unused_command_binding_emits_no_exec_ops_or_runs() {
        let src = r#"
use vix::Target;
use caps::Cc;

pub fn main(target: Target) -> Int {
    let cc = Cc::acquire(target);
    let dead = cc! { -o {p"dead"} };
    7
}
"#;
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            assert_eq!(host_call_count(&machine, "main", EXEC_HOST), 0, "{lane:?}");
            let target = machine.linux_target_handle();
            assert_eq!(
                machine.demand_i64("main", vec![target]).unwrap(),
                7,
                "{lane:?}"
            );
            assert_eq!(run_requested_count(&machine), 0, "{lane:?}");
            assert_eq!(completed_outputs(&machine), Vec::<u64>::new(), "{lane:?}");
        }
    }

    #[test]
    fn types_vix_partials_depths_and_classify_run_on_the_machine() {
        let src = format!(
            "{}\n{}",
            include_str!("../../../playgrounds/snark/src/bundled/vix/samples/types.vix"),
            r#"
pub fn classify_lua() -> String {
    classify(Artifact::Object(p"lua.o"))
}

pub fn classify_lapi() -> String {
    classify(Artifact::Object(p"lapi.o"))
}
"#
        );
        let mut traces = Vec::new();
        for lane in lanes() {
            let mut machine = load_with_lane(&src, lane);
            assert_eq!(
                machine.demand_i64("partials", vec![]).unwrap(),
                42,
                "{lane:?}"
            );
            assert_eq!(machine.demand_i64("depths", vec![]).unwrap(), 2, "{lane:?}");

            let lua = machine.demand_i64("classify_lua", vec![]).unwrap();
            assert_eq!(
                machine.driver.raw_string(lua, "String").unwrap(),
                "the interpreter object",
                "{lane:?}"
            );
            let lapi = machine.demand_i64("classify_lapi", vec![]).unwrap();
            assert_eq!(
                machine.driver.raw_string(lapi, "String").unwrap(),
                "an object",
                "{lane:?}"
            );
            traces.push((lane, machine.trace().to_vec()));
        }
        assert_lane_traces_equal(&traces);
    }

    #[test]
    fn types_vix_named_argument_diagnostics_are_pinned() {
        let missing = r#"
fn scaled(k: Int, x: Int) -> Int { k * x }
pub fn main() -> Int { scaled(k: 2) }
"#;
        let duplicate = r#"
fn scaled(k: Int, x: Int) -> Int { k * x }
pub fn main() -> Int { scaled(k: 1, x: 2, x: 3) }
"#;
        for lane in lanes() {
            let err = match Machine::load_with_lane(missing, lane) {
                Ok(_) => panic!("missing argument source loaded on {lane:?}"),
                Err(err) => err,
            };
            assert!(
                err.contains("`scaled` missing argument(s): [\"x\"]"),
                "{lane:?}: {err}"
            );
            let err = match Machine::load_with_lane(duplicate, lane) {
                Ok(_) => panic!("duplicate argument source loaded on {lane:?}"),
                Err(err) => err,
            };
            assert!(err.contains("duplicate argument `x`"), "{lane:?}: {err}");
        }
    }

    #[test]
    fn match_guard_failure_skips_guarded_body() {
        let src = r#"
enum Artifact { Object(Path), Phony }

fn expensive() -> String { "bad" }

fn classify(a: Artifact) -> String {
    match a {
        Artifact::Object(p) if p == p"nope" => expensive(),
        Artifact::Object(_) => "ok",
        _ => "other",
    }
}

pub fn main() -> String {
    classify(Artifact::Object(p"lua.o"))
}
"#;
        let mut traces = Vec::new();
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            let result = machine.demand_i64("main", vec![]).unwrap();
            assert_eq!(
                machine.driver.raw_string(result, "String").unwrap(),
                "ok",
                "{lane:?}"
            );
            assert_eq!(spawned_count(&machine, "expensive"), 0, "{lane:?}");
            traces.push((lane, machine.trace().to_vec()));
        }
        assert_lane_traces_equal(&traces);
    }

    #[test]
    fn types_vix_toolchain_acquires_capabilities_and_updates_records() {
        const CC_PIN: &str = "acquire:Cc:7e66028935dab99";
        const AR_PIN: &str = "acquire:Ar:7e66028935dab99";

        let src = include_str!("../../../playgrounds/snark/src/bundled/vix/samples/types.vix");
        let mut traces = Vec::new();
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            let (target, _) = machine.driver.intern_windows_target().unwrap();
            let toolchain = machine.demand_i64("toolchain", vec![target]).unwrap();

            let cc = machine.driver.store_field(toolchain, 0).unwrap();
            let ar = machine.driver.store_field(toolchain, 1).unwrap();
            let opt = machine.driver.store_field(toolchain, 2).unwrap();
            let env = machine.driver.store_field(toolchain, 3).unwrap();
            assert_eq!(
                machine.driver.raw_string(cc, "Cc").unwrap(),
                CC_PIN,
                "{lane:?}"
            );
            assert_eq!(
                machine.driver.raw_string(ar, "Ar").unwrap(),
                AR_PIN,
                "{lane:?}"
            );
            assert_eq!(opt, 1, "{lane:?}");

            let env = machine
                .driver
                .map_words(env)
                .unwrap()
                .into_iter()
                .map(|(key_schema, key, value_schema, value, realization)| {
                    assert_eq!(key_schema, "String");
                    assert_eq!(value_schema, "String");
                    assert_eq!(realization, None);
                    (
                        machine.driver.raw_string(key, "String").unwrap(),
                        machine.driver.raw_string(value, "String").unwrap(),
                    )
                })
                .collect::<BTreeMap<_, _>>();
            assert_eq!(
                env,
                BTreeMap::from([
                    ("CFLAGS".to_string(), "-O2".to_string()),
                    ("LDFLAGS".to_string(), "-lm".to_string()),
                ]),
                "{lane:?}"
            );

            let observations = machine
                .trace()
                .iter()
                .filter_map(|event| match event {
                    DriveEvent::Observation { key, replayed, .. } => Some((*key, *replayed)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(
                observations,
                vec![(trace_hash(CC_PIN), false), (trace_hash(AR_PIN), false)],
                "{lane:?}"
            );
            traces.push((lane, machine.trace().to_vec()));
        }
        assert_lane_traces_equal(&traces);
    }

    #[test]
    fn capability_identity_includes_target_arch() {
        let src = r#"
use vix::{Target, Arch};
use caps::Cc;

pub fn cc_for(target: Target) -> Cc {
    Cc::acquire(target)
}

pub fn classify_arch(target: Target) -> Int {
    match target.arch {
        Arch::Aarch64 => 64,
        Arch::X86_64 => 86,
        _ => 0,
    }
}
"#;
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            let (linux_x86, _) = machine.driver.intern_target(0, 0).unwrap();
            let (linux_arm64, _) = machine.driver.intern_target(0, 1).unwrap();

            let x86_cc = machine.demand_i64("cc_for", vec![linux_x86]).unwrap();
            let arm64_cc = machine.demand_i64("cc_for", vec![linux_arm64]).unwrap();

            let x86_fingerprint = machine.driver.raw_string(x86_cc, "Cc").unwrap();
            let arm64_fingerprint = machine.driver.raw_string(arm64_cc, "Cc").unwrap();
            assert_ne!(x86_fingerprint, arm64_fingerprint, "{lane:?}");
            assert!(x86_fingerprint.starts_with("acquire:Cc:"), "{lane:?}");
            assert!(arm64_fingerprint.starts_with("acquire:Cc:"), "{lane:?}");
            assert_eq!(
                machine
                    .demand_i64("classify_arch", vec![linux_arm64])
                    .unwrap(),
                64,
                "{lane:?}"
            );
        }
    }

    #[test]
    fn target_host_and_cross_target_are_distinct_capabilities() {
        let src = r#"
use vix::{Target, Os, Arch};
use caps::{Cc, Rustc};

pub fn host_cc() -> Cc {
    Cc::acquire(Target::host())
}

pub fn cross_cc() -> Cc {
    Cc::acquire(Target { os: Os::Linux, arch: Arch::Aarch64 })
}

pub fn cross_x86_cc() -> Cc {
    Cc::acquire(Target { os: Os::Linux, arch: Arch::X86_64 })
}

pub fn os_only_cc() -> Cc {
    Cc::acquire(Target { os: Os::Linux })
}

pub fn cross_rustc() -> Rustc {
    Rustc::acquire(Target { os: Os::Linux, arch: Arch::Aarch64 })
}
"#;
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);

            let host_cc = machine.demand_i64("host_cc", vec![]).unwrap();
            let cross_cc = machine.demand_i64("cross_cc", vec![]).unwrap();
            let cross_x86_cc = machine.demand_i64("cross_x86_cc", vec![]).unwrap();
            let os_only_cc = machine.demand_i64("os_only_cc", vec![]).unwrap();
            let cross_rustc = machine.demand_i64("cross_rustc", vec![]).unwrap();

            let host_cc = machine.driver.raw_string(host_cc, "Cc").unwrap();
            let cross_cc = machine.driver.raw_string(cross_cc, "Cc").unwrap();
            let cross_x86_cc = machine.driver.raw_string(cross_x86_cc, "Cc").unwrap();
            let os_only_cc = machine.driver.raw_string(os_only_cc, "Cc").unwrap();
            let cross_rustc = machine.driver.raw_string(cross_rustc, "Rustc").unwrap();

            // This is the crate.vix slice-3b split: proc-macro producers acquire
            // host tools, target units acquire target tools, and artifact probes
            // can key ELF/OCI facts by architecture.
            assert!(
                host_cc != cross_cc || host_cc != cross_x86_cc,
                "{lane:?}: host={host_cc} cross_aarch64={cross_cc} cross_x86_64={cross_x86_cc}"
            );
            assert_ne!(cross_cc, cross_rustc, "{lane:?}");
            assert!(cross_cc.starts_with("acquire:Cc:"), "{lane:?}");
            assert!(cross_x86_cc.starts_with("acquire:Cc:"), "{lane:?}");
            assert!(os_only_cc.starts_with("acquire:Cc:"), "{lane:?}");
            assert!(cross_rustc.starts_with("acquire:Rustc:"), "{lane:?}");
        }
    }

    #[test]
    fn store_backed_array_reads_prevent_projection_reuse() {
        let src = r#"
fn append_join(values: [String], value: String) -> String {
    values.push(value).join(",")
}

pub fn first() -> String {
    append_join(["one"], "tail")
}

pub fn second() -> String {
    append_join(["two"], "tail")
}
"#;
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            let first = machine.demand_i64("first", vec![]).unwrap();
            assert_eq!(
                machine.driver.raw_string(first, "String").unwrap(),
                "one,tail"
            );
            let second = machine.demand_i64("second", vec![]).unwrap();
            assert_eq!(
                machine.driver.raw_string(second, "String").unwrap(),
                "two,tail"
            );
        }
    }

    #[test]
    fn lua_vix_runs_on_machine_with_exec_depth_contract() {
        const FETCH_PIN: &str = "fetch:https://www.lua.org/ftp/lua-5.4.8.tar.gz:sha256:f5c9123295667d2cc0841c03490f04d6e66d0eac5e440ab386a944eec30e64d7";
        let src = include_str!("../../../playgrounds/snark/src/bundled/vix/samples/lua.vix");
        let mut traces = Vec::new();
        for lane in lanes() {
            let mut machine = Machine::load_with_lane(src, lane)
                .unwrap()
                .with_fetch_backend(lua_fetch_backend());
            let target = machine.linux_target_handle();
            let handle = machine.demand_i64("lua", vec![target]).unwrap();
            let entries = machine.tree_entries(handle).unwrap();
            assert!(entries.contains_key("lua"), "{lane:?}: {entries:?}");
            assert!(entries["lua"].starts_with("obj("), "{lane:?}: {entries:?}");

            let requested = machine
                .trace()
                .iter()
                .filter(|event| matches!(event, DriveEvent::RunRequested { .. }))
                .count();
            let started = machine
                .trace()
                .iter()
                .filter(|event| matches!(event, DriveEvent::RunStarted { .. }))
                .count();
            let completed = machine
                .trace()
                .iter()
                .filter_map(|event| match event {
                    DriveEvent::RunCompleted {
                        command_name,
                        serving,
                        outputs,
                        ..
                    } => Some((command_name.as_str(), serving, outputs)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(requested, 5, "{lane:?}: {:?}", machine.trace());
            assert_eq!(started, 5, "{lane:?}: {:?}", machine.trace());
            assert_eq!(completed.len(), 3, "{lane:?}: {completed:?}");
            assert!(
                completed
                    .iter()
                    .all(|(command, serving, _)| *command == "cc" && **serving == ExecEvent::Ran),
                "{lane:?}: {completed:?}"
            );
            assert!(
                completed
                    .iter()
                    .any(|(_, _, outputs)| outputs.iter().any(|(path, _)| path == "lua")),
                "{lane:?}: {completed:?}"
            );
            let observations = machine
                .trace()
                .iter()
                .filter_map(|event| match event {
                    DriveEvent::Observation {
                        key_text, replayed, ..
                    } => Some((key_text.as_str(), *replayed)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert!(
                observations.contains(&(FETCH_PIN, false)),
                "{lane:?}: {observations:?}"
            );

            let lua_hash = machine.fn_hash("lua").expect("lua hash");
            machine.clear_trace();
            let warm = machine.demand_i64("lua", vec![target]).unwrap();
            assert_eq!(warm, handle, "{lane:?}");
            assert_eq!(
                machine.trace(),
                &[
                    DriveEvent::Demanded { fn_hash: lua_hash },
                    DriveEvent::MemoHit { fn_hash: lua_hash },
                ],
                "{lane:?}"
            );
            traces.push((lane, machine.trace().to_vec()));
        }
        assert_lane_traces_equal(&traces);
    }

    #[test]
    fn machine_fetch_without_declared_checksum_pins_and_replays() {
        const URL: &str = "https://example.org/source.tar.gz";
        let src = format!(
            r#"
use vix::Tree;

pub fn src_tree(nonce: Int) -> Tree {{
    fetch(url: "{URL}")
}}
"#
        );
        for lane in lanes() {
            let backend = FakeFetchBackend::new().with_archive(
                URL,
                b"source fixture archive",
                Tree::of(&[("src/lib.rs", "pub fn f() {}")]),
            );
            let mut machine = Machine::load_with_lane(&src, lane)
                .unwrap()
                .with_fetch_backend(backend);
            let first = machine.demand_i64("src_tree", vec![1]).unwrap();
            assert_eq!(
                machine.tree_entries(first).unwrap(),
                BTreeMap::from([("src/lib.rs".to_string(), "pub fn f() {}".to_string())]),
                "{lane:?}"
            );
            let second = machine.demand_i64("src_tree", vec![2]).unwrap();
            assert_eq!(first, second, "{lane:?}");
            let observations = machine
                .trace()
                .iter()
                .filter_map(|event| match event {
                    DriveEvent::Observation {
                        key_text, replayed, ..
                    } if key_text.starts_with("fetch:") => Some((key_text.clone(), *replayed)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(
                observations,
                vec![
                    (format!("fetch:{URL}:observed"), false),
                    (format!("fetch:{URL}:observed"), true),
                ],
                "{lane:?}"
            );
        }
    }

    #[test]
    fn machine_fetch_declared_checksum_replays_pin() {
        const URL: &str = "https://example.org/lua.tar.gz";
        let sha256 = crate::fetch::sha256_hex(b"example fixture archive");
        let src = format!(
            r#"
use vix::Tree;

pub fn src_tree(nonce: Int) -> Tree {{
    fetch(url: "{URL}", sha256: "{sha256}")
}}
"#
        );
        for lane in lanes() {
            let backend = FakeFetchBackend::new().with_archive(
                URL,
                b"example fixture archive",
                Tree::of(&[("src/lib.rs", "pub fn f() {}")]),
            );
            let mut machine = Machine::load_with_lane(&src, lane)
                .unwrap()
                .with_fetch_backend(backend);
            let first = machine.demand_i64("src_tree", vec![1]).unwrap();
            let second = machine.demand_i64("src_tree", vec![2]).unwrap();
            assert_eq!(first, second, "{lane:?}");
            let observations = machine
                .trace()
                .iter()
                .filter_map(|event| match event {
                    DriveEvent::Observation {
                        key_text, replayed, ..
                    } if key_text.starts_with("fetch:") => Some((key_text.clone(), *replayed)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let key = format!("fetch:{URL}:sha256:{sha256}");
            assert_eq!(
                observations,
                vec![(key.clone(), false), (key, true)],
                "{lane:?}"
            );
        }
    }

    #[test]
    fn crate_archive_fetches_checksum_pinned_registry_crate_into_source_tree() {
        const URL: &str = "https://static.crates.io/crates/itoa/itoa-1.0.15.crate";
        const ARCHIVE: &[u8] = include_bytes!("../../tests/fixtures/crate/itoa-1.0.15.crate");
        let sha256 = sha256_hex(ARCHIVE);
        assert_eq!(
            sha256,
            "4a5f13b858c8d314ee3e8f639011f7ccefe71f97f96e50151fb991f267928e2c"
        );
        let src = format!(
            r#"
use vix::Tree;

pub fn itoa_source(nonce: Int) -> Tree {{
    let archive = fetch(url: "{URL}", sha256: "{sha256}");
    crate_archive(archive)
}}
"#
        );
        let expected_paths = BTreeSet::from([
            ".cargo_vcs_info.json".to_string(),
            ".github/FUNDING.yml".to_string(),
            ".github/workflows/ci.yml".to_string(),
            ".gitignore".to_string(),
            "Cargo.lock".to_string(),
            "Cargo.toml".to_string(),
            "Cargo.toml.orig".to_string(),
            "LICENSE-APACHE".to_string(),
            "LICENSE-MIT".to_string(),
            "README.md".to_string(),
            "benches/bench.rs".to_string(),
            "src/lib.rs".to_string(),
            "src/udiv128.rs".to_string(),
            "tests/test.rs".to_string(),
        ]);

        for lane in lanes() {
            let backend = FakeFetchBackend::new().with_archive(
                URL,
                ARCHIVE,
                Tree::of_blobs(&[("itoa-1.0.15.crate", ARCHIVE)]),
            );
            let mut machine = Machine::load_with_lane(&src, lane)
                .unwrap()
                .with_fetch_backend(backend);
            let first = machine.demand_i64("itoa_source", vec![1]).unwrap();
            let entries = machine.tree_entries(first).unwrap();
            assert_eq!(
                entries.keys().cloned().collect::<BTreeSet<_>>(),
                expected_paths.clone(),
                "{lane:?}"
            );
            assert!(
                entries["Cargo.toml"].contains("name = \"itoa\""),
                "{lane:?}"
            );
            assert!(
                entries["src/lib.rs"].contains("pub struct Buffer"),
                "{lane:?}"
            );

            let second = machine.demand_i64("itoa_source", vec![2]).unwrap();
            assert_eq!(first, second, "{lane:?}");
            let fetch_observations = machine
                .trace()
                .iter()
                .filter_map(|event| match event {
                    DriveEvent::Observation {
                        key_text, replayed, ..
                    } if key_text.starts_with("fetch:") => Some((key_text.clone(), *replayed)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            let key = format!("fetch:{URL}:sha256:{sha256}");
            assert_eq!(
                fetch_observations,
                vec![(key.clone(), false), (key, true)],
                "{lane:?}"
            );
            assert_eq!(
                artifact_probes_for(&machine, "crate_archive"),
                vec![("tree".to_string(), false), ("tree".to_string(), true)],
                "{lane:?}"
            );
        }
    }

    #[test]
    fn module_set_imported_pub_fn_resolves_and_runs() {
        let set = modules(&[
            (
                "root",
                r#"
use a::answer;

pub fn main() -> Int {
    answer()
}
"#,
            ),
            (
                "a",
                r#"
pub fn answer() -> Int {
    42
}
"#,
            ),
        ]);
        for lane in lanes() {
            let mut machine = load_modules_with_lane("root", set.clone(), lane);
            assert_eq!(machine.demand_i64("main", vec![]).unwrap(), 42, "{lane:?}");
        }
    }

    #[test]
    fn module_set_importing_private_item_errors_loudly() {
        let set = modules(&[
            (
                "root",
                r#"
use a::answer;

pub fn main() -> Int {
    answer()
}
"#,
            ),
            (
                "a",
                r#"
fn answer() -> Int {
    42
}
"#,
            ),
        ]);
        let err = match Machine::load_modules("root", set) {
            Ok(_) => panic!("private import loaded"),
            Err(err) => err,
        };
        assert!(
            err.contains("cannot import private item `a::answer`"),
            "{err}"
        );
    }

    #[test]
    fn cross_module_hashes_match_same_content_in_one_file() {
        let single = r#"
pub fn answer() -> Int {
    40 + 2
}

pub fn main() -> Int {
    answer()
}
"#;
        let split = modules(&[
            (
                "root",
                r#"
use a::answer;

pub fn main() -> Int {
    answer()
}
"#,
            ),
            (
                "a",
                r#"
pub fn answer() -> Int {
    40 + 2
}
"#,
            ),
        ]);
        for lane in lanes() {
            let single = load_with_lane(single, lane);
            let split = load_modules_with_lane("root", split.clone(), lane);
            assert_eq!(single.fn_hash("main"), split.fn_hash("main"), "{lane:?}");
            assert_eq!(
                single.fn_hash("answer"),
                split.fn_hash("a::answer"),
                "{lane:?}"
            );
        }
    }

    #[test]
    fn machine_fn_memo_and_exec_tiers_compose() {
        let src = r#"
use vix::{Tree, Path, Target};
use caps::Cc;

fn get_cc(target: Target) -> Cc {
    Cc::acquire(target)
}

fn object(cc: Cc, src: Tree, unit: Path) -> Tree {
    cc! { -O2 -I {src} -c {src / unit} -o {unit.with_ext("o")} }
}
"#;
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            let target = machine.linux_target_handle();
            let cc = machine.demand_i64("get_cc", vec![target]).unwrap();
            let unit = machine
                .driver
                .intern_raw_value("Path", b"lapi.c".to_vec())
                .0;
            let tree_v1 = machine.intern_tree_concrete(Tree::of(&[
                ("lapi.c", "#include \"lua.h\"\n// api impl"),
                ("lua.h", "// the api"),
                ("README", "docs, never read by cc"),
            ]));
            let first = machine
                .demand_i64("object", vec![cc, tree_v1, unit])
                .unwrap();
            let first_entries = machine.tree_entries(first).unwrap();

            let tree_v2 = machine.intern_tree_concrete(Tree::of(&[
                ("lapi.c", "#include \"lua.h\"\n// api impl"),
                ("lua.h", "// the api"),
                ("README", "docs, EDITED, still never read"),
            ]));
            machine.clear_trace();
            let second = machine
                .demand_i64("object", vec![cc, tree_v2, unit])
                .unwrap();
            assert_eq!(
                machine.tree_entries(second).unwrap(),
                first_entries,
                "{lane:?}"
            );
            assert!(
                machine.trace().iter().any(|event| matches!(
                    event,
                    DriveEvent::RunCompleted {
                        command_name,
                        serving: ExecEvent::Tier2Cutoff { verified: 3 },
                        ..
                    } if command_name == "cc"
                )),
                "{lane:?}: {:?}",
                machine.trace()
            );

            let tree_v3 = machine.intern_tree_concrete(Tree::of(&[
                ("lapi.c", "#include \"lua.h\"\n// api impl"),
                ("lua.h", "// the api CHANGED"),
                ("README", "docs, EDITED, still never read"),
            ]));
            machine.clear_trace();
            let third = machine
                .demand_i64("object", vec![cc, tree_v3, unit])
                .unwrap();
            assert_ne!(
                machine.tree_entries(third).unwrap(),
                first_entries,
                "{lane:?}"
            );
            assert!(
                machine.trace().iter().any(|event| matches!(
                    event,
                    DriveEvent::RunCompleted {
                        serving: ExecEvent::Ran,
                        ..
                    }
                )),
                "{lane:?}: {:?}",
                machine.trace()
            );
        }
    }

    #[test]
    fn machine_commuting_flags_share_exec_identity() {
        let src = r#"
use vix::{Tree, Path, Target};
use caps::Cc;

fn get_cc(target: Target) -> Cc {
    Cc::acquire(target)
}

fn a(cc: Cc, src: Tree, unit: Path) -> Tree {
    cc! { -O2 -Wall -I {src} -c {src / unit} -o {unit.with_ext("o")} }
}

fn b(cc: Cc, src: Tree, unit: Path) -> Tree {
    cc! { -Wall -O2 -I {src} -c {src / unit} -o {unit.with_ext("o")} }
}
"#;
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            let target = machine.linux_target_handle();
            let cc = machine.demand_i64("get_cc", vec![target]).unwrap();
            let unit = machine
                .driver
                .intern_raw_value("Path", b"lapi.c".to_vec())
                .0;
            let tree = machine.intern_tree_concrete(Tree::of(&[
                ("lapi.c", "#include \"lua.h\"\n// api impl"),
                ("lua.h", "// the api"),
            ]));
            let first = machine.demand_i64("a", vec![cc, tree, unit]).unwrap();
            let first_entries = machine.tree_entries(first).unwrap();
            machine.clear_trace();
            let second = machine.demand_i64("b", vec![cc, tree, unit]).unwrap();
            assert_eq!(
                machine.tree_entries(second).unwrap(),
                first_entries,
                "{lane:?}"
            );
            let requested = machine
                .trace()
                .iter()
                .filter_map(|event| match event {
                    DriveEvent::RunRequested { argv, .. } => Some(argv.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(requested.len(), 1, "{lane:?}: {requested:?}");
            assert_eq!(
                requested[0],
                vec![
                    "-Wall".to_string(),
                    "-O2".to_string(),
                    "-I".to_string(),
                    "/m/0".to_string(),
                    "-c".to_string(),
                    "/m/1/lapi.c".to_string(),
                    "-o".to_string(),
                    "lapi.o".to_string(),
                ],
                "{lane:?}"
            );
            assert!(
                machine.trace().iter().any(|event| matches!(
                    event,
                    DriveEvent::RunCompleted {
                        serving: ExecEvent::Tier1Hit,
                        ..
                    }
                )),
                "{lane:?}: {:?}",
                machine.trace()
            );
        }
    }

    #[test]
    fn recursive_scc_hashes_survive_definition_order_on_machine() {
        let ab = r#"
fn a() -> Int { b() }
fn b() -> Int { a() }
"#;
        let ba = r#"
fn b() -> Int { a() }
fn a() -> Int { b() }
"#;
        for lane in lanes() {
            let ab = load_with_lane(ab, lane);
            let ba = load_with_lane(ba, lane);
            assert_eq!(ab.fn_hash("a"), ba.fn_hash("a"), "{lane:?}");
            assert_eq!(ab.fn_hash("b"), ba.fn_hash("b"), "{lane:?}");
        }
    }

    #[test]
    fn lazy_map_value_forces_only_selected_pending_entry() {
        let src = r#"
fn key(n: Int) -> String {
    match n {
        0 => "left",
        _ => "right",
    }
}

fn left() -> Float { 1.0 }
fn right() -> Float { 2.0 }

pub fn pick(n: Int) -> Float {
    let m: Map<String, Float> = {};
    m.insert("left", left()).insert("right", right()).get(key(n)).unwrap()
}
"#;
        let mut cold_traces = Vec::new();
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            assert_eq!(
                host_call_count(&machine, "pick", PENDING_COERCE_HOST),
                1,
                "{lane:?}"
            );
            assert_eq!(
                host_call_count(&machine, "pick", STORE_TAG_HOST),
                0,
                "frame realization flags replace the old Ready/Pending STORE_TAG barrier on {lane:?}"
            );
            assert_eq!(
                (machine.demand_i64("pick", vec![0]).unwrap() as u64),
                1.0f64.to_bits(),
                "{lane:?}"
            );
            assert_eq!(spawned_count(&machine, "left"), 1, "{lane:?}");
            assert_eq!(spawned_count(&machine, "right"), 0, "{lane:?}");
            cold_traces.push((lane, machine.trace().to_vec()));

            let pick_hash = machine.fn_hash("pick").expect("pick hash");
            machine.clear_trace();
            assert_eq!(
                (machine.demand_i64("pick", vec![0]).unwrap() as u64),
                1.0f64.to_bits(),
                "{lane:?}"
            );
            assert_eq!(
                machine.trace(),
                &[
                    DriveEvent::Demanded { fn_hash: pick_hash },
                    DriveEvent::MemoHit { fn_hash: pick_hash },
                ],
                "warm re-demand is just the root memo hit on {lane:?}"
            );
            assert_eq!(spawned_count(&machine, "right"), 0, "{lane:?}");
        }
        assert_lane_traces_equal(&cold_traces);
    }

    #[test]
    fn mixed_ready_and_pending_map_entries_resolve_at_read_barrier() {
        let src = r#"
fn key(n: Int) -> String {
    match n {
        0 => "ready",
        _ => "lazy",
    }
}

fn lazy() -> Float { 2.0 }

pub fn pick(n: Int) -> Float {
    let m: Map<String, Float> = {};
    m.insert("ready", 1.0).insert("lazy", lazy()).get(key(n)).unwrap()
}
"#;
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            assert_eq!(
                host_call_count(&machine, "pick", PENDING_COERCE_HOST),
                1,
                "{lane:?}"
            );
            assert_eq!(
                host_call_count(&machine, "pick", STORE_TAG_HOST),
                0,
                "Ready map entry path no longer pays a STORE_TAG host barrier on {lane:?}"
            );
            assert_eq!(
                (machine.demand_i64("pick", vec![0]).unwrap() as u64),
                1.0f64.to_bits(),
                "{lane:?}"
            );
            assert_eq!(spawned_count(&machine, "lazy"), 0, "{lane:?}");

            machine.clear_trace();
            assert_eq!(
                (machine.demand_i64("pick", vec![1]).unwrap() as u64),
                2.0f64.to_bits(),
                "{lane:?}"
            );
            assert_eq!(spawned_count(&machine, "lazy"), 1, "{lane:?}");
        }
    }

    #[test]
    fn lazy_map_pending_entries_hash_independent_of_insert_order() {
        let src = r#"
fn left() -> Float { 1.0 }
fn right() -> Float { 2.0 }

pub fn lr() -> Map<String, Float> {
    let m: Map<String, Float> = {};
    m.insert("left", left()).insert("right", right())
}

pub fn rl() -> Map<String, Float> {
    let m: Map<String, Float> = {};
    m.insert("right", right()).insert("left", left())
}
"#;
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            let lr = machine.demand_i64("lr", vec![]).unwrap();
            let rl = machine.demand_i64("rl", vec![]).unwrap();
            let lr_entry = machine.driver.store_entry(lr).expect("lr entry");
            let rl_entry = machine.driver.store_entry(rl).expect("rl entry");
            assert_eq!(lr_entry.content_hash, rl_entry.content_hash, "{lane:?}");
            assert_eq!(lr, rl, "canonical map dedupes pending entries on {lane:?}");
            assert_eq!(spawned_count(&machine, "left"), 0, "{lane:?}");
            assert_eq!(spawned_count(&machine, "right"), 0, "{lane:?}");
        }
    }

    #[test]
    fn ready_and_pending_map_entries_hash_differ_under_bitset_encoding() {
        let src = r#"
fn producer() -> Float { 4.2 }

pub fn ready() -> Map<String, Float> {
    let m: Map<String, Float> = {};
    m.insert("x", 4.2)
}

pub fn pending() -> Map<String, Float> {
    let m: Map<String, Float> = {};
    m.insert("x", producer())
}
"#;
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            let ready = machine.demand_i64("ready", vec![]).unwrap();
            let pending = machine.demand_i64("pending", vec![]).unwrap();
            let ready_entry = machine.driver.store_entry(ready).expect("ready entry");
            let pending_entry = machine.driver.store_entry(pending).expect("pending entry");
            assert_ne!(
                ready_entry.content_hash, pending_entry.content_hash,
                "{lane:?}"
            );
            assert_ne!(
                ready, pending,
                "Ready(4.2) and Pending(producer-of-4.2) are intentionally distinct on {lane:?}"
            );
            assert_eq!(spawned_count(&machine, "producer"), 0, "{lane:?}");
        }
    }

    #[test]
    fn unwrapped_pending_identity_moves_without_demanding_bits() {
        let src = r#"
fn key(n: Int) -> String {
    match n {
        0 => "left",
        _ => "right",
    }
}

fn left() -> Float { 1.0 }
fn right() -> Float { 2.0 }

pub fn moved(n: Int) -> Map<String, Float> {
    let source: Map<String, Float> = {};
    let source = source.insert("left", left()).insert("right", right());
    let value = source.get(key(n)).unwrap();
    let out: Map<String, Float> = {};
    out.insert("selected", value)
}
"#;
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            assert_eq!(
                host_call_count(&machine, "moved", PENDING_COERCE_HOST),
                0,
                "{lane:?}"
            );
            let _handle = machine.demand_i64("moved", vec![0]).unwrap();
            assert_eq!(spawned_count(&machine, "left"), 0, "{lane:?}");
            assert_eq!(spawned_count(&machine, "right"), 0, "{lane:?}");

            let moved_hash = machine.fn_hash("moved").expect("moved hash");
            machine.clear_trace();
            let _handle = machine.demand_i64("moved", vec![0]).unwrap();
            assert_eq!(
                machine.trace(),
                &[
                    DriveEvent::Demanded {
                        fn_hash: moved_hash
                    },
                    DriveEvent::MemoHit {
                        fn_hash: moved_hash
                    },
                ],
                "warm re-demand is just the root memo hit on {lane:?}"
            );
            assert_eq!(spawned_count(&machine, "left"), 0, "{lane:?}");
            assert_eq!(spawned_count(&machine, "right"), 0, "{lane:?}");
        }
    }

    #[test]
    fn cargo_toml_projection_runs_on_the_machine() {
        let src = include_str!("../../../playgrounds/snark/src/bundled/vix/samples/cargo.vix");
        let manifest =
            include_str!("../../../playgrounds/snark/src/bundled/vix/samples/fixtures/Cargo.toml");
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            let tree = machine
                .driver
                .intern_tree_concrete(Tree::of(&[("Cargo.toml", manifest)]));
            let tuple = machine.demand_i64("cargo_manifest", vec![tree]).unwrap();
            let name = machine.driver.store_field(tuple, 0).unwrap();
            let version = machine.driver.store_field(tuple, 1).unwrap();
            let facet_version = machine.driver.store_field(tuple, 2).unwrap();
            assert_eq!(
                machine.driver.raw_string(name, "String").unwrap(),
                "mini-real-crate",
                "{lane:?}"
            );
            assert_eq!(
                machine.driver.raw_string(version, "String").unwrap(),
                "0.3.1",
                "{lane:?}"
            );
            assert_eq!(
                machine.driver.raw_string(facet_version, "String").unwrap(),
                "0.50.0-rc.5",
                "{lane:?}"
            );
        }
    }

    #[test]
    fn crate_vix_fake_rustc_builds_lib_on_the_machine() {
        let src = crate_sample_source();
        for lane in lanes() {
            let mut machine = load_with_lane(&src, lane);
            let target = machine.linux_target_handle();
            let crate_tree =
                machine.intern_tree_concrete(mini_vendored_tree("not read by slice-1 rustc\n"));

            let built = machine
                .demand_i64("crate_lib", vec![target, crate_tree])
                .unwrap();
            let entries = machine.tree_entries(built).unwrap();
            assert_eq!(
                entries.keys().collect::<Vec<_>>(),
                vec![
                    &"libmini_vendored.rlib".to_string(),
                    &"libmini_vendored.rmeta".to_string()
                ],
                "{lane:?}: {entries:?}"
            );
            assert!(
                entries["libmini_vendored.rlib"].starts_with("rlib("),
                "{lane:?}: {entries:?}"
            );
            assert!(
                entries["libmini_vendored.rmeta"].starts_with("rmeta("),
                "{lane:?}: {entries:?}"
            );

            let requested = machine
                .trace()
                .iter()
                .filter_map(|event| match event {
                    DriveEvent::RunRequested {
                        command_name, argv, ..
                    } => Some((command_name.as_str(), argv.as_slice())),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(
                requested,
                vec![(
                    "rustc",
                    &[
                        "--crate-name".to_string(),
                        "mini_vendored".to_string(),
                        "--edition".to_string(),
                        "2021".to_string(),
                        "--crate-type".to_string(),
                        "lib".to_string(),
                        "--emit=metadata=libmini_vendored.rmeta,link=libmini_vendored.rlib"
                            .to_string(),
                        "-L".to_string(),
                        "dependency=/m/0".to_string(),
                        "/m/1/lib.rs".to_string(),
                    ][..],
                )],
                "{lane:?}: {requested:?}"
            );
            let completed = machine
                .trace()
                .iter()
                .filter_map(|event| match event {
                    DriveEvent::RunCompleted {
                        command_name,
                        serving,
                        outputs,
                        ..
                    } => Some((command_name.as_str(), serving, outputs)),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(completed.len(), 1, "{lane:?}: {completed:?}");
            assert!(matches!(
                completed.as_slice(),
                [("rustc", ExecEvent::Ran, outputs)]
                    if outputs.iter().any(|(path, _)| path == "libmini_vendored.rlib")
                        && outputs.iter().any(|(path, _)| path == "libmini_vendored.rmeta")
            ));

            let crate_hash = machine.fn_hash("crate_lib").expect("crate_lib hash");
            machine.clear_trace();
            let warm = machine
                .demand_i64("crate_lib", vec![target, crate_tree])
                .unwrap();
            assert_eq!(warm, built, "{lane:?}");
            assert_eq!(
                machine.trace(),
                &[
                    DriveEvent::Demanded {
                        fn_hash: crate_hash
                    },
                    DriveEvent::MemoHit {
                        fn_hash: crate_hash
                    },
                ],
                "warm re-demand is exactly root memo hit and zero runs on {lane:?}"
            );
        }
    }

    #[test]
    fn crate_vix_fake_rustc_unread_crate_file_cuts_off_at_tier2() {
        let src = crate_sample_source();
        for lane in lanes() {
            let mut machine = load_with_lane(&src, lane);
            let target = machine.linux_target_handle();
            let crate_v1 = machine.intern_tree_concrete(mini_vendored_tree("not read by rustc\n"));
            let first = machine
                .demand_i64("crate_lib", vec![target, crate_v1])
                .unwrap();
            let first_entries = machine.tree_entries(first).unwrap();

            let crate_v2 = machine
                .intern_tree_concrete(mini_vendored_tree("edited, still not read by rustc\n"));
            machine.clear_trace();
            let second = machine
                .demand_i64("crate_lib", vec![target, crate_v2])
                .unwrap();
            assert_eq!(
                machine.tree_entries(second).unwrap(),
                first_entries,
                "{lane:?}"
            );
            assert!(
                machine.trace().iter().any(|event| matches!(
                    event,
                    DriveEvent::RunCompleted {
                        command_name,
                        serving: ExecEvent::Tier2Cutoff { verified: 1 },
                        ..
                    } if command_name == "rustc"
                )),
                "{lane:?}: {:?}",
                machine.trace()
            );
        }
    }

    #[test]
    fn crate_vix_fake_rustc_builds_bin_with_dependency_metadata_and_rlib() {
        let src = crate_sample_source();
        for lane in lanes() {
            let mut machine = load_with_lane(&src, lane);
            let target = machine.linux_target_handle();
            let graph = machine.intern_tree_concrete(two_crate_graph_tree());

            let checked = machine
                .demand_i64("crate_bin_check", vec![target, graph])
                .unwrap();
            let check_entries = machine.tree_entries(checked).unwrap();
            assert_eq!(
                check_entries.keys().collect::<Vec<_>>(),
                vec![&"mini_app.rmeta".to_string()],
                "{lane:?}: {check_entries:?}"
            );
            assert!(
                check_entries["mini_app.rmeta"].starts_with("rmeta("),
                "{lane:?}: {check_entries:?}"
            );

            let built = machine
                .demand_i64("crate_bin", vec![target, graph])
                .unwrap();
            let entries = machine.tree_entries(built).unwrap();
            assert_eq!(
                entries.keys().collect::<Vec<_>>(),
                vec![&"mini_app".to_string()],
                "{lane:?}: {entries:?}"
            );
            assert!(
                entries["mini_app"].starts_with("bin("),
                "{lane:?}: {entries:?}"
            );

            let requested = machine
                .trace()
                .iter()
                .filter_map(|event| match event {
                    DriveEvent::RunRequested {
                        command_name, argv, ..
                    } if command_name == "rustc" => Some(argv.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(requested.len(), 4, "{lane:?}: {requested:#?}");
            assert!(
                requested[0]
                    .iter()
                    .any(|arg| arg == "--emit=metadata=libhelper.rmeta,link=libhelper.rlib"),
                "{lane:?}: {requested:#?}"
            );
            assert!(
                requested[1]
                    .iter()
                    .any(|arg| arg == "--emit=metadata=mini_app.rmeta"),
                "{lane:?}: {requested:#?}"
            );
            assert!(
                requested[1]
                    .iter()
                    .any(|arg| arg.starts_with("helper=/m/") && arg.ends_with("/libhelper.rmeta")),
                "{lane:?}: {requested:#?}"
            );
            assert!(
                requested[1]
                    .iter()
                    .any(|arg| arg.starts_with("dependency=/m/")),
                "{lane:?}: {requested:#?}"
            );
            assert!(
                requested[2]
                    .iter()
                    .any(|arg| arg == "--emit=metadata=libhelper.rmeta,link=libhelper.rlib"),
                "{lane:?}: {requested:#?}"
            );
            assert!(
                requested[3].iter().any(|arg| arg == "--emit=link=mini_app"),
                "{lane:?}: {requested:#?}"
            );
            assert!(
                requested[3]
                    .iter()
                    .any(|arg| arg.starts_with("helper=/m/") && arg.ends_with("/libhelper.rlib")),
                "{lane:?}: {requested:#?}"
            );

            let started = machine
                .trace()
                .iter()
                .filter(|event| {
                    matches!(
                        event,
                        DriveEvent::RunStarted {
                            command_name,
                            ..
                        } if command_name == "rustc"
                    )
                })
                .count();
            assert_eq!(started, 4, "{lane:?}: {:?}", machine.trace());
        }
    }

    #[test]
    fn crate_vix_fake_rustc_builds_proc_macro_as_host_unit() {
        let src = crate_sample_source();
        for lane in lanes() {
            let mut machine = load_with_lane(&src, lane);
            let target = cross_target_handle(&machine);
            let graph = machine.intern_tree_concrete(proc_macro_graph_tree());

            let built = machine
                .demand_i64("crate_proc_macro_bin", vec![target, graph])
                .unwrap();
            let entries = machine.tree_entries(built).unwrap();
            assert_eq!(
                entries.keys().collect::<Vec<_>>(),
                vec![&"macro_app".to_string()],
                "{lane:?}: {entries:?}"
            );
            assert!(
                entries["macro_app"].starts_with("bin("),
                "{lane:?}: {entries:?}"
            );

            let requested = machine
                .trace()
                .iter()
                .filter_map(|event| match event {
                    DriveEvent::RunRequested {
                        command_name,
                        capability_key,
                        argv,
                        ..
                    } if command_name == "rustc" => Some((capability_key.clone(), argv.clone())),
                    _ => None,
                })
                .collect::<Vec<_>>();
            assert_eq!(requested.len(), 2, "{lane:?}: {requested:#?}");

            let macro_unit = requested
                .iter()
                .find(|(_, argv)| has_arg_pair(argv, "--crate-name", "emit_answer_macro"))
                .unwrap_or_else(|| panic!("{lane:?}: missing proc-macro unit in {requested:#?}"));
            let consumer_unit = requested
                .iter()
                .find(|(_, argv)| has_arg_pair(argv, "--crate-name", "macro_app"))
                .unwrap_or_else(|| panic!("{lane:?}: missing consumer unit in {requested:#?}"));
            assert_ne!(
                macro_unit.0, consumer_unit.0,
                "{lane:?}: proc-macro producer must acquire host rustc while consumer keeps target rustc"
            );
            assert!(
                has_arg_pair(&macro_unit.1, "--crate-type", "proc-macro"),
                "{lane:?}: {requested:#?}"
            );
            let dylib = host_proc_macro_dylib_name();
            assert!(
                macro_unit
                    .1
                    .iter()
                    .any(|arg| arg == &format!("--emit=link={dylib}")),
                "{lane:?}: {requested:#?}"
            );
            assert!(
                consumer_unit
                    .1
                    .iter()
                    .any(|arg| arg.starts_with("emit_answer_macro=/m/") && arg.ends_with(&dylib)),
                "{lane:?}: {requested:#?}"
            );
        }
    }

    #[test]
    fn inline_json_structural_values_run_on_the_machine() {
        let src = r#"
pub fn parse(input: String) -> (String, Int, Bool) {
    let doc = json(input);
    let package = doc.get("package").unwrap();
    (
        package.get("name").unwrap(),
        package.get("version").unwrap(),
        doc.get("publish").unwrap(),
    )
}
"#;
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            let (input, _) = machine.driver.intern_raw_value(
                "String",
                br#"{"package":{"name":"mini-real-crate","version":3},"publish":false}"#.to_vec(),
            );
            let tuple = machine.demand_i64("parse", vec![input]).unwrap();
            let name = machine.driver.store_field(tuple, 0).unwrap();
            let version = machine.driver.store_field(tuple, 1).unwrap();
            let publish = machine.driver.store_field(tuple, 2).unwrap();
            assert_eq!(
                machine.driver.raw_string(name, "String").unwrap(),
                "mini-real-crate",
                "{lane:?}"
            );
            assert_eq!(version, 3, "{lane:?}");
            assert_eq!(publish, 0, "{lane:?}");
        }
    }

    #[test]
    fn doc_array_coercion_allocates_the_declared_element_schema() {
        let src = r#"
pub fn strings(input: String) -> [String] {
    json(input)
}
"#;
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            let (input, _) = machine
                .driver
                .intern_raw_value("String", br#"["a","b"]"#.to_vec());
            let strings = machine.demand_i64("strings", vec![input]).unwrap();
            let entry = machine.driver.store_entry(strings).expect("string array");
            assert_eq!(entry.schema, "Array<String>", "{lane:?}");
            let (elem_schema, words) = machine.driver.array_words(strings).unwrap();
            assert_eq!(elem_schema, "String", "{lane:?}");
            let rendered = words
                .into_iter()
                .map(|word| machine.driver.raw_string(word, "String").unwrap())
                .collect::<Vec<_>>();
            assert_eq!(rendered, ["a", "b"], "{lane:?}");
        }
    }

    #[test]
    fn elf_structural_projection_contracts_are_pinned() {
        let src = r#"
pub fn arch(input: Blob) -> String { elf(input).arch }
pub fn kind(input: Blob) -> String { elf(input).kind }
pub fn deps(input: Blob) -> [String] { elf(input).dynamic_deps }
pub fn glibc(input: Blob) -> String { elf(input).needs_glibc }
pub fn symbols(input: Blob) -> [String] { elf(input).symbols }
pub fn sections(input: Blob) -> [Doc] { elf(input).sections }
pub fn linker_metadata(input: Blob) -> [String] { elf(input).linker_metadata }
"#;
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            let hello = machine
                .driver
                .intern_raw_value(
                    "Blob",
                    include_bytes!("../../tests/fixtures/elf/hello-x86_64").to_vec(),
                )
                .0;
            let libtiny = machine
                .driver
                .intern_raw_value(
                    "Blob",
                    include_bytes!("../../tests/fixtures/elf/libtiny-x86_64.so").to_vec(),
                )
                .0;
            assert!(matches!(
                super::super::elf::project(
                    include_bytes!("../../tests/fixtures/elf/libtiny-x86_64.so"),
                    super::super::elf::Projection::NeedsGlibc,
                )
                .unwrap(),
                crate::value::Value::Str(value) if value == "GLIBC_2.2.5"
            ));

            let arch = machine.demand_i64("arch", vec![hello]).unwrap();
            assert_eq!(machine.driver.raw_string(arch, "String").unwrap(), "x86_64");

            let kind = machine.demand_i64("kind", vec![hello]).unwrap();
            assert_eq!(machine.driver.raw_string(kind, "String").unwrap(), "dyn");

            let deps = machine.demand_i64("deps", vec![hello]).unwrap();
            assert_eq!(
                rendered_doc_strings(&machine, "deps", deps),
                vec!["libc.so.6"]
            );

            let glibc = machine.demand_i64("glibc", vec![hello]).unwrap();
            assert_eq!(
                machine.driver.raw_string(glibc, "String").unwrap(),
                "GLIBC_2.34"
            );

            let lib_glibc = machine.demand_i64("glibc", vec![libtiny]).unwrap();
            assert_eq!(
                machine.driver.raw_string(lib_glibc, "String").unwrap(),
                "GLIBC_2.2.5"
            );

            let symbols = machine.demand_i64("symbols", vec![libtiny]).unwrap();
            let symbols = rendered_doc_strings(&machine, "symbols", symbols);
            assert!(
                symbols.windows(2).all(|pair| pair[0] <= pair[1]),
                "{lane:?}: {symbols:?}"
            );
            assert!(
                symbols.contains(&"strlen".to_string())
                    && symbols.contains(&"vix_fixture_len".to_string()),
                "{lane:?}: {symbols:?}"
            );

            let metadata = machine
                .demand_i64("linker_metadata", vec![libtiny])
                .unwrap();
            assert_eq!(
                rendered_doc_strings(&machine, "linker_metadata", metadata),
                vec!["GCC: (Ubuntu 15.2.0-16ubuntu1) 15.2.0"]
            );

            let sections = machine.demand_i64("sections", vec![hello]).unwrap();
            let sections = rendered_doc_maps(&machine, "sections", sections);
            assert!(
                sections.iter().any(|section| {
                    section.get("name") == Some(&".dynamic".to_string())
                        && section.get("size") == Some(&"496".to_string())
                }),
                "{lane:?}: {sections:?}"
            );
            assert!(
                sections.iter().any(|section| {
                    section.get("name") == Some(&".gnu.version_r".to_string())
                        && section.get("size") == Some(&"48".to_string())
                }),
                "{lane:?}: {sections:?}"
            );
        }
    }

    #[test]
    fn elf_arch_projection_does_not_parse_symbols() {
        let src = r#"
pub fn arch(input: Blob) -> String { elf(input).arch }
"#;
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            let hello = machine
                .driver
                .intern_raw_value(
                    "Blob",
                    include_bytes!("../../tests/fixtures/elf/hello-x86_64").to_vec(),
                )
                .0;
            let arch = machine.demand_i64("arch", vec![hello]).unwrap();
            assert_eq!(machine.driver.raw_string(arch, "String").unwrap(), "x86_64");
            let probes = artifact_probes(&machine);
            assert_eq!(probes, vec![("arch".to_string(), false)], "{lane:?}");
        }
    }

    #[test]
    fn elf_projection_results_are_memoized_by_content() {
        let src = r#"
pub fn arch_twice(input: Blob) -> (String, String) {
    (elf(input).arch, elf(input).arch)
}
"#;
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            let hello = machine
                .driver
                .intern_raw_value(
                    "Blob",
                    include_bytes!("../../tests/fixtures/elf/hello-x86_64").to_vec(),
                )
                .0;
            let tuple = machine.demand_i64("arch_twice", vec![hello]).unwrap();
            let left = machine.driver.store_field(tuple, 0).unwrap();
            let right = machine.driver.store_field(tuple, 1).unwrap();
            assert_eq!(left, right, "string result is store-deduped on {lane:?}");
            assert_eq!(
                machine.driver.raw_string(left, "String").unwrap(),
                "x86_64",
                "{lane:?}"
            );
            assert_eq!(
                artifact_probes(&machine),
                vec![("arch".to_string(), false), ("arch".to_string(), true)],
                "{lane:?}"
            );
        }
    }

    #[test]
    fn ast_structural_projection_contracts_are_pinned() {
        let src = r#"
pub fn item_count(source: String) -> Int { ast(source).items.len() }
pub fn fn_count(source: String) -> Int { ast(source).fns.len() }
pub fn toolchain_start(source: String) -> Int { ast(source).fn("toolchain").span.start }
pub fn toolchain_param_count(source: String) -> Int { ast(source).fn("toolchain").params.len() }
pub fn toolchain_body_children(source: String) -> Int {
    ast(source).fn("toolchain").body.children.len()
}
"#;
        let types = include_str!("../../../playgrounds/snark/src/bundled/vix/samples/types.vix");
        let parsed = crate::VixParser::new().parse(types).unwrap();
        let expected_items = i64::try_from(parsed.items.len()).unwrap();
        let expected_fns = i64::try_from(
            parsed
                .items
                .iter()
                .filter(|item| matches!(item, ast::Item::Fn(_)))
                .count(),
        )
        .unwrap();
        let toolchain = parsed
            .items
            .iter()
            .find_map(|item| match item {
                ast::Item::Fn(item) if item.name.value == "toolchain" => Some(item),
                _ => None,
            })
            .unwrap();
        let expected_body_children =
            i64::try_from(toolchain.body.stmts.len() + usize::from(toolchain.body.tail.is_some()))
                .unwrap();

        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            let source = machine
                .driver
                .intern_raw_value("String", types.as_bytes().to_vec())
                .0;

            assert_eq!(
                machine.demand_i64("item_count", vec![source]).unwrap(),
                expected_items,
                "{lane:?}"
            );
            assert_eq!(
                machine.demand_i64("fn_count", vec![source]).unwrap(),
                expected_fns,
                "{lane:?}"
            );
            assert_eq!(
                machine.demand_i64("toolchain_start", vec![source]).unwrap(),
                i64::from(toolchain.span.start),
                "{lane:?}"
            );
            assert_eq!(
                machine
                    .demand_i64("toolchain_param_count", vec![source])
                    .unwrap(),
                i64::try_from(toolchain.params.params.len()).unwrap(),
                "{lane:?}"
            );
            assert_eq!(
                machine
                    .demand_i64("toolchain_body_children", vec![source])
                    .unwrap(),
                expected_body_children,
                "{lane:?}"
            );
        }
    }

    #[test]
    fn ast_items_projection_does_not_force_fn_or_body_children() {
        let src = r#"
pub fn item_count(source: String) -> Int { ast(source).items.len() }
"#;
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            let source = machine
                .driver
                .intern_raw_value(
                    "String",
                    include_str!("../../../playgrounds/snark/src/bundled/vix/samples/types.vix")
                        .as_bytes()
                        .to_vec(),
                )
                .0;
            assert!(machine.demand_i64("item_count", vec![source]).unwrap() > 0);
            assert_eq!(
                ast_artifact_probes(&machine),
                vec![("items".to_string(), false)],
                "{lane:?}"
            );
        }
    }

    #[test]
    fn ast_fn_body_children_projection_is_lazy_and_memoized() {
        let src = r#"
pub fn body_twice(source: String) -> (Int, Int) {
    (
        ast(source).fn("toolchain").body.children.len(),
        ast(source).fn("toolchain").body.children.len(),
    )
}
"#;
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            let source = machine
                .driver
                .intern_raw_value(
                    "String",
                    include_str!("../../../playgrounds/snark/src/bundled/vix/samples/types.vix")
                        .as_bytes()
                        .to_vec(),
                )
                .0;
            let tuple = machine.demand_i64("body_twice", vec![source]).unwrap();
            let left = machine.driver.store_field(tuple, 0).unwrap();
            let right = machine.driver.store_field(tuple, 1).unwrap();
            assert_eq!(left, right, "{lane:?}");
            assert_eq!(
                ast_artifact_probes(&machine),
                vec![
                    ("fn".to_string(), false),
                    ("fn.body.children".to_string(), false),
                    ("fn".to_string(), true),
                    ("fn.body.children".to_string(), true),
                ],
                "{lane:?}"
            );
        }
    }

    #[test]
    fn oci_structural_projection_contracts_are_pinned() {
        let src = r#"
pub fn layers(input: Tree) -> [Doc] { oci(input).layers }
pub fn env(input: Tree) -> [String] { oci(input).env }
pub fn entrypoint(input: Tree) -> [String] { oci(input).entrypoint }
pub fn cmd(input: Tree) -> [String] { oci(input).cmd }
pub fn shadow(input: Tree) -> String { oci(input).files.get("etc/message").unwrap().contents }
pub fn shadow_layer(input: Tree) -> String { oci(input).files.get("etc/message").unwrap().layer_digest }
"#;
        let (layout, overlay_digest) = tiny_oci_layout();
        let parsed = super::super::oci::parse_layout(layout.clone()).unwrap();
        assert_eq!(
            super::super::oci::project_file(&parsed, "etc/remove").unwrap(),
            None
        );
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            let input = machine.intern_tree_concrete(layout.clone());

            let layers = machine.demand_i64("layers", vec![input]).unwrap();
            let layers = rendered_doc_maps(&machine, "layers", layers);
            assert_eq!(layers.len(), 3, "{lane:?}");

            let env = machine.demand_i64("env", vec![input]).unwrap();
            assert_eq!(
                rendered_doc_strings(&machine, "env", env),
                vec!["A=base".to_string(), "B=top".to_string()],
                "{lane:?}"
            );
            let entrypoint = machine.demand_i64("entrypoint", vec![input]).unwrap();
            assert_eq!(
                rendered_doc_strings(&machine, "entrypoint", entrypoint),
                vec!["/bin/app".to_string()],
                "{lane:?}"
            );
            let cmd = machine.demand_i64("cmd", vec![input]).unwrap();
            assert_eq!(
                rendered_doc_strings(&machine, "cmd", cmd),
                vec!["--serve".to_string()],
                "{lane:?}"
            );

            let shadow = machine.demand_i64("shadow", vec![input]).unwrap();
            assert_eq!(
                machine.driver.raw_string(shadow, "String").unwrap(),
                "overlay\n",
                "{lane:?}"
            );
            let layer = machine.demand_i64("shadow_layer", vec![input]).unwrap();
            assert_eq!(
                machine.driver.raw_string(layer, "String").unwrap(),
                overlay_digest,
                "{lane:?}"
            );
        }
    }

    #[test]
    fn oci_config_projection_does_not_parse_layers() {
        let src = r#"
pub fn env(input: Tree) -> [String] { oci(input).config.config.Env }
"#;
        let (layout, _) = tiny_oci_layout();
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            let input = machine.intern_tree_concrete(layout.clone());
            let env = machine.demand_i64("env", vec![input]).unwrap();
            assert_eq!(
                rendered_doc_strings(&machine, "env", env),
                vec!["A=base".to_string(), "B=top".to_string()],
                "{lane:?}"
            );
            assert_eq!(
                artifact_probes_for(&machine, "oci"),
                vec![("config".to_string(), false)],
                "{lane:?}"
            );
        }
    }

    #[test]
    fn oci_accepts_layout_blob_archives() {
        let src = r#"
pub fn env(input: Blob) -> [String] { oci(input).env }
"#;
        let (layout, _) = tiny_oci_layout();
        let archive = tree_archive(&layout);
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            let input = machine
                .driver
                .intern_raw_value("Blob", archive.as_bytes().to_vec())
                .0;
            let env = machine.demand_i64("env", vec![input]).unwrap();
            assert_eq!(
                rendered_doc_strings(&machine, "env", env),
                vec!["A=base".to_string(), "B=top".to_string()],
                "{lane:?}"
            );
        }
    }

    #[test]
    fn oci_glibc_preflight_uses_version_ordering_without_execution() {
        let src = include_str!(
            "../../../playgrounds/snark/src/bundled/vix/samples/oci-glibc-preflight.vix"
        );
        let meets = oci_layout_with_libc(include_bytes!("../../tests/fixtures/elf/hello-x86_64"));
        let below =
            oci_layout_with_libc(include_bytes!("../../tests/fixtures/elf/libtiny-x86_64.so"));
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            let meets = machine.intern_tree_concrete(meets.clone());
            let below = machine.intern_tree_concrete(below.clone());
            assert_eq!(machine.demand_i64("matches_floor", vec![meets]).unwrap(), 1);
            assert_eq!(machine.demand_i64("matches_floor", vec![below]).unwrap(), 0);
            assert!(
                machine
                    .trace()
                    .iter()
                    .all(|event| !matches!(event, DriveEvent::RunRequested { .. })),
                "{lane:?}: preflight must not execute"
            );
        }
    }

    #[test]
    fn oci_file_projection_is_path_local_and_memoized() {
        let src = r#"
pub fn shadow_twice(input: Tree) -> (String, String) {
    let files = oci(input).files;
    (
        files.get("etc/message").unwrap().contents,
        files.get("etc/message").unwrap().contents,
    )
}
"#;
        let (layout, _) = tiny_oci_layout();
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            let input = machine.intern_tree_concrete(layout.clone());
            let tuple = machine.demand_i64("shadow_twice", vec![input]).unwrap();
            let left = machine.driver.store_field(tuple, 0).unwrap();
            let right = machine.driver.store_field(tuple, 1).unwrap();
            assert_eq!(
                machine.driver.raw_string(left, "String").unwrap(),
                "overlay\n",
                "{lane:?}"
            );
            assert_eq!(
                machine.driver.raw_string(right, "String").unwrap(),
                "overlay\n",
                "{lane:?}"
            );
            assert_eq!(
                artifact_probes_for(&machine, "oci"),
                vec![
                    ("files".to_string(), false),
                    ("files/etc/message".to_string(), false),
                    ("files/etc/message".to_string(), true),
                ],
                "{lane:?}"
            );
        }
    }

    #[test]
    fn scalar_array_collect_sorts_and_rejects_arguments() {
        let bad = r#"
pub fn bad() -> [Int] {
    [2, 1].collect(0)
}
"#;
        let good = r#"
pub fn good() -> [Int] {
    [2, 1].collect()
}
        "#;
        for lane in lanes() {
            let err = match Machine::load_with_lane(bad, lane) {
                Ok(_) => panic!("bad collect loaded on {lane:?}"),
                Err(err) => err,
            };
            assert_eq!(err, "lowering bad: collect takes no arguments", "{lane:?}");

            let mut machine = load_with_lane(good, lane);
            let array = machine.demand_i64("good", vec![]).unwrap();
            let (schema, words) = machine.driver.array_words(array).unwrap();
            assert_eq!(schema, "Int", "{lane:?}");
            assert_eq!(words, vec![1, 2], "{lane:?}");
        }
    }

    #[test]
    fn store_values_are_totally_ordered_canonically_on_the_machine() {
        let src = r#"
enum Choice { A, B(Int), C(Int) }

pub fn a() -> Choice { Choice::A }
pub fn b1() -> Choice { Choice::B(1) }
pub fn b2() -> Choice { Choice::B(2) }

pub fn za() -> Map<String, Int> {
    let m: Map<String, Int> = {};
    m.insert("z", 1).insert("a", 2)
}

pub fn az() -> Map<String, Int> {
    let m: Map<String, Int> = {};
    m.insert("a", 2).insert("z", 1)
}
"#;
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            let a = machine.demand_i64("a", vec![]).unwrap();
            let b1 = machine.demand_i64("b1", vec![]).unwrap();
            let b2 = machine.demand_i64("b2", vec![]).unwrap();
            assert_eq!(
                machine.driver.compare_store_words("Choice", a, b1).unwrap(),
                std::cmp::Ordering::Less,
                "{lane:?}"
            );
            assert_eq!(
                machine
                    .driver
                    .compare_store_words("Choice", b1, b2)
                    .unwrap(),
                std::cmp::Ordering::Less,
                "{lane:?}"
            );
            assert_eq!(
                machine
                    .driver
                    .compare_store_words("Float", 1.0f64.to_bits() as i64, 2.0f64.to_bits() as i64,)
                    .unwrap(),
                std::cmp::Ordering::Less,
                "{lane:?}"
            );
            assert_eq!(
                machine
                    .driver
                    .compare_store_words(
                        "Float",
                        f64::INFINITY.to_bits() as i64,
                        f64::NAN.to_bits() as i64,
                    )
                    .unwrap(),
                std::cmp::Ordering::Less,
                "{lane:?}"
            );
            assert_eq!(
                machine
                    .driver
                    .compare_store_words(
                        "Float",
                        0.0f64.to_bits() as i64,
                        (-0.0f64).to_bits() as i64,
                    )
                    .unwrap(),
                std::cmp::Ordering::Equal,
                "{lane:?}"
            );

            let za = machine.demand_i64("za", vec![]).unwrap();
            let az = machine.demand_i64("az", vec![]).unwrap();
            let keys = machine
                .driver
                .map_words(za)
                .unwrap()
                .into_iter()
                .map(|(_, key, _, _, _)| machine.driver.raw_string(key, "String").unwrap())
                .collect::<Vec<_>>();
            assert_eq!(keys, vec!["a".to_string(), "z".to_string()], "{lane:?}");
            assert_eq!(
                za, az,
                "canonical map dedupes construction order on {lane:?}"
            );
            assert_eq!(
                machine
                    .driver
                    .compare_store_words("Map<String,Int>", za, az)
                    .unwrap(),
                std::cmp::Ordering::Equal,
                "{lane:?}"
            );
        }
    }

    #[test]
    fn version_values_parse_and_compare_by_semver_precedence() {
        let src = r#"
pub fn release_after_pre() -> Bool { version("1.0.0") > version("1.0.0-rc.1") }
pub fn numeric_pre_before_alpha() -> Bool { version("1.0.0-1") < version("1.0.0-alpha") }
pub fn prerelease_chain() -> Bool {
    version("1.0.0-alpha") < version("1.0.0-alpha.1")
        && version("1.0.0-alpha.1") < version("1.0.0-alpha.beta")
        && version("1.0.0-beta.2") < version("1.0.0-beta.11")
}
pub fn build_metadata_ignored_by_language_equality() -> Bool {
    version("1.2.3+abc") == version("1.2.3+xyz")
}
pub fn glibc_name_normalizes() -> Bool { version("GLIBC_2.35") == version("2.35.0") }
"#;
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            for name in [
                "release_after_pre",
                "numeric_pre_before_alpha",
                "prerelease_chain",
                "build_metadata_ignored_by_language_equality",
                "glibc_name_normalizes",
            ] {
                assert_eq!(
                    machine.demand_i64(name, vec![]).unwrap(),
                    1,
                    "{lane:?}: {name}"
                );
            }

            let a = machine.driver.intern_version_value("1.2.3+abc").unwrap().0;
            let b = machine.driver.intern_version_value("1.2.3+xyz").unwrap().0;
            assert_eq!(
                machine.driver.compare_store_words("Version", a, b).unwrap(),
                std::cmp::Ordering::Less,
                "store ordering remains total on {lane:?}"
            );
        }
    }

    #[test]
    fn malformed_version_parse_is_a_loud_machine_error() {
        let src = r#"pub fn bad() -> Version { version("1.x") }"#;
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            let err = machine.demand_i64("bad", vec![]).unwrap_err();
            assert!(
                err.contains("version(\"1.x\") parse error"),
                "{lane:?}: {err}"
            );
        }
    }

    #[test]
    fn version_set_values_parse_and_obey_cargo_prerelease_rules() {
        let src = r#"
use vix::{Version, VersionSet};

pub fn ops() -> Bool {
    let pre = VersionSet::from_req("^1.2.3-alpha.1");
    let narrow = VersionSet::from_req("^1.2.3");
    let broad = VersionSet::from_req("^1.0.0");
    let union = narrow.union(broad);
    let intersection = narrow.intersect(broad);
    pre.contains(version("1.2.3-alpha.1"))
        && pre.contains(version("1.2.3"))
        && (pre.contains(version("1.2.4-alpha.1")) == false)
        && narrow.subset(broad)
        && union.contains(version("1.1.0"))
        && intersection.contains(version("1.2.9"))
        && broad.complement().contains(version("2.0.0"))
}
"#;
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);
            assert_eq!(machine.demand_i64("ops", vec![]).unwrap(), 1, "{lane:?}");
        }
    }

    fn semantic_cutoff_demo_source(comparator_name: &str) -> String {
        format!(
            r#"
use vix::{{Version, VersionSet}};

fn expensive(req: VersionSet) -> Int {{
    match req.contains(version("1.2.3")) {{
        true => 42,
        false => 0,
    }}
}}

pub fn derived(req: VersionSet) -> Int {{
    expensive(req)
}}

fn {comparator_name}(old: VersionSet, new: VersionSet) -> Bool {{
    new.subset(old)
}}
"#
        )
    }

    fn call_derived(machine: &mut Machine, req: &str) -> i64 {
        machine
            .call(
                "derived",
                &[NamedArg {
                    name: "req".into(),
                    value: MachineArg::String(req.into()),
                }],
            )
            .unwrap()
            .0
    }

    #[test]
    fn semantic_cutoff_hits_on_narrowing_and_misses_on_widening() {
        let src = semantic_cutoff_demo_source("derived__memo_verify_req");
        for lane in lanes() {
            let mut narrowed = load_with_lane(&src, lane);
            assert_eq!(
                narrowed.semantic_comparator_len("derived"),
                Some(1),
                "{lane:?}"
            );
            assert_eq!(call_derived(&mut narrowed, "^1.0.0"), 42, "{lane:?}");
            narrowed.clear_trace();
            assert_eq!(call_derived(&mut narrowed, "^1.2.0"), 42, "{lane:?}");
            assert_eq!(
                memo_semantic_hit_count(&narrowed, "derived"),
                1,
                "{lane:?}: {:?}",
                narrowed.trace()
            );
            assert_eq!(spawned_count(&narrowed, "derived"), 0, "{lane:?}");
            assert_eq!(spawned_count(&narrowed, "expensive"), 0, "{lane:?}");
            assert_eq!(
                semantic_verified_count(&narrowed, "derived"),
                vec![1],
                "{lane:?}"
            );

            let mut widened = load_with_lane(&src, lane);
            assert_eq!(call_derived(&mut widened, "^1.2.0"), 42, "{lane:?}");
            widened.clear_trace();
            assert_eq!(call_derived(&mut widened, "^1.0.0"), 42, "{lane:?}");
            assert_eq!(memo_semantic_hit_count(&widened, "derived"), 0, "{lane:?}");
            assert_eq!(spawned_count(&widened, "derived"), 1, "{lane:?}");
            assert_eq!(spawned_count(&widened, "expensive"), 1, "{lane:?}");
        }
    }

    #[test]
    fn live_event_sink_streams_the_accumulated_trace() {
        for lane in lanes() {
            for mode in [StepMode::Run, StepMode::Step] {
                let mut machine = load_with_lane(CORPUS, lane);
                let seen = Rc::new(RefCell::new(Vec::new()));
                let commands = Rc::new(RefCell::new(Vec::new()));
                let seen_for_sink = Rc::clone(&seen);
                let commands_for_sink = Rc::clone(&commands);
                machine.set_step_mode(mode);
                machine.set_event_sink(Some(Box::new(move |event| {
                    seen_for_sink.borrow_mut().push(event.clone());
                    let command = if mode == StepMode::Step && commands_for_sink.borrow().len() == 1
                    {
                        StepCommand::Resume
                    } else {
                        StepCommand::Step
                    };
                    commands_for_sink.borrow_mut().push(command);
                    command
                })));

                assert_eq!(machine.demand_i64("poly", vec![3]).unwrap(), 29, "{lane:?}");
                assert_eq!(
                    seen.borrow().as_slice(),
                    machine.trace(),
                    "sink-collected events must match trace() on {lane:?} in {mode:?}"
                );
                assert!(!commands.borrow().is_empty(), "{lane:?} in {mode:?}");
            }
        }
    }

    #[test]
    fn entry_param_metadata_exposes_lowered_schemas() {
        let src = r#"
use vix::{Target, Tree};

pub fn build(target: Target, source: Tree, n: Int) -> Tree {
    source
}
"#;
        for lane in lanes() {
            let machine = load_with_lane(src, lane);
            assert_eq!(
                machine.entry_param_schemas("build"),
                Some(["Target".to_string(), "Tree".to_string(), "Int".to_string()].as_slice()),
                "{lane:?}"
            );
            assert_eq!(
                machine.entry_return_schema("build"),
                Some("Tree"),
                "{lane:?}"
            );
            assert_eq!(machine.entry_param_schemas("missing"), None, "{lane:?}");
        }
    }

    #[test]
    fn render_result_is_schema_aware_and_never_forces_pending_values() {
        let src = r#"
enum Choice { A, B(Int), C { name: String } }
struct Pair { left: Int, right: String }

pub fn tupled() -> (String, Int, Bool) { ("mini", 3, false) }
pub fn picked() -> Choice { Choice::C { name: "lua.o" } }
pub fn paired() -> Pair { Pair { left: 7, right: "ok" } }
pub fn mapped() -> Map<String, Int> {
    let m: Map<String, Int> = {};
    m.insert("b", 2).insert("a", 1)
}
pub fn parsed(input: String) -> Doc { json(input) }

fn key(n: Int) -> String {
    match n {
        0 => "left",
        _ => "right",
    }
}

fn left() -> Float { 1.0 }
fn right() -> Float { 2.0 }

pub fn lazy(n: Int) -> Map<String, Float> {
    let source: Map<String, Float> = {};
    let source = source.insert("left", left()).insert("right", right());
    let value = source.get(key(n)).unwrap();
    let out: Map<String, Float> = {};
    out.insert("selected", value)
}
"#;
        for lane in lanes() {
            let mut machine = load_with_lane(src, lane);

            let tuple = machine.demand_i64("tupled", vec![]).unwrap();
            let RenderedValue::Tuple { schema, fields } =
                machine.render_result("tupled", tuple).unwrap()
            else {
                panic!("tupled renders as tuple on {lane:?}");
            };
            assert_eq!(schema, "Tuple<String,Int,Bool>", "{lane:?}");
            assert_eq!(fields.len(), 3, "{lane:?}");
            assert!(matches!(&fields[0].value, RenderedValue::String { value } if value == "mini"));
            assert!(matches!(fields[1].value, RenderedValue::Int { value: 3 }));
            assert!(matches!(
                fields[2].value,
                RenderedValue::Bool { value: false }
            ));

            let choice = machine.demand_i64("picked", vec![]).unwrap();
            let RenderedValue::Enum {
                variant, fields, ..
            } = machine.render_result("picked", choice).unwrap()
            else {
                panic!("picked renders as enum on {lane:?}");
            };
            assert_eq!(variant, "C", "{lane:?}");
            assert_eq!(fields[0].name, "name", "{lane:?}");
            assert!(
                matches!(&fields[0].value, RenderedValue::String { value } if value == "lua.o")
            );

            let pair = machine.demand_i64("paired", vec![]).unwrap();
            let RenderedValue::Record { fields, .. } =
                machine.render_result("paired", pair).unwrap()
            else {
                panic!("paired renders as record on {lane:?}");
            };
            assert_eq!(fields[0].name, "left", "{lane:?}");
            assert_eq!(fields[1].name, "right", "{lane:?}");

            let map = machine.demand_i64("mapped", vec![]).unwrap();
            let RenderedValue::Map { entries, .. } = machine.render_result("mapped", map).unwrap()
            else {
                panic!("mapped renders as map on {lane:?}");
            };
            assert_eq!(entries.len(), 2, "{lane:?}");
            assert!(matches!(&entries[0].key, RenderedValue::String { value } if value == "a"));
            assert!(matches!(entries[0].value, RenderedValue::Int { value: 1 }));
            assert!(matches!(&entries[1].key, RenderedValue::String { value } if value == "b"));
            assert!(matches!(entries[1].value, RenderedValue::Int { value: 2 }));

            let input = machine
                .driver
                .intern_raw_value(
                    "String",
                    br#"{"package":{"name":"mini-real-crate"},"publish":false}"#.to_vec(),
                )
                .0;
            let doc = machine.demand_i64("parsed", vec![input]).unwrap();
            let RenderedValue::Doc {
                variant,
                value: Some(value),
            } = machine.render_result("parsed", doc).unwrap()
            else {
                panic!("parsed renders as doc map on {lane:?}");
            };
            assert_eq!(variant, "Map", "{lane:?}");
            assert!(matches!(*value, RenderedValue::Map { .. }));

            let tree = machine.intern_tree_concrete(Tree::of(&[("a.txt", "hello")]));
            let RenderedValue::Tree { entries } = machine.render_value("Tree", tree).unwrap()
            else {
                panic!("tree renders as tree on {lane:?}");
            };
            assert_eq!(entries[0].path, "a.txt", "{lane:?}");
            assert_eq!(entries[0].contents, "hello", "{lane:?}");

            let lazy = machine.demand_i64("lazy", vec![0]).unwrap();
            let RenderedValue::Map { entries, .. } = machine.render_result("lazy", lazy).unwrap()
            else {
                panic!("lazy map renders as map on {lane:?}");
            };
            assert_eq!(entries.len(), 1, "{lane:?}");
            assert_eq!(
                entries[0].realization.as_deref(),
                Some("Pending"),
                "{lane:?}"
            );
            assert!(matches!(entries[0].value, RenderedValue::Pending { .. }));
            assert_eq!(spawned_count(&machine, "left"), 0, "{lane:?}");
            assert_eq!(spawned_count(&machine, "right"), 0, "{lane:?}");
        }
    }

    fn assert_lane_traces_equal(traces: &[(Lane, Vec<DriveEvent>)]) {
        let Some((first_lane, first_trace)) = traces.first() else {
            return;
        };
        for (lane, trace) in &traces[1..] {
            assert_eq!(
                trace, first_trace,
                "driver trace diverged between {first_lane:?} and {lane:?}"
            );
        }
    }

    fn rendered_doc_strings(machine: &Machine, name: &str, handle: i64) -> Vec<String> {
        let RenderedValue::Array { items, .. } = machine.render_result(name, handle).unwrap()
        else {
            panic!("{name} did not render as an array");
        };
        items
            .into_iter()
            .map(|item| match item {
                RenderedValue::Doc {
                    variant,
                    value: Some(value),
                } if variant == "String" => match *value {
                    RenderedValue::String { value } => value,
                    other => panic!("{name} doc string payload rendered as {other:?}"),
                },
                RenderedValue::String { value } => value,
                other => panic!("{name} array item rendered as {other:?}"),
            })
            .collect()
    }

    fn rendered_doc_maps(
        machine: &Machine,
        name: &str,
        handle: i64,
    ) -> Vec<BTreeMap<String, String>> {
        let RenderedValue::Array { items, .. } = machine.render_result(name, handle).unwrap()
        else {
            panic!("{name} did not render as an array");
        };
        items
            .into_iter()
            .map(|item| {
                let RenderedValue::Doc {
                    variant,
                    value: Some(value),
                } = item
                else {
                    panic!("{name} item did not render as Doc::Map");
                };
                assert_eq!(variant, "Map");
                let RenderedValue::Map { entries, .. } = *value else {
                    panic!("{name} doc map payload did not render as a map");
                };
                entries
                    .into_iter()
                    .map(|entry| {
                        let RenderedValue::String { value: key } = entry.key else {
                            panic!("{name} map key was not a string");
                        };
                        let value = match entry.value {
                            RenderedValue::Doc {
                                variant,
                                value: Some(value),
                            } if variant == "String" => match *value {
                                RenderedValue::String { value } => value,
                                other => panic!("{name} string field rendered as {other:?}"),
                            },
                            RenderedValue::Doc {
                                variant,
                                value: Some(value),
                            } if variant == "Int" => match *value {
                                RenderedValue::Int { value } => value.to_string(),
                                other => panic!("{name} int field rendered as {other:?}"),
                            },
                            other => panic!("{name} map value rendered as {other:?}"),
                        };
                        (key, value)
                    })
                    .collect()
            })
            .collect()
    }

    fn artifact_probes(machine: &Machine) -> Vec<(String, bool)> {
        artifact_probes_for(machine, "elf")
    }

    fn artifact_probes_for(machine: &Machine, wanted: &str) -> Vec<(String, bool)> {
        machine
            .trace()
            .iter()
            .filter_map(|event| match event {
                DriveEvent::ArtifactProbe {
                    format,
                    projection,
                    cache_hit,
                    ..
                } if format == wanted => Some((projection.clone(), *cache_hit)),
                _ => None,
            })
            .collect()
    }

    fn ast_artifact_probes(machine: &Machine) -> Vec<(String, bool)> {
        machine
            .trace()
            .iter()
            .filter_map(|event| match event {
                DriveEvent::ArtifactProbe {
                    format,
                    projection,
                    cache_hit,
                    ..
                } if format == "ast" => Some((projection.clone(), *cache_hit)),
                _ => None,
            })
            .collect()
    }

    fn trace_hash(value: &str) -> u64 {
        let mut h = blake3::Hasher::new();
        h.update(b"vix-debug-u64");
        h.update(format!("{value:?}").as_bytes());
        let hash = h.finalize();
        u64::from_le_bytes(hash.as_bytes()[..8].try_into().expect("blake3 prefix"))
    }

    fn expected_object() -> BTreeMap<String, String> {
        BTreeMap::from([(
            "wanted.o".to_string(),
            "obj(b1fc5679f1748a8f259a9d9ea09d1c81ef43028c7f65aba0ed7a947d04da7251)".to_string(),
        )])
    }

    fn spawned_count(machine: &Machine, name: &str) -> usize {
        let hash = machine.fn_hash(name).expect("function hash");
        machine
            .trace()
            .iter()
            .filter(|event| matches!(event, DriveEvent::Spawned { fn_hash } if *fn_hash == hash))
            .count()
    }

    fn memo_semantic_hit_count(machine: &Machine, name: &str) -> usize {
        let hash = machine.fn_hash(name).expect("function hash");
        machine
            .trace()
            .iter()
            .filter(|event| {
                matches!(event, DriveEvent::MemoSemanticHit { fn_hash, .. } if *fn_hash == hash)
            })
            .count()
    }

    fn semantic_verified_count(machine: &Machine, name: &str) -> Vec<usize> {
        let hash = machine.fn_hash(name).expect("function hash");
        machine
            .trace()
            .iter()
            .filter_map(|event| match event {
                DriveEvent::MemoSemanticHit { fn_hash, verified } if *fn_hash == hash => {
                    Some(*verified)
                }
                _ => None,
            })
            .collect()
    }

    fn host_call_count(machine: &Machine, name: &str, host: u32) -> usize {
        machine
            .fn_ops(name)
            .expect("function ops")
            .iter()
            .filter(|op| matches!(op, Op::HostCall { host: op_host } if *op_host == host))
            .count()
    }

    fn run_requested_count(machine: &Machine) -> usize {
        machine
            .trace()
            .iter()
            .filter(|event| matches!(event, DriveEvent::RunRequested { .. }))
            .count()
    }

    fn cross_target_handle(machine: &Machine) -> i64 {
        let os = if cfg!(target_os = "linux") { 1 } else { 0 };
        let arch = if cfg!(target_arch = "x86_64") { 1 } else { 0 };
        machine
            .driver
            .intern_target(os, arch)
            .expect("cross target")
            .0
    }

    fn host_proc_macro_dylib_name() -> String {
        let ext = if cfg!(target_os = "macos") {
            "dylib"
        } else if cfg!(target_os = "windows") {
            "dll"
        } else {
            "so"
        };
        format!("libemit_answer_macro.{ext}")
    }

    fn has_arg_pair(argv: &[String], flag: &str, value: &str) -> bool {
        argv.windows(2)
            .any(|pair| pair[0] == flag && pair[1] == value)
    }

    fn run_outputs(machine: &Machine, pick: impl Fn(&DriveEvent) -> Option<u64>) -> Vec<u64> {
        let mut outputs: Vec<u64> = machine.trace().iter().filter_map(pick).collect();
        outputs.sort();
        outputs
    }

    fn started_outputs(machine: &Machine) -> Vec<u64> {
        run_outputs(machine, |event| match event {
            DriveEvent::RunStarted {
                command, output, ..
            } => {
                assert_eq!(*command, trace_hash("cc"));
                Some(*output)
            }
            _ => None,
        })
    }

    fn completed_outputs(machine: &Machine) -> Vec<u64> {
        run_outputs(machine, |event| match event {
            DriveEvent::RunCompleted {
                command, output, ..
            } => {
                assert_eq!(*command, trace_hash("cc"));
                Some(*output)
            }
            _ => None,
        })
    }

    fn has_two_starts_before_any_completion(machine: &Machine) -> bool {
        let mut starts = 0usize;
        for event in machine.trace() {
            match event {
                DriveEvent::RunStarted { .. } => {
                    starts += 1;
                    if starts >= 2 {
                        return true;
                    }
                }
                DriveEvent::RunCompleted { .. } => return false,
                _ => {}
            }
        }
        false
    }

    fn output_set(paths: &[&str]) -> Vec<u64> {
        let mut values: Vec<u64> = paths.iter().map(|path| trace_hash(path)).collect();
        values.sort();
        values
    }

    fn load_merge_demand(lane: Lane) -> Machine {
        load_with_lane(
            include_str!("../../../playgrounds/snark/src/bundled/vix/samples/merge-demand.vix"),
            lane,
        )
    }

    #[test]
    fn async_backend_merge_starts_sibling_runs_before_joining_any() {
        let src = r#"
use vix::{Tree, Path, Target};
use caps::Cc;

fn object(cc: Cc, unit: Path) -> Tree {
    cc! { -o {unit.with_ext("o")} }
}

pub fn both(target: Target) -> Tree {
    let cc = Cc::acquire(target);
    let units = [p"a.c", p"b.c"];
    units.map(|u| object(cc, u)).collect()
}
"#;
        for lane in lanes() {
            let backend = Arc::new(DeferredExecBackend);
            let mut machine = Machine::load_with_lane(src, lane)
                .unwrap_or_else(|err| panic!("loads on {lane:?}: {err}"))
                .with_exec_backend(backend);
            let target = machine.linux_target_handle();
            let handle = machine.demand_i64("both", vec![target]).unwrap();
            let entries = machine.tree_entries(handle).unwrap();
            assert_eq!(
                entries.keys().cloned().collect::<Vec<_>>(),
                vec!["a.o".to_string(), "b.o".to_string()],
                "{lane:?}: {entries:?}"
            );
            assert!(
                has_two_starts_before_any_completion(&machine),
                "{lane:?}: {:?}",
                machine.trace()
            );
        }
    }

    #[test]
    fn merge_demand_selected_tunnels_and_never_runs_left() {
        let mut cold_traces = Vec::new();
        let mut first_handle = None;
        for lane in lanes() {
            let mut machine = load_merge_demand(lane);
            let target = machine.linux_target_handle();
            let handle = machine.demand_i64("selected", vec![target]).unwrap();

            assert_eq!(
                machine.tree_entries(handle).unwrap(),
                expected_object(),
                "{lane:?}"
            );
            assert_eq!(spawned_count(&machine, "selected"), 1, "{lane:?}");
            assert_eq!(spawned_count(&machine, "object"), 1, "{lane:?}");
            assert_eq!(
                started_outputs(&machine),
                output_set(&["wanted.o"]),
                "{lane:?}"
            );
            assert_eq!(
                completed_outputs(&machine),
                output_set(&["wanted.o"]),
                "{lane:?}"
            );
            assert!(
                !machine
                    .trace()
                    .iter()
                    .any(|event| matches!(event, DriveEvent::RunRequested { output, .. } if *output == trace_hash("left.o"))),
                "left.o producer is never requested on {lane:?}"
            );
            if let Some(expected) = first_handle {
                assert_eq!(handle, expected, "same selected result handle on {lane:?}");
            } else {
                first_handle = Some(handle);
            }
            cold_traces.push((lane, machine.trace().to_vec()));

            let selected_hash = machine.fn_hash("selected").expect("selected hash");
            machine.clear_trace();
            let warm = machine.demand_i64("selected", vec![target]).unwrap();
            assert_eq!(warm, handle, "{lane:?}");
            assert_eq!(
                machine.trace(),
                &[
                    DriveEvent::Demanded {
                        fn_hash: selected_hash
                    },
                    DriveEvent::MemoHit {
                        fn_hash: selected_hash
                    },
                ],
                "warm selected demand is exactly root memo hit on {lane:?}"
            );
        }
        assert_lane_traces_equal(&cold_traces);
    }

    #[test]
    fn merge_demand_fallback_falls_left_after_right_absence() {
        let mut cold_traces = Vec::new();
        let mut first_handle = None;
        for lane in lanes() {
            let mut machine = load_merge_demand(lane);
            let target = machine.linux_target_handle();
            let handle = machine.demand_i64("fallback", vec![target]).unwrap();

            assert_eq!(
                machine.tree_entries(handle).unwrap(),
                expected_object(),
                "{lane:?}"
            );
            assert_eq!(spawned_count(&machine, "fallback"), 1, "{lane:?}");
            assert_eq!(
                spawned_count(&machine, "object"),
                2,
                "right.o is run to prove absence, then wanted.o is demanded on {lane:?}"
            );
            assert_eq!(
                started_outputs(&machine),
                output_set(&["right.o", "wanted.o"]),
                "{lane:?}"
            );
            assert_eq!(
                completed_outputs(&machine),
                output_set(&["right.o", "wanted.o"]),
                "{lane:?}"
            );
            assert!(
                !machine
                    .trace()
                    .iter()
                    .any(|event| matches!(event, DriveEvent::RunRequested { output, .. } if *output == trace_hash("left.o"))),
                "left.o is outside fallback's demanded path on {lane:?}"
            );
            if let Some(expected) = first_handle {
                assert_eq!(handle, expected, "same fallback result handle on {lane:?}");
            } else {
                first_handle = Some(handle);
            }
            cold_traces.push((lane, machine.trace().to_vec()));
        }
        assert_lane_traces_equal(&cold_traces);
    }

    #[test]
    fn merge_demand_subtree_chain_refines_without_left() {
        let mut cold_traces = Vec::new();
        let mut first_handle = None;
        for lane in lanes() {
            let mut machine = load_merge_demand(lane);
            let target = machine.linux_target_handle();
            let handle = machine.demand_i64("subtree_chain", vec![target]).unwrap();

            assert_eq!(
                machine.tree_entries(handle).unwrap(),
                expected_object(),
                "{lane:?}"
            );
            assert_eq!(spawned_count(&machine, "subtree_chain"), 1, "{lane:?}");
            assert_eq!(spawned_count(&machine, "object"), 1, "{lane:?}");
            assert_eq!(
                started_outputs(&machine),
                output_set(&["x/wanted.o"]),
                "{lane:?}"
            );
            assert_eq!(
                completed_outputs(&machine),
                output_set(&["x/wanted.o"]),
                "{lane:?}"
            );
            assert!(
                !machine
                    .trace()
                    .iter()
                    .any(|event| matches!(event, DriveEvent::RunRequested { output, .. } if *output == trace_hash("left.o"))),
                "left.o producer is never requested through the subtree chain on {lane:?}"
            );
            if let Some(expected) = first_handle {
                assert_eq!(
                    handle, expected,
                    "same subtree_chain result handle on {lane:?}"
                );
            } else {
                first_handle = Some(handle);
            }
            cold_traces.push((lane, machine.trace().to_vec()));
        }
        assert_lane_traces_equal(&cold_traces);
    }
}
