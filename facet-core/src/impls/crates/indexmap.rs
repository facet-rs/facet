#![cfg(feature = "indexmap")]

use alloc::{boxed::Box, string::String};
use core::hash::BuildHasher;
use core::ptr::NonNull;
use indexmap::{IndexMap, IndexSet};

use crate::{PtrConst, PtrMut, PtrUninit};

use crate::{
    Def, Facet, IterVTable, MapDef, MapVTable, OxPtrMut, OxPtrUninit, SetDef, SetVTable, Shape,
    ShapeBuilder, Type, TypeNameFn, TypeNameOpts, TypeOpsIndirect, TypeParam, UserType,
};

type IndexMapIterator<'mem, K, V> = indexmap::map::Iter<'mem, K, V>;
type IndexSetIterator<'mem, T> = indexmap::set::Iter<'mem, T>;

unsafe extern "C" fn indexmap_init_in_place_with_capacity<K, V, S: Default + BuildHasher>(
    uninit: PtrUninit,
    capacity: usize,
) -> PtrMut {
    unsafe {
        uninit.put(IndexMap::<K, V, S>::with_capacity_and_hasher(
            capacity,
            S::default(),
        ))
    }
}

unsafe extern "C" fn indexmap_insert<K: Eq + core::hash::Hash, V, S: BuildHasher>(
    ptr: PtrMut,
    key: PtrMut,
    value: PtrMut,
) {
    let map = unsafe { ptr.as_mut::<IndexMap<K, V, S>>() };
    let key = unsafe { key.read::<K>() };
    let value = unsafe { value.read::<V>() };
    map.insert(key, value);
}

unsafe extern "C" fn indexmap_insert_owned_string_key<'a, K, V, S: BuildHasher>(
    ptr: PtrMut,
    key: PtrMut,
    value: PtrMut,
) -> bool
where
    K: Facet<'a>,
{
    if K::SHAPE.id != <String as Facet>::SHAPE.id {
        return false;
    }

    let map = unsafe { ptr.as_mut::<IndexMap<String, V, S>>() };
    let key = unsafe { key.read::<String>() };
    let value = unsafe { value.read::<V>() };
    map.insert(key, value);
    true
}

unsafe extern "C" fn indexmap_insert_borrowed_str_key<'a, K, V, S: BuildHasher>(
    ptr: PtrMut,
    key: PtrConst,
    value: PtrMut,
) -> bool
where
    K: Facet<'a>,
{
    if K::SHAPE.id != <String as Facet>::SHAPE.id {
        return false;
    }

    let map = unsafe { ptr.as_mut::<IndexMap<String, V, S>>() };
    let key = unsafe { String::from(key.get::<str>()) };
    let value = unsafe { value.read::<V>() };
    map.insert(key, value);
    true
}

unsafe extern "C" fn indexmap_insert_borrowed_str_entry<'a, K, V, S: BuildHasher>(
    ptr: PtrMut,
    key: PtrConst,
    value: PtrConst,
) -> bool
where
    K: Facet<'a>,
    V: Facet<'a>,
{
    if K::SHAPE.id != <String as Facet>::SHAPE.id || V::SHAPE.id != <String as Facet>::SHAPE.id {
        return false;
    }

    let map = unsafe { ptr.as_mut::<IndexMap<String, String, S>>() };
    let key = unsafe { String::from(key.get::<str>()) };
    let value = unsafe { String::from(value.get::<str>()) };
    map.insert(key, value);
    true
}

unsafe extern "C" fn indexmap_len<K, V, S>(ptr: PtrConst) -> usize {
    unsafe { ptr.get::<IndexMap<K, V, S>>().len() }
}

unsafe extern "C" fn indexmap_contains_key<K: Eq + core::hash::Hash, V, S: BuildHasher>(
    ptr: PtrConst,
    key: PtrConst,
) -> bool {
    unsafe { ptr.get::<IndexMap<K, V, S>>().contains_key(key.get::<K>()) }
}

unsafe extern "C" fn indexmap_get_value_ptr<K: Eq + core::hash::Hash, V, S: BuildHasher>(
    ptr: PtrConst,
    key: PtrConst,
) -> *const u8 {
    unsafe {
        ptr.get::<IndexMap<K, V, S>>()
            .get(key.get::<K>())
            .map_or(core::ptr::null(), |v| {
                NonNull::from(v).as_ptr() as *const u8
            })
    }
}

unsafe extern "C" fn indexmap_iter_init<K, V, S>(ptr: PtrConst) -> PtrMut {
    unsafe {
        let map = ptr.get::<IndexMap<K, V, S>>();
        let iter: IndexMapIterator<'_, K, V> = map.iter();
        let iter_state = Box::new(iter);
        PtrMut::new(Box::into_raw(iter_state) as *mut u8)
    }
}

