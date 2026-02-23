//! Process-global cache for `TypePlanCore` built from typed `Facet` roots.
//!
//! The cache retains one `TypePlanCore` per distinct shape for the lifetime of
//! the process (no eviction). This trades bounded memory for fast shared plan
//! reuse in format-layer code.

use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use facet_core::Facet;
use facet_reflect::{AllocError, TypePlan, TypePlanCore};

type ShapeKey = usize;
type PlanEntry = Arc<TypePlanCore>;

fn cache() -> &'static Mutex<HashMap<ShapeKey, PlanEntry>> {
    static PLAN_CACHE: OnceLock<Mutex<HashMap<ShapeKey, PlanEntry>>> = OnceLock::new();
    PLAN_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Get a cached plan as `Arc<TypePlanCore>`, building on cache miss.
pub(crate) fn cached_type_plan_arc<'facet, T>() -> Result<Arc<TypePlanCore>, AllocError>
where
    T: Facet<'facet>,
{
    let shape_key = T::SHAPE as *const _ as usize;
    let mut guard = cache().lock().unwrap_or_else(|poison| poison.into_inner());

    if let Some(plan) = guard.get(&shape_key) {
        return Ok(Arc::clone(plan));
    }

    let plan = TypePlan::<T>::build()?.core();
    guard.insert(shape_key, Arc::clone(&plan));
    Ok(plan)
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

        let first = cached_type_plan_arc::<i32>().unwrap();
        assert_eq!(cache_len_for_tests(), 1);

        let second = cached_type_plan_arc::<i32>().unwrap();
        assert_eq!(cache_len_for_tests(), 1);
        assert!(Arc::ptr_eq(&first, &second));
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
                    let plan = cached_type_plan_arc::<Option<Vec<u64>>>().unwrap();
                    ptrs.push(Arc::as_ptr(&plan) as usize);
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
