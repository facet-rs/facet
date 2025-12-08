/// The layout of pointers to DST is not guaranteed, so we try to detect it in a const-friendly way
pub(crate) enum PtrLayout {
    /// layout is { ptr, metadata }
    PtrFirst,
    /// layout is { metadata, ptr }
    PtrLast,
}

impl PtrLayout {
    pub(crate) const FOR_SLICE: Self = const {
        unsafe {
            // null slice pointer with non-zero length
            let ptr: *const [()] = core::ptr::slice_from_raw_parts(core::ptr::null::<()>(), 1);
            let ptr: [*const (); 2] = core::mem::transmute(ptr);

            // look for the null part
            if ptr[0].is_null() {
                // make sure the length is non-null
                assert!(!ptr[1].is_null());
                Self::PtrFirst
            } else {
                Self::PtrLast
            }
        }
    };

    pub(crate) const FOR_TRAIT: Self = const {
        unsafe {
            trait Trait {}
            impl Trait for () {}

            // null dyn Trait pointer with non-null vtable (has to point to at least size and alignment)
            let ptr: *const dyn Trait = core::ptr::null::<()>();
            let ptr: [*const (); 2] = core::mem::transmute(ptr);

            // look for the null part
            if ptr[0].is_null() {
                // make sure the vtable is non-null
                assert!(!ptr[1].is_null());
                Self::PtrFirst
            } else {
                Self::PtrLast
            }
        }
    };
}

pub(crate) const PTR_FIRST: bool = {
    match (PtrLayout::FOR_SLICE, PtrLayout::FOR_TRAIT) {
        (PtrLayout::PtrFirst, PtrLayout::PtrFirst) => true,
        (PtrLayout::PtrLast, PtrLayout::PtrLast) => false,
        _ => panic!(),
    }
};
