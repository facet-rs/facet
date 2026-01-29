//! Facet implementations for iddqd collection types.
//!
//! iddqd provides maps where keys are extracted from values:
//! - `IdHashMap<T, S>` - hash map with single key
//! - `IdOrdMap<T>` - ordered map (BTree-based)
//! - `BiHashMap<T, S>` - bijective map with two keys
//! - `TriHashMap<T, S>` - trijective map with three keys

#![cfg(feature = "iddqd")]

use alloc::boxed::Box;
use core::hash::BuildHasher;

use iddqd::{BiHashItem, BiHashMap, IdHashItem, IdHashMap, TriHashItem, TriHashMap};

// IdOrdMap requires std (uses thread-locals)
#[cfg(feature = "std")]
use iddqd::{IdOrdItem, IdOrdMap};

use crate::{
    Def, Facet, IterVTable, OxPtrMut, OxPtrUninit, PtrConst, PtrMut, PtrUninit, SetDef, SetVTable,
    Shape, ShapeBuilder, Type, TypeNameFn, TypeNameOpts, TypeOpsIndirect, TypeParam, UserType,
};

// =============================================================================
// IdHashMap<T, S>
// =============================================================================

type IdHashMapIterator<'mem, T> = iddqd::id_hash_map::Iter<'mem, T>;

unsafe fn id_hash_map_init_in_place_with_capacity<
    T: IdHashItem,
    S: Clone + Default + BuildHasher,
>(
    uninit: PtrUninit,
    capacity: usize,
) -> PtrMut {
    unsafe {
        uninit.put(IdHashMap::<T, S>::with_capacity_and_hasher(
            capacity,
            S::default(),
        ))
    }
}

unsafe fn id_hash_map_insert<T: IdHashItem, S: Clone + BuildHasher>(
    ptr: PtrMut,
    value: PtrMut,
) -> bool {
    let map = unsafe { ptr.as_mut::<IdHashMap<T, S>>() };
    let value = unsafe { value.read::<T>() };
    // insert_overwrite returns None if no conflict (value was new)
    map.insert_overwrite(value).is_none()
}

unsafe fn id_hash_map_len<T: IdHashItem, S: Clone + BuildHasher>(ptr: PtrConst) -> usize {
    unsafe { ptr.get::<IdHashMap<T, S>>().len() }
}

unsafe fn id_hash_map_contains<T: IdHashItem, S: Clone + BuildHasher>(
    ptr: PtrConst,
    value: PtrConst,
) -> bool {
    let map = unsafe { ptr.get::<IdHashMap<T, S>>() };
    let value = unsafe { value.get::<T>() };
    let key = value.key();
    map.contains_key(&key)
}

unsafe fn id_hash_map_iter_init<T: IdHashItem, S: Clone + BuildHasher>(ptr: PtrConst) -> PtrMut {
    unsafe {
        let map = ptr.get::<IdHashMap<T, S>>();
        let iter = map.iter();
        let iter_state = Box::new(iter);
        PtrMut::new(Box::into_raw(iter_state) as *mut u8)
    }
}

unsafe fn id_hash_map_iter_next<T: IdHashItem + 'static>(iter_ptr: PtrMut) -> Option<PtrConst> {
    unsafe {
        let state = iter_ptr.as_mut::<IdHashMapIterator<'static, T>>();
        state.next().map(|value| PtrConst::new(value as *const T))
    }
}

unsafe fn id_hash_map_iter_dealloc<T: IdHashItem + 'static>(iter_ptr: PtrMut) {
    unsafe {
        drop(Box::from_raw(
            iter_ptr.as_ptr::<IdHashMapIterator<'_, T>>() as *mut IdHashMapIterator<'_, T>
        ));
    }
}

unsafe fn id_hash_map_drop<T: IdHashItem, S>(ox: OxPtrMut) {
    unsafe {
        core::ptr::drop_in_place(ox.ptr().as_ptr::<IdHashMap<T, S>>() as *mut IdHashMap<T, S>);
    }
}

