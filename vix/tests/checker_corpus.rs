use facet::Facet;
use std::fs;
use std::path::{Path, PathBuf};
use vix::machine::{Machine, MachineArg, NamedArg, RenderedValue};

#[derive(Facet, Debug)]
struct CorpusEntry {
    schema_version: u32,
    id: String,
    outcome: String,
    description: String,
    sources: Vec<CorpusSource>,
    assertions: Vec<CorpusAssertion>,
}

#[derive(Facet, Debug)]
struct CorpusSource {
    path: String,
    module: String,
}

#[derive(Facet, Debug)]
struct CorpusAssertion {
    query: String,
    subject: String,
    expect: String,
    span_start: u32,
    span_end: u32,
}

fn corpus_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/checker-corpus")
}

fn corpus_json_files() -> Vec<PathBuf> {
    let mut files = Vec::new();
    for dir in ["accept", "reject"] {
        let dir = corpus_root().join(dir);
        for entry in fs::read_dir(&dir).unwrap_or_else(|err| panic!("read {dir:?}: {err}")) {
            let path = entry.expect("directory entry").path();
            if path.extension().is_some_and(|ext| ext == "json") {
                files.push(path);
            }
        }
    }
    files.sort();
    files
}

fn load_entry(path: &Path) -> CorpusEntry {
    let text = fs::read_to_string(path).unwrap_or_else(|err| panic!("read {path:?}: {err}"));
    facet_json::from_str(&text).unwrap_or_else(|err| panic!("parse {path:?}: {err}"))
}

#[test]
fn checker_corpus_entries_are_real_facet_json() {
    let files = corpus_json_files();
    assert!(!files.is_empty(), "checker corpus is not empty");

    let root = corpus_root();
    for file in files {
        let entry = load_entry(&file);
        assert_eq!(entry.schema_version, 1, "{file:?}");
        assert!(
            matches!(entry.outcome.as_str(), "accept" | "reject"),
            "{file:?}"
        );
        assert!(!entry.id.is_empty(), "{file:?}");
        assert!(!entry.description.is_empty(), "{file:?}");
        assert!(!entry.sources.is_empty(), "{file:?}");
        assert!(!entry.assertions.is_empty(), "{file:?}");

        for source in &entry.sources {
            assert!(!source.module.is_empty(), "{file:?}");
            let source_path = root.join(&source.path);
            assert!(
                source_path.exists(),
                "{file:?} references missing {source_path:?}"
            );
            assert!(
                source_path.extension().is_some_and(|ext| ext == "vix"),
                "{file:?} source is not .vix: {source_path:?}"
            );
        }

        for assertion in &entry.assertions {
            assert!(!assertion.query.is_empty(), "{file:?}");
            assert!(!assertion.subject.is_empty(), "{file:?}");
            assert!(!assertion.expect.is_empty(), "{file:?}");
            assert!(
                assertion.span_start <= assertion.span_end,
                "{file:?} invalid span in {assertion:?}"
            );
        }
    }
}

