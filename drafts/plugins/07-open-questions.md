# Open Questions

## 1. Scripting Language Syntax

How do we clearly distinguish "meta" (control flow, interpolation) from "output" (generated code)?

See [03-scripting-language.md](03-scripting-language.md) for current exploration.

## 2. How do plugins compose?

If `facet-error` implies `facet-display`, how is that expressed?

Options:
- Script inclusion: one script can invoke another
- Plugin chaining: error's `__facet_invoke!` also invokes display
- Explicit in user code: `#[facet(display, error)]`

## 3. Error messages from scripts

If a script refers to something invalid (wrong field name, missing attr),
where does the error point? Can we get good spans?

## 4. Escaping and edge cases

- What if a doc comment contains template syntax literally?
- What about generics with complex bounds?
- What about `where` clauses?

## 5. Script validation

Can we validate scripts at plugin compile time, before they're used?
(e.g., check that `variant.nonexistent` would fail early)

## 6. Performance: AST Caching

Since scripts are static, `facet-macros` could cache the parsed script AST:

```rust
static SCRIPT_CACHE: Mutex<HashMap<u64, Arc<ScriptAst>>> = ...;

fn get_or_parse_script(script_tokens: TokenStream) -> Arc<ScriptAst> {
    let hash = hash(script_tokens);
    cache.entry(hash).or_insert_with(|| {
        Arc::new(parse_script(script_tokens))
    }).clone()
}
```

This means:
- First invocation of a plugin: parse script, cache AST
- Subsequent invocations: reuse cached AST, only evaluate with new struct data
- Across a crate with 100 error types using `#[facet(error)]`: script parsed once

## 7. What primitives does the script language need?

Looking at examples, the script needs to:

1. **Declare trait impls**: `impl Trait for Self { ... }`
2. **Match on structure**: `match self { variants... }` or `self.field`
3. **Access metadata**: doc comments, attributes, field names, field types
4. **Conditionals**: "if field has attr X" or "if variant is tuple vs struct"
5. **Iterate**: "for each field", "for each variant"
6. **String interpolation**: parse `{field}` in doc comments
7. **Emit code fragments**: the actual `write!(...)` calls, etc.
