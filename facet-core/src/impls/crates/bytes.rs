#![cfg(feature = "bytes")]

use core::ptr::NonNull;

use alloc::boxed::Box;

use bytes::{BufMut as _, Bytes, BytesMut};

use crate::{
    Def, Facet, IterVTable, ListDef, ListTypeOps, ListVTable, PtrConst, PtrMut, PtrUninit, Shape,
    ShapeBuilder, Type, UserType, VTableDirect,
};

type BytesIterator<'mem> = core::slice::Iter<'mem, u8>;

// =============================================================================
// Bytes implementation
// =============================================================================

fn bytes_display(_bytes: &Bytes, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    write!(f, "Bytes")
}

fn bytes_debug(bytes: &Bytes, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    f.debug_tuple("Bytes")
        .field(&alloc::format!("[{} bytes]", bytes.len()))
        .finish()
}

unsafe fn bytes_len(ptr: PtrConst) -> usize {
    unsafe { ptr.get::<Bytes>().len() }
}

unsafe fn bytes_get(ptr: PtrConst, index: usize, _shape: &'static Shape) -> Option<PtrConst> {
    unsafe {
        let bytes = ptr.get::<Bytes>();
        let item = bytes.get(index)?;
        Some(PtrConst::new(item as *const u8))
    }
}

unsafe fn bytes_as_ptr(ptr: PtrConst) -> PtrConst {
    unsafe {
        let bytes: &Bytes = ptr.get::<Bytes>();
        PtrConst::new(NonNull::new_unchecked(bytes.as_ptr() as *mut u8).as_ptr() as *const ())
    }
}

unsafe fn bytes_iter_init(ptr: PtrConst) -> PtrMut {
    unsafe {
        let bytes = ptr.get::<Bytes>();
        let iter: BytesIterator = bytes.iter();
        let iter_state = Box::new(iter);
        PtrMut::new(
            NonNull::new_unchecked(Box::into_raw(iter_state) as *mut u8).as_ptr() as *mut (),
        )
    }
}

unsafe fn bytes_iter_next(iter_ptr: PtrMut) -> Option<PtrConst> {
    unsafe {
        let state = iter_ptr.as_mut::<BytesIterator<'static>>();
        state.next().map(|value| PtrConst::new(value as *const u8))
    }
}

unsafe fn bytes_iter_next_back(iter_ptr: PtrMut) -> Option<PtrConst> {
    unsafe {
        let state = iter_ptr.as_mut::<BytesIterator<'static>>();
        state
            .next_back()
            .map(|value| PtrConst::new(value as *const u8))
    }
}

unsafe fn bytes_iter_dealloc(iter_ptr: PtrMut) {
    unsafe {
        drop(Box::from_raw(
            iter_ptr.as_ptr::<BytesIterator<'_>>() as *mut BytesIterator<'_>
        ));
    }
}

static BYTES_LIST_VTABLE: ListVTable = ListVTable {
    len: bytes_len,
    get: bytes_get,
    get_mut: None,
    as_ptr: Some(bytes_as_ptr),
    as_mut_ptr: None,
};

static BYTES_LIST_TYPE_OPS: ListTypeOps = ListTypeOps {
    init_in_place_with_capacity: None,
    push: None,
    set_len: None,
    as_mut_ptr_typed: None,
    reserve: None,
    capacity: None,
    iter_vtable: IterVTable {
        init_with_value: Some(bytes_iter_init),
        next: bytes_iter_next,
        next_back: Some(bytes_iter_next_back),
        size_hint: None,
        dealloc: bytes_iter_dealloc,
    },
};

/// # Safety
/// `src` must point to a valid `BytesMut`, `dst` must be valid for writes
unsafe fn bytes_try_from(
    dst: *mut Bytes,
    _src_shape: &'static crate::Shape,
    src: crate::PtrConst,
) -> Result<(), alloc::string::String> {
    unsafe {
        // Read the BytesMut (consuming it) and freeze into Bytes
        let bytes_mut = core::ptr::read(src.as_byte_ptr() as *const BytesMut);
        let bytes = bytes_mut.freeze();
        dst.write(bytes);
    }
    Ok(())
}

unsafe impl Facet<'_> for Bytes {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = VTableDirect::builder_for::<Bytes>()
            .display(bytes_display)
            .debug(bytes_debug)
            // Convert from BytesMut (builder type) to Bytes
            .try_from(bytes_try_from)
            .build();

        ShapeBuilder::for_sized::<Bytes>("Bytes")
            .ty(Type::User(UserType::Opaque))
            .def(Def::List(ListDef::with_type_ops(
                &BYTES_LIST_VTABLE,
                &BYTES_LIST_TYPE_OPS,
                u8::SHAPE,
            )))
            .builder_shape(BytesMut::SHAPE)
            .vtable_direct(&VTABLE)
            .send()
            .sync()
            .build()
    };
}

