//! Process-global cache for `TypePlanCore` built from `Shape`.
//!
//! The cache intentionally leaks one `TypePlanCore` per distinct shape for the
//! lifetime of the process. This trades bounded memory for fast, shared plan
//! reuse and `'static` plan references in format-layer code.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use facet_core::Shape;
use facet_reflect::{AllocError, TypePlanCore};

type ShapeKey = usize;
type PlanPtr = usize;

fn cache() -> &'static Mutex<HashMap<ShapeKey, PlanPtr>> {
    static PLAN_CACHE: OnceLock<Mutex<HashMap<ShapeKey, PlanPtr>>> = OnceLock::new();
    PLAN_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Get a cached plan reference for a shape, building and leaking on cache miss.
pub(crate) fn cached_type_plan(shape: &'static Shape) -> Result<&'static TypePlanCore, AllocError> {
    let shape_key = shape as *const Shape as usize;
    let mut guard = cache().lock().unwrap_or_else(|poison| poison.into_inner());

    if let Some(&plan_ptr) = guard.get(&shape_key) {
        // SAFETY: plan_ptr values come from Arc::into_raw and are never freed.
        return Ok(unsafe { &*(plan_ptr as *const TypePlanCore) });
    }

    // SAFETY: caller provides a valid `'static` shape.
    let plan = unsafe { TypePlanCore::from_shape(shape)? };
    let plan_ptr = Arc::into_raw(plan) as usize;
    guard.insert(shape_key, plan_ptr);

    // SAFETY: plan_ptr came from Arc::into_raw and stays valid for process lifetime.
    Ok(unsafe { &*(plan_ptr as *const TypePlanCore) })
}

/// Get a cached plan as `Arc<TypePlanCore>`.
pub(crate) fn cached_type_plan_arc(shape: &'static Shape) -> Result<Arc<TypePlanCore>, AllocError> {
    let plan_ptr = cached_type_plan(shape)? as *const TypePlanCore;
    // SAFETY: plan_ptr is from Arc::into_raw and kept alive forever by the cache leak.
    unsafe {
        Arc::increment_strong_count(plan_ptr);
        Ok(Arc::from_raw(plan_ptr))
    }
}

#[cfg(test)]
fn clear_cache_for_tests() {
    cache()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .clear();
}

#[cfg(test)]
fn cache_len_for_tests() -> usize {
    cache()
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use facet_core::Facet;

    fn test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    #[test]
    fn cache_hit_miss_behavior() {
        let _guard = test_lock()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        clear_cache_for_tests();
        assert_eq!(cache_len_for_tests(), 0);

        let first = cached_type_plan(i32::SHAPE).unwrap();
        assert_eq!(cache_len_for_tests(), 1);

        let second = cached_type_plan(i32::SHAPE).unwrap();
        assert_eq!(cache_len_for_tests(), 1);
        assert!(core::ptr::eq(first, second));
    }

    #[test]
    fn cache_concurrent_access_single_shape() {
        let _guard = test_lock()
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        clear_cache_for_tests();

        let mut joins = Vec::new();
        for _ in 0..12 {
            joins.push(std::thread::spawn(|| {
                let mut ptrs = Vec::new();
                for _ in 0..40 {
                    let plan = cached_type_plan(<Option<Vec<u64>>>::SHAPE).unwrap();
                    ptrs.push(plan as *const TypePlanCore as usize);
                }
                ptrs
            }));
        }

        let mut all_ptrs = Vec::new();
        for join in joins {
            all_ptrs.extend(join.join().unwrap());
        }

        let first = *all_ptrs.first().unwrap();
        assert!(all_ptrs.into_iter().all(|ptr| ptr == first));
        assert_eq!(cache_len_for_tests(), 1);
    }
}
