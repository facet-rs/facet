use core::ptr::NonNull;

use alloc::boxed::Box;
use alloc::rc::{Rc, Weak};
use alloc::vec::Vec;

use crate::{
    Def, Facet, KnownPointer, OxPtrConst, OxPtrMut, OxPtrUninit, PointerDef, PointerFlags,
    PointerVTable, PtrConst, PtrMut, PtrUninit, Shape, ShapeBuilder, SliceBuilderVTable, Type,
    TypeNameOpts, TypeOpsIndirect, UserType, VTableIndirect, Variance, VarianceDep, VarianceDesc,
};

//////////////////////////////////////////////////////////////////////
// Rc<T>
//////////////////////////////////////////////////////////////////////

/// Type name formatter for `Rc<T>`
fn rc_type_name<'a, T: Facet<'a>>(
    _shape: &'static Shape,
    f: &mut core::fmt::Formatter<'_>,
    opts: TypeNameOpts,
) -> core::fmt::Result {
    write!(f, "Rc")?;
    if let Some(opts) = opts.for_children() {
        write!(f, "<")?;
        T::SHAPE.write_type_name(f, opts)?;
        write!(f, ">")?;
    } else {
        write!(f, "<…>")?;
    }
    Ok(())
}

/// Drop function for `Rc<T>`
unsafe fn rc_drop<T>(ox: OxPtrMut) {
    unsafe {
        core::ptr::drop_in_place(ox.ptr().as_ptr::<Rc<T>>() as *mut Rc<T>);
    }
}

/// Debug function for `Rc<T>`
unsafe fn rc_debug<'a, T: Facet<'a>>(
    ox: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    let rc = unsafe { ox.get::<Rc<T>>() };

    // Try to debug the inner value if T has debug
    let inner_ptr = PtrConst::new(NonNull::from(&**rc).as_ptr());
    if let Some(result) = unsafe { T::SHAPE.call_debug(inner_ptr, f) } {
        return Some(result);
    }

    // Fallback: just show it's an Rc
    Some(write!(f, "Rc(...)"))
}

/// Borrow function for PointerVTable
unsafe fn rc_borrow_fn<T>(this: PtrConst) -> PtrConst {
    let ptr = unsafe { Rc::<T>::as_ptr(this.get()) };
    PtrConst::new(ptr)
}

/// New into function for PointerVTable
unsafe fn rc_new_into_fn<'a, 'ptr, T: Facet<'a>>(this: PtrUninit, ptr: PtrMut) -> PtrMut {
    let t = unsafe { ptr.read::<T>() };
    let rc = Rc::new(t);
    unsafe { this.put(rc) }
}

/// Downgrade into function for PointerVTable
unsafe fn rc_downgrade_into_fn<'a, 'ptr, T: Facet<'a>>(strong: PtrMut, weak: PtrUninit) -> PtrMut {
    unsafe { weak.put(Rc::downgrade(strong.get::<Rc<T>>())) }
}

unsafe impl<'a, T: Facet<'a>> Facet<'a> for Rc<T> {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<Rc<T>>("Rc")
            .module_path("alloc::rc")
            .type_name(rc_type_name::<T>)
            .ty(Type::User(UserType::Opaque))
            .def(Def::Pointer(PointerDef {
                vtable: &const {
                    PointerVTable {
                        borrow_fn: Some(rc_borrow_fn::<T>),
                        new_into_fn: Some(rc_new_into_fn::<T>),
                        downgrade_into_fn: Some(|strong, weak| unsafe {
                            rc_downgrade_into_fn::<T>(strong, weak)
                        }),
                        ..PointerVTable::new()
                    }
                },
                pointee: Some(T::SHAPE),
                weak: Some(|| <Weak<T> as Facet>::SHAPE),
                strong: None,
                flags: PointerFlags::EMPTY,
                known: Some(KnownPointer::Rc),
            }))
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: T::SHAPE,
            }])
            .inner(T::SHAPE)
            // Rc<T> propagates T's variance
            .variance(VarianceDesc {
                base: Variance::Bivariant,
                deps: &const { [VarianceDep::covariant(T::SHAPE)] },
            })
            .vtable_indirect(
                &const {
                    VTableIndirect {
                        debug: Some(rc_debug::<T>),
                        display: None,
                        hash: None,
                        invariants: None,
                        parse: None,
                        parse_bytes: None,
                        try_from: None,
                        try_into_inner: None,
                        try_borrow_inner: None,
                        partial_eq: None,
                        partial_cmp: None,
                        cmp: None,
                    }
                },
            )
            .type_ops_indirect(
                &const {
                    TypeOpsIndirect {
                        drop_in_place: rc_drop::<T>,
                        default_in_place: None,
                        clone_into: None,
                        is_truthy: None,
                    }
                },
            )
            .build()
    };
}

