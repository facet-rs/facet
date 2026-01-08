//! Facet implementations for `std::sync` lock types.

use core::ptr::NonNull;
use std::sync::{Mutex, MutexGuard, RwLock, RwLockReadGuard, RwLockWriteGuard};

use crate::{
    Def, Facet, KnownPointer, LockGuardVTable, LockResult, OxPtrMut, PointerDef, PointerFlags,
    PointerVTable, PtrConst, PtrMut, PtrUninit, Shape, ShapeBuilder, Type, TypeNameOpts,
    TypeOpsIndirect, TypeParam, UserType, VTableIndirect, Variance, VarianceDep, VarianceDesc,
};

// ============================================================================
// Mutex<T> Implementation
// ============================================================================

fn type_name_mutex<'a, T: Facet<'a>>(
    _shape: &'static Shape,
    f: &mut core::fmt::Formatter<'_>,
    opts: TypeNameOpts,
) -> core::fmt::Result {
    write!(f, "Mutex")?;
    if let Some(opts) = opts.for_children() {
        write!(f, "<")?;
        T::SHAPE.write_type_name(f, opts)?;
        write!(f, ">")?;
    } else {
        write!(f, "<\u{2026}>")?;
    }
    Ok(())
}

unsafe fn mutex_new_into<T>(this: PtrUninit, value: PtrMut) -> PtrMut {
    unsafe {
        let t = value.read::<T>();
        this.put(Mutex::<T>::new(t))
    }
}

unsafe fn mutex_drop<T>(ox: OxPtrMut) {
    unsafe { core::ptr::drop_in_place(ox.ptr().as_ptr::<Mutex<T>>() as *mut Mutex<T>) }
}

unsafe fn mutex_lock<'a, T: Facet<'a>>(opaque: PtrConst) -> Result<LockResult, ()> {
    unsafe {
        let mutex = &*opaque.as_ptr::<Mutex<T>>();
        // Handle PoisonError by returning Err(())
        let guard = mutex.lock().map_err(|_| ())?;

        // Get pointer to the data through the guard
        let data_ptr = &*guard as *const T as *mut T;

        // Box the guard to keep it alive (type-erased)
        let guard_box = Box::new(guard);
        let guard_ptr = Box::into_raw(guard_box) as *const u8;

        Ok(LockResult::new(
            PtrMut::new(data_ptr as *mut u8),
            PtrConst::new(guard_ptr),
            &const { mutex_guard_vtable::<T>() },
        ))
    }
}

const fn mutex_guard_vtable<T>() -> LockGuardVTable {
    unsafe fn drop_guard<T>(guard: PtrConst) {
        unsafe {
            drop(Box::from_raw(
                guard.as_ptr::<MutexGuard<'_, T>>() as *mut MutexGuard<'_, T>
            ));
        }
    }

    LockGuardVTable {
        drop_in_place: drop_guard::<T>,
    }
}

unsafe impl<'a, T: Facet<'a>> Facet<'a> for Mutex<T> {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<Self>("Mutex")
            .module_path("std::sync")
            .type_name(type_name_mutex::<T>)
            .vtable_indirect(&VTableIndirect::EMPTY)
            .type_ops_indirect(
                &const {
                    TypeOpsIndirect {
                        drop_in_place: mutex_drop::<T>,
                        default_in_place: None,
                        clone_into: None,
                        is_truthy: None,
                    }
                },
            )
            .ty(Type::User(UserType::Opaque))
            .def(Def::Pointer(PointerDef {
                vtable: &const {
                    PointerVTable {
                        lock_fn: Some(mutex_lock::<T>),
                        new_into_fn: Some(mutex_new_into::<T>),
                        ..PointerVTable::new()
                    }
                },
                pointee: Some(T::SHAPE),
                weak: None,
                strong: None,
                flags: PointerFlags::LOCK,
                known: Some(KnownPointer::Mutex),
            }))
            .type_params(&[TypeParam {
                name: "T",
                shape: T::SHAPE,
            }])
            .inner(T::SHAPE)
            // Mutex<T> is invariant w.r.t. T because it provides interior mutability
            .variance(VarianceDesc {
                base: Variance::Bivariant,
                deps: &const { [VarianceDep::invariant(T::SHAPE)] },
            })
            .build()
    };
}

// ============================================================================
// RwLock<T> Implementation
// ============================================================================

