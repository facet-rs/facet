use std::ffi::{CStr, CString, c_char};
use std::sync::Mutex;

static REGISTRY: Mutex<Vec<(CString, CString)>> = Mutex::new(Vec::new());

#[unsafe(no_mangle)]
pub extern "C" fn registry_register(name: *const c_char, value: *const c_char) {
    let name_cstr = unsafe { CStr::from_ptr(name) };
    let value_cstr = unsafe { CStr::from_ptr(value) };
    eprintln!(
        "[registry] registering: {} = {}",
        name_cstr.to_str().unwrap_or("?"),
        value_cstr.to_str().unwrap_or("?")
    );
    REGISTRY
        .lock()
        .unwrap()
        .push((name_cstr.to_owned(), value_cstr.to_owned()));
}

#[unsafe(no_mangle)]
pub extern "C" fn registry_count() -> usize {
    let count = REGISTRY.lock().unwrap().len();
    eprintln!("[registry] count = {}", count);
    count
}

#[unsafe(no_mangle)]
pub extern "C" fn registry_get_name(index: usize) -> *const c_char {
    REGISTRY
        .lock()
        .unwrap()
        .get(index)
        .map(|(n, _)| n.as_ptr())
        .unwrap_or(std::ptr::null())
}

#[unsafe(no_mangle)]
pub extern "C" fn registry_get_value(index: usize) -> *const c_char {
    REGISTRY
        .lock()
        .unwrap()
        .get(index)
        .map(|(_, v)| v.as_ptr())
        .unwrap_or(std::ptr::null())
}
