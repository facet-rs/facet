//! Differential oracle: for each standard tree-sitter grammar + input, snark's
//! parse tree must match REAL tree-sitter's. tree-sitter is the reference
//! implementation snark reimplements, so it is the correct oracle — a self-check
//! (snark vs a hand-built snark slice) can only catch snark disagreeing with
//! itself, never snark disagreeing with tree-sitter, which is the disagreement
//! that matters.
//!
//! Scope: grammars expressible in *both* (standard tree-sitter DSL — no snark
//! extensions like `until`/`nested`/`auto_close`, which tree-sitter can't
//! express and therefore can't be an oracle for). Those extensions need a
//! different oracle (e.g. the gingembre render oracle); this covers the
//! tree-sitter-compatible core, which is exactly where snark MUST match.
//!
//! Skips cleanly if the `tree-sitter` CLI isn't installed.

use std::path::{Path, PathBuf};
use std::process::Command;
use std::{env, fs};

use snark::{
    grammar::RawGrammarJson,
    lexical::LexicalFacts,
    lower::weavy::{
        WeavyParsePlan, parse_prepared_weavy_recovering_with_report_and_scanner,
        parse_prepared_weavy_with_report,
    },
    parser::{ParseTable, ParserGrammar},
    validated::ValidatedGrammar,
};

/// (name, grammar.js, inputs). Standard tree-sitter DSL only.
const CORPUS: &[(&str, &str, &[&str])] = &[
    // Reduce/reduce resolved by dynamic precedence.
    (
        "dyn_reduce_reduce",
        r#"module.exports = grammar({ name: "drr", extras: ($) => [/\s/],
  conflicts: ($) => [[$.x, $.y]],
  rules: {
    source: ($) => choice($.x, $.y),
    x: ($) => prec.dynamic(2, $.ident),
    y: ($) => prec.dynamic(1, $.ident),
    ident: ($) => /[a-z]+/,
  }});"#,
        &["a"],
    ),
    // Filter/call, no prec.left: dynamic precedence resolves shift/reduce.
    (
        "filter_call_dynamic",
        r#"module.exports = grammar({ name: "fcd", extras: ($) => [/\s/],
  conflicts: ($) => [[$.filter, $.call], [$.filter, $.filter]],
  rules: {
    source: ($) => $._e,
    _e: ($) => choice($.filter, $.call, $.ident),
    filter: ($) => prec.dynamic(1,  seq($._e, "|", $.ident, optional($.args))),
    call:   ($) => prec.dynamic(-1, seq($._e, $.args)),
    args:   ($) => seq("(", optional($._e), ")"),
    ident:  ($) => /[a-z]+/,
  }});"#,
        &["x | f", "x | f | g", "x | f(y)"],
    ),
    // Filter/call WITH prec.left: associativity resolves it statically.
    (
        "filter_call_prec_left",
        r#"module.exports = grammar({ name: "fcl", extras: ($) => [/\s/],
  conflicts: ($) => [[$.filter, $.call]],
  rules: {
    source: ($) => $._e,
    _e: ($) => choice($.filter, $.call, $.ident),
    filter: ($) => prec.left(2, seq($._e, "|", $.ident, optional($.args))),
    call:   ($) => prec.left(2, seq($._e, $.args)),
    args:   ($) => seq("(", optional($._e), ")"),
    ident:  ($) => /[a-z]+/,
  }});"#,
        &["x | f(y)"],
    ),
    // Maximal munch resolved by dynamic precedence (input-dependent winner).
    (
        "maximal_pairing",
        r#"module.exports = grammar({ name: "mp", extras: ($) => [/\s/],
  conflicts: ($) => [[$.pair, $.single]],
  rules: {
    source: ($) => repeat1($._chunk),
    _chunk: ($) => choice($.pair, $.single),
    pair:   ($) => prec.dynamic(1, seq($.x, $.x)),
    single: ($) => prec.dynamic(0, $.x),
    x: ($) => "x",
  }});"#,
        // Determinate cases only: `x x` -> one pair, `x x x x` -> two pairs (a
        // unique max-dynprec parse). `x x x` is intentionally omitted: it is a
        // GENUINE tie (`pair single` vs `single pair`, both dyn +1), for which
        // tree-sitter silently picks one and snark currently reports
        // AmbiguousParse. A tie has no determinate reference answer, so it can't
        // be a differential assertion. Whether snark should silently pick (match
        // tree-sitter, drop-in) or surface the ambiguity (stricter, better) is a
        // design decision — see snark/docs/conflict-collapse-examples/README.md.
        &["x x", "x x x x"],
    ),
    // Arithmetic precedence + associativity via prec.left / prec.right.
    (
        "arith_precedence",
        r#"module.exports = grammar({ name: "arith", extras: ($) => [/\s/],
  rules: {
    source: ($) => $._e,
    _e: ($) => choice($.binary, $.number),
    binary: ($) => choice(
      prec.left(1,  seq($._e, "+", $._e)),
      prec.left(2,  seq($._e, "*", $._e)),
      prec.right(3, seq($._e, "^", $._e)),
    ),
    number: ($) => /\d+/,
  }});"#,
        &[
            "1 + 2 * 3",
            "1 * 2 + 3",
            "1 + 2 + 3",
            "2 ^ 3 ^ 2",
            "1 + 2 * 3 ^ 4",
        ],
    ),
];