fn type_name_rwlock<'a, T: Facet<'a>>(
    _shape: &'static Shape,
    f: &mut core::fmt::Formatter<'_>,
    opts: TypeNameOpts,
) -> core::fmt::Result {
    write!(f, "RwLock")?;
    if let Some(opts) = opts.for_children() {
        write!(f, "<")?;
        T::SHAPE.write_type_name(f, opts)?;
        write!(f, ">")?;
    } else {
        write!(f, "<\u{2026}>")?;
    }
    Ok(())
}

unsafe fn rwlock_new_into<T>(this: PtrUninit, value: PtrMut) -> PtrMut {
    unsafe {
        let t = value.read::<T>();
        this.put(RwLock::<T>::new(t))
    }
}

unsafe fn rwlock_drop<T>(ox: OxPtrMut) {
    unsafe { core::ptr::drop_in_place(ox.ptr().as_ptr::<RwLock<T>>() as *mut RwLock<T>) }
}

const fn rwlock_read_guard_vtable<T>() -> LockGuardVTable {
    unsafe fn drop_guard<T>(guard: PtrConst) {
        unsafe {
            drop(Box::from_raw(
                guard.as_ptr::<RwLockReadGuard<'_, T>>() as *mut RwLockReadGuard<'_, T>
            ));
        }
    }

    LockGuardVTable {
        drop_in_place: drop_guard::<T>,
    }
}

const fn rwlock_write_guard_vtable<T>() -> LockGuardVTable {
    unsafe fn drop_guard<T>(guard: PtrConst) {
        unsafe {
            drop(Box::from_raw(
                guard.as_ptr::<RwLockWriteGuard<'_, T>>() as *mut RwLockWriteGuard<'_, T>
            ));
        }
    }

    LockGuardVTable {
        drop_in_place: drop_guard::<T>,
    }
}

unsafe fn rwlock_read<'a, T: Facet<'a>>(opaque: PtrConst) -> Result<LockResult, ()> {
    unsafe {
        let rwlock = &*opaque.as_ptr::<RwLock<T>>();
        // Handle PoisonError by returning Err(())
        let guard = rwlock.read().map_err(|_| ())?;
        let data_ptr = &*guard as *const T;
        let guard_box = Box::new(guard);
        let guard_ptr = Box::into_raw(guard_box) as *const u8;

        Ok(LockResult::new(
            PtrMut::new(data_ptr as *mut u8),
            PtrConst::new(guard_ptr),
            &const { rwlock_read_guard_vtable::<T>() },
        ))
    }
}

unsafe fn rwlock_write<'a, T: Facet<'a>>(opaque: PtrConst) -> Result<LockResult, ()> {
    unsafe {
        let rwlock = &*opaque.as_ptr::<RwLock<T>>();
        // Handle PoisonError by returning Err(())
        let guard = rwlock.write().map_err(|_| ())?;
        let data_ptr = &*guard as *const T as *mut T;
        let guard_box = Box::new(guard);
        let guard_ptr = Box::into_raw(guard_box) as *const u8;

        Ok(LockResult::new(
            PtrMut::new(data_ptr as *mut u8),
            PtrConst::new(guard_ptr),
            &const { rwlock_write_guard_vtable::<T>() },
        ))
    }
}

unsafe impl<'a, T: Facet<'a>> Facet<'a> for RwLock<T> {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<Self>("RwLock")
            .module_path("std::sync")
            .type_name(type_name_rwlock::<T>)
            .vtable_indirect(&VTableIndirect::EMPTY)
            .type_ops_indirect(
                &const {
                    TypeOpsIndirect {
                        drop_in_place: rwlock_drop::<T>,
                        default_in_place: None,
                        clone_into: None,
                        is_truthy: None,
                    }
                },
            )
            .ty(Type::User(UserType::Opaque))
            .def(Def::Pointer(PointerDef {
                vtable: &const {
                    PointerVTable {
                        read_fn: Some(rwlock_read::<T>),
                        write_fn: Some(rwlock_write::<T>),
                        new_into_fn: Some(rwlock_new_into::<T>),
                        ..PointerVTable::new()
                    }
                },
                pointee: Some(T::SHAPE),
                weak: None,
                strong: None,
                flags: PointerFlags::LOCK,
                known: Some(KnownPointer::RwLock),
            }))
            .type_params(&[TypeParam {
                name: "T",
                shape: T::SHAPE,
            }])
            .inner(T::SHAPE)
            // RwLock<T> is invariant w.r.t. T because it provides interior mutability
            .variance(VarianceDesc {
                base: Variance::Bivariant,
                deps: &const { [VarianceDep::invariant(T::SHAPE)] },
            })
            .build()
    };
}

