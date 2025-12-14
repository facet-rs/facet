//! rapace-registry: Service registry with schema support for rapace RPC.
//!
//! This crate provides a registry for RPC services that stores:
//! - Service names and IDs
//! - Method names and IDs
//! - Request/response schemas (via facet shapes)
//! - Supported encodings per method
//!
//! # Global Registry
//!
//! Services automatically register themselves in the global registry when their
//! server is created (via `#[rapace::service]` macro). Access the global registry:
//!
//! ```ignore
//! use rapace_registry::ServiceRegistry;
//!
//! // Read-only access
//! ServiceRegistry::with_global(|registry| {
//!     for service in registry.services() {
//!         println!("Service: {}", service.name);
//!     }
//! });
//!
//! // Mutable access (for manual registration)
//! ServiceRegistry::with_global_mut(|registry| {
//!     let mut builder = registry.register_service("MyService", "docs");
//!     // ...
//!     builder.finish();
//! });
//! ```
//!
//! # Manual Registry
//!
//! For testing or custom scenarios, you can create isolated registries:
//!
//! ```ignore
//! use rapace_registry::ServiceRegistry;
//!
//! let mut registry = ServiceRegistry::new();
//! // ... register services ...
//! ```

#![forbid(unsafe_op_in_unsafe_fn)]

pub mod introspection;

use facet_core::Shape;
use std::collections::HashMap;
use std::sync::LazyLock;

/// A unique identifier for a service within a registry.
///
/// Service IDs are assigned sequentially when services are registered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ServiceId(pub u32);

/// A unique identifier for a method within a registry.
///
/// Method IDs are assigned sequentially across all services to ensure
/// global uniqueness. Method ID 0 is reserved for control frames.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MethodId(pub u32);

impl MethodId {
    /// Reserved method ID for control channel operations.
    pub const CONTROL: MethodId = MethodId(0);
}

/// Supported wire encodings for RPC payloads.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Encoding {
    /// Postcard binary encoding (default, most efficient).
    Postcard = 0,
    /// JSON text encoding (for debugging/interop).
    Json = 1,
}

impl Encoding {
    /// All available encodings.
    pub const ALL: &'static [Encoding] = &[Encoding::Postcard, Encoding::Json];
}

/// Information about an argument to an RPC method.
#[derive(Debug, Clone)]
pub struct ArgInfo {
    /// The argument's name (e.g., "a", "name").
    pub name: &'static str,
    /// The argument's type as a string (e.g., "i32", "String").
    pub type_name: &'static str,
}

/// Information about a single RPC method.
#[derive(Debug)]
pub struct MethodEntry {
    /// The method's unique ID within the registry.
    pub id: MethodId,
    /// The method's name (e.g., "add").
    pub name: &'static str,
    /// The canonical full name (e.g., "Adder.add").
    pub full_name: String,
    /// Documentation string from the method's `///` comments.
    pub doc: String,
    /// The arguments to this method, in order.
    pub args: Vec<ArgInfo>,
    /// The request type's shape (schema).
    pub request_shape: &'static Shape,
    /// The response type's shape (schema).
    pub response_shape: &'static Shape,
    /// Whether this is a streaming method.
    pub is_streaming: bool,
    /// Supported wire encodings for this method.
    pub supported_encodings: Vec<Encoding>,
}

impl MethodEntry {
    /// Check if a given encoding is supported by this method.
    pub fn supports_encoding(&self, encoding: Encoding) -> bool {
        self.supported_encodings.contains(&encoding)
    }
}

/// Information about an RPC service.
#[derive(Debug)]
pub struct ServiceEntry {
    /// The service's unique ID within the registry.
    pub id: ServiceId,
    /// The service's name (e.g., "Adder").
    pub name: &'static str,
    /// Documentation string from the service trait's `///` comments.
    pub doc: String,
    /// Methods provided by this service, keyed by method name.
    pub methods: HashMap<&'static str, MethodEntry>,
}

