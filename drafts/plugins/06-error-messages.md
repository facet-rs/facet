# Better Error Messages via `#[diagnostic::on_unimplemented]`

When a user writes `#[facet(error)]` but hasn't added `facet-error` to their dependencies,
we want a helpful error message, not rustc's generic "cannot find macro" error.

## The Trick: Scope Shadowing + on_unimplemented

We can achieve this using scope shadowing and the `on_unimplemented` diagnostic attribute
([reference](https://doc.rust-lang.org/reference/attributes/diagnostics.html#r-attributes.diagnostic.on_unimplemented)).

### Step 1: Define a marker trait in facet-core

```rust
// In facet-core (always available)
#[diagnostic::on_unimplemented(
    message = "Plugin `{Self}` not found",
    label = "add `{Self}` to your Cargo.toml dependencies",
    note = "facet plugins must be explicitly added as dependencies"
)]
pub trait FacetPlugin {
    // Marker type that plugins export
    type Marker;
}
```

### Step 2: Each plugin exports a marker

```rust
// In facet-error
pub struct PluginMarker;

impl facet::FacetPlugin for PluginMarker {
    type Marker = ();
}
```

### Step 3: derive(Facet) generates validation code

```rust
{
    // This block validates the plugin exists before invoking it
    struct ErrorPluginCheck;

    // If facet-error is in scope, this import succeeds and shadows our struct
    #[allow(unused_imports)]
    use ::facet_error::PluginMarker as ErrorPluginCheck;

    // This trait bound fails with our custom message if the plugin isn't found
    const _: () = {
        fn check<T: ::facet::FacetPlugin>() {}
        check::<ErrorPluginCheck>();
    };
}

// Now safe to invoke the plugin
::facet_error::__facet_invoke! { ... }
```

## Result

If `facet-error` isn't in dependencies, the user sees:

```
error[E0277]: Plugin `ErrorPluginCheck` not found
  --> src/main.rs:3:10
   |
3  | #[derive(Facet)]
   |          ^^^^^ add `facet-error` to your Cargo.toml dependencies
   |
   = note: facet plugins must be explicitly added as dependencies
```

Much better than "cannot find crate `facet_error`"!