// ============================================================================
// MutexGuard<'a, T> Implementation
// ============================================================================

fn type_name_mutex_guard<'a, T: Facet<'a>>(
    _shape: &'static Shape,
    f: &mut core::fmt::Formatter<'_>,
    opts: TypeNameOpts,
) -> core::fmt::Result {
    write!(f, "MutexGuard")?;
    if let Some(opts) = opts.for_children() {
        write!(f, "<")?;
        T::SHAPE.write_type_name(f, opts)?;
        write!(f, ">")?;
    } else {
        write!(f, "<\u{2026}>")?;
    }
    Ok(())
}

unsafe fn mutex_guard_borrow<'a, T: Facet<'a>>(this: PtrConst) -> PtrConst {
    unsafe {
        let guard = this.get::<MutexGuard<'_, T>>();
        let data: &T = guard;
        PtrConst::new(NonNull::from(data).as_ptr())
    }
}

unsafe fn mutex_guard_drop_impl<T>(ox: OxPtrMut) {
    unsafe {
        core::ptr::drop_in_place(ox.ptr().as_ptr::<MutexGuard<'_, T>>() as *mut MutexGuard<'_, T>)
    }
}

unsafe impl<'a, T: Facet<'a>> Facet<'a> for MutexGuard<'a, T> {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<Self>("MutexGuard")
            .module_path("std::sync")
            .type_name(type_name_mutex_guard::<T>)
            .vtable_indirect(&VTableIndirect::EMPTY)
            .type_ops_indirect(
                &const {
                    TypeOpsIndirect {
                        drop_in_place: mutex_guard_drop_impl::<T>,
                        default_in_place: None,
                        clone_into: None,
                        is_truthy: None,
                    }
                },
            )
            .ty(Type::User(UserType::Opaque))
            .def(Def::Pointer(PointerDef {
                vtable: &const {
                    PointerVTable {
                        borrow_fn: Some(mutex_guard_borrow::<T>),
                        ..PointerVTable::new()
                    }
                },
                pointee: Some(T::SHAPE),
                weak: None,
                strong: None,
                flags: PointerFlags::EMPTY,
                known: None,
            }))
            .type_params(&[TypeParam {
                name: "T",
                shape: T::SHAPE,
            }])
            .inner(T::SHAPE)
            // MutexGuard<T> is invariant w.r.t. T (provides mutable access)
            .variance(VarianceDesc {
                base: Variance::Bivariant,
                deps: &const { [VarianceDep::invariant(T::SHAPE)] },
            })
            .build()
    };
}

// ============================================================================
// RwLockReadGuard<'a, T> Implementation
// ============================================================================

fn type_name_rwlock_read_guard<'a, T: Facet<'a>>(
    _shape: &'static Shape,
    f: &mut core::fmt::Formatter<'_>,
    opts: TypeNameOpts,
) -> core::fmt::Result {
    write!(f, "RwLockReadGuard")?;
    if let Some(opts) = opts.for_children() {
        write!(f, "<")?;
        T::SHAPE.write_type_name(f, opts)?;
        write!(f, ">")?;
    } else {
        write!(f, "<\u{2026}>")?;
    }
    Ok(())
}

unsafe fn rwlock_read_guard_borrow<'a, T: Facet<'a>>(this: PtrConst) -> PtrConst {
    unsafe {
        let guard = this.get::<RwLockReadGuard<'_, T>>();
        let data: &T = guard;
        PtrConst::new(NonNull::from(data).as_ptr())
    }
}

unsafe fn rwlock_read_guard_drop_impl<T>(ox: OxPtrMut) {
    unsafe {
        core::ptr::drop_in_place(
            ox.ptr().as_ptr::<RwLockReadGuard<'_, T>>() as *mut RwLockReadGuard<'_, T>
        )
    }
}

