use crate::*;

use alloc::boxed::Box;
use alloc::vec::Vec;

/// Helper for Debug formatting via Shape vtable
struct DebugViaShape(&'static Shape, PtrConst);

impl core::fmt::Debug for DebugViaShape {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match unsafe { self.0.call_debug(self.1, f) } {
            Some(result) => result,
            None => write!(f, "???"),
        }
    }
}

// =============================================================================
// Type-erased vtable functions for Vec<T> - shared across all instantiations
// =============================================================================

/// Vec memory layout (stable across all T)
/// Layout is determined at const time by probing a Vec.
#[repr(C)]
struct VecLayout {
    #[allow(dead_code)]
    cap: usize,
    ptr: *mut u8,
    len: usize,
}

// Compile-time assertion that our VecLayout matches the actual Vec layout
const _: () = {
    // Create a Vec and transmute to probe the layout
    let v: Vec<u8> = Vec::new();
    let fields: [usize; 3] = unsafe { core::mem::transmute(v) };

    // Vec::new() has cap=0, len=0, ptr=non-null (dangling)
    // We can't easily distinguish cap from len when both are 0,
    // but we can at least verify the size matches
    assert!(
        core::mem::size_of::<Vec<u8>>() == core::mem::size_of::<VecLayout>(),
        "VecLayout size mismatch"
    );
    assert!(
        core::mem::align_of::<Vec<u8>>() == core::mem::align_of::<VecLayout>(),
        "VecLayout align mismatch"
    );

    // The pointer field should be non-null even for empty vec (dangling pointer)
    // Fields 0 and 2 should be 0 (cap and len)
    // Field 1 should be non-zero (ptr)
    // This validates our layout: [cap, ptr, len]
    assert!(fields[0] == 0, "expected cap=0 at offset 0");
    assert!(fields[1] != 0, "expected non-null ptr at offset 1");
    assert!(fields[2] == 0, "expected len=0 at offset 2");
};

/// Type-erased len implementation - works for any `Vec<T>`
unsafe fn vec_len_erased(ptr: PtrConst) -> usize {
    unsafe {
        let layout = ptr.as_byte_ptr() as *const VecLayout;
        (*layout).len
    }
}

/// Type-erased get implementation - works for any `Vec<T>` using shape info
unsafe fn vec_get_erased(ptr: PtrConst, index: usize, shape: &'static Shape) -> Option<PtrConst> {
    unsafe {
        let layout = ptr.as_byte_ptr() as *const VecLayout;
        let len = (*layout).len;
        if index >= len {
            return None;
        }
        let elem_size = shape
            .type_params
            .first()?
            .shape
            .layout
            .sized_layout()
            .ok()?
            .size();
        let data_ptr = (*layout).ptr;
        Some(PtrConst::new(data_ptr.add(index * elem_size)))
    }
}

/// Type-erased get_mut implementation - works for any `Vec<T>` using shape info
unsafe fn vec_get_mut_erased(ptr: PtrMut, index: usize, shape: &'static Shape) -> Option<PtrMut> {
    unsafe {
        let layout = ptr.as_byte_ptr() as *const VecLayout;
        let len = (*layout).len;
        if index >= len {
            return None;
        }
        let elem_size = shape
            .type_params
            .first()?
            .shape
            .layout
            .sized_layout()
            .ok()?
            .size();
        let data_ptr = (*layout).ptr;
        Some(PtrMut::new(data_ptr.add(index * elem_size)))
    }
}

/// Shared ListVTable for ALL `Vec<T>` instantiations
///
/// This single vtable is used by every `Vec<T>` regardless of T, eliminating
/// the need to generate separate `vec_len`, `vec_get`, etc. functions for
/// each element type.
static VEC_LIST_VTABLE: ListVTable = ListVTable {
    len: vec_len_erased,
    get: vec_get_erased,
    get_mut: Some(vec_get_mut_erased),
    as_ptr: Some(vec_as_ptr_erased),
    as_mut_ptr: Some(vec_as_mut_ptr_erased),
};

/// Type-erased as_ptr implementation - works for any `Vec<T>`
unsafe fn vec_as_ptr_erased(ptr: PtrConst) -> PtrConst {
    unsafe {
        let layout = ptr.as_byte_ptr() as *const VecLayout;
        PtrConst::new((*layout).ptr)
    }
}

/// Type-erased as_mut_ptr implementation - works for any `Vec<T>`
unsafe fn vec_as_mut_ptr_erased(ptr: PtrMut) -> PtrMut {
    unsafe {
        let layout = ptr.as_byte_ptr() as *const VecLayout;
        PtrMut::new((*layout).ptr)
    }
}

/// Type-erased type_name implementation for Vec
fn vec_type_name(
    shape: &'static Shape,
    f: &mut core::fmt::Formatter<'_>,
    opts: TypeNameOpts,
) -> core::fmt::Result {
    write!(f, "Vec")?;
    if let Some(opts) = opts.for_children() {
        write!(f, "<")?;
        if let Some(tp) = shape.type_params.first() {
            tp.shape.write_type_name(f, opts)?;
        }
        write!(f, ">")?;
    } else {
        write!(f, "<…>")?;
    }
    Ok(())
}

