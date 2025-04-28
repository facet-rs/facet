use super::*;

/// Processes an enum to implement Facet
pub(crate) fn process_enum(parsed: Enum) -> TokenStream {
    let enum_name = parsed.name.to_string();
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
        let attr = attr.value.to_string();
        match attr.as_str() {
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
                return r#"compile_error!("Facet only supports enums with a primitive representation (e.g. #[repr(u8)]) or C-style (e.g. #[repr(C)]")"#
            .into_token_stream()
            }
        }
    }

    let params = EnumParams {
        enum_name: &enum_name,
        variants: &parsed.body.content.0,
        discriminant_type,
        bgp: &bgp,
        where_clauses: &where_clauses,
        rename_rule: container_attributes.rename_rule,
    };

    let processed_body = match (repr_c, discriminant_type) {
        (true, _) => {
            // C-style enum, no discriminant type
            process_c_style_enum(&params)
        }
        (false, Some(_)) => process_primitive_enum(&params),
        _ => {
            return r#"compile_error!("Enums must have an explicit representation (e.g. #[repr(u8)] or #[repr(C)]) to be used with Facet")"#
            .into_token_stream()
        }
    };

    let ProcessedEnumBody {
        shadow_struct_defs,
        variant_expressions,
        repr_type,
    } = processed_body;

    // Join the shadow struct definitions and variant expressions
    let shadow_structs = shadow_struct_defs.join("\n\n");
    let variants = variant_expressions.join(", ");

    let static_decl = if parsed.generics.is_none() {
        generate_static_decl(&enum_name)
    } else {
        String::new()
    };
    let maybe_container_doc = build_maybe_doc(&parsed.attributes);

    // Generate the impl
    let output = format!(
        r#"
{static_decl}

#[automatically_derived]
unsafe impl{bgp_def} ::facet::Facet<'__facet> for {enum_name}{bgp_without_bounds} {where_clauses} {{
    const SHAPE: &'static ::facet::Shape = &const {{
        // Define all shadow structs at the beginning of the const block
        // to ensure they're in scope for offset_of! macros
        {shadow_structs}

        let __facet_variants: &'static [::facet::Variant] = &const {{[
            {variants}
        ]}};

        ::facet::Shape::builder()
            .id(::facet::ConstTypeId::of::<Self>())
            .layout(::core::alloc::Layout::new::<Self>())
            {type_params}
            .vtable(&const {{ ::facet::value_vtable!(
                Self,
                |f, _opts| ::core::fmt::Write::write_str(f, "{enum_name}")
            )}})
            .def(::facet::Def::Enum(::facet::EnumDef::builder()
                // Use variant expressions that just reference the shadow structs
                // which are now defined above
                .variants(__facet_variants)
                .repr(::facet::EnumRepr::{repr_type})
                .build()))
            {maybe_container_doc}
            {container_attributes}
            .build()
    }};
}}
        "#,
        bgp_def = bgp.with_lifetime("__facet").display_with_bounds(),
        bgp_without_bounds = bgp.display_without_bounds(),
        container_attributes = container_attributes.code,
    );

    // Uncomment to see generated code before lexin
    // panic!("output =\n{output}");

    // Return the generated code
    output.into_token_stream()
}

