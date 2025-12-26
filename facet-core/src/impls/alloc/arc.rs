use core::alloc::Layout;
use core::ptr::NonNull;

use alloc::boxed::Box;
use alloc::sync::{Arc, Weak};
use alloc::vec::Vec;

use crate::{
    Def, Facet, KnownPointer, OxPtrConst, OxPtrMut, PointerDef, PointerFlags, PointerVTable,
    PtrConst, PtrMut, PtrUninit, Shape, ShapeBuilder, SliceBuilderVTable, Type, TypeNameOpts,
    TypeOpsIndirect, UserType, VTableIndirect,
};

// Helper functions to create type_name formatters
fn type_name_arc<'a, T: Facet<'a>>(
    _shape: &'static crate::Shape,
    f: &mut core::fmt::Formatter<'_>,
    opts: TypeNameOpts,
) -> core::fmt::Result {
    write!(f, "Arc")?;
    if let Some(opts) = opts.for_children() {
        write!(f, "<")?;
        T::SHAPE.write_type_name(f, opts)?;
        write!(f, ">")?;
    } else {
        write!(f, "<…>")?;
    }
    Ok(())
}

fn type_name_weak<'a, T: Facet<'a>>(
    _shape: &'static crate::Shape,
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

// Pointer VTable functions for Arc<T>
unsafe fn arc_borrow<'a, T: Facet<'a>>(this: PtrConst) -> PtrConst {
    unsafe {
        let arc_ptr = this.as_ptr::<Arc<T>>();
        let ptr = &**arc_ptr;
        PtrConst::new(NonNull::from(ptr).as_ptr())
    }
}

unsafe fn arc_new_into<'a, 'src, T: Facet<'a>>(this: PtrUninit, ptr: PtrMut) -> PtrMut {
    unsafe {
        let t = ptr.read::<T>();
        let arc = Arc::new(t);
        this.put(arc)
    }
}

unsafe fn arc_downgrade_into<'a, 'src, T: Facet<'a>>(strong: PtrMut, weak: PtrUninit) -> PtrMut {
    unsafe { weak.put(Arc::downgrade(strong.as_const().get::<Arc<T>>())) }
}

// Arc<str> specific functions
unsafe fn arc_str_borrow(this: PtrConst) -> PtrConst {
    unsafe {
        let concrete = this.get::<Arc<str>>();
        let s: &str = concrete;
        PtrConst::new(NonNull::from(s).as_ptr())
    }
}

unsafe fn arc_str_downgrade_into(strong: PtrMut, weak: PtrUninit) -> PtrMut {
    unsafe { weak.put(Arc::downgrade(strong.as_const().get::<Arc<str>>())) }
}

// Arc<[U]> specific functions
unsafe fn arc_slice_borrow<'a, U: Facet<'a>>(this: PtrConst) -> PtrConst {
    unsafe {
        let concrete = this.get::<Arc<[U]>>();
        let s: &[U] = concrete;
        PtrConst::new(NonNull::from(s).as_ptr())
    }
}

unsafe fn arc_slice_downgrade_into<'a, 'src, U: Facet<'a>>(
    strong: PtrMut,
    weak: PtrUninit,
) -> PtrMut {
    unsafe { weak.put(Arc::downgrade(strong.as_const().get::<Arc<[U]>>())) }
}

// Slice builder functions
fn slice_builder_new<'a, U: Facet<'a>>() -> PtrMut {
    let v = Box::new(Vec::<U>::new());
    let raw = Box::into_raw(v);
    PtrMut::new(raw as *mut u8)
}

unsafe fn slice_builder_push<'a, U: Facet<'a>>(builder: PtrMut, item: PtrMut) {
    unsafe {
        let vec = builder.as_mut::<Vec<U>>();
        let value = item.read::<U>();
        vec.push(value);
    }
}

