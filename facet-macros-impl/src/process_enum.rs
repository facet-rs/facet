use super::*;
use crate::{
    parsed::{IdentOrLiteral, PRepr, PVariantKind, PrimitiveRepr},
    process_struct::gen_field_from_pfield,
};
use quote::{format_ident, quote, quote_spanned};

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

    let bgp = pe.container.bgp.clone();
    // Use the AST directly for where clauses and generics, as PContainer/PEnum doesn't store them
    let where_clauses_tokens = build_where_clauses(
        parsed.clauses.as_ref(),
        parsed.generics.as_ref(),
        opaque,
        &facet_crate,
    );
    let type_params = build_type_params(parsed.generics.as_ref(), opaque, &facet_crate);

    // Container-level docs from PAttrs
    let maybe_container_doc = match &pe.container.attrs.doc[..] {
        [] => quote! {},
        doc_lines => quote! { .doc(&[#(#doc_lines),*]) },
    };

    let container_attributes_tokens = {
        let mut attribute_tokens: Vec<TokenStream> = Vec::new();
        for attr in &pe.container.attrs.facet {
            // Skip crate attribute - it's handled specially
            if attr.is_builtin() && attr.key_str() == "crate" {
                continue;
            }
            // All attributes go through grammar dispatch
            let ext_attr = emit_attr(attr, &facet_crate);
            attribute_tokens.push(quote! { #ext_attr });
        }

        if attribute_tokens.is_empty() {
            quote! {}
        } else {
            quote! { .attributes(&const { [#(#attribute_tokens),*] }) }
        }
    };

    let type_tag_maybe = {
        if let Some(type_tag) = pe.container.attrs.get_builtin_args("type_tag") {
            quote! { .type_tag(#type_tag) }
        } else {
            quote! {}
        }
    };

    // Determine enum repr (already resolved by PEnum::parse())
    let valid_repr = &pe.repr;

    // Are these relevant for enums? Or is it always `repr(C)` if a `PrimitiveRepr` is present?
    let repr = match &valid_repr {
        PRepr::Transparent => unreachable!("this should be caught by PRepr::parse"),
        PRepr::Rust(_) => quote! { #facet_crate::Repr::default() },
        PRepr::C(_) => quote! { #facet_crate::Repr::c() },
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
            let shadow_discriminant_name =
                quote::format_ident!("__Shadow_CRepr_Discriminant_for_{}", enum_name_str);
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
            let shadow_union_name =
                quote::format_ident!("__Shadow_CRepr_Fields_Union_for_{}", enum_name_str);
            let facet_bgp = bgp.with_lifetime(LifetimeName(format_ident!("__facet")));
            let bgp_with_bounds = facet_bgp.display_with_bounds();
            let bgp_without_bounds = facet_bgp.display_without_bounds();
            let phantom_data = facet_bgp.display_as_phantom_data();
            let all_union_fields: Vec<TokenStream> = pe.variants.iter().map(|pv| {
                // Each field is named after the variant, struct for its fields.
                let variant_ident = match &pv.name.raw {
                    IdentOrLiteral::Ident(id) => id.clone(),
                     IdentOrLiteral::Literal(idx) => format_ident!("_{}", idx), // Should not happen
                };
                let shadow_field_name_ident = quote::format_ident!("__Shadow_CRepr_Field{}_{}", enum_name_str, variant_ident);
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
            let shadow_repr_name =
                quote::format_ident!("__Shadow_CRepr_Struct_for_{}", enum_name_str);
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
                let variant_attrs_tokens = {
                    let name_token = TokenTree::Literal(Literal::string(&display_name));
                    // All attributes go through grammar dispatch
                    if pv.attrs.facet.is_empty() {
                        quote! { .name(#name_token) }
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
                        quote! { .name(#name_token).attributes(&const { [#(#attrs_list),*] }) }
                    }
                };

                let maybe_doc = match &pv.attrs.doc[..] {
                    [] => quote! {},
                    doc_lines => quote! { .doc(&[#(#doc_lines),*]) },
                };

                let shadow_struct_name = match &pv.name.raw {
                    IdentOrLiteral::Ident(id) => {
                        // Use the same naming convention as in the union definition
                        quote::format_ident!("__Shadow_CRepr_Field{}_{}", enum_name_str, id)
                    }
                    IdentOrLiteral::Literal(idx) => {
                        // Use the same naming convention as in the union definition
                        quote::format_ident!(
                            "__Shadow_CRepr_Field{}_{}",
                            enum_name_str,
                            format_ident!("_{}", idx) // Should not happen
                        )
                    }
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
                        exprs.push(quote! {
                            #facet_crate::Variant::builder()
                                #variant_attrs_tokens
                                .discriminant(#discriminant_ts as i64)
                                .data(#facet_crate::StructType::builder().repr(#facet_crate::Repr::c()).unit().build())
                                #maybe_doc
                                .build()
                        });
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
                        exprs.push(quote! {{
                            let fields: &'static [#facet_crate::Field] = &const {[
                                #(#field_defs),*
                            ]};
                            #facet_crate::Variant::builder()
                                #variant_attrs_tokens
                                .discriminant(#discriminant_ts as i64)
                                .data(#facet_crate::StructType::builder().repr(#facet_crate::Repr::c()).tuple().fields(fields).build())
                                #maybe_doc
                                .build()
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

                        exprs.push(quote! {{
                            let fields: &'static [#facet_crate::Field] = &const {[
                                #(#field_defs),*
                            ]};
                            #facet_crate::Variant::builder()
                                #variant_attrs_tokens
                                .discriminant(#discriminant_ts as i64)
                                .data(#facet_crate::StructType::builder().repr(#facet_crate::Repr::c()).struct_().fields(fields).build())
                                #maybe_doc
                                .build()
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
            let facet_bgp = bgp.with_lifetime(LifetimeName(format_ident!("__facet")));
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
                let variant_attrs_tokens = {
                    let name_token = TokenTree::Literal(Literal::string(&display_name));
                    // All attributes go through grammar dispatch
                    if pv.attrs.facet.is_empty() {
                        quote! { .name(#name_token) }
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
                        quote! { .name(#name_token).attributes(&const { [#(#attrs_list),*] }) }
                    }
                };

                let maybe_doc = match &pv.attrs.doc[..] {
                    [] => quote! {},
                    doc_lines => quote! { .doc(&[#(#doc_lines),*]) },
                };

                match &pv.kind {
                    PVariantKind::Unit => {
                        exprs.push(quote! {
                            #facet_crate::Variant::builder()
                                #variant_attrs_tokens
                                .discriminant(#discriminant_ts as i64)
                                .data(#facet_crate::StructType::builder().repr(#facet_crate::Repr::c()).unit().build())
                                #maybe_doc
                                .build()
                        });
                    }
                    PVariantKind::Tuple { fields } => {
                        let shadow_struct_name = match &pv.name.raw {
                            IdentOrLiteral::Ident(id) => {
                                quote::format_ident!(
                                    "__Shadow_RustRepr_Tuple_for_{}_{}",
                                    enum_name_str,
                                    id
                                )
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
                        exprs.push(quote! {{
                            let fields: &'static [#facet_crate::Field] = &const {[
                                #(#field_defs),*
                            ]};
                            #facet_crate::Variant::builder()
                                #variant_attrs_tokens
                                .discriminant(#discriminant_ts as i64)
                                .data(#facet_crate::StructType::builder().repr(#facet_crate::Repr::c()).tuple().fields(fields).build())
                                #maybe_doc
                                .build()
                        }});
                    }
                    PVariantKind::Struct { fields } => {
                        let shadow_struct_name = match &pv.name.raw {
                            IdentOrLiteral::Ident(id) => {
                                // Use a more descriptive name, similar to the Tuple variant case
                                quote::format_ident!(
                                    "__Shadow_RustRepr_Struct_for_{}_{}",
                                    enum_name_str,
                                    id
                                )
                            }
                            IdentOrLiteral::Literal(_) => {
                                // This case should ideally not happen for named struct variants
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
                        exprs.push(quote! {{
                            let fields: &'static [#facet_crate::Field] = &const {[
                                #(#field_defs),*
                            ]};
                            #facet_crate::Variant::builder()
                                #variant_attrs_tokens
                                .discriminant(#discriminant_ts as i64)
                                .data(#facet_crate::StructType::builder().repr(#facet_crate::Repr::c()).struct_().fields(fields).build())
                                #maybe_doc
                                .build()
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
    let facet_bgp = bgp.with_lifetime(LifetimeName(format_ident!("__facet")));
    let bgp_def = facet_bgp.display_with_bounds();
    let bgp_without_bounds = bgp.display_without_bounds();

    let (ty, fields) = if opaque {
        (
            quote! {
                .ty(#facet_crate::Type::User(#facet_crate::UserType::Opaque))
            },
            quote! {},
        )
    } else {
        (
            quote! {
                .ty(#facet_crate::Type::User(#facet_crate::UserType::Enum(#facet_crate::EnumType::builder()
                        // Use variant expressions that just reference the shadow structs
                        // which are now defined above
                        .variants(__facet_variants)
                        .repr(#repr)
                        .enum_repr(#enum_repr_type_tokenstream)
                        .build())
                ))
            },
            quote! {
                let __facet_variants: &'static [#facet_crate::Variant] = &const {[
                    #(#variant_expressions),*
                ]};
            },
        )
    };

    // Generate constructor expressions to suppress dead_code warnings on enum variants.
    // When variants are constructed via reflection (e.g., facet_args::from_std_args()),
    // the compiler doesn't see them being used and warns about dead code.
    // This ensures all variants are "constructed" from the compiler's perspective.
    let variant_constructors: Vec<TokenStream> = pe
        .variants
        .iter()
        .map(|pv| {
            let variant_ident = match &pv.name.raw {
                IdentOrLiteral::Ident(id) => id.clone(),
                IdentOrLiteral::Literal(n) => format_ident!("_{}", n),
            };
            match &pv.kind {
                PVariantKind::Unit => quote! { #enum_name::#variant_ident },
                PVariantKind::Tuple { fields } => {
                    let todos = fields.iter().map(|_| quote! { todo!() });
                    quote! { #enum_name::#variant_ident(#(#todos),*) }
                }
                PVariantKind::Struct { fields } => {
                    let field_inits: Vec<TokenStream> = fields
                        .iter()
                        .map(|pf| {
                            let field_name = match &pf.name.raw {
                                IdentOrLiteral::Ident(id) => id.clone(),
                                IdentOrLiteral::Literal(n) => format_ident!("_{}", n),
                            };
                            quote! { #field_name: todo!() }
                        })
                        .collect();
                    quote! { #enum_name::#variant_ident { #(#field_inits),* } }
                }
            }
        })
        .collect();

    // Generate the impl
    quote! {
        #static_decl

        // Suppress dead_code warnings for enum variants constructed via reflection.
        // See: https://github.com/facet-rs/facet/issues/996
        const _: () = {
            #[allow(dead_code, unreachable_code, clippy::multiple_bound_locations, clippy::diverging_sub_expression)]
            fn __facet_construct_all_variants #bgp_def () -> #enum_name #bgp_without_bounds #where_clauses_tokens {
                loop {
                    #(let _ = #variant_constructors;)*
                }
            }
        };

        #[automatically_derived]
        #[allow(non_camel_case_types)]
        unsafe impl #bgp_def #facet_crate::Facet<'__facet> for #enum_name #bgp_without_bounds #where_clauses_tokens {
            const SHAPE: &'static #facet_crate::Shape = &const {
                #(#shadow_struct_defs)*
                #fields
                #facet_crate::Shape::builder_for_sized::<Self>()
                    .vtable({
                        #facet_crate::value_vtable!(Self, #type_name_fn)
                    })
                    .type_identifier(#enum_name_str)
                    #type_params
                    #ty
                    #maybe_container_doc
                    #container_attributes_tokens
                    #type_tag_maybe
                    .build()
            };
        }
    }
}
