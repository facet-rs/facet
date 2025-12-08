# Dedicated Field Storage for Builtin Attributes

## Goal

Move `default`, `skip_serializing_if`, `invariants`, and `proxy` from the `attributes` array to dedicated fields on `Field` for O(1) access and cleaner code generation.

## Background

See `drafts/macro-attribute-expansion.md` for the complete macro expansion flow documentation.

Currently these attributes go through a 9-step macro expansion dance and end up as type-erased `ExtensionAttr` in an array. We want direct field access instead.

## Changes

### 1. Add types to `facet-core/src/types/ty/field.rs`

```rust
/// Source of a field's default value
pub enum DefaultSource {
    /// Use the type's Default trait via shape vtable
    FromTrait,
    /// Custom expression wrapped in function
    Custom(DefaultInPlaceFn),
}
```

### 2. Add fields to `Field` struct

```rust
pub struct Field {
    // ... existing fields ...
    
    /// Default value source (None = no default)
    pub default: Option<DefaultSource>,
    
    /// Predicate to conditionally skip serialization
    pub skip_serializing_if: Option<SkipSerializingIfFn>,
    
    /// Type invariant validation function
    pub invariants: Option<InvariantsFn>,
    
    /// Proxy shape for custom ser/de
    pub proxy: Option<&'static Shape>,
}
```

Remove `HAS_DEFAULT` flag from `FieldFlags` (redundant).

### 3. Add `FieldBuilder` methods

```rust
impl FieldBuilder {
    pub const fn default_from_trait(mut self) -> Self
    pub const fn default_custom(mut self, f: DefaultInPlaceFn) -> Self
    pub const fn skip_serializing_if(mut self, f: SkipSerializingIfFn) -> Self
    pub const fn invariants(mut self, f: InvariantsFn) -> Self
    pub const fn proxy(mut self, shape: &'static Shape) -> Self
}
```

### 4. Change `process_struct.rs` codegen

**Key principle: Generate `fn` items, NOT closures.** Functions are cheaper (no capture, known size, better inlining).

**Before:**
```rust
"default" => {
    flags.push(quote! { HAS_DEFAULT });
    let ext_attr = emit_attr_for_field(attr, ...);
    attribute_list.push(quote! { #ext_attr });
}
```

**After:**
```rust
"default" => {
    if attr.args.is_empty() {
        default_value = Some(quote! { 
            Some(DefaultSource::FromTrait)
        });
    } else {
        let expr = parse_default_expr(&attr.args);
        // Generate a fn, not a closure
        let fn_name = format_ident!("__default_{}", field_name);
        extra_items.push(quote! {
            unsafe fn #fn_name(__ptr: #facet_crate::PtrUninit<'_>) -> #facet_crate::PtrMut<'_> {
                __ptr.put(#expr)
            }
        });
        default_value = Some(quote! {
            Some(DefaultSource::Custom(#fn_name))
        });
    }
}

"skip_serializing_if" => {
    // User provides a function name, just use it directly
    let fn_name = parse_fn_name(&attr.args);
    skip_serializing_if_value = Some(quote! { Some(#fn_name) });
}

"invariants" => {
    // User provides a function name, just use it directly  
    let fn_name = parse_fn_name(&attr.args);
    invariants_value = Some(quote! { Some(#fn_name) });
}
```

### 5. Update grammar in `facet/src/lib.rs`

Add `#[storage(field)]` to these variants so the grammar system knows not to generate `__attr!` calls:

```rust
#[storage(field)]
Default(make_t or $ty::default()),

#[storage(field)]
SkipSerializingIf(predicate SkipSerializingIfFn),

#[storage(field)]
Invariants(predicate InvariantsFn),

#[storage(field)]
Proxy(shape_type),
```

## Files to modify

1. `facet-core/src/types/ty/field.rs` - Add DefaultSource enum, new fields, builder methods
2. `facet-core/src/types/vtable.rs` - Ensure fn type aliases exported
3. `facet-macros-impl/src/process_struct.rs` - Generate direct field values
4. `facet/src/lib.rs` - Update grammar annotations, fix Attr impl
5. `facet-reflect/` - Update code that looked up these attrs from array

## Implementation order

1. Add DefaultSource enum and fields to Field
2. Add FieldBuilder methods  
3. Update process_struct.rs to generate field values
4. Update grammar annotations
5. Fix facet-reflect usages
6. Build and test full workspace
