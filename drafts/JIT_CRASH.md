# JIT Crash Investigation: macOS ARM64 SIGABRT/SIGSEGV

## Summary

The `test_twitter::test_facet_format_jit_t1_deserialize` test exhibits flaky behavior on macOS ARM64, crashing with various signals (SIGABRT, SIGSEGV, SIGTRAP, SIGKILL). The root cause is related to JIT-compiled code memory management.

## Symptoms

- Test passes ~60% of the time when run in isolation
- Test passes reliably when run with other tests (nextest parallelism)
- Different crash signals observed:
  - **SIGSEGV (signal 11)**: Segmentation fault - accessing invalid memory
  - **SIGABRT (signal 6)**: Abort - often from malloc/free corruption detection
  - **SIGTRAP (signal 5)**: Trace trap - debugger breakpoint or invalid instruction
  - **SIGKILL (signal 9)**: Killed - OS termination (possibly W^X violation)
- Crashes occur during JIT-compiled code execution, often while writing strings with Unicode content

## Architecture Background

### Tier-1 vs Tier-2 JIT

facet-format has two JIT tiers:

| Tier | Description | How it works |
|------|-------------|--------------|
| **T1** | Shape-based, format-agnostic | Compiles against `ParseEvent` stream from any parser |
| **T2** | Format-specific | Compiles against specific format's parser directly |

The flaky test is **T1** - it uses the generic `ParseEvent` interface.

### JIT Compilation Flow

```
Shape (type metadata)
    → Cranelift IR generation (compiler.rs)
    → JITModule::finalize()
    → Function pointer extraction via get_finalized_function()
    → Cached in global HashMap
    → Called during deserialization
```

### Key Insight: JITModule Lifetime

Cranelift's `JITModule` owns the executable memory for compiled functions. When you call:

```rust
let fn_ptr = module.get_finalized_function(func_id);
```

The returned pointer is **only valid while `module` is alive**. If `module` is dropped, the memory is freed and `fn_ptr` becomes a dangling pointer.

## Root Cause Analysis

### Initial Bug (Fixed)

The original `try_compile` function was:

```rust
pub fn try_compile<T>() -> Option<*const u8> {
    let module = JITModule::new(...);
    let func_id = compile_deserializer(&mut module, shape)?;
    module.finalize_definitions()?;
    let fn_ptr = module.get_finalized_function(func_id);
    Some(fn_ptr)  // BUG: module dropped here, fn_ptr now dangling!
}
```

### First Fix: Store JITModule

We created `CachedModule` to keep the module alive:

```rust
pub struct CachedModule {
    module: JITModule,           // Keeps memory alive
    nested_modules: Vec<JITModule>, // Modules for nested structs
    fn_ptr: *const u8,
}
```

And updated `CompiledDeserializer` to hold an `Arc<CachedModule>`:

```rust
pub struct CompiledDeserializer<T, P> {
    fn_ptr: *const u8,
    vtable: ParserVTable,
    _cached: Arc<CachedModule>,  // Prevents module from being dropped
    _phantom: PhantomData<fn(&mut P) -> T>,
}
```

### Remaining Issue: Nested Modules

When compiling a struct like `Twitter`, which contains nested structs like `Status`, `User`, etc., each nested type gets its own `JITModule`. These were being dropped.

The fix collects all nested modules:

```rust
fn compile_deserializer(...) -> Option<(FuncId, Vec<JITModule>)> {
    let mut nested_modules = Vec::new();

    // When compiling nested struct fields:
    if let Some(result) = try_compile_module::<NestedType>() {
        nested_modules.push(result.module);
        nested_modules.extend(result.nested_modules);
    }

    Some((func_id, nested_modules))
}
```

## Current State

After implementing the fix:
- The stress test (`scripts/stress_jit_test.sh`) passed **1500+ iterations** without crash
- However, running via `cargo nextest run -E 'test(jit)'` still shows occasional SIGSEGV

This discrepancy suggests:
1. The module lifetime fix is correct and necessary
2. There may be an additional issue triggered by nextest's parallel execution
3. Possible race condition in the global cache

## Debug Output

The crash typically shows output like:

```
[JIT] next_event: Scalar(Str("一、常に身一つ簡素にして...")) -> writing to 0x16becd240
(test aborted with signal 11: SIGSEGV)
```

This indicates the crash occurs while writing a string value with Unicode content.

## Files Involved

| File | Purpose |
|------|---------|
| `facet-format/src/jit/compiler.rs` | JIT compilation, `CachedModule`, `CompileResult` |
| `facet-format/src/jit/cache.rs` | Global cache of compiled deserializers |
| `facet-format/src/jit/helpers.rs` | Runtime helpers called by JIT code |
| `facet-format/src/jit/mod.rs` | Public API |

## Debugging Tools

### LLDB Script

Created `.lldb_jit_debug`:
```
lldb -s .lldb_jit_debug
```

### Stress Test Script

Created `scripts/stress_jit_test.sh`:
```bash
./scripts/stress_jit_test.sh 1000
```

### Environment Variables

```bash
# macOS malloc debugging
MallocScribble=1 cargo nextest run ...

# Enable Tier-2 diagnostics
FACET_TIER2_DIAG=1 cargo nextest run ...
```

## Hypotheses for Remaining Crashes

1. **Race in Cache Access**: The `RwLock<HashMap>` cache might have a race between read and write locks when multiple tests run in parallel

2. **macOS W^X Protection**: ARM64 macOS has strict Write XOR Execute memory protection. Cranelift handles this, but there may be edge cases

3. **Memory Alignment**: JIT-generated code may have alignment assumptions that aren't always met

4. **Cranelift ABI Issues**: Known issues with Cranelift on macOS ARM64 with certain calling conventions

## Next Steps

1. Add more granular locking around cache operations
2. Verify all nested modules are being collected (recursive structs, enums with struct variants)
3. Check if the issue is specific to certain data patterns (Unicode strings, large structs)
4. Consider using `parking_lot::RwLock` for better performance characteristics
5. Profile memory allocations during JIT compilation

## Related Issues

- Cranelift macOS ARM64 compatibility
- facet-format migration (legacy facet-json → new facet-format-json)
