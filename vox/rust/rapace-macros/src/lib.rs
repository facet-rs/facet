#![doc = include_str!("../README.md")]
#![deny(unsafe_code)]

use heck::{ToShoutySnakeCase, ToSnakeCase};
use proc_macro::TokenStream;
use proc_macro2::{Ident, Span, TokenStream as TokenStream2, TokenTree};
use quote::{format_ident, quote};

mod crate_name;
mod parser;

use crate_name::{FoundCrate, crate_name};

use parser::{Error as MacroError, ParsedTrait, join_doc_lines, parse_trait};

/// Compute a method ID by hashing "ServiceName.method_name" using FNV-1a.
///
/// Spec: `[impl codegen.method-id.computation]` - uses FNV-1a hash of "ServiceName.method_name"
///
/// This generates globally unique method IDs without requiring sequential
/// assignment or a central registry. The hash is truncated to 32 bits.
fn compute_method_id(service_name: &str, method_name: &str) -> u32 {
    // FNV-1a hash constants
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash = FNV_OFFSET;

    // Hash "ServiceName.method_name"
    for byte in service_name.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    // Hash the dot separator
    hash ^= b'.' as u64;
    hash = hash.wrapping_mul(FNV_PRIME);
    // Hash method name
    for byte in method_name.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }

    // Truncate to 32 bits - we XOR the high and low halves to preserve entropy
    ((hash >> 32) ^ hash) as u32
}

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
    let trait_tokens = TokenStream2::from(item.clone());

    let parsed_trait = match parse_trait(&trait_tokens) {
        Ok(parsed) => parsed,
        Err(err) => return err.to_compile_error().into(),
    };

    match generate_service(&parsed_trait) {
        Ok(tokens) => tokens.into(),
        Err(err) => err.to_compile_error().into(),
    }
}

