use alloc::alloc::Layout;
use alloc::vec::Vec;
use core::ptr::NonNull;

use alloc::boxed::Box;

use crate::{
    Def, Facet, KnownPointer, OxPtrMut, PointerDef, PointerFlags, PointerVTable, PtrConst, PtrMut,
    PtrUninit, Shape, ShapeBuilder, SliceBuilderVTable, TryFromError, Type, TypeNameFn,
    TypeNameOpts, TypeOpsIndirect, UserType, VTableIndirect, Variance, VarianceDep, VarianceDesc,
};

// Named function for try_from
unsafe fn try_from(
    src_ptr: PtrConst,
    src_shape: &'static Shape,
    dst: PtrUninit,
) -> Result<PtrMut, TryFromError> {
    let layout = src_shape.layout.sized_layout().unwrap();

    unsafe {
        let alloc = alloc::alloc::alloc(layout);
        if alloc.is_null() {
            alloc::alloc::handle_alloc_error(layout);
        }

        let src_ptr = src_ptr.as_ptr::<u8>();
        core::ptr::copy_nonoverlapping(src_ptr, alloc, layout.size());

        // layout of Box<T> == *mut T == *mut u8
        Ok(dst.put(alloc))
    }
}

// Named function for borrow_fn (sized types)
unsafe fn borrow_fn<'a, T: Facet<'a>>(this: PtrConst) -> PtrConst {
    unsafe {
        let concrete = this.get::<Box<T>>();
        let t: &T = concrete.as_ref();
        PtrConst::new(NonNull::from(t).as_ptr() as *const u8)
    }
}

// Named function for new_into_fn (sized types)
unsafe fn new_into_fn<'a, 'src, T: Facet<'a>>(this: PtrUninit, ptr: PtrMut) -> PtrMut {
    unsafe { try_from(ptr.as_const(), T::SHAPE, this).unwrap() }
}

// Note: This impl is for sized T only. Box<[U]> and Box<str> have separate impls below.
unsafe impl<'a, T: Facet<'a>> Facet<'a> for Box<T> {
    const SHAPE: &'static crate::Shape = &const {
        const fn build_type_name<'a, T: Facet<'a>>() -> TypeNameFn {
            fn type_name_impl<'a, T: Facet<'a>>(
                _shape: &'static crate::Shape,
                f: &mut core::fmt::Formatter<'_>,
                opts: TypeNameOpts,
            ) -> core::fmt::Result {
                write!(f, "Box")?;
                if let Some(opts) = opts.for_children() {
                    write!(f, "<")?;
                    T::SHAPE.write_type_name(f, opts)?;
                    write!(f, ">")?;
                } else {
                    write!(f, "<…>")?;
                }
                Ok(())
            }
            type_name_impl::<T>
        }

        ShapeBuilder::for_sized::<Self>("Box")
            .module_path("alloc::boxed")
            .type_name(build_type_name::<T>())
            .vtable_indirect(&VTableIndirect::EMPTY)
            .type_ops_indirect(
                &const {
                    unsafe fn drop_in_place<'a, T: Facet<'a>>(ox: OxPtrMut) {
                        unsafe {
                            core::ptr::drop_in_place(ox.ptr().as_ptr::<Box<T>>() as *mut Box<T>)
                        };
                    }
                    TypeOpsIndirect {
                        drop_in_place: drop_in_place::<T>,
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
                        borrow_fn: Some(borrow_fn::<T>),
                        new_into_fn: Some(new_into_fn::<T>),
                        ..PointerVTable::new()
                    }
                },
                pointee: Some(T::SHAPE),
                weak: None,
                strong: None,
                flags: PointerFlags::EMPTY,
                known: Some(KnownPointer::Box),
            }))
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: T::SHAPE,
            }])
            .inner(T::SHAPE)
            // Box<T> propagates T's variance
            .variance(VarianceDesc {
                base: Variance::Bivariant,
                deps: &const { [VarianceDep::covariant(T::SHAPE)] },
            })
            .build()
    };
}

// ============================================================================
// Box<[U]> implementation with SliceBuilderVTable
// ============================================================================

// Drop function for Box<[U]>
unsafe fn box_slice_drop<U>(ox: OxPtrMut) {
    unsafe { core::ptr::drop_in_place(ox.ptr().as_ptr::<Box<[U]>>() as *mut Box<[U]>) };
}

// Borrow function for Box<[U]>
unsafe fn box_slice_borrow<'a, U: Facet<'a>>(this: PtrConst) -> PtrConst {
    unsafe {
        let concrete = this.get::<Box<[U]>>();
        let slice: &[U] = concrete.as_ref();
        PtrConst::new(NonNull::from(slice).as_ptr())
    }
}

