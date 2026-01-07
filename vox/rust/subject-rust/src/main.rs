//! Rust subject binary for the roam compliance suite.
//!
//! This demonstrates the minimal code needed to implement a roam service
//! using the roam-stream transport library.

use roam_stream::Server;

// Re-export types from spec_proto for use in generated code
pub use spec_proto::{Canvas, Color, Message, Person, Point, Rectangle, Shape};

// Include generated code (echo::EchoHandler, echo::EchoDispatcher, etc.)
include!(concat!(env!("OUT_DIR"), "/generated.rs"));

// Service implementation using generated EchoHandler trait
struct EchoService;

#[allow(clippy::manual_async_fn)]
impl echo::EchoHandler for EchoService {
    fn echo(
        &self,
        message: String,
    ) -> impl std::future::Future<Output = Result<String, Box<dyn std::error::Error + Send + Sync>>> + Send
    {
        async move { Ok(message) }
    }

    fn reverse(
        &self,
        message: String,
    ) -> impl std::future::Future<Output = Result<String, Box<dyn std::error::Error + Send + Sync>>> + Send
    {
        async move { Ok(message.chars().rev().collect()) }
    }
}

fn main() -> Result<(), String> {
    // Manual runtime (avoid tokio-macros / syn).
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("failed to create tokio runtime: {e}"))?;

    rt.block_on(async {
        let server = Server::new();
        // Use generated dispatcher with our service implementation
        let dispatcher = echo::EchoDispatcher::new(EchoService);
        server
            .run_subject(&dispatcher)
            .await
            .map_err(|e| format!("{e:?}"))
    })
}
