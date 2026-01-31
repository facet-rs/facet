use core::hash::BuildHasher;
use std::collections::HashSet;

use crate::{PtrConst, PtrMut, PtrUninit};

use crate::{
    Def, Facet, HashProxy, IterVTable, OxPtrConst, OxPtrMut, OxPtrUninit, OxRef, SetDef, SetVTable,
    Shape, ShapeBuilder, Type, TypeNameFn, TypeNameOpts, TypeOpsIndirect, TypeParam, UserType,
    VTableIndirect, Variance, VarianceDep, VarianceDesc,
};

type HashSetIterator<'mem, T> = std::collections::hash_set::Iter<'mem, T>;

unsafe fn hashset_init_in_place_with_capacity<T, S: Default + BuildHasher>(
    uninit: PtrUninit,
    capacity: usize,
) -> PtrMut {
    unsafe {
        uninit.put(HashSet::<T, S>::with_capacity_and_hasher(
            capacity,
            S::default(),
        ))
    }
}

unsafe fn hashset_insert<T: Eq + core::hash::Hash + 'static>(ptr: PtrMut, item: PtrMut) -> bool {
    unsafe {
        let set = ptr.as_mut::<HashSet<T>>();
        let item = item.read::<T>();
        set.insert(item)
    }
}

unsafe fn hashset_len<T: 'static>(ptr: PtrConst) -> usize {
    unsafe { ptr.get::<HashSet<T>>().len() }
}

unsafe fn hashset_contains<T: Eq + core::hash::Hash + 'static>(
    ptr: PtrConst,
    item: PtrConst,
) -> bool {
    unsafe { ptr.get::<HashSet<T>>().contains(item.get()) }
}

unsafe fn hashset_iter_init<T: 'static>(ptr: PtrConst) -> PtrMut {
    unsafe {
        let set = ptr.get::<HashSet<T>>();
        let iter: HashSetIterator<'_, T> = set.iter();
        let iter_state = Box::new(iter);
        PtrMut::new(Box::into_raw(iter_state) as *mut u8)
    }
}

unsafe fn hashset_iter_next<T: 'static>(iter_ptr: PtrMut) -> Option<PtrConst> {
    unsafe {
        let state = iter_ptr.as_mut::<HashSetIterator<'static, T>>();
        state.next().map(|value| PtrConst::new(value as *const T))
    }
}

unsafe fn hashset_iter_dealloc<T>(iter_ptr: PtrMut) {
    unsafe {
        drop(Box::from_raw(
            iter_ptr.as_ptr::<HashSetIterator<'_, T>>() as *mut HashSetIterator<'_, T>
        ));
    }
}

