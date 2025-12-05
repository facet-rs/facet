// might come in handy later for the derive macro and ecosystem support
#![allow(dead_code)]
#![allow(unused_imports)]

use core::cmp::Ordering;
use core::fmt;
use core::hash::Hasher;
use core::ptr::NonNull;

use crate::*;

pub const fn has_hash(fields: &[Shape]) -> bool {
    let mut fields = fields;
    while let Some((field, next)) = fields.split_first() {
        if !field.vtable.has_hash() {
            return false;
        }
        fields = next;
    }
    true
}

pub const fn has_partial_eq(fields: &[Shape]) -> bool {
    let mut fields = fields;
    while let Some((field, next)) = fields.split_first() {
        if !field.vtable.has_partial_eq() {
            return false;
        }
        fields = next;
    }
    true
}

pub const fn has_partial_ord(fields: &[Shape]) -> bool {
    let mut fields = fields;
    while let Some((field, next)) = fields.split_first() {
        if !field.vtable.has_partial_ord() {
            return false;
        }
        fields = next;
    }
    true
}

pub const fn has_debug(fields: &[Shape]) -> bool {
    let mut fields = fields;
    while let Some((field, next)) = fields.split_first() {
        if !field.vtable.has_debug() {
            return false;
        }
        fields = next;
    }
    true
}

pub unsafe fn hash_slice(slice: PtrConst, hasher: &mut dyn Hasher, t: &Shape) {
    unsafe {
        let ptr = slice.as_ptr::<[()]>();
        let len = ptr.len();
        let ptr = NonNull::new_unchecked(ptr as *mut ());
        let sizeof = t.layout.sized_layout().unwrap_unchecked().size();
        let hash = t.vtable.hash.hash.unwrap_unchecked();

        for i in 0..len {
            let ptr = ptr.byte_add(sizeof * i);
            hash(PtrConst::new(ptr), hasher);
        }
    }
}

pub unsafe fn hash_fields(ptr: PtrConst, fields: &[Field], hasher: &mut dyn Hasher) {
    for field in fields {
        unsafe {
            let ptr = ptr.field(field.offset);
            let f = field.shape().vtable.hash.hash.unwrap();
            f(ptr, hasher);
        }
    }
}

pub unsafe fn partial_eq_slice(lhs: PtrConst, rhs: PtrConst, t: &Shape) -> bool {
    unsafe {
        let lhs = lhs.as_ptr::<[()]>();
        let rhs = rhs.as_ptr::<[()]>();
        if lhs.len() != rhs.len() {
            return false;
        }

        let len = lhs.len();

        let lhs = NonNull::new_unchecked(lhs as *mut ());
        let rhs = NonNull::new_unchecked(rhs as *mut ());

        let sizeof = t.layout.sized_layout().unwrap_unchecked().size();
        let f = t.vtable.cmp.partial_eq.unwrap_unchecked();
        for i in 0..len {
            let lhs = lhs.byte_add(sizeof * i);
            let rhs = rhs.byte_add(sizeof * i);
            if !f(PtrConst::new(lhs), PtrConst::new(rhs)) {
                return false;
            }
        }
        true
    }
}

pub unsafe fn partial_eq_fields(lhs: PtrConst, rhs: PtrConst, fields: &[Field]) -> bool {
    for field in fields {
        unsafe {
            let lhs = lhs.field(field.offset);
            let rhs = rhs.field(field.offset);
            let f = field.shape().vtable.cmp.partial_eq.unwrap();
            if !f(lhs, rhs) {
                return false;
            }
        }
    }
    true
}

pub unsafe fn partial_ord_slice(lhs: PtrConst, rhs: PtrConst, t: &Shape) -> Option<Ordering> {
    unsafe {
        let lhs = lhs.as_ptr::<[()]>();
        let rhs = rhs.as_ptr::<[()]>();

        let len_l = lhs.len();
        let len_r = rhs.len();

        let lhs = NonNull::new_unchecked(lhs as *mut ());
        let rhs = NonNull::new_unchecked(rhs as *mut ());

        let sizeof = t.layout.sized_layout().unwrap_unchecked().size();
        let f = t.vtable.cmp.partial_ord.unwrap_unchecked();
        for i in 0..Ord::min(len_l, len_r) {
            let lhs = lhs.byte_add(sizeof * i);
            let rhs = rhs.byte_add(sizeof * i);
            match f(PtrConst::new(lhs), PtrConst::new(rhs)) {
                Some(Ordering::Equal) => {}
                e => return e,
            }
        }
        if len_l < len_r {
            return Some(Ordering::Less);
        }
        if len_l > len_r {
            return Some(Ordering::Greater);
        }
        Some(Ordering::Equal)
    }
}