#[test]
fn checker_corpus_matches_vix_checker() {
    let files = corpus_json_files();
    let mut covered = Vec::new();
    let mut pending = Vec::new();

    for file in files {
        let entry = load_entry(&file);
        for assertion in &entry.assertions {
            let key = format!("{}:{}:{}", entry.id, assertion.query, assertion.subject);
            if run_covered_assertion(&entry, assertion)
                .unwrap_or_else(|err| panic!("{file:?} {key}: {err}"))
            {
                covered.push(key);
            } else {
                pending.push(key);
            }
        }
    }
    covered.sort();
    pending.sort();

    assert_eq!(
        covered,
        vec![
            "accept.cargo-manifest:function_signature:cargo_manifest",
            "accept.crate-build:function_signature:crate_lib",
            "accept.crate-build:function_signature:doc_string",
            "accept.elf-policy:function_signature:glibc_policy",
            "accept.eval-self-hosting:function_signature:demo",
            "accept.eval-self-hosting:function_signature:eval",
            "accept.lua-build:function_signature:lua",
            "accept.lua-build:function_signature:object",
            "accept.lua-build:function_signature:sources",
            "accept.merge-demand:function_signature:fallback",
            "accept.merge-demand:function_signature:object",
            "accept.merge-demand:function_signature:selected",
            "accept.merge-demand:function_signature:subtree_chain",
            "accept.types-tour:function_signature:classify",
            "accept.types-tour:function_signature:scaled",
            "accept.types-tour:function_signature:toolchain",
            "reject.omitted-return-type:diagnostics:OmittedReturnType",
        ],
        "covered checker v0 assertions stay explicit"
    );

    assert_eq!(
        pending,
        vec![
            "accept.cargo-manifest:type_of:toml(manifest/p\"Cargo.toml\")",
            "accept.crate-build:abi_class:rustc! block",
            "accept.elf-policy:type_of:elf(input).needs_glibc",
            "accept.eval-self-hosting:abi_class:Expr",
            "accept.eval-self-hosting:decls:crate",
            "accept.eval-self-hosting:exhaustive:eval.match",
            "accept.lua-build:abi_class:cc! blocks",
            "accept.lua-build:exhaustive:lua.target.os.match",
            "accept.lua-build:module_graph:crate.imports",
            "accept.lua-build:type_of:lua.units",
            "accept.merge-demand:type_of:units.map(|u| object(cc,u)).collect()/p\"wanted.o\"",
            "accept.types-tour:abi_class:Toolchain",
            "accept.types-tour:decls:crate",
            "accept.types-tour:exhaustive:classify.match",
            "accept.types-tour:exhaustive:toolchain.target.os.match",
            "accept.types-tour:function_signature:apply",
            "accept.types-tour:function_signature:depths",
            "accept.types-tour:function_signature:partials",
            "accept.types-tour:function_signature:swap",
            "reject.duplicate-argument:diagnostics:DuplicateArgument",
            "reject.duplicate-field:diagnostics:DuplicateField",
            "reject.duplicate-item:diagnostics:DuplicateItem",
            "reject.duplicate-partial-marker:diagnostics:DuplicatePartialMarker",
            "reject.duplicate-pattern-field:diagnostics:DuplicatePatternField",
            "reject.duplicate-struct-field:diagnostics:DuplicateStructField",
            "reject.duplicate-type-param:diagnostics:DuplicateTypeParam",
            "reject.duplicate-type:diagnostics:DuplicateType",
            "reject.duplicate-variant:diagnostics:DuplicateVariant",
            "reject.generic-lowering-unsupported:lowering_plan:GenericLoweringUnsupported",
            "reject.guarded-arm-does-not-cover:diagnostics:GuardedArmDoesNotCover",
            "reject.irrefutable-arm-not-last:diagnostics:IrrefutableArmNotLast",
            "reject.missing-argument:diagnostics:MissingArgument",
            "reject.missing-struct-field:diagnostics:MissingStructField",
            "reject.non-bool-guard:diagnostics:NonBoolGuard",
            "reject.non-exhaustive-match:diagnostics:NonExhaustiveMatch",
            "reject.partial-call-non-prefix:diagnostics:PartialCallNonPrefix",
            "reject.pattern-payload-mismatch:diagnostics:PatternPayloadMismatch",
            "reject.string-is-not-path:diagnostics:StringIsNotPath",
            "reject.too-many-arguments:diagnostics:TooManyArguments",
            "reject.type-mismatch:diagnostics:TypeMismatch",
            "reject.unknown-argument:diagnostics:UnknownArgument",
            "reject.unknown-struct-field:diagnostics:UnknownStructField",
            "reject.unresolved-module:diagnostics:UnresolvedModule",
            "reject.unresolved-type:diagnostics:UnresolvedType",
            "reject.unresolved-value:diagnostics:UnresolvedValue",
        ],
        "checker v0 pending assertions stay explicit"
    );
}