// Slice builder functions for Box<[U]>
fn box_slice_builder_new<'a, U: Facet<'a>>() -> PtrMut {
    let v = Box::new(Vec::<U>::new());
    let raw = Box::into_raw(v);
    PtrMut::new(raw as *mut u8)
}

unsafe fn box_slice_builder_push<'a, U: Facet<'a>>(builder: PtrMut, item: PtrConst) {
    unsafe {
        let vec = builder.as_mut::<Vec<U>>();
        let value = item.read::<U>();
        vec.push(value);
    }
}

unsafe fn box_slice_builder_convert<'a, U: Facet<'a>>(builder: PtrMut) -> PtrConst {
    unsafe {
        let vec_box = Box::from_raw(builder.as_ptr::<Vec<U>>() as *mut Vec<U>);
        let boxed_slice: Box<[U]> = (*vec_box).into_boxed_slice();

        // Allocate memory for the Box<[U]> (which is a fat pointer, 16 bytes on 64-bit)
        let layout = Layout::new::<Box<[U]>>();
        let ptr = alloc::alloc::alloc(layout) as *mut Box<[U]>;
        if ptr.is_null() {
            alloc::alloc::handle_alloc_error(layout);
        }

        // Write the Box<[U]> into the allocation
        ptr.write(boxed_slice);

        PtrConst::new(ptr as *const u8)
    }
}

unsafe fn box_slice_builder_free<'a, U: Facet<'a>>(builder: PtrMut) {
    unsafe {
        let _ = Box::from_raw(builder.as_ptr::<Vec<U>>() as *mut Vec<U>);
    }
}

// ============================================================================
// Box<str> implementation
// ============================================================================

// Drop function for Box<str>
unsafe fn box_str_drop(ox: OxPtrMut) {
    unsafe { core::ptr::drop_in_place(ox.ptr().as_ptr::<Box<str>>() as *mut Box<str>) };
}

// Borrow function for Box<str>
unsafe fn box_str_borrow(this: PtrConst) -> PtrConst {
    unsafe {
        let concrete = this.get::<Box<str>>();
        let s: &str = concrete;
        PtrConst::new(NonNull::from(s).as_ptr())
    }
}

// Module-level static for Box<str> type ops
static BOX_STR_TYPE_OPS: TypeOpsIndirect = TypeOpsIndirect {
    drop_in_place: box_str_drop,
    default_in_place: None,
    clone_into: None,
    is_truthy: None,
};

unsafe impl<'a> Facet<'a> for Box<str> {
    const SHAPE: &'static crate::Shape = &const {
        fn type_name_box_str(
            _shape: &'static crate::Shape,
            f: &mut core::fmt::Formatter<'_>,
            opts: TypeNameOpts,
        ) -> core::fmt::Result {
            write!(f, "Box")?;
            if let Some(opts) = opts.for_children() {
                write!(f, "<")?;
                str::SHAPE.write_type_name(f, opts)?;
                write!(f, ">")?;
            } else {
                write!(f, "<…>")?;
            }
            Ok(())
        }

        ShapeBuilder::for_sized::<Self>("Box")
            .module_path("alloc::boxed")
            .type_name(type_name_box_str)
            .vtable_indirect(&VTableIndirect::EMPTY)
            .type_ops_indirect(&BOX_STR_TYPE_OPS)
            .ty(Type::User(UserType::Opaque))
            .def(Def::Pointer(PointerDef {
                vtable: &const {
                    PointerVTable {
                        borrow_fn: Some(box_str_borrow),
                        ..PointerVTable::new()
                    }
                },
                pointee: Some(str::SHAPE),
                weak: None,
                strong: None,
                flags: PointerFlags::EMPTY,
                known: Some(KnownPointer::Box),
            }))
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: str::SHAPE,
            }])
            .build()
    };
}

