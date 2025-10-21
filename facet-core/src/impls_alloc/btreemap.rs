use core::{ptr::NonNull, write};

use alloc::{boxed::Box, collections::BTreeMap};

use crate::{
    Def, Facet, IterVTable, MapDef, MapVTable, MarkerTraits, PtrConst, PtrMut, Shape, Type,
    UserType, ValueVTable,
};

type BTreeMapIterator<'mem, K, V> = alloc::collections::btree_map::Iter<'mem, K, V>;

unsafe impl<'a, K, V> Facet<'a> for BTreeMap<K, V>
where
    K: Facet<'a> + core::cmp::Eq + core::cmp::Ord,
    V: Facet<'a>,
{
    const SHAPE: &'static crate::Shape = &const {
        Shape::builder_for_sized::<Self>()
            .vtable({
                ValueVTable::builder::<Self>()
                    .marker_traits(|| {
                        let arg_dependent_traits = MarkerTraits::SEND
                            .union(MarkerTraits::SYNC)
                            .union(MarkerTraits::EQ);
                        arg_dependent_traits
                            .intersection(V::SHAPE.vtable.marker_traits())
                            .intersection(K::SHAPE.vtable.marker_traits())
                            // only depends on `A` which we are not generic over (yet)
                            .union(MarkerTraits::UNPIN)
                    })
                    .type_name(|f, opts| {
                        if let Some(opts) = opts.for_children() {
                            write!(f, "{}<", Self::SHAPE.type_identifier)?;
                            K::SHAPE.vtable.type_name()(f, opts)?;
                            write!(f, ", ")?;
                            V::SHAPE.vtable.type_name()(f, opts)?;
                            write!(f, ">")
                        } else {
                            write!(f, "BTreeMap<⋯>")
                        }
                    })
                    .default_in_place(|| {
                        Some(|target| unsafe { target.put(Self::default()).into() })
                    })
                    .build()
            })
            .type_identifier("BTreeMap")
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
                                    map.get(key.get()).map(|v| PtrConst::new(NonNull::from(v)))
                                })
                                .iter_vtable(
                                    IterVTable::builder()
                                        .init_with_value(|ptr| unsafe {
                                            let map = ptr.get::<Self>();
                                            let iter: BTreeMapIterator<'_, K, V> = map.iter();
                                            let state = Box::new(iter);
                                            PtrMut::new(NonNull::new_unchecked(
                                                Box::into_raw(state) as *mut u8,
                                            ))
                                        })
                                        .next(|iter_ptr| unsafe {
                                            let state =
                                                iter_ptr.as_mut::<BTreeMapIterator<'_, K, V>>();
                                            state.next().map(|(key, value)| {
                                                (
                                                    PtrConst::new(NonNull::from(key)),
                                                    PtrConst::new(NonNull::from(value)),
                                                )
                                            })
                                        })
                                        .next_back(|iter_ptr| unsafe {
                                            let state =
                                                iter_ptr.as_mut::<BTreeMapIterator<'_, K, V>>();
                                            state.next_back().map(|(key, value)| {
                                                (
                                                    PtrConst::new(NonNull::from(key)),
                                                    PtrConst::new(NonNull::from(value)),
                                                )
                                            })
                                        })
                                        .dealloc(|iter_ptr| unsafe {
                                            drop(Box::from_raw(
                                                iter_ptr.as_ptr::<BTreeMapIterator<'_, K, V>>()
                                                    as *mut BTreeMapIterator<'_, K, V>,
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
