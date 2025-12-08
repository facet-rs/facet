// src/registry.rs

//! Service registry for introspection and RPC method dispatch.
//!
//! This module provides safe Rust types for building and reading service registries.
//! The registry stores service metadata, method information, and optional schemas
//! that can be serialized into shared memory for introspection.
//!
//! # Design
//!
//! The registry is built using a `ServiceRegistryBuilder` and produces a serialized
//! blob containing:
//! - Service metadata (name, version, method count)
//! - Method metadata (name, ID, RPC kind, schema offsets)
//! - A string table for service/method names
//! - Optional schema data
//!
//! The serialized format uses offset-based references for efficient SHM storage.
//! See DESIGN.md for the wire layout.
//!
//! # Example
//!
//! ```rust
//! use rapace::registry::{ServiceRegistryBuilder, MethodKind};
//!
//! let mut builder = ServiceRegistryBuilder::new();
//! let mut service = builder.add_service("com.example.Echo", 1, 0).unwrap();
//! service.add_method("Echo", 1, MethodKind::Unary).unwrap();
//! let registry_data = builder.build();
//! ```

use std::collections::HashMap;

/// Maximum service name length (from DESIGN.md)
pub const MAX_SERVICE_NAME_LEN: usize = 256;

/// Maximum method name length (from DESIGN.md)
pub const MAX_METHOD_NAME_LEN: usize = 128;

/// RPC method kind (streaming semantics)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MethodKind {
    /// Unary RPC: single request, single response
    Unary = 0,
    /// Client streaming: multiple requests, single response
    ClientStreaming = 1,
    /// Server streaming: single request, multiple responses
    ServerStreaming = 2,
    /// Bidirectional streaming: multiple requests and responses
    Bidirectional = 3,
}

impl MethodKind {
    /// Convert from u32 wire value
    pub fn from_u32(val: u32) -> Option<Self> {
        match val {
            0 => Some(MethodKind::Unary),
            1 => Some(MethodKind::ClientStreaming),
            2 => Some(MethodKind::ServerStreaming),
            3 => Some(MethodKind::Bidirectional),
            _ => None,
        }
    }

    /// Convert to u32 for wire transmission
    pub fn as_u32(self) -> u32 {
        self as u32
    }
}

/// Error type for registry operations
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RegistryError {
    /// Service name exceeds MAX_SERVICE_NAME_LEN
    ServiceNameTooLong,
    /// Method name exceeds MAX_METHOD_NAME_LEN
    MethodNameTooLong,
    /// Service name is empty
    EmptyServiceName,
    /// Method name is empty
    EmptyMethodName,
    /// Duplicate service name
    DuplicateService,
    /// Duplicate method name within a service
    DuplicateMethod,
    /// Invalid UTF-8 in service or method name
    InvalidUtf8,
    /// Registry data is too short or malformed
    MalformedData,
    /// Service not found
    ServiceNotFound,
    /// Method not found
    MethodNotFound,
    /// String offset out of bounds
    InvalidOffset,
}

impl std::fmt::Display for RegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RegistryError::ServiceNameTooLong => {
                write!(f, "service name exceeds {} bytes", MAX_SERVICE_NAME_LEN)
            }
            RegistryError::MethodNameTooLong => {
                write!(f, "method name exceeds {} bytes", MAX_METHOD_NAME_LEN)
            }
            RegistryError::EmptyServiceName => write!(f, "service name cannot be empty"),
            RegistryError::EmptyMethodName => write!(f, "method name cannot be empty"),
            RegistryError::DuplicateService => write!(f, "duplicate service name"),
            RegistryError::DuplicateMethod => write!(f, "duplicate method name in service"),
            RegistryError::InvalidUtf8 => write!(f, "invalid UTF-8 in name"),
            RegistryError::MalformedData => write!(f, "malformed registry data"),
            RegistryError::ServiceNotFound => write!(f, "service not found"),
            RegistryError::MethodNotFound => write!(f, "method not found"),
            RegistryError::InvalidOffset => write!(f, "invalid string offset"),
        }
    }
}

