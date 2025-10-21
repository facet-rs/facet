use core::ptr::NonNull;

use alloc::boxed::Box;
use alloc::rc::{Rc, Weak};
use alloc::vec::Vec;

use crate::{
    Def, Facet, KnownPointer, PointerDef, PointerFlags, PointerVTable, PtrConst, PtrMut, PtrUninit,
    Shape, SliceBuilderVTable, TryBorrowInnerError, TryFromError, TryIntoInnerError, Type,
    UserType, value_vtable,
};

unsafe impl<'a, T: Facet<'a>> Facet<'a> for Rc<T> {
    const SHAPE: &'static crate::Shape = &const {
        // Function to return inner type's shape
        fn inner_shape<'a, T: Facet<'a>>() -> &'static Shape {
            T::SHAPE
        }

        crate::Shape::builder_for_sized::<Self>()
            .vtable({
                // Define the functions for transparent conversion between Rc<T> and T
                unsafe fn try_from<'a, 'src, 'dst, T: Facet<'a>>(
                    src_ptr: PtrConst<'src>,
                    src_shape: &'static Shape,
                    dst: PtrUninit<'dst>,
                ) -> Result<PtrMut<'dst>, TryFromError> {
                    if src_shape.id != T::SHAPE.id {
                        return Err(TryFromError::UnsupportedSourceShape {
                            src_shape,
                            expected: &[T::SHAPE],
                        });
                    }
                    let t = unsafe { src_ptr.read::<T>() };
                    let rc = Rc::new(t);
                    Ok(unsafe { dst.put(rc) })
                }

                unsafe fn try_into_inner<'a, 'src, 'dst, T: Facet<'a>>(
                    src_ptr: PtrMut<'src>,
                    dst: PtrUninit<'dst>,
                ) -> Result<PtrMut<'dst>, TryIntoInnerError> {
                    let rc = unsafe { src_ptr.get::<Rc<T>>() };
                    match Rc::try_unwrap(rc.clone()) {
                        Ok(t) => Ok(unsafe { dst.put(t) }),
                        Err(_) => Err(TryIntoInnerError::Unavailable),
                    }
                }

                unsafe fn try_borrow_inner<'a, 'src, T: Facet<'a>>(
                    src_ptr: PtrConst<'src>,
                ) -> Result<PtrConst<'src>, TryBorrowInnerError> {
                    let rc = unsafe { src_ptr.get::<Rc<T>>() };
                    Ok(PtrConst::new(NonNull::from(&**rc)))
                }

                let mut vtable = value_vtable!(alloc::rc::Rc<T>, |f, opts| {
                    write!(f, "{}", Self::SHAPE.type_identifier)?;
                    if let Some(opts) = opts.for_children() {
                        write!(f, "<")?;
                        T::SHAPE.vtable.type_name()(f, opts)?;
                        write!(f, ">")?;
                    } else {
                        write!(f, "<…>")?;
                    }
                    Ok(())
                });
                {
                    vtable.try_from = || Some(try_from::<T>);
                    vtable.try_into_inner = || Some(try_into_inner::<T>);
                    vtable.try_borrow_inner = || Some(try_borrow_inner::<T>);
                }
                vtable
            })
            .type_identifier("Rc")
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: || T::SHAPE,
            }])
            .ty(Type::User(UserType::Opaque))
            .def(Def::Pointer(
                PointerDef::builder()
                    .pointee(|| T::SHAPE)
                    .flags(PointerFlags::EMPTY)
                    .known(KnownPointer::Rc)
                    .weak(|| <Weak<T> as Facet>::SHAPE)
                    .vtable(
                        &const {
                            PointerVTable::builder()
                                .borrow_fn(|this| {
                                    let ptr = Self::as_ptr(unsafe { this.get() });
                                    PtrConst::new(unsafe { NonNull::new_unchecked(ptr as *mut T) })
                                })
                                .new_into_fn(|this, ptr| {
                                    let t = unsafe { ptr.read::<T>() };
                                    let rc = Rc::new(t);
                                    unsafe { this.put(rc) }
                                })
                                .downgrade_into_fn(|strong, weak| unsafe {
                                    weak.put(Rc::downgrade(strong.get::<Self>()))
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

unsafe impl<'a> Facet<'a> for Rc<str> {
    const SHAPE: &'static crate::Shape = &const {
        // Function to return inner type's shape
        fn inner_shape() -> &'static Shape {
            str::SHAPE
        }

        crate::Shape::builder_for_sized::<Self>()
            .vtable({
                value_vtable!(alloc::rc::Rc<str>, |f, opts| {
                    write!(f, "{}", Self::SHAPE.type_identifier)?;
                    if let Some(opts) = opts.for_children() {
                        write!(f, "<")?;
                        (str::SHAPE.vtable.type_name())(f, opts)?;
                        write!(f, ">")?;
                    } else {
                        write!(f, "<…>")?;
                    }
                    Ok(())
                })
            })
            .type_identifier("Rc")
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: || str::SHAPE,
            }])
            .ty(Type::User(UserType::Opaque))
            .def(Def::Pointer(
                PointerDef::builder()
                    .pointee(|| str::SHAPE)
                    .flags(PointerFlags::EMPTY)
                    .known(KnownPointer::Rc)
                    .weak(|| <Weak<str> as Facet>::SHAPE)
                    .vtable(
                        &const {
                            PointerVTable::builder()
                                .borrow_fn(|this| unsafe {
                                    let concrete = this.get::<Rc<str>>();
                                    let s: &str = concrete;
                                    PtrConst::new(NonNull::from(s))
                                })
                                .downgrade_into_fn(|strong, weak| unsafe {
                                    weak.put(Rc::downgrade(strong.get::<Self>()))
                                })
                                .build()
                        },
                    )
                    .build(),
            ))
            .inner(inner_shape)
            .build()
    };
}

unsafe impl<'a, U: Facet<'a>> Facet<'a> for Rc<[U]> {
    const SHAPE: &'static crate::Shape = &const {
        // Function to return inner type's shape
        fn inner_shape<'a, U: Facet<'a>>() -> &'static Shape {
            <[U]>::SHAPE
        }

        fn slice_builder_new<'a, U: Facet<'a>>() -> PtrMut<'static> {
            let v = Box::new(Vec::<U>::new());
            let raw = Box::into_raw(v);
            PtrMut::new(unsafe { NonNull::new_unchecked(raw) })
        }

        fn slice_builder_push<'a, U: Facet<'a>>(builder: PtrMut, item: PtrMut) {
            unsafe {
                let vec = builder.as_mut::<Vec<U>>();
                let value = item.read::<U>();
                vec.push(value);
            }
        }

        fn slice_builder_convert<'a, U: Facet<'a>>(builder: PtrMut<'static>) -> PtrConst<'static> {
            unsafe {
                let vec_box = Box::from_raw(builder.as_ptr::<Vec<U>>() as *mut Vec<U>);
                let arc: Rc<[U]> = (*vec_box).into();
                let arc_box = Box::new(arc);
                PtrConst::new(NonNull::new_unchecked(Box::into_raw(arc_box)))
            }
        }

        fn slice_builder_free<'a, U: Facet<'a>>(builder: PtrMut<'static>) {
            unsafe {
                let _ = Box::from_raw(builder.as_ptr::<Vec<U>>() as *mut Vec<U>);
            }
        }

        crate::Shape::builder_for_sized::<Self>()
            .vtable({
                value_vtable!(alloc::rc::Rc<[U]>, |f, opts| {
                    write!(f, "{}", Self::SHAPE.type_identifier)?;
                    if let Some(opts) = opts.for_children() {
                        write!(f, "<")?;
                        (<[U]>::SHAPE.vtable.type_name())(f, opts)?;
                        write!(f, ">")?;
                    } else {
                        write!(f, "<…>")?;
                    }
                    Ok(())
                })
            })
            .type_identifier("Rc")
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: || <[U]>::SHAPE,
            }])
            .ty(Type::User(UserType::Opaque))
            .def(Def::Pointer(
                PointerDef::builder()
                    .pointee(|| <[U]>::SHAPE)
                    .flags(PointerFlags::EMPTY)
                    .known(KnownPointer::Rc)
                    .weak(|| <Weak<[U]> as Facet>::SHAPE)
                    .vtable(
                        &const {
                            PointerVTable::builder()
                                .borrow_fn(|this| unsafe {
                                    let concrete = this.get::<Rc<[U]>>();
                                    let s: &[U] = concrete;
                                    PtrConst::new(NonNull::from(s))
                                })
                                .downgrade_into_fn(|strong, weak| unsafe {
                                    weak.put(Rc::downgrade(strong.get::<Self>()))
                                })
                                .slice_builder_vtable(
                                    &const {
                                        SliceBuilderVTable::builder()
                                            .new_fn(slice_builder_new::<U>)
                                            .push_fn(slice_builder_push::<U>)
                                            .convert_fn(slice_builder_convert::<U>)
                                            .free_fn(slice_builder_free::<U>)
                                            .build()
                                    },
                                )
                                .build()
                        },
                    )
                    .build(),
            ))
            .inner(inner_shape::<U>)
            .build()
    };
}