unsafe impl<'a, T: Facet<'a>> Facet<'a> for RwLockReadGuard<'a, T> {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<Self>("RwLockReadGuard")
            .module_path("std::sync")
            .type_name(type_name_rwlock_read_guard::<T>)
            .vtable_indirect(&VTableIndirect::EMPTY)
            .type_ops_indirect(
                &const {
                    TypeOpsIndirect {
                        drop_in_place: rwlock_read_guard_drop_impl::<T>,
                        default_in_place: None,
                        clone_into: None,
                        is_truthy: None,
                    }
                },
            )
            .ty(Type::User(UserType::Opaque))
            .def(Def::Pointer(PointerDef {
                vtable: &const {
                    PointerVTable {
                        borrow_fn: Some(rwlock_read_guard_borrow::<T>),
                        ..PointerVTable::new()
                    }
                },
                pointee: Some(T::SHAPE),
                weak: None,
                strong: None,
                flags: PointerFlags::EMPTY,
                known: None,
            }))
            .type_params(&[TypeParam {
                name: "T",
                shape: T::SHAPE,
            }])
            .inner(T::SHAPE)
            // RwLockReadGuard<T> is covariant w.r.t. T (only provides shared access)
            .variance(VarianceDesc {
                base: Variance::Bivariant,
                deps: &const { [VarianceDep::covariant(T::SHAPE)] },
            })
            .build()
    };
}

// ============================================================================
// RwLockWriteGuard<'a, T> Implementation
// ============================================================================

fn type_name_rwlock_write_guard<'a, T: Facet<'a>>(
    _shape: &'static Shape,
    f: &mut core::fmt::Formatter<'_>,
    opts: TypeNameOpts,
) -> core::fmt::Result {
    write!(f, "RwLockWriteGuard")?;
    if let Some(opts) = opts.for_children() {
        write!(f, "<")?;
        T::SHAPE.write_type_name(f, opts)?;
        write!(f, ">")?;
    } else {
        write!(f, "<\u{2026}>")?;
    }
    Ok(())
}

unsafe fn rwlock_write_guard_borrow<'a, T: Facet<'a>>(this: PtrConst) -> PtrConst {
    unsafe {
        let guard = this.get::<RwLockWriteGuard<'_, T>>();
        let data: &T = guard;
        PtrConst::new(NonNull::from(data).as_ptr())
    }
}

unsafe fn rwlock_write_guard_drop_impl<T>(ox: OxPtrMut) {
    unsafe {
        core::ptr::drop_in_place(
            ox.ptr().as_ptr::<RwLockWriteGuard<'_, T>>() as *mut RwLockWriteGuard<'_, T>
        )
    }
}

unsafe impl<'a, T: Facet<'a>> Facet<'a> for RwLockWriteGuard<'a, T> {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<Self>("RwLockWriteGuard")
            .module_path("std::sync")
            .type_name(type_name_rwlock_write_guard::<T>)
            .vtable_indirect(&VTableIndirect::EMPTY)
            .type_ops_indirect(
                &const {
                    TypeOpsIndirect {
                        drop_in_place: rwlock_write_guard_drop_impl::<T>,
                        default_in_place: None,
                        clone_into: None,
                        is_truthy: None,
                    }
                },
            )
            .ty(Type::User(UserType::Opaque))
            .def(Def::Pointer(PointerDef {
                vtable: &const {
                    PointerVTable {
                        borrow_fn: Some(rwlock_write_guard_borrow::<T>),
                        ..PointerVTable::new()
                    }
                },
                pointee: Some(T::SHAPE),
                weak: None,
                strong: None,
                flags: PointerFlags::EMPTY,
                known: None,
            }))
            .type_params(&[TypeParam {
                name: "T",
                shape: T::SHAPE,
            }])
            .inner(T::SHAPE)
            // RwLockWriteGuard<T> is invariant w.r.t. T (provides mutable access)
            .variance(VarianceDesc {
                base: Variance::Bivariant,
                deps: &const { [VarianceDep::invariant(T::SHAPE)] },
            })
            .build()
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Shape verification tests
    // ========================================================================

    #[test]
    fn test_mutex_shape() {
        facet_testhelpers::setup();

        let shape = <Mutex<i32>>::SHAPE;
        assert_eq!(shape.type_identifier, "Mutex");

        // Verify type params
        let [type_param] = shape.type_params else {
            panic!("Mutex should have 1 type param");
        };
        assert_eq!(type_param.name, "T");
        assert_eq!(type_param.shape, i32::SHAPE);
    }

