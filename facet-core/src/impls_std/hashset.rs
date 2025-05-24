use core::hash::{BuildHasher, Hash};
use std::collections::HashSet;

use crate::ptr::{PtrConst, PtrMut};

use crate::{
    Def, Facet, IterVTable, MarkerTraits, SetDef, SetVTable, Shape, Type, TypeParam, UserType,
    VTableView, ValueVTable,
};

type HashSetIterator<'mem, T> = std::collections::hash_set::Iter<'mem, T>;

unsafe impl<'a, T, S> Facet<'a> for HashSet<T, S>
where
    T: Facet<'a> + core::cmp::Eq + core::hash::Hash,
    S: Facet<'a> + Default + BuildHasher,
{
    const VTABLE: &'static ValueVTable = &const {
        ValueVTable::builder::<Self>()
            .marker_traits(|| {
                MarkerTraits::SEND
                    .union(MarkerTraits::SYNC)
                    .union(MarkerTraits::EQ)
                    .union(MarkerTraits::UNPIN)
                    .intersection(T::SHAPE.vtable.marker_traits())
            })
            .type_name(|f, opts| {
                if let Some(opts) = opts.for_children() {
                    write!(f, "{}<", Self::SHAPE.type_identifier)?;
                    (T::SHAPE.vtable.type_name)(f, opts)?;
                    write!(f, ">")
                } else {
                    write!(f, "HashSet<⋯>")
                }
            })
            .default_in_place(|| Some(|target| unsafe { target.put(Self::default()) }))
            .eq(|| Some(|a, b| a == b))
            .debug(|| {
                if (T::SHAPE.vtable.debug)().is_some() {
                    Some(|value, f| {
                        let t_debug = <VTableView<T>>::of().debug().unwrap();
                        write!(f, "{{")?;
                        for (i, item) in value.iter().enumerate() {
                            if i > 0 {
                                write!(f, ", ")?;
                            }
                            (t_debug)(item, f)?;
                        }
                        write!(f, "}}")
                    })
                } else {
                    None
                }
            })
            .clone_into(|| {
                if (T::SHAPE.vtable.clone_into)().is_some() {
                    Some(|src, dst| unsafe {
                        let set = src;
                        let mut new_set =
                            HashSet::with_capacity_and_hasher(set.len(), S::default());

                        let t_clone_into = <VTableView<T>>::of().clone_into().unwrap();

                        for item in set {
                            use crate::TypedPtrUninit;
                            use core::mem::MaybeUninit;

                            let mut new_item = MaybeUninit::<T>::uninit();
                            let uninit_item = TypedPtrUninit::new(new_item.as_mut_ptr());

                            (t_clone_into)(item, uninit_item);

                            new_set.insert(new_item.assume_init());
                        }

                        dst.put(new_set)
                    })
                } else {
                    None
                }
            })
            .hash(|| {
                if (T::SHAPE.vtable.hash)().is_some() {
                    Some(|set, hasher_this, hasher_write_fn| unsafe {
                        use crate::HasherProxy;
                        let t_hash = <VTableView<T>>::of().hash().unwrap();
                        let mut hasher = HasherProxy::new(hasher_this, hasher_write_fn);
                        set.len().hash(&mut hasher);
                        for item in set {
                            (t_hash)(item, hasher_this, hasher_write_fn);
                        }
                    })
                } else {
                    None
                }
            })
            .build()
    };

    const SHAPE: &'static Shape<'static> = &const {
        Shape::builder_for_sized::<Self>()
            .type_identifier("HashSet")
            .type_params(&[
                TypeParam {
                    name: "T",
                    shape: || T::SHAPE,
                },
                TypeParam {
                    name: "S",
                    shape: || S::SHAPE,
                },
            ])
            .ty(Type::User(UserType::Opaque))
            .def(Def::Set(
                SetDef::builder()
                    .t(|| T::SHAPE)
                    .vtable(
                        &const {
                            SetVTable::builder()
                                .init_in_place_with_capacity(|uninit, capacity| unsafe {
                                    uninit
                                        .put(Self::with_capacity_and_hasher(capacity, S::default()))
                                })
                                .insert(|ptr, item| unsafe {
                                    let set = ptr.as_mut::<HashSet<T>>();
                                    let item = item.read::<T>();
                                    set.insert(item)
                                })
                                .len(|ptr| unsafe {
                                    let set = ptr.get::<HashSet<T>>();
                                    set.len()
                                })
                                .contains(|ptr, item| unsafe {
                                    let set = ptr.get::<HashSet<T>>();
                                    set.contains(item.get())
                                })
                                .iter_vtable(
                                    IterVTable::builder()
                                        .init_with_value(|ptr| unsafe {
                                            let set = ptr.get::<HashSet<T>>();
                                            let iter: HashSetIterator<'_, T> = set.iter();
                                            let iter_state = Box::new(iter);
                                            PtrMut::new(Box::into_raw(iter_state) as *mut u8)
                                        })
                                        .next(|iter_ptr| unsafe {
                                            let state = iter_ptr.as_mut::<HashSetIterator<'_, T>>();
                                            state.next().map(|value| PtrConst::new(value))
                                        })
                                        .dealloc(|iter_ptr| unsafe {
                                            drop(Box::from_raw(
                                                iter_ptr.as_ptr::<HashSetIterator<'_, T>>()
                                                    as *mut HashSetIterator<'_, T>,
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
    use alloc::string::String;
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
    fn test_hashset_vtable_1_new_insert_iter_drop() -> eyre::Result<()> {
        facet_testhelpers::setup();

        let hashset_shape = <HashSet<String>>::SHAPE;
        let hashset_def = hashset_shape
            .def
            .into_set()
            .expect("HashSet<T> should have a set definition");

        // Allocate memory for the HashSet
        let hashset_uninit_ptr = hashset_shape.allocate()?;

        // Create the HashSet with a capacity of 3
        let hashset_ptr =
            unsafe { (hashset_def.vtable.init_in_place_with_capacity_fn)(hashset_uninit_ptr, 3) };

        // The HashSet is empty, so ensure its length is 0
        let hashset_actual_length = unsafe { (hashset_def.vtable.len_fn)(hashset_ptr.as_const()) };
        assert_eq!(hashset_actual_length, 0);

        // 5 sample values to insert
        let strings = ["foo", "bar", "bazz", "fizzbuzz", "fifth thing"];

        // Insert the 5 values into the HashSet
        let mut hashset_length = 0;
        for string in strings {
            // Create the value
            let mut new_value = string.to_string();

            // Insert the value
            let did_insert = unsafe {
                (hashset_def.vtable.insert_fn)(hashset_ptr, PtrMut::new(&raw mut new_value))
            };

            // The value now belongs to the HashSet, so forget it
            core::mem::forget(new_value);

            assert!(did_insert, "expected value to be inserted in the HashSet");

            // Ensure the HashSet's length increased by 1
            hashset_length += 1;
            let hashset_actual_length =
                unsafe { (hashset_def.vtable.len_fn)(hashset_ptr.as_const()) };
            assert_eq!(hashset_actual_length, hashset_length);
        }

        // Insert the same 5 values again, ensuring they are deduplicated
        for string in strings {
            // Create the value
            let mut new_value = string.to_string();

            // Try to insert the value
            let did_insert = unsafe {
                (hashset_def.vtable.insert_fn)(hashset_ptr, PtrMut::new(&raw mut new_value))
            };

            // The value now belongs to the HashSet, so forget it
            core::mem::forget(new_value);

            assert!(
                !did_insert,
                "expected value to not be inserted in the HashSet"
            );

            // Ensure the HashSet's length did not increase
            let hashset_actual_length =
                unsafe { (hashset_def.vtable.len_fn)(hashset_ptr.as_const()) };
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

        // Get the function pointer for dropping the HashSet
        let drop_fn =
            (hashset_shape.vtable.drop_in_place)().expect("HashSet<T> should have drop_in_place");

        // Drop the HashSet in place
        unsafe { drop_fn(hashset_ptr) };

        // Deallocate the memory
        unsafe { hashset_shape.deallocate_mut(hashset_ptr)? };

        Ok(())
    }
}