unsafe fn id_hash_map_default<T: IdHashItem, S: Clone + Default + BuildHasher>(ox: OxPtrUninit) {
    unsafe { ox.put(IdHashMap::<T, S>::default()) };
}

unsafe impl<'a, T, S> Facet<'a> for IdHashMap<T, S>
where
    T: Facet<'a> + IdHashItem + 'static,
    S: Facet<'a> + Clone + Default + BuildHasher + 'static,
{
    const SHAPE: &'static Shape = &const {
        const fn build_set_vtable<
            T: IdHashItem + 'static,
            S: Clone + Default + BuildHasher + 'static,
        >() -> SetVTable {
            SetVTable::builder()
                .init_in_place_with_capacity(id_hash_map_init_in_place_with_capacity::<T, S>)
                .insert(id_hash_map_insert::<T, S>)
                .len(id_hash_map_len::<T, S>)
                .contains(id_hash_map_contains::<T, S>)
                .iter_vtable(IterVTable {
                    init_with_value: Some(id_hash_map_iter_init::<T, S>),
                    next: id_hash_map_iter_next::<T>,
                    next_back: None,
                    size_hint: None,
                    dealloc: id_hash_map_iter_dealloc::<T>,
                })
                .build()
        }

        const fn build_type_name<'a, T: Facet<'a>>() -> TypeNameFn {
            fn type_name_impl<'a, T: Facet<'a>>(
                _shape: &'static Shape,
                f: &mut core::fmt::Formatter<'_>,
                opts: TypeNameOpts,
            ) -> core::fmt::Result {
                write!(f, "IdHashMap")?;
                if let Some(opts) = opts.for_children() {
                    write!(f, "<")?;
                    T::SHAPE.write_type_name(f, opts)?;
                    write!(f, ">")?;
                } else {
                    write!(f, "<\u{2026}>")?;
                }
                Ok(())
            }
            type_name_impl::<T>
        }

        ShapeBuilder::for_sized::<Self>("IdHashMap")
            .module_path("iddqd")
            .type_name(build_type_name::<T>())
            .ty(Type::User(UserType::Opaque))
            .def(Def::Set(SetDef::new(
                &const { build_set_vtable::<T, S>() },
                T::SHAPE,
            )))
            .type_params(&[TypeParam {
                name: "T",
                shape: T::SHAPE,
            }])
            .inner(T::SHAPE)
            .type_ops_indirect(
                &const {
                    TypeOpsIndirect {
                        drop_in_place: id_hash_map_drop::<T, S>,
                        default_in_place: Some(id_hash_map_default::<T, S>),
                        clone_into: None,
                        is_truthy: None,
                    }
                },
            )
            .build()
    };
}

// =============================================================================
// IdOrdMap<T> (requires std due to thread-local usage)
// =============================================================================

#[cfg(feature = "std")]
type IdOrdMapIterator<'mem, T> = iddqd::id_ord_map::Iter<'mem, T>;

#[cfg(feature = "std")]
unsafe fn id_ord_map_init_in_place_with_capacity<T: IdOrdItem>(
    uninit: PtrUninit,
    capacity: usize,
) -> PtrMut {
    unsafe { uninit.put(IdOrdMap::<T>::with_capacity(capacity)) }
}

#[cfg(feature = "std")]
unsafe fn id_ord_map_insert<T: IdOrdItem>(ptr: PtrMut, value: PtrMut) -> bool {
    let map = unsafe { ptr.as_mut::<IdOrdMap<T>>() };
    let value = unsafe { value.read::<T>() };
    map.insert_overwrite(value).is_none()
}

#[cfg(feature = "std")]
unsafe fn id_ord_map_len<T: IdOrdItem>(ptr: PtrConst) -> usize {
    unsafe { ptr.get::<IdOrdMap<T>>().len() }
}

