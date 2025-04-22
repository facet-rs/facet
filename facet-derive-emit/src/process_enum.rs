use super::*;

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
pub(crate) fn process_enum(parsed: Enum) -> TokenStream {
    let enum_name = parsed.name.to_string();
    let (generics_def, generics_use) = generics_split_for_impl(parsed.generics.as_ref());
    let where_clauses = build_where_clauses(parsed.clauses.as_ref(), parsed.generics.as_ref());
    let type_params = build_type_params(parsed.generics.as_ref());

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

    let processed_body = match (repr_c, discriminant_type) {
        (true, _) => {
            // C-style enum, no discriminant type
            process_c_style_enum(
                &enum_name,
                &parsed.body.content.0,
                discriminant_type,
                &generics_def,
                &generics_use,
                &where_clauses,
            )
        }
        (false, Some(discriminant_type)) => process_primitive_enum(
            &enum_name,
            &parsed.body.content.0,
            discriminant_type,
            &generics_def,
            &generics_use,
            &where_clauses,
        ),
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
unsafe impl<'facet, {generics_def}> ::facet::Facet<'facet> for {enum_name}<{generics_use}> {where_clauses} {{
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
            .vtable(::facet::value_vtable!(
                Self,
                |f, _opts| ::core::fmt::Write::write_str(f, "{enum_name}")
            ))
            .def(::facet::Def::Enum(::facet::EnumDef::builder()
                // Use variant expressions that just reference the shadow structs
                // which are now defined above
                .variants(__facet_variants)
                .repr(::facet::EnumRepr::{repr_type})
                .build()))
            {maybe_container_doc}
            .build()
    }};
}}
        "#,
    );

    // Output generated code
    // Don't use panic for debugging as it makes code unreachable

    // Return the generated code
    output.into_token_stream()
}