unsafe impl<'a, U: Facet<'a>> Facet<'a> for Box<[U]> {
    const SHAPE: &'static crate::Shape = &const {
        fn type_name_box_slice<'a, U: Facet<'a>>(
            _shape: &'static crate::Shape,
            f: &mut core::fmt::Formatter<'_>,
            opts: TypeNameOpts,
        ) -> core::fmt::Result {
            write!(f, "Box")?;
            if let Some(opts) = opts.for_children() {
                write!(f, "<")?;
                <[U]>::SHAPE.write_type_name(f, opts)?;
                write!(f, ">")?;
            } else {
                write!(f, "<…>")?;
            }
            Ok(())
        }

        ShapeBuilder::for_sized::<Self>("Box")
            .module_path("alloc::boxed")
            .type_name(type_name_box_slice::<U>)
            .vtable_indirect(&VTableIndirect::EMPTY)
            .type_ops_indirect(
                &const {
                    TypeOpsIndirect {
                        drop_in_place: box_slice_drop::<U>,
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
                        borrow_fn: Some(box_slice_borrow::<U>),
                        slice_builder_vtable: Some(
                            &const {
                                SliceBuilderVTable::new(
                                    box_slice_builder_new::<U>,
                                    box_slice_builder_push::<U>,
                                    box_slice_builder_convert::<U>,
                                    box_slice_builder_free::<U>,
                                )
                            },
                        ),
                        ..PointerVTable::new()
                    }
                },
                pointee: Some(<[U]>::SHAPE),
                weak: None,
                strong: None,
                flags: PointerFlags::EMPTY,
                known: Some(KnownPointer::Box),
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

    use alloc::boxed::Box;
    use alloc::string::String;

    use super::*;

    #[test]
    fn test_box_type_params() {
        let [type_param_1] = <Box<i32>>::SHAPE.type_params else {
            panic!("Box<T> should only have 1 type param")
        };
        assert_eq!(type_param_1.shape(), i32::SHAPE);
    }

    #[test]
    fn test_box_vtable_1_new_borrow_drop() {
        facet_testhelpers::setup();

        let box_shape = <Box<String>>::SHAPE;
        let box_def = box_shape
            .def
            .into_pointer()
            .expect("Box<T> should have a smart pointer definition");

        // Allocate memory for the Box
        let box_uninit_ptr = box_shape.allocate().unwrap();

        // Get the function pointer for creating a new Box from a value
        let new_into_fn = box_def
            .vtable
            .new_into_fn
            .expect("Box<T> should have new_into_fn");

        // Create the value and initialize the Box
        let mut value = ManuallyDrop::new(String::from("example"));
        let box_ptr = unsafe {
            new_into_fn(
                box_uninit_ptr,
                PtrMut::new(NonNull::from(&mut value).as_ptr()),
            )
        };
        // The value now belongs to the Box, prevent its drop

        // Get the function pointer for borrowing the inner value
        let borrow_fn = box_def
            .vtable
            .borrow_fn
            .expect("Box<T> should have borrow_fn");

        // Borrow the inner value and check it
        let borrowed_ptr = unsafe { borrow_fn(box_ptr.as_const()) };
        // SAFETY: borrowed_ptr points to a valid String within the Box
        assert_eq!(unsafe { borrowed_ptr.get::<String>() }, "example");

        // Drop the Box in place
        // SAFETY: box_ptr points to a valid Box<String>
        unsafe {
            box_shape
                .call_drop_in_place(box_ptr)
                .expect("Box<T> should have drop_in_place");
        }

        // Deallocate the memory
        // SAFETY: box_ptr was allocated by box_shape and is now dropped (but memory is still valid)
        unsafe { box_shape.deallocate_mut(box_ptr).unwrap() };
    }

    #[test]
    fn test_box_slice_builder() {
        facet_testhelpers::setup();

        // Get the shapes we'll be working with
        let box_slice_shape = <Box<[u8]>>::SHAPE;
        let box_slice_def = box_slice_shape
            .def
            .into_pointer()
            .expect("Box<[u8]> should have a smart pointer definition");

        // Get the slice builder vtable
        let slice_builder_vtable = box_slice_def
            .vtable
            .slice_builder_vtable
            .expect("Box<[u8]> should have slice_builder_vtable");

        // 1. Create a new builder
        let builder_ptr = (slice_builder_vtable.new_fn)();

        // 2. Push some items to the builder
        let push_fn = slice_builder_vtable.push_fn;
        let values: [u8; 5] = [1, 2, 3, 4, 5];
        for &value in &values {
            let value_copy = value;
            let value_ptr = PtrConst::new(&value_copy);
            unsafe { push_fn(builder_ptr, value_ptr) };
        }

        // 3. Convert the builder to Box<[u8]>
        let convert_fn = slice_builder_vtable.convert_fn;
        let box_slice_ptr = unsafe { convert_fn(builder_ptr) };

        // 4. Verify the contents by borrowing
        let borrow_fn = box_slice_def
            .vtable
            .borrow_fn
            .expect("Box<[u8]> should have borrow_fn");
        let borrowed_ptr = unsafe { borrow_fn(box_slice_ptr) };

        // Convert the wide pointer to a slice reference
        let slice = unsafe { borrowed_ptr.get::<[u8]>() };
        assert_eq!(slice, &[1, 2, 3, 4, 5]);

        // 5. Clean up - the Box<[u8]> was boxed by convert_fn, we need to deallocate the Box
        unsafe {
            let _ = Box::from_raw(box_slice_ptr.as_ptr::<Box<[u8]>>() as *mut Box<[u8]>);
        }
    }
}