//////////////////////////////////////////////////////////////////////
// Rc<str>
//////////////////////////////////////////////////////////////////////

/// Type name formatter for `Rc<str>`
fn rc_str_type_name(
    _shape: &'static Shape,
    f: &mut core::fmt::Formatter<'_>,
    opts: TypeNameOpts,
) -> core::fmt::Result {
    write!(f, "Rc")?;
    if let Some(opts) = opts.for_children() {
        write!(f, "<")?;
        str::SHAPE.write_type_name(f, opts)?;
        write!(f, ">")?;
    } else {
        write!(f, "<…>")?;
    }
    Ok(())
}

/// Drop function for `Rc<str>`
unsafe fn rc_str_drop(ox: OxPtrMut) {
    unsafe {
        core::ptr::drop_in_place(ox.ptr().as_ptr::<Rc<str>>() as *mut Rc<str>);
    }
}

/// Clone function for `Rc<str>`
unsafe fn rc_str_clone(src: OxPtrConst, dst: OxPtrMut) {
    unsafe {
        let value = src.get::<Rc<str>>().clone();
        (dst.ptr().as_ptr::<Rc<str>>() as *mut Rc<str>).write(value);
    }
}

/// Debug function for `Rc<str>`
unsafe fn rc_str_debug(
    ox: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    let rc = unsafe { ox.get::<Rc<str>>() };
    let s: &str = rc;
    Some(write!(f, "Rc({s:?})"))
}

/// Borrow function for `Rc<str>`
unsafe fn rc_str_borrow_fn(this: PtrConst) -> PtrConst {
    unsafe {
        let concrete = this.get::<Rc<str>>();
        let s: &str = concrete;
        PtrConst::new(NonNull::from(s).as_ptr())
    }
}

/// Downgrade into function for `Rc<str>`
unsafe fn rc_str_downgrade_into_fn(strong: PtrMut, weak: PtrUninit) -> PtrMut {
    unsafe { weak.put(Rc::downgrade(strong.get::<Rc<str>>())) }
}

unsafe impl<'a> Facet<'a> for Rc<str> {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableIndirect = VTableIndirect {
            debug: Some(rc_str_debug),
            display: None,
            hash: None,
            invariants: None,
            parse: None,
            parse_bytes: None,
            try_from: None,
            try_into_inner: None,
            try_borrow_inner: None,
            partial_eq: None,
            partial_cmp: None,
            cmp: None,
        };

        const TYPE_OPS: TypeOpsIndirect = TypeOpsIndirect {
            drop_in_place: rc_str_drop,
            default_in_place: None,
            clone_into: Some(rc_str_clone),
            is_truthy: None,
        };

        ShapeBuilder::for_sized::<Rc<str>>("Rc")
            .module_path("alloc::rc")
            .type_name(rc_str_type_name)
            .ty(Type::User(UserType::Opaque))
            .def(Def::Pointer(PointerDef {
                vtable: &const {
                    PointerVTable {
                        borrow_fn: Some(rc_str_borrow_fn),
                        downgrade_into_fn: Some(|strong, weak| unsafe {
                            rc_str_downgrade_into_fn(strong, weak)
                        }),
                        ..PointerVTable::new()
                    }
                },
                pointee: Some(str::SHAPE),
                weak: Some(|| <Weak<str> as Facet>::SHAPE),
                strong: None,
                flags: PointerFlags::EMPTY,
                known: Some(KnownPointer::Rc),
            }))
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: str::SHAPE,
            }])
            .vtable_indirect(&VTABLE)
            .type_ops_indirect(&TYPE_OPS)
            .build()
    };
}

