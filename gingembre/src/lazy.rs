//! Lazy value system for fine-grained dependency tracking.
//!
//! This module provides [`LazyValue`], a wrapper around [`facet_value::Value`]
//! that supports lazy resolution. Field access on lazy values extends a path
//! rather than immediately resolving, allowing dependency tracking systems
//! (like picante) to track exactly which values are accessed.
//!
//! # How It Works
//!
//! ```text
//! data.versions.dodeca.version
//!      ↓
//! LazyValue { path: [] }
//!      ↓ .versions
//! LazyValue { path: ["versions"] }  ← no resolution yet!
//!      ↓ .dodeca
//! LazyValue { path: ["versions", "dodeca"] }
//!      ↓ .version
//! LazyValue { path: ["versions", "dodeca", "version"] }
//!      ↓ print/compare/etc
//! resolver.resolve(path).await → concrete value + dependency tracked
//! ```

use crate::error::{
    DataPathNotFoundError, SourceLocation, SourceSpan, TemplateError, TemplateSource, TypeError,
    UnknownFieldError,
};
use facet_value::DestructuredRef;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

/// A path through a data tree.
///
/// Paths are sequences of string segments representing navigation through
/// nested objects and arrays. For example, `["versions", "dodeca", "version"]`
/// represents `data.versions.dodeca.version`.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Default)]
pub struct DataPath(pub Vec<String>);

impl DataPath {
    /// Create an empty (root) path.
    pub fn root() -> Self {
        Self(Vec::new())
    }

    /// Create a new path by appending a segment.
    pub fn push(&self, segment: impl Into<String>) -> Self {
        let mut new_path = self.0.clone();
        new_path.push(segment.into());
        Self(new_path)
    }

    /// Get the path segments.
    pub fn segments(&self) -> &[String] {
        &self.0
    }

    /// Check if this is the root path.
    pub fn is_root(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::fmt::Display for DataPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.0.is_empty() {
            write!(f, "(root)")
        } else {
            write!(f, "{}", self.0.join("."))
        }
    }
}

/// Trait for resolving lazy data values by path (async).
///
/// Implementations should track dependencies - each unique path that is
/// resolved becomes a separate dependency in the tracking system.
///
/// # Example
///
/// A picante-backed resolver would create a tracked query for each path,
/// allowing fine-grained cache invalidation when specific values change.
///
/// An RPC-backed resolver would call back to the host for each resolution,
/// with the host tracking dependencies.
pub trait DataResolver: Send + Sync {
    /// Resolve a value at the given path.
    ///
    /// Returns `None` if the path doesn't exist in the data tree.
    fn resolve(
        &self,
        path: &DataPath,
    ) -> Pin<Box<dyn Future<Output = Option<facet_value::Value>> + Send + '_>>;

    /// Get all child keys at a path (for iteration).
    ///
    /// Returns `None` if the path doesn't exist or points to a non-container value.
    /// For objects, returns the field names.
    /// For arrays, returns string indices ("0", "1", etc.).
    fn keys_at(
        &self,
        path: &DataPath,
    ) -> Pin<Box<dyn Future<Output = Option<Vec<String>>> + Send + '_>>;

    /// Get the number of children at a path.
    ///
    /// Returns `None` if the path doesn't exist or isn't a container.
    fn len_at(&self, path: &DataPath) -> Pin<Box<dyn Future<Output = Option<usize>> + Send + '_>> {
        let path = path.clone();
        Box::pin(async move {
            // Default implementation calls keys_at
            // Implementations can override for efficiency
            self.keys_at(&path).await.map(|k| k.len())
        })
    }
}

/// A value that may be lazily resolved.
///
/// Field access on lazy values extends the path rather than immediately
/// resolving. Resolution happens only when a concrete value is needed
/// (printing, comparison, arithmetic, etc.).
///
/// This enables fine-grained dependency tracking: each unique path that
/// is actually used becomes a separate tracked dependency.
#[derive(Clone)]
pub enum LazyValue {
    /// A concrete, already-resolved value.
    Concrete(facet_value::Value),

    /// A lazy reference that will be resolved on demand.
    Lazy {
        /// The resolver that can fetch values by path.
        resolver: Arc<dyn DataResolver>,
        /// The path through the data tree.
        path: DataPath,
    },
}

impl std::fmt::Debug for LazyValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Concrete(v) => write!(f, "Concrete({:?})", v),
            Self::Lazy { path, .. } => write!(f, "Lazy {{ path: {} }}", path),
        }
    }
}

impl LazyValue {
    /// Create a concrete value (no lazy resolution).
    pub fn concrete(value: facet_value::Value) -> Self {
        Self::Concrete(value)
    }