unsafe impl<'a, T: Facet<'a>> Facet<'a> for Weak<T> {
    const SHAPE: &'static crate::Shape = &const {
        // Function to return inner type's shape
        fn inner_shape<'a, T: Facet<'a>>() -> &'static Shape {
            T::SHAPE
        }

        crate::Shape::builder_for_sized::<Self>()
            .vtable({
                value_vtable!(alloc::rc::Weak<T>, |f, opts| {
                    write!(f, "{}", Self::SHAPE.type_identifier)?;
                    if let Some(opts) = opts.for_children() {
                        write!(f, "<")?;
                        T::SHAPE.vtable.type_name()(f, opts)?;
                        write!(f, ">")?;
                    } else {
                        write!(f, "<…>")?;
                    }
                    Ok(())
                })
            })
            .type_identifier("Weak")
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: || T::SHAPE,
            }])
            .ty(Type::User(UserType::Opaque))
            .def(Def::Pointer(
                PointerDef::builder()
                    .pointee(|| T::SHAPE)
                    .flags(PointerFlags::WEAK)
                    .known(KnownPointer::RcWeak)
                    .strong(|| <Rc<T> as Facet>::SHAPE)
                    .vtable(
                        &const {
                            PointerVTable::builder()
                                .upgrade_into_fn(|weak, strong| unsafe {
                                    Some(strong.put(weak.get::<Self>().upgrade()?))
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

unsafe impl<'a> Facet<'a> for Weak<str> {
    const SHAPE: &'static crate::Shape = &const {
        // Function to return inner type's shape
        fn inner_shape() -> &'static Shape {
            str::SHAPE
        }

        crate::Shape::builder_for_sized::<Self>()
            .vtable({
                value_vtable!(alloc::rc::Weak<str>, |f, opts| {
                    write!(f, "{}", Self::SHAPE.type_identifier)?;
                    if let Some(opts) = opts.for_children() {
                        write!(f, "<")?;
                        (str::SHAPE.vtable.type_name())(f, opts)?;
                        write!(f, ">")?;
                    } else {
                        write!(f, "<…>")?;
                    }
                    Ok(())
                })
            })
            .type_identifier("Weak")
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: || str::SHAPE,
            }])
            .ty(Type::User(UserType::Opaque))
            .def(Def::Pointer(
                PointerDef::builder()
                    .pointee(|| str::SHAPE)
                    .flags(PointerFlags::WEAK)
                    .known(KnownPointer::RcWeak)
                    .strong(|| <Rc<str> as Facet>::SHAPE)
                    .vtable(
                        &const {
                            PointerVTable::builder()
                                .upgrade_into_fn(|weak, strong| unsafe {
                                    Some(strong.put(weak.get::<Self>().upgrade()?))
                                })
                                .build()
                        },
                    )
                    .build(),
            ))
            .inner(inner_shape)
            .build()
    };
}

unsafe impl<'a, U: Facet<'a>> Facet<'a> for Weak<[U]> {
    const SHAPE: &'static crate::Shape = &const {
        fn inner_shape<'a, U: Facet<'a>>() -> &'static Shape {
            <[U]>::SHAPE
        }

        crate::Shape::builder_for_sized::<Self>()
            .vtable({
                value_vtable!(alloc::rc::Weak<[U]>, |f, opts| {
                    write!(f, "{}", Self::SHAPE.type_identifier)?;
                    if let Some(opts) = opts.for_children() {
                        write!(f, "<")?;
                        (<[U]>::SHAPE.vtable.type_name())(f, opts)?;
                        write!(f, ">")?;
                    } else {
                        write!(f, "<…>")?;
                    }
                    Ok(())
                })
            })
            .type_identifier("Weak")
            .type_params(&[crate::TypeParam {
                name: "T",
                shape: || <[U]>::SHAPE,
            }])
            .ty(Type::User(UserType::Opaque))
            .def(Def::Pointer(
                PointerDef::builder()
                    .pointee(|| <[U]>::SHAPE)
                    .flags(PointerFlags::WEAK)
                    .known(KnownPointer::RcWeak)
                    .strong(|| <Rc<[U]> as Facet>::SHAPE)
                    .vtable(
                        &const {
                            PointerVTable::builder()
                                .upgrade_into_fn(|weak, strong| unsafe {
                                    Some(strong.put(weak.get::<Self>().upgrade()?))
                                })
                                .build()
                        },
                    )
                    .build(),
            ))
            .inner(inner_shape::<U>)
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
        let rc_ptr = unsafe { new_into_fn(rc_uninit_ptr, PtrMut::new(NonNull::from(&mut value))) };

        // Get the function pointer for borrowing the inner value
        let borrow_fn = rc_def
            .vtable
            .borrow_fn
            .expect("Rc<T> should have borrow_fn");

        // Borrow the inner value and check it
        let borrowed_ptr = unsafe { borrow_fn(rc_ptr.as_const()) };
        // SAFETY: borrowed_ptr points to a valid String within the Rc
        assert_eq!(unsafe { borrowed_ptr.get::<String>() }, "example");

        // Get the function pointer for dropping the Rc
        let drop_fn = (rc_shape.vtable.drop_in_place)().expect("Rc<T> should have drop_in_place");

        // Drop the Rc in place
        // SAFETY: rc_ptr points to a valid Rc<String>
        unsafe { drop_fn(rc_ptr) };

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
        let rc1_ptr =
            unsafe { new_into_fn(rc1_uninit_ptr, PtrMut::new(NonNull::from(&mut value))) };

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
        let rc_drop_fn = (rc_shape.vtable.drop_in_place)().unwrap();
        let weak_drop_fn = (weak_shape.vtable.drop_in_place)().unwrap();

        unsafe {
            // Drop Rcs
            rc_drop_fn(rc1_ptr);
            rc_shape.deallocate_mut(rc1_ptr).unwrap();
            rc_drop_fn(rc2_ptr);
            rc_shape.deallocate_mut(rc2_ptr).unwrap();

            // Drop Weak
            weak_drop_fn(weak1_ptr);
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
        let rc1_ptr =
            unsafe { new_into_fn(rc1_uninit_ptr, PtrMut::new(NonNull::from(&mut value))) };

        // 2. Downgrade rc1 to create a weak pointer (weak1)
        let weak1_uninit_ptr = weak_shape.allocate().unwrap();
        let downgrade_into_fn = rc_def.vtable.downgrade_into_fn.unwrap();
        // SAFETY: rc1_ptr is valid, weak1_uninit_ptr is allocated for Weak
        let weak1_ptr = unsafe { downgrade_into_fn(rc1_ptr, weak1_uninit_ptr) };

        // 3. Drop and free the strong pointer (rc1)
        let rc_drop_fn = (rc_shape.vtable.drop_in_place)().unwrap();
        unsafe {
            rc_drop_fn(rc1_ptr);
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
        let weak_drop_fn = (weak_shape.vtable.drop_in_place)().unwrap();
        unsafe {
            // Deallocate the *uninitialized* memory allocated for the failed upgrade attempt
            rc_shape.deallocate_uninit(rc2_uninit_ptr).unwrap();

            // Drop and deallocate the weak pointer
            weak_drop_fn(weak1_ptr);
            weak_shape.deallocate_mut(weak1_ptr).unwrap();
        }
    }
}
