use super::*;
use crate::process_struct::{TraitSources, gen_field_from_pfield, gen_trait_bounds, gen_vtable};
use quote::{format_ident, quote, quote_spanned};

/// Generate a Variant using VariantBuilder for more compact output.
///
/// NOTE: This function generates code that uses short aliases from the 𝟋 prelude.
/// It MUST be called within a context where `use #facet_crate::𝟋::*` has been emitted.
fn gen_variant(
    name: impl quote::ToTokens,
    discriminant: impl quote::ToTokens,
    attributes: impl quote::ToTokens,
    struct_kind: impl quote::ToTokens,
    fields: impl quote::ToTokens,
    doc: impl quote::ToTokens,
) -> TokenStream {
    quote! {
        𝟋VarB::new(
            #name,
            𝟋STyB::new(#struct_kind, #fields).build()
        )
        .discriminant(Some(#discriminant as _))
        .attributes(#attributes)
        .doc(#doc)
        .build()
    }
}

/// Generate a unit variant using the pre-built StructType::UNIT constant.
/// NOTE: This function generates code that uses short aliases from the 𝟋 prelude.
/// It MUST be called within a context where `use #facet_crate::𝟋::*` has been emitted.
fn gen_unit_variant(
    name: impl quote::ToTokens,
    discriminant: impl quote::ToTokens,
    attributes: impl quote::ToTokens,
    doc: impl quote::ToTokens,
) -> TokenStream {
    quote! {
        𝟋VarB::new(#name, 𝟋STy::UNIT)
            .discriminant(Some(#discriminant as _))
            .attributes(#attributes)
            .doc(#doc)
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

    let enum_name = &pe.container.name;
    let enum_name_str = enum_name.to_string();

    let opaque = pe
        .container
        .attrs
        .facet
        .iter()
        .any(|a| a.is_builtin() && a.key_str() == "opaque");

    // Get the facet crate path (custom or default ::facet)
    let facet_crate = pe.container.attrs.facet_crate();

    let type_name_fn =
        generate_type_name_fn(enum_name, parsed.generics.as_ref(), opaque, &facet_crate);

    // Determine trait sources and generate vtable accordingly
    let trait_sources = TraitSources::from_attrs(&pe.container.attrs);
    let vtable_code = gen_vtable(&facet_crate, &type_name_fn, &trait_sources);
    let vtable_init = quote! { const { #vtable_code } };

    let bgp = pe.container.bgp.clone();
    // Use the AST directly for where clauses and generics, as PContainer/PEnum doesn't store them
    let where_clauses_tokens = build_where_clauses(
        parsed.clauses.as_ref(),
        parsed.generics.as_ref(),
        opaque,
        &facet_crate,
    );
    let type_params_call = build_type_params_call(parsed.generics.as_ref(), opaque, &facet_crate);

    // Container-level docs - returns builder call only if there are doc comments and doc feature is enabled
    #[cfg(feature = "doc")]
    let doc_call = match &pe.container.attrs.doc[..] {
        [] => quote! {},
        doc_lines => quote! { .doc(&[#(#doc_lines),*]) },
    };
    #[cfg(not(feature = "doc"))]
    let doc_call = quote! {};

    // Container attributes - returns builder call only if there are attributes
    let attributes_call = {
        let mut attribute_tokens: Vec<TokenStream> = Vec::new();
        for attr in &pe.container.attrs.facet {
            // These attributes are handled specially and not emitted to runtime:
            // - crate: sets the facet crate path
            // - traits: compile-time directive for vtable generation
            // - auto_traits: compile-time directive for vtable generation
            if attr.is_builtin() {
                let key = attr.key_str();
                if matches!(key.as_str(), "crate" | "traits" | "auto_traits" | "proxy") {
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

                    unsafe fn __proxy_convert_in<'mem>(
                        proxy_ptr: #facet_crate::PtrConst<'mem>,
                        field_ptr: #facet_crate::PtrUninit<'mem>,
                    ) -> ::core::result::Result<#facet_crate::PtrMut<'mem>, __alloc::string::String> {
                        let proxy: #proxy_type = proxy_ptr.read();
                        match <#enum_type #bgp_display as ::core::convert::TryFrom<#proxy_type>>::try_from(proxy) {
                            ::core::result::Result::Ok(value) => ::core::result::Result::Ok(field_ptr.put(value)),
                            ::core::result::Result::Err(e) => ::core::result::Result::Err(__alloc::string::ToString::to_string(&e)),
                        }
                    }

                    unsafe fn __proxy_convert_out<'mem>(
                        field_ptr: #facet_crate::PtrConst<'mem>,
                        proxy_ptr: #facet_crate::PtrUninit<'mem>,
                    ) -> ::core::result::Result<#facet_crate::PtrMut<'mem>, __alloc::string::String> {
                        let field_ref: &#enum_type #bgp_display = field_ptr.get();
                        match <#proxy_type as ::core::convert::TryFrom<&#enum_type #bgp_display>>::try_from(field_ref) {
                            ::core::result::Result::Ok(proxy) => ::core::result::Result::Ok(proxy_ptr.put(proxy)),
                            ::core::result::Result::Err(e) => ::core::result::Result::Err(__alloc::string::ToString::to_string(&e)),
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
    let enum_repr_ts_from_primitive = |primitive_repr: PrimitiveRepr| -> TokenStream {
        let type_name_str = primitive_repr.type_name().to_string();
        let enum_repr_variant_ident = format_ident!("{}", type_name_str.to_uppercase());
        quote! { #facet_crate::EnumRepr::#enum_repr_variant_ident }
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
                union #shadow_union_name #bgp_with_bounds #where_clauses_tokens { #(#all_union_fields),* }
            });

            // Shadow repr struct for enum as a whole
            let shadow_repr_name = quote::format_ident!("_R");
            shadow_defs.push(quote! {
                #[repr(C)]
                #[allow(non_snake_case)]
                #[allow(dead_code)]
                struct #shadow_repr_name #bgp_with_bounds #where_clauses_tokens {
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

                let discriminant_ts = if let Some(discriminant) = discriminant {
                    if discriminant_offset > 0 {
                        quote! { #discriminant + #discriminant_offset }
                    } else {
                        quote! { #discriminant }
                    }
                } else {
                    quote! { #discriminant_offset }
                };

                let display_name = pv.name.effective.clone();
                let name_token = TokenTree::Literal(Literal::string(&display_name));
                let variant_attributes = {
                    if pv.attrs.facet.is_empty() {
                        quote! { &[] }
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
                        quote! { &const {[#(#attrs_list),*]} }
                    }
                };

                #[cfg(feature = "doc")]
                let variant_doc = match &pv.attrs.doc[..] {
                    [] => quote! { &[] },
                    doc_lines => quote! { &[#(#doc_lines),*] },
                };
                #[cfg(not(feature = "doc"))]
                let variant_doc = quote! { &[] };

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
                            struct #shadow_struct_name #bgp_with_bounds #where_clauses_tokens { _phantom: #phantom_data }
                        });
                        let variant = gen_unit_variant(
                            &name_token,
                            &discriminant_ts,
                            &variant_attributes,
                            &variant_doc,
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
                            struct #shadow_struct_name #bgp_with_bounds #where_clauses_tokens {
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
                                )
                            })
                            .collect();
                        let kind = quote! { 𝟋Sk::Tuple };
                        let variant = gen_variant(
                            &name_token,
                            &discriminant_ts,
                            &variant_attributes,
                            &kind,
                            &quote! { fields },
                            &variant_doc,
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
                            struct #shadow_struct_name #bgp_with_bounds #where_clauses_tokens {
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
                                )
                            })
                            .collect();

                        let kind = quote! { 𝟋Sk::Struct };
                        let variant = gen_variant(
                            &name_token,
                            &discriminant_ts,
                            &variant_attributes,
                            &kind,
                            &quote! { fields },
                            &variant_doc,
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

            // Generate the EnumRepr token stream
            let repr_type_ts = match prim_opt {
                None => {
                    quote! { #facet_crate::EnumRepr::from_discriminant_size::<#shadow_discriminant_name>() }
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

                let discriminant_ts = if let Some(discriminant) = discriminant {
                    if discriminant_offset > 0 {
                        quote! { #discriminant + #discriminant_offset }
                    } else {
                        quote! { #discriminant }
                    }
                } else {
                    quote! { #discriminant_offset }
                };

                let display_name = pv.name.effective.clone();
                let name_token = TokenTree::Literal(Literal::string(&display_name));
                let variant_attributes = {
                    if pv.attrs.facet.is_empty() {
                        quote! { &[] }
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
                        quote! { &const {[#(#attrs_list),*]} }
                    }
                };

                #[cfg(feature = "doc")]
                let variant_doc = match &pv.attrs.doc[..] {
                    [] => quote! { &[] },
                    doc_lines => quote! { &[#(#doc_lines),*] },
                };
                #[cfg(not(feature = "doc"))]
                let variant_doc = quote! { &[] };

                match &pv.kind {
                    PVariantKind::Unit => {
                        let variant = gen_unit_variant(
                            &name_token,
                            &discriminant_ts,
                            &variant_attributes,
                            &variant_doc,
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
                            struct #shadow_struct_name #bgp_with_bounds #where_clauses_tokens {
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
                                )
                            })
                            .collect();
                        let kind = quote! { 𝟋Sk::Tuple };
                        let variant = gen_variant(
                            &name_token,
                            &discriminant_ts,
                            &variant_attributes,
                            &kind,
                            &quote! { fields },
                            &variant_doc,
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
                            struct #shadow_struct_name #bgp_with_bounds #where_clauses_tokens {
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
                                )
                            })
                            .collect();
                        let kind = quote! { 𝟋Sk::Struct };
                        let variant = gen_variant(
                            &name_token,
                            &discriminant_ts,
                            &variant_attributes,
                            &kind,
                            &quote! { fields },
                            &variant_doc,
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

    // Only make static_decl for non-generic enums
    let static_decl = if parsed.generics.is_none() {
        generate_static_decl(enum_name, &facet_crate)
    } else {
        quote! {}
    };

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

    // Generate the impl
    quote! {
        #static_decl

        // Suppress dead_code warnings for enum variants constructed via reflection.
        // See: https://github.com/facet-rs/facet/issues/996
        const _: () = {
            #[allow(dead_code, unreachable_code, clippy::multiple_bound_locations, clippy::diverging_sub_expression)]
            fn __facet_construct_all_variants #bgp_def () -> #enum_name #bgp_without_bounds #where_clauses_tokens {
                loop {
                    #(#variant_constructors;)*
                }
            }
        };

        #trait_assertion_fn

        #[automatically_derived]
        #[allow(non_camel_case_types)]
        unsafe impl #bgp_def #facet_crate::Facet<'ʄ> for #enum_name #bgp_without_bounds #where_clauses_tokens {
            const SHAPE: &'static #facet_crate::Shape = &const {
                use #facet_crate::𝟋::*;
                #(#shadow_struct_defs)*
                #fields
                𝟋ShpB::for_sized::<Self>(#type_name_fn, #enum_name_str)
                    .vtable(#vtable_init)
                    .ty(#ty_field)
                    .def(𝟋Def::Undefined)
                    #type_params_call
                    #doc_call
                    #attributes_call
                    #type_tag_call
                    #proxy_call
                    .build()
            };
        }
    }
}