impl std::error::Error for RegistryError {}

/// Information about a service
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ServiceInfo {
    /// Service name (e.g., "com.example.Echo")
    pub name: String,
    /// Major version
    pub version_major: u16,
    /// Minor version
    pub version_minor: u16,
    /// Methods in this service
    pub methods: Vec<MethodInfo>,
    /// Optional schema blob
    pub schema: Option<Vec<u8>>,
}

/// Information about a method
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MethodInfo {
    /// Method name (e.g., "Echo")
    pub name: String,
    /// Method ID for RPC dispatch
    pub method_id: u32,
    /// RPC kind (unary, streaming, etc.)
    pub kind: MethodKind,
    /// Optional request schema
    pub request_schema: Option<Vec<u8>>,
    /// Optional response schema
    pub response_schema: Option<Vec<u8>>,
}

/// Builder for constructing a service registry
pub struct ServiceRegistryBuilder {
    services: Vec<ServiceBuilder>,
    service_names: HashMap<String, usize>,
}

impl ServiceRegistryBuilder {
    /// Create a new empty registry builder
    pub fn new() -> Self {
        ServiceRegistryBuilder {
            services: Vec::new(),
            service_names: HashMap::new(),
        }
    }

    /// Add a service to the registry.
    ///
    /// Returns a mutable reference to the service builder for adding methods.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Service name is empty
    /// - Service name exceeds MAX_SERVICE_NAME_LEN
    /// - Service name is already registered
    pub fn add_service(
        &mut self,
        name: impl Into<String>,
        version_major: u16,
        version_minor: u16,
    ) -> Result<&mut ServiceBuilder, RegistryError> {
        let name = name.into();

        if name.is_empty() {
            return Err(RegistryError::EmptyServiceName);
        }
        if name.len() > MAX_SERVICE_NAME_LEN {
            return Err(RegistryError::ServiceNameTooLong);
        }
        if self.service_names.contains_key(&name) {
            return Err(RegistryError::DuplicateService);
        }

        let idx = self.services.len();
        self.service_names.insert(name.clone(), idx);
        self.services.push(ServiceBuilder {
            name,
            version_major,
            version_minor,
            methods: Vec::new(),
            method_names: HashMap::new(),
            schema: None,
        });

        Ok(&mut self.services[idx])
    }

