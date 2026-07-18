//! Return-position type inference for generic functions.
//!
//! A type parameter that appears only in the return type (`fn f<T>(…) -> T`) is
//! invisible to argument-driven inference. Since Vix rejects turbofish generic
//! calls at parse, the only way to instantiate such a function is from the
//! call's expected type. This is the enabler for decode aliases as vix
//! functions (`fn json_decode<T>(s: String) -> T { json_decode(s) }`-shaped).

use vix::compiler::Compiler;

#[test]
fn return_only_type_param_infers_from_expected() {
    let src = r#"
struct Pkg { name: String }
fn decode_json<T>(text: String) -> T { json_decode(text) }
pub fn main() -> Pkg { decode_json("{\"name\":\"blake3\"}") }
"#;
    Compiler::default()
        .compile(src)
        .expect("return-only T is inferred from the expected return type");
}
