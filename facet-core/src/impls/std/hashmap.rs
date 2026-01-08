use core::hash::BuildHasher;
use core::ptr::NonNull;
use std::collections::HashMap;
use std::hash::RandomState;

use crate::{PtrConst, PtrMut, PtrUninit};

use crate::{
    Def, Facet, IterVTable, MapDef, MapVTable, Shape, ShapeBuilder, Type, TypeNameFn, TypeNameOpts,
    TypeOpsIndirect, TypeParam, UserType, VTableDirect, VTableIndirect, Variance, VarianceDep,
    VarianceDesc,
};

type HashMapIterator<'mem, K, V> = std::collections::hash_map::Iter<'mem, K, V>;

unsafe fn hashmap_init_in_place_with_capacity<K, V, S: Default + BuildHasher>(
    uninit: PtrUninit,
    capacity: usize,
) -> PtrMut {
    unsafe {
        uninit.put(HashMap::<K, V, S>::with_capacity_and_hasher(
            capacity,
            S::default(),
        ))
    }
}

unsafe fn hashmap_insert<K: Eq + core::hash::Hash, V>(ptr: PtrMut, key: PtrMut, value: PtrMut) {
    let map = unsafe { ptr.as_mut::<HashMap<K, V>>() };
    let key = unsafe { key.read::<K>() };
    let value = unsafe { value.read::<V>() };
    map.insert(key, value);
}

unsafe fn hashmap_len<K, V>(ptr: PtrConst) -> usize {
    unsafe { ptr.get::<HashMap<K, V>>().len() }
}

unsafe fn hashmap_contains_key<K: Eq + core::hash::Hash, V>(ptr: PtrConst, key: PtrConst) -> bool {
    unsafe { ptr.get::<HashMap<K, V>>().contains_key(key.get()) }
}

unsafe fn hashmap_get_value_ptr<K: Eq + core::hash::Hash, V>(
    ptr: PtrConst,
    key: PtrConst,
) -> Option<PtrConst> {
    unsafe {
        ptr.get::<HashMap<K, V>>()
            .get(key.get())
            .map(|v| PtrConst::new(NonNull::from(v).as_ptr()))
    }
}

/// Build a HashMap from a contiguous slice of (K, V) pairs.
///
/// This uses `from_iter` with known capacity to avoid rehashing.
unsafe fn hashmap_from_pair_slice<K: Eq + core::hash::Hash, V, S: Default + BuildHasher>(
    uninit: PtrUninit,
    pairs_ptr: *mut u8,
    count: usize,
) -> PtrMut {
    // Create an iterator that reads and moves (K, V) pairs from the buffer
    let pairs = pairs_ptr as *mut (K, V);
    let iter = (0..count).map(|i| unsafe {
        let pair_ptr = pairs.add(i);
        core::ptr::read(pair_ptr)
    });

    // Build HashMap with from_iter (which uses reserve internally)
    let map: HashMap<K, V, S> = iter.collect();
    unsafe { uninit.put(map) }
}

unsafe fn hashmap_iter_init<K, V>(ptr: PtrConst) -> PtrMut {
    unsafe {
        let map = ptr.get::<HashMap<K, V>>();
        let iter: HashMapIterator<'_, K, V> = map.iter();
        let iter_state = Box::new(iter);
        PtrMut::new(Box::into_raw(iter_state) as *mut u8)
    }
}

unsafe fn hashmap_iter_next<K, V>(iter_ptr: PtrMut) -> Option<(PtrConst, PtrConst)> {
    unsafe {
        // SAFETY: We're extending the lifetime from '_ to 'static through a raw pointer cast.
        // This is sound because:
        // 1. The iterator was allocated in hashmap_iter_init and lives until hashmap_iter_dealloc
        // 2. We only return pointers (PtrConst), not references with the extended lifetime
        // 3. The actual lifetime of the data is managed by the HashMap, not this iterator
        let ptr = iter_ptr.as_mut_ptr::<HashMapIterator<'_, K, V>>();
        let state = &mut *ptr;
        state.next().map(|(key, value)| {
            (
                PtrConst::new(NonNull::from(key).as_ptr()),
                PtrConst::new(NonNull::from(value).as_ptr()),
            )
        })
    }
}

unsafe fn hashmap_iter_dealloc<K, V>(iter_ptr: PtrMut) {
    unsafe {
        drop(Box::from_raw(
            iter_ptr.as_ptr::<HashMapIterator<'_, K, V>>() as *mut HashMapIterator<'_, K, V>,
        ));
    }
}

unsafe fn hashmap_drop<K, V, S>(ox: crate::OxPtrMut) {
    unsafe {
        core::ptr::drop_in_place(ox.as_mut::<HashMap<K, V, S>>());
    }
}

unsafe fn hashmap_default<K, V, S: Default + BuildHasher>(ox: crate::OxPtrMut) {
    unsafe { ox.ptr().as_uninit().put(HashMap::<K, V, S>::default()) };
}

unsafe fn hashmap_is_truthy<K, V>(ptr: PtrConst) -> bool {
    !unsafe { ptr.get::<HashMap<K, V>>().is_empty() }
}

// TODO: Debug, PartialEq, Eq for HashMap, HashSet
unsafe impl<'a, K, V, S> Facet<'a> for HashMap<K, V, S>
where
    K: Facet<'a> + core::cmp::Eq + core::hash::Hash,
    V: Facet<'a>,
    S: 'a + Default + BuildHasher,
{
    const SHAPE: &'static Shape = &const {
        const fn build_map_vtable<K: Eq + core::hash::Hash, V, S: Default + BuildHasher>()
        -> MapVTable {
            MapVTable::builder()
                .init_in_place_with_capacity(hashmap_init_in_place_with_capacity::<K, V, S>)
                .insert(hashmap_insert::<K, V>)
                .len(hashmap_len::<K, V>)
                .contains_key(hashmap_contains_key::<K, V>)
                .get_value_ptr(hashmap_get_value_ptr::<K, V>)
                .iter_vtable(IterVTable {
                    init_with_value: Some(hashmap_iter_init::<K, V>),
                    next: hashmap_iter_next::<K, V>,
                    next_back: None,
                    size_hint: None,
                    dealloc: hashmap_iter_dealloc::<K, V>,
                })
                .from_pair_slice(Some(hashmap_from_pair_slice::<K, V, S>))
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
                write!(f, "HashMap")?;
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

        ShapeBuilder::for_sized::<Self>("HashMap")
            .decl_id_prim()
            .module_path("std::collections::hash_map")
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
            // HashMap<K, V> combines K and V variances
            .variance(VarianceDesc {
                base: Variance::Bivariant,
                deps: &const {
                    [
                        VarianceDep::covariant(K::SHAPE),
                        VarianceDep::covariant(V::SHAPE),
                    ]
                },
            })
            .vtable_indirect(&VTableIndirect::EMPTY)
            .type_ops_indirect(
                &const {
                    TypeOpsIndirect {
                        drop_in_place: hashmap_drop::<K, V, S>,
                        default_in_place: Some(hashmap_default::<K, V, S>),
                        clone_into: None,
                        is_truthy: Some(hashmap_is_truthy::<K, V>),
                    }
                },
            )
            .build()
    };
}

unsafe impl Facet<'_> for RandomState {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = VTableDirect::empty();

        ShapeBuilder::for_sized::<Self>("RandomState")
            .decl_id_prim()
            .module_path("std::hash")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar)
            .vtable_direct(&VTABLE)
            .build()
    };
}
