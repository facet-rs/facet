#[cfg(feature = "native")]
use std::{ffi::OsStr, fs, path::PathBuf, process::Command};

#[cfg(feature = "boa")]
use boa_engine::{Context, JsValue, Source};

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
    #[error("failed to run tree-sitter: {0}")]
    TreeSitterIo(#[from] std::io::Error),
    #[cfg(feature = "native")]
    #[error("tree-sitter generate failed with status {status}: {stderr}")]
    TreeSitterFailed { status: String, stderr: String },
    #[cfg(feature = "boa")]
    #[error("Boa failed while evaluating {path}: {message}")]
    Boa { path: String, message: String },
    #[cfg(feature = "boa")]
    #[error("expected JavaScript string from {operation}, got {value}")]
    ExpectedString {
        operation: &'static str,
        value: String,
    },
    #[cfg(feature = "boa")]
    #[error("failed to convert JavaScript string from {operation}: {message}")]
    InvalidJsString {
        operation: &'static str,
        message: String,
    },
    #[cfg(feature = "native")]
    #[error("Boa output differs from tree-sitter output")]
    Mismatch,
    #[error("official tree-sitter DSL entrypoint marker was not found")]
    OfficialDslMarkerMissing,
    #[error("{0}")]
    Usage(String),
}

pub fn official_tree_sitter_dsl_source() -> &'static str {
    OFFICIAL_TREE_SITTER_DSL
}

#[cfg(feature = "boa")]
pub fn emit_source_with_boa(grammar_source: &str, source_name: &str) -> Result<String> {
    let mut context = Context::default();

    eval(
        &mut context,
        official_tree_sitter_dsl_prelude()?,
        "vendor/tree-sitter-generate-0.26.9/dsl.js",
    )?;
    eval(
        &mut context,
        "globalThis.module = { exports: {} };\nglobalThis.exports = globalThis.module.exports;",
        "commonjs-shim.js",
    )?;
    eval(&mut context, grammar_source, source_name)?;
    eval_to_string(&mut context, EMIT_SCRIPT, "emit.js", "emit")
}

#[cfg(feature = "native")]
pub fn emit_with_boa(grammar_path: &std::path::Path) -> Result<String> {
    let grammar_source = read_to_string(grammar_path)?;
    emit_source_with_boa(&grammar_source, &grammar_path.display().to_string())
}

#[cfg(feature = "native")]
pub fn emit_with_tree_sitter(grammar_path: &std::path::Path) -> Result<String> {
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
pub fn check_against_tree_sitter(grammar_path: &std::path::Path) -> Result<()> {
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
fn read_to_string(path: &std::path::Path) -> Result<String> {
    fs::read_to_string(path).map_err(|source| Error::Read {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(feature = "native")]
pub fn write_string(path: &std::path::Path, contents: &str) -> Result<()> {
    fs::write(path, contents).map_err(|source| Error::Write {
        path: path.to_path_buf(),
        source,
    })
}

#[cfg(feature = "boa")]
fn eval(context: &mut Context, source: &str, path: &str) -> Result<JsValue> {
    context
        .eval(Source::from_bytes(source).with_path(std::path::Path::new(path)))
        .map_err(|err| Error::Boa {
            path: path.to_string(),
            message: err.to_string(),
        })
}

#[cfg(feature = "boa")]
fn eval_to_string(
    context: &mut Context,
    source: &str,
    path: &str,
    operation: &'static str,
) -> Result<String> {
    let value = eval(context, source, path)?;
    js_value_to_string(value, context, operation)
}

#[cfg(feature = "boa")]
fn js_value_to_string(
    value: JsValue,
    context: &mut Context,
    operation: &'static str,
) -> Result<String> {
    let value_display = value.display().to_string();
    let js_string = value.to_string(context).map_err(|err| Error::Boa {
        path: operation.to_string(),
        message: err.to_string(),
    })?;

    if value.is_string() {
        js_string
            .to_std_string()
            .map_err(|err| Error::InvalidJsString {
                operation,
                message: err.to_string(),
            })
    } else {
        Err(Error::ExpectedString {
            operation,
            value: value_display,
        })
    }
}

#[cfg(feature = "native")]
pub fn grammar_arg(arg: Option<&OsStr>) -> PathBuf {
    arg.map_or_else(|| PathBuf::from(DEFAULT_LUA_GRAMMAR), PathBuf::from)
}

#[cfg(feature = "boa")]
const EMIT_SCRIPT: &str = r#"
const grammarObj = module.exports && module.exports.grammar
  ? module.exports.grammar
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

    #[cfg(feature = "boa")]
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
    fn emits_lua_grammar_with_boa() {
        let json = emit_with_boa(std::path::Path::new(DEFAULT_LUA_GRAMMAR)).unwrap();

        assert!(json.contains("\"name\": \"lua\""));
        assert!(json.contains("\"chunk\""));
        assert!(json.contains("\"IMMEDIATE_TOKEN\""));
    }

    #[cfg(feature = "native")]
    #[test]
    fn boa_lua_output_matches_tree_sitter_oracle() {
        check_against_tree_sitter(std::path::Path::new(DEFAULT_LUA_GRAMMAR)).unwrap();
    }

    #[test]
    fn uses_official_tree_sitter_dsl_runtime() {
        let prelude = official_tree_sitter_dsl_prelude().unwrap();

        assert!(prelude.contains("function grammar(baseGrammar, options)"));
        assert!(prelude.contains("globalThis.grammar = grammar;"));
        assert!(!prelude.contains(OFFICIAL_ENTRYPOINT_MARKER));
    }
}