    #[test]
    fn test_rwlock_shape() {
        facet_testhelpers::setup();

        let shape = <RwLock<String>>::SHAPE;
        assert_eq!(shape.type_identifier, "RwLock");

        // Verify type params
        let [type_param] = shape.type_params else {
            panic!("RwLock should have 1 type param");
        };
        assert_eq!(type_param.name, "T");
        assert_eq!(type_param.shape, String::SHAPE);
    }

    #[test]
    fn test_mutex_guard_shape() {
        facet_testhelpers::setup();

        let shape = <MutexGuard<'_, i32>>::SHAPE;
        assert_eq!(shape.type_identifier, "MutexGuard");

        let [type_param] = shape.type_params else {
            panic!("MutexGuard should have 1 type param");
        };
        assert_eq!(type_param.name, "T");
    }

    #[test]
    fn test_rwlock_read_guard_shape() {
        facet_testhelpers::setup();

        let shape = <RwLockReadGuard<'_, i32>>::SHAPE;
        assert_eq!(shape.type_identifier, "RwLockReadGuard");

        let [type_param] = shape.type_params else {
            panic!("RwLockReadGuard should have 1 type param");
        };
        assert_eq!(type_param.name, "T");
    }

    #[test]
    fn test_rwlock_write_guard_shape() {
        facet_testhelpers::setup();

        let shape = <RwLockWriteGuard<'_, i32>>::SHAPE;
        assert_eq!(shape.type_identifier, "RwLockWriteGuard");

        let [type_param] = shape.type_params else {
            panic!("RwLockWriteGuard should have 1 type param");
        };
        assert_eq!(type_param.name, "T");
    }

    // ========================================================================
    // VTable presence tests
    // ========================================================================

    #[test]
    fn test_mutex_vtable() {
        facet_testhelpers::setup();

        let shape = <Mutex<i32>>::SHAPE;
        let pointer_def = shape
            .def
            .into_pointer()
            .expect("Mutex should be a pointer type");

        // Mutex should have lock_fn and new_into_fn
        assert!(
            pointer_def.vtable.lock_fn.is_some(),
            "Mutex should have lock_fn"
        );
        assert!(
            pointer_def.vtable.new_into_fn.is_some(),
            "Mutex should have new_into_fn"
        );

        // Mutex should NOT have read_fn or write_fn (those are for RwLock)
        assert!(
            pointer_def.vtable.read_fn.is_none(),
            "Mutex should not have read_fn"
        );
        assert!(
            pointer_def.vtable.write_fn.is_none(),
            "Mutex should not have write_fn"
        );

        // Verify flags
        assert!(
            pointer_def.flags.contains(PointerFlags::LOCK),
            "Mutex should have LOCK flag"
        );
        assert_eq!(pointer_def.known, Some(KnownPointer::Mutex));
    }

    #[test]
    fn test_rwlock_vtable() {
        facet_testhelpers::setup();

        let shape = <RwLock<i32>>::SHAPE;
        let pointer_def = shape
            .def
            .into_pointer()
            .expect("RwLock should be a pointer type");

        // RwLock should have read_fn, write_fn, and new_into_fn
        assert!(
            pointer_def.vtable.read_fn.is_some(),
            "RwLock should have read_fn"
        );
        assert!(
            pointer_def.vtable.write_fn.is_some(),
            "RwLock should have write_fn"
        );
        assert!(
            pointer_def.vtable.new_into_fn.is_some(),
            "RwLock should have new_into_fn"
        );

        // RwLock should NOT have lock_fn (that's for Mutex)
        assert!(
            pointer_def.vtable.lock_fn.is_none(),
            "RwLock should not have lock_fn"
        );

        // Verify flags
        assert!(
            pointer_def.flags.contains(PointerFlags::LOCK),
            "RwLock should have LOCK flag"
        );
        assert_eq!(pointer_def.known, Some(KnownPointer::RwLock));
    }

    #[test]
    fn test_guard_vtables_have_borrow_fn() {
        facet_testhelpers::setup();

        // MutexGuard
        let mutex_guard_shape = <MutexGuard<'_, i32>>::SHAPE;
        let mutex_guard_def = mutex_guard_shape
            .def
            .into_pointer()
            .expect("MutexGuard should be a pointer type");
        assert!(
            mutex_guard_def.vtable.borrow_fn.is_some(),
            "MutexGuard should have borrow_fn"
        );

        // RwLockReadGuard
        let read_guard_shape = <RwLockReadGuard<'_, i32>>::SHAPE;
        let read_guard_def = read_guard_shape
            .def
            .into_pointer()
            .expect("RwLockReadGuard should be a pointer type");
        assert!(
            read_guard_def.vtable.borrow_fn.is_some(),
            "RwLockReadGuard should have borrow_fn"
        );

        // RwLockWriteGuard
        let write_guard_shape = <RwLockWriteGuard<'_, i32>>::SHAPE;
        let write_guard_def = write_guard_shape
            .def
            .into_pointer()
            .expect("RwLockWriteGuard should be a pointer type");
        assert!(
            write_guard_def.vtable.borrow_fn.is_some(),
            "RwLockWriteGuard should have borrow_fn"
        );
    }