fn generate_service(input: &ParsedTrait) -> Result<TokenStream2, MacroError> {
    let trait_name = &input.ident;
    let trait_name_str = trait_name.to_string();
    let trait_snake = trait_name_str.to_snake_case();
    let trait_shouty = trait_name_str.to_shouty_snake_case();
    let vis = &input.vis_tokens;

    // Detect the rapace crate name
    // Try to find `rapace` first (the facade crate).
    // If not found, check if we're IN the rapace crate itself.
    // If neither, check if rapace_core is available (for internal crates).
    let rapace_crate = match crate_name("rapace") {
        Ok(FoundCrate::Itself) => quote!(rapace),
        Ok(FoundCrate::Name(name)) => {
            let ident = Ident::new(&name, Span::call_site());
            quote!(#ident)
        }
        Err(_) => {
            // rapace not found - check if this is an internal crate with direct dependencies
            if crate_name("rapace_core").is_ok() {
                // We have rapace_core - this is likely an internal crate
                // Create a local rapace module that re-exports what we need
                return Err(MacroError::new(
                    Span::call_site(),
                    "Internal crates using rapace_macros must add `rapace` as a dependency, \
                     or you can create a facade module. See rapace-testkit for an example.",
                ));
            } else {
                return Err(MacroError::new(
                    Span::call_site(),
                    "rapace crate not found in dependencies. Add `rapace = \"...\"` to your Cargo.toml",
                ));
            }
        }
    };

    // Capture trait doc comments
    let trait_doc = join_doc_lines(&input.doc_lines);

    // Rewrite the user-authored `async fn` trait into an RPITIT trait that guarantees
    // `Send` futures.
    //
    // Why: `RpcSession` dispatch uses `tokio::spawn`, which requires the dispatcher
    // future to be `Send`. Boxing would satisfy this but is ergonomically awful for
    // implementors. RPITIT lets implementors keep writing `async fn` while we encode
    // the `Send` requirement in the trait contract.
    let trait_doc_attr = if trait_doc.is_empty() {
        quote! {}
    } else {
        quote! { #[doc = #trait_doc] }
    };
    let rewritten_methods = input.methods.iter().map(|m| {
        let method_name = &m.name;
        let method_doc = join_doc_lines(&m.doc_lines);
        let method_doc_attr = if method_doc.is_empty() {
            quote! {}
        } else {
            quote! { #[doc = #method_doc] }
        };
        let args = m.args.iter().map(|a| {
            let name = &a.name;
            let ty = &a.ty;
            quote! { #name: #ty }
        });
        let return_type = &m.return_type;
        quote! {
            #method_doc_attr
            fn #method_name(&self, #(#args),*) -> impl ::std::future::Future<Output = #return_type> + Send + '_;
        }
    });
    let trait_tokens = quote! {
        #[allow(clippy::type_complexity)]
        #trait_doc_attr
        #vis trait #trait_name {
            #(#rewritten_methods)*
        }
    };

    let methods: Vec<MethodInfo> = input
        .methods
        .iter()
        .map(MethodInfo::try_from_parsed)
        .collect::<Result<_, _>>()?;

    let client_name = format_ident!("{}Client", trait_name);
    let server_name = format_ident!("{}Server", trait_name);

    // Generate client methods with hashed method IDs
    let client_methods_hardcoded = methods.iter().map(|m| {
        let method_id = compute_method_id(&trait_name_str, &m.name.to_string());
        generate_client_method(m, method_id, &trait_name_str, &rapace_crate)
    });

    // Generate client methods that use stored method IDs from registry
    let client_methods_registry = methods
        .iter()
        .enumerate()
        .map(|(idx, m)| generate_client_method_registry(m, idx, &trait_name_str, &rapace_crate));

    // Generate server dispatch arms (for unary and error fallback)
    let dispatch_arms = methods.iter().map(|m| {
        let method_id = compute_method_id(&trait_name_str, &m.name.to_string());
        generate_dispatch_arm(m, method_id, &rapace_crate)
    });

    // Generate streaming dispatch arms
    let streaming_dispatch_arms = methods.iter().map(|m| {
        let method_id = compute_method_id(&trait_name_str, &m.name.to_string());
        generate_streaming_dispatch_arm(m, method_id, &rapace_crate)
    });

    // Generate a helper that identifies streaming method IDs by consulting the registry.
    //
    // This avoids maintaining a separate "streaming method id set" in generated code and
    // keeps the answer consistent with the on-wire method id space.
    let is_streaming_method_fn = quote! {
        fn __is_streaming_method_id(method_id: u32) -> bool {
            ::#rapace_crate::registry::ServiceRegistry::with_global(|reg| {
                reg.method_by_id(::#rapace_crate::registry::MethodId(method_id))
                    .map(|m| m.is_streaming)
                    .unwrap_or(false)
            })
        }
    };

    // Generate method ID constants
    let method_id_consts = methods.iter().map(|m| {
        let method_id = compute_method_id(&trait_name_str, &m.name.to_string());
        let method_shouty = m.name.to_string().to_shouty_snake_case();
        let const_name = format_ident!("{}_METHOD_ID_{}", trait_shouty, method_shouty);
        quote! {
            #vis const #const_name: u32 = #method_id;
        }
    });

    // Collect all method IDs into an array for the METHOD_IDS constant
    let all_method_ids: Vec<_> = methods
        .iter()
        .map(|m| compute_method_id(&trait_name_str, &m.name.to_string()))
        .collect();

    // Check for hash collisions within this service
    // Spec: `[impl codegen.method-id.collision]` - collisions within a service are detected
    // at macro expansion time and produce a compile error.
    let mut seen_ids = std::collections::HashSet::new();
    for (method, &method_id) in methods.iter().zip(all_method_ids.iter()) {
        if !seen_ids.insert(method_id) {
            // Find which other method has the same ID, using the precomputed IDs
            let collision_with = methods
                .iter()
                .zip(all_method_ids.iter())
                .find(|&(m, &id)| id == method_id && m.name != method.name)
                .map(|(m, _)| m.name.to_string())
                .unwrap_or_else(|| "unknown".to_string());

            return Err(MacroError::new(
                method.name.span(),
                format!(
                    "Hash collision detected: methods '{}' and '{}' both hash to method_id {}. \
                     This is extremely rare but indicates you need to rename one of these methods.",
                    method.name, collision_with, method_id
                ),
            ));
        }
    }

    // Generate registry registration code
    let register_fn_name = format_ident!("{}_register", trait_snake);
    let register_fn = generate_register_fn(
        &trait_name_str,
        &trait_doc,
        &methods,
        &rapace_crate,
        &register_fn_name,
        vis,
    );

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

    // Generate blanket impl for Arc<T> to solve orphan rule issues
    // when trait is in a separate -proto crate
    let arc_impl_methods = input.methods.iter().map(|m| {
        let method_name = &m.name;
        let args = m.args.iter().map(|a| {
            let name = &a.name;
            let ty = &a.ty;
            quote! { #name: #ty }
        });
        let arg_names = m.args.iter().map(|a| &a.name);
        let return_type = &m.return_type;
        quote! {
            fn #method_name(&self, #(#args),*) -> impl ::std::future::Future<Output = #return_type> + Send + '_ {
                (**self).#method_name(#(#arg_names),*)
            }
        }
    });

    // Detect if rapace_cell is available in the dependency graph.
    // We'll generate different impls depending on whether we're inside rapace_cell or not.
    let rapace_cell_crate = match crate_name("rapace_cell") {
        Ok(FoundCrate::Itself) => Some((quote!(crate), true)),
        Ok(FoundCrate::Name(name)) => {
            let ident = Ident::new(&name, Span::call_site());
            Some((quote!(#ident), false))
        }
        Err(_) => {
            // Try with hyphen
            match crate_name("rapace-cell") {
                Ok(FoundCrate::Itself) => Some((quote!(crate), true)),
                Ok(FoundCrate::Name(name)) => {
                    let ident = Ident::new(&name, Span::call_site());
                    Some((quote!(#ident), false))
                }
                Err(_) => None,
            }
        }
    };

    let service_dispatch_impl = if let Some((rapace_cell_path, is_inside_rapace_cell)) =
        rapace_cell_crate
    {
        // Generate a local wrapper type that implements ServiceDispatch.
        // This wrapper satisfies orphan rules in ALL crates because it's local to the service definition.
        let dispatch_wrapper_name = format_ident!("{}Dispatch", trait_name);

        let dispatch_wrapper_impl = quote! {
            // Auto-generated ServiceDispatch wrapper for multi-service cells.
            //
            // This wrapper type is local to this crate, so it can legally implement
            // ServiceDispatch (from rapace_cell) without violating orphan rules.
            //
            // # Usage
            //
            // ```ignore
            // use rapace_cell::DispatcherBuilder;
            //
            // let dispatcher = DispatcherBuilder::new()
            //     .add_service(FooServer::new(foo_impl).into_dispatch())
            //     .add_service(BarServer::new(bar_impl).into_dispatch())
            //     .build(buffer_pool);
            // ```

            /// Auto-generated wrapper type that implements `ServiceDispatch`.
            ///
            /// Use `{ServiceName}Server::into_dispatch()` to create this type.
            #vis struct #dispatch_wrapper_name<S>(::std::sync::Arc<#server_name<S>>);

            const _: () = {

                #[allow(unused_qualifications)]
                impl<S: #trait_name + Send + Sync + 'static> #rapace_cell_path::ServiceDispatch for #dispatch_wrapper_name<S> {
                    fn method_ids(&self) -> &'static [u32] {
                        #server_name::<S>::METHOD_IDS
                    }

                    fn dispatch(
                        &self,
                        method_id: u32,
                        frame: #rapace_cell_path::Frame,
                        buffer_pool: &#rapace_cell_path::BufferPool,
                    ) -> ::std::pin::Pin<
                        ::std::boxed::Box<
                            dyn ::std::future::Future<
                                    Output = ::std::result::Result<
                                        #rapace_cell_path::Frame,
                                        #rapace_cell_path::RpcError,
                                    >,
                                > + Send
                                + 'static,
                        >,
                    > {
                        // Clone the Arc to get an owned 'static reference
                        let server = ::std::sync::Arc::clone(&self.0);
                        let buffer_pool = buffer_pool.clone();
                        // Frame is moved into the closure - no copying needed!
                        ::std::boxed::Box::pin(async move {
                            // Use fully-qualified syntax to call the inherent dispatch method
                            #server_name::dispatch(&*server, method_id, &frame, &buffer_pool).await
                        })
                    }
                }

                impl<S: #trait_name + Send + Sync + 'static> #server_name<S> {
                    /// Convert this server into a type that implements `ServiceDispatch`.
                    ///
                    /// This wraps the server in an `Arc` and returns a dispatcher-compatible type.
                    /// Use this with `DispatcherBuilder::add_service()` in multi-service cells.
                    ///
                    /// # Example
                    ///
                    /// ```ignore
                    /// let dispatcher = DispatcherBuilder::new()
                    ///     .add_service(FooServer::new(foo_impl).into_dispatch())
                    ///     .build(buffer_pool);
                    /// ```
                    pub fn into_dispatch(self) -> #dispatch_wrapper_name<S> {
                        #dispatch_wrapper_name(::std::sync::Arc::new(self))
                    }
                }
            };
        };

        // If we're inside rapace_cell, ALSO generate the Arc<FooServer<S>> impl for backwards compatibility
        if is_inside_rapace_cell {
            quote! {
                #dispatch_wrapper_impl

                // Also generate Arc<#server_name<S>> impl for backwards compatibility within rapace_cell.
                //
                // This eliminates the need for manual wrapper boilerplate. Users wrap in Arc at the call site:
                //
                // ```ignore
                // use rapace_cell::DispatcherBuilder;
                //
                // let dispatcher = DispatcherBuilder::new()
                //     .add_service(Arc::new(FooServer::new(foo_impl)))
                //     .add_service(Arc::new(BarServer::new(bar_impl)))
                //     .build(buffer_pool);
                //
                // // Or use the convenience helper:
                // let dispatcher = DispatcherBuilder::new()
                //     .add_service(FooServer::new(foo_impl).into_service())
                //     .build(buffer_pool);
                // ```
                //
                // Within the rapace_cell crate itself, this implementation is generated automatically; it is not emitted in downstream crates due to orphan rule restrictions (see docs above).
                const _: () = {
                    #[allow(unused_qualifications)]
                    impl<S: #trait_name + Send + Sync + 'static> #rapace_cell_path::ServiceDispatch for ::std::sync::Arc<#server_name<S>> {
                        fn method_ids(&self) -> &'static [u32] {
                            #server_name::<S>::METHOD_IDS
                        }

                        fn dispatch(
                            &self,
                            method_id: u32,
                            frame: #rapace_cell_path::Frame,
                            buffer_pool: &#rapace_cell_path::BufferPool,
                        ) -> ::std::pin::Pin<
                            ::std::boxed::Box<
                                dyn ::std::future::Future<
                                        Output = ::std::result::Result<
                                            #rapace_cell_path::Frame,
                                            #rapace_cell_path::RpcError,
                                        >,
                                    > + Send
                                    + 'static,
                            >,
                        > {
                            // Clone the Arc to get an owned 'static reference
                            let server = ::std::sync::Arc::clone(self);
                            let buffer_pool = buffer_pool.clone();
                            // Frame is moved into the closure - no copying needed!
                            ::std::boxed::Box::pin(async move {
                                // Use fully-qualified syntax to call the inherent dispatch method,
                                // not the ServiceDispatch trait method we're implementing.
                                #server_name::dispatch(&*server, method_id, &frame, &buffer_pool).await
                            })
                        }
                    }

                    impl<S: #trait_name + Send + Sync + 'static> #server_name<S> {
                        /// Convert this server into a type that implements `ServiceDispatch`.
                        ///
                        /// This is a convenience method that wraps the server in an `Arc`.
                        /// Equivalent to `Arc::new(server)`.
                        ///
                        /// # Example
                        ///
                        /// ```ignore
                        /// let dispatcher = DispatcherBuilder::new()
                        ///     .add_service(FooServer::new(foo_impl).into_service())
                        ///     .build(buffer_pool);
                        /// ```
                        pub fn into_service(self) -> ::std::sync::Arc<Self> {
                            ::std::sync::Arc::new(self)
                        }
                    }
                };
            }
        } else {
            // Not inside rapace_cell - just generate the wrapper
            dispatch_wrapper_impl
        }
    } else {
        quote! {
            // ServiceDispatch implementation not generated.
            //
            // This happens either because:
            // 1. rapace_cell is not in the dependency tree, or
            // 2. rapace_cell is a dependency (not the current crate), which would
            //    cause an orphan rule violation if we tried to impl ServiceDispatch
            //    for Arc<#server_name<S>>.
            //
            // To use this server with DispatcherBuilder, use the cell_service! macro
            // which generates a local newtype wrapper that satisfies orphan rules.
        }
    };

    let expanded = quote! {
        // Rewritten Send-future trait
        #trait_tokens

        // Blanket impl for Arc<T> where T implements the trait.
        // This allows users to implement the trait on their type T,
        // then use Arc<T> directly without hitting orphan rule issues
        // when the trait is in a separate crate.
        impl<T: #trait_name + ?Sized> #trait_name for ::std::sync::Arc<T> {
            #(#arc_impl_methods)*
        }

        #(#method_id_consts)*

        #register_fn

        #service_dispatch_impl

        /// Client stub for the #trait_name service.
        ///
        /// Generic over the transport type `T`, enabling monomorphization when the
        /// transport is known at compile time, or use `AnyTransport` for type erasure.
        ///
        /// # Generic Transport
        ///
        /// - Use `FooClient<ShmTransport>` for zero-cost monomorphization
        /// - Use `FooClient<AnyTransport>` for runtime polymorphism
        ///
        /// The session's [`run`](::#rapace_crate::rapace_core::RpcSession::run) task must be running.
        /// For multi-service scenarios where method IDs must be globally unique,
        /// use [`#registry_client_name`] instead.
        ///
        /// # Usage
        ///
        /// ```ignore
        /// // Monomorphized (concrete transport type)
        /// let session: Arc<RpcSession<ShmTransport>> = Arc::new(RpcSession::new(shm_transport));
        /// let client: FooClient<ShmTransport> = FooClient::new(session.clone());
        ///
        /// // Type-erased (dynamic transport)
        /// let session: Arc<RpcSession<AnyTransport>> = Arc::new(RpcSession::new(AnyTransport::new(transport)));
        /// let client: FooClient<AnyTransport> = FooClient::new(session.clone());
        ///
        /// tokio::spawn(session.clone().run()); // Start the demux loop
        /// let result = client.some_method(args).await?;
        /// ```
        #vis struct #client_name<T: ::#rapace_crate::rapace_core::Transport> {
            session: ::std::sync::Arc<::#rapace_crate::rapace_core::RpcSession<T>>,
        }

        impl<T: ::#rapace_crate::rapace_core::Transport> #client_name<T> {
            /// Create a new client with the given RPC session.
            ///
            /// Uses compile-time, on-wire method IDs (hashed `Service.method`).
            /// For registry-resolved method IDs, use [`#registry_client_name::new`].
            ///
            /// The provided session must be shared (`Arc::clone`) with the call site
            /// and have its demux loop (`tokio::spawn(session.clone().run())`) running.
            pub fn new(session: ::std::sync::Arc<::#rapace_crate::rapace_core::RpcSession<T>>) -> Self {
                Self { session }
            }

            /// Get a reference to the underlying session.
            pub fn session(&self) -> &::std::sync::Arc<::#rapace_crate::rapace_core::RpcSession<T>> {
                &self.session
            }

            #(#client_methods_hardcoded)*
        }

        /// Registry-aware client stub for the #trait_name service.
        ///
        /// Generic over the transport type `T`, just like [`#client_name`].
        /// This client resolves method IDs from a [`ServiceRegistry`] at construction time.
        /// This can be useful when you want to validate the service/methods are registered
        /// (or when building tooling around introspection).
        /// It has the same [`RpcSession`](::#rapace_crate::rapace_core::RpcSession) requirements as [`#client_name`].
        #vis struct #registry_client_name<T: ::#rapace_crate::rapace_core::Transport> {
            session: ::std::sync::Arc<::#rapace_crate::rapace_core::RpcSession<T>>,
            #(pub #method_id_fields,)*
        }

        impl<T: ::#rapace_crate::rapace_core::Transport> #registry_client_name<T> {
            /// Create a new registry-aware client.
            ///
            /// Looks up method IDs from the registry. The service must be registered
            /// in the registry before calling this constructor.
            ///
            /// The session's demux loop (`session.run()`) must be running for RPC calls to work.
            ///
            /// # Panics
            ///
            /// Panics if the service or any of its methods are not found in the registry.
            pub fn new(session: ::std::sync::Arc<::#rapace_crate::rapace_core::RpcSession<T>>, registry: &::#rapace_crate::registry::ServiceRegistry) -> Self {
                Self {
                    session,
                    #(#method_id_lookups,)*
                }
            }

            /// Get a reference to the underlying session.
            pub fn session(&self) -> &::std::sync::Arc<::#rapace_crate::rapace_core::RpcSession<T>> {
                &self.session
            }

            #(#client_methods_registry)*
        }

        /// Server dispatcher for the #trait_name service.
        ///
        /// Integrate this with an [`RpcSession`](::#rapace_crate::rapace_core::RpcSession)
        /// by calling [`RpcSession::set_dispatcher`](::#rapace_crate::rapace_core::RpcSession::set_dispatcher)
        /// and forwarding `method_id`/`payload` into [`dispatch`] or [`dispatch_streaming`].
        #vis struct #server_name<S> {
            service: S,
        }

        impl<S: #trait_name + Send + Sync + 'static> #server_name<S> {
            /// All method IDs for this service, for use with `cell_service!` and `DispatcherBuilder`.
            ///
            /// This constant array is **generated automatically** from the methods defined in
            /// this service. It contains the FNV-1a hashed method IDs for all methods,
            /// enabling O(1) `HashMap`-based dispatch in multi-service cells.
            ///
            /// # Generated Content
            ///
            /// - **Automatically generated** - Users do **not** need to maintain or specify
            ///   these IDs manually (this replaces the older manual method-ID workflow from issue #82)
            /// - **Order**: Method IDs appear in the same order as method definitions in the trait
            /// - **Uniqueness**: Hash collisions are detected at macro expansion time and will
            ///   produce a compile error with a helpful message indicating which methods collided
            /// - **Contents**: Includes all methods (both unary and streaming) defined in the service
            ///
            /// # Usage
            ///
            /// The dispatcher uses these IDs to route incoming RPC calls to the correct handler.
            /// When a frame arrives with `method_id = 0x12345678`, the dispatcher looks it up
            /// in its `HashMap<u32, Handler>` to find the matching method handler in O(1) time.
            ///
            /// Each service also generates individual `METHOD_ID_*` constants for direct use:
            /// ```ignore
            /// const METHOD_ID_FOO: u32 = ...;  // for MyService::foo
            /// const METHOD_ID_BAR: u32 = ...;  // for MyService::bar
            /// ```
            #vis const METHOD_IDS: &'static [u32] = &[#(#all_method_ids),*];

            /// Auto-register this service in the global registry.
            ///
            /// Called automatically from `new()`. Uses `OnceCell` to ensure registration
            /// happens exactly once, even if multiple server instances are created.
            fn __auto_register() {
                use ::std::sync::OnceLock;
                static REGISTERED: OnceLock<()> = OnceLock::new();

                REGISTERED.get_or_init(|| {
                    ::#rapace_crate::registry::ServiceRegistry::with_global_mut(|registry| {
                        #register_fn_name(registry);
                    });
                });
            }

            /// Create a new server with the given service implementation.
            ///
            /// This automatically registers the service in the global registry
            /// on first invocation (subsequent calls are no-ops).
            pub fn new(service: S) -> Self {
                Self::__auto_register();
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
            pub async fn serve(
                self,
                transport: ::#rapace_crate::rapace_core::AnyTransport,
            ) -> ::std::result::Result<(), ::#rapace_crate::rapace_core::RpcError> {
                ::#rapace_crate::tracing::debug!("serve: entering loop, waiting for requests");
                loop {
                    // Receive next request frame
                    let request = match transport.recv_frame().await {
                        Ok(frame) => {
                            ::#rapace_crate::tracing::debug!(
                                method_id = frame.desc.method_id,
                                channel_id = frame.desc.channel_id,
                                flags = ?frame.desc.flags,
                                payload_len = frame.payload_bytes().len(),
                                "serve: received frame"
                            );
                            frame
                        }
                        Err(::#rapace_crate::rapace_core::TransportError::Closed) => {
                            ::#rapace_crate::tracing::debug!("serve: transport closed");
                            // Connection closed gracefully
                            return Ok(());
                        }
                        Err(e) => {
                            ::#rapace_crate::tracing::error!(?e, "serve: transport error");
                            return Err(::#rapace_crate::rapace_core::RpcError::Transport(e));
                        }
                    };

                    // Skip non-data frames (control frames, etc.)
                    if !request.desc.flags.contains(::#rapace_crate::rapace_core::FrameFlags::DATA) {
                        ::#rapace_crate::tracing::debug!("serve: skipping non-DATA frame");
                        continue;
                    }

                    // Dispatch the request
                    ::#rapace_crate::tracing::debug!(
                        method_id = request.desc.method_id,
                        channel_id = request.desc.channel_id,
                        "serve: dispatching to dispatch_streaming"
                    );
                    if let Err(e) = self.dispatch_streaming(
                        request.desc.method_id,
                        request.desc.channel_id,
                        &request,
                        &transport,
                    ).await {
                        ::#rapace_crate::tracing::error!(?e, "serve: dispatch_streaming returned error");
                        // Send error response
                        let mut desc = ::#rapace_crate::rapace_core::MsgDescHot::new();
                        desc.channel_id = request.desc.channel_id;
                        desc.flags = ::#rapace_crate::rapace_core::FrameFlags::ERROR | ::#rapace_crate::rapace_core::FrameFlags::EOS;

                        // Encode error: [code: u32 LE][message_len: u32 LE][message bytes]
                        let (code, message): (u32, ::std::string::String) = match &e {
                            ::#rapace_crate::rapace_core::RpcError::Status { code, message } => (*code as u32, message.clone()),
                            ::#rapace_crate::rapace_core::RpcError::Transport(_) => (::#rapace_crate::rapace_core::ErrorCode::Internal as u32, "transport error".into()),
                            ::#rapace_crate::rapace_core::RpcError::Cancelled => (::#rapace_crate::rapace_core::ErrorCode::Cancelled as u32, "cancelled".into()),
                            ::#rapace_crate::rapace_core::RpcError::DeadlineExceeded => (::#rapace_crate::rapace_core::ErrorCode::DeadlineExceeded as u32, "deadline exceeded".into()),
                            ::#rapace_crate::rapace_core::RpcError::Serialize(e) => (::#rapace_crate::rapace_core::ErrorCode::Internal as u32, ::std::format!("serialize error: {}", e)),
                            ::#rapace_crate::rapace_core::RpcError::Deserialize(e) => (::#rapace_crate::rapace_core::ErrorCode::Internal as u32, ::std::format!("deserialize error: {}", e)),
                        };
                        let mut err_bytes = ::std::vec::Vec::with_capacity(8 + message.len());
                        err_bytes.extend_from_slice(&code.to_le_bytes());
                        err_bytes.extend_from_slice(&(message.len() as u32).to_le_bytes());
                        err_bytes.extend_from_slice(message.as_bytes());

                        let frame = ::#rapace_crate::rapace_core::Frame::with_payload(desc, err_bytes);
                        let _ = transport.send_frame(frame).await;
                    }
                }
            }

            /// Serve a single request from the transport.
            ///
            /// This is useful for testing or when you want to handle each request
            /// individually.
            pub async fn serve_one(
                &self,
                transport: &#rapace_crate::rapace_core::AnyTransport,
            ) -> ::std::result::Result<(), #rapace_crate::rapace_core::RpcError> {
                // Receive next request frame
                let request = transport.recv_frame().await
                    .map_err(#rapace_crate::rapace_core::RpcError::Transport)?;

                // Skip non-data frames
                if !request.desc.flags.contains(#rapace_crate::rapace_core::FrameFlags::DATA) {
                    return Ok(());
                }

                // Dispatch the request
                self.dispatch_streaming(
                    request.desc.method_id,
                    request.desc.channel_id,
                    &request,
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
                request_frame: &#rapace_crate::rapace_core::Frame,
                buffer_pool: &#rapace_crate::rapace_core::BufferPool,
            ) -> ::std::result::Result<#rapace_crate::rapace_core::Frame, #rapace_crate::rapace_core::RpcError> {
                match method_id {
                    #(#dispatch_arms)*
                    _ => Err(#rapace_crate::rapace_core::RpcError::Status {
                        code: #rapace_crate::rapace_core::ErrorCode::Unimplemented,
                        message: ::std::format!("unknown method_id: {}", method_id),
                    }),
                }
            }

            /// Dispatch a streaming request to the appropriate method.
            ///
            /// The method sends frames via the provided transport.
            pub async fn dispatch_streaming(
                &self,
                method_id: u32,
                channel_id: u32,
                request_frame: &#rapace_crate::rapace_core::Frame,
                transport: &#rapace_crate::rapace_core::AnyTransport,
            ) -> ::std::result::Result<(), #rapace_crate::rapace_core::RpcError> {
                #rapace_crate::tracing::debug!(method_id, channel_id, "dispatch_streaming: entered");
                match method_id {
                    #(#streaming_dispatch_arms)*
                    _ => Err(#rapace_crate::rapace_core::RpcError::Status {
                        code: #rapace_crate::rapace_core::ErrorCode::Unimplemented,
                        message: ::std::format!("unknown method_id: {}", method_id),
                    }),
                }
            }

            #is_streaming_method_fn

            /// Create a dispatcher closure suitable for `RpcSession::set_dispatcher`.
            ///
            /// This handles both unary and server-streaming methods:
            /// - Unary methods return a single response `Frame`.
            /// - Streaming methods require the request to be flagged `NO_REPLY` and are
            ///   served by calling `dispatch_streaming` to emit DATA/EOS frames on the
            ///   provided transport.
            ///
            /// To ensure streaming calls work correctly, use rapace's generated streaming
            /// clients (they set `NO_REPLY` automatically).
            pub fn into_session_dispatcher(
                self,
                transport: ::#rapace_crate::rapace_core::AnyTransport,
            ) -> impl Fn(
                ::#rapace_crate::rapace_core::Frame,
            ) -> ::std::pin::Pin<
                Box<
                    dyn ::std::future::Future<
                            Output = ::std::result::Result<
                                ::#rapace_crate::rapace_core::Frame,
                                ::#rapace_crate::rapace_core::RpcError,
                            >,
                        > + Send
                        + 'static,
                >,
            > + Send
              + Sync
              + 'static {
                use ::#rapace_crate::rapace_core::{ErrorCode, Frame, FrameFlags, MsgDescHot, RpcError};

                let server = ::std::sync::Arc::new(self);
                move |frame: Frame| {
                    let server = server.clone();
                    let transport = transport.clone();
                    Box::pin(async move {
                        let method_id = frame.desc.method_id;
                        let channel_id = frame.desc.channel_id;
                        let flags = frame.desc.flags;

                        if Self::__is_streaming_method_id(method_id) {
                            // Enforce NO_REPLY: streaming methods do not produce a unary response frame.
                            if !flags.contains(FrameFlags::NO_REPLY) {
                                return Err(RpcError::Status {
                                    code: ErrorCode::InvalidArgument,
                                    message: "streaming request missing NO_REPLY flag".into(),
                                });
                            }

                            // Serve the streaming method by sending DATA/EOS frames on the transport.
                            if let Err(err) = server
                                .dispatch_streaming(method_id, channel_id, &frame, &transport)
                                .await
                            {
                                // If dispatch_streaming fails before it could send an ERROR frame,
                                // send one here so streaming clients don't hang.
                                let (code, message): (u32, String) = match &err {
                                    RpcError::Status { code, message } => (*code as u32, message.clone()),
                                    RpcError::Transport(_) => (ErrorCode::Internal as u32, "transport error".into()),
                                    RpcError::Cancelled => (ErrorCode::Cancelled as u32, "cancelled".into()),
                                    RpcError::DeadlineExceeded => (ErrorCode::DeadlineExceeded as u32, "deadline exceeded".into()),
                                    RpcError::Serialize(e) => (ErrorCode::Internal as u32, format!("serialize error: {}", e)),
                                    RpcError::Deserialize(e) => (ErrorCode::Internal as u32, format!("deserialize error: {}", e)),
                                };

                                let mut err_bytes = Vec::with_capacity(8 + message.len());
                                err_bytes.extend_from_slice(&code.to_le_bytes());
                                err_bytes.extend_from_slice(&(message.len() as u32).to_le_bytes());
                                err_bytes.extend_from_slice(message.as_bytes());

                                let mut desc = MsgDescHot::new();
                                desc.channel_id = channel_id;
                                desc.flags = FrameFlags::ERROR | FrameFlags::EOS;
                                let frame = Frame::with_payload(desc, err_bytes);
                                let _ = transport.send_frame(frame).await;
                            }

                            // The session will ignore this because the request had NO_REPLY set.
                            Ok(Frame::new(MsgDescHot::new()))
                        } else {
                            server.dispatch(method_id, &frame, transport.buffer_pool()).await
                        }
                    })
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
    /// Server-streaming: single request, returns `Streaming<T>`.
    ServerStreaming {
        /// The type T in `Streaming<T>`.
        item_type: TokenStream2,
    },
}