#[cfg(feature = "std")]
unsafe fn id_ord_map_contains<T: IdOrdItem>(ptr: PtrConst, value: PtrConst) -> bool {
    let map = unsafe { ptr.get::<IdOrdMap<T>>() };
    let value = unsafe { value.get::<T>() };
    let key = value.key();
    map.contains_key(&key)
}

#[cfg(feature = "std")]
unsafe fn id_ord_map_iter_init<T: IdOrdItem>(ptr: PtrConst) -> PtrMut {
    unsafe {
        let map = ptr.get::<IdOrdMap<T>>();
        let iter = map.iter();
        let iter_state = Box::new(iter);
        PtrMut::new(Box::into_raw(iter_state) as *mut u8)
    }
}

#[cfg(feature = "std")]
unsafe fn id_ord_map_iter_next<T: IdOrdItem + 'static>(iter_ptr: PtrMut) -> Option<PtrConst> {
    unsafe {
        let state = iter_ptr.as_mut::<IdOrdMapIterator<'static, T>>();
        state.next().map(|value| PtrConst::new(value as *const T))
    }
}

#[cfg(feature = "std")]
unsafe fn id_ord_map_iter_dealloc<T: IdOrdItem + 'static>(iter_ptr: PtrMut) {
    unsafe {
        drop(Box::from_raw(
            iter_ptr.as_ptr::<IdOrdMapIterator<'_, T>>() as *mut IdOrdMapIterator<'_, T>
        ));
    }
}

#[cfg(feature = "std")]
unsafe fn id_ord_map_drop<T: IdOrdItem>(ox: OxPtrMut) {
    unsafe {
        core::ptr::drop_in_place(ox.ptr().as_ptr::<IdOrdMap<T>>() as *mut IdOrdMap<T>);
    }
}

#[cfg(feature = "std")]
unsafe fn id_ord_map_default<T: IdOrdItem>(ox: OxPtrUninit) {
    unsafe { ox.put(IdOrdMap::<T>::new()) };
}

#[cfg(feature = "std")]
unsafe impl<'a, T> Facet<'a> for IdOrdMap<T>
where
    T: Facet<'a> + IdOrdItem + 'static,
{
    const SHAPE: &'static Shape = &const {
        const fn build_set_vtable<T: IdOrdItem + 'static>() -> SetVTable {
            SetVTable::builder()
                .init_in_place_with_capacity(id_ord_map_init_in_place_with_capacity::<T>)
                .insert(id_ord_map_insert::<T>)
                .len(id_ord_map_len::<T>)
                .contains(id_ord_map_contains::<T>)
                .iter_vtable(IterVTable {
                    init_with_value: Some(id_ord_map_iter_init::<T>),
                    next: id_ord_map_iter_next::<T>,
                    next_back: None,
                    size_hint: None,
                    dealloc: id_ord_map_iter_dealloc::<T>,
                })
                .build()
        }

        const fn build_type_name<'a, T: Facet<'a>>() -> TypeNameFn {
            fn type_name_impl<'a, T: Facet<'a>>(
                _shape: &'static Shape,
                f: &mut core::fmt::Formatter<'_>,
                opts: TypeNameOpts,
            ) -> core::fmt::Result {
                write!(f, "IdOrdMap")?;
                if let Some(opts) = opts.for_children() {
                    write!(f, "<")?;
                    T::SHAPE.write_type_name(f, opts)?;
                    write!(f, ">")?;
                } else {
                    write!(f, "<\u{2026}>")?;
                }
                Ok(())
            }
            type_name_impl::<T>
        }

        ShapeBuilder::for_sized::<Self>("IdOrdMap")
            .module_path("iddqd")
            .type_name(build_type_name::<T>())
            .ty(Type::User(UserType::Opaque))
            .def(Def::Set(SetDef::new(
                &const { build_set_vtable::<T>() },
                T::SHAPE,
            )))
            .type_params(&[TypeParam {
                name: "T",
                shape: T::SHAPE,
            }])
            .inner(T::SHAPE)
            .type_ops_indirect(
                &const {
                    TypeOpsIndirect {
                        drop_in_place: id_ord_map_drop::<T>,
                        default_in_place: Some(id_ord_map_default::<T>),
                        clone_into: None,
                        is_truthy: None,
                    }
                },
            )
            .build()
    };
}

