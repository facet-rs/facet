use proc_macro::TokenStream;

type CodegenFn = fn(&str) -> String;

extern "Rust" {
    fn registry_list() -> Vec<(&'static str, CodegenFn)>;
}

#[proc_macro]
pub fn invoke_b(input: TokenStream) -> TokenStream {
    let type_name = input.to_string();
    let type_name = if type_name.is_empty() {
        "MyError"
    } else {
        &type_name
    };

    eprintln!("[plugin-b] invoke_b called for type '{}'", type_name);

    let codegens = unsafe { registry_list() };

    if codegens.is_empty() {
        eprintln!("[plugin-b] NO CODEGENS FOUND - registry not shared!");
        return TokenStream::new();
    }

    let mut output = String::new();
    for (name, codegen) in &codegens {
        eprintln!("[plugin-b] running codegen: {}", name);
        let code = codegen(type_name);
        eprintln!("[plugin-b] generated:\n{}", code);
        output.push_str(&code);
    }

    output.parse().unwrap_or_else(|e| {
        eprintln!("[plugin-b] parse error: {}", e);
        TokenStream::new()
    })
}