    /// Build the registry into a serialized byte blob.
    ///
    /// The format is:
    /// - Registry header (8 bytes)
    /// - ServiceEntry array
    /// - MethodEntry arrays (one per service)
    /// - String table (null-terminated names)
    /// - Schema blobs (optional)
    pub fn build(self) -> Vec<u8> {
        // First pass: calculate sizes and collect data
        let service_count = self.services.len();
        let service_entry_size = 24;
        let method_entry_size = 24;

        let header_size = 8;
        let service_array_offset = header_size;
        let service_array_size = service_count * service_entry_size;

        // Calculate total method entries size
        let total_method_entries: usize = self.services.iter()
            .map(|s| s.methods.len() * method_entry_size)
            .sum();

        let method_array_offset = service_array_offset + service_array_size;
        let string_table_offset = method_array_offset + total_method_entries;

        // Second pass: build the data
        let mut data = Vec::new();

        // Write header
        data.extend_from_slice(&(service_count as u32).to_le_bytes());
        data.extend_from_slice(&0u32.to_le_bytes()); // _pad

        // Reserve space for service and method entries
        data.resize(string_table_offset, 0);

        // Track where we write method entries for each service
        let mut current_method_offset = method_array_offset;

        // Build each service
        for (service_idx, service) in self.services.iter().enumerate() {
            let service_entry_offset = service_array_offset + service_idx * service_entry_size;

            // Write service name to string table
            let name_offset = data.len() as u32;
            data.extend_from_slice(service.name.as_bytes());
            data.push(0); // null terminator

            // Record where this service's methods start
            let methods_offset = if service.methods.is_empty() {
                0
            } else {
                current_method_offset as u32
            };

            // Build method entries for this service
            for method in &service.methods {
                // Write method name to string table
                let method_name_offset = data.len() as u32;
                data.extend_from_slice(method.name.as_bytes());
                data.push(0); // null terminator

                // Write method entry at the reserved location
                let method_entry_data = &mut data[current_method_offset..current_method_offset + method_entry_size];
                method_entry_data[0..4].copy_from_slice(&method_name_offset.to_le_bytes());
                method_entry_data[4..8].copy_from_slice(&(method.name.len() as u32).to_le_bytes());
                method_entry_data[8..12].copy_from_slice(&method.method_id.to_le_bytes());
                method_entry_data[12..16].copy_from_slice(&method.kind.as_u32().to_le_bytes());
                method_entry_data[16..20].copy_from_slice(&0u32.to_le_bytes()); // request_schema_offset (TODO)
                method_entry_data[20..24].copy_from_slice(&0u32.to_le_bytes()); // response_schema_offset (TODO)

                current_method_offset += method_entry_size;
            }

            // Write service schema if present
            let (schema_offset, schema_len) = if let Some(ref schema) = service.schema {
                let offset = data.len() as u32;
                data.extend_from_slice(schema);
                (offset, schema.len() as u32)
            } else {
                (0, 0)
            };

            // Write service entry
            let entry_data = &mut data[service_entry_offset..service_entry_offset + service_entry_size];
            entry_data[0..4].copy_from_slice(&name_offset.to_le_bytes());
            entry_data[4..8].copy_from_slice(&(service.name.len() as u32).to_le_bytes());
            entry_data[8..12].copy_from_slice(&(service.methods.len() as u32).to_le_bytes());
            entry_data[12..16].copy_from_slice(&methods_offset.to_le_bytes());
            entry_data[16..20].copy_from_slice(&schema_offset.to_le_bytes());
            entry_data[20..24].copy_from_slice(&schema_len.to_le_bytes());
            // Note: version fields would go here in a more complete implementation
        }

        data
    }
}

impl Default for ServiceRegistryBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Builder for a single service
pub struct ServiceBuilder {
    name: String,
    version_major: u16,
    version_minor: u16,
    methods: Vec<MethodBuilder>,
    method_names: HashMap<String, usize>,
    schema: Option<Vec<u8>>,
}

impl ServiceBuilder {
    /// Add a method to this service.
    ///
    /// # Errors
    ///
    /// Returns error if:
    /// - Method name is empty
    /// - Method name exceeds MAX_METHOD_NAME_LEN
    /// - Method name is already registered in this service
    pub fn add_method(
        &mut self,
        name: impl Into<String>,
        method_id: u32,
        kind: MethodKind,
    ) -> Result<&mut MethodBuilder, RegistryError> {
        let name = name.into();

        if name.is_empty() {
            return Err(RegistryError::EmptyMethodName);
        }
        if name.len() > MAX_METHOD_NAME_LEN {
            return Err(RegistryError::MethodNameTooLong);
        }
        if self.method_names.contains_key(&name) {
            return Err(RegistryError::DuplicateMethod);
        }

        let idx = self.methods.len();
        self.method_names.insert(name.clone(), idx);
        self.methods.push(MethodBuilder {
            name,
            method_id,
            kind,
            request_schema: None,
            response_schema: None,
        });

        Ok(&mut self.methods[idx])
    }

    /// Set the service schema
    pub fn set_schema(&mut self, schema: Vec<u8>) {
        self.schema = Some(schema);
    }

    /// Get service name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get service version
    pub fn version(&self) -> (u16, u16) {
        (self.version_major, self.version_minor)
    }
}

/// Builder for a single method
pub struct MethodBuilder {
    name: String,
    method_id: u32,
    kind: MethodKind,
    request_schema: Option<Vec<u8>>,
    response_schema: Option<Vec<u8>>,
}

