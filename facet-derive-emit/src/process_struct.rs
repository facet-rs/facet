use super::*;
use quote::{format_ident, quote};

/// Generates the `::facet::Field` definition `TokenStream` from a `PStructField`.
pub(crate) fn gen_field_from_pfield(
    field: &PStructField,
    struct_name: &Ident,
    bgp: &BoundedGenericParams,
    base_offset: Option<TokenStream>,
) -> TokenStream {
    let field_name_effective = &field.name.effective;
    let field_name_raw = &field.name.raw;
    let field_type = &field.ty; // TokenStream of the type

    let tts: facet_derive_parse::TokenStream = field_type.clone();
    let field_type_static = tts
        .to_token_iter()
        .parse::<Vec<LifetimeOrTt>>()
        .unwrap()
        .into_iter()
        .map(|lott| match lott {
            LifetimeOrTt::TokenTree(tt) => quote! { #tt },
            LifetimeOrTt::Lifetime(_) => quote! { 'static },
        })
        .collect::<TokenStream>();

    let bgp_without_bounds = bgp.display_without_bounds();

    // Determine field flags and other attributes from field.attrs
    let mut flags = quote! {};
    let mut flags_empty = true;

    let mut vtable_items: Vec<TokenStream> = vec![];
    let mut attribute_list: Vec<TokenStream> = vec![];
    let doc_lines: Vec<TokenStream> = field.attrs.doc.iter().map(|doc| quote!(#doc)).collect();
    let mut shape_of = quote! { shape_of };
    let mut asserts: Vec<TokenStream> = vec![];

    // Process attributes other than rename rules, which are handled by PName
    for attr in &field.attrs.facet {
        match attr {
            PFacetAttr::Sensitive => {
                if flags_empty {
                    flags_empty = false;
                    flags = quote! { ::facet::FieldFlags::SENSITIVE };
                } else {
                    flags = quote! { #flags.union(::facet::FieldFlags::SENSITIVE) };
                }
            }
            PFacetAttr::Default => {
                if flags_empty {
                    flags_empty = false;
                    flags = quote! { ::facet::FieldFlags::DEFAULT };
                } else {
                    flags = quote! { #flags.union(::facet::FieldFlags::DEFAULT) };
                }
                asserts.push(quote! {
                    ::facet::static_assertions::assert_impl_all!(#field_type_static: ::core::default::Default);
                })
            }
            PFacetAttr::DefaultEquals { expr } => {
                if flags_empty {
                    flags_empty = false;
                    flags = quote! { ::facet::FieldFlags::DEFAULT };
                } else {
                    flags = quote! { #flags.union(::facet::FieldFlags::DEFAULT) };
                }

                vtable_items.push(quote! {
                    .default_fn(|ptr| {
                        unsafe { ptr.put::<#field_type>(#expr) }
                    })
                });
            }
            PFacetAttr::Child => {
                if flags_empty {
                    flags_empty = false;
                    flags = quote! { ::facet::FieldFlags::CHILD };
                } else {
                    flags = quote! { #flags.union(::facet::FieldFlags::CHILD) };
                }
            }
            PFacetAttr::Flatten => {
                if flags_empty {
                    flags_empty = false;
                    flags = quote! { ::facet::FieldFlags::FLATTEN };
                } else {
                    flags = quote! { #flags.union(::facet::FieldFlags::FLATTEN) };
                }
            }
            PFacetAttr::Opaque => {
                shape_of = quote! { shape_of_opaque };
            }
            PFacetAttr::Arbitrary { content } => {
                attribute_list.push(quote! { ::facet::FieldAttribute::Arbitrary(#content) });
            }
            PFacetAttr::SkipSerializing => {
                if flags_empty {
                    flags_empty = false;
                    flags = quote! { ::facet::FieldFlags::SKIP_SERIALIZING };
                } else {
                    flags = quote! { #flags.union(::facet::FieldFlags::SKIP_SERIALIZING) };
                }
            }
            PFacetAttr::SkipSerializingIf { expr } => {
                let predicate = expr;
                let field_ty = field_type;
                vtable_items.push(quote! {
                    .skip_serializing_if(unsafe { ::core::mem::transmute((#predicate) as fn(&#field_ty) -> bool) })
                });
            }
            // These are handled by PName or are container-level, so ignore them for field attributes.
            PFacetAttr::RenameAll { .. } => {} // Explicitly ignore rename attributes here
            PFacetAttr::Transparent
            | PFacetAttr::Invariants { .. }
            | PFacetAttr::DenyUnknownFields => {}
        }
    }

    let maybe_attributes = if attribute_list.is_empty() {
        quote! {}
    } else {
        quote! { .attributes(&const { [#(#attribute_list),*] }) }
    };

    let maybe_field_doc = if doc_lines.is_empty() {
        quote! {}
    } else {
        quote! { .doc(&[#(#doc_lines),*]) }
    };

    let maybe_vtable = if vtable_items.is_empty() {
        quote! {}
    } else {
        quote! {
            .vtable(&const {
                ::facet::FieldVTable::builder()
                    #(#vtable_items)*
                    .build()
            })
        }
    };

    let maybe_flags = if flags_empty {
        quote! {}
    } else {
        quote! { .flags(#flags) }
    };

    // Calculate the final offset, incorporating the base_offset if present
    let final_offset = match base_offset {
        Some(base) => {
            quote! { #base + ::core::mem::offset_of!(#struct_name #bgp_without_bounds, #field_name_raw) }
        }
        None => {
            quote! { ::core::mem::offset_of!(#struct_name #bgp_without_bounds, #field_name_raw) }
        }
    };

    quote! {
        {
            #(#asserts)*;
            ::facet::Field::builder()
                // Use the effective name (after rename rules) for metadata
                .name(#field_name_effective)
                // Use the raw field name/index TokenStream for shape_of and offset_of
                .shape(::facet::#shape_of(&|s: &#struct_name #bgp_without_bounds| &s.#field_name_raw))
                .offset(#final_offset)
                #maybe_flags
                #maybe_attributes
                #maybe_field_doc
                #maybe_vtable
                .build()
        }
    }
}

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
    let ps = PStruct::parse(&parsed); // Use the parsed representation

    let struct_name_ident = format_ident!("{}", ps.container.name);
    let struct_name = &ps.container.name;
    let struct_name_str = struct_name.to_string();

    // Use PStruct for kind and fields
    let (kind, fields_vec) = match &ps.kind {
        PStructKind::Struct { fields } => {
            let kind = quote!(::facet::StructKind::Struct);
            let fields_vec = fields
                .iter()
                .map(|field| gen_field_from_pfield(field, struct_name, &ps.container.bgp, None))
                .collect::<Vec<_>>();
            (kind, fields_vec)
        }
        PStructKind::TupleStruct { fields } => {
            let kind = quote!(::facet::StructKind::TupleStruct);
            let fields_vec = fields
                .iter()
                .map(|field| gen_field_from_pfield(field, struct_name, &ps.container.bgp, None))
                .collect::<Vec<_>>();
            (kind, fields_vec)
        }
        PStructKind::UnitStruct => {
            let kind = quote!(::facet::StructKind::Unit);
            (kind, vec![])
        }
    };

    // Still need original AST for where clauses and type params for build_ helpers
    let where_clauses_ast = match &parsed.kind {
        StructKind::Struct { clauses, .. } => clauses.as_ref(),
        StructKind::TupleStruct { clauses, .. } => clauses.as_ref(),
        StructKind::UnitStruct { clauses, .. } => clauses.as_ref(),
    };
    let where_clauses = build_where_clauses(where_clauses_ast, parsed.generics.as_ref());
    let type_params = build_type_params(parsed.generics.as_ref());

    // Static decl using PStruct BGP
    let static_decl = if ps.container.bgp.params.is_empty() {
        generate_static_decl(struct_name)
    } else {
        TokenStream::new()
    };

    // Doc comments from PStruct
    let maybe_container_doc = if ps.container.attrs.doc.is_empty() {
        quote! {}
    } else {
        let doc_lines = ps.container.attrs.doc.iter().map(|s| quote!(#s));
        quote! { .doc(&[#(#doc_lines),*]) }
    };

    // Container attributes from PStruct
    let container_attributes_tokens = {
        let mut items = Vec::new();
        for attr in &ps.container.attrs.facet {
            match attr {
                PFacetAttr::DenyUnknownFields => {
                    items.push(quote! { ::facet::ShapeAttribute::DenyUnknownFields });
                }
                PFacetAttr::Default | PFacetAttr::DefaultEquals { .. } => {
                    // Corresponds to `#[facet(default)]` on container
                    items.push(quote! { ::facet::ShapeAttribute::Default });
                }
                PFacetAttr::Transparent => {
                    items.push(quote! { ::facet::ShapeAttribute::Transparent });
                }
                PFacetAttr::RenameAll { .. } => {}
                PFacetAttr::Arbitrary { content } => {
                    items.push(quote! { ::facet::ShapeAttribute::Arbitrary(#content) });
                }
                // Others not applicable at container level or handled elsewhere
                PFacetAttr::Sensitive
                | PFacetAttr::Opaque
                | PFacetAttr::Invariants { .. }
                | PFacetAttr::SkipSerializing
                | PFacetAttr::SkipSerializingIf { .. }
                | PFacetAttr::Flatten
                | PFacetAttr::Child => {}
            }
        }
        if items.is_empty() {
            quote! {}
        } else {
            quote! { .attributes(&[#(#items),*]) }
        }
    };

    // Invariants from PStruct
    let invariant_maybe = {
        let mut invariant_fns = Vec::new();
        for attr in &ps.container.attrs.facet {
            if let PFacetAttr::Invariants { expr } = attr {
                invariant_fns.push(expr);
            }
        }

        if !invariant_fns.is_empty() {
            let tests = invariant_fns.iter().map(|expr| {
                quote! {
                    if !#expr(value) {
                        return false;
                    }
                }
            });

            let bgp_display = ps.container.bgp.display_without_bounds(); // Use the BGP from PStruct
            quote! {
                unsafe fn invariants<'mem>(value: ::facet::PtrConst<'mem>) -> bool {
                    let value = value.get::<#struct_name_ident #bgp_display>();
                    #(#tests)*
                    true
                }

                vtable.invariants = Some(invariants);
            }
        } else {
            quote! {}
        }
    };

    // Transparent logic using PStruct
    let inner_field = if ps.container.attrs.is_transparent() {
        match &ps.kind {
            PStructKind::TupleStruct { fields } => {
                if fields.len() > 1 {
                    return quote! {
                        compile_error!("Transparent structs must be tuple structs with zero or one field");
                    };
                }
                fields.first().cloned() // Use first field if it exists, None otherwise (ZST case)
            }
            _ => {
                return quote! {
                    compile_error!("Transparent structs must be tuple structs");
                };
            }
        }
    } else {
        None
    };

    // Add try_from_inner implementation for transparent types
    let try_from_inner_code = if ps.container.attrs.is_transparent() {
        if let Some(inner_field) = &inner_field {
            // Transparent struct with one field
            let inner_field_type = &inner_field.ty;
            let bgp_without_bounds = ps.container.bgp.display_without_bounds();

            quote! {
                // Define the try_from function for the value vtable
                unsafe fn try_from<'shape, 'src, 'dst>(
                    src_ptr: ::facet::PtrConst<'src>,
                    src_shape: &'shape ::facet::Shape<'shape>,
                    dst: ::facet::PtrUninit<'dst>
                ) -> Result<::facet::PtrMut<'dst>, ::facet::TryFromError<'shape>> {
                    match <#inner_field_type as ::facet::Facet>::SHAPE.vtable.try_from {
                        Some(inner_try) => unsafe { (inner_try)(src_ptr, src_shape, dst) },
                        None => {
                            // Otherwise, check if source shape is exactly the inner shape
                            if src_shape != <#inner_field_type as ::facet::Facet>::SHAPE {
                                return Err(::facet::TryFromError::UnsupportedSourceShape {
                                    src_shape,
                                    expected: const { &[ &<#inner_field_type as ::facet::Facet>::SHAPE ] },
                                });
                            }
                            // Read the inner value and construct the wrapper.
                            let inner: #inner_field_type = unsafe { src_ptr.read() };
                            Ok(unsafe { dst.put(inner) }) // Construct wrapper
                        }
                    }
                }
                vtable.try_from = Some(try_from);

                // Define the try_into_inner function for the value vtable
                unsafe fn try_into_inner<'src, 'dst>(
                    src_ptr: ::facet::PtrMut<'src>,
                    dst: ::facet::PtrUninit<'dst>
                ) -> Result<::facet::PtrMut<'dst>, ::facet::TryIntoInnerError> {
                    let wrapper = unsafe { src_ptr.get::<#struct_name_ident #bgp_without_bounds>() };
                    Ok(unsafe { dst.put(wrapper.0.clone()) }) // Assume tuple struct field 0
                }
                vtable.try_into_inner = Some(try_into_inner);

                // Define the try_borrow_inner function for the value vtable
                unsafe fn try_borrow_inner<'src>(
                    src_ptr: ::facet::PtrConst<'src>
                ) -> Result<::facet::PtrConst<'src>, ::facet::TryBorrowInnerError> {
                    // Get the wrapper value
                    let wrapper = unsafe { src_ptr.get::<#struct_name_ident #bgp_without_bounds>() };
                    // Return a pointer to the inner value (field 0)
                    Ok(::facet::PtrConst::new(&wrapper.0 as *const _ as *const u8))
                }
                vtable.try_borrow_inner = Some(try_borrow_inner);
            }
        } else {
            // Transparent ZST struct (like struct Unit;)
            quote! {
                // Define the try_from function for the value vtable (ZST case)
                unsafe fn try_from<'src, 'dst>(
                     src_ptr: ::facet::PtrConst<'src>,
                     src_shape: &'static ::facet::Shape,
                     dst: ::facet::PtrUninit<'dst>
                ) -> Result<::facet::PtrMut<'dst>, ::facet::TryFromError> {
                     if src_shape.layout.size() == 0 {
                         Ok(unsafe { dst.put(#struct_name_ident) }) // Construct ZST
                     } else {
                         Err(::facet::TryFromError::UnsupportedSourceShape {
                             src_shape,
                             expected: const { &[ <() as ::facet::Facet>::SHAPE ] }, // Expect unit-like shape
                         })
                     }
                }
                 vtable.try_from = Some(try_from);

                // ZSTs cannot be meaningfully borrowed or converted *into* an inner value
                // try_into_inner and try_borrow_inner remain None
            }
        }
    } else {
        quote! {} // Not transparent
    };

    // Generate the inner shape function for transparent types
    let inner_shape_fn = if ps.container.attrs.is_transparent() {
        if let Some(inner_field) = &inner_field {
            let ty = &inner_field.ty;
            quote! {
                // Function to return inner type's shape
                fn inner_shape() -> &'static ::facet::Shape<'static> {
                    <#ty as ::facet::Facet>::SHAPE
                }
            }
        } else {
            // Transparent ZST case
            quote! {
                fn inner_shape() -> &'static ::facet::Shape<'static> {
                    <() as ::facet::Facet>::SHAPE // Inner shape is unit
                }
            }
        }
    } else {
        quote! {}
    };

    let inner_setter = if ps.container.attrs.is_transparent() {
        quote! { .inner(inner_shape) }
    } else {
        quote! {}
    };

    // Generics from PStruct
    let facet_bgp = ps
        .container
        .bgp
        .with_lifetime(LifetimeName(format_ident!("__facet")));
    let bgp_def = facet_bgp.display_with_bounds();
    let bgp_without_bounds = ps.container.bgp.display_without_bounds();

    // Final quote block using refactored parts
    let result = quote! {
        #static_decl

        #[automatically_derived]
        unsafe impl #bgp_def ::facet::Facet<'__facet> for #struct_name_ident #bgp_without_bounds #where_clauses {
            const VTABLE: &'static ::facet::ValueVTable = &const {
                let mut vtable = ::facet::value_vtable!(
                    Self,
                    |f, _opts| ::core::fmt::Write::write_str(f, #struct_name_str)
                );
                #invariant_maybe
                #try_from_inner_code // Use the generated code for transparent types
                vtable
            };

            const SHAPE: &'static ::facet::Shape<'static> = &const {
                let fields: &'static [::facet::Field] = &const {[#(#fields_vec),*]};

                #inner_shape_fn // Include inner_shape function if needed

                ::facet::Shape::builder_for_sized::<Self>()
                    #type_params // Still from parsed.generics
                    .ty(::facet::Type::User(::facet::UserType::Struct(::facet::StructType::builder()
                        .repr(::facet::Repr::c())
                        .kind(#kind)
                        .fields(fields)
                        .build()
                    )))
                    #inner_setter // Use transparency flag from PStruct
                    #maybe_container_doc // From ps.container.attrs.doc
                    #container_attributes_tokens // From ps.container.attrs.facet
                    .build()
            };
        }
    };

    result
}
