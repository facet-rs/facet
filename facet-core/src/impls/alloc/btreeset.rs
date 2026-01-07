use alloc::boxed::Box;
use alloc::collections::BTreeSet;

use crate::{PtrConst, PtrMut, PtrUninit};

use crate::{
    Def, Facet, IterVTable, OxPtrMut, SetDef, SetVTable, Shape, ShapeBuilder, TypeNameFn,
    TypeNameOpts, TypeOpsIndirect, TypeParam, VTableIndirect,
};

type BTreeSetIterator<'mem, T> = alloc::collections::btree_set::Iter<'mem, T>;

unsafe fn btreeset_init_in_place_with_capacity<T>(uninit: PtrUninit, _capacity: usize) -> PtrMut {
    unsafe { uninit.put(BTreeSet::<T>::new()) }
}

unsafe fn btreeset_insert<T: Eq + Ord + 'static>(ptr: PtrMut, item: PtrMut) -> bool {
    unsafe {
        let set = ptr.as_mut::<BTreeSet<T>>();
        let item = item.read::<T>();
        set.insert(item)
    }
}

unsafe fn btreeset_len<T: 'static>(ptr: PtrConst) -> usize {
    unsafe { ptr.get::<BTreeSet<T>>().len() }
}

unsafe fn btreeset_contains<T: Eq + Ord + 'static>(ptr: PtrConst, item: PtrConst) -> bool {
    unsafe { ptr.get::<BTreeSet<T>>().contains(item.get()) }
}

unsafe fn btreeset_iter_init<T: 'static>(ptr: PtrConst) -> PtrMut {
    unsafe {
        let set = ptr.get::<BTreeSet<T>>();
        let iter: BTreeSetIterator<'_, T> = set.iter();
        let iter_state = Box::new(iter);
        PtrMut::new(Box::into_raw(iter_state) as *mut u8)
    }
}

unsafe fn btreeset_iter_next<T: 'static>(iter_ptr: PtrMut) -> Option<PtrConst> {
    unsafe {
        let state = iter_ptr.as_mut::<BTreeSetIterator<'static, T>>();
        state.next().map(|value| PtrConst::new(value as *const T))
    }
}

unsafe fn btreeset_iter_next_back<T: 'static>(iter_ptr: PtrMut) -> Option<PtrConst> {
    unsafe {
        let state = iter_ptr.as_mut::<BTreeSetIterator<'static, T>>();
        state
            .next_back()
            .map(|value| PtrConst::new(value as *const T))
    }
}

unsafe fn btreeset_iter_dealloc<T>(iter_ptr: PtrMut) {
    unsafe {
        drop(Box::from_raw(
            iter_ptr.as_ptr::<BTreeSetIterator<'_, T>>() as *mut BTreeSetIterator<'_, T>
        ));
    }
}

/// Drop implementation for `BTreeSet<T>`
unsafe fn btreeset_drop<T>(ox: OxPtrMut) {
    unsafe {
        core::ptr::drop_in_place(ox.ptr().as_ptr::<BTreeSet<T>>() as *mut BTreeSet<T>);
    }
}

/// Default implementation for `BTreeSet<T>`
unsafe fn btreeset_default<T>(ox: OxPtrMut) {
    unsafe { ox.ptr().as_uninit().put(BTreeSet::<T>::new()) };
}

unsafe impl<'a, T> Facet<'a> for BTreeSet<T>
where
    T: Facet<'a> + core::cmp::Eq + core::cmp::Ord + 'static,
{
    const SHAPE: &'static crate::Shape = &const {
        const fn build_set_vtable<T: Eq + Ord + 'static>() -> SetVTable {
            SetVTable::builder()
                .init_in_place_with_capacity(btreeset_init_in_place_with_capacity::<T>)
                .insert(btreeset_insert::<T>)
                .len(btreeset_len::<T>)
                .contains(btreeset_contains::<T>)
                .iter_vtable(IterVTable {
                    init_with_value: Some(btreeset_iter_init::<T>),
                    next: btreeset_iter_next::<T>,
                    next_back: Some(btreeset_iter_next_back::<T>),
                    size_hint: None,
                    dealloc: btreeset_iter_dealloc::<T>,
                })
                .build()
        }

        const fn build_type_name<'a, T: Facet<'a>>() -> TypeNameFn {
            fn type_name_impl<'a, T: Facet<'a>>(
                _shape: &'static Shape,
                f: &mut core::fmt::Formatter<'_>,
                opts: TypeNameOpts,
            ) -> core::fmt::Result {
                write!(f, "BTreeSet")?;
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

        ShapeBuilder::for_sized::<Self>("BTreeSet")
            .type_name(build_type_name::<T>())
            .vtable_indirect(&VTableIndirect::EMPTY)
            .type_ops_indirect(
                &const {
                    TypeOpsIndirect {
                        drop_in_place: btreeset_drop::<T>,
                        default_in_place: Some(btreeset_default::<T>),
                        clone_into: None,
                        is_truthy: None,
                    }
                },
            )
            .def(Def::Set(SetDef::new(
                &const { build_set_vtable::<T>() },
                T::SHAPE,
            )))
            .type_params(&[TypeParam {
                name: "T",
                shape: T::SHAPE,
            }])
            .inner(T::SHAPE)
            // BTreeSet<T> propagates T's variance
            .variance(Shape::computed_variance)
            .build()
    };
}

