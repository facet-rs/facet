use facet_macros_parse::function::*;
use facet_macros_parse::{FunctionSignature, Ident, Span, TokenStream, extract_type_params};
use quote::quote;

/// Entry point for the facet_fn attribute macro
pub fn facet_fn(_attr: TokenStream, item: TokenStream) -> TokenStream {
    // Convert to proc_macro2 for parsing (quote requires it)
    let parsed = parse_function_signature(item);
    let result = generate_function_shape(parsed);

    // Convert back
    result.to_string().parse().unwrap()
}

/// Entry point for the fn_shape procedural macro  
pub fn fn_shape(input: TokenStream) -> TokenStream {
    let parsed = parse_fn_shape_input(input);
    let result = generate_fn_shape_call(parsed);

    result.to_string().parse().unwrap()
}

pub fn generate_function_shape(parsed: FunctionSignature) -> TokenStream {
    let fn_name = parsed.name;
    let generics = parsed.generics;
    let params = parsed.parameters;
    let return_type = parsed.return_type;
    let body = parsed.body;

    let hidden_mod = Ident::new(&format!("__fn_shape_{}", fn_name), Span::call_site());
    let shape_name = Ident::new(
        &format!("{}_SHAPE", fn_name.to_string().to_uppercase()),
        Span::call_site(),
    );
    let defs: Vec<_> = params
        .iter()
        .map(|p| {
            let name = &p.name;
            let ty = &p.param_type_tokens();
            quote! { #name: #ty }
        })
        .collect();
    let idents: Vec<_> = params
        .iter()
        .map(|p| {
            let name = &p.name;
            quote! { #name }
        })
        .collect();
    let types: Vec<_> = params
        .iter()
        .map(|p| {
            let ty = &p.param_type_tokens();
            quote! { #ty }
        })
        .collect();
    let names: Vec<_> = params
        .iter()
        .map(|p| p.name.to_string())
        .collect::<Vec<_>>();
    let arity = params.len();
    let fn_name_str = fn_name.to_string();

    // Extract  type parameters for PhantomData using unsynn parsing
    let generics_type = if let Some(ref generics_ts) = generics {
        extract_type_params(generics_ts.clone())
    } else {
        quote! { () }
    };

    let shape_definition = quote! {
        pub fn shape #generics () -> FunctionShape<( #( #types ),* ), #return_type, #generics_type> {
            FunctionShape::new(
                #fn_name_str,
                #arity,
                &[ #( #names ),* ]
            )
        }
    };

    let out = quote! {
        // 1) Move the real implementation into a private module
        #[allow(non_snake_case)]
        mod #hidden_mod {
            use super::*;
            pub(super) fn inner #generics ( #( #defs ),* ) -> #return_type #body

            #[derive(Debug, Clone)]
            pub struct FunctionShape<Args, Ret, Generics = ()> {
                pub name: &'static str,
                pub param_count: usize,
                pub param_names: &'static [&'static str],
                _args: core::marker::PhantomData<Args>,
                _ret: core::marker::PhantomData<Ret>,
                _generics: core::marker::PhantomData<Generics>,
            }

            impl<Args, Ret, Generics> FunctionShape<Args, Ret, Generics> {
                pub const fn new(
                    name: &'static str,
                    param_count: usize,
                    param_names: &'static [&'static str],
                ) -> Self {
                    Self {
                        name,
                        param_count,
                        param_names,
                        _args: core::marker::PhantomData,
                        _ret: core::marker::PhantomData,
                        _generics: core::marker::PhantomData,
                    }
                }
            }

            #shape_definition
        }

        // 2) Public wrapper retains the exact original signature
        pub fn #fn_name #generics ( #( #defs ),* ) -> #return_type {
            #hidden_mod::inner( #( #idents ),* )
        }

        // 3) Re-export the shape function with function name
        pub use #hidden_mod::shape as #shape_name;
    };

    out
}

fn generate_fn_shape_call(parsed: ParsedFnShapeInput) -> TokenStream {
    let fn_name = parsed.name;
    let generic_args = parsed.generics;

    // Generate the shape function name
    let shape_name = Ident::new(
        &format!("{}_SHAPE", fn_name.to_string().to_uppercase()),
        Span::call_site(),
    );

    if let Some(generics) = generic_args {
        quote! { #shape_name::#generics() }
    } else {
        quote! { #shape_name() }
    }
}