pub unsafe fn partial_ord_fields(
    lhs: PtrConst,
    rhs: PtrConst,
    fields: &[Field],
) -> Option<Ordering> {
    for field in fields {
        unsafe {
            let lhs = lhs.field(field.offset);
            let rhs = rhs.field(field.offset);
            let f = field.shape().vtable.cmp.partial_ord.unwrap();
            match f(lhs, rhs) {
                Some(Ordering::Equal) => {}
                e => return e,
            }
        }
    }
    Some(Ordering::Equal)
}

pub unsafe fn ord_slice(lhs: PtrConst, rhs: PtrConst, t: &Shape) -> Ordering {
    unsafe {
        let lhs = lhs.as_ptr::<[()]>();
        let rhs = rhs.as_ptr::<[()]>();

        let len_l = lhs.len();
        let len_r = rhs.len();

        let lhs = NonNull::new_unchecked(lhs as *mut ());
        let rhs = NonNull::new_unchecked(rhs as *mut ());

        let sizeof = t.layout.sized_layout().unwrap_unchecked().size();
        let f = t.vtable.cmp.ord.unwrap_unchecked();
        for i in 0..Ord::min(len_l, len_r) {
            let lhs = lhs.byte_add(sizeof * i);
            let rhs = rhs.byte_add(sizeof * i);
            match f(PtrConst::new(lhs), PtrConst::new(rhs)) {
                Ordering::Equal => {}
                e => return e,
            }
        }
        if len_l < len_r {
            return Ordering::Less;
        }
        if len_l > len_r {
            return Ordering::Greater;
        }
        Ordering::Equal
    }
}

pub unsafe fn ord_fields(lhs: PtrConst, rhs: PtrConst, fields: &[Field]) -> Ordering {
    for field in fields {
        unsafe {
            let lhs = lhs.field(field.offset);
            let rhs = rhs.field(field.offset);
            let f = field.shape().vtable.cmp.ord.unwrap();
            match f(lhs, rhs) {
                Ordering::Equal => {}
                e => return e,
            }
        }
    }
    Ordering::Equal
}

pub struct Debug<'a> {
    pub(crate) ptr: PtrConst<'a>,
    pub(crate) f: unsafe fn(PtrConst<'_>, &mut fmt::Formatter<'_>) -> Result<(), fmt::Error>,
}

impl fmt::Debug for Debug<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        unsafe { (self.f)(self.ptr, f) }
    }
}

pub struct Hash<'a> {
    pub(crate) ptr: PtrConst<'a>,
    pub(crate) f: unsafe fn(PtrConst<'_>, &mut dyn Hasher),
}

impl core::hash::Hash for Hash<'_> {
    fn hash<H: Hasher>(&self, state: &mut H) {
        unsafe { (self.f)(self.ptr, state) }
    }
}

pub unsafe fn debug_slice<'a, 'b>(
    slice: PtrConst,
    fmt: fmt::DebugList<'a, 'b>,
    t: &Shape,
) -> fmt::DebugList<'a, 'b> {
    let mut fmt = fmt;
    unsafe {
        let ptr = slice.as_ptr::<[()]>();
        let len = ptr.len();
        let ptr = NonNull::new_unchecked(ptr as *mut ());
        let sizeof = t.layout.sized_layout().unwrap_unchecked().size();
        let f = t.vtable.format.debug.unwrap_unchecked();

        for i in 0..len {
            let ptr = PtrConst::new(ptr.byte_add(sizeof * i));
            fmt.entry(&Debug { ptr, f });
        }
    }
    fmt
}

pub unsafe fn debug_struct<'a, 'b>(
    ptr: PtrConst,
    fmt: fmt::DebugStruct<'a, 'b>,
    fields: &[Field],
) -> fmt::DebugStruct<'a, 'b> {
    let mut fmt = fmt;
    for field in fields {
        unsafe {
            let ptr = ptr.field(field.offset);
            let f = field.shape().vtable.format.debug.unwrap();
            fmt.field(field.name, &Debug { ptr, f });
        }
    }
    fmt
}

pub const fn vtable_for_list<
    'facet,
    T: Facet<'facet>,
    List: ?Sized + Facet<'facet> + AsRef<[T]>,
