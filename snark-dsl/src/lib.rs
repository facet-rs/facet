#[cfg(feature = "native")]
use std::{
    collections::BTreeMap,
    ffi::OsStr,
    fs,
    io::Write as _,
    path::{Path, PathBuf},
    process::Command,
};

#[cfg(feature = "typed-ast")]
pub mod typed_ast;

const OFFICIAL_TREE_SITTER_DSL: &str = include_str!("../vendor/tree-sitter-generate-0.26.9/dsl.js");
const OFFICIAL_ENTRYPOINT_MARKER: &str =
    "const grammarPath = getEnv(\"TREE_SITTER_GRAMMAR_PATH\");";
#[cfg(feature = "native")]
pub const DEFAULT_LUA_GRAMMAR: &str =
    "/Users/amos/oss/arborium/langs/group-hazel/lua/def/grammar/grammar.js";

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[cfg(feature = "native")]
    #[error("failed to read {path}: {source}")]
    Read {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[cfg(feature = "native")]
    #[error("failed to write {path}: {source}")]
    Write {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[cfg(feature = "native")]
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[cfg(feature = "native")]
    #[error("tree-sitter generate failed with status {status}: {stderr}")]
    TreeSitterFailed { status: String, stderr: String },
    #[cfg(feature = "native")]
    #[error("no JavaScript runtime found on PATH (tried: node, bun)")]
    JsRuntimeNotFound,
    #[cfg(feature = "native")]
    #[error("failed while evaluating {path}: {message}")]
    Js { path: String, message: String },
    #[cfg(feature = "native")]
    #[error("expected JavaScript string from {operation}, got {value}")]
    ExpectedString {
        operation: &'static str,
        value: String,
    },
    #[cfg(feature = "native")]
    #[error("failed to convert JavaScript string from {operation}: {message}")]
    InvalidJsString {
        operation: &'static str,
        message: String,
    },
    #[cfg(feature = "native")]
    #[error("emitted output differs from tree-sitter output")]
    Mismatch,
    #[error("official tree-sitter DSL entrypoint marker was not found")]
    OfficialDslMarkerMissing,
    #[error("{0}")]
    Usage(String),
}

pub fn official_tree_sitter_dsl_source() -> &'static str {
    OFFICIAL_TREE_SITTER_DSL
}

const COMMONJS_SHIM: &str = "globalThis.module = { exports: {} };\nglobalThis.exports = globalThis.module.exports;\nglobalThis.console ??= { log() {}, warn() {}, error() {} };\nglobalThis.process ??= { env: {} };";

#[cfg(feature = "native")]
pub fn emit_source_with_boa(grammar_source: &str, source_name: &str) -> Result<String> {
    let mut session = JsSession::new();
    session.exec(
        official_tree_sitter_dsl_prelude()?,
        "vendor/tree-sitter-generate-0.26.9/dsl.js",
    );
    session.exec(COMMONJS_SHIM, "commonjs-shim.js");
    session.exec(SNARK_DSL_EXTENSIONS, "snark-dsl-extensions.js");
    session.exec(grammar_source, source_name);
    session.eval_to_string(EMIT_SCRIPT, "emit.js")
}

/// Like [`emit_source_with_boa`], but also returns the AST-enrichment annotations the
/// grammar registered via the `ast({...})` helper (JSON keyed by node kind). The grammar
/// JSON is byte-for-byte what `emit_source_with_boa` produces — annotations ride a side
/// registry, so tree-sitter compatibility is untouched.
#[cfg(feature = "native")]
pub fn emit_source_with_annotations_boa(
    grammar_source: &str,
    source_name: &str,
) -> Result<(String, String)> {
    let mut session = JsSession::new();
    session.exec(
        official_tree_sitter_dsl_prelude()?,
        "vendor/tree-sitter-generate-0.26.9/dsl.js",
    );
    session.exec(COMMONJS_SHIM, "commonjs-shim.js");
    session.exec(SNARK_DSL_EXTENSIONS, "snark-dsl-extensions.js");
    session.exec(grammar_source, source_name);
    let grammar_json = session.eval_to_string(EMIT_SCRIPT, "emit.js")?;
    let annotations_json = session.eval_to_string(
        "JSON.stringify(globalThis.__snark_ast ?? {})",
        "ast-emit.js",
    )?;
    Ok((grammar_json, annotations_json))
}