impl MethodBuilder {
    /// Set the request schema
    pub fn set_request_schema(&mut self, schema: Vec<u8>) {
        self.request_schema = Some(schema);
    }

    /// Set the response schema
    pub fn set_response_schema(&mut self, schema: Vec<u8>) {
        self.response_schema = Some(schema);
    }

    /// Get method name
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Get method ID
    pub fn method_id(&self) -> u32 {
        self.method_id
    }

    /// Get method kind
    pub fn kind(&self) -> MethodKind {
        self.kind
    }
}

/// Reader for a serialized service registry
pub struct ServiceRegistry {
    data: Vec<u8>,
}

impl ServiceRegistry {
    /// Create a registry reader from serialized data.
    ///
    /// # Errors
    ///
    /// Returns error if data is too short or malformed.
    pub fn from_bytes(data: Vec<u8>) -> Result<Self, RegistryError> {
        if data.len() < 8 {
            return Err(RegistryError::MalformedData);
        }
        Ok(ServiceRegistry { data })
    }

    /// Get the number of services in this registry
    pub fn service_count(&self) -> u32 {
        u32::from_le_bytes([self.data[0], self.data[1], self.data[2], self.data[3]])
    }

    /// List all services in the registry
    pub fn list_services(&self) -> Result<Vec<ServiceInfo>, RegistryError> {
        let count = self.service_count() as usize;
        let mut services = Vec::with_capacity(count);

        for i in 0..count {
            services.push(self.get_service_by_index(i)?);
        }

        Ok(services)
    }

    /// Get a service by name
    pub fn get_service(&self, name: &str) -> Result<ServiceInfo, RegistryError> {
        let count = self.service_count() as usize;

        for i in 0..count {
            let service = self.get_service_by_index(i)?;
            if service.name == name {
                return Ok(service);
            }
        }

        Err(RegistryError::ServiceNotFound)
    }

