# Proc-Macro Plugin Registry Experiment

Testing whether two proc-macro crates can share state via a common dylib dependency.

## The Trick

1. **registry** - A `crate-type = "dylib"` crate with a static `Mutex<Vec<Plugin>>`
2. **plugin-a** - A proc-macro that links to registry and registers itself at load time (via `ctor`)
3. **plugin-b** - A proc-macro that links to registry and reads the plugin list

The hypothesis: since both proc-macros link to the same dylib, and dylibs are loaded once by the dynamic linker (ld.so), they should share the same static memory. When rustc loads plugin-a, it registers. When rustc loads plugin-b, it can see plugin-a's registration.

## Why This Matters

If this works, facet could have an extensible macro system:

- `facet-macros` reads a plugin registry during `#[derive(Facet)]`
- `facet-error` registers an error codegen plugin when loaded
- `facet-builder` registers a builder codegen plugin when loaded
- etc.

No double-parsing, no build.rs, no separate crates for types. Just depend on facet-error and it "lights up" error codegen in the Facet derive.

## Testing

```bash
cd user-crate
cargo build 2>&1 | grep -E '\[registry\]|\[plugin'
```

Expected output if it works:
```
[registry] registering plugin: plugin-a
[plugin-b] invoke_b called, checking registry...
[registry] listing 1 plugins
[plugin-b] found plugin: plugin-a -> hello from plugin-a
```

Expected output if it doesn't work:
```
[registry] registering plugin: plugin-a
[plugin-b] invoke_b called, checking registry...
[registry] listing 0 plugins
[plugin-b] NO PLUGINS FOUND - registry not shared!
```
