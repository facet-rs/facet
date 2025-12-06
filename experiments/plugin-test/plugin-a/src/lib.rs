use proc_macro::TokenStream;

// Register when this proc-macro is loaded
#[ctor::ctor]
fn init() {
    registry::register_plugin("plugin-a", || "hello from plugin-a");
}

#[proc_macro]
pub fn invoke_a(_input: TokenStream) -> TokenStream {
    eprintln!("[plugin-a] invoke_a called");
    TokenStream::new()
}
