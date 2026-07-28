use vix::surface::SurfaceParser;
use vix::surface::ast::{CommandAtom, CommandFused, CommandItem, Item};

fn parse_command(source: &str) -> CommandItem {
    let file = SurfaceParser::new()
        .parse(source)
        .expect("command declaration parses");
    let command = file.items.iter().find_map(|item| match item {
        Item::Command(command) => Some(command.as_ref()),
        _ => None,
    });
    command.expect("source declares a command").clone()
}

#[test]
fn command_declaration_lowers_to_algebraic_surface_ast() {
    let source = r#"
enum CrateType { Bin, Lib, ProcMacro }

command Rustc -> Tree {
    program "rustc"
    grammar {
        [--crate-name {crate_name: String}]
        [--crate-type {crate_type: CrateType}]
        [--cfg {cfg: String}]*
        {input: Input<Path>}
        [-o {output: Output<Path>}]
    }
}
"#;

    let file = SurfaceParser::new()
        .parse(source)
        .expect("command declaration parses");
    assert_eq!(file.items.len(), 2);

    let Item::Command(command) = &file.items[1] else {
        panic!("second item is a command declaration");
    };
    assert_eq!(command.name.value, "Rustc");
    assert_eq!(command.program.value, "rustc");
    assert!(command.return_type.is_some());
    let pattern = command.grammar.pattern.as_ref().expect("pattern");
    assert_eq!(pattern.alternatives.len(), 1);

    let terms = &pattern.alternatives[0].terms;
    assert_eq!(terms.len(), 5);
    assert!(matches!(terms[0].atom, CommandAtom::Optional(_)));
    assert_eq!(
        terms[2].quantifier.as_ref().map(|q| q.value.as_str()),
        Some("*")
    );
    assert!(matches!(terms[3].atom, CommandAtom::Slot(_)));
}

#[test]
fn command_grammar_supports_alternatives_groups_and_repetition() {
    let source = r#"
command Cc -> Tree {
    program "cc"
    grammar {
        {flags: Flag}*
        {inputs: Input<Path>}+
        (-c -o {object: Output<Path>} | -shared -o {library: Output<Path>})
    }
}
"#;

    let command = parse_command(source);
    let pattern = command.grammar.pattern.as_ref().expect("pattern");
    let terms = &pattern.alternatives[0].terms;
    assert_eq!(
        terms[0].quantifier.as_ref().map(|q| q.value.as_str()),
        Some("*")
    );
    assert_eq!(
        terms[1].quantifier.as_ref().map(|q| q.value.as_str()),
        Some("+")
    );

    let CommandAtom::Group(group) = &terms[2].atom else {
        panic!("third term is a grouped alternative");
    };
    assert_eq!(group.pattern.alternatives.len(), 2);
}

#[test]
fn command_role_payloads_remain_ordinary_surface_types() {
    let source = r#"
struct Config { value: String }

command Tool -> Tree {
    program "tool"
    grammar {
        {flag: Flag}*
        {input: Input<Path>}
        {config: Config}
    }
}
"#;

    let command = parse_command(source);
    let pattern = command.grammar.pattern.as_ref().expect("pattern");
    assert_eq!(pattern.alternatives[0].terms.len(), 3);
}

