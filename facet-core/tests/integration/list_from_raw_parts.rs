//! Exercises `ListTypeOps::from_raw_parts` for `Vec<T>` through the same facet
//! API a type-erased engine front door uses: fetch the op (and the `len`/`as_ptr`
//! vtable functions) off the `ListDef`, hand it an engine-allocated, engine-filled
//! buffer, and read the resulting list back.
//!
//! This mirrors the phon typed-engine bridge: the engine owns the element buffer,
//! `from_raw_parts` adopts it into the list handle, and `len`/`as_ptr` read it out.

use core::alloc::Layout;
use facet_core::{Def, Facet, PtrConst, PtrMut, PtrUninit, Shape};

fn list_def(shape: &'static Shape) -> &'static facet_core::ListDef {
    match &shape.def {
        Def::List(d) => d,
        _ => panic!("expected a List def"),
    }
}

#[test]
fn vec_u32_from_raw_parts_adopts_engine_buffer() {
    let shape = <Vec<u32> as Facet>::SHAPE;
    let ld = list_def(shape);

    let from_raw_parts = ld
        .from_raw_parts()
        .expect("Vec<u32> must expose from_raw_parts");
    let as_ptr = ld.vtable.as_ptr.expect("Vec<u32> must expose as_ptr");
    let len_fn = ld.vtable.len;

    let elems: [u32; 4] = [1, 2, 999, 0xDEAD_BEEF];

    // Engine-owned allocation: allocate a buffer for the elements and fill it,
    // exactly as the typed engine's decode path does.
    let layout = Layout::array::<u32>(elems.len()).unwrap();
    let buf = unsafe { std::alloc::alloc(layout) } as *mut u32;
    assert!(!buf.is_null());
    for (i, &v) in elems.iter().enumerate() {
        unsafe { buf.add(i).write(v) };
    }

    // Adopt the buffer into a fresh Vec<u32> via from_raw_parts.
    let mut slot = std::mem::MaybeUninit::<Vec<u32>>::uninit();
    unsafe {
        from_raw_parts(
            PtrUninit::new(slot.as_mut_ptr().cast::<u8>()),
            PtrMut::new(buf.cast::<u8>()),
            elems.len(),
            elems.len(),
        );
    }
    let v = unsafe { slot.assume_init() };
    assert_eq!(v, elems.to_vec());

    // Read it back through the same vtable ops the encode path uses.
    let handle = PtrConst::new(core::ptr::from_ref(&v));
    let len = unsafe { len_fn(handle) };
    assert_eq!(len, elems.len());
    let data = unsafe { as_ptr(handle) }.as_byte_ptr() as *const u32;
    for (i, &expected) in elems.iter().enumerate() {
        assert_eq!(unsafe { *data.add(i) }, expected);
    }
}

#[test]
fn vec_u32_from_raw_parts_empty() {
    let shape = <Vec<u32> as Facet>::SHAPE;
    let ld = list_def(shape);
    let from_raw_parts = ld.from_raw_parts().unwrap();

    // The engine hands a dangling-but-aligned pointer with capacity 0 for an
    // empty sequence; from_raw_parts must not touch it.
    let dangling = core::mem::align_of::<u32>() as *mut u8;
    let mut slot = std::mem::MaybeUninit::<Vec<u32>>::uninit();
    unsafe {
        from_raw_parts(
            PtrUninit::new(slot.as_mut_ptr().cast::<u8>()),
            PtrMut::new(dangling),
            0,
            0,
        );
    }
    let v = unsafe { slot.assume_init() };
    assert!(v.is_empty());
}
