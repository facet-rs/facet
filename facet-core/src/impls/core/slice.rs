use core::cmp::Ordering;

use crate::{OxPtrConst, *};

unsafe fn slice_len<T>(ptr: PtrConst) -> usize {
    unsafe {
        let slice = ptr.get::<[T]>();
        slice.len()
    }
}

unsafe fn slice_as_ptr<T>(ptr: PtrConst) -> PtrConst {
    unsafe {
        let slice = ptr.get::<[T]>();
        PtrConst::new(slice.as_ptr())
    }
}

unsafe fn slice_as_mut_ptr<T>(ptr: PtrMut) -> PtrMut {
    unsafe {
        let slice = ptr.as_mut::<[T]>();
        PtrMut::new(slice.as_mut_ptr())
    }
}

unsafe fn slice_drop(ox: OxPtrMut) {
    let shape = ox.shape();
    let Some(def) = get_slice_def(shape) else {
        return;
    };
    let len = unsafe { (def.vtable.len)(ox.ptr().as_const()) };
    let slice_ptr = unsafe { (def.vtable.as_mut_ptr)(ox.ptr()) };
    let Some(stride) = def
        .t
        .layout
        .sized_layout()
        .ok()
        .map(|l| l.pad_to_align().size())
    else {
        return;
    };

    for i in 0..len {
        let elem_ptr = unsafe { PtrMut::new((slice_ptr.as_byte_ptr() as *mut u8).add(i * stride)) };
        unsafe { def.t.call_drop_in_place(elem_ptr) };
    }
}

#[inline(always)]
unsafe fn slice_truthy<T>(ptr: PtrConst) -> bool {
    !unsafe { ptr.get::<[T]>() }.is_empty()
}