//////////////////////////////////////////////////////////////////////
// Rc<[U]>
//////////////////////////////////////////////////////////////////////

fn slice_builder_new<'a, U: Facet<'a>>() -> PtrMut {
    let v = Box::new(Vec::<U>::new());
    let raw = Box::into_raw(v);
    PtrMut::new(unsafe { NonNull::new_unchecked(raw as *mut u8) }.as_ptr())
}

fn slice_builder_push<'a, U: Facet<'a>>(builder: PtrMut, item: PtrMut) {
    unsafe {
        let vec = builder.as_mut::<Vec<U>>();
        let value = item.read::<U>();
        vec.push(value);
    }
}

fn slice_builder_convert<'a, U: Facet<'a>>(builder: PtrMut) -> PtrConst {
    unsafe {
        let vec_box = Box::from_raw(builder.as_ptr::<Vec<U>>() as *mut Vec<U>);
        let rc: Rc<[U]> = (*vec_box).into();
        let rc_box = Box::new(rc);
        PtrConst::new(NonNull::new_unchecked(Box::into_raw(rc_box) as *mut u8).as_ptr())
    }
}

fn slice_builder_free<'a, U: Facet<'a>>(builder: PtrMut) {
    unsafe {
        let _ = Box::from_raw(builder.as_ptr::<Vec<U>>() as *mut Vec<U>);
    }
}

/// Type name formatter for `Rc<[U]>`
fn rc_slice_type_name<'a, U: Facet<'a>>(
    _shape: &'static Shape,
    f: &mut core::fmt::Formatter<'_>,
    opts: TypeNameOpts,
) -> core::fmt::Result {
    write!(f, "Rc")?;
    if let Some(opts) = opts.for_children() {
        write!(f, "<")?;
        <[U]>::SHAPE.write_type_name(f, opts)?;
        write!(f, ">")?;
    } else {
        write!(f, "<…>")?;
    }
    Ok(())
}

/// Drop function for `Rc<[U]>`
unsafe fn rc_slice_drop<U>(ox: OxPtrMut) {
    unsafe {
        core::ptr::drop_in_place(ox.ptr().as_ptr::<Rc<[U]>>() as *mut Rc<[U]>);
    }
}

/// Debug function for `Rc<[U]>`
unsafe fn rc_slice_debug<'a, U: Facet<'a>>(
    ox: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    let rc = unsafe { ox.get::<Rc<[U]>>() };
    let slice: &[U] = rc;

    // Try to debug the slice if U has debug
    let slice_ptr = PtrConst::new(NonNull::from(slice).as_ptr());
    if let Some(result) = unsafe { <[U]>::SHAPE.call_debug(slice_ptr, f) } {
        return Some(result);
    }

    Some(write!(f, "Rc([...])"))
}

/// Borrow function for `Rc<[U]>`
unsafe fn rc_slice_borrow_fn<U>(this: PtrConst) -> PtrConst {
    unsafe {
        let concrete = this.get::<Rc<[U]>>();
        let s: &[U] = concrete;
        PtrConst::new(NonNull::from(s).as_ptr())
    }
}

/// Downgrade into function for `Rc<[U]>`
unsafe fn rc_slice_downgrade_into_fn<'a, 'ptr, U: Facet<'a>>(
    strong: PtrMut,
    weak: PtrUninit,
) -> PtrMut {
    unsafe { weak.put(Rc::downgrade(strong.get::<Rc<[U]>>())) }
}

