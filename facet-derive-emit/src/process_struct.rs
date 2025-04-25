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
    let is_transparent = parsed.is_transparent();
    let struct_name = parsed.name.to_string();

    // Generate field definitions
    let bgp = BoundedGenericParams::parse(parsed.generics.as_ref());

    let kind;
    let where_clauses;
    let type_params = build_type_params(parsed.generics.as_ref());
    let set_attributes = build_container_attributes(&parsed.attributes);

    // For transparent, extract the inner type
    let inner_type = if is_transparent {
        match &parsed.kind {
            StructKind::TupleStruct { fields, .. } => {
                if fields.content.0.len() != 1 {
                    panic!("Transparent structs must have exactly one field");
                }
                fields.content.0[0].value.typ.tokens_to_string()
            }
            _ => panic!("Transparent structs must be tuple structs with a single field"),
        }
    } else {
        String::new() // Not used for non-transparent
    };

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
                        &bgp,
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
                        &bgp,
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
                let value = value.get::<{struct_name}{bgp}>();
                {tests}
                true
            }}
            "#,
            bgp = bgp.display_without_bounds(),
        );

        invariant_maybe = format!(
            r#"
            {invariant_fn}

            vtable.invariants = Some(invariants);
            "#
        );
    }

    // Add try_from_inner implementation for transparent types
    let try_from_inner_code = if is_transparent {
        format!(
            r#"
            // Define the try_from function for the value vtable
            unsafe fn try_from<'src, 'dst>(
                src_ptr: ::facet::PtrConst<'src>,
                src_shape: &'static ::facet::Shape,
                dst: ::facet::PtrUninit<'dst>
            ) -> Result<::facet::PtrMut<'dst>, ::facet::TryFromError> {{
                match <{inner_type} as ::facet::Facet>::SHAPE.vtable.try_from {{
                    Some(inner_try) => {{
                        unsafe {{
                            (inner_try)(src_ptr, src_shape, dst)
                        }}
                    }},
                    None => {{
                        if src_shape != <{inner_type} as ::facet::Facet>::SHAPE {{
                            return Err(::facet::TryFromError::UnsupportedSourceShape {{
                                src_shape,
                                expected: const {{ &[ &<{inner_type} as ::facet::Facet>::SHAPE ] }},
                            }});
                        }}

                        let inner: {inner_type} = unsafe {{ src_ptr.read() }};
                        Ok(unsafe {{ dst.put(inner) }})
                    }}
                }}
            }}

            vtable.try_from = Some(try_from);

            // Define the try_into_inner function for the value vtable
            unsafe fn try_into_inner<'src, 'dst>(
                src_ptr: ::facet::PtrConst<'src>,
                dst: ::facet::PtrUninit<'dst>
            ) -> Result<::facet::PtrMut<'dst>, ::facet::TryIntoInnerError> {{
                // Get the wrapper value we're converting from
                let wrapper = unsafe {{ src_ptr.get::<{struct_name}{bgp_without_bounds}>() }};
                // Extract inner value and put it in the destination
                Ok(unsafe {{ dst.put(wrapper.0.clone()) }})
            }}

            vtable.try_into_inner = Some(try_into_inner);

            // Define the try_borrow_inner function for the value vtable
            unsafe fn try_borrow_inner<'src>(
                src_ptr: ::facet::PtrConst<'src>
            ) -> Result<::facet::PtrConst<'src>, ::facet::TryBorrowInnerError> {{
                // Get the wrapper value
                let wrapper = unsafe {{ src_ptr.get::<{struct_name}{bgp_without_bounds}>() }};
                // Return a pointer to the inner value
                Ok(::facet::PtrConst::new(&wrapper.0 as *const _ as *const u8))
            }}

            vtable.try_borrow_inner = Some(try_borrow_inner);
            "#,
            bgp_without_bounds = bgp.display_without_bounds(),
        )
    } else {
        String::new()
    };

    // Generate the inner shape function for transparent types
    let inner_shape_fn = if is_transparent {
        format!(
            r#"
        // Function to return inner type's shape
        fn inner_shape() -> &'static ::facet::Shape {{
            <{inner_type} as ::facet::Facet>::SHAPE
        }}
            "#
        )
    } else {
        String::new()
    };

    // Generate the impl
    let output = format!(
        r#"
{static_decl}

#[automatically_derived]
unsafe impl{bgp_def} ::facet::Facet<'__facet> for {struct_name}{bgp_without_bounds} {where_clauses} {{
    const SHAPE: &'static ::facet::Shape = &const {{
        let fields: &'static [::facet::Field] = &const {{[{fields}]}};

        let vtable = &const {{
            let mut vtable = ::facet::value_vtable!(
                Self,
                |f, _opts| ::core::fmt::Write::write_str(f, "{struct_name}")
            );
            {invariant_maybe}
            {try_from_inner_code}
            vtable
        }};

{inner_shape_fn}

        ::facet::Shape::builder()
            .id(::facet::ConstTypeId::of::<Self>())
            .layout(::core::alloc::Layout::new::<Self>())
            {type_params}
            .vtable(vtable)
            .def(::facet::Def::Struct(::facet::StructDef::builder()
                .kind({kind})
                .fields(fields)
                .build()))
            {inner_setter}
            {maybe_container_doc}
            {set_attributes}
            .build()
    }};
}}
        "#,
        bgp_def = bgp.with_lifetime("__facet").display_with_bounds(),
        bgp_without_bounds = bgp.display_without_bounds(),
        inner_setter = if is_transparent {
            ".inner(inner_shape)"
        } else {
            ""
        },
    );

    // Uncomment to see generated code before lexin
    // panic!("output =\n{output}");

    output.into_token_stream()
}
