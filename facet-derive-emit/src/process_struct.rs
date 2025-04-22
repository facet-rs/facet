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
pub(crate) fn process_struct(parsed: Struct) -> TokenStream {
    let struct_name = parsed.name.to_string();

    // Generate field definitions
    let (generics_def, generics_use) = generics_split_for_impl(parsed.generics.as_ref());
    let kind;
    let where_clauses;
    let type_params = build_type_params(parsed.generics.as_ref());
    let set_attributes = build_container_attributes(&parsed.attributes);

    let fields = match &parsed.kind {
        StructKind::Struct { clauses, fields } => {
            kind = "::facet::StructKind::Struct";
            where_clauses = clauses.as_ref();
            fields
                .content
                .0
                .iter()
                .map(|field| {
                    let field_name = field.value.name.to_string();
                    gen_struct_field(
                        &field_name,
                        &field.value.typ.tokens_to_string(),
                        &struct_name,
                        &generics_use,
                        &field.value.attributes,
                        None,
                    )
                })
                .collect::<Vec<String>>()
        }
        StructKind::TupleStruct {
            fields,
            clauses,
            semi: _,
        } => {
            kind = "::facet::StructKind::TupleStruct";
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
                        &field.value.typ.tokens_to_string(),
                        &struct_name,
                        &generics_use,
                        &field.value.attributes,
                        None,
                    )
                })
                .collect::<Vec<String>>()
        }
        StructKind::UnitStruct { clauses, semi: _ } => {
            kind = "::facet::StructKind::Unit";
            where_clauses = clauses.as_ref();
            vec![]
        }
    }
    .join(", ");

    let where_clauses = build_where_clauses(where_clauses, parsed.generics.as_ref());
    let static_decl = if parsed.generics.is_none() {
        generate_static_decl(&struct_name)
    } else {
        String::new()
    };
    let maybe_container_doc = build_maybe_doc(&parsed.attributes);

    let mut invariant_maybe = "".to_string();
    let invariant_attrs = parsed
        .attributes
        .iter()
        .filter_map(|attr| match &attr.body.content {
            AttributeInner::Facet(facet_attr) => match &facet_attr.inner.content {
                FacetInner::Invariants(invariant_inner) => Some(invariant_inner),
                _ => None,
            },
            _ => None,
        })
        .collect::<Vec<_>>();

    if !invariant_attrs.is_empty() {
        let tests = invariant_attrs
            .iter()
            .map(|invariant| {
                let invariant_name = invariant.value.as_str();
                format!(
                    r#"
                    if !value.{invariant_name}() {{
                        return false;
                    }}
                    "#
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let invariant_fn = format!(
            r#"
            unsafe fn invariants<'mem>(value: ::facet::PtrConst<'mem>) -> bool {{
                let value = value.get::<{struct_name}<{generics_use}>>();
                {tests}
                true
            }}
            "#
        );

        invariant_maybe = format!(
            r#"
            {invariant_fn}

            vtable.invariants = Some(invariants);
            "#
        );
    }

    // Generate the impl
    let output = format!(
        r#"
{static_decl}

#[automatically_derived]
unsafe impl<'facet, {generics_def}> ::facet::Facet<'facet> for {struct_name}<{generics_use}> {where_clauses} {{
    const SHAPE: &'static ::facet::Shape = &const {{
        let fields: &'static [::facet::Field] = &const {{[{fields}]}};

        let vtable = &const {{
            let mut vtable = ::facet::value_vtable_inner!(
                Self,
                |f, _opts| ::core::fmt::Write::write_str(f, "{struct_name}")
            );
            {invariant_maybe}
            vtable
        }};

        ::facet::Shape::builder()
            .id(::facet::ConstTypeId::of::<Self>())
            .layout(::core::alloc::Layout::new::<Self>())
            {type_params}
            .vtable(vtable)
            .def(::facet::Def::Struct(::facet::Struct::builder()
                .kind({kind})
                .fields(fields)
                .build()))
            {maybe_container_doc}
            {set_attributes}
            .build()
    }};
}}
        "#
    );

    output.into_token_stream()
}