unsafe impl<'a, U: Facet<'a>> Facet<'a> for Rc<[U]> {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<Rc<[U]>>("Rc")
            .module_path("alloc::rc")
            .type_name(rc_slice_type_name::<U>)
            .ty(Type::User(UserType::Opaque))
            .def(Def::Pointer(PointerDef {
                vtable: &const {
                    PointerVTable {
                        borrow_fn: Some(rc_slice_borrow_fn::<U>),
                        downgrade_into_fn: Some(|strong, weak| unsafe {
                            rc_slice_downgrade_into_fn::<U>(strong, weak)
                        }),
                        slice_builder_vtable: Some(
                            &const {
                                SliceBuilderVTable::new(
                                    slice_builder_new::<U>,
                                    slice_builder_push::<U>,
                                    slice_builder_convert::<U>,
                                    slice_builder_free::<U>,
                                )
                            },
                        ),
                        ..PointerVTable::new()
                    }
                },
                pointee: Some(<[U]>::SHAPE),
                weak: Some(|| <Weak<[U]> as Facet>::SHAPE),
                strong: None,
                flags: PointerFlags::EMPTY,
                known: Some(KnownPointer::Rc),
            }))
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: <[U]>::SHAPE,
            }])
            .vtable_indirect(
                &const {
                    VTableIndirect {
                        debug: Some(rc_slice_debug::<U>),
                        display: None,
                        hash: None,
                        invariants: None,
                        parse: None,
                        parse_bytes: None,
                        try_from: None,
                        try_into_inner: None,
                        try_borrow_inner: None,
                        partial_eq: None,
                        partial_cmp: None,
                        cmp: None,
                    }
                },
            )
            .type_ops_indirect(
                &const {
                    TypeOpsIndirect {
                        drop_in_place: rc_slice_drop::<U>,
                        default_in_place: None,
                        clone_into: None,
                        is_truthy: None,
                    }
                },
            )
            .build()
    };
}

//////////////////////////////////////////////////////////////////////
// Weak<T>
//////////////////////////////////////////////////////////////////////

/// Type name formatter for `Weak<T>`
fn weak_type_name<'a, T: Facet<'a>>(
    _shape: &'static Shape,
    f: &mut core::fmt::Formatter<'_>,
    opts: TypeNameOpts,
) -> core::fmt::Result {
    write!(f, "Weak")?;
    if let Some(opts) = opts.for_children() {
        write!(f, "<")?;
        T::SHAPE.write_type_name(f, opts)?;
        write!(f, ">")?;
    } else {
        write!(f, "<…>")?;
    }
    Ok(())
}

/// Drop function for `Weak<T>`
unsafe fn weak_drop<T>(ox: OxPtrMut) {
    unsafe {
        core::ptr::drop_in_place(ox.ptr().as_ptr::<Weak<T>>() as *mut Weak<T>);
    }
}

/// Default function for `Weak<T>`
unsafe fn weak_default<T>(ox: OxPtrUninit) {
    unsafe { ox.put(Weak::<T>::new()) };
}

/// Clone function for `Weak<T>`
unsafe fn weak_clone<T>(src: OxPtrConst, dst: OxPtrMut) {
    unsafe {
        let value = src.get::<Weak<T>>().clone();
        (dst.ptr().as_ptr::<Weak<T>>() as *mut Weak<T>).write(value);
    }
}

/// Debug function for `Weak<T>`
unsafe fn weak_debug(
    _ox: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    Some(write!(f, "(Weak)"))
}

/// Upgrade into function for `Weak<T>`
unsafe fn weak_upgrade_into_fn<'a, 'ptr, T: Facet<'a>>(
    weak: PtrMut,
    strong: PtrUninit,
) -> Option<PtrMut> {
    unsafe { Some(strong.put(weak.get::<Weak<T>>().upgrade()?)) }
}

