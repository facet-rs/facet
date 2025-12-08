# Macro Attribute Expansion Flow

This document traces the complete macro expansion flow for `#[facet(default)]` on a field.

## Starting Point

```rust
#[derive(Facet)]
struct Foo {
    #[facet(default)]
    bar: String,
}
```

## The Complete Chain

```
#[facet(default)]
    ↓
emit_attr_for_field()                    [facet-macros-impl/src/extension.rs]
    ↓
::facet::__attr!(@ns { ::facet::builtin } default { bar : String })
    ↓
define_attr_grammar! { ... }             [facet/src/lib.rs]
    ↓
__make_parse_attr! proc-macro            [facet-macros-impl/src/attr_grammar/make_parse_attr.rs]
    ↓
Generates: macro_rules! __attr! { ... }
    ↓
Final expansion: ExtensionAttr { ns: None, key: "default", data: fn_ptr, shape: ... }
```

---

## Step 1: `#[derive(Facet)]` Entry Point

**File:** `facet-macros/src/lib.rs`

```rust
#[proc_macro_derive(Facet, attributes(facet))]
pub fn facet_macros(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    facet_macros_impl::facet_macros(input.into()).into()
}
```

Delegates to `facet-macros-impl`.

---

## Step 2: Struct Parsing

**File:** `facet-macros-impl/src/derive.rs`

```rust
pub fn facet_macros(input: TokenStream) -> TokenStream {
    let mut i = input.clone().to_token_iter();
    match i.parse::<Cons<AdtDecl, EndOfStream>>() {
        Ok(it) => match it.first {
            AdtDecl::Struct(parsed) => process_struct::process_struct(parsed),
            AdtDecl::Enum(parsed) => process_enum::process_enum(parsed),
        },
        // ...
    }
}
```

The field's `#[facet(default)]` is parsed as `PFacetAttr`:
- `ns: None` (no namespace = builtin)
- `key: "default"`
- `args: TokenStream::new()` (empty)

---

## Step 3: Emit Attribute Invocation

**File:** `facet-macros-impl/src/extension.rs`

```rust
pub fn emit_attr_for_field(
    attr: &PFacetAttr,
    field_name: &impl ToTokens,
    field_type: &TokenStream,
    facet_crate: &TokenStream,
) -> TokenStream {
    let key = &attr.key;  // "default"
    let args = &attr.args;  // empty

    match &attr.ns {
        None => {
            // Builtin: route to ::facet::__attr!
            if args.is_empty() {
                quote! {
                    #facet_crate::__attr!(@ns { #facet_crate::builtin } #key { #field_name : #field_type })
                }
            } else {
                quote! {
                    #facet_crate::__attr!(@ns { #facet_crate::builtin } #key { #field_name : #field_type | #args })
                }
            }
        }
        Some(ns) => {
            // Namespaced: route to the namespace's __attr! macro
            // ...
        }
    }
}
```

**Generates:**
```rust
::facet::__attr!(@ns { ::facet::builtin } default { bar : String })
```

---

## Step 4: Grammar Definition

**File:** `facet/src/lib.rs`

```rust
pub mod builtin {
    crate::define_attr_grammar! {
        builtin;
        ns "";
        crate_path ::facet::builtin;

        pub enum Attr {
            // ...
            Default(make_t or $ty::default()),
            // ...
        }
    }
}
```

The `define_attr_grammar!` macro:

```rust
#[macro_export]
macro_rules! define_attr_grammar {
    ($($grammar:tt)*) => {
        $crate::__make_parse_attr! { $($grammar)* }
    };
}
```

This calls the `__make_parse_attr` proc-macro.

---

## Step 5: Grammar Compilation

**File:** `facet-macros-impl/src/attr_grammar/make_parse_attr.rs`

The proc-macro parses the DSL. For `Default(make_t or $ty::default())`:

```rust
// Check for make_t - expression that "makes a T", wrapped in closure
{
    let token_stream: TokenStream2 = tokens.iter().cloned().collect();
    let mut iter = token_stream.to_token_iter();
    if let Ok(make_t) = iter.parse::<MakeTPayload>() {
        if iter.next().is_none() {
            let use_ty_default_fallback = make_t.fallback.is_some();
            return Ok(VariantKind::MakeT {
                use_ty_default_fallback,
            });
        }
    }
}
```

So `Default(make_t or $ty::default())` becomes:
- **Variant name:** `Default`
- **Kind:** `MakeT { use_ty_default_fallback: true }`

---

## Step 6: `__attr!` Macro Generation

**File:** `facet-macros-impl/src/attr_grammar/make_parse_attr.rs`

For `MakeT` variant with `use_ty_default_fallback = true`:

