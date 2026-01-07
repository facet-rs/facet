use alloc::{boxed::Box, collections::BTreeMap};

use crate::{
    Def, Facet, IterVTable, MapDef, MapVTable, OxPtrMut, PtrConst, PtrMut, PtrUninit, Shape,
    ShapeBuilder, TypeNameFn, TypeNameOpts, TypeOpsIndirect, TypeParam, VTableIndirect, Variance,
    VarianceDep, VarianceDesc,
};

type BTreeMapIterator<'mem, K, V> = alloc::collections::btree_map::Iter<'mem, K, V>;

unsafe fn btreemap_init_in_place_with_capacity<K, V>(
    uninit: PtrUninit,
    _capacity: usize,
) -> PtrMut {
    unsafe { uninit.put(BTreeMap::<K, V>::new()) }
}

unsafe fn btreemap_insert<K: Eq + Ord + 'static, V: 'static>(
    ptr: PtrMut,
    key: PtrMut,
    value: PtrMut,
) {
    unsafe {
        let map = ptr.as_mut::<BTreeMap<K, V>>();
        let k = key.read::<K>();
        let v = value.read::<V>();
        map.insert(k, v);
    }
}

unsafe fn btreemap_len<K: 'static, V: 'static>(ptr: PtrConst) -> usize {
    unsafe { ptr.get::<BTreeMap<K, V>>().len() }
}

unsafe fn btreemap_contains_key<K: Eq + Ord + 'static, V: 'static>(
    ptr: PtrConst,
    key: PtrConst,
) -> bool {
    unsafe { ptr.get::<BTreeMap<K, V>>().contains_key(key.get()) }
}

unsafe fn btreemap_get_value_ptr<K: Eq + Ord + 'static, V: 'static>(
    ptr: PtrConst,
    key: PtrConst,
) -> Option<PtrConst> {
    unsafe {
        ptr.get::<BTreeMap<K, V>>()
            .get(key.get())
            .map(|v| PtrConst::new(v as *const V))
    }
}

unsafe fn btreemap_iter_init<K: 'static, V: 'static>(ptr: PtrConst) -> PtrMut {
    unsafe {
        let map = ptr.get::<BTreeMap<K, V>>();
        let iter: BTreeMapIterator<'_, K, V> = map.iter();
        let state = Box::new(iter);
        PtrMut::new(Box::into_raw(state) as *mut u8)
    }
}

unsafe fn btreemap_iter_next<K: 'static, V: 'static>(
    iter_ptr: PtrMut,
) -> Option<(PtrConst, PtrConst)> {
    unsafe {
        let state = iter_ptr.as_mut::<BTreeMapIterator<'static, K, V>>();
        state.next().map(|(key, value)| {
            (
                PtrConst::new(key as *const K),
                PtrConst::new(value as *const V),
            )
        })
    }
}

unsafe fn btreemap_iter_next_back<K: 'static, V: 'static>(
    iter_ptr: PtrMut,
) -> Option<(PtrConst, PtrConst)> {
    unsafe {
        let state = iter_ptr.as_mut::<BTreeMapIterator<'static, K, V>>();
        state.next_back().map(|(key, value)| {
            (
                PtrConst::new(key as *const K),
                PtrConst::new(value as *const V),
            )
        })
    }
}

unsafe fn btreemap_iter_dealloc<K, V>(iter_ptr: PtrMut) {
    unsafe {
        drop(Box::from_raw(
            iter_ptr.as_ptr::<BTreeMapIterator<'_, K, V>>() as *mut BTreeMapIterator<'_, K, V>,
        ))
    }
}

/// Build a BTreeMap from a contiguous slice of (K, V) pairs.
unsafe fn btreemap_from_pair_slice<K: Eq + Ord + 'static, V: 'static>(
    uninit: PtrUninit,
    pairs_ptr: *mut u8,
    count: usize,
) -> PtrMut {
    let pairs = pairs_ptr as *mut (K, V);
    let iter = (0..count).map(|i| unsafe {
        let pair_ptr = pairs.add(i);
        core::ptr::read(pair_ptr)
    });
    let map: BTreeMap<K, V> = iter.collect();
    unsafe { uninit.put(map) }
}

/// Drop for BTreeMap<K, V>
unsafe fn btreemap_drop<K: 'static, V: 'static>(ox: OxPtrMut) {
    unsafe {
        core::ptr::drop_in_place(ox.as_mut::<BTreeMap<K, V>>());
    }
}

/// Default for BTreeMap<K, V>
unsafe fn btreemap_default<K: 'static, V: 'static>(ox: OxPtrMut) {
    unsafe { ox.ptr().as_uninit().put(BTreeMap::<K, V>::new()) };
}

// TODO: Debug, Hash, PartialEq, Eq, PartialOrd, Ord, for BTreeMap, BTreeSet
unsafe impl<'a, K, V> Facet<'a> for BTreeMap<K, V>
where
    K: Facet<'a> + core::cmp::Eq + core::cmp::Ord + 'static,
    V: Facet<'a> + 'static,
{
    const SHAPE: &'static crate::Shape = &const {
        const fn build_map_vtable<K: Eq + Ord + 'static, V: 'static>() -> MapVTable {
            MapVTable::builder()
                .init_in_place_with_capacity(btreemap_init_in_place_with_capacity::<K, V>)
                .insert(btreemap_insert::<K, V>)
                .len(btreemap_len::<K, V>)
                .contains_key(btreemap_contains_key::<K, V>)
                .get_value_ptr(btreemap_get_value_ptr::<K, V>)
                .iter_vtable(IterVTable {
                    init_with_value: Some(btreemap_iter_init::<K, V>),
                    next: btreemap_iter_next::<K, V>,
                    next_back: Some(btreemap_iter_next_back::<K, V>),
                    size_hint: None,
                    dealloc: btreemap_iter_dealloc::<K, V>,
                })
                .from_pair_slice(Some(btreemap_from_pair_slice::<K, V>))
                .pair_stride(core::mem::size_of::<(K, V)>())
                .value_offset_in_pair(core::mem::offset_of!((K, V), 1))
                .build()
        }

        const VTABLE: VTableIndirect = VTableIndirect::EMPTY;

        const fn build_type_name<'a, K: Facet<'a>, V: Facet<'a>>() -> TypeNameFn {
            fn type_name_impl<'a, K: Facet<'a>, V: Facet<'a>>(
                _shape: &'static Shape,
                f: &mut core::fmt::Formatter<'_>,
                opts: TypeNameOpts,
            ) -> core::fmt::Result {
                write!(f, "BTreeMap")?;
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

        ShapeBuilder::for_sized::<Self>("BTreeMap")
            .type_name(build_type_name::<K, V>())
            .vtable_indirect(&VTABLE)
            .def(Def::Map(MapDef::new(
                &const { build_map_vtable::<K, V>() },
                K::SHAPE,
                V::SHAPE,
            )))
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
            // BTreeMap<K, V> combines K and V variances
            .variance(VarianceDesc {
                base: Variance::Bivariant,
                deps: &const {
                    [
                        VarianceDep::covariant(K::SHAPE),
                        VarianceDep::covariant(V::SHAPE),
                    ]
                },
            })
            .type_ops_indirect(
                &const {
                    TypeOpsIndirect {
                        drop_in_place: btreemap_drop::<K, V>,
                        default_in_place: Some(btreemap_default::<K, V>),
                        clone_into: None,
                        is_truthy: None,
                    }
                },
            )
            .build()
    };
}