unsafe fn indexmap_iter_next<K, V>(iter_ptr: PtrMut) -> Option<(PtrConst, PtrConst)> {
    unsafe {
        let state = iter_ptr.as_mut::<IndexMapIterator<'_, K, V>>();
        state.next().map(|(key, value)| {
            (
                PtrConst::new(NonNull::from(key).as_ptr()),
                PtrConst::new(NonNull::from(value).as_ptr()),
            )
        })
    }
}

unsafe extern "C" fn indexmap_iter_dealloc<K, V>(iter_ptr: PtrMut) {
    unsafe {
        drop(Box::from_raw(
            iter_ptr.as_ptr::<IndexMapIterator<'_, K, V>>() as *mut IndexMapIterator<'_, K, V>,
        ));
    }
}

unsafe fn indexmap_drop<K, V, S>(target: OxPtrMut) {
    unsafe {
        core::ptr::drop_in_place(
            target.ptr().as_ptr::<IndexMap<K, V, S>>() as *mut IndexMap<K, V, S>
        );
    }
}

unsafe fn indexmap_default<K, V, S: Default + BuildHasher>(ox: OxPtrUninit) -> bool {
    unsafe { ox.put(IndexMap::<K, V, S>::default()) };
    true
}

/// Build an IndexMap from a contiguous slice of (K, V) pairs.
unsafe extern "C" fn indexmap_from_pair_slice<
    K: Eq + core::hash::Hash,
    V,
    S: Default + BuildHasher,
>(
    uninit: PtrUninit,
    pairs_ptr: *mut u8,
    count: usize,
) -> PtrMut {
    let pairs = pairs_ptr as *mut (K, V);
    let mut map = IndexMap::<K, V, S>::with_capacity_and_hasher(count, S::default());
    for index in 0..count {
        let (key, value) = unsafe { core::ptr::read(pairs.add(index)) };
        map.insert(key, value);
    }
    unsafe { uninit.put(map) }
}

unsafe extern "C" fn indexset_init_in_place_with_capacity<T, S: Default + BuildHasher>(
    uninit: PtrUninit,
    capacity: usize,
) -> PtrMut {
    unsafe {
        uninit.put(IndexSet::<T, S>::with_capacity_and_hasher(
            capacity,
            S::default(),
        ))
    }
}

unsafe extern "C" fn indexset_insert<T: Eq + core::hash::Hash, S: BuildHasher>(
    ptr: PtrMut,
    item: PtrMut,
) -> bool {
    let set = unsafe { ptr.as_mut::<IndexSet<T, S>>() };
    let item = unsafe { item.read::<T>() };
    set.insert(item)
}

unsafe extern "C" fn indexset_len<T, S>(ptr: PtrConst) -> usize {
    unsafe { ptr.get::<IndexSet<T, S>>().len() }
}

unsafe extern "C" fn indexset_contains<T: Eq + core::hash::Hash, S: BuildHasher>(
    ptr: PtrConst,
    item: PtrConst,
) -> bool {
    unsafe { ptr.get::<IndexSet<T, S>>().contains(item.get::<T>()) }
}

unsafe extern "C" fn indexset_iter_init<T, S>(ptr: PtrConst) -> PtrMut {
    unsafe {
        let set = ptr.get::<IndexSet<T, S>>();
        let iter: IndexSetIterator<'_, T> = set.iter();
        let iter_state = Box::new(iter);
        PtrMut::new(Box::into_raw(iter_state) as *mut u8)
    }
}

unsafe fn indexset_iter_next<T>(iter_ptr: PtrMut) -> Option<PtrConst> {
    unsafe {
        let state = iter_ptr.as_mut::<IndexSetIterator<'_, T>>();
        state.next().map(|value| PtrConst::new(value as *const T))
    }
}

unsafe fn indexset_iter_next_back<T>(iter_ptr: PtrMut) -> Option<PtrConst> {
    unsafe {
        let state = iter_ptr.as_mut::<IndexSetIterator<'_, T>>();
        state
            .next_back()
            .map(|value| PtrConst::new(value as *const T))
    }
}

unsafe extern "C" fn indexset_iter_dealloc<T>(iter_ptr: PtrMut) {
    unsafe {
        drop(Box::from_raw(
            iter_ptr.as_ptr::<IndexSetIterator<'_, T>>() as *mut IndexSetIterator<'_, T>
        ));
    }
}