/// Run standalone `ast({...})` annotation source (with the tree-sitter DSL + snark
/// extensions available) and return the captured registry JSON. Useful when the grammar
/// is emitted separately and only the AST enrichment is needed.
#[cfg(feature = "native")]
pub fn annotations_from_source(source: &str, source_name: &str) -> Result<String> {
    let mut session = JsSession::new();
    session.exec(
        official_tree_sitter_dsl_prelude()?,
        "vendor/tree-sitter-generate-0.26.9/dsl.js",
    );
    session.exec(COMMONJS_SHIM, "commonjs-shim.js");
    session.exec(SNARK_DSL_EXTENSIONS, "snark-dsl-extensions.js");
    session.exec(source, source_name);
    session.eval_to_string(
        "JSON.stringify(globalThis.__snark_ast ?? {})",
        "ast-emit.js",
    )
}

#[cfg(all(test, feature = "native"))]
mod ast_annotation_tests {
    use super::emit_source_with_annotations_boa;

    #[test]
    fn captures_ast_annotations_without_touching_grammar_json() {
        let src = r#"
module.exports = grammar({
  name: "toy",
  rules: {
    source: $ => repeat($.number),
    number: $ => /[0-9]+/,
  },
});
ast({
  number: { decode: "i64" },
  binary: { as: "BinaryExpr", fields: { left: "Expr", right: "Expr" } },
});
"#;
        let (grammar, annotations) =
            emit_source_with_annotations_boa(src, "toy.js").expect("emit toy grammar");
        // grammar.json is standard tree-sitter — the ast() call left no trace in it.
        assert!(grammar.contains("\"number\""), "grammar: {grammar}");
        assert!(
            !grammar.contains("BinaryExpr"),
            "annotation leaked into grammar.json: {grammar}"
        );
        // annotations came back keyed by node kind.
        assert!(
            annotations.contains("\"decode\":\"i64\""),
            "annotations: {annotations}"
        );
        assert!(
            annotations.contains("\"as\":\"BinaryExpr\""),
            "annotations: {annotations}"
        );
    }
}

#[cfg(feature = "native")]
pub fn emit_with_boa(grammar_path: &Path) -> Result<String> {
    emit_grammar_file_with_boa(grammar_path)
}

#[cfg(feature = "native")]
pub fn emit_with_tree_sitter(grammar_path: &Path) -> Result<String> {
    let temp = tempfile::tempdir()?;
    let output = Command::new("tree-sitter")
        .args(["generate", "--no-parser", "--output"])
        .arg(temp.path())
        .arg(grammar_path)
        .output()?;

    if !output.status.success() {
        return Err(Error::TreeSitterFailed {
            status: output.status.to_string(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        });
    }

    let grammar_json = temp.path().join("grammar.json");
    read_to_string(&grammar_json)
}

#[cfg(feature = "native")]
pub fn check_against_tree_sitter(grammar_path: &Path) -> Result<()> {
    let boa_json = emit_with_boa(grammar_path)?;
    let tree_sitter_json = emit_with_tree_sitter(grammar_path)?;

    if boa_json == tree_sitter_json {
        Ok(())
    } else {
        Err(Error::Mismatch)
    }
}

pub fn official_tree_sitter_dsl_prelude() -> Result<&'static str> {
    OFFICIAL_TREE_SITTER_DSL
        .split_once(OFFICIAL_ENTRYPOINT_MARKER)
        .map(|(prelude, _)| prelude)
        .ok_or(Error::OfficialDslMarkerMissing)
}