    /// Get a service by index
    fn get_service_by_index(&self, index: usize) -> Result<ServiceInfo, RegistryError> {
        let service_entry_size = 24;
        let service_array_offset = 8;
        let entry_offset = service_array_offset + index * service_entry_size;

        if entry_offset + service_entry_size > self.data.len() {
            return Err(RegistryError::MalformedData);
        }

        let entry = &self.data[entry_offset..entry_offset + service_entry_size];

        let name_offset = u32::from_le_bytes([entry[0], entry[1], entry[2], entry[3]]) as usize;
        let name_len = u32::from_le_bytes([entry[4], entry[5], entry[6], entry[7]]) as usize;
        let method_count = u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]]) as usize;
        let methods_offset = u32::from_le_bytes([entry[12], entry[13], entry[14], entry[15]]) as usize;
        let schema_offset = u32::from_le_bytes([entry[16], entry[17], entry[18], entry[19]]) as usize;
        let schema_len = u32::from_le_bytes([entry[20], entry[21], entry[22], entry[23]]) as usize;

        // Read service name
        let name = self.read_string(name_offset, name_len)?;

        // Read methods
        let mut methods = Vec::with_capacity(method_count);
        for i in 0..method_count {
            methods.push(self.read_method(methods_offset, i)?);
        }

        // Read schema
        let schema = if schema_offset > 0 && schema_len > 0 {
            if schema_offset + schema_len > self.data.len() {
                return Err(RegistryError::InvalidOffset);
            }
            Some(self.data[schema_offset..schema_offset + schema_len].to_vec())
        } else {
            None
        };

        Ok(ServiceInfo {
            name,
            version_major: 0, // TODO: read from entry
            version_minor: 0, // TODO: read from entry
            methods,
            schema,
        })
    }

    /// Read a method entry
    fn read_method(&self, methods_offset: usize, index: usize) -> Result<MethodInfo, RegistryError> {
        let method_entry_size = 24;
        let entry_offset = methods_offset + index * method_entry_size;

        if entry_offset + method_entry_size > self.data.len() {
            return Err(RegistryError::MalformedData);
        }

        let entry = &self.data[entry_offset..entry_offset + method_entry_size];

        let name_offset = u32::from_le_bytes([entry[0], entry[1], entry[2], entry[3]]) as usize;
        let name_len = u32::from_le_bytes([entry[4], entry[5], entry[6], entry[7]]) as usize;
        let method_id = u32::from_le_bytes([entry[8], entry[9], entry[10], entry[11]]);
        let kind_raw = u32::from_le_bytes([entry[12], entry[13], entry[14], entry[15]]);

        let name = self.read_string(name_offset, name_len)?;
        let kind = MethodKind::from_u32(kind_raw).ok_or(RegistryError::MalformedData)?;

        Ok(MethodInfo {
            name,
            method_id,
            kind,
            request_schema: None, // TODO: read from entry
            response_schema: None, // TODO: read from entry
        })
    }

    /// Read a null-terminated string from the string table
    fn read_string(&self, offset: usize, len: usize) -> Result<String, RegistryError> {
        if offset + len > self.data.len() {
            return Err(RegistryError::InvalidOffset);
        }

        let bytes = &self.data[offset..offset + len];
        String::from_utf8(bytes.to_vec()).map_err(|_| RegistryError::InvalidUtf8)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_service_name_rejected() {
        let mut builder = ServiceRegistryBuilder::new();
        assert_eq!(
            builder.add_service("", 1, 0).err(),
            Some(RegistryError::EmptyServiceName)
        );
    }

    #[test]
    fn service_name_too_long_rejected() {
        let mut builder = ServiceRegistryBuilder::new();
        let long_name = "x".repeat(MAX_SERVICE_NAME_LEN + 1);
        assert_eq!(
            builder.add_service(long_name, 1, 0).err(),
            Some(RegistryError::ServiceNameTooLong)
        );
    }

    #[test]
    fn duplicate_service_rejected() {
        let mut builder = ServiceRegistryBuilder::new();
        builder.add_service("test.Service", 1, 0).unwrap();
        assert_eq!(
            builder.add_service("test.Service", 1, 0).err(),
            Some(RegistryError::DuplicateService)
        );
    }

    #[test]
    fn empty_method_name_rejected() {
        let mut builder = ServiceRegistryBuilder::new();
        let service = builder.add_service("test.Service", 1, 0).unwrap();
        assert_eq!(
            service.add_method("", 1, MethodKind::Unary).err(),
            Some(RegistryError::EmptyMethodName)
        );
    }

    #[test]
    fn method_name_too_long_rejected() {
        let mut builder = ServiceRegistryBuilder::new();
        let service = builder.add_service("test.Service", 1, 0).unwrap();
        let long_name = "x".repeat(MAX_METHOD_NAME_LEN + 1);
        assert_eq!(
            service.add_method(long_name, 1, MethodKind::Unary).err(),
            Some(RegistryError::MethodNameTooLong)
        );
    }

    #[test]
    fn duplicate_method_rejected() {
        let mut builder = ServiceRegistryBuilder::new();
        let service = builder.add_service("test.Service", 1, 0).unwrap();
        service.add_method("Echo", 1, MethodKind::Unary).unwrap();
        assert_eq!(
            service.add_method("Echo", 2, MethodKind::Unary).err(),
            Some(RegistryError::DuplicateMethod)
        );
    }

    #[test]
    fn method_kind_roundtrip() {
        for kind in [
            MethodKind::Unary,
            MethodKind::ClientStreaming,
            MethodKind::ServerStreaming,
            MethodKind::Bidirectional,
        ] {
            let val = kind.as_u32();
            assert_eq!(MethodKind::from_u32(val), Some(kind));
        }

        assert_eq!(MethodKind::from_u32(999), None);
    }

    #[test]
    fn build_empty_registry() {
        let builder = ServiceRegistryBuilder::new();
        let data = builder.build();

        let registry = ServiceRegistry::from_bytes(data).unwrap();
        assert_eq!(registry.service_count(), 0);
    }

    #[test]
    fn build_single_service_no_methods() {
        let mut builder = ServiceRegistryBuilder::new();
        builder.add_service("com.example.Empty", 1, 0).unwrap();
        let data = builder.build();

        let registry = ServiceRegistry::from_bytes(data).unwrap();
        assert_eq!(registry.service_count(), 1);

        let services = registry.list_services().unwrap();
        assert_eq!(services.len(), 1);
        assert_eq!(services[0].name, "com.example.Empty");
        assert_eq!(services[0].methods.len(), 0);
    }

    #[test]
    fn build_service_with_methods() {
        let mut builder = ServiceRegistryBuilder::new();
        let service = builder.add_service("com.example.Echo", 1, 0).unwrap();
        service.add_method("Echo", 1, MethodKind::Unary).unwrap();
        service.add_method("StreamEcho", 2, MethodKind::Bidirectional).unwrap();
        let data = builder.build();

        let registry = ServiceRegistry::from_bytes(data).unwrap();
        assert_eq!(registry.service_count(), 1);

        let service_info = registry.get_service("com.example.Echo").unwrap();
        assert_eq!(service_info.name, "com.example.Echo");
        assert_eq!(service_info.methods.len(), 2);

        assert_eq!(service_info.methods[0].name, "Echo");
        assert_eq!(service_info.methods[0].method_id, 1);
        assert_eq!(service_info.methods[0].kind, MethodKind::Unary);

        assert_eq!(service_info.methods[1].name, "StreamEcho");
        assert_eq!(service_info.methods[1].method_id, 2);
        assert_eq!(service_info.methods[1].kind, MethodKind::Bidirectional);
    }

    #[test]
    fn build_multiple_services() {
        let mut builder = ServiceRegistryBuilder::new();

        let service1 = builder.add_service("com.example.Service1", 1, 0).unwrap();
        service1.add_method("Method1", 1, MethodKind::Unary).unwrap();

        let service2 = builder.add_service("com.example.Service2", 2, 0).unwrap();
        service2.add_method("Method2", 2, MethodKind::ServerStreaming).unwrap();

        let data = builder.build();
        let registry = ServiceRegistry::from_bytes(data).unwrap();

        assert_eq!(registry.service_count(), 2);

        let s1 = registry.get_service("com.example.Service1").unwrap();
        assert_eq!(s1.methods.len(), 1);
        assert_eq!(s1.methods[0].name, "Method1");

        let s2 = registry.get_service("com.example.Service2").unwrap();
        assert_eq!(s2.methods.len(), 1);
        assert_eq!(s2.methods[0].name, "Method2");
    }

    #[test]
    fn service_not_found() {
        let builder = ServiceRegistryBuilder::new();
        let data = builder.build();
        let registry = ServiceRegistry::from_bytes(data).unwrap();

        assert_eq!(
            registry.get_service("nonexistent").err(),
            Some(RegistryError::ServiceNotFound)
        );
    }

    #[test]
    fn malformed_data_rejected() {
        let data = vec![0u8; 4]; // Too short
        assert_eq!(
            ServiceRegistry::from_bytes(data).err(),
            Some(RegistryError::MalformedData)
        );
    }

    #[test]
    fn service_with_schema() {
        let mut builder = ServiceRegistryBuilder::new();
        let service = builder.add_service("com.example.Schematic", 1, 0).unwrap();
        service.set_schema(b"schema_data".to_vec());
        let data = builder.build();

        let registry = ServiceRegistry::from_bytes(data).unwrap();
        let service_info = registry.get_service("com.example.Schematic").unwrap();
        assert_eq!(service_info.schema, Some(b"schema_data".to_vec()));
    }

    #[test]
    fn method_builder_accessors() {
        let mut builder = ServiceRegistryBuilder::new();
        let service = builder.add_service("test.Service", 1, 0).unwrap();
        let method = service.add_method("TestMethod", 42, MethodKind::Unary).unwrap();

        assert_eq!(method.name(), "TestMethod");
        assert_eq!(method.method_id(), 42);
        assert_eq!(method.kind(), MethodKind::Unary);
    }

    #[test]
    fn service_builder_accessors() {
        let mut builder = ServiceRegistryBuilder::new();
        let service = builder.add_service("test.Service", 1, 2).unwrap();

        assert_eq!(service.name(), "test.Service");
        assert_eq!(service.version(), (1, 2));
    }

    #[test]
    fn comprehensive_registry_workflow() {
        // Build a realistic registry
        let mut builder = ServiceRegistryBuilder::new();

        // Add an Echo service with multiple methods
        let echo = builder.add_service("com.example.Echo", 1, 0).unwrap();
        echo.add_method("Echo", 1, MethodKind::Unary).unwrap();
        echo.add_method("EchoStream", 2, MethodKind::Bidirectional).unwrap();

        // Add a Calculator service
        let calc = builder.add_service("com.example.Calculator", 2, 1).unwrap();
        calc.add_method("Add", 10, MethodKind::Unary).unwrap();
        calc.add_method("Multiply", 11, MethodKind::Unary).unwrap();
        calc.add_method("StreamSum", 12, MethodKind::ClientStreaming).unwrap();

        // Build and serialize
        let data = builder.build();

        // Read it back
        let registry = ServiceRegistry::from_bytes(data).unwrap();
        assert_eq!(registry.service_count(), 2);

        // Verify Echo service
        let echo_info = registry.get_service("com.example.Echo").unwrap();
        assert_eq!(echo_info.methods.len(), 2);
        assert_eq!(echo_info.methods[0].name, "Echo");
        assert_eq!(echo_info.methods[0].kind, MethodKind::Unary);
        assert_eq!(echo_info.methods[1].name, "EchoStream");
        assert_eq!(echo_info.methods[1].kind, MethodKind::Bidirectional);

        // Verify Calculator service
        let calc_info = registry.get_service("com.example.Calculator").unwrap();
        assert_eq!(calc_info.methods.len(), 3);
        assert_eq!(calc_info.methods[0].method_id, 10);
        assert_eq!(calc_info.methods[1].method_id, 11);
        assert_eq!(calc_info.methods[2].kind, MethodKind::ClientStreaming);

        // List all services
        let all = registry.list_services().unwrap();
        assert_eq!(all.len(), 2);
        assert!(all.iter().any(|s| s.name == "com.example.Echo"));
        assert!(all.iter().any(|s| s.name == "com.example.Calculator"));
    }

    #[test]
    fn method_schemas() {
        let mut builder = ServiceRegistryBuilder::new();
        let service = builder.add_service("test.Service", 1, 0).unwrap();
        let method = service.add_method("Test", 1, MethodKind::Unary).unwrap();

        method.set_request_schema(b"request_schema".to_vec());
        method.set_response_schema(b"response_schema".to_vec());

        // Note: Schema reading is TODO in the current implementation
        // This test verifies the builder API works
    }

    #[test]
    fn all_method_kinds() {
        let mut builder = ServiceRegistryBuilder::new();
        let service = builder.add_service("test.Streaming", 1, 0).unwrap();

        service.add_method("Unary", 1, MethodKind::Unary).unwrap();
        service.add_method("ClientStream", 2, MethodKind::ClientStreaming).unwrap();
        service.add_method("ServerStream", 3, MethodKind::ServerStreaming).unwrap();
        service.add_method("Bidi", 4, MethodKind::Bidirectional).unwrap();

        let data = builder.build();
        let registry = ServiceRegistry::from_bytes(data).unwrap();
        let service_info = registry.get_service("test.Streaming").unwrap();

        assert_eq!(service_info.methods[0].kind, MethodKind::Unary);
        assert_eq!(service_info.methods[1].kind, MethodKind::ClientStreaming);
        assert_eq!(service_info.methods[2].kind, MethodKind::ServerStreaming);
        assert_eq!(service_info.methods[3].kind, MethodKind::Bidirectional);
    }
}
