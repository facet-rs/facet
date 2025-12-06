use proc_macro::TokenStream;
use std::ffi::c_char;

extern "C" {
    fn registry_register(name: *const c_char, value: *const c_char);
}

#[ctor::ctor]
fn init() {
    unsafe {
        registry_register(c"plugin-a".as_ptr(), c"hello from plugin-a".as_ptr());
    }
}

#[proc_macro]
pub fn invoke_a(_input: TokenStream) -> TokenStream {
    eprintln!("[plugin-a] invoke_a called");
    TokenStream::new()
}
