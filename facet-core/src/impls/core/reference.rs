//! Facet implementation for references (&T and &mut T)

use core::fmt;

use crate::{
    Def, Facet, HashProxy, KnownPointer, OxPtrConst, OxPtrMut, PointerDef, PointerFlags,
    PointerType, PointerVTable, PtrConst, Shape, ShapeBuilder, Type, TypeNameOpts, TypeOpsIndirect,
    TypeParam, VTableIndirect, ValuePointerType, Variance, VarianceDep, VarianceDesc,
};

/// Type-erased type_name for &T - reads the pointee type from the shape
fn ref_type_name(
    shape: &'static Shape,
    f: &mut fmt::Formatter<'_>,
    opts: TypeNameOpts,
) -> fmt::Result {
    let pointee = match &shape.def {
        Def::Pointer(ptr_def) => ptr_def.pointee,
        _ => None,
    };

    write!(f, "&")?;
    if let Some(pointee) = pointee {
        if let Some(opts) = opts.for_children() {
            pointee.write_type_name(f, opts)?;
        } else {
            write!(f, "…")?;
        }
    } else {
        write!(f, "?")?;
    }
    Ok(())
}

/// Type-erased type_name for &mut T - reads the pointee type from the shape
fn ref_mut_type_name(
    shape: &'static Shape,
    f: &mut fmt::Formatter<'_>,
    opts: TypeNameOpts,
) -> fmt::Result {
    let pointee = match &shape.def {
        Def::Pointer(ptr_def) => ptr_def.pointee,
        _ => None,
    };

    write!(f, "&mut ")?;
    if let Some(pointee) = pointee {
        if let Some(opts) = opts.for_children() {
            pointee.write_type_name(f, opts)?;
        } else {
            write!(f, "…")?;
        }
    } else {
        write!(f, "?")?;
    }
    Ok(())
}

/// Helper to dereference a reference and get a PtrConst to the pointee.
/// Handles both thin and wide pointers correctly.
///
/// # Safety
/// - `ox` must point to a valid reference
/// - The returned PtrConst is only valid for the lifetime of the referent
unsafe fn deref_to_pointee(ox: OxPtrConst) -> Option<(&'static Shape, PtrConst)> {
    let shape = ox.shape();
    let Def::Pointer(ptr_def) = shape.def else {
        return None;
    };
    let pointee_shape = ptr_def.pointee?;

    // ox.ptr() points to the reference itself (e.g., points to a &T or &str value in memory)
    // We need to READ that reference value to get the pointee.
    //
    // For thin pointers (&T where T: Sized): read a single pointer-sized value
    // For wide pointers (&str, &[T]): read a fat pointer (ptr + metadata)
    //
    // The vtable functions for the pointee expect a PtrConst that points TO the data.
    // For sized types: just a thin pointer to the data
    // For unsized types: a wide pointer with the metadata preserved

    let ref_size = shape.layout.sized_layout().ok()?.size();

    if ref_size == core::mem::size_of::<*const ()>() {
        // Thin pointer - read the pointer value and create thin PtrConst
        let inner_ptr = unsafe { *(ox.ptr().as_byte_ptr() as *const *const u8) };
        Some((pointee_shape, PtrConst::new(inner_ptr as *const ())))
    } else {
        // Wide/fat pointer - read the fat pointer value from memory
        // ox.ptr() points to a &str (or &[T], etc.) stored in memory
        // We need to read that fat pointer and create a PtrConst from it
        //
        // The fat pointer is stored at ox.ptr(), we read it as the appropriate reference type
        // and then create a PtrConst from that reference

        // Read the fat pointer from memory - it's stored as [ptr, metadata]
        let fat_ptr_location = ox.ptr().as_byte_ptr() as *const [*const u8; 2];
        let [data_ptr, metadata] = unsafe { *fat_ptr_location };

        // Create a new PtrConst with the proper wide pointer representation
        Some((
            pointee_shape,
            PtrConst::new_wide(data_ptr, metadata as *const ()),
        ))
    }
}

/// Debug for &T - delegates to T's debug
unsafe fn ref_debug(ox: OxPtrConst, f: &mut core::fmt::Formatter<'_>) -> Option<core::fmt::Result> {
    let (pointee_shape, inner) = unsafe { deref_to_pointee(ox)? };
    unsafe { pointee_shape.call_debug(inner, f) }
}

