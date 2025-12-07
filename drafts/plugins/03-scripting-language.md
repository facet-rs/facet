# Scripting Language

**Status: Work in Progress**

The script/template language is how plugins express what code to generate.

## Requirements

The script needs to:

1. **Declare trait impls**: `impl Trait for Self { ... }`
2. **Match on structure**: `match self { variants... }` or `self.field`
3. **Access metadata**: doc comments, attributes, field names, field types
4. **Conditionals**: "if field has attr X" or "if variant is tuple vs struct"
5. **Iterate**: "for each field", "for each variant"
6. **String interpolation**: parse `{field}` in doc comments
7. **Emit code fragments**: the actual `write!(...)` calls, etc.

## Design Constraint: Tokens, Not Strings

The script should be passed as **tokens**, not as a raw string. This gives us:

- Better span information for errors
- No string escaping issues
- Parseable with unsynn

## Syntax Exploration

### Option A: quote-style with explicit control flow

Use `#name` for interpolation (like `quote!`), `@for`/`@if` for control flow:

```rust
@plugins {
    error => {
        impl ::core::fmt::Display for #Self {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                match self {
                    @for variant in variants {
                        Self::#variant.name #variant.pattern => write!(f, #variant.doc),
                    }
                }
            }
        }

        impl ::std::error::Error for #Self {
            fn source(&self) -> Option<&(dyn ::std::error::Error + 'static)> {
                match self {
                    @for variant in variants {
                        @if variant.has_attr("from") {
                            Self::#variant.name(e) => Some(e),
                        }
                    }
                    _ => None,
                }
            }
        }

        @for variant in variants {
            @if variant.has_attr("from") {
                impl From<#variant.fields[0].ty> for #Self {
                    fn from(source: #variant.fields[0].ty) -> Self {
                        Self::#variant.name(source)
                    }
                }
            }
        }
    }
}
```

**Pros:**
- Familiar `#` interpolation from quote
- Explicit control flow is readable

**Cons:**
- Hard to tell what's "meta" (script) vs "output" (generated code)
- `@for` and `@if` look like valid Rust (could confuse)

### Option B: ???

(More options to explore)

## Available Variables

Whatever syntax we choose, scripts have access to:

- `#Self` — the type name (with generics)
- `#self_name` — the type name (without generics, for patterns)
- `variants` — list of enum variants (empty for structs)
- `fields` — list of struct fields (for structs)
- `variant.name` — variant name
- `variant.doc` — variant doc comment
- `variant.pattern` — destructuring pattern like `{ field1, field2 }` or `(v0, v1)`
- `variant.fields` — fields of this variant
- `field.name` — field name (or index for tuple)
- `field.ty` — field type
- `field.doc` — field doc comment
- `field.has_attr("x")` — check for `#[facet(x)]` attribute
- `field.attr("x")` — get value of `#[facet(x = value)]`

## Open Questions

1. How do we clearly distinguish meta (control flow) from output?
2. Should we use a different sigil for control flow? (`%for`? `$for`?)
3. How do we handle filters/transformations (e.g., doc → format string)?
4. Can we validate scripts at plugin compile time?
