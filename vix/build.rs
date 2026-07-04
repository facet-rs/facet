use std::env;
use std::path::PathBuf;

use snark_dsl::typed_ast::{TypedAstConfig, generate_typed_ast};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let repo = manifest.parent().unwrap().to_path_buf();
    let grammar_js = repo.join("playgrounds/snark/src/bundled/vix/grammar.js");
    let ann_js = manifest.join("vix_ast.snark.js");
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());

    println!("cargo:rerun-if-changed=build.rs");
    generate_typed_ast(&TypedAstConfig {
        grammar_js: &grammar_js,
        annotations_js: &ann_js,
        out_dir: &out,
        grammar_output: "vix_grammar.json",
        ast_output: "vix_ast.rs",
        annotation_source_name: "vix_ast.snark.js",
        generated_by: "vix/build.rs",
        language_name: "vix",
    })
    .expect("generate vix typed AST");
}