struct MethodInfo {
    name: Ident,
    args: Vec<(Ident, TokenStream2)>, // (name, type) pairs, excluding &self
    return_type: TokenStream2,
    kind: MethodKind,
    doc: String,
}

impl MethodInfo {
    fn try_from_parsed(method: &parser::ParsedMethod) -> Result<Self, MacroError> {
        let doc = join_doc_lines(&method.doc_lines);

        let args = method
            .args
            .iter()
            .map(|arg| (arg.name.clone(), arg.ty.clone()))
            .collect();

        let return_type = method.return_type.clone();
        let kind = if let Some(item_type) = extract_streaming_return_type(&return_type) {
            MethodKind::ServerStreaming { item_type }
        } else {
            MethodKind::Unary
        };

        Ok(Self {
            name: method.name.clone(),
            args,
            return_type,
            kind,
            doc,
        })
    }
}

fn generate_client_method(
    method: &MethodInfo,
    method_id: u32,
    service_name: &str,
    rapace_crate: &TokenStream2,
) -> TokenStream2 {
    match &method.kind {
        MethodKind::Unary => {
            generate_client_method_unary(method, method_id, service_name, rapace_crate)
        }
        MethodKind::ServerStreaming { item_type } => generate_client_method_server_streaming(
            method,
            method_id,
            service_name,
            item_type,
            rapace_crate,
        ),
    }
}