unsafe fn slice_builder_convert<'a, U: Facet<'a>>(builder: PtrMut) -> PtrConst {
    unsafe {
        let vec_box = Box::from_raw(builder.as_ptr::<Vec<U>>() as *mut Vec<U>);
        let arc: Arc<[U]> = (*vec_box).into();

        // Allocate memory for the Arc (which is a fat pointer, 16 bytes on 64-bit)
        let layout = Layout::new::<Arc<[U]>>();
        let ptr = alloc::alloc::alloc(layout) as *mut Arc<[U]>;
        if ptr.is_null() {
            alloc::alloc::handle_alloc_error(layout);
        }

        // Write the Arc into the allocation
        ptr.write(arc);

        PtrConst::new(ptr as *const u8)
    }
}

unsafe fn slice_builder_free<'a, U: Facet<'a>>(builder: PtrMut) {
    unsafe {
        let _ = Box::from_raw(builder.as_ptr::<Vec<U>>() as *mut Vec<U>);
    }
}

// Debug functions for Weak
unsafe fn weak_debug(
    _this: OxPtrConst,
    f: &mut core::fmt::Formatter<'_>,
) -> Option<core::fmt::Result> {
    Some(write!(f, "(Weak)"))
}

// Drop functions for Weak
unsafe fn weak_drop<T: ?Sized>(ox: OxPtrMut) {
    unsafe {
        core::ptr::drop_in_place(ox.ptr().as_ptr::<Weak<T>>() as *mut Weak<T>);
    }
}

// Clone functions for Weak
unsafe fn weak_clone<T: Clone>(src: OxPtrConst, dst: OxPtrMut) {
    unsafe {
        let value = src.get::<Weak<T>>().clone();
        (dst.ptr().as_ptr::<Weak<T>>() as *mut Weak<T>).write(value);
    }
}

// Default functions for Weak
unsafe fn weak_default<T>(target: OxPtrMut) {
    unsafe {
        (target.ptr().as_ptr::<Weak<T>>() as *mut Weak<T>).write(Weak::<T>::new());
    }
}

// Upgrade functions for Weak
unsafe fn weak_upgrade_into<'a, 'src, T: Facet<'a>>(
    weak: PtrMut,
    strong: PtrUninit,
) -> Option<PtrMut> {
    unsafe { Some(strong.put(weak.as_const().get::<Weak<T>>().upgrade()?)) }
}

// Type operations for Arc<T>
unsafe fn arc_drop<T>(ox: OxPtrMut) {
    unsafe { core::ptr::drop_in_place(ox.ptr().as_ptr::<Arc<T>>() as *mut Arc<T>) };
}

unsafe impl<'a, T: Facet<'a>> Facet<'a> for Arc<T> {
    const SHAPE: &'static crate::Shape = &const {
        ShapeBuilder::for_sized::<Self>("Arc")
            .type_name(type_name_arc::<T>)
            .vtable_indirect(&VTableIndirect::EMPTY)
            .type_ops_indirect(
                &const {
                    TypeOpsIndirect {
                        drop_in_place: arc_drop::<T>,
                        default_in_place: None,
                        clone_into: None,
                        is_truthy: None,
                    }
                },
            )
            .ty(Type::User(UserType::Opaque))
            .def(Def::Pointer(PointerDef {
                vtable: &const {
                    PointerVTable {
                        borrow_fn: Some(arc_borrow::<T>),
                        new_into_fn: Some(arc_new_into::<T>),
                        downgrade_into_fn: Some(arc_downgrade_into::<T>),
                        ..PointerVTable::new()
                    }
                },
                pointee: Some(T::SHAPE),
                weak: Some(|| <Weak<T> as Facet>::SHAPE),
                strong: None,
                flags: PointerFlags::ATOMIC,
                known: Some(KnownPointer::Arc),
            }))
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: T::SHAPE,
            }])
            .inner(T::SHAPE)
            // Arc<T> propagates T's variance
            .variance(Shape::computed_variance)
            .build()
    };
}

// Type operations for Arc<str>
unsafe fn arc_str_drop(ox: OxPtrMut) {
    unsafe { core::ptr::drop_in_place(ox.ptr().as_ptr::<Arc<str>>() as *mut Arc<str>) };
}

