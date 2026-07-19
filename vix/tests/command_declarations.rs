use vix::VixParser;
use vix::ast::{CommandAtom, Item};

#[test]
fn command_declaration_lowers_to_algebraic_ast() {
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

    let file = VixParser::new()
        .parse(source)
        .expect("command declaration parses");
    assert_eq!(file.items.len(), 2);

    let Item::Command(command) = &file.items[1] else {
        panic!("second item is a command declaration");
    };
    assert_eq!(command.name.value, "Rustc");
    assert_eq!(command.program.value, "rustc");
    assert!(command.return_type.is_some());
    assert_eq!(command.grammar.pattern.alternatives.len(), 1);

    let terms = &command.grammar.pattern.alternatives[0].terms;
    assert_eq!(terms.len(), 5);
    assert!(matches!(terms[0].atom, CommandAtom::Optional(_)));
    assert_eq!(
        terms[2].quantifier.as_ref().map(|q| q.value.as_str()),
        Some("*")
    );
    assert!(matches!(terms[3].atom, CommandAtom::Slot(_)));
}

#[test]
fn command_grammar_supports_alternatives_and_groups() {
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

    let file = VixParser::new()
        .parse(source)
        .expect("command alternatives parse");
    let Item::Command(command) = &file.items[0] else {
        panic!("item is a command declaration");
    };
    let terms = &command.grammar.pattern.alternatives[0].terms;
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