/// Extract the SliceDef from a shape, returns None if not a slice
#[inline]
fn get_slice_def(shape: &'static Shape) -> Option<&'static SliceDef> {
    match shape.def {
        Def::Slice(ref def) => Some(def),
        _ => None,
    }
}

/// Debug for `[T]`
unsafe fn slice_debug(
    ox: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    let shape = ox.shape();
    let def = get_slice_def(shape)?;
    let ptr = ox.ptr();

    let len = unsafe { (def.vtable.len)(ptr) };
    let slice_ptr = unsafe { (def.vtable.as_ptr)(ptr) };
    let stride = def.t.layout.sized_layout().ok()?.pad_to_align().size();

    let mut list = f.debug_list();
    for i in 0..len {
        // SAFETY: We're iterating within bounds of the slice, and the caller
        // guarantees the OxPtrConst points to a valid slice.
        let elem_ptr = unsafe { PtrConst::new((slice_ptr.raw_ptr()).add(i * stride)) };
        let elem_ox = unsafe { OxRef::new(elem_ptr, def.t) };
        list.entry(&elem_ox);
    }
    Some(list.finish())
}

/// Hash for `[T]`
unsafe fn slice_hash(ox: OxPtrConst, hasher: &mut HashProxy<'_>) -> Option<()> {
    let shape = ox.shape();
    let def = get_slice_def(shape)?;
    let ptr = ox.ptr();

    let len = unsafe { (def.vtable.len)(ptr) };
    let slice_ptr = unsafe { (def.vtable.as_ptr)(ptr) };
    let stride = def.t.layout.sized_layout().ok()?.pad_to_align().size();

    for i in 0..len {
        let elem_ptr = unsafe { PtrConst::new((slice_ptr.raw_ptr()).add(i * stride)) };
        unsafe { def.t.call_hash(elem_ptr, hasher)? };
    }
    Some(())
}

/// PartialEq for `[T]`
unsafe fn slice_partial_eq(a: OxPtrConst, b: OxPtrConst) -> Option<bool> {
    let shape = a.shape();
    let def = get_slice_def(shape)?;

    let a_len = unsafe { (def.vtable.len)(a.ptr()) };
    let b_len = unsafe { (def.vtable.len)(b.ptr()) };

    if a_len != b_len {
        return Some(false);
    }

    let a_ptr = unsafe { (def.vtable.as_ptr)(a.ptr()) };
    let b_ptr = unsafe { (def.vtable.as_ptr)(b.ptr()) };
    let stride = def.t.layout.sized_layout().ok()?.pad_to_align().size();

    for i in 0..a_len {
        let a_elem = unsafe { PtrConst::new((a_ptr.raw_ptr()).add(i * stride)) };
        let b_elem = unsafe { PtrConst::new((b_ptr.raw_ptr()).add(i * stride)) };
        if !unsafe { def.t.call_partial_eq(a_elem, b_elem)? } {
            return Some(false);
        }
    }
    Some(true)
}

/// PartialOrd for `[T]`
unsafe fn slice_partial_cmp(a: OxPtrConst, b: OxPtrConst) -> Option<Option<Ordering>> {
    let shape = a.shape();
    let def = get_slice_def(shape)?;

    let a_len = unsafe { (def.vtable.len)(a.ptr()) };
    let b_len = unsafe { (def.vtable.len)(b.ptr()) };

    let a_ptr = unsafe { (def.vtable.as_ptr)(a.ptr()) };
    let b_ptr = unsafe { (def.vtable.as_ptr)(b.ptr()) };
    let stride = def.t.layout.sized_layout().ok()?.pad_to_align().size();

    let min_len = a_len.min(b_len);
    for i in 0..min_len {
        let a_elem = unsafe { PtrConst::new((a_ptr.raw_ptr()).add(i * stride)) };
        let b_elem = unsafe { PtrConst::new((b_ptr.raw_ptr()).add(i * stride)) };
        match unsafe { def.t.call_partial_cmp(a_elem, b_elem)? } {
            Some(Ordering::Equal) => continue,
            other => return Some(other),
        }
    }
    Some(Some(a_len.cmp(&b_len)))
}

/// Ord for `[T]`
unsafe fn slice_cmp(a: OxPtrConst, b: OxPtrConst) -> Option<Ordering> {
    let shape = a.shape();
    let def = get_slice_def(shape)?;

    let a_len = unsafe { (def.vtable.len)(a.ptr()) };
    let b_len = unsafe { (def.vtable.len)(b.ptr()) };

    let a_ptr = unsafe { (def.vtable.as_ptr)(a.ptr()) };
    let b_ptr = unsafe { (def.vtable.as_ptr)(b.ptr()) };
    let stride = def.t.layout.sized_layout().ok()?.pad_to_align().size();

    let min_len = a_len.min(b_len);
    for i in 0..min_len {
        let a_elem = unsafe { PtrConst::new((a_ptr.raw_ptr()).add(i * stride)) };
        let b_elem = unsafe { PtrConst::new((b_ptr.raw_ptr()).add(i * stride)) };
        match unsafe { def.t.call_cmp(a_elem, b_elem)? } {
            Ordering::Equal => continue,
            other => return Some(other),
        }
    }
    Some(a_len.cmp(&b_len))
}

// Shared vtable for all [T]
const SLICE_VTABLE: VTableIndirect = VTableIndirect {
    display: None,
    debug: Some(slice_debug),
    hash: Some(slice_hash),
    invariants: None,
    parse: None,
    parse_bytes: None,
    try_from: None,
    try_into_inner: None,
    try_borrow_inner: None,
    partial_eq: Some(slice_partial_eq),
    partial_cmp: Some(slice_partial_cmp),
    cmp: Some(slice_cmp),
};

unsafe impl<'a, T> Facet<'a> for [T]
where
    T: Facet<'a>,
{
    const SHAPE: &'static Shape = &const {
        const fn build_type_name<'a, T: Facet<'a>>() -> TypeNameFn {
            fn type_name_impl<'a, T: Facet<'a>>(
                _shape: &'static Shape,
                f: &mut core::fmt::Formatter<'_>,
                opts: TypeNameOpts,
            ) -> core::fmt::Result {
                if let Some(opts) = opts.for_children() {
                    write!(f, "[")?;
                    T::SHAPE.write_type_name(f, opts)?;
                    write!(f, "]")
                } else {
                    write!(f, "[…]")
                }
            }
            type_name_impl::<T>
        }

        const fn build_slice_vtable<'a, T: Facet<'a>>() -> SliceVTable {
            SliceVTable {
                len: slice_len::<T>,
                as_ptr: slice_as_ptr::<T>,
                as_mut_ptr: slice_as_mut_ptr::<T>,
            }
        }

        ShapeBuilder::for_unsized::<Self>("[_]")
            .decl_id(crate::DeclId::new(crate::decl_id_hash("[T]")))
            .type_name(build_type_name::<T>())
            .vtable_indirect(&SLICE_VTABLE)
            .ty(Type::Sequence(SequenceType::Slice(SliceType {
                t: T::SHAPE,
            })))
            .def(Def::Slice(SliceDef::new(
                &const { build_slice_vtable::<T>() },
                T::SHAPE,
            )))
            .type_params(&[TypeParam {
                name: "T",
                shape: T::SHAPE,
            }])
            // [T] propagates T's variance
            .variance(VarianceDesc {
                base: Variance::Bivariant,
                deps: &const { [VarianceDep::covariant(T::SHAPE)] },
            })
            .type_ops_indirect(
                &const {
                    TypeOpsIndirect {
                        drop_in_place: slice_drop,
                        default_in_place: None,
                        clone_into: None,
                        is_truthy: Some(slice_truthy::<T>),
                    }
                },
            )
            .build()
    };
}