// Type operations for Weak<str>
unsafe fn weak_str_drop(ox: OxPtrMut) {
    unsafe {
        core::ptr::drop_in_place(ox.ptr().as_ptr::<Weak<str>>() as *mut Weak<str>);
    }
}

// Module-level static for Arc<str> type ops
static ARC_STR_TYPE_OPS: TypeOpsIndirect = TypeOpsIndirect {
    drop_in_place: arc_str_drop,
    default_in_place: None,
    clone_into: None,
    is_truthy: None,
};

unsafe impl<'a> Facet<'a> for Arc<str> {
    const SHAPE: &'static crate::Shape = &const {
        fn type_name_arc_str(
            _shape: &'static crate::Shape,
            f: &mut core::fmt::Formatter<'_>,
            opts: TypeNameOpts,
        ) -> core::fmt::Result {
            write!(f, "Arc")?;
            if let Some(opts) = opts.for_children() {
                write!(f, "<")?;
                str::SHAPE.write_type_name(f, opts)?;
                write!(f, ">")?;
            } else {
                write!(f, "<…>")?;
            }
            Ok(())
        }

        ShapeBuilder::for_sized::<Self>("Arc")
            .type_name(type_name_arc_str)
            .vtable_indirect(&const { VTableIndirect::EMPTY })
            .type_ops_indirect(&ARC_STR_TYPE_OPS)
            .ty(Type::User(UserType::Opaque))
            .def(Def::Pointer(PointerDef {
                vtable: &const {
                    PointerVTable {
                        borrow_fn: Some(arc_str_borrow),
                        downgrade_into_fn: Some(arc_str_downgrade_into),
                        ..PointerVTable::new()
                    }
                },
                pointee: Some(str::SHAPE),
                weak: Some(|| <Weak<str> as Facet>::SHAPE),
                strong: None,
                flags: PointerFlags::ATOMIC,
                known: Some(KnownPointer::Arc),
            }))
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: str::SHAPE,
            }])
            .build()
    };
}

// Type operations for Arc<[U]>
unsafe fn arc_slice_drop<U>(ox: OxPtrMut) {
    unsafe { core::ptr::drop_in_place(ox.ptr().as_ptr::<Arc<[U]>>() as *mut Arc<[U]>) };
}

// Type operations for Weak<[U]>
unsafe fn weak_slice_drop<U>(ox: OxPtrMut) {
    unsafe {
        core::ptr::drop_in_place(ox.ptr().as_ptr::<Weak<[U]>>() as *mut Weak<[U]>);
    }
}

unsafe impl<'a, U: Facet<'a>> Facet<'a> for Arc<[U]> {
    const SHAPE: &'static crate::Shape = &const {
        fn type_name_arc_slice<'a, U: Facet<'a>>(
            _shape: &'static crate::Shape,
            f: &mut core::fmt::Formatter<'_>,
            opts: TypeNameOpts,
        ) -> core::fmt::Result {
            write!(f, "Arc")?;
            if let Some(opts) = opts.for_children() {
                write!(f, "<")?;
                <[U]>::SHAPE.write_type_name(f, opts)?;
                write!(f, ">")?;
            } else {
                write!(f, "<…>")?;
            }
            Ok(())
        }

        ShapeBuilder::for_sized::<Self>("Arc")
            .type_name(type_name_arc_slice::<U>)
            .vtable_indirect(&VTableIndirect::EMPTY)
            .type_ops_indirect(
                &const {
                    TypeOpsIndirect {
                        drop_in_place: arc_slice_drop::<U>,
                        default_in_place: None,
                        clone_into: None,
                        is_truthy: None,
                    }
                },
            )
            .ty(Type::User(UserType::Opaque))
            .def(Def::Pointer(PointerDef {
                vtable: &const {
                    PointerVTable {
                        borrow_fn: Some(arc_slice_borrow::<U>),
                        downgrade_into_fn: Some(arc_slice_downgrade_into::<U>),
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
                flags: PointerFlags::ATOMIC,
                known: Some(KnownPointer::Arc),
            }))
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: <[U]>::SHAPE,
            }])
            .build()
    };
}

