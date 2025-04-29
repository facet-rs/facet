use super::*;
use quote::quote;

/// Processes an enum to implement Facet
pub(crate) fn process_enum(parsed: Enum) -> TokenStream {
    let enum_name = parsed.name.clone();
    let enum_name_str = enum_name.to_string();
    let bgp = BoundedGenericParams::parse(parsed.generics.as_ref());
    let where_clauses = build_where_clauses(parsed.clauses.as_ref(), parsed.generics.as_ref());
    let type_params = build_type_params(parsed.generics.as_ref());
    let container_attributes = build_container_attributes(&parsed.attributes);

    // collect all `#repr(..)` attrs
    // either multiple attrs, or a single attr with multiple values
    let attr_iter = parsed
        .attributes
        .iter()
        .filter_map(|attr| {
            if let AttributeInner::Repr(repr_attr) = &attr.body.content {
                if repr_attr.attr.content.0.is_empty() {
                    // treat empty repr as non-existent
                    // (this shouldn't be possible, but just in case)
                    None
                } else {
                    Some(repr_attr)
                }
            } else {
                None
            }
        })
        .flat_map(|repr_attr| repr_attr.attr.content.0.iter());

    let mut repr_c = false;
    let mut discriminant_type = None;

    for attr in attr_iter {
        let attr_str = attr.value.to_string();
        match attr_str.as_str() {
            // this is #[repr(C)]
            "C" => repr_c = true,

            // set the repr type
            // NOTE: we're not worried about multiple
            // clashing types here -- that's rustc's problem
            "u8" => discriminant_type = Some(Discriminant::U8),
            "u16" => discriminant_type = Some(Discriminant::U16),
            "u32" => discriminant_type = Some(Discriminant::U32),
            "u64" => discriminant_type = Some(Discriminant::U64),
            "usize" => discriminant_type = Some(Discriminant::USize),
            "i8" => discriminant_type = Some(Discriminant::I8),
            "i16" => discriminant_type = Some(Discriminant::I16),
            "i32" => discriminant_type = Some(Discriminant::I32),
            "i64" => discriminant_type = Some(Discriminant::I64),
            "isize" => discriminant_type = Some(Discriminant::ISize),
            _ => {
                return quote! {
                    compile_error!("Facet only supports enums with a primitive representation (e.g. #[repr(u8)]) or C-style (e.g. #[repr(C)]")
                };
            }
        }
    }

    let params = EnumParams {
        enum_name: &enum_name_str,
        variants: &parsed.body.content.0,
        discriminant_type,
        bgp: &bgp,
        where_clauses: where_clauses.clone(),
        rename_rule: container_attributes.rename_rule,
    };

    let processed_body = match (repr_c, discriminant_type) {
        (true, _) => {
            // C-style enum, no discriminant type
            process_c_style_enum(&params)
        }
        (false, Some(_)) => process_primitive_enum(&params),
        _ => {
            return quote! {
                compile_error!("Enums must have an explicit representation (e.g. #[repr(u8)] or #[repr(C)]) to be used with Facet")
            };
        }
    };

    let ProcessedEnumBody {
        shadow_struct_defs,
        variant_expressions,
        repr_type,
    } = processed_body;

    let static_decl = if parsed.generics.is_none() {
        generate_static_decl(&enum_name)
    } else {
        quote! {}
    };
    let maybe_container_doc = build_maybe_doc(&parsed.attributes);

    let facet_bgp = params
        .bgp
        .with_lifetime(LifetimeName(quote::format_ident!("__facet")));
    let bgp_def = facet_bgp.display_with_bounds();
    let bgp_without_bounds = params.bgp.display_without_bounds();
    let container_attributes_tokens = container_attributes.tokens;

    // Generate the impl
    quote! {
        #static_decl

        #[automatically_derived]
        unsafe impl #bgp_def ::facet::Facet<'__facet> for #enum_name #bgp_without_bounds #where_clauses {
            const SHAPE: &'static ::facet::Shape = &const {
                // Define all shadow structs at the beginning of the const block
                // to ensure they're in scope for offset_of! macros
                #(#shadow_struct_defs)*

                let __facet_variants: &'static [::facet::Variant] = &const {[
                    #(#variant_expressions),*
                ]};

                ::facet::Shape::builder()
                    .id(::facet::ConstTypeId::of::<Self>())
                    .layout(::core::alloc::Layout::new::<Self>())
                    #type_params
                    .vtable(&const { ::facet::value_vtable!(
                        Self,
                        |f, _opts| ::core::fmt::Write::write_str(f, #enum_name_str)
                    )})
                    .def(::facet::Def::Enum(::facet::EnumDef::builder()
                        // Use variant expressions that just reference the shadow structs
                        // which are now defined above
                        .variants(__facet_variants)
                        .repr(#repr_type)
                        .build()))
                    #maybe_container_doc
                    #container_attributes_tokens
                    .build()
            };
        }
    }
}