// =============================================================================
// BiHashMap<T, S>
// =============================================================================

type BiHashMapIterator<'mem, T> = iddqd::bi_hash_map::Iter<'mem, T>;

unsafe fn bi_hash_map_init_in_place_with_capacity<
    T: BiHashItem,
    S: Clone + Default + BuildHasher,
>(
    uninit: PtrUninit,
    capacity: usize,
) -> PtrMut {
    unsafe {
        uninit.put(BiHashMap::<T, S>::with_capacity_and_hasher(
            capacity,
            S::default(),
        ))
    }
}

unsafe fn bi_hash_map_insert<T: BiHashItem, S: Clone + BuildHasher>(
    ptr: PtrMut,
    value: PtrMut,
) -> bool {
    let map = unsafe { ptr.as_mut::<BiHashMap<T, S>>() };
    let value = unsafe { value.read::<T>() };
    // insert_overwrite returns Vec<T> of displaced items
    map.insert_overwrite(value).is_empty()
}

unsafe fn bi_hash_map_len<T: BiHashItem, S: Clone + BuildHasher>(ptr: PtrConst) -> usize {
    unsafe { ptr.get::<BiHashMap<T, S>>().len() }
}

unsafe fn bi_hash_map_contains<T: BiHashItem, S: Clone + BuildHasher>(
    ptr: PtrConst,
    value: PtrConst,
) -> bool {
    let map = unsafe { ptr.get::<BiHashMap<T, S>>() };
    let value = unsafe { value.get::<T>() };
    let key1 = value.key1();
    let key2 = value.key2();
    map.contains_key_unique(&key1, &key2)
}

unsafe fn bi_hash_map_iter_init<T: BiHashItem, S: Clone + BuildHasher>(ptr: PtrConst) -> PtrMut {
    unsafe {
        let map = ptr.get::<BiHashMap<T, S>>();
        let iter = map.iter();
        let iter_state = Box::new(iter);
        PtrMut::new(Box::into_raw(iter_state) as *mut u8)
    }
}

unsafe fn bi_hash_map_iter_next<T: BiHashItem + 'static>(iter_ptr: PtrMut) -> Option<PtrConst> {
    unsafe {
        let state = iter_ptr.as_mut::<BiHashMapIterator<'static, T>>();
        state.next().map(|value| PtrConst::new(value as *const T))
    }
}

unsafe fn bi_hash_map_iter_dealloc<T: BiHashItem + 'static>(iter_ptr: PtrMut) {
    unsafe {
        drop(Box::from_raw(
            iter_ptr.as_ptr::<BiHashMapIterator<'_, T>>() as *mut BiHashMapIterator<'_, T>
        ));
    }
}

unsafe fn bi_hash_map_drop<T: BiHashItem, S>(ox: OxPtrMut) {
    unsafe {
        core::ptr::drop_in_place(ox.ptr().as_ptr::<BiHashMap<T, S>>() as *mut BiHashMap<T, S>);
    }
}

unsafe fn bi_hash_map_default<T: BiHashItem, S: Clone + Default + BuildHasher>(ox: OxPtrUninit) {
    unsafe { ox.put(BiHashMap::<T, S>::default()) };
}

