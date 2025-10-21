use core::ptr::NonNull;

use alloc::boxed::Box;
use alloc::collections::BTreeSet;

use crate::ptr::{PtrConst, PtrMut};

use crate::{
    Def, Facet, IterVTable, MarkerTraits, SetDef, SetVTable, Shape, Type, TypeParam, UserType,
    ValueVTable,
};

type BTreeSetIterator<'mem, T> = alloc::collections::btree_set::Iter<'mem, T>;

unsafe impl<'a, T> Facet<'a> for BTreeSet<T>
where
    T: Facet<'a> + core::cmp::Eq + core::cmp::Ord,
{
    const SHAPE: &'static Shape = &const {
        Shape::builder_for_sized::<Self>()
            .vtable({
                ValueVTable::builder::<Self>()
                    .marker_traits({
                        MarkerTraits::SEND
                            .union(MarkerTraits::SYNC)
                            .union(MarkerTraits::EQ)
                            .union(MarkerTraits::UNPIN)
                            .intersection(T::SHAPE.vtable.marker_traits())
                    })
                    .type_name(|f, opts| {
                        if let Some(opts) = opts.for_children() {
                            write!(f, "{}<", Self::SHAPE.type_identifier)?;
                            T::SHAPE.vtable.type_name()(f, opts)?;
                            write!(f, ">")
                        } else {
                            write!(f, "{}<⋯>", Self::SHAPE.type_identifier)
                        }
                    })
                    .default_in_place({
                        Some(|target| unsafe { target.put(Self::default()).into() })
                    })
                    .build()
            })
            .type_identifier("BTreeSet")
            .type_params(&[TypeParam {
                name: "T",
                shape: T::SHAPE,
            }])
            .ty(Type::User(UserType::Opaque))
            .def(Def::Set(
                SetDef::builder()
                    .t(T::SHAPE)
                    .vtable(
                        &const {
                            SetVTable::builder()
                                .init_in_place_with_capacity(|uninit, _capacity| unsafe {
                                    uninit.put(Self::new())
                                })
                                .insert(|ptr, item| unsafe {
                                    let set = ptr.as_mut::<BTreeSet<T>>();
                                    let item = item.read::<T>();
                                    set.insert(item)
                                })
                                .len(|ptr| unsafe {
                                    let set = ptr.get::<BTreeSet<T>>();
                                    set.len()
                                })
                                .contains(|ptr, item| unsafe {
                                    let set = ptr.get::<BTreeSet<T>>();
                                    set.contains(item.get())
                                })
                                .iter_vtable(
                                    IterVTable::builder()
                                        .init_with_value(|ptr| {
                                            let set = unsafe { ptr.get::<BTreeSet<T>>() };
                                            let iter: BTreeSetIterator<'_, T> = set.iter();
                                            let iter_state = Box::new(iter);
                                            PtrMut::new(unsafe {
                                                NonNull::new_unchecked(
                                                    Box::into_raw(iter_state) as *mut u8
                                                )
                                            })
                                        })
                                        .next(|iter_ptr| {
                                            let state = unsafe {
                                                iter_ptr.as_mut::<BTreeSetIterator<'_, T>>()
                                            };
                                            state
                                                .next()
                                                .map(|value| PtrConst::new(NonNull::from(value)))
                                        })
                                        .next_back(|iter_ptr| {
                                            let state = unsafe {
                                                iter_ptr.as_mut::<BTreeSetIterator<'_, T>>()
                                            };
                                            state
                                                .next_back()
                                                .map(|value| PtrConst::new(NonNull::from(value)))
                                        })
                                        .dealloc(|iter_ptr| unsafe {
                                            drop(Box::from_raw(
                                                iter_ptr.as_ptr::<BTreeSetIterator<'_, T>>()
                                                    as *mut BTreeSetIterator<'_, T>,
                                            ));
                                        })
                                        .build(),
                                )
                                .build()
                        },
                    )
                    .build(),
            ))
            .build()
    };
}

#[cfg(test)]
mod tests {
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
            unsafe { (btreeset_def.vtable.init_in_place_with_capacity_fn)(btreeset_uninit_ptr, 0) };

        // The BTreeSet is empty, so ensure its length is 0
        let btreeset_actual_length =
            unsafe { (btreeset_def.vtable.len_fn)(btreeset_ptr.as_const()) };
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
                (btreeset_def.vtable.insert_fn)(
                    btreeset_ptr,
                    PtrMut::new(NonNull::from(&mut new_value)),
                )
            };

            // The value now belongs to the BTreeSet, so forget it
            core::mem::forget(new_value);

            assert!(did_insert, "expected value to be inserted in the BTreeSet");

            // Ensure the BTreeSet's length increased by 1
            btreeset_length += 1;
            let btreeset_actual_length =
                unsafe { (btreeset_def.vtable.len_fn)(btreeset_ptr.as_const()) };
            assert_eq!(btreeset_actual_length, btreeset_length);
        }

        // Insert the same 5 values again, ensuring they are deduplicated
        for string in strings {
            // Create the value
            let mut new_value = string.to_string();

            // Try to insert the value
            let did_insert = unsafe {
                (btreeset_def.vtable.insert_fn)(
                    btreeset_ptr,
                    PtrMut::new(NonNull::from(&mut new_value)),
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
                unsafe { (btreeset_def.vtable.len_fn)(btreeset_ptr.as_const()) };
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

        // Get the function pointer for dropping the BTreeSet
        let drop_fn = btreeset_shape
            .vtable
            .drop_in_place
            .expect("BTreeSet<T> should have drop_in_place");

        // Drop the BTreeSet in place
        unsafe { drop_fn(btreeset_ptr) };

        // Deallocate the memory
        unsafe { btreeset_shape.deallocate_mut(btreeset_ptr).unwrap() };
    }
}
