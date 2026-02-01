//! ShapeDesc - an abstraction over static Shape and dynamic Tuple2Shape.
//!
//! This module provides a unified way to handle both compile-time static shapes
//! and runtime-synthesized shapes (like map entry tuples).

use std::alloc::Layout;
use std::collections::HashMap;
use std::fmt;
use std::sync::{LazyLock, Mutex};

use facet_core::{ConstTypeId, Def, Shape, Type, UserType};

/// Small, Copy handle to shape information.
/// Either a static Shape or a leaked Tuple2Shape.
#[derive(Copy, Clone)]
pub enum ShapeDesc {
    /// A static shape from compile-time reflection.
    Static(&'static Shape),
    /// A dynamically created 2-field tuple shape (for map entries).
    Tuple2(&'static Tuple2Shape),
}

/// A 2-field tuple shape (for map entries).
/// Created via Box::leak and cached by (key_type_id, value_type_id).
pub struct Tuple2Shape {
    /// The layout of the tuple (key + padding + value).
    pub layout: Layout,
    /// Type name for error reporting.
    pub type_name: &'static str,
    /// Fields: (offset, shape) for field 0 (key) and field 1 (value).
    pub fields: [(usize, ShapeDesc); 2],
}

/// Global cache: one Tuple2Shape per unique (K, V) type combination.
static TUPLE2_CACHE: LazyLock<Mutex<HashMap<(ConstTypeId, ConstTypeId), &'static Tuple2Shape>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

/// Get or create a Tuple2Shape for the given map definition.
/// Uses the vtable's `pair_stride` and `value_offset_in_pair` for correct layout.
/// The result is cached globally, so repeated calls with the same types return the same shape.
#[allow(clippy::disallowed_methods)] // Box::leak is intentional here - one leak per (K,V) type pair, cached forever
pub fn tuple2(map_def: &facet_core::MapDef) -> &'static Tuple2Shape {
    let key_shape = map_def.k;
    let value_shape = map_def.v;
    let cache_key = (key_shape.id, value_shape.id);
    let mut cache = TUPLE2_CACHE.lock().unwrap();

    if let Some(&cached) = cache.get(&cache_key) {
        return cached;
    }

    // Use vtable values for correct tuple layout
    let pair_stride = map_def.vtable.pair_stride;
    let value_offset = map_def.vtable.value_offset_in_pair;
    let key_layout = key_shape
        .layout
        .sized_layout()
        .expect("key shape must be sized");
    let total_align = key_layout.align().max(
        value_shape
            .layout
            .sized_layout()
            .expect("value shape must be sized")
            .align(),
    );
    let layout =
        Layout::from_size_align(pair_stride, total_align).expect("valid layout for tuple2");

    let tuple2 = Box::leak(Box::new(Tuple2Shape {
        layout,
        type_name: "map entry",
        fields: [
            (0, ShapeDesc::Static(key_shape)),
            (value_offset, ShapeDesc::Static(value_shape)),
        ],
    }));

    cache.insert(cache_key, tuple2);
    tuple2
}

impl ShapeDesc {
    /// Get the sized layout of this shape.
    pub fn layout(&self) -> Layout {
        match self {
            Self::Static(s) => s.layout.sized_layout().expect("static shape must be sized"),
            Self::Tuple2(t) => t.layout,
        }
    }

    /// Get the type of this shape.
    /// Returns a static reference since both Static and Tuple2 variants hold static data.
    pub fn ty(&self) -> &'static Type {
        match self {
            Self::Static(s) => &s.ty,
            Self::Tuple2(_) => {
                // Tuple2 is a synthetic 2-field struct, but we don't have a static Type for it.
                // Return Undefined so pattern matching falls through to default handling.
                static UNDEFINED: Type = Type::Undefined;
                &UNDEFINED
            }
        }
    }

