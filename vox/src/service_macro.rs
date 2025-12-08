// src/service_macro.rs

//! Declarative macros for defining RPC services without proc-macros.
//!
//! This module provides `define_service!` and related macros for generating
//! type-safe RPC bindings, client stubs, and server traits.

use crate::error::Result as RapaceResult;
use crate::registry::MethodKind;
use serde::{de::DeserializeOwned, Serialize};
use std::marker::PhantomData;

/// Marker trait for RPC methods.
pub trait Method {
    const NAME: &'static str;
    const ID: u32;
    type Request: Serialize + DeserializeOwned;
    type Response: Serialize + DeserializeOwned;
    fn kind() -> MethodKind;
}

/// Marker trait for unary RPC methods.
pub trait UnaryMethod: Method {}

/// Marker trait for client streaming RPC methods.
pub trait ClientStreamingMethod: Method {}

/// Marker trait for server streaming RPC methods.
pub trait ServerStreamingMethod: Method {}

/// Marker trait for bidirectional streaming RPC methods.
pub trait BidirectionalMethod: Method {}

/// Placeholder for request streams.
pub struct RequestStream<T> {
    _phantom: PhantomData<T>,
}

impl<T> RequestStream<T> {
    pub async fn recv(&mut self) -> Option<T> {
        unimplemented!("RequestStream::recv - requires async runtime integration")
    }
}

/// Placeholder for response sinks.
pub struct ResponseSink<T> {
    _phantom: PhantomData<T>,
}

impl<T> ResponseSink<T> {
    pub async fn send(&mut self, _msg: T) -> RapaceResult<()> {
        unimplemented!("ResponseSink::send - requires async runtime integration")
    }

    pub async fn close(self) -> RapaceResult<()> {
        Ok(())
    }
}

/// Main macro for defining RPC services - simplified version
#[macro_export]
macro_rules! define_service {
    (
        $(#[$service_meta:meta])*
        service $service_name:ident {
            $(
                $(#[$method_meta:meta])*
                rpc $method_name:ident ( $req_type:ty ) -> $resp_type:ty ;
            )*
        }
    ) => {
        // Service trait
        $(#[$service_meta])*
        pub trait $service_name {
            const NAME: &'static str = stringify!($service_name);
        }

        // Method counter for ID generation
        const _START: u32 = $crate::dispatch::USER_METHOD_ID_START;

        // Generate methods
        $crate::__gen_methods! {
            counter 0;
            [
                $( { meta: [$(#[$method_meta])*], name: $method_name, req: $req_type, resp: $resp_type } )*
            ]
        }

        // Client stub
        paste::paste! {
            pub struct [<$service_name Client>] {
                _phantom: std::marker::PhantomData<()>,
            }

            impl [<$service_name Client>] {
                pub fn new() -> Self {
                    Self { _phantom: std::marker::PhantomData }
                }

                $(
                    $(#[$method_meta])*
                    pub async fn $method_name(&self, _req: $req_type) -> $crate::error::Result<$resp_type> {
                        unimplemented!(concat!("Client method `", stringify!($method_name), "`"))
                    }
                )*
            }

            impl Default for [<$service_name Client>] {
                fn default() -> Self {
                    Self::new()
                }
            }

            // Server handler trait
            #[allow(async_fn_in_trait)]
            pub trait [<$service_name Handler>] {
                $(
                    $(#[$method_meta])*
                    async fn $method_name(&self, req: $req_type) -> $crate::error::Result<$resp_type>;
                )*
            }
        }
    };
}

/// Internal macro for generating method structs with incrementing IDs
#[doc(hidden)]
#[macro_export]
macro_rules! __gen_methods {
    // Base case
    (counter $counter:expr; []) => {};

    // Recursive case
    (
        counter $counter:expr;
        [
            { meta: [$(#[$meta:meta])*], name: $name:ident, req: $req:ty, resp: $resp:ty }
            $($rest:tt)*
        ]
    ) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy)]
        pub struct $name;

        impl $crate::service_macro::Method for $name {
            const NAME: &'static str = stringify!($name);
            const ID: u32 = _START + $counter;
            type Request = $req;
            type Response = $resp;

            fn kind() -> $crate::registry::MethodKind {
                $crate::registry::MethodKind::Unary
            }
        }

        impl $crate::service_macro::UnaryMethod for $name {}

        $crate::__gen_methods! {
            counter $counter + 1;
            [$($rest)*]
        }
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dispatch::USER_METHOD_ID_START;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    pub struct LogRequest {
        pub level: String,
        pub message: String,
    }

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    pub struct LogResponse {
        pub success: bool,
    }

    define_service! {
        /// A logging service for testing
        service Logger {
            /// Log a message at the given level
            rpc log(LogRequest) -> LogResponse;

            /// Log another message
            rpc log2(LogRequest) -> LogResponse;
        }
    }

    #[test]
    fn test_service_trait() {
        struct TestService;
        impl Logger for TestService {}
        assert_eq!(TestService::NAME, "Logger");
    }

    #[test]
    fn test_method_structs() {
        assert_eq!(log::NAME, "log");
        assert_eq!(log::ID, USER_METHOD_ID_START);
        assert_eq!(log::kind(), MethodKind::Unary);

        assert_eq!(log2::NAME, "log2");
        assert_eq!(log2::ID, USER_METHOD_ID_START + 1);
    }

    #[test]
    fn test_client_stub_exists() {
        let client = LoggerClient::new();
        assert!(std::mem::size_of_val(&client) >= 0);
    }

    #[test]
    fn test_handler_trait_compiles() {
        struct TestHandler;

        impl LoggerHandler for TestHandler {
            async fn log(&self, req: LogRequest) -> RapaceResult<LogResponse> {
                Ok(LogResponse {
                    success: !req.message.is_empty(),
                })
            }

            async fn log2(&self, req: LogRequest) -> RapaceResult<LogResponse> {
                Ok(LogResponse {
                    success: !req.message.is_empty(),
                })
            }
        }

        let _handler = TestHandler;
    }
}
