//! TypeId -> compiled function cache

use facet_core::Shape;
use parking_lot::RwLock;
use std::any::TypeId;
use std::collections::HashMap;
use std::sync::LazyLock;

use super::compiler::CompiledDeserializer;

static CACHE: LazyLock<RwLock<HashMap<TypeId, CompiledDeserializer>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

// Wrapper for Shape pointer to implement Send/Sync
// Safety: Shape pointers are always to 'static data
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
struct ShapePtr(*const Shape);
unsafe impl Send for ShapePtr {}
unsafe impl Sync for ShapePtr {}

// Cache by Shape pointer for nested struct lookups
static SHAPE_CACHE: LazyLock<RwLock<HashMap<ShapePtr, CompiledDeserializer>>> =
    LazyLock::new(|| RwLock::new(HashMap::new()));

/// Cache for JIT-compiled deserializers.
pub struct JitCache;

impl JitCache {
    /// Clear all cached deserializers.
    /// Useful for benchmarking or if you need to force recompilation.
    pub fn clear() {
        CACHE.write().clear();
    }

    /// Number of cached deserializers.
    pub fn len() -> usize {
        CACHE.read().len()
    }

    /// Check if cache is empty.
    pub fn is_empty() -> bool {
        CACHE.read().is_empty()
    }
}

pub(super) fn get(type_id: TypeId) -> Option<CompiledDeserializer> {
    CACHE.read().get(&type_id).copied()
}

pub(super) fn insert(type_id: TypeId, func: CompiledDeserializer) {
    CACHE.write().insert(type_id, func);
}

// Shape-keyed cache functions for nested struct lookups
pub(super) fn get_by_shape(shape: &'static Shape) -> Option<CompiledDeserializer> {
    SHAPE_CACHE
        .read()
        .get(&ShapePtr(shape as *const Shape))
        .copied()
}

pub(super) fn insert_by_shape(shape: &'static Shape, func: CompiledDeserializer) {
    SHAPE_CACHE
        .write()
        .insert(ShapePtr(shape as *const Shape), func);
}
