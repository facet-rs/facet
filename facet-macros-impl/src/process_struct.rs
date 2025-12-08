//! Struct processing and vtable generation for the Facet derive macro.
//!
//! # Vtable Trait Detection
//!
//! The vtable contains function pointers for various trait implementations (Debug, Clone,
//! PartialEq, etc.). There are three ways these can be populated:
//!
//! ## 1. Derive Detection (Fastest - No Specialization)
//!
//! When `#[derive(Debug, Clone, Facet)]` is used, the macro can see the other derives
//! and knows those traits are implemented. It generates direct function pointers without
//! any specialization overhead.
//!
//! ```ignore
//! #[derive(Debug, Clone, Facet)]  // Debug and Clone detected from derives
//! struct Foo { ... }
//! ```
//!
//! ## 2. Explicit Declaration (No Specialization)
//!
//! For traits that are implemented manually (not via derive), use `#[facet(traits(...))]`:
//!
//! ```ignore
//! #[derive(Facet)]
//! #[facet(traits(Debug, PartialEq))]  // Explicit declaration
//! struct Foo { ... }
//!
//! impl Debug for Foo { ... }  // Manual implementation
//! impl PartialEq for Foo { ... }
//! ```
//!
//! This generates compile-time assertions to verify the traits are actually implemented.
//!
//! ## 3. Auto-Detection (Uses Specialization)
//!
//! For backward compatibility or when you don't want to list traits manually, use
//! `#[facet(auto_traits)]`. This uses the `impls!` macro to detect traits at compile
//! time via specialization tricks:
//!
//! ```ignore
//! #[derive(Debug, Facet)]
//! #[facet(auto_traits)]  // Auto-detect all other traits
//! struct Foo { ... }
//! ```
//!
//! **Note:** Auto-detection is slower to compile because it generates specialization
//! code for each trait. Use derive detection or explicit declaration when possible.
//!
//! ## Layered Resolution
//!
//! For each vtable entry, the macro checks sources in order:
//! 1. Is the trait in `#[derive(...)]`? → Use direct impl
//! 2. Is the trait in `#[facet(traits(...))]`? → Use direct impl
//! 3. Is `#[facet(auto_traits)]` present? → Use `impls!` detection
//! 4. Otherwise → Set to `None`
//!
//! Note: `#[facet(traits(...))]` and `#[facet(auto_traits)]` are mutually exclusive.
//! You can combine derives with either one:
//! ```ignore
//! #[derive(Debug, Clone, Facet)]  // Debug, Clone detected from derives
//! #[facet(traits(Display))]       // Display declared explicitly (manual impl)
//! struct Foo { ... }
//! ```

use quote::{format_ident, quote, quote_spanned};

use super::*;

/// Sources of trait information for vtable generation.
///
/// The vtable generation uses a layered approach:
/// 1. **Derives** - traits detected from `#[derive(...)]` next to `#[derive(Facet)]`
/// 2. **Declared** - traits explicitly listed in `#[facet(traits(...))]`
/// 3. **Implied** - traits implied by other attributes (e.g., `#[facet(default)]` implies Default)
/// 4. **Auto** - if `#[facet(auto_traits)]` is present, use `impls!` for remaining traits
/// 5. **None** - if none of the above apply, emit `None` for that trait
pub(crate) struct TraitSources<'a> {
    /// Traits detected from #[derive(...)] attributes
    pub known_derives: &'a KnownDerives,
    /// Traits explicitly declared via #[facet(traits(...))]
    pub declared_traits: Option<&'a DeclaredTraits>,
    /// Whether to auto-detect remaining traits via specialization
    pub auto_traits: bool,
    /// Whether `#[facet(default)]` is present (implies Default trait)
    pub facet_default: bool,
}

impl<'a> TraitSources<'a> {
    /// Create trait sources from parsed attributes
    pub fn from_attrs(attrs: &'a PAttrs) -> Self {
        Self {
            known_derives: &attrs.known_derives,
            declared_traits: attrs.declared_traits.as_ref(),
            auto_traits: attrs.auto_traits,
            facet_default: attrs.has_builtin("default"),
        }
    }

    /// Check if a trait is known from derives
    fn has_derive(&self, check: impl FnOnce(&KnownDerives) -> bool) -> bool {
        check(self.known_derives)
    }

    /// Check if a trait is explicitly declared
    fn has_declared(&self, check: impl FnOnce(&DeclaredTraits) -> bool) -> bool {
        self.declared_traits.is_some_and(check)
    }

    /// Check if we should use auto-detection for this trait
    fn should_auto(&self) -> bool {
        self.auto_traits
    }
}

