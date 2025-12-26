#![doc = include_str!("../README.md")]

// Re-export types from rapace-registry for convenience
pub use rapace_registry::introspection::{
    ArgInfo, DefaultServiceIntrospection, MethodInfo, ServiceInfo,
};

/// Service introspection RPC interface.
///
/// This service allows clients to query what services and methods are registered
/// in the global service registry.
#[rapace::service]
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

    #[tokio_test_lite::test]
    async fn test_introspection_impl() {
        let introspection = DefaultServiceIntrospection::new();

        // Should work without errors (calling inherent methods, not async trait methods)
        let _services = introspection.list_services();
        let _has_zero = introspection.has_method(0);
        let _describe = introspection.describe_service("NonExistent");
    }
}
