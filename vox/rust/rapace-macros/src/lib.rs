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
//! // - `calculator_method_ids()` -> &CalculatorMethodIds
//! // - `calculator_dispatch_unary()` -> dispatch helper for server implementations
//! // - `CalculatorClient<C>` -> client stub (requires `C: rapace_session::UnaryCaller`)
//! ```

#![deny(unsafe_code)]

use heck::ToSnakeCase;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};

mod crate_name;
mod parser;

use crate_name::FoundCrate;
use parser::{ParsedTrait, Type, parse_trait};

/// Returns the token stream for accessing the `rapace` crate.
///
/// This handles the case where the user has renamed the crate in their Cargo.toml.
fn rapace_crate() -> TokenStream2 {
    match crate_name::crate_name("rapace") {
        Ok(FoundCrate::Itself) => quote! { crate },
        Ok(FoundCrate::Name(name)) => {
            let ident = format_ident!("{}", name);
            quote! { ::#ident }
        }
        Err(_) => {
            // Fallback to the canonical name
            quote! { ::rapace }
        }
    }
}

/// Marks a trait as a rapace RPC service and generates codegen helpers.
///
/// # Generated Items
///
/// For a trait named `Calculator`:
/// - The original trait definition (unchanged)
/// - `calculator_service_detail()` - Returns `ServiceDetail` for codegen
/// - `calculator_method_ids()` - Returns a lazily-computed set of method IDs
/// - `calculator_dispatch_unary()` - Decodes arguments, calls the service, encodes response payload
/// - `CalculatorClient<C>` - Client stub operating over a `UnaryCaller`
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
    // Note: Stream validation deferred to runtime via Facet Shapes (per spec r[streaming.error-no-streams])

    let trait_name = &parsed.name;
    let trait_ident = format_ident!("{}", trait_name);
    let trait_snake = trait_name.to_snake_case();

    // Get the path to the rapace crate (handles crate renames)
    let rapace = rapace_crate();

    let service_detail_fn_name = format_ident!("{}_service_detail", trait_snake);
    let method_ids_struct_name = format_ident!("{}MethodIds", trait_name);
    let method_ids_fn_name = format_ident!("{}_method_ids", trait_snake);
    let dispatch_fn_name = format_ident!("{}_dispatch_unary", trait_snake);
    let client_struct_name = format_ident!("{}Client", trait_name);
    let method_details = generate_method_details(parsed, &rapace);
    let method_id_fields = generate_method_id_fields(parsed);
    let method_ids_init = generate_method_ids_init(parsed, &method_ids_struct_name, &rapace);
    let dispatch_arms = generate_dispatch_arms(parsed, &rapace);
    let client_methods = generate_client_methods(parsed, &method_ids_fn_name, &rapace);

    let service_doc = parsed
        .doc
        .as_ref()
        .map(|d| quote! { Some(#d.into()) })
        .unwrap_or_else(|| quote! { None });

    Ok(quote! {
        #[allow(async_fn_in_trait)]
        // Emit the original trait unchanged
        #original

        /// Returns the service detail for codegen.
        pub fn #service_detail_fn_name() -> #rapace::schema::ServiceDetail {
            #rapace::schema::ServiceDetail {
                name: #trait_name.into(),
                methods: vec![#(#method_details),*],
                doc: #service_doc,
            }
        }

        /// Method IDs for `#trait_ident` (computed from the canonical signature hash).
        #[derive(Debug, Clone, Copy, PartialEq, Eq)]
        pub struct #method_ids_struct_name {
            #(#method_id_fields),*
        }

        /// Lazily compute method IDs for this service from its `ServiceDetail`.
        pub fn #method_ids_fn_name() -> &'static #method_ids_struct_name {
            static IDS: ::std::sync::LazyLock<#method_ids_struct_name> = ::std::sync::LazyLock::new(|| {
                #method_ids_init
            });
            &IDS
        }

        /// Dispatch a unary request payload to the service implementation.
        ///
        /// This returns the *response payload bytes* (POSTCARD-encoded `Result<T, RapaceError<E>>`).
        pub async fn #dispatch_fn_name<S: #trait_ident + ?Sized>(
            service: &S,
            method_id: u64,
            payload: &[u8],
        ) -> ::core::result::Result<::std::vec::Vec<u8>, #rapace::session::DispatchError> {
            let ids = #method_ids_fn_name();
            match method_id {
                #(#dispatch_arms)*
                _ => {
                    let result: #rapace::session::CallResult<(), #rapace::session::Never> =
                        ::core::result::Result::Err(#rapace::session::RapaceError::UnknownMethod);
                    #rapace::__private::facet_postcard::to_vec(&result).map_err(#rapace::session::DispatchError::Encode)
                }
            }
        }

        /// Client stub for `#trait_ident` operating over a `rapace::session::UnaryCaller`.
        pub struct #client_struct_name<C> {
            caller: C,
        }

        impl<C> #client_struct_name<C> {
            pub fn new(caller: C) -> Self {
                Self { caller }
            }

            pub fn into_inner(self) -> C {
                self.caller
            }

            pub fn caller(&self) -> &C {
                &self.caller
            }

            pub fn caller_mut(&mut self) -> &mut C {
                &mut self.caller
            }
        }

        impl<C> #client_struct_name<C>
        where
            C: #rapace::session::UnaryCaller,
        {
            #(#client_methods)*
        }
    })
}

