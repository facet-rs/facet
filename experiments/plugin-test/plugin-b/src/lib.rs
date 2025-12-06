use proc_macro::TokenStream;

#[proc_macro]
pub fn invoke_b(_input: TokenStream) -> TokenStream {
    eprintln!("[plugin-b] invoke_b called, checking registry...");

    let plugins = registry::list_plugins();
    for (name, f) in &plugins {
        eprintln!("[plugin-b] found plugin: {} -> {}", name, f());
    }

    if plugins.is_empty() {
        eprintln!("[plugin-b] NO PLUGINS FOUND - registry not shared!");
    }

    TokenStream::new()
}
