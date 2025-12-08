// examples/service_macro_demo.rs
//
//! Demonstration of the service_macro module for defining RPC services.
//!
//! This example shows how to use `define_service!` to create type-safe RPC services
//! without requiring proc-macros.

use rapace::define_service;
use rapace::error::Result;
use rapace::service_macro::Method;
use serde::{Deserialize, Serialize};

// Define request/response types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EchoRequest {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EchoResponse {
    pub message: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GreetRequest {
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GreetResponse {
    pub greeting: String,
}

// Define the service
define_service! {
    /// An example RPC service demonstrating unary calls
    service Echo {
        /// Echo back the received message
        rpc echo(EchoRequest) -> EchoResponse;

        /// Greet a person by name
        rpc greet(GreetRequest) -> GreetResponse;
    }
}

// Implement the server handler
struct EchoService;

impl EchoHandler for EchoService {
    async fn echo(&self, req: EchoRequest) -> Result<EchoResponse> {
        Ok(EchoResponse {
            message: req.message,
        })
    }

    async fn greet(&self, req: GreetRequest) -> Result<GreetResponse> {
        Ok(GreetResponse {
            greeting: format!("Hello, {}!", req.name),
        })
    }
}

fn main() {
    println!("Service Macro Demo");
    println!("==================");
    println!();
    // Use a helper to show the service name since the trait isn't dyn compatible
    struct DemoEcho;
    impl Echo for DemoEcho {}
    println!("Service: {}", DemoEcho::NAME);
    println!();
    println!("Methods:");
    println!("  - {}: ID {}", echo::NAME, echo::ID);
    println!("  - {}: ID {}", greet::NAME, greet::ID);
    println!();
    println!("Method kinds:");
    println!("  - {}: {:?}", echo::NAME, echo::kind());
    println!("  - {}: {:?}", greet::NAME, greet::kind());
    println!();

    // Create a client
    let client = EchoClient::new();
    println!("Created client: {:?}", std::any::type_name_of_val(&client));

    // Create a server handler
    let _handler = EchoService;
    println!("Created handler: EchoService");
}
