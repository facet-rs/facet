use super::*;
use unsynn::*;

/// Processes an enum to implement Facet
///
/// Example input:
/// ```rust
/// #[repr(u8)]
/// enum Color {
///     Red,
///     Green,
///     Blue(u8, u8),
///     Custom { r: u8, g: u8, b: u8 }
/// }
/// ```
pub(crate) fn process_enum(parsed: Enum) -> proc_macro::TokenStream {
    let enum_name = parsed.name.to_string();

    // Find the explicit repr attribute and map to
    // the corresponding discriminant type
    let (discriminant_type, repr_type) = match parsed.attributes.iter().find_map(|attr| {
        if let AttributeInner::Repr(repr_attr) = &attr.body.content {
            Some(repr_attr.attr.content.to_string())
        } else {
            None
        }
    }) {
        Some(type_name) => {
            let repr_type = match type_name.as_str() {
                "u8" => "U8",
                "u16" => "U16",
                "u32" => "U32",
                "u64" => "U64",
                "usize" => "USize",
                "i8" => "I8",
                "i16" => "I16",
                "i32" => "I32",
                "i64" => "I64",
                "isize" => "ISize",
                _ => {
                    return r#"compile_error!("Enums must have an explicit *primitive* representation (e.g. #[repr(u8)]) to be used with Facet")"#
                    .into_token_stream()
                    .into();
                }
            };
            (type_name, repr_type)
        }
        None => {
            return r#"compile_error!("Enums must have an explicit *primitive* representation (e.g. #[repr(u8)]) to be used with Facet")"#
                    .into_token_stream()
                    .into();
        }
    };

    // Collect shadow struct definitions separately from variant expressions
    let mut shadow_struct_defs = Vec::new();
    let mut variant_expressions = Vec::new();

    // Process each variant using enumerate to get discriminant values
    for (discriminant_value, var_like) in parsed.body.content.0.iter().enumerate() {
        match &var_like.value {
            EnumVariantLike::Unit(unit) => {
                let variant_name = unit.name.to_string();
                let maybe_doc = build_maybe_doc(&unit.attributes);

                variant_expressions.push(format!(
                    "facet::Variant::builder()
                    .name({variant_name:?})
                    .discriminant(Some({discriminant_value}))
                    .kind(facet::VariantKind::Unit)
                    {maybe_doc}
                    .build()",
                ));
            }
            EnumVariantLike::Tuple(tuple) => {
                let variant_name = tuple.name.to_string();
                let maybe_doc = build_maybe_doc(&tuple.attributes);

                // Generate shadow struct for this tuple variant to calculate offsets
                let shadow_struct_name = format!("__Shadow{}_{}", enum_name, variant_name);

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
                    "#[repr(C)] struct {} {{ _discriminant: {}, {} }}",
                    shadow_struct_name, discriminant_type, fields_with_types
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
                        gen_struct_field(&field_name, &shadow_struct_name, &field.value.attributes)
                    })
                    .collect::<Vec<String>>()
                    .join(", ");

                // Add variant expression - now with discriminant
                variant_expressions.push(format!(
                    "{{
                        static FIELDS: &[facet::Field] = &[
                            {fields}
                        ];

                        facet::Variant::builder()
                            .name({variant_name:?})
                            .discriminant(Some({discriminant_value}))
                            .kind(facet::VariantKind::Tuple {{ fields: FIELDS }})
                            {maybe_doc}
                            .build()
                    }}",
                ));
            }
            EnumVariantLike::Struct(struct_var) => {
                let variant_name = struct_var.name.to_string();
                let maybe_doc = build_maybe_doc(&struct_var.attributes);

                // Generate shadow struct for this struct variant to calculate offsets
                let shadow_struct_name = format!("__Shadow{}_{}", enum_name, variant_name);

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
                    "#[repr(C)] struct {} {{ _discriminant: {}, {} }}",
                    shadow_struct_name, discriminant_type, fields_with_types
                ));

                // Build the list of field types with calculated offsets
                let fields = struct_var
                    .fields
                    .content
                    .0
                    .iter()
                    .map(|field| {
                        let field_name = field.value.name.to_string();
                        gen_struct_field(&field_name, &shadow_struct_name, &field.value.attributes)
                    })
                    .collect::<Vec<String>>()
                    .join(", ");

                // Add variant expression - now with discriminant
                variant_expressions.push(format!(
                    "{{
                        static FIELDS: &[facet::Field] = &[
                            {fields}
                        ];

                        facet::Variant::builder()
                            .name({variant_name:?})
                            .discriminant(Some({discriminant_value}))
                            .kind(facet::VariantKind::Struct {{ fields: FIELDS }})
                            {maybe_doc}
                            .build()
                    }}",
                ));
            }
        }
    }

    // Join the shadow struct definitions and variant expressions
    let shadow_structs = shadow_struct_defs.join("\n\n");
    let variants = variant_expressions.join(", ");

    let static_decl = generate_static_decl(&enum_name);
    let maybe_container_doc = build_maybe_doc(&parsed.attributes);

    // Generate the impl
    let output = format!(
        r#"
{static_decl}

#[automatically_derived]
unsafe impl facet::Facet for {enum_name} {{
    const SHAPE: &'static facet::Shape = &const {{
        // Define all shadow structs at the beginning of the const block
        // to ensure they're in scope for offset_of! macros
        {shadow_structs}

        facet::Shape::builder()
            .id(facet::ConstTypeId::of::<{enum_name}>())
            .layout(core::alloc::Layout::new::<Self>())
            .vtable(facet::value_vtable!(
                {enum_name},
                |f, _opts| core::fmt::Write::write_str(f, "{enum_name}")
            ))
            .def(facet::Def::Enum(facet::EnumDef::builder()
                // Use variant expressions that just reference the shadow structs
                // which are now defined above
                .variants(&const {{
                    static VARIANTS: &[facet::Variant] = &[ {variants} ];
                    VARIANTS
                }})
                .repr(facet::EnumRepr::{repr_type})
                .build()))
            {maybe_container_doc}
            .build()
    }};
}}
        "#
    );

    // Output generated code
    // Don't use panic for debugging as it makes code unreachable

    // Return the generated code
    output.into_token_stream().into()
}
