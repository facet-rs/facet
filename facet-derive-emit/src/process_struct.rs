use super::normalize_ident_str;
use super::*;
use quote::quote;

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
    let ps = PStruct::parse(&parsed);

    let struct_name = &parsed.name;
    let struct_name_str = struct_name.to_string();

    let kind;
    let where_clauses;
    let type_params = build_type_params(parsed.generics.as_ref());
    let container_attributes = build_container_attributes(&parsed.attributes);

    // For transparent, extract the inner type
    let inner_field = if ps.container.attrs.is_transparent() {
        match ps.kind {
            PStructKind::TupleStruct { fields } => {
                if fields.len() != 1 {
                    // well, apparently you can have zero-sized fields in a transparent struct 🤷
                }
                Some(fields[0].clone())
            }
            _ => {
                return quote! {
                    compile_error!("Transparent structs must be tuple structs with a single field");
                };
            }
        }
    } else {
        None
    };

    let fields_vec = match &parsed.kind {
        StructKind::Struct { clauses, fields } => {
            kind = quote!(::facet::StructKind::Struct);
            where_clauses = clauses.as_ref();
            fields
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
                        struct_name: &struct_name_str,
                        bgp: &ps.container.bgp,
                        attrs: &field.value.attributes,
                        base_field_offset: None,
                        rename_rule: container_attributes.rename_rule,
                    })
                })
                .collect::<Vec<_>>()
        }
        StructKind::TupleStruct {
            fields,
            clauses,
            semi: _,
        } => {
            kind = quote!(::facet::StructKind::TupleStruct);
            where_clauses = clauses.as_ref();
            fields
                .content
                .0
                .iter()
                .enumerate()
                .map(|(index, field)| {
                    let field_name = format!("{index}");
                    gen_struct_field(FieldInfo {
                        raw_field_name: &field_name,
                        normalized_field_name: &field_name,
                        field_type: &field.value.typ.tokens_to_string(),
                        struct_name: &struct_name_str,
                        bgp: &ps.container.bgp,
                        attrs: &field.value.attributes,
                        base_field_offset: None,
                        rename_rule: container_attributes.rename_rule,
                    })
                })
                .collect::<Vec<_>>()
        }
        StructKind::UnitStruct { clauses, semi: _ } => {
            kind = quote!(::facet::StructKind::Unit);
            where_clauses = clauses.as_ref();
            vec![]
        }
    };

    let where_clauses = build_where_clauses(where_clauses, parsed.generics.as_ref());
    let static_decl = if parsed.generics.is_none() {
        generate_static_decl(&struct_name_str)
    } else {
        TokenStream::new()
    };
    let maybe_container_doc = build_maybe_doc(&parsed.attributes);

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

    let invariant_maybe = if !invariant_attrs.is_empty() {
        let tests = invariant_attrs.iter().map(|invariant| {
            // FIXME: just don't take a string
            let invariant_name: TokenStream = invariant
                .value
                .as_str()
                .parse()
                .expect("Invalid invariant name");
            quote! {
                if !value.#invariant_name() {
                    return false;
                }
            }
        });

        let bgp = ps.container.bgp.display_without_bounds();
        quote! {
            unsafe fn invariants<'mem>(value: ::facet::PtrConst<'mem>) -> bool {
                let value = value.get::<#struct_name #bgp>();
                #(#tests)*
                true
            }

            vtable.invariants = Some(invariants);
        }
    } else {
        quote! {}
    };

    // Add try_from_inner implementation for transparent types
    let try_from_inner_code = if let Some(inner_field) = &inner_field {
        let inner_field_type = &inner_field.ty;
        let bgp_without_bounds = ps.container.bgp.display_without_bounds();

        quote! {
            // Define the try_from function for the value vtable
            unsafe fn try_from<'src, 'dst>(
                src_ptr: ::facet::PtrConst<'src>,
                src_shape: &'static ::facet::Shape,
                dst: ::facet::PtrUninit<'dst>
            ) -> Result<::facet::PtrMut<'dst>, ::facet::TryFromError> {
                match <#inner_field_type as ::facet::Facet>::SHAPE.vtable.try_from {
                    Some(inner_try) => unsafe { (inner_try)(src_ptr, src_shape, dst) },
                    None => {
                        if src_shape != <#inner_field_type as ::facet::Facet>::SHAPE {
                            return Err(::facet::TryFromError::UnsupportedSourceShape {
                                src_shape,
                                expected: const { &[ &<#inner_field_type as ::facet::Facet>::SHAPE ] },
                            });
                        }

                        let inner: #inner_field_type = unsafe { src_ptr.read() };
                        Ok(unsafe { dst.put(inner) })
                    }
                }
            }

            vtable.try_from = Some(try_from);

            // Define the try_into_inner function for the value vtable
            unsafe fn try_into_inner<'src, 'dst>(
                src_ptr: ::facet::PtrConst<'src>,
                dst: ::facet::PtrUninit<'dst>
            ) -> Result<::facet::PtrMut<'dst>, ::facet::TryIntoInnerError> {
                let wrapper = unsafe { src_ptr.get::<#struct_name #bgp_without_bounds>() };
                Ok(unsafe { dst.put(wrapper.0.clone()) })
            }

            vtable.try_into_inner = Some(try_into_inner);

            // Define the try_borrow_inner function for the value vtable
            unsafe fn try_borrow_inner<'src>(
                src_ptr: ::facet::PtrConst<'src>
            ) -> Result<::facet::PtrConst<'src>, ::facet::TryBorrowInnerError> {
                // Get the wrapper value
                let wrapper = unsafe { src_ptr.get::<#struct_name #bgp_without_bounds>() };
                // Return a pointer to the inner value
                Ok(::facet::PtrConst::new(&wrapper.0 as *const _ as *const u8))
            }

            vtable.try_borrow_inner = Some(try_borrow_inner);
        }
    } else {
        quote! {}
    };

    // Generate the inner shape function for transparent types
    let inner_shape_fn = if let Some(inner_field) = &inner_field {
        let ty = &inner_field.ty;
        quote! {
            // Function to return inner type's shape
            fn inner_shape() -> &'static ::facet::Shape {
                <#ty as ::facet::Facet>::SHAPE
            }
        }
    } else {
        quote! {}
    };

    let inner_setter = if inner_field.is_some() {
        quote! { .inner(inner_shape) }
    } else {
        quote! {}
    };

    let bgp_def = ps.container.bgp.with_lifetime("__facet");
    let bgp_def = bgp_def.display_with_bounds();
    let bgp_without_bounds = ps.container.bgp.display_without_bounds();

    let result = quote! {
        #static_decl

        #[automatically_derived]
        unsafe impl #bgp_def ::facet::Facet<'__facet> for #struct_name #bgp_without_bounds #where_clauses {
            const SHAPE: &'static ::facet::Shape = &const {
                let fields: &'static [::facet::Field] = &const {[#(#fields_vec),*]};

                let vtable = &const {
                    let mut vtable = ::facet::value_vtable!(
                        Self,
                        |f, _opts| ::core::fmt::Write::write_str(f, #struct_name_str)
                    );
                    #invariant_maybe
                    #try_from_inner_code
                    vtable
                };

                #inner_shape_fn

                ::facet::Shape::builder()
                    .id(::facet::ConstTypeId::of::<Self>())
                    .layout(::core::alloc::Layout::new::<Self>())
                    #type_params
                    .vtable(vtable)
                    .def(::facet::Def::Struct(::facet::StructDef::builder()
                        .kind(#kind)
                        .fields(fields)
                        .build()))
                    #inner_setter
                    #maybe_container_doc
                    #container_attributes
                    .build()
            };
        }
    };

    result
}