#[test]
fn snark_parse_matches_tree_sitter() {
    if !tree_sitter_available() {
        eprintln!("skipping snark-ts-diff: `tree-sitter` CLI not found on PATH");
        return;
    }

    let mut failures = Vec::new();
    for (name, grammar, inputs) in CORPUS {
        let dir = match generate_parser(name, grammar) {
            Ok(dir) => dir,
            Err(err) => {
                failures.push(format!("[{name}] tree-sitter generate failed: {err}"));
                continue;
            }
        };
        let grammar_path = dir.join("grammar.js");
        for input in *inputs {
            let ts = normalize(&tree_sitter_sexp(&dir, input));
            let sn = normalize(&snark_sexp(&grammar_path, input));
            if ts != sn {
                failures.push(format!(
                    "[{name}] input {input:?}\n    tree-sitter: {ts}\n    snark:       {sn}"
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "snark diverged from tree-sitter on {} case(s):\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// snark DELIBERATELY diverges from tree-sitter on a genuine dynamic-precedence
/// tie: tree-sitter silently picks one parse, snark surfaces `AmbiguousParse` so
/// the grammar author learns their grammar is ambiguous. This is intentional and
/// preferred — do NOT "fix" it to match tree-sitter's coin flip.
#[test]
fn snark_surfaces_genuine_ambiguity_tree_sitter_hides() {
    if !tree_sitter_available() {
        eprintln!("skipping: `tree-sitter` CLI not found on PATH");
        return;
    }
    let grammar = CORPUS
        .iter()
        .find(|(name, ..)| *name == "maximal_pairing")
        .map(|(_, grammar, _)| *grammar)
        .expect("maximal_pairing grammar");
    let dir = generate_parser("maximal_pairing_tie", grammar).expect("tree-sitter generate");

    // `x x x`: `pair single` and `single pair`, both dynamic precedence +1.
    let ts = normalize(&tree_sitter_sexp(&dir, "x x x"));
    assert!(
        !ts.is_empty() && !ts.contains("ERROR"),
        "tree-sitter should silently produce one tree: {ts}"
    );
    let sn = snark_sexp(&dir.join("grammar.js"), "x x x");
    assert!(
        sn.contains("AmbiguousParse"),
        "snark should surface the ambiguity, not pick one silently; got: {sn}"
    );
}

#[test]
fn bundled_graphql_and_thrift_lex_without_notoken() {
    let tree_sitter_available = tree_sitter_available();
    if !tree_sitter_available {
        eprintln!("skipping tree-sitter oracle: `tree-sitter` CLI not found on PATH");
    }

    let mut failures = Vec::new();
    for (name, grammar_path, sample_path) in [
        (
            "graphql",
            bundled_path("graphql/grammar.js"),
            bundled_path("graphql/samples/starwars_schema.graphql"),
        ),
        (
            "thrift",
            bundled_path("thrift/grammar.js"),
            bundled_path("thrift/samples/tutorial.thrift"),
        ),
    ] {
        let grammar = match fs::read_to_string(&grammar_path) {
            Ok(grammar) => grammar,
            Err(err) => {
                failures.push(format!(
                    "[{name}] could not read grammar {}: {err}",
                    grammar_path.display()
                ));
                continue;
            }
        };
        let input = match fs::read_to_string(&sample_path) {
            Ok(input) => input,
            Err(err) => {
                failures.push(format!(
                    "[{name}] could not read sample {}: {err}",
                    sample_path.display()
                ));
                continue;
            }
        };
        let sn = snark_sexp(&grammar_path, &input);
        if sn.starts_with("PARSE-ERR:") || sn.contains("NoToken") {
            failures.push(format!(
                "[{name}] Snark failed to lex/parse bundled sample: {sn}"
            ));
        }
        if tree_sitter_available {
            let Ok(dir) = generate_parser(name, &grammar) else {
                continue;
            };
            fs::write(dir.join("input.txt"), &input).expect("write bundled sample");
            let ts = tree_sitter_parse_file(&dir, "input.txt");
            if !ts.trim().is_empty() && ts.contains("ERROR") {
                failures.push(format!(
                    "[{name}] tree-sitter did not parse the bundled sample cleanly: {ts}"
                ));
            }
        }
    }

    assert!(
        failures.is_empty(),
        "bundled lexer regression(s):\n{}",
        failures.join("\n")
    );
}

#[test]
fn bundled_graphql_unterminated_block_string_recovers_to_error_root() {
    if !tree_sitter_available() {
        eprintln!("skipping: `tree-sitter` CLI not found on PATH");
        return;
    }

    let grammar_path = bundled_path("graphql/grammar.js");
    let grammar = fs::read_to_string(&grammar_path).expect("read bundled GraphQL grammar");
    let dir = generate_parser("graphql_unterminated_block_string", &grammar)
        .expect("tree-sitter generate");
    let input = "\"\"\"broken\n";
    let tree_sitter_output = tree_sitter_sexp(&dir, input);
    let ts = normalize(tree_sitter_output.lines().next().unwrap_or_default());
    assert_eq!(ts, "(ERROR)");

    let sn = normalize(&snark_recovering_sexp(&grammar_path, input));
    assert_eq!(sn, ts);
}

#[test]
fn readiness_accepts_frozen_grammar_json() {
    let grammar_path =
        workspace_path("snark/tests/fixtures/packages/tree-sitter-css-reduced/src/grammar.json");
    let out = Command::new(env!("CARGO_BIN_EXE_snark-ts-diff"))
        .arg("readiness")
        .arg(&grammar_path)
        .output()
        .expect("run snark-ts-diff readiness");
    assert!(
        out.status.success(),
        "readiness failed: stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("visibility: parser=true lexer=false full=false"));
    assert!(stdout.contains("lexer_ops:"));
    assert!(stdout.contains("  Pattern:"));
    assert!(stdout.contains("stencil_descriptors:"));
    assert!(stdout.contains("snark.tree_sitter.lex domain=Lexing lowering=LexerGraph"));
    assert!(stdout.contains("snark.tree_sitter.reduce domain=Tree lowering=DialectOp"));
    assert!(stdout.contains("effect_order=Ordered"));
    assert!(stdout.contains("may_fail=true may_allocate=true calls_user_code=false opaque=false"));
    assert!(stdout.contains("resources=[ResourceEffect"));
    assert!(stdout.contains("host_barriers=0 opaque=0 host_calls=0"));
    assert!(stdout.contains("Lexer(ExternalScanner):"));
    assert!(stdout.contains("native_hostcall_blocks: total="));
    assert!(stdout.contains("native_hostcall_block_barriers:"));
}

// ---------------------------------------------------------------------------

fn tree_sitter_available() -> bool {
    Command::new("tree-sitter")
        .arg("--version")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Write the grammar and run `tree-sitter generate` in a fresh temp dir.
fn generate_parser(name: &str, grammar: &str) -> Result<PathBuf, String> {
    let dir = env::temp_dir().join(format!("snark-ts-diff-{name}"));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    fs::write(dir.join("grammar.js"), grammar).map_err(|e| e.to_string())?;
    let out = Command::new("tree-sitter")
        .arg("generate")
        .current_dir(&dir)
        .output()
        .map_err(|e| e.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).into_owned());
    }
    Ok(dir)
}

fn tree_sitter_sexp(dir: &Path, input: &str) -> String {
    let _ = fs::write(dir.join("in.txt"), input);
    tree_sitter_parse_file(dir, "in.txt")
}

fn tree_sitter_parse_file(dir: &Path, path: &str) -> String {
    let out = Command::new("tree-sitter")
        .arg("parse")
        .arg(path)
        .current_dir(dir)
        .output()
        .expect("run tree-sitter parse");
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn bundled_path(relative: &str) -> PathBuf {
    workspace_path(format!("playgrounds/snark/src/bundled/{relative}"))
}

fn workspace_path(relative: impl AsRef<Path>) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("snark-ts-diff lives inside the facet workspace")
        .join(relative)
}

/// Snark's named-node s-expression through the production Weavy executor.
fn snark_sexp(grammar_path: &Path, input: &str) -> String {
    let json = match snark_dsl::emit_with_boa(grammar_path) {
        Ok(json) => json,
        Err(e) => return format!("EMIT-ERR: {e}"),
    };
    let raw = match RawGrammarJson::from_tree_sitter_json_str(&json) {
        Ok(raw) => raw,
        Err(e) => return format!("IMPORT-ERR: {e:?}"),
    };
    let validated = match ValidatedGrammar::from_raw(&raw) {
        Ok(v) => v,
        Err(e) => return format!("VALIDATE-ERR: {e:?}"),
    };
    let lexical = LexicalFacts::from_grammar(&validated);
    let normalized = match ParserGrammar::normalize_from_validated(&validated, &lexical) {
        Ok(n) => n,
        Err(e) => return format!("NORMALIZE-ERR: {e:?}"),
    };
    let parser = match normalized.prepare_productions_for_items() {
        Ok(p) => p,
        Err(e) => return format!("PREPARE-ERR: {e:?}"),
    };
    let table = match ParseTable::from_grammar(&parser) {
        Ok(t) => t,
        Err(e) => return format!("TABLE-ERR: {e:?}"),
    };
    let plan = match WeavyParsePlan::new(&validated, &parser, &table) {
        Ok(p) => p,
        Err(e) => return format!("PLAN-ERR: {e:?}"),
    };
    match parse_prepared_weavy_with_report(&plan, &parser, &table, input) {
        Ok(report) => report.tree().to_sexp(),
        Err(e) => format!("PARSE-ERR: {e:?}"),
    }
}

fn snark_recovering_sexp(grammar_path: &Path, input: &str) -> String {
    let json = match snark_dsl::emit_with_boa(grammar_path) {
        Ok(json) => json,
        Err(e) => return format!("EMIT-ERR: {e}"),
    };
    let raw = match RawGrammarJson::from_tree_sitter_json_str(&json) {
        Ok(raw) => raw,
        Err(e) => return format!("IMPORT-ERR: {e:?}"),
    };
    let validated = match ValidatedGrammar::from_raw(&raw) {
        Ok(v) => v,
        Err(e) => return format!("VALIDATE-ERR: {e:?}"),
    };
    let lexical = LexicalFacts::from_grammar(&validated);
    let normalized = match ParserGrammar::normalize_from_validated(&validated, &lexical) {
        Ok(n) => n,
        Err(e) => return format!("NORMALIZE-ERR: {e:?}"),
    };
    let parser = match normalized.prepare_productions_for_items() {
        Ok(p) => p,
        Err(e) => return format!("PREPARE-ERR: {e:?}"),
    };
    let table = match ParseTable::from_grammar(&parser) {
        Ok(t) => t,
        Err(e) => return format!("TABLE-ERR: {e:?}"),
    };
    let plan = match WeavyParsePlan::new(&validated, &parser, &table) {
        Ok(p) => p,
        Err(e) => return format!("PLAN-ERR: {e:?}"),
    };
    match parse_prepared_weavy_recovering_with_report_and_scanner(
        &plan, &parser, &table, input, None,
    ) {
        Ok(report) => report.tree().to_sexp(),
        Err(e) => format!("PARSE-ERR: {e:?}"),
    }
}

/// Canonicalize an s-expression for comparison: drop tree-sitter position
/// ranges (`[r, c] - [r, c]`), anonymous quoted terminals, and all whitespace —
/// leaving only the `(named-node …)` structure both sides agree on.
fn normalize(sexp: &str) -> String {
    let mut out = String::new();
    let mut in_bracket = false;
    for c in sexp.chars() {
        match c {
            '[' => in_bracket = true,
            ']' => in_bracket = false,
            _ if in_bracket => {}
            '(' | ')' => out.push(c),
            c if c.is_alphanumeric() || c == '_' => out.push(c),
            _ => {}
        }
    }
    out
}
