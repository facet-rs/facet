use alloc::boxed::Box;
use core::hash::BuildHasher;
use core::ptr::NonNull;
use indexmap::IndexMap;

use crate::ptr::{PtrConst, PtrMut};

use crate::{
    Def, Facet, IterVTable, MapDef, MapVTable, MarkerTraits, Shape, Type, TypeParam, UserType,
    ValueVTable,
};

type IndexMapIterator<'mem, K, V> = indexmap::map::Iter<'mem, K, V>;

unsafe impl<'a, K, V, S> Facet<'a> for IndexMap<K, V, S>
where
    K: Facet<'a> + core::cmp::Eq + core::hash::Hash,
    V: Facet<'a>,
    S: 'a + Default + BuildHasher,
{
    const SHAPE: &'static Shape = &const {
        Shape::builder_for_sized::<Self>()
            .vtable(
                ValueVTable::builder::<Self>()
                    .marker_traits({
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
                        write!(f, "{}<", Self::SHAPE.type_identifier)?;
                        if let Some(opts) = opts.for_children() {
                            K::SHAPE.vtable.type_name()(f, opts)?;
                            write!(f, ", ")?;
                            V::SHAPE.vtable.type_name()(f, opts)?;
                        } else {
                            write!(f, "…")?;
                        }
                        write!(f, ">")
                    })
                    .default_in_place({
                        Some(|target| unsafe { target.put(Self::default()).into() })
                    })
                    .build(),
            )
            .type_identifier("IndexMap")
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
            .ty(Type::User(UserType::Opaque))
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
                                    let map = ptr.as_mut::<IndexMap<K, V, S>>();
                                    let key = key.read::<K>();
                                    let value = value.read::<V>();
                                    map.insert(key, value);
                                })
                                .len(|ptr| unsafe {
                                    let map = ptr.get::<IndexMap<K, V, S>>();
                                    map.len()
                                })
                                .contains_key(|ptr, key| unsafe {
                                    let map = ptr.get::<IndexMap<K, V, S>>();
                                    map.contains_key(key.get::<K>())
                                })
                                .get_value_ptr(|ptr, key| unsafe {
                                    let map = ptr.get::<IndexMap<K, V, S>>();
                                    map.get(key.get::<K>())
                                        .map(|v| PtrConst::new(NonNull::from(v)))
                                })
                                .iter_vtable(
                                    IterVTable::builder()
                                        .init_with_value(|ptr| unsafe {
                                            let map = ptr.get::<IndexMap<K, V, S>>();
                                            let iter: IndexMapIterator<'_, K, V> = map.iter();
                                            let iter_state = Box::new(iter);
                                            PtrMut::new(NonNull::new_unchecked(Box::into_raw(
                                                iter_state,
                                            )
                                                as *mut u8))
                                        })
                                        .next(|iter_ptr| unsafe {
                                            let state =
                                                iter_ptr.as_mut::<IndexMapIterator<'_, K, V>>();
                                            state.next().map(|(key, value)| {
                                                (
                                                    PtrConst::new(NonNull::from(key)),
                                                    PtrConst::new(NonNull::from(value)),
                                                )
                                            })
                                        })
                                        .dealloc(|iter_ptr| unsafe {
                                            drop(Box::from_raw(
                                                iter_ptr.as_ptr::<IndexMapIterator<'_, K, V>>()
                                                    as *mut IndexMapIterator<'_, K, V>,
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