/// Build a variant name and attributes, applying rename attribute or rename_all rule
fn build_variant_attributes(
    variant_name: &str,
    attributes: &[Attribute],
    rename_rule: Option<RenameRule>,
) -> ContainerAttributes {
    let mut display_name = variant_name.to_string(); // Keep as string for `.name()`
    let mut attribute_list: Vec<TokenStream> = Vec::new();
    let mut rename_all_rule: Option<RenameRule> = None;
    for attr in attributes {
        if let AttributeInner::Facet(facet_attr) = &attr.body.content {
            // Iterate over the comma-delimited items inside #[facet(...)]
            for delimited_facet_inner in &facet_attr.inner.content.0 {
                let facet_inner = &delimited_facet_inner.value; // Get the FacetInner
                match facet_inner {
                    FacetInner::Sensitive(_) => {
                        // TODO
                    }
                    FacetInner::Invariants(_) => {
                        // dealt with elsewhere
                    }
                    FacetInner::Opaque(_) => {
                        // TODO
                    }
                    FacetInner::DenyUnknownFields(_) => {
                        // not applicable to variants
                    }
                    FacetInner::DefaultEquals(_) | FacetInner::Default(_) => {
                        // not applicable to variants
                    }
                    FacetInner::Transparent(_) => {
                        // not applicable to variants
                    }
                    FacetInner::RenameAll(rename_all_inner) => {
                        let rule_str = rename_all_inner.value.value().trim_matches('"');
                        if let Some(rule) = RenameRule::from_str(rule_str) {
                            rename_all_rule = Some(rule);
                            attribute_list.push(quote! {
                                ::facet::VariantAttribute::RenameAll(#rule_str)
                            });
                        } else {
                            panic!("Unknown rename_all rule for enum variant: {:?}", rule_str);
                        }
                    }
                    FacetInner::Arbitrary(tt) => {
                        let attr_str = tt.tokens_to_string();
                        attribute_list.push(quote! {
                            ::facet::VariantAttribute::Arbitrary(#attr_str)
                        });
                    }
                    FacetInner::SkipSerializing(_) => {
                        // Not applicable to variants
                    }
                    FacetInner::SkipSerializingIf(_) => {
                        // Not applicable to variants
                    }
                }
            }
        }
    }

    // Apply container-level rename_all rule if there's no explicit rename on the variant
    if rename_rule.is_some() {
        display_name = rename_rule.unwrap().apply(variant_name);
        // Add the Rename attribute based on the container rule if it changed the name
        if display_name != variant_name {
            attribute_list.push(quote! {
                ::facet::VariantAttribute::Rename(#display_name)
            });
        }
    }

    let name_token = TokenTree::Literal(Literal::string(&display_name));

    let tokens = if attribute_list.is_empty() {
        quote! {
            .name(#name_token)
        }
    } else {
        quote! {
            .name(#name_token)
            .attributes(&[#(#attribute_list),*])
        }
    };

    ContainerAttributes {
        tokens,
        rename_rule: rename_all_rule, // Return variant-level rename_all rule for its fields
    }
}

// mirrors facet_core::types::EnumRepr
#[derive(Clone, Copy)]
enum Discriminant {
    U8,
    U16,
    U32,
    U64,
    USize,
    I8,
    I16,
    I32,
    I64,
    ISize,
}

impl Discriminant {
    fn as_enum_repr_ident(&self) -> Ident {
        quote::format_ident!("{}", self.as_enum_repr_str())
    }

    fn as_enum_repr_str(&self) -> &'static str {
        match self {
            Discriminant::U8 => "U8",
            Discriminant::U16 => "U16",
            Discriminant::U32 => "U32",
            Discriminant::U64 => "U64",
            Discriminant::USize => "USize",
            Discriminant::I8 => "I8",
            Discriminant::I16 => "I16",
            Discriminant::I32 => "I32",
            Discriminant::I64 => "I64",
            Discriminant::ISize => "ISize",
        }
    }

    fn as_rust_type_ident(&self) -> Ident {
        quote::format_ident!("{}", self.as_rust_type_str())
    }

    fn as_rust_type_str(&self) -> &'static str {
        match self {
            Discriminant::U8 => "u8",
            Discriminant::U16 => "u16",
            Discriminant::U32 => "u32",
            Discriminant::U64 => "u64",
            Discriminant::USize => "usize",
            Discriminant::I8 => "i8",
            Discriminant::I16 => "i16",
            Discriminant::I32 => "i32",
            Discriminant::I64 => "i64",
            Discriminant::ISize => "isize",
        }
    }
}

struct ProcessedEnumBody {
    shadow_struct_defs: Vec<TokenStream>,
    variant_expressions: Vec<TokenStream>,
    repr_type: TokenStream,
}

type EnumVariant = Delimited<EnumVariantLike, Comma>;

struct EnumParams<'a> {
    // Core identification
    enum_name: &'a str,
    variants: &'a [EnumVariant],

    // Type information
    discriminant_type: Option<Discriminant>,
    bgp: &'a BoundedGenericParams,
    where_clauses: TokenStream,

    // Attributes and customization
    rename_rule: Option<RenameRule>,
}

impl EnumParams<'_> {
    fn with_facet_lifetime(&self) -> BoundedGenericParams {
        self.bgp.with(BoundedGenericParam {
            bounds: None,
            param: GenericParamName::Lifetime(LifetimeName(quote::format_ident!("__facet"))),
        })
    }
}

/// C-style enums (i.e. #[repr(C)], #[repr(C, u*)] and #[repr(C, i*)]) are laid out
/// as a #[repr(C)] struct with two fields: the discriminant and the union of all the variants.
///
/// See: <https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.primitive.adt>
///
/// To calculate the offsets of each variant, we create a shadow struct that mimics this
/// structure and use the `offset_of!` macro to calculate the offsets of each field.
fn process_c_style_enum(params: &EnumParams) -> ProcessedEnumBody {
    let facet_bgp = params.with_facet_lifetime();
    let bgp_with_bounds = facet_bgp.display_with_bounds();
    let bgp_without_bounds = facet_bgp.display_without_bounds();
    let phantom_data = facet_bgp.display_as_phantom_data();
    let where_clauses = &params.where_clauses;

    // Collect shadow struct definitions separately from variant expressions
    let mut shadow_struct_defs: Vec<TokenStream> = Vec::new();
    let mut variant_expressions: Vec<TokenStream> = Vec::new();

    // first, create an enum to represent the discriminant type
    let shadow_discriminant_name_ident =
        quote::format_ident!("__ShadowDiscriminant{}", params.enum_name);
    let all_variant_names: Vec<Ident> = params
        .variants
        .iter()
        .map(|var_like| match &var_like.value.variant {
            EnumVariantData::Unit(unit) => unit.name.clone(),
            EnumVariantData::Tuple(tuple) => tuple.name.clone(),
            EnumVariantData::Struct(struct_var) => struct_var.name.clone(),
        })
        .collect();

    let repr_attr_content = if let Some(d) = params.discriminant_type {
        let ty_ident = d.as_rust_type_ident();
        quote! { #ty_ident }
    } else {
        quote! { C }
    };
    shadow_struct_defs.push(quote! {
        #[repr(#repr_attr_content)]
        #[allow(dead_code)]
        enum #shadow_discriminant_name_ident { #(#all_variant_names),* }
    });

    // we'll also generate a shadow union for the fields
    let shadow_union_name_ident = quote::format_ident!("__ShadowFields{}", params.enum_name);
    let all_union_fields: Vec<TokenStream> = params
        .variants
        .iter()
        .map(|var_like| match &var_like.value.variant {
            EnumVariantData::Unit(unit) => unit.name.clone(),
            EnumVariantData::Tuple(tuple) => tuple.name.clone(),
            EnumVariantData::Struct(struct_var) => struct_var.name.clone(),
        })
        .map(|variant_name_ident| {
            let shadow_field_name_ident =
                quote::format_ident!("__ShadowField{}_{}", params.enum_name, variant_name_ident);
            quote! {
                #variant_name_ident: ::core::mem::ManuallyDrop<#shadow_field_name_ident #bgp_without_bounds>
            }
        })
        .collect();

    shadow_struct_defs.push(quote! {
        #[repr(C)]
        #[allow(non_snake_case, dead_code)]
        union #shadow_union_name_ident #bgp_with_bounds #where_clauses { #(#all_union_fields),* }
    });

    // Create a shadow struct to represent the enum layout
    let shadow_repr_name_ident = quote::format_ident!("__ShadowRepr{}", params.enum_name);
    shadow_struct_defs.push(quote! {
        #[repr(C)]
        #[allow(non_snake_case)]
        #[allow(dead_code)]
        struct #shadow_repr_name_ident #bgp_with_bounds #where_clauses {
            _discriminant: #shadow_discriminant_name_ident,
            _phantom: #phantom_data,
            _fields: #shadow_union_name_ident #bgp_without_bounds,
        }
    });

    // Discriminant values are either manually defined, or incremented from the last one
    // See: <https://doc.rust-lang.org/reference/items/enumerations.html#implicit-discriminants>
    let mut discriminant_value = 0;
    for var_like in params.variants.iter() {
        if let Some(x) = &var_like.value.discriminant {
            discriminant_value = get_discriminant_value(&x.second);
        }

        match &var_like.value.variant {
            EnumVariantData::Unit(unit) => {
                let variant_name = unit.name.to_string();
                let container_attributes =
                    build_variant_attributes(&variant_name, &unit.attributes, params.rename_rule);
                let maybe_doc = build_maybe_doc(&unit.attributes);

                // Generate shadow struct for this tuple variant to calculate offsets
                let shadow_struct_name_ident =
                    quote::format_ident!("__ShadowField{}_{}", params.enum_name, variant_name);

                // Add shadow struct definition
                shadow_struct_defs.push(quote! {
                    #[repr(C)]
                    #[allow(non_snake_case, dead_code)]
                    struct #shadow_struct_name_ident #bgp_with_bounds #where_clauses { _phantom: #phantom_data }
                });

                let container_attrs_tokens = container_attributes.tokens;
                variant_expressions.push(quote! {
                    ::facet::Variant::builder()
                        #container_attrs_tokens
                        .discriminant(#discriminant_value)
                        .fields(::facet::StructDef::builder().unit().build())
                        #maybe_doc
                        .build()
                });
            }
            EnumVariantData::Tuple(tuple) => {
                let variant_name = tuple.name.to_string();
                let container_attributes =
                    build_variant_attributes(&variant_name, &tuple.attributes, params.rename_rule);
                let maybe_doc = build_maybe_doc(&tuple.attributes);

                // Generate shadow struct for this tuple variant to calculate offsets
                let shadow_struct_name_ident =
                    quote::format_ident!("__ShadowField{}_{}", params.enum_name, variant_name);

                // Build the list of fields and types for the shadow struct
                let fields_with_types: Vec<TokenStream> = tuple
                    .fields
                    .content
                    .0
                    .iter()
                    .enumerate()
                    .map(|(idx, field)| {
                        let field_name_ident = quote::format_ident!("_{}", idx);
                        let typ = &field.value.typ.to_token_stream();
                        quote! { #field_name_ident: #typ }
                    })
                    .collect();

                // Add shadow struct definition
                shadow_struct_defs.push(quote! {
                    #[repr(C)]
                    #[allow(non_snake_case, dead_code)]
                    struct #shadow_struct_name_ident #bgp_with_bounds #where_clauses {
                        #(#fields_with_types),* ,
                        _phantom: #phantom_data
                    }
                });

                let variant_offset = quote! {
                    ::core::mem::offset_of!(#shadow_repr_name_ident #bgp_without_bounds, _fields)
                };

                // Build the list of field types with calculated offsets
                let fields: Vec<TokenStream> = tuple
                    .fields
                    .content
                    .0
                    .iter()
                    .enumerate()
                    .map(|(idx, field)| {
                        let field_name_str = format!("_{idx}");
                        gen_struct_field(FieldInfo {
                            raw_field_name: &field_name_str,
                            normalized_field_name: &field_name_str,
                            field_type: field.value.typ.to_token_stream(),
                            struct_name: &shadow_struct_name_ident.to_string(),
                            bgp: &facet_bgp,
                            attrs: &field.value.attributes,
                            base_field_offset: Some(variant_offset.clone()),
                            rename_rule: container_attributes.rename_rule,
                        })
                    })
                    .collect();

                let container_attrs_tokens = container_attributes.tokens;
                variant_expressions.push(quote! {{
                    let fields: &'static [::facet::Field] = &const {[
                        #(#fields),*
                    ]};

                    ::facet::Variant::builder()
                        #container_attrs_tokens
                        .discriminant(#discriminant_value)
                        .fields(::facet::StructDef::builder().tuple().fields(fields).build())
                        #maybe_doc
                        .build()
                }});
            }
            EnumVariantData::Struct(struct_var) => {
                let variant_name = struct_var.name.to_string();
                let container_attributes = build_variant_attributes(
                    &variant_name,
                    &struct_var.attributes,
                    params.rename_rule,
                );
                let maybe_doc = build_maybe_doc(&struct_var.attributes);

                // Generate shadow struct for this struct variant to calculate offsets
                let shadow_struct_name_ident =
                    quote::format_ident!("__ShadowField{}_{}", params.enum_name, variant_name);

                // Build the list of fields and types
                let fields_with_types: Vec<TokenStream> = struct_var
                    .fields
                    .content
                    .0
                    .iter()
                    .map(|field| {
                        let name = &field.value.name;
                        let typ = &field.value.typ.to_token_stream();
                        quote! { #name: #typ }
                    })
                    .collect();

                // Add shadow struct definition
                shadow_struct_defs.push(quote! {
                    #[repr(C)]
                    #[allow(non_snake_case, dead_code)]
                    struct #shadow_struct_name_ident #bgp_with_bounds #where_clauses {
                        #(#fields_with_types),* ,
                        _phantom: #phantom_data
                    }
                });

                let variant_offset = quote! {
                    ::core::mem::offset_of!(#shadow_repr_name_ident #bgp_without_bounds, _fields)
                };

                // Build the list of field types with calculated offsets
                let fields: Vec<TokenStream> = struct_var
                    .fields
                    .content
                    .0
                    .iter()
                    .map(|field| {
                        // Handle raw identifiers (like r#type) by stripping the 'r#' prefix.
                        let raw_field_name = field.value.name.to_string(); // e.g., "r#type"
                        let normalized_field_name = normalize_ident_str(&raw_field_name); // e.g., "type"
                        gen_struct_field(FieldInfo {
                            raw_field_name: &raw_field_name,
                            normalized_field_name,
                            field_type: field.value.typ.to_token_stream(),
                            struct_name: &shadow_struct_name_ident.to_string(),
                            bgp: &facet_bgp,
                            attrs: &field.value.attributes,
                            base_field_offset: Some(variant_offset.clone()),
                            rename_rule: container_attributes.rename_rule,
                        })
                    })
                    .collect();

                let container_attrs_tokens = container_attributes.tokens;
                variant_expressions.push(quote! {{
                    let fields: &'static [::facet::Field] = &const {[
                        #(#fields),*
                    ]};

                    ::facet::Variant::builder()
                        #container_attrs_tokens
                        .discriminant(#discriminant_value)
                        .fields(::facet::StructDef::builder().struct_().fields(fields).build())
                        #maybe_doc
                        .build()
                }});
            }
        }
        discriminant_value += 1;
    }

    let repr_type_ts = params.discriminant_type.map_or_else(
        || quote! { ::facet::EnumRepr::from_discriminant_size::<#shadow_discriminant_name_ident>() },
        |d| { let repr_ident = d.as_enum_repr_ident(); quote!{ ::facet::EnumRepr::#repr_ident } },
    );

    ProcessedEnumBody {
        shadow_struct_defs,
        variant_expressions,
        repr_type: repr_type_ts,
    }
}

/// Primitive enums (i.e. #[repr(u*)] and #[repr(i*)]) are laid out
/// as a union of all the variants, with the discriminant as an "inner" tag in the struct.
///
/// See: <https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.primitive.adt>
///
/// To calculate the offsets of each variant, we create a shadow struct that mimics this
/// structure and use the `offset_of!` macro to calculate the offsets of each field.
fn process_primitive_enum(params: &EnumParams) -> ProcessedEnumBody {
    let facet_bgp = params.with_facet_lifetime();
    let bgp_with_bounds = facet_bgp.display_with_bounds();
    let phantom_data = facet_bgp.display_as_phantom_data();
    let where_clauses = &params.where_clauses;

    // Collect shadow struct definitions separately from variant expressions
    let mut shadow_struct_defs: Vec<TokenStream> = Vec::new();
    let mut variant_expressions: Vec<TokenStream> = Vec::new();

    // We can safely unwrap because this function is only called when discriminant_type is Some
    let discriminant_type = params
        .discriminant_type
        .expect("discriminant_type should be Some when process_primitive_enum is called");
    let discriminant_rust_type_ident = discriminant_type.as_rust_type_ident();

    // Discriminant values are either manually defined, or incremented from the last one
    // See: <https://doc.rust-lang.org/reference/items/enumerations.html#implicit-discriminants>
    let mut discriminant_value = 0;
    for var_like in params.variants.iter() {
        if let Some(x) = &var_like.value.discriminant {
            discriminant_value = get_discriminant_value(&x.second);
        }
        match &var_like.value.variant {
            EnumVariantData::Unit(unit) => {
                let variant_name = unit.name.to_string();
                let container_attributes =
                    build_variant_attributes(&variant_name, &unit.attributes, params.rename_rule);
                let maybe_doc = build_maybe_doc(&unit.attributes);
                let container_attrs_tokens = container_attributes.tokens;

                variant_expressions.push(quote! {
                    ::facet::Variant::builder()
                        #container_attrs_tokens
                        .discriminant(#discriminant_value)
                        .fields(::facet::StructDef::builder().unit().build())
                        #maybe_doc
                        .build()
                });
            }
            EnumVariantData::Tuple(tuple) => {
                let variant_name = tuple.name.to_string();
                let container_attributes =
                    build_variant_attributes(&variant_name, &tuple.attributes, params.rename_rule);
                let maybe_doc = build_maybe_doc(&tuple.attributes);

                // Generate shadow struct for this tuple variant to calculate offsets
                let shadow_struct_name_ident =
                    quote::format_ident!("__Shadow{}_{}", params.enum_name, variant_name);

                // Build the list of fields and types for the shadow struct
                let fields_with_types: Vec<TokenStream> = tuple
                    .fields
                    .content
                    .0
                    .iter()
                    .enumerate()
                    .map(|(idx, field)| {
                        let field_name_ident = quote::format_ident!("_{}", idx);
                        let typ = &field.value.typ.to_token_stream();
                        quote! { #field_name_ident: #typ }
                    })
                    .collect();

                // Add shadow struct definition
                shadow_struct_defs.push(quote! {
                    #[repr(C)]
                    #[allow(non_snake_case, dead_code)]
                    struct #shadow_struct_name_ident #bgp_with_bounds #where_clauses {
                        _discriminant: #discriminant_rust_type_ident,
                        _phantom: #phantom_data,
                        #(#fields_with_types),*
                    }
                });

                // Build the list of field types with calculated offsets
                let fields: Vec<TokenStream> = tuple
                    .fields
                    .content
                    .0
                    .iter()
                    .enumerate()
                    .map(|(idx, field)| {
                        let field_name_str = format!("_{idx}");
                        gen_struct_field(FieldInfo {
                            raw_field_name: &field_name_str,
                            normalized_field_name: &field_name_str,
                            field_type: field.value.typ.to_token_stream(),
                            struct_name: &shadow_struct_name_ident.to_string(),
                            bgp: &facet_bgp,
                            attrs: &field.value.attributes,
                            base_field_offset: None,
                            rename_rule: container_attributes.rename_rule,
                        })
                    })
                    .collect();

                let container_attrs_tokens = container_attributes.tokens;
                variant_expressions.push(quote! {{
                    let fields: &'static [::facet::Field] = &const {[
                        #(#fields),*
                    ]};

                    ::facet::Variant::builder()
                        #container_attrs_tokens
                        .discriminant(#discriminant_value)
                        .fields(::facet::StructDef::builder().tuple().fields(fields).build())
                        #maybe_doc
                        .build()
                }});
            }
            EnumVariantData::Struct(struct_var) => {
                let variant_name = struct_var.name.to_string();
                let container_attributes = build_variant_attributes(
                    &variant_name,
                    &struct_var.attributes,
                    params.rename_rule,
                );
                let maybe_doc = build_maybe_doc(&struct_var.attributes);

                // Generate shadow struct for this struct variant to calculate offsets
                let shadow_struct_name_ident =
                    quote::format_ident!("__Shadow{}_{}", params.enum_name, variant_name);

                // Build the list of fields and types
                let fields_with_types: Vec<TokenStream> = struct_var
                    .fields
                    .content
                    .0
                    .iter()
                    .map(|field| {
                        let name = &field.value.name;
                        let typ = &field.value.typ.to_token_stream();
                        quote! { #name: #typ }
                    })
                    .collect();

                // Add shadow struct definition
                shadow_struct_defs.push(quote! {
                    #[repr(C)]
                    #[allow(non_snake_case, dead_code)]
                    struct #shadow_struct_name_ident #bgp_with_bounds #where_clauses {
                        _discriminant: #discriminant_rust_type_ident,
                        _phantom: #phantom_data,
                        #(#fields_with_types),*
                    }
                });

                // Build the list of field types with calculated offsets
                let fields: Vec<TokenStream> = struct_var
                    .fields
                    .content
                    .0
                    .iter()
                    .map(|field| {
                        // Handle raw identifiers (like r#type) by stripping the 'r#' prefix.
                        let raw_field_name = field.value.name.to_string(); // e.g., "r#type"
                        let normalized_field_name = normalize_ident_str(&raw_field_name); // e.g., "type"
                        gen_struct_field(FieldInfo {
                            raw_field_name: &raw_field_name,
                            normalized_field_name,
                            field_type: field.value.typ.to_token_stream(),
                            struct_name: &shadow_struct_name_ident.to_string(),
                            bgp: &facet_bgp,
                            attrs: &field.value.attributes,
                            base_field_offset: None,
                            rename_rule: container_attributes.rename_rule,
                        })
                    })
                    .collect();

                let container_attrs_tokens = container_attributes.tokens;
                // Add variant expression - now with discriminant
                // variant offset is zero since all fields are
                // already computed relative to the discriminant
                variant_expressions.push(quote! {{
                    let fields: &'static [::facet::Field] = &const {[
                        #(#fields),*
                    ]};

                    ::facet::Variant::builder()
                        #container_attrs_tokens
                        .discriminant(#discriminant_value)
                        .fields(::facet::StructDef::builder().struct_().fields(fields).build())
                        #maybe_doc
                        .build()
                }});
            }
        }
        discriminant_value += 1;
    }

    let repr_ident = discriminant_type.as_enum_repr_ident();
    let repr_type_ts = quote! { ::facet::EnumRepr::#repr_ident };

    ProcessedEnumBody {
        shadow_struct_defs,
        variant_expressions,
        repr_type: repr_type_ts,
    }
}
