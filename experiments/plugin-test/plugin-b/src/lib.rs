use proc_macro::TokenStream;
use std::ffi::{CStr, c_char};

extern "C" {
    fn registry_count() -> usize;
    fn registry_get_name(index: usize) -> *const c_char;
    fn registry_get_value(index: usize) -> *const c_char;
}

#[proc_macro]
pub fn invoke_b(_input: TokenStream) -> TokenStream {
    eprintln!("[plugin-b] invoke_b called, checking registry...");

    unsafe {
        let count = registry_count();
        for i in 0..count {
            let name = CStr::from_ptr(registry_get_name(i)).to_str().unwrap_or("?");
            let value = CStr::from_ptr(registry_get_value(i))
                .to_str()
                .unwrap_or("?");
            eprintln!("[plugin-b] found plugin: {} = {}", name, value);
        }

        if count == 0 {
            eprintln!("[plugin-b] NO PLUGINS FOUND - registry not shared!");
        }
    }

    TokenStream::new()
}