#[cfg(feature = "native")]
fn read_to_string(path: &Path) -> Result<String> {
    fs::read_to_string(path).map_err(|source| Error::Read {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(feature = "native")]
fn emit_grammar_file_with_boa(grammar_path: &Path) -> Result<String> {
    let root = grammar_module_root(grammar_path);
    let entry = grammar_path
        .strip_prefix(root)
        .unwrap_or(grammar_path)
        .to_string_lossy()
        .replace('\\', "/");
    let mut modules = BTreeMap::new();
    collect_js_modules(root, root, &mut modules)?;

    let mut loader = String::new();
    loader.push_str("const __snark_module_sources = new Map([\n");
    for (path, source) in modules {
        loader.push_str("  [");
        loader.push_str(&js_string_literal(&path));
        loader.push_str(", ");
        loader.push_str(&js_string_literal(&source));
        loader.push_str("],\n");
    }
    loader.push_str("]);\n");
    loader.push_str(COMMONJS_LOADER);
    loader.push_str("globalThis.module = { exports: __snark_load_module(");
    loader.push_str(&js_string_literal(&entry));
    loader.push_str(") };\n");
    loader.push_str("globalThis.exports = globalThis.module.exports;\n");

    emit_source_with_boa(&loader, &grammar_path.display().to_string())
}

#[cfg(feature = "native")]
fn grammar_module_root(grammar_path: &Path) -> &Path {
    if let Some(root) = grammar_path
        .ancestors()
        .find(|ancestor| ancestor.file_name().is_some_and(|name| name == "langs"))
        .and_then(Path::parent)
    {
        return root;
    }
    grammar_path
        .parent()
        .and_then(|parent| {
            if parent.file_name().is_some_and(|name| name == "grammar") {
                parent.parent()
            } else {
                Some(parent)
            }
        })
        .unwrap_or_else(|| Path::new("."))
}

#[cfg(feature = "native")]
fn collect_js_modules(
    root: &Path,
    dir: &Path,
    modules: &mut BTreeMap<String, String>,
) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_js_modules(root, &path, modules)?;
        } else if path
            .extension()
            .is_some_and(|extension| extension == "js" || extension == "mjs" || extension == "json")
        {
            let key = path
                .strip_prefix(root)
                .unwrap_or(&path)
                .to_string_lossy()
                .replace('\\', "/");
            modules.insert(key, read_to_string(&path)?);
        }
    }
    Ok(())
}

#[cfg(feature = "native")]
fn js_string_literal(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\u{2028}' => out.push_str("\\u2028"),
            '\u{2029}' => out.push_str("\\u2029"),
            ch if ch.is_control() => {
                use std::fmt::Write as _;
                let _ = write!(out, "\\u{:04X}", ch as u32);
            }
            ch => out.push(ch),
        }
    }
    out.push('"');
    out
}

#[cfg(feature = "native")]
pub fn write_string(path: &std::path::Path, contents: &str) -> Result<()> {
    fs::write(path, contents).map_err(|source| Error::Write {
        path: path.to_path_buf(),
        source,
    })
}

/// Accumulates a sequence of JS chunks (prelude, shims, the grammar itself, ...) that all
/// share one global scope, then hands them to a real JS runtime (`node`, falling back to
/// `bun`) as a single script per evaluated result. Mirrors the shape of the old
/// Boa-`Context`-backed API: `exec` runs a chunk for its side effects, `eval_to_string`
/// re-runs everything accumulated so far plus one final expression and captures its value.
#[cfg(feature = "native")]
struct JsSession {
    script: String,
}

#[cfg(feature = "native")]
impl JsSession {
    fn new() -> Self {
        Self {
            script: String::new(),
        }
    }

    fn exec(&mut self, source: &str, path: &str) {
        self.script.push_str(&Self::guarded_eval_statement(
            source,
            path,
            "process.exit(1);",
        ));
    }

    /// Re-runs everything executed so far, plus `source`, in a fresh process, and returns
    /// `source`'s completion value (which must be a JS string).
    fn eval_to_string(&self, source: &str, path: &str) -> Result<String> {
        let mut script = self.script.clone();
        script.push_str(&format!(
            "const __snark_out = eval({});\n\
             if (typeof __snark_out !== \"string\") {{\n\
             \x20\x20process.stderr.write(\"expected a string result from {path}, got \" + (typeof __snark_out) + \": \" + String(__snark_out));\n\
             \x20\x20process.exit(2);\n\
             }}\n\
             process.stdout.write(__snark_out);\n",
            js_string_literal(&with_source_url(source, path)),
        ));
        run_js(&script, path)
    }

    fn guarded_eval_statement(source: &str, path: &str, on_error: &str) -> String {
        format!(
            "try {{\n  eval({});\n}} catch (e) {{\n  process.stderr.write({} + \": \" + (e && e.stack ? e.stack : String(e)));\n  {on_error}\n}}\n",
            js_string_literal(&with_source_url(source, path)),
            js_string_literal(path),
        )
    }
}

#[cfg(feature = "native")]
fn with_source_url(source: &str, path: &str) -> String {
    format!("{source}\n//# sourceURL={path}")
}

