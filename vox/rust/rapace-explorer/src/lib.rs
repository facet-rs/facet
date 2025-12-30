#![doc = include_str!("../README.md")]
#![deny(unsafe_code)]

use rapace::Streaming;

// ============================================================================
// Types
// ============================================================================

/// Summary of a service for the list view.
#[derive(Clone, Debug, facet::Facet)]
pub struct ServiceSummary {
    pub id: u32,
    pub name: String,
    pub doc: String,
    pub method_count: u32,
}

/// Serializable representation of a facet shape for form generation.
#[derive(Clone, Debug, facet::Facet)]
#[repr(u8)]
pub enum ShapeInfo {
    /// Scalar types (integers, floats, booleans)
    Scalar {
        type_name: String,
        /// Hint: "integer", "unsigned", "float", "boolean"
        affinity: String,
    },
    /// String type
    String,
    /// Optional type wrapping another shape
    Option {
        #[facet(recursive_type)]
        inner: Box<ShapeInfo>,
    },
    /// List/Vec type
    List {
        #[facet(recursive_type)]
        item: Box<ShapeInfo>,
    },
    /// Struct with named fields
    Struct {
        type_name: String,
        fields: Vec<FieldInfo>,
    },
    /// Enum with variants
    Enum {
        type_name: String,
        variants: Vec<VariantInfo>,
    },
    /// Map type (HashMap, BTreeMap)
    Map {
        #[facet(recursive_type)]
        key: Box<ShapeInfo>,
        #[facet(recursive_type)]
        value: Box<ShapeInfo>,
    },
    /// Fallback for types we can't represent
    Unknown { type_name: String },
}

/// Field in a struct shape
#[derive(Clone, Debug, facet::Facet)]
pub struct FieldInfo {
    pub name: String,
    #[facet(recursive_type)]
    pub shape: ShapeInfo,
}

/// Variant in an enum shape
#[derive(Clone, Debug, facet::Facet)]
pub struct VariantInfo {
    pub name: String,
    /// None for unit variants, Some for tuple/struct variants
    pub fields: Option<Vec<FieldInfo>>,
}

/// Details of an argument to a method.
#[derive(Clone, Debug, facet::Facet)]
pub struct ArgDetail {
    pub name: String,
    pub type_name: String,
    /// Shape information for generating typed form inputs
    pub shape: ShapeInfo,
}

/// Details of a method.
#[derive(Clone, Debug, facet::Facet)]
pub struct MethodDetail {
    pub id: u32,
    pub name: String,
    pub full_name: String,
    pub doc: String,
    pub args: Vec<ArgDetail>,
    pub is_streaming: bool,
    pub request_type: String,
    pub response_type: String,
}

/// Full details of a service including methods.
#[derive(Clone, Debug, facet::Facet)]
pub struct ServiceDetail {
    pub id: u32,
    pub name: String,
    pub doc: String,
    pub methods: Vec<MethodDetail>,
}

/// Request to call a method dynamically.
#[derive(Clone, Debug, facet::Facet)]
pub struct CallRequest {
    pub service: String,
    pub method: String,
    /// JSON-encoded arguments
    pub args_json: String,
}

/// Response from calling a method.
#[derive(Clone, Debug, facet::Facet)]
pub struct CallResponse {
    /// JSON-encoded result (or null on error)
    pub result_json: String,
    /// Error message if call failed
    pub error: Option<String>,
}

/// A single item from a streaming response.
#[derive(Clone, Debug, facet::Facet)]
pub struct StreamItem {
    /// JSON-encoded value
    pub value_json: String,
}

// ============================================================================
// Explorer Service Trait
// ============================================================================

/// The Explorer service provides service discovery and dynamic method invocation.
///
/// This is the only service the frontend needs to know about - it acts as a
/// proxy to all other registered services.
#[allow(async_fn_in_trait)]
#[rapace::service]
pub trait Explorer {
    /// List all registered services.
    async fn list_services(&self) -> Vec<crate::ServiceSummary>;

    /// Get details for a specific service by ID.
    async fn get_service(&self, service_id: u32) -> Option<crate::ServiceDetail>;

    /// Call a unary method dynamically.
    ///
    /// Arguments are passed as a JSON string and the result is returned as JSON.
    async fn call_unary(&self, request: crate::CallRequest) -> crate::CallResponse;

    /// Call a streaming method dynamically.
    ///
    /// Arguments are passed as a JSON string and results are streamed as JSON.
    async fn call_streaming(&self, request: crate::CallRequest) -> Streaming<crate::StreamItem>;
}
