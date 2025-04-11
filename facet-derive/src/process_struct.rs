use super::*;

/// Processes a regular struct to implement Facet
///
/// Example input:
/// ```rust
/// struct Blah {
///     foo: u32,
///     bar: String,
/// }
/// ```
pub(crate) fn process_struct(parsed: Struct) -> proc_macro::TokenStream {
    let struct_name = parsed.name.to_string();

    // Generate field definitions
    let (generics_def, generics_use) = generics_split_for_impl(parsed.generics.as_ref());
    let kind;
    let where_clauses;
    let fields = match &parsed.kind {
        StructKind::Struct { clauses, fields } => {
            kind = "facet::StructKind::Struct";
            where_clauses = clauses.as_ref();
            fields
                .content
                .0
                .iter()
                .map(|field| {
                    let field_name = field.value.name.to_string();
                    gen_struct_field(
                        &field_name,
                        &struct_name,
                        &generics_use,
                        &field.value.attributes,
                    )
                })
                .collect::<Vec<String>>()
        }
        StructKind::TupleStruct {
            fields,
            clauses,
            semi: _,
        } => {
            kind = "facet::StructKind::TupleStruct";
            where_clauses = clauses.as_ref();
            fields
                .content
                .0
                .iter()
                .enumerate()
                .map(|(index, field)| {
                    let field_name = format!("{index}");
                    gen_struct_field(
                        &field_name,
                        &struct_name,
                        &generics_use,
                        &field.value.attributes,
                    )
                })
                .collect::<Vec<String>>()
        }
        StructKind::UnitStruct { clauses, semi: _ } => {
            kind = "facet::StructKind::Unit";
            where_clauses = clauses.as_ref();
            vec![]
        }
    }
    .join(", ");

    let static_decl = if parsed.generics.is_none() {
        generate_static_decl(&struct_name)
    } else {
        String::new()
    };
    let maybe_container_doc = build_maybe_doc(&parsed.attributes);
    let where_clauses = where_clauses.map_or(String::new(), ToString::to_string);

    // Generate the impl
    let output = format!(
        r#"
{static_decl}

#[automatically_derived]
unsafe impl<{generics_def}> facet::Facet for {struct_name}<{generics_use}> {where_clauses} {{
    const SHAPE: &'static facet::Shape = &const {{
        let fields: &'static [facet::Field] = &const {{[{fields}]}};

        facet::Shape::builder()
            .id(facet::ConstTypeId::of::<Self>())
            .layout(core::alloc::Layout::new::<Self>())
            .vtable(facet::value_vtable!(
                Self,
                |f, _opts| core::fmt::Write::write_str(f, "{struct_name}")
            ))
            .def(facet::Def::Struct(facet::StructDef::builder()
                .kind({kind})
                .fields(fields)
                .build()))
            {maybe_container_doc}
            .build()
    }};
}}
        "#
    );

    output.into_token_stream().into()
}