    /// Create a lazy value with a resolver and path.
    pub fn lazy(resolver: Arc<dyn DataResolver>, path: DataPath) -> Self {
        Self::Lazy { resolver, path }
    }

    /// Create a lazy value at the root path.
    pub fn lazy_root(resolver: Arc<dyn DataResolver>) -> Self {
        Self::Lazy {
            resolver,
            path: DataPath::root(),
        }
    }

    /// Check if this is a lazy (unresolved) value.
    pub fn is_lazy(&self) -> bool {
        matches!(self, Self::Lazy { .. })
    }

    /// Check if this is a concrete (resolved) value.
    pub fn is_concrete(&self) -> bool {
        matches!(self, Self::Concrete(_))
    }

    /// Force resolution to a concrete value.
    ///
    /// For concrete values, returns a clone.
    /// For lazy values, calls the resolver and returns the result.
    ///
    /// # Errors
    ///
    /// Returns an error if the path doesn't exist in the data tree.
    /// Uses a default location for error reporting; use `resolve_at` for precise locations.
    pub async fn resolve(&self) -> Result<facet_value::Value, TemplateError> {
        match self {
            Self::Concrete(value) => Ok(value.clone()),
            Self::Lazy { resolver, path } => resolver.resolve(path).await.ok_or_else(|| {
                DataPathNotFoundError {
                    path: path.to_string(),
                    loc: SourceLocation::new(
                        SourceSpan::default(),
                        crate::error::NamedSource::new("<unknown>", ""),
                    ),
                }
                .into()
            }),
        }
    }

    /// Force resolution with source location for error reporting.
    pub async fn resolve_at(
        &self,
        span: crate::ast::Span,
        source: &TemplateSource,
    ) -> Result<facet_value::Value, TemplateError> {
        match self {
            Self::Concrete(value) => Ok(value.clone()),
            Self::Lazy { resolver, path } => resolver.resolve(path).await.ok_or_else(|| {
                DataPathNotFoundError {
                    path: path.to_string(),
                    loc: SourceLocation::new(span, source.named_source()),
                }
                .into()
            }),
        }
    }

    /// Try to resolve, returning None if the path doesn't exist.
    pub async fn try_resolve(&self) -> Option<facet_value::Value> {
        match self {
            Self::Concrete(value) => Some(value.clone()),
            Self::Lazy { resolver, path } => resolver.resolve(path).await,
        }
    }

    /// Access a field by name.
    ///
    /// For lazy values, this extends the path without resolving.
    /// For concrete values, this performs normal field access.
    pub fn field(
        &self,
        name: &str,
        span: crate::ast::Span,
        source: &TemplateSource,
    ) -> Result<LazyValue, TemplateError> {
        match self {
            Self::Lazy { resolver, path } => {
                // Extend the path - don't resolve yet!
                Ok(Self::Lazy {
                    resolver: resolver.clone(),
                    path: path.push(name),
                })
            }
            Self::Concrete(value) => {
                // Normal field access on concrete value
                match value.destructure_ref() {
                    DestructuredRef::Object(obj) => {
                        obj.get(name).cloned().map(Self::Concrete).ok_or_else(|| {
                            UnknownFieldError {
                                base_type: "dict".to_string(),
                                field: name.to_string(),
                                known_fields: obj.keys().map(|k| k.to_string()).collect(),
                                loc: SourceLocation::new(span, source.named_source()),
                            }
                            .into()
                        })
                    }
                    _ => Err(TypeError {
                        expected: "object or dict".to_string(),
                        found: self.type_name().to_string(),
                        context: format!("accessing field `{}`", name),
                        loc: SourceLocation::new(span, source.named_source()),
                    }
                    .into()),
                }
            }
        }
    }

    /// Access an element by index.
    ///
    /// For lazy values, this extends the path with the index.
    /// For concrete values, this performs normal index access.
    pub fn index(
        &self,
        idx: i64,
        span: crate::ast::Span,
        source: &TemplateSource,
    ) -> Result<LazyValue, TemplateError> {
        match self {
            Self::Lazy { resolver, path } => {
                // Extend the path with the index
                Ok(Self::Lazy {
                    resolver: resolver.clone(),
                    path: path.push(idx.to_string()),
                })
            }
            Self::Concrete(value) => match value.destructure_ref() {
                DestructuredRef::Array(arr) => {
                    let i = if idx < 0 {
                        (arr.len() as i64 + idx) as usize
                    } else {
                        idx as usize
                    };
                    arr.get(i).cloned().map(Self::Concrete).ok_or_else(|| {
                        TypeError {
                            expected: format!("index < {}", arr.len()),
                            found: format!("index {i}"),
                            context: "list index".to_string(),
                            loc: SourceLocation::new(span, source.named_source()),
                        }
                        .into()
                    })
                }
                _ => Err(TypeError {
                    expected: "list".to_string(),
                    found: self.type_name().to_string(),
                    context: "index access".to_string(),
                    loc: SourceLocation::new(span, source.named_source()),
                }
                .into()),
            },
        }
    }