    /// Get the definition of this shape.
    /// Returns a static reference since both Static and Tuple2 variants hold static data.
    pub fn def(&self) -> &'static Def {
        match self {
            Self::Static(s) => &s.def,
            Self::Tuple2(_) => {
                // Tuple2 is like a tuple - uses Def::Undefined
                static UNDEFINED: Def = Def::Undefined;
                &UNDEFINED
            }
        }
    }

    /// Call default_in_place on a value of this shape.
    ///
    /// # Safety
    /// - `ptr` must point to uninitialized memory of the correct size/alignment.
    ///
    /// Returns `Some(())` if default was successfully written, `None` if the type has no default.
    /// Note: Tuple2Shape (map entries) do not support default - returns None.
    pub unsafe fn call_default_in_place(&self, ptr: facet_core::PtrUninit) -> Option<()> {
        match self {
            Self::Static(s) => {
                // SAFETY: caller guarantees ptr is valid
                unsafe { s.call_default_in_place(ptr) }
            }
            Self::Tuple2(_) => {
                // Map entries don't have a default - they're built field by field
                None
            }
        }
    }

    /// Call drop_in_place on a value of this shape.
    ///
    /// # Safety
    /// - `ptr` must point to a valid, initialized value of this shape's type.
    pub unsafe fn call_drop_in_place(&self, ptr: facet_core::PtrMut) {
        match self {
            Self::Static(s) => {
                // SAFETY: caller guarantees ptr is valid
                unsafe { s.call_drop_in_place(ptr) };
            }
            Self::Tuple2(t) => {
                // Drop both fields
                let (key_offset, key_shape) = &t.fields[0];
                let (value_offset, value_shape) = &t.fields[1];

                // SAFETY: caller guarantees ptr is valid, offsets are correct by construction
                unsafe {
                    let key_ptr = facet_core::PtrMut::new(ptr.as_mut_byte_ptr().add(*key_offset));
                    let value_ptr =
                        facet_core::PtrMut::new(ptr.as_mut_byte_ptr().add(*value_offset));

                    key_shape.call_drop_in_place(key_ptr);
                    value_shape.call_drop_in_place(value_ptr);
                }
            }
        }
    }

    /// Get field information by index.
    /// Returns (offset, shape) for the field, or None if index is out of bounds.
    pub fn field(&self, idx: u32) -> Option<(usize, ShapeDesc)> {
        match self {
            Self::Static(s) => {
                // Get field from struct type
                if let Type::User(UserType::Struct(ref st)) = s.ty {
                    let field = st.fields.get(idx as usize)?;
                    Some((field.offset, ShapeDesc::Static(field.shape())))
                } else {
                    None
                }
            }
            Self::Tuple2(t) => {
                let idx = idx as usize;
                if idx < 2 { Some(t.fields[idx]) } else { None }
            }
        }
    }

    /// Get the type identifier string for error reporting.
    pub fn type_identifier(&self) -> &str {
        match self {
            Self::Static(s) => s.type_identifier,
            Self::Tuple2(t) => t.type_name,
        }
    }

    /// Check if this shape is structurally equivalent to another.
    /// Handles cross-variant comparison so (K,V)::SHAPE equals Tuple2Shape{K,V}.
    pub fn is_shape(&self, other: ShapeDesc) -> bool {
        match (self, other) {
            (Self::Static(a), ShapeDesc::Static(b)) => a.is_shape(b),
            (Self::Tuple2(a), ShapeDesc::Tuple2(b)) => {
                // Compare layouts and field shapes
                a.layout == b.layout
                    && a.fields[0].0 == b.fields[0].0 // offset
                    && a.fields[0].1.is_shape(b.fields[0].1) // shape
                    && a.fields[1].0 == b.fields[1].0
                    && a.fields[1].1.is_shape(b.fields[1].1)
            }
            // Cross-variant: structural comparison
            (Self::Static(s), ShapeDesc::Tuple2(t)) => structural_tuple_eq(s, t),
            (Self::Tuple2(t), ShapeDesc::Static(s)) => structural_tuple_eq(s, t),
        }
    }

    /// Get the underlying static shape, if this is a Static variant.
    pub fn as_static(&self) -> Option<&'static Shape> {
        match self {
            Self::Static(s) => Some(s),
            Self::Tuple2(_) => None,
        }
    }

    /// Check if this is a Tuple2 shape (map entry).
    pub fn is_tuple2(&self) -> bool {
        matches!(self, Self::Tuple2(_))
    }
}