    // ========================================================================
    // Functional tests
    // ========================================================================

    #[test]
    fn test_mutex_lock_and_access() {
        facet_testhelpers::setup();

        let mutex = Mutex::new(42i32);

        // Get the shape and pointer def
        let shape = <Mutex<i32>>::SHAPE;
        let pointer_def = shape.def.into_pointer().unwrap();
        let lock_fn = pointer_def.vtable.lock_fn.unwrap();

        // Lock the mutex using the vtable
        let mutex_ptr = PtrConst::new(&mutex as *const _ as *const u8);
        let lock_result = unsafe { lock_fn(mutex_ptr) }.expect("Lock should succeed");

        // Access the data through the lock result
        let data_ptr = lock_result.data();
        let value = unsafe { data_ptr.as_const().get::<i32>() };
        assert_eq!(*value, 42);

        // Lock is released when lock_result is dropped
        drop(lock_result);

        // Verify we can lock again (proves the lock was released)
        let lock_result2 = unsafe { lock_fn(mutex_ptr) }.expect("Second lock should succeed");
        drop(lock_result2);
    }

    #[test]
    fn test_rwlock_read_access() {
        facet_testhelpers::setup();

        let rwlock = RwLock::new(String::from("hello"));

        let shape = <RwLock<String>>::SHAPE;
        let pointer_def = shape.def.into_pointer().unwrap();
        let read_fn = pointer_def.vtable.read_fn.unwrap();

        let rwlock_ptr = PtrConst::new(&rwlock as *const _ as *const u8);

        // Acquire read lock
        let read_result = unsafe { read_fn(rwlock_ptr) }.expect("Read lock should succeed");

        // Access the data
        let data_ptr = read_result.data();
        let value = unsafe { data_ptr.as_const().get::<String>() };
        assert_eq!(value.as_str(), "hello");

        drop(read_result);
    }

    #[test]
    fn test_rwlock_write_access() {
        facet_testhelpers::setup();

        let rwlock = RwLock::new(100i32);

        let shape = <RwLock<i32>>::SHAPE;
        let pointer_def = shape.def.into_pointer().unwrap();
        let write_fn = pointer_def.vtable.write_fn.unwrap();

        let rwlock_ptr = PtrConst::new(&rwlock as *const _ as *const u8);

        // Acquire write lock
        let write_result = unsafe { write_fn(rwlock_ptr) }.expect("Write lock should succeed");

        // Modify the data through the lock
        let data_ptr = write_result.data();
        unsafe {
            *data_ptr.as_mut_ptr::<i32>() = 200;
        }

        drop(write_result);

        // Verify the modification persisted
        let read_fn = pointer_def.vtable.read_fn.unwrap();
        let read_result = unsafe { read_fn(rwlock_ptr) }.expect("Read lock should succeed");
        let value = unsafe { read_result.data().as_const().get::<i32>() };
        assert_eq!(*value, 200);
    }

    #[test]
    fn test_mutex_exclusive_access() {
        facet_testhelpers::setup();

        // This test verifies the basic mutex behavior (exclusive access)
        let mutex = Mutex::new(42i32);

        let shape = <Mutex<i32>>::SHAPE;
        let pointer_def = shape.def.into_pointer().unwrap();
        let lock_fn = pointer_def.vtable.lock_fn.unwrap();

        let mutex_ptr = PtrConst::new(&mutex as *const _ as *const u8);

        // First lock
        let lock1 = unsafe { lock_fn(mutex_ptr) }.expect("First lock should succeed");

        // The mutex is now held - in a single-threaded test, we can't test blocking,
        // but we can verify the lock was acquired and data is accessible
        let value = unsafe { lock1.data().as_const().get::<i32>() };
        assert_eq!(*value, 42);

        // Release the lock
        drop(lock1);
    }
}