#[cfg(feature = "native")]
fn run_js(script: &str, path: &str) -> Result<String> {
    let mut file = tempfile::Builder::new()
        .prefix("snark-dsl-")
        .suffix(".mjs")
        .tempfile()?;
    file.write_all(script.as_bytes())?;
    file.flush()?;

    let runtime = js_runtime()?;
    let output = Command::new(runtime).arg(file.path()).output()?;

    match output.status.code() {
        Some(2) => Err(Error::ExpectedString {
            operation: "js output",
            value: String::from_utf8_lossy(&output.stderr).into_owned(),
        }),
        Some(0) => String::from_utf8(output.stdout).map_err(|error| Error::InvalidJsString {
            operation: "js output",
            message: error.to_string(),
        }),
        _ => Err(Error::Js {
            path: path.to_string(),
            message: String::from_utf8_lossy(&output.stderr).into_owned(),
        }),
    }
}

#[cfg(feature = "native")]
fn js_runtime() -> Result<&'static str> {
    for candidate in ["node", "bun"] {
        if Command::new(candidate)
            .arg("--version")
            .output()
            .is_ok_and(|output| output.status.success())
        {
            return Ok(candidate);
        }
    }
    Err(Error::JsRuntimeNotFound)
}

#[cfg(feature = "native")]
pub fn grammar_arg(arg: Option<&OsStr>) -> PathBuf {
    arg.map_or_else(|| PathBuf::from(DEFAULT_LUA_GRAMMAR), PathBuf::from)
}

#[cfg(feature = "native")]
const SNARK_DSL_EXTENSIONS: &str = r#"
globalThis.until = function until(...markers) {
  return { type: "UNTIL", markers: markers.flat() };
};

globalThis.nested = function nested(open, close) {
  return { type: "NESTED", open, close };
};

globalThis.auto_close = function auto_close(options) {
  return {
    type: "AUTO_CLOSE",
    tag: options.tag,
    open: options.open,
    close: options.close,
    closed_by: options.closed_by,
    open_node: options.open_node,
    close_node: options.close_node,
    tag_name_node: options.tag_name_node,
    start_prefix: options.start_prefix,
    end_prefix: options.end_prefix,
    closed_by_tags: options.closed_by_tags,
    rules: options.rules,
  };
};

// AST-enrichment annotations keyed by node kind (`as`/`decode`/`drop`/`enum`/`fields`).
// This is the wrapper channel: pure metadata capture that snark's codegen reads AFTER
// the grammar runs. It never touches `module.exports`, so `grammar.json`/`node-types.json`
// stay 100% standard tree-sitter — the grammar remains portable while carrying AST types.
globalThis.__snark_ast = {};
globalThis.ast = function ast(spec) {
  for (const kind in spec) globalThis.__snark_ast[kind] = spec[kind];
  return spec;
};
"#;

#[cfg(feature = "native")]
const COMMONJS_LOADER: &str = r#"
const __snark_module_cache = new Map();

function __snark_normalize_path(path) {
  const out = [];
  for (const part of path.replaceAll("\\", "/").split("/")) {
    if (!part || part === ".") continue;
    if (part === "..") out.pop();
    else out.push(part);
  }
  return out.join("/");
}

function __snark_dirname(path) {
  const normalized = __snark_normalize_path(path);
  const index = normalized.lastIndexOf("/");
  return index >= 0 ? normalized.slice(0, index) : "";
}

function __snark_resolve_module(parent, specifier) {
  if (!specifier.startsWith("./") && !specifier.startsWith("../")) {
    const dependency = __snark_resolve_grammar_dependency(specifier);
    if (dependency !== null) return dependency;
    throw new Error(`cannot require non-relative grammar module ${specifier}`);
  }
  const base = __snark_dirname(parent);
  const path = __snark_normalize_path((base ? base + "/" : "") + specifier);
  const paths = [path];
  if ((base === "grammar" || base.endsWith("/grammar")) && specifier.startsWith("../")) {
    paths.push(__snark_normalize_path(base + "/" + specifier.slice(3)));
  }
  const candidates = paths.flatMap(path => [
    path,
    path + ".js",
    path + ".mjs",
    path + ".json",
    path + "/index.js",
    path + "/index.mjs",
    path + "/index.json",
    path + "/grammar.js",
    path + "/grammar.mjs",
  ]);
  for (const candidate of candidates) {
    if (__snark_module_sources.has(candidate)) return candidate;
  }
  throw new Error(`could not resolve grammar module ${specifier} from ${parent}`);
}

