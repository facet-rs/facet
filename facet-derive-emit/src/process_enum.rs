use super::*;
// Import PRepr, PrimitiveRepr, PStructField, etc. from parsed module
use crate::{
    parsed::{IdentOrLiteral, PFacetAttr, PRepr, PVariantKind, PrimitiveRepr},
    process_struct::gen_field_from_pfield,
};
use quote::{format_ident, quote};

/// Processes an enum to implement Facet
pub(crate) fn process_enum(parsed: Enum) -> TokenStream {
    // Use already-parsed PEnum, including container/variant/field attributes and rename rules
    let pe = PEnum::parse(&parsed);

    let enum_name = pe.container.name.clone();
    let enum_name_str = enum_name.to_string();
    let bgp = pe.container.bgp.clone();
    // Use the AST directly for where clauses and generics, as PContainer/PEnum doesn't store them
    let where_clauses_tokens =
        build_where_clauses(parsed.clauses.as_ref(), parsed.generics.as_ref());
    let type_params = build_type_params(parsed.generics.as_ref());

    // Container-level docs from PAttrs
    let maybe_container_doc = match &pe.container.attrs.doc[..] {
        [] => quote! {},
        doc_lines => quote! { .doc(&[#(#doc_lines),*]) },
    };

    let container_attributes_tokens = {
        let mut attribute_tokens: Vec<TokenStream> = Vec::new();
        for attr in &pe.container.attrs.facet {
            match attr {
                PFacetAttr::DenyUnknownFields => {
                    attribute_tokens.push(quote! { ::facet::ShapeAttribute::DenyUnknownFields });
                }
                PFacetAttr::Arbitrary { content } => {
                    attribute_tokens.push(quote! { ::facet::ShapeAttribute::Arbitrary(#content) });
                }
                PFacetAttr::RenameAll { rule } => {
                    // RenameAll is handled by PName logic, but add it as ShapeAttribute too
                    let rule_str = rule.apply(""); // Hack to get str - improve RenameRule display
                    attribute_tokens.push(quote! { ::facet::ShapeAttribute::RenameAll(#rule_str) });
                }
                PFacetAttr::Invariants { .. } => {
                    // Note: Facet vtable does not currently support invariants directly on enums
                    // Maybe panic or warn here? For now, ignoring.
                    panic!("Invariants are not supported on enums")
                }
                // Opaque, Transparent, SkipSerializing/If, Default/Equals are not relevant/valid for enum containers.
                _ => {}
            }
        }

        if attribute_tokens.is_empty() {
            quote! {}
        } else {
            quote! { .attributes(&const { [#(#attribute_tokens),*] }) }
        }
    };

    // Determine enum repr (already resolved by PEnum::parse())
    let valid_repr = &pe.repr;

    // Helper for EnumRepr TS (token stream) generation for primitives
    fn enum_repr_ts_from_primitive(primitive_repr: PrimitiveRepr) -> TokenStream {
        let type_name_str = primitive_repr.type_name().to_string();
        let enum_repr_variant_ident = format_ident!("{}", type_name_str.to_uppercase());
        quote! { ::facet::EnumRepr::#enum_repr_variant_ident }
    }

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
                    let mut tokens = Vec::new();
                    let name_token = TokenTree::Literal(Literal::string(&display_name));
                    // Attributes from PAttrs
                    if pv.attrs.facet.is_empty() {
                        tokens.push(quote! { .name(#name_token) });
                    } else {
                        let mut attrs_list = Vec::new();
                        for attr in &pv.attrs.facet {
                            if let PFacetAttr::Arbitrary { content } = attr {
                                attrs_list.push(
                                    quote! { ::facet::VariantAttribute::Arbitrary(#content) },
                                );
                            }
                        }
                        if attrs_list.is_empty() {
                            tokens.push(quote! { .name(#name_token) });
                        } else {
                            tokens.push(
                                quote! { .name(#name_token).attributes(&[#(#attrs_list),*]) },
                            );
                        }
                    }
                    quote! { #(#tokens)* }
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
                            ::facet::Variant::builder()
                                #variant_attrs_tokens
                                .discriminant(#discriminant_ts)
                                .data(::facet::StructType::builder().repr(::facet::Repr::c()).unit().build())
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
                                )
                            })
                            .collect();
                        exprs.push(quote! {{
                            let fields: &'static [::facet::Field] = &const {[
                                #(#field_defs),*
                            ]};
                            ::facet::Variant::builder()
                                #variant_attrs_tokens
                                .discriminant(#discriminant_ts)
                                .data(::facet::StructType::builder().repr(::facet::Repr::c()).tuple().fields(fields).build())
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
                                )
                            })
                            .collect();

                        exprs.push(quote! {{
                            let fields: &'static [::facet::Field] = &const {[
                                #(#field_defs),*
                            ]};
                            ::facet::Variant::builder()
                                #variant_attrs_tokens
                                .discriminant(#discriminant_ts)
                                .data(::facet::StructType::builder().repr(::facet::Repr::c()).struct_().fields(fields).build())
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
                    quote! { ::facet::EnumRepr::from_discriminant_size::<#shadow_discriminant_name>() }
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
                    let mut tokens = Vec::new();
                    let name_token = TokenTree::Literal(Literal::string(&display_name));
                    if pv.attrs.facet.is_empty() {
                        tokens.push(quote! { .name(#name_token) });
                    } else {
                        let mut attrs_list = Vec::new();
                        for attr in &pv.attrs.facet {
                            if let PFacetAttr::Arbitrary { content } = attr {
                                attrs_list.push(
                                    quote! { ::facet::VariantAttribute::Arbitrary(#content) },
                                );
                            }
                        }
                        if attrs_list.is_empty() {
                            tokens.push(quote! { .name(#name_token) });
                        } else {
                            tokens.push(
                                quote! { .name(#name_token).attributes(&[#(#attrs_list),*]) },
                            );
                        }
                    }
                    quote! { #(#tokens)* }
                };

                let maybe_doc = match &pv.attrs.doc[..] {
                    [] => quote! {},
                    doc_lines => quote! { .doc(&[#(#doc_lines),*]) },
                };

                match &pv.kind {
                    PVariantKind::Unit => {
                        exprs.push(quote! {
                            ::facet::Variant::builder()
                                #variant_attrs_tokens
                                .discriminant(#discriminant_ts)
                                .data(::facet::StructType::builder().repr(::facet::Repr::c()).unit().build())
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
                                gen_field_from_pfield(&pf, &shadow_struct_name, &facet_bgp, None)
                            })
                            .collect();
                        exprs.push(quote! {{
                            let fields: &'static [::facet::Field] = &const {[
                                #(#field_defs),*
                            ]};
                            ::facet::Variant::builder()
                                #variant_attrs_tokens
                                .discriminant(#discriminant_ts)
                                .data(::facet::StructType::builder().repr(::facet::Repr::c()).tuple().fields(fields).build())
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
                                gen_field_from_pfield(pf, &shadow_struct_name, &facet_bgp, None)
                            })
                            .collect();
                        exprs.push(quote! {{
                            let fields: &'static [::facet::Field] = &const {[
                                #(#field_defs),*
                            ]};
                            ::facet::Variant::builder()
                                #variant_attrs_tokens
                                .discriminant(#discriminant_ts)
                                .data(::facet::StructType::builder().repr(::facet::Repr::c()).struct_().fields(fields).build())
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
                compile_error!("Facet requires enums to have an explicit representation (e.g., #[repr(C)], #[repr(u8)])")
            };
        }
    };

    // Only make static_decl for non-generic enums
    let static_decl = if parsed.generics.is_none() {
        generate_static_decl(&enum_name)
    } else {
        quote! {}
    };

    // Set up generics for impl blocks
    let facet_bgp = bgp.with_lifetime(LifetimeName(format_ident!("__facet")));
    let bgp_def = facet_bgp.display_with_bounds();
    let bgp_without_bounds = bgp.display_without_bounds();

    // Generate the impl
    quote! {
        #static_decl

        #[automatically_derived]
        #[allow(non_camel_case_types)]
        unsafe impl #bgp_def ::facet::Facet<'__facet> for #enum_name #bgp_without_bounds #where_clauses_tokens {
            const VTABLE: &'static ::facet::ValueVTable = &const { ::facet::value_vtable!(
                Self,
                |f, _opts| ::core::fmt::Write::write_str(f, #enum_name_str)
            )};

            const SHAPE: &'static ::facet::Shape = &const {
                #(#shadow_struct_defs)*

                let __facet_variants: &'static [::facet::Variant] = &const {[
                    #(#variant_expressions),*
                ]};

                ::facet::Shape::builder_for_sized::<Self>()
                    #type_params
                    .ty(::facet::Type::User(::facet::UserType::Enum(::facet::EnumType::builder()
                            // Use variant expressions that just reference the shadow structs
                            // which are now defined above
                            .variants(__facet_variants)
                            .repr(::facet::Repr::c())
                            .enum_repr(#enum_repr_type_tokenstream)
                            .build())
                    ))
                    #maybe_container_doc
                    #container_attributes_tokens
                    .build()
            };
        }
    }
}
