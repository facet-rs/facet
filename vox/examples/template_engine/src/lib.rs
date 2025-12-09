//! Template Engine with Host Callbacks - Shared Library
//!
//! This module contains the service definitions and implementations shared between
//! the main demo binary and the cross-process test helper.

use std::collections::HashMap;
use std::sync::Arc;

use rapace_core::{Frame, FrameFlags, RpcError, Transport};
use rapace_testkit::RpcSession;

// Required by the macro
#[allow(unused)]
use rapace_registry;

// ============================================================================
// Service Definitions (Transport-Agnostic)
// ============================================================================

/// Service for fetching values by path.
///
/// The host implements this, backed by whatever data source it uses
/// (in dodeca, this would be salsa queries).
#[allow(async_fn_in_trait)]
#[rapace_macros::service]
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
#[rapace_macros::service]
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
pub struct TemplateEngineImpl<T: Transport + Send + Sync + 'static> {
    session: Arc<RpcSession<T>>,
}

impl<T: Transport + Send + Sync + 'static> TemplateEngineImpl<T> {
    pub fn new(session: Arc<RpcSession<T>>) -> Self {
        Self { session }
    }

    /// Call ValueHost.get_value via RpcSession.
    async fn get_value(&self, path: Vec<String>) -> Result<Option<String>, RpcError> {
        let channel_id = self.session.next_channel_id();
        let payload = facet_postcard::to_vec(&path).unwrap();

        // method_id 1 = get_value (ValueHost's first method)
        let response = self.session.call(channel_id, 1, payload).await?;

        // Check for error
        if response.flags.contains(FrameFlags::ERROR) {
            return Err(rapace_testkit::parse_error_payload(&response.payload));
        }

        // Decode response
        let result: Option<String> = facet_postcard::from_slice(&response.payload).map_err(|e| {
            RpcError::Status {
                code: rapace_core::ErrorCode::Internal,
                message: format!("decode error: {:?}", e),
            }
        })?;

        Ok(result)
    }
}

impl<T: Transport + Send + Sync + 'static> TemplateEngine for TemplateEngineImpl<T> {
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
                match self.get_value(path).await {
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
) -> impl Fn(u32, u32, Vec<u8>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Frame, RpcError>> + Send>>
       + Send
       + Sync
       + 'static {
    move |_channel_id, method_id, payload| {
        let value_host = value_host.clone();
        Box::pin(async move {
            let server = ValueHostServer::new(value_host.as_ref().clone());
            server.dispatch(method_id, &payload).await
        })
    }
}

/// Create a dispatcher for TemplateEngine service.
pub fn create_template_engine_dispatcher<T: Transport + Send + Sync + 'static>(
    session: Arc<RpcSession<T>>,
) -> impl Fn(u32, u32, Vec<u8>) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<Frame, RpcError>> + Send>>
       + Send
       + Sync
       + 'static {
    move |_channel_id, method_id, payload| {
        let engine = TemplateEngineImpl::new(session.clone());
        let server = TemplateEngineServer::new(engine);
        Box::pin(async move { server.dispatch(method_id, &payload).await })
    }
}