unsafe impl<'a, T: Facet<'a>> Facet<'a> for Weak<T> {
    const SHAPE: &'static crate::Shape = &const {
        const VTABLE: VTableIndirect = VTableIndirect {
            display: None,
            debug: Some(weak_debug),
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

        ShapeBuilder::for_sized::<Self>("Weak")
            .type_name(type_name_weak::<T>)
            .vtable_indirect(&VTABLE)
            .type_ops_indirect(
                &const {
                    TypeOpsIndirect {
                        drop_in_place: weak_drop::<T>,
                        default_in_place: Some(weak_default::<T>),
                        clone_into: Some(weak_clone::<Weak<T>>),
                        is_truthy: None,
                    }
                },
            )
            .ty(Type::User(UserType::Opaque))
            .def(Def::Pointer(PointerDef {
                vtable: &const {
                    PointerVTable {
                        upgrade_into_fn: Some(weak_upgrade_into::<T>),
                        ..PointerVTable::new()
                    }
                },
                pointee: Some(T::SHAPE),
                weak: None,
                strong: Some(<Arc<T> as Facet>::SHAPE),
                flags: PointerFlags::ATOMIC.union(PointerFlags::WEAK),
                known: Some(KnownPointer::ArcWeak),
            }))
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: T::SHAPE,
            }])
            .inner(T::SHAPE)
            // Weak<T> propagates T's variance
            .variance(Shape::computed_variance)
            .build()
    };
}

// Module-level statics for Weak<str>
static WEAK_STR_TYPE_OPS: TypeOpsIndirect = TypeOpsIndirect {
    drop_in_place: weak_str_drop,
    default_in_place: None,
    clone_into: Some(weak_clone::<Weak<str>>),
    is_truthy: None,
};

static WEAK_VTABLE: VTableIndirect = VTableIndirect {
    display: None,
    debug: Some(weak_debug),
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

unsafe impl<'a> Facet<'a> for Weak<str> {
    const SHAPE: &'static crate::Shape = &const {
        fn type_name_weak_str(
            _shape: &'static crate::Shape,
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

        ShapeBuilder::for_sized::<Self>("Weak")
            .type_name(type_name_weak_str)
            .vtable_indirect(&WEAK_VTABLE)
            .type_ops_indirect(&WEAK_STR_TYPE_OPS)
            .ty(Type::User(UserType::Opaque))
            .def(Def::Pointer(PointerDef {
                vtable: &const {
                    PointerVTable {
                        upgrade_into_fn: Some(|weak, strong| unsafe {
                            let upgraded = weak.as_const().get::<Weak<str>>().upgrade()?;
                            Some(strong.put(upgraded))
                        }),
                        ..PointerVTable::new()
                    }
                },
                pointee: Some(str::SHAPE),
                weak: None,
                strong: Some(<Arc<str> as Facet>::SHAPE),
                flags: PointerFlags::ATOMIC.union(PointerFlags::WEAK),
                known: Some(KnownPointer::ArcWeak),
            }))
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: str::SHAPE,
            }])
            .build()
    };
}

unsafe impl<'a, U: Facet<'a>> Facet<'a> for Weak<[U]> {
    const SHAPE: &'static crate::Shape = &const {
        fn type_name_weak_slice<'a, U: Facet<'a>>(
            _shape: &'static crate::Shape,
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

        const VTABLE: VTableIndirect = VTableIndirect {
            display: None,
            debug: Some(weak_debug),
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

        ShapeBuilder::for_sized::<Self>("Weak")
            .type_name(type_name_weak_slice::<U>)
            .vtable_indirect(&VTABLE)
            .type_ops_indirect(
                &const {
                    TypeOpsIndirect {
                        drop_in_place: weak_slice_drop::<U>,
                        default_in_place: None,
                        clone_into: Some(weak_clone::<Weak<[U]>>),
                        is_truthy: None,
                    }
                },
            )
            .ty(Type::User(UserType::Opaque))
            .def(Def::Pointer(PointerDef {
                vtable: &const {
                    PointerVTable {
                        upgrade_into_fn: Some(|weak, strong| unsafe {
                            let upgraded = weak.as_const().get::<Weak<[U]>>().upgrade()?;
                            Some(strong.put(upgraded))
                        }),
                        ..PointerVTable::new()
                    }
                },
                pointee: Some(<[U]>::SHAPE),
                weak: None,
                strong: Some(<Arc<[U]> as Facet>::SHAPE),
                flags: PointerFlags::ATOMIC.union(PointerFlags::WEAK),
                known: Some(KnownPointer::ArcWeak),
            }))
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: <[U]>::SHAPE,
            }])
            .build()
    };
}