```rust
VariantKind::MakeT { use_ty_default_fallback } => {
    let no_args_body = if *use_ty_default_fallback {
        quote! {
            ::facet::ExtensionAttr {
                ns: #ns_expr,
                key: #key_str,
                data: &const {
                    ::core::option::Option::Some(
                        (|__ptr: ::facet::PtrUninit<'_>| unsafe {
                            __ptr.put(<$ty as ::core::default::Default>::default())
                        }) as ::facet::DefaultInPlaceFn
                    )
                } as *const ::core::option::Option<::facet::DefaultInPlaceFn> as *const (),
                shape: <() as ::facet::Facet>::SHAPE,
            }
        }
    } else { /* ... */ };

    quote! {
        // Field-level: no args
        (@ns { $ns:path } #key_ident { $field:tt : $ty:ty }) => {{
            #no_args_body
        }};
        // Field-level with `= expr`
        (@ns { $ns:path } #key_ident { $field:tt : $ty:ty | = $expr:expr }) => {{
            ::facet::ExtensionAttr {
                ns: #ns_expr,
                key: #key_str,
                data: &const {
                    ::core::option::Option::Some(
                        (|__ptr: ::facet::PtrUninit<'_>| unsafe { __ptr.put($expr) })
                            as ::facet::DefaultInPlaceFn
                    )
                } as *const ::core::option::Option<::facet::DefaultInPlaceFn> as *const (),
                shape: <() as ::facet::Facet>::SHAPE,
            }
        }};
    }
}
```

---

## Step 7: Generated `__attr!` Macro

The proc-macro generates this `macro_rules!`:

```rust
#[macro_export]
macro_rules! __attr {
    // Default, no args: uses $ty::default()
    (@ns { $ns:path } default { $field:tt : $ty:ty }) => {{
        ::facet::ExtensionAttr {
            ns: ::core::option::Option::None,
            key: "default",
            data: &const {
                ::core::option::Option::Some(
                    (|__ptr: ::facet::PtrUninit<'_>| unsafe {
                        __ptr.put(<$ty as ::core::default::Default>::default())
                    }) as ::facet::DefaultInPlaceFn
                )
            } as *const ::core::option::Option<::facet::DefaultInPlaceFn> as *const (),
            shape: <() as ::facet::Facet>::SHAPE,
        }
    }};

    // Default with explicit value
    (@ns { $ns:path } default { $field:tt : $ty:ty | = $expr:expr }) => {{
        ::facet::ExtensionAttr {
            ns: ::core::option::Option::None,
            key: "default",
            data: &const {
                ::core::option::Option::Some(
                    (|__ptr: ::facet::PtrUninit<'_>| unsafe { __ptr.put($expr) })
                        as ::facet::DefaultInPlaceFn
                )
            } as *const ::core::option::Option<::facet::DefaultInPlaceFn> as *const (),
            shape: <() as ::facet::Facet>::SHAPE,
        }
    }};

    // Unknown attribute
    (@ns { $ns:path } $unknown:ident $($tt:tt)*) => {
        ::facet::__attr_error!(...)
    };
}
```

---

## Step 8: Macro Invocation

When the derive macro's generated code contains:
```rust
::facet::__attr!(@ns { ::facet::builtin } default { bar : String })
```

It matches the first arm with:
- `$ns:path` = `::facet::builtin`
- `$field:tt` = `bar`
- `$ty:ty` = `String`

---

## Step 9: Final Expansion

```rust
::facet::ExtensionAttr {
    ns: ::core::option::Option::None,
    key: "default",
    data: &const {
        ::core::option::Option::Some(
            (|__ptr: ::facet::PtrUninit<'_>| unsafe {
                __ptr.put(<String as ::core::default::Default>::default())
            }) as ::facet::DefaultInPlaceFn
        )
    } as *const ::core::option::Option<::facet::DefaultInPlaceFn> as *const (),
    shape: <() as ::facet::Facet>::SHAPE,
}
```

This creates an `ExtensionAttr` where:
- `ns` = `None` (builtin attribute)
- `key` = `"default"`
- `data` = type-erased pointer to `Option<DefaultInPlaceFn>`
- `shape` = `()` shape (meaningless placeholder)

The function pointer, when called, will:
1. Take a `PtrUninit<'_>` (pointer to uninitialized memory)
2. Call `String::default()` to get an empty string
3. Write it to the pointer via `ptr.put()`
4. Return the now-initialized pointer

---

## Problems with Current Design

1. **Type erasure via raw pointer cast**: The `Option<DefaultInPlaceFn>` is cast to `*const ()` losing type safety

2. **Fake shape**: The `shape` field is set to `<() as Facet>::SHAPE` which is meaningless - it doesn't represent the actual type of the data

3. **O(n) lookup at runtime**: The `ExtensionAttr` goes into the `attributes` array, requiring linear search to find

4. **Redundant storage**: We store both `HAS_DEFAULT` flag (O(1)) AND the attr in the array (O(n))

## Desired Design

Move `default`, `skip_serializing_if`, `invariants`, and `proxy` to dedicated fields on `Field`:

```rust
pub struct Field {
    // ... existing fields ...
    
    /// Function to create default value in-place
    pub default: Option<DefaultInPlaceFn>,
    
    /// Predicate to skip serialization
    pub skip_serializing_if: Option<SkipSerializingIfFn>,
    
    /// Invariant validation function  
    pub invariants: Option<InvariantsFn>,
    
    /// Proxy shape for ser/de
    pub proxy: Option<&'static Shape>,
}
```

This gives O(1) access to both existence check and the actual value.
