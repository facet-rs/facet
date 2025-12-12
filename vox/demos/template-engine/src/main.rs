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

use rapace::{InProcTransport, RpcSession, Transport};

// Import from library
use rapace_template_engine::{
    TemplateEngineClient, ValueHostImpl, create_template_engine_dispatcher,
    create_value_host_dispatcher,
};

#[tokio::main]
async fn main() {
    println!("=== Template Engine with Host Callbacks Demo ===\n");

    // Create a transport pair (in-memory for demo)
    let (host_transport, plugin_transport) = InProcTransport::pair();
    let host_transport = Arc::new(host_transport);
    let plugin_transport = Arc::new(plugin_transport);

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
    let host_session = Arc::new(RpcSession::with_channel_start(host_transport.clone(), 1));

    // Set dispatcher for ValueHost service
    host_session.set_dispatcher(create_value_host_dispatcher(value_host_impl.clone()));

    // Spawn the host's demux loop
    let host_session_clone = host_session.clone();
    let host_handle = tokio::spawn(async move { host_session_clone.run().await });

    // ========== PLUGIN SIDE ==========
    // Create RpcSession for the plugin (uses even channel IDs: 2, 4, 6, ...)
    let plugin_session = Arc::new(RpcSession::with_channel_start(plugin_transport.clone(), 2));

    // Set dispatcher for TemplateEngine service
    plugin_session.set_dispatcher(create_template_engine_dispatcher(plugin_session.clone()));

    // Spawn the plugin's demux loop
    let plugin_session_clone = plugin_session.clone();
    let plugin_handle = tokio::spawn(async move { plugin_session_clone.run().await });

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
    let _ = host_transport.close().await;
    let _ = plugin_transport.close().await;
    host_handle.abort();
    plugin_handle.abort();

    println!("=== Demo Complete ===");
}

