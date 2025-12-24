//! Template Engine with Host Callbacks - Demo Binary
//!
//! This example demonstrates a **bidirectional service pattern** where:
//! - The **host** provides a `ValueHost` service for lazy value lookups
//! - The **plugin** provides a `TemplateEngine` service that renders templates
//! - During `render()`, the plugin **calls back into the host** to fetch values
//!
//! ## Architecture
//!
//! ```text
//!                    host_session                 plugin_session
//! ┌─────────────────────┐     │                     │     ┌─────────────────────┐
//! │        HOST         │     │                     │     │       PLUGIN        │
//! ├─────────────────────┤     │                     │     ├─────────────────────┤
//! │                     │     │                     │     │                     │
//! │ ValueHostServer ◄───┼─────┼── get_value() ──────┼─────┼─ (via RpcSession)   │
//! │  (dispatcher)       │     │                     │     │                     │
//! │                     │     │                     │     │                     │
//! │ (via RpcSession) ───┼─────┼── render() ─────────┼─────┼►TemplateEngineServer│
//! │                     │     │                     │     │  (dispatcher)       │
//! └─────────────────────┘     │                     │     └─────────────────────┘
//! ```

use std::sync::Arc;

use rapace::{RpcSession, Transport};

// Import from library
use rapace_template_engine::{
    TemplateEngineClient, ValueHostImpl, create_template_engine_dispatcher,
    create_value_host_dispatcher,
};

fn main() {
    let rt = tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
        .unwrap();
    rt.block_on(async_main());
}

async fn async_main() {
    println!("=== Template Engine with Host Callbacks Demo ===\n");

    // Create a transport pair (in-memory for demo)
    let (host_transport, cell_transport) = Transport::mem_pair();

    // Set up the value host with some test data
    let mut value_host_impl = ValueHostImpl::new();
    value_host_impl.set("user.name", "Alice");
    value_host_impl.set("site.title", "MySite");
    value_host_impl.set("site.domain", "example.com");
    let value_host_impl = Arc::new(value_host_impl);

    println!("Values configured:");
    println!("  user.name = Alice");
    println!("  site.title = MySite");
    println!("  site.domain = example.com");
    println!();

    // ========== HOST SIDE ==========
    // Create RpcSession for the host (uses odd channel IDs: 1, 3, 5, ...)
    let host_session = Arc::new(RpcSession::with_channel_start(host_transport, 1));

    // Set dispatcher for ValueHost service
    host_session.set_dispatcher(create_value_host_dispatcher(
        value_host_impl.clone(),
        host_session.buffer_pool().clone(),
    ));

    // Spawn the host's demux loop
    let host_session_clone = host_session.clone();
    let host_handle = tokio::spawn(async move { host_session_clone.run().await });

    // ========== CELL SIDE ==========
    // Create RpcSession for the cell (uses even channel IDs: 2, 4, 6, ...)
    let cell_session = Arc::new(RpcSession::with_channel_start(cell_transport, 2));

    // Set dispatcher for TemplateEngine service
    cell_session.set_dispatcher(create_template_engine_dispatcher(
        cell_session.clone(),
        cell_session.buffer_pool().clone(),
    ));

    // Spawn the cell's demux loop
    let cell_session_clone = cell_session.clone();
    let cell_handle = tokio::spawn(async move { cell_session_clone.run().await });

    // ========== MAKE RPC CALLS ==========
    let client = TemplateEngineClient::new(host_session.clone());

    // Test 1: Simple template
    println!("--- Test 1: Simple Template ---");
    let template = "Hello, {{user.name}}!";
    println!("Template: {}", template);
    match client.render(template.to_string()).await {
        Ok(rendered) => println!("Rendered: {}", rendered),
        Err(e) => println!("Error: {:?}", e),
    }
    println!();

    // Test 2: Multiple placeholders
    println!("--- Test 2: Multiple Placeholders ---");
    let template = "Welcome to {{site.title}} at {{site.domain}}, {{user.name}}!";
    println!("Template: {}", template);
    match client.render(template.to_string()).await {
        Ok(rendered) => println!("Rendered: {}", rendered),
        Err(e) => println!("Error: {:?}", e),
    }
    println!();

    // Test 3: Missing value
    println!("--- Test 3: Missing Value ---");
    let template = "Contact: {{user.email}}";
    println!("Template: {}", template);
    match client.render(template.to_string()).await {
        Ok(rendered) => println!("Rendered: {}", rendered),
        Err(e) => println!("Error: {:?}", e),
    }
    println!();

    // Clean up
    host_session.close();
    cell_session.close();
    host_handle.abort();
    cell_handle.abort();

    println!("=== Demo Complete ===");
}

