//! Procedural macros for roam RPC service definitions.
//!
//! The `#[service]` macro generates everything needed for a roam RPC service:
//! - The service trait with proper return types
//! - A dispatcher for server-side request handling
//! - A client for making RPC calls
//! - Method ID functions for wire protocol

#![deny(unsafe_code)]

use heck::ToSnakeCase;
use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};

mod crate_name;
mod parser;

use crate_name::FoundCrate;
use parser::{ServiceMethod, ServiceTrait, ToTokens, Type};

/// Returns the token stream for accessing the `roam` crate.
fn roam_crate() -> TokenStream2 {
    match crate_name::crate_name("roam") {
        Ok(FoundCrate::Itself) => quote! { crate },
        Ok(FoundCrate::Name(name)) => {
            let ident = format_ident!("{}", name);
            quote! { ::#ident }
        }
        Err(_) => quote! { ::roam },
    }
}

/// Marks a trait as a roam RPC service and generates all service code.
///
/// # Generated Items
///
/// For a trait named `Calculator`, this generates:
/// - `mod calculator` containing:
///   - `pub use` of common types (Tx, Rx, RoamError, etc.)
///   - `mod method_id` with lazy method ID functions
///   - `trait Calculator` - the service trait
///   - `struct CalculatorDispatcher<H>` - server-side dispatcher
///   - `struct CalculatorClient` - client for making calls
///
/// # Example
///
/// ```ignore
/// #[roam::service]
/// trait Calculator {
///     async fn add(&self, a: i32, b: i32) -> i32;
/// }
/// ```
#[proc_macro_attribute]
pub fn service(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = TokenStream2::from(item);

    let parsed = match parser::parse(&input) {
        Ok(p) => p,
        Err(e) => return e.to_compile_error().into(),
    };

    match generate_service(&parsed) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn generate_service(parsed: &ServiceTrait) -> Result<TokenStream2, parser::Error> {
    // Validate: no channels in error types
    for method in parsed.methods() {
        let return_type = method.return_type();
        if let Some((_, err_ty)) = return_type.as_result()
            && err_ty.contains_channel()
        {
            return Err(parser::Error::new(
                proc_macro2::Span::call_site(),
                format!(
                    "method `{}` has Channel (Tx/Rx) in error type - channels are not allowed in error types",
                    method.name()
                ),
            ));
        }
    }

    let roam = roam_crate();

    let method_id_mod = generate_method_id_module(parsed, &roam);
    let service_trait = generate_service_trait(parsed, &roam);
    let dispatcher = generate_dispatcher(parsed, &roam);
    let client = generate_client(parsed, &roam);
    let service_detail_fn = generate_service_detail_fn(parsed, &roam);

    // Generate items directly in the current module scope - no wrapper module.
    // This avoids type resolution issues since all types are already in scope.
    // Note: We use fully qualified paths for RoamError and Never instead of
    // importing them, to allow multiple services in the same module.
    Ok(quote! {
        #method_id_mod
        #service_trait
        #dispatcher
        #client
        #service_detail_fn
    })
}

// ============================================================================
// Method ID Generation
// ============================================================================

fn generate_method_id_module(parsed: &ServiceTrait, roam: &TokenStream2) -> TokenStream2 {
    let service_name = parsed.name();
    let mod_name = format_ident!("{}_method_id", service_name.to_snake_case());
    let method_fns: Vec<TokenStream2> = parsed
        .methods()
        .map(|m| generate_method_id_fn(m, &service_name, roam))
        .collect();

    quote! {
        /// Method IDs for the service (computed lazily at runtime).
        #[allow(non_snake_case, clippy::all, unused)]
        pub mod #mod_name {
            use std::sync::LazyLock;
            use super::*;

            #(#method_fns)*
        }
    }
}

fn generate_method_id_fn(
    method: &ServiceMethod,
    service_name: &str,
    roam: &TokenStream2,
) -> TokenStream2 {
    let method_name = method.name();
    let fn_name = format_ident!("{}", method_name.to_snake_case());

    // Build args array - use the types directly
    let arg_shapes: Vec<TokenStream2> = method
        .args()
        .map(|arg| {
            let ty = arg.ty.to_token_stream();
            quote! { <#ty as #roam::facet::Facet>::SHAPE }
        })
        .collect();

    let args_array = if arg_shapes.is_empty() {
        quote! { &[] }
    } else {
        quote! { &[#(#arg_shapes),*] }
    };

    let return_type = method.return_type();
    let return_ty_tokens = return_type.to_token_stream();
    let return_shape = quote! { <#return_ty_tokens as #roam::facet::Facet>::SHAPE };

    quote! {
        pub fn #fn_name() -> u64 {
            static ID: LazyLock<u64> = LazyLock::new(|| {
                #roam::hash::method_id_from_shapes(
                    #service_name,
                    #method_name,
                    #args_array,
                    #return_shape,
                )
            });
            *ID
        }
    }
}

// ============================================================================
// Service Trait Generation
// ============================================================================

fn generate_service_trait(parsed: &ServiceTrait, roam: &TokenStream2) -> TokenStream2 {
    let trait_name = format_ident!("{}", parsed.name());

    let trait_doc = parsed.doc().map(|d| quote! { #[doc = #d] });

    let methods: Vec<TokenStream2> = parsed
        .methods()
        .map(|m| generate_trait_method(m, roam))
        .collect();

    quote! {
        #trait_doc
        pub trait #trait_name
        where
            Self: Send + Sync,
        {
            #(#methods)*
        }
    }
}

fn generate_trait_method(method: &ServiceMethod, roam: &TokenStream2) -> TokenStream2 {
    let method_name = format_ident!("{}", method.name().to_snake_case());
    let method_doc = method.doc().map(|d| quote! { #[doc = #d] });

    // Parameters
    let params: Vec<TokenStream2> = method
        .args()
        .map(|arg| {
            let name = format_ident!("{}", arg.name().to_snake_case());
            let ty = arg.ty.to_token_stream();
            quote! { #name: #ty }
        })
        .collect();

    // Return type - wrap in Result<T, RoamError<E>> or Result<T, RoamError<Never>>
    let return_type = method.return_type();
    let full_return = format_handler_return_type(&return_type, roam);

    quote! {
        #method_doc
        fn #method_name(&self, #(#params),*) -> impl std::future::Future<Output = #full_return> + Send;
    }
}

/// Format the return type as Result<T, RoamError<E>> for handler trait.
fn format_handler_return_type(return_type: &Type, roam: &TokenStream2) -> TokenStream2 {
    if let Some((ok_ty, err_ty)) = return_type.as_result() {
        let ok_tokens = ok_ty.to_token_stream();
        let err_tokens = err_ty.to_token_stream();
        quote! { Result<#ok_tokens, #roam::session::RoamError<#err_tokens>> }
    } else {
        let ty_tokens = return_type.to_token_stream();
        quote! { Result<#ty_tokens, #roam::session::RoamError<#roam::session::Never>> }
    }
}

// ============================================================================
// Dispatcher Generation
// ============================================================================

fn generate_dispatcher(parsed: &ServiceTrait, roam: &TokenStream2) -> TokenStream2 {
    let trait_name = format_ident!("{}", parsed.name());
    let dispatcher_name = format_ident!("{}Dispatcher", parsed.name());
    let method_id_mod = format_ident!("{}_method_id", parsed.name().to_snake_case());

    // Generate dispatch methods
    let dispatch_methods: Vec<TokenStream2> = parsed
        .methods()
        .map(|m| generate_dispatch_method(m, roam))
        .collect();

    // Generate the if-else chain for ServiceDispatcher impl
    let dispatch_arms: Vec<TokenStream2> = parsed
        .methods()
        .enumerate()
        .map(|(i, m)| {
            let method_name = format_ident!("{}", m.name().to_snake_case());
            let dispatch_name = format_ident!("dispatch_{}", m.name().to_snake_case());
            let keyword = if i == 0 {
                quote! { if }
            } else {
                quote! { else if }
            };
            quote! {
                #keyword method_id == #method_id_mod::#method_name() {
                    self.#dispatch_name(payload, request_id, registry)
                }
            }
        })
        .collect();

    quote! {
        /// Dispatcher for this service.
        #[derive(Clone)]
        pub struct #dispatcher_name<H> {
            handler: H,
        }

        impl<H> #dispatcher_name<H>
        where
            H: #trait_name + Clone + 'static,
        {
            pub fn new(handler: H) -> Self {
                Self { handler }
            }

            #(#dispatch_methods)*
        }

        impl<H> #roam::session::ServiceDispatcher for #dispatcher_name<H>
        where
            H: #trait_name + Clone + 'static,
        {
            fn dispatch(
                &self,
                method_id: u64,
                payload: Vec<u8>,
                request_id: u64,
                registry: &mut #roam::session::ChannelRegistry,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
                #(#dispatch_arms)*
                else {
                    #roam::session::dispatch_unknown_method(request_id, registry)
                }
            }
        }
    }
}

fn generate_dispatch_method(method: &ServiceMethod, roam: &TokenStream2) -> TokenStream2 {
    let method_name = format_ident!("{}", method.name().to_snake_case());
    let dispatch_name = format_ident!("dispatch_{}", method.name().to_snake_case());

    // Build arg types tuple
    let arg_types: Vec<TokenStream2> = method.args().map(|arg| arg.ty.to_token_stream()).collect();

    let tuple_type = if arg_types.is_empty() {
        quote! { () }
    } else if arg_types.len() == 1 {
        let t = &arg_types[0];
        quote! { (#t,) }
    } else {
        quote! { (#(#arg_types),*) }
    };

    // Build destructure pattern
    let arg_names: Vec<proc_macro2::Ident> = method
        .args()
        .map(|arg| format_ident!("{}", arg.name().to_snake_case()))
        .collect();

    let destructure = if arg_names.is_empty() {
        quote! { _args }
    } else if arg_names.len() == 1 {
        let n = &arg_names[0];
        quote! { (#n,) }
    } else {
        quote! { (#(#arg_names),*) }
    };

    let args_call = if arg_names.is_empty() {
        quote! {}
    } else {
        quote! { #(#arg_names),* }
    };

    quote! {
        fn #dispatch_name(
            &self,
            payload: Vec<u8>,
            request_id: u64,
            registry: &mut #roam::session::ChannelRegistry,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
            let handler = self.handler.clone();
            #roam::session::dispatch_call(payload, request_id, registry, move |#destructure: #tuple_type| async move {
                handler.#method_name(#args_call).await
            })
        }
    }
}

// ============================================================================
// Client Generation
// ============================================================================

fn generate_client(parsed: &ServiceTrait, roam: &TokenStream2) -> TokenStream2 {
    let client_name = format_ident!("{}Client", parsed.name());
    let method_id_mod = format_ident!("{}_method_id", parsed.name().to_snake_case());

    let client_doc = format!(
        "Client for {} service.\n\n\
        Generic over any [`Caller`]({roam}::session::Caller) implementation, \
        allowing use with both [`ConnectionHandle`]({roam}::session::ConnectionHandle) \
        and reconnecting clients.",
        parsed.name()
    );

    let client_methods: Vec<TokenStream2> = parsed
        .methods()
        .map(|m| generate_client_method(m, &method_id_mod, roam))
        .collect();

    quote! {
        #[doc = #client_doc]
        #[derive(Clone)]
        pub struct #client_name<C: #roam::session::Caller = #roam::session::ConnectionHandle> {
            caller: C,
        }

        impl<C: #roam::session::Caller> #client_name<C> {
            /// Create a new client wrapping the given caller.
            pub fn new(caller: C) -> Self {
                Self { caller }
            }

            #(#client_methods)*
        }
    }
}

fn generate_client_method(
    method: &ServiceMethod,
    method_id_mod: &proc_macro2::Ident,
    roam: &TokenStream2,
) -> TokenStream2 {
    let method_name = format_ident!("{}", method.name().to_snake_case());
    let method_doc = method.doc().map(|d| quote! { #[doc = #d] });

    // Parameters
    let params: Vec<TokenStream2> = method
        .args()
        .map(|arg| {
            let name = format_ident!("{}", arg.name().to_snake_case());
            let ty = arg.ty.to_token_stream();
            quote! { #name: #ty }
        })
        .collect();

    let arg_names: Vec<proc_macro2::Ident> = method
        .args()
        .map(|arg| format_ident!("{}", arg.name().to_snake_case()))
        .collect();

    // Build args tuple
    let args_tuple = if arg_names.is_empty() {
        quote! { () }
    } else if arg_names.len() == 1 {
        let n = &arg_names[0];
        quote! { (#n,) }
    } else {
        quote! { (#(#arg_names),*) }
    };

    // Return type - Result<Result<T, RoamError<E>>, C::Error>
    let return_type = method.return_type();
    let client_return = format_client_return_type(&return_type, roam);

    quote! {
        #method_doc
        pub async fn #method_name(&self, #(#params),*) -> #client_return {
            let mut args = #args_tuple;
            let payload = #roam::session::Caller::call(&self.caller, #method_id_mod::#method_name(), &mut args).await?;
            #roam::session::CallError::decode_response(&payload).map_err(Into::into)
        }
    }
}

/// Format the return type as Result<Result<T, RoamError<E>>, C::Error> for client.
fn format_client_return_type(return_type: &Type, roam: &TokenStream2) -> TokenStream2 {
    if let Some((ok_ty, err_ty)) = return_type.as_result() {
        let ok_tokens = ok_ty.to_token_stream();
        let err_tokens = err_ty.to_token_stream();
        quote! { Result<Result<#ok_tokens, #roam::session::RoamError<#err_tokens>>, C::Error> }
    } else {
        let ty_tokens = return_type.to_token_stream();
        quote! { Result<Result<#ty_tokens, #roam::session::RoamError<#roam::session::Never>>, C::Error> }
    }
}

// ============================================================================
// Service Detail Function Generation (for codegen in other languages)
// ============================================================================

fn generate_service_detail_fn(parsed: &ServiceTrait, roam: &TokenStream2) -> TokenStream2 {
    let trait_name = parsed.name();
    let trait_snake = trait_name.to_snake_case();
    let fn_name = format_ident!("{}_service_detail", trait_snake);

    let method_details = generate_method_details(parsed, roam);

    let service_doc = parsed
        .doc()
        .map(|d| quote! { Some(#d.into()) })
        .unwrap_or_else(|| quote! { None });

    quote! {
        /// Returns the service detail for codegen.
        pub fn #fn_name() -> #roam::schema::ServiceDetail {
            #roam::schema::ServiceDetail {
                name: #trait_name.into(),
                methods: vec![#(#method_details),*],
                doc: #service_doc,
            }
        }
    }
}

fn generate_method_details(parsed: &ServiceTrait, roam: &TokenStream2) -> Vec<TokenStream2> {
    let service_name = parsed.name();

    parsed
        .methods()
        .map(|m| {
            let method_name = m.name();
            let method_doc = m
                .doc()
                .map(|d| quote! { Some(#d.into()) })
                .unwrap_or_else(|| quote! { None });

            let arg_exprs: Vec<TokenStream2> = m
                .args()
                .map(|arg| {
                    let arg_name = arg.name();
                    let ty = arg.ty.to_token_stream();
                    quote! {
                        #roam::schema::ArgDetail {
                            name: #arg_name.into(),
                            ty: <#ty as #roam::facet::Facet>::SHAPE,
                        }
                    }
                })
                .collect();

            let return_type = m.return_type();
            let return_ty_tokens = return_type.to_token_stream();

            quote! {
                #roam::schema::MethodDetail {
                    service_name: #service_name.into(),
                    method_name: #method_name.into(),
                    args: vec![#(#arg_exprs),*],
                    return_type: <#return_ty_tokens as #roam::facet::Facet>::SHAPE,
                    doc: #method_doc,
                }
            }
        })
        .collect()
}
