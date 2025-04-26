use alloc::collections::VecDeque;
use core::hash::Hash;
use std::collections::HashMap;
use std::hash::RandomState;

use crate::ptr::{PtrConst, PtrMut};

use crate::{
    Def, Facet, MapDef, MapIterVTable, MapVTable, MarkerTraits, ScalarAffinity, ScalarDef, Shape,
    TypeParam, ValueVTable, value_vtable,
};

struct HashMapIterator<'mem, K> {
    map: PtrConst<'mem>,
    keys: VecDeque<&'mem K>,
}

unsafe impl<'a, K, V, S> Facet<'a> for HashMap<K, V, S>
where
    K: Facet<'a> + core::cmp::Eq + core::hash::Hash,
    V: Facet<'a>,
    S: Facet<'a> + Default,
{
    const SHAPE: &'static Shape = &const {
        Shape::builder_for_sized::<Self>()
            .type_params(&[
                TypeParam {
                    name: "K",
                    shape: || K::SHAPE,
                },
                TypeParam {
                    name: "V",
                    shape: || V::SHAPE,
                },
                TypeParam {
                    name: "S",
                    shape: || S::SHAPE,
                },
            ])
            .vtable(
                &const {
                    let mut builder = ValueVTable::builder()
                        .marker_traits({
                            let arg_dependent_traits = MarkerTraits::SEND
                                .union(MarkerTraits::SYNC)
                                .union(MarkerTraits::EQ)
                                .union(MarkerTraits::UNPIN);
                            arg_dependent_traits
                                .intersection(V::SHAPE.vtable.marker_traits)
                                .intersection(K::SHAPE.vtable.marker_traits)
                        })
                        .type_name(|f, opts| {
                            if let Some(opts) = opts.for_children() {
                                write!(f, "HashMap<")?;
                                (K::SHAPE.vtable.type_name)(f, opts)?;
                                write!(f, ", ")?;
                                (V::SHAPE.vtable.type_name)(f, opts)?;
                                write!(f, ">")
                            } else {
                                write!(f, "HashMap<⋯>")
                            }
                        })
                        .drop_in_place(|value| unsafe { value.drop_in_place::<HashMap<K, V>>() });

                    if K::SHAPE.vtable.debug.is_some() && V::SHAPE.vtable.debug.is_some() {
                        builder = builder.debug(|value, f| unsafe {
                            let value = value.get::<HashMap<K, V>>();
                            let k_debug = K::SHAPE.vtable.debug.unwrap_unchecked();
                            let v_debug = V::SHAPE.vtable.debug.unwrap_unchecked();
                            write!(f, "{{")?;
                            for (i, (key, val)) in value.iter().enumerate() {
                                if i > 0 {
                                    write!(f, ", ")?;
                                }
                                (k_debug)(PtrConst::new(key as *const _), f)?;
                                write!(f, ": ")?;
                                (v_debug)(PtrConst::new(val as *const _), f)?;
                            }
                            write!(f, "}}")
                        });
                    }

                    builder =
                        builder.default_in_place(|target| unsafe { target.put(Self::default()) });

                    builder = builder
                        .clone_into(|src, dst| unsafe { dst.put(src.get::<HashMap<K, V>>()) });

                    if V::SHAPE.vtable.eq.is_some() {
                        builder = builder.eq(|a, b| unsafe {
                            let a = a.get::<HashMap<K, V>>();
                            let b = b.get::<HashMap<K, V>>();
                            let v_eq = V::SHAPE.vtable.eq.unwrap_unchecked();
                            a.len() == b.len()
                                && a.iter().all(|(key_a, val_a)| {
                                    b.get(key_a).is_some_and(|val_b| {
                                        (v_eq)(
                                            PtrConst::new(val_a as *const _),
                                            PtrConst::new(val_b as *const _),
                                        )
                                    })
                                })
                        });
                    }

                    if V::SHAPE.vtable.hash.is_some() {
                        builder = builder.hash(|value, hasher_this, hasher_write_fn| unsafe {
                            use crate::HasherProxy;
                            let map = value.get::<HashMap<K, V>>();
                            let v_hash = V::SHAPE.vtable.hash.unwrap_unchecked();
                            let mut hasher = HasherProxy::new(hasher_this, hasher_write_fn);
                            map.len().hash(&mut hasher);
                            for (k, v) in map {
                                k.hash(&mut hasher);
                                (v_hash)(
                                    PtrConst::new(v as *const _),
                                    hasher_this,
                                    hasher_write_fn,
                                );
                            }
                        });
                    }

                    builder.build()
                },
            )
            .def(Def::Map(
                MapDef::builder()
                    .k(K::SHAPE)
                    .v(V::SHAPE)
                    .vtable(
                        &const {
                            MapVTable::builder()
                                .init_in_place_with_capacity(|uninit, capacity| unsafe {
                                    uninit
                                        .put(Self::with_capacity_and_hasher(capacity, S::default()))
                                })
                                .insert(|ptr, key, value| unsafe {
                                    let map = ptr.as_mut::<HashMap<K, V>>();
                                    let key = key.read::<K>();
                                    let value = value.read::<V>();
                                    map.insert(key, value);
                                })
                                .len(|ptr| unsafe {
                                    let map = ptr.get::<HashMap<K, V>>();
                                    map.len()
                                })
                                .contains_key(|ptr, key| unsafe {
                                    let map = ptr.get::<HashMap<K, V>>();
                                    map.contains_key(key.get())
                                })
                                .get_value_ptr(|ptr, key| unsafe {
                                    let map = ptr.get::<HashMap<K, V>>();
                                    map.get(key.get()).map(|v| PtrConst::new(v as *const _))
                                })
                                .iter(|ptr| unsafe {
                                    let map = ptr.get::<HashMap<K, V>>();
                                    let keys: VecDeque<&K> = map.keys().collect();
                                    let iter_state = Box::new(HashMapIterator { map: ptr, keys });
                                    PtrMut::new(Box::into_raw(iter_state) as *mut u8)
                                })
                                .iter_vtable(
                                    MapIterVTable::builder()
                                        .next(|iter_ptr| unsafe {
                                            let state = iter_ptr.as_mut::<HashMapIterator<'_, K>>();
                                            let map = state.map.get::<HashMap<K, V>>();
                                            while let Some(key) = state.keys.pop_front() {
                                                if let Some(value) = map.get(key) {
                                                    return Some((
                                                        PtrConst::new(key as *const K),
                                                        PtrConst::new(value as *const V),
                                                    ));
                                                }
                                            }

                                            None
                                        })
                                        .dealloc(|iter_ptr| unsafe {
                                            drop(Box::from_raw(
                                                iter_ptr.as_ptr::<HashMapIterator<'_, K>>()
                                                    as *mut HashMapIterator<'_, K>,
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

unsafe impl Facet<'_> for RandomState {
    const SHAPE: &'static Shape = &const {
        Shape::builder_for_sized::<Self>()
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(ScalarAffinity::opaque().build())
                    .build(),
            ))
            .vtable(&const { value_vtable!((), |f, _opts| write!(f, "RandomState")) })
            .build()
    };
}