/// Build a variant name and attributes, applying rename attribute or rename_all rule
fn build_variant_attributes(
    variant_name: &str,
    attributes: &[Attribute],
    rename_rule: Option<RenameRule>,
) -> ContainerAttributes {
    let mut has_explicit_rename = false;
    let mut display_name = variant_name.to_string();
    let mut attribute_list: Vec<String> = vec![];
    let mut rename_all_rule: Option<RenameRule> = None;
    for attr in attributes {
        if let AttributeInner::Facet(facet_attr) = &attr.body.content {
            match &facet_attr.inner.content {
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
                        attribute_list.push(format!(
                            r#"::facet::VariantAttribute::RenameAll({:?})"#,
                            rule_str
                        ));
                    } else {
                        panic!("Unknown rename_all rule for enum variant: {:?}", rule_str);
                    }
                }
                FacetInner::Other(tt) => {
                    let attr_str = tt.tokens_to_string();

                    // Split the attributes by commas to handle multiple attributes
                    let attrs = attr_str.split(',').map(|s| s.trim()).collect::<Vec<_>>();

                    for attr in attrs {
                        if let Some(equal_pos) = attr.find('=') {
                            let key = attr[..equal_pos].trim();
                            if key == "rename" {
                                has_explicit_rename = true;
                                let value = attr[equal_pos + 1..].trim().trim_matches('"');
                                // Keep the Rename attribute for reflection
                                attribute_list.push(format!(
                                    r#"::facet::VariantAttribute::Rename({:?})"#,
                                    value
                                ));
                                display_name = value.to_string();
                            } else if key == "rename_all" {
                                let rule_str = attr[equal_pos + 1..].trim().trim_matches('"');
                                if let Some(rule) = RenameRule::from_str(rule_str) {
                                    rename_all_rule = Some(rule);
                                    attribute_list.push(format!(
                                        r#"::facet::VariantAttribute::RenameAll({:?})"#,
                                        rule_str
                                    ));
                                }
                            } else {
                                attribute_list.push(format!(
                                    r#"::facet::VariantAttribute::Arbitrary({:?})"#,
                                    attr
                                ));
                            }
                        } else {
                            attribute_list.push(format!(
                                r#"::facet::VariantAttribute::Arbitrary({:?})"#,
                                attr
                            ));
                        }
                    }
                }
            }
        }
    }

    if !has_explicit_rename && rename_rule.is_some() {
        display_name = rename_rule.unwrap().apply(variant_name);
    }

    let attributes_string = if attribute_list.is_empty() {
        format!(".name({:?})", display_name)
    } else {
        format!(
            ".name({:?}).attributes(&[{}])",
            display_name,
            attribute_list.join(", ")
        )
    };

    ContainerAttributes {
        code: attributes_string,
        rename_rule: rename_all_rule,
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
    fn as_enum_repr(&self) -> &'static str {
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

    fn as_rust_type(&self) -> &'static str {
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
    shadow_struct_defs: Vec<String>,
    variant_expressions: Vec<String>,
    repr_type: String,
}

type EnumVariant = Delimited<EnumVariantLike, Comma>;

struct EnumParams<'a> {
    // Core identification
    enum_name: &'a str,
    variants: &'a [EnumVariant],

    // Type information
    discriminant_type: Option<Discriminant>,
    bgp: &'a BoundedGenericParams,
    where_clauses: &'a str,

    // Attributes and customization
    rename_rule: Option<RenameRule>,
}

