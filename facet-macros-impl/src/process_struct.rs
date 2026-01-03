//! Struct processing and vtable generation for the Facet derive macro.
//!
//! # Vtable Trait Detection
//!
//! The vtable contains function pointers for various trait implementations (Debug, Clone,
//! PartialEq, etc.). There are two ways these can be populated:
//!
//! ## 1. Explicit Declaration (No Specialization)
//!
//! Use `#[facet(traits(...))]` to declare which traits are implemented:
//!
//! ```ignore
//! #[derive(Debug, Clone, Facet)]
//! #[facet(traits(Debug, Clone))]  // Explicit declaration
//! struct Foo { ... }
//! ```
//!
//! This generates compile-time assertions to verify the traits are actually implemented.
//!
//! **Note:** Rust strips `#[derive(...)]` attributes before passing to derive macros,
//! so the Facet macro cannot automatically detect derived traits. You must explicitly
//! declare them via `#[facet(traits(...))]`.
//!
//! ## 2. Auto-Detection (Uses Specialization)
//!
//! For backward compatibility or when you don't want to list traits manually, use
//! `#[facet(auto_traits)]`. This uses the `impls!` macro to detect traits at compile
//! time via specialization tricks:
//!
//! ```ignore
//! #[derive(Debug, Facet)]
//! #[facet(auto_traits)]  // Auto-detect all traits
//! struct Foo { ... }
//! ```
//!
//! **Note:** Auto-detection is slower to compile because it generates specialization
//! code for each trait. Use explicit declaration when possible.
//!
//! ## Layered Resolution
//!
//! For each vtable entry, the macro checks sources in order:
//! 1. Is the trait in `#[facet(traits(...))]`? → Use direct impl
//! 2. Is `#[facet(auto_traits)]` present? → Use `impls!` detection
//! 3. Otherwise → Set to `None`
//!
//! Note: `#[facet(traits(...))]` and `#[facet(auto_traits)]` are mutually exclusive.

use quote::{format_ident, quote, quote_spanned};

use super::*;

/// Information about transparent type for vtable generation.
///
/// This is used to generate `try_borrow_inner` functions for transparent/newtype wrappers.
pub(crate) struct TransparentInfo<'a> {
    /// The inner field type (for tuple struct with one field)
    pub inner_field_type: Option<&'a TokenStream>,
    /// Whether the inner field is opaque
    pub inner_is_opaque: bool,
    /// Whether this is a ZST (unit-like transparent struct)
    pub is_zst: bool,
}

/// Sources of trait information for vtable generation.
///
/// The vtable generation uses a layered approach:
/// 1. **Declared** - traits explicitly listed in `#[facet(traits(...))]`
/// 2. **Implied** - traits implied by other attributes (e.g., `#[facet(default)]` implies Default)
/// 3. **Auto** - if `#[facet(auto_traits)]` is present, use `impls!` for remaining traits
/// 4. **None** - if none of the above apply, emit `None` for that trait
pub(crate) struct TraitSources<'a> {
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
            declared_traits: attrs.declared_traits.as_ref(),
            auto_traits: attrs.auto_traits,
            facet_default: attrs.has_builtin("default"),
        }
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
/// 1. If explicitly declared → direct impl (no specialization)
/// 2. If auto_traits enabled → use `impls!` macro for detection
/// 3. Otherwise → None
///
/// When `auto_traits` is NOT enabled, generates `ValueVTableThinInstant` using
/// helper functions like `debug_for::<Self>()`. This avoids closures that would
/// require `T: 'static` bounds.
///
/// When `auto_traits` IS enabled, falls back to `ValueVTable::builder()` pattern
/// (ThinDelayed) which uses closures for runtime trait detection.
pub(crate) fn gen_vtable(
    facet_crate: &TokenStream,
    type_name_fn: &TokenStream,
    sources: &TraitSources<'_>,
    transparent: Option<&TransparentInfo<'_>>,
    struct_type: &TokenStream,
    invariants_fn: Option<&TokenStream>,
) -> TokenStream {
    // If auto_traits is enabled, use VTableIndirect with runtime trait detection.
    if sources.auto_traits {
        return gen_vtable_indirect(
            facet_crate,
            type_name_fn,
            sources,
            struct_type,
            invariants_fn,
        );
    }

    // Otherwise, use VTableDirect with compile-time trait resolution.
    gen_vtable_direct(
        facet_crate,
        type_name_fn,
        sources,
        transparent,
        struct_type,
        invariants_fn,
    )
}

