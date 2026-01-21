use super::*;
use crate::process_struct::{
    TraitSources, gen_field_from_pfield, gen_trait_bounds, gen_type_ops, gen_vtable,
    phantom_attr_use,
};
use proc_macro2::Literal;
use quote::{format_ident, quote, quote_spanned};

/// Generate a Variant using VariantBuilder for more compact output.
///
/// NOTE: This function generates code that uses short aliases from the 𝟋 prelude.
/// It MUST be called within a context where `use #facet_crate::𝟋::*` has been emitted.
fn gen_variant(
    name: impl quote::ToTokens,
    rename: Option<impl quote::ToTokens>,
    discriminant: impl quote::ToTokens,
    attributes: Option<impl quote::ToTokens>,
    struct_kind: impl quote::ToTokens,
    fields: impl quote::ToTokens,
    doc: Option<impl quote::ToTokens>,
) -> TokenStream {
    // Only emit builder calls when there's actual content
    let rename_call = rename.map(|r| quote! { .rename(#r) });
    let attributes_call = attributes.map(|a| quote! { .attributes(#a) });
    let doc_call = doc.map(|d| quote! { .doc(#d) });

    quote! {
        𝟋VarB::new(
            #name,
            𝟋STyB::new(#struct_kind, #fields).build()
        )
        #rename_call
        .discriminant(#discriminant)
        #attributes_call
        #doc_call
        .build()
    }
}

/// Generate a unit variant using the pre-built StructType::UNIT constant.
/// NOTE: This function generates code that uses short aliases from the 𝟋 prelude.
/// It MUST be called within a context where `use #facet_crate::𝟋::*` has been emitted.
fn gen_unit_variant(
    name: impl quote::ToTokens,
    rename: Option<impl quote::ToTokens>,
    discriminant: impl quote::ToTokens,
    attributes: Option<impl quote::ToTokens>,
    doc: Option<impl quote::ToTokens>,
) -> TokenStream {
    // Only emit builder calls when there's actual content
    let rename_call = rename.map(|r| quote! { .rename(#r) });
    let attributes_call = attributes.map(|a| quote! { .attributes(#a) });
    let doc_call = doc.map(|d| quote! { .doc(#d) });

    quote! {
        𝟋VarB::new(#name, 𝟋STy::UNIT)
            #rename_call
            .discriminant(#discriminant)
            #attributes_call
            #doc_call
            .build()
    }
}

/// Processes an enum to implement Facet
pub(crate) fn process_enum(parsed: Enum) -> TokenStream {
    // Use already-parsed PEnum, including container/variant/field attributes and rename rules
    let pe = PEnum::parse(&parsed);

    // Emit any collected errors as compile_error! with proper spans
    if !pe.container.attrs.errors.is_empty() {
        let errors = pe.container.attrs.errors.iter().map(|e| {
            let msg = &e.message;
            let span = e.span;
            quote_spanned! { span => compile_error!(#msg); }
        });
        return quote! { #(#errors)* };
    }

    // Validate: pod and invariants are mutually exclusive
    // Note: enums don't currently support container-level invariants, but check anyway
    let has_pod = pe.container.attrs.has_builtin("pod");
    let has_invariants = pe
        .container
        .attrs
        .facet
        .iter()
        .any(|a| a.is_builtin() && a.key_str() == "invariants");
    if has_pod && has_invariants {
        let pod_span = pe
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

    let enum_name = &pe.container.name;
    let enum_name_str = enum_name.to_string();

    let opaque = pe
        .container
        .attrs
        .facet
        .iter()
        .any(|a| a.is_builtin() && a.key_str() == "opaque");

    let skip_all_unless_truthy = pe.container.attrs.has_builtin("skip_all_unless_truthy");

    let truthy_attr: Option<TokenStream> = pe.container.attrs.facet.iter().find_map(|attr| {
        if attr.is_builtin() && attr.key_str() == "truthy" {
            let args = &attr.args;
            if args.is_empty() {
                return None;
            }
            // Use args directly to preserve spans for IDE hover/navigation
            Some(args.clone())
        } else {
            None
        }
    });

    // Get the facet crate path (custom or default ::facet)
    let facet_crate = pe.container.attrs.facet_crate();

    // Collect phantom use statements for IDE hover support on attribute names.
    // These link attribute spans to their facet::builtin::Attr variants.
    let mut phantom_attr_uses: Vec<TokenStream> = Vec::new();
    // Container-level attributes
    for attr in &pe.container.attrs.facet {
        if let Some(phantom) = phantom_attr_use(attr, &facet_crate) {
            phantom_attr_uses.push(phantom);
        }
    }
    // Variant-level and field-level attributes
    for variant in &pe.variants {
        for attr in &variant.attrs.facet {
            if let Some(phantom) = phantom_attr_use(attr, &facet_crate) {
                phantom_attr_uses.push(phantom);
            }
        }
        // Fields within the variant (from variant.kind)
        let fields: &[PStructField] = match &variant.kind {
            PVariantKind::Unit => &[],
            PVariantKind::Tuple { fields } => fields,
            PVariantKind::Struct { fields } => fields,
        };
        for field in fields {
            for attr in &field.attrs.facet {
                if let Some(phantom) = phantom_attr_use(attr, &facet_crate) {
                    phantom_attr_uses.push(phantom);
                }
            }
        }
    }

    let type_name_fn =
        generate_type_name_fn(enum_name, parsed.generics.as_ref(), opaque, &facet_crate);

    // Determine trait sources and generate vtable accordingly
    // Enums don't support transparent semantics, so pass None
    let trait_sources = TraitSources::from_attrs(&pe.container.attrs);
    let bgp_for_vtable = pe.container.bgp.display_without_bounds();
    let enum_type_for_vtable = quote! { #enum_name #bgp_for_vtable };

    // Check if from_ref or try_from_ref attribute is present (for gen_vtable)
    let has_from_ref =
        pe.container.attrs.facet.iter().any(|a| {
            a.is_builtin() && (a.key_str() == "from_ref" || a.key_str() == "try_from_ref")
        });
    let (try_from_fn_direct, try_from_fn_indirect): (Option<TokenStream>, Option<TokenStream>) =
        if has_from_ref {
            (
                Some(quote! { <Self>::__facet_try_from_ref }),
                Some(quote! { <Self>::__facet_try_from_ref_indirect }),
            )
        } else {
            (None, None)
        };

    let vtable_code = gen_vtable(
        &facet_crate,
        &type_name_fn,
        &trait_sources,
        None,
        &enum_type_for_vtable,
        None,  // enums don't support container-level invariants yet
        false, // enums don't need inherent borrow_inner (not transparent)
        try_from_fn_direct.as_ref(),
        try_from_fn_indirect.as_ref(),
    );
    // Note: vtable_code already contains &const { ... } for the VTableDirect,
    // no need for an extra const { } wrapper around VTableErased
    let vtable_init = vtable_code;

    // Generate TypeOps for drop/default/clone operations
    // Check if enum has type or const generics (not just lifetimes)
    let has_type_or_const_generics = pe.container.bgp.params.iter().any(|p| {
        matches!(
            p.param,
            facet_macro_parse::GenericParamName::Type(_)
                | facet_macro_parse::GenericParamName::Const(_)
        )
    });
    let type_ops_init = gen_type_ops(
        &facet_crate,
        &trait_sources,
        &enum_type_for_vtable,
        has_type_or_const_generics,
        truthy_attr.as_ref(),
    );

    let bgp = pe.container.bgp.clone();
    // Use the AST directly for where clauses and generics, as PContainer/PEnum doesn't store them
    let where_clauses = build_where_clauses(
        parsed.clauses.as_ref(),
        parsed.generics.as_ref(),
        opaque,
        &facet_crate,
        &pe.container.attrs.custom_bounds,
    );
    let type_params_call = build_type_params_call(parsed.generics.as_ref(), opaque, &facet_crate);

    // Container-level docs - returns builder call only if there are doc comments and doc feature is enabled
    #[cfg(feature = "doc")]
    let doc_call = if pe.container.attrs.doc.is_empty() || crate::is_no_doc() {
        quote! {}
    } else {
        let doc_lines = &pe.container.attrs.doc;
        quote! { .doc(&[#(#doc_lines),*]) }
    };
    #[cfg(not(feature = "doc"))]
    let doc_call = quote! {};

    // Source location - only emit if doc feature is enabled
    #[cfg(feature = "doc")]
    let source_location_call = if crate::is_no_doc() {
        quote! {}
    } else {
        quote! {
            .source_file(::core::file!())
            .source_line(::core::line!())
            .source_column(::core::column!())
        }
    };
    #[cfg(not(feature = "doc"))]
    let source_location_call = quote! {};

    // Declaration ID - always emitted, computed from source location + type kind + type name
    // Uses # as delimiter since it cannot appear in Rust identifiers
    let decl_id_call = quote! {
        .decl_id(𝟋DId::new(𝟋dih(::core::concat!(
            ::core::file!(), ":",
            ::core::line!(), ":",
            ::core::column!(), "#",
            "enum", "#",
            #enum_name_str
        ))))
    };

    // Container attributes - returns builder call only if there are attributes
    let attributes_call = {
        let mut attribute_tokens: Vec<TokenStream> = Vec::new();
        for attr in &pe.container.attrs.facet {
            // These attributes are handled specially and not emitted to runtime:
            // - crate: sets the facet crate path
            // - traits: compile-time directive for vtable generation
            // - auto_traits: compile-time directive for vtable generation
            // - where: compile-time directive for custom generic bounds
            if attr.is_builtin() {
                let key = attr.key_str();
                if matches!(
                    key.as_str(),
                    "crate"
                        | "traits"
                        | "auto_traits"
                        | "proxy"
                        | "truthy"
                        | "skip_all_unless_truthy"
                        | "where"
                ) {
                    continue;
                }
            }
            // All attributes go through grammar dispatch
            let ext_attr = emit_attr(attr, &facet_crate);
            attribute_tokens.push(quote! { #ext_attr });
        }

        if attribute_tokens.is_empty() {
            quote! {}
        } else {
            quote! { .attributes(&const {[#(#attribute_tokens),*]}) }
        }
    };

    // Type tag - returns builder call only if present
    let type_tag_call = {
        if let Some(type_tag) = pe.container.attrs.get_builtin_args("type_tag") {
            quote! { .type_tag(#type_tag) }
        } else {
            quote! {}
        }
    };

    // Tag field name for internally/adjacently tagged enums - returns builder call only if present
    let tag_call = {
        if let Some(tag) = pe.container.attrs.get_builtin_args("tag") {
            quote! { .tag(#tag) }
        } else {
            quote! {}
        }
    };

    // Content field name for adjacently tagged enums - returns builder call only if present
    let content_call = {
        if let Some(content) = pe.container.attrs.get_builtin_args("content") {
            quote! { .content(#content) }
        } else {
            quote! {}
        }
    };

    // Untagged flag - returns builder call only if present
    let untagged_call = {
        if pe.container.attrs.has_builtin("untagged") {
            quote! { .untagged() }
        } else {
            quote! {}
        }
    };

    let is_numeric_call = {
        if pe.container.attrs.has_builtin("is_numeric") {
            quote! { .is_numeric() }
        } else {
            quote! {}
        }
    };

    // POD flag - marks type as Plain Old Data (no invariants)
    let pod_call = if pe.container.attrs.has_builtin("pod") {
        quote! { .pod() }
    } else {
        quote! {}
    };

    // Container-level proxy from PEnum - generates ProxyDef with conversion functions
    let proxy_call = {
        if let Some(attr) = pe
            .container
            .attrs
            .facet
            .iter()
            .find(|a| a.is_builtin() && a.key_str() == "proxy")
        {
            let proxy_type = &attr.args;
            let enum_type = &enum_name;
            let bgp_display = pe.container.bgp.display_without_bounds();

            quote! {
                .proxy(&const {
                    extern crate alloc as __alloc;

                    unsafe fn __proxy_convert_in(
                        proxy_ptr: #facet_crate::PtrConst,
                        field_ptr: #facet_crate::PtrUninit,
                    ) -> ::core::result::Result<#facet_crate::PtrMut, __alloc::string::String> {
                        let proxy: #proxy_type = proxy_ptr.read();
                        match <#enum_type #bgp_display as ::core::convert::TryFrom<#proxy_type>>::try_from(proxy) {
                            𝟋Ok(value) => 𝟋Ok(field_ptr.put(value)),
                            𝟋Err(e) => 𝟋Err(__alloc::string::ToString::to_string(&e)),
                        }
                    }

                    unsafe fn __proxy_convert_out(
                        field_ptr: #facet_crate::PtrConst,
                        proxy_ptr: #facet_crate::PtrUninit,
                    ) -> ::core::result::Result<#facet_crate::PtrMut, __alloc::string::String> {
                        let field_ref: &#enum_type #bgp_display = field_ptr.get();
                        match <#proxy_type as ::core::convert::TryFrom<&#enum_type #bgp_display>>::try_from(field_ref) {
                            𝟋Ok(proxy) => 𝟋Ok(proxy_ptr.put(proxy)),
                            𝟋Err(e) => 𝟋Err(__alloc::string::ToString::to_string(&e)),
                        }
                    }

                    #facet_crate::ProxyDef {
                        shape: <#proxy_type as #facet_crate::Facet>::SHAPE,
                        convert_in: __proxy_convert_in,
                        convert_out: __proxy_convert_out,
                    }
                })
            }
        } else {
            quote! {}
        }
    };

    // Determine enum repr (already resolved by PEnum::parse())
    let valid_repr = &pe.repr;

    // Are these relevant for enums? Or is it always `repr(C)` if a `PrimitiveRepr` is present?
    let repr = match &valid_repr {
        PRepr::Transparent => unreachable!("this should be caught by PRepr::parse"),
        PRepr::Rust(_) => quote! { 𝟋Repr::RUST },
        PRepr::C(_) => quote! { 𝟋Repr::C },
        PRepr::RustcWillCatch => {
            // rustc will emit the error - return empty TokenStream
            return quote! {};
        }
    };

    // Helper for EnumRepr TS (token stream) generation for primitives
    // Uses prelude alias 𝟋ERpr for compact output
    let enum_repr_ts_from_primitive = |primitive_repr: PrimitiveRepr| -> TokenStream {
        let type_name_str = primitive_repr.type_name().to_string();
        let enum_repr_variant_ident = format_ident!("{}", type_name_str.to_uppercase());
        quote! { 𝟋ERpr::#enum_repr_variant_ident }
    };

    // --- Processing code for shadow struct/fields/variant_expressions ---
    // A. C-style enums have shadow-discriminant, shadow-union, shadow-struct
    // B. Primitive enums have simpler layout.
    let (shadow_struct_defs, variant_expressions, enum_repr_type_tokenstream) = match valid_repr {
        PRepr::C(prim_opt) => {
            // Shadow discriminant
            let shadow_discriminant_name = quote::format_ident!("_D");
            let all_variant_names: Vec<Ident> = pe
                .variants
                .iter()
                .map(|pv| match &pv.name.raw {
                    IdentOrLiteral::Ident(id) => id.clone(),
                    IdentOrLiteral::Literal(n) => format_ident!("_{}", n), // Should not happen for enums
                })
                .collect();

            let repr_attr_content = match prim_opt {
                Some(p) => p.type_name(),
                None => quote! { C },
            };
            let mut shadow_defs = vec![quote! {
                #[repr(#repr_attr_content)]
                #[allow(dead_code)]
                enum #shadow_discriminant_name { #(#all_variant_names),* }
            }];

            // Shadow union
            let shadow_union_name = quote::format_ident!("_U");
            let facet_bgp = bgp.with_lifetime(LifetimeName(format_ident!("ʄ")));
            let bgp_with_bounds = facet_bgp.display_with_bounds();
            let bgp_without_bounds = facet_bgp.display_without_bounds();
            let phantom_data = facet_bgp.display_as_phantom_data();
            let all_union_fields: Vec<TokenStream> = pe.variants.iter().map(|pv| {
                // Each field is named after the variant, struct for its fields.
                let variant_ident = match &pv.name.raw {
                    IdentOrLiteral::Ident(id) => id.clone(),
                     IdentOrLiteral::Literal(idx) => format_ident!("_{}", idx), // Should not happen
                };
                let shadow_field_name_ident = quote::format_ident!("_F{}", variant_ident);
                quote! {
                    #variant_ident: ::core::mem::ManuallyDrop<#shadow_field_name_ident #bgp_without_bounds>
                }
            }).collect();

            shadow_defs.push(quote! {
                #[repr(C)]
                #[allow(non_snake_case, dead_code)]
                union #shadow_union_name #bgp_with_bounds #where_clauses { #(#all_union_fields),* }
            });

            // Shadow repr struct for enum as a whole
            let shadow_repr_name = quote::format_ident!("_R");
            shadow_defs.push(quote! {
                #[repr(C)]
                #[allow(non_snake_case)]
                #[allow(dead_code)]
                struct #shadow_repr_name #bgp_with_bounds #where_clauses {
                    _discriminant: #shadow_discriminant_name,
                    _phantom: #phantom_data,
                    _fields: #shadow_union_name #bgp_without_bounds,
                }
            });

            // Generate variant_expressions
            let mut discriminant: Option<&TokenStream> = None;
            let mut discriminant_offset: i64 = 0;
            let mut exprs = Vec::new();

            for pv in pe.variants.iter() {
                if let Some(dis) = &pv.discriminant {
                    discriminant = Some(dis);
                    discriminant_offset = 0;
                }

                // Only cast to i64 when we have a user-provided discriminant expression
                let discriminant_ts = if let Some(discriminant) = discriminant {
                    if discriminant_offset > 0 {
                        let offset_lit = Literal::i64_unsuffixed(discriminant_offset);
                        quote! { (#discriminant + #offset_lit) as i64 }
                    } else {
                        quote! { #discriminant as i64 }
                    }
                } else {
                    // Simple unsuffixed literal
                    let lit = Literal::i64_unsuffixed(discriminant_offset);
                    quote! { #lit }
                };

                let variant_name = &pv.name.original;
                let name_token = TokenTree::Literal(Literal::string(variant_name));
                let rename_token: Option<TokenStream> =
                    pv.name.rename.as_ref().map(|r| quote! { #r });
                let variant_attributes: Option<TokenStream> = if pv.attrs.facet.is_empty() {
                    None
                } else {
                    let attrs_list: Vec<TokenStream> = pv
                        .attrs
                        .facet
                        .iter()
                        .map(|attr| {
                            let ext_attr = emit_attr(attr, &facet_crate);
                            quote! { #ext_attr }
                        })
                        .collect();
                    Some(quote! { &const {[#(#attrs_list),*]} })
                };

                #[cfg(feature = "doc")]
                let variant_doc: Option<TokenStream> =
                    if pv.attrs.doc.is_empty() || crate::is_no_doc() {
                        None
                    } else {
                        let doc_lines = &pv.attrs.doc;
                        Some(quote! { &[#(#doc_lines),*] })
                    };
                #[cfg(not(feature = "doc"))]
                let variant_doc: Option<TokenStream> = None;

                let shadow_struct_name = match &pv.name.raw {
                    IdentOrLiteral::Ident(id) => quote::format_ident!("_F{}", id),
                    IdentOrLiteral::Literal(idx) => quote::format_ident!("_F{}", idx),
                };

                let variant_offset = quote! {
                    ::core::mem::offset_of!(#shadow_repr_name #bgp_without_bounds, _fields)
                };

                // Determine field structure for the variant
                match &pv.kind {
                    PVariantKind::Unit => {
                        // Generate unit shadow struct for the variant
                        shadow_defs.push(quote! {
                            #[repr(C)]
                            #[allow(non_snake_case, dead_code)]
                            struct #shadow_struct_name #bgp_with_bounds #where_clauses { _phantom: #phantom_data }
                        });
                        let variant = gen_unit_variant(
                            &name_token,
                            rename_token.as_ref(),
                            &discriminant_ts,
                            variant_attributes.as_ref(),
                            variant_doc.as_ref(),
                        );
                        exprs.push(variant);
                    }
                    PVariantKind::Tuple { fields } => {
                        // Tuple shadow struct
                        let fields_with_types: Vec<TokenStream> = fields
                            .iter()
                            .enumerate()
                            .map(|(idx, pf)| {
                                let field_ident = format_ident!("_{}", idx);
                                let typ = &pf.ty;
                                quote! { #field_ident: #typ }
                            })
                            .collect();
                        shadow_defs.push(quote! {
                            #[repr(C)]
                            #[allow(non_snake_case, dead_code)]
                            struct #shadow_struct_name #bgp_with_bounds #where_clauses {
                                #(#fields_with_types),* ,
                                _phantom: #phantom_data
                            }
                        });
                        let field_defs: Vec<TokenStream> = fields
                            .iter()
                            .enumerate()
                            .map(|(idx, pf)| {
                                let mut pfield = pf.clone();
                                let field_ident = format_ident!("_{}", idx);
                                pfield.name.raw = IdentOrLiteral::Ident(field_ident);
                                gen_field_from_pfield(
                                    &pfield,
                                    &shadow_struct_name,
                                    &facet_bgp,
                                    Some(variant_offset.clone()),
                                    &facet_crate,
                                    skip_all_unless_truthy,
                                )
                            })
                            .collect();
                        let kind = quote! { 𝟋Sk::TupleStruct };
                        let variant = gen_variant(
                            &name_token,
                            rename_token.as_ref(),
                            &discriminant_ts,
                            variant_attributes.as_ref(),
                            &kind,
                            &quote! { fields },
                            variant_doc.as_ref(),
                        );
                        exprs.push(quote! {{
                            let fields: &'static [𝟋Fld] = &const {[
                                #(#field_defs),*
                            ]};
                            #variant
                        }});
                    }
                    PVariantKind::Struct { fields } => {
                        let fields_with_types: Vec<TokenStream> = fields
                            .iter()
                            .map(|pf| {
                                // Use raw name for struct field definition
                                let field_name = match &pf.name.raw {
                                    IdentOrLiteral::Ident(id) => quote! { #id },
                                    IdentOrLiteral::Literal(_) => {
                                        panic!("Struct variant cannot have literal field names")
                                    }
                                };
                                let typ = &pf.ty;
                                quote! { #field_name: #typ }
                            })
                            .collect();

                        // Handle empty fields case explicitly
                        let struct_fields = if fields_with_types.is_empty() {
                            // Only add phantom data for empty struct variants
                            quote! { _phantom: #phantom_data }
                        } else {
                            // Add fields plus phantom data for non-empty struct variants
                            quote! { #(#fields_with_types),*, _phantom: #phantom_data }
                        };
                        shadow_defs.push(quote! {
                            #[repr(C)]
                            #[allow(non_snake_case, dead_code)]
                            struct #shadow_struct_name #bgp_with_bounds #where_clauses {
                                #struct_fields
                            }
                        });

                        let field_defs: Vec<TokenStream> = fields
                            .iter()
                            .map(|pf| {
                                gen_field_from_pfield(
                                    pf,
                                    &shadow_struct_name,
                                    &facet_bgp,
                                    Some(variant_offset.clone()),
                                    &facet_crate,
                                    skip_all_unless_truthy,
                                )
                            })
                            .collect();

                        let kind = quote! { 𝟋Sk::Struct };
                        let variant = gen_variant(
                            &name_token,
                            rename_token.as_ref(),
                            &discriminant_ts,
                            variant_attributes.as_ref(),
                            &kind,
                            &quote! { fields },
                            variant_doc.as_ref(),
                        );
                        exprs.push(quote! {{
                            let fields: &'static [𝟋Fld] = &const {[
                                #(#field_defs),*
                            ]};
                            #variant
                        }});
                    }
                };

                // C-style enums increment discriminant unless explicitly set
                discriminant_offset += 1;
            }

            // Generate the EnumRepr token stream (uses prelude alias 𝟋ERpr)
            let repr_type_ts = match prim_opt {
                None => {
                    quote! { 𝟋ERpr::from_discriminant_size::<#shadow_discriminant_name>() }
                }
                Some(p) => enum_repr_ts_from_primitive(*p),
            };

            (shadow_defs, exprs, repr_type_ts)
        }
        PRepr::Rust(Some(prim)) => {
            // Treat as primitive repr
            let facet_bgp = bgp.with_lifetime(LifetimeName(format_ident!("ʄ")));
            let bgp_with_bounds = facet_bgp.display_with_bounds();
            let phantom_data = facet_bgp.display_as_phantom_data();
            let discriminant_rust_type = prim.type_name();
            let mut shadow_defs = Vec::new();

            // Generate variant_expressions
            let mut discriminant: Option<&TokenStream> = None;
            let mut discriminant_offset: i64 = 0;

            let mut exprs = Vec::new();

            for pv in pe.variants.iter() {
                if let Some(dis) = &pv.discriminant {
                    discriminant = Some(dis);
                    discriminant_offset = 0;
                }

                // Only cast to i64 when we have a user-provided discriminant expression
                let discriminant_ts = if let Some(discriminant) = discriminant {
                    if discriminant_offset > 0 {
                        let offset_lit = Literal::i64_unsuffixed(discriminant_offset);
                        quote! { (#discriminant + #offset_lit) as i64 }
                    } else {
                        quote! { #discriminant as i64 }
                    }
                } else {
                    // Simple unsuffixed literal
                    let lit = Literal::i64_unsuffixed(discriminant_offset);
                    quote! { #lit }
                };

                let variant_name = &pv.name.original;
                let name_token = TokenTree::Literal(Literal::string(variant_name));
                let rename_token: Option<TokenStream> =
                    pv.name.rename.as_ref().map(|r| quote! { #r });
                let variant_attributes: Option<TokenStream> = if pv.attrs.facet.is_empty() {
                    None
                } else {
                    let attrs_list: Vec<TokenStream> = pv
                        .attrs
                        .facet
                        .iter()
                        .map(|attr| {
                            let ext_attr = emit_attr(attr, &facet_crate);
                            quote! { #ext_attr }
                        })
                        .collect();
                    Some(quote! { &const {[#(#attrs_list),*]} })
                };

                #[cfg(feature = "doc")]
                let variant_doc: Option<TokenStream> =
                    if pv.attrs.doc.is_empty() || crate::is_no_doc() {
                        None
                    } else {
                        let doc_lines = &pv.attrs.doc;
                        Some(quote! { &[#(#doc_lines),*] })
                    };
                #[cfg(not(feature = "doc"))]
                let variant_doc: Option<TokenStream> = None;

                match &pv.kind {
                    PVariantKind::Unit => {
                        let variant = gen_unit_variant(
                            &name_token,
                            rename_token.as_ref(),
                            &discriminant_ts,
                            variant_attributes.as_ref(),
                            variant_doc.as_ref(),
                        );
                        exprs.push(variant);
                    }
                    PVariantKind::Tuple { fields } => {
                        let shadow_struct_name = match &pv.name.raw {
                            IdentOrLiteral::Ident(id) => {
                                quote::format_ident!("_T{}", id)
                            }
                            IdentOrLiteral::Literal(_) => {
                                panic!(
                                    "Enum variant names cannot be literals for tuple variants in #[repr(Rust)]"
                                )
                            }
                        };
                        let fields_with_types: Vec<TokenStream> = fields
                            .iter()
                            .enumerate()
                            .map(|(idx, pf)| {
                                let field_ident = format_ident!("_{}", idx);
                                let typ = &pf.ty;
                                quote! { #field_ident: #typ }
                            })
                            .collect();
                        shadow_defs.push(quote! {
                            #[repr(C)] // Layout variants like C structs
                            #[allow(non_snake_case, dead_code)]
                            struct #shadow_struct_name #bgp_with_bounds #where_clauses {
                                _discriminant: #discriminant_rust_type,
                                _phantom: #phantom_data,
                                #(#fields_with_types),*
                            }
                        });
                        let field_defs: Vec<TokenStream> = fields
                            .iter()
                            .enumerate()
                            .map(|(idx, pf)| {
                                let mut pf = pf.clone();
                                let field_ident = format_ident!("_{}", idx);
                                pf.name.raw = IdentOrLiteral::Ident(field_ident);
                                gen_field_from_pfield(
                                    &pf,
                                    &shadow_struct_name,
                                    &facet_bgp,
                                    None,
                                    &facet_crate,
                                    skip_all_unless_truthy,
                                )
                            })
                            .collect();
                        let kind = quote! { 𝟋Sk::TupleStruct };
                        let variant = gen_variant(
                            &name_token,
                            rename_token.as_ref(),
                            &discriminant_ts,
                            variant_attributes.as_ref(),
                            &kind,
                            &quote! { fields },
                            variant_doc.as_ref(),
                        );
                        exprs.push(quote! {{
                            let fields: &'static [𝟋Fld] = &const {[
                                #(#field_defs),*
                            ]};
                            #variant
                        }});
                    }
                    PVariantKind::Struct { fields } => {
                        let shadow_struct_name = match &pv.name.raw {
                            IdentOrLiteral::Ident(id) => {
                                quote::format_ident!("_S{}", id)
                            }
                            IdentOrLiteral::Literal(_) => {
                                panic!(
                                    "Enum variant names cannot be literals for struct variants in #[repr(Rust)]"
                                )
                            }
                        };
                        let fields_with_types: Vec<TokenStream> = fields
                            .iter()
                            .map(|pf| {
                                let field_name = match &pf.name.raw {
                                    IdentOrLiteral::Ident(id) => quote! { #id },
                                    IdentOrLiteral::Literal(_) => {
                                        panic!("Struct variant cannot have literal field names")
                                    }
                                };
                                let typ = &pf.ty;
                                quote! { #field_name: #typ }
                            })
                            .collect();
                        shadow_defs.push(quote! {
                            #[repr(C)] // Layout variants like C structs
                            #[allow(non_snake_case, dead_code)]
                            struct #shadow_struct_name #bgp_with_bounds #where_clauses {
                                _discriminant: #discriminant_rust_type,
                                _phantom: #phantom_data,
                                #(#fields_with_types),*
                            }
                        });
                        let field_defs: Vec<TokenStream> = fields
                            .iter()
                            .map(|pf| {
                                gen_field_from_pfield(
                                    pf,
                                    &shadow_struct_name,
                                    &facet_bgp,
                                    None,
                                    &facet_crate,
                                    skip_all_unless_truthy,
                                )
                            })
                            .collect();
                        let kind = quote! { 𝟋Sk::Struct };
                        let variant = gen_variant(
                            &name_token,
                            rename_token.as_ref(),
                            &discriminant_ts,
                            variant_attributes.as_ref(),
                            &kind,
                            &quote! { fields },
                            variant_doc.as_ref(),
                        );
                        exprs.push(quote! {{
                            let fields: &'static [𝟋Fld] = &const {[
                                #(#field_defs),*
                            ]};
                            #variant
                        }});
                    }
                }
                // Rust-style enums increment discriminant unless explicitly set
                discriminant_offset += 1;
            }
            let repr_type_ts = enum_repr_ts_from_primitive(*prim);
            (shadow_defs, exprs, repr_type_ts)
        }
        PRepr::Transparent => {
            return quote! {
                compile_error!("#[repr(transparent)] is not supported on enums by Facet");
            };
        }
        PRepr::Rust(None) => {
            return quote! {
                compile_error!("Facet requires enums to have an explicit representation (e.g., #[repr(C)], #[repr(u8)])");
            };
        }
        PRepr::RustcWillCatch => {
            // rustc will emit an error for the invalid repr (e.g., conflicting hints).
            // Return empty TokenStream so we don't add misleading errors.
            return quote! {};
        }
    };

    // Static decl removed - the TYPENAME_SHAPE static was redundant since
    // <T as Facet>::SHAPE is already accessible and nobody was using the static

    // Set up generics for impl blocks
    let facet_bgp = bgp.with_lifetime(LifetimeName(format_ident!("ʄ")));
    let bgp_def = facet_bgp.display_with_bounds();
    let bgp_without_bounds = bgp.display_without_bounds();

    let (ty_field, fields) = if opaque {
        (
            quote! {
                𝟋Ty::User(𝟋UTy::Opaque)
            },
            quote! {},
        )
    } else {
        // Inline the const block directly into the builder call
        (
            quote! {
                𝟋Ty::User(𝟋UTy::Enum(
                    𝟋ETyB::new(#enum_repr_type_tokenstream, &const {[
                        #(#variant_expressions),*
                    ]})
                        .repr(#repr)
                        .build()
                ))
            },
            quote! {},
        )
    };

    // Generate constructor expressions to suppress dead_code warnings on enum variants.
    // When variants are constructed via reflection (e.g., facet_args::from_std_args()),
    // the compiler doesn't see them being used and warns about dead code.
    // This ensures all variants are "constructed" from the compiler's perspective.
    // We use explicit type annotations to help inference with const generics and
    // unused type parameters.
    let variant_constructors: Vec<TokenStream> = pe
        .variants
        .iter()
        .map(|pv| {
            let variant_ident = match &pv.name.raw {
                IdentOrLiteral::Ident(id) => id.clone(),
                IdentOrLiteral::Literal(n) => format_ident!("_{}", n),
            };
            match &pv.kind {
                PVariantKind::Unit => quote! { let _: #enum_name #bgp_without_bounds = #enum_name::#variant_ident },
                PVariantKind::Tuple { fields } => {
                    let loops = fields.iter().map(|_| quote! { loop {} });
                    quote! { let _: #enum_name #bgp_without_bounds = #enum_name::#variant_ident(#(#loops),*) }
                }
                PVariantKind::Struct { fields } => {
                    let field_inits: Vec<TokenStream> = fields
                        .iter()
                        .map(|pf| {
                            let field_name = match &pf.name.raw {
                                IdentOrLiteral::Ident(id) => id.clone(),
                                IdentOrLiteral::Literal(n) => format_ident!("_{}", n),
                            };
                            quote! { #field_name: loop {} }
                        })
                        .collect();
                    quote! { let _: #enum_name #bgp_without_bounds = #enum_name::#variant_ident { #(#field_inits),* } }
                }
            }
        })
        .collect();

    // Compute variance - for non-opaque types, use BIVARIANT which falls back to field walking
    let variance_call = if opaque {
        // Opaque types don't expose internals, use invariant for safety
        quote! { .variance(𝟋VncD::INVARIANT) }
    } else {
        // Use BIVARIANT - the computed_variance_impl will walk fields when deps is empty
        quote! { .variance(𝟋CV) }
    };

    // TypeOps for drop, default, clone - convert Option<TokenStream> to a call
    let type_ops_call = match type_ops_init {
        Some(ops) => quote! { .type_ops(#ops) },
        None => quote! {},
    };

    // Type name function - for generic types, this formats with type parameters
    let type_name_call = if parsed.generics.is_some() && !opaque {
        quote! { .type_name(#type_name_fn) }
    } else {
        quote! {}
    };

    // Generate static assertions for declared traits (catches lies at compile time)
    // We put this in a generic function outside the const block so it can reference generic parameters
    let facet_default = pe.container.attrs.has_builtin("default");
    let trait_assertion_fn = if let Some(bounds) =
        gen_trait_bounds(pe.container.attrs.declared_traits.as_ref(), facet_default)
    {
        // Note: where_clauses_tokens already includes "where" keyword if non-empty
        // We need to add the trait bounds as an additional constraint
        quote! {
            const _: () = {
                #[allow(dead_code, clippy::multiple_bound_locations)]
                fn __facet_assert_traits #bgp_def (_: &#enum_name #bgp_without_bounds)
                where
                    #enum_name #bgp_without_bounds: #bounds
                {}
            };
        }
    } else {
        quote! {}
    };

    // from_ref / try_from_ref handling - generates try_from function
    // Similar to proxy, we define helper functions in an inherent impl
    let from_ref_inherent_impl = {
        // Look for from_ref or try_from_ref attribute
        let from_ref_attr = pe
            .container
            .attrs
            .facet
            .iter()
            .find(|a| a.is_builtin() && a.key_str() == "from_ref");
        let try_from_ref_attr = pe
            .container
            .attrs
            .facet
            .iter()
            .find(|a| a.is_builtin() && a.key_str() == "try_from_ref");

        if let Some(func_attr) = from_ref_attr.or(try_from_ref_attr) {
            let is_fallible = try_from_ref_attr.is_some();

            // Use raw tokens directly (like proxy does)
            let func_path = &func_attr.args;

            let enum_type = enum_name;
            let helper_bgp = pe
                .container
                .bgp
                .with_lifetime(LifetimeName(format_ident!("ʄ")));
            let bgp_def_for_helper = helper_bgp.display_with_bounds();

            let (helper_fn, helper_call, unwrap_val) = if is_fallible {
                (
                    quote! {
                        #[inline]
                        const fn __facet_get_src_ref_shape<'f, F, Ref: #facet_crate::Facet<'f> + 'f, Out, Err>(_fn: &F) -> &'static #facet_crate::Shape
                        where
                            F: Fn(Ref) -> ::core::result::Result<Out, Err>,
                        {
                            Ref::SHAPE
                        }
                    },
                    quote! { __facet_get_src_ref_shape::<_, _, Self, _>(&#func_path) },
                    quote! {
                       match value {
                            ::core::result::Result::Ok(v) => v,
                            ::core::result::Result::Err(e) => { return #facet_crate::TryFromOutcome::Failed(__alloc::string::ToString::to_string(&e).into()) }
                       }
                    },
                )
            } else {
                (
                    quote! {
                        #[inline]
                        const fn __facet_get_src_ref_shape<'f, F, Ref: #facet_crate::Facet<'f> + 'f, Out>(_fn: &F) -> &'static #facet_crate::Shape
                        where
                            F: Fn(Ref) -> Out,
                        {
                            Ref::SHAPE
                        }
                    },
                    quote! { __facet_get_src_ref_shape::<_, _, Self>(&#func_path) },
                    quote! { value },
                )
            };

            quote! {
                #[doc(hidden)]
                impl #bgp_def_for_helper #enum_type #bgp_without_bounds
                #where_clauses
                {
                    /// try_from function for VTableDirect (raw pointer signature)
                    #[doc(hidden)]
                    unsafe fn __facet_try_from_ref(
                        dst: *mut Self,
                        src_shape: &'static #facet_crate::Shape,
                        src: #facet_crate::PtrConst,
                    ) -> #facet_crate::TryFromOutcome {
                        extern crate alloc as __alloc;

                        #helper_fn

                        // Ensure source shape matches the expected reference type
                        if src_shape.id != #helper_call.id {
                            return #facet_crate::TryFromOutcome::Unsupported;
                        }
                        let value = #func_path(unsafe { src.get() });
                        unsafe { dst.write(#unwrap_val) };
                        #facet_crate::TryFromOutcome::Converted
                    }

                    /// try_from wrapper for VTableIndirect (OxPtrUninit signature)
                    #[doc(hidden)]
                    unsafe fn __facet_try_from_ref_indirect(
                        dst: #facet_crate::OxPtrUninit,
                        src_shape: &'static #facet_crate::Shape,
                        src: #facet_crate::PtrConst,
                    ) -> #facet_crate::TryFromOutcome {
                        Self::__facet_try_from_ref(
                            dst.ptr().as_mut_byte_ptr() as *mut Self,
                            src_shape,
                            src,
                        )
                    }
                }
            }
        } else {
            // No from_ref/try_from_ref attribute, or missing ref_type (validated earlier)
            quote! {}
        }
    };

    // Static declaration for release builds (pre-evaluates SHAPE)
    let static_decl =
        crate::derive::generate_static_decl(enum_name, &facet_crate, has_type_or_const_generics);

    // Generate phantom use block for IDE hover support on attribute names.
    // This links attribute spans to facet::builtin::Attr variants.
    let phantom_attr_block = if phantom_attr_uses.is_empty() {
        quote! {}
    } else {
        quote! {
            const _: () = { #(#phantom_attr_uses)* };
        }
    };

    // Generate the impl
    quote! {
        #phantom_attr_block

        // Suppress dead_code warnings for enum variants constructed via reflection.
        // See: https://github.com/facet-rs/facet/issues/996
        const _: () = {
            #[allow(dead_code, unreachable_code, clippy::multiple_bound_locations, clippy::diverging_sub_expression)]
            fn __facet_construct_all_variants #bgp_def () -> #enum_name #bgp_without_bounds #where_clauses {
                loop {
                    #(#variant_constructors;)*
                }
            }
        };

        #trait_assertion_fn

        #[automatically_derived]
        #[allow(non_camel_case_types)]
        unsafe impl #bgp_def #facet_crate::Facet<'ʄ> for #enum_name #bgp_without_bounds #where_clauses {
            const SHAPE: &'static #facet_crate::Shape = &const {
                use #facet_crate::𝟋::*;
                #(#shadow_struct_defs)*
                #fields
                𝟋ShpB::for_sized::<Self>(#enum_name_str)
                    .module_path(::core::module_path!())
                    #decl_id_call
                    #source_location_call
                    .vtable(#vtable_init)
                    #type_ops_call
                    .ty(#ty_field)
                    .def(𝟋Def::Undefined)
                    #type_params_call
                    #type_name_call
                    #doc_call
                    #attributes_call
                    #type_tag_call
                    #tag_call
                    #content_call
                    #untagged_call
                    #is_numeric_call
                    #pod_call
                    #proxy_call
                    #variance_call
                    .build()
            };
        }

        #static_decl

        // from_ref inherent impl
        #from_ref_inherent_impl
    }
}