impl EnumParams<'_> {
    fn with_facet_lifetime(&self) -> BoundedGenericParams {
        self.bgp.with(BoundedGenericParam {
            bounds: None,
            param: GenericParamName::Lifetime("__facet".into()),
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

    // Collect shadow struct definitions separately from variant expressions
    let mut shadow_struct_defs = Vec::new();
    let mut variant_expressions = Vec::new();

    // first, create an enum to represent the discriminant type
    let shadow_discriminant_name = format!("__ShadowDiscriminant{}", params.enum_name);
    let all_variant_names = params
        .variants
        .iter()
        .map(|var_like| match &var_like.value.variant {
            EnumVariantData::Unit(unit) => unit.name.to_string(),
            EnumVariantData::Tuple(tuple) => tuple.name.to_string(),
            EnumVariantData::Struct(struct_var) => struct_var.name.to_string(),
        })
        .collect::<Vec<_>>()
        .join(", ");
    shadow_struct_defs.push(format!(
        "#[repr({repr})] enum {shadow_discriminant_name} {{ {all_variant_names} }}",
        // repr is either C or the explicit discriminant type
        repr = params
            .discriminant_type
            .map(|d| d.as_rust_type())
            .unwrap_or("C")
    ));

    // we'll also generate a shadow union for the fields
    let shadow_union_name = format!("__ShadowFields{}", params.enum_name);

    let all_union_fields = params
        .variants
        .iter()
        .map(|var_like| match &var_like.value.variant {
            EnumVariantData::Unit(unit) => unit.name.to_string(),
            EnumVariantData::Tuple(tuple) => tuple.name.to_string(),
            EnumVariantData::Struct(struct_var) => struct_var.name.to_string(),
        })
        .map(|variant_name| {
            format!(
                "{variant_name}: std::mem::ManuallyDrop<__ShadowField{}_{variant_name}{bgp}>",
                params.enum_name,
                bgp = facet_bgp.display_without_bounds()
            )
        })
        .collect::<Vec<_>>()
        .join(", ");

    shadow_struct_defs.push(format!(
        "#[repr(C)] union {shadow_union_name}{bgp} {} {{ {all_union_fields} }}",
        params.where_clauses,
        bgp = facet_bgp.display_with_bounds()
    ));

    // Create a shadow struct to represent the enum layout
    let shadow_repr_name = format!("__ShadowRepr{}", params.enum_name);

    shadow_struct_defs.push(format!(
        "#[repr(C)] struct {shadow_repr_name}{struct_bgp} {} {{
            _discriminant: {shadow_discriminant_name},
            _phantom: {phantom},
            _fields: {shadow_union_name}{fields_bgp},
        }}",
        params.where_clauses,
        struct_bgp = facet_bgp.display_with_bounds(),
        fields_bgp = facet_bgp.display_without_bounds(),
        phantom = facet_bgp.display_as_phantom_data(),
    ));

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
                let shadow_struct_name =
                    format!("__ShadowField{}_{variant_name}", params.enum_name);

                // Add shadow struct definition
                shadow_struct_defs.push(format!(
                    "#[repr(C)] struct {shadow_struct_name}{bgp} {} {{ _phantom: {phantom} }}",
                    params.where_clauses,
                    bgp = facet_bgp.display_with_bounds(),
                    phantom = facet_bgp.display_as_phantom_data(),
                ));

                // variant offset is offset of the `_fields` union
                variant_expressions.push(format!(
                    "::facet::Variant::builder()
                    {container_attributes}
                    .discriminant({discriminant_value})
                    .fields(::facet::StructDef::builder().unit().build())
                    {maybe_doc}
                    .build()",
                    container_attributes = container_attributes.code
                ));
            }
            EnumVariantData::Tuple(tuple) => {
                let variant_name = tuple.name.to_string();
                let container_attributes =
                    build_variant_attributes(&variant_name, &tuple.attributes, params.rename_rule);
                let maybe_doc = build_maybe_doc(&tuple.attributes);

                // Generate shadow struct for this tuple variant to calculate offsets
                let shadow_struct_name =
                    format!("__ShadowField{}_{variant_name}", params.enum_name);

                // Build the list of fields and types for the shadow struct
                let fields_with_types = tuple
                    .fields
                    .content
                    .0
                    .iter()
                    .enumerate()
                    .map(|(idx, field)| {
                        let typ = VerbatimDisplay(&field.value.typ).to_string();
                        format!("_{}: {}", idx, typ)
                    })
                    .collect::<Vec<String>>()
                    .join(", ");

                // Add shadow struct definition
                shadow_struct_defs.push(format!(
                    "#[repr(C)] struct {shadow_struct_name}{bgp} {} {{ {fields_with_types}, _phantom: {phantom} }}",
                    params.where_clauses,
                    bgp = facet_bgp.display_with_bounds(),
                    phantom = facet_bgp.display_as_phantom_data(),
                ));

                let variant_offset = format!(
                    "::core::mem::offset_of!({shadow_repr_name}{facet_bgp_use}, _fields)",
                    facet_bgp_use = facet_bgp.display_without_bounds()
                );

                // Build the list of field types with calculated offsets
                let fields = tuple
                    .fields
                    .content
                    .0
                    .iter()
                    .enumerate()
                    .map(|(idx, field)| {
                        let field_name = format!("_{idx}");
                        gen_struct_field(FieldInfo {
                            raw_field_name: &field_name,
                            normalized_field_name: &field_name,
                            field_type: &field.value.typ.tokens_to_string(),
                            struct_name: &shadow_struct_name,
                            bgp: &facet_bgp,
                            attrs: &field.value.attributes,
                            base_field_offset: Some(&variant_offset),
                            rename_rule: container_attributes.rename_rule,
                        })
                    })
                    .collect::<Vec<String>>()
                    .join(", ");

                // Add variant expression - now with discriminant
                variant_expressions.push(format!(
                    "{{
                        let fields: &'static [::facet::Field] = &const {{[
                            {fields}
                        ]}};

                        ::facet::Variant::builder()
                            {container_attributes}
                            .discriminant({discriminant_value})
                            .fields(::facet::StructDef::builder().tuple().fields(fields).build())
                            {maybe_doc}
                            .build()
                    }}",
                    container_attributes = container_attributes.code
                ));
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
                let shadow_struct_name =
                    format!("__ShadowField{}_{variant_name}", params.enum_name);

                // Build the list of fields and types
                let fields_with_types = struct_var
                    .fields
                    .content
                    .0
                    .iter()
                    .map(|field| {
                        let name = field.value.name.to_string();
                        let typ = VerbatimDisplay(&field.value.typ).to_string();
                        format!("{}: {}", name, typ)
                    })
                    .collect::<Vec<String>>()
                    .join(", ");

                // Add shadow struct definition
                shadow_struct_defs.push(format!(
                    "#[repr(C)] struct {shadow_struct_name}{bgp} {} {{ {fields_with_types}, _phantom: {phantom} }}",
                    params.where_clauses,
                    bgp = facet_bgp.display_with_bounds(),
                    phantom = facet_bgp.display_as_phantom_data(),
                ));

                let variant_offset = format!(
                    "::core::mem::offset_of!({shadow_repr_name}{facet_bgp_use}, _fields)",
                    facet_bgp_use = facet_bgp.display_without_bounds()
                );

                // Build the list of field types with calculated offsets
                let fields = struct_var
                    .fields
                    .content
                    .0
                    .iter()
                    .map(|field| {
                        // Handle raw identifiers (like r#type) by stripping the 'r#' prefix.
                        let raw_field_name = field.value.name.to_string(); // e.g., "r#type"
                        let normalized_field_name = normalize_ident_str(&raw_field_name); // e.g., "type"
                        let field_type = field.value.typ.tokens_to_string();
                        gen_struct_field(FieldInfo {
                            raw_field_name: &raw_field_name,
                            normalized_field_name,
                            field_type: &field_type,
                            struct_name: &shadow_struct_name,
                            bgp: &facet_bgp,
                            attrs: &field.value.attributes,
                            base_field_offset: Some(&variant_offset),
                            rename_rule: container_attributes.rename_rule,
                        })
                    })
                    .collect::<Vec<String>>()
                    .join(", ");

                // Add variant expression - now with discriminant
                variant_expressions.push(format!(
                    "{{
                        let fields: &'static [::facet::Field] = &const {{[
                            {fields}
                        ]}};

                        ::facet::Variant::builder()
                            {container_attributes}
                            .discriminant({discriminant_value})
                            .fields(::facet::StructDef::builder().struct_().fields(fields).build())
                            {maybe_doc}
                            .build()
                    }}",
                    container_attributes = container_attributes.code
                ));
            }
        }
        discriminant_value += 1;
    }

    ProcessedEnumBody {
        shadow_struct_defs,
        variant_expressions,
        repr_type: params.discriminant_type.map_or_else(
            || format!("from_discriminant_size::<{shadow_discriminant_name}>()"),
            |d| d.as_enum_repr().to_string(),
        ),
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

    // Collect shadow struct definitions separately from variant expressions
    let mut shadow_struct_defs = Vec::new();
    let mut variant_expressions = Vec::new();

    // We can safely unwrap because this function is only called when discriminant_type is Some
    let discriminant_type = params
        .discriminant_type
        .expect("discriminant_type should be Some when process_primitive_enum is called");

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

                variant_expressions.push(format!(
                    "::facet::Variant::builder()
                    {container_attributes}
                    .discriminant({discriminant_value})
                    .fields(::facet::StructDef::builder().unit().build())
                    {maybe_doc}
                    .build()",
                    container_attributes = container_attributes.code
                ));
            }
            EnumVariantData::Tuple(tuple) => {
                let variant_name = tuple.name.to_string();
                let container_attributes =
                    build_variant_attributes(&variant_name, &tuple.attributes, params.rename_rule);
                let maybe_doc = build_maybe_doc(&tuple.attributes);

                // Generate shadow struct for this tuple variant to calculate offsets
                let shadow_struct_name = format!("__Shadow{}_{}", params.enum_name, variant_name);

                // Build the list of fields and types for the shadow struct
                let fields_with_types = tuple
                    .fields
                    .content
                    .0
                    .iter()
                    .enumerate()
                    .map(|(idx, field)| {
                        let typ = VerbatimDisplay(&field.value.typ).to_string();
                        format!("_{}: {}", idx, typ)
                    })
                    .collect::<Vec<String>>()
                    .join(", ");

                // Add shadow struct definition
                shadow_struct_defs.push(format!(
                    "#[repr(C)] struct {shadow_struct_name}{bgp} {}  {{
                        _discriminant: {discriminant},
                        _phantom: {phantom},
                        {fields_with_types}
                    }}",
                    params.where_clauses,
                    bgp = facet_bgp.display_with_bounds(),
                    phantom = facet_bgp.display_as_phantom_data(),
                    discriminant = discriminant_type.as_rust_type(),
                ));

                // Build the list of field types with calculated offsets
                let fields = tuple
                    .fields
                    .content
                    .0
                    .iter()
                    .enumerate()
                    .map(|(idx, field)| {
                        let field_name = format!("_{idx}");
                        gen_struct_field(FieldInfo {
                            raw_field_name: &field_name,
                            normalized_field_name: &field_name,
                            field_type: &field.value.typ.tokens_to_string(),
                            struct_name: &shadow_struct_name,
                            bgp: &facet_bgp,
                            attrs: &field.value.attributes,
                            base_field_offset: None,
                            rename_rule: container_attributes.rename_rule,
                        })
                    })
                    .collect::<Vec<String>>()
                    .join(", ");

                // Add variant expression - now with discriminant
                variant_expressions.push(format!(
                    "{{
                        let fields: &'static [::facet::Field] = &const {{[
                            {fields}
                        ]}};

                        ::facet::Variant::builder()
                            {container_attributes}
                            .discriminant({discriminant_value})
                            .fields(::facet::StructDef::builder().tuple().fields(fields).build())
                            {maybe_doc}
                            .build()
                    }}",
                    container_attributes = container_attributes.code
                ));
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
                let shadow_struct_name = format!("__Shadow{}_{}", params.enum_name, variant_name);

                // Build the list of fields and types
                let fields_with_types = struct_var
                    .fields
                    .content
                    .0
                    .iter()
                    .map(|field| {
                        let name = field.value.name.to_string();
                        let typ = VerbatimDisplay(&field.value.typ).to_string();
                        format!("{}: {}", name, typ)
                    })
                    .collect::<Vec<String>>()
                    .join(", ");

                // Add shadow struct definition
                shadow_struct_defs.push(format!(
                    "#[repr(C)] struct {shadow_struct_name}{bgp} {} {{
                        _discriminant: {discriminant},
                        _phantom: {phantom},
                        {fields_with_types}
                    }}",
                    params.where_clauses,
                    bgp = facet_bgp.display_with_bounds(),
                    phantom = facet_bgp.display_as_phantom_data(),
                    discriminant = discriminant_type.as_rust_type(),
                ));

                // Build the list of field types with calculated offsets
                let fields = struct_var
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
                            field_type: &field.value.typ.tokens_to_string(),
                            struct_name: &shadow_struct_name,
                            bgp: &facet_bgp,
                            attrs: &field.value.attributes,
                            base_field_offset: None,
                            rename_rule: container_attributes.rename_rule,
                        })
                    })
                    .collect::<Vec<String>>()
                    .join(", ");

                // Add variant expression - now with discriminant
                // variant offset is zero since all fields are
                // already computed relative to the discriminant
                variant_expressions.push(format!(
                    "{{
                        let fields: &'static [::facet::Field] = &const {{[
                            {fields}
                        ]}};

                        ::facet::Variant::builder()
                            {container_attributes}
                            .discriminant({discriminant_value})
                            .fields(::facet::StructDef::builder().struct_().fields(fields).build())
                            {maybe_doc}
                            .build()
                    }}",
                    container_attributes = container_attributes.code
                ));
            }
        }
        discriminant_value += 1;
    }

    ProcessedEnumBody {
        shadow_struct_defs,
        variant_expressions,
        repr_type: discriminant_type.as_enum_repr().to_string(),
    }
}
