use std::{env, path::PathBuf, time::Instant};

use snark::{
    grammar::RawGrammarJson,
    lexical::LexicalFacts,
    parser::{ParseAction, ParseTable, ParserGrammar, ParserSymbol, RuntimeParser, TreeEvent},
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
    let dump_errors = env::var_os("SNARK_NGINX_DUMP_ERRORS").is_some();
    if dump_errors {
        if let Err(error) = RuntimeParser::new(&validated, &parser, &table)
            .expect("runtime should build")
            .parse_with_report(&input)
        {
            println!("non-recovering parse error: {error}");
            for step in error.trace().iter().rev().take(24).rev() {
                let action = match step.action {
                    ParseAction::Shift { state, .. } => format!("shift {}", state.get()),
                    ParseAction::ShiftExtra => "shift-extra".to_owned(),
                    ParseAction::Reduce { production, .. } => {
                        format!("reduce {}", production.get())
                    }
                    ParseAction::Accept { .. } => "accept".to_owned(),
                    ParseAction::Recover => "recover".to_owned(),
                };
                println!(
                    "  trace state {} byte {} lookahead {:?} action {}",
                    step.state.get(),
                    step.byte_position,
                    step.lookahead,
                    action
                );
            }
        }
    }
    let parse_result = RuntimeParser::new(&validated, &parser, &table)
        .expect("runtime should build")
        .parse_compact_with_report(&input);
    let parsed_at = Instant::now();
    println!("language: {}", raw.name);
    println!("input bytes: {}", input.len());
    let report = match parse_result {
        Ok(report) => report,
        Err(error) => {
            println!("parse failed: {error}");
            match RuntimeParser::new(&validated, &parser, &table)
                .expect("runtime should build")
                .parse_recovering_compact_with_report(&input)
            {
                Ok(report) => {
                    println!(
                        "recovering parse accepted with {} ERROR and {} MISSING nodes",
                        report
                            .accepted_tree_events()
                            .iter()
                            .filter(|event| matches!(event, TreeEvent::Error { .. }))
                            .count(),
                        report
                            .accepted_tree_events()
                            .iter()
                            .filter(|event| matches!(event, TreeEvent::Missing { .. }))
                            .count()
                    );
                }
                Err(recovery_error) => {
                    println!("recovering parse failed: {recovery_error}");
                }
            }
            println!("emit grammar.js: {:?}", emitted_at.duration_since(start));
            println!(
                "prepare parser: {:?}",
                prepared_at.duration_since(emitted_at)
            );
            println!("strict parse: {:?}", parsed_at.duration_since(prepared_at));
            return;
        }
    };

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
    let first_error_byte = accepted_events
        .iter()
        .find_map(|event| match event {
            TreeEvent::Error { bytes, .. } => Some(bytes.start().get()),
            _ => None,
        })
        .unwrap_or(0);

    println!("accepted branches: {}", report.accepted_count());
    println!("failed branches: {}", report.failure_count());
    println!("max live versions: {}", report.max_live_versions());
    println!("trace events: {}", report.trace_events().len());
    println!("tree events: {}", report.tree_events().len());
    println!("accepted tree events: {}", accepted_events.len());
    println!("accepted ERROR nodes: {error_count}");
    println!("accepted MISSING nodes: {missing_count}");
    println!("sexp bytes: {}", sexp.len());
    if dump_errors {
        for event in accepted_events
            .iter()
            .filter(|event| matches!(event, TreeEvent::Error { .. } | TreeEvent::Missing { .. }))
        {
            match event {
                TreeEvent::Error {
                    bytes,
                    points,
                    error_cost,
                    ..
                } => {
                    let snippet = input
                        .get(bytes.start().get() as usize..bytes.end().get() as usize)
                        .unwrap_or("")
                        .replace('\n', "\\n");
                    println!(
                        "accepted ERROR row {} col {} bytes {}..{} cost {} snippet {:?}",
                        points.start().row().get() + 1,
                        points.start().column().get() + 1,
                        bytes.start().get(),
                        bytes.end().get(),
                        error_cost,
                        snippet
                    );
                }
                TreeEvent::Missing {
                    symbol,
                    bytes,
                    points,
                    ..
                } => {
                    println!(
                        "accepted MISSING {:?} row {} col {} byte {}",
                        symbol,
                        points.start().row().get() + 1,
                        points.start().column().get() + 1,
                        bytes.start().get()
                    );
                }
                _ => {}
            }
        }
    }
    if dump_errors && first_error_byte > 0 {
        let window_start = first_error_byte.saturating_sub(160);
        let window_end = first_error_byte.saturating_add(80);
        println!("accepted tokens around first ERROR:");
        for event in accepted_events.iter().filter_map(|event| match event {
            TreeEvent::Token {
                symbol,
                bytes,
                points,
                ..
            } if bytes.end().get() >= window_start && bytes.start().get() <= window_end => {
                Some((symbol, bytes, points))
            }
            _ => None,
        }) {
            let (symbol, bytes, points) = event;
            let name = match symbol {
                ParserSymbol::Terminal(terminal) => parser.symbols().terminals()
                    [terminal.get() as usize]
                    .spelling()
                    .to_owned(),
                ParserSymbol::External(external) => parser.symbols().externals()
                    [external.get() as usize]
                    .name()
                    .unwrap_or("<external>")
                    .to_owned(),
                ParserSymbol::Eof => "<eof>".to_owned(),
                ParserSymbol::Nonterminal(nonterminal) => format!("nonterminal#{nonterminal:?}"),
                ParserSymbol::Internal(internal) => format!("internal#{internal:?}"),
                _ => "<unknown>".to_owned(),
            };
            let snippet = input
                .get(bytes.start().get() as usize..bytes.end().get() as usize)
                .unwrap_or("")
                .replace('\n', "\\n");
            println!(
                "  row {} col {} bytes {}..{} token {:?} text {:?}",
                points.start().row().get() + 1,
                points.start().column().get() + 1,
                bytes.start().get(),
                bytes.end().get(),
                name,
                snippet
            );
        }
    }
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
