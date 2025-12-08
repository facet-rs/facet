//! rapace-macros: Proc macros for rapace RPC.
//!
//! Provides `#[rapace::service]` which generates:
//! - Client stubs with async methods
//! - Server dispatch by method_id

use proc_macro::TokenStream;
use proc_macro2::TokenStream as TokenStream2;
use quote::{format_ident, quote};
use syn::{
    parse_macro_input, FnArg, Ident, ItemTrait, Pat, ReturnType, TraitItem, TraitItemFn, Type,
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
/// # Limitations (v0.1)
///
/// - Only unary RPCs (no streaming)
/// - All methods must be `async fn`
/// - Return type is wrapped in `Result<T, RpcError>` on the client side
/// - Uses `facet_postcard` for serialization (types must implement `Facet`)
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

    // Generate server dispatch arms
    let dispatch_arms = methods.iter().enumerate().map(|(idx, m)| {
        let method_id = (idx + 1) as u32;
        generate_dispatch_arm(m, method_id)
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

        impl<T: ::rapace_core::Transport> #client_name<T> {
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
            /// Returns a response frame on success.
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
        }
    };

    Ok(expanded)
}

struct MethodInfo {
    name: Ident,
    args: Vec<(Ident, Type)>, // (name, type) pairs, excluding &self
    return_type: Type,
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

    Ok(MethodInfo {
        name,
        args,
        return_type,
    })
}

fn generate_client_method(method: &MethodInfo, method_id: u32) -> TokenStream2 {
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
                return Err(::rapace_core::RpcError::Status {
                    code: ::rapace_core::ErrorCode::Internal,
                    message: "remote error".into(),
                });
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

fn generate_dispatch_arm(method: &MethodInfo, method_id: u32) -> TokenStream2 {
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