impl ServiceEntry {
    /// Look up a method by name.
    pub fn method(&self, name: &str) -> Option<&MethodEntry> {
        self.methods.get(name)
    }

    /// Iterate over all methods in this service.
    pub fn iter_methods(&self) -> impl Iterator<Item = &MethodEntry> {
        self.methods.values()
    }
}

/// A registry of RPC services and their methods.
///
/// The registry assigns globally unique method IDs and provides
/// lookup by name or ID.
#[derive(Debug, Default)]
pub struct ServiceRegistry {
    /// Services keyed by name.
    services_by_name: HashMap<&'static str, ServiceEntry>,
    /// Method lookup by ID for fast dispatch.
    methods_by_id: HashMap<MethodId, MethodLookup>,
    /// Next service ID to assign.
    next_service_id: u32,
    /// Next method ID to assign (starts at 1, 0 is reserved for control).
    next_method_id: u32,
}

/// Lookup result for method by ID.
#[derive(Debug, Clone)]
struct MethodLookup {
    service_name: &'static str,
    method_name: &'static str,
}

impl ServiceRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            services_by_name: HashMap::new(),
            methods_by_id: HashMap::new(),
            next_service_id: 0,
            next_method_id: 1, // 0 is reserved for control
        }
    }

    /// Register a new service with the given name and documentation.
    ///
    /// Returns a builder for adding methods to the service.
    pub fn register_service(
        &mut self,
        name: &'static str,
        doc: impl Into<String>,
    ) -> ServiceBuilder<'_> {
        let id = ServiceId(self.next_service_id);
        self.next_service_id += 1;

        ServiceBuilder {
            registry: self,
            service_name: name,
            service_doc: doc.into(),
            service_id: id,
            methods: HashMap::new(),
        }
    }

    /// Look up a service by name.
    pub fn service(&self, name: &str) -> Option<&ServiceEntry> {
        self.services_by_name.get(name)
    }

    /// Look up a method by service name and method name.
    pub fn lookup_method(&self, service_name: &str, method_name: &str) -> Option<&MethodEntry> {
        self.services_by_name
            .get(service_name)
            .and_then(|s| s.method(method_name))
    }

    /// Look up a method by its ID.
    pub fn method_by_id(&self, id: MethodId) -> Option<&MethodEntry> {
        let lookup = self.methods_by_id.get(&id)?;
        self.lookup_method(lookup.service_name, lookup.method_name)
    }

    /// Resolve a (service_name, method_name) pair to a MethodId.
    pub fn resolve_method_id(&self, service_name: &str, method_name: &str) -> Option<MethodId> {
        self.lookup_method(service_name, method_name).map(|m| m.id)
    }

    /// Iterate over all registered services.
    pub fn iter_services(&self) -> impl Iterator<Item = &ServiceEntry> {
        self.services_by_name.values()
    }

    /// Iterate over all registered services (alias for iter_services).
    pub fn services(&self) -> impl Iterator<Item = &ServiceEntry> {
        self.iter_services()
    }

    /// Look up a service by its ID.
    pub fn service_by_id(&self, id: ServiceId) -> Option<&ServiceEntry> {
        self.services_by_name.values().find(|s| s.id == id)
    }

    /// Get the total number of registered services.
    pub fn service_count(&self) -> usize {
        self.services_by_name.len()
    }

    /// Get the total number of registered methods (excluding control).
    pub fn method_count(&self) -> usize {
        self.methods_by_id.len()
    }

    /// Get a reference to the global service registry.
    ///
    /// Use this when you need direct access to the RwLock for complex operations.
    /// For simple read/write access, prefer `with_global` or `with_global_mut`.
    pub fn global() -> &'static parking_lot::RwLock<ServiceRegistry> {
        &GLOBAL_REGISTRY
    }

    /// Access the global registry with a read lock.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use rapace_registry::ServiceRegistry;
    ///
    /// ServiceRegistry::with_global(|registry| {
    ///     for service in registry.services() {
    ///         println!("Service: {}", service.name);
    ///     }
    /// });
    /// ```
    pub fn with_global<F, R>(f: F) -> R
    where
        F: FnOnce(&ServiceRegistry) -> R,
    {
        f(&GLOBAL_REGISTRY.read())
    }

    /// Modify the global registry with a write lock.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use rapace_registry::ServiceRegistry;
    ///
    /// ServiceRegistry::with_global_mut(|registry| {
    ///     let mut builder = registry.register_service("MyService", "docs");
    ///     // ... add methods ...
    ///     builder.finish();
    /// });
    /// ```
    pub fn with_global_mut<F, R>(f: F) -> R
    where
        F: FnOnce(&mut ServiceRegistry) -> R,
    {
        f(&mut GLOBAL_REGISTRY.write())
    }
}

