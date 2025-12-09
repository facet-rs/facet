//! rapace-macros: Proc macros for rapace RPC.
//!
//! Provides `#[rapace::service]` which generates:
//! - Client stubs with async methods
//! - Server dispatch by method_id

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    parse_macro_input, FnArg, GenericArgument, Ident, ItemTrait, Pat, PathArguments, ReturnType,
    TraitItem, TraitItemFn, Type, TypePath,
};

/// Generates RPC client and server from a trait definition.
///
/// # Example
///
/// ```ignore
/// #[rapace::service]
/// trait Calculator {
///     async fn add(&self, a: i32, b: i32) -> i32;
/// }
///
/// // Generated:
/// // - CalculatorClient<T: Transport> with async fn add(&self, a: i32, b: i32) -> Result<i32, RpcError>
/// // - CalculatorServer<S: Calculator> with dispatch method
/// ```
///
/// # Streaming RPCs
///
/// For server-streaming, return `Streaming<T>`:
///
/// ```ignore
/// use rapace_core::Streaming;
///
/// #[rapace::service]
/// trait RangeService {
///     async fn range(&self, n: u32) -> Streaming<u32>;
/// }
/// ```
///
/// The client method becomes:
/// `async fn range(&self, n: u32) -> Result<Streaming<u32>, RpcError>`
#[proc_macro_attribute]
pub fn service(_attr: TokenStream, item: TokenStream) -> TokenStream {
    let input = parse_macro_input!(item as ItemTrait);

    match generate_service(&input) {
        Ok(tokens) => tokens.into(),
        Err(e) => e.to_compile_error().into(),
    }
}

