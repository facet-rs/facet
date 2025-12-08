use crate::Variance;
use core::hash::BuildHasher;
use core::ptr::NonNull;
use std::collections::HashMap;
use std::hash::RandomState;

use crate::ptr::{PtrConst, PtrMut};

use crate::{
    Def, Facet, IterVTable, MapDef, MapVTable, Shape, Type, TypeParam, UserType, ValueVTable,
    value_vtable,
};

type HashMapIterator<'mem, K, V> = std::collections::hash_map::Iter<'mem, K, V>;

// TODO: Debug, PartialEq, Eq for HashMap, HashSet
unsafe impl<'a, K, V, S> Facet<'a> for HashMap<K, V, S>
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
            def: Def::Map(MapDef {
                vtable: &const {
                    MapVTable {
                        init_in_place_with_capacity_fn: |uninit, capacity| unsafe {
                            uninit.put(Self::with_capacity_and_hasher(capacity, S::default()))
                        },
                        insert_fn: |ptr, key, value| unsafe {
                            let map = ptr.as_mut::<HashMap<K, V>>();
                            let key = key.read::<K>();
                            let value = value.read::<V>();
                            map.insert(key, value);
                        },
                        len_fn: |ptr| unsafe {
                            let map = ptr.get::<HashMap<K, V>>();
                            map.len()
                        },
                        contains_key_fn: |ptr, key| unsafe {
                            let map = ptr.get::<HashMap<K, V>>();
                            map.contains_key(key.get())
                        },
                        get_value_ptr_fn: |ptr, key| unsafe {
                            let map = ptr.get::<HashMap<K, V>>();
                            map.get(key.get()).map(|v| PtrConst::new(NonNull::from(v)))
                        },
                        iter_vtable: IterVTable {
                            init_with_value: Some(|ptr| unsafe {
                                let map = ptr.get::<HashMap<K, V>>();
                                let iter: HashMapIterator<'_, K, V> = map.iter();
                                let iter_state = Box::new(iter);
                                PtrMut::new(NonNull::new_unchecked(
                                    Box::into_raw(iter_state) as *mut u8
                                ))
                            }),
                            next: |iter_ptr| unsafe {
                                let state = iter_ptr.as_mut::<HashMapIterator<'_, K, V>>();
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
                                    iter_ptr.as_ptr::<HashMapIterator<'_, K, V>>()
                                        as *mut HashMapIterator<'_, K, V>,
                                ));
                            },
                        },
                    }
                },
                k: K::SHAPE,
                v: V::SHAPE,
            }),
            type_identifier: "HashMap",
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
            variance: Variance::Invariant,
        }
    };
}

unsafe impl Facet<'_> for RandomState {
    const SHAPE: &'static Shape = &const {
        Shape {
            id: Shape::id_of::<Self>(),
            layout: Shape::layout_of::<Self>(),
            vtable: value_vtable!((), |f, _opts| write!(f, "{}", Self::SHAPE.type_identifier)),
            ty: Type::User(UserType::Opaque),
            def: Def::Scalar,
            type_identifier: "RandomState",
            type_params: &[],
            doc: &[],
            attributes: &[],
            type_tag: None,
            inner: None,
            proxy: None,
            variance: Variance::Invariant,
        }
    };
}