unsafe impl<'a, T, S> Facet<'a> for BiHashMap<T, S>
where
    T: Facet<'a> + BiHashItem + 'static,
    S: Facet<'a> + Clone + Default + BuildHasher + 'static,
{
    const SHAPE: &'static Shape = &const {
        const fn build_set_vtable<
            T: BiHashItem + 'static,
            S: Clone + Default + BuildHasher + 'static,
        >() -> SetVTable {
            SetVTable::builder()
                .init_in_place_with_capacity(bi_hash_map_init_in_place_with_capacity::<T, S>)
                .insert(bi_hash_map_insert::<T, S>)
                .len(bi_hash_map_len::<T, S>)
                .contains(bi_hash_map_contains::<T, S>)
                .iter_vtable(IterVTable {
                    init_with_value: Some(bi_hash_map_iter_init::<T, S>),
                    next: bi_hash_map_iter_next::<T>,
                    next_back: None,
                    size_hint: None,
                    dealloc: bi_hash_map_iter_dealloc::<T>,
                })
                .build()
        }

        const fn build_type_name<'a, T: Facet<'a>>() -> TypeNameFn {
            fn type_name_impl<'a, T: Facet<'a>>(
                _shape: &'static Shape,
                f: &mut core::fmt::Formatter<'_>,
                opts: TypeNameOpts,
            ) -> core::fmt::Result {
                write!(f, "BiHashMap")?;
                if let Some(opts) = opts.for_children() {
                    write!(f, "<")?;
                    T::SHAPE.write_type_name(f, opts)?;
                    write!(f, ">")?;
                } else {
                    write!(f, "<\u{2026}>")?;
                }
                Ok(())
            }
            type_name_impl::<T>
        }

        ShapeBuilder::for_sized::<Self>("BiHashMap")
            .module_path("iddqd")
            .type_name(build_type_name::<T>())
            .ty(Type::User(UserType::Opaque))
            .def(Def::Set(SetDef::new(
                &const { build_set_vtable::<T, S>() },
                T::SHAPE,
            )))
            .type_params(&[TypeParam {
                name: "T",
                shape: T::SHAPE,
            }])
            .inner(T::SHAPE)
            .type_ops_indirect(
                &const {
                    TypeOpsIndirect {
                        drop_in_place: bi_hash_map_drop::<T, S>,
                        default_in_place: Some(bi_hash_map_default::<T, S>),
                        clone_into: None,
                        is_truthy: None,
                    }
                },
            )
            .build()
    };
}

// =============================================================================
// TriHashMap<T, S>
// =============================================================================

type TriHashMapIterator<'mem, T> = iddqd::tri_hash_map::Iter<'mem, T>;

unsafe fn tri_hash_map_init_in_place_with_capacity<
    T: TriHashItem,
    S: Clone + Default + BuildHasher,
>(
    uninit: PtrUninit,
    capacity: usize,
) -> PtrMut {
    unsafe {
        uninit.put(TriHashMap::<T, S>::with_capacity_and_hasher(
            capacity,
            S::default(),
        ))
    }
}

unsafe fn tri_hash_map_insert<T: TriHashItem, S: Clone + BuildHasher>(
    ptr: PtrMut,
    value: PtrMut,
) -> bool {
    let map = unsafe { ptr.as_mut::<TriHashMap<T, S>>() };
    let value = unsafe { value.read::<T>() };
    // insert_overwrite returns Vec<T> of displaced items
    map.insert_overwrite(value).is_empty()
}

unsafe fn tri_hash_map_len<T: TriHashItem, S: Clone + BuildHasher>(ptr: PtrConst) -> usize {
    unsafe { ptr.get::<TriHashMap<T, S>>().len() }
}

unsafe fn tri_hash_map_contains<T: TriHashItem, S: Clone + BuildHasher>(
    ptr: PtrConst,
    value: PtrConst,
) -> bool {
    let map = unsafe { ptr.get::<TriHashMap<T, S>>() };
    let value = unsafe { value.get::<T>() };
    let key1 = value.key1();
    let key2 = value.key2();
    let key3 = value.key3();
    map.contains_key_unique(&key1, &key2, &key3)
}

