#![cfg(feature = "indexmap")]

use alloc::boxed::Box;
use core::hash::BuildHasher;
use core::ptr::NonNull;
use indexmap::IndexMap;

use crate::{PtrConst, PtrMut, PtrUninit};

use crate::{
    Def, Facet, IterVTable, MapDef, MapVTable, OxPtrMut, Shape, ShapeBuilder, Type, TypeNameFn,
    TypeNameOpts, TypeOpsIndirect, TypeParam, UserType,
};

type IndexMapIterator<'mem, K, V> = indexmap::map::Iter<'mem, K, V>;

unsafe fn indexmap_init_in_place_with_capacity<K, V, S: Default + BuildHasher>(
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

unsafe fn indexmap_insert<K: Eq + core::hash::Hash, V, S: BuildHasher>(
    ptr: PtrMut,
    key: PtrMut,
    value: PtrMut,
) {
    let map = unsafe { ptr.as_mut::<IndexMap<K, V, S>>() };
    let key = unsafe { key.read::<K>() };
    let value = unsafe { value.read::<V>() };
    map.insert(key, value);
}

unsafe fn indexmap_len<K, V, S>(ptr: PtrConst) -> usize {
    unsafe { ptr.get::<IndexMap<K, V, S>>().len() }
}

unsafe fn indexmap_contains_key<K: Eq + core::hash::Hash, V, S: BuildHasher>(
    ptr: PtrConst,
    key: PtrConst,
) -> bool {
    unsafe { ptr.get::<IndexMap<K, V, S>>().contains_key(key.get::<K>()) }
}

unsafe fn indexmap_get_value_ptr<K: Eq + core::hash::Hash, V, S: BuildHasher>(
    ptr: PtrConst,
    key: PtrConst,
) -> Option<PtrConst> {
    unsafe {
        ptr.get::<IndexMap<K, V, S>>()
            .get(key.get::<K>())
            .map(|v| PtrConst::new(NonNull::from(v).as_ptr()))
    }
}

unsafe fn indexmap_iter_init<K, V, S>(ptr: PtrConst) -> PtrMut {
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

unsafe fn indexmap_iter_dealloc<K, V>(iter_ptr: PtrMut) {
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

unsafe fn indexmap_default<K, V, S: Default + BuildHasher>(ox: OxPtrMut) {
    unsafe { ox.ptr().as_uninit().put(IndexMap::<K, V, S>::default()) };
}

/// Build an IndexMap from a contiguous slice of (K, V) pairs.
unsafe fn indexmap_from_pair_slice<K: Eq + core::hash::Hash, V, S: Default + BuildHasher>(
    uninit: PtrUninit,
    pairs_ptr: *mut u8,
    count: usize,
) -> PtrMut {
    let pairs = pairs_ptr as *mut (K, V);
    let iter = (0..count).map(|i| unsafe {
        let pair_ptr = pairs.add(i);
        core::ptr::read(pair_ptr)
    });
    let map: IndexMap<K, V, S> = iter.collect();
    unsafe { uninit.put(map) }
}

unsafe impl<'a, K, V, S> Facet<'a> for IndexMap<K, V, S>
where
    K: Facet<'a> + core::cmp::Eq + core::hash::Hash,
    V: Facet<'a>,
    S: 'a + Default + BuildHasher,
{
    const SHAPE: &'static Shape = &const {
        const fn build_map_vtable<K: Eq + core::hash::Hash, V, S: Default + BuildHasher>()
        -> MapVTable {
            MapVTable::builder()
                .init_in_place_with_capacity(indexmap_init_in_place_with_capacity::<K, V, S>)
                .insert(indexmap_insert::<K, V, S>)
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