unsafe impl<'a, T: Facet<'a>> Facet<'a> for Weak<T> {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<Weak<T>>("Weak")
            .module_path("alloc::rc")
            .type_name(weak_type_name::<T>)
            .ty(Type::User(UserType::Opaque))
            .def(Def::Pointer(PointerDef {
                vtable: &const {
                    PointerVTable {
                        upgrade_into_fn: Some(|weak, strong| unsafe {
                            weak_upgrade_into_fn::<T>(weak, strong)
                        }),
                        ..PointerVTable::new()
                    }
                },
                pointee: Some(T::SHAPE),
                weak: None,
                strong: Some(<Rc<T> as Facet>::SHAPE),
                flags: PointerFlags::WEAK,
                known: Some(KnownPointer::RcWeak),
            }))
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: T::SHAPE,
            }])
            .inner(T::SHAPE)
            // Weak<T> propagates T's variance
            .variance(VarianceDesc {
                base: Variance::Bivariant,
                deps: &const { [VarianceDep::covariant(T::SHAPE)] },
            })
            .vtable_indirect(
                &const {
                    VTableIndirect {
                        debug: Some(weak_debug),
                        display: None,
                        hash: None,
                        invariants: None,
                        parse: None,
                        parse_bytes: None,
                        try_from: None,
                        try_into_inner: None,
                        try_borrow_inner: None,
                        partial_eq: None,
                        partial_cmp: None,
                        cmp: None,
                    }
                },
            )
            .type_ops_indirect(
                &const {
                    TypeOpsIndirect {
                        drop_in_place: weak_drop::<T>,
                        default_in_place: Some(weak_default::<T>),
                        clone_into: Some(weak_clone::<T>),
                        is_truthy: None,
                    }
                },
            )
            .build()
    };
}

//////////////////////////////////////////////////////////////////////
// Weak<str>
//////////////////////////////////////////////////////////////////////

/// Type name formatter for `Weak<str>`
fn weak_str_type_name(
    _shape: &'static Shape,
    f: &mut core::fmt::Formatter<'_>,
    opts: TypeNameOpts,
) -> core::fmt::Result {
    write!(f, "Weak")?;
    if let Some(opts) = opts.for_children() {
        write!(f, "<")?;
        str::SHAPE.write_type_name(f, opts)?;
        write!(f, ">")?;
    } else {
        write!(f, "<…>")?;
    }
    Ok(())
}

/// Drop function for `Weak<str>`
unsafe fn weak_str_drop(ox: OxPtrMut) {
    unsafe {
        core::ptr::drop_in_place(ox.ptr().as_ptr::<Weak<str>>() as *mut Weak<str>);
    }
}

/// Clone function for `Weak<str>`
unsafe fn weak_str_clone(src: OxPtrConst, dst: OxPtrMut) {
    unsafe {
        let value = src.get::<Weak<str>>().clone();
        (dst.ptr().as_ptr::<Weak<str>>() as *mut Weak<str>).write(value);
    }
}

/// Upgrade into function for `Weak<str>`
unsafe fn weak_str_upgrade_into_fn(weak: PtrMut, strong: PtrUninit) -> Option<PtrMut> {
    unsafe { Some(strong.put(weak.get::<Weak<str>>().upgrade()?)) }
}

unsafe impl<'a> Facet<'a> for Weak<str> {
    const SHAPE: &'static Shape = &const {
        const VTABLE: VTableIndirect = VTableIndirect {
            debug: Some(weak_debug),
            display: None,
            hash: None,
            invariants: None,
            parse: None,
            parse_bytes: None,
            try_from: None,
            try_into_inner: None,
            try_borrow_inner: None,
            partial_eq: None,
            partial_cmp: None,
            cmp: None,
        };

        const TYPE_OPS: TypeOpsIndirect = TypeOpsIndirect {
            drop_in_place: weak_str_drop,
            default_in_place: None,
            clone_into: Some(weak_str_clone),
            is_truthy: None,
        };