// =============================================================================
// BytesMut implementation
// =============================================================================

fn bytes_mut_display(_bytes: &BytesMut, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    write!(f, "BytesMut")
}

fn bytes_mut_debug(bytes: &BytesMut, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
    f.debug_tuple("BytesMut")
        .field(&alloc::format!("[{} bytes]", bytes.len()))
        .finish()
}

unsafe fn bytes_mut_len(ptr: PtrConst) -> usize {
    unsafe { ptr.get::<BytesMut>().len() }
}

unsafe fn bytes_mut_get(ptr: PtrConst, index: usize, _shape: &'static Shape) -> Option<PtrConst> {
    unsafe {
        let bytes = ptr.get::<BytesMut>();
        let item = bytes.get(index)?;
        Some(PtrConst::new(item as *const u8))
    }
}

unsafe fn bytes_mut_get_mut(ptr: PtrMut, index: usize, _shape: &'static Shape) -> Option<PtrMut> {
    unsafe {
        let bytes = ptr.as_mut::<BytesMut>();
        let item = bytes.get_mut(index)?;
        Some(PtrMut::new(item as *mut u8 as *mut ()))
    }
}

unsafe fn bytes_mut_as_ptr(ptr: PtrConst) -> PtrConst {
    unsafe {
        let bytes = ptr.get::<BytesMut>();
        PtrConst::new(NonNull::new_unchecked(bytes.as_ptr() as *mut u8).as_ptr() as *const ())
    }
}

unsafe fn bytes_mut_as_mut_ptr(ptr: PtrMut) -> PtrMut {
    unsafe {
        let bytes = ptr.as_mut::<BytesMut>();
        PtrMut::new(NonNull::new_unchecked(bytes.as_mut_ptr()).as_ptr() as *mut ())
    }
}

unsafe fn bytes_mut_init_in_place_with_capacity(data: PtrUninit, capacity: usize) -> PtrMut {
    unsafe { data.put(BytesMut::with_capacity(capacity)) }
}

unsafe fn bytes_mut_push(ptr: PtrMut, item: PtrMut) {
    unsafe {
        let bytes = ptr.as_mut::<BytesMut>();
        let item = item.read::<u8>();
        (*bytes).put_u8(item);
    }
}

unsafe fn bytes_mut_iter_init(ptr: PtrConst) -> PtrMut {
    unsafe {
        let bytes = ptr.get::<BytesMut>();
        let iter: BytesIterator = bytes.iter();
        let iter_state = Box::new(iter);
        PtrMut::new(
            NonNull::new_unchecked(Box::into_raw(iter_state) as *mut u8).as_ptr() as *mut (),
        )
    }
}

static BYTES_MUT_LIST_VTABLE: ListVTable = ListVTable {
    len: bytes_mut_len,
    get: bytes_mut_get,
    get_mut: Some(bytes_mut_get_mut),
    as_ptr: Some(bytes_mut_as_ptr),
    as_mut_ptr: Some(bytes_mut_as_mut_ptr),
};

static BYTES_MUT_LIST_TYPE_OPS: ListTypeOps = ListTypeOps {
    init_in_place_with_capacity: Some(bytes_mut_init_in_place_with_capacity),
    push: Some(bytes_mut_push),
    set_len: None, // BytesMut has different semantics - not supported for direct-fill
    as_mut_ptr_typed: None,
    reserve: None,
    capacity: None,
    iter_vtable: IterVTable {
        init_with_value: Some(bytes_mut_iter_init),
        next: bytes_iter_next, // Reuse from Bytes - same iterator type
        next_back: Some(bytes_iter_next_back),
        size_hint: None,
        dealloc: bytes_iter_dealloc,
    },
};

unsafe impl Facet<'_> for BytesMut {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect = VTableDirect::builder_for::<BytesMut>()
            .display(bytes_mut_display)
            .debug(bytes_mut_debug)
            .build();

        ShapeBuilder::for_sized::<BytesMut>("BytesMut")
            .ty(Type::User(UserType::Opaque))
            .def(Def::List(ListDef::with_type_ops(
                &BYTES_MUT_LIST_VTABLE,
                &BYTES_MUT_LIST_TYPE_OPS,
                u8::SHAPE,
            )))
            .vtable_direct(&VTABLE)
            .send()
            .sync()
            .build()
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_bytes_shape() {
        // Basic shape verification - detailed peek tests are in facet-reflect
        let shape = <Bytes as Facet>::SHAPE;
        assert_eq!(shape.type_identifier, "Bytes");

        let shape_mut = <BytesMut as Facet>::SHAPE;
        assert_eq!(shape_mut.type_identifier, "BytesMut");
    }
}
