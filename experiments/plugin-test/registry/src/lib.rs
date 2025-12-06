use std::sync::Mutex;

pub type CodegenFn = fn(&str) -> String;

static REGISTRY: Mutex<Vec<(&'static str, CodegenFn)>> = Mutex::new(Vec::new());

#[unsafe(no_mangle)]
pub fn registry_register(name: &'static str, codegen: CodegenFn) {
    eprintln!("[registry] registering codegen: {}", name);
    REGISTRY.lock().unwrap().push((name, codegen));
}

#[unsafe(no_mangle)]
pub fn registry_list() -> Vec<(&'static str, CodegenFn)> {
    let plugins = REGISTRY.lock().unwrap().clone();
    eprintln!("[registry] listing {} codegens", plugins.len());
    plugins
}