// ============================================================================
// Tests (in-process only - cross-process tests use the helper binary)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to run template engine scenario with RpcSession
    async fn run_scenario(host_transport: Transport, cell_transport: Transport) {
        // Set up values
        let mut value_host_impl = ValueHostImpl::new();
        value_host_impl.set("user.name", "Alice");
        value_host_impl.set("site.title", "MySite");
        let value_host_impl = Arc::new(value_host_impl);

        // Host session (odd channel IDs)
        let host_session = Arc::new(RpcSession::with_channel_start(host_transport, 1));
        host_session.set_dispatcher(create_value_host_dispatcher(
            value_host_impl.clone(),
            host_session.buffer_pool().clone(),
        ));
        let host_session_clone = host_session.clone();
        let host_handle = tokio::spawn(async move { host_session_clone.run().await });

        // Cell session (even channel IDs)
        let cell_session = Arc::new(RpcSession::with_channel_start(cell_transport, 2));
        cell_session.set_dispatcher(create_template_engine_dispatcher(
            cell_session.clone(),
            cell_session.buffer_pool().clone(),
        ));
        let cell_session_clone = cell_session.clone();
        let cell_handle = tokio::spawn(async move { cell_session_clone.run().await });

        // Test the scenario using the generated client
        let client = TemplateEngineClient::new(host_session.clone());
        let rendered = client
            .render("Hi {{user.name}} - {{site.title}}".to_string())
            .await
            .unwrap();
        assert_eq!(rendered, "Hi Alice - MySite");

        // Cleanup
        host_session.close();
        cell_session.close();
        host_handle.abort();
        cell_handle.abort();
    }

    #[tokio_test_lite::test]
    async fn test_mem_transport() {
        let (host_transport, cell_transport) = Transport::mem_pair();
        run_scenario(host_transport, cell_transport).await;
    }

    #[tokio_test_lite::test]
    async fn test_simple_placeholder() {
        let (host_transport, cell_transport) = Transport::mem_pair();

        let mut value_host_impl = ValueHostImpl::new();
        value_host_impl.set("user.name", "Bob");
        let value_host_impl = Arc::new(value_host_impl);

        let host_session = Arc::new(RpcSession::with_channel_start(host_transport, 1));
        host_session.set_dispatcher(create_value_host_dispatcher(
            value_host_impl.clone(),
            host_session.buffer_pool().clone(),
        ));
        let host_session_clone = host_session.clone();
        let host_handle = tokio::spawn(async move { host_session_clone.run().await });

        let cell_session = Arc::new(RpcSession::with_channel_start(cell_transport, 2));
        cell_session.set_dispatcher(create_template_engine_dispatcher(
            cell_session.clone(),
            cell_session.buffer_pool().clone(),
        ));
        let cell_session_clone = cell_session.clone();
        let cell_handle = tokio::spawn(async move { cell_session_clone.run().await });

        let client = TemplateEngineClient::new(host_session.clone());
        let rendered = client
            .render("Hello, {{user.name}}!".to_string())
            .await
            .unwrap();
        assert_eq!(rendered, "Hello, Bob!");

        host_session.close();
        cell_session.close();
        host_handle.abort();
        cell_handle.abort();
    }

    #[tokio_test_lite::test]
    async fn test_multiple_placeholders() {
        let (host_transport, cell_transport) = Transport::mem_pair();

        let mut value_host_impl = ValueHostImpl::new();
        value_host_impl.set("user.name", "Alice");
        value_host_impl.set("site.name", "TestSite");
        value_host_impl.set("site.domain", "test.com");
        let value_host_impl = Arc::new(value_host_impl);

        let host_session = Arc::new(RpcSession::with_channel_start(host_transport, 1));
        host_session.set_dispatcher(create_value_host_dispatcher(
            value_host_impl.clone(),
            host_session.buffer_pool().clone(),
        ));
        let host_session_clone = host_session.clone();
        let host_handle = tokio::spawn(async move { host_session_clone.run().await });

        let cell_session = Arc::new(RpcSession::with_channel_start(cell_transport, 2));
        cell_session.set_dispatcher(create_template_engine_dispatcher(
            cell_session.clone(),
            cell_session.buffer_pool().clone(),
        ));
        let cell_session_clone = cell_session.clone();
        let cell_handle = tokio::spawn(async move { cell_session_clone.run().await });

        let client = TemplateEngineClient::new(host_session.clone());
        let rendered = client
            .render("Hi {{user.name}} from {{site.name}} on {{site.domain}}".to_string())
            .await
            .unwrap();
        assert_eq!(rendered, "Hi Alice from TestSite on test.com");

        host_session.close();
        cell_session.close();
        host_handle.abort();
        cell_handle.abort();
    }

    #[tokio_test_lite::test]
    async fn test_missing_value() {
        let (host_transport, cell_transport) = Transport::mem_pair();

        let value_host_impl = Arc::new(ValueHostImpl::new()); // Empty

        let host_session = Arc::new(RpcSession::with_channel_start(host_transport, 1));
        host_session.set_dispatcher(create_value_host_dispatcher(
            value_host_impl.clone(),
            host_session.buffer_pool().clone(),
        ));
        let host_session_clone = host_session.clone();
        let host_handle = tokio::spawn(async move { host_session_clone.run().await });

        let cell_session = Arc::new(RpcSession::with_channel_start(cell_transport, 2));
        cell_session.set_dispatcher(create_template_engine_dispatcher(
            cell_session.clone(),
            cell_session.buffer_pool().clone(),
        ));
        let cell_session_clone = cell_session.clone();
        let cell_handle = tokio::spawn(async move { cell_session_clone.run().await });

        let client = TemplateEngineClient::new(host_session.clone());
        let rendered = client
            .render("Hello, {{user.name}}!".to_string())
            .await
            .unwrap();
        assert_eq!(rendered, "Hello, !");

        host_session.close();
        cell_session.close();
        host_handle.abort();
        cell_handle.abort();
    }
}