/// Global process-level service registry.
///
/// All services automatically register here when their server is created
/// (via the `#[rapace::service]` macro). This enables runtime service discovery
/// and introspection.
static GLOBAL_REGISTRY: LazyLock<parking_lot::RwLock<ServiceRegistry>> =
    LazyLock::new(|| parking_lot::RwLock::new(ServiceRegistry::new()));

/// Builder for registering methods on a service.
pub struct ServiceBuilder<'a> {
    registry: &'a mut ServiceRegistry,
    service_name: &'static str,
    service_doc: String,
    service_id: ServiceId,
    methods: HashMap<&'static str, MethodEntry>,
}

impl ServiceBuilder<'_> {
    /// Add a unary method to the service.
    pub fn add_method(
        &mut self,
        name: &'static str,
        doc: impl Into<String>,
        args: Vec<ArgInfo>,
        request_shape: &'static Shape,
        response_shape: &'static Shape,
    ) -> MethodId {
        self.add_method_inner(name, doc.into(), args, request_shape, response_shape, false)
    }

    /// Add a streaming method to the service.
    pub fn add_streaming_method(
        &mut self,
        name: &'static str,
        doc: impl Into<String>,
        args: Vec<ArgInfo>,
        request_shape: &'static Shape,
        response_shape: &'static Shape,
    ) -> MethodId {
        self.add_method_inner(name, doc.into(), args, request_shape, response_shape, true)
    }

    fn add_method_inner(
        &mut self,
        name: &'static str,
        doc: String,
        args: Vec<ArgInfo>,
        request_shape: &'static Shape,
        response_shape: &'static Shape,
        is_streaming: bool,
    ) -> MethodId {
        let id = MethodId(self.registry.next_method_id);
        self.registry.next_method_id += 1;

        let full_name = format!("{}.{}", self.service_name, name);

        let entry = MethodEntry {
            id,
            name,
            full_name,
            doc,
            args,
            request_shape,
            response_shape,
            is_streaming,
            supported_encodings: vec![Encoding::Postcard], // Default to postcard only
        };

        self.methods.insert(name, entry);

        // Register in the global method lookup
        self.registry.methods_by_id.insert(
            id,
            MethodLookup {
                service_name: self.service_name,
                method_name: name,
            },
        );

        id
    }

    /// Finish building the service and add it to the registry.
    pub fn finish(self) {
        let entry = ServiceEntry {
            id: self.service_id,
            name: self.service_name,
            doc: self.service_doc,
            methods: self.methods,
        };
        self.registry
            .services_by_name
            .insert(self.service_name, entry);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;

    #[derive(Facet)]
    struct AddRequest {
        a: i32,
        b: i32,
    }

    #[derive(Facet)]
    struct AddResponse {
        result: i32,
    }

    #[derive(Facet)]
    struct RangeRequest {
        n: u32,
    }

    #[derive(Facet)]
    struct RangeItem {
        value: u32,
    }

    #[test]
    fn test_register_service() {
        let mut registry = ServiceRegistry::new();

        let mut builder = registry.register_service("Adder", "A simple adder service.");
        let add_id = builder.add_method(
            "add",
            "Add two numbers together.",
            vec![
                ArgInfo {
                    name: "a",
                    type_name: "i32",
                },
                ArgInfo {
                    name: "b",
                    type_name: "i32",
                },
            ],
            <AddRequest as Facet>::SHAPE,
            <AddResponse as Facet>::SHAPE,
        );
        builder.finish();

        assert_eq!(registry.service_count(), 1);
        assert_eq!(registry.method_count(), 1);

        let service = registry.service("Adder").unwrap();
        assert_eq!(service.name, "Adder");
        assert_eq!(service.doc, "A simple adder service.");
        assert_eq!(service.id.0, 0);

        let method = service.method("add").unwrap();
        assert_eq!(method.id, add_id);
        assert_eq!(method.name, "add");
        assert_eq!(method.full_name, "Adder.add");
        assert_eq!(method.doc, "Add two numbers together.");
        assert!(!method.is_streaming);
        assert_eq!(method.args.len(), 2);
        assert_eq!(method.args[0].name, "a");
        assert_eq!(method.args[1].name, "b");
    }

    #[test]
    fn test_register_multiple_services() {
        let mut registry = ServiceRegistry::new();

        // Register Adder
        let mut builder = registry.register_service("Adder", "");
        let add_id = builder.add_method(
            "add",
            "",
            vec![
                ArgInfo {
                    name: "a",
                    type_name: "i32",
                },
                ArgInfo {
                    name: "b",
                    type_name: "i32",
                },
            ],
            <AddRequest as Facet>::SHAPE,
            <AddResponse as Facet>::SHAPE,
        );
        builder.finish();

        // Register RangeService
        let mut builder = registry.register_service("RangeService", "");
        let range_id = builder.add_streaming_method(
            "range",
            "",
            vec![ArgInfo {
                name: "n",
                type_name: "u32",
            }],
            <RangeRequest as Facet>::SHAPE,
            <RangeItem as Facet>::SHAPE,
        );
        builder.finish();

        assert_eq!(registry.service_count(), 2);
        assert_eq!(registry.method_count(), 2);

        // Method IDs should be unique across services
        assert_ne!(add_id, range_id);
        assert_eq!(add_id.0, 1); // First method after control (0)
        assert_eq!(range_id.0, 2);

        // Lookup by name
        let method = registry.lookup_method("RangeService", "range").unwrap();
        assert!(method.is_streaming);

        // Lookup by ID
        let method = registry.method_by_id(range_id).unwrap();
        assert_eq!(method.full_name, "RangeService.range");
    }

    #[test]
    fn test_resolve_method_id() {
        let mut registry = ServiceRegistry::new();

        let mut builder = registry.register_service("Adder", "");
        builder.add_method(
            "add",
            "",
            vec![
                ArgInfo {
                    name: "a",
                    type_name: "i32",
                },
                ArgInfo {
                    name: "b",
                    type_name: "i32",
                },
            ],
            <AddRequest as Facet>::SHAPE,
            <AddResponse as Facet>::SHAPE,
        );
        builder.finish();

        let id = registry.resolve_method_id("Adder", "add").unwrap();
        assert_eq!(id.0, 1);

        // Non-existent lookups return None
        assert!(registry.resolve_method_id("Adder", "subtract").is_none());
        assert!(registry.resolve_method_id("Calculator", "add").is_none());
    }

    #[test]
    fn test_method_id_zero_reserved() {
        assert_eq!(MethodId::CONTROL.0, 0);

        let mut registry = ServiceRegistry::new();
        let mut builder = registry.register_service("Test", "");
        let first_method_id = builder.add_method(
            "test",
            "",
            vec![],
            <AddRequest as Facet>::SHAPE,
            <AddResponse as Facet>::SHAPE,
        );
        builder.finish();

        // First assigned method ID should be 1, not 0
        assert_eq!(first_method_id.0, 1);
    }

    #[test]
    fn test_encoding_support() {
        let mut registry = ServiceRegistry::new();

        let mut builder = registry.register_service("Adder", "");
        builder.add_method(
            "add",
            "",
            vec![
                ArgInfo {
                    name: "a",
                    type_name: "i32",
                },
                ArgInfo {
                    name: "b",
                    type_name: "i32",
                },
            ],
            <AddRequest as Facet>::SHAPE,
            <AddResponse as Facet>::SHAPE,
        );
        builder.finish();

        let method = registry.lookup_method("Adder", "add").unwrap();

        // By default, only Postcard is supported
        assert!(method.supports_encoding(Encoding::Postcard));
        assert!(!method.supports_encoding(Encoding::Json));
    }

    #[test]
    fn test_shapes_are_present() {
        let mut registry = ServiceRegistry::new();

        let mut builder = registry.register_service("Adder", "");
        builder.add_method(
            "add",
            "",
            vec![
                ArgInfo {
                    name: "a",
                    type_name: "i32",
                },
                ArgInfo {
                    name: "b",
                    type_name: "i32",
                },
            ],
            <AddRequest as Facet>::SHAPE,
            <AddResponse as Facet>::SHAPE,
        );
        builder.finish();

        let method = registry.lookup_method("Adder", "add").unwrap();

        // Shapes should be non-null static references
        assert!(!method.request_shape.type_identifier.is_empty());
        assert!(!method.response_shape.type_identifier.is_empty());
    }

    #[test]
    fn test_docs_captured() {
        let mut registry = ServiceRegistry::new();

        let service_doc = "This is the service documentation.\nIt can span multiple lines.";
        let method_doc = "This method adds two numbers.\n\n# Arguments\n* `a` - First number\n* `b` - Second number";

        let mut builder = registry.register_service("Calculator", service_doc);
        builder.add_method(
            "add",
            method_doc,
            vec![
                ArgInfo {
                    name: "a",
                    type_name: "i32",
                },
                ArgInfo {
                    name: "b",
                    type_name: "i32",
                },
            ],
            <AddRequest as Facet>::SHAPE,
            <AddResponse as Facet>::SHAPE,
        );
        builder.finish();

        let service = registry.service("Calculator").unwrap();
        assert_eq!(service.doc, service_doc);

        let method = service.method("add").unwrap();
        assert_eq!(method.doc, method_doc);
    }

    #[test]
    fn test_global_registry() {
        // Register a service in the global registry
        ServiceRegistry::with_global_mut(|registry| {
            let mut builder = registry.register_service("GlobalTestService", "Test service");
            builder.add_method(
                "test_method",
                "Test method",
                vec![],
                <AddRequest as Facet>::SHAPE,
                <AddResponse as Facet>::SHAPE,
            );
            builder.finish();
        });

        // Verify it's accessible
        ServiceRegistry::with_global(|registry| {
            let service = registry.service("GlobalTestService").unwrap();
            assert_eq!(service.name, "GlobalTestService");
            assert_eq!(service.doc, "Test service");

            let method = service.method("test_method").unwrap();
            assert_eq!(method.name, "test_method");
        });
    }

    #[test]
    fn test_global_registry_method_by_id() {
        // Register and get method ID
        let method_id = ServiceRegistry::with_global_mut(|registry| {
            let mut builder = registry.register_service("MethodIdTest", "");
            let id = builder.add_method(
                "lookup_test",
                "",
                vec![],
                <AddRequest as Facet>::SHAPE,
                <AddResponse as Facet>::SHAPE,
            );
            builder.finish();
            id
        });

        // Look up by ID
        ServiceRegistry::with_global(|registry| {
            let method = registry.method_by_id(method_id).unwrap();
            assert_eq!(method.full_name, "MethodIdTest.lookup_test");
        });
    }
}