/// Build an IndexSet from a contiguous slice of elements.
unsafe extern "C" fn indexset_from_slice<T: Eq + core::hash::Hash, S: Default + BuildHasher>(
    uninit: PtrUninit,
    elements_ptr: *mut u8,
    count: usize,
) -> PtrMut {
    let elements = elements_ptr as *mut T;
    let mut set = IndexSet::<T, S>::with_capacity_and_hasher(count, S::default());
    for index in 0..count {
        let element = unsafe { core::ptr::read(elements.add(index)) };
        set.insert(element);
    }
    unsafe { uninit.put(set) }
}

unsafe fn indexset_drop<T, S>(target: OxPtrMut) {
    unsafe {
        core::ptr::drop_in_place(target.ptr().as_ptr::<IndexSet<T, S>>() as *mut IndexSet<T, S>);
    }
}

unsafe fn indexset_default<T, S: Default + BuildHasher>(ox: OxPtrUninit) -> bool {
    unsafe { ox.put(IndexSet::<T, S>::default()) };
    true
}

unsafe impl<'a, K, V, S> Facet<'a> for IndexMap<K, V, S>
where
    K: Facet<'a> + core::cmp::Eq + core::hash::Hash,
    V: Facet<'a>,
    S: 'a + Default + BuildHasher,
{
    const SHAPE: &'static Shape = &const {
        const fn build_map_vtable<
            'a,
            K: Facet<'a> + Eq + core::hash::Hash,
            V: Facet<'a>,
            S: Default + BuildHasher,
        >() -> MapVTable {
            MapVTable::builder()
                .init_in_place_with_capacity(indexmap_init_in_place_with_capacity::<K, V, S>)
                .insert(indexmap_insert::<K, V, S>)
                .insert_borrowed_str_key(Some(indexmap_insert_borrowed_str_key::<K, V, S>))
                .insert_borrowed_str_entry(Some(indexmap_insert_borrowed_str_entry::<K, V, S>))
                .insert_owned_string_key(Some(indexmap_insert_owned_string_key::<K, V, S>))
                .len(indexmap_len::<K, V, S>)
                .contains_key(indexmap_contains_key::<K, V, S>)
                .get_value_ptr(indexmap_get_value_ptr::<K, V, S>)
                .iter_vtable(IterVTable {
                    init_with_value: Some(indexmap_iter_init::<K, V, S>),
                    next: indexmap_iter_next::<K, V>,
                    next_back: None,
                    size_hint: None,
                    dealloc: indexmap_iter_dealloc::<K, V>,
                })
                .from_pair_slice(Some(indexmap_from_pair_slice::<K, V, S>))
                .pair_stride(core::mem::size_of::<(K, V)>())
                .value_offset_in_pair(core::mem::offset_of!((K, V), 1))
                .build()
        }

        const fn build_type_name<'a, K: Facet<'a>, V: Facet<'a>>() -> TypeNameFn {
            fn type_name_impl<'a, K: Facet<'a>, V: Facet<'a>>(
                _shape: &'static Shape,
                f: &mut core::fmt::Formatter<'_>,
                opts: TypeNameOpts,
            ) -> core::fmt::Result {
                write!(f, "IndexMap")?;
                if let Some(opts) = opts.for_children() {
                    write!(f, "<")?;
                    K::SHAPE.write_type_name(f, opts)?;
                    write!(f, ", ")?;
                    V::SHAPE.write_type_name(f, opts)?;
                    write!(f, ">")?;
                } else {
                    write!(f, "<…>")?;
                }
                Ok(())
            }
            type_name_impl::<K, V>
        }

        ShapeBuilder::for_sized::<Self>("IndexMap")
            .module_path("indexmap")
            .type_name(build_type_name::<K, V>())
            .ty(Type::User(UserType::Opaque))
            .def(Def::Map(MapDef {
                vtable: &const { build_map_vtable::<K, V, S>() },
                k: K::SHAPE,
                v: V::SHAPE,
            }))
            .type_params(&[
                TypeParam {
                    name: "K",
                    shape: K::SHAPE,
                },
                TypeParam {
                    name: "V",
                    shape: V::SHAPE,
                },
            ])
            .type_ops_indirect(
                &const {
                    TypeOpsIndirect {
                        drop_in_place: indexmap_drop::<K, V, S>,
                        default_in_place: Some(indexmap_default::<K, V, S>),
                        clone_into: None,
                        is_truthy: None,
                    }
                },
            )
            .build()
    };
}