        ShapeBuilder::for_sized::<Weak<str>>("Weak")
            .module_path("alloc::rc")
            .type_name(weak_str_type_name)
            .ty(Type::User(UserType::Opaque))
            .def(Def::Pointer(PointerDef {
                vtable: &const {
                    PointerVTable {
                        upgrade_into_fn: Some(|weak, strong| unsafe {
                            weak_str_upgrade_into_fn(weak, strong)
                        }),
                        ..PointerVTable::new()
                    }
                },
                pointee: Some(str::SHAPE),
                weak: None,
                strong: Some(<Rc<str> as Facet>::SHAPE),
                flags: PointerFlags::WEAK,
                known: Some(KnownPointer::RcWeak),
            }))
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: str::SHAPE,
            }])
            .vtable_indirect(&VTABLE)
            .type_ops_indirect(&TYPE_OPS)
            .build()
    };
}

//////////////////////////////////////////////////////////////////////
// Weak<[U]>
//////////////////////////////////////////////////////////////////////

/// Type name formatter for `Weak<[U]>`
fn weak_slice_type_name<'a, U: Facet<'a>>(
    _shape: &'static Shape,
    f: &mut core::fmt::Formatter<'_>,
    opts: TypeNameOpts,
) -> core::fmt::Result {
    write!(f, "Weak")?;
    if let Some(opts) = opts.for_children() {
        write!(f, "<")?;
        <[U]>::SHAPE.write_type_name(f, opts)?;
        write!(f, ">")?;
    } else {
        write!(f, "<…>")?;
    }
    Ok(())
}

/// Drop function for `Weak<[U]>`
unsafe fn weak_slice_drop<U>(ox: OxPtrMut) {
    unsafe {
        core::ptr::drop_in_place(ox.ptr().as_ptr::<Weak<[U]>>() as *mut Weak<[U]>);
    }
}

/// Clone function for `Weak<[U]>`
unsafe fn weak_slice_clone<U>(src: OxPtrConst, dst: OxPtrMut) {
    unsafe {
        let value = src.get::<Weak<[U]>>().clone();
        (dst.ptr().as_ptr::<Weak<[U]>>() as *mut Weak<[U]>).write(value);
    }
}

/// Upgrade into function for `Weak<[U]>`
unsafe fn weak_slice_upgrade_into_fn<'a, 'ptr, U: Facet<'a>>(
    weak: PtrMut,
    strong: PtrUninit,
) -> Option<PtrMut> {
    unsafe { Some(strong.put(weak.get::<Weak<[U]>>().upgrade()?)) }
}

unsafe impl<'a, U: Facet<'a>> Facet<'a> for Weak<[U]> {
    const SHAPE: &'static Shape = &const {
        ShapeBuilder::for_sized::<Weak<[U]>>("Weak")
            .module_path("alloc::rc")
            .type_name(weak_slice_type_name::<U>)
            .ty(Type::User(UserType::Opaque))
            .def(Def::Pointer(PointerDef {
                vtable: &const {
                    PointerVTable {
                        upgrade_into_fn: Some(|weak, strong| unsafe {
                            weak_slice_upgrade_into_fn::<U>(weak, strong)
                        }),
                        ..PointerVTable::new()
                    }
                },
                pointee: Some(<[U]>::SHAPE),
                weak: None,
                strong: Some(<Rc<[U]> as Facet>::SHAPE),
                flags: PointerFlags::WEAK,
                known: Some(KnownPointer::RcWeak),
            }))
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: <[U]>::SHAPE,
            }])
            .vtable_indirect(
                &const {
                    VTableIndirect {
                        debug: Some(weak_debug),
                        display: None,
                        hash: None,
                        invariants: None,
                        parse: None,
                        parse_bytes: None,
                        try_from: None,
                        try_into_inner: None,
                        try_borrow_inner: None,
                        partial_eq: None,
                        partial_cmp: None,
                        cmp: None,
                    }
                },
            )
            .type_ops_indirect(
                &const {
                    TypeOpsIndirect {
                        drop_in_place: weak_slice_drop::<U>,
                        default_in_place: None,
                        clone_into: Some(weak_slice_clone::<U>),
                        is_truthy: None,
                    }
                },
            )
            .build()
    };
}

