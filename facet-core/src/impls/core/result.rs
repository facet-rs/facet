//! Facet implementation for Result<T, E>

use core::cmp::Ordering;

use crate::{
    Def, Facet, HashProxy, OxPtrConst, OxPtrMut, OxRef, PtrConst, PtrMut, ResultDef, ResultVTable,
    Shape, ShapeBuilder, Type, TypeOpsIndirect, TypeParam, UserType, VTableIndirect, Variance,
    VarianceDep, VarianceDesc,
};

/// Extract the ResultDef from a shape, returns None if not a Result
#[inline]
const fn get_result_def(shape: &'static Shape) -> Option<&'static ResultDef> {
    match shape.def {
        Def::Result(ref def) => Some(def),
        _ => None,
    }
}

#[inline]
unsafe fn result_get_ok_ptr(def: &ResultDef, ptr: PtrConst) -> Option<PtrConst> {
    let raw = unsafe { (def.vtable.get_ok)(ptr) };
    if raw.is_null() {
        None
    } else {
        Some(PtrConst::new_sized(raw))
    }
}

#[inline]
unsafe fn result_get_err_ptr(def: &ResultDef, ptr: PtrConst) -> Option<PtrConst> {
    let raw = unsafe { (def.vtable.get_err)(ptr) };
    if raw.is_null() {
        None
    } else {
        Some(PtrConst::new_sized(raw))
    }
}

fn result_type_name(
    shape: &'static Shape,
    f: &mut core::fmt::Formatter<'_>,
    opts: crate::TypeNameOpts,
) -> core::fmt::Result {
    write!(f, "Result")?;
    if let Some(opts) = opts.for_children() {
        write!(f, "<")?;
        if let Some(t) = shape.type_params.first() {
            t.shape.write_type_name(f, opts)?;
        }
        if let Some(e) = shape.type_params.get(1) {
            write!(f, ", ")?;
            e.shape.write_type_name(f, opts)?;
        }
        write!(f, ">")?;
    } else {
        write!(f, "<…>")?;
    }
    Ok(())
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
        let ok_ptr = unsafe { result_get_ok_ptr(def, ptr)? };
        let ok_ox = unsafe { OxRef::new(ok_ptr, def.t) };
        Some(f.debug_tuple("Ok").field(&ok_ox).finish())
    } else {
        // SAFETY: is_ok returned false, so get_err returns a valid pointer.
        let err_ptr = unsafe { result_get_err_ptr(def, ptr)? };
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
        let ok_ptr = unsafe { result_get_ok_ptr(def, ptr)? };
        unsafe { def.t.call_hash(ok_ptr, hasher)? };
    } else {
        1u8.hash(hasher);
        let err_ptr = unsafe { result_get_err_ptr(def, ptr)? };
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
            let a_ok = unsafe { result_get_ok_ptr(def, a_ptr)? };
            let b_ok = unsafe { result_get_ok_ptr(def, b_ptr)? };
            unsafe { def.t.call_partial_eq(a_ok, b_ok)? }
        }
        (false, false) => {
            let a_err = unsafe { result_get_err_ptr(def, a_ptr)? };
            let b_err = unsafe { result_get_err_ptr(def, b_ptr)? };
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
            let a_ok = unsafe { result_get_ok_ptr(def, a_ptr)? };
            let b_ok = unsafe { result_get_ok_ptr(def, b_ptr)? };
            unsafe { def.t.call_partial_cmp(a_ok, b_ok)? }
        }
        (false, false) => {
            let a_err = unsafe { result_get_err_ptr(def, a_ptr)? };
            let b_err = unsafe { result_get_err_ptr(def, b_ptr)? };
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
            let a_ok = unsafe { result_get_ok_ptr(def, a_ptr)? };
            let b_ok = unsafe { result_get_ok_ptr(def, b_ptr)? };
            unsafe { def.t.call_cmp(a_ok, b_ok)? }
        }
        (false, false) => {
            let a_err = unsafe { result_get_err_ptr(def, a_ptr)? };
            let b_err = unsafe { result_get_err_ptr(def, b_ptr)? };
            unsafe { def.e.call_cmp(a_err, b_err)? }
        }
        // Ok is greater than Err (following std::cmp::Ord for Result)
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
    })
}

/// Drop for `Result<T, E>`
///
/// Calls `core::ptr::drop_in_place` on the full `Result<T, E>` so the compiler's
/// drop glue handles both variants correctly. We can't go through `vtable.get_err`
/// / `vtable.get_ok` to locate the inner value: those use `Result::as_ref().ok()`,
/// which retags under Stacked Borrows as `SharedReadOnly`, so dropping through
/// the resulting pointer is UB (miri catches this).
unsafe fn result_drop<T, E>(ox: OxPtrMut) {
    unsafe { core::ptr::drop_in_place(ox.as_mut::<Result<T, E>>()) };
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

/// Check if Result<T, E> is Ok
unsafe extern "C" fn result_is_ok<T, E>(result: PtrConst) -> bool {
    unsafe { result.get::<Result<T, E>>().is_ok() }
}

/// Get the Ok value from Result<T, E> if present
unsafe extern "C" fn result_get_ok<T, E>(result: PtrConst) -> *const u8 {
    unsafe {
        result
            .get::<Result<T, E>>()
            .as_ref()
            .ok()
            .map_or(core::ptr::null(), |t| t as *const T as *const u8)
    }
}

/// Get the Err value from Result<T, E> if present
unsafe extern "C" fn result_get_err<T, E>(result: PtrConst) -> *const u8 {
    unsafe {
        result
            .get::<Result<T, E>>()
            .as_ref()
            .err()
            .map_or(core::ptr::null(), |e| e as *const E as *const u8)
    }
}

/// Initialize Result<T, E> with Ok(value)
unsafe extern "C" fn result_init_ok<T, E>(result: crate::PtrUninit, value: PtrMut) -> PtrMut {
    unsafe { result.put(Result::<T, E>::Ok(value.read::<T>())) }
}

/// Initialize Result<T, E> with Err(value)
unsafe extern "C" fn result_init_err<T, E>(result: crate::PtrUninit, value: PtrMut) -> PtrMut {
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

        const fn build_type_ops<T, E>() -> TypeOpsIndirect {
            TypeOpsIndirect {
                drop_in_place: result_drop::<T, E>,
                default_in_place: None,
                clone_into: None,
                is_truthy: None,
            }
        }

        ShapeBuilder::for_sized::<Result<T, E>>("Result")
            .module_path("core::result")
            .type_name(result_type_name)
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
            // Result<T, E> combines T and E variances
            .variance(VarianceDesc {
                base: Variance::Bivariant,
                deps: &const {
                    [
                        VarianceDep::covariant(T::SHAPE),
                        VarianceDep::covariant(E::SHAPE),
                    ]
                },
            })
            .vtable_indirect(&RESULT_VTABLE)
            .type_ops_indirect(&const { build_type_ops::<T, E>() })
            .build()
    };
}
