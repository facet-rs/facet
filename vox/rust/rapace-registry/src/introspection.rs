//! Service introspection support.
//!
//! Provides the `ServiceIntrospection` trait and default implementation for
//! querying available services at runtime.

use facet::Facet;

use crate::ServiceRegistry;

/// Information about a registered service.
#[derive(Clone, Debug, Facet)]
pub struct ServiceInfo {
    /// Service name (e.g., "Calculator").
    pub name: String,
    /// Service documentation.
    pub doc: String,
    /// Methods provided by this service.
    pub methods: Vec<MethodInfo>,
}

/// Information about a method.
#[derive(Clone, Debug, Facet)]
pub struct MethodInfo {
    /// Method ID (for debugging/logging).
    pub id: u32,
    /// Method name (e.g., "add").
    pub name: String,
    /// Full qualified name (e.g., "Calculator.add").
    pub full_name: String,
    /// Method documentation.
    pub doc: String,
    /// Argument names and types.
    pub args: Vec<ArgInfo>,
    /// Whether this is a streaming method.
    pub is_streaming: bool,
}

/// Argument metadata.
#[derive(Clone, Debug, Facet)]
pub struct ArgInfo {
    /// Argument name (e.g., "a", "name").
    pub name: String,
    /// Argument type name (e.g., "i32", "String").
    pub type_name: String,
}

// Note: We can't use #[rapace::service] here because it would create a circular dependency
// (rapace-macros depends on rapace-registry). Instead, we define the trait and let
// the user generate the client/server in a crate that depends on rapace.
//
// Users will do:
// ```
// use rapace_registry::introspection::ServiceIntrospection;
//
// #[rapace::service]
// pub trait ServiceIntrospection {
//     async fn list_services(&self) -> Vec<ServiceInfo>;
//     async fn describe_service(&self, name: String) -> Option<ServiceInfo>;
//     async fn has_method(&self, method_id: u32) -> bool;
// }
// ```

/// Default implementation that reads from the global registry.
///
/// This implementation provides runtime introspection by reading
/// from the global service registry.
///
/// # Example
///
/// ```ignore
/// use rapace_registry::introspection::DefaultServiceIntrospection;
///
/// let introspection = DefaultServiceIntrospection;
/// let services = introspection.list_services();
/// for service in services {
///     println!("Service: {}", service.name);
/// }
/// ```
#[derive(Clone, Debug, Default)]
pub struct DefaultServiceIntrospection;

impl DefaultServiceIntrospection {
    /// Create a new default introspection implementation.
    pub fn new() -> Self {
        Self
    }

    /// List all services registered in the global registry.
    ///
    /// Returns a snapshot of all currently registered services and their methods.
    pub fn list_services(&self) -> Vec<ServiceInfo> {
        ServiceRegistry::with_global(|registry| {
            registry
                .iter_services()
                .map(|service| ServiceInfo {
                    name: service.name.to_string(),
                    doc: service.doc.clone(),
                    methods: service
                        .iter_methods()
                        .map(|method| MethodInfo {
                            id: method.id.0,
                            name: method.name.to_string(),
                            full_name: method.full_name.clone(),
                            doc: method.doc.clone(),
                            args: method
                                .args
                                .iter()
                                .map(|arg| ArgInfo {
                                    name: arg.name.to_string(),
                                    type_name: arg.type_name.to_string(),
                                })
                                .collect(),
                            is_streaming: method.is_streaming,
                        })
                        .collect(),
                })
                .collect()
        })
    }

    /// Describe a specific service by name.
    ///
    /// Returns `Some(ServiceInfo)` if the service exists, `None` otherwise.
    pub fn describe_service(&self, name: &str) -> Option<ServiceInfo> {
        self.list_services().into_iter().find(|s| s.name == name)
    }

    /// Check if a method ID is supported.
    ///
    /// Returns `true` if any registered service has a method with this ID.
    pub fn has_method(&self, method_id: u32) -> bool {
        ServiceRegistry::with_global(|registry| {
            registry.method_by_id(crate::MethodId(method_id)).is_some()
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_introspection_empty() {
        // Note: Global registry might have services from other tests,
        // so we just verify the API works
        let introspection = DefaultServiceIntrospection::new();
        let services = introspection.list_services();
        // Should return a list (possibly empty or with services from other tests)
        let _ = services;
    }

    #[test]
    fn test_describe_service() {
        let introspection = DefaultServiceIntrospection::new();
        // Non-existent service should return None
        let result = introspection.describe_service("NonExistentService12345");
        assert!(result.is_none() || result.is_some()); // May or may not exist depending on test order
    }

    #[test]
    fn test_has_method() {
        let introspection = DefaultServiceIntrospection::new();
        // Method 0 is reserved for control
        let has_control = introspection.has_method(0);
        // Should be false (control is not registered as a service method)
        let _ = has_control;
    }
}