/// Get the ListDef from a shape, panics if not a list
#[inline]
fn get_list_def(shape: &'static Shape) -> &'static ListDef {
    match &shape.def {
        Def::List(list_def) => list_def,
        _ => panic!("expected List def"),
    }
}

/// Type-erased debug implementation for Vec
unsafe fn vec_debug_erased(
    ox: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    let shape = ox.shape();
    let elem_shape = shape.type_params.first().map(|tp| tp.shape)?;
    if !elem_shape.vtable.has_debug() {
        return None;
    }

    let list_def = get_list_def(shape);
    let ptr = ox.ptr();
    let len = unsafe { (list_def.vtable.len)(ptr) };

    let mut list = f.debug_list();
    for i in 0..len {
        if let Some(item_ptr) = unsafe { (list_def.vtable.get)(ptr, i, shape) } {
            list.entry(&DebugViaShape(elem_shape, item_ptr));
        }
    }
    Some(list.finish())
}

/// Type-erased partial_eq implementation for Vec
unsafe fn vec_partial_eq_erased(ox_a: OxPtrConst, ox_b: OxPtrConst) -> Option<bool> {
    let shape = ox_a.shape();
    let elem_shape = shape.type_params.first().map(|tp| tp.shape)?;
    if !elem_shape.vtable.has_partial_eq() {
        return None;
    }

    let list_def = get_list_def(shape);
    let ptr_a = ox_a.ptr();
    let ptr_b = ox_b.ptr();
    let len_a = unsafe { (list_def.vtable.len)(ptr_a) };
    let len_b = unsafe { (list_def.vtable.len)(ptr_b) };

    if len_a != len_b {
        return Some(false);
    }

    for i in 0..len_a {
        let item_a = unsafe { (list_def.vtable.get)(ptr_a, i, shape) }?;
        let item_b = unsafe { (list_def.vtable.get)(ptr_b, i, shape) }?;
        match unsafe { elem_shape.call_partial_eq(item_a, item_b) } {
            Some(true) => continue,
            Some(false) => return Some(false),
            None => return None,
        }
    }
    Some(true)
}

/// Type-erased partial_cmp implementation for Vec
unsafe fn vec_partial_cmp_erased(
    ox_a: OxPtrConst,
    ox_b: OxPtrConst,
) -> Option<Option<core::cmp::Ordering>> {
    let shape = ox_a.shape();
    let elem_shape = shape.type_params.first().map(|tp| tp.shape)?;
    if !elem_shape.vtable.has_partial_ord() {
        return None;
    }

    let list_def = get_list_def(shape);
    let ptr_a = ox_a.ptr();
    let ptr_b = ox_b.ptr();
    let len_a = unsafe { (list_def.vtable.len)(ptr_a) };
    let len_b = unsafe { (list_def.vtable.len)(ptr_b) };

    let min_len = len_a.min(len_b);

    for i in 0..min_len {
        let item_a = unsafe { (list_def.vtable.get)(ptr_a, i, shape) }?;
        let item_b = unsafe { (list_def.vtable.get)(ptr_b, i, shape) }?;
        match unsafe { elem_shape.call_partial_cmp(item_a, item_b) } {
            Some(Some(core::cmp::Ordering::Equal)) => continue,
            Some(ord) => return Some(ord),
            None => return None,
        }
    }
    Some(Some(len_a.cmp(&len_b)))
}

/// Type-erased cmp implementation for Vec
unsafe fn vec_cmp_erased(ox_a: OxPtrConst, ox_b: OxPtrConst) -> Option<core::cmp::Ordering> {
    let shape = ox_a.shape();
    let elem_shape = shape.type_params.first().map(|tp| tp.shape)?;
    if !elem_shape.vtable.has_ord() {
        return None;
    }

    let list_def = get_list_def(shape);
    let ptr_a = ox_a.ptr();
    let ptr_b = ox_b.ptr();
    let len_a = unsafe { (list_def.vtable.len)(ptr_a) };
    let len_b = unsafe { (list_def.vtable.len)(ptr_b) };

    let min_len = len_a.min(len_b);

    for i in 0..min_len {
        let item_a = unsafe { (list_def.vtable.get)(ptr_a, i, shape) }?;
        let item_b = unsafe { (list_def.vtable.get)(ptr_b, i, shape) }?;
        match unsafe { elem_shape.call_cmp(item_a, item_b) } {
            Some(core::cmp::Ordering::Equal) => continue,
            Some(ord) => return Some(ord),
            None => return None,
        }
    }
    Some(len_a.cmp(&len_b))
}

// =============================================================================
// Generic functions that still need T
// =============================================================================

type VecIterator<'mem, T> = core::slice::Iter<'mem, T>;

unsafe fn vec_init_in_place_with_capacity<T>(uninit: PtrUninit, capacity: usize) -> PtrMut {
    unsafe { uninit.put(Vec::<T>::with_capacity(capacity)) }
}

unsafe fn vec_push<T: 'static>(ptr: PtrMut, item: PtrMut) {
    unsafe {
        let vec = ptr.as_mut::<Vec<T>>();
        let item = item.read::<T>();
        vec.push(item);
    }
}

