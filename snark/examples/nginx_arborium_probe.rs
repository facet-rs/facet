use std::{env, path::PathBuf, time::Instant};

use snark::{
    grammar::RawGrammarJson,
    lexical::LexicalFacts,
    parser::{ParseTable, ParserGrammar, RuntimeParser, TreeEvent},
    validated::ValidatedGrammar,
};

fn main() {
    let def = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/Users/amos/oss/arborium/langs/group-maple/nginx/def"));
    let grammar_js = def.join("grammar/grammar.js");
    let sample = def.join("samples/nginx.conf");

    let start = Instant::now();
    let grammar_json =
        snark_dsl::emit_with_boa(&grammar_js).expect("grammar.js should emit grammar JSON");
    let emitted_at = Instant::now();

    let raw = RawGrammarJson::from_tree_sitter_json_str(&grammar_json)
        .expect("emitted grammar JSON should import");
    let validated = ValidatedGrammar::from_raw(&raw).expect("grammar should validate");
    let lexical = LexicalFacts::from_grammar(&validated);
    let parser = ParserGrammar::normalize_from_validated(&validated, &lexical)
        .expect("grammar should normalize")
        .prepare_productions_for_items()
        .expect("productions should prepare");
    let table = ParseTable::from_grammar(&parser).expect("parse table should build");
    let prepared_at = Instant::now();

    let input = std::fs::read_to_string(&sample).expect("sample should be readable");
    let report = RuntimeParser::new(&validated, &parser, &table)
        .expect("runtime should build")
        .parse_recovering_with_report(&input)
        .expect("recovering parse should return a report");
    let parsed_at = Instant::now();

    let accepted_events = report.accepted_tree_events();
    let error_count = accepted_events
        .iter()
        .filter(|event| matches!(event, TreeEvent::Error { .. }))
        .count();
    let missing_count = accepted_events
        .iter()
        .filter(|event| matches!(event, TreeEvent::Missing { .. }))
        .count();
    let sexp = report.tree().to_sexp();
    let sexp_at = Instant::now();

    println!("language: {}", raw.name);
    println!("input bytes: {}", input.len());
    println!("accepted branches: {}", report.accepted_count());
    println!("failed branches: {}", report.failure_count());
    println!("max live versions: {}", report.max_live_versions());
    println!("trace events: {}", report.trace_events().len());
    println!("tree events: {}", report.tree_events().len());
    println!("accepted tree events: {}", accepted_events.len());
    println!("accepted ERROR nodes: {error_count}");
    println!("accepted MISSING nodes: {missing_count}");
    println!("sexp bytes: {}", sexp.len());
    println!("emit grammar.js: {:?}", emitted_at.duration_since(start));
    println!(
        "prepare parser: {:?}",
        prepared_at.duration_since(emitted_at)
    );
    println!(
        "recovering parse: {:?}",
        parsed_at.duration_since(prepared_at)
    );
    println!("sexp projection: {:?}", sexp_at.duration_since(parsed_at));
}