#[cfg(test)]
mod tests {
    use core::mem::ManuallyDrop;

    use alloc::string::String;
    use alloc::sync::{Arc, Weak as ArcWeak};

    use super::*;

    #[test]
    fn test_arc_type_params() {
        let [type_param_1] = <Arc<i32>>::SHAPE.type_params else {
            panic!("Arc<T> should only have 1 type param")
        };
        assert_eq!(type_param_1.shape(), i32::SHAPE);
    }

    #[test]
    fn test_arc_vtable_1_new_borrow_drop() {
        facet_testhelpers::setup();

        let arc_shape = <Arc<String>>::SHAPE;
        let arc_def = arc_shape
            .def
            .into_pointer()
            .expect("Arc<T> should have a smart pointer definition");

        // Allocate memory for the Arc
        let arc_uninit_ptr = arc_shape.allocate().unwrap();

        // Get the function pointer for creating a new Arc from a value
        let new_into_fn = arc_def
            .vtable
            .new_into_fn
            .expect("Arc<T> should have new_into_fn");

        // Create the value and initialize the Arc
        let mut value = ManuallyDrop::new(String::from("example"));
        let arc_ptr = unsafe {
            new_into_fn(
                arc_uninit_ptr,
                PtrMut::new(NonNull::from(&mut value).as_ptr()),
            )
        };
        // The value now belongs to the Arc, prevent its drop

        // Get the function pointer for borrowing the inner value
        let borrow_fn = arc_def
            .vtable
            .borrow_fn
            .expect("Arc<T> should have borrow_fn");

        // Borrow the inner value and check it
        let borrowed_ptr = unsafe { borrow_fn(arc_ptr.as_const()) };
        // SAFETY: borrowed_ptr points to a valid String within the Arc
        assert_eq!(unsafe { borrowed_ptr.get::<String>() }, "example");

        // Drop the Arc in place
        // SAFETY: arc_ptr points to a valid Arc<String>
        unsafe {
            arc_shape
                .call_drop_in_place(arc_ptr)
                .expect("Arc<T> should have drop_in_place");
        }

        // Deallocate the memory
        // SAFETY: arc_ptr was allocated by arc_shape and is now dropped (but memory is still valid)
        unsafe { arc_shape.deallocate_mut(arc_ptr).unwrap() };
    }

