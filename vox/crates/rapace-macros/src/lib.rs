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
    let vis = &input.vis;

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

    // Generate client methods
    let client_methods = methods.iter().enumerate().map(|(idx, m)| {
        let method_id = (idx + 1) as u32; // method_id 0 is reserved for control
        generate_client_method(m, method_id)
    });

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

    let mod_name = format_ident!("{}_methods", trait_name.to_string().to_lowercase());

    let expanded = quote! {
        // Keep the original trait
        #input

        /// Method ID constants for this service.
        #vis mod #mod_name {
            #(#method_id_consts)*
        }

        /// Client stub for the #trait_name service.
        #vis struct #client_name<T> {
            transport: ::std::sync::Arc<T>,
            next_msg_id: ::std::sync::atomic::AtomicU64,
            next_channel_id: ::std::sync::atomic::AtomicU32,
        }

        impl<T: ::rapace_core::Transport + 'static> #client_name<T> {
            /// Create a new client with the given transport.
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

            #(#client_methods)*
        }

        /// Server dispatcher for the #trait_name service.
        #vis struct #server_name<S> {
            service: S,
        }

        impl<S: #trait_name + Send + Sync> #server_name<S> {
            /// Create a new server with the given service implementation.
            pub fn new(service: S) -> Self {
                Self { service }
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
}

fn parse_method(method: &TraitItemFn) -> syn::Result<MethodInfo> {
    let sig = &method.sig;
    let name = sig.ident.clone();

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