fn generate_service(input: &ItemTrait) -> syn::Result<TokenStream2> {
    let trait_name = &input.ident;
    let trait_name_str = trait_name.to_string();
    let vis = &input.vis;

    // Capture trait doc comments
    let trait_doc = collect_doc(&input.attrs);

    let client_name = format_ident!("{}Client", trait_name);
    let server_name = format_ident!("{}Server", trait_name);

    // Collect method information
    let methods: Vec<MethodInfo> = input
        .items
        .iter()
        .filter_map(|item| {
            if let TraitItem::Fn(method) = item {
                Some(parse_method(method))
            } else {
                None
            }
        })
        .collect::<syn::Result<Vec<_>>>()?;

    // Generate client methods (with hardcoded IDs for backwards compatibility)
    let client_methods_hardcoded = methods.iter().enumerate().map(|(idx, m)| {
        let method_id = (idx + 1) as u32; // method_id 0 is reserved for control
        generate_client_method(m, method_id)
    });

    // Generate client methods that use stored method IDs from registry
    let client_methods_registry = methods
        .iter()
        .enumerate()
        .map(|(idx, m)| generate_client_method_registry(m, idx));

    // Generate server dispatch arms (for unary and error fallback)
    let dispatch_arms = methods.iter().enumerate().map(|(idx, m)| {
        let method_id = (idx + 1) as u32;
        generate_dispatch_arm(m, method_id)
    });

    // Generate streaming dispatch arms
    let streaming_dispatch_arms = methods.iter().enumerate().map(|(idx, m)| {
        let method_id = (idx + 1) as u32;
        generate_streaming_dispatch_arm(m, method_id)
    });

    // Generate method ID constants
    let method_id_consts = methods.iter().enumerate().map(|(idx, m)| {
        let method_id = (idx + 1) as u32;
        let const_name = format_ident!("METHOD_ID_{}", m.name.to_string().to_uppercase());
        quote! {
            pub const #const_name: u32 = #method_id;
        }
    });

    // Generate registry registration code
    let register_fn = generate_register_fn(&trait_name_str, &trait_doc, &methods);

    // Generate registry-aware client struct and constructor
    let registry_client_name = format_ident!("{}RegistryClient", trait_name);
    let method_id_fields: Vec<_> = methods
        .iter()
        .map(|m| {
            let field_name = format_ident!("{}_method_id", m.name);
            quote! { #field_name: u32 }
        })
        .collect();
    let method_id_lookups: Vec<_> = methods
        .iter()
        .map(|m| {
            let field_name = format_ident!("{}_method_id", m.name);
            let method_name = m.name.to_string();
            quote! {
                #field_name: registry.resolve_method_id(#trait_name_str, #method_name)
                    .expect(concat!("method ", #method_name, " not found in registry"))
                    .0
            }
        })
        .collect();

    let mod_name = format_ident!("{}_methods", trait_name.to_string().to_lowercase());

    let expanded = quote! {
        // Keep the original trait
        #input

        /// Method ID constants for this service.
        #vis mod #mod_name {
            #(#method_id_consts)*

            #register_fn
        }

        /// Client stub for the #trait_name service.
        ///
        /// This client uses hardcoded method IDs (1, 2, ...) and is suitable
        /// for simple single-service use cases. For multi-service scenarios
        /// where method IDs must be globally unique, use [`#registry_client_name`] instead.
        #vis struct #client_name<T> {
            transport: ::std::sync::Arc<T>,
            next_msg_id: ::std::sync::atomic::AtomicU64,
            next_channel_id: ::std::sync::atomic::AtomicU32,
        }

        impl<T: ::rapace_core::Transport + 'static> #client_name<T> {
            /// Create a new client with the given transport.
            ///
            /// Uses hardcoded method IDs (1, 2, ...). For registry-based method IDs,
            /// use [`#registry_client_name::new`] instead.
            pub fn new(transport: ::std::sync::Arc<T>) -> Self {
                Self {
                    transport,
                    next_msg_id: ::std::sync::atomic::AtomicU64::new(1),
                    next_channel_id: ::std::sync::atomic::AtomicU32::new(1),
                }
            }

            fn next_msg_id(&self) -> u64 {
                self.next_msg_id.fetch_add(1, ::std::sync::atomic::Ordering::Relaxed)
            }

            fn next_channel_id(&self) -> u32 {
                self.next_channel_id.fetch_add(1, ::std::sync::atomic::Ordering::Relaxed)
            }

            /// Parse error payload into RpcError.
            fn parse_error_payload(payload: &[u8]) -> ::rapace_core::RpcError {
                if payload.len() < 8 {
                    return ::rapace_core::RpcError::Status {
                        code: ::rapace_core::ErrorCode::Internal,
                        message: "malformed error response".into(),
                    };
                }

                let error_code = u32::from_le_bytes([
                    payload[0], payload[1], payload[2], payload[3]
                ]);
                let message_len = u32::from_le_bytes([
                    payload[4], payload[5], payload[6], payload[7]
                ]) as usize;

                if payload.len() < 8 + message_len {
                    return ::rapace_core::RpcError::Status {
                        code: ::rapace_core::ErrorCode::Internal,
                        message: "malformed error response".into(),
                    };
                }

                let code = ::rapace_core::ErrorCode::from_u32(error_code)
                    .unwrap_or(::rapace_core::ErrorCode::Internal);
                let message = ::std::string::String::from_utf8_lossy(
                    &payload[8..8 + message_len]
                ).into_owned();

                ::rapace_core::RpcError::Status { code, message }
            }

            #(#client_methods_hardcoded)*
        }

        /// Registry-aware client stub for the #trait_name service.
        ///
        /// This client looks up method IDs from a [`ServiceRegistry`] at construction time,
        /// ensuring that method IDs are globally unique across all registered services.
        #vis struct #registry_client_name<T> {
            transport: ::std::sync::Arc<T>,
            next_msg_id: ::std::sync::atomic::AtomicU64,
            next_channel_id: ::std::sync::atomic::AtomicU32,
            #(#method_id_fields,)*
        }

        impl<T: ::rapace_core::Transport + 'static> #registry_client_name<T> {
            /// Create a new registry-aware client.
            ///
            /// Looks up method IDs from the registry. The service must be registered
            /// in the registry before calling this constructor.
            ///
            /// # Panics
            ///
            /// Panics if the service or any of its methods are not found in the registry.
            pub fn new(transport: ::std::sync::Arc<T>, registry: &::rapace_registry::ServiceRegistry) -> Self {
                Self {
                    transport,
                    next_msg_id: ::std::sync::atomic::AtomicU64::new(1),
                    next_channel_id: ::std::sync::atomic::AtomicU32::new(1),
                    #(#method_id_lookups,)*
                }
            }

            fn next_msg_id(&self) -> u64 {
                self.next_msg_id.fetch_add(1, ::std::sync::atomic::Ordering::Relaxed)
            }

            fn next_channel_id(&self) -> u32 {
                self.next_channel_id.fetch_add(1, ::std::sync::atomic::Ordering::Relaxed)
            }

            /// Parse error payload into RpcError.
            fn parse_error_payload(payload: &[u8]) -> ::rapace_core::RpcError {
                if payload.len() < 8 {
                    return ::rapace_core::RpcError::Status {
                        code: ::rapace_core::ErrorCode::Internal,
                        message: "malformed error response".into(),
                    };
                }

                let error_code = u32::from_le_bytes([
                    payload[0], payload[1], payload[2], payload[3]
                ]);
                let message_len = u32::from_le_bytes([
                    payload[4], payload[5], payload[6], payload[7]
                ]) as usize;

                if payload.len() < 8 + message_len {
                    return ::rapace_core::RpcError::Status {
                        code: ::rapace_core::ErrorCode::Internal,
                        message: "malformed error response".into(),
                    };
                }

                let code = ::rapace_core::ErrorCode::from_u32(error_code)
                    .unwrap_or(::rapace_core::ErrorCode::Internal);
                let message = ::std::string::String::from_utf8_lossy(
                    &payload[8..8 + message_len]
                ).into_owned();

                ::rapace_core::RpcError::Status { code, message }
            }

            #(#client_methods_registry)*
        }

        /// Server dispatcher for the #trait_name service.
        #vis struct #server_name<S> {
            service: S,
        }

        impl<S: #trait_name + Send + Sync + 'static> #server_name<S> {
            /// Create a new server with the given service implementation.
            pub fn new(service: S) -> Self {
                Self { service }
            }

            /// Serve requests from the transport until the connection closes.
            ///
            /// This is the main server loop. It reads frames from the transport,
            /// dispatches them to the appropriate method, and sends responses.
            ///
            /// # Example
            ///
            /// ```ignore
            /// let server = CalculatorServer::new(CalculatorImpl);
            /// server.serve(transport).await?;
            /// ```
            pub async fn serve<T: ::rapace_core::Transport + 'static>(
                self,
                transport: ::std::sync::Arc<T>,
            ) -> ::std::result::Result<(), ::rapace_core::RpcError> {
                loop {
                    // Receive next request frame
                    let request = match transport.recv_frame().await {
                        Ok(frame) => frame,
                        Err(::rapace_core::TransportError::Closed) => {
                            // Connection closed gracefully
                            return Ok(());
                        }
                        Err(e) => {
                            return Err(::rapace_core::RpcError::Transport(e));
                        }
                    };

                    // Skip non-data frames (control frames, etc.)
                    if !request.desc.flags.contains(::rapace_core::FrameFlags::DATA) {
                        continue;
                    }

                    // Dispatch the request
                    if let Err(e) = self.dispatch_streaming(
                        request.desc.method_id,
                        request.payload,
                        transport.as_ref(),
                    ).await {
                        // Send error response
                        let mut desc = ::rapace_core::MsgDescHot::new();
                        desc.channel_id = request.desc.channel_id;
                        desc.flags = ::rapace_core::FrameFlags::ERROR | ::rapace_core::FrameFlags::EOS;

                        // Encode error: [code: u32 LE][message_len: u32 LE][message bytes]
                        let (code, message): (u32, ::std::string::String) = match &e {
                            ::rapace_core::RpcError::Status { code, message } => (*code as u32, message.clone()),
                            ::rapace_core::RpcError::Transport(_) => (::rapace_core::ErrorCode::Internal as u32, "transport error".into()),
                            ::rapace_core::RpcError::Cancelled => (::rapace_core::ErrorCode::Cancelled as u32, "cancelled".into()),
                            ::rapace_core::RpcError::DeadlineExceeded => (::rapace_core::ErrorCode::DeadlineExceeded as u32, "deadline exceeded".into()),
                        };
                        let mut err_bytes = ::std::vec::Vec::with_capacity(8 + message.len());
                        err_bytes.extend_from_slice(&code.to_le_bytes());
                        err_bytes.extend_from_slice(&(message.len() as u32).to_le_bytes());
                        err_bytes.extend_from_slice(message.as_bytes());

                        let frame = ::rapace_core::Frame::with_payload(desc, err_bytes);
                        let _ = transport.send_frame(&frame).await;
                    }
                }
            }

            /// Serve a single request from the transport.
            ///
            /// This is useful for testing or when you want to handle each request
            /// individually.
            pub async fn serve_one<T: ::rapace_core::Transport + 'static>(
                &self,
                transport: &T,
            ) -> ::std::result::Result<(), ::rapace_core::RpcError> {
                // Receive next request frame
                let request = transport.recv_frame().await
                    .map_err(::rapace_core::RpcError::Transport)?;

                // Skip non-data frames
                if !request.desc.flags.contains(::rapace_core::FrameFlags::DATA) {
                    return Ok(());
                }

                // Dispatch the request
                self.dispatch_streaming(
                    request.desc.method_id,
                    request.payload,
                    transport,
                ).await
            }

            /// Dispatch a request frame to the appropriate method.
            ///
            /// Returns a response frame on success for unary methods.
            /// For streaming methods, use `dispatch_streaming` instead.
            pub async fn dispatch(
                &self,
                method_id: u32,
                request_payload: &[u8],
            ) -> ::std::result::Result<::rapace_core::Frame, ::rapace_core::RpcError> {
                match method_id {
                    #(#dispatch_arms)*
                    _ => Err(::rapace_core::RpcError::Status {
                        code: ::rapace_core::ErrorCode::Unimplemented,
                        message: ::std::format!("unknown method_id: {}", method_id),
                    }),
                }
            }

            /// Dispatch a streaming request to the appropriate method.
            ///
            /// The method sends frames via the provided transport.
            pub async fn dispatch_streaming<T: ::rapace_core::Transport + 'static>(
                &self,
                method_id: u32,
                request_payload: &[u8],
                transport: &T,
            ) -> ::std::result::Result<(), ::rapace_core::RpcError> {
                match method_id {
                    #(#streaming_dispatch_arms)*
                    _ => Err(::rapace_core::RpcError::Status {
                        code: ::rapace_core::ErrorCode::Unimplemented,
                        message: ::std::format!("unknown method_id: {}", method_id),
                    }),
                }
            }
        }
    };

    Ok(expanded)
}

