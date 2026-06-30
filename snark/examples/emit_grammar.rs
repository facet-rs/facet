//! Emit a tree-sitter `grammar.js` to its `grammar.json` (via the boa JS evaluator),
//! printing the JSON to stdout. Used to pre-emit embedded-language grammars for the
//! playground's `languages/<lang>/src/grammar.json` injection convention.
//!
//! Usage: cargo run -p snark --features json-import --example emit_grammar -- <grammar.js>

use std::{env, path::PathBuf};

fn main() {
    let path = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .expect("usage: emit_grammar <grammar.js>");
    let json = snark_dsl::emit_with_boa(&path).expect("grammar.js should emit grammar JSON");
    print!("{json}");
}