/// Extract the SetDef from a shape, returns None if not a Set
#[inline]
const fn get_set_def(shape: &'static Shape) -> Option<&'static SetDef> {
    match shape.def {
        Def::Set(ref def) => Some(def),
        _ => None,
    }
}

/// Debug for `HashSet<T>` - delegates to inner T's debug if available
unsafe fn hashset_debug(
    ox: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    let shape = ox.shape();
    let def = get_set_def(shape)?;
    let ptr = ox.ptr();

    let mut debug_set = f.debug_set();

    // Initialize iterator
    let iter_init = def.vtable.iter_vtable.init_with_value?;
    let iter_ptr = unsafe { iter_init(ptr) };

    // Iterate over all elements
    loop {
        let item_ptr = unsafe { (def.vtable.iter_vtable.next)(iter_ptr) };
        let Some(item_ptr) = item_ptr else {
            break;
        };
        // SAFETY: The iterator returns valid pointers to set items.
        // The caller guarantees the OxPtrConst points to a valid HashSet.
        let item_ox = unsafe { OxRef::new(item_ptr, def.t) };
        debug_set.entry(&item_ox);
    }

    // Deallocate iterator
    unsafe {
        (def.vtable.iter_vtable.dealloc)(iter_ptr);
    }

    Some(debug_set.finish())
}

/// Hash for `HashSet<T>` - delegates to inner T's hash if available
unsafe fn hashset_hash(ox: OxPtrConst, hasher: &mut HashProxy<'_>) -> Option<()> {
    let shape = ox.shape();
    let def = get_set_def(shape)?;
    let ptr = ox.ptr();

    use core::hash::Hash;

    // Hash the length first
    let len = unsafe { (def.vtable.len)(ptr) };
    len.hash(hasher);

    // Initialize iterator
    let iter_init = def.vtable.iter_vtable.init_with_value?;
    let iter_ptr = unsafe { iter_init(ptr) };

    // Hash all elements
    loop {
        let item_ptr = unsafe { (def.vtable.iter_vtable.next)(iter_ptr) };
        let Some(item_ptr) = item_ptr else {
            break;
        };
        unsafe { def.t.call_hash(item_ptr, hasher)? };
    }

    // Deallocate iterator
    unsafe {
        (def.vtable.iter_vtable.dealloc)(iter_ptr);
    }

    Some(())
}

/// PartialEq for `HashSet<T>`
unsafe fn hashset_partial_eq(a: OxPtrConst, b: OxPtrConst) -> Option<bool> {
    let shape = a.shape();
    let def = get_set_def(shape)?;

    let a_ptr = a.ptr();
    let b_ptr = b.ptr();

    let a_len = unsafe { (def.vtable.len)(a_ptr) };
    let b_len = unsafe { (def.vtable.len)(b_ptr) };

    // If lengths differ, sets are not equal
    if a_len != b_len {
        return Some(false);
    }

    // Initialize iterator for set a
    let iter_init = def.vtable.iter_vtable.init_with_value?;
    let iter_ptr = unsafe { iter_init(a_ptr) };

    // Check if all elements from a are contained in b
    let mut all_contained = true;
    loop {
        let item_ptr = unsafe { (def.vtable.iter_vtable.next)(iter_ptr) };
        let Some(item_ptr) = item_ptr else {
            break;
        };
        let contained = unsafe { (def.vtable.contains)(b_ptr, item_ptr) };
        if !contained {
            all_contained = false;
            break;
        }
    }

    // Deallocate iterator
    unsafe {
        (def.vtable.iter_vtable.dealloc)(iter_ptr);
    }

    Some(all_contained)
}

/// Drop for HashSet<T, S>
unsafe fn hashset_drop<T: 'static, S: 'static>(ox: OxPtrMut) {
    unsafe {
        core::ptr::drop_in_place(ox.as_mut::<HashSet<T, S>>());
    }
}

/// Default for HashSet<T, S>
unsafe fn hashset_default<T: 'static, S: Default + BuildHasher + 'static>(ox: OxPtrUninit) -> bool {
    unsafe { ox.put(HashSet::<T, S>::default()) };
    true
}

unsafe impl<'a, T, S> Facet<'a> for HashSet<T, S>
where
    T: Facet<'a> + core::cmp::Eq + core::hash::Hash + 'static,
    S: Facet<'a> + Default + BuildHasher + 'static,
{
    const SHAPE: &'static Shape = &const {
        const fn build_set_vtable<
            T: Eq + core::hash::Hash + 'static,
            S: Default + BuildHasher + 'static,
        >() -> SetVTable {
            SetVTable::builder()
                .init_in_place_with_capacity(hashset_init_in_place_with_capacity::<T, S>)
                .insert(hashset_insert::<T>)
                .len(hashset_len::<T>)
                .contains(hashset_contains::<T>)
                .iter_vtable(IterVTable {
                    init_with_value: Some(hashset_iter_init::<T>),
                    next: hashset_iter_next::<T>,
                    next_back: None,
                    size_hint: None,
                    dealloc: hashset_iter_dealloc::<T>,
                })
                .build()
        }

        const fn build_type_name<'a, T: Facet<'a>>() -> TypeNameFn {
            fn type_name_impl<'a, T: Facet<'a>>(
                _shape: &'static Shape,
                f: &mut core::fmt::Formatter<'_>,
                opts: TypeNameOpts,
            ) -> core::fmt::Result {
                write!(f, "HashSet")?;
                if let Some(opts) = opts.for_children() {
                    write!(f, "<")?;
                    T::SHAPE.write_type_name(f, opts)?;
                    write!(f, ">")?;
                } else {
                    write!(f, "<…>")?;
                }
                Ok(())
            }
            type_name_impl::<T>
        }

        ShapeBuilder::for_sized::<Self>("HashSet")
            .module_path("std::collections::hash_set")
            .type_name(build_type_name::<T>())
            .ty(Type::User(UserType::Opaque))
            .def(Def::Set(SetDef::new(
                &const { build_set_vtable::<T, S>() },
                T::SHAPE,
            )))
            .type_params(&[
                TypeParam {
                    name: "T",
                    shape: T::SHAPE,
                },
                TypeParam {
                    name: "S",
                    shape: S::SHAPE,
                },
            ])
            .inner(T::SHAPE)
            // HashSet<T> propagates T's variance
            .variance(VarianceDesc {
                base: Variance::Bivariant,
                deps: &const { [VarianceDep::covariant(T::SHAPE)] },
            })
            .vtable_indirect(
                &const {
                    VTableIndirect {
                        debug: Some(hashset_debug),
                        hash: Some(hashset_hash),
                        partial_eq: Some(hashset_partial_eq),
                        ..VTableIndirect::EMPTY
                    }
                },
            )
            .type_ops_indirect(
                &const {
                    TypeOpsIndirect {
                        drop_in_place: hashset_drop::<T, S>,
                        default_in_place: Some(hashset_default::<T, S>),
                        clone_into: None,
                        is_truthy: None,
                    }
                },
            )
            .build()
    };
}