/// C-style enums (i.e. #[repr(C)], #[repr(C, u*)] and #[repr(C, i*)]) are laid out
/// as a #[repr(C)] struct with two fiels: the discriminant and the union of all the variants.
///
/// See: <https://doc.rust-lang.org/reference/type-layout.html#r-layout.repr.primitive.adt>
///
/// To calculate the offsets of each variant, we create a shadow struct that mimics this
/// structure and use the `offset_of!` macro to calculate the offsets of each field.
fn process_c_style_enum(
    enum_name: &str,
    variants: &[EnumVariant],
    discriminant_type: Option<Discriminant>,
    generics_def: &str,
    generics_use: &str,
    where_clauses: &str,
) -> ProcessedEnumBody {
    // Collect shadow struct definitions separately from variant expressions
    let mut shadow_struct_defs = Vec::new();
    let mut variant_expressions = Vec::new();

    // first, create an enum to represent the discriminant type
    let shadow_discriminant_name = format!("__ShadowDiscriminant{enum_name}");
    let all_variant_names = variants
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
        repr = discriminant_type.map(|d| d.as_rust_type()).unwrap_or("C")
    ));

    // we'll also generate a shadow union for the fields
    let shadow_union_name = format!("__ShadowFields{enum_name}");
    let all_union_fields = variants
        .iter()
        .map(|var_like| match &var_like.value.variant {
            EnumVariantData::Unit(unit) => unit.name.to_string(),
            EnumVariantData::Tuple(tuple) => tuple.name.to_string(),
            EnumVariantData::Struct(struct_var) => struct_var.name.to_string(),
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
            _phantom: ::core::marker::PhantomData<*mut &'facet ()>,
            _fields: {shadow_union_name},
        }}",
    ));

    // Discriminant values are either manually defined, or incremented from the last one
    // See: <https://doc.rust-lang.org/reference/items/enumerations.html#implicit-discriminants>
    let mut discriminant_value = 0;
    for var_like in variants.iter() {
        if let Some(x) = &var_like.value.discriminant {
            discriminant_value = get_discriminant_value(&x.second);
        }
        match &var_like.value.variant {
            EnumVariantData::Unit(unit) => {
                let variant_name = unit.name.to_string();
                let maybe_doc = build_maybe_doc(&unit.attributes);

                // Generate shadow struct for this tuple variant to calculate offsets
                let shadow_struct_name = format!("__ShadowField{enum_name}_{variant_name}");

                // Add shadow struct definition
                shadow_struct_defs.push(format!("#[repr(C)] struct {shadow_struct_name};",));

                // variant offset is offset of the `_fields` union
                variant_expressions.push(format!(
                    "::facet::Variant::builder()
                    .name({variant_name:?})
                    .discriminant({discriminant_value})
                    .fields(::facet::Struct::builder().unit().build())
                    {maybe_doc}
                    .build()",
                ));
            }
            EnumVariantData::Tuple(tuple) => {
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
                    "#[repr(C)] struct {shadow_struct_name}<'facet, {generics_def}> {where_clauses} {{  {fields_with_types} }}",
                ));

                let variant_offset =
                    format!("::core::mem::offset_of!({shadow_repr_name}, _fields)");

                // Build the list of field types with calculated offsets
                let fields = tuple
                    .fields
                    .content
                    .0
                    .iter()
                    .enumerate()
                    .map(|(idx, field)| {
                        let field_name = format!("_{idx}");
                        gen_struct_field(
                            &field_name,
                            &field.value.typ.tokens_to_string(),
                            &shadow_struct_name,
                            generics_use,
                            &field.value.attributes,
                            Some(&variant_offset),
                        )
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
                            .name({variant_name:?})
                            .discriminant({discriminant_value})
                            .fields(::facet::Struct::builder().tuple().fields(fields).build())
                            {maybe_doc}
                            .build()
                    }}",
                ));
            }
            EnumVariantData::Struct(struct_var) => {
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
                    "#[repr(C)] struct {shadow_struct_name}<'facet, {generics_def}> {where_clauses} {{  {fields_with_types} }}"
                ));

                let variant_offset =
                    format!("::core::mem::offset_of!({shadow_repr_name}, _fields)");

                // Build the list of field types with calculated offsets
                let fields = struct_var
                    .fields
                    .content
                    .0
                    .iter()
                    .map(|field| {
                        let field_name = field.value.name.to_string();
                        let field_type = field.value.typ.tokens_to_string();
                        gen_struct_field(
                            &field_name,
                            &field_type,
                            &shadow_struct_name,
                            generics_use,
                            &field.value.attributes,
                            Some(&variant_offset),
                        )
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
                            .name({variant_name:?})
                            .discriminant({discriminant_value})
                            .fields(::facet::Struct::builder().struct_().fields(fields).build())
                            {maybe_doc}
                            .build()
                    }}",
                ));
            }
        }
        discriminant_value += 1;
    }

    ProcessedEnumBody {
        shadow_struct_defs,
        variant_expressions,
        repr_type: discriminant_type.map_or_else(
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
fn process_primitive_enum(
    enum_name: &str,
    variants: &[EnumVariant],
    discriminant_type: Discriminant,
    generics_def: &str,
    generics_use: &str,
    where_clauses: &str,
) -> ProcessedEnumBody {
    // Collect shadow struct definitions separately from variant expressions
    let mut shadow_struct_defs = Vec::new();
    let mut variant_expressions = Vec::new();

    // Discriminant values are either manually defined, or incremented from the last one
    // See: <https://doc.rust-lang.org/reference/items/enumerations.html#implicit-discriminants>
    let mut discriminant_value = 0;
    for var_like in variants.iter() {
        if let Some(x) = &var_like.value.discriminant {
            discriminant_value = get_discriminant_value(&x.second);
        }
        match &var_like.value.variant {
            EnumVariantData::Unit(unit) => {
                let variant_name = unit.name.to_string();
                let maybe_doc = build_maybe_doc(&unit.attributes);

                variant_expressions.push(format!(
                    "::facet::Variant::builder()
                    .name({variant_name:?})
                    .discriminant({discriminant_value})
                    .fields(::facet::Struct::builder().unit().build())
                    {maybe_doc}
                    .build()",
                ));
            }
            EnumVariantData::Tuple(tuple) => {
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
                    "#[repr(C)] struct {shadow_struct_name}<'facet, {generics_def}> {where_clauses}  {{ _discriminant: {}, _phantom: ::core::marker::PhantomData<*mut &'facet ()>, {fields_with_types} }}",
                    discriminant_type.as_rust_type(),
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
                        gen_struct_field(
                            &field_name,
                            &field.value.typ.tokens_to_string(),
                            &shadow_struct_name,
                            generics_use,
                            &field.value.attributes,
                            None,
                        )
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
                            .name({variant_name:?})
                            .discriminant({discriminant_value})
                            .fields(::facet::Struct::builder().tuple().fields(fields).build())
                            {maybe_doc}
                            .build()
                    }}",
                ));
            }
            EnumVariantData::Struct(struct_var) => {
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
                    "#[repr(C)] struct {shadow_struct_name}<'facet, {generics_def}> {where_clauses} {{ _discriminant: {}, _phantom: ::core::marker::PhantomData<*mut &'facet ()>, {fields_with_types} }}",
                    discriminant_type.as_rust_type(),
                ));

                // Build the list of field types with calculated offsets
                let fields = struct_var
                    .fields
                    .content
                    .0
                    .iter()
                    .map(|field| {
                        let field_name = field.value.name.to_string();
                        gen_struct_field(
                            &field_name,
                            &field.value.typ.tokens_to_string(),
                            &shadow_struct_name,
                            generics_use,
                            &field.value.attributes,
                            None,
                        )
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
                            .name({variant_name:?})
                            .discriminant({discriminant_value})
                            .fields(::facet::Struct::builder().struct_().fields(fields).build())
                            {maybe_doc}
                            .build()
                    }}",
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