fn generate_method_details(parsed: &ParsedTrait, rapace: &TokenStream2) -> Vec<TokenStream2> {
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
                        &format!("{}.{} argument `{}`", parsed.name, m.name, arg.name),
                        rapace,
                    );
                    quote! {
                        #rapace::schema::ArgDetail {
                            name: #arg_name.into(),
                            type_info: #type_detail,
                        }
                    }
                })
                .collect();

            let return_type_detail = type_detail_expr(
                &m.return_type,
                &format!("{}.{} return type", parsed.name, m.name),
                rapace,
            );

            quote! {
                #rapace::schema::MethodDetail {
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

fn type_detail_expr(ty: &Type, context: &str, rapace: &TokenStream2) -> TokenStream2 {
    let ty_tokens = ty.to_tokens();
    let ty_s = ty_tokens.to_string();
    quote! {
        #rapace::reflect::type_detail::<#ty_tokens>().unwrap_or_else(|e| {
            panic!(
                "failed to compute Rapace TypeDetail for {} (type: `{}`): {}",
                #context,
                #ty_s,
                e,
            )
        })
    }
}

fn generate_method_id_fields(parsed: &ParsedTrait) -> Vec<TokenStream2> {
    parsed
        .methods
        .iter()
        .map(|m| {
            let name = format_ident!("{}", m.name.to_snake_case());
            quote! { pub #name: u64 }
        })
        .collect()
}

fn generate_method_ids_init(
    parsed: &ParsedTrait,
    method_ids_struct_name: &proc_macro2::Ident,
    rapace: &TokenStream2,
) -> TokenStream2 {
    let trait_name = &parsed.name;
    let trait_snake = trait_name.to_snake_case();
    let service_detail_fn_name = format_ident!("{}_service_detail", trait_snake);

    let vars: Vec<_> = parsed
        .methods
        .iter()
        .map(|m| format_ident!("id_{}", m.name.to_snake_case()))
        .collect();

    let mut init_arms = Vec::new();
    for (method, var) in parsed.methods.iter().zip(vars.iter()) {
        let method_name = &method.name;
        init_arms.push(quote! { #method_name => { #var = Some(id); } });
    }

    let field_inits: Vec<_> = parsed
        .methods
        .iter()
        .zip(vars.iter())
        .map(|(m, var)| {
            let field = format_ident!("{}", m.name.to_snake_case());
            let msg = format!("service method id missing: {}.{}", parsed.name, m.name);
            quote! { #field: #var.expect(#msg) }
        })
        .collect();

    quote! {
        let svc = #service_detail_fn_name();
        #(let mut #vars: ::core::option::Option<u64> = None;)*
        for m in &svc.methods {
            let id = #rapace::hash::method_id_from_detail(m);
            match m.method_name.as_str() {
                #(#init_arms)*
                _ => {}
            }
        }
        #method_ids_struct_name { #(#field_inits),* }
    }
}

fn generate_dispatch_arms(parsed: &ParsedTrait, rapace: &TokenStream2) -> Vec<TokenStream2> {
    parsed
        .methods
        .iter()
        .map(|m| {
            let method_ident = format_ident!("{}", m.name);
            let method_id_field = format_ident!("{}", m.name.to_snake_case());
            let (ok_ty, user_err_ty) = method_ok_and_err_types(&m.return_type);
            let ok_ty_tokens = ok_ty.to_tokens();
            let user_err_ty_tokens = user_err_ty.map(|t| t.to_tokens());
            let args_tuple_ty = args_tuple_type(&m.args);
            let args_pat = args_tuple_pattern(&m.args);
            let arg_idents: Vec<_> = m.args.iter().map(|a| format_ident!("{}", a.name)).collect();

            let call_and_wrap = if let Some(user_err_ty) = user_err_ty_tokens.as_ref() {
                quote! {
                    let out: ::core::result::Result<#ok_ty_tokens, #user_err_ty> =
                        service.#method_ident(#(#arg_idents),*).await;
                    let result: #rapace::session::CallResult<#ok_ty_tokens, #user_err_ty> =
                        out.map_err(#rapace::session::RapaceError::User);
                    #rapace::__private::facet_postcard::to_vec(&result).map_err(#rapace::session::DispatchError::Encode)
                }
            } else {
                quote! {
                    let out: #ok_ty_tokens = service.#method_ident(#(#arg_idents),*).await;
                    let result: #rapace::session::CallResult<#ok_ty_tokens, #rapace::session::Never> =
                        ::core::result::Result::Ok(out);
                    #rapace::__private::facet_postcard::to_vec(&result).map_err(#rapace::session::DispatchError::Encode)
                }
            };

            let invalid_payload = if let Some(user_err_ty) = user_err_ty_tokens.as_ref() {
                quote! {
                    let result: #rapace::session::CallResult<#ok_ty_tokens, #user_err_ty> =
                        ::core::result::Result::Err(#rapace::session::RapaceError::InvalidPayload);
                    return #rapace::__private::facet_postcard::to_vec(&result).map_err(#rapace::session::DispatchError::Encode);
                }
            } else {
                quote! {
                    let result: #rapace::session::CallResult<#ok_ty_tokens, #rapace::session::Never> =
                        ::core::result::Result::Err(#rapace::session::RapaceError::InvalidPayload);
                    return #rapace::__private::facet_postcard::to_vec(&result).map_err(#rapace::session::DispatchError::Encode);
                }
            };

            quote! {
                id if id == ids.#method_id_field => {
                    let decoded: #args_tuple_ty = match #rapace::__private::facet_postcard::from_slice(payload) {
                        Ok(v) => v,
                        Err(_) => { #invalid_payload }
                    };
                    let #args_pat = decoded;
                    #call_and_wrap
                }
            }
        })
        .collect()
}

fn generate_client_methods(
    parsed: &ParsedTrait,
    method_ids_fn_name: &proc_macro2::Ident,
    rapace: &TokenStream2,
) -> Vec<TokenStream2> {
    parsed
        .methods
        .iter()
        .map(|m| {
            let method_ident = format_ident!("{}", m.name);
            let method_id_field = format_ident!("{}", m.name.to_snake_case());
            let fn_args = m.args.iter().map(|arg| {
                let name = format_ident!("{}", arg.name);
                let ty = arg.ty.to_tokens();
                quote! { #name: #ty }
            });
            let arg_idents: Vec<_> = m.args.iter().map(|a| format_ident!("{}", a.name)).collect();

            let (ok_ty, user_err_ty) = method_ok_and_err_types(&m.return_type);
            let ok_ty_tokens = ok_ty.to_tokens();
            let user_err_ty_tokens = user_err_ty.map(|t| t.to_tokens());
            let (result_ty, decode_expr) = if needs_borrowed_call_result(ok_ty, user_err_ty)
            {
                let err_ty_tokens = user_err_ty_tokens.unwrap_or_else(|| quote! { #rapace::session::Never });
                (
                    quote! { #rapace::session::BorrowedCallResult<#ok_ty_tokens, #err_ty_tokens> },
                    quote! {
                        let owned: #rapace::session::BorrowedCallResult<#ok_ty_tokens, #err_ty_tokens> =
                            #rapace::session::OwnedMessage::try_new(frame, |payload| {
                                #rapace::__private::facet_postcard::from_slice_borrowed(payload)
                            })
                            .map_err(#rapace::session::ClientError::Decode)?;
                        Ok(owned)
                    },
                )
            } else {
                let err_ty_tokens = user_err_ty_tokens.unwrap_or_else(|| quote! { #rapace::session::Never });
                (
                    quote! { #rapace::session::CallResult<#ok_ty_tokens, #err_ty_tokens> },
                    quote! {
                        let decoded: #rapace::session::CallResult<#ok_ty_tokens, #err_ty_tokens> =
                            #rapace::__private::facet_postcard::from_slice(frame.payload_bytes())
                                .map_err(#rapace::session::ClientError::Decode)?;
                        Ok(decoded)
                    },
                )
            };

            let encode_args = args_encode_expr(&arg_idents, rapace);

            quote! {
                pub async fn #method_ident(
                    &mut self,
                    #(#fn_args),*
                ) -> ::core::result::Result<
                    #result_ty,
                    #rapace::session::ClientError<<C as #rapace::session::UnaryCaller>::Error>,
                > {
                    let ids = #method_ids_fn_name();
                    let request_payload = #encode_args.map_err(#rapace::session::ClientError::Encode)?;
                    let frame = self
                        .caller
                        .call_unary(ids.#method_id_field, request_payload)
                        .await
                        .map_err(#rapace::session::ClientError::Transport)?;
                    #decode_expr
                }
            }
        })
        .collect()
}

fn args_tuple_type(args: &[parser::ParsedArg]) -> TokenStream2 {
    let tys: Vec<_> = args.iter().map(|a| a.ty.to_tokens()).collect();
    match tys.len() {
        0 => quote! { () },
        1 => {
            let t0 = &tys[0];
            quote! { (#t0,) }
        }
        _ => quote! { ( #(#tys),* ) },
    }
}

fn args_tuple_pattern(args: &[parser::ParsedArg]) -> TokenStream2 {
    let idents: Vec<_> = args.iter().map(|a| format_ident!("{}", a.name)).collect();
    match idents.len() {
        0 => quote! { () },
        1 => {
            let a0 = &idents[0];
            quote! { (#a0,) }
        }
        _ => quote! { ( #(#idents),* ) },
    }
}

fn args_encode_expr(arg_idents: &[proc_macro2::Ident], rapace: &TokenStream2) -> TokenStream2 {
    match arg_idents.len() {
        0 => quote! { #rapace::__private::facet_postcard::to_vec(&()) },
        1 => {
            let a0 = &arg_idents[0];
            quote! { #rapace::__private::facet_postcard::to_vec(&(#a0,)) }
        }
        _ => quote! { #rapace::__private::facet_postcard::to_vec(&(#(#arg_idents),*)) },
    }
}

fn needs_borrowed_call_result(ok_ty: &Type, err_ty: Option<&Type>) -> bool {
    ok_ty.has_lifetime() || err_ty.is_some_and(|t| t.has_lifetime())
}

fn method_ok_and_err_types(return_ty: &Type) -> (&Type, Option<&Type>) {
    if let Some((ok, err)) = return_ty.as_result() {
        (ok, Some(err))
    } else {
        (return_ty, None)
    }
}

/// rs[impl streaming.error-no-streams] - validate no Stream in error position
fn validate_no_stream_in_errors(parsed: &ParsedTrait) -> Result<(), parser::Error> {
    for method in &parsed.methods {
        let (_ok_ty, err_ty) = method_ok_and_err_types(&method.return_type);
        if let Some(err_ty) = err_ty {
            if err_ty.contains_stream() {
                return Err(parser::Error::new(
                    proc_macro2::Span::call_site(),
                    format!(
                        "Stream is not allowed in error type: {}.{} has error type {}",
                        parsed.name,
                        method.name,
                        err_ty.to_string()
                    ),
                ));
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_stream_in_error_type() {
        let input: TokenStream2 = r#"
            trait Bad {
                async fn bad_method(&self) -> Result<String, Stream<Error>>;
            }
        "#.parse().unwrap();

        let parsed = parse_trait(&input).expect("parse should succeed");
        let result = validate_no_stream_in_errors(&parsed);

        assert!(result.is_err(), "Should reject Stream in error type");
        let err = result.unwrap_err();
        assert!(err.message.contains("Stream"), "Error should mention Stream");
        assert!(err.message.contains("bad_method"), "Error should mention method name");
    }

    #[test]
    fn accepts_stream_in_ok_type() {
        let input: TokenStream2 = r#"
            trait Good {
                async fn good_method(&self) -> Result<Stream<String>, Error>;
            }
        "#.parse().unwrap();

        let parsed = parse_trait(&input).expect("parse should succeed");
        let result = validate_no_stream_in_errors(&parsed);

        assert!(result.is_ok(), "Should allow Stream in Ok type");
    }

    #[test]
    fn accepts_non_result_stream() {
        let input: TokenStream2 = r#"
            trait Good {
                async fn streaming(&self) -> Stream<String>;
            }
        "#.parse().unwrap();

        let parsed = parse_trait(&input).expect("parse should succeed");
        let result = validate_no_stream_in_errors(&parsed);

        assert!(result.is_ok(), "Should allow Stream as return type");
    }
}
