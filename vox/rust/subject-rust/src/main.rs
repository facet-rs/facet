//! Rust subject binary for the roam compliance suite.
//!
//! This demonstrates the minimal code needed to implement a roam service
//! using the roam-stream transport library.

use roam_stream::{Server, ServiceDispatcher, StreamRegistry};

// Service implementation
struct EchoService;

impl spec_proto::Echo for EchoService {
    async fn echo(&self, message: String) -> String {
        message
    }

    async fn reverse(&self, message: String) -> String {
        message.chars().rev().collect()
    }
}

// Dispatcher wraps the generated dispatch function
struct EchoDispatcher(EchoService);

impl ServiceDispatcher for EchoDispatcher {
    fn is_streaming(&self, _method_id: u64) -> bool {
        // Echo service has no streaming methods
        false
    }

    async fn dispatch_unary(&self, method_id: u64, payload: &[u8]) -> Result<Vec<u8>, String> {
        spec_proto::echo_dispatch_unary(&self.0, method_id, payload)
            .await
            .map_err(|e| format!("{e:?}"))
    }

    fn dispatch_streaming(
        &self,
        method_id: u64,
        _payload: &[u8],
        _registry: &mut StreamRegistry,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<u8>, String>> + Send + '_>>
    {
        // Echo service has no streaming methods
        Box::pin(async move { Err(format!("no streaming methods: {method_id}")) })
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
        server
            .run_subject(&EchoDispatcher(EchoService))
            .await
            .map_err(|e| format!("{e:?}"))
    })
}
