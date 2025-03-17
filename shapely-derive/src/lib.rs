use unsynn::*;

keyword! {
    Pub = "pub";
    Struct = "struct";
    Enum = "enum";
}

unsynn! {
    struct AttributeInner {
        name: Ident,
        attr: ParenthesisGroupContaining<Vec<Ident>>,
    }

    struct Attribute {
        _pound: Pound,
        body: BracketGroupContaining<AttributeInner>,
    }

    struct StructLike {
        attributes: Vec<Attribute>,
        _pub: Option<Pub>,
        _kw_struct: Struct,
        name: Ident,
        body: BraceGroupContaining<CommaDelimitedVec<FieldLike>>,
    }

    struct FieldLike {
        _pub: Option<Pub>,
        name: Ident,
        _colon: Colon,
        typ: Ident,
    }

    struct EnumLike {
        attributes: Vec<Attribute>,
        _pub: Option<Pub>,
        _kw_enum: Enum,
        name: Ident,
        body: BraceGroupContaining<CommaDelimitedVec<EnumVariantLike>>,
    }

    enum EnumVariantLike {
        Unit(UnitVariant),
        Tuple(TupleVariant),
        Struct(StructVariant),
    }

    struct UnitVariant {
        name: Ident,
    }

    struct TupleVariant {
        name: Ident,
        _paren: ParenthesisGroupContaining<CommaDelimitedVec<Ident>>,
    }

    struct StructVariant {
        name: Ident,
        _brace: BraceGroupContaining<CommaDelimitedVec<FieldLike>>,
    }
}

#[proc_macro_derive(Shapely)]
pub fn shapely_derive(input: proc_macro::TokenStream) -> proc_macro::TokenStream {
    let input = TokenStream::from(input);
    let mut i = input.to_token_iter();

    // Try to parse as struct first
    let struct_result = i.parse::<StructLike>();

    if let Ok(parsed) = struct_result {
        let struct_name = parsed.name.to_string();
        let fields = parsed
            .body
            .content
            .0
            .iter()
            .map(|field| field.value.name.to_string())
            .collect::<Vec<String>>();

        // Create the fields string for struct_fields! macro
        let fields_str = fields.join(", ");

        // Generate the impl
        let output = format!(
            r#"
            impl shapely::Shapely for {struct_name} {{
                fn shape() -> shapely::Shape {{
                    shapely::Shape {{
                        name: |f| std::fmt::Write::write_str(f, "{struct_name}"),
                        typeid: shapely::mini_typeid::of::<Self>(),
                        layout: std::alloc::Layout::new::<Self>(),
                        innards: shapely::Innards::Struct {{
                            fields: shapely::struct_fields!({struct_name}, ({fields_str})),
                        }},
                        set_to_default: None,
                        drop_in_place: Some(|ptr| unsafe {{ std::ptr::drop_in_place(ptr as *mut Self) }}),
                    }}
                }}
            }}
        "#
        );
        return output.into_token_stream().into();
    }

    // Try to parse as enum
    i = input.to_token_iter(); // Reset iterator
    let enum_result = i.parse::<EnumLike>();

    if let Ok(parsed) = enum_result {
        let enum_name = parsed.name.to_string();

        // Check for explicit repr attribute
        let has_repr = parsed
            .attributes
            .iter()
            .any(|attr| attr.body.content.name == "repr");

        if !has_repr {
            return r#"compile_error!("Enums must have an explicit representation (e.g. #[repr(u8)]) to be used with Shapely")"#
                .into_token_stream()
                .into();
        }

        // Process each variant
        let variants = parsed
            .body
            .content
            .0
            .iter()
            .map(|var_like| match &var_like.value {
                EnumVariantLike::Unit(unit) => {
                    let variant_name = unit.name.to_string();
                    format!("shapely::enum_unit_variant!({enum_name}, {variant_name})")
                }
                EnumVariantLike::Tuple(tuple) => {
                    let variant_name = tuple.name.to_string();
                    let field_types = tuple
                        ._paren
                        .content
                        .0
                        .iter()
                        .map(|field| field.value.to_string())
                        .collect::<Vec<String>>()
                        .join(", ");

                    format!(
                        "shapely::enum_tuple_variant!({enum_name}, {variant_name}, [{field_types}])"
                    )
                }
                EnumVariantLike::Struct(struct_var) => {
                    let variant_name = struct_var.name.to_string();
                    let fields = struct_var
                        ._brace
                        .content
                        .0
                        .iter()
                        .map(|field| {
                            let name = field.value.name.to_string();
                            let typ = field.value.typ.to_string();
                            format!("{name}: {typ}")
                        })
                        .collect::<Vec<String>>()
                        .join(", ");

                    format!(
                        "shapely::enum_struct_variant!({enum_name}, {variant_name}, {{{fields}}})"
                    )
                }
            })
            .collect::<Vec<String>>()
            .join(", ");

        // Extract the repr type
        let mut repr_type = "Default"; // Default fallback
        for attr in &parsed.attributes {
            if attr.body.content.name == "repr" {
                // Access the Vec directly from the attr.body.content.attr.content field
                let attrs = &attr.body.content.attr.content;
                if !attrs.is_empty() {
                    repr_type = match attrs[0].to_string().as_str() {
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
                        _ => "Default", // Unknown repr type
                    };
                }
                break;
            }
        }

        // Generate the impl
        let output = format!(
            r#"
            impl shapely::Shapely for {enum_name} {{
                fn shape() -> shapely::Shape {{
                    shapely::Shape {{
                        name: |f| std::fmt::Write::write_str(f, "{enum_name}"),
                        typeid: shapely::mini_typeid::of::<Self>(),
                        layout: std::alloc::Layout::new::<Self>(),
                        innards: shapely::Innards::Enum {{
                            variants: shapely::enum_variants!({enum_name}, [{variants}]),
                            repr: shapely::EnumRepr::{repr_type},
                        }},
                        set_to_default: None,
                        drop_in_place: Some(|ptr| unsafe {{ std::ptr::drop_in_place(ptr as *mut Self) }}),
                    }}
                }}
            }}
        "#
        );
        return output.into_token_stream().into();
    }

    // If we get here, couldn't parse as struct or enum
    panic!("Could not parse input as struct or enum");
}