    #[test]
    fn test_arc_vtable_2_downgrade_upgrade_drop() {
        facet_testhelpers::setup();

        let arc_shape = <Arc<String>>::SHAPE;
        let arc_def = arc_shape
            .def
            .into_pointer()
            .expect("Arc<T> should have a smart pointer definition");

        let weak_shape = <ArcWeak<String>>::SHAPE;
        let weak_def = weak_shape
            .def
            .into_pointer()
            .expect("ArcWeak<T> should have a smart pointer definition");

        // 1. Create the first Arc (arc1)
        let arc1_uninit_ptr = arc_shape.allocate().unwrap();
        let new_into_fn = arc_def.vtable.new_into_fn.unwrap();
        let mut value = ManuallyDrop::new(String::from("example"));
        let arc1_ptr = unsafe {
            new_into_fn(
                arc1_uninit_ptr,
                PtrMut::new(NonNull::from(&mut value).as_ptr()),
            )
        };

        // 2. Downgrade arc1 to create a weak pointer (weak1)
        let weak1_uninit_ptr = weak_shape.allocate().unwrap();
        let downgrade_into_fn = arc_def.vtable.downgrade_into_fn.unwrap();
        // SAFETY: arc1_ptr points to a valid Arc, weak1_uninit_ptr is allocated for a Weak
        let weak1_ptr = unsafe { downgrade_into_fn(arc1_ptr, weak1_uninit_ptr) };

        // 3. Upgrade weak1 to create a second Arc (arc2)
        let arc2_uninit_ptr = arc_shape.allocate().unwrap();
        let upgrade_into_fn = weak_def.vtable.upgrade_into_fn.unwrap();
        // SAFETY: weak1_ptr points to a valid Weak, arc2_uninit_ptr is allocated for an Arc.
        // Upgrade should succeed as arc1 still exists.
        let arc2_ptr = unsafe { upgrade_into_fn(weak1_ptr, arc2_uninit_ptr) }
            .expect("Upgrade should succeed while original Arc exists");

        // Check the content of the upgraded Arc
        let borrow_fn = arc_def.vtable.borrow_fn.unwrap();
        // SAFETY: arc2_ptr points to a valid Arc<String>
        let borrowed_ptr = unsafe { borrow_fn(arc2_ptr.as_const()) };
        // SAFETY: borrowed_ptr points to a valid String
        assert_eq!(unsafe { borrowed_ptr.get::<String>() }, "example");

        // 4. Drop everything and free memory
        unsafe {
            // Drop Arcs
            arc_shape.call_drop_in_place(arc1_ptr).unwrap();
            arc_shape.deallocate_mut(arc1_ptr).unwrap();
            arc_shape.call_drop_in_place(arc2_ptr).unwrap();
            arc_shape.deallocate_mut(arc2_ptr).unwrap();

            // Drop Weak
            weak_shape.call_drop_in_place(weak1_ptr).unwrap();
            weak_shape.deallocate_mut(weak1_ptr).unwrap();
        }
    }

    #[test]
    fn test_arc_vtable_3_downgrade_drop_try_upgrade() {
        facet_testhelpers::setup();

        let arc_shape = <Arc<String>>::SHAPE;
        let arc_def = arc_shape
            .def
            .into_pointer()
            .expect("Arc<T> should have a smart pointer definition");

        let weak_shape = <ArcWeak<String>>::SHAPE;
        let weak_def = weak_shape
            .def
            .into_pointer()
            .expect("ArcWeak<T> should have a smart pointer definition");

        // 1. Create the strong Arc (arc1)
        let arc1_uninit_ptr = arc_shape.allocate().unwrap();
        let new_into_fn = arc_def.vtable.new_into_fn.unwrap();
        let mut value = ManuallyDrop::new(String::from("example"));
        let arc1_ptr = unsafe {
            new_into_fn(
                arc1_uninit_ptr,
                PtrMut::new(NonNull::from(&mut value).as_ptr()),
            )
        };

        // 2. Downgrade arc1 to create a weak pointer (weak1)
        let weak1_uninit_ptr = weak_shape.allocate().unwrap();
        let downgrade_into_fn = arc_def.vtable.downgrade_into_fn.unwrap();
        // SAFETY: arc1_ptr is valid, weak1_uninit_ptr is allocated for Weak
        let weak1_ptr = unsafe { downgrade_into_fn(arc1_ptr, weak1_uninit_ptr) };

        // 3. Drop and free the strong pointer (arc1)
        unsafe {
            arc_shape.call_drop_in_place(arc1_ptr).unwrap();
            arc_shape.deallocate_mut(arc1_ptr).unwrap();
        }

        // 4. Attempt to upgrade the weak pointer (weak1)
        let upgrade_into_fn = weak_def.vtable.upgrade_into_fn.unwrap();
        let arc2_uninit_ptr = arc_shape.allocate().unwrap();
        // SAFETY: weak1_ptr is valid (though points to dropped data), arc2_uninit_ptr is allocated for Arc
        let upgrade_result = unsafe { upgrade_into_fn(weak1_ptr, arc2_uninit_ptr) };

        // Assert that the upgrade failed
        assert!(
            upgrade_result.is_none(),
            "Upgrade should fail after the strong Arc is dropped"
        );

        // 5. Clean up: Deallocate the memory intended for the failed upgrade and drop/deallocate the weak pointer
        unsafe {
            // Deallocate the *uninitialized* memory allocated for the failed upgrade attempt
            arc_shape.deallocate_uninit(arc2_uninit_ptr).unwrap();

            // Drop and deallocate the weak pointer
            weak_shape.call_drop_in_place(weak1_ptr).unwrap();
            weak_shape.deallocate_mut(weak1_ptr).unwrap();
        }
    }

