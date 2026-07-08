use std::{env, path::PathBuf, process::ExitCode};

use snark_dsl::{check_against_tree_sitter, emit_with_boa, emit_with_tree_sitter, grammar_arg};

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(err) => {
            eprintln!("{err}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> snark_dsl::Result<()> {
    let mut args = env::args_os();
    let _program = args.next();

    let Some(command) = args.next() else {
        print_usage();
        return Ok(());
    };

    match command.to_string_lossy().as_ref() {
        "emit" => {
            let grammar = grammar_arg(args.next().as_deref());
            println!("{}", emit_with_boa(&grammar)?);
        }
        "oracle" => {
            let grammar = grammar_arg(args.next().as_deref());
            println!("{}", emit_with_tree_sitter(&grammar)?);
        }
        "check" => {
            let grammar = grammar_arg(args.next().as_deref());
            check_against_tree_sitter(&grammar)?;
            println!(
                "emitted output matches tree-sitter output for {}",
                display_path(grammar)
            );
        }
        "-h" | "--help" | "help" => print_usage(),
        other => {
            eprintln!("unknown command: {other}");
            print_usage();
            return Err(snark_dsl::Error::Usage("unknown command".to_string()));
        }
    }

    Ok(())
}

fn display_path(path: PathBuf) -> String {
    path.display().to_string()
}

fn print_usage() {
    eprintln!(
        "Usage:\n  snark-dsl emit [grammar.js]\n  snark-dsl oracle [grammar.js]\n  snark-dsl check [grammar.js]\n\nIf grammar.js is omitted, the Arborium Hazel Lua grammar is used."
    );
}