    /// Access by string key (for object["key"] syntax).
    pub fn index_str(
        &self,
        key: &str,
        span: crate::ast::Span,
        source: &TemplateSource,
    ) -> Result<LazyValue, TemplateError> {
        match self {
            Self::Lazy { resolver, path } => Ok(Self::Lazy {
                resolver: resolver.clone(),
                path: path.push(key),
            }),
            Self::Concrete(value) => match value.destructure_ref() {
                DestructuredRef::Object(obj) => {
                    obj.get(key).cloned().map(Self::Concrete).ok_or_else(|| {
                        UnknownFieldError {
                            base_type: "dict".to_string(),
                            field: key.to_string(),
                            known_fields: obj.keys().map(|k| k.to_string()).collect(),
                            loc: SourceLocation::new(span, source.named_source()),
                        }
                        .into()
                    })
                }
                _ => Err(TypeError {
                    expected: "dict".to_string(),
                    found: self.type_name().to_string(),
                    context: "string index access".to_string(),
                    loc: SourceLocation::new(span, source.named_source()),
                }
                .into()),
            },
        }
    }

    /// Get an iterator over (key, value) pairs.
    ///
    /// For lazy values, this resolves the keys but keeps values lazy.
    /// For concrete values, this iterates normally.
    pub async fn iter_pairs(&self) -> Vec<(String, LazyValue)> {
        match self {
            Self::Lazy { resolver, path } => {
                // Get keys at this path, but keep values lazy!
                let keys = resolver.keys_at(path).await.unwrap_or_default();
                keys.into_iter()
                    .map(|key| {
                        let child_path = path.push(&key);
                        (
                            key,
                            Self::Lazy {
                                resolver: resolver.clone(),
                                path: child_path,
                            },
                        )
                    })
                    .collect()
            }
            Self::Concrete(value) => match value.destructure_ref() {
                DestructuredRef::Object(obj) => obj
                    .iter()
                    .map(|(k, v)| (k.to_string(), Self::Concrete(v.clone())))
                    .collect(),
                DestructuredRef::Array(arr) => arr
                    .iter()
                    .enumerate()
                    .map(|(i, v)| (i.to_string(), Self::Concrete(v.clone())))
                    .collect(),
                _ => Vec::new(),
            },
        }
    }

    /// Get an iterator over values only (for simple for loops).
    pub async fn iter_values(&self) -> Vec<LazyValue> {
        match self {
            Self::Lazy { resolver, path } => {
                let keys = resolver.keys_at(path).await.unwrap_or_default();
                keys.into_iter()
                    .map(|key| Self::Lazy {
                        resolver: resolver.clone(),
                        path: path.push(&key),
                    })
                    .collect()
            }
            Self::Concrete(value) => match value.destructure_ref() {
                DestructuredRef::Array(arr) => arr.iter().cloned().map(Self::Concrete).collect(),
                DestructuredRef::Object(obj) => {
                    // For object iteration, return key-value pair objects
                    obj.iter()
                        .map(|(k, v)| {
                            let mut entry = facet_value::VObject::new();
                            entry.insert(
                                facet_value::VString::from("key"),
                                facet_value::Value::from(k.as_str()),
                            );
                            entry.insert(facet_value::VString::from("value"), v.clone());
                            Self::Concrete(entry.into())
                        })
                        .collect()
                }
                DestructuredRef::String(s) => s
                    .as_str()
                    .chars()
                    .map(|c| Self::Concrete(facet_value::Value::from(c)))
                    .collect(),
                _ => Vec::new(),
            },
        }
    }

    /// Check if the value is truthy (forces resolution for lazy values).
    pub async fn is_truthy(&self) -> bool {
        match self.try_resolve().await {
            Some(v) => crate::eval::ValueExt::is_truthy(&v),
            None => false,
        }
    }

    /// Check if the value is null.
    pub async fn is_null(&self) -> bool {
        match self.try_resolve().await {
            Some(v) => v.is_null(),
            None => true, // Unresolvable paths are treated as null
        }
    }

