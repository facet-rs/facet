//! Code generation core for vox RPC service macros.
//!
//! This crate contains all the code generation logic for the `#[service]` proc macro,
//! extracted into a regular library so it can be unit-tested with insta snapshots.

#![deny(unsafe_code)]

use ::quote::{format_ident, quote};
use heck::ToSnakeCase;
use proc_macro2::TokenStream as TokenStream2;

pub mod crate_name;

pub use vox_macros_parse::*;

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

/// Returns the token stream for accessing the `vox` crate.
pub fn vox_crate() -> TokenStream2 {
    match crate_name::crate_name("vox") {
        Ok(FoundCrate::Itself) => quote! { crate },
        Ok(FoundCrate::Name(name)) => {
            let ident = format_ident!("{}", name);
            quote! { ::#ident }
        }
        Err(_) => quote! { ::vox },
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

fn type_is_tx(ty: &Type) -> bool {
    match ty {
        Type::Reference(TypeRef { inner, .. }) => type_is_tx(inner),
        Type::PathWithGenerics(PathWithGenerics { path, .. }) => path.last_segment() == "Tx",
        Type::Path(path) => path.last_segment() == "Tx",
        Type::Tuple(_) => false,
    }
}

// r[service-macro.is-source-of-truth]
// r[impl rpc]
// r[impl rpc.service]
// r[impl rpc.service.methods]
/// Generate all service code for a parsed trait.
///
/// Takes a `vox` token stream (the path to the vox crate) so that this function
/// can be called from tests with a fixed path like `::vox`.
pub fn generate_service(parsed: &ServiceTrait, vox: &TokenStream2) -> Result<TokenStream2, Error> {
    // r[impl rpc.channel.placement]
    // Validate: channels are only allowed in method args.
    for method in parsed.methods() {
        match method.receiver_kind() {
            ReceiverKind::RefSelf => {}
            ReceiverKind::RefMutSelf => {
                let span = method
                    .params
                    .content
                    .receiver
                    .to_token_stream()
                    .into_iter()
                    .next()
                    .map(|tt| tt.span())
                    .unwrap_or_else(proc_macro2::Span::call_site);
                return Err(Error::new(
                    span,
                    format!(
                        "method `{}` receiver must be `&self`; `&mut self` is not supported in #[vox::service] traits",
                        method.name()
                    ),
                ));
            }
            ReceiverKind::SelfValue => {
                let span = method
                    .params
                    .content
                    .receiver
                    .to_token_stream()
                    .into_iter()
                    .next()
                    .map(|tt| tt.span())
                    .unwrap_or_else(proc_macro2::Span::call_site);
                return Err(Error::new(
                    span,
                    format!(
                        "method `{}` receiver must be `&self`; `self` is not supported in #[vox::service] traits",
                        method.name()
                    ),
                ));
            }
            ReceiverKind::MutSelfValue => {
                let span = method
                    .params
                    .content
                    .receiver
                    .to_token_stream()
                    .into_iter()
                    .next()
                    .map(|tt| tt.span())
                    .unwrap_or_else(proc_macro2::Span::call_site);
                return Err(Error::new(
                    span,
                    format!(
                        "method `{}` receiver must be `&self`; `mut self` is not supported in #[vox::service] traits",
                        method.name()
                    ),
                ));
            }
            ReceiverKind::TypedSelf => {
                let span = method
                    .params
                    .content
                    .receiver
                    .to_token_stream()
                    .into_iter()
                    .next()
                    .map(|tt| tt.span())
                    .unwrap_or_else(proc_macro2::Span::call_site);
                return Err(Error::new(
                    span,
                    format!(
                        "method `{}` receiver must be `&self`; typed receivers like `self: Type` are not supported in #[vox::service] traits",
                        method.name()
                    ),
                ));
            }
            ReceiverKind::MutTypedSelf => {
                let span = method
                    .params
                    .content
                    .receiver
                    .to_token_stream()
                    .into_iter()
                    .next()
                    .map(|tt| tt.span())
                    .unwrap_or_else(proc_macro2::Span::call_site);
                return Err(Error::new(
                    span,
                    format!(
                        "method `{}` receiver must be `&self`; typed receivers like `mut self: Type` are not supported in #[vox::service] traits",
                        method.name()
                    ),
                ));
            }
        }

        if !method.is_async() {
            let span = method
                .name
                .to_token_stream()
                .into_iter()
                .next()
                .map(|tt| tt.span())
                .unwrap_or_else(proc_macro2::Span::call_site);
            return Err(Error::new(
                span,
                format!(
                    "method `{}` must be declared `async fn` in a #[vox::service] trait",
                    method.name()
                ),
            ));
        }

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

        if method.is_persist() && method.args().any(|arg| arg.ty.contains_channel()) {
            return Err(Error::new(
                proc_macro2::Span::call_site(),
                format!(
                    "method `{}` declares `#[vox(persist)]` but has Channel (Tx/Rx) arguments - persist methods cannot carry channels",
                    method.name()
                ),
            ));
        }

        let (ok_ty, err_ty) = method_ok_and_err_types(&return_type);
        if ok_ty.has_elided_reference_lifetime() {
            return Err(Error::new(
                proc_macro2::Span::call_site(),
                format!(
                    "method `{}` return type uses an elided reference lifetime; use explicit `'vox` (for example `&'vox str`)",
                    method.name()
                ),
            ));
        }
        if ok_ty.has_non_named_lifetime("vox") {
            return Err(Error::new(
                proc_macro2::Span::call_site(),
                format!(
                    "method `{}` return type may only use lifetime `'vox` for borrowed response data",
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

    let service_descriptor_fn = generate_service_descriptor_fn(parsed, vox);
    let service_trait = generate_service_trait(parsed, vox);
    let dispatcher = generate_dispatcher(parsed, vox);
    let client = generate_client(parsed, vox);
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

fn generate_service_descriptor_fn(parsed: &ServiceTrait, vox: &TokenStream2) -> TokenStream2 {
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
                #vox::hash::method_descriptor_with_retry::<#args_tuple_ty, #return_ty_tokens>(
                    #service_name,
                    #method_name_str,
                    &[#(#arg_name_strs),*],
                    #method_doc_expr,
                    #vox::RetryPolicy {
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
        pub fn #descriptor_fn_name() -> &'static #vox::session::ServiceDescriptor {
            static DESCRIPTOR: std::sync::OnceLock<&'static #vox::session::ServiceDescriptor> = std::sync::OnceLock::new();
            DESCRIPTOR.get_or_init(|| {
                let methods: Vec<&'static #vox::session::MethodDescriptor> = vec![
                    #(#method_descriptors),*
                ];
                Box::leak(Box::new(#vox::session::ServiceDescriptor {
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

fn generate_service_trait(parsed: &ServiceTrait, vox: &TokenStream2) -> TokenStream2 {
    let trait_name = parsed.name.clone();
    let trait_doc = parsed.doc().map(|d| quote! { #[doc = #d] });

    let methods: Vec<TokenStream2> = parsed
        .methods()
        .map(|m| generate_trait_method(m, vox))
        .collect();

    quote! {
        #trait_doc
        pub trait #trait_name
        where
            Self: Clone + #vox::MaybeSend + #vox::MaybeSync + 'static,
        {
            #(#methods)*
        }
    }
}

fn generate_trait_method(method: &ServiceMethod, vox: &TokenStream2) -> TokenStream2 {
    let method_name = format_ident!("{}", method.name().to_snake_case());
    let method_doc = method.doc().map(|d| quote! { #[doc = #d] });
    let wants_context = method.wants_context();

    let return_type = method.return_type();
    let (ok_ty_ref, err_ty_ref) = method_ok_and_err_types(&return_type);
    let ok_has_vox_lifetime = ok_ty_ref.has_named_lifetime("vox");
    let method_lifetime = if ok_has_vox_lifetime {
        quote! { <'vox> }
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

    let context_param = wants_context.then(|| quote! { cx: &#vox::RequestContext<'_> });

    if ok_has_vox_lifetime {
        let ok_ty = ok_ty_ref.to_token_stream();
        let err_ty = err_ty_ref
            .map(Type::to_token_stream)
            .unwrap_or_else(|| quote! { ::core::convert::Infallible });
        let mut signature_params = Vec::new();
        if let Some(context_param) = context_param.clone() {
            signature_params.push(context_param);
        }
        signature_params.push(quote! { call: impl #vox::Call<'vox, #ok_ty, #err_ty> });
        signature_params.extend(params);
        quote! {
            #method_doc
            fn #method_name #method_lifetime (&self, #(#signature_params),*) -> impl std::future::Future<Output = ()> + #vox::MaybeSend;
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
            fn #method_name (&self, #(#signature_params),*) -> impl std::future::Future<Output = #output_ty> + #vox::MaybeSend;
        }
    }
}

// ============================================================================
// Dispatcher Generation
// ============================================================================

fn generate_dispatcher(parsed: &ServiceTrait, vox: &TokenStream2) -> TokenStream2 {
    let trait_name = parsed.name.clone();
    let dispatcher_name = format_ident!("{}Dispatcher", parsed.name());
    let descriptor_fn_name = format_ident!("{}_service_descriptor", parsed.name().to_snake_case());

    // Generate the if-else dispatch arms inside handle()
    let dispatch_arms: Vec<TokenStream2> = parsed
        .methods()
        .enumerate()
        .map(|(i, m)| generate_dispatch_arm(m, i, vox, &descriptor_fn_name))
        .collect();
    let retry_policy_arms: Vec<TokenStream2> = parsed
        .methods()
        .enumerate()
        .map(|(i, m)| {
            let persist = m.is_persist();
            let idem = m.is_idem();
            quote! {
                if method_id == #descriptor_fn_name().methods[#i].id {
                    return #vox::RetryPolicy {
                        persist: #persist,
                        idem: #idem,
                    };
                }
            }
        })
        .collect();
    let args_have_channels_arms: Vec<TokenStream2> = parsed
        .methods()
        .enumerate()
        .map(|(i, _m)| {
            quote! {
                if method_id == #descriptor_fn_name().methods[#i].id {
                    return #vox::hash::shape_contains_channel(
                        #descriptor_fn_name().methods[#i].args_shape,
                    );
                }
            }
        })
        .collect();
    let response_wire_shape_arms: Vec<TokenStream2> = parsed
        .methods()
        .enumerate()
        .map(|(i, m)| {
            let return_type = m.return_type();
            let (ok_ty_ref, err_ty_ref) = method_ok_and_err_types(&return_type);
            let ok_ty = to_static_type_tokens(ok_ty_ref);
            let err_ty = err_ty_ref
                .map(to_static_type_tokens)
                .unwrap_or_else(|| quote! { ::core::convert::Infallible });
            quote! {
                if method_id == #descriptor_fn_name().methods[#i].id {
                    return Some(
                        <Result<#ok_ty, #vox::VoxError<#err_ty>> as #vox::facet::Facet<'static>>::SHAPE,
                    );
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
                #vox::Payload::PostcardBytes(bytes) => bytes,
                _ => {
                    reply.send_error(#vox::VoxError::<::core::convert::Infallible>::InvalidPayload("args not PostcardBytes".into())).await;
                    return;
                }
            };
            #(#dispatch_arms)*
            reply.send_error(#vox::VoxError::<::core::convert::Infallible>::UnknownMethod).await;
        }
    };

    quote! {
        /// Dispatcher for this service.
        ///
        /// Wraps a handler and implements [`#vox::Handler`] by routing incoming
        /// calls to the appropriate trait method by method ID.
        #[derive(Clone)]
        pub struct #dispatcher_name<H> {
            handler: H,
            middlewares: Vec<::std::sync::Arc<dyn #vox::ServerMiddleware>>,
        }

        impl<H> #dispatcher_name<H>
        where
            H: #trait_name,
        {
            /// Create a new dispatcher wrapping the given handler.
            pub fn new(handler: H) -> Self {
                Self {
                    handler,
                    middlewares: vec![],
                }
            }

            /// Append a server middleware to this dispatcher.
            pub fn with_middleware(mut self, middleware: impl #vox::ServerMiddleware) -> Self {
                self.middlewares.push(::std::sync::Arc::new(middleware));
                self
            }

            async fn run_post_hooks(
                &self,
                context: &#vox::RequestContext<'_>,
                outcome: #vox::ServerCallOutcome,
            ) {
                for middleware in self.middlewares.iter().rev() {
                    middleware.post(context, outcome).await;
                }
            }
        }

        impl<H, R> #vox::Handler<R> for #dispatcher_name<H>
        where
            H: #trait_name,
            R: #vox::ReplySink,
        {
            fn retry_policy(&self, method_id: #vox::MethodId) -> #vox::RetryPolicy {
                #(#retry_policy_arms)*
                #vox::RetryPolicy::VOLATILE
            }

            fn args_have_channels(&self, method_id: #vox::MethodId) -> bool {
                #(#args_have_channels_arms)*
                false
            }

            fn response_wire_shape(
                &self,
                method_id: #vox::MethodId,
            ) -> Option<&'static #vox::facet::Shape> {
                #(#response_wire_shape_arms)*
                None
            }

            async fn handle(&self, call: #vox::SelfRef<#vox::RequestCall<'static>>, reply: R, schemas: ::std::sync::Arc<#vox::SchemaRecvTracker>) {
                #dispatch_body
            }
        }

        impl<H> #vox::ConnectionAcceptor for #dispatcher_name<H>
        where
            H: #trait_name,
        {
            fn accept(
                &self,
                _conn_id: #vox::ConnectionId,
                peer_settings: &#vox::ConnectionSettings,
                _metadata: &[#vox::MetadataEntry],
            ) -> ::core::result::Result<#vox::AcceptedConnection, #vox::Metadata<'static>> {
                Ok(#vox::AcceptedConnection {
                    settings: #vox::ConnectionSettings {
                        parity: peer_settings.parity.other(),
                        max_concurrent_requests: peer_settings.max_concurrent_requests,
                    },
                    metadata: vec![],
                    setup: #vox::ConnectionSetup::Handler(Box::new(self.clone())),
                })
            }
        }
    }
}

fn generate_dispatch_arm(
    method: &ServiceMethod,
    method_index: usize,
    vox: &TokenStream2,
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

    let args_let = quote! { let args: #args_tuple_type };

    let return_type = method.return_type();
    let (ok_ty_ref, err_ty_ref) = method_ok_and_err_types(&return_type);
    let ok_has_vox_lifetime = ok_ty_ref.has_named_lifetime("vox");
    let is_fallible = return_type.as_result().is_some();
    let ok_ty = ok_ty_ref.to_token_stream();
    // For the response Shape expression, we need 'static (Shape is always 'static).
    // Replace 'vox with 'static so the Shape reference is valid in the dispatch scope.
    let ok_ty_dispatch: proc_macro2::TokenStream = ok_ty
        .to_string()
        .replace("'vox", "'static")
        .parse()
        .expect("ok_ty_dispatch parse");
    let err_ty = err_ty_ref
        .map(Type::to_token_stream)
        .unwrap_or_else(|| quote! { ::core::convert::Infallible });

    let context_setup = {
        quote! {
            let extensions = #vox::Extensions::new();
            let context = #vox::RequestContext::with_transport(
                #descriptor_fn_name().methods[#idx],
                &call.metadata,
                reply.request_id(),
                reply.connection_id(),
                &extensions,
            );
            if !self.middlewares.is_empty() {
                for middleware in &self.middlewares {
                    middleware
                        .pre(#vox::ServerRequest::new(context, #vox::Peek::new(&args)))
                        .await;
                }
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

    let invoke_and_reply = if ok_has_vox_lifetime {
        quote! {
            let (reply, outcome_handle) = #vox::observe_reply(
                reply,
                #vox::ServerResponseContext::new(
                    context.method(),
                    context.request_id(),
                    context.connection_id(),
                    context.extensions().clone(),
                ),
                self.middlewares.clone(),
            );
            let sink_call = #vox::SinkCall::new(reply);
            self.handler.#method_fn(#(#borrowed_handler_args),*).await;
            if !self.middlewares.is_empty() {
                self.run_post_hooks(&context, outcome_handle.outcome()).await;
            }
        }
    } else if is_fallible {
        quote! {
            let (reply, outcome_handle) = #vox::observe_reply(
                reply,
                #vox::ServerResponseContext::new(
                    context.method(),
                    context.request_id(),
                    context.connection_id(),
                    context.extensions().clone(),
                ),
                self.middlewares.clone(),
            );
            let result = self.handler.#method_fn(#(#plain_handler_args),*).await;
            let sink_call = #vox::SinkCall::new(reply);
            #vox::Call::<'_, #ok_ty, #err_ty>::reply(sink_call, result).await;
            if !self.middlewares.is_empty() {
                self.run_post_hooks(&context, outcome_handle.outcome()).await;
            }
        }
    } else {
        quote! {
            let (reply, outcome_handle) = #vox::observe_reply(
                reply,
                #vox::ServerResponseContext::new(
                    context.method(),
                    context.request_id(),
                    context.connection_id(),
                    context.extensions().clone(),
                ),
                self.middlewares.clone(),
            );
            let value = self.handler.#method_fn(#(#plain_handler_args),*).await;
            let sink_call = #vox::SinkCall::new(reply);
            #vox::Call::<'_, #ok_ty, #err_ty>::ok(sink_call, value).await;
            if !self.middlewares.is_empty() {
                self.run_post_hooks(&context, outcome_handle.outcome()).await;
            }
        }
    };

    quote! {
        if method_id == #descriptor_fn_name().methods[#idx].id {
            // Channel binding happens during deserialization via thread-local binder.
            let deser_result = if let Some(binder) = reply.channel_binder() {
                #vox::with_channel_binder(binder, || {
                    #vox::schema_deser::schema_deserialize_args_borrowed::<#args_tuple_type>(
                        args_bytes,
                        method_id,
                        &schemas,
                    )
                })
            } else {
                #vox::schema_deser::schema_deserialize_args_borrowed::<#args_tuple_type>(
                    args_bytes,
                    method_id,
                    &schemas,
                )
            };
            #args_let = match deser_result {
                Ok(v) => v,
                Err(e) => {
                    ::std::eprintln!(
                        "[rpc] dispatch args decode failed: method={:?} error={}",
                        method_id,
                        e
                    );
                    reply
                        .send_typed_error::<#ok_ty_dispatch, ::core::convert::Infallible>(
                            #vox::VoxError::<::core::convert::Infallible>::InvalidPayload(e.to_string())
                        )
                        .await;
                    return;
                }
            };
            #context_setup
            #destructure
            #invoke_and_reply
            return;
        }
    }
}

// ============================================================================
// Client Generation
// ============================================================================

// r[impl rpc.caller]
fn generate_client(parsed: &ServiceTrait, vox: &TokenStream2) -> TokenStream2 {
    let client_name = format_ident!("{}Client", parsed.name());
    let descriptor_fn_name = format_ident!("{}_service_descriptor", parsed.name().to_snake_case());
    let service_name = parsed.name();
    let service_name_str = service_name.to_string();

    let client_doc = format!(
        "Client for the `{service_name}` service.\n\n\
        Stores a [`Caller`]({vox}::Caller) and an optional [`SessionHandle`]({vox}::SessionHandle) as public fields.",
    );

    let client_methods: Vec<TokenStream2> = parsed
        .methods()
        .enumerate()
        .map(|(i, m)| generate_client_method(m, i, &descriptor_fn_name, vox))
        .collect();

    quote! {
        #[doc = #client_doc]
        #[must_use = "Dropping this client may close the connection if it is the last caller."]
        #[derive(Clone)]
        pub struct #client_name {
            /// The underlying caller for making RPC calls.
            pub caller: #vox::Caller,
            /// The session handle, if this client is on a root connection.
            pub session: Option<#vox::SessionHandle>,
        }

        impl #client_name {
            /// Create a new client wrapping the given caller.
            pub fn new(caller: #vox::Caller) -> Self {
                Self {
                    caller,
                    session: None,
                }
            }

            /// Append a client middleware to this client.
            pub fn with_middleware(self, middleware: impl #vox::ClientMiddleware) -> Self {
                Self {
                    caller: self
                        .caller
                        .with_middleware(#descriptor_fn_name(), middleware),
                    session: self.session,
                }
            }

            #(#client_methods)*
        }

        impl #vox::FromVoxSession for #client_name {
            const SERVICE_NAME: Option<&'static str> = Some(#service_name_str);

            fn from_vox_session(
                caller: #vox::Caller,
                session: Option<#vox::SessionHandle>,
            ) -> Self {
                Self {
                    caller,
                    session,
                }
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
    vox: &TokenStream2,
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
    let tx_arg_indices: Vec<proc_macro2::Literal> = method
        .args()
        .enumerate()
        .filter(|(_index, arg)| type_is_tx(&arg.ty))
        .map(|(index, _arg)| proc_macro2::Literal::usize_unsuffixed(index))
        .collect();
    let channel_retry_mode = if method.args().any(|arg| arg.ty.contains_channel()) {
        if method.is_idem() {
            quote! { #vox::ChannelRetryMode::Idem }
        } else {
            quote! { #vox::ChannelRetryMode::NonIdem }
        }
    } else {
        quote! { #vox::ChannelRetryMode::None }
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
    let ok_uses_vox_lifetime = ok_type_for_lifetimes.has_named_lifetime("vox");
    let (ok_ty_decode, err_ty, client_return) = if let Some((ok, err)) = return_type.as_result() {
        let ok_t = ok.to_token_stream();
        let ok_t_static = to_static_type_tokens(ok);
        let err_t = err.to_token_stream();
        (
            if ok_uses_vox_lifetime {
                ok_t_static.clone()
            } else {
                ok_t.clone()
            },
            err_t.clone(),
            if ok_uses_vox_lifetime {
                quote! { Result<#vox::SelfRef<#ok_t_static>, #vox::VoxError<#err_t>> }
            } else {
                quote! { Result<#ok_t, #vox::VoxError<#err_t>> }
            },
        )
    } else {
        let t = return_type.to_token_stream();
        let t_static = to_static_type_tokens(&return_type);
        (
            if ok_uses_vox_lifetime {
                t_static.clone()
            } else {
                t.clone()
            },
            quote! { ::core::convert::Infallible },
            if ok_uses_vox_lifetime {
                quote! { Result<#vox::SelfRef<#t_static>, #vox::VoxError> }
            } else {
                quote! { Result<#t, #vox::VoxError> }
            },
        )
    };

    let args_binding = quote! { let args = #args_tuple; };
    let finish_retry_bindings = if tx_arg_indices.is_empty() {
        quote! {}
    } else {
        quote! { #( args.#tx_arg_indices.finish_retry_binding(); )* }
    };

    if ok_uses_vox_lifetime {
        quote! {
            #method_doc
            pub async fn #method_name(&self, #(#params),*) -> #client_return {
                let method_id = #descriptor_fn_name().methods[#idx].id;
                #args_binding
                let mut metadata = Default::default();
                #vox::ensure_channel_retry_mode(&mut metadata, #channel_retry_mode);
                let req = #vox::RequestCall {
                    method_id,
                    args: #vox::Payload::outgoing(&args),
                    metadata,
                    schemas: Default::default(),
                };
                let with_tracker = match self.caller.call(req).await {
                    Ok(with_tracker) => with_tracker,
                    Err(e) => {
                        #finish_retry_bindings
                        return Err(match e {
                            #vox::VoxError::UnknownMethod => #vox::VoxError::<#err_ty>::UnknownMethod,
                            #vox::VoxError::InvalidPayload(msg) => #vox::VoxError::<#err_ty>::InvalidPayload(msg),
                            #vox::VoxError::Cancelled => #vox::VoxError::<#err_ty>::Cancelled,
                            #vox::VoxError::ConnectionClosed => #vox::VoxError::<#err_ty>::ConnectionClosed,
                            #vox::VoxError::SessionShutdown => #vox::VoxError::<#err_ty>::SessionShutdown,
                            #vox::VoxError::SendFailed => #vox::VoxError::<#err_ty>::SendFailed,
                            #vox::VoxError::Indeterminate => #vox::VoxError::<#err_ty>::Indeterminate,
                            #vox::VoxError::User(never) => match never {},
                        });
                    }
                };
                let #vox::WithTracker { value: response, tracker: schema_tracker } = with_tracker;
                response.try_repack(|resp, _bytes| {
                    let ret_bytes = match &resp.ret {
                        #vox::Payload::PostcardBytes(bytes) => bytes,
                        _ => return Err(#vox::VoxError::<#err_ty>::InvalidPayload("response not PostcardBytes".into())),
                    };
                    let result: Result<#ok_ty_decode, #vox::VoxError<#err_ty>> =
                        #vox::schema_deser::schema_deserialize_response_borrowed::<Result<#ok_ty_decode, #vox::VoxError<#err_ty>>>(ret_bytes, method_id, &schema_tracker)
                            .map_err(|e| {
                                #finish_retry_bindings
                                #vox::VoxError::<#err_ty>::InvalidPayload(e.to_string())
                            })?;
                    match result {
                        Ok(ret) => Ok(ret),
                        Err(err) => {
                            #finish_retry_bindings
                            Err(err)
                        }
                    }
                })
            }
        }
    } else {
        quote! {
            #method_doc
            pub async fn #method_name(&self, #(#params),*) -> #client_return {
                let method_id = #descriptor_fn_name().methods[#idx].id;
                #args_binding
                let mut metadata = Default::default();
                #vox::ensure_channel_retry_mode(&mut metadata, #channel_retry_mode);
                let req = #vox::RequestCall {
                    method_id,
                    args: #vox::Payload::outgoing(&args),
                    metadata,
                    schemas: Default::default(),
                };
                let with_tracker = match self.caller.call(req).await {
                    Ok(with_tracker) => with_tracker,
                    Err(e) => {
                        #finish_retry_bindings
                        return Err(match e {
                            #vox::VoxError::UnknownMethod => #vox::VoxError::<#err_ty>::UnknownMethod,
                            #vox::VoxError::InvalidPayload(msg) => #vox::VoxError::<#err_ty>::InvalidPayload(msg),
                            #vox::VoxError::Cancelled => #vox::VoxError::<#err_ty>::Cancelled,
                            #vox::VoxError::ConnectionClosed => #vox::VoxError::<#err_ty>::ConnectionClosed,
                            #vox::VoxError::SessionShutdown => #vox::VoxError::<#err_ty>::SessionShutdown,
                            #vox::VoxError::SendFailed => #vox::VoxError::<#err_ty>::SendFailed,
                            #vox::VoxError::Indeterminate => #vox::VoxError::<#err_ty>::Indeterminate,
                            #vox::VoxError::User(never) => match never {},
                        });
                    }
                };
                let #vox::WithTracker { value: response, tracker: schema_tracker } = with_tracker;
                let ret_bytes = match &response.ret {
                    #vox::Payload::PostcardBytes(bytes) => bytes,
                    _ => return Err(#vox::VoxError::<#err_ty>::InvalidPayload("response not PostcardBytes".into())),
                };
                let result: Result<#ok_ty_decode, #vox::VoxError<#err_ty>> =
                    #vox::schema_deser::schema_deserialize_response::<Result<#ok_ty_decode, #vox::VoxError<#err_ty>>>(
                        ret_bytes,
                        method_id,
                        &schema_tracker,
                    )
                    .map_err(|e| {
                        #finish_retry_bindings
                        #vox::VoxError::<#err_ty>::InvalidPayload(e.to_string())
                    })?;
                match result {
                    Ok(ret) => Ok(ret),
                    Err(err) => {
                        #finish_retry_bindings
                        Err(err)
                    }
                }
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
        let parsed = vox_macros_parse::parse_trait(&input).unwrap();
        let vox = quote! { ::vox };
        let ts = crate::generate_service(&parsed, &vox).unwrap();
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
                #[vox::context]
                async fn record(&self, payload: String) -> &'vox str;

                async fn ping(&self) -> u64;
            }
        }));
    }

    #[test]
    fn method_retry_helper_attributes() {
        assert_snapshot!(generate(quote! {
            trait Billing {
                #[vox(idem)]
                async fn get_balance(&self, account: String) -> u64;

                #[vox(persist)]
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
        let parsed = vox_macros_parse::parse_trait(&quote! {
            trait Streamer { async fn stream(&self) -> Rx<i32>; }
        })
        .unwrap();
        let vox = quote! { ::vox };
        let err = crate::generate_service(&parsed, &vox).unwrap_err();
        assert_eq!(
            err.message,
            "method `stream` has Channel (Tx/Rx) in return type - channels are only allowed in method arguments"
        );
    }

    #[test]
    fn rejects_non_async_methods() {
        let parsed = vox_macros_parse::parse_trait(&quote! {
            trait Svc { fn ping(&self) -> u32; }
        })
        .unwrap();
        let vox = quote! { ::vox };
        let err = crate::generate_service(&parsed, &vox).unwrap_err();
        assert_eq!(
            err.message,
            "method `ping` must be declared `async fn` in a #[vox::service] trait"
        );
    }

    #[test]
    fn rejects_mut_ref_receiver() {
        let parsed = vox_macros_parse::parse_trait(&quote! {
            trait Svc { async fn ping(&mut self) -> u32; }
        })
        .unwrap();
        let vox = quote! { ::vox };
        let err = crate::generate_service(&parsed, &vox).unwrap_err();
        assert_eq!(
            err.message,
            "method `ping` receiver must be `&self`; `&mut self` is not supported in #[vox::service] traits"
        );
    }

    #[test]
    fn rejects_value_receiver() {
        let parsed = vox_macros_parse::parse_trait(&quote! {
            trait Svc { async fn ping(self) -> u32; }
        })
        .unwrap();
        let vox = quote! { ::vox };
        let err = crate::generate_service(&parsed, &vox).unwrap_err();
        assert_eq!(
            err.message,
            "method `ping` receiver must be `&self`; `self` is not supported in #[vox::service] traits"
        );
    }

    #[test]
    fn rejects_mut_value_receiver() {
        let parsed = vox_macros_parse::parse_trait(&quote! {
            trait Svc { async fn ping(mut self) -> u32; }
        })
        .unwrap();
        let vox = quote! { ::vox };
        let err = crate::generate_service(&parsed, &vox).unwrap_err();
        assert_eq!(
            err.message,
            "method `ping` receiver must be `&self`; `mut self` is not supported in #[vox::service] traits"
        );
    }

    #[test]
    fn rejects_typed_self_receiver() {
        let parsed = vox_macros_parse::parse_trait(&quote! {
            trait Svc { async fn ping(self: Box<Self>) -> u32; }
        })
        .unwrap();
        let vox = quote! { ::vox };
        let err = crate::generate_service(&parsed, &vox).unwrap_err();
        assert_eq!(
            err.message,
            "method `ping` receiver must be `&self`; typed receivers like `self: Type` are not supported in #[vox::service] traits"
        );
    }

    #[test]
    fn rejects_persist_methods_with_channel_arguments() {
        let parsed = vox_macros_parse::parse_trait(&quote! {
            trait Streamer {
                #[vox(persist)]
                async fn stream(&self, output: Tx<i32>) -> u64;
            }
        })
        .unwrap();
        let vox = quote! { ::vox };
        let err = crate::generate_service(&parsed, &vox).unwrap_err();
        assert_eq!(
            err.message,
            "method `stream` declares `#[vox(persist)]` but has Channel (Tx/Rx) arguments - persist methods cannot carry channels"
        );
    }

    #[test]
    fn rejects_non_vox_return_lifetime() {
        let parsed = vox_macros_parse::parse_trait(&quote! {
            trait Svc { async fn bad(&self) -> &'a str; }
        })
        .unwrap();
        let vox = quote! { ::vox };
        let err = crate::generate_service(&parsed, &vox).unwrap_err();
        assert_eq!(
            err.message,
            "method `bad` return type may only use lifetime `'vox` for borrowed response data"
        );
    }

    #[test]
    fn rejects_elided_return_lifetime() {
        let parsed = vox_macros_parse::parse_trait(&quote! {
            trait Svc { async fn bad(&self) -> &str; }
        })
        .unwrap();
        let vox = quote! { ::vox };
        let err = crate::generate_service(&parsed, &vox).unwrap_err();
        assert_eq!(
            err.message,
            "method `bad` return type uses an elided reference lifetime; use explicit `'vox` (for example `&'vox str`)"
        );
    }

    #[test]
    fn rejects_borrowed_error_type() {
        let parsed = vox_macros_parse::parse_trait(&quote! {
            trait Svc { async fn bad(&self) -> Result<u32, &'vox str>; }
        })
        .unwrap();
        let vox = quote! { ::vox };
        let err = crate::generate_service(&parsed, &vox).unwrap_err();
        assert_eq!(
            err.message,
            "method `bad` error type must be owned (no lifetimes), because client errors are not wrapped in SelfRef"
        );
    }

    #[test]
    fn borrowed_vox_return() {
        assert_snapshot!(generate(quote! {
            trait Hasher { async fn hash(&self, payload: String) -> &'vox str; }
        }));
    }

    #[test]
    fn borrowed_vox_return_call_style() {
        assert_snapshot!(generate(quote! {
            trait Hasher { async fn hash(&self, payload: String) -> &'vox str; }
        }));
    }

    #[test]
    fn borrowed_vox_cow_return() {
        assert_snapshot!(generate(quote! {
            trait TextSvc {
                async fn normalize(&self, input: String) -> ::std::borrow::Cow<'vox, str>;
            }
        }));
    }

    #[test]
    fn borrowed_return_mixed_with_borrowed_args_and_channels_compiles_to_expected_shapes() {
        assert_snapshot!(generate(quote! {
            trait WordLab {
                async fn is_short(&self, word: &str) -> bool;
                async fn classify(&self, word: String) -> &'vox str;
                async fn transform(&self, prefix: &str, input: Rx<String>, output: Tx<String>) -> u32;
            }
        }));
    }
}