fn run_covered_assertion(entry: &CorpusEntry, assertion: &CorpusAssertion) -> Result<bool, String> {
    match (entry.outcome.as_str(), assertion.query.as_str()) {
        ("accept", "function_signature") if plain_signature_assertion(assertion) => {
            let source = entry_source(entry)?;
            let actual = checker_string(
                "function_signature",
                &[
                    ("source", MachineArg::String(source)),
                    ("name", MachineArg::String(assertion.subject.clone())),
                ],
            )?;
            if actual != assertion.expect {
                return Err(format!(
                    "function_signature({}) = {actual:?}, expected {:?}",
                    assertion.subject, assertion.expect
                ));
            }
            Ok(true)
        }
        ("reject", "diagnostics") if assertion.subject == "OmittedReturnType" => {
            let source = entry_source(entry)?;
            let function = "main".to_string();
            let kind = checker_string(
                "diagnostic_kind",
                &[
                    ("source", MachineArg::String(source.clone())),
                    ("function", MachineArg::String(function.clone())),
                ],
            )?;
            let suggestion = checker_string(
                "diagnostic_suggestion",
                &[
                    ("source", MachineArg::String(source.clone())),
                    ("function", MachineArg::String(function.clone())),
                ],
            )?;
            let span_start = checker_int(
                "diagnostic_span_start",
                &[
                    ("source", MachineArg::String(source.clone())),
                    ("function", MachineArg::String(function.clone())),
                ],
            )?;
            let span_end = checker_int(
                "diagnostic_span_end",
                &[
                    ("source", MachineArg::String(source)),
                    ("function", MachineArg::String(function)),
                ],
            )?;
            if kind != assertion.subject {
                return Err(format!("diagnostic kind = {kind:?}"));
            }
            if suggestion != "Int" {
                return Err(format!("diagnostic suggestion = {suggestion:?}"));
            }
            if (span_start, span_end)
                != (
                    i64::from(assertion.span_start),
                    i64::from(assertion.span_end),
                )
            {
                return Err(format!(
                    "diagnostic span = {span_start}..{span_end}, expected {}..{}",
                    assertion.span_start, assertion.span_end
                ));
            }
            Ok(true)
        }
        _ => Ok(false),
    }
}

fn plain_signature_assertion(assertion: &CorpusAssertion) -> bool {
    assertion.expect.contains(" abi=")
        && !assertion.expect.contains(" checked ")
        && !assertion.expect.contains(" callable_param=")
        && !assertion.expect.contains(" partial=")
        && !assertion.expect.contains(" tuple_index=")
}

fn entry_source(entry: &CorpusEntry) -> Result<String, String> {
    let [source] = entry.sources.as_slice() else {
        return Err(format!(
            "checker v0 expects one source, got {}",
            entry.sources.len()
        ));
    };
    let path = corpus_root().join(&source.path);
    fs::read_to_string(&path).map_err(|err| format!("read {path:?}: {err}"))
}

fn checker_string(function: &str, args: &[(&str, MachineArg)]) -> Result<String, String> {
    let rendered = checker_rendered(function, args)?;
    let RenderedValue::String { value } = rendered else {
        return Err(format!("{function} returned {rendered:?}, not String"));
    };
    Ok(value)
}

fn checker_int(function: &str, args: &[(&str, MachineArg)]) -> Result<i64, String> {
    let rendered = checker_rendered(function, args)?;
    let RenderedValue::Int { value } = rendered else {
        return Err(format!("{function} returned {rendered:?}, not Int"));
    };
    Ok(value)
}

fn checker_rendered(function: &str, args: &[(&str, MachineArg)]) -> Result<RenderedValue, String> {
    let mut machine = Machine::load(include_str!("../checker/checker.vix"))?;
    let args = args
        .iter()
        .map(|(name, value)| NamedArg {
            name: (*name).to_string(),
            value: value.clone(),
        })
        .collect::<Vec<_>>();
    let handle = machine.call(function, &args)?;
    machine.render_result(function, handle.0)
}
