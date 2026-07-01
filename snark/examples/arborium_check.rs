//! Pre-flight every *scanner-free* arborium grammar through snark: emit grammar.js ->
//! grammar.json, build the parse table, and (if a sample exists) parse it. Reports which
//! grammars are usable in snark today, so we know what to vendor into the playground.
//!
//! Usage: cargo run -p snark --features json-import,weavy-lowering --example arborium_check -- [LANGS_DIR]
//! Default LANGS_DIR = ~/oss/arborium/langs

use std::{
    panic::{self, AssertUnwindSafe},
    path::{Path, PathBuf},
};

use snark::{
    grammar::RawGrammarJson,
    lexical::LexicalFacts,
    lower::weavy::{WeavyParsePlan, parse_prepared_weavy_recovering_with_report_and_scanner},
    parser::{ParseTable, ParserGrammar, TreeEvent},
    validated::ValidatedGrammar,
};

fn main() {
    let langs_dir = std::env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join("oss/arborium/langs"));
    // Skip grammars whose grammar.js exceeds this many bytes (large grammars build
    // slowly under the current table builder). Default: no cap.
    let max_bytes: u64 = std::env::args()
        .nth(2)
        .and_then(|s| s.parse().ok())
        .unwrap_or(u64::MAX);

    let mut defs = Vec::new();
    collect_defs(&langs_dir, &mut defs);
    defs.sort();

    // Silence panic backtraces; we report failures ourselves.
    panic::set_hook(Box::new(|_| {}));

    let (mut ok, mut emit_fail, mut build_fail, mut parse_dirty) = (0, 0, 0, 0);
    let mut usable: Vec<String> = Vec::new();

    for def in &defs {
        let lang = lang_name(def);
        let grammar_js = def.join("grammar/grammar.js");
        if has_scanner(def) {
            continue;
        }
        if std::fs::metadata(&grammar_js).map(|m| m.len()).unwrap_or(0) > max_bytes {
            continue;
        }

        let outcome = panic::catch_unwind(AssertUnwindSafe(|| check_one(def, &grammar_js)));
        match outcome {
            Ok(Ok(CheckOk { sample_errors })) => {
                ok += 1;
                usable.push(lang.clone());
                let tail = match sample_errors {
                    Some(0) => "sample: clean".to_string(),
                    Some(n) => {
                        parse_dirty += 1;
                        format!("sample: {n} errors")
                    }
                    None => "(no sample)".to_string(),
                };
                println!("OK    {lang:<22} {tail}");
            }
            Ok(Err(CheckErr::Emit(e))) => {
                emit_fail += 1;
                println!("EMIT  {lang:<22} {}", first_line(&e));
            }
            Ok(Err(CheckErr::Build(e))) => {
                build_fail += 1;
                println!("BUILD {lang:<22} {}", first_line(&e));
            }
            Err(_) => {
                build_fail += 1;
                println!("PANIC {lang:<22} (panicked during build/parse)");
            }
        }
    }

    let _ = panic::take_hook();
    println!("\n==== summary ====");
    println!(
        "scanner-free defs: {}",
        usable.len() + emit_fail + build_fail
    );
    println!("usable in snark:   {ok}  ({parse_dirty} parsed a sample with errors)");
    println!("emit failures:     {emit_fail}");
    println!("build failures:    {build_fail}");
    println!("\nusable: {}", usable.join(" "));
}

struct CheckOk {
    sample_errors: Option<usize>,
}

enum CheckErr {
    Emit(String),
    Build(String),
}

fn check_one(def: &Path, grammar_js: &Path) -> Result<CheckOk, CheckErr> {
    let grammar_json =
        snark_dsl::emit_with_boa(grammar_js).map_err(|e| CheckErr::Emit(e.to_string()))?;
    let raw = RawGrammarJson::from_tree_sitter_json_str(&grammar_json)
        .map_err(|e| CheckErr::Build(e.to_string()))?;
    let validated = ValidatedGrammar::from_raw(&raw).map_err(|e| CheckErr::Build(e.to_string()))?;
    let lexical = LexicalFacts::from_grammar(&validated);
    let parser = ParserGrammar::normalize_from_validated(&validated, &lexical)
        .map_err(|e| CheckErr::Build(e.to_string()))?
        .prepare_productions_for_items()
        .map_err(|e| CheckErr::Build(e.to_string()))?;
    let table = ParseTable::from_grammar(&parser).map_err(|e| CheckErr::Build(e.to_string()))?;
    let plan = WeavyParsePlan::new(&validated, &parser, &table)
        .map_err(|e| CheckErr::Build(e.to_string()))?;

    let sample_errors = first_sample(def).and_then(|sample| {
        let input = std::fs::read_to_string(&sample).ok()?;
        let report = parse_prepared_weavy_recovering_with_report_and_scanner(
            &plan, &validated, &parser, &table, &input, None,
        )
        .ok()?;
        Some(
            report
                .accepted_tree_events()
                .iter()
                .filter(|e| matches!(e, TreeEvent::Error { .. } | TreeEvent::Missing { .. }))
                .count(),
        )
    });

    Ok(CheckOk { sample_errors })
}

fn collect_defs(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if path.join("grammar/grammar.js").is_file()
                && path.file_name().is_some_and(|n| n == "def")
            {
                out.push(path);
            } else {
                collect_defs(&path, out);
            }
        }
    }
}

fn has_scanner(def: &Path) -> bool {
    def.join("grammar/scanner.c").is_file() || def.join("grammar/scanner.cc").is_file()
}

fn first_sample(def: &Path) -> Option<PathBuf> {
    let samples = def.join("samples");
    let mut entries: Vec<PathBuf> = std::fs::read_dir(&samples)
        .ok()?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_file())
        .collect();
    entries.sort();
    entries.into_iter().next()
}

fn lang_name(def: &Path) -> String {
    def.parent()
        .and_then(|p| p.file_name())
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_else(|| def.display().to_string())
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").chars().take(80).collect()
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default()
}