/// Generates a VTableDirect using direct trait method references.
/// This avoids closures and uses the pattern from vtable_direct! macro.
/// Uses `Self` inside the const block, which properly resolves to the implementing type
/// without requiring that lifetime parameters outlive 'static.
fn gen_vtable_direct(
    facet_crate: &TokenStream,
    _type_name_fn: &TokenStream,
    sources: &TraitSources<'_>,
    transparent: Option<&TransparentInfo<'_>>,
    struct_type: &TokenStream,
    invariants_fn: Option<&TokenStream>,
) -> TokenStream {
    // Display: check declared
    let display_call = if sources.has_declared(|d| d.display) {
        quote! { .display(<Self as ::core::fmt::Display>::fmt) }
    } else {
        quote! {}
    };

    // Debug: check declared
    let debug_call = if sources.has_declared(|d| d.debug) {
        quote! { .debug(<Self as ::core::fmt::Debug>::fmt) }
    } else {
        quote! {}
    };

    // PartialEq: check declared
    let partial_eq_call = if sources.has_declared(|d| d.partial_eq) {
        quote! { .partial_eq(<Self as ::core::cmp::PartialEq>::eq) }
    } else {
        quote! {}
    };

    // PartialOrd: check declared
    let partial_ord_call = if sources.has_declared(|d| d.partial_ord) {
        quote! { .partial_cmp(<Self as ::core::cmp::PartialOrd>::partial_cmp) }
    } else {
        quote! {}
    };

    // Ord: check declared
    let ord_call = if sources.has_declared(|d| d.ord) {
        quote! { .cmp(<Self as ::core::cmp::Ord>::cmp) }
    } else {
        quote! {}
    };

    // Hash: check declared
    let hash_call = if sources.has_declared(|d| d.hash) {
        quote! { .hash(<Self as ::core::hash::Hash>::hash::<#facet_crate::HashProxy>) }
    } else {
        quote! {}
    };

    // Transparent type functions: try_borrow_inner
    // For transparent types (newtypes), we generate a function to borrow the inner value
    // Note: We still need the concrete struct_type here because we're dealing with field access
    let try_borrow_inner_call = if let Some(info) = transparent {
        if info.inner_is_opaque {
            // Opaque inner field - no borrow possible
            quote! {}
        } else if info.is_zst {
            // ZST case - no inner value to borrow
            quote! {}
        } else if let Some(inner_ty) = info.inner_field_type {
            // Transparent struct with one field - generate try_borrow_inner
            // The function signature for VTableDirect is: unsafe fn(*const T) -> Result<Ptr, String>
            quote! {
                .try_borrow_inner({
                    unsafe fn __try_borrow_inner(src: *const #struct_type) -> ::core::result::Result<#facet_crate::PtrMut, #facet_crate::𝟋::𝟋Str> {
                        // src points to the wrapper (tuple struct), field 0 is the inner value
                        // We cast away const because try_borrow_inner returns PtrMut for flexibility
                        // (caller can downgrade to PtrConst if needed)
                        let wrapper_ptr = src as *mut #struct_type;
                        let inner_ptr: *mut #inner_ty = unsafe { &raw mut (*wrapper_ptr).0 };
                        ::core::result::Result::Ok(#facet_crate::PtrMut::new(inner_ptr as *mut u8))
                    }
                    __try_borrow_inner
                })
            }
        } else {
            quote! {}
        }
    } else {
        quote! {}
    };

    // Invariants: container-level invariants function
    let invariants_call = if let Some(inv_fn) = invariants_fn {
        quote! { .invariants(#inv_fn) }
    } else {
        quote! {}
    };

    // Generate VTableErased::Direct with a static VTableDirect
    // Uses prelude aliases for compact output (𝟋VtE, 𝟋VtD)
    // NOTE: drop_in_place, default_in_place, clone_into are now in TypeOps, not VTable
    quote! {
        𝟋VtE::Direct(&const {
            𝟋VtD::builder_for::<Self>()
                #display_call
                #debug_call
                #partial_eq_call
                #partial_ord_call
                #ord_call
                #hash_call
                #invariants_call
                #try_borrow_inner_call
                .build()
        })
    }
}

/// Generates a VTableIndirect using the specialization-based auto_traits approach.
/// Used when `#[facet(auto_traits)]` is enabled for runtime trait detection.
///
/// This generates functions that use `OxRef`/`OxMut` and return `Option<T>` to indicate
/// whether the trait is implemented.
fn gen_vtable_indirect(
    facet_crate: &TokenStream,
    _type_name_fn: &TokenStream,
    sources: &TraitSources<'_>,
    struct_type: &TokenStream,
    invariants_fn: Option<&TokenStream>,
) -> TokenStream {
    // For VTableIndirect, functions take OxRef/OxMut and return Option<T>
    // The Option layer allows returning None when trait is not implemented

    // Display: check declared then auto
    let display_field = if sources.has_declared(|d| d.display) {
        quote! {
            display: ::core::option::Option::Some({
                unsafe fn __display(data: #facet_crate::OxPtrConst, f: &mut ::core::fmt::Formatter<'_>) -> ::core::option::Option<::core::fmt::Result> {
                    let data: &#struct_type = data.ptr().get();
                    ::core::option::Option::Some(::core::fmt::Display::fmt(data, f))
                }
                __display
            }),
        }
    } else if sources.should_auto() {
        quote! {
            display: ::core::option::Option::Some({
                unsafe fn __display(data: #facet_crate::OxPtrConst, f: &mut ::core::fmt::Formatter<'_>) -> ::core::option::Option<::core::fmt::Result> {
                    if impls!(#struct_type: ::core::fmt::Display) {
                        let data: &#struct_type = data.ptr().get();
                        ::core::option::Option::Some((&&Spez(data)).spez_display(f))
                    } else {
                        ::core::option::Option::None
                    }
                }
                __display
            }),
        }
    } else {
        quote! { display: ::core::option::Option::None, }
    };

    // Debug: check declared, then auto
    let debug_field = if sources.has_declared(|d| d.debug) {
        quote! {
            debug: ::core::option::Option::Some({
                unsafe fn __debug(data: #facet_crate::OxPtrConst, f: &mut ::core::fmt::Formatter<'_>) -> ::core::option::Option<::core::fmt::Result> {
                    let data: &#struct_type = data.ptr().get();
                    ::core::option::Option::Some(::core::fmt::Debug::fmt(data, f))
                }
                __debug
            }),
        }
    } else if sources.should_auto() {
        quote! {
            debug: ::core::option::Option::Some({
                unsafe fn __debug(data: #facet_crate::OxPtrConst, f: &mut ::core::fmt::Formatter<'_>) -> ::core::option::Option<::core::fmt::Result> {
                    if impls!(#struct_type: ::core::fmt::Debug) {
                        let data: &#struct_type = data.ptr().get();
                        ::core::option::Option::Some((&&Spez(data)).spez_debug(f))
                    } else {
                        ::core::option::Option::None
                    }
                }
                __debug
            }),
        }
    } else {
        quote! { debug: ::core::option::Option::None, }
    };

    // PartialEq: check declared, then auto
    let partial_eq_field = if sources.has_declared(|d| d.partial_eq) {
        quote! {
            partial_eq: ::core::option::Option::Some({
                unsafe fn __partial_eq(left: #facet_crate::OxPtrConst, right: #facet_crate::OxPtrConst) -> ::core::option::Option<bool> {
                    let left: &#struct_type = left.ptr().get();
                    let right: &#struct_type = right.ptr().get();
                    ::core::option::Option::Some(<#struct_type as ::core::cmp::PartialEq>::eq(left, right))
                }
                __partial_eq
            }),
        }
    } else if sources.should_auto() {
        quote! {
            partial_eq: ::core::option::Option::Some({
                unsafe fn __partial_eq(left: #facet_crate::OxPtrConst, right: #facet_crate::OxPtrConst) -> ::core::option::Option<bool> {
                    if impls!(#struct_type: ::core::cmp::PartialEq) {
                        let left: &#struct_type = left.ptr().get();
                        let right: &#struct_type = right.ptr().get();
                        ::core::option::Option::Some((&&Spez(left)).spez_partial_eq(&&Spez(right)))
                    } else {
                        ::core::option::Option::None
                    }
                }
                __partial_eq
            }),
        }
    } else {
        quote! { partial_eq: ::core::option::Option::None, }
    };

    // PartialOrd: check declared, then auto
    let partial_cmp_field = if sources.has_declared(|d| d.partial_ord) {
        quote! {
            partial_cmp: ::core::option::Option::Some({
                unsafe fn __partial_cmp(left: #facet_crate::OxPtrConst, right: #facet_crate::OxPtrConst) -> ::core::option::Option<::core::option::Option<::core::cmp::Ordering>> {
                    let left: &#struct_type = left.ptr().get();
                    let right: &#struct_type = right.ptr().get();
                    ::core::option::Option::Some(<#struct_type as ::core::cmp::PartialOrd>::partial_cmp(left, right))
                }
                __partial_cmp
            }),
        }
    } else if sources.should_auto() {
        quote! {
            partial_cmp: ::core::option::Option::Some({
                unsafe fn __partial_cmp(left: #facet_crate::OxPtrConst, right: #facet_crate::OxPtrConst) -> ::core::option::Option<::core::option::Option<::core::cmp::Ordering>> {
                    if impls!(#struct_type: ::core::cmp::PartialOrd) {
                        let left: &#struct_type = left.ptr().get();
                        let right: &#struct_type = right.ptr().get();
                        ::core::option::Option::Some((&&Spez(left)).spez_partial_cmp(&&Spez(right)))
                    } else {
                        ::core::option::Option::None
                    }
                }
                __partial_cmp
            }),
        }
    } else {
        quote! { partial_cmp: ::core::option::Option::None, }
    };

    // Ord: check declared, then auto
    let cmp_field = if sources.has_declared(|d| d.ord) {
        quote! {
            cmp: ::core::option::Option::Some({
                unsafe fn __cmp(left: #facet_crate::OxPtrConst, right: #facet_crate::OxPtrConst) -> ::core::option::Option<::core::cmp::Ordering> {
                    let left: &#struct_type = left.ptr().get();
                    let right: &#struct_type = right.ptr().get();
                    ::core::option::Option::Some(<#struct_type as ::core::cmp::Ord>::cmp(left, right))
                }
                __cmp
            }),
        }
    } else if sources.should_auto() {
        quote! {
            cmp: ::core::option::Option::Some({
                unsafe fn __cmp(left: #facet_crate::OxPtrConst, right: #facet_crate::OxPtrConst) -> ::core::option::Option<::core::cmp::Ordering> {
                    if impls!(#struct_type: ::core::cmp::Ord) {
                        let left: &#struct_type = left.ptr().get();
                        let right: &#struct_type = right.ptr().get();
                        ::core::option::Option::Some((&&Spez(left)).spez_cmp(&&Spez(right)))
                    } else {
                        ::core::option::Option::None
                    }
                }
                __cmp
            }),
        }
    } else {
        quote! { cmp: ::core::option::Option::None, }
    };

    // Hash: check declared, then auto
    let hash_field = if sources.has_declared(|d| d.hash) {
        quote! {
            hash: ::core::option::Option::Some({
                unsafe fn __hash(value: #facet_crate::OxPtrConst, hasher: &mut #facet_crate::HashProxy<'_>) -> ::core::option::Option<()> {
                    let value: &#struct_type = value.ptr().get();
                    <#struct_type as ::core::hash::Hash>::hash(value, hasher);
                    ::core::option::Option::Some(())
                }
                __hash
            }),
        }
    } else if sources.should_auto() {
        quote! {
            hash: ::core::option::Option::Some({
                unsafe fn __hash(value: #facet_crate::OxPtrConst, hasher: &mut #facet_crate::HashProxy<'_>) -> ::core::option::Option<()> {
                    if impls!(#struct_type: ::core::hash::Hash) {
                        let value: &#struct_type = value.ptr().get();
                        (&&Spez(value)).spez_hash(hasher);
                        ::core::option::Option::Some(())
                    } else {
                        ::core::option::Option::None
                    }
                }
                __hash
            }),
        }
    } else {
        quote! { hash: ::core::option::Option::None, }
    };

    // Parse (FromStr): no derive exists, only auto-detect if enabled
    let parse_field = if sources.should_auto() {
        quote! {
            parse: ::core::option::Option::Some({
                unsafe fn __parse(s: &str, target: #facet_crate::OxPtrMut) -> ::core::option::Option<::core::result::Result<(), #facet_crate::ParseError>> {
                    if impls!(#struct_type: ::core::str::FromStr) {
                        ::core::option::Option::Some(
                            match (&&SpezEmpty::<#struct_type>::SPEZ).spez_parse(s, target.ptr().as_uninit()) {
                                ::core::result::Result::Ok(_) => ::core::result::Result::Ok(()),
                                ::core::result::Result::Err(e) => ::core::result::Result::Err(e),
                            }
                        )
                    } else {
                        ::core::option::Option::None
                    }
                }
                __parse
            }),
        }
    } else {
        quote! { parse: ::core::option::Option::None, }
    };

    // Invariants: container-level invariants function (wrapped for OxRef signature)
    let invariants_field = if let Some(inv_fn) = invariants_fn {
        quote! {
            invariants: ::core::option::Option::Some({
                unsafe fn __invariants(data: #facet_crate::OxPtrConst) -> ::core::option::Option<#facet_crate::𝟋::𝟋Result<(), #facet_crate::𝟋::𝟋Str>> {
                    let value: &#struct_type = data.ptr().get();
                    ::core::option::Option::Some(#inv_fn(value))
                }
                __invariants
            }),
        }
    } else {
        quote! { invariants: ::core::option::Option::None, }
    };

    // Return VTableErased::Indirect wrapping a VTableIndirect using struct literal syntax
    // Uses prelude aliases for compact output (𝟋VtE)
    // NOTE: drop_in_place, default_in_place, clone_into are now in TypeOps, not VTable
    quote! {
        𝟋VtE::Indirect(&const {
            #facet_crate::VTableIndirect {
                #display_field
                #debug_field
                #hash_field
                #invariants_field
                #parse_field
                parse_bytes: ::core::option::Option::None,
                try_from: ::core::option::Option::None,
                try_into_inner: ::core::option::Option::None,
                try_borrow_inner: ::core::option::Option::None,
                #partial_eq_field
                #partial_cmp_field
                #cmp_field
            }
        })
    }
}

/// Generates TypeOps for per-type operations (drop, default, clone).
/// Returns `Option<TokenStream>` - Some if any TypeOps is needed, None if no ops.
///
/// Uses TypeOpsDirect for non-generic types, TypeOpsIndirect for generic types.
pub(crate) fn gen_type_ops(
    facet_crate: &TokenStream,
    sources: &TraitSources<'_>,
    struct_type: &TokenStream,
    has_type_or_const_generics: bool,
    truthy_fn: Option<&TokenStream>,
) -> Option<TokenStream> {
    // Only use TypeOpsIndirect when there are actual type or const generics.
    // For auto_traits WITHOUT generics, we can still use TypeOpsDirect since
    // the helper functions can use `Self` which resolves to the concrete type.
    if has_type_or_const_generics {
        return gen_type_ops_indirect(facet_crate, sources, struct_type, truthy_fn);
    }

    // Use TypeOpsDirect for non-generic types (including auto_traits without generics)
    gen_type_ops_direct(facet_crate, sources, struct_type, truthy_fn)
}

/// Generates TypeOpsDirect for non-generic types.
/// Returns Some(TokenStream) if any ops are needed, None otherwise.
///
/// Uses raw pointers (`*mut ()`, `*const ()`) for type-erased function signatures,
/// matching VTableDirect's approach. The `Self` type is used inside the const block
/// which properly resolves without capturing lifetime parameters.
fn gen_type_ops_direct(
    facet_crate: &TokenStream,
    sources: &TraitSources<'_>,
    struct_type: &TokenStream,
    truthy_fn: Option<&TokenStream>,
) -> Option<TokenStream> {
    // Check if Default is available (from declared traits or #[facet(default)])
    let has_default = sources.has_declared(|d| d.default) || sources.facet_default;

    // Check if Clone is available (from declared traits)
    let has_clone = sources.has_declared(|d| d.clone);

    // Generate default_in_place field
    // Uses helper function 𝟋default_for::<Self>() which returns fn(*mut Self),
    // then transmutes to fn(*mut ()) for the erased signature
    let default_field = if has_default {
        quote! {
            default_in_place: ::core::option::Option::Some(
                unsafe { ::core::mem::transmute(#facet_crate::𝟋::𝟋default_for::<Self>() as unsafe fn(*mut Self)) }
            ),
        }
    } else if sources.should_auto() {
        // For auto_traits, generate an inline function that uses the Spez pattern.
        // The function hardcodes struct_type, so specialization resolves correctly.
        // The impls! check determines whether we return Some or None at const-eval time.
        quote! {
            default_in_place: if #facet_crate::𝟋::impls!(#struct_type: ::core::default::Default) {
                ::core::option::Option::Some({
                    unsafe fn __default_in_place(ptr: *mut ()) {
                        let target = #facet_crate::PtrUninit::new(ptr as *mut u8);
                        unsafe { (&&&#facet_crate::𝟋::SpezEmpty::<#struct_type>::SPEZ).spez_default_in_place(target) };
                    }
                    __default_in_place
                })
            } else {
                ::core::option::Option::None
            },
        }
    } else {
        quote! { default_in_place: ::core::option::Option::None, }
    };

    // Generate clone_into field
    // Uses helper function 𝟋clone_for::<Self>() which returns fn(*const Self, *mut Self),
    // then transmutes to fn(*const (), *mut ()) for the erased signature
    let clone_field = if has_clone {
        quote! {
            clone_into: ::core::option::Option::Some(
                unsafe { ::core::mem::transmute(#facet_crate::𝟋::𝟋clone_for::<Self>() as unsafe fn(*const Self, *mut Self)) }
            ),
        }
    } else if sources.should_auto() {
        // For auto_traits, generate an inline function that uses the Spez pattern.
        // The function hardcodes struct_type, so specialization resolves correctly.
        // The impls! check determines whether we return Some or None at const-eval time.
        quote! {
            clone_into: if #facet_crate::𝟋::impls!(#struct_type: ::core::clone::Clone) {
                ::core::option::Option::Some({
                    unsafe fn __clone_into(src: *const (), dst: *mut ()) {
                        let src_ref: &#struct_type = unsafe { &*(src as *const #struct_type) };
                        let target = #facet_crate::PtrUninit::new(dst as *mut u8);
                        unsafe { (&&&#facet_crate::𝟋::Spez(src_ref)).spez_clone_into(target) };
                    }
                    __clone_into
                })
            } else {
                ::core::option::Option::None
            },
        }
    } else {
        quote! { clone_into: ::core::option::Option::None, }
    };

    // Generate TypeOpsDirect struct literal
    // Uses transmute to convert typed fn pointers to erased fn(*mut ()) etc.
    // Uses Self inside the const block which resolves to the implementing type
    let truthy_field = if let Some(truthy) = truthy_fn {
        quote! {
            is_truthy: ::core::option::Option::Some({
                unsafe fn __truthy(value: #facet_crate::PtrConst) -> bool {
                    let this: &#struct_type = unsafe { value.get::<#struct_type>() };
                    #truthy(this)
                }
                __truthy
            }),
        }
    } else {
        quote! { is_truthy: ::core::option::Option::None, }
    };

    Some(quote! {
        #facet_crate::TypeOps::Direct(&const {
            #facet_crate::TypeOpsDirect {
                drop_in_place: unsafe { ::core::mem::transmute(::core::ptr::drop_in_place::<Self> as unsafe fn(*mut Self)) },
                #default_field
                #clone_field
                #truthy_field
            }
        })
    })
}

/// Generates TypeOpsIndirect for generic types with auto_traits.
/// Returns Some(TokenStream) if any ops are needed, None otherwise.
///
/// Uses helper functions that take a type parameter to avoid the "can't use Self
/// from outer item" error in function items.
fn gen_type_ops_indirect(
    facet_crate: &TokenStream,
    sources: &TraitSources<'_>,
    _struct_type: &TokenStream,
    truthy_fn: Option<&TokenStream>,
) -> Option<TokenStream> {
    // For TypeOpsIndirect, we always need drop_in_place
    // default_in_place and clone_into are optional based on available traits
    // Note: We use helper functions 𝟋indirect_*_for::<Self>() which have their own
    // generic parameter, avoiding the "can't use Self from outer item" issue.

    // Check if Default is available
    // Note: For auto_traits, we could use specialization but it's complex.
    // For now, only generate default_in_place when Default is explicitly known.
    let default_field = if sources.has_declared(|d| d.default) || sources.facet_default {
        quote! {
            default_in_place: ::core::option::Option::Some(#facet_crate::𝟋::𝟋indirect_default_for::<Self>()),
        }
    } else {
        // For auto_traits or no default, set to None
        // Runtime detection of Default not supported in TypeOps yet
        quote! { default_in_place: ::core::option::Option::None, }
    };

    // Check if Clone is available
    // Note: For auto_traits, we could use specialization but it's complex.
    // For now, only generate clone_into when Clone is explicitly known.
    let clone_field = if sources.has_declared(|d| d.clone) {
        quote! {
            clone_into: ::core::option::Option::Some(#facet_crate::𝟋::𝟋indirect_clone_for::<Self>()),
        }
    } else {
        // For auto_traits or no clone, set to None
        // Runtime detection of Clone not supported in TypeOps yet
        quote! { clone_into: ::core::option::Option::None, }
    };

    let truthy_field = if let Some(truthy) = truthy_fn {
        quote! {
            is_truthy: ::core::option::Option::Some({
                unsafe fn __truthy(value: #facet_crate::PtrConst) -> bool {
                    let this: &Self = unsafe { value.get::<Self>() };
                    #truthy(this)
                }
                __truthy
            }),
        }
    } else {
        quote! { is_truthy: ::core::option::Option::None, }
    };

    Some(quote! {
        #facet_crate::TypeOps::Indirect(&const {
            #facet_crate::TypeOpsIndirect {
                drop_in_place: #facet_crate::𝟋::𝟋indirect_drop_for::<Self>(),
                #default_field
                #clone_field
                #truthy_field
            }
        })
    })
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
    skip_all_unless_truthy: bool,
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

    // Track what kind of default was specified
    enum DefaultKind {
        FromTrait,
        Custom(TokenStream),
    }

    let mut flags: Vec<TokenStream> = Vec::new();
    let mut rename_value: Option<TokenStream> = None;
    let mut alias_value: Option<TokenStream> = None;
    let mut default_value: Option<DefaultKind> = None;
    let mut skip_serializing_if_value: Option<TokenStream> = None;
    let mut invariants_value: Option<TokenStream> = None;
    let mut proxy_value: Option<TokenStream> = None;
    let mut metadata_value: Option<String> = None;
    let mut attribute_list: Vec<TokenStream> = Vec::new();

    let mut want_truthy_skip = skip_all_unless_truthy;

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
                    // Default goes into dedicated field, not attributes
                    let args = &attr.args;
                    if args.is_empty() {
                        // #[facet(default)] - use Default trait
                        default_value = Some(DefaultKind::FromTrait);
                    } else {
                        // #[facet(default = expr)] - use custom expression
                        // Parse `= expr` to get just the expr
                        let args_str = args.to_string();
                        let expr_str = args_str.trim_start_matches('=').trim();
                        let expr: TokenStream = expr_str.parse().unwrap_or_else(|_| args.clone());
                        default_value = Some(DefaultKind::Custom(expr));
                    }
                }
                "recursive_type" => {
                    // recursive_type sets a flag
                    flags.push(quote! { 𝟋FF::RECURSIVE_TYPE });
                }
                "metadata" => {
                    // metadata = kind - marks field as metadata, excluded from structural hashing
                    // Parse `= ident` to get just the ident as a string
                    let args = &attr.args;
                    let args_str = args.to_string();
                    let kind_str = args_str.trim_start_matches('=').trim();
                    metadata_value = Some(kind_str.to_string());
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
                "skip_serializing_if" => {
                    // User provides a function name: #[facet(skip_serializing_if = fn_name)]
                    // We need to wrap it in a type-erased function that takes PtrConst
                    let args = &attr.args;
                    let args_str = args.to_string();
                    let fn_name_str = args_str.trim_start_matches('=').trim();
                    let fn_name: TokenStream = fn_name_str.parse().unwrap_or_else(|_| args.clone());
                    // Generate a wrapper function that converts PtrConst to the expected type
                    skip_serializing_if_value = Some(quote! {
                        {
                            unsafe fn __skip_ser_if_wrapper(ptr: #facet_crate::PtrConst) -> bool {
                                let value: &#field_type = unsafe { ptr.get() };
                                #fn_name(value)
                            }
                            __skip_ser_if_wrapper
                        }
                    });
                }
                "skip_unless_truthy" => {
                    want_truthy_skip = true;
                }
                "invariants" => {
                    // User provides a function name: #[facet(invariants = fn_name)]
                    let args = &attr.args;
                    let args_str = args.to_string();
                    let fn_name_str = args_str.trim_start_matches('=').trim();
                    let fn_name: TokenStream = fn_name_str.parse().unwrap_or_else(|_| args.clone());
                    invariants_value = Some(quote! { #fn_name });
                }
                "proxy" => {
                    // User provides a type: #[facet(proxy = ProxyType)]
                    let args = &attr.args;
                    let args_str = args.to_string();
                    let type_str = args_str.trim_start_matches('=').trim();
                    let proxy_type: TokenStream = type_str.parse().unwrap_or_else(|_| args.clone());
                    // Generate a full ProxyDef with convert functions for field-level proxy
                    proxy_value = Some(quote! {
                        &const {
                            extern crate alloc as __alloc;

                            unsafe fn __proxy_convert_in(
                                proxy_ptr: #facet_crate::PtrConst,
                                field_ptr: #facet_crate::PtrUninit,
                            ) -> ::core::result::Result<#facet_crate::PtrMut, __alloc::string::String> {
                                let proxy: #proxy_type = proxy_ptr.read();
                                match <#field_type as ::core::convert::TryFrom<#proxy_type>>::try_from(proxy) {
                                    ::core::result::Result::Ok(value) => ::core::result::Result::Ok(field_ptr.put(value)),
                                    ::core::result::Result::Err(e) => ::core::result::Result::Err(__alloc::string::ToString::to_string(&e)),
                                }
                            }

                            unsafe fn __proxy_convert_out(
                                field_ptr: #facet_crate::PtrConst,
                                proxy_ptr: #facet_crate::PtrUninit,
                            ) -> ::core::result::Result<#facet_crate::PtrMut, __alloc::string::String> {
                                let field_ref: &#field_type = field_ptr.get();
                                match <#proxy_type as ::core::convert::TryFrom<&#field_type>>::try_from(field_ref) {
                                    ::core::result::Result::Ok(proxy) => ::core::result::Result::Ok(proxy_ptr.put(proxy)),
                                    ::core::result::Result::Err(e) => ::core::result::Result::Err(__alloc::string::ToString::to_string(&e)),
                                }
                            }

                            #facet_crate::ProxyDef {
                                shape: <#proxy_type as #facet_crate::Facet>::SHAPE,
                                convert_in: __proxy_convert_in,
                                convert_out: __proxy_convert_out,
                            }
                        }
                    });
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

    if skip_serializing_if_value.is_none() && want_truthy_skip {
        skip_serializing_if_value = Some(quote! {
            {
                unsafe fn __truthiness_with_fallback(
                    shape: &'static #facet_crate::Shape,
                    ptr: #facet_crate::PtrConst,
                ) -> Option<bool> {
                    if let Some(truthy) = shape.truthiness_fn() {
                        return Some(unsafe { truthy(ptr) });
                    }
                    if let #facet_crate::Def::Pointer(ptr_def) = shape.def {
                        if let (Some(inner_shape), Some(borrow)) =
                            (ptr_def.pointee(), ptr_def.vtable.borrow_fn)
                        {
                            let inner_ptr = unsafe { borrow(ptr) };
                            return __truthiness_with_fallback(inner_shape, inner_ptr);
                        }
                    }
                    if let #facet_crate::Type::User(#facet_crate::UserType::Struct(st)) = shape.ty
                        && matches!(st.kind, #facet_crate::StructKind::Tuple)
                    {
                        for field in st.fields {
                            if field.shape.get().layout.sized_layout().is_err() {
                                continue;
                            }
                            let field_ptr = #facet_crate::PtrConst::new(unsafe {
                                ptr.as_byte_ptr().add(field.offset)
                            } as *const ());
                            if let Some(true) = __truthiness_with_fallback(field.shape.get(), field_ptr) {
                                return Some(true);
                            }
                        }
                        return Some(false);
                    }
                    None
                }

                unsafe fn __skip_unless_truthy(ptr: #facet_crate::PtrConst) -> bool {
                    let shape = <#field_type as #facet_crate::Facet>::SHAPE;
                    match __truthiness_with_fallback(shape, ptr) {
                        Some(result) => !result,
                        None => false,
                    }
                }
                __skip_unless_truthy
            }
        });
    }

    let maybe_attributes = if attribute_list.is_empty() {
        quote! { &[] }
    } else {
        quote! { &const {[#(#attribute_list),*]} }
    };

    #[cfg(feature = "doc")]
    let maybe_field_doc = if doc_lines.is_empty() || crate::is_no_doc() {
        quote! { &[] }
    } else {
        quote! { &[#(#doc_lines),*] }
    };
    #[cfg(not(feature = "doc"))]
    let maybe_field_doc = quote! { &[] };

    // Calculate the final offset, incorporating the base_offset if present
    let final_offset = match base_offset {
        Some(base) => {
            quote! { #base + ::core::mem::offset_of!(#struct_name #bgp_without_bounds, #field_name_raw) }
        }
        None => {
            quote! { ::core::mem::offset_of!(#struct_name #bgp_without_bounds, #field_name_raw) }
        }
    };

    // === Direct Field construction (avoiding builder pattern for faster const eval) ===
    // Uses short aliases from `use #facet_crate::𝟋::*` in the enclosing const block

    // Shape reference: always use a function for lazy evaluation
    // This moves const eval from compile time to runtime, improving compile times
    // ShapeRef is a tuple struct: ShapeRef(fn() -> &'static Shape)
    let is_opaque = field.attrs.has_builtin("opaque");
    let shape_ref_expr = if is_recursive {
        // Recursive types need a closure to break the cycle
        quote! { 𝟋ShpR(|| #shape_expr) }
    } else if is_opaque {
        // Opaque fields use Opaque<T> wrapper
        quote! { 𝟋ShpR(𝟋shp::<#facet_crate::Opaque<#field_type>>) }
    } else {
        // Normal fields use shape_of::<T> which is monomorphized per type
        quote! { 𝟋ShpR(𝟋shp::<#field_type>) }
    };

    // Flags: combine all flags or use empty
    let flags_expr = if flags.is_empty() {
        quote! { 𝟋NOFL }
    } else if flags.len() == 1 {
        let f = &flags[0];
        quote! { #f }
    } else {
        let first = &flags[0];
        let rest = &flags[1..];
        quote! { #first #(.union(#rest))* }
    };

    // Rename: Option
    let rename_expr = match &rename_value {
        Some(rename) => quote! { ::core::option::Option::Some(#rename) },
        None => quote! { ::core::option::Option::None },
    };

    // Alias: Option
    let alias_expr = match &alias_value {
        Some(alias) => quote! { ::core::option::Option::Some(#alias) },
        None => quote! { ::core::option::Option::None },
    };

    // Default: Option<DefaultSource>
    let default_expr = match &default_value {
        Some(DefaultKind::FromTrait) => {
            // When a field has 'opaque' attribute, the field shape doesn't have Default vtable
            // because Opaque<T> doesn't expose T's vtable. Instead, generate a custom default
            // function. Special case: Option<T> always defaults to None regardless of T's traits.
            if field.attrs.has_builtin("opaque") {
                // Check if the field type looks like Option<...>
                let type_str = field_type.to_token_stream().to_string();
                let is_option = type_str.starts_with("Option") || type_str.contains(":: Option");

                if is_option {
                    // Option<T> always defaults to None
                    quote! {
                        ::core::option::Option::Some(𝟋DS::Custom({
                            unsafe fn __default(__ptr: #facet_crate::PtrUninit) -> #facet_crate::PtrMut {
                                __ptr.put(<#field_type>::None)
                            }
                            __default
                        }))
                    }
                } else {
                    // For non-Option opaque types, call Default::default()
                    quote! {
                        ::core::option::Option::Some(𝟋DS::Custom({
                            unsafe fn __default(__ptr: #facet_crate::PtrUninit) -> #facet_crate::PtrMut {
                                __ptr.put(<#field_type as ::core::default::Default>::default())
                            }
                            __default
                        }))
                    }
                }
            } else {
                quote! { ::core::option::Option::Some(𝟋DS::FromTrait) }
            }
        }
        Some(DefaultKind::Custom(expr)) => {
            // Use vtable's try_from to convert the expression to the field type.
            // This allows `default = "foo"` to work for String fields,
            // and `default = 42` to work for any integer type.
            // If the types are the same, we just write directly.
            quote! {
                ::core::option::Option::Some(𝟋DS::Custom({
                    unsafe fn __default(__ptr: #facet_crate::PtrUninit) -> #facet_crate::PtrMut {
                        // Helper function to get shape from a value via type inference
                        #[inline]
                        fn __shape_of_val<'a, T: #facet_crate::Facet<'a>>(_: &T) -> &'static #facet_crate::Shape {
                            T::SHAPE
                        }
                        // Create the source value
                        let __src_value = #expr;
                        // Get shapes for source and destination types
                        let __src_shape = __shape_of_val(&__src_value);
                        let __dst_shape = <#field_type as #facet_crate::Facet>::SHAPE;

                        // If types are the same (by shape id), just write directly
                        if __src_shape.id == __dst_shape.id {
                            return unsafe { __ptr.put(__src_value) };
                        }

                        // Create a pointer to the source value
                        let __src_ptr = #facet_crate::PtrConst::new(
                            &__src_value as *const _ as *const u8
                        );
                        // Get destination pointer
                        let __dst_ptr = #facet_crate::PtrMut::new(__ptr.as_byte_ptr() as *mut u8);
                        // Call try_from via vtable
                        match unsafe { __dst_shape.call_try_from(__src_shape, __src_ptr, __dst_ptr) } {
                            Some(Ok(())) => {
                                // Don't run destructor on source value since we consumed it
                                ::core::mem::forget(__src_value);
                                unsafe { __ptr.assume_init() }
                            },
                            Some(Err(e)) => panic!("default value conversion failed: {}", e),
                            None => panic!("type {} does not support try_from", __dst_shape.type_identifier),
                        }
                    }
                    __default
                }))
            }
        }
        None => quote! { ::core::option::Option::None },
    };

    // Skip serializing if: Option
    let skip_ser_if_expr = match &skip_serializing_if_value {
        Some(skip_ser_if) => quote! { ::core::option::Option::Some(#skip_ser_if) },
        None => quote! { ::core::option::Option::None },
    };

    // Invariants: Option
    let invariants_expr = match &invariants_value {
        Some(inv) => quote! { ::core::option::Option::Some(#inv) },
        None => quote! { ::core::option::Option::None },
    };

    // Proxy: Option (requires alloc feature in facet-core)
    // We always emit this field since we can't check facet-core's features from generated code.
    // If facet-core was built without alloc, this will cause a compile error (acceptable trade-off).
    let proxy_expr = match &proxy_value {
        Some(proxy) => quote! { ::core::option::Option::Some(#proxy) },
        None => quote! { ::core::option::Option::None },
    };

    // Metadata: Option<&'static str>
    let metadata_expr = match &metadata_value {
        Some(kind) => quote! { ::core::option::Option::Some(#kind) },
        None => quote! { ::core::option::Option::None },
    };

    // Direct Field struct literal
    quote! {
        𝟋Fld {
            name: #field_name_effective,
            shape: #shape_ref_expr,
            offset: #final_offset,
            flags: #flags_expr,
            rename: #rename_expr,
            alias: #alias_expr,
            attributes: #maybe_attributes,
            doc: #maybe_field_doc,
            default: #default_expr,
            skip_serializing_if: #skip_ser_if_expr,
            invariants: #invariants_expr,
            proxy: #proxy_expr,
            metadata: #metadata_expr,
        }
    }
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

    // Validate: pod and invariants are mutually exclusive
    let has_pod = ps.container.attrs.has_builtin("pod");
    let has_invariants = ps
        .container
        .attrs
        .facet
        .iter()
        .any(|a| a.is_builtin() && a.key_str() == "invariants");
    if has_pod && has_invariants {
        // Find the span of the pod attribute for better error location
        let pod_span = ps
            .container
            .attrs
            .facet
            .iter()
            .find(|a| a.is_builtin() && a.key_str() == "pod")
            .map(|a| a.key.span())
            .unwrap_or_else(proc_macro2::Span::call_site);
        return quote_spanned! { pod_span =>
            compile_error!("#[facet(pod)] and #[facet(invariants = ...)] are mutually exclusive. \
                POD types by definition have no invariants.");
        };
    }

    let struct_name_ident = format_ident!("{}", ps.container.name);
    let struct_name = &ps.container.name;
    let struct_name_str = struct_name.to_string();

    let opaque = ps.container.attrs.has_builtin("opaque");

    let skip_all_unless_truthy = ps.container.attrs.has_builtin("skip_all_unless_truthy");

    let truthy_attr: Option<TokenStream> = ps.container.attrs.facet.iter().find_map(|attr| {
        if attr.is_builtin() && attr.key_str() == "truthy" {
            let args = &attr.args;
            if args.is_empty() {
                return None;
            }
            let args_str = args.to_string();
            let fn_name_str = args_str.trim_start_matches('=').trim();
            let fn_name: TokenStream = fn_name_str.parse().unwrap_or_else(|_| args.clone());
            Some(fn_name)
        } else {
            None
        }
    });

    // Get the facet crate path (custom or default ::facet)
    let facet_crate = ps.container.attrs.facet_crate();

    let type_name_fn =
        generate_type_name_fn(struct_name, parsed.generics.as_ref(), opaque, &facet_crate);

    // Determine if this struct should use transparent semantics (needed for vtable generation)
    // Transparent is enabled if:
    // 1. #[facet(transparent)] is explicitly set, OR
    // 2. #[repr(transparent)] is set AND the struct is a tuple struct with exactly 0 or 1 field
    let has_explicit_facet_transparent = ps.container.attrs.has_builtin("transparent");
    let has_repr_transparent = ps.container.attrs.is_repr_transparent();

    let repr_implies_facet_transparent = if has_repr_transparent && !has_explicit_facet_transparent
    {
        match &ps.kind {
            PStructKind::TupleStruct { fields } => fields.len() <= 1,
            _ => false,
        }
    } else {
        false
    };

    let use_transparent_semantics =
        has_explicit_facet_transparent || repr_implies_facet_transparent;

    // For transparent types, get the inner field info
    let inner_field: Option<PStructField> = if use_transparent_semantics {
        match &ps.kind {
            PStructKind::TupleStruct { fields } => {
                if fields.len() > 1 {
                    return quote! {
                        compile_error!("Transparent structs must be tuple structs with zero or one field");
                    };
                }
                fields.first().cloned()
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

    // Build transparent info for vtable generation
    let transparent_info = if use_transparent_semantics {
        Some(TransparentInfo {
            inner_field_type: inner_field.as_ref().map(|f| &f.ty),
            inner_is_opaque: inner_field
                .as_ref()
                .is_some_and(|f| f.attrs.has_builtin("opaque")),
            is_zst: inner_field.is_none(),
        })
    } else {
        None
    };

    // Determine trait sources and generate vtable accordingly
    let trait_sources = TraitSources::from_attrs(&ps.container.attrs);
    // Build the struct type token stream (e.g., `MyStruct` or `MyStruct<T, U>`)
    // We need this because `Self` is not available inside `&const { }` blocks
    let bgp_for_vtable = ps.container.bgp.display_without_bounds();
    let struct_type_for_vtable = quote! { #struct_name_ident #bgp_for_vtable };

    // Extract container-level invariants and generate wrapper function
    let invariants_wrapper: Option<TokenStream> = {
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
                        return 𝟋Result::Err(𝟋Str::from("invariant check failed"));
                    }
                }
            });

            Some(quote! {
                {
                    fn __invariants_wrapper(value: &#struct_type_for_vtable) -> 𝟋Result<(), 𝟋Str> {
                        use #facet_crate::𝟋::*;
                        #(#tests)*
                        𝟋Result::Ok(())
                    }
                    __invariants_wrapper
                }
            })
        } else {
            None
        }
    };

    let vtable_code = gen_vtable(
        &facet_crate,
        &type_name_fn,
        &trait_sources,
        transparent_info.as_ref(),
        &struct_type_for_vtable,
        invariants_wrapper.as_ref(),
    );
    // Note: vtable_code already contains &const { ... } for the VTableDirect,
    // no need for an extra const { } wrapper around VTableErased
    let vtable_init = vtable_code;

    // Generate TypeOps for drop, default, clone operations
    // Check if the type has any type or const generics (NOT lifetimes)
    // Lifetimes don't affect layout, so types like RawJson<'a> can use TypeOpsDirect
    // Only types like Vec<T> need TypeOpsIndirect
    let has_type_or_const_generics = ps.container.bgp.params.iter().any(|p| {
        matches!(
            p.param,
            facet_macro_parse::GenericParamName::Type(_)
                | facet_macro_parse::GenericParamName::Const(_)
        )
    });
    let type_ops_init = gen_type_ops(
        &facet_crate,
        &trait_sources,
        &struct_type_for_vtable,
        has_type_or_const_generics,
        truthy_attr.as_ref(),
    );

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
                    gen_field_from_pfield(
                        field,
                        struct_name,
                        &ps.container.bgp,
                        None,
                        &facet_crate,
                        skip_all_unless_truthy,
                    )
                })
                .collect::<Vec<_>>();
            (kind, fields_vec)
        }
        PStructKind::TupleStruct { fields } => {
            let kind = quote!(𝟋Sk::TupleStruct);
            let fields_vec = fields
                .iter()
                .map(|field| {
                    gen_field_from_pfield(
                        field,
                        struct_name,
                        &ps.container.bgp,
                        None,
                        &facet_crate,
                        skip_all_unless_truthy,
                    )
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
        quote! { .variance(𝟋Vnc::INVARIANT) }
    } else {
        // Point to Shape::computed_variance - it takes &Shape and walks fields
        quote! { .variance(𝟋CV) }
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

    // Static decl removed - the TYPENAME_SHAPE static was redundant since
    // <T as Facet>::SHAPE is already accessible and nobody was using the static

    // Doc comments from PStruct - returns value for struct literal
    // doc call - only emit if there are doc comments and doc feature is enabled
    #[cfg(feature = "doc")]
    let doc_call = if ps.container.attrs.doc.is_empty() || crate::is_no_doc() {
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
                        "invariants"
                            | "crate"
                            | "traits"
                            | "auto_traits"
                            | "proxy"
                            | "truthy"
                            | "skip_all_unless_truthy"
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

    // POD flag - marks type as Plain Old Data (no invariants)
    let pod_call = if ps.container.attrs.has_builtin("pod") {
        quote! { .pod() }
    } else {
        quote! {}
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
                    unsafe fn __facet_proxy_convert_in(
                        proxy_ptr: #facet_crate::PtrConst,
                        field_ptr: #facet_crate::PtrUninit,
                    ) -> ::core::result::Result<#facet_crate::PtrMut, #facet_crate::𝟋::𝟋Str> {
                        extern crate alloc as __alloc;
                        let proxy: #proxy_type = proxy_ptr.read();
                        match <#struct_type #bgp_display as ::core::convert::TryFrom<#proxy_type>>::try_from(proxy) {
                            ::core::result::Result::Ok(value) => ::core::result::Result::Ok(field_ptr.put(value)),
                            ::core::result::Result::Err(e) => ::core::result::Result::Err(__alloc::string::ToString::to_string(&e)),
                        }
                    }

                    #[doc(hidden)]
                    unsafe fn __facet_proxy_convert_out(
                        field_ptr: #facet_crate::PtrConst,
                        proxy_ptr: #facet_crate::PtrUninit,
                    ) -> ::core::result::Result<#facet_crate::PtrMut, #facet_crate::𝟋::𝟋Str> {
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

            // Reference the inherent methods from within the SHAPE const block.
            // We use <Self> syntax which works inside &const { } blocks and properly
            // refers to the monomorphized type from the enclosing impl.
            let proxy_ref = quote! {
                .proxy(&const {
                    #facet_crate::ProxyDef {
                        shape: <Self>::__facet_proxy_shape(),
                        convert_in: <Self>::__facet_proxy_convert_in,
                        convert_out: <Self>::__facet_proxy_convert_out,
                    }
                })
            };

            (proxy_impl, proxy_ref)
        } else {
            (quote! {}, quote! {})
        }
    };

    // Generate the inner shape field value for transparent types
    // inner call - only emit for transparent types
    let inner_call = if use_transparent_semantics {
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

    // Type name function - for generic types, this formats with type parameters
    let type_name_call = if parsed.generics.is_some() && !opaque {
        quote! { .type_name(#type_name_fn) }
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

    // Generate ty_field and optionally a hoisted __FIELDS const
    // Hoisting avoids &const { [...] } which causes 12+ promotions per struct
    let (ty_field, fields_const) = if opaque {
        (
            quote! {
                #facet_crate::Type::User(#facet_crate::UserType::Opaque)
            },
            quote! {},
        )
    } else if fields_vec.is_empty() {
        // Optimize: use &[] for empty fields to avoid const block overhead
        (
            quote! {
                𝟋Ty::User(𝟋UTy::Struct(
                    𝟋STyB::new(#kind, &[]).repr(#repr).build()
                ))
            },
            quote! {},
        )
    } else {
        // Hoist fields array to associated const to avoid promotions
        let num_fields = fields_vec.len();
        (
            quote! {
                𝟋Ty::User(𝟋UTy::Struct(
                    𝟋STyB::new(#kind, &Self::__FIELDS).repr(#repr).build()
                ))
            },
            quote! {
                const __FIELDS: [#facet_crate::Field; #num_fields] = {
                    use #facet_crate::𝟋::*;
                    [#(#fields_vec),*]
                };
            },
        )
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

    // Vtable is now fully built in gen_vtable, including invariants
    let vtable_field = quote! { #vtable_init };

    // TypeOps for drop, default, clone - convert Option<TokenStream> to a call
    let type_ops_call = match type_ops_init {
        Some(ops) => quote! { .type_ops(#ops) },
        None => quote! {},
    };

    // Hoist the entire SHAPE construction to an inherent impl const
    // This avoids &const {} promotions - the reference is to a plain const, not an inline const block
    let shape_inherent_impl = quote! {
        #[doc(hidden)]
        impl #bgp_def #struct_name_ident #bgp_without_bounds #where_clauses {
            #fields_const

            const __SHAPE_DATA: #facet_crate::Shape = {
                use #facet_crate::𝟋::*;

                𝟋ShpB::for_sized::<Self>(#struct_name_str)
                    .vtable(#vtable_field)
                    #type_ops_call
                    .ty(#ty_field)
                    .def(𝟋Def::Undefined)
                    #type_params_call
                    #type_name_call
                    #doc_call
                    #attributes_call
                    #type_tag_call
                    #proxy_call
                    #inner_call
                    #variance_call
                    #pod_call
                    .build()
            };
        }
    };

    // Static declaration for release builds (pre-evaluates SHAPE)
    let static_decl = crate::derive::generate_static_decl(
        &struct_name_ident,
        &facet_crate,
        has_type_or_const_generics,
    );

    // Final quote block using refactored parts
    let result = quote! {
        #dead_code_suppression

        #trait_assertion_fn

        // Proxy inherent impl (outside the Facet impl so generic params are in scope)
        #proxy_inherent_impl

        // Hoisted SHAPE data const (avoids &const {} promotions)
        #shape_inherent_impl

        #[automatically_derived]
        unsafe impl #bgp_def #facet_crate::Facet<'ʄ> for #struct_name_ident #bgp_without_bounds #where_clauses {
            const SHAPE: &'static #facet_crate::Shape = &Self::__SHAPE_DATA;
        }

        #static_decl
    };

    result
}
