use quote::{format_ident, quote, quote_spanned};

use super::*;

/// Generates the `::facet::Field` definition `TokenStream` from a `PStructField`.
pub(crate) fn gen_field_from_pfield(
    field: &PStructField,
    struct_name: &Ident,
    bgp: &BoundedGenericParams,
    base_offset: Option<TokenStream>,
    facet_crate: &TokenStream,
) -> TokenStream {
    let field_name_effective = &field.name.effective;
    let field_name_raw = &field.name.raw;
    let field_type = &field.ty;

    let bgp_without_bounds = bgp.display_without_bounds();

    let doc_lines: Vec<String> = field
        .attrs
        .doc
        .iter()
        .map(|doc| doc.as_str().replace("\\\"", "\""))
        .collect();

    // Check for opaque attribute to determine shape_of variant
    // (Required at compile time - shape_of requires Facet, shape_of_opaque wraps in Opaque)
    let shape_of = if field.attrs.has_builtin("opaque") {
        quote! { shape_of_opaque }
    } else {
        quote! { shape_of }
    };

    // All attributes go through grammar dispatch
    // Note: deserialize_with and serialize_with have been REMOVED from the grammar.
    // Use #[facet(proxy = Type)] for custom serialization instead.
    let mut attribute_list: Vec<TokenStream> = field
        .attrs
        .facet
        .iter()
        .map(|attr| {
            let ext_attr = emit_attr_for_field(attr, field_name_raw, field_type, facet_crate);
            quote! { #ext_attr }
        })
        .collect();

    // Generate proxy conversion function pointers when proxy attribute is present
    if let Some(attr) = field
        .attrs
        .facet
        .iter()
        .find(|a| a.is_builtin() && a.key_str() == "proxy")
    {
        let proxy_type = &attr.args;

        // Generate __proxy_in: converts proxy -> field type via TryFrom
        attribute_list.push(quote! {
            #facet_crate::ExtensionAttr {
                ns: ::core::option::Option::None,
                key: "__proxy_in",
                data: &const {
                    extern crate alloc as __alloc;
                    unsafe fn __proxy_convert_in<'mem>(
                        proxy_ptr: #facet_crate::PtrConst<'mem>,
                        field_ptr: #facet_crate::PtrUninit<'mem>,
                    ) -> ::core::result::Result<#facet_crate::PtrMut<'mem>, __alloc::string::String> {
                        let proxy: #proxy_type = proxy_ptr.read();
                        match <#field_type as ::core::convert::TryFrom<#proxy_type>>::try_from(proxy) {
                            ::core::result::Result::Ok(value) => ::core::result::Result::Ok(field_ptr.put(value)),
                            ::core::result::Result::Err(e) => ::core::result::Result::Err(__alloc::string::ToString::to_string(&e)),
                        }
                    }
                    __proxy_convert_in as #facet_crate::ProxyConvertInFn
                } as *const #facet_crate::ProxyConvertInFn as *const (),
                shape: <() as #facet_crate::Facet>::SHAPE,
            }
        });

        // Generate __proxy_out: converts &field type -> proxy via TryFrom
        attribute_list.push(quote! {
            #facet_crate::ExtensionAttr {
                ns: ::core::option::Option::None,
                key: "__proxy_out",
                data: &const {
                    extern crate alloc as __alloc;
                    unsafe fn __proxy_convert_out<'mem>(
                        field_ptr: #facet_crate::PtrConst<'mem>,
                        proxy_ptr: #facet_crate::PtrUninit<'mem>,
                    ) -> ::core::result::Result<#facet_crate::PtrMut<'mem>, __alloc::string::String> {
                        let field_ref: &#field_type = field_ptr.get();
                        match <#proxy_type as ::core::convert::TryFrom<&#field_type>>::try_from(field_ref) {
                            ::core::result::Result::Ok(proxy) => ::core::result::Result::Ok(proxy_ptr.put(proxy)),
                            ::core::result::Result::Err(e) => ::core::result::Result::Err(__alloc::string::ToString::to_string(&e)),
                        }
                    }
                    __proxy_convert_out as #facet_crate::ProxyConvertOutFn
                } as *const #facet_crate::ProxyConvertOutFn as *const (),
                shape: <() as #facet_crate::Facet>::SHAPE,
            }
        });
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
            #facet_crate::Field::builder()
                // Use the effective name (after rename rules) for metadata
                .name(#field_name_effective)
                // Use the raw field name/index TokenStream for shape_of and offset_of
                .shape(|| #facet_crate::#shape_of(&|s: &#struct_name #bgp_without_bounds| &s.#field_name_raw))
                .offset(#final_offset)
                #maybe_attributes
                #maybe_field_doc
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

    // Emit any collected errors as compile_error! with proper spans
    if !ps.container.attrs.errors.is_empty() {
        let errors = ps.container.attrs.errors.iter().map(|e| {
            let msg = &e.message;
            let span = e.span;
            quote_spanned! { span => compile_error!(#msg); }
        });
        return quote! { #(#errors)* };
    }

    let struct_name_ident = format_ident!("{}", ps.container.name);
    let struct_name = &ps.container.name;
    let struct_name_str = struct_name.to_string();

    let opaque = ps.container.attrs.has_builtin("opaque");

    // Get the facet crate path (custom or default ::facet)
    let facet_crate = ps.container.attrs.facet_crate();

    let type_name_fn =
        generate_type_name_fn(struct_name, parsed.generics.as_ref(), opaque, &facet_crate);

    // TODO: I assume the `PrimitiveRepr` is only relevant for enums, and does not need to be preserved?
    let repr = match &ps.container.attrs.repr {
        PRepr::Transparent => quote! { #facet_crate::Repr::transparent() },
        PRepr::Rust(_) => quote! { #facet_crate::Repr::default() },
        PRepr::C(_) => quote! { #facet_crate::Repr::c() },
        PRepr::RustcWillCatch => {
            // rustc will emit an error for the invalid repr.
            // Return empty TokenStream so we don't add misleading errors.
            return quote! {};
        }
    };

    // Use PStruct for kind and fields
    let (kind, fields_vec) = match &ps.kind {
        PStructKind::Struct { fields } => {
            let kind = quote!(#facet_crate::StructKind::Struct);
            let fields_vec = fields
                .iter()
                .map(|field| {
                    gen_field_from_pfield(field, struct_name, &ps.container.bgp, None, &facet_crate)
                })
                .collect::<Vec<_>>();
            (kind, fields_vec)
        }
        PStructKind::TupleStruct { fields } => {
            let kind = quote!(#facet_crate::StructKind::TupleStruct);
            let fields_vec = fields
                .iter()
                .map(|field| {
                    gen_field_from_pfield(field, struct_name, &ps.container.bgp, None, &facet_crate)
                })
                .collect::<Vec<_>>();
            (kind, fields_vec)
        }
        PStructKind::UnitStruct => {
            let kind = quote!(#facet_crate::StructKind::Unit);
            (kind, vec![])
        }
    };

    // Still need original AST for where clauses and type params for build_ helpers
    let where_clauses_ast = match &parsed.kind {
        StructKind::Struct { clauses, .. } => clauses.as_ref(),
        StructKind::TupleStruct { clauses, .. } => clauses.as_ref(),
        StructKind::UnitStruct { clauses, .. } => clauses.as_ref(),
    };
    let where_clauses = build_where_clauses(
        where_clauses_ast,
        parsed.generics.as_ref(),
        opaque,
        &facet_crate,
    );
    let type_params = build_type_params(parsed.generics.as_ref(), opaque, &facet_crate);

    // Static decl using PStruct BGP
    let static_decl = if ps.container.bgp.params.is_empty() {
        generate_static_decl(struct_name, &facet_crate)
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

    // Container attributes - most go through grammar dispatch
    // Filter out `invariants` and `crate` since they're handled specially
    let container_attributes_tokens = {
        let items: Vec<TokenStream> = ps
            .container
            .attrs
            .facet
            .iter()
            .filter(|attr| {
                // invariants is handled specially - it populates vtable.invariants
                // crate is handled specially - it sets the facet crate path
                !(attr.is_builtin()
                    && (attr.key_str() == "invariants" || attr.key_str() == "crate"))
            })
            .map(|attr| {
                let ext_attr = emit_attr(attr, &facet_crate);
                quote! { #ext_attr }
            })
            .collect();

        if items.is_empty() {
            quote! {}
        } else {
            quote! { .attributes(&const { [#(#items),*] }) }
        }
    };

    // Type tag from PStruct
    let type_tag_maybe = {
        if let Some(type_tag) = ps.container.attrs.get_builtin_args("type_tag") {
            quote! { .type_tag(#type_tag) }
        } else {
            quote! {}
        }
    };

    // Invariants from PStruct - extract invariant function expressions
    let invariant_maybe = {
        let invariant_exprs: Vec<&TokenStream> = ps
            .container
            .attrs
            .facet
            .iter()
            .filter(|attr| attr.is_builtin() && attr.key_str() == "invariants")
            .map(|attr| &attr.args)
            .collect();

        if !invariant_exprs.is_empty() {
            let tests = invariant_exprs.iter().map(|expr| {
                quote! {
                    if !#expr(value) {
                        return false;
                    }
                }
            });

            let bgp_display = ps.container.bgp.display_without_bounds();
            quote! {
                unsafe fn invariants<'mem>(value: #facet_crate::PtrConst<'mem>) -> bool {
                    let value = value.get::<#struct_name_ident #bgp_display>();
                    #(#tests)*
                    true
                }

                {
                    vtable.invariants = Some(invariants);
                }
            }
        } else {
            quote! {}
        }
    };

    // Transparent logic using PStruct
    let inner_field = if ps.container.attrs.has_builtin("transparent") {
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
    let try_from_inner_code = if ps.container.attrs.has_builtin("transparent") {
        if let Some(inner_field) = &inner_field {
            if !inner_field.attrs.has_builtin("opaque") {
                // Transparent struct with one field
                let inner_field_type = &inner_field.ty;
                let bgp_without_bounds = ps.container.bgp.display_without_bounds();

                quote! {
                    // Define the try_from function for the value vtable
                    unsafe fn try_from<'src, 'dst>(
                        src_ptr: #facet_crate::PtrConst<'src>,
                        src_shape: &'static #facet_crate::Shape,
                        dst: #facet_crate::PtrUninit<'dst>
                    ) -> Result<#facet_crate::PtrMut<'dst>, #facet_crate::TryFromError> {
                        // Try the inner type's try_from function if it exists
                        let inner_result = match <#inner_field_type as #facet_crate::Facet>::SHAPE.vtable.try_from {
                            Some(inner_try) => unsafe { (inner_try)(src_ptr, src_shape, dst) },
                            None => Err(#facet_crate::TryFromError::UnsupportedSourceShape {
                                src_shape,
                                expected: const { &[ &<#inner_field_type as #facet_crate::Facet>::SHAPE ] },
                            })
                        };

                        match inner_result {
                            Ok(result) => Ok(result),
                            Err(_) => {
                                // If inner_try failed, check if source shape is exactly the inner shape
                                if src_shape != <#inner_field_type as #facet_crate::Facet>::SHAPE {
                                    return Err(#facet_crate::TryFromError::UnsupportedSourceShape {
                                        src_shape,
                                        expected: const { &[ &<#inner_field_type as #facet_crate::Facet>::SHAPE ] },
                                    });
                                }
                                // Read the inner value and construct the wrapper.
                                let inner: #inner_field_type = unsafe { src_ptr.read() };
                                Ok(unsafe { dst.put(inner) }) // Construct wrapper
                            }
                        }
                    }

                    // Define the try_into_inner function for the value vtable
                    unsafe fn try_into_inner<'src, 'dst>(
                        src_ptr: #facet_crate::PtrMut<'src>,
                        dst: #facet_crate::PtrUninit<'dst>
                    ) -> Result<#facet_crate::PtrMut<'dst>, #facet_crate::TryIntoInnerError> {
                        let wrapper = unsafe { src_ptr.get::<#struct_name_ident #bgp_without_bounds>() };
                        Ok(unsafe { dst.put(wrapper.0.clone()) }) // Assume tuple struct field 0
                    }

                    // Define the try_borrow_inner function for the value vtable
                    unsafe fn try_borrow_inner<'src>(
                        src_ptr: #facet_crate::PtrConst<'src>
                    ) -> Result<#facet_crate::PtrConst<'src>, #facet_crate::TryBorrowInnerError> {
                        let wrapper = unsafe { src_ptr.get::<#struct_name_ident #bgp_without_bounds>() };
                        // Return a pointer to the inner field (field 0 for tuple struct)
                        Ok(#facet_crate::PtrConst::new(::core::ptr::NonNull::from(&wrapper.0)))
                    }

                    {
                        vtable.try_from = Some(try_from);
                        vtable.try_into_inner = Some(try_into_inner);
                        vtable.try_borrow_inner = Some(try_borrow_inner);
                    }
                }
            } else {
                quote! {} // No try_from can be done for opaque
            }
        } else {
            // Transparent ZST struct (like struct Unit;)
            quote! {
                // Define the try_from function for the value vtable (ZST case)
                unsafe fn try_from<'src, 'dst>(
                    src_ptr: #facet_crate::PtrConst<'src>,
                    src_shape: &'static #facet_crate::Shape,
                    dst: #facet_crate::PtrUninit<'dst>
                ) -> Result<#facet_crate::PtrMut<'dst>, #facet_crate::TryFromError> {
                    if src_shape.layout.size() == 0 {
                         Ok(unsafe { dst.put(#struct_name_ident) }) // Construct ZST
                    } else {
                        Err(#facet_crate::TryFromError::UnsupportedSourceShape {
                            src_shape,
                            expected: const { &[ <() as #facet_crate::Facet>::SHAPE ] }, // Expect unit-like shape
                        })
                    }
                }

                {
                    vtable.try_from = Some(try_from);
                }

                // ZSTs cannot be meaningfully borrowed or converted *into* an inner value
                // try_into_inner and try_borrow_inner remain None
            }
        }
    } else {
        quote! {} // Not transparent
    };

    // Generate the inner shape function for transparent types
    let inner_setter = if ps.container.attrs.has_builtin("transparent") {
        let inner_shape_val = if let Some(inner_field) = &inner_field {
            let ty = &inner_field.ty;
            if inner_field.attrs.has_builtin("opaque") {
                quote! { <#facet_crate::Opaque<#ty> as #facet_crate::Facet>::SHAPE }
            } else {
                quote! { <#ty as #facet_crate::Facet>::SHAPE }
            }
        } else {
            // Transparent ZST case
            quote! { <() as #facet_crate::Facet>::SHAPE }
        };
        quote! { .inner(#inner_shape_val) }
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

    let (ty, fields) = if opaque {
        (
            quote! {
                .ty(#facet_crate::Type::User(#facet_crate::UserType::Opaque))
            },
            quote! {},
        )
    } else {
        (
            quote! {
                .ty(#facet_crate::Type::User(#facet_crate::UserType::Struct(#facet_crate::StructType::builder()
                    .repr(#repr)
                    .kind(#kind)
                    .fields(fields)
                    .build()
                )))
            },
            quote! {
                let fields: &'static [#facet_crate::Field] = &const {[#(#fields_vec),*]};
            },
        )
    };

    // Generate code to suppress dead_code warnings on structs constructed via reflection.
    // When structs are constructed via reflection (e.g., facet_args::from_std_args()),
    // the compiler doesn't see them being used and warns about dead code.
    // This function ensures the struct type is "used" from the compiler's perspective.
    // See: https://github.com/facet-rs/facet/issues/996
    let dead_code_suppression = quote! {
        const _: () = {
            #[allow(dead_code, clippy::multiple_bound_locations)]
            fn __facet_use_struct #bgp_def (__v: &#struct_name_ident #bgp_without_bounds) #where_clauses {
                let _ = __v;
            }
        };
    };

    // Final quote block using refactored parts
    let result = quote! {
        #static_decl

        #dead_code_suppression

        #[automatically_derived]
        unsafe impl #bgp_def #facet_crate::Facet<'__facet> for #struct_name_ident #bgp_without_bounds #where_clauses {
            const SHAPE: &'static #facet_crate::Shape = &const {
                #fields

                #facet_crate::Shape::builder_for_sized::<Self>()
                    .vtable({
                        let mut vtable = #facet_crate::value_vtable!(Self, #type_name_fn);
                        #invariant_maybe
                        #try_from_inner_code // Use the generated code for transparent types
                        vtable
                    })
                    .type_identifier(#struct_name_str)
                    #type_params // Still from parsed.generics
                    #ty
                    #inner_setter // Use transparency flag from PStruct
                    #maybe_container_doc // From ps.container.attrs.doc
                    #container_attributes_tokens // From ps.container.attrs.facet
                    #type_tag_maybe
                    .build()
            };
        }
    };

    result
}