/// Display for &T - delegates to T's display
unsafe fn ref_display(
    ox: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    let (pointee_shape, inner) = unsafe { deref_to_pointee(ox)? };
    unsafe { pointee_shape.call_display(inner, f) }
}

/// Hash for &T - delegates to T's hash
unsafe fn ref_hash(ox: OxPtrConst, hasher: &mut HashProxy<'_>) -> Option<()> {
    let (pointee_shape, inner) = unsafe { deref_to_pointee(ox)? };
    unsafe { pointee_shape.call_hash(inner, hasher) }
}

/// PartialEq for &T - delegates to T's partial_eq
unsafe fn ref_partial_eq(a: OxPtrConst, b: OxPtrConst) -> Option<bool> {
    let (pointee_shape, a_inner) = unsafe { deref_to_pointee(a)? };
    let (_, b_inner) = unsafe { deref_to_pointee(b)? };
    unsafe { pointee_shape.call_partial_eq(a_inner, b_inner) }
}

/// PartialOrd for &T - delegates to T's partial_cmp
unsafe fn ref_partial_cmp(a: OxPtrConst, b: OxPtrConst) -> Option<Option<core::cmp::Ordering>> {
    let (pointee_shape, a_inner) = unsafe { deref_to_pointee(a)? };
    let (_, b_inner) = unsafe { deref_to_pointee(b)? };
    unsafe { pointee_shape.call_partial_cmp(a_inner, b_inner) }
}

/// Ord for &T - delegates to T's cmp
unsafe fn ref_cmp(a: OxPtrConst, b: OxPtrConst) -> Option<core::cmp::Ordering> {
    let (pointee_shape, a_inner) = unsafe { deref_to_pointee(a)? };
    let (_, b_inner) = unsafe { deref_to_pointee(b)? };
    unsafe { pointee_shape.call_cmp(a_inner, b_inner) }
}

/// Drop for &T and &mut T (no-op, references don't need dropping)
unsafe fn ref_drop(_ptr: OxPtrMut) {
    // References don't need dropping
}

/// Clone for &T - just copies the reference (since &T is Copy)
unsafe fn ref_clone(src: OxPtrConst, dst: OxPtrMut) {
    // For references, clone is just a memcpy of the pointer
    let Some(size) = src.shape().layout.sized_layout().ok().map(|l| l.size()) else {
        return;
    };
    unsafe {
        core::ptr::copy_nonoverlapping(src.ptr().as_byte_ptr(), dst.ptr().as_mut_byte_ptr(), size);
    }
}

// Shared vtable for all &T (immutable refs are Copy, so Clone is trivial)
const REF_VTABLE: VTableIndirect = VTableIndirect {
    display: Some(ref_display),
    debug: Some(ref_debug),
    hash: Some(ref_hash),
    invariants: None,
    parse: None,
    parse_bytes: None,
    try_from: None,
    try_into_inner: None,
    try_borrow_inner: None,
    partial_eq: Some(ref_partial_eq),
    partial_cmp: Some(ref_partial_cmp),
    cmp: Some(ref_cmp),
};

// Type operations for &T (references are Copy, so they can be cloned)
static REF_TYPE_OPS: TypeOpsIndirect = TypeOpsIndirect {
    drop_in_place: ref_drop,
    default_in_place: None,
    clone_into: Some(ref_clone),
    is_truthy: None,
};

// Vtable for &mut T (not Clone since &mut T is not Clone)
const REF_MUT_VTABLE: VTableIndirect = VTableIndirect {
    display: Some(ref_display),
    debug: Some(ref_debug),
    hash: Some(ref_hash),
    invariants: None,
    parse: None,
    parse_bytes: None,
    try_from: None,
    try_into_inner: None,
    try_borrow_inner: None,
    partial_eq: Some(ref_partial_eq),
    partial_cmp: Some(ref_partial_cmp),
    cmp: Some(ref_cmp),
};

// Type operations for &mut T (no Clone since &mut T is not Clone)
static REF_MUT_TYPE_OPS: TypeOpsIndirect = TypeOpsIndirect {
    drop_in_place: ref_drop,
    default_in_place: None,
    clone_into: None,
    is_truthy: None,
};