// ============================================================================
// Tests (in-process only - cross-process tests use the helper binary)
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper to run template engine scenario with RpcSession
    async fn run_scenario<T: Transport + Send + Sync + 'static>(
        host_transport: Arc<T>,
        plugin_transport: Arc<T>,
    ) {
        // Set up values
        let mut value_host_impl = ValueHostImpl::new();
        value_host_impl.set("user.name", "Alice");
        value_host_impl.set("site.title", "MySite");
        let value_host_impl = Arc::new(value_host_impl);

        // Host session (odd channel IDs)
        let host_session = Arc::new(RpcSession::with_channel_start(host_transport.clone(), 1));
        host_session.set_dispatcher(create_value_host_dispatcher(value_host_impl.clone()));
        let host_session_clone = host_session.clone();
        let host_handle = tokio::spawn(async move { host_session_clone.run().await });

        // Plugin session (even channel IDs)
        let plugin_session = Arc::new(RpcSession::with_channel_start(plugin_transport.clone(), 2));
        plugin_session.set_dispatcher(create_template_engine_dispatcher(plugin_session.clone()));
        let plugin_session_clone = plugin_session.clone();
        let plugin_handle = tokio::spawn(async move { plugin_session_clone.run().await });

        // Test the scenario using the generated client
        let client = TemplateEngineClient::new(host_session.clone());
        let rendered = client
            .render("Hi {{user.name}} - {{site.title}}".to_string())
            .await
            .unwrap();
        assert_eq!(rendered, "Hi Alice - MySite");

        // Cleanup
        let _ = host_transport.close().await;
        let _ = plugin_transport.close().await;
        host_handle.abort();
        plugin_handle.abort();
    }

    #[tokio::test]
    async fn test_inproc_transport() {
        let (host_transport, plugin_transport) = InProcTransport::pair();
        run_scenario(Arc::new(host_transport), Arc::new(plugin_transport)).await;
    }

    #[tokio::test]
    async fn test_simple_placeholder() {
        let (host_transport, plugin_transport) = InProcTransport::pair();
        let host_transport = Arc::new(host_transport);
        let plugin_transport = Arc::new(plugin_transport);

        let mut value_host_impl = ValueHostImpl::new();
        value_host_impl.set("user.name", "Bob");
        let value_host_impl = Arc::new(value_host_impl);

        let host_session = Arc::new(RpcSession::with_channel_start(host_transport.clone(), 1));
        host_session.set_dispatcher(create_value_host_dispatcher(value_host_impl.clone()));
        let host_session_clone = host_session.clone();
        let host_handle = tokio::spawn(async move { host_session_clone.run().await });

        let plugin_session = Arc::new(RpcSession::with_channel_start(plugin_transport.clone(), 2));
        plugin_session.set_dispatcher(create_template_engine_dispatcher(plugin_session.clone()));
        let plugin_session_clone = plugin_session.clone();
        let plugin_handle = tokio::spawn(async move { plugin_session_clone.run().await });

        let client = TemplateEngineClient::new(host_session.clone());
        let rendered = client
            .render("Hello, {{user.name}}!".to_string())
            .await
            .unwrap();
        assert_eq!(rendered, "Hello, Bob!");

        let _ = host_transport.close().await;
        let _ = plugin_transport.close().await;
        host_handle.abort();
        plugin_handle.abort();
    }

    #[tokio::test]
    async fn test_multiple_placeholders() {
        let (host_transport, plugin_transport) = InProcTransport::pair();
        let host_transport = Arc::new(host_transport);
        let plugin_transport = Arc::new(plugin_transport);

        let mut value_host_impl = ValueHostImpl::new();
        value_host_impl.set("user.name", "Alice");
        value_host_impl.set("site.name", "TestSite");
        value_host_impl.set("site.domain", "test.com");
        let value_host_impl = Arc::new(value_host_impl);

        let host_session = Arc::new(RpcSession::with_channel_start(host_transport.clone(), 1));
        host_session.set_dispatcher(create_value_host_dispatcher(value_host_impl.clone()));
        let host_session_clone = host_session.clone();
        let host_handle = tokio::spawn(async move { host_session_clone.run().await });

        let plugin_session = Arc::new(RpcSession::with_channel_start(plugin_transport.clone(), 2));
        plugin_session.set_dispatcher(create_template_engine_dispatcher(plugin_session.clone()));
        let plugin_session_clone = plugin_session.clone();
        let plugin_handle = tokio::spawn(async move { plugin_session_clone.run().await });

        let client = TemplateEngineClient::new(host_session.clone());
        let rendered = client
            .render("Hi {{user.name}} from {{site.name}} on {{site.domain}}".to_string())
            .await
            .unwrap();
        assert_eq!(rendered, "Hi Alice from TestSite on test.com");

        let _ = host_transport.close().await;
        let _ = plugin_transport.close().await;
        host_handle.abort();
        plugin_handle.abort();
    }

    #[tokio::test]
    async fn test_missing_value() {
        let (host_transport, plugin_transport) = InProcTransport::pair();
        let host_transport = Arc::new(host_transport);
        let plugin_transport = Arc::new(plugin_transport);

        let value_host_impl = Arc::new(ValueHostImpl::new()); // Empty

        let host_session = Arc::new(RpcSession::with_channel_start(host_transport.clone(), 1));
        host_session.set_dispatcher(create_value_host_dispatcher(value_host_impl.clone()));
        let host_session_clone = host_session.clone();
        let host_handle = tokio::spawn(async move { host_session_clone.run().await });

        let plugin_session = Arc::new(RpcSession::with_channel_start(plugin_transport.clone(), 2));
        plugin_session.set_dispatcher(create_template_engine_dispatcher(plugin_session.clone()));
        let plugin_session_clone = plugin_session.clone();
        let plugin_handle = tokio::spawn(async move { plugin_session_clone.run().await });

        let client = TemplateEngineClient::new(host_session.clone());
        let rendered = client
            .render("Hello, {{user.name}}!".to_string())
            .await
            .unwrap();
        assert_eq!(rendered, "Hello, !");

        let _ = host_transport.close().await;
        let _ = plugin_transport.close().await;
        host_handle.abort();
        plugin_handle.abort();
    }
}
