//! Code generation core for roam RPC service macros.
//!
//! This crate contains all the code generation logic for the `#[service]` proc macro,
//! extracted into a regular library so it can be unit-tested with insta snapshots.

#![deny(unsafe_code)]

use ::quote::{format_ident, quote};
use heck::ToSnakeCase;
use proc_macro2::TokenStream as TokenStream2;

pub mod crate_name;

pub use roam_macros_parse::*;

use crate_name::FoundCrate;

/// Error type for validation/codegen errors.
#[derive(Debug, Clone)]
pub struct Error {
    pub span: proc_macro2::Span,
    pub message: String,
}

impl Error {
    pub fn new(span: proc_macro2::Span, message: impl Into<String>) -> Self {
        Self {
            span,
            message: message.into(),
        }
    }

    pub fn to_compile_error(&self) -> TokenStream2 {
        let msg = &self.message;
        let span = self.span;
        quote::quote_spanned! {span=> compile_error!(#msg); }
    }
}

impl From<ParseError> for Error {
    fn from(err: ParseError) -> Self {
        Self::new(proc_macro2::Span::call_site(), err.to_string())
    }
}

/// Parse a trait definition from a token stream, returning a codegen-friendly error.
pub fn parse(tokens: &TokenStream2) -> Result<ServiceTrait, Error> {
    parse_trait(tokens).map_err(Error::from)
}

/// Returns the token stream for accessing the `roam` crate.
pub fn roam_crate() -> TokenStream2 {
    match crate_name::crate_name("roam") {
        Ok(FoundCrate::Itself) => quote! { crate },
        Ok(FoundCrate::Name(name)) => {
            let ident = format_ident!("{}", name);
            quote! { ::#ident }
        }
        Err(_) => quote! { ::roam },
    }
}

/// Convert a parsed type into a token stream where every borrowed lifetime is `'static`.
///
/// This is used for descriptor hashing and client borrowed-return decode paths, where
/// we need a concrete `'static` shape type independent of method-local lifetimes.
fn to_static_type_tokens(ty: &Type) -> TokenStream2 {
    match ty {
        Type::Reference(TypeRef { mutable, inner, .. }) => {
            let inner = to_static_type_tokens(inner);
            if mutable.is_some() {
                quote! { &'static mut #inner }
            } else {
                quote! { &'static #inner }
            }
        }
        Type::Tuple(TypeTuple(group)) => {
            let elems: Vec<TokenStream2> = group
                .content
                .iter()
                .map(|entry| to_static_type_tokens(&entry.value))
                .collect();
            match elems.len() {
                0 => quote! { () },
                1 => {
                    let t = &elems[0];
                    quote! { (#t,) }
                }
                _ => quote! { (#(#elems),*) },
            }
        }
        Type::PathWithGenerics(PathWithGenerics { path, args, .. }) => {
            let path = path.to_token_stream();
            let args: Vec<TokenStream2> = args
                .iter()
                .map(|entry| match &entry.value {
                    GenericArgument::Lifetime(_) => quote! { 'static },
                    GenericArgument::Type(inner) => to_static_type_tokens(inner),
                })
                .collect();
            quote! { #path < #(#args),* > }
        }
        Type::Path(path) => path.to_token_stream(),
    }
}

// r[service-macro.is-source-of-truth]
// r[impl rpc]
// r[impl rpc.service]
// r[impl rpc.service.methods]
/// Generate all service code for a parsed trait.
///
/// Takes a `roam` token stream (the path to the roam crate) so that this function
/// can be called from tests with a fixed path like `::roam`.
pub fn generate_service(parsed: &ServiceTrait, roam: &TokenStream2) -> Result<TokenStream2, Error> {
    // r[impl rpc.channel.placement]
    // Validate: channels are only allowed in method args.
    for method in parsed.methods() {
        let return_type = method.return_type();
        if return_type.contains_channel() {
            return Err(Error::new(
                proc_macro2::Span::call_site(),
                format!(
                    "method `{}` has Channel (Tx/Rx) in return type - channels are only allowed in method arguments",
                    method.name()
                ),
            ));
        }

        let (ok_ty, err_ty) = method_ok_and_err_types(&return_type);
        if ok_ty.has_elided_reference_lifetime() {
            return Err(Error::new(
                proc_macro2::Span::call_site(),
                format!(
                    "method `{}` return type uses an elided reference lifetime; use explicit `'roam` (for example `&'roam str`)",
                    method.name()
                ),
            ));
        }
        if ok_ty.has_non_named_lifetime("roam") {
            return Err(Error::new(
                proc_macro2::Span::call_site(),
                format!(
                    "method `{}` return type may only use lifetime `'roam` for borrowed response data",
                    method.name()
                ),
            ));
        }
        if let Some(err_ty) = err_ty
            && (err_ty.has_lifetime() || err_ty.has_elided_reference_lifetime())
        {
            return Err(Error::new(
                proc_macro2::Span::call_site(),
                format!(
                    "method `{}` error type must be owned (no lifetimes), because client errors are not wrapped in SelfRef",
                    method.name()
                ),
            ));
        }
    }

    let service_descriptor_fn = generate_service_descriptor_fn(parsed, roam);
    let service_trait = generate_service_trait(parsed, roam);
    let dispatcher = generate_dispatcher(parsed, roam);
    let client = generate_client(parsed, roam);
    Ok(quote! {
        #service_descriptor_fn
        #service_trait
        #dispatcher
        #client
    })
}

// ============================================================================
// Service Descriptor Generation
// ============================================================================

fn generate_service_descriptor_fn(parsed: &ServiceTrait, roam: &TokenStream2) -> TokenStream2 {
    let service_name = parsed.name();
    let descriptor_fn_name = format_ident!("{}_service_descriptor", service_name.to_snake_case());

    // Build method descriptor expressions
    let method_descriptors: Vec<TokenStream2> = parsed
        .methods()
        .map(|m| {
            let method_name_str = m.name();

            // Build args tuple type and return type
            let arg_types: Vec<TokenStream2> =
                m.args().map(|arg| to_static_type_tokens(&arg.ty)).collect();
            let args_tuple_ty = quote! { (#(#arg_types,)*) };
            let arg_name_strs: Vec<String> = m.args().map(|arg| arg.name().to_string()).collect();

            let return_type = m.return_type();
            let return_ty_tokens = to_static_type_tokens(&return_type);
            let retry_persist = m.is_persist();
            let retry_idem = m.is_idem();

            let method_doc_expr = match m.doc() {
                Some(d) => quote! { Some(#d) },
                None => quote! { None },
            };

            quote! {
                #roam::hash::method_descriptor_with_retry::<#args_tuple_ty, #return_ty_tokens>(
                    #service_name,
                    #method_name_str,
                    &[#(#arg_name_strs),*],
                    #method_doc_expr,
                    #roam::RetryPolicy {
                        persist: #retry_persist,
                        idem: #retry_idem,
                    },
                )
            }
        })
        .collect();

    let service_doc_expr = match parsed.doc() {
        Some(d) => quote! { Some(#d) },
        None => quote! { None },
    };

    quote! {
        #[allow(non_snake_case, clippy::all)]
        pub fn #descriptor_fn_name() -> &'static #roam::session::ServiceDescriptor {
            static DESCRIPTOR: std::sync::OnceLock<&'static #roam::session::ServiceDescriptor> = std::sync::OnceLock::new();
            DESCRIPTOR.get_or_init(|| {
                let methods: Vec<&'static #roam::session::MethodDescriptor> = vec![
                    #(#method_descriptors),*
                ];
                Box::leak(Box::new(#roam::session::ServiceDescriptor {
                    service_name: #service_name,
                    methods: Box::leak(methods.into_boxed_slice()),
                    doc: #service_doc_expr,
                }))
            })
        }
    }
}

// ============================================================================
// Service Trait Generation
// ============================================================================

fn generate_service_trait(parsed: &ServiceTrait, roam: &TokenStream2) -> TokenStream2 {
    let trait_name = parsed.name.clone();
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
    let wants_context = method.wants_context();

    let return_type = method.return_type();
    let (ok_ty_ref, err_ty_ref) = method_ok_and_err_types(&return_type);
    let ok_has_roam_lifetime = ok_ty_ref.has_named_lifetime("roam");
    let method_lifetime = if ok_has_roam_lifetime {
        quote! { <'roam> }
    } else {
        quote! {}
    };

    let params: Vec<TokenStream2> = method
        .args()
        .map(|arg| {
            let name = format_ident!("{}", arg.name().to_snake_case());
            let ty = arg.ty.to_token_stream();
            quote! { #name: #ty }
        })
        .collect();

    let context_param = wants_context.then(|| quote! { cx: &#roam::RequestContext<'_> });

    if ok_has_roam_lifetime {
        let ok_ty = ok_ty_ref.to_token_stream();
        let err_ty = err_ty_ref
            .map(Type::to_token_stream)
            .unwrap_or_else(|| quote! { ::core::convert::Infallible });
        let mut signature_params = Vec::new();
        if let Some(context_param) = context_param.clone() {
            signature_params.push(context_param);
        }
        signature_params.push(quote! { call: impl #roam::Call<'roam, #ok_ty, #err_ty> });
        signature_params.extend(params);
        quote! {
            #method_doc
            fn #method_name #method_lifetime (&self, #(#signature_params),*) -> impl std::future::Future<Output = ()> + Send;
        }
    } else {
        let output_ty = return_type.to_token_stream();
        let mut signature_params = Vec::new();
        if let Some(context_param) = context_param {
            signature_params.push(context_param);
        }
        signature_params.extend(params);
        quote! {
            #method_doc
            fn #method_name (&self, #(#signature_params),*) -> impl std::future::Future<Output = #output_ty> + Send;
        }
    }
}

// ============================================================================
// Dispatcher Generation
// ============================================================================

fn generate_dispatcher(parsed: &ServiceTrait, roam: &TokenStream2) -> TokenStream2 {
    let trait_name = parsed.name.clone();
    let dispatcher_name = format_ident!("{}Dispatcher", parsed.name());
    let descriptor_fn_name = format_ident!("{}_service_descriptor", parsed.name().to_snake_case());

    // Generate the if-else dispatch arms inside handle()
    let dispatch_arms: Vec<TokenStream2> = parsed
        .methods()
        .enumerate()
        .map(|(i, m)| generate_dispatch_arm(m, i, roam, &descriptor_fn_name))
        .collect();
    let retry_policy_arms: Vec<TokenStream2> = parsed
        .methods()
        .enumerate()
        .map(|(i, m)| {
            let persist = m.is_persist();
            let idem = m.is_idem();
            quote! {
                if method_id == #descriptor_fn_name().methods[#i].id {
                    return #roam::RetryPolicy {
                        persist: #persist,
                        idem: #idem,
                    };
                }
            }
        })
        .collect();

    let no_methods = dispatch_arms.is_empty();

    let dispatch_body = if no_methods {
        quote! {
            let _ = (call, reply);
        }
    } else {
        // r[impl rpc.unknown-method]
        quote! {
            let method_id = call.method_id;
            let args_bytes = match &call.args {
                #roam::Payload::Incoming(bytes) => bytes,
                _ => {
                    reply.send_error(#roam::RoamError::<::core::convert::Infallible>::InvalidPayload).await;
                    return;
                }
            };
            #(#dispatch_arms)*
            reply.send_error(#roam::RoamError::<::core::convert::Infallible>::UnknownMethod).await;
        }
    };

    quote! {
        /// Dispatcher for this service.
        ///
        /// Wraps a handler and implements [`#roam::Handler`] by routing incoming
        /// calls to the appropriate trait method by method ID.
        #[derive(Clone)]
        pub struct #dispatcher_name<H> {
            handler: H,
            middlewares: Vec<::std::sync::Arc<dyn #roam::ServerMiddleware>>,
        }

        impl<H> #dispatcher_name<H>
        where
            H: #trait_name + Clone + Send + Sync + 'static,
        {
            /// Create a new dispatcher wrapping the given handler.
            pub fn new(handler: H) -> Self {
                Self {
                    handler,
                    middlewares: vec![],
                }
            }

            /// Append a server middleware to this dispatcher.
            pub fn with_middleware(mut self, middleware: impl #roam::ServerMiddleware) -> Self {
                self.middlewares.push(::std::sync::Arc::new(middleware));
                self
            }

            async fn run_pre_hooks(&self, context: &#roam::RequestContext<'_>) {
                for middleware in &self.middlewares {
                    middleware.pre(context).await;
                }
            }

            async fn run_post_hooks(
                &self,
                context: &#roam::RequestContext<'_>,
                outcome: #roam::ServerCallOutcome,
            ) {
                for middleware in self.middlewares.iter().rev() {
                    middleware.post(context, outcome).await;
                }
            }
        }

        impl<H, R> #roam::Handler<R> for #dispatcher_name<H>
        where
            H: #trait_name + Clone + Send + Sync + 'static,
            R: #roam::ReplySink,
        {
            fn retry_policy(&self, method_id: #roam::MethodId) -> #roam::RetryPolicy {
                #(#retry_policy_arms)*
                #roam::RetryPolicy::default()
            }

            async fn handle(&self, call: #roam::SelfRef<#roam::RequestCall<'static>>, reply: R) {
                #dispatch_body
            }
        }
    }
}

fn generate_dispatch_arm(
    method: &ServiceMethod,
    method_index: usize,
    roam: &TokenStream2,
    descriptor_fn_name: &proc_macro2::Ident,
) -> TokenStream2 {
    let method_fn = format_ident!("{}", method.name().to_snake_case());
    let idx = method_index;
    let wants_context = method.wants_context();

    // Build args tuple type for deserialization
    let arg_types: Vec<TokenStream2> = method
        .args()
        .map(|a| to_static_type_tokens(&a.ty))
        .collect();
    let args_tuple_type = match arg_types.len() {
        0 => quote! { () },
        1 => {
            let t = &arg_types[0];
            quote! { (#t,) }
        }
        _ => quote! { (#(#arg_types),*) },
    };

    // Destructure args tuple into named bindings
    let arg_names: Vec<proc_macro2::Ident> = method
        .args()
        .map(|a| format_ident!("{}", a.name().to_snake_case()))
        .collect();
    let destructure = match arg_names.len() {
        0 => quote! { let () = args; },
        1 => {
            let n = &arg_names[0];
            quote! { let (#n,) = args; }
        }
        _ => quote! { let (#(#arg_names),*) = args; },
    };

    let _ = idx;

    let has_channels = method.args().any(|a| a.ty.contains_channel());

    let channel_binding = if has_channels {
        quote! {
            #[cfg(not(target_arch = "wasm32"))]
            {
                if let Some(binder) = reply.channel_binder() {
                    let plan = #roam::RpcPlan::for_type::<#args_tuple_type>();
                    if !plan.channel_locations.is_empty() {
                        // SAFETY: args is a valid, initialized value of type #args_tuple_type
                        // and we have exclusive access to it via &mut.
                        #[allow(unsafe_code)]
                        unsafe {
                            #roam::bind_channels_callee_args(
                                &mut args as *mut _ as *mut u8,
                                plan,
                                &call.channels,
                                binder,
                            );
                        }
                    }
                }
            }
        }
    } else {
        quote! {}
    };

    // When there are channels, args must be mut for binding
    let args_let = if has_channels {
        quote! { let mut args: #args_tuple_type }
    } else {
        quote! { let args: #args_tuple_type }
    };

    let return_type = method.return_type();
    let (ok_ty_ref, err_ty_ref) = method_ok_and_err_types(&return_type);
    let ok_has_roam_lifetime = ok_ty_ref.has_named_lifetime("roam");
    let is_fallible = return_type.as_result().is_some();
    let ok_ty = ok_ty_ref.to_token_stream();
    let err_ty = err_ty_ref
        .map(Type::to_token_stream)
        .unwrap_or_else(|| quote! { ::core::convert::Infallible });

    let context_setup = {
        quote! {
            let extensions = #roam::Extensions::new();
            let context = #roam::RequestContext::with_extensions(
                #descriptor_fn_name().methods[#idx],
                &call.metadata,
                &extensions,
            );
            if !self.middlewares.is_empty() {
                self.run_pre_hooks(&context).await;
            }
        }
    };

    let plain_handler_args: Vec<TokenStream2> = std::iter::empty()
        .chain(wants_context.then(|| quote! { &context }))
        .chain(arg_names.iter().map(|name| quote! { #name }))
        .collect();

    let borrowed_handler_args: Vec<TokenStream2> = std::iter::empty()
        .chain(wants_context.then(|| quote! { &context }))
        .chain(std::iter::once(quote! { sink_call }))
        .chain(arg_names.iter().map(|name| quote! { #name }))
        .collect();

    let invoke_and_reply = if ok_has_roam_lifetime {
        quote! {
            let (reply, outcome_handle) = #roam::observe_reply(reply);
            let sink_call = #roam::SinkCall::new(reply);
            self.handler.#method_fn(#(#borrowed_handler_args),*).await;
            if !self.middlewares.is_empty() {
                self.run_post_hooks(&context, outcome_handle.outcome()).await;
            }
        }
    } else if is_fallible {
        quote! {
            let (reply, outcome_handle) = #roam::observe_reply(reply);
            let result = self.handler.#method_fn(#(#plain_handler_args),*).await;
            let sink_call = #roam::SinkCall::new(reply);
            #roam::Call::<'_, #ok_ty, #err_ty>::reply(sink_call, result).await;
            if !self.middlewares.is_empty() {
                self.run_post_hooks(&context, outcome_handle.outcome()).await;
            }
        }
    } else {
        quote! {
            let (reply, outcome_handle) = #roam::observe_reply(reply);
            let value = self.handler.#method_fn(#(#plain_handler_args),*).await;
            let sink_call = #roam::SinkCall::new(reply);
            #roam::Call::<'_, #ok_ty, #err_ty>::ok(sink_call, value).await;
            if !self.middlewares.is_empty() {
                self.run_post_hooks(&context, outcome_handle.outcome()).await;
            }
        }
    };

    quote! {
        if method_id == #descriptor_fn_name().methods[#idx].id {
            #args_let = match #roam::facet_postcard::from_slice_borrowed(args_bytes) {
                Ok(v) => v,
                Err(_) => {
                    reply.send_error(#roam::RoamError::<::core::convert::Infallible>::InvalidPayload).await;
                    return;
                }
            };
            #channel_binding
            #destructure
            #context_setup
            #invoke_and_reply
            return;
        }
    }
}

// ============================================================================
// Client Generation
// ============================================================================

// r[impl rpc.caller]
fn generate_client(parsed: &ServiceTrait, roam: &TokenStream2) -> TokenStream2 {
    let client_name = format_ident!("{}Client", parsed.name());
    let descriptor_fn_name = format_ident!("{}_service_descriptor", parsed.name().to_snake_case());
    let service_name = parsed.name();

    let client_doc = format!(
        "Client for the `{service_name}` service.\n\n\
        Stores a type-erased [`Caller`]({roam}::Caller) implementation.",
    );

    let client_methods: Vec<TokenStream2> = parsed
        .methods()
        .enumerate()
        .map(|(i, m)| generate_client_method(m, i, &descriptor_fn_name, roam))
        .collect();

    quote! {
        #[doc = #client_doc]
        #[must_use = "Dropping this client may close the connection if it is the last caller."]
        #[derive(Clone)]
        pub struct #client_name {
            caller: #roam::ErasedCaller,
        }

        impl #client_name {
            /// Create a new client wrapping the given caller.
            pub fn new(caller: impl #roam::Caller) -> Self {
                Self {
                    caller: #roam::ErasedCaller::new(caller),
                }
            }

            /// Append a client middleware to this client.
            pub fn with_middleware(self, middleware: impl #roam::ClientMiddleware) -> Self {
                Self {
                    caller: self
                        .caller
                        .with_middleware(#descriptor_fn_name(), middleware),
                }
            }

            /// Resolve when the underlying connection closes.
            pub async fn closed(&self) {
                #roam::Caller::closed(&self.caller).await;
            }

            /// Return whether the underlying connection is still considered connected.
            pub fn is_connected(&self) -> bool {
                #roam::Caller::is_connected(&self.caller)
            }

            #(#client_methods)*
        }

        impl From<#roam::DriverCaller> for #client_name {
            fn from(caller: #roam::DriverCaller) -> Self {
                Self::new(caller)
            }
        }
    }
}

// r[impl zerocopy.send.borrowed]
// r[impl zerocopy.send.borrowed-in-struct]
// r[impl zerocopy.send.lifetime]
fn generate_client_method(
    method: &ServiceMethod,
    method_index: usize,
    descriptor_fn_name: &proc_macro2::Ident,
    roam: &TokenStream2,
) -> TokenStream2 {
    let method_name = format_ident!("{}", method.name().to_snake_case());
    let method_doc = method.doc().map(|d| quote! { #[doc = #d] });
    let idx = method_index;

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

    // Args tuple type (for RpcPlan::for_type)
    let arg_types: Vec<TokenStream2> = method
        .args()
        .map(|a| to_static_type_tokens(&a.ty))
        .collect();
    let args_tuple_type = match arg_types.len() {
        0 => quote! { () },
        1 => {
            let t = &arg_types[0];
            quote! { (#t,) }
        }
        _ => quote! { (#(#arg_types),*) },
    };

    // Args tuple value (for serialization)
    let args_tuple = match arg_names.len() {
        0 => quote! { () },
        1 => {
            let n = &arg_names[0];
            quote! { (#n,) }
        }
        _ => quote! { (#(#arg_names),*) },
    };

    // r[impl rpc.fallible]
    // r[impl rpc.fallible.caller-signature]
    let return_type = method.return_type();
    let (ok_type_for_lifetimes, _) = method_ok_and_err_types(&return_type);
    let ok_uses_roam_lifetime = ok_type_for_lifetimes.has_named_lifetime("roam");
    let (ok_ty_decode, err_ty, client_return) = if let Some((ok, err)) = return_type.as_result() {
        let ok_t = ok.to_token_stream();
        let ok_t_static = to_static_type_tokens(ok);
        let err_t = err.to_token_stream();
        (
            if ok_uses_roam_lifetime {
                ok_t_static.clone()
            } else {
                ok_t.clone()
            },
            err_t.clone(),
            if ok_uses_roam_lifetime {
                quote! { Result<#roam::SelfRef<#ok_t_static>, #roam::RoamError<#err_t>> }
            } else {
                quote! { Result<#ok_t, #roam::RoamError<#err_t>> }
            },
        )
    } else {
        let t = return_type.to_token_stream();
        let t_static = to_static_type_tokens(&return_type);
        (
            if ok_uses_roam_lifetime {
                t_static.clone()
            } else {
                t.clone()
            },
            quote! { ::core::convert::Infallible },
            if ok_uses_roam_lifetime {
                quote! { Result<#roam::SelfRef<#t_static>, #roam::RoamError> }
            } else {
                quote! { Result<#t, #roam::RoamError> }
            },
        )
    };

    let has_channels = method.args().any(|a| a.ty.contains_channel());

    let (args_binding, channel_binding) = if has_channels {
        (
            quote! { let mut args = #args_tuple; },
            quote! {
                #[cfg(not(target_arch = "wasm32"))]
                let channels = if let Some(binder) = #roam::Caller::channel_binder(&self.caller) {
                    let plan = #roam::RpcPlan::for_type::<#args_tuple_type>();
                    // SAFETY: args is a valid, initialized value of the args tuple type
                    // and we have exclusive access to it via &mut.
                    #[allow(unsafe_code)]
                    unsafe {
                        #roam::bind_channels_caller_args(
                            &mut args as *mut _ as *mut u8,
                            plan,
                            binder,
                        )
                    }
                } else {
                    vec![]
                };
                #[cfg(target_arch = "wasm32")]
                let channels: Vec<#roam::ChannelId> = vec![];
            },
        )
    } else {
        (
            quote! { let args = #args_tuple; },
            quote! { let channels = vec![]; },
        )
    };

    if ok_uses_roam_lifetime {
        quote! {
            #method_doc
            pub async fn #method_name(&self, #(#params),*) -> #client_return {
                let method_id = #descriptor_fn_name().methods[#idx].id;
                #args_binding
                #channel_binding
                let req = #roam::RequestCall {
                    method_id,
                    args: #roam::Payload::outgoing(&args),
                    channels,
                    metadata: Default::default(),
                };
                let response = #roam::Caller::call(&self.caller, req).await.map_err(|e| match e {
                    #roam::RoamError::UnknownMethod => #roam::RoamError::<#err_ty>::UnknownMethod,
                    #roam::RoamError::InvalidPayload => #roam::RoamError::<#err_ty>::InvalidPayload,
                    #roam::RoamError::Cancelled => #roam::RoamError::<#err_ty>::Cancelled,
                    #roam::RoamError::User(never) => match never {},
                })?;
                response.try_repack(|resp, _bytes| {
                    let ret_bytes = match &resp.ret {
                        #roam::Payload::Incoming(bytes) => bytes,
                        _ => return Err(#roam::RoamError::<#err_ty>::InvalidPayload),
                    };
                    let result: Result<#ok_ty_decode, #roam::RoamError<#err_ty>> =
                        #roam::facet_postcard::from_slice_borrowed(ret_bytes)
                            .map_err(|_| #roam::RoamError::<#err_ty>::InvalidPayload)?;
                    let ret = result?;
                    Ok(ret)
                })
            }
        }
    } else {
        quote! {
            #method_doc
            pub async fn #method_name(&self, #(#params),*) -> #client_return {
                let method_id = #descriptor_fn_name().methods[#idx].id;
                #args_binding
                #channel_binding
                let req = #roam::RequestCall {
                    method_id,
                    args: #roam::Payload::outgoing(&args),
                    channels,
                    metadata: Default::default(),
                };
                let response = #roam::Caller::call(&self.caller, req).await.map_err(|e| match e {
                    #roam::RoamError::UnknownMethod => #roam::RoamError::<#err_ty>::UnknownMethod,
                    #roam::RoamError::InvalidPayload => #roam::RoamError::<#err_ty>::InvalidPayload,
                    #roam::RoamError::Cancelled => #roam::RoamError::<#err_ty>::Cancelled,
                    #roam::RoamError::User(never) => match never {},
                })?;
                let ret_bytes = match &response.ret {
                    #roam::Payload::Incoming(bytes) => bytes,
                    _ => return Err(#roam::RoamError::<#err_ty>::InvalidPayload),
                };
                let result: Result<#ok_ty_decode, #roam::RoamError<#err_ty>> =
                    #roam::facet_postcard::from_slice(ret_bytes)
                        .map_err(|_| #roam::RoamError::<#err_ty>::InvalidPayload)?;
                result
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use insta::assert_snapshot;
    use quote::quote;

    fn prettyprint(ts: proc_macro2::TokenStream) -> String {
        use std::io::Write;
        use std::process::{Command, Stdio};

        let mut child = Command::new("rustfmt")
            .args(["--edition", "2024"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("failed to spawn rustfmt");

        child
            .stdin
            .take()
            .unwrap()
            .write_all(ts.to_string().as_bytes())
            .unwrap();

        let output = child.wait_with_output().expect("rustfmt failed");
        assert!(
            output.status.success(),
            "rustfmt exited with {}",
            output.status
        );
        String::from_utf8(output.stdout).expect("rustfmt output not UTF-8")
    }

    fn generate(input: proc_macro2::TokenStream) -> String {
        let parsed = roam_macros_parse::parse_trait(&input).unwrap();
        let roam = quote! { ::roam };
        let ts = crate::generate_service(&parsed, &roam).unwrap();
        prettyprint(ts)
    }

    #[test]
    fn adder_infallible() {
        assert_snapshot!(generate(quote! {
            pub trait Adder { async fn add(&self, a: i32, b: i32) -> i32; }
        }));
    }

    #[test]
    fn fallible() {
        assert_snapshot!(generate(quote! {
            trait Calc { async fn div(&self, a: f64, b: f64) -> Result<f64, DivError>; }
        }));
    }

    #[test]
    fn no_args() {
        assert_snapshot!(generate(quote! {
            trait Ping { async fn ping(&self) -> u64; }
        }));
    }

    #[test]
    fn explicit_request_context_opt_in() {
        assert_snapshot!(generate(quote! {
            trait Audit {
                #[roam::context]
                async fn record(&self, payload: String) -> &'roam str;

                async fn ping(&self) -> u64;
            }
        }));
    }

    #[test]
    fn method_retry_helper_attributes() {
        assert_snapshot!(generate(quote! {
            trait Billing {
                #[roam(idem)]
                async fn get_balance(&self, account: String) -> u64;

                #[roam(persist)]
                async fn send_money(&self, from: String, to: String) -> Result<u64, TransferError>;
            }
        }));
    }

    #[test]
    fn unit_return() {
        assert_snapshot!(generate(quote! {
            trait Notifier { async fn notify(&self, msg: String); }
        }));
    }

    #[test]
    fn streaming_tx() {
        assert_snapshot!(generate(quote! {
            trait Streamer { async fn count_up(&self, start: i32, output: Tx<i32>) -> i32; }
        }));
    }

    #[test]
    fn rejects_channels_in_return_type() {
        let parsed = roam_macros_parse::parse_trait(&quote! {
            trait Streamer { async fn stream(&self) -> Rx<i32>; }
        })
        .unwrap();
        let roam = quote! { ::roam };
        let err = crate::generate_service(&parsed, &roam).unwrap_err();
        assert_eq!(
            err.message,
            "method `stream` has Channel (Tx/Rx) in return type - channels are only allowed in method arguments"
        );
    }

    #[test]
    fn rejects_non_roam_return_lifetime() {
        let parsed = roam_macros_parse::parse_trait(&quote! {
            trait Svc { async fn bad(&self) -> &'a str; }
        })
        .unwrap();
        let roam = quote! { ::roam };
        let err = crate::generate_service(&parsed, &roam).unwrap_err();
        assert_eq!(
            err.message,
            "method `bad` return type may only use lifetime `'roam` for borrowed response data"
        );
    }

    #[test]
    fn rejects_elided_return_lifetime() {
        let parsed = roam_macros_parse::parse_trait(&quote! {
            trait Svc { async fn bad(&self) -> &str; }
        })
        .unwrap();
        let roam = quote! { ::roam };
        let err = crate::generate_service(&parsed, &roam).unwrap_err();
        assert_eq!(
            err.message,
            "method `bad` return type uses an elided reference lifetime; use explicit `'roam` (for example `&'roam str`)"
        );
    }

    #[test]
    fn rejects_borrowed_error_type() {
        let parsed = roam_macros_parse::parse_trait(&quote! {
            trait Svc { async fn bad(&self) -> Result<u32, &'roam str>; }
        })
        .unwrap();
        let roam = quote! { ::roam };
        let err = crate::generate_service(&parsed, &roam).unwrap_err();
        assert_eq!(
            err.message,
            "method `bad` error type must be owned (no lifetimes), because client errors are not wrapped in SelfRef"
        );
    }

    #[test]
    fn borrowed_roam_return() {
        assert_snapshot!(generate(quote! {
            trait Hasher { async fn hash(&self, payload: String) -> &'roam str; }
        }));
    }

    #[test]
    fn borrowed_roam_return_call_style() {
        assert_snapshot!(generate(quote! {
            trait Hasher { async fn hash(&self, payload: String) -> &'roam str; }
        }));
    }

    #[test]
    fn borrowed_roam_cow_return() {
        assert_snapshot!(generate(quote! {
            trait TextSvc {
                async fn normalize(&self, input: String) -> ::std::borrow::Cow<'roam, str>;
            }
        }));
    }

    #[test]
    fn borrowed_return_mixed_with_borrowed_args_and_channels_compiles_to_expected_shapes() {
        assert_snapshot!(generate(quote! {
            trait WordLab {
                async fn is_short(&self, word: &str) -> bool;
                async fn classify(&self, word: String) -> &'roam str;
                async fn transform(&self, prefix: &str, input: Rx<String>, output: Tx<String>) -> u32;
            }
        }));
    }
}