/// Set the length of a Vec (for direct-fill operations).
///
/// # Safety
/// - `ptr` must point to an initialized `Vec<T>`
/// - `len` must not exceed the Vec's capacity
/// - All elements at indices `0..len` must be properly initialized
unsafe fn vec_set_len<T: 'static>(ptr: PtrMut, len: usize) {
    unsafe {
        let vec = ptr.as_mut::<Vec<T>>();
        vec.set_len(len);
    }
}

/// Get raw mutable pointer to Vec's data buffer as `*mut u8`.
///
/// # Safety
/// - `ptr` must point to an initialized `Vec<T>`
unsafe fn vec_as_mut_ptr_typed<T: 'static>(ptr: PtrMut) -> *mut u8 {
    unsafe {
        let vec = ptr.as_mut::<Vec<T>>();
        vec.as_mut_ptr() as *mut u8
    }
}

unsafe fn vec_iter_init<T: 'static>(ptr: PtrConst) -> PtrMut {
    unsafe {
        let vec = ptr.get::<Vec<T>>();
        let iter: VecIterator<T> = vec.iter();
        let iter_state = Box::new(iter);
        PtrMut::new(Box::into_raw(iter_state) as *mut u8)
    }
}

unsafe fn vec_iter_next<T: 'static>(iter_ptr: PtrMut) -> Option<PtrConst> {
    unsafe {
        let state = iter_ptr.as_mut::<VecIterator<'static, T>>();
        state.next().map(|value| PtrConst::new(value as *const T))
    }
}

unsafe fn vec_iter_next_back<T: 'static>(iter_ptr: PtrMut) -> Option<PtrConst> {
    unsafe {
        let state = iter_ptr.as_mut::<VecIterator<'static, T>>();
        state
            .next_back()
            .map(|value| PtrConst::new(value as *const T))
    }
}

unsafe fn vec_iter_dealloc<T>(iter_ptr: PtrMut) {
    unsafe {
        drop(Box::from_raw(
            iter_ptr.as_ptr::<VecIterator<'_, T>>() as *mut VecIterator<'_, T>
        ))
    }
}

unsafe impl<'a, T> Facet<'a> for Vec<T>
where
    T: Facet<'a> + 'static,
{
    const SHAPE: &'static Shape =
        &const {
            // Per-T operations that must be monomorphized
            const fn build_list_type_ops<T: 'static>() -> ListTypeOps {
                ListTypeOps::builder()
                    .init_in_place_with_capacity(vec_init_in_place_with_capacity::<T>)
                    .push(vec_push::<T>)
                    .set_len(vec_set_len::<T>)
                    .as_mut_ptr_typed(vec_as_mut_ptr_typed::<T>)
                    .iter_vtable(IterVTable {
                        init_with_value: Some(vec_iter_init::<T>),
                        next: vec_iter_next::<T>,
                        next_back: Some(vec_iter_next_back::<T>),
                        size_hint: None,
                        dealloc: vec_iter_dealloc::<T>,
                    })
                    .build()
            }

            ShapeBuilder::for_sized::<Self>("Vec")
            .type_name(vec_type_name)
            .ty(Type::User(UserType::Opaque))
            .def(Def::List(ListDef::with_type_ops(
                // Use the SHARED vtable for all Vec<T>!
                &VEC_LIST_VTABLE,
                &const { build_list_type_ops::<T>() },
                T::SHAPE,
            )))
            .type_params(&[TypeParam {
                name: "T",
                shape: T::SHAPE,
            }])
            .inner(T::SHAPE)
            // Vec<T> propagates T's variance
            .variance(Shape::computed_variance)
            .vtable_indirect(&const {
                VTableIndirect {
                    debug: Some(vec_debug_erased),
                    partial_eq: Some(vec_partial_eq_erased),
                    partial_cmp: Some(vec_partial_cmp_erased),
                    cmp: Some(vec_cmp_erased),
                    display: None,
                    hash: None,
                    invariants: None,
                    parse: None,
                    try_from: None,
                    try_into_inner: None,
                    try_borrow_inner: None,
                }
            })
            .type_ops_indirect(&const {
                unsafe fn drop_in_place<T>(ox: OxPtrMut) {
                    unsafe {
                        core::ptr::drop_in_place(ox.ptr().as_ptr::<Vec<T>>() as *mut Vec<T>);
                    }
                }

                unsafe fn default_in_place<T>(ox: OxPtrMut) {
                    unsafe { ox.ptr().as_uninit().put(Vec::<T>::new()) };
                }

                unsafe fn truthy<T>(ptr: PtrConst) -> bool {
                    !unsafe { ptr.get::<Vec<T>>() }.is_empty()
                }

                TypeOpsIndirect {
                    drop_in_place: drop_in_place::<T>,
                    default_in_place: Some(default_in_place::<T>),
                    clone_into: None,
                    is_truthy: Some(truthy::<T>),
                }
            })
            .build()
        };
}