>() -> ValueVTable {
    ValueVTable {
        type_name: |_, _| Ok(()),
        drop_in_place: ValueVTable::drop_in_place_for::<List>(),
        default_in_place: None,
        clone_into: None,
        parse: None,
        invariants: None,
        try_from: None,
        try_into_inner: None,
        try_borrow_inner: None,
        format: FormatVTable {
            display: None,
            debug: {
                if T::SHAPE.vtable.has_debug() {
                    Some(|value, f| unsafe {
                        debug_slice(
                            PtrConst::new::<[T]>(value.get::<List>().as_ref().into()),
                            f.debug_list(),
                            T::SHAPE,
                        )
                        .finish()
                    })
                } else {
                    None
                }
            },
        },
        cmp: CmpVTable {
            partial_eq: {
                if T::SHAPE.vtable.has_partial_eq() {
                    Some(|a, b| unsafe {
                        partial_eq_slice(
                            PtrConst::new::<[T]>(a.get::<List>().as_ref().into()),
                            PtrConst::new::<[T]>(b.get::<List>().as_ref().into()),
                            T::SHAPE,
                        )
                    })
                } else {
                    None
                }
            },
            partial_ord: {
                if T::SHAPE.vtable.has_partial_ord() {
                    Some(|a, b| unsafe {
                        partial_ord_slice(
                            PtrConst::new::<[T]>(a.get::<List>().as_ref().into()),
                            PtrConst::new::<[T]>(b.get::<List>().as_ref().into()),
                            T::SHAPE,
                        )
                    })
                } else {
                    None
                }
            },
            ord: {
                if T::SHAPE.vtable.has_ord() {
                    Some(|a, b| unsafe {
                        ord_slice(
                            PtrConst::new::<[T]>(a.get::<List>().as_ref().into()),
                            PtrConst::new::<[T]>(b.get::<List>().as_ref().into()),
                            T::SHAPE,
                        )
                    })
                } else {
                    None
                }
            },
        },
        hash: HashVTable {
            hash: {
                if T::SHAPE.vtable.has_hash() {
                    Some(|value, hasher| unsafe {
                        hash_slice(
                            PtrConst::new::<[T]>(value.get::<List>().as_ref().into()),
                            hasher,
                            T::SHAPE,
                        )
                    })
                } else {
                    None
                }
            },
        },
        markers: MarkerTraits::EMPTY,
    }
}

pub const fn vtable_for_ptr<
    'facet,
    T: ?Sized + Facet<'facet>,
    Ptr: ?Sized + Facet<'facet> + AsRef<T>,
>() -> ValueVTable {
    ValueVTable {
        type_name: |_, _| Ok(()),
        drop_in_place: ValueVTable::drop_in_place_for::<Ptr>(),
        default_in_place: None,
        clone_into: None,
        parse: None,
        invariants: None,
        try_from: None,
        try_into_inner: None,
        try_borrow_inner: None,
        format: FormatVTable {
            display: None,
            debug: {
                if T::SHAPE.vtable.has_debug() {
                    Some(|value, f| unsafe {
                        (T::SHAPE.vtable.format.debug.unwrap())(
                            PtrConst::new::<T>(value.get::<Ptr>().as_ref().into()),
                            f,
                        )
                    })
                } else {
                    None
                }
            },
        },
        cmp: CmpVTable {
            partial_eq: {
                if T::SHAPE.vtable.has_partial_eq() {
                    Some(|a, b| unsafe {
                        (T::SHAPE.vtable.cmp.partial_eq.unwrap())(
                            PtrConst::new::<T>(a.get::<Ptr>().as_ref().into()),
                            PtrConst::new::<T>(b.get::<Ptr>().as_ref().into()),
                        )
                    })
                } else {
                    None
                }
            },
            partial_ord: {
                if T::SHAPE.vtable.has_partial_ord() {
                    Some(|a, b| unsafe {
                        (T::SHAPE.vtable.cmp.partial_ord.unwrap())(
                            PtrConst::new::<T>(a.get::<Ptr>().as_ref().into()),
                            PtrConst::new::<T>(b.get::<Ptr>().as_ref().into()),
                        )
                    })
                } else {
                    None
                }
            },
            ord: {
                if T::SHAPE.vtable.has_ord() {
                    Some(|a, b| unsafe {
                        (T::SHAPE.vtable.cmp.ord.unwrap())(
                            PtrConst::new::<T>(a.get::<Ptr>().as_ref().into()),
                            PtrConst::new::<T>(b.get::<Ptr>().as_ref().into()),
                        )
                    })
                } else {
                    None
                }
            },
        },
        hash: HashVTable {
            hash: {
                if T::SHAPE.vtable.has_hash() {
                    Some(|value, hasher| unsafe {
                        (T::SHAPE.vtable.hash.hash.unwrap())(
                            PtrConst::new::<T>(value.get::<Ptr>().as_ref().into()),
                            hasher,
                        )
                    })
                } else {
                    None
                }
            },
        },
        markers: MarkerTraits::EMPTY,
    }
}
