#![cfg(feature = "bstr")]

use alloc::boxed::Box;
use alloc::vec::Vec;
use core::ptr::NonNull;

use bstr::{BStr, BString};

use crate::{
    Def, Facet, IterVTable, ListDef, ListTypeOps, ListVTable, PtrConst, PtrMut, PtrUninit,
    SequenceType, Shape, ShapeBuilder, SliceDef, SliceType, SliceVTable, Type, TypeOpsDirect,
    TypeOpsIndirect, UserType, VTableDirect, VTableIndirect, type_ops_direct, vtable_direct,
};

type ByteIterator<'mem> = core::slice::Iter<'mem, u8>;

unsafe extern "C" fn bstring_len(ptr: PtrConst) -> usize {
    unsafe { ptr.get::<BString>().len() }
}

unsafe fn bstring_get(ptr: PtrConst, index: usize, _shape: &'static Shape) -> Option<PtrConst> {
    unsafe {
        let bytes = ptr.get::<BString>();
        let item = bytes.get(index)?;
        Some(PtrConst::new(item as *const u8))
    }
}

unsafe fn bstring_get_mut(ptr: PtrMut, index: usize, _shape: &'static Shape) -> Option<PtrMut> {
    unsafe {
        let bytes = ptr.as_mut::<BString>();
        let item = bytes.get_mut(index)?;
        Some(PtrMut::new(item as *mut u8 as *mut ()))
    }
}

unsafe extern "C" fn bstring_as_ptr(ptr: PtrConst) -> PtrConst {
    unsafe {
        let bytes = ptr.get::<BString>();
        PtrConst::new(NonNull::new_unchecked(bytes.as_ptr() as *mut u8).as_ptr() as *const ())
    }
}

unsafe extern "C" fn bstring_as_mut_ptr(ptr: PtrMut) -> PtrMut {
    unsafe {
        let bytes = ptr.as_mut::<BString>();
        PtrMut::new(NonNull::new_unchecked(bytes.as_mut_ptr()).as_ptr() as *mut ())
    }
}

unsafe extern "C" fn bstring_init_in_place_with_capacity(
    data: PtrUninit,
    capacity: usize,
) -> PtrMut {
    unsafe { data.put(BString::from(Vec::<u8>::with_capacity(capacity))) }
}

unsafe extern "C" fn bstring_push(ptr: PtrMut, item: PtrMut) {
    unsafe {
        let bytes = ptr.as_mut::<BString>();
        let item = item.read::<u8>();
        bytes.push(item);
    }
}

unsafe extern "C" fn bstring_pop(ptr: PtrMut, out: PtrUninit) -> bool {
    unsafe {
        let bytes = ptr.as_mut::<BString>();
        match bytes.pop() {
            Some(value) => {
                out.put(value);
                true
            }
            None => false,
        }
    }
}

unsafe extern "C" fn bstring_set_len(ptr: PtrMut, len: usize) {
    unsafe {
        let bytes = ptr.as_mut::<BString>();
        bytes.set_len(len);
    }
}

unsafe extern "C" fn bstring_as_mut_ptr_typed(ptr: PtrMut) -> *mut u8 {
    unsafe {
        let bytes = ptr.as_mut::<BString>();
        bytes.as_mut_ptr()
    }
}

unsafe extern "C" fn bstring_reserve(ptr: PtrMut, additional: usize) {
    unsafe {
        let bytes = ptr.as_mut::<BString>();
        bytes.reserve(additional);
    }
}

unsafe extern "C" fn bstring_capacity(ptr: PtrConst) -> usize {
    unsafe { ptr.get::<BString>().capacity() }
}

unsafe extern "C" fn bstring_from_raw_parts(
    list: PtrUninit,
    ptr: PtrMut,
    len: usize,
    capacity: usize,
) {
    unsafe {
        let data = ptr.as_mut_byte_ptr();
        let vec = Vec::<u8>::from_raw_parts(data, len, capacity);
        list.put(BString::from(vec));
    }
}

unsafe extern "C" fn byte_iter_init_from_bstring(ptr: PtrConst) -> PtrMut {
    unsafe {
        let bytes = ptr.get::<BString>();
        let iter: ByteIterator = bytes.iter();
        let iter_state = Box::new(iter);
        PtrMut::new(
            NonNull::new_unchecked(Box::into_raw(iter_state) as *mut u8).as_ptr() as *mut (),
        )
    }
}

unsafe fn byte_iter_next(iter_ptr: PtrMut) -> Option<PtrConst> {
    unsafe {
        let state = iter_ptr.as_mut::<ByteIterator<'static>>();
        state.next().map(|value| PtrConst::new(value as *const u8))
    }
}