/// Borrow function for &T - dereferences to get inner pointer
unsafe fn ref_borrow<T: ?Sized>(this: PtrConst) -> PtrConst {
    let ptr: &&T = unsafe { this.get::<&T>() };
    let ptr: &T = ptr;
    // Don't cast to *const u8 - that loses metadata for wide pointers like &str
    PtrConst::new(ptr as *const T)
}

/// Borrow function for &mut T - dereferences to get inner pointer
unsafe fn ref_mut_borrow<T: ?Sized>(this: PtrConst) -> PtrConst {
    let ptr: &&mut T = unsafe { this.get::<&mut T>() };
    let ptr: &T = ptr;
    // Don't cast to *const u8 - that loses metadata for wide pointers like &str
    PtrConst::new(ptr as *const T)
}

unsafe impl<'a, T: ?Sized + Facet<'a>> Facet<'a> for &'a T {
    const SHAPE: &'static Shape = &const {
        const fn build_pointer_vtable<T: ?Sized>() -> PointerVTable {
            PointerVTable {
                borrow_fn: Some(ref_borrow::<T>),
                ..PointerVTable::new()
            }
        }

        ShapeBuilder::for_sized::<&T>("&T")
            .decl_id(crate::DeclId::new(crate::decl_id_hash("#ref#&T")))
            .type_name(ref_type_name)
            .ty({
                let vpt = ValuePointerType {
                    mutable: false,
                    wide: size_of::<*const T>() != size_of::<*const ()>(),
                    target: T::SHAPE,
                };
                Type::Pointer(PointerType::Reference(vpt))
            })
            .def(Def::Pointer(PointerDef {
                vtable: &const { build_pointer_vtable::<T>() },
                pointee: Some(T::SHAPE),
                weak: None,
                strong: None,
                flags: PointerFlags::EMPTY,
                known: Some(KnownPointer::SharedReference),
            }))
            .type_params(&[TypeParam {
                name: "T",
                shape: T::SHAPE,
            }])
            .inner(T::SHAPE)
            // &'a T is covariant with respect to 'a and covariant with respect to T
            // See: https://doc.rust-lang.org/reference/subtyping.html#r-subtyping.variance.builtin-types
            .variance(VarianceDesc {
                base: Variance::Covariant,
                deps: &const { [VarianceDep::covariant(T::SHAPE)] },
            })
            .vtable_indirect(&REF_VTABLE)
            .type_ops_indirect(&REF_TYPE_OPS)
            .build()
    };
}

unsafe impl<'a, T: ?Sized + Facet<'a>> Facet<'a> for &'a mut T {
    const SHAPE: &'static Shape = &const {
        const fn build_pointer_vtable<T: ?Sized>() -> PointerVTable {
            PointerVTable {
                borrow_fn: Some(ref_mut_borrow::<T>),
                ..PointerVTable::new()
            }
        }

        ShapeBuilder::for_sized::<&mut T>("&mut T")
            .decl_id(crate::DeclId::new(crate::decl_id_hash("#ref#&mut T")))
            .type_name(ref_mut_type_name)
            .ty({
                let vpt = ValuePointerType {
                    mutable: true,
                    wide: size_of::<*const T>() != size_of::<*const ()>(),
                    target: T::SHAPE,
                };
                Type::Pointer(PointerType::Reference(vpt))
            })
            .def(Def::Pointer(PointerDef {
                vtable: &const { build_pointer_vtable::<T>() },
                pointee: Some(T::SHAPE),
                weak: None,
                strong: None,
                flags: PointerFlags::EMPTY,
                known: Some(KnownPointer::ExclusiveReference),
            }))
            .type_params(&[TypeParam {
                name: "T",
                shape: T::SHAPE,
            }])
            .inner(T::SHAPE)
            // &'a mut T is covariant with respect to 'a and invariant with respect to T.
            //
            // For `computed_variance()` (overall lifetime variance): if `T` contributes
            // `Bivariant` (no lifetime constraints), it doesn't affect the result, so the
            // outcome is `Covariant` (from 'a). Otherwise the invariant dependency forces
            // `Invariant`.
            .variance(VarianceDesc {
                base: Variance::Covariant,
                deps: &const { [VarianceDep::invariant(T::SHAPE)] },
            })
            .vtable_indirect(&REF_MUT_VTABLE)
            .type_ops_indirect(&REF_MUT_TYPE_OPS)
            .build()
    };
}
