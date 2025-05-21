use crate::{
    Def, Facet, KnownSmartPointer, PtrConst, PtrMut, PtrUninit, Shape, SmartPointerDef,
    SmartPointerFlags, SmartPointerVTable, TryBorrowInnerError, TryFromError, TryIntoInnerError,
    Type, UserType, ValueVTable, value_vtable,
};

unsafe impl<'a, T: Facet<'a>> Facet<'a> for alloc::boxed::Box<T> {
    const VTABLE: &'static ValueVTable = &const {
        // Define the functions for transparent conversion between Box<T> and T
        unsafe fn try_from<'a, 'shape, 'src, 'dst, T: Facet<'a>>(
            src_ptr: PtrConst<'src>,
            src_shape: &'shape Shape<'shape>,
            dst: PtrUninit<'dst>,
        ) -> Result<PtrMut<'dst>, TryFromError<'shape>> {
            if src_shape.id != T::SHAPE.id {
                return Err(TryFromError::UnsupportedSourceShape {
                    src_shape,
                    expected: &[T::SHAPE],
                });
            }
            let t = unsafe { src_ptr.read::<T>() };
            let boxed = alloc::boxed::Box::new(t);
            Ok(unsafe { dst.put(boxed) })
        }

        unsafe fn try_into_inner<'a, 'src, 'dst, T: Facet<'a>>(
            src_ptr: PtrMut<'src>,
            dst: PtrUninit<'dst>,
        ) -> Result<PtrMut<'dst>, TryIntoInnerError> {
            let boxed = unsafe { src_ptr.read::<alloc::boxed::Box<T>>() };
            Ok(unsafe { dst.put(*boxed) })
        }

        unsafe fn try_borrow_inner<'a, 'src, T: Facet<'a>>(
            src_ptr: PtrConst<'src>,
        ) -> Result<PtrConst<'src>, TryBorrowInnerError> {
            let boxed = unsafe { src_ptr.get::<alloc::boxed::Box<T>>() };
            Ok(PtrConst::new(&**boxed))
        }

        let mut vtable = value_vtable!(alloc::boxed::Box<T>, |f, opts| {
            write!(f, "Box")?;
            if let Some(opts) = opts.for_children() {
                write!(f, "<")?;
                (T::SHAPE.vtable.type_name)(f, opts)?;
                write!(f, ">")?;
            } else {
                write!(f, "<…>")?;
            }
            Ok(())
        });
        vtable.try_from = || Some(try_from::<T>);
        vtable.try_into_inner = || Some(try_into_inner::<T>);
        vtable.try_borrow_inner = || Some(try_borrow_inner::<T>);
        vtable
    };

    const SHAPE: &'static crate::Shape<'static> = &const {
        // Function to return inner type's shape
        fn inner_shape<'a, T: Facet<'a>>() -> &'static Shape<'static> {
            T::SHAPE
        }

        crate::Shape::builder_for_sized::<Self>()
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: || T::SHAPE,
            }])
            .ty(Type::User(UserType::Opaque))
            .def(Def::SmartPointer(
                SmartPointerDef::builder()
                    .pointee(|| T::SHAPE)
                    .flags(SmartPointerFlags::EMPTY)
                    .known(KnownSmartPointer::Box)
                    .vtable(
                        &const {
                            SmartPointerVTable::builder()
                                .borrow_fn(|this| {
                                    let ptr = unsafe {
                                        &raw const **this.as_ptr::<alloc::boxed::Box<T>>()
                                    };
                                    PtrConst::new(ptr)
                                })
                                .new_into_fn(|this, ptr| {
                                    let t = unsafe { ptr.read::<T>() };
                                    let boxed = alloc::boxed::Box::new(t);
                                    unsafe { this.put(boxed) }
                                })
                                .build()
                        },
                    )
                    .build(),
            ))
            .inner(inner_shape::<T>)
            .build()
    };
}

#[cfg(test)]
mod tests {
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
    fn test_box_vtable_1_new_borrow_drop() -> eyre::Result<()> {
        facet_testhelpers::setup();

        let box_shape = <Box<String>>::SHAPE;
        let box_def = box_shape
            .def
            .into_smart_pointer()
            .expect("Box<T> should have a smart pointer definition");

        // Allocate memory for the Box
        let box_uninit_ptr = box_shape.allocate()?;

        // Get the function pointer for creating a new Box from a value
        let new_into_fn = box_def
            .vtable
            .new_into_fn
            .expect("Box<T> should have new_into_fn");

        // Create the value and initialize the Box
        let mut value = String::from("example");
        let box_ptr = unsafe { new_into_fn(box_uninit_ptr, PtrMut::new(&raw mut value)) };
        // The value now belongs to the Box, prevent its drop
        core::mem::forget(value);

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
        let drop_fn = (box_shape.vtable.drop_in_place)().expect("Box<T> should have drop_in_place");

        // Drop the Box in place
        // SAFETY: box_ptr points to a valid Box<String>
        unsafe { drop_fn(box_ptr) };

        // Deallocate the memory
        // SAFETY: box_ptr was allocated by box_shape and is now dropped (but memory is still valid)
        unsafe { box_shape.deallocate_mut(box_ptr)? };

        Ok(())
    }
}