unsafe fn byte_iter_next_back(iter_ptr: PtrMut) -> Option<PtrConst> {
    unsafe {
        let state = iter_ptr.as_mut::<ByteIterator<'static>>();
        state
            .next_back()
            .map(|value| PtrConst::new(value as *const u8))
    }
}

unsafe extern "C" fn byte_iter_dealloc(iter_ptr: PtrMut) {
    unsafe {
        drop(Box::from_raw(
            iter_ptr.as_ptr::<ByteIterator<'_>>() as *mut ByteIterator<'_>
        ));
    }
}

unsafe fn bstring_truthy(ptr: PtrConst) -> bool {
    !unsafe { ptr.get::<BString>() }.is_empty()
}

static BSTRING_TYPE_OPS: TypeOpsDirect = TypeOpsDirect {
    is_truthy: Some(bstring_truthy),
    ..type_ops_direct!(BString => Default, Clone)
};

static BSTRING_LIST_VTABLE: ListVTable = ListVTable {
    len: bstring_len,
    get: bstring_get,
    get_mut: Some(bstring_get_mut),
    as_ptr: Some(bstring_as_ptr),
    as_mut_ptr: Some(bstring_as_mut_ptr),
    swap: None,
};

static BSTRING_LIST_TYPE_OPS: ListTypeOps = ListTypeOps {
    init_in_place_with_capacity: Some(bstring_init_in_place_with_capacity),
    push: Some(bstring_push),
    pop: Some(bstring_pop),
    set_len: Some(bstring_set_len),
    as_mut_ptr_typed: Some(bstring_as_mut_ptr_typed),
    reserve: Some(bstring_reserve),
    capacity: Some(bstring_capacity),
    from_raw_parts: Some(bstring_from_raw_parts),
    iter_vtable: IterVTable {
        init_with_value: Some(byte_iter_init_from_bstring),
        next: byte_iter_next,
        next_back: Some(byte_iter_next_back),
        size_hint: None,
        dealloc: byte_iter_dealloc,
    },
};

unsafe impl Facet<'_> for BString {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableDirect =
            vtable_direct!(BString => Display, Debug, Hash, PartialEq, PartialOrd, Ord,);

        ShapeBuilder::for_sized::<BString>("BString")
            .module_path("bstr")
            .ty(Type::User(UserType::Opaque))
            .def(Def::List(ListDef::with_type_ops(
                &BSTRING_LIST_VTABLE,
                &BSTRING_LIST_TYPE_OPS,
                u8::SHAPE,
            )))
            .vtable_direct(&VTABLE)
            .type_ops_direct(&BSTRING_TYPE_OPS)
            .send()
            .sync()
            .build()
    };
}

unsafe extern "C" fn bstr_len(ptr: PtrConst) -> usize {
    unsafe { ptr.get::<BStr>().len() }
}

unsafe extern "C" fn bstr_as_ptr(ptr: PtrConst) -> PtrConst {
    unsafe {
        let bytes = ptr.get::<BStr>();
        PtrConst::new(NonNull::new_unchecked(bytes.as_ptr() as *mut u8).as_ptr() as *const ())
    }
}

unsafe extern "C" fn bstr_as_mut_ptr(ptr: PtrMut) -> PtrMut {
    unsafe {
        let bytes = ptr.as_mut::<BStr>();
        PtrMut::new(NonNull::new_unchecked(bytes.as_mut_ptr()).as_ptr() as *mut ())
    }
}

unsafe fn bstr_truthy(ptr: PtrConst) -> bool {
    !unsafe { ptr.get::<BStr>() }.is_empty()
}

unsafe impl<'a> Facet<'a> for BStr {
    const SHAPE: &'static Shape = &const {
        const SLICE_VTABLE: SliceVTable = SliceVTable {
            len: bstr_len,
            as_ptr: bstr_as_ptr,
            as_mut_ptr: bstr_as_mut_ptr,
        };

        ShapeBuilder::for_unsized::<BStr>("BStr")
            .module_path("bstr")
            .ty(Type::Sequence(SequenceType::Slice(SliceType {
                t: u8::SHAPE,
            })))
            .def(Def::Slice(SliceDef::new(&SLICE_VTABLE, u8::SHAPE)))
            .vtable_indirect(&const { VTableIndirect::EMPTY })
            .type_ops_indirect(
                &const {
                    TypeOpsIndirect {
                        drop_in_place: |_| {},
                        default_in_place: None,
                        clone_into: None,
                        is_truthy: Some(bstr_truthy),
                    }
                },
            )
            .send()
            .sync()
            .build()
    };
}
