use std::env;
use std::fs;
use std::path::PathBuf;

use snark_dsl::typed_ast::{TypedAstConfig, generate_typed_ast};

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let repo = manifest.parent().unwrap().to_path_buf();
    let grammar_js = repo.join("playgrounds/snark/src/bundled/vix/grammar.js");
    let surface_grammar_js = manifest.join("surface/grammar.js");
    let cfg_grammar_js = repo.join("playgrounds/snark/src/bundled/cfg/grammar.js");
    let ann_js = manifest.join("vix_ast.snark.js");
    let surface_ann_js = manifest.join("surface_ast.snark.js");
    let out = PathBuf::from(env::var("OUT_DIR").unwrap());

    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed={}", grammar_js.display());
    println!("cargo:rerun-if-changed={}", ann_js.display());
    println!("cargo:rerun-if-changed={}", surface_grammar_js.display());
    println!("cargo:rerun-if-changed={}", surface_ann_js.display());
    println!("cargo:rerun-if-changed={}", cfg_grammar_js.display());
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
    generate_typed_ast(&TypedAstConfig {
        grammar_js: &surface_grammar_js,
        annotations_js: &surface_ann_js,
        out_dir: &out,
        grammar_output: "vix_surface_grammar.json",
        ast_output: "vix_surface_ast.rs",
        annotation_source_name: "surface_ast.snark.js",
        generated_by: "vix/build.rs",
        language_name: "vix_surface",
    })
    .expect("generate vix surface typed AST");
    let cfg_grammar = snark_dsl::emit_with_boa(&cfg_grammar_js).expect("emit cfg grammar");
    fs::write(out.join("cfg_grammar.json"), cfg_grammar).expect("write cfg grammar");
}
