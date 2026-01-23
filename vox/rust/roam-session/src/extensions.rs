//! Type-safe extension storage for request context.
//!
//! This module provides the [`Extensions`] type, which allows middleware to
//! insert typed values that handlers can later retrieve.
//!
//! # Attribution
//!
//! This implementation is adapted from the `http` crate's `Extensions` type.
//! See: <https://docs.rs/http/latest/src/http/extensions.rs.html>
//!
//! The `http` crate is dual-licensed under MIT and Apache-2.0, same as roam.

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::fmt;
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::Arc;

/// A type map for storing arbitrary typed values.
///
/// Extensions are keyed by [`TypeId`], so each type can have at most one value.
/// This is the same pattern used by `http::Extensions`.
///
/// # Example
///
/// ```
/// use roam_session::Extensions;
///
/// struct UserId(u64);
/// struct RequestId(String);
///
/// let mut ext = Extensions::new();
/// ext.insert(UserId(42));
/// ext.insert(RequestId("abc-123".into()));
///
/// assert_eq!(ext.get::<UserId>().unwrap().0, 42);
/// assert_eq!(ext.get::<RequestId>().unwrap().0, "abc-123");
/// ```
/// Note: Extensions uses Arc for value storage, making Clone cheap (just
/// cloning Arc pointers). This is important for the CURRENT_EXTENSIONS
/// task-local which needs to clone extensions for async scoping.
#[derive(Default, Clone)]
pub struct Extensions {
    // Use Option<Box<...>> so empty Extensions has no allocation.
    // Most requests won't use extensions at all.
    map: Option<Box<AnyMap>>,
}

// A hasher optimized for TypeId keys.
// TypeIds are already well-distributed, so we just use the lower bits directly.
#[derive(Default)]
struct IdHasher(u64);

impl Hasher for IdHasher {
    fn write(&mut self, _: &[u8]) {
        unreachable!("TypeId calls write_u64");
    }

    #[inline]
    fn write_u64(&mut self, id: u64) {
        self.0 = id;
    }

    #[inline]
    fn write_u128(&mut self, id: u128) {
        // TypeId on some platforms uses u128
        self.0 = id as u64;
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }
}

type AnyMap = HashMap<TypeId, Arc<dyn Any + Send + Sync>, BuildHasherDefault<IdHasher>>;

impl Extensions {
    /// Create an empty `Extensions`.
    #[inline]
    pub fn new() -> Self {
        Self { map: None }
    }

    /// Insert a value into the extensions.
    ///
    /// If a value of this type already existed and this is the only reference
    /// to it, it is returned. Otherwise returns `None`.
    pub fn insert<T: Send + Sync + 'static>(&mut self, val: T) -> Option<T> {
        self.map
            .get_or_insert_with(Default::default)
            .insert(TypeId::of::<T>(), Arc::new(val))
            .and_then(|arc| {
                // Try to unwrap - only succeeds if we have the only reference
                arc.downcast::<T>()
                    .ok()
                    .and_then(|arc| Arc::try_unwrap(arc).ok())
            })
    }

    /// Get a reference to a value of type `T`, if it exists.
    pub fn get<T: Send + Sync + 'static>(&self) -> Option<&T> {
        self.map
            .as_ref()
            .and_then(|map| map.get(&TypeId::of::<T>()))
            .and_then(|boxed| boxed.downcast_ref())
    }

    /// Remove a value of type `T`.
    ///
    /// Returns the value if it existed and this is the only reference to it.
    /// Otherwise returns `None` (the value is still removed from this Extensions).
    pub fn remove<T: Send + Sync + 'static>(&mut self) -> Option<T> {
        self.map
            .as_mut()
            .and_then(|map| map.remove(&TypeId::of::<T>()))
            .and_then(|arc| {
                arc.downcast::<T>()
                    .ok()
                    .and_then(|arc| Arc::try_unwrap(arc).ok())
            })
    }

    /// Clear all extensions.
    pub fn clear(&mut self) {
        if let Some(map) = self.map.as_mut() {
            map.clear();
        }
    }

    /// Returns `true` if there are no extensions.
    pub fn is_empty(&self) -> bool {
        self.map.as_ref().is_none_or(|map| map.is_empty())
    }

    /// Returns the number of extensions.
    pub fn len(&self) -> usize {
        self.map.as_ref().map_or(0, |map| map.len())
    }

    /// Extend this `Extensions` with another, moving all values.
    pub fn extend(&mut self, other: Self) {
        if let Some(other_map) = other.map {
            self.map
                .get_or_insert_with(Default::default)
                .extend(*other_map);
        }
    }
}

impl fmt::Debug for Extensions {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Extensions")
            .field("len", &self.len())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_and_get() {
        let mut ext = Extensions::new();

        assert!(ext.get::<i32>().is_none());

        ext.insert(42i32);
        assert_eq!(ext.get::<i32>(), Some(&42));

        ext.insert(100i32);
        assert_eq!(ext.get::<i32>(), Some(&100));
    }

    #[test]
    fn test_multiple_types() {
        let mut ext = Extensions::new();

        ext.insert(42i32);
        ext.insert("hello");
        ext.insert(1.234f64);

        assert_eq!(ext.get::<i32>(), Some(&42));
        assert_eq!(ext.get::<&str>(), Some(&"hello"));
        assert_eq!(ext.get::<f64>(), Some(&1.234));
    }

    #[test]
    fn test_remove() {
        let mut ext = Extensions::new();
        ext.insert(42i32);

        assert_eq!(ext.remove::<i32>(), Some(42));
        assert!(ext.get::<i32>().is_none());
    }

    #[test]
    fn test_clone() {
        let mut ext = Extensions::new();
        ext.insert(42i32);
        ext.insert("hello");

        // Clone shares the Arc-wrapped values
        let ext2 = ext.clone();
        assert_eq!(ext2.get::<i32>(), Some(&42));
        assert_eq!(ext2.get::<&str>(), Some(&"hello"));

        // Original still works
        assert_eq!(ext.get::<i32>(), Some(&42));
    }

    #[test]
    fn test_empty_no_allocation() {
        let ext = Extensions::new();
        assert!(ext.map.is_none());
        assert!(ext.is_empty());
        assert_eq!(ext.len(), 0);
    }
}
