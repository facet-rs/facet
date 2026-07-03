use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};

use crate::ast::{self, Expr, Item, SourceFile};
use crate::VixParser;

#[derive(Clone)]
pub(crate) struct EnumInfo {
    pub(crate) variants: Vec<(String, VariantShape)>,
}

#[derive(Clone)]
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
}

pub(crate) fn load_module_tables(source: &str) -> Result<ModuleTables, String> {
    // Table construction is the expensive part (seconds in dev profile);
    // the parser itself is immutable after build — share one per process.
    static PARSER: std::sync::OnceLock<VixParser> = std::sync::OnceLock::new();
    let parser = PARSER.get_or_init(VixParser::new);
    let file: SourceFile = parser.parse(source).map_err(|e| e.message)?;
    let mut fns = HashMap::new();
    let mut fn_hashes = HashMap::new();
    let mut enums = HashMap::new();
    let mut structs = HashMap::new();
    for item in &file.items {
        match item {
            Item::Fn(f) => {
                fn_hashes.insert(f.name.value.clone(), canon_ast_hash(f));
                fns.insert(f.name.value.clone(), (**f).clone());
            }
            Item::Enum(e) => {
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
    Ok(ModuleTables {
        fns,
        fn_hashes,
        enums,
        structs,
    })
}

fn canon_ast_hash(item: &ast::FnItem) -> u64 {
    let mut canonical = item.clone();
    canonical.strip_spans();
    let bytes = phon::api::encode(&canonical).expect("AST serializes");
    let mut h = DefaultHasher::new();
    bytes.hash(&mut h);
    h.finish()
}