fn generate_client_method_registry(
    method: &MethodInfo,
    method_index: usize,
    service_name: &str,
    rapace_crate: &TokenStream2,
) -> TokenStream2 {
    match &method.kind {
        MethodKind::Unary => {
            generate_client_method_unary_registry(method, method_index, service_name, rapace_crate)
        }
        MethodKind::ServerStreaming { item_type } => {
            generate_client_method_server_streaming_registry(
                method,
                method_index,
                service_name,
                item_type,
                rapace_crate,
            )
        }
    }
}

fn generate_client_method_unary(
    method: &MethodInfo,
    method_id: u32,
    service_name: &str,
    rapace_crate: &TokenStream2,
) -> TokenStream2 {
    let name = &method.name;
    let method_name_str = name.to_string();
    let return_type = &method.return_type;

    let arg_names: Vec<_> = method.args.iter().map(|(name, _)| name).collect();
    let arg_types: Vec<_> = method.args.iter().map(|(_, ty)| ty).collect();

    // Generate the argument list for the function signature
    let fn_args = arg_names.iter().zip(arg_types.iter()).map(|(name, ty)| {
        quote! { #name: #ty }
    });

    // Spec: `[impl codegen.args.encoding]` - multiple args encoded as tuples:
    // () -> Unit (empty payload), (a: T) -> T, (a: T, b: U) -> (T, U), etc.
    let encode_expr = if arg_names.is_empty() {
        quote! { #rapace_crate::postcard_to_pooled_buf(self.session.buffer_pool(), &())? }
    } else if arg_names.len() == 1 {
        let arg = &arg_names[0];
        quote! { #rapace_crate::postcard_to_pooled_buf(self.session.buffer_pool(), &#arg)? }
    } else {
        quote! { #rapace_crate::postcard_to_pooled_buf(self.session.buffer_pool(), &(#(#arg_names.clone()),*))? }
    };

    // Check if the return type has a lifetime parameter for zero-copy deserialization
    let uses_zero_copy = type_has_lifetime(return_type);

    // Generate different code paths for zero-copy vs owned deserialization
    let (actual_return_type, decode_and_return) = if uses_zero_copy {
        // Zero-copy path: return OwnedMessage<T> which borrows from the frame
        let wrapped_return = quote! { #rapace_crate::rapace_core::OwnedMessage<#return_type> };
        let decode = quote! {
            // Zero-copy deserialization: response borrows from frame
            let owned = #rapace_crate::rapace_core::OwnedMessage::<#return_type>::try_new(
                response,
                |payload| #rapace_crate::facet_postcard::from_slice(payload)
            ).map_err(|e| #rapace_crate::rapace_core::RpcError::Status {
                code: #rapace_crate::rapace_core::ErrorCode::Internal,
                message: ::std::format!("deserialize error: {:?}", e),
            })?;
            Ok(owned)
        };
        (wrapped_return, decode)
    } else {
        // Owned path: copy data out of frame (current behavior)
        let decode = quote! {
            // Owned deserialization: copy data from frame
            let result: #return_type = #rapace_crate::facet_postcard::from_slice(response.payload_bytes())
                .map_err(|e| #rapace_crate::rapace_core::RpcError::Status {
                    code: #rapace_crate::rapace_core::ErrorCode::Internal,
                    message: ::std::format!("deserialize error: {:?}", e),
                })?;
            Ok(result)
        };
        (quote! { #return_type }, decode)
    };

    quote! {
        /// Call the #name method on the remote service.
        pub async fn #name(&self, #(#fn_args),*) -> ::std::result::Result<#actual_return_type, #rapace_crate::rapace_core::RpcError> {
            use #rapace_crate::rapace_core::FrameFlags;

            // Encode request using pooled serialization
            let request_bytes = #encode_expr;

            // Call via session
            let channel_id = self.session.next_channel_id();
            #rapace_crate::tracing::debug!(
                service = #service_name,
                method = #method_name_str,
                method_id = #method_id,
                channel_id,
                "RPC call start"
            );
            let response = self.session.call_pooled(channel_id, #method_id, request_bytes).await?;
            #rapace_crate::tracing::debug!(
                service = #service_name,
                method = #method_name_str,
                method_id = #method_id,
                channel_id,
                "RPC call complete"
            );

            // Check for error flag
            if response.flags().contains(FrameFlags::ERROR) {
                return Err(#rapace_crate::rapace_core::parse_error_payload(response.payload_bytes()));
            }

            #decode_and_return
        }
    }
}

fn extract_streaming_return_type(ty: &TokenStream2) -> Option<TokenStream2> {
    let tokens: Vec<TokenTree> = ty.clone().into_iter().collect();
    let mut index = 0;
    while index < tokens.len() {
        match &tokens[index] {
            TokenTree::Ident(ident) if ident == "Streaming" => {
                let mut search = index + 1;
                while search < tokens.len() {
                    match &tokens[search] {
                        TokenTree::Punct(p) if p.as_char() == '<' => {
                            let inner = collect_generic_tokens(&tokens, search)?;
                            return select_stream_item_type(inner);
                        }
                        TokenTree::Punct(p) if p.as_char() == ':' => {
                            search += 1;
                            continue;
                        }
                        _ => break,
                    }
                }
            }
            _ => {}
        }
        index += 1;
    }
    None
}

fn collect_generic_tokens(tokens: &[TokenTree], start: usize) -> Option<TokenStream2> {
    let mut depth = 0usize;
    let mut inner = TokenStream2::new();
    let mut i = start;
    while i < tokens.len() {
        match &tokens[i] {
            TokenTree::Punct(p) if p.as_char() == '<' => {
                depth += 1;
                if depth > 1 {
                    inner.extend(std::iter::once(tokens[i].clone()));
                }
            }
            TokenTree::Punct(p) if p.as_char() == '>' => {
                if depth == 0 {
                    return None;
                }
                depth -= 1;
                if depth == 0 {
                    return Some(inner);
                }
                inner.extend(std::iter::once(tokens[i].clone()));
            }
            other => {
                if depth >= 1 {
                    inner.extend(std::iter::once(other.clone()));
                }
            }
        }
        i += 1;
    }
    None
}

fn select_stream_item_type(inner: TokenStream2) -> Option<TokenStream2> {
    let segments = split_top_level(inner, ',');
    for segment in segments.into_iter().rev() {
        let text = segment.to_string();
        if text.trim().is_empty() {
            continue;
        }
        if text.trim_start().starts_with('\'') {
            continue;
        }
        return Some(segment);
    }
    None
}

/// Check if a type token stream contains a lifetime parameter.
///
/// Returns true if the type contains `'a`, `'_`, `'static`, or any other lifetime.
/// This is used to detect types that can borrow from the input buffer for zero-copy
/// deserialization.
///
/// Note: We distinguish lifetimes from character literals by checking that `'` is
/// followed by an identifier (e.g., `'a`, `'static`) or `_`.
fn type_has_lifetime(ty: &TokenStream2) -> bool {
    let mut iter = ty.clone().into_iter().peekable();
    while let Some(tt) = iter.next() {
        match &tt {
            // Check for lifetime: 'a, '_, 'static, etc.
            // Must be followed by an identifier to distinguish from char literals like 'c'
            TokenTree::Punct(p) if p.as_char() == '\'' => {
                if let Some(next) = iter.peek() {
                    match next {
                        TokenTree::Ident(_) => return true,
                        // Underscore lifetime '_
                        TokenTree::Punct(p2) if p2.as_char() == '_' => return true,
                        _ => {}
                    }
                }
            }
            // Recursively check inside groups (parentheses, brackets, braces)
            TokenTree::Group(g) => {
                if type_has_lifetime(&g.stream()) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

fn split_top_level(tokens: TokenStream2, delimiter: char) -> Vec<TokenStream2> {
    let mut parts = Vec::new();
    let mut current = TokenStream2::new();
    let mut angle_depth = 0usize;
    for tt in tokens.into_iter() {
        match &tt {
            TokenTree::Punct(p) if p.as_char() == '<' => {
                angle_depth += 1;
                current.extend(std::iter::once(tt));
            }
            TokenTree::Punct(p) if p.as_char() == '>' => {
                angle_depth = angle_depth.saturating_sub(1);
                current.extend(std::iter::once(tt));
            }
            TokenTree::Punct(p) if p.as_char() == delimiter && angle_depth == 0 => {
                parts.push(current);
                current = TokenStream2::new();
                continue;
            }
            _ => current.extend(std::iter::once(tt)),
        }
    }
    parts.push(current);
    parts
}

fn generate_client_method_server_streaming(
    method: &MethodInfo,
    method_id: u32,
    service_name: &str,
    item_type: &TokenStream2,
    rapace_crate: &TokenStream2,
) -> TokenStream2 {
    let name = &method.name;
    let method_name_str = name.to_string();

    let arg_names: Vec<_> = method.args.iter().map(|(name, _)| name).collect();
    let arg_types: Vec<_> = method.args.iter().map(|(_, ty)| ty).collect();

    // Generate the argument list for the function signature
    let fn_args = arg_names.iter().zip(arg_types.iter()).map(|(name, ty)| {
        quote! { #name: #ty }
    });

    // For encoding, serialize args as a tuple using pooled serialization
    let encode_expr = if arg_names.is_empty() {
        quote! { #rapace_crate::postcard_to_pooled_buf(self.session.buffer_pool(), &())? }
    } else if arg_names.len() == 1 {
        let arg = &arg_names[0];
        quote! { #rapace_crate::postcard_to_pooled_buf(self.session.buffer_pool(), &#arg)? }
    } else {
        quote! { #rapace_crate::postcard_to_pooled_buf(self.session.buffer_pool(), &(#(#arg_names.clone()),*))? }
    };

    quote! {
        /// Call the #name server-streaming method on the remote service.
        ///
        /// Returns a stream that yields items as they arrive from the server.
        /// The stream ends when the server sends EOS, or yields an error if
        /// the server sends an ERROR frame.
        pub async fn #name(&self, #(#fn_args),*) -> ::std::result::Result<#rapace_crate::rapace_core::Streaming<#item_type>, #rapace_crate::rapace_core::RpcError> {
            use #rapace_crate::rapace_core::{ErrorCode, RpcError};

            #rapace_crate::tracing::debug!(
                service = #service_name,
                method = #method_name_str,
                method_id = #method_id,
                "RPC streaming call start"
            );

            let request_bytes = #encode_expr;

            // Start the streaming call - this registers a tunnel and sends the request
            let rx = self.session
                .start_streaming_call_pooled(#method_id, request_bytes)
                .await?;

            // Build a Stream<Item = Result<#item_type, RpcError>> with explicit termination on EOS
            let stream = #rapace_crate::rapace_core::futures_stream::unfold(rx, |mut rx| async move {
                let chunk = rx.recv().await?;

                // Error chunk - parse and return as error
                if chunk.is_error() {
                    let err = #rapace_crate::rapace_core::parse_error_payload(chunk.payload_bytes());
                    return Some((Err(err), rx));
                }

                // Empty EOS chunk - stream is done
                if chunk.is_eos() && chunk.payload_bytes().is_empty() {
                    return None;
                }

                // DATA chunk (possibly with EOS flag for final item) - deserialize
                let result: ::std::result::Result<#item_type, RpcError> = #rapace_crate::facet_postcard::from_slice(chunk.payload_bytes())
                    .map_err(|e| RpcError::Status {
                        code: ErrorCode::Internal,
                        message: ::std::format!("deserialize error: {:?}", e),
                    });

                match result {
                    Ok(item) => Some((Ok(item), rx)),
                    Err(err) => Some((Err(err), rx)),
                }
            });

            Ok(::std::boxed::Box::pin(stream))
        }
    }
}

fn generate_dispatch_arm(
    method: &MethodInfo,
    method_id: u32,
    rapace_crate: &TokenStream2,
) -> TokenStream2 {
    match &method.kind {
        MethodKind::Unary => generate_dispatch_arm_unary(method, method_id, rapace_crate),
        MethodKind::ServerStreaming { .. } => {
            // Streaming methods are handled by dispatch_streaming, not dispatch
            // For the dispatch() method, return error for streaming methods
            quote! {
                #method_id => {
                    Err(#rapace_crate::rapace_core::RpcError::Status {
                        code: #rapace_crate::rapace_core::ErrorCode::Internal,
                        message: "streaming method called via unary dispatch".into(),
                    })
                }
            }
        }
    }
}

fn generate_streaming_dispatch_arm(
    method: &MethodInfo,
    method_id: u32,
    rapace_crate: &TokenStream2,
) -> TokenStream2 {
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
                    let #arg: #ty = #rapace_crate::facet_postcard::from_slice(request_frame.payload_bytes())
                        .map_err(|e| #rapace_crate::rapace_core::RpcError::Status {
                            code: #rapace_crate::rapace_core::ErrorCode::InvalidArgument,
                            message: ::std::format!("deserialize error: {:?}", e),
                        })?;
                    let result: #return_type = self.service.#name(#arg).await;
                }
            } else {
                let tuple_type = quote! { (#(#arg_types),*) };
                quote! {
                    let (#(#arg_names),*): #tuple_type = #rapace_crate::facet_postcard::from_slice(request_frame.payload_bytes())
                        .map_err(|e| #rapace_crate::rapace_core::RpcError::Status {
                            code: #rapace_crate::rapace_core::ErrorCode::InvalidArgument,
                            message: ::std::format!("deserialize error: {:?}", e),
                        })?;
                    let result: #return_type = self.service.#name(#(#arg_names),*).await;
                }
            };

            quote! {
                #method_id => {
                    #decode_and_call

                    // Encode and send response frame using pooled serialization
                    let response_bytes = #rapace_crate::postcard_to_pooled_buf(
                        transport.buffer_pool(),
                        &result
                    )?;

                    let mut desc = #rapace_crate::rapace_core::MsgDescHot::new();
                    desc.channel_id = channel_id;
                    // Spec: `[impl core.call.response.flags]` - responses have RESPONSE flag
                    desc.flags = #rapace_crate::rapace_core::FrameFlags::DATA | #rapace_crate::rapace_core::FrameFlags::EOS | #rapace_crate::rapace_core::FrameFlags::RESPONSE;

                    let frame = if response_bytes.len() <= #rapace_crate::rapace_core::INLINE_PAYLOAD_SIZE {
                        #rapace_crate::rapace_core::Frame::with_inline_payload(desc, &response_bytes)
                            .expect("inline payload should fit")
                    } else {
                        #rapace_crate::rapace_core::Frame::with_pooled_payload(desc, response_bytes)
                    };

                    transport.send_frame(frame).await
                        .map_err(#rapace_crate::rapace_core::RpcError::Transport)?;
                    Ok(())
                }
            }
        }
        MethodKind::ServerStreaming { .. } => {
            generate_streaming_dispatch_arm_server_streaming(method, method_id, rapace_crate)
        }
    }
}

fn generate_streaming_dispatch_arm_server_streaming(
    method: &MethodInfo,
    method_id: u32,
    rapace_crate: &TokenStream2,
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
            let #arg: #ty = #rapace_crate::facet_postcard::from_slice(request_frame.payload_bytes())
                .map_err(|e| #rapace_crate::rapace_core::RpcError::Status {
                    code: #rapace_crate::rapace_core::ErrorCode::InvalidArgument,
                    message: ::std::format!("deserialize error: {:?}", e),
                })?;
        }
    } else {
        let tuple_type = quote! { (#(#arg_types),*) };
        quote! {
            let (#(#arg_names),*): #tuple_type = #rapace_crate::facet_postcard::from_slice(request_frame.payload_bytes())
                .map_err(|e| #rapace_crate::rapace_core::RpcError::Status {
                    code: #rapace_crate::rapace_core::ErrorCode::InvalidArgument,
                    message: ::std::format!("deserialize error: {:?}", e),
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
            use #rapace_crate::futures::stream::StreamExt;
            #rapace_crate::tracing::debug!(channel_id, "streaming dispatch: starting to iterate stream");

            loop {
                #rapace_crate::tracing::trace!(channel_id, "streaming dispatch: waiting for next item");
                match stream.next().await {
                    Some(Ok(item)) => {
                        #rapace_crate::tracing::debug!(channel_id, "streaming dispatch: got item, encoding");
                        // Encode item using pooled serialization
                        let item_bytes = #rapace_crate::postcard_to_pooled_buf(
                            transport.buffer_pool(),
                            &item
                        )?;

                        // Send DATA frame (not EOS yet)
                        let mut desc = #rapace_crate::rapace_core::MsgDescHot::new();
                        desc.channel_id = channel_id;
                        desc.flags = #rapace_crate::rapace_core::FrameFlags::DATA;

                        let frame = if item_bytes.len() <= #rapace_crate::rapace_core::INLINE_PAYLOAD_SIZE {
                            #rapace_crate::rapace_core::Frame::with_inline_payload(desc, &item_bytes)
                                .expect("inline payload should fit")
                        } else {
                            #rapace_crate::rapace_core::Frame::with_pooled_payload(desc, item_bytes)
                        };

                        #rapace_crate::tracing::debug!(channel_id, payload_len = frame.payload_bytes().len(), "streaming dispatch: sending DATA frame");
                        transport.send_frame(frame).await
                            .map_err(#rapace_crate::rapace_core::RpcError::Transport)?;
                        #rapace_crate::tracing::debug!(channel_id, "streaming dispatch: DATA frame sent");
                    }
                    Some(Err(err)) => {
                        #rapace_crate::tracing::warn!(channel_id, ?err, "streaming dispatch: got error from stream");
                        // Send ERROR frame and break
                        let mut desc = #rapace_crate::rapace_core::MsgDescHot::new();
                        desc.channel_id = channel_id;
                        desc.flags = #rapace_crate::rapace_core::FrameFlags::ERROR | #rapace_crate::rapace_core::FrameFlags::EOS;

                        // Encode error: [code: u32 LE][message_len: u32 LE][message bytes]
                        let (code, message): (u32, String) = match &err {
                            #rapace_crate::rapace_core::RpcError::Status { code, message } => (*code as u32, message.clone()),
                            #rapace_crate::rapace_core::RpcError::Transport(_) => (#rapace_crate::rapace_core::ErrorCode::Internal as u32, "transport error".into()),
                            #rapace_crate::rapace_core::RpcError::Cancelled => (#rapace_crate::rapace_core::ErrorCode::Cancelled as u32, "cancelled".into()),
                            #rapace_crate::rapace_core::RpcError::DeadlineExceeded => (#rapace_crate::rapace_core::ErrorCode::DeadlineExceeded as u32, "deadline exceeded".into()),
                            #rapace_crate::rapace_core::RpcError::Serialize(e) => (#rapace_crate::rapace_core::ErrorCode::Internal as u32, format!("serialize error: {}", e)),
                            #rapace_crate::rapace_core::RpcError::Deserialize(e) => (#rapace_crate::rapace_core::ErrorCode::Internal as u32, format!("deserialize error: {}", e)),
                        };
                        let mut err_bytes = Vec::with_capacity(8 + message.len());
                        err_bytes.extend_from_slice(&code.to_le_bytes());
                        err_bytes.extend_from_slice(&(message.len() as u32).to_le_bytes());
                        err_bytes.extend_from_slice(message.as_bytes());

                        let frame = #rapace_crate::rapace_core::Frame::with_payload(desc, err_bytes);
                        transport.send_frame(frame).await
                            .map_err(#rapace_crate::rapace_core::RpcError::Transport)?;
                        return Ok(());
                    }
                    None => {
                        #rapace_crate::tracing::debug!(channel_id, "streaming dispatch: stream ended, sending EOS");
                        // Stream is complete: send EOS frame
                        let mut desc = #rapace_crate::rapace_core::MsgDescHot::new();
                        desc.channel_id = channel_id;
                        desc.flags = #rapace_crate::rapace_core::FrameFlags::EOS;
                        let frame = #rapace_crate::rapace_core::Frame::new(desc);
                        transport.send_frame(frame).await
                            .map_err(#rapace_crate::rapace_core::RpcError::Transport)?;
                        #rapace_crate::tracing::debug!(channel_id, "streaming dispatch: EOS sent, returning");
                        return Ok(());
                    }
                }
            }
        }
    }
}

fn generate_dispatch_arm_unary(
    method: &MethodInfo,
    method_id: u32,
    rapace_crate: &TokenStream2,
) -> TokenStream2 {
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
            let #arg: #ty = #rapace_crate::facet_postcard::from_slice(request_frame.payload_bytes())
                .map_err(|e| #rapace_crate::rapace_core::RpcError::Status {
                    code: #rapace_crate::rapace_core::ErrorCode::InvalidArgument,
                    message: ::std::format!("deserialize error: {:?}", e),
                })?;
            let result: #return_type = self.service.#name(#arg).await;
        }
    } else {
        // Multiple args - decode as tuple
        let tuple_type = quote! { (#(#arg_types),*) };
        quote! {
            let (#(#arg_names),*): #tuple_type = #rapace_crate::facet_postcard::from_slice(request_frame.payload_bytes())
                .map_err(|e| #rapace_crate::rapace_core::RpcError::Status {
                    code: #rapace_crate::rapace_core::ErrorCode::InvalidArgument,
                    message: ::std::format!("deserialize error: {:?}", e),
                })?;
            let result: #return_type = self.service.#name(#(#arg_names),*).await;
        }
    };

    quote! {
        #method_id => {
            #decode_and_call

            // Encode response using pooled serialization
            let response_bytes = #rapace_crate::postcard_to_pooled_buf(buffer_pool, &result)?;

            // Build response frame
            // Spec: `[impl core.call.response.flags]` - responses have RESPONSE flag
            let mut desc = #rapace_crate::rapace_core::MsgDescHot::new();
            desc.flags = #rapace_crate::rapace_core::FrameFlags::DATA | #rapace_crate::rapace_core::FrameFlags::EOS | #rapace_crate::rapace_core::FrameFlags::RESPONSE;

            let frame = if response_bytes.len() <= #rapace_crate::rapace_core::INLINE_PAYLOAD_SIZE {
                #rapace_crate::rapace_core::Frame::with_inline_payload(desc, &response_bytes)
                    .expect("inline payload should fit")
            } else {
                #rapace_crate::rapace_core::Frame::with_pooled_payload(desc, response_bytes)
            };

            Ok(frame)
        }
    }
}

/// Generate the `register` function for service registration.
///
/// This generates a function that registers the service and its methods
/// with a `ServiceRegistry`, capturing request/response schemas via facet.
fn generate_register_fn(
    service_name: &str,
    service_doc: &str,
    methods: &[MethodInfo],
    rapace_crate: &TokenStream2,
    register_fn_name: &Ident,
    vis: &TokenStream2,
) -> TokenStream2 {
    let method_registrations: Vec<TokenStream2> = methods
        .iter()
        .map(|m| {
            let method_name = m.name.to_string();
            let method_doc = &m.doc;
            let arg_types: Vec<_> = m.args.iter().map(|(_, ty)| ty).collect();

            // Generate argument info
            let arg_infos: Vec<TokenStream2> = m
                .args
                .iter()
                .map(|(name, ty)| {
                    let name_str = name.to_string();
                    let type_str = quote!(#ty).to_string();
                    quote! {
                        #rapace_crate::registry::ArgInfo {
                            name: #name_str,
                            type_name: #type_str,
                        }
                    }
                })
                .collect();

            // Request shape: tuple of arg types, or () if no args, or single type if one arg
            let request_shape_expr = if arg_types.is_empty() {
                quote! { <() as #rapace_crate::facet_core::Facet>::SHAPE }
            } else if arg_types.len() == 1 {
                let ty = &arg_types[0];
                quote! { <#ty as #rapace_crate::facet_core::Facet>::SHAPE }
            } else {
                quote! { <(#(#arg_types),*) as #rapace_crate::facet_core::Facet>::SHAPE }
            };

            // Response shape: the return type (or inner type for streaming)
            let response_shape_expr = match &m.kind {
                MethodKind::Unary => {
                    let return_type = &m.return_type;
                    quote! { <#return_type as #rapace_crate::facet_core::Facet>::SHAPE }
                }
                MethodKind::ServerStreaming { item_type } => {
                    quote! { <#item_type as #rapace_crate::facet_core::Facet>::SHAPE }
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
        #vis fn #register_fn_name(registry: &mut #rapace_crate::registry::ServiceRegistry) {
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
    service_name: &str,
    rapace_crate: &TokenStream2,
) -> TokenStream2 {
    let name = &method.name;
    let method_name_str = name.to_string();
    let return_type = &method.return_type;
    let method_id_field = format_ident!("{}_method_id", name);

    let arg_names: Vec<_> = method.args.iter().map(|(name, _)| name).collect();
    let arg_types: Vec<_> = method.args.iter().map(|(_, ty)| ty).collect();

    let fn_args = arg_names.iter().zip(arg_types.iter()).map(|(name, ty)| {
        quote! { #name: #ty }
    });

    let encode_expr = if arg_names.is_empty() {
        quote! { #rapace_crate::postcard_to_pooled_buf(self.session.buffer_pool(), &())? }
    } else if arg_names.len() == 1 {
        let arg = &arg_names[0];
        quote! { #rapace_crate::postcard_to_pooled_buf(self.session.buffer_pool(), &#arg)? }
    } else {
        quote! { #rapace_crate::postcard_to_pooled_buf(self.session.buffer_pool(), &(#(#arg_names.clone()),*))? }
    };

    quote! {
        /// Call the #name method on the remote service.
        pub async fn #name(&self, #(#fn_args),*) -> ::std::result::Result<#return_type, #rapace_crate::rapace_core::RpcError> {
            use #rapace_crate::rapace_core::FrameFlags;

            let request_bytes = #encode_expr;

            // Call via session with registry-assigned method ID
            let channel_id = self.session.next_channel_id();
            #rapace_crate::tracing::debug!(
                service = #service_name,
                method = #method_name_str,
                method_id = self.#method_id_field,
                channel_id,
                "RPC call start"
            );
            let response = self.session.call_pooled(channel_id, self.#method_id_field, request_bytes).await?;
            #rapace_crate::tracing::debug!(
                service = #service_name,
                method = #method_name_str,
                method_id = self.#method_id_field,
                channel_id,
                "RPC call complete"
            );

            if response.flags().contains(FrameFlags::ERROR) {
                return Err(#rapace_crate::rapace_core::parse_error_payload(response.payload_bytes()));
            }

            let result: #return_type = #rapace_crate::facet_postcard::from_slice(response.payload_bytes())
                .map_err(|e| #rapace_crate::rapace_core::RpcError::Status {
                    code: #rapace_crate::rapace_core::ErrorCode::Internal,
                    message: ::std::format!("deserialize error: {:?}", e),
                })?;

            Ok(result)
        }
    }
}

/// Generate a server-streaming client method that uses a stored method ID from the registry.
fn generate_client_method_server_streaming_registry(
    method: &MethodInfo,
    _method_index: usize,
    service_name: &str,
    item_type: &TokenStream2,
    rapace_crate: &TokenStream2,
) -> TokenStream2 {
    let name = &method.name;
    let method_name_str = name.to_string();
    let method_id_field = format_ident!("{}_method_id", name);

    let arg_names: Vec<_> = method.args.iter().map(|(name, _)| name).collect();
    let arg_types: Vec<_> = method.args.iter().map(|(_, ty)| ty).collect();

    let fn_args = arg_names.iter().zip(arg_types.iter()).map(|(name, ty)| {
        quote! { #name: #ty }
    });

    // For encoding, serialize args as a tuple using pooled serialization
    let encode_expr = if arg_names.is_empty() {
        quote! { #rapace_crate::postcard_to_pooled_buf(self.session.buffer_pool(), &())? }
    } else if arg_names.len() == 1 {
        let arg = &arg_names[0];
        quote! { #rapace_crate::postcard_to_pooled_buf(self.session.buffer_pool(), &#arg)? }
    } else {
        quote! { #rapace_crate::postcard_to_pooled_buf(self.session.buffer_pool(), &(#(#arg_names.clone()),*))? }
    };

    quote! {
        /// Call the #name server-streaming method on the remote service.
        ///
        /// Returns a stream that yields items as they arrive from the server.
        /// The stream ends when the server sends EOS, or yields an error if
        /// the server sends an ERROR frame.
        pub async fn #name(&self, #(#fn_args),*) -> ::std::result::Result<#rapace_crate::rapace_core::Streaming<#item_type>, #rapace_crate::rapace_core::RpcError> {
            use #rapace_crate::rapace_core::{ErrorCode, RpcError};

            #rapace_crate::tracing::debug!(
                service = #service_name,
                method = #method_name_str,
                method_id = self.#method_id_field,
                "RPC streaming call start"
            );

            let request_bytes = #encode_expr;

            // Start the streaming call with registry-assigned method ID
            let rx = self.session
                .start_streaming_call_pooled(self.#method_id_field, request_bytes)
                .await?;

            // Build a Stream<Item = Result<#item_type, RpcError>> with explicit termination on EOS
            let stream = #rapace_crate::rapace_core::futures_stream::unfold(rx, |mut rx| async move {
                let chunk = rx.recv().await?;

                // Error chunk - parse and return as error
                if chunk.is_error() {
                    let err = #rapace_crate::rapace_core::parse_error_payload(chunk.payload_bytes());
                    return Some((Err(err), rx));
                }

                // Empty EOS chunk - stream is done
                if chunk.is_eos() && chunk.payload_bytes().is_empty() {
                    return None;
                }

                // DATA chunk (possibly with EOS flag for final item) - deserialize
                let result: ::std::result::Result<#item_type, RpcError> = #rapace_crate::facet_postcard::from_slice(chunk.payload_bytes())
                    .map_err(|e| RpcError::Status {
                        code: ErrorCode::Internal,
                        message: ::std::format!("deserialize error: {:?}", e),
                    });

                match result {
                    Ok(item) => Some((Ok(item), rx)),
                    Err(err) => Some((Err(err), rx)),
                }
            });

            Ok(::std::boxed::Box::pin(stream))
        }
    }
}
