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

    let full_method_name = format!("{}.{}", service_name, method_name);
    quote! {
        pub fn #fn_name() -> u64 {
            static ID: LazyLock<u64> = LazyLock::new(|| {
                let id = #roam::hash::method_id_from_shapes(
                    #service_name,
                    #method_name,
                    #args_array,
                    #return_shape,
                );
                // Register method name for diagnostics (string literal from macro)
                static METHOD_NAME: &str = #full_method_name;
                #roam::session::diagnostic::register_method_name(id, METHOD_NAME);
                id
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

    // Parameters - cx: &Context comes first
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
        fn #method_name(&self, cx: &#roam::Context, #(#params),*) -> impl std::future::Future<Output = #full_return> + Send;
    }
}

/// Format the return type for handler trait - uses original type as-is.
fn format_handler_return_type(return_type: &Type, _roam: &TokenStream2) -> TokenStream2 {
    return_type.to_token_stream()
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
                    self.#dispatch_name(cx, payload, registry)
                }
            }
        })
        .collect();

    // Generate method ID calls for method_ids()
    let method_id_calls: Vec<TokenStream2> = parsed
        .methods()
        .map(|m| {
            let method_name = format_ident!("{}", m.name().to_snake_case());
            quote! { #method_id_mod::#method_name() }
        })
        .collect();

    quote! {
        /// Dispatcher for this service.
        ///
        /// Supports middleware that can inspect deserialized args before the handler runs.
        /// Middleware is configured via [`with_middleware`](Self::with_middleware).
        #[derive(Clone)]
        pub struct #dispatcher_name<H> {
            handler: H,
            middleware: Vec<std::sync::Arc<dyn #roam::session::Middleware>>,
        }

        impl<H> #dispatcher_name<H> {
            /// Returns all method IDs handled by this dispatcher.
            pub fn method_ids() -> Vec<u64> {
                vec![#(#method_id_calls),*]
            }
        }

        impl<H> #dispatcher_name<H>
        where
            H: #trait_name + Clone + 'static,
        {
            /// Create a new dispatcher with no middleware.
            pub fn new(handler: H) -> Self {
                Self {
                    handler,
                    middleware: Vec::new(),
                }
            }

            /// Add middleware to this dispatcher.
            ///
            /// Middleware runs after deserialization but before the handler.
            /// It can inspect args via reflection and reject requests.
            /// Middleware runs in the order it was added.
            pub fn with_middleware<M: #roam::session::Middleware + 'static>(mut self, mw: M) -> Self {
                self.middleware.push(std::sync::Arc::new(mw));
                self
            }

            /// Add already-Arc'd middleware to this dispatcher.
            pub fn with_middleware_arc(mut self, mw: std::sync::Arc<dyn #roam::session::Middleware>) -> Self {
                self.middleware.push(mw);
                self
            }

            #(#dispatch_methods)*
        }

        impl<H> #roam::session::ServiceDispatcher for #dispatcher_name<H>
        where
            H: #trait_name + Clone + 'static,
        {
            fn method_ids(&self) -> Vec<u64> {
                Self::method_ids()
            }

            fn dispatch(
                &self,
                cx: #roam::Context,
                payload: Vec<u8>,
                registry: &mut #roam::session::ChannelRegistry,
            ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
                let method_id = cx.method_id().raw();
                #(#dispatch_arms)*
                else {
                    #roam::session::dispatch_unknown_method(&cx, registry)
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

    // Build arg names for destructuring and calling
    let arg_names: Vec<proc_macro2::Ident> = method
        .args()
        .map(|arg| format_ident!("{}", arg.name().to_snake_case()))
        .collect();

    let args_call = if arg_names.is_empty() {
        quote! {}
    } else {
        quote! { #(#arg_names),* }
    };

    // Build a let binding to destructure args
    let args_binding = if arg_names.is_empty() {
        quote! { let _args: () = args; }
    } else if arg_names.len() == 1 {
        let n = &arg_names[0];
        quote! { let (#n,) = args; }
    } else {
        quote! { let (#(#arg_names),*) = args; }
    };

    let method_name_str = method.name();

    // For logging, we need to reference the args tuple (no colors for log output)
    let args_log = if arg_names.is_empty() {
        quote! { "()" }
    } else {
        quote! { args.pretty_with(#roam::PrettyPrinter::new().with_colors(#roam::facet_pretty::ColorMode::Never).with_max_content_len(64)) }
    };

    // Determine how to handle the result based on return type
    let return_type = method.return_type();
    let is_fallible = return_type.as_result().is_some();

    let (result_type, error_type) = if let Some((ok_ty, err_ty)) = return_type.as_result() {
        (ok_ty.to_token_stream(), Some(err_ty.to_token_stream()))
    } else {
        (return_type.to_token_stream(), None)
    };

    // Generate the post-middleware and response sending code based on fallibility.
    // We create SendPeek before calling the async functions to avoid capturing
    // raw pointers in the Future state (raw pointers are not Send).
    let send_response = if is_fallible {
        let err_ty = error_type.unwrap();
        quote! {
            match &result {
                Ok(value) => {
                    // Create SendPeek before calling async function to avoid !Send pointer
                    // SAFETY: value is valid, initialized, and Send
                    let send_peek = unsafe {
                        let peek = #roam::facet::Peek::unchecked_new(
                            #roam::facet_core::PtrConst::new((value as *const #result_type).cast::<u8>()),
                            <#result_type as #roam::facet::Facet>::SHAPE,
                        );
                        #roam::session::SendPeek::new(peek)
                    };

                    // Run post-middleware (observes outcome)
                    if !middleware.is_empty() {
                        let outcome = #roam::session::MethodOutcome::Ok(send_peek);
                        #roam::session::run_post_middleware(&cx, outcome, &middleware).await;
                    }

                    #roam::session::send_ok_response(
                        send_peek,
                        &driver_tx,
                        conn_id,
                        request_id,
                    ).await;
                }
                Err(error) => {
                    // Create SendPeek before calling async function
                    // SAFETY: error is valid, initialized, and Send
                    let send_peek = unsafe {
                        let peek = #roam::facet::Peek::unchecked_new(
                            #roam::facet_core::PtrConst::new((error as *const #err_ty).cast::<u8>()),
                            <#err_ty as #roam::facet::Facet>::SHAPE,
                        );
                        #roam::session::SendPeek::new(peek)
                    };

                    // Run post-middleware (observes outcome)
                    if !middleware.is_empty() {
                        let outcome = #roam::session::MethodOutcome::Err(send_peek);
                        #roam::session::run_post_middleware(&cx, outcome, &middleware).await;
                    }

                    #roam::session::send_error_response(
                        send_peek,
                        &driver_tx,
                        conn_id,
                        request_id,
                    ).await;
                }
            }
        }
    } else {
        quote! {
            // Create SendPeek before calling async function to avoid !Send pointer
            // SAFETY: result is valid, initialized, and Send
            let send_peek = unsafe {
                let peek = #roam::facet::Peek::unchecked_new(
                    #roam::facet_core::PtrConst::new((&result as *const #result_type).cast::<u8>()),
                    <#result_type as #roam::facet::Facet>::SHAPE,
                );
                #roam::session::SendPeek::new(peek)
            };

            // Run post-middleware (observes outcome)
            if !middleware.is_empty() {
                let outcome = #roam::session::MethodOutcome::Ok(send_peek);
                #roam::session::run_post_middleware(&cx, outcome, &middleware).await;
            }

            #roam::session::send_ok_response(
                send_peek,
                &driver_tx,
                conn_id,
                request_id,
            ).await;
        }
    };

    quote! {
        #[allow(unsafe_code)]
        fn #dispatch_name(
            &self,
            cx: #roam::Context,
            payload: Vec<u8>,
            registry: &mut #roam::session::ChannelRegistry,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = ()> + Send + 'static>> {
            use std::mem::MaybeUninit;

            let handler = self.handler.clone();
            let middleware = self.middleware.clone();
            let driver_tx = registry.driver_tx();
            let dispatch_ctx = registry.dispatch_context();
            let channels = cx.channels.clone();
            let conn_id = cx.conn_id;
            let request_id = cx.request_id.raw();

            // 1. Allocate args on stack
            let mut args_slot = MaybeUninit::<#tuple_type>::uninit();

            // 2. Prepare: deserialize, validate channels, patch IDs, bind streams (all SYNC)
            // SAFETY: args_slot is valid memory for tuple_type
            if let Err(e) = unsafe {
                #roam::session::prepare_sync(
                    args_slot.as_mut_ptr().cast(),
                    <#tuple_type as #roam::facet::Facet>::SHAPE,
                    &payload,
                    &channels,
                    registry,
                )
            } {
                return Box::pin(async move {
                    #roam::session::send_prepare_error(e, &driver_tx, conn_id, request_id).await;
                });
            }

            // 3. Read args - moves ownership for the async block
            // SAFETY: args are fully initialized after prepare_sync
            let args: #tuple_type = unsafe { args_slot.assume_init_read() };

            Box::pin(#roam::session::DISPATCH_CONTEXT.scope(dispatch_ctx, async move {
                let mut cx = cx;

                // 4. Run pre-middleware (ASYNC)
                if !middleware.is_empty() {
                    // SAFETY: args is valid, initialized, and the tuple type is Send
                    let args_peek = unsafe {
                        let peek = #roam::facet::Peek::unchecked_new(
                            #roam::facet_core::PtrConst::new((&args as *const #tuple_type).cast::<u8>()),
                            <#tuple_type as #roam::facet::Facet>::SHAPE,
                        );
                        #roam::session::SendPeek::new(peek)
                    };

                    if let Err(rejection) = #roam::session::run_pre_middleware(
                        args_peek,
                        &mut cx,
                        &middleware,
                    ).await {
                        // Still run post-middleware so it can clean up (e.g., end tracing spans)
                        #roam::session::run_post_middleware(
                            &cx,
                            #roam::session::MethodOutcome::Rejected,
                            &middleware,
                        ).await;

                        #roam::session::send_prepare_error(
                            #roam::session::PrepareError::Rejected(rejection),
                            &driver_tx,
                            conn_id,
                            request_id,
                        ).await;
                        return;
                    }
                }

                // 5. Log, destructure args, call handler
                // Scope CURRENT_EXTENSIONS so code inside the handler (like TracingCaller)
                // can access extensions set by middleware.
                use #roam::facet_pretty::FacetPretty;
                if #method_name_str != "emit_tracing" {
                    #roam::tracing::debug!(target: "roam::rpc", method = #method_name_str, args = %#args_log, "handling");
                }
                #args_binding
                let result = #roam::session::CURRENT_EXTENSIONS.scope(
                    cx.extensions.clone(),
                    handler.#method_name(&cx, #args_call)
                ).await;

                // 6. Send response
                #send_response
            }))
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

    // Build args tuple type for CallFuture
    let args_tuple_type = if arg_names.is_empty() {
        quote! { () }
    } else {
        let arg_types: Vec<TokenStream2> =
            method.args().map(|arg| arg.ty.to_token_stream()).collect();
        if arg_types.len() == 1 {
            let ty = &arg_types[0];
            quote! { (#ty,) }
        } else {
            quote! { (#(#arg_types),*) }
        }
    };

    // Build args tuple value
    let args_tuple = if arg_names.is_empty() {
        quote! { () }
    } else if arg_names.len() == 1 {
        let n = &arg_names[0];
        quote! { (#n,) }
    } else {
        quote! { (#(#arg_names),*) }
    };

    // Return type and error type depend on whether method is fallible
    let return_type = method.return_type();
    let (ok_ty, err_ty, _client_return) = format_client_return_type(&return_type, roam);

    // The CallFuture type
    let call_future_return = quote! {
        #roam::session::CallFuture<C, #args_tuple_type, #ok_ty, #err_ty>
    };

    quote! {
        #method_doc
        pub fn #method_name(&self, #(#params),*) -> #call_future_return {
            #roam::session::CallFuture::new(
                self.caller.clone(),
                #method_id_mod::#method_name(),
                #args_tuple,
            )
        }
    }
}

/// Format the return type as Result<T, CallError<E>> for client.
///
/// Returns (ok_type, err_type, full_return_type) for use in codegen.
fn format_client_return_type(
    return_type: &Type,
    roam: &TokenStream2,
) -> (TokenStream2, TokenStream2, TokenStream2) {
    if let Some((ok_ty, err_ty)) = return_type.as_result() {
        let ok_tokens = ok_ty.to_token_stream();
        let err_tokens = err_ty.to_token_stream();
        (
            ok_tokens.clone(),
            err_tokens.clone(),
            quote! { Result<#ok_tokens, #roam::session::CallError<#err_tokens>> },
        )
    } else {
        let ty_tokens = return_type.to_token_stream();
        (
            ty_tokens.clone(),
            quote! { ::core::convert::Infallible },
            quote! { Result<#ty_tokens, #roam::session::CallError<::core::convert::Infallible>> },
        )
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