/// Method kind: unary or server-streaming.
#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)]
enum MethodKind {
    /// Unary RPC: single request, single response.
    Unary,
    /// Server-streaming: single request, returns Streaming<T>.
    ServerStreaming {
        /// The type T in Streaming<T>.
        item_type: Type,
    },
}

struct MethodInfo {
    name: Ident,
    args: Vec<(Ident, Type)>, // (name, type) pairs, excluding &self
    return_type: Type,
    kind: MethodKind,
    doc: String,
}

/// Extract doc comments from attributes.
///
/// Collects all `#[doc = "..."]` attributes (which are what `///` comments
/// become after parsing) and joins them with newlines.
fn collect_doc(attrs: &[syn::Attribute]) -> String {
    attrs
        .iter()
        .filter_map(|attr| {
            // Check if this is a #[doc = "..."] attribute
            if !attr.path().is_ident("doc") {
                return None;
            }

            // Parse the meta to get the string value
            if let syn::Meta::NameValue(nv) = &attr.meta {
                if let syn::Expr::Lit(syn::ExprLit {
                    lit: syn::Lit::Str(s),
                    ..
                }) = &nv.value
                {
                    return Some(s.value());
                }
            }
            None
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn parse_method(method: &TraitItemFn) -> syn::Result<MethodInfo> {
    let sig = &method.sig;
    let name = sig.ident.clone();

    // Capture doc comments
    let doc = collect_doc(&method.attrs);

    // Check it's async
    if sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            sig,
            "rapace::service methods must be async",
        ));
    }

    // Parse arguments (skip &self)
    let args: Vec<(Ident, Type)> = sig
        .inputs
        .iter()
        .filter_map(|arg| match arg {
            FnArg::Receiver(_) => None,
            FnArg::Typed(pat_type) => {
                if let Pat::Ident(pat_ident) = &*pat_type.pat {
                    Some((pat_ident.ident.clone(), (*pat_type.ty).clone()))
                } else {
                    None // Skip complex patterns for now
                }
            }
        })
        .collect();

    // Parse return type
    let return_type = match &sig.output {
        ReturnType::Default => syn::parse_quote!(()),
        ReturnType::Type(_, ty) => (**ty).clone(),
    };

    // Check if return type is Streaming<T>
    let kind = if let Some(item_type) = extract_streaming_return_type(&return_type) {
        MethodKind::ServerStreaming { item_type }
    } else {
        MethodKind::Unary
    };

    Ok(MethodInfo {
        name,
        args,
        return_type,
        kind,
        doc,
    })
}

