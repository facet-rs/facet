//! Facet implementation for [T; N] arrays

use core::{cmp::Ordering, fmt};

use crate::{
    ArrayDef, ArrayVTable, Def, Facet, HashProxy, OxPtrConst, OxPtrMut, OxRef, PtrConst, PtrMut,
    Shape, ShapeBuilder, Type, TypeNameOpts, TypeOpsIndirect, TypeParam, VTableIndirect,
};

/// Extract the ArrayDef from a shape, returns None if not an array
#[inline]
fn get_array_def(shape: &'static Shape) -> Option<&'static ArrayDef> {
    match shape.def {
        Def::Array(ref def) => Some(def),
        _ => None,
    }
}

/// Type-erased type_name for arrays - reads T and N from the shape
fn array_type_name(
    shape: &'static Shape,
    f: &mut fmt::Formatter<'_>,
    opts: TypeNameOpts,
) -> fmt::Result {
    let def = match &shape.def {
        Def::Array(def) => def,
        _ => return write!(f, "[?; ?]"),
    };

    if let Some(opts) = opts.for_children() {
        write!(f, "[")?;
        def.t.write_type_name(f, opts)?;
        write!(f, "; {}]", def.n)
    } else {
        write!(f, "[…; {}]", def.n)
    }
}

/// Debug for [T; N] - formats as array literal
unsafe fn array_debug(
    ox: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    let shape = ox.shape();
    let def = get_array_def(shape)?;
    let ptr = ox.ptr();

    let mut list = f.debug_list();
    let slice_ptr = unsafe { (def.vtable.as_ptr)(ptr) };
    let stride = def.t.layout.sized_layout().ok()?.pad_to_align().size();

    for i in 0..def.n {
        // SAFETY: We're iterating within bounds of the array, and the caller
        // guarantees the OxPtrConst points to a valid array.
        let elem_ptr = unsafe { PtrConst::new((slice_ptr.as_byte_ptr()).add(i * stride)) };
        let elem_ox = unsafe { OxRef::new(elem_ptr, def.t) };
        list.entry(&elem_ox);
    }
    Some(list.finish())
}

/// Hash for [T; N] - hashes each element
unsafe fn array_hash(ox: OxPtrConst, hasher: &mut HashProxy<'_>) -> Option<()> {
    let shape = ox.shape();
    let def = get_array_def(shape)?;
    let ptr = ox.ptr();

    let slice_ptr = unsafe { (def.vtable.as_ptr)(ptr) };
    let stride = def.t.layout.sized_layout().ok()?.pad_to_align().size();

    for i in 0..def.n {
        let elem_ptr = unsafe { PtrConst::new((slice_ptr.as_byte_ptr()).add(i * stride)) };
        unsafe { def.t.call_hash(elem_ptr, hasher)? };
    }
    Some(())
}

/// PartialEq for [T; N]
unsafe fn array_partial_eq(a: OxPtrConst, b: OxPtrConst) -> Option<bool> {
    let shape = a.shape();
    let def = get_array_def(shape)?;

    let a_ptr = unsafe { (def.vtable.as_ptr)(a.ptr()) };
    let b_ptr = unsafe { (def.vtable.as_ptr)(b.ptr()) };
    let stride = def.t.layout.sized_layout().ok()?.pad_to_align().size();

    for i in 0..def.n {
        let a_elem = unsafe { PtrConst::new((a_ptr.as_byte_ptr()).add(i * stride)) };
        let b_elem = unsafe { PtrConst::new((b_ptr.as_byte_ptr()).add(i * stride)) };
        if !unsafe { def.t.call_partial_eq(a_elem, b_elem)? } {
            return Some(false);
        }
    }
    Some(true)
}

/// PartialOrd for [T; N]
unsafe fn array_partial_cmp(a: OxPtrConst, b: OxPtrConst) -> Option<Option<Ordering>> {
    let shape = a.shape();
    let def = get_array_def(shape)?;

    let a_ptr = unsafe { (def.vtable.as_ptr)(a.ptr()) };
    let b_ptr = unsafe { (def.vtable.as_ptr)(b.ptr()) };
    let stride = def.t.layout.sized_layout().ok()?.pad_to_align().size();

    for i in 0..def.n {
        let a_elem = unsafe { PtrConst::new((a_ptr.as_byte_ptr()).add(i * stride)) };
        let b_elem = unsafe { PtrConst::new((b_ptr.as_byte_ptr()).add(i * stride)) };
        match unsafe { def.t.call_partial_cmp(a_elem, b_elem)? } {
            Some(Ordering::Equal) => continue,
            other => return Some(other),
        }
    }
    Some(Some(Ordering::Equal))
}

/// Ord for [T; N]
unsafe fn array_cmp(a: OxPtrConst, b: OxPtrConst) -> Option<Ordering> {
    let shape = a.shape();
    let def = get_array_def(shape)?;

    let a_ptr = unsafe { (def.vtable.as_ptr)(a.ptr()) };
    let b_ptr = unsafe { (def.vtable.as_ptr)(b.ptr()) };
    let stride = def.t.layout.sized_layout().ok()?.pad_to_align().size();

    for i in 0..def.n {
        let a_elem = unsafe { PtrConst::new((a_ptr.as_byte_ptr()).add(i * stride)) };
        let b_elem = unsafe { PtrConst::new((b_ptr.as_byte_ptr()).add(i * stride)) };
        match unsafe { def.t.call_cmp(a_elem, b_elem)? } {
            Ordering::Equal => continue,
            other => return Some(other),
        }
    }
    Some(Ordering::Equal)
}

/// Drop for [T; N]
unsafe fn array_drop(ox: OxPtrMut) {
    let shape = ox.shape();
    let Some(def) = get_array_def(shape) else {
        return;
    };
    let ptr = ox.ptr();

    let slice_ptr = unsafe { (def.vtable.as_mut_ptr)(ptr) };
    let Some(stride) = def
        .t
        .layout
        .sized_layout()
        .ok()
        .map(|l| l.pad_to_align().size())
    else {
        return;
    };

    for i in 0..def.n {
        let elem_ptr = unsafe { PtrMut::new((slice_ptr.as_byte_ptr() as *mut u8).add(i * stride)) };
        unsafe { def.t.call_drop_in_place(elem_ptr) };
    }
}

/// Default for [T; N] - default-initializes each element
unsafe fn array_default(ox: OxPtrMut) {
    let shape = ox.shape();
    let Some(def) = get_array_def(shape) else {
        return;
    };
    let ptr = ox.ptr();

    let slice_ptr = unsafe { (def.vtable.as_mut_ptr)(ptr) };
    let Some(stride) = def
        .t
        .layout
        .sized_layout()
        .ok()
        .map(|l| l.pad_to_align().size())
    else {
        return;
    };

    for i in 0..def.n {
        let elem_ptr = unsafe { PtrMut::new((slice_ptr.as_byte_ptr() as *mut u8).add(i * stride)) };
        if unsafe { def.t.call_default_in_place(elem_ptr) }.is_none() {
            return;
        }
    }
}

/// Clone for [T; N] - clones each element
unsafe fn array_clone(src: OxPtrConst, dst: OxPtrMut) {
    let shape = src.shape();
    let Some(def) = get_array_def(shape) else {
        return;
    };

    let src_ptr = unsafe { (def.vtable.as_ptr)(src.ptr()) };
    let dst_ptr = unsafe { (def.vtable.as_mut_ptr)(dst.ptr()) };
    let Some(stride) = def
        .t
        .layout
        .sized_layout()
        .ok()
        .map(|l| l.pad_to_align().size())
    else {
        return;
    };

    for i in 0..def.n {
        let src_elem = unsafe { PtrConst::new((src_ptr.as_byte_ptr()).add(i * stride)) };
        let dst_elem = unsafe { PtrMut::new((dst_ptr.as_byte_ptr() as *mut u8).add(i * stride)) };
        if unsafe { def.t.call_clone_into(src_elem, dst_elem) }.is_none() {
            return;
        }
    }
}

// Shared vtable for all [T; N]
const ARRAY_VTABLE: VTableIndirect = VTableIndirect {
    display: None,
    debug: Some(array_debug),
    hash: Some(array_hash),
    invariants: None,
    parse: None,
    parse_bytes: None,
    try_from: None,
    try_into_inner: None,
    try_borrow_inner: None,
    partial_eq: Some(array_partial_eq),
    partial_cmp: Some(array_partial_cmp),
    cmp: Some(array_cmp),
};

/// Get pointer to array data buffer
unsafe fn array_as_ptr<T, const N: usize>(ptr: PtrConst) -> PtrConst {
    let array = unsafe { ptr.get::<[T; N]>() };
    PtrConst::new(array.as_ptr() as *const u8)
}

/// Get mutable pointer to array data buffer
unsafe fn array_as_mut_ptr<T, const N: usize>(ptr: PtrMut) -> PtrMut {
    let array = unsafe { ptr.as_mut::<[T; N]>() };
    PtrMut::new(array.as_mut_ptr() as *mut u8)
}

unsafe impl<'a, T, const N: usize> Facet<'a> for [T; N]
where
    T: Facet<'a>,
{
    const SHAPE: &'static Shape = &const {
        const fn build_array_vtable<T, const N: usize>() -> ArrayVTable {
            ArrayVTable::builder()
                .as_ptr(array_as_ptr::<T, N>)
                .as_mut_ptr(array_as_mut_ptr::<T, N>)
                .build()
        }

        ShapeBuilder::for_sized::<[T; N]>("[T; N]")
            .type_name(array_type_name)
            .ty(Type::Sequence(crate::SequenceType::Array(
                crate::ArrayType { t: T::SHAPE, n: N },
            )))
            .def(Def::Array(ArrayDef::new(
                &const { build_array_vtable::<T, N>() },
                T::SHAPE,
                N,
            )))
            .type_params(&[TypeParam {
                name: "T",
                shape: T::SHAPE,
            }])
            .inner(T::SHAPE)
            // [T; N] propagates T's variance
            .variance(Shape::computed_variance)
            .vtable_indirect(&ARRAY_VTABLE)
            .type_ops_indirect(
                &const {
                    unsafe fn truthy<const N: usize>(_: PtrConst) -> bool {
                        N != 0
                    }

                    TypeOpsIndirect {
                        drop_in_place: array_drop,
                        default_in_place: Some(array_default),
                        clone_into: Some(array_clone),
                        is_truthy: Some(truthy::<N>),
                    }
                },
            )
            .build()
    };
}
