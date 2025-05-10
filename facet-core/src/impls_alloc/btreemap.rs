use core::write;

use alloc::{
    boxed::Box,
    collections::{BTreeMap, VecDeque},
};

use crate::{
    Def, Facet, MapDef, MapIterVTable, MapVTable, MarkerTraits, PtrConst, PtrMut, Shape, Type,
    UserType, VTableView, ValueVTable,
};

struct BTreeMapIterator<'mem, K> {
    map: PtrConst<'mem>,
    keys: VecDeque<&'mem K>,
}

unsafe impl<'a, K, V> Facet<'a> for BTreeMap<K, V>
where
    K: Facet<'a> + core::cmp::Eq + core::cmp::Ord,
    V: Facet<'a>,
{
    const VTABLE: &'static ValueVTable = &const {
        let mut builder = ValueVTable::builder::<Self>()
            .marker_traits({
                let arg_dependent_traits = MarkerTraits::SEND
                    .union(MarkerTraits::SYNC)
                    .union(MarkerTraits::EQ);
                arg_dependent_traits
                    .intersection(V::SHAPE.vtable.marker_traits)
                    .intersection(K::SHAPE.vtable.marker_traits)
                    // only depends on `A` which we are not generic over (yet)
                    .union(MarkerTraits::UNPIN)
            })
            .type_name(|f, opts| {
                if let Some(opts) = opts.for_children() {
                    write!(f, "BTreeMap<")?;
                    (K::SHAPE.vtable.type_name)(f, opts)?;
                    write!(f, ", ")?;
                    (V::SHAPE.vtable.type_name)(f, opts)?;
                    write!(f, ">")
                } else {
                    write!(f, "BTreeMap<⋯>")
                }
            });

        if K::SHAPE.vtable.debug.is_some() && V::SHAPE.vtable.debug.is_some() {
            builder = builder.debug(|value, f| {
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
        }

        builder = builder.default_in_place(|target| unsafe { target.put(Self::default()) });

        if V::SHAPE.vtable.clone_into.is_some() && K::SHAPE.vtable.clone_into.is_some() {
            builder = builder.clone_into(|src, dst| unsafe {
                let mut new_map = BTreeMap::new();

                let k_clone_into = <VTableView<K>>::of().clone_into().unwrap();
                let v_clone_into = <VTableView<V>>::of().clone_into().unwrap();

                for (k, v) in src {
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
            });
        }

        if V::SHAPE.vtable.eq.is_some() {
            builder = builder.eq(|a, b| {
                let v_eq = <VTableView<V>>::of().eq().unwrap();
                a.len() == b.len()
                    && a.iter().all(|(key_a, val_a)| {
                        b.get(key_a).is_some_and(|val_b| (v_eq)(val_a, val_b))
                    })
            });
        }

        if K::SHAPE.vtable.hash.is_some() && V::SHAPE.vtable.hash.is_some() {
            builder = builder.hash(|map, hasher_this, hasher_write_fn| unsafe {
                use crate::HasherProxy;
                use core::hash::Hash;

                let k_hash = <VTableView<K>>::of().hash().unwrap();
                let v_hash = <VTableView<V>>::of().hash().unwrap();
                let mut hasher = HasherProxy::new(hasher_this, hasher_write_fn);
                map.len().hash(&mut hasher);
                for (k, v) in map {
                    (k_hash)(k, hasher_this, hasher_write_fn);
                    (v_hash)(v, hasher_this, hasher_write_fn);
                }
            });
        }

        builder.build()
    };

    const SHAPE: &'static crate::Shape = &const {
        Shape::builder_for_sized::<Self>()
            .type_params(&[
                crate::TypeParam {
                    name: "K",
                    shape: || K::SHAPE,
                },
                crate::TypeParam {
                    name: "V",
                    shape: || V::SHAPE,
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
                                .init_in_place_with_capacity(|uninit, _capacity| unsafe {
                                    uninit.put(Self::new())
                                })
                                .insert(|ptr, key, value| unsafe {
                                    let map = ptr.as_mut::<Self>();
                                    let k = key.read::<K>();
                                    let v = value.read::<V>();
                                    map.insert(k, v);
                                })
                                .len(|ptr| unsafe {
                                    let map = ptr.get::<Self>();
                                    map.len()
                                })
                                .contains_key(|ptr, key| unsafe {
                                    let map = ptr.get::<Self>();
                                    map.contains_key(key.get())
                                })
                                .get_value_ptr(|ptr, key| unsafe {
                                    let map = ptr.get::<Self>();
                                    map.get(key.get()).map(|v| PtrConst::new(v as *const _))
                                })
                                .iter(|ptr| unsafe {
                                    let map = ptr.get::<Self>();
                                    let keys: VecDeque<&K> = map.keys().collect();
                                    let iter_state = Box::new(BTreeMapIterator { map: ptr, keys });
                                    PtrMut::new(Box::into_raw(iter_state) as *mut u8)
                                })
                                .iter_vtable(
                                    MapIterVTable::builder()
                                        .next(|iter_ptr| unsafe {
                                            let state =
                                                iter_ptr.as_mut::<BTreeMapIterator<'_, K>>();
                                            let map = state.map.get::<Self>();
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
                                                iter_ptr.as_ptr::<BTreeMapIterator<'_, K>>()
                                                    as *mut BTreeMapIterator<'_, K>,
                                            ))
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
