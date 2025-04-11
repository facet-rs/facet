use super::*;
use unsynn::*;

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
    let mut repr_type = None;

    for attr in attr_iter {
        let attr = attr.value.to_string();
        match attr.as_str() {
            // this is #[repr(C)]
            "C" => repr_c = true,

            // set the repr type
            // NOTE: we're not worried about multiple
            // clashing types here -- that's rustc's problem
            "u8" => repr_type = Some(Discriminant::U8),
            "u16" => repr_type = Some(Discriminant::U16),
            "u32" => repr_type = Some(Discriminant::U32),
            "u64" => repr_type = Some(Discriminant::U64),
            "usize" => repr_type = Some(Discriminant::USize),
            "i8" => repr_type = Some(Discriminant::I8),
            "i16" => repr_type = Some(Discriminant::I16),
            "i32" => repr_type = Some(Discriminant::I32),
            "i64" => repr_type = Some(Discriminant::I64),
            "isize" => repr_type = Some(Discriminant::ISize),
            _ => {
                return r#"compile_error!("Facet only supports enums with a primitive representation (e.g. #[repr(u8)]) or C-style (e.g. #[repr(C)]")"#
            .into_token_stream()
            .into();
            }
        }
    }

    match (repr_c, repr_type) {
        (true, _) => {
            // C-style enum, no discriminant type
            process_c_style_enum(parsed, repr_type)
        }
        (false, Some(repr_type)) => process_primitive_enum(parsed, repr_type),
        _ => {
            r#"compile_error!("Enums must have an explicit representation (e.g. #[repr(u8)] or #[repr(C)]) to be used with Facet")"#
            .into_token_stream()
            .into()
        }
    }
}