unsafe fn tri_hash_map_iter_init<T: TriHashItem, S: Clone + BuildHasher>(ptr: PtrConst) -> PtrMut {
    unsafe {
        let map = ptr.get::<TriHashMap<T, S>>();
        let iter = map.iter();
        let iter_state = Box::new(iter);
        PtrMut::new(Box::into_raw(iter_state) as *mut u8)
    }
}

unsafe fn tri_hash_map_iter_next<T: TriHashItem + 'static>(iter_ptr: PtrMut) -> Option<PtrConst> {
    unsafe {
        let state = iter_ptr.as_mut::<TriHashMapIterator<'static, T>>();
        state.next().map(|value| PtrConst::new(value as *const T))
    }
}

unsafe fn tri_hash_map_iter_dealloc<T: TriHashItem + 'static>(iter_ptr: PtrMut) {
    unsafe {
        drop(Box::from_raw(
            iter_ptr.as_ptr::<TriHashMapIterator<'_, T>>() as *mut TriHashMapIterator<'_, T>,
        ));
    }
}

unsafe fn tri_hash_map_drop<T: TriHashItem, S>(ox: OxPtrMut) {
    unsafe {
        core::ptr::drop_in_place(ox.ptr().as_ptr::<TriHashMap<T, S>>() as *mut TriHashMap<T, S>);
    }
}

unsafe fn tri_hash_map_default<T: TriHashItem, S: Clone + Default + BuildHasher>(ox: OxPtrUninit) {
    unsafe { ox.put(TriHashMap::<T, S>::default()) };
}

unsafe impl<'a, T, S> Facet<'a> for TriHashMap<T, S>
where
    T: Facet<'a> + TriHashItem + 'static,
    S: Facet<'a> + Clone + Default + BuildHasher + 'static,
{
    const SHAPE: &'static Shape = &const {
        const fn build_set_vtable<
            T: TriHashItem + 'static,
            S: Clone + Default + BuildHasher + 'static,
        >() -> SetVTable {
            SetVTable::builder()
                .init_in_place_with_capacity(tri_hash_map_init_in_place_with_capacity::<T, S>)
                .insert(tri_hash_map_insert::<T, S>)
                .len(tri_hash_map_len::<T, S>)
                .contains(tri_hash_map_contains::<T, S>)
                .iter_vtable(IterVTable {
                    init_with_value: Some(tri_hash_map_iter_init::<T, S>),
                    next: tri_hash_map_iter_next::<T>,
                    next_back: None,
                    size_hint: None,
                    dealloc: tri_hash_map_iter_dealloc::<T>,
                })
                .build()
        }

        const fn build_type_name<'a, T: Facet<'a>>() -> TypeNameFn {
            fn type_name_impl<'a, T: Facet<'a>>(
                _shape: &'static Shape,
                f: &mut core::fmt::Formatter<'_>,
                opts: TypeNameOpts,
            ) -> core::fmt::Result {
                write!(f, "TriHashMap")?;
                if let Some(opts) = opts.for_children() {
                    write!(f, "<")?;
                    T::SHAPE.write_type_name(f, opts)?;
                    write!(f, ">")?;
                } else {
                    write!(f, "<\u{2026}>")?;
                }
                Ok(())
            }
            type_name_impl::<T>
        }

        ShapeBuilder::for_sized::<Self>("TriHashMap")
            .module_path("iddqd")
            .type_name(build_type_name::<T>())
            .ty(Type::User(UserType::Opaque))
            .def(Def::Set(SetDef::new(
                &const { build_set_vtable::<T, S>() },
                T::SHAPE,
            )))
            .type_params(&[TypeParam {
                name: "T",
                shape: T::SHAPE,
            }])
            .inner(T::SHAPE)
            .type_ops_indirect(
                &const {
                    TypeOpsIndirect {
                        drop_in_place: tri_hash_map_drop::<T, S>,
                        default_in_place: Some(tri_hash_map_default::<T, S>),
                        clone_into: None,
                        is_truthy: None,
                    }
                },
            )
            .build()
    };
}
