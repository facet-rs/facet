use proc_macro::TokenStream;

type CodegenFn = fn(&str) -> String;

extern "Rust" {
    fn registry_register(name: &'static str, codegen: CodegenFn);
}

fn error_codegen(type_name: &str) -> String {
    format!(
        r#"
impl std::fmt::Display for {type_name} {{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {{
        write!(f, "Error: {type_name}")
    }}
}}
impl std::error::Error for {type_name} {{}}
"#
    )
}

#[ctor::ctor]
fn init() {
    unsafe {
        registry_register("error", error_codegen);
    }
}

#[proc_macro]
pub fn invoke_a(_input: TokenStream) -> TokenStream {
    eprintln!("[plugin-a] invoke_a called, registered error codegen");
    TokenStream::new()
}
