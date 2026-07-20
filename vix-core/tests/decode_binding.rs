//! The single `decode(document, Format::…)` binding.
//!
//! Format is a `Format` enum selector read at lower time; the target type comes
//! from the expected type, so a generic wrapper `fn json_decode<T>(s) -> T`
//! forwards `T` here via return-position inference. This is the primitive seam
//! the json_decode/toml_decode vix functions wrap — json vs toml is a request
//! field the DecodePrimitive reads, not a distinct compiler intrinsic.

use vix::compiler::Compiler;

#[test]
fn decode_binding_infers_target_from_return() {
    let src = r#"
enum Format { Json, Toml }
struct Pkg { name: String }
fn from_json<T>(text: String) -> T { decode(text, Format::Json) }
pub fn main() -> Pkg { from_json("{\"name\":\"blake3\"}") }
"#;
    Compiler::default()
        .compile(src)
        .expect("decode(text, Format::Json) via a generic wrapper compiles");
}

#[test]
fn decode_binding_reads_toml_format() {
    let src = r#"
enum Format { Json, Toml }
struct Manifest { name: String }
fn from_toml<T>(text: String) -> T { decode(text, Format::Toml) }
pub fn main() -> Manifest { from_toml("name = \"taxon\"\n") }
"#;
    Compiler::default()
        .compile(src)
        .expect("decode(text, Format::Toml) via a generic wrapper compiles");
}

#[test]
fn decode_binding_rejects_unknown_format() {
    // A `Format` variant that is not a decoder is rejected at the binding.
    let src = r#"
enum Format { Json, Toml, Yaml }
struct Pkg { name: String }
fn from<T>(text: String) -> T { decode(text, Format::Yaml) }
pub fn main() -> Pkg { from("{}") }
"#;
    assert!(
        Compiler::default().compile(src).is_err(),
        "Format::Yaml is not a decode format"
    );
}