#[cfg(test)]
mod tests {
    use alloc::string::String;
    use core::ptr::NonNull;
    use std::collections::HashSet;
    use std::hash::RandomState;

    use super::*;

    #[test]
    fn test_hashset_type_params() {
        // HashSet should have a type param for both its value type
        // and its hasher state
        let [type_param_1, type_param_2] = <HashSet<i32>>::SHAPE.type_params else {
            panic!("HashSet<T> should have 2 type params")
        };
        assert_eq!(type_param_1.shape(), i32::SHAPE);
        assert_eq!(type_param_2.shape(), RandomState::SHAPE);
    }

    #[test]
    fn test_hashset_vtable_1_new_insert_iter_drop() {
        facet_testhelpers::setup();

        let hashset_shape = <HashSet<String>>::SHAPE;
        let hashset_def = hashset_shape
            .def
            .into_set()
            .expect("HashSet<T> should have a set definition");

        // Allocate memory for the HashSet
        let hashset_uninit_ptr = hashset_shape.allocate().unwrap();

        // Create the HashSet with a capacity of 3
        let hashset_ptr =
            unsafe { (hashset_def.vtable.init_in_place_with_capacity)(hashset_uninit_ptr, 3) };

        // The HashSet is empty, so ensure its length is 0
        let hashset_actual_length = unsafe { (hashset_def.vtable.len)(hashset_ptr.as_const()) };
        assert_eq!(hashset_actual_length, 0);

        // 5 sample values to insert
        let strings = ["foo", "bar", "bazz", "fizzbuzz", "fifth thing"];

        // Insert the 5 values into the HashSet
        let mut hashset_length = 0;
        for string in strings {
            // Create the value
            let mut new_value = core::mem::ManuallyDrop::new(string.to_string());

            // Insert the value
            let did_insert = unsafe {
                (hashset_def.vtable.insert)(
                    hashset_ptr,
                    PtrMut::new(NonNull::from(&mut new_value).as_ptr()),
                )
            };

            assert!(did_insert, "expected value to be inserted in the HashSet");

            // Ensure the HashSet's length increased by 1
            hashset_length += 1;
            let hashset_actual_length = unsafe { (hashset_def.vtable.len)(hashset_ptr.as_const()) };
            assert_eq!(hashset_actual_length, hashset_length);
        }

        // Insert the same 5 values again, ensuring they are deduplicated
        for string in strings {
            // Create the value
            let mut new_value = core::mem::ManuallyDrop::new(string.to_string());

            // Try to insert the value
            let did_insert = unsafe {
                (hashset_def.vtable.insert)(
                    hashset_ptr,
                    PtrMut::new(NonNull::from(&mut new_value).as_ptr()),
                )
            };

            assert!(
                !did_insert,
                "expected value to not be inserted in the HashSet"
            );

            // Ensure the HashSet's length did not increase
            let hashset_actual_length = unsafe { (hashset_def.vtable.len)(hashset_ptr.as_const()) };
            assert_eq!(hashset_actual_length, hashset_length);
        }

        // Create a new iterator over the HashSet
        let iter_init_with_value_fn = hashset_def.vtable.iter_vtable.init_with_value.unwrap();
        let hashset_iter_ptr = unsafe { iter_init_with_value_fn(hashset_ptr.as_const()) };

        // Collect all the items from the HashSet's iterator
        let mut iter_items = HashSet::<&str>::new();
        loop {
            // Get the next item from the iterator
            let item_ptr = unsafe { (hashset_def.vtable.iter_vtable.next)(hashset_iter_ptr) };
            let Some(item_ptr) = item_ptr else {
                break;
            };

            let item = unsafe { item_ptr.get::<String>() };

            // Insert the item into the set of items returned from the iterator
            let did_insert = iter_items.insert(&**item);

            assert!(did_insert, "HashSet iterator returned duplicate item");
        }

        // Deallocate the iterator
        unsafe {
            (hashset_def.vtable.iter_vtable.dealloc)(hashset_iter_ptr);
        }

        // Ensure the iterator returned all of the strings
        assert_eq!(iter_items, strings.iter().copied().collect::<HashSet<_>>());

        // Drop the HashSet in place
        unsafe {
            hashset_shape
                .call_drop_in_place(hashset_ptr)
                .expect("HashSet<T> should have drop_in_place");

            // Deallocate the memory
            hashset_shape.deallocate_mut(hashset_ptr).unwrap();
        }
    }
}
