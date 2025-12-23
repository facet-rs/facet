//! Lightweight structural equality checking for Facet types.
//!
//! This module provides structural equality checking that short-circuits on the
//! first difference. Unlike `facet_diff::tree_diff()`, it does not allocate
//! or compute an edit script, making it suitable for performance-critical paths
//! like input deduplication and derived value caching.
//!
//! The key advantage over vtable-based `PartialEq` is that we can structurally
//! compare containers (Vec, HashMap, etc.) even when their elements don't
//! implement `PartialEq`. We recursively compare each element using reflection.

use facet::Facet;
use facet_core::{Def, StructKind, Type, UserType};
use facet_reflect::{HasFields, Peek};
use std::any::Any;

/// Check if two type-erased Facet values are equal.
///
/// This function:
/// - Downcasts both `dyn Any` values to the concrete type `V`
/// - Performs structural comparison using facet reflection
/// - Works even when inner types don't implement PartialEq
///
/// # Type Parameters
/// - `V`: Must implement `Facet<'static>` for shape information
///
/// # Returns
/// - `true` if values are structurally equal
/// - `false` if types don't match or values differ
pub fn facet_eq<V>(a: &dyn Any, b: &dyn Any) -> bool
where
    V: Facet<'static> + 'static,
{
    // Downcast to concrete type
    let Some(a) = a.downcast_ref::<V>() else {
        return false;
    };
    let Some(b) = b.downcast_ref::<V>() else {
        return false;
    };

    facet_eq_direct(a, b)
}

/// Check if two concrete Facet values are structurally equal.
///
/// Uses facet's reflection to perform deep structural comparison.
/// Unlike vtable-based PartialEq, this works even for containers
/// whose elements don't implement PartialEq.
///
/// # Performance
///
/// - O(1) allocation (no Vec allocation like tree_diff)
/// - Early return on first mismatch
/// - Uses facet's reflection/vtables directly
///
/// # Examples
///
/// ```ignore
/// use picante::facet_eq::facet_eq_direct;
///
/// #[derive(Facet)]
/// struct User {
///     name: String,
///     age: u32,
/// }
///
/// let u1 = User { name: "Alice".into(), age: 30 };
/// let u2 = User { name: "Alice".into(), age: 30 };
/// let u3 = User { name: "Bob".into(), age: 30 };
///
/// assert!(facet_eq_direct(&u1, &u2)); // Same values
/// assert!(!facet_eq_direct(&u1, &u3)); // Different names
/// ```
#[inline]
pub fn facet_eq_direct<V>(a: &V, b: &V) -> bool
where
    V: Facet<'static> + 'static,
{
    let peek_a = Peek::new(a);
    let peek_b = Peek::new(b);

    peek_eq(peek_a, peek_b)
}