    /// Get a type name for error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            Self::Lazy { .. } => "lazy",
            Self::Concrete(v) => match v.destructure_ref() {
                DestructuredRef::Null => "none",
                DestructuredRef::Bool(_) => "bool",
                DestructuredRef::Number(_) => "number",
                DestructuredRef::String(_) => "string",
                DestructuredRef::Bytes(_) => "bytes",
                DestructuredRef::Array(_) => "list",
                DestructuredRef::Object(_) => "dict",
                DestructuredRef::DateTime(_) => "datetime",
                DestructuredRef::QName(_) => "qname",
                DestructuredRef::Uuid(_) => "uuid",
                DestructuredRef::Char(_) => "char",
                _ => "unknown",
            },
        }
    }

    /// Render the value to a string (forces resolution).
    pub async fn render_to_string(&self) -> String {
        match self.try_resolve().await {
            Some(v) => crate::eval::ValueExt::render_to_string(&v),
            None => String::new(),
        }
    }

    /// Check if this value is marked as "safe" (forces resolution).
    pub async fn is_safe(&self) -> bool {
        match self.try_resolve().await {
            Some(v) => crate::eval::ValueExt::is_safe(&v),
            None => false,
        }
    }

    /// Get the length of a container (may not require full resolution).
    pub async fn len(&self) -> Option<usize> {
        match self {
            Self::Lazy { resolver, path } => resolver.len_at(path).await,
            Self::Concrete(value) => match value.destructure_ref() {
                DestructuredRef::Array(arr) => Some(arr.len()),
                DestructuredRef::Object(obj) => Some(obj.len()),
                DestructuredRef::String(s) => Some(s.len()),
                _ => None,
            },
        }
    }

    /// Check if the container is empty.
    pub async fn is_empty(&self) -> bool {
        self.len().await.map(|l| l == 0).unwrap_or(true)
    }
}

// Conversion from facet_value::Value
impl From<facet_value::Value> for LazyValue {
    fn from(value: facet_value::Value) -> Self {
        Self::Concrete(value)
    }
}

// Convenience conversions
impl From<&str> for LazyValue {
    fn from(s: &str) -> Self {
        Self::Concrete(facet_value::Value::from(s))
    }
}

impl From<String> for LazyValue {
    fn from(s: String) -> Self {
        Self::Concrete(facet_value::Value::from(s.as_str()))
    }
}

impl From<i64> for LazyValue {
    fn from(n: i64) -> Self {
        Self::Concrete(facet_value::Value::from(n))
    }
}

impl From<f64> for LazyValue {
    fn from(n: f64) -> Self {
        Self::Concrete(facet_value::Value::from(n))
    }
}