/// Check if a static Shape is structurally equivalent to a Tuple2Shape.
fn structural_tuple_eq(s: &'static Shape, t: &Tuple2Shape) -> bool {
    // Must be a 2-field struct/tuple
    let fields = match &s.ty {
        Type::User(UserType::Struct(st)) if st.fields.len() == 2 => st.fields,
        _ => return false,
    };

    // Check layout matches
    let Ok(s_layout) = s.layout.sized_layout() else {
        return false;
    };
    if s_layout != t.layout {
        return false;
    }

    // Check each field: offset and shape
    fields[0].offset == t.fields[0].0
        && ShapeDesc::Static(fields[0].shape()).is_shape(t.fields[0].1)
        && fields[1].offset == t.fields[1].0
        && ShapeDesc::Static(fields[1].shape()).is_shape(t.fields[1].1)
}

impl fmt::Display for ShapeDesc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.type_identifier())
    }
}

impl fmt::Debug for ShapeDesc {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Static(s) => write!(f, "ShapeDesc::Static({})", s.type_identifier),
            Self::Tuple2(t) => write!(f, "ShapeDesc::Tuple2({})", t.type_name),
        }
    }
}

impl From<&'static Shape> for ShapeDesc {
    fn from(shape: &'static Shape) -> Self {
        Self::Static(shape)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet_core::{Def, Facet};
    use std::collections::HashMap;

    /// Helper to extract MapDef from a HashMap shape.
    fn map_def_for<K: for<'a> Facet<'a> + Eq + std::hash::Hash, V: for<'a> Facet<'a>>()
    -> &'static facet_core::MapDef {
        // HashMap::SHAPE.def is Def::Map(MapDef) - we need a reference to the MapDef.
        // Since SHAPE is 'static, we can take a reference to the embedded MapDef.
        let shape = HashMap::<K, V>::SHAPE;
        match &shape.def {
            Def::Map(map_def) => {
                // SAFETY: shape is 'static, so the reference to its embedded MapDef is 'static
                // This transmute converts &'_ MapDef to &'static MapDef
                // which is safe because shape (and its def field) live for 'static
                unsafe {
                    std::mem::transmute::<&facet_core::MapDef, &'static facet_core::MapDef>(map_def)
                }
            }
            _ => panic!("expected Map def"),
        }
    }

    #[test]
    fn test_tuple2_cache() {
        // Getting the same tuple twice should return the same pointer
        let map_def = map_def_for::<String, i32>();
        let t1 = tuple2(map_def);
        let t2 = tuple2(map_def);
        assert!(std::ptr::eq(t1, t2));

        // Different types should return different pointers
        let map_def2 = map_def_for::<i32, String>();
        let t3 = tuple2(map_def2);
        assert!(!std::ptr::eq(t1, t3));
    }

    #[test]
    fn test_tuple2_layout() {
        let map_def = map_def_for::<u8, u64>();
        let t = tuple2(map_def);

        // u8 is 1 byte, u64 is 8 bytes with 8-byte alignment
        // So value offset should be 8 (aligned)
        assert_eq!(t.fields[0].0, 0); // key at 0
        assert_eq!(t.fields[1].0, 8); // value at 8 (aligned)
        assert_eq!(t.layout.size(), 16); // 8 + 8
        assert_eq!(t.layout.align(), 8); // max(1, 8)
    }

    #[test]
    fn test_shape_desc_display() {
        let desc = ShapeDesc::Static(String::SHAPE);
        assert!(desc.to_string().contains("String"));

        let map_def = map_def_for::<String, i32>();
        let t = tuple2(map_def);
        let desc = ShapeDesc::Tuple2(t);
        assert_eq!(desc.to_string(), "map entry");
    }

    #[test]
    fn test_is_shape_static_vs_static() {
        let a = ShapeDesc::Static(String::SHAPE);
        let b = ShapeDesc::Static(String::SHAPE);
        let c = ShapeDesc::Static(i32::SHAPE);

        assert!(a.is_shape(b));
        assert!(!a.is_shape(c));
    }

    #[test]
    fn test_is_shape_tuple2_vs_tuple2() {
        let map_def1 = map_def_for::<String, i32>();
        let map_def2 = map_def_for::<i32, String>();

        let t1 = tuple2(map_def1);
        let t2 = tuple2(map_def1);
        let t3 = tuple2(map_def2);

        assert!(ShapeDesc::Tuple2(t1).is_shape(ShapeDesc::Tuple2(t2)));
        assert!(!ShapeDesc::Tuple2(t1).is_shape(ShapeDesc::Tuple2(t3)));
    }
}