/// Internal recursive equality check for Peek values.
///
/// This mirrors the structure of `Peek::structural_hash()` but instead of
/// hashing, it compares values and returns early on the first difference.
fn peek_eq<'mem, 'facet>(a: Peek<'mem, 'facet>, b: Peek<'mem, 'facet>) -> bool {
    // Different shapes are never equal
    if a.shape() != b.shape() {
        return false;
    }

    // Handle known structural types first - we can do better than their vtable partial_eq
    // (e.g., Vec's vtable partial_eq fails if elements lack PartialEq, but we can still
    // compare structurally by recursing on each element)
    match a.shape().ty {
        Type::User(UserType::Struct(_)) => {
            let Ok(struct_a) = a.into_struct() else {
                return false;
            };
            let Ok(struct_b) = b.into_struct() else {
                return false;
            };

            // Compare each field
            let fields_a: Vec<_> = struct_a
                .fields()
                .filter(|(field, _)| !field.is_metadata())
                .collect();
            let fields_b: Vec<_> = struct_b
                .fields()
                .filter(|(field, _)| !field.is_metadata())
                .collect();

            if fields_a.len() != fields_b.len() {
                return false;
            }

            for ((field_a, peek_a), (field_b, peek_b)) in fields_a.iter().zip(fields_b.iter()) {
                if field_a.name != field_b.name {
                    return false;
                }
                if !peek_eq(*peek_a, *peek_b) {
                    return false;
                }
            }
            true
        }

        Type::User(UserType::Enum(_)) => {
            let Ok(enum_a) = a.into_enum() else {
                return false;
            };
            let Ok(enum_b) = b.into_enum() else {
                return false;
            };

            let Ok(variant_a) = enum_a.active_variant() else {
                return false;
            };
            let Ok(variant_b) = enum_b.active_variant() else {
                return false;
            };

            // Different variants are never equal
            if variant_a.name != variant_b.name {
                return false;
            }

            // Compare variant fields based on struct kind
            match variant_a.data.kind {
                StructKind::Unit => {
                    // Unit variants have no fields
                    true
                }
                StructKind::Tuple | StructKind::TupleStruct => {
                    // Compare tuple fields by index
                    if variant_a.data.fields.len() != variant_b.data.fields.len() {
                        return false;
                    }

                    for i in 0..variant_a.data.fields.len() {
                        let Ok(Some(field_a)) = enum_a.field(i) else {
                            return false;
                        };
                        let Ok(Some(field_b)) = enum_b.field(i) else {
                            return false;
                        };

                        if !peek_eq(field_a, field_b) {
                            return false;
                        }
                    }
                    true
                }
                StructKind::Struct => {
                    // Compare struct fields by name
                    for field in variant_a.data.fields {
                        let Ok(Some(field_a)) = enum_a.field_by_name(field.name) else {
                            return false;
                        };
                        let Ok(Some(field_b)) = enum_b.field_by_name(field.name) else {
                            return false;
                        };

                        if !peek_eq(field_a, field_b) {
                            return false;
                        }
                    }
                    true
                }
            }
        }

        _ => {
            // Handle container types via Def before trying vtable partial_eq
            match a.shape().def {
                Def::List(_) | Def::Array(_) | Def::Slice(_) => {
                    let Ok(list_a) = a.into_list() else {
                        return false;
                    };
                    let Ok(list_b) = b.into_list() else {
                        return false;
                    };

                    if list_a.len() != list_b.len() {
                        return false;
                    }

                    for i in 0..list_a.len() {
                        let Some(elem_a) = list_a.get(i) else {
                            return false;
                        };
                        let Some(elem_b) = list_b.get(i) else {
                            return false;
                        };

                        if !peek_eq(elem_a, elem_b) {
                            return false;
                        }
                    }
                    true
                }

                Def::Map(_) => {
                    let Ok(map_a) = a.into_map() else {
                        return false;
                    };
                    let Ok(map_b) = b.into_map() else {
                        return false;
                    };

                    // Maps must have the same length
                    if map_a.len() != map_b.len() {
                        return false;
                    }

                    // Use the map's native lookup (O(1) for HashMap, O(log n) for BTreeMap)
                    for (key_a, value_a) in map_a.iter() {
                        // Look up key_a in map_b using the vtable's get
                        let Ok(Some(value_b)) = map_b.get_peek(key_a) else {
                            return false; // Key not found in map_b
                        };
                        if !peek_eq(value_a, value_b) {
                            return false;
                        }
                    }
                    true
                }

                Def::Set(_) => {
                    let Ok(set_a) = a.into_set() else {
                        return false;
                    };
                    let Ok(set_b) = b.into_set() else {
                        return false;
                    };

                    // Sets must have the same length
                    if set_a.len() != set_b.len() {
                        return false;
                    }

                    // Use the set's native lookup (O(1) for HashSet, O(log n) for BTreeSet)
                    for elem_a in set_a.iter() {
                        let Ok(true) = set_b.contains_peek(elem_a) else {
                            return false; // Element not found in set_b
                        };
                    }
                    true
                }

                Def::Option(_) => {
                    let Ok(opt_a) = a.into_option() else {
                        return false;
                    };
                    let Ok(opt_b) = b.into_option() else {
                        return false;
                    };

                    match (opt_a.value(), opt_b.value()) {
                        (Some(inner_a), Some(inner_b)) => peek_eq(inner_a, inner_b),
                        (None, None) => true,
                        _ => false,
                    }
                }

                Def::Result(_) => {
                    let Ok(result_a) = a.into_result() else {
                        return false;
                    };
                    let Ok(result_b) = b.into_result() else {
                        return false;
                    };

                    match (result_a.ok(), result_b.ok()) {
                        (Some(ok_a), Some(ok_b)) => peek_eq(ok_a, ok_b),
                        (None, None) => match (result_a.err(), result_b.err()) {
                            (Some(err_a), Some(err_b)) => peek_eq(err_a, err_b),
                            _ => false,
                        },
                        _ => false,
                    }
                }

                _ => {
                    // Not a known container type, try vtable partial_eq for scalars
                    // This handles primitives (i32, bool, etc.) and types that derive PartialEq
                    if let Ok(result) = a.partial_eq(&b) {
                        return result;
                    }

                    // No vtable partial_eq available and we don't know how to structurally compare
                    // this type. Conservatively return false.
                    false
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet::Facet;
    use std::collections::HashMap;

    // A type that does NOT implement PartialEq
    #[allow(clippy::derived_hash_with_manual_eq)]
    #[derive(Facet, Debug, Clone, Hash, Eq)]
    struct NoPartialEq {
        value: i32,
    }

    // Manually implement PartialEq for Hash/Eq requirements but NOT derive it
    // so facet won't see it in the vtable
    impl PartialEq for NoPartialEq {
        fn eq(&self, other: &Self) -> bool {
            self.value == other.value
        }
    }

    // A wrapper that truly has no PartialEq
    #[derive(Facet, Debug, Clone)]
    struct TrulyNoEq {
        data: i32,
    }

    #[derive(Facet, Debug, Clone, PartialEq)]
    struct WithPartialEq {
        value: i32,
    }

    #[derive(Facet, Debug, Clone, PartialEq)]
    struct Container {
        items: Vec<WithPartialEq>,
    }

    #[derive(Facet, Debug, Clone)]
    struct ContainerNoEq {
        items: Vec<TrulyNoEq>,
    }

    #[test]
    fn test_simple_struct_equality() {
        let a = WithPartialEq { value: 42 };
        let b = WithPartialEq { value: 42 };
        let c = WithPartialEq { value: 99 };

        assert!(facet_eq_direct(&a, &b));
        assert!(!facet_eq_direct(&a, &c));
    }

    #[test]
    fn test_vec_with_partial_eq_elements() {
        let a = Container {
            items: vec![WithPartialEq { value: 1 }, WithPartialEq { value: 2 }],
        };
        let b = Container {
            items: vec![WithPartialEq { value: 1 }, WithPartialEq { value: 2 }],
        };
        let c = Container {
            items: vec![WithPartialEq { value: 1 }, WithPartialEq { value: 3 }],
        };

        assert!(facet_eq_direct(&a, &b));
        assert!(!facet_eq_direct(&a, &c));
    }

    #[test]
    fn test_vec_without_partial_eq_elements() {
        // This is the key test - Vec<TrulyNoEq> where TrulyNoEq doesn't have PartialEq
        let a = ContainerNoEq {
            items: vec![TrulyNoEq { data: 1 }, TrulyNoEq { data: 2 }],
        };
        let b = ContainerNoEq {
            items: vec![TrulyNoEq { data: 1 }, TrulyNoEq { data: 2 }],
        };
        let c = ContainerNoEq {
            items: vec![TrulyNoEq { data: 1 }, TrulyNoEq { data: 3 }],
        };

        assert!(facet_eq_direct(&a, &b), "equal containers should be equal");
        assert!(
            !facet_eq_direct(&a, &c),
            "different containers should not be equal"
        );
    }

    #[test]
    fn test_hashmap_equality() {
        let mut a: HashMap<String, i32> = HashMap::new();
        a.insert("one".to_string(), 1);
        a.insert("two".to_string(), 2);

        let mut b: HashMap<String, i32> = HashMap::new();
        b.insert("two".to_string(), 2);
        b.insert("one".to_string(), 1);

        let mut c: HashMap<String, i32> = HashMap::new();
        c.insert("one".to_string(), 1);
        c.insert("two".to_string(), 99);

        assert!(facet_eq_direct(&a, &b), "same maps should be equal");
        assert!(
            !facet_eq_direct(&a, &c),
            "different values should not be equal"
        );
    }

    #[test]
    fn test_option_equality() {
        let a: Option<i32> = Some(42);
        let b: Option<i32> = Some(42);
        let c: Option<i32> = Some(99);
        let d: Option<i32> = None;

        assert!(facet_eq_direct(&a, &b));
        assert!(!facet_eq_direct(&a, &c));
        assert!(!facet_eq_direct(&a, &d));
        assert!(facet_eq_direct(&d, &d));
    }

    #[test]
    fn test_nested_containers() {
        let a: Vec<Vec<i32>> = vec![vec![1, 2], vec![3, 4]];
        let b: Vec<Vec<i32>> = vec![vec![1, 2], vec![3, 4]];
        let c: Vec<Vec<i32>> = vec![vec![1, 2], vec![3, 5]];

        assert!(facet_eq_direct(&a, &b));
        assert!(!facet_eq_direct(&a, &c));
    }

    #[test]
    fn test_truly_no_eq_vtable_check() {
        // Verify that TrulyNoEq has no partial_eq in its vtable
        let a = TrulyNoEq { data: 42 };
        let peek = Peek::new(&a);

        // This should return Err because TrulyNoEq doesn't have PartialEq
        let result = peek.partial_eq(&peek);
        assert!(
            result.is_err(),
            "TrulyNoEq should NOT have PartialEq vtable, but got: {:?}",
            result
        );
    }
}
