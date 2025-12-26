#![allow(clippy::type_complexity)]
//! Template Engine with Host Callbacks - Shared Library
//!
//! This module contains the service definitions and implementations shared between
//! the main demo binary and the cross-process test helper.

use std::collections::HashMap;
use std::sync::Arc;

use rapace::{BufferPool, Frame, RpcError, Session};

// ============================================================================
// Service Definitions (Transport-Agnostic)
// ============================================================================

/// Service for fetching values by path.
///
/// The host implements this, backed by whatever data source it uses
/// (in dodeca, this would be salsa queries).
#[allow(async_fn_in_trait)]
#[rapace::service]
pub trait ValueHost {
    /// Fetch a value by path, e.g. `["user", "name"]`.
    ///
    /// Returns `None` if the path doesn't exist.
    async fn get_value(&self, path: Vec<String>) -> Option<String>;
}

/// Service for rendering templates.
///
/// The plugin implements this, calling back to the host for values.
#[allow(async_fn_in_trait)]
#[rapace::service]
pub trait TemplateEngine {
    /// Render a template, lazily asking the host for values.
    ///
    /// Placeholders like `{{path.to.value}}` are replaced by calling
    /// `ValueHost::get_value()` on the host.
    async fn render(&self, template: String) -> String;
}

// ============================================================================
// Host Implementation (ValueHost service)
// ============================================================================

/// Host-side implementation of ValueHost.
///
/// In a real system, this would be backed by salsa queries or a database.
/// For this example, we use a simple in-memory HashMap.
#[derive(Clone)]
pub struct ValueHostImpl {
    values: HashMap<String, String>,
}

impl ValueHostImpl {
    pub fn new() -> Self {
        Self {
            values: HashMap::new(),
        }
    }

    /// Set a value at a dotted path (e.g., "user.name").
    pub fn set(&mut self, path: &str, value: &str) {
        self.values.insert(path.to_string(), value.to_string());
    }
}

impl Default for ValueHostImpl {
    fn default() -> Self {
        Self::new()
    }
}

impl ValueHost for ValueHostImpl {
    async fn get_value(&self, path: Vec<String>) -> Option<String> {
        let key = path.join(".");
        self.values.get(&key).cloned()
    }
}

// ============================================================================
// Plugin Implementation (TemplateEngine service)
// ============================================================================

/// Plugin-side implementation of TemplateEngine.
///
/// Uses the RpcSession to call back into the host for values.
pub struct TemplateEngineImpl {
    client: ValueHostClient<rapace::AnyTransport>,
}

impl TemplateEngineImpl {
    pub fn new(session: Arc<Session>) -> Self {
        Self {
            client: ValueHostClient::new(session),
        }
    }
}

impl TemplateEngine for TemplateEngineImpl {
    async fn render(&self, template: String) -> String {
        let mut result = String::new();
        let mut chars = template.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '{' && chars.peek() == Some(&'{') {
                // Consume the second '{'
                chars.next();

                // Collect the placeholder content until '}}'
                let mut placeholder = String::new();
                while let Some(&next) = chars.peek() {
                    if next == '}' {
                        chars.next();
                        if chars.peek() == Some(&'}') {
                            chars.next();
                            break;
                        } else {
                            placeholder.push('}');
                        }
                    } else {
                        placeholder.push(chars.next().unwrap());
                    }
                }

                // Split the placeholder on '.' to get the path
                let path: Vec<String> = placeholder
                    .trim()
                    .split('.')
                    .map(|s| s.to_string())
                    .collect();

                // Call back to the host to get the value
                match self.client.get_value(path).await {
                    Ok(Some(value)) => result.push_str(&value),
                    Ok(None) => {
                        // Value not found - leave empty
                    }
                    Err(_e) => {
                        // RPC error - leave empty (in production, might want to propagate)
                    }
                }
            } else {
                result.push(c);
            }
        }

        result
    }
}

// ============================================================================
// Dispatcher Helpers
// ============================================================================

/// Create a dispatcher for ValueHost service.
pub fn create_value_host_dispatcher(
    value_host: Arc<ValueHostImpl>,
    buffer_pool: BufferPool,
) -> impl Fn(
    Frame,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Frame, RpcError>> + Send>>
+ Send
+ Sync
+ 'static {
    move |request| {
        let value_host = value_host.clone();
        let buffer_pool = buffer_pool.clone();
        Box::pin(async move {
            let server = ValueHostServer::new(value_host.as_ref().clone());
            server
                .dispatch(request.desc.method_id, &request, &buffer_pool)
                .await
        })
    }
}

/// Create a dispatcher for TemplateEngine service.
pub fn create_template_engine_dispatcher(
    session: Arc<Session>,
    buffer_pool: BufferPool,
) -> impl Fn(
    Frame,
) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Frame, RpcError>> + Send>>
+ Send
+ Sync
+ 'static {
    move |request| {
        let buffer_pool = buffer_pool.clone();
        let engine = TemplateEngineImpl::new(session.clone());
        let server = TemplateEngineServer::new(engine);
        Box::pin(async move {
            server
                .dispatch(request.desc.method_id, &request, &buffer_pool)
                .await
        })
    }
}