function __snark_resolve_grammar_dependency(specifier) {
  const match = /^tree-sitter-([^/]+)\/grammar(?:\.js)?$/.exec(specifier);
  if (!match) return null;
  const grammarId = match[1];
  for (const candidate of [
    `node_modules/tree-sitter-${grammarId}/grammar.js`,
    `tree-sitter-${grammarId}/grammar.js`,
    `langs/${grammarId}/def/grammar/grammar.js`,
  ]) {
    if (__snark_module_sources.has(candidate)) return candidate;
  }
  for (const key of __snark_module_sources.keys()) {
    if (
      key.endsWith(`/node_modules/tree-sitter-${grammarId}/grammar.js`) ||
      key.endsWith(`/tree-sitter-${grammarId}/grammar.js`) ||
      key.endsWith(`/${grammarId}/def/grammar/grammar.js`)
    ) {
      return key;
    }
  }
  return null;
}

function __snark_load_module(path) {
  const resolved = __snark_normalize_path(path);
  if (__snark_module_cache.has(resolved)) return __snark_module_cache.get(resolved).exports;
  const source = __snark_module_sources.get(resolved);
  if (source === undefined) throw new Error(`missing grammar module ${resolved}`);
  const module = { exports: {} };
  __snark_module_cache.set(resolved, module);
  if (resolved.endsWith(".json")) {
    module.exports = JSON.parse(source);
    return module.exports;
  }
  const require = specifier => __snark_load_module(__snark_resolve_module(resolved, specifier));
  const commonjs = __snark_source_to_commonjs(source, resolved);
  const execute = new Function("module", "exports", "require", "__default", commonjs + "\n; return module.exports;");
  module.exports = execute(module, module.exports, require, __snark_default_export);
  return module.exports;
}

function __snark_default_export(value) {
  return value && typeof value === "object" && "default" in value ? value.default : value;
}

