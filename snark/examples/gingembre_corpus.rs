//! Parse the whole gingembre template corpus with a snark grammar and report which
//! templates parse cleanly (no Error / Missing tree events).
//!
//! Usage:
//!   cargo run -p snark --example gingembre_corpus -- [GRAMMAR_JS] [CORPUS_ROOT]
//!
//! Defaults:
//!   GRAMMAR_JS  = playgrounds/snark/src/bundled/gingembre/grammar.js (relative to repo)
//!   CORPUS_ROOT = ~/oss/dodeca
//!
//! A file is in the corpus if it ends in `.html` and contains a gingembre delimiter
//! (`{{`, `{%`, or `{#`).

use std::{env, path::PathBuf};

use snark::{
    grammar::RawGrammarJson,
    lexical::LexicalFacts,
    parser::{ParseTable, ParserGrammar, RuntimeParser, TreeEvent},
    validated::ValidatedGrammar,
};

fn main() {
    let grammar_js = env::args_os().nth(1).map(PathBuf::from).unwrap_or_else(|| {
        repo_root().join("playgrounds/snark/src/bundled/gingembre/grammar.js")
    });
    let corpus_root = env::args_os()
        .nth(2)
        .map(PathBuf::from)
        .unwrap_or_else(|| home().join("oss/dodeca"));

    // Build the parser once.
    let grammar_json =
        snark_dsl::emit_with_boa(&grammar_js).expect("grammar.js should emit grammar JSON");
    let raw = RawGrammarJson::from_tree_sitter_json_str(&grammar_json)
        .expect("emitted grammar JSON should import");
    let validated = ValidatedGrammar::from_raw(&raw).expect("grammar should validate");
    let lexical = LexicalFacts::from_grammar(&validated);
    let parser = ParserGrammar::normalize_from_validated(&validated, &lexical)
        .expect("grammar should normalize")
        .prepare_productions_for_items()
        .expect("productions should prepare");
    let t_table = std::time::Instant::now();
    let table = ParseTable::from_grammar(&parser).expect("parse table should build");
    let table_ms = t_table.elapsed().as_secs_f64() * 1000.0;

    // Time one runtime construction (compiles the lexer) and one parse in isolation.
    let probe_input = "alpha";
    let t_rt = std::time::Instant::now();
    let probe_rt = RuntimeParser::new(&validated, &parser, &table).expect("runtime");
    let runtime_new_ms = t_rt.elapsed().as_secs_f64() * 1000.0;
    let t_p = std::time::Instant::now();
    let _ = probe_rt.parse_recovering_with_report(probe_input);
    let parse_probe_ms = t_p.elapsed().as_secs_f64() * 1000.0;
    println!(
        "[timing] table_build={table_ms:.1}ms  RuntimeParser::new={runtime_new_ms:.1}ms  parse('alpha')={parse_probe_ms:.1}ms",
    );

    println!("language: {}", raw.name);
    println!("grammar:  {}", grammar_js.display());
    println!("corpus:   {}\n", corpus_root.display());

    let mut files = Vec::new();
    collect_templates(&corpus_root, &mut files);
    files.sort();

    let (mut clean, mut dirty) = (0usize, 0usize);
    let mut worst: Vec<(usize, usize, PathBuf)> = Vec::new();

    for path in &files {
        let input = std::fs::read_to_string(path).unwrap_or_default();
        let runtime = RuntimeParser::new(&validated, &parser, &table)
            .expect("runtime should build");
        let report = match runtime.parse_recovering_with_report(&input) {
            Ok(report) => report,
            Err(error) => {
                dirty += 1;
                println!("FAIL  {:<60} hard error: {error}", rel(path, &corpus_root));
                continue;
            }
        };
        let events = report.accepted_tree_events();
        let errors = events
            .iter()
            .filter(|e| matches!(e, TreeEvent::Error { .. }))
            .count();
        let missing = events
            .iter()
            .filter(|e| matches!(e, TreeEvent::Missing { .. }))
            .count();

        if errors == 0 && missing == 0 {
            clean += 1;
        } else {
            dirty += 1;
            worst.push((errors + missing, errors, path.clone()));
            let first = events.iter().find_map(|e| match e {
                TreeEvent::Error { bytes, .. } => Some(bytes.start().get()),
                _ => None,
            });
            println!(
                "DIRTY {:<60} errors={errors} missing={missing}{}",
                rel(path, &corpus_root),
                first.map(|b| format!(" first_error_byte={b}")).unwrap_or_default(),
            );
        }
    }

    worst.sort_by(|a, b| b.0.cmp(&a.0));
    println!("\n==== summary ====");
    println!("templates: {}", files.len());
    println!("clean:     {clean}");
    println!("dirty:     {dirty}");
    if !worst.is_empty() {
        println!("\nworst offenders:");
        for (total, errors, path) in worst.iter().take(10) {
            println!("  {total:>3} (err={errors}) {}", rel(path, &corpus_root));
        }
    }
}

fn collect_templates(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
        if path.is_dir() {
            // Skip deps, build output, and rendered site output (`public`/`dist` hold
            // generated HTML that only *shows* template syntax in prose).
            if matches!(name, "node_modules" | "target" | ".git" | "public" | "dist") {
                continue;
            }
            collect_templates(&path, out);
        } else if path.extension().is_some_and(|e| e == "html") {
            // Skip arborium headers and anything without a gingembre delimiter.
            if name == "arborium-header.html" {
                continue;
            }
            if let Ok(text) = std::fs::read_to_string(&path) {
                if text.contains("{{") || text.contains("{%") || text.contains("{#") {
                    out.push(path);
                }
            }
        }
    }
}

fn rel(path: &std::path::Path, root: &std::path::Path) -> String {
    path.strip_prefix(root).unwrap_or(path).display().to_string()
}

fn repo_root() -> PathBuf {
    // examples run with CWD = crate dir (snark/); repo root is its parent.
    env::var_os("CARGO_MANIFEST_DIR")
        .map(PathBuf::from)
        .and_then(|p| p.parent().map(PathBuf::from))
        .unwrap_or_else(|| PathBuf::from("."))
}

fn home() -> PathBuf {
    env::var_os("HOME").map(PathBuf::from).unwrap_or_default()
}