    #[test]
    fn test_arc_vtable_6_slice_builder() {
        facet_testhelpers::setup();

        // Get the shapes we'll be working with
        let arc_slice_shape = <Arc<[i32]>>::SHAPE;
        let arc_slice_def = arc_slice_shape
            .def
            .into_pointer()
            .expect("Arc<[i32]> should have a smart pointer definition");

        // Get the slice builder vtable
        let slice_builder_vtable = arc_slice_def
            .vtable
            .slice_builder_vtable
            .expect("Arc<[i32]> should have slice_builder_vtable");

        // 1. Create a new builder
        let builder_ptr = (slice_builder_vtable.new_fn)();

        // 2. Push some items to the builder
        let push_fn = slice_builder_vtable.push_fn;
        let values = [1i32, 2, 3, 4, 5];
        for &value in &values {
            let mut value_copy = value;
            let value_ptr = PtrMut::new(NonNull::from(&mut value_copy).as_ptr());
            unsafe { push_fn(builder_ptr, value_ptr) };
        }

        // 3. Convert the builder to Arc<[i32]>
        let convert_fn = slice_builder_vtable.convert_fn;
        let arc_slice_ptr = unsafe { convert_fn(builder_ptr) };

        // 4. Verify the contents by borrowing
        let borrow_fn = arc_slice_def
            .vtable
            .borrow_fn
            .expect("Arc<[i32]> should have borrow_fn");
        let borrowed_ptr = unsafe { borrow_fn(arc_slice_ptr) };

        // Convert the wide pointer to a slice reference
        let slice = unsafe { borrowed_ptr.get::<[i32]>() };
        assert_eq!(slice, &[1, 2, 3, 4, 5]);

        // 5. Clean up - the Arc<[i32]> was boxed by convert_fn, we need to deallocate the Box
        unsafe {
            let _ = Box::from_raw(arc_slice_ptr.as_ptr::<Arc<[i32]>>() as *mut Arc<[i32]>);
        }
    }

    #[test]
    fn test_arc_vtable_7_slice_builder_free() {
        facet_testhelpers::setup();

        // Get the shapes we'll be working with
        let arc_slice_shape = <Arc<[String]>>::SHAPE;
        let arc_slice_def = arc_slice_shape
            .def
            .into_pointer()
            .expect("Arc<[String]> should have a smart pointer definition");

        // Get the slice builder vtable
        let slice_builder_vtable = arc_slice_def
            .vtable
            .slice_builder_vtable
            .expect("Arc<[String]> should have slice_builder_vtable");

        // 1. Create a new builder
        let builder_ptr = (slice_builder_vtable.new_fn)();

        // 2. Push some items to the builder
        let push_fn = slice_builder_vtable.push_fn;
        let strings = ["hello", "world", "test"];
        for &s in &strings {
            let mut value = ManuallyDrop::new(String::from(s));
            let value_ptr = PtrMut::new(NonNull::from(&mut value).as_ptr());
            unsafe { push_fn(builder_ptr, value_ptr) };
        }

        // 3. Instead of converting, test the free function
        // This simulates abandoning the builder without creating the Arc
        let free_fn = slice_builder_vtable.free_fn;
        unsafe { free_fn(builder_ptr) };

        // If we get here without panicking, the free worked correctly
    }
}
