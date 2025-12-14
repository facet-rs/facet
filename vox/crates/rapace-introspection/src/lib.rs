//! Service introspection RPC service.
//!
//! Provides a `ServiceIntrospection` RPC service that allows clients to query
//! what services and methods are available at runtime.
//!
//! # Example
//!
//! ```ignore
//! use rapace_introspection::{ServiceIntrospection, ServiceIntrospectionServer};
//! use rapace_registry::introspection::DefaultServiceIntrospection;
//!
//! // Create introspection server
//! let introspection = DefaultServiceIntrospection::new();
//! let server = ServiceIntrospectionServer::new(introspection);
//!
//! // Add to your cell's dispatcher
//! use rapace_cell::DispatcherBuilder;
//! let dispatcher = DispatcherBuilder::new()
//!     .add_service(server)
//!     .build();
//! ```

// Re-export types from rapace-registry for convenience
pub use rapace_registry::introspection::{ArgInfo, DefaultServiceIntrospection, MethodInfo, ServiceInfo};

/// Service introspection RPC interface.
///
/// This service allows clients to query what services and methods are registered
/// in the global service registry.
#[rapace::service]
#[async_trait::async_trait]
pub trait ServiceIntrospection {
    /// List all services registered in this process.
    ///
    /// Returns a snapshot of all currently registered services and their methods.
    async fn list_services(&self) -> Vec<ServiceInfo>;

    /// Describe a specific service by name.
    ///
    /// Returns detailed information about the service if it exists, `None` otherwise.
    async fn describe_service(&self, name: String) -> Option<ServiceInfo>;

    /// Check if a method ID is supported.
    ///
    /// Returns `true` if any registered service has a method with this ID.
    async fn has_method(&self, method_id: u32) -> bool;
}

#[async_trait::async_trait]
impl ServiceIntrospection for DefaultServiceIntrospection {
    async fn list_services(&self) -> Vec<ServiceInfo> {
        self.list_services()
    }

    async fn describe_service(&self, name: String) -> Option<ServiceInfo> {
        self.describe_service(&name)
    }

    async fn has_method(&self, method_id: u32) -> bool {
        self.has_method(method_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_introspection_impl() {
        let introspection = DefaultServiceIntrospection::new();

        // Should work without errors
        let _services = introspection.list_services().await;
        let _has_zero = introspection.has_method(0).await;
        let _describe = introspection.describe_service("NonExistent".to_string()).await;
    }
}
