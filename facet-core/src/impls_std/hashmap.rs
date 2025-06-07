use core::hash::BuildHasher;
use std::collections::HashMap;
use std::hash::RandomState;

use crate::ptr::{PtrConst, PtrMut};

use crate::{
    Def, Facet, IterVTable, MapDef, MapVTable, MarkerTraits, ScalarAffinity, ScalarDef, Shape,
    Type, TypeParam, UserType, ValueVTable, value_vtable,
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
                    K::SHAPE.vtable.type_name()(f, opts)?;
                    write!(f, ", ")?;
                    V::SHAPE.vtable.type_name()(f, opts)?;
                    write!(f, ">")
                } else {
                    write!(f, "{}<⋯>", Self::SHAPE.type_identifier)
                }
            })
            .default_in_place(|| Some(|target| unsafe { target.put(Self::default()) }))
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