/// Generates the vtable for a type based on trait sources.
///
/// Uses a layered approach for each trait:
/// 1. If known from derives → direct impl (no specialization)
/// 2. If explicitly declared → direct impl (no specialization)
/// 3. If auto_traits enabled → use `impls!` macro for detection
/// 4. Otherwise → None
pub(crate) fn gen_vtable(
    facet_crate: &TokenStream,
    type_name_fn: &TokenStream,
    sources: &TraitSources<'_>,
) -> TokenStream {
    // Helper to generate a direct implementation (no specialization)
    let direct_display = quote! {
        Some(|data, f| {
            let data = unsafe { data.get::<Self>() };
            core::fmt::Display::fmt(data, f)
        })
    };
    let direct_debug = quote! {
        Some(|data, f| {
            let data = unsafe { data.get::<Self>() };
            core::fmt::Debug::fmt(data, f)
        })
    };
    let direct_default = quote! {
        Some(|target| unsafe {
            target.put(<Self as core::default::Default>::default())
        })
    };
    let direct_clone = quote! {
        Some(|src, dst| unsafe {
            let src = src.get::<Self>();
            dst.put(<Self as core::clone::Clone>::clone(src))
        })
    };
    let direct_partial_eq = quote! {
        Some(|left, right| {
            let left = unsafe { left.get::<Self>() };
            let right = unsafe { right.get::<Self>() };
            <Self as core::cmp::PartialEq>::eq(left, right)
        })
    };
    let direct_partial_ord = quote! {
        Some(|left, right| {
            let left = unsafe { left.get::<Self>() };
            let right = unsafe { right.get::<Self>() };
            <Self as core::cmp::PartialOrd>::partial_cmp(left, right)
        })
    };
    let direct_ord = quote! {
        Some(|left, right| {
            let left = unsafe { left.get::<Self>() };
            let right = unsafe { right.get::<Self>() };
            <Self as core::cmp::Ord>::cmp(left, right)
        })
    };
    let direct_hash = quote! {
        Some(|value, hasher| {
            let value = unsafe { value.get::<Self>() };
            <Self as core::hash::Hash>::hash(value, hasher)
        })
    };

    // Auto-detection versions using spez
    let auto_display = quote! {
        if #facet_crate::spez::impls!(Self: core::fmt::Display) {
            Some(|data, f| {
                let data = unsafe { data.get::<Self>() };
                use #facet_crate::spez::*;
                (&&Spez(data)).spez_display(f)
            })
        } else {
            None
        }
    };
    let auto_debug = quote! {
        if #facet_crate::spez::impls!(Self: core::fmt::Debug) {
            Some(|data, f| {
                let data = unsafe { data.get::<Self>() };
                use #facet_crate::spez::*;
                (&&Spez(data)).spez_debug(f)
            })
        } else {
            None
        }
    };
    let auto_default = quote! {
        if #facet_crate::spez::impls!(Self: core::default::Default) {
            Some(|target| unsafe {
                use #facet_crate::spez::*;
                (&&SpezEmpty::<Self>::SPEZ).spez_default_in_place(target)
            })
        } else {
            None
        }
    };
    let auto_clone = quote! {
        if #facet_crate::spez::impls!(Self: core::clone::Clone) {
            Some(|src, dst| unsafe {
                use #facet_crate::spez::*;
                let src = src.get::<Self>();
                (&&Spez(src)).spez_clone_into(dst)
            })
        } else {
            None
        }
    };
    let auto_partial_eq = quote! {
        if #facet_crate::spez::impls!(Self: core::cmp::PartialEq) {
            Some(|left, right| {
                let left = unsafe { left.get::<Self>() };
                let right = unsafe { right.get::<Self>() };
                use #facet_crate::spez::*;
                (&&Spez(left)).spez_partial_eq(&&Spez(right))
            })
        } else {
            None
        }
    };
    let auto_partial_ord = quote! {
        if #facet_crate::spez::impls!(Self: core::cmp::PartialOrd) {
            Some(|left, right| {
                let left = unsafe { left.get::<Self>() };
                let right = unsafe { right.get::<Self>() };
                use #facet_crate::spez::*;
                (&&Spez(left)).spez_partial_cmp(&&Spez(right))
            })
        } else {
            None
        }
    };
    let auto_ord = quote! {
        if #facet_crate::spez::impls!(Self: core::cmp::Ord) {
            Some(|left, right| {
                let left = unsafe { left.get::<Self>() };
                let right = unsafe { right.get::<Self>() };
                use #facet_crate::spez::*;
                (&&Spez(left)).spez_cmp(&&Spez(right))
            })
        } else {
            None
        }
    };
    let auto_hash = quote! {
        if #facet_crate::spez::impls!(Self: core::hash::Hash) {
            Some(|value, hasher| {
                let value = unsafe { value.get::<Self>() };
                use #facet_crate::spez::*;
                (&&Spez(value)).spez_hash(&mut { hasher })
            })
        } else {
            None
        }
    };
    let auto_parse = quote! {
        if #facet_crate::spez::impls!(Self: core::str::FromStr) {
            Some(|s, target| {
                use #facet_crate::spez::*;
                unsafe { (&&SpezEmpty::<Self>::SPEZ).spez_parse(s, target) }
            })
        } else {
            None
        }
    };

    // For each trait: derive > declared > auto > none
    // Only emit the builder call if we have a value (not None)

    // Display: no derive exists, so check declared then auto
    let display_call = if sources.has_declared(|d| d.display) {
        quote! { .display_opt(#direct_display) }
    } else if sources.should_auto() {
        quote! { .display_opt(#auto_display) }
    } else {
        quote! {}
    };

    // Debug: check derive, then declared, then auto
    let debug_call = if sources.has_derive(|d| d.debug) || sources.has_declared(|d| d.debug) {
        quote! { .debug_opt(#direct_debug) }
    } else if sources.should_auto() {
        quote! { .debug_opt(#auto_debug) }
    } else {
        quote! {}
    };

    // Default: check derive, then declared, then facet(default), then auto
    // Note: #[facet(default)] implies the type implements Default
    let default_call = if sources.has_derive(|d| d.default)
        || sources.has_declared(|d| d.default)
        || sources.facet_default
    {
        quote! { .default_in_place_opt(#direct_default) }
    } else if sources.should_auto() {
        quote! { .default_in_place_opt(#auto_default) }
    } else {
        quote! {}
    };

    // Clone: check derive (including Copy which implies Clone), then declared, then auto
    let clone_call = if sources.has_derive(|d| d.clone || d.copy)
        || sources.has_declared(|d| d.clone || d.copy)
    {
        quote! { .clone_into_opt(#direct_clone) }
    } else if sources.should_auto() {
        quote! { .clone_into_opt(#auto_clone) }
    } else {
        quote! {}
    };

    // PartialEq: check derive, then declared, then auto
    let partial_eq_call =
        if sources.has_derive(|d| d.partial_eq) || sources.has_declared(|d| d.partial_eq) {
            quote! { .partial_eq_opt(#direct_partial_eq) }
        } else if sources.should_auto() {
            quote! { .partial_eq_opt(#auto_partial_eq) }
        } else {
            quote! {}
        };

    // PartialOrd: check derive, then declared, then auto
    let partial_ord_call =
        if sources.has_derive(|d| d.partial_ord) || sources.has_declared(|d| d.partial_ord) {
            quote! { .partial_ord_opt(#direct_partial_ord) }
        } else if sources.should_auto() {
            quote! { .partial_ord_opt(#auto_partial_ord) }
        } else {
            quote! {}
        };

    // Ord: check derive, then declared, then auto
    let ord_call = if sources.has_derive(|d| d.ord) || sources.has_declared(|d| d.ord) {
        quote! { .ord_opt(#direct_ord) }
    } else if sources.should_auto() {
        quote! { .ord_opt(#auto_ord) }
    } else {
        quote! {}
    };

    // Hash: check derive, then declared, then auto
    let hash_call = if sources.has_derive(|d| d.hash) || sources.has_declared(|d| d.hash) {
        quote! { .hash_opt(#direct_hash) }
    } else if sources.should_auto() {
        quote! { .hash_opt(#auto_hash) }
    } else {
        quote! {}
    };

    // Parse (FromStr): no derive exists, only auto-detect if enabled
    let parse_call = if sources.should_auto() {
        quote! { .parse_opt(#auto_parse) }
    } else {
        quote! {}
    };

    // Marker traits - these set bitflags in MarkerTraits
    // Copy: derive (Copy implies Clone), declared, or auto
    let has_copy =
        sources.has_derive(|d| d.copy) || sources.has_declared(|d| d.copy) || sources.auto_traits;
    // Send: declared or auto (no standard derive for Send)
    let has_send = sources.has_declared(|d| d.send) || sources.auto_traits;
    // Sync: declared or auto (no standard derive for Sync)
    let has_sync = sources.has_declared(|d| d.sync) || sources.auto_traits;
    // Eq: derive (PartialEq + Eq), declared, or auto
    let has_eq =
        sources.has_derive(|d| d.eq) || sources.has_declared(|d| d.eq) || sources.auto_traits;
    // Unpin: declared or auto (no standard derive for Unpin)
    let has_unpin = sources.has_declared(|d| d.unpin) || sources.auto_traits;

    // Build markers expression
    let markers_entry = if has_copy || has_send || has_sync || has_eq || has_unpin {
        // At least one marker trait might be set
        let copy_check = if has_copy
            && sources.auto_traits
            && !sources.has_derive(|d| d.copy)
            && !sources.has_declared(|d| d.copy)
        {
            // Auto-detect Copy
            quote! {
                if #facet_crate::spez::impls!(Self: core::marker::Copy) {
                    markers = markers.with_copy();
                }
            }
        } else if sources.has_derive(|d| d.copy) || sources.has_declared(|d| d.copy) {
            // Directly known to be Copy
            quote! { markers = markers.with_copy(); }
        } else {
            quote! {}
        };

        let send_check = if has_send && sources.auto_traits && !sources.has_declared(|d| d.send) {
            quote! {
                if #facet_crate::spez::impls!(Self: core::marker::Send) {
                    markers = markers.with_send();
                }
            }
        } else if sources.has_declared(|d| d.send) {
            quote! { markers = markers.with_send(); }
        } else {
            quote! {}
        };

        let sync_check = if has_sync && sources.auto_traits && !sources.has_declared(|d| d.sync) {
            quote! {
                if #facet_crate::spez::impls!(Self: core::marker::Sync) {
                    markers = markers.with_sync();
                }
            }
        } else if sources.has_declared(|d| d.sync) {
            quote! { markers = markers.with_sync(); }
        } else {
            quote! {}
        };

        let eq_check = if has_eq
            && sources.auto_traits
            && !sources.has_derive(|d| d.eq)
            && !sources.has_declared(|d| d.eq)
        {
            quote! {
                if #facet_crate::spez::impls!(Self: core::cmp::Eq) {
                    markers = markers.with_eq();
                }
            }
        } else if sources.has_derive(|d| d.eq) || sources.has_declared(|d| d.eq) {
            quote! { markers = markers.with_eq(); }
        } else {
            quote! {}
        };

        let unpin_check = if has_unpin && sources.auto_traits && !sources.has_declared(|d| d.unpin)
        {
            quote! {
                if #facet_crate::spez::impls!(Self: core::marker::Unpin) {
                    markers = markers.with_unpin();
                }
            }
        } else if sources.has_declared(|d| d.unpin) {
            quote! { markers = markers.with_unpin(); }
        } else {
            quote! {}
        };

        quote! {{
            let mut markers = #facet_crate::MarkerTraits::EMPTY;
            #copy_check
            #send_check
            #sync_check
            #eq_check
            #unpin_check
            markers
        }}
    } else {
        quote! { #facet_crate::MarkerTraits::EMPTY }
    };

    quote! {
        #facet_crate::ValueVTable::builder(#type_name_fn)
            .drop_in_place(#facet_crate::ValueVTable::drop_in_place_for::<Self>())
            #display_call
            #debug_call
            #default_call
            #clone_call
            #partial_eq_call
            #partial_ord_call
            #ord_call
            #hash_call
            #parse_call
            .markers(#markers_entry)
            .build()
    }
}

/// Generate trait bounds for static assertions.
/// Returns a TokenStream of bounds like `core::fmt::Debug + core::clone::Clone`
/// that can be used in a where clause.
///
/// `facet_default` is true when `#[facet(default)]` is present, which implies Default.
pub(crate) fn gen_trait_bounds(
    declared: Option<&DeclaredTraits>,
    facet_default: bool,
) -> Option<TokenStream> {
    let mut bounds = Vec::new();

    if let Some(declared) = declared {
        if declared.display {
            bounds.push(quote! { core::fmt::Display });
        }
        if declared.debug {
            bounds.push(quote! { core::fmt::Debug });
        }
        if declared.clone {
            bounds.push(quote! { core::clone::Clone });
        }
        if declared.copy {
            bounds.push(quote! { core::marker::Copy });
        }
        if declared.partial_eq {
            bounds.push(quote! { core::cmp::PartialEq });
        }
        if declared.eq {
            bounds.push(quote! { core::cmp::Eq });
        }
        if declared.partial_ord {
            bounds.push(quote! { core::cmp::PartialOrd });
        }
        if declared.ord {
            bounds.push(quote! { core::cmp::Ord });
        }
        if declared.hash {
            bounds.push(quote! { core::hash::Hash });
        }
        if declared.default {
            bounds.push(quote! { core::default::Default });
        }
        if declared.send {
            bounds.push(quote! { core::marker::Send });
        }
        if declared.sync {
            bounds.push(quote! { core::marker::Sync });
        }
        if declared.unpin {
            bounds.push(quote! { core::marker::Unpin });
        }
    }

    // #[facet(default)] implies Default trait
    if facet_default && !declared.is_some_and(|d| d.default) {
        bounds.push(quote! { core::default::Default });
    }

    if bounds.is_empty() {
        None
    } else {
        Some(quote! { #(#bounds)+* })
    }
}

/// Generates the `::facet::Field` definition `TokenStream` from a `PStructField`.
pub(crate) fn gen_field_from_pfield(
    field: &PStructField,
    struct_name: &Ident,
    bgp: &BoundedGenericParams,
    base_offset: Option<TokenStream>,
    facet_crate: &TokenStream,
) -> TokenStream {
    let field_name_effective = &field.name.effective;
    let field_name_raw = &field.name.raw;
    let field_type = &field.ty;

    let bgp_without_bounds = bgp.display_without_bounds();

    #[cfg(feature = "doc")]
    let doc_lines: Vec<String> = field
        .attrs
        .doc
        .iter()
        .map(|doc| doc.as_str().replace("\\\"", "\""))
        .collect();
    #[cfg(not(feature = "doc"))]
    let doc_lines: Vec<String> = Vec::new();

    // Check if this field is marked as a recursive type
    let is_recursive = field.attrs.has_builtin("recursive_type");

    // Generate the shape expression directly using the field type
    // For opaque fields, wrap in Opaque<T>
    // NOTE: Uses short alias from `use #facet_crate::𝟋::*` in the enclosing const block
    let shape_expr = if field.attrs.has_builtin("opaque") {
        quote! { <#facet_crate::Opaque<#field_type> as 𝟋Fct>::SHAPE }
    } else {
        quote! { <#field_type as 𝟋Fct>::SHAPE }
    };

    // Process attributes, separating flag attrs and field attrs from the attribute slice.
    // Attributes with #[storage(flag)] go into FieldFlags for O(1) access.
    // Attributes with #[storage(field)] go into dedicated Field struct fields.
    // Everything else goes into the attributes slice.
    //
    // Flag attrs: sensitive, flatten, child, skip, skip_serializing, skip_deserializing
    // Field attrs: rename, alias
    // Note: default also sets HAS_DEFAULT flag (handled below)

    let mut flags: Vec<TokenStream> = Vec::new();
    let mut rename_value: Option<TokenStream> = None;
    let mut alias_value: Option<TokenStream> = None;
    let mut attribute_list: Vec<TokenStream> = Vec::new();

    for attr in &field.attrs.facet {
        if attr.is_builtin() {
            let key = attr.key_str();
            match key.as_str() {
                // Flag attrs - set bit in FieldFlags, don't add to attribute_list
                "sensitive" => {
                    flags.push(quote! { 𝟋FF::SENSITIVE });
                }
                "flatten" => {
                    flags.push(quote! { 𝟋FF::FLATTEN });
                }
                "child" => {
                    flags.push(quote! { 𝟋FF::CHILD });
                }
                "skip" => {
                    flags.push(quote! { 𝟋FF::SKIP });
                }
                "skip_serializing" => {
                    flags.push(quote! { 𝟋FF::SKIP_SERIALIZING });
                }
                "skip_deserializing" => {
                    flags.push(quote! { 𝟋FF::SKIP_DESERIALIZING });
                }
                "default" => {
                    // Default sets the HAS_DEFAULT flag AND goes into attributes
                    flags.push(quote! { 𝟋FF::HAS_DEFAULT });
                    let ext_attr =
                        emit_attr_for_field(attr, field_name_raw, field_type, facet_crate);
                    attribute_list.push(quote! { #ext_attr });
                }
                "recursive_type" => {
                    // recursive_type sets a flag
                    flags.push(quote! { 𝟋FF::RECURSIVE_TYPE });
                }
                // Field attrs - store in dedicated field, don't add to attribute_list
                "rename" => {
                    // Extract the string literal from args
                    let args = &attr.args;
                    rename_value = Some(quote! { #args });
                }
                "alias" => {
                    // Extract the string literal from args
                    let args = &attr.args;
                    alias_value = Some(quote! { #args });
                }
                // Everything else goes to attributes slice
                _ => {
                    let ext_attr =
                        emit_attr_for_field(attr, field_name_raw, field_type, facet_crate);
                    attribute_list.push(quote! { #ext_attr });
                }
            }
        } else {
            // Non-builtin (namespaced) attrs always go to attributes slice
            let ext_attr = emit_attr_for_field(attr, field_name_raw, field_type, facet_crate);
            attribute_list.push(quote! { #ext_attr });
        }
    }

    // Generate proxy conversion function pointers when proxy attribute is present
    if let Some(attr) = field
        .attrs
        .facet
        .iter()
        .find(|a| a.is_builtin() && a.key_str() == "proxy")
    {
        let proxy_type = &attr.args;

        // Generate __proxy_in: converts proxy -> field type via TryFrom
        attribute_list.push(quote! {
            #facet_crate::ExtensionAttr {
                ns: ::core::option::Option::None,
                key: "__proxy_in",
                data: &const {
                    extern crate alloc as __alloc;
                    unsafe fn __proxy_convert_in<'mem>(
                        proxy_ptr: #facet_crate::PtrConst<'mem>,
                        field_ptr: #facet_crate::PtrUninit<'mem>,
                    ) -> ::core::result::Result<#facet_crate::PtrMut<'mem>, __alloc::string::String> {
                        let proxy: #proxy_type = proxy_ptr.read();
                        match <#field_type as ::core::convert::TryFrom<#proxy_type>>::try_from(proxy) {
                            ::core::result::Result::Ok(value) => ::core::result::Result::Ok(field_ptr.put(value)),
                            ::core::result::Result::Err(e) => ::core::result::Result::Err(__alloc::string::ToString::to_string(&e)),
                        }
                    }
                    __proxy_convert_in as #facet_crate::ProxyConvertInFn
                } as *const #facet_crate::ProxyConvertInFn as *const (),
                shape: <() as #facet_crate::Facet>::SHAPE,
            }
        });

        // Generate __proxy_out: converts &field type -> proxy via TryFrom
        attribute_list.push(quote! {
            #facet_crate::ExtensionAttr {
                ns: ::core::option::Option::None,
                key: "__proxy_out",
                data: &const {
                    extern crate alloc as __alloc;
                    unsafe fn __proxy_convert_out<'mem>(
                        field_ptr: #facet_crate::PtrConst<'mem>,
                        proxy_ptr: #facet_crate::PtrUninit<'mem>,
                    ) -> ::core::result::Result<#facet_crate::PtrMut<'mem>, __alloc::string::String> {
                        let field_ref: &#field_type = field_ptr.get();
                        match <#proxy_type as ::core::convert::TryFrom<&#field_type>>::try_from(field_ref) {
                            ::core::result::Result::Ok(proxy) => ::core::result::Result::Ok(proxy_ptr.put(proxy)),
                            ::core::result::Result::Err(e) => ::core::result::Result::Err(__alloc::string::ToString::to_string(&e)),
                        }
                    }
                    __proxy_convert_out as #facet_crate::ProxyConvertOutFn
                } as *const #facet_crate::ProxyConvertOutFn as *const (),
                shape: <() as #facet_crate::Facet>::SHAPE,
            }
        });
    }

    let maybe_attributes = if attribute_list.is_empty() {
        quote! { &[] }
    } else {
        quote! { &const {[#(#attribute_list),*]} }
    };

    let maybe_field_doc = if doc_lines.is_empty() {
        quote! { &[] }
    } else {
        quote! { &[#(#doc_lines),*] }
    };

    // Calculate the final offset, incorporating the base_offset if present
    let final_offset = match base_offset {
        Some(base) => {
            quote! { #base + ::core::mem::offset_of!(#struct_name #bgp_without_bounds, #field_name_raw) }
        }
        None => {
            quote! { ::core::mem::offset_of!(#struct_name #bgp_without_bounds, #field_name_raw) }
        }
    };

    // Use FieldBuilder for more compact generated code
    // NOTE: Uses short alias from `use #facet_crate::𝟋::*` in the enclosing const block
    //
    // For most fields, use `new` with a direct shape reference (more efficient).
    // For recursive type fields (marked with #[facet(recursive_type)]), use `new_lazy`
    // with a closure to break cycles.
    let builder = if is_recursive {
        quote! {
            𝟋FldB::new_lazy(
                #field_name_effective,
                || #shape_expr,
                #final_offset,
            )
        }
    } else {
        quote! {
            𝟋FldB::new(
                #field_name_effective,
                #shape_expr,
                #final_offset,
            )
        }
    };

    // Build the chain of builder method calls
    let mut builder_chain = builder;

    // Add flags if any were collected
    if !flags.is_empty() {
        let flags_expr = if flags.len() == 1 {
            let f = &flags[0];
            quote! { #f }
        } else {
            // Union multiple flags together
            let first = &flags[0];
            let rest = &flags[1..];
            quote! { #first #(.union(#rest))* }
        };
        builder_chain = quote! { #builder_chain.flags(#flags_expr) };
    }

    // Add rename if present
    if let Some(rename) = &rename_value {
        builder_chain = quote! { #builder_chain.rename(#rename) };
    }

    // Add alias if present
    if let Some(alias) = &alias_value {
        builder_chain = quote! { #builder_chain.alias(#alias) };
    }

    // Add attributes if any
    if !attribute_list.is_empty() {
        builder_chain = quote! { #builder_chain.attributes(#maybe_attributes) };
    }

    // Add doc if present
    if !doc_lines.is_empty() {
        builder_chain = quote! { #builder_chain.doc(#maybe_field_doc) };
    }

    // Finally call build
    quote! { #builder_chain.build() }
}

/// Processes a regular struct to implement Facet
///
/// Example input:
/// ```rust
/// struct Blah {
///     foo: u32,
///     bar: String,
/// }
/// ```
pub(crate) fn process_struct(parsed: Struct) -> TokenStream {
    let ps = PStruct::parse(&parsed); // Use the parsed representation

    // Emit any collected errors as compile_error! with proper spans
    if !ps.container.attrs.errors.is_empty() {
        let errors = ps.container.attrs.errors.iter().map(|e| {
            let msg = &e.message;
            let span = e.span;
            quote_spanned! { span => compile_error!(#msg); }
        });
        return quote! { #(#errors)* };
    }

    let struct_name_ident = format_ident!("{}", ps.container.name);
    let struct_name = &ps.container.name;
    let struct_name_str = struct_name.to_string();

    let opaque = ps.container.attrs.has_builtin("opaque");

    // Get the facet crate path (custom or default ::facet)
    let facet_crate = ps.container.attrs.facet_crate();

    let type_name_fn =
        generate_type_name_fn(struct_name, parsed.generics.as_ref(), opaque, &facet_crate);

    // Determine trait sources and generate vtable accordingly
    let trait_sources = TraitSources::from_attrs(&ps.container.attrs);
    let vtable_code = gen_vtable(&facet_crate, &type_name_fn, &trait_sources);
    let vtable_init = quote! { const { #vtable_code } };

    // TODO: I assume the `PrimitiveRepr` is only relevant for enums, and does not need to be preserved?
    // NOTE: Uses short aliases from `use #facet_crate::𝟋::*` in the const block
    let repr = match &ps.container.attrs.repr {
        PRepr::Transparent => quote! { 𝟋Repr::TRANSPARENT },
        PRepr::Rust(_) => quote! { 𝟋Repr::RUST },
        PRepr::C(_) => quote! { 𝟋Repr::C },
        PRepr::RustcWillCatch => {
            // rustc will emit an error for the invalid repr.
            // Return empty TokenStream so we don't add misleading errors.
            return quote! {};
        }
    };

    // Use PStruct for kind and fields
    let (kind, fields_vec) = match &ps.kind {
        PStructKind::Struct { fields } => {
            let kind = quote!(𝟋Sk::Struct);
            let fields_vec = fields
                .iter()
                .map(|field| {
                    gen_field_from_pfield(field, struct_name, &ps.container.bgp, None, &facet_crate)
                })
                .collect::<Vec<_>>();
            (kind, fields_vec)
        }
        PStructKind::TupleStruct { fields } => {
            let kind = quote!(𝟋Sk::TupleStruct);
            let fields_vec = fields
                .iter()
                .map(|field| {
                    gen_field_from_pfield(field, struct_name, &ps.container.bgp, None, &facet_crate)
                })
                .collect::<Vec<_>>();
            (kind, fields_vec)
        }
        PStructKind::UnitStruct => {
            let kind = quote!(𝟋Sk::Unit);
            (kind, vec![])
        }
    };

    // Compute variance - delegate to Shape::computed_variance() at runtime
    let variance_call = if opaque {
        // Opaque types don't expose internals, use invariant for safety
        quote! { .variance(#facet_crate::Variance::INVARIANT) }
    } else {
        // Point to Shape::computed_variance - it takes &Shape and walks fields
        quote! {
            .variance(#facet_crate::Shape::computed_variance)
        }
    };

    // Still need original AST for where clauses and type params for build_ helpers
    let where_clauses_ast = match &parsed.kind {
        StructKind::Struct { clauses, .. } => clauses.as_ref(),
        StructKind::TupleStruct { clauses, .. } => clauses.as_ref(),
        StructKind::UnitStruct { clauses, .. } => clauses.as_ref(),
    };
    let where_clauses = build_where_clauses(
        where_clauses_ast,
        parsed.generics.as_ref(),
        opaque,
        &facet_crate,
    );
    let type_params_call = build_type_params_call(parsed.generics.as_ref(), opaque, &facet_crate);

    // Static decl using PStruct BGP
    let static_decl = if ps.container.bgp.params.is_empty() {
        generate_static_decl(struct_name, &facet_crate)
    } else {
        TokenStream::new()
    };

    // Doc comments from PStruct - returns value for struct literal
    // doc call - only emit if there are doc comments and doc feature is enabled
    #[cfg(feature = "doc")]
    let doc_call = if ps.container.attrs.doc.is_empty() {
        quote! {}
    } else {
        let doc_lines = ps.container.attrs.doc.iter().map(|s| quote!(#s));
        quote! { .doc(&[#(#doc_lines),*]) }
    };
    #[cfg(not(feature = "doc"))]
    let doc_call = quote! {};

    // Container attributes - most go through grammar dispatch
    // Filter out `invariants` and `crate` since they're handled specially
    // Returns builder call only if there are attributes
    let attributes_call = {
        let items: Vec<TokenStream> = ps
            .container
            .attrs
            .facet
            .iter()
            .filter(|attr| {
                // These attributes are handled specially and not emitted to runtime:
                // - invariants: populates vtable.invariants
                // - crate: sets the facet crate path
                // - traits: compile-time directive for vtable generation
                // - auto_traits: compile-time directive for vtable generation
                // - proxy: sets Shape::proxy for container-level proxy
                if attr.is_builtin() {
                    let key = attr.key_str();
                    !matches!(
                        key.as_str(),
                        "invariants" | "crate" | "traits" | "auto_traits" | "proxy"
                    )
                } else {
                    true
                }
            })
            .map(|attr| {
                let ext_attr = emit_attr(attr, &facet_crate);
                quote! { #ext_attr }
            })
            .collect();

        if items.is_empty() {
            quote! {}
        } else {
            quote! { .attributes(&const {[#(#items),*]}) }
        }
    };

    // Type tag from PStruct - returns builder call only if present
    let type_tag_call = {
        if let Some(type_tag) = ps.container.attrs.get_builtin_args("type_tag") {
            quote! { .type_tag(#type_tag) }
        } else {
            quote! {}
        }
    };

    // Container-level proxy from PStruct - generates ProxyDef with conversion functions
    //
    // The challenge: Generic type parameters aren't available inside `const { }` blocks.
    // Solution: We define the proxy functions as inherent methods on the type (outside const),
    // then reference them via Self::method inside the Facet impl. This works because:
    // 1. Inherent impl methods CAN use generic parameters from their impl block
    // 2. Inside the Facet impl's const SHAPE, `Self` refers to the concrete monomorphized type
    // 3. Function pointers to Self::method get properly monomorphized
    let (proxy_inherent_impl, proxy_call) = {
        if let Some(attr) = ps
            .container
            .attrs
            .facet
            .iter()
            .find(|a| a.is_builtin() && a.key_str() == "proxy")
        {
            let proxy_type = &attr.args;
            let struct_type = &struct_name_ident;
            let bgp_display = ps.container.bgp.display_without_bounds();
            // Compute bgp locally for the inherent impl
            let helper_bgp = ps
                .container
                .bgp
                .with_lifetime(LifetimeName(format_ident!("ʄ")));
            let bgp_def_for_helper = helper_bgp.display_with_bounds();

            // Define an inherent impl with the proxy helper methods
            // These are NOT in a const block, so generic params ARE available
            // We need where clauses for:
            // 1. The proxy type must implement Facet (for __facet_proxy_shape)
            // 2. The TryFrom conversions (checked when methods are called)
            // Compute the where_clauses for the helper impl by adding the proxy Facet bound
            // Build the combined where clause - we need to add proxy: Facet to existing clauses
            let proxy_where = {
                // Build additional clause tokens (comma-separated)
                let additional_clauses = quote! { #proxy_type: #facet_crate::Facet<'ʄ> };

                // where_clauses is either empty or "where X: Y, ..."
                // We need to append our clause
                if where_clauses.is_empty() {
                    quote! { where #additional_clauses }
                } else {
                    quote! { #where_clauses, #additional_clauses }
                }
            };

            let proxy_impl = quote! {
                #[doc(hidden)]
                impl #bgp_def_for_helper #struct_type #bgp_display
                #proxy_where
                {
                    #[doc(hidden)]
                    unsafe fn __facet_proxy_convert_in<'mem>(
                        proxy_ptr: #facet_crate::PtrConst<'mem>,
                        field_ptr: #facet_crate::PtrUninit<'mem>,
                    ) -> ::core::result::Result<#facet_crate::PtrMut<'mem>, #facet_crate::𝟋::𝟋Str> {
                        extern crate alloc as __alloc;
                        let proxy: #proxy_type = proxy_ptr.read();
                        match <#struct_type #bgp_display as ::core::convert::TryFrom<#proxy_type>>::try_from(proxy) {
                            ::core::result::Result::Ok(value) => ::core::result::Result::Ok(field_ptr.put(value)),
                            ::core::result::Result::Err(e) => ::core::result::Result::Err(__alloc::string::ToString::to_string(&e)),
                        }
                    }

                    #[doc(hidden)]
                    unsafe fn __facet_proxy_convert_out<'mem>(
                        field_ptr: #facet_crate::PtrConst<'mem>,
                        proxy_ptr: #facet_crate::PtrUninit<'mem>,
                    ) -> ::core::result::Result<#facet_crate::PtrMut<'mem>, #facet_crate::𝟋::𝟋Str> {
                        extern crate alloc as __alloc;
                        let field_ref: &#struct_type #bgp_display = field_ptr.get();
                        match <#proxy_type as ::core::convert::TryFrom<&#struct_type #bgp_display>>::try_from(field_ref) {
                            ::core::result::Result::Ok(proxy) => ::core::result::Result::Ok(proxy_ptr.put(proxy)),
                            ::core::result::Result::Err(e) => ::core::result::Result::Err(__alloc::string::ToString::to_string(&e)),
                        }
                    }

                    #[doc(hidden)]
                    const fn __facet_proxy_shape() -> &'static #facet_crate::Shape {
                        <#proxy_type as #facet_crate::Facet>::SHAPE
                    }
                }
            };

            // Reference the inherent methods from within the SHAPE const block
            // Self::method works because Self in the Facet impl refers to the struct type
            let proxy_ref = quote! {
                .proxy(&const {
                    #facet_crate::ProxyDef {
                        shape: Self::__facet_proxy_shape(),
                        convert_in: Self::__facet_proxy_convert_in,
                        convert_out: Self::__facet_proxy_convert_out,
                    }
                })
            };

            (proxy_impl, proxy_ref)
        } else {
            (quote! {}, quote! {})
        }
    };

    // Invariants from PStruct - extract invariant function expressions
    let invariant_maybe = {
        let invariant_exprs: Vec<&TokenStream> = ps
            .container
            .attrs
            .facet
            .iter()
            .filter(|attr| attr.is_builtin() && attr.key_str() == "invariants")
            .map(|attr| &attr.args)
            .collect();

        if !invariant_exprs.is_empty() {
            let tests = invariant_exprs.iter().map(|expr| {
                quote! {
                    if !#expr(value) {
                        return false;
                    }
                }
            });

            let bgp_display = ps.container.bgp.display_without_bounds();
            quote! {
                unsafe fn invariants<'mem>(value: #facet_crate::PtrConst<'mem>) -> bool {
                    let value = value.get::<#struct_name_ident #bgp_display>();
                    #(#tests)*
                    true
                }

                {
                    vtable.invariants = Some(invariants);
                }
            }
        } else {
            quote! {}
        }
    };

    // Transparent logic using PStruct
    let inner_field = if ps.container.attrs.has_builtin("transparent") {
        match &ps.kind {
            PStructKind::TupleStruct { fields } => {
                if fields.len() > 1 {
                    return quote! {
                        compile_error!("Transparent structs must be tuple structs with zero or one field");
                    };
                }
                fields.first().cloned() // Use first field if it exists, None otherwise (ZST case)
            }
            _ => {
                return quote! {
                    compile_error!("Transparent structs must be tuple structs");
                };
            }
        }
    } else {
        None
    };

    // Add try_from_inner implementation for transparent types
    let try_from_inner_code = if ps.container.attrs.has_builtin("transparent") {
        if let Some(inner_field) = &inner_field {
            if !inner_field.attrs.has_builtin("opaque") {
                // Transparent struct with one field
                let inner_field_type = &inner_field.ty;
                let bgp_without_bounds = ps.container.bgp.display_without_bounds();

                quote! {
                    // Define the try_from function for the value vtable
                    unsafe fn try_from<'src, 'dst>(
                        src_ptr: #facet_crate::PtrConst<'src>,
                        src_shape: &'static #facet_crate::Shape,
                        dst: #facet_crate::PtrUninit<'dst>
                    ) -> Result<#facet_crate::PtrMut<'dst>, #facet_crate::TryFromError> {
                        // Try the inner type's try_from function if it exists
                        let inner_result = match <#inner_field_type as #facet_crate::Facet>::SHAPE.vtable.try_from {
                            Some(inner_try) => unsafe { (inner_try)(src_ptr, src_shape, dst) },
                            None => Err(#facet_crate::TryFromError::UnsupportedSourceShape {
                                src_shape,
                                expected: const { &[ &<#inner_field_type as #facet_crate::Facet>::SHAPE ] },
                            })
                        };

                        match inner_result {
                            Ok(result) => Ok(result),
                            Err(_) => {
                                // If inner_try failed, check if source shape is exactly the inner shape
                                if src_shape != <#inner_field_type as #facet_crate::Facet>::SHAPE {
                                    return Err(#facet_crate::TryFromError::UnsupportedSourceShape {
                                        src_shape,
                                        expected: const { &[ &<#inner_field_type as #facet_crate::Facet>::SHAPE ] },
                                    });
                                }
                                // Read the inner value and construct the wrapper.
                                let inner: #inner_field_type = unsafe { src_ptr.read() };
                                Ok(unsafe { dst.put(inner) }) // Construct wrapper
                            }
                        }
                    }

                    // Define the try_into_inner function for the value vtable
                    unsafe fn try_into_inner<'src, 'dst>(
                        src_ptr: #facet_crate::PtrMut<'src>,
                        dst: #facet_crate::PtrUninit<'dst>
                    ) -> Result<#facet_crate::PtrMut<'dst>, #facet_crate::TryIntoInnerError> {
                        let wrapper = unsafe { src_ptr.get::<#struct_name_ident #bgp_without_bounds>() };
                        Ok(unsafe { dst.put(wrapper.0.clone()) }) // Assume tuple struct field 0
                    }

                    // Define the try_borrow_inner function for the value vtable
                    unsafe fn try_borrow_inner<'src>(
                        src_ptr: #facet_crate::PtrConst<'src>
                    ) -> Result<#facet_crate::PtrConst<'src>, #facet_crate::TryBorrowInnerError> {
                        let wrapper = unsafe { src_ptr.get::<#struct_name_ident #bgp_without_bounds>() };
                        // Return a pointer to the inner field (field 0 for tuple struct)
                        Ok(#facet_crate::PtrConst::new(::core::ptr::NonNull::from(&wrapper.0)))
                    }

                    {
                        vtable.try_from = Some(try_from);
                        vtable.try_into_inner = Some(try_into_inner);
                        vtable.try_borrow_inner = Some(try_borrow_inner);
                    }
                }
            } else {
                quote! {} // No try_from can be done for opaque
            }
        } else {
            // Transparent ZST struct (like struct Unit;)
            quote! {
                // Define the try_from function for the value vtable (ZST case)
                unsafe fn try_from<'src, 'dst>(
                    src_ptr: #facet_crate::PtrConst<'src>,
                    src_shape: &'static #facet_crate::Shape,
                    dst: #facet_crate::PtrUninit<'dst>
                ) -> Result<#facet_crate::PtrMut<'dst>, #facet_crate::TryFromError> {
                    if src_shape.layout.size() == 0 {
                         Ok(unsafe { dst.put(#struct_name_ident) }) // Construct ZST
                    } else {
                        Err(#facet_crate::TryFromError::UnsupportedSourceShape {
                            src_shape,
                            expected: const { &[ <() as #facet_crate::Facet>::SHAPE ] }, // Expect unit-like shape
                        })
                    }
                }

                {
                    vtable.try_from = Some(try_from);
                }

                // ZSTs cannot be meaningfully borrowed or converted *into* an inner value
                // try_into_inner and try_borrow_inner remain None
            }
        }
    } else {
        quote! {} // Not transparent
    };

    // Generate the inner shape field value for transparent types
    // inner call - only emit for transparent types
    let inner_call = if ps.container.attrs.has_builtin("transparent") {
        let inner_shape_val = if let Some(inner_field) = &inner_field {
            let ty = &inner_field.ty;
            if inner_field.attrs.has_builtin("opaque") {
                quote! { <#facet_crate::Opaque<#ty> as #facet_crate::Facet>::SHAPE }
            } else {
                quote! { <#ty as #facet_crate::Facet>::SHAPE }
            }
        } else {
            // Transparent ZST case
            quote! { <() as #facet_crate::Facet>::SHAPE }
        };
        quote! { .inner(#inner_shape_val) }
    } else {
        quote! {}
    };

    // Generics from PStruct
    let facet_bgp = ps
        .container
        .bgp
        .with_lifetime(LifetimeName(format_ident!("ʄ")));
    let bgp_def = facet_bgp.display_with_bounds();
    let bgp_without_bounds = ps.container.bgp.display_without_bounds();

    let (ty_field, fields) = if opaque {
        (
            quote! {
                #facet_crate::Type::User(#facet_crate::UserType::Opaque)
            },
            quote! {},
        )
    } else {
        // Optimize: use &[] for empty fields to avoid const block overhead
        if fields_vec.is_empty() {
            (
                quote! {
                    𝟋Ty::User(𝟋UTy::Struct(
                        𝟋STyB::new(#kind, &[]).repr(#repr).build()
                    ))
                },
                quote! {},
            )
        } else {
            // Inline the const block directly into the builder call
            (
                quote! {
                    𝟋Ty::User(𝟋UTy::Struct(
                        𝟋STyB::new(#kind, &const {[#(#fields_vec),*]}).repr(#repr).build()
                    ))
                },
                quote! {},
            )
        }
    };

    // Generate code to suppress dead_code warnings on structs constructed via reflection.
    // When structs are constructed via reflection (e.g., facet_args::from_std_args()),
    // the compiler doesn't see them being used and warns about dead code.
    // This function ensures the struct type is "used" from the compiler's perspective.
    // See: https://github.com/facet-rs/facet/issues/996
    let dead_code_suppression = quote! {
        const _: () = {
            #[allow(dead_code, clippy::multiple_bound_locations)]
            fn __facet_use_struct #bgp_def (__v: &#struct_name_ident #bgp_without_bounds) #where_clauses {
                let _ = __v;
            }
        };
    };

    // Generate static assertions for declared traits (catches lies at compile time)
    // We put this in a generic function outside the const block so it can reference generic parameters
    let facet_default = ps.container.attrs.has_builtin("default");
    let trait_assertion_fn = if let Some(bounds) =
        gen_trait_bounds(ps.container.attrs.declared_traits.as_ref(), facet_default)
    {
        // Note: where_clauses already includes "where" keyword if non-empty
        // We need to add the trait bounds as an additional constraint
        quote! {
            const _: () = {
                #[allow(dead_code, clippy::multiple_bound_locations)]
                fn __facet_assert_traits #bgp_def (_: &#struct_name_ident #bgp_without_bounds)
                where
                    #struct_name_ident #bgp_without_bounds: #bounds
                {}
            };
        }
    } else {
        quote! {}
    };

    // Check if we need vtable mutations (invariants or transparent type functions)
    let has_invariants = ps
        .container
        .attrs
        .facet
        .iter()
        .any(|attr| attr.is_builtin() && attr.key_str() == "invariants");
    let is_transparent = ps.container.attrs.has_builtin("transparent");
    let needs_vtable_mutations = has_invariants || is_transparent;

    // Generate vtable field - use simpler form when no mutations needed
    let vtable_field = if needs_vtable_mutations {
        quote! {
            {
                let mut vtable = #vtable_init;
                #invariant_maybe
                #try_from_inner_code
                vtable
            }
        }
    } else {
        quote! { #vtable_init }
    };

    // Final quote block using refactored parts
    let result = quote! {
        #static_decl

        #dead_code_suppression

        #trait_assertion_fn

        // Proxy inherent impl (outside the Facet impl so generic params are in scope)
        #proxy_inherent_impl

        #[automatically_derived]
        unsafe impl #bgp_def #facet_crate::Facet<'ʄ> for #struct_name_ident #bgp_without_bounds #where_clauses {
            const SHAPE: &'static #facet_crate::Shape = &const {
                use #facet_crate::𝟋::*;
                #fields

                𝟋ShpB::for_sized::<Self>(#type_name_fn, #struct_name_str)
                    .vtable(#vtable_field)
                    .ty(#ty_field)
                    .def(𝟋Def::Undefined)
                    #type_params_call
                    #doc_call
                    #attributes_call
                    #type_tag_call
                    #proxy_call
                    #inner_call
                    #variance_call
                    .build()
            };
        }
    };

    result
}
