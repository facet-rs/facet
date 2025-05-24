use core::hash::{BuildHasher, Hash};
use std::collections::HashMap;
use std::hash::RandomState;

use crate::ptr::{PtrConst, PtrMut};

use crate::{
    Def, Facet, IterVTable, MapDef, MapVTable, MarkerTraits, ScalarAffinity, ScalarDef, Shape,
    Type, TypeParam, UserType, VTableView, ValueVTable, value_vtable,
};

type HashMapIterator<'mem, K, V> = std::collections::hash_map::Iter<'mem, K, V>;

unsafe impl<'a, K, V, S> Facet<'a> for HashMap<K, V, S>
where
    K: Facet<'a> + core::cmp::Eq + core::hash::Hash,
    V: Facet<'a>,
    S: Facet<'a> + Default + BuildHasher,
{
    const VTABLE: &'static ValueVTable = &const {
        ValueVTable::builder::<Self>()
            .marker_traits(|| {
                let arg_dependent_traits = MarkerTraits::SEND
                    .union(MarkerTraits::SYNC)
                    .union(MarkerTraits::EQ)
                    .union(MarkerTraits::UNPIN)
                    .union(MarkerTraits::UNWIND_SAFE)
                    .union(MarkerTraits::REF_UNWIND_SAFE);
                arg_dependent_traits
                    .intersection(V::SHAPE.vtable.marker_traits())
                    .intersection(K::SHAPE.vtable.marker_traits())
            })
            .type_name(|f, opts| {
                if let Some(opts) = opts.for_children() {
                    write!(f, "{}<", Self::SHAPE.type_identifier)?;
                    (K::SHAPE.vtable.type_name)(f, opts)?;
                    write!(f, ", ")?;
                    (V::SHAPE.vtable.type_name)(f, opts)?;
                    write!(f, ">")
                } else {
                    write!(f, "{}<⋯>", Self::SHAPE.type_identifier)
                }
            })
            .debug(|| {
                if (K::SHAPE.vtable.debug)().is_some() && (V::SHAPE.vtable.debug)().is_some() {
                    Some(|value, f| {
                        let k_debug = <VTableView<K>>::of().debug().unwrap();
                        let v_debug = <VTableView<V>>::of().debug().unwrap();
                        write!(f, "{{")?;
                        for (i, (key, val)) in value.iter().enumerate() {
                            if i > 0 {
                                write!(f, ", ")?;
                            }
                            (k_debug)(key, f)?;
                            write!(f, ": ")?;
                            (v_debug)(val, f)?;
                        }
                        write!(f, "}}")
                    })
                } else {
                    None
                }
            })
            .default_in_place(|| Some(|target| unsafe { target.put(Self::default()) }))
            .clone_into(|| {
                if (K::SHAPE.vtable.clone_into)().is_some()
                    && (V::SHAPE.vtable.clone_into)().is_some()
                {
                    Some(|src, dst| unsafe {
                        let map = src;
                        let mut new_map =
                            HashMap::with_capacity_and_hasher(map.len(), S::default());

                        let k_clone_into = <VTableView<K>>::of().clone_into().unwrap();
                        let v_clone_into = <VTableView<V>>::of().clone_into().unwrap();

                        for (k, v) in map {
                            use crate::TypedPtrUninit;
                            use core::mem::MaybeUninit;

                            let mut new_k = MaybeUninit::<K>::uninit();
                            let mut new_v = MaybeUninit::<V>::uninit();

                            let uninit_k = TypedPtrUninit::new(new_k.as_mut_ptr());
                            let uninit_v = TypedPtrUninit::new(new_v.as_mut_ptr());

                            (k_clone_into)(k, uninit_k);
                            (v_clone_into)(v, uninit_v);

                            new_map.insert(new_k.assume_init(), new_v.assume_init());
                        }

                        dst.put(new_map)
                    })
                } else {
                    None
                }
            })
            .eq(|| {
                if (V::SHAPE.vtable.eq)().is_some() {
                    Some(|a, b| {
                        let v_eq = <VTableView<V>>::of().eq().unwrap();
                        a.len() == b.len()
                            && a.iter().all(|(key_a, val_a)| {
                                b.get(key_a).is_some_and(|val_b| (v_eq)(val_a, val_b))
                            })
                    })
                } else {
                    None
                }
            })
            .hash(|| {
                if (V::SHAPE.vtable.hash)().is_some() {
                    Some(|map, hasher_this, hasher_write_fn| unsafe {
                        use crate::HasherProxy;
                        let v_hash = <VTableView<V>>::of().hash().unwrap();
                        let mut hasher = HasherProxy::new(hasher_this, hasher_write_fn);
                        map.len().hash(&mut hasher);
                        for (k, v) in map {
                            k.hash(&mut hasher);
                            (v_hash)(v, hasher_this, hasher_write_fn);
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
            .type_identifier("HashMap")
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
            .ty(Type::User(UserType::Opaque))
            .def(Def::Map(
                MapDef::builder()
                    .k(|| K::SHAPE)
                    .v(|| V::SHAPE)
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
                                    map.get(key.get()).map(|v| PtrConst::new(v))
                                })
                                .iter_vtable(
                                    IterVTable::builder()
                                        .init_with_value(|ptr| unsafe {
                                            let map = ptr.get::<HashMap<K, V>>();
                                            let iter: HashMapIterator<'_, K, V> = map.iter();
                                            let iter_state = Box::new(iter);
                                            PtrMut::new(Box::into_raw(iter_state) as *mut u8)
                                        })
                                        .next(|iter_ptr| unsafe {
                                            let state =
                                                iter_ptr.as_mut::<HashMapIterator<'_, K, V>>();
                                            state.next().map(|(key, value)| {
                                                (
                                                    PtrConst::new(key as *const K),
                                                    PtrConst::new(value as *const V),
                                                )
                                            })
                                        })
                                        .dealloc(|iter_ptr| unsafe {
                                            drop(Box::from_raw(
                                                iter_ptr.as_ptr::<HashMapIterator<'_, K, V>>()
                                                    as *mut HashMapIterator<'_, K, V>,
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
    const VTABLE: &'static ValueVTable =
        &const { value_vtable!((), |f, _opts| write!(f, "{}", Self::SHAPE.type_identifier)) };

    const SHAPE: &'static Shape<'static> = &const {
        Shape::builder_for_sized::<Self>()
            .type_identifier("RandomState")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Scalar(
                ScalarDef::builder()
                    .affinity(&const { ScalarAffinity::opaque().build() })
                    .build(),
            ))
            .build()
    };
}
