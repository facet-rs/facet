use core::{alloc::Layout, ptr::NonNull};

use alloc::boxed::Box;

use crate::{
    Def, Facet, KnownPointer, PointerDef, PointerFlags, PointerVTable, PtrConst, PtrMut, PtrUninit,
    Shape, TryBorrowInnerError, TryFromError, TryIntoInnerError, Type, UserType, ValueVTable,
    shape_util::vtable_for_ptr,
};

// Define the functions for transparent conversion between Box<T> and T
unsafe fn try_from<'src, 'dst>(
    src_ptr: PtrConst<'src>,
    src_shape: &'static Shape,
    dst: PtrUninit<'dst>,
) -> Result<PtrMut<'dst>, TryFromError> {
    let layout = src_shape.layout.sized_layout().unwrap();

    unsafe {
        let alloc = alloc::alloc::alloc(layout);
        if alloc.is_null() {
            alloc::alloc::handle_alloc_error(layout);
        }

        let src_ptr = src_ptr.as_ptr::<u8>();
        core::ptr::copy_nonoverlapping(src_ptr, alloc, layout.size());

        // layout of
        // Box<T> == *mut T == *mut u8
        Ok(dst.put(alloc))
    }
}

unsafe fn try_into_inner<'a, 'src, 'dst, T: ?Sized + Facet<'a>>(
    src_ptr: PtrMut<'src>,
    dst: PtrUninit<'dst>,
) -> Result<PtrMut<'dst>, TryIntoInnerError> {
    if const { size_of::<*const T>() == size_of::<*const ()>() } {
        let boxed = unsafe { src_ptr.read::<Box<T>>() };
        let layout = Layout::for_value(&*boxed);
        let ptr = Box::into_raw(boxed) as *mut u8;
        unsafe {
            core::ptr::copy_nonoverlapping(ptr, dst.as_mut_byte_ptr(), layout.size());
            alloc::alloc::dealloc(ptr, layout);
            Ok(dst.assume_init())
        }
    } else {
        panic!();
    }
}
unsafe impl<'a, T: ?Sized + Facet<'a>> Facet<'a> for Box<T> {
    const SHAPE: &'static crate::Shape = &const {
        unsafe fn try_borrow_inner<'a, 'src, T: ?Sized + Facet<'a>>(
            src_ptr: PtrConst<'src>,
        ) -> Result<PtrConst<'src>, TryBorrowInnerError> {
            let boxed = unsafe { src_ptr.get::<Box<T>>() };
            Ok(PtrConst::new(NonNull::from(&**boxed)))
        }

        Shape {
            id: Shape::id_of::<Self>(),
            layout: Shape::layout_of::<Self>(),
            vtable: ValueVTable {
                type_name: |f, opts| {
                    write!(f, "{}", Self::SHAPE.type_identifier)?;
                    if let Some(opts) = opts.for_children() {
                        write!(f, "<")?;
                        (T::SHAPE.vtable.type_name())(f, opts)?;
                        write!(f, ">")?;
                    } else {
                        write!(f, "<…>")?;
                    }
                    Ok(())
                },
                try_from: if size_of::<*const T>() == size_of::<*const ()>() {
                    Some(try_from)
                } else {
                    None
                },
                try_into_inner: if size_of::<*const T>() == size_of::<*const ()>() {
                    Some(try_into_inner::<T>)
                } else {
                    None
                },
                try_borrow_inner: Some(try_borrow_inner::<T>),
                ..vtable_for_ptr::<T, Self>()
            },
            ty: Type::User(UserType::Opaque),
            def: Def::Pointer(PointerDef {
                vtable: &const {
                    PointerVTable {
                        borrow_fn: Some(|this| unsafe {
                            let concrete = this.get::<Box<T>>();
                            let t: &T = concrete.as_ref();
                            PtrConst::new(NonNull::from(t))
                        }),
                        new_into_fn: if size_of::<*const T>() == size_of::<*const ()>() {
                            Some(|this, ptr| unsafe {
                                try_from(ptr.as_const(), T::SHAPE, this).unwrap()
                            })
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
            }),
            type_identifier: "Box",
            type_params: &[crate::TypeParam {
                name: "T",
                shape: T::SHAPE,
            }],
            doc: &[],
            attributes: &[],
            type_tag: None,
            inner: Some(T::SHAPE),
            proxy: None,
            variance: Shape::computed_variance,
        }
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
        let box_ptr =
            unsafe { new_into_fn(box_uninit_ptr, PtrMut::new(NonNull::from(&mut value))) };
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

        // Get the function pointer for dropping the Box
        let drop_fn = box_shape
            .vtable
            .drop_in_place
            .expect("Box<T> should have drop_in_place");

        // Drop the Box in place
        // SAFETY: box_ptr points to a valid Box<String>
        unsafe { drop_fn(box_ptr) };

        // Deallocate the memory
        // SAFETY: box_ptr was allocated by box_shape and is now dropped (but memory is still valid)
        unsafe { box_shape.deallocate_mut(box_ptr).unwrap() };
    }
}
