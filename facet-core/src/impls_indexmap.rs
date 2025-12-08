use alloc::boxed::Box;
use core::hash::BuildHasher;
use core::ptr::NonNull;
use indexmap::IndexMap;

use crate::ptr::{PtrConst, PtrMut};

use crate::{
    Def, Facet, IterVTable, MapDef, MapVTable, Shape, Type, TypeParam, UserType, ValueVTable,
    Variance,
};

type IndexMapIterator<'mem, K, V> = indexmap::map::Iter<'mem, K, V>;

unsafe impl<'a, K, V, S> Facet<'a> for IndexMap<K, V, S>
where
    K: Facet<'a> + core::cmp::Eq + core::hash::Hash,
    V: Facet<'a>,
    S: 'a + Default + BuildHasher,
{
    const SHAPE: &'static Shape = &const {
        Shape {
            id: Shape::id_of::<Self>(),
            layout: Shape::layout_of::<Self>(),
            vtable: ValueVTable {
                type_name: |f, opts| {
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
                drop_in_place: ValueVTable::drop_in_place_for::<Self>(),
                default_in_place: Some(|target| unsafe { target.put(Self::default()) }),
                ..ValueVTable::new(|_, _| Ok(()))
            },
            ty: Type::User(UserType::Opaque),
            def: Def::Map(MapDef::new(
                &const {
                    MapVTable {
                        init_in_place_with_capacity_fn: |uninit, capacity| unsafe {
                            uninit.put(Self::with_capacity_and_hasher(capacity, S::default()))
                        },
                        insert_fn: |ptr, key, value| unsafe {
                            let map = ptr.as_mut::<IndexMap<K, V, S>>();
                            let key = key.read::<K>();
                            let value = value.read::<V>();
                            map.insert(key, value);
                        },
                        len_fn: |ptr| unsafe {
                            let map = ptr.get::<IndexMap<K, V, S>>();
                            map.len()
                        },
                        contains_key_fn: |ptr, key| unsafe {
                            let map = ptr.get::<IndexMap<K, V, S>>();
                            map.contains_key(key.get::<K>())
                        },
                        get_value_ptr_fn: |ptr, key| unsafe {
                            let map = ptr.get::<IndexMap<K, V, S>>();
                            map.get(key.get::<K>())
                                .map(|v| PtrConst::new(NonNull::from(v)))
                        },
                        iter_vtable: IterVTable {
                            init_with_value: Some(|ptr| unsafe {
                                let map = ptr.get::<IndexMap<K, V, S>>();
                                let iter: IndexMapIterator<'_, K, V> = map.iter();
                                let iter_state = Box::new(iter);
                                PtrMut::new(NonNull::new_unchecked(
                                    Box::into_raw(iter_state) as *mut u8
                                ))
                            }),
                            next: |iter_ptr| unsafe {
                                let state = iter_ptr.as_mut::<IndexMapIterator<'_, K, V>>();
                                state.next().map(|(key, value)| {
                                    (
                                        PtrConst::new(NonNull::from(key)),
                                        PtrConst::new(NonNull::from(value)),
                                    )
                                })
                            },
                            next_back: None,
                            size_hint: None,
                            dealloc: |iter_ptr| unsafe {
                                drop(Box::from_raw(
                                    iter_ptr.as_ptr::<IndexMapIterator<'_, K, V>>()
                                        as *mut IndexMapIterator<'_, K, V>,
                                ));
                            },
                        },
                    }
                },
                K::SHAPE,
                V::SHAPE,
            )),
            type_identifier: "IndexMap",
            type_params: &[
                TypeParam {
                    name: "K",
                    shape: K::SHAPE,
                },
                TypeParam {
                    name: "V",
                    shape: V::SHAPE,
                },
            ],
            doc: &[],
            attributes: &[],
            type_tag: None,
            inner: None,
            proxy: None,
            // IndexMap<K, V, S> is covariant in K and V, but we use INVARIANT as a
            // safe conservative default since computed_variance doesn't yet support
            // multiple type parameters
            variance: Variance::INVARIANT,
        }
    };
}