#[cfg(test)]
mod tests {
    use core::mem::ManuallyDrop;

    use alloc::rc::{Rc, Weak as RcWeak};
    use alloc::string::String;

    use super::*;

    #[test]
    fn test_rc_type_params() {
        let [type_param_1] = <Rc<i32>>::SHAPE.type_params else {
            panic!("Rc<T> should only have 1 type param")
        };
        assert_eq!(type_param_1.shape(), i32::SHAPE);
    }

    #[test]
    fn test_rc_vtable_1_new_borrow_drop() {
        facet_testhelpers::setup();

        let rc_shape = <Rc<String>>::SHAPE;
        let rc_def = rc_shape
            .def
            .into_pointer()
            .expect("Rc<T> should have a smart pointer definition");

        // Allocate memory for the Rc
        let rc_uninit_ptr = rc_shape.allocate().unwrap();

        // Get the function pointer for creating a new Rc from a value
        let new_into_fn = rc_def
            .vtable
            .new_into_fn
            .expect("Rc<T> should have new_into_fn");

        // Create the value and initialize the Rc
        let mut value = ManuallyDrop::new(String::from("example"));
        let rc_ptr = unsafe {
            new_into_fn(
                rc_uninit_ptr,
                PtrMut::new(NonNull::from(&mut value).as_ptr()),
            )
        };

        // Get the function pointer for borrowing the inner value
        let borrow_fn = rc_def
            .vtable
            .borrow_fn
            .expect("Rc<T> should have borrow_fn");

        // Borrow the inner value and check it
        let borrowed_ptr = unsafe { borrow_fn(rc_ptr.as_const()) };
        // SAFETY: borrowed_ptr points to a valid String within the Rc
        assert_eq!(unsafe { borrowed_ptr.get::<String>() }, "example");

        // Drop the Rc in place
        // SAFETY: rc_ptr points to a valid Rc<String>
        unsafe {
            rc_shape
                .call_drop_in_place(rc_ptr)
                .expect("Rc<T> should have drop_in_place");
        }

        // Deallocate the memory
        // SAFETY: rc_ptr was allocated by rc_shape and is now dropped (but memory is still valid)
        unsafe { rc_shape.deallocate_mut(rc_ptr).unwrap() };
    }

    #[test]
    fn test_rc_vtable_2_downgrade_upgrade_drop() {
        facet_testhelpers::setup();

        let rc_shape = <Rc<String>>::SHAPE;
        let rc_def = rc_shape
            .def
            .into_pointer()
            .expect("Rc<T> should have a smart pointer definition");

        let weak_shape = <RcWeak<String>>::SHAPE;
        let weak_def = weak_shape
            .def
            .into_pointer()
            .expect("RcWeak<T> should have a smart pointer definition");

        // 1. Create the first Rc (rc1)
        let rc1_uninit_ptr = rc_shape.allocate().unwrap();
        let new_into_fn = rc_def.vtable.new_into_fn.unwrap();
        let mut value = ManuallyDrop::new(String::from("example"));
        let rc1_ptr = unsafe {
            new_into_fn(
                rc1_uninit_ptr,
                PtrMut::new(NonNull::from(&mut value).as_ptr()),
            )
        };

        // 2. Downgrade rc1 to create a weak pointer (weak1)
        let weak1_uninit_ptr = weak_shape.allocate().unwrap();
        let downgrade_into_fn = rc_def.vtable.downgrade_into_fn.unwrap();
        // SAFETY: rc1_ptr points to a valid Rc, weak1_uninit_ptr is allocated for a Weak
        let weak1_ptr = unsafe { downgrade_into_fn(rc1_ptr, weak1_uninit_ptr) };

        // 3. Upgrade weak1 to create a second Rc (rc2)
        let rc2_uninit_ptr = rc_shape.allocate().unwrap();
        let upgrade_into_fn = weak_def.vtable.upgrade_into_fn.unwrap();
        // SAFETY: weak1_ptr points to a valid Weak, rc2_uninit_ptr is allocated for an Rc.
        // Upgrade should succeed as rc1 still exists.
        let rc2_ptr = unsafe { upgrade_into_fn(weak1_ptr, rc2_uninit_ptr) }
            .expect("Upgrade should succeed while original Rc exists");

        // Check the content of the upgraded Rc
        let borrow_fn = rc_def.vtable.borrow_fn.unwrap();
        // SAFETY: rc2_ptr points to a valid Rc<String>
        let borrowed_ptr = unsafe { borrow_fn(rc2_ptr.as_const()) };
        // SAFETY: borrowed_ptr points to a valid String
        assert_eq!(unsafe { borrowed_ptr.get::<String>() }, "example");

        // 4. Drop everything and free memory
        unsafe {
            // Drop Rcs
            rc_shape.call_drop_in_place(rc1_ptr).unwrap();
            rc_shape.deallocate_mut(rc1_ptr).unwrap();

            rc_shape.call_drop_in_place(rc2_ptr).unwrap();
            rc_shape.deallocate_mut(rc2_ptr).unwrap();

            // Drop Weak
            weak_shape.call_drop_in_place(weak1_ptr).unwrap();
            weak_shape.deallocate_mut(weak1_ptr).unwrap();
        }
    }

