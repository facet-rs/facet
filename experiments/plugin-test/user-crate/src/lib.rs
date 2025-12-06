// First invoke plugin-a (which registers itself via ctor)
plugin_a::invoke_a!();

// Then invoke plugin-b (which reads the registry)
plugin_b::invoke_b!();
