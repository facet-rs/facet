//! Facet implementation for Result<T, E>

use core::cmp::Ordering;

use crate::{
    Def, Facet, HashProxy, OxPtrConst, OxPtrMut, OxRef, PtrConst, PtrMut, ResultDef, ResultVTable,
    Shape, ShapeBuilder, Type, TypeOpsIndirect, TypeParam, UserType, VTableIndirect,
};

/// Extract the ResultDef from a shape, returns None if not a Result
#[inline]
fn get_result_def(shape: &'static Shape) -> Option<&'static ResultDef> {
    match shape.def {
        Def::Result(ref def) => Some(def),
        _ => None,
    }
}

/// Debug for Result<T, E> - delegates to inner T/E's debug if available
unsafe fn result_debug(
    ox: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    let shape = ox.shape();
    let def = get_result_def(shape)?;
    let ptr = ox.ptr();

    if unsafe { (def.vtable.is_ok)(ptr) } {
        // SAFETY: is_ok returned true, so get_ok returns a valid pointer.
        // The caller guarantees the OxPtrConst points to a valid Result.
        let ok_ptr = unsafe { (def.vtable.get_ok)(ptr)? };
        let ok_ox = unsafe { OxRef::new(ok_ptr, def.t) };
        Some(f.debug_tuple("Ok").field(&ok_ox).finish())
    } else {
        // SAFETY: is_ok returned false, so get_err returns a valid pointer.
        let err_ptr = unsafe { (def.vtable.get_err)(ptr)? };
        let err_ox = unsafe { OxRef::new(err_ptr, def.e) };
        Some(f.debug_tuple("Err").field(&err_ox).finish())
    }
}

/// Hash for Result<T, E> - delegates to inner T/E's hash if available
unsafe fn result_hash(ox: OxPtrConst, hasher: &mut HashProxy<'_>) -> Option<()> {
    let shape = ox.shape();
    let def = get_result_def(shape)?;
    let ptr = ox.ptr();

    use core::hash::Hash;
    if unsafe { (def.vtable.is_ok)(ptr) } {
        0u8.hash(hasher);
        let ok_ptr = unsafe { (def.vtable.get_ok)(ptr)? };
        unsafe { def.t.call_hash(ok_ptr, hasher)? };
    } else {
        1u8.hash(hasher);
        let err_ptr = unsafe { (def.vtable.get_err)(ptr)? };
        unsafe { def.e.call_hash(err_ptr, hasher)? };
    }
    Some(())
}

/// PartialEq for Result<T, E>
unsafe fn result_partial_eq(a: OxPtrConst, b: OxPtrConst) -> Option<bool> {
    let shape = a.shape();
    let def = get_result_def(shape)?;

    let a_ptr = a.ptr();
    let b_ptr = b.ptr();
    let a_is_ok = unsafe { (def.vtable.is_ok)(a_ptr) };
    let b_is_ok = unsafe { (def.vtable.is_ok)(b_ptr) };

    Some(match (a_is_ok, b_is_ok) {
        (true, true) => {
            let a_ok = unsafe { (def.vtable.get_ok)(a_ptr)? };
            let b_ok = unsafe { (def.vtable.get_ok)(b_ptr)? };
            unsafe { def.t.call_partial_eq(a_ok, b_ok)? }
        }
        (false, false) => {
            let a_err = unsafe { (def.vtable.get_err)(a_ptr)? };
            let b_err = unsafe { (def.vtable.get_err)(b_ptr)? };
            unsafe { def.e.call_partial_eq(a_err, b_err)? }
        }
        _ => false,
    })
}

/// PartialOrd for Result<T, E>
unsafe fn result_partial_cmp(a: OxPtrConst, b: OxPtrConst) -> Option<Option<Ordering>> {
    let shape = a.shape();
    let def = get_result_def(shape)?;

    let a_ptr = a.ptr();
    let b_ptr = b.ptr();
    let a_is_ok = unsafe { (def.vtable.is_ok)(a_ptr) };
    let b_is_ok = unsafe { (def.vtable.is_ok)(b_ptr) };

    Some(match (a_is_ok, b_is_ok) {
        (true, true) => {
            let a_ok = unsafe { (def.vtable.get_ok)(a_ptr)? };
            let b_ok = unsafe { (def.vtable.get_ok)(b_ptr)? };
            unsafe { def.t.call_partial_cmp(a_ok, b_ok)? }
        }
        (false, false) => {
            let a_err = unsafe { (def.vtable.get_err)(a_ptr)? };
            let b_err = unsafe { (def.vtable.get_err)(b_ptr)? };
            unsafe { def.e.call_partial_cmp(a_err, b_err)? }
        }
        // Ok is greater than Err (following std::cmp::Ord for Result)
        (true, false) => Some(Ordering::Greater),
        (false, true) => Some(Ordering::Less),
    })
}

/// Ord for Result<T, E>
unsafe fn result_cmp(a: OxPtrConst, b: OxPtrConst) -> Option<Ordering> {
    let shape = a.shape();
    let def = get_result_def(shape)?;

    let a_ptr = a.ptr();
    let b_ptr = b.ptr();
    let a_is_ok = unsafe { (def.vtable.is_ok)(a_ptr) };
    let b_is_ok = unsafe { (def.vtable.is_ok)(b_ptr) };

    Some(match (a_is_ok, b_is_ok) {
        (true, true) => {
            let a_ok = unsafe { (def.vtable.get_ok)(a_ptr)? };
            let b_ok = unsafe { (def.vtable.get_ok)(b_ptr)? };
            unsafe { def.t.call_cmp(a_ok, b_ok)? }
        }
        (false, false) => {
            let a_err = unsafe { (def.vtable.get_err)(a_ptr)? };
            let b_err = unsafe { (def.vtable.get_err)(b_ptr)? };
            unsafe { def.e.call_cmp(a_err, b_err)? }
        }
        // Ok is greater than Err (following std::cmp::Ord for Result)
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
    })
}

/// Drop for Result<T, E>
unsafe fn result_drop(ox: OxPtrMut) {
    let shape = ox.shape();
    let Some(def) = get_result_def(shape) else {
        return;
    };
    let ptr = ox.ptr();

    if unsafe { (def.vtable.is_ok)(ptr.as_const()) } {
        let Some(ok_ptr) = (unsafe { (def.vtable.get_ok)(ptr.as_const()) }) else {
            return;
        };
        let ok_ptr_mut = PtrMut::new(ok_ptr.as_byte_ptr() as *mut u8);
        unsafe { def.t.call_drop_in_place(ok_ptr_mut) };
    } else {
        let Some(err_ptr) = (unsafe { (def.vtable.get_err)(ptr.as_const()) }) else {
            return;
        };
        let err_ptr_mut = PtrMut::new(err_ptr.as_byte_ptr() as *mut u8);
        unsafe { def.e.call_drop_in_place(err_ptr_mut) };
    }
}

// Shared vtable for all Result<T, E>
const RESULT_VTABLE: VTableIndirect = VTableIndirect {
    display: None,
    debug: Some(result_debug),
    hash: Some(result_hash),
    invariants: None,
    parse: None,
    parse_bytes: None,
    try_from: None,
    try_into_inner: None,
    try_borrow_inner: None,
    partial_eq: Some(result_partial_eq),
    partial_cmp: Some(result_partial_cmp),
    cmp: Some(result_cmp),
};

// Type operations for all Result<T, E>
static RESULT_TYPE_OPS: TypeOpsIndirect = TypeOpsIndirect {
    drop_in_place: result_drop,
    default_in_place: None,
    clone_into: None,
    is_truthy: None,
};

/// Check if Result<T, E> is Ok
unsafe fn result_is_ok<T, E>(result: PtrConst) -> bool {
    unsafe { result.get::<Result<T, E>>().is_ok() }
}

/// Get the Ok value from Result<T, E> if present
unsafe fn result_get_ok<T, E>(result: PtrConst) -> Option<PtrConst> {
    unsafe {
        result
            .get::<Result<T, E>>()
            .as_ref()
            .ok()
            .map(|t| PtrConst::new(t as *const T))
    }
}

/// Get the Err value from Result<T, E> if present
unsafe fn result_get_err<T, E>(result: PtrConst) -> Option<PtrConst> {
    unsafe {
        result
            .get::<Result<T, E>>()
            .as_ref()
            .err()
            .map(|e| PtrConst::new(e as *const E))
    }
}

/// Initialize Result<T, E> with Ok(value)
unsafe fn result_init_ok<T, E>(result: crate::PtrUninit, value: PtrConst) -> PtrMut {
    unsafe { result.put(Result::<T, E>::Ok(value.read::<T>())) }
}

/// Initialize Result<T, E> with Err(value)
unsafe fn result_init_err<T, E>(result: crate::PtrUninit, value: PtrConst) -> PtrMut {
    unsafe { result.put(Result::<T, E>::Err(value.read::<E>())) }
}

unsafe impl<'a, T: Facet<'a>, E: Facet<'a>> Facet<'a> for Result<T, E> {
    const SHAPE: &'static Shape = &const {
        const fn build_result_vtable<T, E>() -> ResultVTable {
            ResultVTable::builder()
                .is_ok(result_is_ok::<T, E>)
                .get_ok(result_get_ok::<T, E>)
                .get_err(result_get_err::<T, E>)
                .init_ok(result_init_ok::<T, E>)
                .init_err(result_init_err::<T, E>)
                .build()
        }

        ShapeBuilder::for_sized::<Result<T, E>>("Result")
            .ty(Type::User(UserType::Opaque))
            .def(Def::Result(ResultDef::new(
                &const { build_result_vtable::<T, E>() },
                T::SHAPE,
                E::SHAPE,
            )))
            .type_params(&[
                TypeParam {
                    name: "T",
                    shape: T::SHAPE,
                },
                TypeParam {
                    name: "E",
                    shape: E::SHAPE,
                },
            ])
            .vtable_indirect(&RESULT_VTABLE)
            .type_ops_indirect(&RESULT_TYPE_OPS)
            .build()
    };
}
