use core::ptr::NonNull;

use crate::{
    Def, Facet, HashProxy, KnownPointer, OxPtrConst, PointerDef, PointerFlags, PointerVTable,
    PtrConst, PtrMut, PtrUninit, Shape, ShapeBuilder, Type, TypeParam, UserType, VTableIndirect,
};

// Debug for NonNull<T> - just prints the pointer value
unsafe fn nonnull_debug(
    ox: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    let ptr = ox.ptr();
    // Read the NonNull<T> which is just a pointer
    let inner_ptr = unsafe { *(ptr.as_byte_ptr() as *const *const u8) };
    Some(write!(f, "{inner_ptr:p}"))
}

// Hash for NonNull<T> - hash the pointer value
unsafe fn nonnull_hash(ox: OxPtrConst, hasher: &mut HashProxy<'_>) -> Option<()> {
    use core::hash::Hasher;
    let ptr = ox.ptr();
    let inner_ptr = unsafe { *(ptr.as_byte_ptr() as *const *const u8) };
    hasher.write_usize(inner_ptr as usize);
    Some(())
}

// PartialEq for NonNull<T> - compare pointer values
unsafe fn nonnull_partial_eq(a: OxPtrConst, b: OxPtrConst) -> Option<bool> {
    let a_ptr = a.ptr();
    let b_ptr = b.ptr();
    let a_inner = unsafe { *(a_ptr.as_byte_ptr() as *const *const u8) };
    let b_inner = unsafe { *(b_ptr.as_byte_ptr() as *const *const u8) };
    Some(a_inner == b_inner)
}

// PartialOrd for NonNull<T> - compare pointer values
unsafe fn nonnull_partial_cmp(a: OxPtrConst, b: OxPtrConst) -> Option<Option<core::cmp::Ordering>> {
    let a_ptr = a.ptr();
    let b_ptr = b.ptr();
    let a_inner = unsafe { *(a_ptr.as_byte_ptr() as *const *const u8) };
    let b_inner = unsafe { *(b_ptr.as_byte_ptr() as *const *const u8) };
    Some(a_inner.partial_cmp(&b_inner))
}

// Ord for NonNull<T> - compare pointer values
unsafe fn nonnull_cmp(a: OxPtrConst, b: OxPtrConst) -> Option<core::cmp::Ordering> {
    let a_ptr = a.ptr();
    let b_ptr = b.ptr();
    let a_inner = unsafe { *(a_ptr.as_byte_ptr() as *const *const u8) };
    let b_inner = unsafe { *(b_ptr.as_byte_ptr() as *const *const u8) };
    Some(a_inner.cmp(&b_inner))
}

// Shared vtable for all NonNull<T>
const NONNULL_VTABLE: VTableIndirect = VTableIndirect {
    display: None,
    debug: Some(nonnull_debug),
    hash: Some(nonnull_hash),
    invariants: None,
    parse: None,
    parse_bytes: None,
    try_from: None,
    try_into_inner: None,
    try_borrow_inner: None,
    partial_eq: Some(nonnull_partial_eq),
    partial_cmp: Some(nonnull_partial_cmp),
    cmp: Some(nonnull_cmp),
};

// Named function for borrow_fn
unsafe fn borrow_fn<'a, T: Facet<'a>>(this: PtrConst) -> PtrConst {
    unsafe {
        let ptr = this.get::<NonNull<T>>();
        PtrConst::new(ptr.as_ptr())
    }
}

// Named function for new_into_fn
unsafe fn new_into_fn<'a, 'ptr, T: Facet<'a>>(this: PtrUninit, ptr: PtrMut) -> PtrMut {
    unsafe {
        let raw_ptr = ptr.read::<*mut T>();
        let non_null = core::ptr::NonNull::new_unchecked(raw_ptr);
        this.put(non_null)
    }
}

unsafe impl<'a, T: Facet<'a>> Facet<'a> for core::ptr::NonNull<T> {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<Self>("NonNull")
            .decl_id(crate::DeclId::new(crate::decl_id_hash("NonNull")))
            .ty(Type::User(UserType::Opaque))
            .def(Def::Pointer(PointerDef {
                vtable: &const {
                    PointerVTable {
                        borrow_fn: Some(borrow_fn::<T>),
                        new_into_fn: Some(new_into_fn::<T>),
                        ..PointerVTable::new()
                    }
                },
                pointee: Some(T::SHAPE),
                weak: None,
                strong: None,
                flags: PointerFlags::EMPTY,
                known: Some(KnownPointer::NonNull),
            }))
            .type_params(&[TypeParam {
                name: "T",
                shape: T::SHAPE,
            }])
            .vtable_indirect(&NONNULL_VTABLE)
            .eq()
            .copy()
            .build()
    };
}