    #[test]
    fn test_rc_vtable_3_downgrade_drop_try_upgrade() {
        facet_testhelpers::setup();

        let rc_shape = <Rc<String>>::SHAPE;
        let rc_def = rc_shape
            .def
            .into_pointer()
            .expect("Rc<T> should have a smart pointer definition");

        let weak_shape = <RcWeak<String>>::SHAPE;
        let weak_def = weak_shape
            .def
            .into_pointer()
            .expect("RcWeak<T> should have a smart pointer definition");

        // 1. Create the strong Rc (rc1)
        let rc1_uninit_ptr = rc_shape.allocate().unwrap();
        let new_into_fn = rc_def.vtable.new_into_fn.unwrap();
        let mut value = ManuallyDrop::new(String::from("example"));
        let rc1_ptr = unsafe {
            new_into_fn(
                rc1_uninit_ptr,
                PtrMut::new(NonNull::from(&mut value).as_ptr()),
            )
        };

        // 2. Downgrade rc1 to create a weak pointer (weak1)
        let weak1_uninit_ptr = weak_shape.allocate().unwrap();
        let downgrade_into_fn = rc_def.vtable.downgrade_into_fn.unwrap();
        // SAFETY: rc1_ptr is valid, weak1_uninit_ptr is allocated for Weak
        let weak1_ptr = unsafe { downgrade_into_fn(rc1_ptr, weak1_uninit_ptr) };

        // 3. Drop and free the strong pointer (rc1)
        unsafe {
            rc_shape.call_drop_in_place(rc1_ptr).unwrap();
            rc_shape.deallocate_mut(rc1_ptr).unwrap();
        }

        // 4. Attempt to upgrade the weak pointer (weak1)
        let upgrade_into_fn = weak_def.vtable.upgrade_into_fn.unwrap();
        let rc2_uninit_ptr = rc_shape.allocate().unwrap();
        // SAFETY: weak1_ptr is valid (though points to dropped data), rc2_uninit_ptr is allocated for Rc
        let upgrade_result = unsafe { upgrade_into_fn(weak1_ptr, rc2_uninit_ptr) };

        // Assert that the upgrade failed
        assert!(
            upgrade_result.is_none(),
            "Upgrade should fail after the strong Rc is dropped"
        );

        // 5. Clean up: Deallocate the memory intended for the failed upgrade and drop/deallocate the weak pointer
        unsafe {
            // Deallocate the *uninitialized* memory allocated for the failed upgrade attempt
            rc_shape.deallocate_uninit(rc2_uninit_ptr).unwrap();

            // Drop and deallocate the weak pointer
            weak_shape.call_drop_in_place(weak1_ptr).unwrap();
            weak_shape.deallocate_mut(weak1_ptr).unwrap();
        }
    }
}
