use std::sync::Mutex;

pub type PluginFn = fn() -> &'static str;

static REGISTRY: Mutex<Vec<(&'static str, PluginFn)>> = Mutex::new(Vec::new());

pub fn register_plugin(name: &'static str, f: PluginFn) {
    eprintln!("[registry] registering plugin: {}", name);
    REGISTRY.lock().unwrap().push((name, f));
}

pub fn list_plugins() -> Vec<(&'static str, PluginFn)> {
    let plugins = REGISTRY.lock().unwrap().clone();
    eprintln!("[registry] listing {} plugins", plugins.len());
    plugins
}