/// Try to extract T from `rapace_core::Streaming<T>` or `Streaming<T>`.
fn extract_streaming_return_type(ty: &Type) -> Option<Type> {
    let Type::Path(TypePath { path, .. }) = ty else {
        return None;
    };

    // Look at the last segment of the path
    let last = path.segments.last()?;

    if last.ident != "Streaming" {
        return None;
    }

    // Expect `Streaming<T>`
    let PathArguments::AngleBracketed(args) = &last.arguments else {
        return None;
    };

    for arg in &args.args {
        if let GenericArgument::Type(item_type) = arg {
            return Some(item_type.clone());
        }
    }

    None
}

fn generate_client_method(method: &MethodInfo, method_id: u32) -> TokenStream2 {
    match &method.kind {
        MethodKind::Unary => generate_client_method_unary(method, method_id),
        MethodKind::ServerStreaming { item_type } => {
            generate_client_method_server_streaming(method, method_id, item_type)
        }
    }
}

fn generate_client_method_registry(method: &MethodInfo, method_index: usize) -> TokenStream2 {
    match &method.kind {
        MethodKind::Unary => generate_client_method_unary_registry(method, method_index),
        MethodKind::ServerStreaming { item_type } => {
            generate_client_method_server_streaming_registry(method, method_index, item_type)
        }
    }
}

