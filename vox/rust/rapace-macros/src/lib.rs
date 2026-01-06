//! Procedural macros for rapace RPC service definitions.
//!
//! # Example
//!
//! ```ignore
//! #[rapace_service_macros::service]
//! trait Calculator {
//!     /// Add two numbers.
//!     async fn add(&self, a: i32, b: i32) -> i32;
//!
//!     /// Generate numbers from 0 to n-1.
//!     async fn range(&self, n: u32) -> Vec<u32>;
//! }
//!
//! // Generated:
//! // - The original trait (unchanged)
//! // - `calculator_service_detail()` -> ServiceDetail
//! ```

#![deny(unsafe_code)]

use heck::ToSnakeCase;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};

mod parser;

use parser::{ParsedTrait, parse_trait};

/// Marks a trait as a rapace RPC service and generates codegen helpers.
///
/// # Generated Items
///
/// For a trait named `Calculator`:
/// - The original trait definition (unchanged)
/// - `calculator_service_detail()` - Returns `ServiceDetail` for codegen
#[proc_macro_attribute]
pub fn service(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = TokenStream2::from(item);

    let parsed = match parse_trait(&input) {
        Ok(p) => p,
        Err(e) => return e.to_compile_error().into(),
    };

    match generate_service(&parsed, input) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn generate_service(
    parsed: &ParsedTrait,
    original: TokenStream2,
) -> Result<TokenStream2, parser::Error> {
    let trait_name = &parsed.name;
    let trait_snake = trait_name.to_snake_case();

    let service_detail_fn_name = format_ident!("{}_service_detail", trait_snake);
    let method_details = generate_method_details(parsed);

    let service_doc = parsed
        .doc
        .as_ref()
        .map(|d| quote! { Some(#d.into()) })
        .unwrap_or_else(|| quote! { None });

    Ok(quote! {
        // Emit the original trait unchanged
        #original

        /// Returns the service detail for codegen.
        pub fn #service_detail_fn_name() -> ::rapace_schema::ServiceDetail {
            ::rapace_schema::ServiceDetail {
                name: #trait_name.into(),
                methods: vec![#(#method_details),*],
                doc: #service_doc,
            }
        }
    })
}

fn generate_method_details(parsed: &ParsedTrait) -> Vec<TokenStream2> {
    let service_name = &parsed.name;

    parsed
        .methods
        .iter()
        .map(|m| {
            let method_name = &m.name;
            let method_doc = m
                .doc
                .as_ref()
                .map(|d| quote! { Some(#d.into()) })
                .unwrap_or_else(|| quote! { None });

            let arg_exprs: Vec<TokenStream2> = m
                .args
                .iter()
                .map(|arg| {
                    let arg_name = &arg.name;
                    let type_detail = type_detail_expr(
                        &arg.ty,
                        &format!(
                            "{}.{} argument `{}`",
                            parsed.name, m.name, arg.name
                        ),
                    );
                    quote! {
                        ::rapace_schema::ArgDetail {
                            name: #arg_name.into(),
                            type_info: #type_detail,
                        }
                    }
                })
                .collect();

            let return_type_detail = type_detail_expr(
                &m.return_type,
                &format!("{}.{} return type", parsed.name, m.name),
            );

            quote! {
                ::rapace_schema::MethodDetail {
                    service_name: #service_name.into(),
                    method_name: #method_name.into(),
                    args: vec![#(#arg_exprs),*],
                    return_type: #return_type_detail,
                    doc: #method_doc,
                }
            }
        })
        .collect()
}

fn type_detail_expr(ty: &TokenStream2, context: &str) -> TokenStream2 {
    let ty_s = ty.to_string();
    quote! {
        ::rapace_reflect::type_detail::<#ty>().unwrap_or_else(|e| {
            panic!(
                "failed to compute Rapace TypeDetail for {} (type: `{}`): {}",
                #context,
                #ty_s,
                e,
            )
        })
    }
}