/// C-style enums (i.e. #[repr(C)], #[repr(C, u*)] and #[repr(C, i*)]) are laid out
/// as a #[repr(C)] struct with two fiels: the discriminant and the union of all the variants.
///
/// See: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.primitive.adt
///
/// To calculate the offsets of each variant, we create a shadow struct that mimics this
/// structure and use the `offset_of!` macro to calculate the offsets of each field.
fn process_c_style_enum(
    parsed: Enum,
    discriminant_type: Option<Discriminant>,
) -> proc_macro::TokenStream {
    let enum_name = parsed.name.to_string();

    // Collect shadow struct definitions separately from variant expressions
    let mut shadow_struct_defs = Vec::new();
    let mut variant_expressions = Vec::new();

    // first, create an enum to represent the discriminant type
    let shadow_discriminant_name = format!("__ShadowDiscriminant{enum_name}");
    let all_variant_names = parsed
        .body
        .content
        .0
        .iter()
        .map(|var_like| match &var_like.value {
            EnumVariantLike::Unit(unit) => unit.name.to_string(),
            EnumVariantLike::Tuple(tuple) => tuple.name.to_string(),
            EnumVariantLike::Struct(struct_var) => struct_var.name.to_string(),
        })
        .collect::<Vec<_>>()
        .join(", ");
    shadow_struct_defs.push(format!(
        "#[repr({repr})] enum {shadow_discriminant_name} {{ {all_variant_names} }}",
        // repr is either C or the explicit discriminant type
        repr = discriminant_type.map(|d| d.as_rust_type()).unwrap_or("C")
    ));

    // we'll also generate a shadow union for the fields
    let shadow_union_name = format!("__ShadowFields{enum_name}");
    let all_union_fields = parsed
        .body
        .content
        .0
        .iter()
        .map(|var_like| match &var_like.value {
            EnumVariantLike::Unit(unit) => unit.name.to_string(),
            EnumVariantLike::Tuple(tuple) => tuple.name.to_string(),
            EnumVariantLike::Struct(struct_var) => struct_var.name.to_string(),
        })
        .map(|variant_name| {
            format!(
                "{variant_name}: std::mem::ManuallyDrop<__ShadowField{enum_name}_{variant_name}>"
            )
        })
        .collect::<Vec<_>>()
        .join(", ");

    shadow_struct_defs.push(format!(
        "#[repr(C)] union {shadow_union_name} {{ {all_union_fields} }}",
    ));

    // Create a shadow struct to represent the enum layout
    let shadow_repr_name = format!("__ShadowRepr{enum_name}");

    shadow_struct_defs.push(format!(
        "#[repr(C)] struct {shadow_repr_name} {{
            _discriminant: {shadow_discriminant_name},
            _fields: {shadow_union_name},
        }}",
    ));

    // Process each variant using enumerate to get discriminant values
    for (discriminant_value, var_like) in parsed.body.content.0.iter().enumerate() {
        match &var_like.value {
            EnumVariantLike::Unit(unit) => {
                let variant_name = unit.name.to_string();
                let maybe_doc = build_maybe_doc(&unit.attributes);

                // Generate shadow struct for this tuple variant to calculate offsets
                let shadow_struct_name = format!("__ShadowField{enum_name}_{variant_name}");

                // Add shadow struct definition
                shadow_struct_defs.push(format!("#[repr(C)] struct {shadow_struct_name};",));

                // variant offset is offset of the `_fields` union
                variant_expressions.push(format!(
                    "facet::Variant::builder()
                    .name({variant_name:?})
                    .discriminant(Some({discriminant_value}))
                    .offset(::core::mem::offset_of!({shadow_repr_name}, _fields))
                    .kind(facet::VariantKind::Unit)
                    {maybe_doc}
                    .build()",
                ));
            }
            EnumVariantLike::Tuple(tuple) => {
                let variant_name = tuple.name.to_string();
                let maybe_doc = build_maybe_doc(&tuple.attributes);

                // Generate shadow struct for this tuple variant to calculate offsets
                let shadow_struct_name = format!("__ShadowField{enum_name}_{variant_name}");

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
                    "#[repr(C)] struct {shadow_struct_name} {{  {fields_with_types} }}",
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
                            .offset(::core::mem::offset_of!({shadow_repr_name}, _fields))
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
                let shadow_struct_name = format!("__ShadowField{}_{}", enum_name, variant_name);

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
                    "#[repr(C)] struct {shadow_struct_name} {{  {fields_with_types} }}"
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
                            .offset(::core::mem::offset_of!({shadow_repr_name}, _fields))
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
        "#,
        // for #[repr(C)] enums, we compute the enum representation from the size of the discriminant
        // for #[repr(C, u*)] enums, we use the explicit type
        repr_type = discriminant_type.map_or_else(
            || format!("from_discriminant_size::<{shadow_discriminant_name}>()"),
            |d| d.as_enum_repr().to_string()
        )
    );

    // Output generated code
    // Don't use panic for debugging as it makes code unreachable

    // Return the generated code
    output.into_token_stream().into()
}

/// Primitive enums (i.e. #[repr(u*)] and #[repr(i*)]) are laid out
/// as a union of all the variants, with the discriminant as an "inner" tag in the struct.
///
/// See: https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.primitive.adt
///
/// To calculate the offsets of each variant, we create a shadow struct that mimics this
/// structure and use the `offset_of!` macro to calculate the offsets of each field.
fn process_primitive_enum(
    parsed: Enum,
    discriminant_type: Discriminant,
) -> proc_macro::TokenStream {
    let enum_name = parsed.name.to_string();

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
                    .offset(0)
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
                    shadow_struct_name,
                    discriminant_type.as_rust_type(),
                    fields_with_types
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
                            .offset(0)
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
                    shadow_struct_name,
                    discriminant_type.as_rust_type(),
                    fields_with_types
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
                // variant offset is zero since all fields are
                // already computed relative to the discriminant
                variant_expressions.push(format!(
                    "{{
                        static FIELDS: &[facet::Field] = &[
                            {fields}
                        ];

                        facet::Variant::builder()
                            .name({variant_name:?})
                            .discriminant(Some({discriminant_value}))
                            .offset(0)
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
        "#,
        repr_type = discriminant_type.as_enum_repr()
    );

    // Output generated code
    // Don't use panic for debugging as it makes code unreachable

    // Return the generated code
    output.into_token_stream().into()
}