#[cfg(test)]
mod tests {
    use core::ptr::NonNull;

    use alloc::collections::BTreeSet;
    use alloc::string::String;
    use alloc::vec::Vec;

    use super::*;

    #[test]
    fn test_btreesetset_type_params() {
        let [type_param_1] = <BTreeSet<i32>>::SHAPE.type_params else {
            panic!("BTreeSet<T> should have 1 type param")
        };
        assert_eq!(type_param_1.shape(), i32::SHAPE);
    }

    #[test]
    fn test_btreeset_vtable_1_new_insert_iter_drop() {
        facet_testhelpers::setup();

        let btreeset_shape = <BTreeSet<String>>::SHAPE;
        let btreeset_def = btreeset_shape
            .def
            .into_set()
            .expect("BTreeSet<T> should have a set definition");

        // Allocate memory for the BTreeSet
        let btreeset_uninit_ptr = btreeset_shape.allocate().unwrap();

        // Create the BTreeSet
        let btreeset_ptr =
            unsafe { (btreeset_def.vtable.init_in_place_with_capacity)(btreeset_uninit_ptr, 0) };

        // The BTreeSet is empty, so ensure its length is 0
        let btreeset_actual_length = unsafe { (btreeset_def.vtable.len)(btreeset_ptr.as_const()) };
        assert_eq!(btreeset_actual_length, 0);

        // 5 sample values to insert
        let strings = ["foo", "bar", "bazz", "fizzbuzz", "fifth thing"];

        // Insert the 5 values into the BTreeSet
        let mut btreeset_length = 0;
        for string in strings {
            // Create the value
            let mut new_value = string.to_string();

            // Insert the value
            let did_insert = unsafe {
                (btreeset_def.vtable.insert)(
                    btreeset_ptr,
                    PtrMut::new(NonNull::from(&mut new_value).as_ptr()),
                )
            };

            // The value now belongs to the BTreeSet, so forget it
            core::mem::forget(new_value);

            assert!(did_insert, "expected value to be inserted in the BTreeSet");

            // Ensure the BTreeSet's length increased by 1
            btreeset_length += 1;
            let btreeset_actual_length =
                unsafe { (btreeset_def.vtable.len)(btreeset_ptr.as_const()) };
            assert_eq!(btreeset_actual_length, btreeset_length);
        }

        // Insert the same 5 values again, ensuring they are deduplicated
        for string in strings {
            // Create the value
            let mut new_value = string.to_string();

            // Try to insert the value
            let did_insert = unsafe {
                (btreeset_def.vtable.insert)(
                    btreeset_ptr,
                    PtrMut::new(NonNull::from(&mut new_value).as_ptr()),
                )
            };

            // The value now belongs to the BTreeSet, so forget it
            core::mem::forget(new_value);

            assert!(
                !did_insert,
                "expected value to not be inserted in the BTreeSet"
            );

            // Ensure the BTreeSet's length did not increase
            let btreeset_actual_length =
                unsafe { (btreeset_def.vtable.len)(btreeset_ptr.as_const()) };
            assert_eq!(btreeset_actual_length, btreeset_length);
        }

        // Create a new iterator over the BTreeSet
        let iter_init_with_value_fn = btreeset_def.vtable.iter_vtable.init_with_value.unwrap();
        let btreeset_iter_ptr = unsafe { iter_init_with_value_fn(btreeset_ptr.as_const()) };

        // Collect all the items from the BTreeSet's iterator
        let mut iter_items = Vec::<&str>::new();
        loop {
            // Get the next item from the iterator
            let item_ptr = unsafe { (btreeset_def.vtable.iter_vtable.next)(btreeset_iter_ptr) };
            let Some(item_ptr) = item_ptr else {
                break;
            };

            let item = unsafe { item_ptr.get::<String>() };

            // Add the item into the list of items returned from the iterator
            iter_items.push(&**item);
        }

        // Deallocate the iterator
        unsafe {
            (btreeset_def.vtable.iter_vtable.dealloc)(btreeset_iter_ptr);
        }

        // BTrees iterate in sorted order, so ensure the iterator returned
        // each item in order
        let mut strings_sorted = strings.to_vec();
        strings_sorted.sort();
        assert_eq!(iter_items, strings_sorted);

        // Drop the BTreeSet in place
        unsafe {
            btreeset_shape
                .call_drop_in_place(btreeset_ptr)
                .expect("BTreeSet<T> should have drop_in_place");
        }

        // Deallocate the memory
        unsafe { btreeset_shape.deallocate_mut(btreeset_ptr).unwrap() };
    }
}