fn generate_client_method_unary(method: &MethodInfo, method_id: u32) -> TokenStream2 {
    let name = &method.name;
    let return_type = &method.return_type;

    let arg_names: Vec<_> = method.args.iter().map(|(name, _)| name).collect();
    let arg_types: Vec<_> = method.args.iter().map(|(_, ty)| ty).collect();

    // Generate the argument list for the function signature
    let fn_args = arg_names.iter().zip(arg_types.iter()).map(|(name, ty)| {
        quote! { #name: #ty }
    });

    // For encoding, serialize args as a tuple using facet_postcard
    let encode_expr = if arg_names.is_empty() {
        quote! { ::facet_postcard::to_vec(&()).unwrap() }
    } else if arg_names.len() == 1 {
        let arg = &arg_names[0];
        quote! { ::facet_postcard::to_vec(&#arg).unwrap() }
    } else {
        quote! { ::facet_postcard::to_vec(&(#(#arg_names.clone()),*)).unwrap() }
    };

    quote! {
        /// Call the #name method on the remote service.
        pub async fn #name(&self, #(#fn_args),*) -> ::std::result::Result<#return_type, ::rapace_core::RpcError> {
            use ::rapace_core::{Frame, FrameFlags, MsgDescHot, Transport};

            // Encode request using facet_postcard
            let request_bytes: ::std::vec::Vec<u8> = #encode_expr;

            // Build request descriptor
            let mut desc = MsgDescHot::new();
            desc.msg_id = self.next_msg_id();
            desc.channel_id = self.next_channel_id();
            desc.method_id = #method_id;
            desc.flags = FrameFlags::DATA | FrameFlags::EOS;

            // Create frame
            let frame = if request_bytes.len() <= ::rapace_core::INLINE_PAYLOAD_SIZE {
                Frame::with_inline_payload(desc, &request_bytes)
                    .expect("inline payload should fit")
            } else {
                Frame::with_payload(desc, request_bytes)
            };

            // Send request
            self.transport.send_frame(&frame).await
                .map_err(::rapace_core::RpcError::Transport)?;

            // Receive response
            let response = self.transport.recv_frame().await
                .map_err(::rapace_core::RpcError::Transport)?;

            // Check for error flag
            if response.desc.flags.contains(FrameFlags::ERROR) {
                // Parse error payload: [ErrorCode as u32 LE][message_len as u32 LE][message bytes]
                if response.payload.len() < 8 {
                    return Err(::rapace_core::RpcError::Status {
                        code: ::rapace_core::ErrorCode::Internal,
                        message: "malformed error response".into(),
                    });
                }

                let error_code = u32::from_le_bytes([
                    response.payload[0],
                    response.payload[1],
                    response.payload[2],
                    response.payload[3]
                ]);
                let message_len = u32::from_le_bytes([
                    response.payload[4],
                    response.payload[5],
                    response.payload[6],
                    response.payload[7]
                ]) as usize;

                if response.payload.len() < 8 + message_len {
                    return Err(::rapace_core::RpcError::Status {
                        code: ::rapace_core::ErrorCode::Internal,
                        message: "malformed error response".into(),
                    });
                }

                let code = ::rapace_core::ErrorCode::from_u32(error_code)
                    .unwrap_or(::rapace_core::ErrorCode::Internal);
                let message = ::std::string::String::from_utf8_lossy(
                    &response.payload[8..8 + message_len]
                ).into_owned();

                return Err(::rapace_core::RpcError::Status { code, message });
            }

            // Decode response using facet_postcard
            let result: #return_type = ::facet_postcard::from_bytes(response.payload)
                .map_err(|e| ::rapace_core::RpcError::Status {
                    code: ::rapace_core::ErrorCode::Internal,
                    message: ::std::format!("decode error: {:?}", e),
                })?;

            Ok(result)
        }
    }
}

fn generate_client_method_server_streaming(
    method: &MethodInfo,
    method_id: u32,
    item_type: &Type,
) -> TokenStream2 {
    let name = &method.name;

    let arg_names: Vec<_> = method.args.iter().map(|(name, _)| name).collect();
    let arg_types: Vec<_> = method.args.iter().map(|(_, ty)| ty).collect();

    // Generate the argument list for the function signature
    let fn_args = arg_names.iter().zip(arg_types.iter()).map(|(name, ty)| {
        quote! { #name: #ty }
    });

    // For encoding, serialize args as a tuple using facet_postcard
    let encode_expr = if arg_names.is_empty() {
        quote! { ::facet_postcard::to_vec(&()).unwrap() }
    } else if arg_names.len() == 1 {
        let arg = &arg_names[0];
        quote! { ::facet_postcard::to_vec(&#arg).unwrap() }
    } else {
        quote! { ::facet_postcard::to_vec(&(#(#arg_names.clone()),*)).unwrap() }
    };

    // Return type is Result<Streaming<T>, RpcError>
    quote! {
        /// Call the #name server-streaming method on the remote service.
        ///
        /// Returns a stream of items. The stream ends when EOS is received.
        pub async fn #name(&self, #(#fn_args),*) -> ::std::result::Result<::rapace_core::Streaming<#item_type>, ::rapace_core::RpcError> {
            use ::rapace_core::{Frame, FrameFlags, MsgDescHot, Transport};

            // Encode request using facet_postcard
            let request_bytes: ::std::vec::Vec<u8> = #encode_expr;

            // Build request descriptor
            let mut desc = MsgDescHot::new();
            desc.msg_id = self.next_msg_id();
            desc.channel_id = self.next_channel_id();
            desc.method_id = #method_id;
            desc.flags = FrameFlags::DATA | FrameFlags::EOS; // Request is complete

            // Create frame
            let frame = if request_bytes.len() <= ::rapace_core::INLINE_PAYLOAD_SIZE {
                Frame::with_inline_payload(desc, &request_bytes)
                    .expect("inline payload should fit")
            } else {
                Frame::with_payload(desc, request_bytes)
            };

            // Send request
            self.transport.send_frame(&frame).await
                .map_err(::rapace_core::RpcError::Transport)?;

            // Set up receive channel
            let (tx, rx) = ::tokio::sync::mpsc::channel::<::std::result::Result<#item_type, ::rapace_core::RpcError>>(16);

            // Clone transport for the spawned task
            let transport = ::std::sync::Arc::clone(&self.transport);

            // Spawn task to receive stream items
            ::tokio::spawn(async move {
                loop {
                    let response = match transport.recv_frame().await {
                        Ok(r) => r,
                        Err(e) => {
                            let _ = tx.send(Err(::rapace_core::RpcError::Transport(e))).await;
                            break;
                        }
                    };

                    // Check for error flag
                    if response.desc.flags.contains(FrameFlags::ERROR) {
                        let err = Self::parse_error_payload(response.payload);
                        let _ = tx.send(Err(err)).await;
                        break;
                    }

                    // Check if this is a data frame
                    if response.desc.flags.contains(FrameFlags::DATA) {
                        // Decode the item
                        match ::facet_postcard::from_bytes::<#item_type>(response.payload) {
                            Ok(item) => {
                                if tx.send(Ok(item)).await.is_err() {
                                    break; // Receiver dropped
                                }
                            }
                            Err(e) => {
                                let _ = tx.send(Err(::rapace_core::RpcError::Status {
                                    code: ::rapace_core::ErrorCode::Internal,
                                    message: ::std::format!("decode error: {:?}", e),
                                })).await;
                                break;
                            }
                        }
                    }

                    // Check for EOS - stream is complete
                    if response.desc.flags.contains(FrameFlags::EOS) {
                        break;
                    }
                }
            });

            // Convert receiver to Streaming<T>
            let stream = ::tokio_stream::wrappers::ReceiverStream::new(rx);
            Ok(::std::boxed::Box::pin(stream))
        }
    }
}

fn generate_dispatch_arm(method: &MethodInfo, method_id: u32) -> TokenStream2 {
    match &method.kind {
        MethodKind::Unary => generate_dispatch_arm_unary(method, method_id),
        MethodKind::ServerStreaming { .. } => {
            // Streaming methods are handled by dispatch_streaming, not dispatch
            // For the dispatch() method, return error for streaming methods
            quote! {
                #method_id => {
                    Err(::rapace_core::RpcError::Status {
                        code: ::rapace_core::ErrorCode::Internal,
                        message: "streaming method called via unary dispatch".into(),
                    })
                }
            }
        }
    }
}

fn generate_streaming_dispatch_arm(method: &MethodInfo, method_id: u32) -> TokenStream2 {
    match &method.kind {
        MethodKind::Unary => {
            // For unary methods in streaming dispatch, call the regular dispatch and send the frame
            let name = &method.name;
            let return_type = &method.return_type;
            let arg_names: Vec<_> = method.args.iter().map(|(name, _)| name).collect();
            let arg_types: Vec<_> = method.args.iter().map(|(_, ty)| ty).collect();

            let decode_and_call = if arg_names.is_empty() {
                quote! {
                    let result: #return_type = self.service.#name().await;
                }
            } else if arg_names.len() == 1 {
                let arg = &arg_names[0];
                let ty = &arg_types[0];
                quote! {
                    let #arg: #ty = ::facet_postcard::from_bytes(request_payload)
                        .map_err(|e| ::rapace_core::RpcError::Status {
                            code: ::rapace_core::ErrorCode::InvalidArgument,
                            message: ::std::format!("decode error: {:?}", e),
                        })?;
                    let result: #return_type = self.service.#name(#arg).await;
                }
            } else {
                let tuple_type = quote! { (#(#arg_types),*) };
                quote! {
                    let (#(#arg_names),*): #tuple_type = ::facet_postcard::from_bytes(request_payload)
                        .map_err(|e| ::rapace_core::RpcError::Status {
                            code: ::rapace_core::ErrorCode::InvalidArgument,
                            message: ::std::format!("decode error: {:?}", e),
                        })?;
                    let result: #return_type = self.service.#name(#(#arg_names),*).await;
                }
            };

            quote! {
                #method_id => {
                    #decode_and_call

                    // Encode and send response frame
                    let response_bytes: ::std::vec::Vec<u8> = ::facet_postcard::to_vec(&result)
                        .map_err(|e| ::rapace_core::RpcError::Status {
                            code: ::rapace_core::ErrorCode::Internal,
                            message: ::std::format!("encode error: {:?}", e),
                        })?;

                    let mut desc = ::rapace_core::MsgDescHot::new();
                    desc.flags = ::rapace_core::FrameFlags::DATA | ::rapace_core::FrameFlags::EOS;

                    let frame = if response_bytes.len() <= ::rapace_core::INLINE_PAYLOAD_SIZE {
                        ::rapace_core::Frame::with_inline_payload(desc, &response_bytes)
                            .expect("inline payload should fit")
                    } else {
                        ::rapace_core::Frame::with_payload(desc, response_bytes)
                    };

                    transport.send_frame(&frame).await
                        .map_err(::rapace_core::RpcError::Transport)?;
                    Ok(())
                }
            }
        }
        MethodKind::ServerStreaming { item_type } => {
            generate_streaming_dispatch_arm_server_streaming(method, method_id, item_type)
        }
    }
}

fn generate_streaming_dispatch_arm_server_streaming(
    method: &MethodInfo,
    method_id: u32,
    _item_type: &Type,
) -> TokenStream2 {
    let name = &method.name;
    let arg_names: Vec<_> = method.args.iter().map(|(name, _)| name).collect();
    let arg_types: Vec<_> = method.args.iter().map(|(_, ty)| ty).collect();

    let decode_args = if arg_names.is_empty() {
        quote! {}
    } else if arg_names.len() == 1 {
        let arg = &arg_names[0];
        let ty = &arg_types[0];
        quote! {
            let #arg: #ty = ::facet_postcard::from_bytes(request_payload)
                .map_err(|e| ::rapace_core::RpcError::Status {
                    code: ::rapace_core::ErrorCode::InvalidArgument,
                    message: ::std::format!("decode error: {:?}", e),
                })?;
        }
    } else {
        let tuple_type = quote! { (#(#arg_types),*) };
        quote! {
            let (#(#arg_names),*): #tuple_type = ::facet_postcard::from_bytes(request_payload)
                .map_err(|e| ::rapace_core::RpcError::Status {
                    code: ::rapace_core::ErrorCode::InvalidArgument,
                    message: ::std::format!("decode error: {:?}", e),
                })?;
        }
    };

    let call_args = if arg_names.is_empty() {
        quote! {}
    } else {
        quote! { #(#arg_names),* }
    };

    quote! {
        #method_id => {
            #decode_args

            // Call the service method to get a stream
            let mut stream = self.service.#name(#call_args).await;

            // Iterate over the stream and send frames
            use ::tokio_stream::StreamExt;

            loop {
                match stream.next().await {
                    Some(Ok(item)) => {
                        // Encode item
                        let item_bytes: ::std::vec::Vec<u8> = ::facet_postcard::to_vec(&item)
                            .map_err(|e| ::rapace_core::RpcError::Status {
                                code: ::rapace_core::ErrorCode::Internal,
                                message: ::std::format!("encode error: {:?}", e),
                            })?;

                        // Send DATA frame (not EOS yet)
                        let mut desc = ::rapace_core::MsgDescHot::new();
                        desc.flags = ::rapace_core::FrameFlags::DATA;

                        let frame = if item_bytes.len() <= ::rapace_core::INLINE_PAYLOAD_SIZE {
                            ::rapace_core::Frame::with_inline_payload(desc, &item_bytes)
                                .expect("inline payload should fit")
                        } else {
                            ::rapace_core::Frame::with_payload(desc, item_bytes)
                        };

                        transport.send_frame(&frame).await
                            .map_err(::rapace_core::RpcError::Transport)?;
                    }
                    Some(Err(err)) => {
                        // Send ERROR frame and break
                        let mut desc = ::rapace_core::MsgDescHot::new();
                        desc.flags = ::rapace_core::FrameFlags::ERROR | ::rapace_core::FrameFlags::EOS;

                        // Encode error: [code: u32 LE][message_len: u32 LE][message bytes]
                        let (code, message): (u32, &str) = match &err {
                            ::rapace_core::RpcError::Status { code, message } => (*code as u32, message.as_str()),
                            ::rapace_core::RpcError::Transport(_) => (::rapace_core::ErrorCode::Internal as u32, "transport error"),
                            ::rapace_core::RpcError::Cancelled => (::rapace_core::ErrorCode::Cancelled as u32, "cancelled"),
                            ::rapace_core::RpcError::DeadlineExceeded => (::rapace_core::ErrorCode::DeadlineExceeded as u32, "deadline exceeded"),
                        };
                        let mut err_bytes = Vec::with_capacity(8 + message.len());
                        err_bytes.extend_from_slice(&code.to_le_bytes());
                        err_bytes.extend_from_slice(&(message.len() as u32).to_le_bytes());
                        err_bytes.extend_from_slice(message.as_bytes());

                        let frame = ::rapace_core::Frame::with_payload(desc, err_bytes);
                        transport.send_frame(&frame).await
                            .map_err(::rapace_core::RpcError::Transport)?;
                        return Ok(());
                    }
                    None => {
                        // Stream is complete: send EOS frame
                        let mut desc = ::rapace_core::MsgDescHot::new();
                        desc.flags = ::rapace_core::FrameFlags::EOS;
                        let frame = ::rapace_core::Frame::new(desc);
                        transport.send_frame(&frame).await
                            .map_err(::rapace_core::RpcError::Transport)?;
                        return Ok(());
                    }
                }
            }
        }
    }
}

fn generate_dispatch_arm_unary(method: &MethodInfo, method_id: u32) -> TokenStream2 {
    let name = &method.name;
    let return_type = &method.return_type;
    let arg_names: Vec<_> = method.args.iter().map(|(name, _)| name).collect();
    let arg_types: Vec<_> = method.args.iter().map(|(_, ty)| ty).collect();

    // Generate decode expression for args
    let decode_and_call = if arg_names.is_empty() {
        quote! {
            // No arguments to decode
            let result: #return_type = self.service.#name().await;
        }
    } else if arg_names.len() == 1 {
        let arg = &arg_names[0];
        let ty = &arg_types[0];
        quote! {
            let #arg: #ty = ::facet_postcard::from_bytes(request_payload)
                .map_err(|e| ::rapace_core::RpcError::Status {
                    code: ::rapace_core::ErrorCode::InvalidArgument,
                    message: ::std::format!("decode error: {:?}", e),
                })?;
            let result: #return_type = self.service.#name(#arg).await;
        }
    } else {
        // Multiple args - decode as tuple
        let tuple_type = quote! { (#(#arg_types),*) };
        quote! {
            let (#(#arg_names),*): #tuple_type = ::facet_postcard::from_bytes(request_payload)
                .map_err(|e| ::rapace_core::RpcError::Status {
                    code: ::rapace_core::ErrorCode::InvalidArgument,
                    message: ::std::format!("decode error: {:?}", e),
                })?;
            let result: #return_type = self.service.#name(#(#arg_names),*).await;
        }
    };

    quote! {
        #method_id => {
            #decode_and_call

            // Encode response using facet_postcard
            let response_bytes: ::std::vec::Vec<u8> = ::facet_postcard::to_vec(&result)
                .map_err(|e| ::rapace_core::RpcError::Status {
                    code: ::rapace_core::ErrorCode::Internal,
                    message: ::std::format!("encode error: {:?}", e),
                })?;

            // Build response frame
            let mut desc = ::rapace_core::MsgDescHot::new();
            desc.flags = ::rapace_core::FrameFlags::DATA | ::rapace_core::FrameFlags::EOS;

            let frame = if response_bytes.len() <= ::rapace_core::INLINE_PAYLOAD_SIZE {
                ::rapace_core::Frame::with_inline_payload(desc, &response_bytes)
                    .expect("inline payload should fit")
            } else {
                ::rapace_core::Frame::with_payload(desc, response_bytes)
            };

            Ok(frame)
        }
    }
}

/// Generate the `register` function for service registration.
///
/// This generates a function that registers the service and its methods
/// with a `ServiceRegistry`, capturing request/response schemas via facet.
fn generate_register_fn(service_name: &str, service_doc: &str, methods: &[MethodInfo]) -> TokenStream2 {
    let method_registrations: Vec<TokenStream2> = methods
        .iter()
        .map(|m| {
            let method_name = m.name.to_string();
            let method_doc = &m.doc;
            let arg_types: Vec<_> = m.args.iter().map(|(_, ty)| ty).collect();

            // Generate argument info
            let arg_infos: Vec<TokenStream2> = m.args.iter().map(|(name, ty)| {
                let name_str = name.to_string();
                let type_str = quote!(#ty).to_string();
                quote! {
                    ::rapace_registry::ArgInfo {
                        name: #name_str,
                        type_name: #type_str,
                    }
                }
            }).collect();

            // Request shape: tuple of arg types, or () if no args, or single type if one arg
            let request_shape_expr = if arg_types.is_empty() {
                quote! { <() as ::facet_core::Facet>::SHAPE }
            } else if arg_types.len() == 1 {
                let ty = &arg_types[0];
                quote! { <#ty as ::facet_core::Facet>::SHAPE }
            } else {
                quote! { <(#(#arg_types),*) as ::facet_core::Facet>::SHAPE }
            };

            // Response shape: the return type (or inner type for streaming)
            let response_shape_expr = match &m.kind {
                MethodKind::Unary => {
                    let return_type = &m.return_type;
                    quote! { <#return_type as ::facet_core::Facet>::SHAPE }
                }
                MethodKind::ServerStreaming { item_type } => {
                    quote! { <#item_type as ::facet_core::Facet>::SHAPE }
                }
            };

            // Is this a streaming method?
            let is_streaming = matches!(m.kind, MethodKind::ServerStreaming { .. });

            if is_streaming {
                quote! {
                    builder.add_streaming_method(
                        #method_name,
                        #method_doc,
                        vec![#(#arg_infos),*],
                        #request_shape_expr,
                        #response_shape_expr,
                    );
                }
            } else {
                quote! {
                    builder.add_method(
                        #method_name,
                        #method_doc,
                        vec![#(#arg_infos),*],
                        #request_shape_expr,
                        #response_shape_expr,
                    );
                }
            }
        })
        .collect();

    quote! {
        /// Register this service with a registry.
        ///
        /// This function registers the service and all its methods,
        /// capturing request/response schemas and documentation via facet.
        ///
        /// # Example
        ///
        /// ```ignore
        /// use rapace_registry::ServiceRegistry;
        ///
        /// let mut registry = ServiceRegistry::new();
        /// adder_methods::register(&mut registry);
        /// ```
        pub fn register(registry: &mut ::rapace_registry::ServiceRegistry) {
            let mut builder = registry.register_service(#service_name, #service_doc);
            #(#method_registrations)*
            builder.finish();
        }
    }
}

/// Generate a unary client method that uses a stored method ID from the registry.
fn generate_client_method_unary_registry(
    method: &MethodInfo,
    _method_index: usize,
) -> TokenStream2 {
    let name = &method.name;
    let return_type = &method.return_type;
    let method_id_field = format_ident!("{}_method_id", name);

    let arg_names: Vec<_> = method.args.iter().map(|(name, _)| name).collect();
    let arg_types: Vec<_> = method.args.iter().map(|(_, ty)| ty).collect();

    let fn_args = arg_names.iter().zip(arg_types.iter()).map(|(name, ty)| {
        quote! { #name: #ty }
    });

    let encode_expr = if arg_names.is_empty() {
        quote! { ::facet_postcard::to_vec(&()).unwrap() }
    } else if arg_names.len() == 1 {
        let arg = &arg_names[0];
        quote! { ::facet_postcard::to_vec(&#arg).unwrap() }
    } else {
        quote! { ::facet_postcard::to_vec(&(#(#arg_names.clone()),*)).unwrap() }
    };

    quote! {
        /// Call the #name method on the remote service.
        pub async fn #name(&self, #(#fn_args),*) -> ::std::result::Result<#return_type, ::rapace_core::RpcError> {
            use ::rapace_core::{Frame, FrameFlags, MsgDescHot, Transport};

            let request_bytes: ::std::vec::Vec<u8> = #encode_expr;

            let mut desc = MsgDescHot::new();
            desc.msg_id = self.next_msg_id();
            desc.channel_id = self.next_channel_id();
            desc.method_id = self.#method_id_field;
            desc.flags = FrameFlags::DATA | FrameFlags::EOS;

            let frame = if request_bytes.len() <= ::rapace_core::INLINE_PAYLOAD_SIZE {
                Frame::with_inline_payload(desc, &request_bytes)
                    .expect("inline payload should fit")
            } else {
                Frame::with_payload(desc, request_bytes)
            };

            self.transport.send_frame(&frame).await
                .map_err(::rapace_core::RpcError::Transport)?;

            let response = self.transport.recv_frame().await
                .map_err(::rapace_core::RpcError::Transport)?;

            if response.desc.flags.contains(FrameFlags::ERROR) {
                return Err(Self::parse_error_payload(response.payload));
            }

            let result: #return_type = ::facet_postcard::from_bytes(response.payload)
                .map_err(|e| ::rapace_core::RpcError::Status {
                    code: ::rapace_core::ErrorCode::Internal,
                    message: ::std::format!("decode error: {:?}", e),
                })?;

            Ok(result)
        }
    }
}

/// Generate a server-streaming client method that uses a stored method ID from the registry.
fn generate_client_method_server_streaming_registry(
    method: &MethodInfo,
    _method_index: usize,
    item_type: &Type,
) -> TokenStream2 {
    let name = &method.name;
    let method_id_field = format_ident!("{}_method_id", name);

    let arg_names: Vec<_> = method.args.iter().map(|(name, _)| name).collect();
    let arg_types: Vec<_> = method.args.iter().map(|(_, ty)| ty).collect();

    let fn_args = arg_names.iter().zip(arg_types.iter()).map(|(name, ty)| {
        quote! { #name: #ty }
    });

    let encode_expr = if arg_names.is_empty() {
        quote! { ::facet_postcard::to_vec(&()).unwrap() }
    } else if arg_names.len() == 1 {
        let arg = &arg_names[0];
        quote! { ::facet_postcard::to_vec(&#arg).unwrap() }
    } else {
        quote! { ::facet_postcard::to_vec(&(#(#arg_names.clone()),*)).unwrap() }
    };

    quote! {
        /// Call the #name server-streaming method on the remote service.
        pub async fn #name(&self, #(#fn_args),*) -> ::std::result::Result<::rapace_core::Streaming<#item_type>, ::rapace_core::RpcError> {
            use ::rapace_core::{Frame, FrameFlags, MsgDescHot, Transport};

            let request_bytes: ::std::vec::Vec<u8> = #encode_expr;

            let mut desc = MsgDescHot::new();
            desc.msg_id = self.next_msg_id();
            desc.channel_id = self.next_channel_id();
            desc.method_id = self.#method_id_field;
            desc.flags = FrameFlags::DATA | FrameFlags::EOS;

            let frame = if request_bytes.len() <= ::rapace_core::INLINE_PAYLOAD_SIZE {
                Frame::with_inline_payload(desc, &request_bytes)
                    .expect("inline payload should fit")
            } else {
                Frame::with_payload(desc, request_bytes)
            };

            self.transport.send_frame(&frame).await
                .map_err(::rapace_core::RpcError::Transport)?;

            let (tx, rx) = ::tokio::sync::mpsc::channel::<::std::result::Result<#item_type, ::rapace_core::RpcError>>(16);
            let transport = ::std::sync::Arc::clone(&self.transport);

            ::tokio::spawn(async move {
                loop {
                    let response = match transport.recv_frame().await {
                        Ok(r) => r,
                        Err(e) => {
                            let _ = tx.send(Err(::rapace_core::RpcError::Transport(e))).await;
                            break;
                        }
                    };

                    if response.desc.flags.contains(FrameFlags::ERROR) {
                        let err = Self::parse_error_payload(response.payload);
                        let _ = tx.send(Err(err)).await;
                        break;
                    }

                    if response.desc.flags.contains(FrameFlags::DATA) {
                        match ::facet_postcard::from_bytes::<#item_type>(response.payload) {
                            Ok(item) => {
                                if tx.send(Ok(item)).await.is_err() {
                                    break;
                                }
                            }
                            Err(e) => {
                                let _ = tx.send(Err(::rapace_core::RpcError::Status {
                                    code: ::rapace_core::ErrorCode::Internal,
                                    message: ::std::format!("decode error: {:?}", e),
                                })).await;
                                break;
                            }
                        }
                    }

                    if response.desc.flags.contains(FrameFlags::EOS) {
                        break;
                    }
                }
            });

            let stream = ::tokio_stream::wrappers::ReceiverStream::new(rx);
            Ok(::std::boxed::Box::pin(stream))
        }
    }
}
