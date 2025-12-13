use core::ptr::NonNull;

use alloc::boxed::Box;

use crate::{
    Def, Facet, KnownPointer, PointerDef, PointerFlags, PointerVTable, PtrConst, PtrMut, PtrUninit,
    Shape, ShapeBuilder, TryFromError, Type, TypeNameFn, TypeNameOpts, TypeOpsIndirect, UserType,
    VTableIndirect,
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

// Named function for borrow_fn
unsafe fn borrow_fn<'a, T: ?Sized + Facet<'a>>(this: PtrConst) -> PtrConst {
    unsafe {
        let concrete = this.get::<Box<T>>();
        let t: &T = concrete.as_ref();
        PtrConst::new(NonNull::from(t).as_ptr())
    }
}

// Named function for new_into_fn
unsafe fn new_into_fn<'a, 'src, T: ?Sized + Facet<'a>>(this: PtrUninit, ptr: PtrMut) -> PtrMut {
    unsafe { try_from(ptr.as_const(), T::SHAPE, this).unwrap() }
}

unsafe impl<'a, T: ?Sized + Facet<'a>> Facet<'a> for Box<T> {
    const SHAPE: &'static crate::Shape = &const {
        const fn build_type_name<'a, T: ?Sized + Facet<'a>>() -> TypeNameFn {
            fn type_name_impl<'a, T: ?Sized + Facet<'a>>(
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
            .type_name(build_type_name::<T>())
            .vtable_indirect(&VTableIndirect::EMPTY)
            .type_ops_indirect(
                &const {
                    unsafe fn drop_in_place<'a, T: ?Sized + Facet<'a>>(ox: crate::OxPtrMut) {
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
                        new_into_fn: if size_of::<*const T>() == size_of::<*const ()>() {
                            Some(new_into_fn::<T>)
                        } else {
                            None
                        },
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
            // Box<T> propagates T's variance
            .variance(Shape::computed_variance)
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
}