// A zero-argument program is a command whose grammar denotes the empty argv
// language.
#[test]
fn empty_grammar_declares_a_zero_argument_program() {
    let command = parse_command(r#"command True { program "true" grammar { } }"#);
    assert!(command.grammar.pattern.is_none());
}

// Adjacency fuses: atoms flush against each other continue one argv element,
// whitespace starts the next one. `-D{define}` is one element; `-D {define}`
// is two.
#[test]
fn adjacency_fuses_atoms_into_one_argv_element() {
    let command = parse_command(
        r#"command Cc { program "cc" grammar { -D{define: String} -D {detached: String} } }"#,
    );
    let pattern = command.grammar.pattern.as_ref().expect("pattern");
    let terms = &pattern.alternatives[0].terms;
    assert_eq!(terms.len(), 3, "fused, detached literal, detached slot");

    let CommandAtom::Literal(head) = &terms[0].atom else {
        panic!("fused term starts with the literal");
    };
    assert_eq!(head.value, "-D");
    assert_eq!(terms[0].fused_atoms.len(), 1);
    let CommandFused::Slot(slot) = &terms[0].fused_atoms[0] else {
        panic!("fused continuation is the slot");
    };
    assert_eq!(slot.name.value, "define");

    let CommandAtom::Literal(detached) = &terms[1].atom else {
        panic!("second term is the detached literal");
    };
    assert_eq!(detached.value, "-D");
    assert!(terms[1].fused_atoms.is_empty());
    assert!(matches!(terms[2].atom, CommandAtom::Slot(_)));
}

// Chains fuse too: protoc's `--plugin=protoc-gen-{name}={plugin}` is a single
// argv element built from literal/slot fragments, and `-j[{jobs}]` fuses an
// optional value onto its flag.
#[test]
fn fused_chains_and_fused_optionals_stay_one_element() {
    let command = parse_command(
        r#"command Tools { program "tools" grammar { --plugin=protoc-gen-{name: String}={plugin: Input<Path>} -j[{jobs: Int}] } }"#,
    );
    let pattern = command.grammar.pattern.as_ref().expect("pattern");
    let terms = &pattern.alternatives[0].terms;
    assert_eq!(terms.len(), 2);

    let chain = &terms[0];
    assert!(matches!(&chain.atom, CommandAtom::Literal(l) if l.value == "--plugin=protoc-gen-"));
    assert_eq!(chain.fused_atoms.len(), 3);
    assert!(matches!(&chain.fused_atoms[0], CommandFused::Slot(s) if s.name.value == "name"));
    assert!(matches!(&chain.fused_atoms[1], CommandFused::Literal(l) if l.value == "="));
    assert!(matches!(&chain.fused_atoms[2], CommandFused::Slot(s) if s.name.value == "plugin"));

    let flag = &terms[1];
    assert!(matches!(&flag.atom, CommandAtom::Literal(l) if l.value == "-j"));
    assert_eq!(flag.fused_atoms.len(), 1);
    assert!(matches!(&flag.fused_atoms[0], CommandFused::Optional(_)));
}

// Quoted argv elements carry characters the bare literal cannot: grammar
// metacharacters and whitespace, with ordinary string escapes. find's `{}` is
// the canonical case. Quoted atoms fuse by adjacency like everything else.
#[test]
fn quoted_literals_carry_reserved_characters_and_fuse() {
    let command = parse_command(
        r#"command Find { program "find" grammar { -exec {tool: String} "{}" ";" --label="a b"{suffix: String} } }"#,
    );
    let pattern = command.grammar.pattern.as_ref().expect("pattern");
    let terms = &pattern.alternatives[0].terms;
    assert_eq!(terms.len(), 5);
    assert!(matches!(&terms[2].atom, CommandAtom::Str(s) if s.value == "{}"));
    assert!(matches!(&terms[3].atom, CommandAtom::Str(s) if s.value == ";"));

    let fused = &terms[4];
    assert!(matches!(&fused.atom, CommandAtom::Literal(l) if l.value == "--label="));
    assert_eq!(fused.fused_atoms.len(), 2);
    assert!(matches!(&fused.fused_atoms[0], CommandFused::Str(s) if s.value == "a b"));
    assert!(matches!(&fused.fused_atoms[1], CommandFused::Slot(s) if s.name.value == "suffix"));
}

// A quantifier binds only when attached; a detached `*` or `+` is an ordinary
// argv literal. (`token.immediate` semantics — the lexer refuses the
// quantifier across whitespace.)
#[test]
fn detached_star_is_a_literal_not_a_quantifier() {
    let command =
        parse_command(r#"command Sh { program "sh" grammar { {dir: Path} * {glob: String}* } }"#);
    let pattern = command.grammar.pattern.as_ref().expect("pattern");
    let terms = &pattern.alternatives[0].terms;
    assert_eq!(terms.len(), 3);
    assert!(matches!(terms[0].atom, CommandAtom::Slot(_)));
    assert!(
        terms[0].quantifier.is_none(),
        "detached `*` must not quantify"
    );
    assert!(matches!(&terms[1].atom, CommandAtom::Literal(l) if l.value == "*"));
    assert_eq!(
        terms[2].quantifier.as_ref().map(|q| q.value.as_str()),
        Some("*")
    );
}

// The quantifier outranks fused literals right after an atom: `{x}*` is a
// quantified slot, never a slot fused with a `*` literal. A literal that
// begins mid-element with `*`/`+` needs the quoted spelling.
#[test]
fn quantifier_wins_over_fusion_and_quoting_is_the_escape_hatch() {
    let command =
        parse_command(r#"command Q { program "q" grammar { {a: String}* {b: String}"+suffix" } }"#);
    let pattern = command.grammar.pattern.as_ref().expect("pattern");
    let terms = &pattern.alternatives[0].terms;
    assert_eq!(terms.len(), 2);
    assert_eq!(
        terms[0].quantifier.as_ref().map(|q| q.value.as_str()),
        Some("*")
    );
    assert!(terms[0].fused_atoms.is_empty());
    assert_eq!(terms[1].fused_atoms.len(), 1);
    assert!(matches!(&terms[1].fused_atoms[0], CommandFused::Str(s) if s.value == "+suffix"));
}

// `[x]+` reads "at least once" but denotes the same language as `[x]*` — the
// surface rejects it with a typed diagnostic naming both correct spellings.
#[test]
fn plus_quantified_optional_is_rejected_as_misleading() {
    let parser = SurfaceParser::new();
    let error = parser
        .parse(r#"command M { program "m" grammar { [-v]+ } }"#)
        .expect_err("`[…]+` is rejected");
    let rendered = format!("{error:?}");
    assert!(
        rendered.contains("MisleadingQuantifier"),
        "diagnostic identity is MisleadingQuantifier: {rendered}"
    );

    for accepted in [
        r#"command M { program "m" grammar { [-v]* } }"#,
        r#"command M { program "m" grammar { (-v)+ } }"#,
        r#"command M { program "m" grammar { -j[{jobs: Int}]+ } }"#,
    ] {
        parser
            .parse(accepted)
            .unwrap_or_else(|error| panic!("unexpectedly rejected:\n{accepted}\n{error:?}"));
    }
}

// Longest match keeps punctuation-bearing literals whole: a quantifier after
// a bare literal is unreachable (`gcc+` is one literal). `(gcc)+` is the
// spelling for literal repetition.
#[test]
fn literal_quantifiers_are_unreachable_and_groups_repeat_instead() {
    let command = parse_command(r#"command Cxx { program "cxx" grammar { c++ gcc+ (gcc)+ } }"#);
    let pattern = command.grammar.pattern.as_ref().expect("pattern");
    let terms = &pattern.alternatives[0].terms;
    assert_eq!(terms.len(), 3);
    assert!(matches!(&terms[0].atom, CommandAtom::Literal(l) if l.value == "c++"));
    assert!(matches!(&terms[1].atom, CommandAtom::Literal(l) if l.value == "gcc+"));
    assert!(terms[1].quantifier.is_none());
    assert!(matches!(terms[2].atom, CommandAtom::Group(_)));
    assert_eq!(
        terms[2].quantifier.as_ref().map(|q| q.value.as_str()),
        Some("+")
    );
}