function __snark_source_to_commonjs(source, path) {
  let out = source;
  out = out.replace(
    /(^|\n)[ \t]*import\s+\*\s+as\s+([A-Za-z_$][\w$]*)\s+from\s+['"]([^'"]+)['"][ \t]*;?/g,
    (_match, prefix, name, specifier) => `${prefix}const ${name} = require(${JSON.stringify(specifier)});`,
  );
  out = out.replace(
    /(^|\n)[ \t]*import\s+\{([\s\S]*?)\}\s+from\s+['"]([^'"]+)['"][ \t]*;?/g,
    (_match, prefix, names, specifier) => `${prefix}const { ${__snark_named_import_bindings(names)} } = require(${JSON.stringify(specifier)});`,
  );
  out = out.replace(
    /(^|\n)[ \t]*import\s+([A-Za-z_$][\w$]*)\s+from\s+['"]([^'"]+)['"][ \t]*;?/g,
    (_match, prefix, name, specifier) => `${prefix}const ${name} = __default(require(${JSON.stringify(specifier)}));`,
  );
  out = out.replace(
    /(^|\n)\s*export\s+const\s+([A-Za-z_$][\w$]*)\s*=/g,
    (_match, prefix, name) => `${prefix}const ${name} = exports.${name} =`,
  );
  out = out.replace(
    /(^|\n)\s*export\s+function\s+([A-Za-z_$][\w$]*)\s*\(/g,
    (_match, prefix, name) => `${prefix}exports.${name} = function ${name}(`,
  );
  out = out.replace(/(^|\n)\s*export\s+default\s+/m, "$1module.exports.default = ");
  if (/^\s*(import|export)\s/m.test(out)) {
    throw new Error(`${path} uses unsupported ESM syntax`);
  }
  return out;
}

function __snark_named_import_bindings(names) {
  return names
    .split(",")
    .map(name => name.trim())
    .filter(Boolean)
    .map(name => {
      const alias = /^([A-Za-z_$][\w$]*)\s+as\s+([A-Za-z_$][\w$]*)$/.exec(name);
      return alias ? `${alias[1]}: ${alias[2]}` : name;
    })
    .join(", ");
}
"#;

#[cfg(feature = "native")]
const EMIT_SCRIPT: &str = r#"
const defaultExport = module.exports && module.exports.default;
const grammarObj = module.exports && module.exports.grammar
  ? module.exports.grammar
  : defaultExport && defaultExport.grammar
    ? defaultExport.grammar
    : defaultExport && defaultExport.name
      ? defaultExport
      : module.exports;
normalizeBoaPatternSources(grammarObj);
JSON.stringify({
  "$schema": "https://tree-sitter.github.io/tree-sitter/assets/schemas/grammar.schema.json",
  ...grammarObj,
}, null, 2);

function normalizeBoaPatternSources(root) {
  const stack = [root];
  while (stack.length > 0) {
    const value = stack.pop();
    if (!value || typeof value !== "object") continue;

    if (value.type === "PATTERN" && typeof value.value === "string") {
      value.value = normalizePatternSourceLikeNode(value.value);
    }

    for (const key of Object.keys(value)) {
      stack.push(value[key]);
    }
  }
}

function normalizePatternSourceLikeNode(source) {
  let out = "";
  let escaped = false;
  let inCharacterClass = false;

  for (const ch of source) {
    if (escaped) {
      if (inCharacterClass && ch === "/") {
        out += "/";
      } else {
        out += "\\" + ch;
      }
      escaped = false;
      continue;
    }

    if (ch === "\\") {
      escaped = true;
      continue;
    }

    if (ch === "[") {
      inCharacterClass = true;
    } else if (ch === "]") {
      inCharacterClass = false;
    }

    out += ch;
  }

  if (escaped) out += "\\";
  return out;
}
"#;

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "native")]
    #[test]
    fn emits_grammar_source_with_boa() {
        let json = emit_source_with_boa(
            "module.exports = grammar({ name: 'mini', rules: { source_file: $ => repeat($.item), item: $ => token(/a+/) } });",
            "mini/grammar.js",
        )
        .unwrap();

        assert!(json.contains("\"name\": \"mini\""));
        assert!(json.contains("\"source_file\""));
        assert!(json.contains("\"PATTERN\""));
    }

    #[cfg(feature = "native")]
    #[test]
    fn emits_snark_lexical_primitives_with_boa() {
        let json = emit_source_with_boa(
            "module.exports = grammar({ name: 'mini', rules: { source_file: $ => seq(token(until('{{', '{#')), token(nested('{#', '#}'))) } });",
            "mini/grammar.js",
        )
        .unwrap();

        assert!(json.contains("\"type\": \"UNTIL\""));
        assert!(json.contains("\"markers\""));
        assert!(json.contains("\"type\": \"NESTED\""));
        assert!(json.contains("\"open\": \"{#\""));
    }

    #[cfg(feature = "native")]
    #[test]
    fn emits_snark_auto_close_primitive_with_boa() {
        let json = emit_source_with_boa(
            "module.exports = grammar({ name: 'mini', rules: { source_file: $ => seq('<p>', auto_close({ tag: 'implicit_end_tag', open_node: 'start_tag', close_node: 'end_tag', tag_name_node: 'tag_name', start_prefix: '<', end_prefix: '</', rules: [{ tag: 'p', closed_by_tags: ['p', 'div'] }] })) } });",
            "mini/grammar.js",
        )
        .unwrap();

        assert!(json.contains("\"type\": \"AUTO_CLOSE\""));
        assert!(json.contains("\"tag\": \"implicit_end_tag\""));
        assert!(json.contains("\"open_node\": \"start_tag\""));
        assert!(json.contains("\"rules\""));
    }

    #[cfg(feature = "native")]
    #[test]
    fn emits_commonjs_helper_grammar_with_boa() {
        let temp = tempfile::tempdir().unwrap();
        let grammar = temp.path().join("grammar.js");
        let helper_dir = temp.path().join("grammar");
        fs::create_dir(&helper_dir).unwrap();
        fs::write(
            &grammar,
            r#"
const helper = require("./grammar/helper");
module.exports = grammar({
  name: "mini_commonjs",
  rules: {
    source_file: $ => helper.wrap($.item),
    item: $ => helper.item,
  },
});
"#,
        )
        .unwrap();
        fs::write(
            helper_dir.join("helper.js"),
            r#"
module.exports = {
  item: /[a-z]+/,
  wrap: rule => repeat1(rule),
};
"#,
        )
        .unwrap();

        let json = emit_with_boa(&grammar).unwrap();

        assert!(json.contains("\"name\": \"mini_commonjs\""));
        assert!(json.contains("\"source_file\""));
        assert!(json.contains("\"REPEAT1\""));
    }

    #[cfg(feature = "native")]
    #[test]
    fn emits_esm_helper_grammar_with_boa() {
        let temp = tempfile::tempdir().unwrap();
        let grammar = temp.path().join("grammar.js");
        let helper_dir = temp.path().join("grammar");
        fs::create_dir(&helper_dir).unwrap();
        fs::write(
            &grammar,
            r#"
import words from "./grammar/words.js"
import { wrap as one_or_more } from "./grammar/helpers.js";

export default grammar({
  name: "mini_esm",
  rules: {
    source_file: $ => one_or_more($.item),
    item: $ => words.item,
  },
});
"#,
        )
        .unwrap();
        fs::write(
            helper_dir.join("words.js"),
            r#"
export default {
  item: /[a-z]+/,
};
"#,
        )
        .unwrap();
        fs::write(
            helper_dir.join("helpers.js"),
            r#"
export const wrap = rule => repeat1(rule);
"#,
        )
        .unwrap();

        let json = emit_with_boa(&grammar).unwrap();

        assert!(json.contains("\"name\": \"mini_esm\""));
        assert!(json.contains("\"source_file\""));
        assert!(json.contains("\"REPEAT1\""));
    }

    #[cfg(feature = "native")]
    #[test]
    fn emits_arborium_style_sibling_mjs_helper_with_boa() {
        let temp = tempfile::tempdir().unwrap();
        let bundle_root = temp.path().join("def");
        let grammar_dir = bundle_root.join("grammar");
        let common_dir = bundle_root.join("common");
        fs::create_dir_all(&grammar_dir).unwrap();
        fs::create_dir_all(&common_dir).unwrap();
        let grammar = grammar_dir.join("grammar.js");
        fs::write(
            &grammar,
            r#"
import * as c from "../common/common.mjs";

export default grammar({
  name: "mini_arborium",
  rules: {
    source_file: $ => c.wrap($.item),
    item: $ => c.item,
  },
});
"#,
        )
        .unwrap();
        fs::write(
            common_dir.join("common.mjs"),
            r#"
export const item = /[a-z]+/;
export function wrap(rule) {
  return repeat1(rule);
}
"#,
        )
        .unwrap();

        let json = emit_with_boa(&grammar).unwrap();

        assert!(json.contains("\"name\": \"mini_arborium\""));
        assert!(json.contains("\"source_file\""));
        assert!(json.contains("\"REPEAT1\""));
    }

    #[cfg(feature = "native")]
    #[test]
    fn emits_rehomed_arborium_common_helper_with_boa() {
        let temp = tempfile::tempdir().unwrap();
        let bundle_root = temp.path().join("def");
        let grammar_dir = bundle_root.join("grammar");
        let common_dir = grammar_dir.join("common");
        fs::create_dir_all(&common_dir).unwrap();
        let grammar = grammar_dir.join("grammar.js");
        fs::write(
            &grammar,
            r#"
const common = require("../common/common");

module.exports = grammar({
  name: "mini_rehomed",
  rules: {
    source_file: $ => common.wrap($.item),
    item: $ => common.item,
  },
});
"#,
        )
        .unwrap();
        fs::write(
            common_dir.join("common.js"),
            r#"
const data = require("./data.json");

module.exports = {
  item: /[a-z]+/,
  wrap: rule => process.env.SNARK_DSL_TEST ? rule : data.repeat ? repeat1(rule) : rule,
};
"#,
        )
        .unwrap();
        fs::write(common_dir.join("data.json"), r#"{ "repeat": true }"#).unwrap();

        let json = emit_with_boa(&grammar).unwrap();

        assert!(json.contains("\"name\": \"mini_rehomed\""));
        assert!(json.contains("\"source_file\""));
        assert!(json.contains("\"REPEAT1\""));
    }

    #[cfg(feature = "native")]
    #[test]
    fn resolves_bundled_tree_sitter_package_grammar_with_boa() {
        let temp = tempfile::tempdir().unwrap();
        let bundle_root = temp.path().join("def");
        let grammar_dir = bundle_root.join("grammar");
        let dependency_dir = bundle_root.join("node_modules/tree-sitter-base");
        fs::create_dir_all(&grammar_dir).unwrap();
        fs::create_dir_all(&dependency_dir).unwrap();
        let grammar = grammar_dir.join("grammar.js");
        fs::write(
            &grammar,
            r#"
const base = require("tree-sitter-base/grammar");

module.exports = grammar(base, {
  name: "mini_inherited",
  rules: {
    source_file: $ => seq($.base_item, $.item),
    item: $ => /b+/,
  },
});
"#,
        )
        .unwrap();
        fs::write(
            dependency_dir.join("grammar.js"),
            r#"
module.exports = grammar({
  name: "base",
  rules: {
    source_file: $ => $.base_item,
    base_item: $ => /a+/,
  },
});
"#,
        )
        .unwrap();

        let json = emit_with_boa(&grammar).unwrap();

        assert!(json.contains("\"name\": \"mini_inherited\""));
        assert!(json.contains("\"base_item\""));
        assert!(json.contains("\"item\""));
    }

    #[cfg(feature = "native")]
    #[test]
    fn resolves_arborium_sibling_tree_sitter_package_grammar_with_boa() {
        let temp = tempfile::tempdir().unwrap();
        let arborium = temp.path().join("arborium");
        let base_dir = arborium.join("langs/group-birch/c/def/grammar");
        let derived_dir = arborium.join("langs/group-birch/cpp/def/grammar");
        fs::create_dir_all(&base_dir).unwrap();
        fs::create_dir_all(&derived_dir).unwrap();
        fs::write(
            base_dir.join("grammar.js"),
            r#"
module.exports = grammar({
  name: "c",
  rules: {
    source_file: $ => $.base_item,
    base_item: $ => /a+/,
  },
});
"#,
        )
        .unwrap();
        let grammar = derived_dir.join("grammar.js");
        fs::write(
            &grammar,
            r#"
const base = require("tree-sitter-c/grammar");

module.exports = grammar(base, {
  name: "cpp",
  rules: {
    source_file: $ => seq($.base_item, $.item),
    item: $ => /b+/,
  },
});
"#,
        )
        .unwrap();

        let json = emit_with_boa(&grammar).unwrap();

        assert!(json.contains("\"name\": \"cpp\""));
        assert!(json.contains("\"base_item\""));
        assert!(json.contains("\"item\""));
    }

    // Vendored fixture, not `DEFAULT_LUA_GRAMMAR` (that's a CLI convenience
    // default pointing at a local arborium checkout, not available in CI).
    const LUA_GRAMMAR_FIXTURE: &str =
        concat!(env!("CARGO_MANIFEST_DIR"), "/tests/fixtures/lua/grammar.js");
    const LUA_GRAMMAR_ORACLE_JSON: &str =
        include_str!("../tests/fixtures/lua/grammar.tree-sitter.json");

    #[cfg(feature = "native")]
    #[test]
    fn emits_lua_grammar_with_boa() {
        let json = emit_with_boa(std::path::Path::new(LUA_GRAMMAR_FIXTURE)).unwrap();

        assert!(json.contains("\"name\": \"lua\""));
        assert!(json.contains("\"chunk\""));
        assert!(json.contains("\"IMMEDIATE_TOKEN\""));
    }

    // Compares against a committed tree-sitter oracle snapshot rather than
    // shelling out to the `tree-sitter` CLI, which CI does not provision.
    // Regenerate with `snark-dsl/scripts/regenerate-lua-fixture.sh` after
    // changing the fixture grammar or upgrading tree-sitter-cli.
    #[cfg(feature = "native")]
    #[test]
    fn boa_lua_output_matches_tree_sitter_oracle() {
        let boa_json = emit_with_boa(std::path::Path::new(LUA_GRAMMAR_FIXTURE)).unwrap();
        assert_eq!(boa_json.trim(), LUA_GRAMMAR_ORACLE_JSON.trim());
    }

    // Exercises the real tree-sitter CLI against the fixture to catch drift
    // against the committed oracle snapshot. Requires `tree-sitter` on PATH,
    // so it's opt-in (`cargo test -- --ignored`) rather than part of the
    // default CI run.
    #[cfg(feature = "native")]
    #[test]
    #[ignore = "requires the tree-sitter CLI on PATH"]
    fn boa_lua_output_matches_live_tree_sitter_oracle() {
        check_against_tree_sitter(std::path::Path::new(LUA_GRAMMAR_FIXTURE)).unwrap();
    }

    #[test]
    fn uses_official_tree_sitter_dsl_runtime() {
        let prelude = official_tree_sitter_dsl_prelude().unwrap();

        assert!(prelude.contains("function grammar(baseGrammar, options)"));
        assert!(prelude.contains("globalThis.grammar = grammar;"));
        assert!(!prelude.contains(OFFICIAL_ENTRYPOINT_MARKER));
    }
}
