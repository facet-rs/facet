use core::ptr::NonNull;

use alloc::{boxed::Box, collections::BTreeMap};

use crate::{
    Def, Facet, IterVTable, MapDef, MapVTable, PtrConst, PtrMut, ShapeBuilder, TypeParam,
    ValueVTable,
};

type BTreeMapIterator<'mem, K, V> = alloc::collections::btree_map::Iter<'mem, K, V>;

// TODO: Debug, Hash, PartialEq, Eq, PartialOrd, Ord, for BTreeMap, BTreeSet
unsafe impl<'a, K, V> Facet<'a> for BTreeMap<K, V>
where
    K: Facet<'a> + core::cmp::Eq + core::cmp::Ord,
    V: Facet<'a>,
{
    const SHAPE: &'static crate::Shape = &const {
        ShapeBuilder::for_sized::<Self>(
            |f, opts| {
                write!(f, "{}<", Self::SHAPE.type_identifier)?;
                if let Some(opts) = opts.for_children() {
                    K::SHAPE.vtable.type_name()(f, opts)?;
                    write!(f, ", ")?;
                    V::SHAPE.vtable.type_name()(f, opts)?;
                } else {
                    write!(f, "…")?;
                }
                write!(f, ">")
            },
            "BTreeMap",
        )
        .drop_in_place(ValueVTable::drop_in_place_for::<Self>())
        .default_in_place(|target| unsafe { target.put(Self::default()) })
        .def(Def::Map(MapDef::new(
            &const {
                MapVTable {
                    init_in_place_with_capacity_fn: |uninit, _capacity| unsafe {
                        uninit.put(Self::new())
                    },
                    insert_fn: |ptr, key, value| unsafe {
                        let map = ptr.as_mut::<Self>();
                        let k = key.read::<K>();
                        let v = value.read::<V>();
                        map.insert(k, v);
                    },
                    len_fn: |ptr| unsafe {
                        let map = ptr.get::<Self>();
                        map.len()
                    },
                    contains_key_fn: |ptr, key| unsafe {
                        let map = ptr.get::<Self>();
                        map.contains_key(key.get())
                    },
                    get_value_ptr_fn: |ptr, key| unsafe {
                        let map = ptr.get::<Self>();
                        map.get(key.get()).map(|v| PtrConst::new(NonNull::from(v)))
                    },
                    iter_vtable: IterVTable {
                        init_with_value: Some(|ptr| unsafe {
                            let map = ptr.get::<Self>();
                            let iter: BTreeMapIterator<'_, K, V> = map.iter();
                            let state = Box::new(iter);
                            PtrMut::new(NonNull::new_unchecked(Box::into_raw(state) as *mut u8))
                        }),
                        next: |iter_ptr| unsafe {
                            let state = iter_ptr.as_mut::<BTreeMapIterator<'_, K, V>>();
                            state.next().map(|(key, value)| {
                                (
                                    PtrConst::new(NonNull::from(key)),
                                    PtrConst::new(NonNull::from(value)),
                                )
                            })
                        },
                        next_back: Some(|iter_ptr| unsafe {
                            let state = iter_ptr.as_mut::<BTreeMapIterator<'_, K, V>>();
                            state.next_back().map(|(key, value)| {
                                (
                                    PtrConst::new(NonNull::from(key)),
                                    PtrConst::new(NonNull::from(value)),
                                )
                            })
                        }),
                        size_hint: None,
                        dealloc: |iter_ptr| unsafe {
                            drop(Box::from_raw(
                                iter_ptr.as_ptr::<BTreeMapIterator<'_, K, V>>()
                                    as *mut BTreeMapIterator<'_, K, V>,
                            ))
                        },
                    },
                }
            },
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
        .build()
    };
}
