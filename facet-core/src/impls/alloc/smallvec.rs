#![cfg(feature = "smallvec")]

use crate::*;

use alloc::boxed::Box;
use smallvec::{Array, SmallVec};

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
// Type-specific vtable functions for SmallVec<A>
// =============================================================================

/// Type-erased type_name implementation for SmallVec
fn smallvec_type_name<A: Array>(
    shape: &'static Shape,
    f: &mut core::fmt::Formatter<'_>,
    opts: TypeNameOpts,
) -> core::fmt::Result {
    write!(f, "SmallVec")?;
    if let Some(opts) = opts.for_children() {
        write!(f, "<[")?;
        if let Some(tp) = shape.type_params.first() {
            tp.shape.write_type_name(f, opts)?;
        }
        write!(f, "; {}]>", A::size())?;
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

/// Type-erased debug implementation for SmallVec
unsafe fn smallvec_debug_erased(
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

/// Type-erased partial_eq implementation for SmallVec
unsafe fn smallvec_partial_eq_erased(ox_a: OxPtrConst, ox_b: OxPtrConst) -> Option<bool> {
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

/// Type-erased partial_cmp implementation for SmallVec
unsafe fn smallvec_partial_cmp_erased(
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

/// Type-erased cmp implementation for SmallVec
unsafe fn smallvec_cmp_erased(ox_a: OxPtrConst, ox_b: OxPtrConst) -> Option<core::cmp::Ordering> {
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
// Generic functions that need the array type A
// =============================================================================

type SmallVecIterator<'mem, T> = core::slice::Iter<'mem, T>;

unsafe fn smallvec_len<A: Array>(ptr: PtrConst) -> usize {
    unsafe { ptr.get::<SmallVec<A>>().len() }
}

unsafe fn smallvec_get<A: Array>(
    ptr: PtrConst,
    index: usize,
    _shape: &'static Shape,
) -> Option<PtrConst> {
    unsafe {
        let sv = ptr.get::<SmallVec<A>>();
        sv.get(index)
            .map(|item| PtrConst::new(item as *const A::Item))
    }
}

unsafe fn smallvec_get_mut<A: Array>(
    ptr: PtrMut,
    index: usize,
    _shape: &'static Shape,
) -> Option<PtrMut> {
    unsafe {
        let sv = ptr.as_mut::<SmallVec<A>>();
        sv.get_mut(index)
            .map(|item| PtrMut::new(item as *mut A::Item))
    }
}

unsafe fn smallvec_as_ptr<A: Array>(ptr: PtrConst) -> PtrConst {
    unsafe {
        let sv = ptr.get::<SmallVec<A>>();
        PtrConst::new(sv.as_ptr() as *const u8)
    }
}

unsafe fn smallvec_as_mut_ptr<A: Array>(ptr: PtrMut) -> PtrMut {
    unsafe {
        let sv = ptr.as_mut::<SmallVec<A>>();
        PtrMut::new(sv.as_mut_ptr() as *mut u8)
    }
}

unsafe fn smallvec_init_in_place_with_capacity<A: Array>(
    uninit: PtrUninit,
    capacity: usize,
) -> PtrMut {
    unsafe { uninit.put(SmallVec::<A>::with_capacity(capacity)) }
}

unsafe fn smallvec_push<A: Array>(ptr: PtrMut, item: PtrMut) {
    unsafe {
        let sv = ptr.as_mut::<SmallVec<A>>();
        let item = item.read::<A::Item>();
        sv.push(item);
    }
}

/// Set the length of a SmallVec (for direct-fill operations).
///
/// # Safety
/// - `ptr` must point to an initialized `SmallVec<A>`
/// - `len` must not exceed the SmallVec's capacity
/// - All elements at indices `0..len` must be properly initialized
unsafe fn smallvec_set_len<A: Array>(ptr: PtrMut, len: usize) {
    unsafe {
        let sv = ptr.as_mut::<SmallVec<A>>();
        sv.set_len(len);
    }
}

/// Get raw mutable pointer to SmallVec's data buffer as `*mut u8`.
///
/// # Safety
/// - `ptr` must point to an initialized `SmallVec<A>`
unsafe fn smallvec_as_mut_ptr_typed<A: Array>(ptr: PtrMut) -> *mut u8 {
    unsafe {
        let sv = ptr.as_mut::<SmallVec<A>>();
        sv.as_mut_ptr() as *mut u8
    }
}

/// Reserve capacity for at least `additional` more elements.
///
/// # Safety
/// - `ptr` must point to an initialized `SmallVec<A>`
unsafe fn smallvec_reserve<A: Array>(ptr: PtrMut, additional: usize) {
    unsafe {
        let sv = ptr.as_mut::<SmallVec<A>>();
        sv.reserve(additional);
    }
}

/// Get the current capacity of the SmallVec.
///
/// # Safety
/// - `ptr` must point to an initialized `SmallVec<A>`
unsafe fn smallvec_capacity<A: Array>(ptr: PtrConst) -> usize {
    unsafe {
        let sv = ptr.get::<SmallVec<A>>();
        sv.capacity()
    }
}

unsafe fn smallvec_iter_init<A: Array>(ptr: PtrConst) -> PtrMut {
    unsafe {
        let sv = ptr.get::<SmallVec<A>>();
        let iter: SmallVecIterator<A::Item> = sv.iter();
        let iter_state = Box::new(iter);
        PtrMut::new(Box::into_raw(iter_state) as *mut u8)
    }
}

unsafe fn smallvec_iter_next<A: Array>(iter_ptr: PtrMut) -> Option<PtrConst>
where
    A::Item: 'static,
{
    unsafe {
        let state = iter_ptr.as_mut::<SmallVecIterator<'static, A::Item>>();
        state
            .next()
            .map(|value| PtrConst::new(value as *const A::Item))
    }
}

unsafe fn smallvec_iter_next_back<A: Array>(iter_ptr: PtrMut) -> Option<PtrConst>
where
    A::Item: 'static,
{
    unsafe {
        let state = iter_ptr.as_mut::<SmallVecIterator<'static, A::Item>>();
        state
            .next_back()
            .map(|value| PtrConst::new(value as *const A::Item))
    }
}

unsafe fn smallvec_iter_dealloc<A: Array>(iter_ptr: PtrMut) {
    unsafe {
        drop(Box::from_raw(
            iter_ptr.as_ptr::<SmallVecIterator<'_, A::Item>>()
                as *mut SmallVecIterator<'_, A::Item>,
        ))
    }
}

unsafe impl<'a, A> Facet<'a> for SmallVec<A>
where
    A: Array + 'static,
    A::Item: Facet<'a> + 'static,
{
    const SHAPE: &'static Shape = &const {
        // Per-A vtable (since SmallVec's layout depends on A)
        const fn build_list_vtable<A: Array + 'static>() -> ListVTable {
            ListVTable {
                len: smallvec_len::<A>,
                get: smallvec_get::<A>,
                get_mut: Some(smallvec_get_mut::<A>),
                as_ptr: Some(smallvec_as_ptr::<A>),
                as_mut_ptr: Some(smallvec_as_mut_ptr::<A>),
            }
        }

        // Per-A operations that must be monomorphized
        const fn build_list_type_ops<A: Array + 'static>() -> ListTypeOps {
            ListTypeOps::builder()
                .init_in_place_with_capacity(smallvec_init_in_place_with_capacity::<A>)
                .push(smallvec_push::<A>)
                .set_len(smallvec_set_len::<A>)
                .as_mut_ptr_typed(smallvec_as_mut_ptr_typed::<A>)
                .reserve(smallvec_reserve::<A>)
                .capacity(smallvec_capacity::<A>)
                .iter_vtable(IterVTable {
                    init_with_value: Some(smallvec_iter_init::<A>),
                    next: smallvec_iter_next::<A>,
                    next_back: Some(smallvec_iter_next_back::<A>),
                    size_hint: None,
                    dealloc: smallvec_iter_dealloc::<A>,
                })
                .build()
        }

        ShapeBuilder::for_sized::<Self>("SmallVec")
            .module_path("smallvec")
            .type_name(smallvec_type_name::<A>)
            .ty(Type::User(UserType::Opaque))
            .def(Def::List(ListDef::with_type_ops(
                &const { build_list_vtable::<A>() },
                &const { build_list_type_ops::<A>() },
                <A::Item as Facet<'a>>::SHAPE,
            )))
            .type_params(&[TypeParam {
                name: "T",
                shape: <A::Item as Facet<'a>>::SHAPE,
            }])
            .inner(<A::Item as Facet<'a>>::SHAPE)
            // SmallVec<A> propagates A::Item's variance
            .variance(VarianceDesc {
                base: Variance::Bivariant,
                deps: &const { [VarianceDep::covariant(<A::Item as Facet<'a>>::SHAPE)] },
            })
            .vtable_indirect(
                &const {
                    VTableIndirect {
                        debug: Some(smallvec_debug_erased),
                        partial_eq: Some(smallvec_partial_eq_erased),
                        partial_cmp: Some(smallvec_partial_cmp_erased),
                        cmp: Some(smallvec_cmp_erased),
                        display: None,
                        hash: None,
                        invariants: None,
                        parse: None,
                        parse_bytes: None,
                        try_from: None,
                        try_into_inner: None,
                        try_borrow_inner: None,
                    }
                },
            )
            .type_ops_indirect(
                &const {
                    unsafe fn drop_in_place<A: Array>(ox: OxPtrMut) {
                        unsafe {
                            core::ptr::drop_in_place(
                                ox.ptr().as_ptr::<SmallVec<A>>() as *mut SmallVec<A>
                            );
                        }
                    }

                    unsafe fn default_in_place<A: Array>(ox: OxPtrUninit) {
                        unsafe { ox.put(SmallVec::<A>::new()) };
                    }

                    unsafe fn truthy<A: Array>(ptr: PtrConst) -> bool {
                        !unsafe { ptr.get::<SmallVec<A>>() }.is_empty()
                    }

                    TypeOpsIndirect {
                        drop_in_place: drop_in_place::<A>,
                        default_in_place: Some(default_in_place::<A>),
                        clone_into: None,
                        is_truthy: Some(truthy::<A>),
                    }
                },
            )
            .build()
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_smallvec_type_params() {
        let [type_param_1] = <SmallVec<[i32; 4]>>::SHAPE.type_params else {
            panic!("SmallVec<[T; N]> should have 1 type param")
        };
        assert_eq!(type_param_1.shape(), i32::SHAPE);
    }

    #[test]
    fn test_smallvec_is_list() {
        let shape = <SmallVec<[i32; 4]>>::SHAPE;
        let list_def = shape
            .def
            .into_list()
            .expect("SmallVec should have a List def");
        assert_eq!(list_def.t(), i32::SHAPE);
    }

    #[test]
    fn test_smallvec_list_ops() {
        facet_testhelpers::setup();

        let shape = <SmallVec<[i32; 4]>>::SHAPE;
        let list_def = shape
            .def
            .into_list()
            .expect("SmallVec should have a List def");

        // Create a SmallVec
        let sv: SmallVec<[i32; 4]> = smallvec::smallvec![1, 2, 3];

        // Test len
        let ptr = PtrConst::new(&sv as *const _ as *const u8);
        let len = unsafe { (list_def.vtable.len)(ptr) };
        assert_eq!(len, 3);

        // Test get
        let elem = unsafe { (list_def.vtable.get)(ptr, 1, shape) };
        assert!(elem.is_some());
        let val = unsafe { *elem.unwrap().get::<i32>() };
        assert_eq!(val, 2);

        // Test get out of bounds
        let elem = unsafe { (list_def.vtable.get)(ptr, 10, shape) };
        assert!(elem.is_none());
    }

    #[test]
    fn test_smallvec_partial_eq() {
        facet_testhelpers::setup();

        let sv1: SmallVec<[i32; 4]> = smallvec::smallvec![1, 2, 3];
        let sv2: SmallVec<[i32; 4]> = smallvec::smallvec![1, 2, 3];
        let sv3: SmallVec<[i32; 4]> = smallvec::smallvec![1, 2, 4];

        let shape = <SmallVec<[i32; 4]>>::SHAPE;
        let ptr1 = PtrConst::new(&sv1 as *const _ as *const u8);
        let ptr2 = PtrConst::new(&sv2 as *const _ as *const u8);
        let ptr3 = PtrConst::new(&sv3 as *const _ as *const u8);

        let ox1 = OxPtrConst::new(ptr1, shape);
        let ox2 = OxPtrConst::new(ptr2, shape);
        let ox3 = OxPtrConst::new(ptr3, shape);

        let result = unsafe { smallvec_partial_eq_erased(ox1, ox2) };
        assert_eq!(result, Some(true));

        let result = unsafe { smallvec_partial_eq_erased(ox1, ox3) };
        assert_eq!(result, Some(false));
    }
}