impl From<bool> for LazyValue {
    fn from(b: bool) -> Self {
        Self::Concrete(facet_value::Value::from(b))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet_value::{VObject, VString, Value};
    use std::sync::Mutex;

    fn test_span() -> crate::ast::Span {
        crate::ast::span(0, 0)
    }

    /// A simple in-memory data resolver for testing.
    struct TestResolver {
        data: Value,
        /// Track which paths were resolved (for testing)
        resolved_paths: Mutex<Vec<DataPath>>,
    }

    impl TestResolver {
        fn new(data: Value) -> Arc<Self> {
            Arc::new(Self {
                data,
                resolved_paths: Mutex::new(Vec::new()),
            })
        }

        fn resolved_paths(&self) -> Vec<DataPath> {
            self.resolved_paths.lock().unwrap().clone()
        }
    }

    impl DataResolver for TestResolver {
        fn resolve(
            &self,
            path: &DataPath,
        ) -> Pin<Box<dyn Future<Output = Option<Value>> + Send + '_>> {
            let path = path.clone();
            Box::pin(async move {
                self.resolved_paths.lock().unwrap().push(path.clone());

                let mut current = self.data.clone();
                for segment in path.segments() {
                    current = match current.destructure_ref() {
                        DestructuredRef::Object(obj) => obj.get(segment.as_str())?.clone(),
                        DestructuredRef::Array(arr) => {
                            let idx: usize = segment.parse().ok()?;
                            arr.get(idx)?.clone()
                        }
                        _ => return None,
                    };
                }
                Some(current)
            })
        }

        fn keys_at(
            &self,
            path: &DataPath,
        ) -> Pin<Box<dyn Future<Output = Option<Vec<String>>> + Send + '_>> {
            let path = path.clone();
            Box::pin(async move {
                let value = self.resolve(&path).await?;
                match value.destructure_ref() {
                    DestructuredRef::Object(obj) => {
                        Some(obj.keys().map(|k| k.to_string()).collect())
                    }
                    DestructuredRef::Array(arr) => {
                        Some((0..arr.len()).map(|i| i.to_string()).collect())
                    }
                    _ => None,
                }
            })
        }
    }

    fn make_test_data() -> Value {
        let mut versions = VObject::new();

        let mut dodeca = VObject::new();
        dodeca.insert(VString::from("version"), Value::from("0.1.0"));
        dodeca.insert(VString::from("name"), Value::from("Dodeca"));
        versions.insert(VString::from("dodeca"), Value::from(dodeca));

        let mut facet = VObject::new();
        facet.insert(VString::from("version"), Value::from("0.2.0"));
        versions.insert(VString::from("facet"), Value::from(facet));

        let mut root = VObject::new();
        root.insert(VString::from("versions"), Value::from(versions));
        Value::from(root)
    }

    #[test]
    fn test_data_path() {
        let root = DataPath::root();
        assert!(root.is_root());
        assert_eq!(root.segments(), &[] as &[String]);

        let child = root.push("versions");
        assert!(!child.is_root());
        assert_eq!(child.segments(), &["versions"]);

        let grandchild = child.push("dodeca");
        assert_eq!(grandchild.segments(), &["versions", "dodeca"]);
    }

    #[tokio::test]
    async fn test_lazy_field_access_extends_path() {
        let resolver = TestResolver::new(make_test_data());
        let lazy = LazyValue::lazy_root(resolver.clone());

        // Field access should NOT resolve yet
        let source = crate::error::TemplateSource::new("test", "");
        let versions = lazy.field("versions", test_span(), &source).unwrap();

        // Nothing should be resolved yet!
        assert!(resolver.resolved_paths().is_empty());
        assert!(versions.is_lazy());

        // Further field access still doesn't resolve
        let dodeca = versions.field("dodeca", test_span(), &source).unwrap();
        assert!(resolver.resolved_paths().is_empty());

        // Now resolve
        let value = dodeca.field("version", test_span(), &source).unwrap();
        let resolved = value.resolve().await.unwrap();

        // NOW it should have resolved
        assert_eq!(resolver.resolved_paths().len(), 1);
        assert_eq!(
            resolver.resolved_paths()[0].segments(),
            &["versions", "dodeca", "version"]
        );

        // Check the value
        if let DestructuredRef::String(s) = resolved.destructure_ref() {
            assert_eq!(s.as_str(), "0.1.0");
        } else {
            panic!("Expected string");
        }
    }

    #[tokio::test]
    async fn test_lazy_iteration() {
        let resolver = TestResolver::new(make_test_data());
        let lazy = LazyValue::lazy_root(resolver.clone());

        let source = crate::error::TemplateSource::new("test", "");
        let versions = lazy.field("versions", test_span(), &source).unwrap();

        // Get pairs - this should resolve keys but NOT values
        let pairs = versions.iter_pairs().await;
        assert_eq!(pairs.len(), 2);

        // Keys were resolved to get the list
        assert_eq!(resolver.resolved_paths().len(), 1);

        // But values are still lazy!
        for (key, value) in &pairs {
            assert!(value.is_lazy(), "Value for {} should be lazy", key);
        }

        // Now access one value
        let (_, dodeca) = pairs.iter().find(|(k, _)| k == "dodeca").unwrap();
        let version = dodeca.field("version", test_span(), &source).unwrap();
        let _ = version.resolve().await.unwrap();

        // Now we should have resolved the specific path
        let paths = resolver.resolved_paths();
        assert!(
            paths
                .iter()
                .any(|p| p.segments() == ["versions", "dodeca", "version"])
        );
    }

    #[tokio::test]
    async fn test_concrete_value_access() {
        let mut obj = VObject::new();
        obj.insert(VString::from("name"), Value::from("test"));
        let concrete = LazyValue::Concrete(Value::from(obj));

        let source = crate::error::TemplateSource::new("test", "");
        let name = concrete.field("name", test_span(), &source).unwrap();

        // Should still be concrete
        assert!(name.is_concrete());

        let resolved = name.resolve().await.unwrap();
        if let DestructuredRef::String(s) = resolved.destructure_ref() {
            assert_eq!(s.as_str(), "test");
        } else {
            panic!("Expected string");
        }
    }

    #[tokio::test]
    async fn test_render_to_string() {
        let resolver = TestResolver::new(make_test_data());
        let lazy = LazyValue::lazy_root(resolver.clone());

        let source = crate::error::TemplateSource::new("test", "");
        let version = lazy
            .field("versions", test_span(), &source)
            .unwrap()
            .field("dodeca", test_span(), &source)
            .unwrap()
            .field("version", test_span(), &source)
            .unwrap();

        // render_to_string triggers resolution
        assert_eq!(version.render_to_string().await, "0.1.0");
    }
}