unsafe impl<'a, T, S> Facet<'a> for IndexSet<T, S>
where
    T: Facet<'a> + core::cmp::Eq + core::hash::Hash,
    S: Facet<'a> + Default + BuildHasher,
{
    const SHAPE: &'static Shape = &const {
        const fn build_set_vtable<T: Eq + core::hash::Hash, S: Default + BuildHasher>() -> SetVTable
        {
            SetVTable::builder()
                .init_in_place_with_capacity(indexset_init_in_place_with_capacity::<T, S>)
                .insert(indexset_insert::<T, S>)
                .len(indexset_len::<T, S>)
                .contains(indexset_contains::<T, S>)
                .iter_vtable(IterVTable {
                    init_with_value: Some(indexset_iter_init::<T, S>),
                    next: indexset_iter_next::<T>,
                    next_back: Some(indexset_iter_next_back::<T>),
                    size_hint: None,
                    dealloc: indexset_iter_dealloc::<T>,
                })
                .from_slice(Some(indexset_from_slice::<T, S>))
                .build()
        }

        const fn build_type_name<'a, T: Facet<'a>>() -> TypeNameFn {
            fn type_name_impl<'a, T: Facet<'a>>(
                _shape: &'static Shape,
                f: &mut core::fmt::Formatter<'_>,
                opts: TypeNameOpts,
            ) -> core::fmt::Result {
                write!(f, "IndexSet")?;
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

        ShapeBuilder::for_sized::<Self>("IndexSet")
            .module_path("indexmap")
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
            .type_ops_indirect(
                &const {
                    TypeOpsIndirect {
                        drop_in_place: indexset_drop::<T, S>,
                        default_in_place: Some(indexset_default::<T, S>),
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
    use alloc::vec::Vec;
    use core::ptr::NonNull;
    use indexmap::IndexSet;
    use std::hash::RandomState;

    use super::*;

    #[test]
    fn test_indexset_type_params() {
        let [type_param_1, type_param_2] = <IndexSet<i32, RandomState>>::SHAPE.type_params else {
            panic!("IndexSet<T> should have 2 type params")
        };
        assert_eq!(type_param_1.shape(), i32::SHAPE);
        assert_eq!(type_param_2.shape(), RandomState::SHAPE);
    }

    #[test]
    fn test_indexset_vtable_new_insert_iter_drop() {
        facet_testhelpers::setup();

        let indexset_shape = <IndexSet<String, RandomState>>::SHAPE;
        let indexset_def = indexset_shape
            .def
            .into_set()
            .expect("IndexSet<T> should have a set definition");

        let indexset_uninit_ptr = indexset_shape.allocate().unwrap();
        let indexset_ptr =
            unsafe { (indexset_def.vtable.init_in_place_with_capacity)(indexset_uninit_ptr, 3) };

        let indexset_actual_length = unsafe { (indexset_def.vtable.len)(indexset_ptr.as_const()) };
        assert_eq!(indexset_actual_length, 0);

        let strings = ["alpha", "beta", "gamma"];
        for (expected_len, &string) in strings.iter().enumerate() {
            let value = String::from(string);
            let value_ptr = PtrMut::new(NonNull::from(&value).as_ptr() as *mut u8);
            let did_insert = unsafe { (indexset_def.vtable.insert)(indexset_ptr, value_ptr) };
            assert!(
                did_insert,
                "expected value to be inserted into the IndexSet"
            );
            let actual_length = unsafe { (indexset_def.vtable.len)(indexset_ptr.as_const()) };
            assert_eq!(actual_length, expected_len + 1);
            core::mem::forget(value);
        }

        let duplicate = String::from("beta");
        let duplicate_ptr = PtrMut::new(NonNull::from(&duplicate).as_ptr() as *mut u8);
        let did_insert = unsafe { (indexset_def.vtable.insert)(indexset_ptr, duplicate_ptr) };
        assert!(!did_insert, "expected duplicate insertion to be rejected");
        let actual_length = unsafe { (indexset_def.vtable.len)(indexset_ptr.as_const()) };
        assert_eq!(actual_length, strings.len());
        core::mem::forget(duplicate);

        let iter_ptr = unsafe {
            (indexset_def
                .vtable
                .iter_vtable
                .init_with_value
                .expect("IndexSet<T> should provide init_with_value"))(
                indexset_ptr.as_const()
            )
        };

        let mut iter_items = Vec::<String>::new();
        loop {
            let item_ptr = unsafe { (indexset_def.vtable.iter_vtable.next)(iter_ptr) };
            let Some(item_ptr) = item_ptr else {
                break;
            };
            let item = unsafe { item_ptr.get::<String>() };
            iter_items.push(item.clone());
        }
        unsafe {
            (indexset_def.vtable.iter_vtable.dealloc)(iter_ptr);
        }

        let expected_items = strings
            .iter()
            .map(|value| String::from(*value))
            .collect::<Vec<_>>();
        assert_eq!(iter_items, expected_items);

        unsafe {
            indexset_shape
                .call_drop_in_place(indexset_ptr)
                .expect("IndexSet<T> should have drop_in_place");
            indexset_shape.deallocate_mut(indexset_ptr).unwrap();
        }
    }
}
